//! A counted token is a new one: `tokens_in − cache_read + tokens_out`, the
//! same sum in the day's spend and in the Factory page's totals.

use crate::common;

use chrono::{TimeZone, Utc};
use tack_db::repo::{
    budgets::{counted_by_request, spent_today},
    metrics::{attempt_totals, profile_successful_run_tokens},
};

#[tokio::test]
async fn cache_reads_are_shown_and_never_counted() {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await.id.to_string();
    let at = "2026-10-18T09:00:00Z";
    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, ?, ?)",
    )
    .bind(at)
    .bind(at)
    .execute(repo.pool())
    .await
    .unwrap();

    // (request, usage): a cache read, no cache figure, and nothing measured.
    let rows = [
        (
            "R1",
            Some(
                r#"{"tokens_in":{"value":100000,"source":"measured"},"tokens_out":{"value":2000,"source":"measured"},"cache_read_tokens":90000}"#,
            ),
        ),
        (
            "R2",
            Some(
                r#"{"tokens_in":{"value":500,"source":"measured"},"tokens_out":{"value":100,"source":"measured"}}"#,
            ),
        ),
        ("R3", None),
    ];
    for (id, usage) in rows {
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
             repository_snapshot, permission_policy, metadata, created_at, updated_at) \
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', ?, ?)",
        )
        .bind(id)
        .bind(&item)
        .bind(id)
        .bind(at)
        .bind(at)
        .execute(repo.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
             fencing_token, lease_issued_at, lease_expires_at, created_at, updated_at, usage) \
             VALUES (?, ?, 1, 'run', 1, ?, ?, ?, ?, ?)",
        )
        .bind(format!("A-{id}"))
        .bind(id)
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(usage)
        .execute(repo.pool())
        .await
        .unwrap();
    }

    let project = project.id.to_string();
    let now = Utc.with_ymd_and_hms(2026, 10, 18, 12, 0, 0).unwrap();
    // 100 000 − 90 000 + 2 000 = 12 000, and 500 + 100 = 600 whole.
    assert_eq!(
        spent_today(repo.pool(), &project, now).await.unwrap(),
        12_600
    );
    // A run's own figure is the same sum.
    let ids = ["R1", "R2"].map(String::from);
    let by_run = counted_by_request(repo.pool(), &ids).await.unwrap();
    assert_eq!((by_run["R1"], by_run["R2"]), (12_000, 600));
    let totals = attempt_totals(repo.pool(), &project, None).await.unwrap();
    let split = totals.implementation;
    assert_eq!(
        (
            split.tokens,
            split.cache_read,
            split.attempts,
            split.unmeasured
        ),
        (12_600, 90_000, 3, 1)
    );
}

#[tokio::test]
async fn a_profiles_history_counts_its_last_successful_measured_runs() {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await.id.to_string();
    let at = "2026-10-18T09:00:00Z";
    for sql in [
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, '2026-10-18T09:00:00Z', '2026-10-18T09:00:00Z')",
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, created_at, updated_at) \
         VALUES ('P', 'P', 'x', '{}', '{}', 'custom', '2026-10-18T09:00:00Z', '2026-10-18T09:00:00Z')",
    ] {
        sqlx::query(sql).execute(repo.pool()).await.unwrap();
    }
    // (id, profile, attempt state, tokens out, what refused it): the first four count.
    let rows = [
        ("R1", "P", "succeeded", Some(1000), None),
        ("R2", "P", "succeeded", Some(3000), None),
        ("R3", "P", "succeeded", Some(2000), Some("mrp_accept")),
        ("R4", "P", "succeeded", Some(4000), None),
        ("R5", "P", "failed", Some(9000), None),
        ("R6", "P", "succeeded", None, None),
        ("R7", "P", "succeeded", Some(8000), Some("mrp_reject")),
        ("R8", "P", "succeeded", Some(8000), Some("attempt_rejected")),
        ("R9", "P", "succeeded", Some(8000), Some("rework")),
        ("R10", "other", "succeeded", Some(8000), None),
    ];
    for (n, (id, profile, state, out, refusal)) in rows.into_iter().enumerate() {
        let created = format!("2026-10-18T09:{n:02}:00Z");
        let profile = (profile == "P").then_some("P");
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_id, agent_profile_snapshot, \
             repository_snapshot, permission_policy, metadata, created_at, updated_at) \
             VALUES (?, ?, 's', ?, 'f', 'exact_runner', 'run', ?, '{}', '{}', '{}', '{}', ?, ?)",
        )
        .bind(id)
        .bind(&item)
        .bind(id)
        .bind(profile)
        .bind(&created)
        .bind(&created)
        .execute(repo.pool())
        .await
        .unwrap();
        let usage = out.map(|n| {
            format!(
                r#"{{"tokens_in":{{"value":{},"source":"measured"}},"tokens_out":{{"value":{n},"source":"measured"}},"cache_read_tokens":50000}}"#,
                n + 50_000
            )
        });
        sqlx::query(
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
             fencing_token, state, lease_issued_at, lease_expires_at, created_at, updated_at, usage) \
             VALUES (?, ?, 1, 'run', 1, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("A-{id}"))
        .bind(id)
        .bind(state)
        .bind(&created)
        .bind(&created)
        .bind(&created)
        .bind(&created)
        .bind(usage)
        .execute(repo.pool())
        .await
        .unwrap();
        match refusal {
            Some("mrp_accept" | "mrp_reject") => {
                let verdict = if refusal == Some("mrp_reject") {
                    "reject"
                } else {
                    "accept"
                };
                sqlx::query(
                    "INSERT INTO mrp_reviews (attempt_id, artifact_id, verdict) VALUES (?, 'x', ?)",
                )
                .bind(format!("A-{id}"))
                .bind(verdict)
                .execute(repo.pool())
                .await
                .unwrap();
            }
            Some("attempt_rejected") => {
                sqlx::query("INSERT INTO attempt_reviews (attempt_id, verdict, reviewed_at) VALUES (?, 'rejected', ?)")
                    .bind(format!("A-{id}"))
                    .bind(&created)
                    .execute(repo.pool())
                    .await
                    .unwrap();
            }
            Some("rework") => {
                sqlx::query(
                    "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
                     request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
                     repository_snapshot, permission_policy, metadata, created_at, updated_at) \
                     VALUES ('R9b', ?, 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', ?, ?)",
                )
                .bind(&item)
                .bind(format!("rework-A-{id}"))
                .bind(&created)
                .bind(&created)
                .execute(repo.pool())
                .await
                .unwrap();
            }
            _ => {}
        }
    }

    // Counted, newest first: the 50 000 cache read of each is left out, so each counts twice its output.
    let counted = profile_successful_run_tokens(repo.pool(), "P", 20)
        .await
        .unwrap();
    assert_eq!(counted, vec![8000, 4000, 6000, 2000]);
    // Only the last `limit`.
    let last_two = profile_successful_run_tokens(repo.pool(), "P", 2)
        .await
        .unwrap();
    assert_eq!(last_two, vec![8000, 4000]);
}
