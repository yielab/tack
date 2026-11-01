//! Migration 098: a project may move an item when a run starts. Projects that exist keep NULL
//! (no move); a project made afterwards starts with the workflow's first in-progress status.

use tack_core::models::{CreateProject, ProjectType};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

#[tokio::test]
async fn migration_098_leaves_existing_projects_not_moving_items() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "097_plan_instructions_name_no_file")
        .await
        .expect("up to 097");
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES ('w', 'W', '{}')")
        .execute(&pool)
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, project_type, on_finish_status, created_at, updated_at) \
         VALUES ('p', 'w', 'P', 'software', 'Done', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("project at 097");

    migrations::run_all(&pool).await.expect("upgrade to 098");

    let (start, finish): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT on_start_status, on_finish_status FROM projects WHERE id = 'p'")
            .fetch_one(&pool)
            .await
            .expect("row");
    assert_eq!(start, None);
    assert_eq!(finish.as_deref(), Some("Done"));
}

#[tokio::test]
async fn a_new_project_starts_with_the_workflows_first_in_progress_status() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'W', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");

    for project_type in [ProjectType::Software, ProjectType::Custom] {
        let project = repo
            .create_project(
                workspace,
                CreateProject {
                    name: "P".into(),
                    description: None,
                    project_type,
                    template: None,
                    code_origin: None,
                    repository: None,
                    default_branch: None,
                    workspace_mode: None,
                },
            )
            .await
            .expect("project");
        let first = project.workflow.find_first_in_progress_status();
        assert!(first.is_some());
        assert_eq!(project.on_start_status.as_deref(), first);
        let stored = repo.get_project(project.id).await.unwrap().unwrap();
        assert_eq!(stored.on_start_status, project.on_start_status);
    }
}
