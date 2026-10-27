use crate::common::{self, create_test_workspace, setup_test_db};
use tack_core::models::*;
use tack_core::vocabulary;
use tack_core::workflow;
use tack_protocol::Clock;

/// A `CreateItem` with every field but `title` at its zero/default value —
/// tests override only the fields their assertions actually check via
/// struct-update syntax (`CreateItem { priority: ..., ..item_input("x") }`).
fn item_input(title: &str) -> CreateItem {
    CreateItem {
        recurrence: None,
        recurrence_of: None,
        title: title.into(),
        description: None,
        item_type: None,
        parent_id: None,
        priority: None,
        estimate: None,
        estimate_unit: None,
        tags: None,
        due_date: None,
        sprint_id: None,
        assignee: None,
        source_artifact_id: None,
    }
}

/// A `CreateProject` with every field but `name` at its default (a Software
/// project, no description/template) — same struct-update convention as
/// [`item_input`].
fn project_input(name: &str) -> CreateProject {
    CreateProject {
        name: name.into(),
        description: None,
        project_type: ProjectType::Software,
        template: None,
        code_origin: None,
        repository: None,
        default_branch: None,
        workspace_mode: None,
    }
}

/// A `CreateProjectTemplate` with every field but `name`/`project_type` at
/// its default (no vocabulary/workflow/fields/boards override).
fn template_input(
    name: &str,
    project_type: ProjectType,
) -> tack_core::models::CreateProjectTemplate {
    tack_core::models::CreateProjectTemplate {
        name: name.into(),
        description: None,
        project_type,
        vocabulary: None,
        workflow: None,
        custom_fields: None,
        default_boards: None,
        roles: Vec::new(),
        profiles: Vec::new(),
        automation: None,
    }
}

/// A minimal text field, for tests that only need *a* custom field to exist.
fn text_field(name: &str) -> CreateCustomField {
    CreateCustomField {
        name: name.into(),
        field_type: CustomFieldType::Text,
        description: None,
        required: Some(false),
        default_value: None,
        options: None,
        validation: None,
    }
}

/// A required select field with the given options, otherwise matching
/// [`text_field`]'s defaults.
fn select_field(name: &str, options: &[&str]) -> CreateCustomField {
    CreateCustomField {
        field_type: CustomFieldType::Select,
        required: Some(true),
        options: Some(options.iter().map(|s| s.to_string()).collect()),
        ..text_field(name)
    }
}

/// A `CreateBoard` with no description or filters — the shape every board
/// test in this file needs beyond name/grouping/is_default.
fn board_input(name: &str, grouping: BoardGrouping, is_default: bool) -> CreateBoard {
    CreateBoard {
        name: name.into(),
        description: None,
        grouping: Some(grouping),
        filters: None,
        is_default: Some(is_default),
    }
}

/// Applies `input` and returns the updated item — panics on a missing item,
/// which none of this file's `update_item` tests exercise.
async fn set_item(repo: &tack_db::Repository, id: uuid::Uuid, input: UpdateItem) -> Item {
    repo.update_item(id, input).await.unwrap().unwrap()
}

/// Re-fetches an item, panicking if it no longer exists.
async fn item_now(repo: &tack_db::Repository, id: uuid::Uuid) -> Item {
    repo.get_item(id).await.unwrap().unwrap()
}

/// A `CreateSprint` with no goal or dates set.
fn sprint_input(name: &str) -> CreateSprint {
    CreateSprint {
        name: name.into(),
        goal: None,
        start_date: None,
        end_date: None,
    }
}

/// Re-fetches `item_id` and asserts its `sprint_id` equals `expected`.
async fn assert_sprint_id(
    repo: &tack_db::Repository,
    item_id: uuid::Uuid,
    expected: Option<uuid::Uuid>,
    msg: &str,
) {
    assert_eq!(item_now(repo, item_id).await.sprint_id, expected, "{msg}");
}

/// An `UpdateItem` moving to `status`/`category`, nothing else changed.
fn status_update(status: &str, category: workflow::StatusCategory) -> UpdateItem {
    UpdateItem {
        status: Some(status.into()),
        status_category: Some(category),
        ..Default::default()
    }
}

// ─── GitHub link Tests ────────────────────────────

#[tokio::test]
async fn github_link_round_trip_and_upsert() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    // No link initially.
    assert_eq!(repo.get_github_link(item.id).await.unwrap(), None);

    // Set, then read back.
    repo.set_github_link(item.id, "acme/widgets", 42)
        .await
        .unwrap();
    assert_eq!(
        repo.get_github_link(item.id).await.unwrap(),
        Some(("acme/widgets".to_string(), 42))
    );

    // Upsert replaces the existing link (no duplicate-key error).
    repo.set_github_link(item.id, "acme/widgets", 99)
        .await
        .unwrap();
    assert_eq!(
        repo.get_github_link(item.id).await.unwrap(),
        Some(("acme/widgets".to_string(), 99))
    );
}

// ─── Project Tests ───────────────────────────────────────────

#[tokio::test]
async fn create_and_get_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Test Project".into(),
                description: Some("A test project".into()),
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(project.name, "Test Project");
    assert_eq!(project.project_type, ProjectType::Software);

    let fetched = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Test Project");
}

#[tokio::test]
async fn list_projects() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    for i in 0..3 {
        repo.create_project(
            ws_id,
            CreateProject {
                name: format!("Project {i}"),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();
    }

    let projects = repo.list_projects(ws_id).await.unwrap();
    assert_eq!(projects.len(), 3);
}

#[tokio::test]
async fn update_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Old Name".into(),
                description: None,
                project_type: ProjectType::Personal,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();

    let updated = repo
        .update_project(
            project.id,
            UpdateProject {
                name: Some("New Name".into()),
                description: None,
                vocabulary: None,
                workflow: None,
                default_model: None,
                github_token_ref: None,
                archived: None,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "New Name");
}

#[tokio::test]
async fn delete_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "To Delete".into(),
                description: None,
                project_type: ProjectType::Custom,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();

    assert!(repo.delete_project(project.id).await.unwrap());
    assert!(repo.get_project(project.id).await.unwrap().is_none());
}

// ─── Item Tests ──────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_items() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let status = project.workflow.initial_status().unwrap();

    let epic_input = CreateItem {
        description: Some("An epic".into()),
        item_type: Some(ItemType::Epic),
        priority: Some(Priority::High),
        estimate: Some(13.0),
        tags: Some(vec!["backend".into()]),
        ..item_input("Epic One")
    };
    let epic = repo
        .create_item(project.id, &status, epic_input)
        .await
        .unwrap();
    assert_eq!(epic.title, "Epic One");
    assert_eq!(epic.item_type, ItemType::Epic);
    assert_eq!(epic.priority, Priority::High);

    let task_input = CreateItem {
        item_type: Some(ItemType::Task),
        parent_id: Some(epic.id),
        ..item_input("Task under epic")
    };
    let task = repo
        .create_item(project.id, &status, task_input)
        .await
        .unwrap();
    assert_eq!(task.parent_id, Some(epic.id));

    let items = repo
        .list_items(project.id, &ItemFilter::default())
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn update_item_status() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = repo
        .create_item(project.id, "Backlog", item_input("Move me"))
        .await
        .unwrap();

    assert_eq!(item.status, "Backlog");

    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                status: Some("In Progress".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.status, "In Progress");
}

#[tokio::test]
async fn item_tree() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;

    let epic_input = CreateItem {
        item_type: Some(ItemType::Epic),
        ..item_input("Root Epic")
    };
    let epic = repo
        .create_item(project.id, "Backlog", epic_input)
        .await
        .unwrap();

    let child_input = CreateItem {
        item_type: Some(ItemType::Task),
        parent_id: Some(epic.id),
        ..item_input("Child Task")
    };
    repo.create_item(project.id, "Backlog", child_input)
        .await
        .unwrap();

    let tree = repo.get_item_tree(project.id).await.unwrap();
    assert_eq!(tree.len(), 2);
    // Root items come first (parent_id is NULL)
    assert!(tree[0].parent_id.is_none());
}

// ─── Sprint Tests ────────────────────────────────────────────

#[tokio::test]
async fn sprint_lifecycle() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Sprint Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
            },
        )
        .await
        .unwrap();

    let sprint = repo
        .create_sprint(
            project.id,
            CreateSprint {
                name: "Sprint 1".into(),
                goal: Some("Ship MVP".into()),
                start_date: None,
                end_date: None,
            },
        )
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
async fn roles_and_assignment() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let input = CreateProject {
        project_type: ProjectType::Construction,
        ..project_input("Role Test")
    };
    let project = repo.create_project(ws_id, input).await.unwrap();

    let role = repo
        .create_role(
            project.id,
            CreateRole {
                name: "Electrician".into(),
                color: Some("#f59e0b".into()),
                icon: None,
            },
        )
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(project.id, &initial_status, item_input("Wire the kitchen"))
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
async fn comments() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let input = CreateProject {
        project_type: ProjectType::Personal,
        ..project_input("Comment Test")
    };
    let project = repo.create_project(ws_id, input).await.unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(project.id, &initial_status, item_input("Commentable"))
        .await
        .unwrap();

    repo.create_comment(
        item.id,
        CreateComment {
            content: "First comment".into(),
            author: Some("Alice".into()),
        },
    )
    .await
    .unwrap();

    repo.create_comment(
        item.id,
        CreateComment {
            content: "Second comment".into(),
            author: None,
        },
    )
    .await
    .unwrap();

    let comments = repo.list_comments(item.id).await.unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].content, "First comment");
}

// ─── Vocabulary Tests ────────────────────────────────────────

#[tokio::test]
async fn project_vocabulary_by_type() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let construction_input = CreateProject {
        project_type: ProjectType::Construction,
        ..project_input("Building Project")
    };
    let construction = repo
        .create_project(ws_id, construction_input)
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

    let homework_input = CreateProject {
        project_type: ProjectType::Homework,
        ..project_input("Math Homework")
    };
    let homework = repo.create_project(ws_id, homework_input).await.unwrap();
    assert_eq!(
        vocabulary::resolve(&homework.vocabulary, "task"),
        "Assignment"
    );
}

// ─── Workflow Tests ──────────────────────────────────────────

#[test]
fn workflow_transition_validation() {
    let wf = workflow::construction_workflow();

    // Valid: Permit -> Procurement
    assert!(wf.validate_transition("Permit", "Procurement").is_ok());

    // Invalid: Permit -> Handover (skipping steps)
    assert!(wf.validate_transition("Permit", "Handover").is_err());

    // Valid: Inspect -> Build (rework loop)
    assert!(wf.validate_transition("Inspect", "Build").is_ok());
}

#[test]
fn wip_limits() {
    let wf = workflow::scrum_workflow();

    // In Progress has WIP limit of 5
    assert!(wf.check_wip_limit("In Progress", 4).is_ok());
    assert!(wf.check_wip_limit("In Progress", 5).is_err());

    // Backlog has no limit
    assert!(wf.check_wip_limit("Backlog", 9999).is_ok());
}

// ─── Template Tests (v1.2) ───────────────────────────────────

#[tokio::test]
async fn create_and_get_template() {
    let repo = setup_test_db().await;

    use tack_core::models::CreateProjectTemplate;
    use tack_db::repo::templates;

    let template_data = CreateProjectTemplate {
        name: "My Software Template".into(),
        description: Some("A custom scrum template".into()),
        project_type: ProjectType::Software,
        vocabulary: None,
        workflow: None,
        custom_fields: None,
        default_boards: None,
        roles: Vec::new(),
        profiles: Vec::new(),
        automation: None,
    };

    let template = templates::create_template(&repo, template_data)
        .await
        .unwrap();

    assert_eq!(template.name, "My Software Template");
    assert_eq!(template.project_type, ProjectType::Software);
    assert!(!template.is_builtin);

    let fetched = templates::get_template(&repo, template.id).await.unwrap();
    assert_eq!(fetched.name, "My Software Template");
}

#[tokio::test]
async fn list_templates_with_filter() {
    let repo = setup_test_db().await;
    use tack_db::repo::templates;

    // Create templates of different types
    let software = template_input("Software Template", ProjectType::Software);
    templates::create_template(&repo, software).await.unwrap();
    let construction = template_input("Construction Template", ProjectType::Construction);
    templates::create_template(&repo, construction)
        .await
        .unwrap();

    // List all templates
    let all = templates::list_templates(&repo, None).await.unwrap();
    assert_eq!(all.len(), 2);

    // Filter by type
    let software_only = templates::list_templates(&repo, Some(ProjectType::Software))
        .await
        .unwrap();
    assert_eq!(software_only.len(), 1);
    assert_eq!(software_only[0].name, "Software Template");
}

#[tokio::test]
async fn delete_template_not_builtin() {
    let repo = setup_test_db().await;

    use tack_core::models::CreateProjectTemplate;
    use tack_db::repo::templates;

    let user_template = templates::create_template(
        &repo,
        CreateProjectTemplate {
            name: "User Template".into(),
            description: None,
            project_type: ProjectType::Personal,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            default_boards: None,
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
        },
    )
    .await
    .unwrap();

    // Should be able to delete user template
    templates::delete_template(&repo, user_template.id)
        .await
        .unwrap();

    // Verify deleted
    let result = templates::get_template(&repo, user_template.id).await;
    assert!(result.is_err());
}

// ─── Custom Fields Tests (v1.2) ──────────────────────────────

#[tokio::test]
async fn create_and_list_custom_fields() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    use tack_db::repo::custom_fields;

    let field1 = custom_fields::create_field(&repo, project.id, text_field("Customer"))
        .await
        .unwrap();
    assert_eq!(field1.name, "Customer");
    assert_eq!(field1.field_type, CustomFieldType::Text);
    assert!(!field1.required);

    let select = select_field("Priority Level", &["Low", "Medium", "High"]);
    let field2 = custom_fields::create_field(&repo, project.id, select)
        .await
        .unwrap();
    assert_eq!(field2.field_type, CustomFieldType::Select);
    assert!(field2.required);
    assert_eq!(field2.options.as_ref().unwrap().len(), 3);

    let fields = custom_fields::list_fields_for_project(&repo, project.id)
        .await
        .unwrap();
    assert_eq!(fields.len(), 2);
}

#[tokio::test]
async fn custom_field_value_upsert() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;
    use tack_db::repo::custom_fields;

    let field = custom_fields::create_field(&repo, project.id, text_field("Customer"))
        .await
        .unwrap();

    // `set_field_value` upserts and returns the freshly-reloaded row, so its
    // own return value already proves the write landed — no separate get.
    let value =
        custom_fields::set_field_value(&repo, item.id, field.id, serde_json::json!("Acme Corp"))
            .await
            .unwrap();
    assert_eq!(value.value, serde_json::json!("Acme Corp"));

    let updated =
        custom_fields::set_field_value(&repo, item.id, field.id, serde_json::json!("Updated Corp"))
            .await
            .unwrap();
    assert_eq!(updated.value, serde_json::json!("Updated Corp"));
}

#[tokio::test]
async fn custom_field_cascade_delete() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    use tack_db::repo::custom_fields;

    let field = custom_fields::create_field(&repo, project.id, text_field("Test Field"))
        .await
        .unwrap();

    // Set value
    custom_fields::set_field_value(&repo, item.id, field.id, serde_json::json!("Test Value"))
        .await
        .unwrap();

    // Delete field - should cascade delete values
    custom_fields::delete_field(&repo, field.id).await.unwrap();

    // Value should be gone
    let result = custom_fields::get_field_value(&repo, item.id, field.id).await;
    assert!(result.is_err());
}

// ─── Multiple Boards Tests (v1.2) ────────────────────────────

#[tokio::test]
async fn create_and_list_boards() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    use tack_db::repo::boards;

    let main = board_input("Main Board", BoardGrouping::Status, true);
    let board1 = boards::create_board(&repo, project.id, main).await.unwrap();
    assert_eq!(board1.name, "Main Board");
    assert!(board1.is_default);

    let priority = board_input("Priority View", BoardGrouping::Priority, false);
    let board2 = boards::create_board(&repo, project.id, priority)
        .await
        .unwrap();
    assert!(!board2.is_default);

    let all_boards = boards::list_boards(&repo, project.id).await.unwrap();
    assert_eq!(all_boards.len(), 2);
}

#[tokio::test]
async fn default_board_management() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    use tack_db::repo::boards;

    // Create first board as default, then create a second and make it default.
    let one = board_input("Board 1", BoardGrouping::Status, true);
    let board1 = boards::create_board(&repo, project.id, one).await.unwrap();
    let two = board_input("Board 2", BoardGrouping::Priority, true);
    let board2 = boards::create_board(&repo, project.id, two).await.unwrap();

    // Board 1 should no longer be default; board 2 should be.
    let board1_updated = boards::get_board(&repo, board1.id).await.unwrap();
    assert!(!board1_updated.is_default);
    assert!(board2.is_default);

    let default = boards::get_default_board(&repo, project.id).await.unwrap();
    assert_eq!(default.unwrap().id, board2.id);
}

#[tokio::test]
async fn board_grouping_types() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    use tack_db::repo::boards;

    // Test all grouping types
    let groupings = [
        ("Status Board", BoardGrouping::Status),
        ("Priority Board", BoardGrouping::Priority),
        ("Type Board", BoardGrouping::ItemType),
        ("Sprint Board", BoardGrouping::Sprint),
    ];

    for (name, grouping) in groupings {
        let input = board_input(name, grouping, false);
        let board = boards::create_board(&repo, project.id, input)
            .await
            .unwrap();
        assert_eq!(board.name, name);
    }

    let all_boards = boards::list_boards(&repo, project.id).await.unwrap();
    assert_eq!(all_boards.len(), 4);
}

// ─── Correctness hotfix regressions ─────────────────

/// `update_item` must persist a sprint assignment and later clear it when
/// the PATCH sends `null` (double-`Option` `Some(None)`), while an absent field
/// (outer `None`) leaves the value untouched.
#[tokio::test]
async fn update_item_persists_and_clears_sprint_id() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.sprint_id, None);

    let sprint = repo
        .create_sprint(project.id, sprint_input("Sprint 1"))
        .await
        .unwrap();

    // Assign the sprint, then re-fetch: it persists.
    let set_sprint = UpdateItem {
        sprint_id: Some(Some(sprint.id)),
        ..Default::default()
    };
    set_item(&repo, item.id, set_sprint).await;
    let msg = "sprint assignment must survive a re-fetch";
    assert_sprint_id(&repo, item.id, Some(sprint.id), msg).await;

    // An unrelated update (absent sprint_id) must NOT disturb the assignment.
    let rename = UpdateItem {
        title: Some("renamed".into()),
        ..Default::default()
    };
    set_item(&repo, item.id, rename).await;
    let msg = "absent sprint_id must leave the value untouched";
    assert_sprint_id(&repo, item.id, Some(sprint.id), msg).await;

    // A `null` clears it.
    let clear_sprint = UpdateItem {
        sprint_id: Some(None),
        ..Default::default()
    };
    set_item(&repo, item.id, clear_sprint).await;
    assert_sprint_id(&repo, item.id, None, "sprint_id: null must clear it").await;
}

/// Same absent/set/clear contract for `due_date`.
#[tokio::test]
async fn update_item_persists_and_clears_due_date() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.due_date, None);

    let due = chrono::Utc::now();
    let set_due = UpdateItem {
        due_date: Some(Some(due)),
        ..Default::default()
    };
    set_item(&repo, item.id, set_due).await;
    assert!(
        item_now(&repo, item.id).await.due_date.is_some(),
        "due_date must persist across a re-fetch"
    );

    let clear_due = UpdateItem {
        due_date: Some(None),
        ..Default::default()
    };
    set_item(&repo, item.id, clear_due).await;
    assert_eq!(
        item_now(&repo, item.id).await.due_date,
        None,
        "due_date: null must clear it"
    );
}

/// Moving an item across status categories maintains started_at /
/// completed_at: enter in-progress → stamp started_at; enter done → stamp
/// completed_at; leave done → clear completed_at (keeping started_at).
#[tokio::test]
async fn status_transition_timestamps() {
    use tack_core::workflow::StatusCategory;

    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert!(item.started_at.is_none());
    assert!(item.completed_at.is_none());

    // → In Progress: started_at stamped, completed_at still empty.
    let in_progress = status_update("In Progress", StatusCategory::InProgress);
    let updated = set_item(&repo, item.id, in_progress).await;
    assert!(updated.started_at.is_some(), "started_at must stamp");
    assert!(updated.completed_at.is_none());
    let started = updated.started_at;

    // → Done: completed_at stamped, started_at preserved.
    let done = status_update("Done", StatusCategory::Done);
    let updated = set_item(&repo, item.id, done).await;
    assert!(updated.completed_at.is_some(), "completed_at must stamp");
    assert_eq!(updated.started_at, started, "started_at preserved");

    // → back to a Todo column: completed_at cleared, started_at preserved.
    let todo = status_update("To Do", StatusCategory::Todo);
    let updated = set_item(&repo, item.id, todo).await;
    assert!(
        updated.completed_at.is_none(),
        "completed_at must clear when leaving done"
    );
    assert_eq!(
        updated.started_at, started,
        "started_at must still be preserved"
    );
}

/// Foreign keys are enforced on every pooled connection, so an insert
/// that references a non-existent project is rejected instead of orphaning.
#[tokio::test]
async fn foreign_key_rejects_orphan_item() {
    let repo = setup_test_db().await;
    let bogus_project = uuid::Uuid::new_v4();

    let result = repo
        .create_item(
            bogus_project,
            "Backlog",
            CreateItem {
                recurrence: None,
                recurrence_of: None,
                title: "orphan".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
                source_artifact_id: None,
            },
        )
        .await;

    assert!(
        result.is_err(),
        "inserting an item with a dangling project_id must be rejected by the FK constraint"
    );
}

// ─── Outline batches ─────────────────────────────────────────────────────────

fn outline_item(title: &str, parent: Option<&str>) -> tack_core::outline::OutlineItem {
    tack_core::outline::OutlineItem {
        reference: Some(title.into()),
        title: title.into(),
        parent: parent.map(str::to_string),
        ..Default::default()
    }
}

#[tokio::test]
async fn outline_batch_writes_parents_and_dependencies() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = repo
        .create_project(ws_id, project_input("P"))
        .await
        .unwrap();
    let mut kid = outline_item("Kid", Some("Epic"));
    kid.depends_on = vec!["Other".into()];
    let items = [kid, outline_item("Epic", None), outline_item("Other", None)];
    let resolved = tack_core::outline::resolve(&items, &project.workflow).unwrap();

    let (created, deps) = repo
        .create_outline_items(project.id, resolved, ItemSource::Outline)
        .await
        .unwrap();

    assert_eq!(deps, 1);
    let epic = created.iter().find(|i| i.title == "Epic").unwrap();
    let kid = created.iter().find(|i| i.title == "Kid").unwrap();
    assert_eq!(kid.parent_id, Some(epic.id));
    assert_eq!(
        repo.list_dependencies_for_project(project.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn outline_batch_failing_midway_writes_nothing() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = repo
        .create_project(ws_id, project_input("P"))
        .await
        .unwrap();
    // The second row's parent passes the outline check (an id is assumed to
    // exist) but breaks the foreign key, after the first row was inserted.
    let missing = uuid::Uuid::new_v4().to_string();
    let items = [
        outline_item("Fine", None),
        outline_item("Broken", Some(&missing)),
    ];
    let resolved = tack_core::outline::resolve(&items, &project.workflow).unwrap();

    let result = repo
        .create_outline_items(project.id, resolved, ItemSource::Outline)
        .await;

    assert!(result.is_err());
    let left = repo
        .list_items(project.id, &Default::default())
        .await
        .unwrap();
    assert!(
        left.is_empty(),
        "the first row must roll back with the second"
    );
}

#[tokio::test]
async fn bundle_reads_return_what_two_inserts_put_in_and_a_profile_writes_in_place() {
    use tack_core::outline::bundle::ProfileRow;
    use tack_protocol::SystemClock;

    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = repo
        .create_project(ws_id, project_input("P"))
        .await
        .unwrap();
    let role = repo
        .create_role(
            project.id,
            CreateRole {
                name: "Content".into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();
    let field = repo
        .create_field(
            project.id,
            CreateCustomField {
                name: "Dialect".into(),
                field_type: CustomFieldType::Text,
                description: None,
                required: None,
                default_value: None,
                options: None,
                validation: None,
            },
        )
        .await
        .unwrap();
    let mut items = Vec::new();
    for title in ["One", "Two"] {
        items.push(
            repo.create_item(project.id, "Backlog", item_input(title))
                .await
                .unwrap(),
        );
    }
    for (item, value) in items.iter().zip(["x", "y"]) {
        repo.assign_role_to_item(item.id, role.id).await.unwrap();
        repo.set_field_value(item.id, field.id, serde_json::json!(value))
            .await
            .unwrap();
    }

    let pairs = repo.item_roles_for_project(project.id).await.unwrap();
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(items[0].id, role.id)) && pairs.contains(&(items[1].id, role.id)));
    let values = repo.field_values_for_project(project.id).await.unwrap();
    assert_eq!(values.len(), 2);
    assert!(values.iter().all(|v| v.field_id == field.id));

    sqlx::query("INSERT INTO agent_fleets (id, name, default_policy, created_at, updated_at) VALUES ('f1', 'Home', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
        .execute(repo.pool())
        .await
        .unwrap();
    assert_eq!(
        repo.fleet_names().await.unwrap(),
        [("f1".to_string(), "Home".to_string())]
    );

    let mut row = ProfileRow {
        id: "ap_mine".into(),
        name: "First".into(),
        kind: "custom".into(),
        instructions: "x".into(),
        tool_policy: serde_json::json!({"agent": "docket"}),
        limits: serde_json::json!({}),
        fleet_id: Some("f1".into()),
        delivers: Some("an answer".into()),
        builtin: true,
    };
    repo.write_agent_profile(&row, SystemClock.now())
        .await
        .unwrap();
    row.name = "Second".into();
    repo.write_agent_profile(&row, SystemClock.now())
        .await
        .unwrap();
    let rows = repo.list_agent_profile_rows().await.unwrap();
    let mine: Vec<_> = rows.iter().filter(|r| r.id == "ap_mine").collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].name, "Second");
    assert_eq!(mine[0].fleet_id.as_deref(), Some("f1"));
    assert_eq!(mine[0].delivers.as_deref(), Some("an answer"));
    assert!(!mine[0].builtin, "builtin is never written");
}
