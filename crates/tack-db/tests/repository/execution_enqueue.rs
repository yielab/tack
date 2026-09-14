//! Enqueueing execution requests, snapshot validation, and the legacy
//! (pre-`request_snapshot`) schema-quarantine migration.

use crate::common::execution_fixture::{FakeClock, Fixture, request};

use chrono::Duration;
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{EnqueueResult, NewRunner, RequestSelection},
};

#[tokio::test]
async fn enqueue_rejects_malformed_and_contradictory_snapshots() {
    let fx = Fixture::new().await;
    let mut missing = fx.request("snapshot-missing", "snapshot-missing", "same");
    missing.request_snapshot = Box::leak(
        missing
            .request_snapshot
            .replace("\"metadata\":{\"source\":\"test\"}", "")
            .replace(",,", ",")
            .replace(",}", "}")
            .into_boxed_str(),
    );
    assert!(fx.repo.enqueue_execution(missing, &fx.clock).await.is_err());

    let mut malformed = fx.request("snapshot-malformed", "snapshot-malformed", "same");
    malformed.request_snapshot = "{not-json";
    assert!(
        fx.repo
            .enqueue_execution(malformed, &fx.clock)
            .await
            .is_err()
    );

    let mut contradictory = fx.request("snapshot-cross", "snapshot-cross", "same");
    contradictory.request_snapshot = Box::leak(
        contradictory
            .request_snapshot
            .replace(
                "\"request_id\":\"snapshot-cross\"",
                "\"request_id\":\"other\"",
            )
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(contradictory, &fx.clock)
            .await
            .is_err()
    );

    let mut wrong_created_at = fx.request("snapshot-clock", "snapshot-clock", "same");
    wrong_created_at.request_snapshot = Box::leak(
        wrong_created_at
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(wrong_created_at, &fx.clock)
            .await
            .is_err()
    );

    for (suffix, from, to) in [
        (
            "created-by",
            r#""created_by":{"source":"operator","subject_id":"test"}"#,
            r#""created_by":"operator""#,
        ),
        (
            "profile-policy",
            r#""tool_policy":{"mode":"safe"}"#,
            r#""tool_policy":"safe""#,
        ),
        ("budgets", r#""budgets":{"limit":1}"#, r#""budgets":1"#),
        ("repository-kind", r#""kind":"git""#, r#""kind":1"#),
        (
            "metadata",
            r#""metadata":{"source":"test"}"#,
            r#""metadata":false"#,
        ),
        (
            "environment",
            r#""value":"test","secret_reference":null"#,
            r#""value":"test","secret_reference":"secret://mode""#,
        ),
    ] {
        let id = Box::leak(format!("snapshot-{suffix}").into_boxed_str());
        let mut invalid = fx.request(id, id, "same");
        invalid.request_snapshot =
            Box::leak(invalid.request_snapshot.replace(from, to).into_boxed_str());
        assert!(
            fx.repo.enqueue_execution(invalid, &fx.clock).await.is_err(),
            "{suffix}"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// Inserts a legacy (pre-`request_snapshot`) execution request row, every
/// column but `id`/`key`/`state` fixed at a valid placeholder.
async fn insert_legacy_request(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    key: &str,
    state: &str,
) {
    sqlx::query("INSERT INTO execution_requests(id,item_id,idempotency_scope,idempotency_key,request_fingerprint,state,selector_kind,selector_id,agent_profile_snapshot,repository_snapshot,permission_policy,created_at,updated_at) VALUES(?, 'legacy-item', 'legacy', ?, 'legacy-fingerprint', ?, 'exact_runner', 'runner-a', '{}', '{}', '{}', '2026-08-07T12:00:00Z', '2026-08-07T12:00:00Z')")
        .bind(id)
        .bind(key)
        .bind(state)
        .execute(conn)
        .await
        .unwrap();
}

/// Overwrites a legacy row's `request_snapshot` once the column exists.
async fn set_legacy_snapshot(pool: &sqlx::SqlitePool, id: &str, key: &str, snapshot: &str) {
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot=? WHERE id=? AND idempotency_key=?",
    )
    .bind(snapshot)
    .bind(id)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn m060_quarantines_all_nonterminal_malformed_legacy_snapshots() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "052_execution_report_replays")
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    for (id, key, state) in [
        ("legacy-request", "legacy-key", "queued"),
        ("legacy-partial", "legacy-partial-key", "leased"),
        ("legacy-malformed", "legacy-malformed-key", "running"),
        ("legacy-terminal", "legacy-terminal-key", "succeeded"),
        ("legacy-created-at", "legacy-created-at-key", "queued"),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            "queued",
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            "queued",
        ),
        ("legacy-valid", "legacy-valid-key", "queued"),
    ] {
        insert_legacy_request(&mut connection, id, key, state).await;
    }
    drop(connection);
    migrations::run_up_to(&pool, "058_execution_recovery_replay_response")
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{\"created_by\":{},\"selector\":{},\"repository\":{}}' WHERE id = 'legacy-partial'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{not-json' WHERE id IN ('legacy-malformed', 'legacy-terminal')")
        .execute(&pool)
        .await
        .unwrap();
    type Mutator = fn(&str) -> String;
    let mutations: [(&str, &str, Mutator); 4] = [
        ("legacy-created-at", "legacy-created-at-key", |s| {
            s.replace("2026-08-07T12:00:00Z", "2026-08-07 12:00:00")
        }),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            |s| s.replace("\"timeout_seconds\":60", "\"timeout_seconds\":-1"),
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            |s| s.replace("\"timeout_seconds\":60", "\"timeout_seconds\":60.5"),
        ),
        ("legacy-valid", "legacy-valid-key", |s| s.to_owned()),
    ];
    for (id, key, mutate) in mutations {
        let snapshot = mutate(request(id, "legacy-item", key, "same").request_snapshot);
        set_legacy_snapshot(&pool, id, key, &snapshot).await;
    }
    migrations::run_up_to(&pool, "059_quarantine_legacy_execution_request_snapshots")
        .await
        .unwrap();
    let m059_cohort: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, state FROM execution_requests WHERE id LIKE 'legacy-%timeout' OR id = 'legacy-created-at' OR id = 'legacy-valid' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        m059_cohort,
        vec![
            ("legacy-created-at".into(), "queued".into()),
            ("legacy-fractional-timeout".into(), "queued".into()),
            ("legacy-negative-timeout".into(), "queued".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    migrations::run_all(&pool).await.unwrap();
    let after: Vec<(String, String)> =
        sqlx::query_as("SELECT id, state FROM execution_requests ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        after,
        vec![
            ("legacy-created-at".into(), "needs_operator".into()),
            ("legacy-fractional-timeout".into(), "needs_operator".into()),
            ("legacy-malformed".into(), "needs_operator".into()),
            ("legacy-negative-timeout".into(), "needs_operator".into()),
            ("legacy-partial".into(), "needs_operator".into()),
            ("legacy-request".into(), "needs_operator".into()),
            ("legacy-terminal".into(), "succeeded".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    let repo = Repository::new(pool);
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-a",
            name: "Runner A",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-legacy-valid",
            "attempt-legacy-valid",
            Duration::seconds(60),
            &clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap()
        .expect("the later valid queued row remains claimable");
    assert_eq!(claimed.lease.request_id, "legacy-valid");
}

#[tokio::test]
async fn old_schema_upgrades_to_all_ten_execution_tables() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .unwrap();
    migrations::run_all(&pool).await.unwrap();
    for table in [
        "agent_fleets",
        "agent_runners",
        "agent_fleet_members",
        "agent_profiles",
        "model_profiles",
        "execution_requests",
        "execution_attempts",
        "execution_events",
        "execution_artifacts",
        "execution_decisions",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{table} missing after upgrade");
    }
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations WHERE name BETWEEN '039_agent_fleets' AND '048_execution_decisions'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(migration_count, 10);
}

#[tokio::test]
async fn enqueue_is_idempotent_and_conflicting_reuse_is_rejected() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-c", "key-a", "different").await,
        EnqueueResult::Conflict
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn enqueue_replay_compares_frozen_snapshot_before_current_time() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    fx.clock.advance(Duration::seconds(1));
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into()),
        "an exact retry uses the original frozen snapshot rather than the new clock"
    );
    let mut changed_timestamp = fx.request("request-a", "key-a", "same");
    changed_timestamp.request_snapshot = Box::leak(
        changed_timestamp
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert_eq!(
        fx.repo
            .enqueue_execution(changed_timestamp, &fx.clock)
            .await
            .unwrap(),
        EnqueueResult::Conflict,
        "same idempotency key with a changed frozen timestamp is not an exact replay"
    );
}
