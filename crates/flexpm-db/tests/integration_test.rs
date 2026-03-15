use flexpm_core::models::*;
use flexpm_core::vocabulary;
use flexpm_core::workflow;
use flexpm_db::{init_pool, migrations, Repository};

/// Helper to create an in-memory SQLite database for testing.
async fn setup_test_db() -> Repository {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_all(&pool).await.unwrap();
    Repository::new(pool)
}

/// Helper to create a default workspace for tests.
async fn create_test_workspace(repo: &Repository) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test', ?)"
    )
    .bind(id.to_string())
    .bind(&vocab)
    .execute(repo.pool())
    .await
    .unwrap();
    id
}

// ─── Project Tests ───────────────────────────────────────────

#[tokio::test]
async fn test_create_and_get_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Test Project".into(),
            description: Some("A test project".into()),
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();

    assert_eq!(project.name, "Test Project");
    assert_eq!(project.project_type, ProjectType::Software);

    let fetched = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Test Project");
}

#[tokio::test]
async fn test_list_projects() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    for i in 0..3 {
        repo.create_project(ws_id, CreateProject {
            name: format!("Project {i}"),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();
    }

    let projects = repo.list_projects(ws_id).await.unwrap();
    assert_eq!(projects.len(), 3);
}

#[tokio::test]
async fn test_update_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Old Name".into(),
            description: None,
            project_type: ProjectType::Personal,
            template: None,
        })
        .await
        .unwrap();

    let updated = repo
        .update_project(project.id, UpdateProject {
            name: Some("New Name".into()),
            description: None,
            vocabulary: None,
            workflow: None,
            archived: None,
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "New Name");
}

#[tokio::test]
async fn test_delete_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "To Delete".into(),
            description: None,
            project_type: ProjectType::Custom,
            template: None,
        })
        .await
        .unwrap();

    assert!(repo.delete_project(project.id).await.unwrap());
    assert!(repo.get_project(project.id).await.unwrap().is_none());
}

// ─── Item Tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_create_and_list_items() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Item Test".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();

    // Create parent epic
    let epic = repo
        .create_item(project.id, &initial_status, CreateItem {
            title: "Epic One".into(),
            description: Some("An epic".into()),
            item_type: Some(ItemType::Epic),
            parent_id: None,
            priority: Some(Priority::High),
            estimate: Some(13.0),
            estimate_unit: None,
            tags: Some(vec!["backend".into()]),
            due_date: None,
            sprint_id: None,
        })
        .await
        .unwrap();

    assert_eq!(epic.title, "Epic One");
    assert_eq!(epic.item_type, ItemType::Epic);
    assert_eq!(epic.priority, Priority::High);

    // Create child task under epic
    let task = repo
        .create_item(project.id, &initial_status, CreateItem {
            title: "Task under epic".into(),
            description: None,
            item_type: Some(ItemType::Task),
            parent_id: Some(epic.id),
            priority: None,
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
        })
        .await
        .unwrap();

    assert_eq!(task.parent_id, Some(epic.id));

    // List items
    let items = repo
        .list_items(project.id, &ItemFilter {
            status: None,
            item_type: None,
            priority: None,
            sprint_id: None,
            parent_id: None,
            tag: None,
            search: None,
            page: None,
            per_page: None,
        })
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_update_item_status() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Status Test".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();

    let item = repo
        .create_item(project.id, "Backlog", CreateItem {
            title: "Move me".into(),
            description: None,
            item_type: None,
            parent_id: None,
            priority: None,
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
        })
        .await
        .unwrap();

    assert_eq!(item.status, "Backlog");

    let updated = repo
        .update_item(item.id, UpdateItem {
            title: None,
            description: None,
            item_type: None,
            status: Some("In Progress".into()),
            priority: None,
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
            sort_order: None,
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.status, "In Progress");
}

#[tokio::test]
async fn test_item_tree() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Tree Test".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();

    let epic = repo
        .create_item(project.id, "Backlog", CreateItem {
            title: "Root Epic".into(),
            description: None,
            item_type: Some(ItemType::Epic),
            parent_id: None,
            priority: None, estimate: None, estimate_unit: None, tags: None, due_date: None, sprint_id: None,
        })
        .await
        .unwrap();

    repo.create_item(project.id, "Backlog", CreateItem {
        title: "Child Task".into(),
        description: None,
        item_type: Some(ItemType::Task),
        parent_id: Some(epic.id),
        priority: None, estimate: None, estimate_unit: None, tags: None, due_date: None, sprint_id: None,
    })
    .await
    .unwrap();

    let tree = repo.get_item_tree(project.id).await.unwrap();
    assert_eq!(tree.len(), 2);
    // Root items come first (parent_id is NULL)
    assert!(tree[0].parent_id.is_none());
}

// ─── Sprint Tests ────────────────────────────────────────────

#[tokio::test]
async fn test_sprint_lifecycle() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Sprint Test".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        })
        .await
        .unwrap();

    let sprint = repo
        .create_sprint(project.id, CreateSprint {
            name: "Sprint 1".into(),
            goal: Some("Ship MVP".into()),
            start_date: None,
            end_date: None,
        })
        .await
        .unwrap();

    assert_eq!(sprint.status, SprintStatus::Planning);

    repo.update_sprint_status(sprint.id, SprintStatus::Active)
        .await
        .unwrap();

    let fetched = repo.get_sprint(sprint.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, SprintStatus::Active);
}

// ─── Role Tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_roles_and_assignment() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Role Test".into(),
            description: None,
            project_type: ProjectType::Construction,
            template: None,
        })
        .await
        .unwrap();

    let role = repo
        .create_role(project.id, CreateRole {
            name: "Electrician".into(),
            color: Some("#f59e0b".into()),
            icon: None,
        })
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(project.id, &initial_status, CreateItem {
            title: "Wire the kitchen".into(),
            description: None,
            item_type: None,
            parent_id: None,
            priority: None, estimate: None, estimate_unit: None, tags: None, due_date: None, sprint_id: None,
        })
        .await
        .unwrap();

    // Assign role
    repo.assign_role_to_item(item.id, role.id).await.unwrap();

    let roles = repo.get_roles_for_item(item.id).await.unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].name, "Electrician");

    // Remove role
    repo.remove_role_from_item(item.id, role.id).await.unwrap();
    let roles = repo.get_roles_for_item(item.id).await.unwrap();
    assert_eq!(roles.len(), 0);
}

// ─── Comment Tests ───────────────────────────────────────────

#[tokio::test]
async fn test_comments() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(ws_id, CreateProject {
            name: "Comment Test".into(),
            description: None,
            project_type: ProjectType::Personal,
            template: None,
        })
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(project.id, &initial_status, CreateItem {
            title: "Commentable".into(),
            description: None,
            item_type: None, parent_id: None, priority: None, estimate: None,
            estimate_unit: None, tags: None, due_date: None, sprint_id: None,
        })
        .await
        .unwrap();

    repo.create_comment(item.id, CreateComment {
        content: "First comment".into(),
        author: Some("Alice".into()),
    })
    .await
    .unwrap();

    repo.create_comment(item.id, CreateComment {
        content: "Second comment".into(),
        author: None,
    })
    .await
    .unwrap();

    let comments = repo.list_comments(item.id).await.unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].content, "First comment");
}

// ─── Vocabulary Tests ────────────────────────────────────────

#[tokio::test]
async fn test_project_vocabulary_by_type() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let construction = repo
        .create_project(ws_id, CreateProject {
            name: "Building Project".into(),
            description: None,
            project_type: ProjectType::Construction,
            template: None,
        })
        .await
        .unwrap();

    // Construction vocabulary should use domain-specific terms
    assert_eq!(
        vocabulary::resolve(&construction.vocabulary, "task"),
        "Work Order"
    );
    assert_eq!(
        vocabulary::resolve(&construction.vocabulary, "sprint"),
        "Phase"
    );

    let homework = repo
        .create_project(ws_id, CreateProject {
            name: "Math Homework".into(),
            description: None,
            project_type: ProjectType::Homework,
            template: None,
        })
        .await
        .unwrap();

    assert_eq!(
        vocabulary::resolve(&homework.vocabulary, "task"),
        "Assignment"
    );
}

// ─── Workflow Tests ──────────────────────────────────────────

#[test]
fn test_workflow_transition_validation() {
    let wf = workflow::construction_workflow();

    // Valid: Permit -> Procurement
    assert!(wf.validate_transition("Permit", "Procurement").is_ok());

    // Invalid: Permit -> Handover (skipping steps)
    assert!(wf.validate_transition("Permit", "Handover").is_err());

    // Valid: Inspect -> Build (rework loop)
    assert!(wf.validate_transition("Inspect", "Build").is_ok());
}

#[test]
fn test_wip_limits() {
    let wf = workflow::scrum_workflow();

    // In Progress has WIP limit of 5
    assert!(wf.check_wip_limit("In Progress", 4).is_ok());
    assert!(wf.check_wip_limit("In Progress", 5).is_err());

    // Backlog has no limit
    assert!(wf.check_wip_limit("Backlog", 9999).is_ok());
}
