//! Migration 101: every project that exists starts at the Propose rung with no creates allowed,
//! and a change set is kept once per idempotency key.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_101_leaves_existing_projects_at_propose_with_no_creates() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "100_team_activity")
        .await
        .expect("up to 100");
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES ('w', 'W', '{}')")
        .execute(&pool)
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, project_type, created_at, updated_at) \
         VALUES ('p', 'w', 'P', 'software', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("project at 100");

    migrations::run_all(&pool).await.expect("upgrade to 101");

    let rung: (String, i64, i64) = sqlx::query_as(
        "SELECT agents_on_board, agents_create_per_set, agents_create_per_day FROM projects WHERE id = 'p'",
    )
    .fetch_one(&pool)
    .await
    .expect("rung");
    assert_eq!(rung, ("propose".to_string(), 0, 0));

    let insert = |id: &str| {
        sqlx::query(
            "INSERT INTO change_sets (id, project_id, principal, origin, reason, idempotency_key, ops, status, created_at) \
             VALUES (?, 'p', 'agent:x', 'mcp', 'r', 'k', '[]', 'waiting', '2026-01-01T00:00:00Z')",
        )
        .bind(id.to_string())
    };
    insert("a").execute(&pool).await.expect("first");
    assert!(insert("b").execute(&pool).await.is_err(), "same key twice");
}
