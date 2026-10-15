//! Migration 088: a role may name an agent profile; a profile may name a fleet.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_088_adds_nullable_role_profile_and_profile_fleet() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "087_autonomy")
        .await
        .expect("up to 087");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO roles (id, project_id, name, color, created_at) VALUES ('r', 'p', 'Content', '#fff', '2026-01-01T00:00:00Z')",
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) VALUES ('ap', 'Mine', 'x', '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    migrations::run_all(&pool).await.expect("upgrade to 088");

    let role: (Option<String>,) =
        sqlx::query_as("SELECT agent_profile_id FROM roles WHERE id = 'r'")
            .fetch_one(&pool)
            .await
            .expect("role row");
    assert_eq!(role.0, None);
    let profile: (Option<String>,) =
        sqlx::query_as("SELECT fleet_id FROM agent_profiles WHERE id = 'ap'")
            .fetch_one(&pool)
            .await
            .expect("profile row");
    assert_eq!(profile.0, None);
}
