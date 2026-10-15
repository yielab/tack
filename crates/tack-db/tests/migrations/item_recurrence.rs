//! Migration 089: an item may repeat, and remembers the item it repeats.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_089_adds_nullable_recurrence_columns_and_checks_the_values() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "088_role_profile_fleet")
        .await
        .expect("up to 088");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO items (id, project_id, title, status, created_at, updated_at) VALUES ('i', 'p', 'T', 'To Do', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    migrations::run_all(&pool).await.expect("upgrade to 089");

    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT recurrence, recurrence_of FROM items WHERE id = 'i'")
            .fetch_one(&pool)
            .await
            .expect("item row");
    assert_eq!(row, (None, None));
    sqlx::query("UPDATE items SET recurrence = 'monthly' WHERE id = 'i'")
        .execute(&pool)
        .await
        .expect("a known value");
    assert!(
        sqlx::query("UPDATE items SET recurrence = 'daily' WHERE id = 'i'")
            .execute(&pool)
            .await
            .is_err(),
        "an unknown value is refused"
    );
}
