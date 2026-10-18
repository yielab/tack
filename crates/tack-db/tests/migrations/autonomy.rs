//! Migrations 087 (autonomy, merges, budgets and brief versions) and 092 (no dollars per day).

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

// 092 on a database stopped at 091: the dollar column goes, the token limit stays.
#[tokio::test]
async fn migration_092_drops_dollars_per_day_and_keeps_the_other_limits() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "091_template_bundle")
        .await
        .expect("up to 091");
    for sql in [
        "INSERT INTO workspaces (id, name) VALUES ('ws', 'W')",
        "INSERT INTO projects (id, workspace_id, name) VALUES ('p', 'ws', 'P')",
        "INSERT INTO project_budgets (project_id, tokens_per_day, usd_per_day, auto_merges_per_day, queue_items_max) \
         VALUES ('p', 2000000, 20.0, 5, 8)",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    migrations::run_all(&pool).await.expect("upgrade to 092");

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('project_budgets') ORDER BY cid")
            .fetch_all(&pool)
            .await
            .expect("columns");
    assert_eq!(
        columns,
        [
            "project_id",
            "tokens_per_day",
            "auto_merges_per_day",
            "queue_items_max"
        ]
    );
    let kept: (i64, i64, i64) = sqlx::query_as(
        "SELECT tokens_per_day, auto_merges_per_day, queue_items_max FROM project_budgets WHERE project_id = 'p'",
    )
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(kept, (2_000_000, 5, 8));
}
