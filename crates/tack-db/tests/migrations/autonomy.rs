//! Migration 087: autonomy, merges, budgets and brief versions.

use tack_db::{init_pool, migrations};

// 087 on a database stopped at 086: the existing project takes the new defaults, and
// the existing brief becomes version 1 with an empty sha.
#[tokio::test]
async fn migration_087_defaults_projects_and_versions_existing_briefs() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "086_one_model_string")
        .await
        .expect("up to 086");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO items (id, project_id, title, status) VALUES ('item', 'p', 'T', 'To Do')",
        "INSERT INTO item_briefs (item_id, acceptance, constraints) VALUES ('item', '[\"a\"]', '[]')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    migrations::run_all(&pool).await.expect("upgrade to 087");

    let project: (String, String, Option<String>, i64, i64) = sqlx::query_as(
        "SELECT autonomy_level, verify_require, attestation_public_key, \
         verify_loop_max_attempts, merge_on_done FROM projects WHERE id = 'p'",
    )
    .fetch_one(&pool)
    .await
    .expect("project row");
    assert_eq!(project, ("L3".into(), "[]".into(), None, 0, 0));

    let version: (i64, String, String) = sqlx::query_as(
        "SELECT version, acceptance, brief_sha256 FROM item_brief_versions WHERE item_id = 'item'",
    )
    .fetch_one(&pool)
    .await
    .expect("brief version row");
    assert_eq!(version, (1, "[\"a\"]".into(), String::new()));
}
