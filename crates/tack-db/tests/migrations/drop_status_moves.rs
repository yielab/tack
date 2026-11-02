//! Migration 103: a run no longer moves a task's status, so the project's two settings and the
//! request's policy column are gone, and a project that had them set still loads.

use tack_core::models::ProjectType;
use tack_core::vocabulary::vocabulary_for_type;
use tack_core::workflow::workflow_for_type;
use tack_db::{Repository, init_pool, migrations};

async fn columns(pool: &sqlx::SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("columns")
}

#[tokio::test]
async fn migration_103_drops_the_status_move_settings_and_keeps_the_project() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "101_change_sets")
        .await
        .expect("up to 101");
    assert!(
        columns(&pool, "projects")
            .await
            .contains(&"on_start_status".to_owned())
    );
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES ('22222222-2222-2222-2222-222222222222', 'W', '{}')")
        .execute(&pool)
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, project_type, vocabulary, workflow, on_start_status, on_finish_status, created_at, updated_at) \
         VALUES ('11111111-1111-1111-1111-111111111111', '22222222-2222-2222-2222-222222222222', 'P', 'software', ?, ?, 'In Progress', 'Done', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(serde_json::to_string(&vocabulary_for_type(&ProjectType::Software)).unwrap())
    .bind(serde_json::to_string(&workflow_for_type(&ProjectType::Software)).unwrap())
    .execute(&pool)
    .await
    .expect("project at 101");

    migrations::run_all(&pool).await.expect("upgrade to 103");

    let project_columns = columns(&pool, "projects").await;
    assert!(!project_columns.contains(&"on_start_status".to_owned()));
    assert!(!project_columns.contains(&"on_finish_status".to_owned()));
    assert!(
        !columns(&pool, "execution_requests")
            .await
            .contains(&"status_map_policy_id".to_owned())
    );
    let repo = Repository::new(pool);
    let project = repo
        .get_project("11111111-1111-1111-1111-111111111111".parse().unwrap())
        .await
        .expect("query")
        .expect("the project is still there");
    assert_eq!(project.name, "P");
}
