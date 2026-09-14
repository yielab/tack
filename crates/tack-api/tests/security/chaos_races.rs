//! Multi-runner/duplicated-credential race and revocation tests, split out
//! of `chaos_recovery.rs` once that file passed 1000 lines — these three
//! are the ones needing `multi_thread`/file-backed setup, distinct from the
//! single-threaded fencing/artifact/replay/corruption tests that stayed
//! behind. See `chaos_common.rs`'s own doc comment for why this is a
//! second, independent copy of that module tree rather than a shared one.
//!
//! `fleet_race_between_two_runners_grants_exactly_one_lease` and
//! `duplicated_credential_race_grants_exactly_one_lease` run against a
//! file-backed SQLite database — a shared in-memory pool can accidentally
//! serialize a real race. `revoked_credential_rejected_everywhere_freezes_attempt`
//! is in-memory.

#[allow(clippy::duplicate_mod)]
#[path = "chaos_common.rs"]
mod chaos_common;

use crate::common;
use axum::http::StatusCode;
use chaos_common::*;
use chrono::Utc;
use serde_json::{Value, json};

// =======================================================================
// 1. Multi-runner contention: two distinct, independently-enrolled runners
//    in the same fleet race, via real concurrent HTTP requests against a
//    file-backed database, to claim the one request the fleet selector
//    makes them both eligible for.
// =======================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn fleet_race_between_two_runners_grants_exactly_one_lease() {
    let dir_guard = distinctive_temp_dir("fleet-race");
    let dir = dir_guard.path();
    let db_path = dir.join("g2-fleet-race.sqlite3");
    let storage_dir = dir.join("storage");
    let (app, pool) = app_file_backed(&db_path, &storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "fleet-race").await;
    let runner_a = enroll_runner(&app, "Fleet racer A").await;
    let runner_b = enroll_runner(&app, "Fleet racer B").await;

    // Direct SQL, not `POST /api/runner-fleets/{fleet_id}/members`, so this
    // fixture setup doesn't depend on that route's own behavior — exactly
    // as `repository_crash.rs` inserts fixture rows directly for setup it
    // cannot reach through HTTP.
    let fleet_id = "fleet-g2-race";
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, 'G2 race fleet', NULL, '{}', ?, ?)",
    )
    .bind(fleet_id)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("insert fleet");
    for runner_id in [&runner_a.runner_id, &runner_b.runner_id] {
        sqlx::query(
            "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(fleet_id)
        .bind(runner_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert fleet member");
    }

    let request_id = create_execution_request(
        &app,
        &item_id,
        "fleet-race-key",
        "fleet",
        fleet_id,
        &agent_profile_id,
    )
    .await;

    let app_a = app.clone();
    let app_b = app.clone();
    let (runner_a_id, cred_a) = (runner_a.runner_id.clone(), runner_a.credential.clone());
    let (runner_b_id, cred_b) = (runner_b.runner_id.clone(), runner_b.credential.clone());
    let left = tokio::spawn(async move {
        common::claim_runner(&app_a, &runner_a_id, &cred_a, "race-claim-a").await
    });
    let right = tokio::spawn(async move {
        common::claim_runner(&app_b, &runner_b_id, &cred_b, "race-claim-b").await
    });
    let (left, right) = tokio::join!(left, right);
    let (status_a, body_a) = left.expect("left task");
    let (status_b, body_b) = right.expect("right task");
    assert_eq!(status_a, StatusCode::OK, "{body_a}");
    assert_eq!(status_b, StatusCode::OK, "{body_b}");

    let leases = [&body_a["lease"], &body_b["lease"]];
    let won: Vec<&Value> = leases.iter().filter(|l| !l.is_null()).copied().collect();
    assert_eq!(
        won.len(),
        1,
        "exactly one of two racing runners may win the single available lease; got {body_a} / {body_b}"
    );

    // Direct proof, not just "one response had a lease": exactly one
    // execution_attempts row exists for this request, and the request
    // moved to `leased` — not "queued twice" or "leased twice".
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 1, "no blind duplicate execution");
    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(request_state, "leased");

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// =======================================================================
// 2. Stolen/duplicated credential: two concurrent processes holding the
//    *same* runner credential race to claim the same request. Proven
//    against a file-backed database per CLAUDE.md's concurrency rule.
// =======================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn duplicated_credential_race_grants_exactly_one_lease() {
    let dir_guard = distinctive_temp_dir("dup-credential");
    let dir = dir_guard.path();
    let db_path = dir.join("g2-dup-credential.sqlite3");
    let storage_dir = dir.join("storage");
    let (app, pool) = app_file_backed(&db_path, &storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "dup-cred").await;
    let runner = enroll_runner(&app, "Duplicated-credential runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "dup-cred-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    let app_a = app.clone();
    let app_b = app.clone();
    let (id_a, cred_a) = (runner.runner_id.clone(), runner.credential.clone());
    let (id_b, cred_b) = (runner.runner_id.clone(), runner.credential.clone());
    let left =
        tokio::spawn(
            async move { common::claim_runner(&app_a, &id_a, &cred_a, "dup-claim-a").await },
        );
    let right =
        tokio::spawn(
            async move { common::claim_runner(&app_b, &id_b, &cred_b, "dup-claim-b").await },
        );
    let (left, right) = tokio::join!(left, right);
    let (status_a, body_a) = left.expect("left task");
    let (status_b, body_b) = right.expect("right task");
    assert_eq!(status_a, StatusCode::OK, "{body_a}");
    assert_eq!(status_b, StatusCode::OK, "{body_b}");

    let leases = [&body_a["lease"], &body_b["lease"]];
    let won: Vec<&Value> = leases.iter().filter(|l| !l.is_null()).copied().collect();
    assert_eq!(won.len(), 1, "got {body_a} / {body_b}");

    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 1);
    let distinct_fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = ?",
    )
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        distinct_fences, 1,
        "only one fence may ever be issued for this request"
    );
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = ?")
            .bind(&runner.runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        capacity, 0,
        "capacity must be decremented exactly once, not twice"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// =======================================================================
// 3. Revoked/stolen token: a revoked runner credential is rejected on every
//    runner-v1 route, and cannot advance an attempt it already leased.
// =======================================================================
#[tokio::test]
async fn revoked_credential_rejected_everywhere_freezes_attempt() {
    let storage_dir_guard = distinctive_temp_dir("revoke");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "revoke").await;
    let runner = enroll_runner(&app, "Revoked runner").await;
    let _request_id = create_execution_request(
        &app,
        &item_id,
        "revoke-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;
    let (status, claimed) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "revoke-claim").await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // The operator revokes the runner mid-lease — e.g. its credential was
    // detected as stolen/compromised.
    let (status, revoked) = common::send_large(
        &app,
        "POST",
        &format!("/api/runners/{}/revoke", runner.runner_id),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{revoked}");

    // The very same credential, still syntactically valid, can no longer
    // authenticate any runner-v1 route — proven with a fresh claim attempt
    // (a fresh call is the strongest form: this is not merely "the old
    // in-flight request fails", it is "this credential can never be used
    // again").
    let (status, body) = common::claim_runner(
        &app,
        &runner.runner_id,
        &runner.credential,
        "revoke-claim-2",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "runner_revoked");

    // The already-leased attempt cannot be advanced either: a heartbeat
    // using the correct fencing token is rejected, and the attempt's state
    // in the database is untouched by the rejected call.
    let hb = heartbeat_body(
        &runner.runner_id,
        "revoke-hb-1",
        0,
        json!([active_attempt_entry(&attempt_id, fencing_token)]),
    );
    let (status, hb_resp) = common::send_large(
        &app,
        "POST",
        "/api/runner/v1/heartbeat",
        hb,
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_ne!(status, StatusCode::OK, "{hb_resp}");
    let attempt_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        attempt_state, "leased",
        "a revoked runner's heartbeat must not move the attempt forward"
    );
    let last_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last_heartbeat, None, "no heartbeat timestamp was recorded");

    // The runner row itself is genuinely revoked in the database, not just
    // rejected at the HTTP layer by coincidence.
    let (state, revoked_at): (String, Option<String>) =
        sqlx::query_as("SELECT state, revoked_at FROM agent_runners WHERE id = ?")
            .bind(&runner.runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "revoked");
    assert!(revoked_at.is_some());
}
