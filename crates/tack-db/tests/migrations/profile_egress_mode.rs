//! Migration 111: a profile carries its web access mode. A role that reads and judges starts
//! closed, one that browses starts open, every other role (and a new column's default) listed.

use tack_db::{init_pool, migrations};

async fn insert(pool: &sqlx::SqlitePool, id: &str, kind: &str) {
    sqlx::query(
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, kind, created_at, updated_at) \
         VALUES (?, ?, 'i', '{}', '{}', ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(id)
    .bind(kind)
    .execute(pool)
    .await
    .expect("profile");
}

async fn mode(pool: &sqlx::SqlitePool, id: &str) -> String {
    sqlx::query_scalar("SELECT egress_mode FROM agent_profiles WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("mode")
}

#[tokio::test]
async fn migration_111_gives_each_existing_profile_the_mode_of_its_kind() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "110_prompt_template_sha")
        .await
        .expect("up to 110");
    for kind in ["implementer", "reviewer", "planner", "researcher", "custom"] {
        insert(&pool, kind, kind).await;
    }
    migrations::run_all(&pool).await.expect("111");
    assert_eq!(mode(&pool, "implementer").await, "listed");
    assert_eq!(mode(&pool, "reviewer").await, "closed");
    assert_eq!(mode(&pool, "planner").await, "closed");
    assert_eq!(mode(&pool, "researcher").await, "open");
    assert_eq!(mode(&pool, "custom").await, "listed");
}

#[tokio::test]
async fn a_new_profile_without_a_mode_is_listed_and_the_column_refuses_any_other_word() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("all");
    insert(&pool, "mine", "custom").await;
    assert_eq!(mode(&pool, "mine").await, "listed");
    let refused = sqlx::query("UPDATE agent_profiles SET egress_mode = 'wide' WHERE id = 'mine'")
        .execute(&pool)
        .await;
    assert!(refused.is_err(), "an unknown mode is not stored");
}
