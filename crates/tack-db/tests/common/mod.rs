use tack_core::{
    models::{CreateItem, CreateProject, ItemType, Priority, ProjectType},
    vocabulary,
};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

/// Create an in-memory SQLite pool with all migrations applied.
pub async fn setup_test_db() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

/// Insert a bare workspace row; returns its ID.
///
/// `allow(dead_code)` because each integration-test file compiles `common` into its own
/// binary: `orch_metrics_test.rs` needs `setup_test_db` but never creates a workspace,
/// so this is genuinely unused *there* while still used by every other test binary.
#[allow(dead_code)]
pub async fn create_test_workspace(repo: &Repository) -> Uuid {
    let id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test Workspace', ?)",
    )
    .bind(id.to_string())
    .bind(&vocab)
    .execute(repo.pool())
    .await
    .expect("insert workspace");
    id
}

/// Create a software project in the given workspace; returns the Project.
#[allow(dead_code)]
pub async fn make_project(repo: &Repository, workspace_id: Uuid) -> tack_core::models::Project {
    repo.create_project(
        workspace_id,
        CreateProject {
            name: "Test Project".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        },
    )
    .await
    .expect("create project")
}

/// Create a minimal task item in the project's initial workflow status.
#[allow(dead_code)]
pub async fn make_item(
    repo: &Repository,
    project: &tack_core::models::Project,
) -> tack_core::models::Item {
    let status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();
    repo.create_item(
        project.id,
        &status,
        CreateItem {
            title: "Test Item".into(),
            description: None,
            item_type: Some(ItemType::Task),
            parent_id: None,
            priority: Some(Priority::Medium),
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
            assignee: None,
        },
    )
    .await
    .expect("create item")
}
