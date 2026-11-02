//! Migration 102: items gain a `hidden` flag that is off for every item that exists.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_102_leaves_existing_items_visible() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "101_change_sets")
        .await
        .expect("up to 101");
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
    .expect("project");
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) VALUES ('i', 'p', 'T', 'backlog')",
    )
    .execute(&pool)
    .await
    .expect("item at 101");

    migrations::run_all(&pool).await.expect("upgrade to 102");

    let hidden: i64 = sqlx::query_scalar("SELECT hidden FROM items WHERE id = 'i'")
        .fetch_one(&pool)
        .await
        .expect("hidden");
    assert_eq!(hidden, 0);
}
