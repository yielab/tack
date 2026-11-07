//! Migration 109: the pack review and the run review are one record per attempt. A pack review
//! joins the run's row (or becomes it) with its reason, and what the operator saw of a pack stays.

use sqlx::Row;
use tack_db::{init_pool, migrations};

/// `(verdict, reason, reviewed_by, artifact_id, reviewed_at)` of an attempt's one review row.
async fn review(
    pool: &sqlx::SqlitePool,
    attempt: &str,
) -> Vec<(
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
)> {
    sqlx::query(
        "SELECT verdict, reason, reviewed_by, artifact_id, reviewed_at FROM attempt_reviews WHERE attempt_id = ?",
    )
    .bind(attempt)
    .fetch_all(pool)
    .await
    .expect("review rows")
    .iter()
    .map(|r| {
        (
            r.get("verdict"),
            r.get("reason"),
            r.get("reviewed_by"),
            r.get("artifact_id"),
            r.get("reviewed_at"),
        )
    })
    .collect()
}

#[tokio::test]
async fn migration_109_joins_the_two_review_stores_into_one_row_per_attempt() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "108_drop_boards")
        .await
        .expect("up to 108");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO items (id, project_id, title, status) VALUES ('item', 'p', 'T', 'To Do')",
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, available_capacity, \
         capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('run', 'r', 'h', 'active', '{}', 1, 1, '{}', 1, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    for attempt in ["A1", "A2", "A3", "A4"] {
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, request_fingerprint, \
             selector_kind, selector_id, agent_profile_snapshot, repository_snapshot, permission_policy, metadata, \
             created_at, updated_at) \
             VALUES (?, 'item', 's', ?, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
        )
        .bind(format!("R-{attempt}"))
        .bind(attempt)
        .execute(&pool)
        .await
        .expect("request");
        sqlx::query(
            "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, state, \
             lease_issued_at, lease_expires_at, created_at, updated_at) \
             VALUES (?, ?, 1, 'run', 1, 'succeeded', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
        )
        .bind(attempt)
        .bind(format!("R-{attempt}"))
        .execute(&pool)
        .await
        .expect("attempt");
    }
    for sql in [
        // A1: both stores hold a verdict, the run's has no note.
        "INSERT INTO attempt_reviews (attempt_id, verdict, note, reviewed_at) VALUES ('A1', 'accepted', NULL, '2026-11-01T10:00:00Z')",
        "INSERT INTO mrp_reviews VALUES ('A1', 'pack-1', 'accept', 'looks right', '2026-11-01T09:50:00Z', '2026-11-01T09:59:00Z', 'operator')",
        // A2: only the pack was reviewed.
        "INSERT INTO mrp_reviews VALUES ('A2', 'pack-2', 'reject', 'misses the edge case', NULL, '2026-11-02T10:00:00Z', 'operator')",
        // A3: only the run was reviewed, with a note.
        "INSERT INTO attempt_reviews (attempt_id, verdict, note, reviewed_at) VALUES ('A3', 'rejected', 'wrong files', '2026-11-03T10:00:00Z')",
        // A4: the pack was only opened.
        "INSERT INTO mrp_reviews (attempt_id, artifact_id, viewed_at) VALUES ('A4', 'pack-4', '2026-11-04T10:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    for (attempt, artifact) in [("A1", "pack-1"), ("A2", "pack-2"), ("A4", "pack-4")] {
        sqlx::query(
            "INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, created_at) \
             VALUES (?, ?, ?, 'report', 'mrp.json', 'application/vnd.tack.mrp+json', 1, 'x', '2026-11-01T09:00:00Z')",
        )
        .bind(format!("row-{artifact}"))
        .bind(attempt)
        .bind(artifact)
        .execute(&pool)
        .await
        .expect("artifact");
    }

    migrations::run_all(&pool).await.expect("upgrade to 109");

    // Two source rows, one destination row: the run's verdict and time, the pack's reason.
    // The pack's constant "operator" named nobody, so the reviewer stays unknown.
    assert_eq!(
        review(&pool, "A1").await,
        vec![(
            "accepted".into(),
            Some("looks right".into()),
            None,
            Some("pack-1".into()),
            "2026-11-01T10:00:00Z".into()
        )]
    );
    assert_eq!(
        review(&pool, "A2").await,
        vec![(
            "rejected".into(),
            Some("misses the edge case".into()),
            None,
            Some("pack-2".into()),
            "2026-11-02T10:00:00Z".into()
        )]
    );
    assert_eq!(
        review(&pool, "A3").await,
        vec![(
            "rejected".into(),
            Some("wrong files".into()),
            None,
            None,
            "2026-11-03T10:00:00Z".into()
        )]
    );
    // An opened pack is not a verdict: no review row, and the time it was opened moved to the pack.
    assert!(review(&pool, "A4").await.is_empty());
    let viewed: Vec<(String, Option<String>)> =
        sqlx::query("SELECT attempt_id, viewed_at FROM execution_artifacts ORDER BY attempt_id")
            .fetch_all(&pool)
            .await
            .expect("artifacts")
            .iter()
            .map(|r| (r.get("attempt_id"), r.get("viewed_at")))
            .collect();
    assert_eq!(
        viewed,
        vec![
            ("A1".into(), Some("2026-11-01T09:50:00Z".into())),
            ("A2".into(), None),
            ("A4".into(), Some("2026-11-04T10:00:00Z".into())),
        ]
    );
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(&pool)
            .await
            .expect("tables");
    assert!(!tables.contains(&"mrp_reviews".to_owned()));
}
