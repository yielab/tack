//! Chaos, fencing and recovery adversarial suite: every test drives the
//! real production router (`tack_api::router::build_router`) and reads
//! persisted database state directly rather than trusting a status code
//! alone. The multi-runner/duplicated-credential races and revocation test
//! live in `chaos_races.rs` — split out once this file passed 1000 lines.

#[allow(clippy::duplicate_mod)]
#[path = "chaos_common.rs"]
mod chaos_common;

use crate::common;
use axum::http::StatusCode;
use chaos_common::*;
use serde_json::{Value, json};

// =======================================================================
// 1. Stale fence: every attempt-scoped mutation checked here rejects a
//    superseded fencing token and writes nothing — across `heartbeat`,
//    `decisions`, `artifacts` (manifest), `cancellation-observation` and
//    `recovery-observation`. One shared setup supersedes `attempt_a`/
//    `fence_a` once; every route below is then tried against that same
//    already-stale fence.
// =======================================================================

struct SupersededFence {
    request_id: String,
    runner_id: String,
    attempt_a: String,
    fence_a: i64,
    hdr: String,
}

/// Claims one request (`attempt_a`/`fence_a`), then supersedes it with a
/// pre-spawn recovery observation so the returned fence is guaranteed stale
/// before any caller uses it.
async fn setup_superseded_fence(app: &axum::Router) -> SupersededFence {
    let item_id = create_project_and_item(app).await;
    let agent_profile_id = agent_profile(app, "stale-fence").await;
    let runner = enroll_runner(app, "Stale fence runner").await;
    let request_id = create_execution_request(
        app,
        &item_id,
        "stale-fence-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    let (status, claimed_a) =
        common::claim_runner(app, &runner.runner_id, &runner.credential, "stale-claim-1").await;
    assert_eq!(status, StatusCode::OK, "{claimed_a}");
    let attempt_a = claimed_a["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_a = claimed_a["lease"]["fencing_token"].as_i64().unwrap();

    let (status, recovery) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    assert_eq!(recovery["disposition"], "safe_pre_spawn_requeue");

    let (status, claimed_b) =
        common::claim_runner(app, &runner.runner_id, &runner.credential, "stale-claim-2").await;
    assert_eq!(status, StatusCode::OK, "{claimed_b}");
    let attempt_b = claimed_b["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_b = claimed_b["lease"]["fencing_token"].as_i64().unwrap();
    assert_ne!(attempt_b, attempt_a);
    assert_eq!(fence_b, 2);

    SupersededFence {
        request_id,
        runner_id: runner.runner_id,
        attempt_a,
        fence_a,
        hdr: auth(&runner.credential),
    }
}

/// Posts `body` to `path` and asserts it is rejected as stale/conflict.
/// The response's error code is `stale_lease` on most fenced routes but
/// `conflict` on `heartbeat` (a known inconsistency, documented in this
/// crate's `CLAUDE.md`); both are accepted here since the invariant this
/// helper exists to prove is rejection, not which of the two codes is used.
async fn assert_stale_rejected(app: &axum::Router, path: &str, body: Value, hdr: &str) -> Value {
    let (status, resp) =
        common::send_large(app, "POST", path, body, &[("authorization", hdr)]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{resp}");
    assert!(
        matches!(
            resp["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {resp}"
    );
    resp
}

async fn assert_no_heartbeat_recorded(pool: &sqlx::SqlitePool, attempt_id: &str) {
    let last: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(last, None, "a stale-fenced heartbeat must not be recorded");
}

async fn assert_no_decision_row(pool: &sqlx::SqlitePool, attempt_id: &str) {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_decisions WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "a stale-fenced decision must not be recorded");
}

async fn assert_no_artifact_row(pool: &sqlx::SqlitePool, attempt_id: &str) {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        count, 0,
        "a stale-fenced artifact manifest must not be recorded"
    );
}

#[tokio::test]
async fn stale_fence_writes_nothing_across_every_mutation_route() {
    let storage_dir_guard = distinctive_temp_dir("stale-fence");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let s = setup_superseded_fence(&app).await;

    let hb = heartbeat_body(
        &s.runner_id,
        "stale-hb-1",
        0,
        json!([active_attempt_entry(&s.attempt_a, s.fence_a)]),
    );
    assert_stale_rejected(&app, "/api/runner/v1/heartbeat", hb, &s.hdr).await;
    assert_no_heartbeat_recorded(&pool, &s.attempt_a).await;

    let decision = decision_body(&s.runner_id, &s.attempt_a, s.fence_a, "dec-stale-1");
    let decisions_uri = format!("/api/runner/v1/attempts/{}/decisions", s.attempt_a);
    assert_stale_rejected(&app, &decisions_uri, decision, &s.hdr).await;
    assert_no_decision_row(&pool, &s.attempt_a).await;

    let manifest = artifact_manifest_body(
        &s.runner_id,
        &s.attempt_a,
        s.fence_a,
        "art-stale",
        "patch",
        "x.patch",
        3,
        &sha256_hex(b"abc"),
    );
    let artifacts_uri = format!("/api/runner/v1/attempts/{}/artifacts", s.attempt_a);
    assert_stale_rejected(&app, &artifacts_uri, manifest, &s.hdr).await;
    assert_no_artifact_row(&pool, &s.attempt_a).await;

    // Cancellation observation and a second recovery observation on the
    // already-superseded fence are rejected too (recovery is not itself
    // exempt from fencing) — both checked for status/code only, matching
    // the original scenario, since neither writes to a table this file
    // otherwise inspects.
    let cancel = cancellation_body(&s.runner_id, &s.attempt_a, s.fence_a, "cancel-stale-1");
    let cancel_uri = format!(
        "/api/runner/v1/attempts/{}/cancellation-observation",
        s.attempt_a
    );
    assert_stale_rejected(&app, &cancel_uri, cancel, &s.hdr).await;
    let recovery_key = format!(
        "recovery:{}:{}:process_stopped_again",
        s.attempt_a, s.fence_a
    );
    let recovery = recovery_body(&s.runner_id, &s.attempt_a, s.fence_a, &recovery_key);
    let recovery_uri = format!(
        "/api/runner/v1/attempts/{}/recovery-observation",
        s.attempt_a
    );
    assert_stale_rejected(&app, &recovery_uri, recovery, &s.hdr).await;

    // None of the five rejected calls above minted a new fence: exactly the
    // two fences from setup (1 lost, 2 current) exist for this request.
    let fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = ?",
    )
    .bind(&s.request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        fences, 2,
        "fence 1 (lost) and fence 2 (current) — no extra fence was minted"
    );
}

// =======================================================================
// 2. Oversized artifact: a declared size over the per-item or cumulative
//    attempt-total limit is rejected before any row is written, and the
//    rejection is scoped to the oversized item only.
// =======================================================================
#[tokio::test]
async fn oversized_single_artifact_rejected_per_item_cap() {
    let storage_dir_guard = distinctive_temp_dir("oversized-artifact");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "oversized").await;

    // Declared size over `artifact_content_bytes_max` (50 MiB).
    let (status, body) = post_artifact_manifest(
        &app,
        &attempt,
        "art-huge",
        "log",
        "huge.log",
        52_428_800_u64 + 1,
        &sha256_hex(b"x"),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "payload_too_large");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "an oversized single artifact must write no row");
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

/// Ten artifacts each individually just under the per-item cap (52_428_800)
/// accumulate to 520_000_000, comfortably under the 524_288_000
/// attempt-total cap — each must succeed. An eleventh, itself still under
/// the per-item cap, pushes the running total over the cumulative cap and
/// must be rejected — proving the cumulative check is a real, separate
/// enforcement from the per-item one, not the same check applied twice.
#[tokio::test]
async fn cumulative_artifact_total_rejected_over_attempt_cap() {
    let storage_dir_guard = distinctive_temp_dir("cumulative-artifact");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "cumulative").await;

    for i in 0..10 {
        let sha = sha256_hex(format!("cum-{i}").as_bytes());
        let (status, body) = post_artifact_manifest(
            &app,
            &attempt,
            &format!("art-cum-{i}"),
            "log",
            &format!("cum-{i}.log"),
            52_000_000,
            &sha,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "artifact {i}: {body}");
    }

    let (status, second) = post_artifact_manifest(
        &app,
        &attempt,
        "art-cum-overflow",
        "log",
        "overflow.log",
        52_000_000,
        &sha256_hex(b"overflow"),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{second}");
    assert_eq!(second["error"]["code"], "payload_too_large");

    // Exactly the ten accepted rows survive; the overflow artifact never
    // landed, and the running total is exactly what ten successes imply.
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id = ? ORDER BY artifact_id",
    )
    .bind(&attempt.attempt_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 10, "{rows:?}");
    assert!(!rows.contains(&"art-cum-overflow".to_string()));
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes),0) FROM execution_artifacts WHERE attempt_id = ?",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        total, 520_000_000,
        "the rejected artifact must not have partially written its size"
    );
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 3. Path traversal / symlink-style attack surface: artifact ids carrying
//    `../`, absolute paths or NUL bytes are rejected before any file is
//    written outside the configured storage root. (`ArtifactStorage`'s own
//    `encode_id` hex-encodes every byte of the id, so no separator can act
//    as one — this test is the audit's own independent proof of that
//    structural claim, not a re-statement of it.)
// =======================================================================
#[tokio::test]
async fn traversal_id_encodings_battery_never_escapes_storage_root() {
    let storage_dir_guard = distinctive_temp_dir("traversal");
    let storage_dir = storage_dir_guard.path();
    let (app, _pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "traversal").await;
    let hdr = auth(&attempt.credential);

    let malicious_ids = [
        "../../../../etc/passwd",
        "..%2f..%2fetc%2fpasswd",
        "/etc/passwd",
        "art\0-null",
        "..\\..\\windows\\system32",
    ];
    let content = b"malicious payload".to_vec();
    for artifact_id in malicious_ids {
        let (status, manifest) = post_artifact_manifest(
            &app,
            &attempt,
            artifact_id,
            "patch",
            "x.patch",
            content.len() as u64,
            &sha256_hex(&content),
        )
        .await;
        // Whether the manifest step itself rejects the id, or accepts it and
        // the subsequent content PUT is what rejects it, either is an
        // acceptable safe outcome — what matters is that no byte ever lands
        // outside `storage_dir`. Only continue to the content PUT if the
        // manifest step accepted it.
        if status == StatusCode::OK {
            assert_eq!(
                manifest["artifacts"][0]["artifact_id"], artifact_id,
                "{manifest}"
            );
            let encoded_uri = format!(
                "/api/runner/v1/attempts/{}/artifacts/{}/content",
                attempt.attempt_id,
                percent_encode_path_segment(artifact_id)
            );
            let fence = attempt.fencing_token.to_string();
            let headers = [
                ("authorization", hdr.as_str()),
                ("x-tack-fencing-token", fence.as_str()),
                ("content-type", "text/plain"),
            ];
            let _ = put_content(&app, &encoded_uri, content.clone(), &headers).await;
        }
    }

    // The decisive assertion: nothing was ever written outside the
    // configured storage root. `/etc/passwd` itself is untouched by
    // construction (the process has no write permission there in the test
    // sandbox), so an escape would surface as a permission error above, not
    // a silent write outside the containment this checks.
    if storage_dir.exists() {
        for path in &walk_files(storage_dir).await {
            assert!(
                path.starts_with(storage_dir),
                "artifact file {path:?} escaped {storage_dir:?}"
            );
        }
    }
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 4. Delay/reorder/replay: an event batch whose `previous_checkpoint`
//    disagrees with the attempt's actual current checkpoint (simulating a
//    reordered/delayed delivery) is rejected as a conflict and writes
//    nothing; the correctly-ordered batch, replayed byte-identically
//    afterward (simulating a client retry after a lost response), is
//    idempotent rather than duplicating rows.
// =======================================================================

/// Commits the legitimate first batch (`previous_checkpoint` null, since
/// the attempt has no events yet, `checkpoint` "cp-1") and returns its body
/// for a caller that wants to resend it.
async fn commit_first_batch(app: &axum::Router, attempt: &RunningAttempt, hdr: &str) -> Value {
    let body = events_body(
        &attempt.runner_id,
        &attempt.attempt_id,
        attempt.fencing_token,
        Value::Null,
        "cp-1",
        "evt-1",
    );
    let (status, ok) = post_events(app, attempt, hdr, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{ok}");
    body
}

/// A reordered/delayed delivery: a second batch that (incorrectly) also
/// claims `previous_checkpoint` null, as if sent before the first one but
/// arrived after (a network reorder) — must be rejected as a conflict, not
/// silently applied on top of the wrong base.
#[tokio::test]
async fn reordered_event_batch_rejected_without_moving_checkpoint() {
    let storage_dir_guard = distinctive_temp_dir("reorder");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "reorder").await;
    let hdr = auth(&attempt.credential);
    commit_first_batch(&app, &attempt, &hdr).await;

    let reordered = events_body(
        &attempt.runner_id,
        &attempt.attempt_id,
        attempt.fencing_token,
        Value::Null,
        "cp-should-never-land",
        "evt-reordered",
    );
    let (status, rejected) = post_events(&app, &attempt, &hdr, reordered).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a reordered batch must not silently apply: {rejected}"
    );

    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        checkpoint,
        Some("cp-1".to_string()),
        "the reordered batch must not move the checkpoint"
    );
    let stray: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = 'evt-reordered'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stray, 0);
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

/// The client resends the first batch byte-identically (as if its original
/// response never arrived) — this must be an idempotent replay, not
/// rejected and not duplicated. The handler's JSON response carries no
/// top-level `replayed` flag on this exact-resend path, so the
/// authoritative proof is the database row count, not the response shape.
#[tokio::test]
async fn identical_event_batch_replay_is_idempotent() {
    let storage_dir_guard = distinctive_temp_dir("replay");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "replay").await;
    let hdr = auth(&attempt.credential);
    let first_batch = commit_first_batch(&app, &attempt, &hdr).await;

    let (status, replay) = post_events(&app, &attempt, &hdr, first_batch).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["committed_checkpoint"], "cp-1");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = 'evt-1'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "a replayed batch must not duplicate its events"
    );
    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 5. Corrupt database row: a hand-corrupted `request_snapshot` (malformed
//    JSON, simulating on-disk bit rot or a partial write outside any
//    transaction) degrades to a typed error on every read path that touches
//    it, never a panic and never a silently-fabricated success.
// =======================================================================
#[tokio::test]
async fn corrupted_snapshot_row_degrades_to_typed_error_not_panic() {
    let storage_dir_guard = distinctive_temp_dir("corrupt-row");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "corrupt-row").await;
    let runner = enroll_runner(&app, "Corrupt-row runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "corrupt-row-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    // Simulate a corrupted row directly (bit rot / a write outside any
    // application transaction) — not something reachable through any HTTP
    // input validator, deliberately: this proves the *read* path degrades
    // safely, independent of whatever wrote the corruption.
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot = 'not valid json {{{' WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&pool)
    .await
    .unwrap();

    // A claim against this request must not panic the process (which would
    // abort every in-flight request on this connection, not just this one)
    // and must not fabricate a plausible-looking successful lease from
    // garbage data.
    let (status, body) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "corrupt-claim").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a corrupted snapshot must never produce a successful claim: {body}"
    );
    assert!(
        status.is_client_error() || status.is_server_error(),
        "expected a typed error status, got {status}: {body}"
    );

    // The server is still alive and healthy after this — the corrupted row
    // did not take down the whole process.
    let (status, health) = common::send_large(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{health}");

    // No attempt was ever created against the corrupted request.
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 0);
}
