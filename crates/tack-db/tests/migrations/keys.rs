//! Migration 115: a key on roles, fields, sprints, profiles and items, filled from the names of the
//! rows a board already has.

use tack_db::{init_pool, migrations};

async fn keys(pool: &sqlx::SqlitePool, table: &str, column: &str) -> Vec<(String, String)> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {column}, key FROM {table} ORDER BY created_at, rowid"
    )))
    .fetch_all(pool)
    .await
    .expect("keys")
}

#[tokio::test]
async fn migration_115_gives_every_visible_row_a_key_once() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "114_reviewer_verdicts")
        .await
        .expect("up to 114");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO roles (id, project_id, name, created_at) VALUES ('r1', 'p', 'Dev Team', '2026-01-01T00:00:01Z')",
        "INSERT INTO roles (id, project_id, name, created_at) VALUES ('r2', 'p', 'dev team', '2026-01-01T00:00:02Z')",
        "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) VALUES ('ap1', 'Writer', 'x', '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO items (id, project_id, title, status, created_at, updated_at) VALUES ('i1', 'p', 'Login: GitHub', 'To Do', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO items (id, project_id, title, status, hidden, created_at, updated_at) VALUES ('i2', 'p', 'Anchor', 'To Do', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }

    migrations::run_all(&pool).await.expect("115");
    let expected = [
        (
            "roles",
            "name",
            vec![("Dev Team", "dev-team"), ("dev team", "dev-team-2")],
        ),
        ("agent_profiles", "name", vec![("Writer", "writer")]),
        (
            "items",
            "title",
            vec![("Login: GitHub", "login-github"), ("Anchor", "")],
        ),
    ];
    for (table, column, rows) in &expected {
        let rows: Vec<(String, String)> = rows
            .iter()
            .map(|(name, key)| (name.to_string(), key.to_string()))
            .collect();
        assert_eq!(keys(&pool, table, column).await, rows, "{table}");
    }

    migrations::run_all(&pool).await.expect("a second start");
    for (table, column, rows) in &expected {
        assert_eq!(
            keys(&pool, table, column).await.len(),
            rows.len(),
            "{table}"
        );
    }
    assert_eq!(
        keys(&pool, "roles", "name").await[1].1,
        "dev-team-2",
        "a second start changes nothing"
    );

    for (table, index) in [
        ("roles", "idx_roles_key"),
        (
            "custom_field_definitions",
            "idx_custom_field_definitions_key",
        ),
        ("sprints", "idx_sprints_key"),
        ("agent_profiles", "idx_agent_profiles_key"),
        ("items", "idx_items_key"),
    ] {
        let found: Vec<(i64, String, i64, String, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("PRAGMA index_list({table})")))
                .fetch_all(&pool)
                .await
                .expect("index_list");
        let row = found.iter().find(|row| row.1 == index);
        assert!(
            row.is_some_and(|row| row.2 == 1 && row.4 == 1),
            "{index} is a partial unique index: {found:?}"
        );
    }
}

#[tokio::test]
async fn a_built_in_profile_has_its_key_on_the_first_start() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    tack_db::repo::execution::seed_builtin_profiles(
        &tack_db::Repository::new(pool.clone()),
        chrono::Utc::now(),
    )
    .await
    .expect("seed");
    let unkeyed: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM agent_profiles WHERE builtin = 1 AND key = ''")
            .fetch_all(&pool)
            .await
            .expect("query");
    assert!(unkeyed.is_empty(), "built-ins without a key: {unkeyed:?}");
}
