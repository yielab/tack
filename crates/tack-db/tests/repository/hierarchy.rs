//! The tree rule inside the store's transactions: where an item may go is asked of
//! `tack_core::hierarchy` while the row is locked, and a refusal writes nothing.

use tack_core::CoreError;
use tack_core::hierarchy::Hierarchy;
use tack_core::models::{CreateItem, CreateSprint, Item, Project, UpdateItem};
use tack_db::Repository;
use tack_db::repo::items::{AtomicItemUpdateOutcome, ItemError};
use tack_test_support::{create_test_workspace, make_project, setup_test_db};
use uuid::Uuid;

async fn make(
    repo: &Repository,
    project: &Project,
    title: &str,
    item_type: &str,
    parent: Option<Uuid>,
) -> Item {
    let status = project.workflow.initial_status().unwrap().to_string();
    repo.create_item(
        project.id,
        &status,
        CreateItem {
            title: title.into(),
            item_type: Some(item_type.into()),
            parent_id: parent,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

async fn move_to(
    repo: &Repository,
    project: &Project,
    item: &Item,
    parent: Option<Uuid>,
) -> AtomicItemUpdateOutcome {
    repo.update_item_atomically(
        item.id,
        UpdateItem {
            parent_id: Some(parent),
            ..Default::default()
        },
        &project.workflow,
        &Hierarchy,
        None,
    )
    .await
    .unwrap()
}

fn refusal(outcome: AtomicItemUpdateOutcome) -> CoreError {
    match outcome {
        AtomicItemUpdateOutcome::Rejected(error) => error,
        other => panic!("the move was not refused: {other:?}"),
    }
}

async fn fixture() -> (Repository, Project) {
    let repo = setup_test_db().await;
    let workspace = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace).await;
    (repo, project)
}

#[tokio::test]
async fn an_item_moves_under_a_parent_and_back_to_the_top() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None).await;
    let task = make(&repo, &project, "Pay", "task", None).await;

    assert!(matches!(
        move_to(&repo, &project, &task, Some(epic.id)).await,
        AtomicItemUpdateOutcome::Updated { version: 2, .. }
    ));
    assert_eq!(
        repo.get_item(task.id).await.unwrap().unwrap().parent_id,
        Some(epic.id)
    );

    assert!(matches!(
        move_to(&repo, &project, &task, None).await,
        AtomicItemUpdateOutcome::Updated { version: 3, .. }
    ));
    assert_eq!(
        repo.get_item(task.id).await.unwrap().unwrap().parent_id,
        None
    );
}

#[tokio::test]
async fn a_move_the_rule_refuses_leaves_the_row_and_its_version_as_they_were() {
    let (repo, project) = fixture().await;
    let epic = make(&repo, &project, "Checkout", "epic", None).await;
    let task = make(&repo, &project, "Pay", "task", Some(epic.id)).await;
    let version = tack_test_support::probe::get_item_version(&repo, epic.id)
        .await
        .unwrap();

    let error = refusal(move_to(&repo, &project, &epic, Some(task.id)).await);
    assert!(matches!(error, CoreError::ParentLoop { .. }), "{error}");
    assert_eq!(
        tack_test_support::probe::get_item_version(&repo, epic.id)
            .await
            .unwrap(),
        version
    );
    assert_eq!(
        repo.get_item(epic.id).await.unwrap().unwrap().parent_id,
        None
    );

    // A group under work, in the same transaction as another change, writes neither.
    let other = make(&repo, &project, "Other", "task", None).await;
    let outcome = repo
        .update_item_atomically(
            epic.id,
            UpdateItem {
                title: Some("Renamed".into()),
                parent_id: Some(Some(other.id)),
                ..Default::default()
            },
            &project.workflow,
            &Hierarchy,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(refusal(outcome), CoreError::GroupUnderWork { .. }));
    let kept = repo.get_item(epic.id).await.unwrap().unwrap();
    assert_eq!((kept.title.as_str(), kept.parent_id), ("Checkout", None));
}

#[tokio::test]
async fn a_tree_deeper_than_five_levels_is_refused_by_the_subtree_as_well() {
    let (repo, project) = fixture().await;
    let mut parent = None;
    let mut chain = Vec::new();
    for level in 0..4 {
        let item = make(&repo, &project, &format!("L{level}"), "task", parent).await;
        parent = Some(item.id);
        chain.push(item);
    }
    // The fifth level is the most.
    let fifth = make(&repo, &project, "L4", "task", parent).await;
    let error = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                title: "L5".into(),
                parent_id: Some(fifth.id),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ItemError::Refused(CoreError::TreeTooDeep)));

    // A holder with one level under it cannot go under the fourth level either.
    let holder = make(&repo, &project, "Holder", "task", None).await;
    make(&repo, &project, "Under", "task", Some(holder.id)).await;
    let error = refusal(move_to(&repo, &project, &holder, Some(chain[3].id)).await);
    assert!(matches!(error, CoreError::TreeTooDeep), "{error}");
}

#[tokio::test]
async fn a_new_group_cannot_be_made_under_work_or_in_a_sprint() {
    let (repo, project) = fixture().await;
    let task = make(&repo, &project, "Pay", "task", None).await;
    let error = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                title: "Checkout".into(),
                item_type: Some("epic".into()),
                parent_id: Some(task.id),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ItemError::Refused(CoreError::GroupUnderWork { .. })
    ));

    let sprint = repo
        .create_sprint(
            project.id,
            CreateSprint {
                name: "S1".into(),
                goal: None,
                start_date: None,
                end_date: None,
            },
        )
        .await
        .unwrap();
    let epic = make(&repo, &project, "Checkout", "epic", None).await;
    let outcome = repo
        .update_item_atomically(
            epic.id,
            UpdateItem {
                sprint_id: Some(Some(sprint.id)),
                ..Default::default()
            },
            &project.workflow,
            &Hierarchy,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        refusal(outcome).to_string(),
        "An epic does not go in a sprint: its items do."
    );

    // Work with children counts as a group.
    let parent = make(&repo, &project, "Parent", "task", None).await;
    make(&repo, &project, "Child", "task", Some(parent.id)).await;
    let outcome = repo
        .update_item_atomically(
            parent.id,
            UpdateItem {
                sprint_id: Some(Some(sprint.id)),
                ..Default::default()
            },
            &project.workflow,
            &Hierarchy,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(refusal(outcome), CoreError::GroupInSprint { .. }));
}

#[tokio::test]
async fn a_task_cannot_become_an_epic_under_a_task() {
    let (repo, project) = fixture().await;
    let parent = make(&repo, &project, "Pay", "task", None).await;
    let child = make(&repo, &project, "Receipt", "task", Some(parent.id)).await;
    let outcome = repo
        .update_item_atomically(
            child.id,
            UpdateItem {
                item_type: Some("epic".into()),
                ..Default::default()
            },
            &project.workflow,
            &Hierarchy,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(refusal(outcome), CoreError::GroupUnderWork { .. }));
}

#[tokio::test]
async fn a_parent_of_another_project_is_refused() {
    let (repo, project) = fixture().await;
    let workspace = create_test_workspace(&repo).await;
    let other = make_project(&repo, workspace).await;
    let foreign = make(&repo, &other, "Elsewhere", "epic", None).await;
    let task = make(&repo, &project, "Pay", "task", None).await;
    let error = refusal(move_to(&repo, &project, &task, Some(foreign.id)).await);
    assert!(matches!(error, CoreError::ParentInOtherProject));
}

#[tokio::test]
async fn the_tree_of_a_project_comes_in_tree_order() {
    let (repo, project) = fixture().await;
    let first = make(&repo, &project, "First", "epic", None).await;
    let loose = make(&repo, &project, "Loose", "task", None).await;
    let inside = make(&repo, &project, "Inside", "task", Some(first.id)).await;
    let rows = tack_db::queries::items::tree_of_project(&repo, project.id)
        .await
        .unwrap();
    let order: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    assert_eq!(order, vec![first.id, inside.id, loose.id]);
    assert_eq!(rows[1].entry.parent, Some(first.id));
}
