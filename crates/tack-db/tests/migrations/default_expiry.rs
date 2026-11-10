//! Migration 113: a pending decision and an active runner credential made before the defaults
//! existed get an expiry, and the sweep then settles an overdue decision the ordinary way.

use chrono::{DateTime, Duration, Utc};
use tack_db::{Repository, init_pool, migrations};

async fn expiry(pool: &sqlx::SqlitePool, table: &str, id: &str) -> Option<String> {
    let sql = match table {
        "agent_runners" => "SELECT credential_expires_at FROM agent_runners WHERE id = ?",
        _ => "SELECT expires_at FROM execution_decisions WHERE id = ?",
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("expiry")
}

#[tokio::test]
async fn migration_113_gives_old_rows_their_expiry_and_the_sweep_settles_the_overdue_ones() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "112_drop_rate_card")
        .await
        .expect("up to 112");
    let now: DateTime<Utc> = Utc::now();
    let days_ago = |days: i64| (now - Duration::days(days)).to_rfc3339();
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO items (id, project_id, title, status) VALUES ('item', 'p', 'T', 'To Do')",
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, request_fingerprint, \
         selector_kind, selector_id, agent_profile_snapshot, repository_snapshot, permission_policy, metadata, \
         created_at, updated_at) \
         VALUES ('R', 'item', 's', 'k', 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    // Active, pending enrollment, revoked, and one that already holds an expiry.
    for (id, state, revoked, expires) in [
        ("run-active", "active", None, None),
        ("run-pending", "pending_enrollment", None, None),
        ("run-revoked", "revoked", Some("2026-11-02T00:00:00Z"), None),
        ("run-set", "active", None, Some("2026-12-01T00:00:00+00:00")),
    ] {
        sqlx::query(
            "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, available_capacity, \
             capability_snapshot, protocol_version, revoked_at, credential_expires_at, created_at, updated_at) \
             VALUES (?, ?, 'h', ?, '{}', 1, 1, '{}', 1, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(state)
        .bind(revoked)
        .bind(expires)
        .bind(days_ago(200))
        .bind(days_ago(200))
        .execute(&pool)
        .await
        .expect("runner");
    }
    sqlx::query(
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, state, \
         lease_issued_at, lease_expires_at, created_at, updated_at) \
         VALUES ('A', 'R', 1, 'run-active', 1, 'waiting_decision', '2026-11-01T00:00:00Z', '2099-01-01T00:00:00Z', \
         '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("attempt");
    // Created three days ago (overdue), one hour ago (still waiting), already settled, and with an
    // expiry of its own.
    let own = (now + Duration::hours(2)).to_rfc3339();
    for (id, state, created, expires) in [
        ("old", "pending", days_ago(3), None),
        (
            "recent",
            "pending",
            (now - Duration::hours(1)).to_rfc3339(),
            None,
        ),
        ("settled", "resolved", days_ago(3), None),
        ("own", "pending", days_ago(3), Some(own.clone())),
    ] {
        sqlx::query(
            "INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, state, prompt, expires_at, created_at, updated_at) \
             VALUES (?, 'A', ?, 'approval', ?, 'ok?', ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(state)
        .bind(expires)
        .bind(&created)
        .bind(&created)
        .execute(&pool)
        .await
        .expect("decision");
    }

    migrations::run_all(&pool).await.expect("113");

    let at = |raw: Option<String>| {
        DateTime::parse_from_rfc3339(&raw.expect("an expiry")).expect("a timestamp")
    };
    let old_created = DateTime::parse_from_rfc3339(&days_ago(3)).unwrap();
    let old_expires = at(expiry(&pool, "execution_decisions", "old").await);
    assert!(
        (old_expires - (old_created + Duration::hours(24)))
            .num_seconds()
            .abs()
            <= 1,
        "{old_expires}"
    );
    let recent_expires = at(expiry(&pool, "execution_decisions", "recent").await);
    assert!(recent_expires > now, "{recent_expires}");
    assert_eq!(expiry(&pool, "execution_decisions", "settled").await, None);
    assert_eq!(expiry(&pool, "execution_decisions", "own").await, Some(own));

    let active = at(expiry(&pool, "agent_runners", "run-active").await);
    assert!(
        (active.to_utc() - (now + Duration::days(90)))
            .num_seconds()
            .abs()
            <= 60,
        "{active}"
    );
    assert_eq!(expiry(&pool, "agent_runners", "run-pending").await, None);
    assert_eq!(expiry(&pool, "agent_runners", "run-revoked").await, None);
    assert_eq!(
        expiry(&pool, "agent_runners", "run-set").await.as_deref(),
        Some("2026-12-01T00:00:00+00:00")
    );

    // The sweep closes the overdue one as it closes any other, and leaves the rest.
    let repo = Repository::new(pool.clone());
    let expired = repo
        .expire_overdue_execution_decisions(now, r#"{"kind":"system"}"#)
        .await
        .expect("sweep");
    assert_eq!(expired, 1);
    let states: Vec<(String, String)> =
        sqlx::query_as("SELECT id, state FROM execution_decisions ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("states");
    assert_eq!(
        states,
        [
            ("old", "expired"),
            ("own", "pending"),
            ("recent", "pending"),
            ("settled", "resolved"),
        ]
        .map(|(id, state)| (id.to_owned(), state.to_owned()))
    );
}
