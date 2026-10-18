//! A counted token is a new one: `tokens_in − cache_read + tokens_out`, the
//! same sum in the day's spend and in the Factory page's totals.

use crate::common;

use chrono::{TimeZone, Utc};
use tack_db::repo::{budgets::spent_today, metrics::attempt_totals};

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
