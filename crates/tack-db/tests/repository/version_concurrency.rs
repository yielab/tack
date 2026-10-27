//! `items.version` (migration 034) backs `handlers::items`'s `ETag`/`If-Match`
//! support. `update_item_atomically` and `check_and_update_parent_status` are the write
//! paths that must bump it.

use crate::common::{self, create_test_workspace, make_item, make_project};
use tack_core::models::{CreateItem, ItemType, Priority, UpdateItem};

#[tokio::test]
async fn fresh_item_starts_at_version_one() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let version = repo
        .get_item_version(item.id)
        .await
        .expect("db call")
        .expect("item exists");
    assert_eq!(version, 1, "migration 034's column default is 1");
}

#[tokio::test]
async fn an_item_update_bumps_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let before = repo.get_item_version(item.id).await.unwrap().unwrap();
    common::update_item(
        &repo,
        item.id,
        UpdateItem {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .await;
    let after = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "an item update must bump version: {before} -> {after}"
    );
}

fn item_titled(title: &str, item_type: ItemType, parent_id: Option<uuid::Uuid>) -> CreateItem {
    CreateItem {
        recurrence: None,
        recurrence_of: None,
        title: title.into(),
        description: None,
        item_type: Some(item_type),
        parent_id,
        priority: Some(Priority::Medium),
        estimate: None,
        estimate_unit: None,
        tags: None,
        due_date: None,
        sprint_id: None,
        assignee: None,
        source_artifact_id: None,
    }
}

#[tokio::test]
async fn check_and_update_parent_status_bumps_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    let parent = repo
        .create_item(
            project.id,
            "Backlog",
            item_titled("Parent", ItemType::Epic, None),
        )
        .await
        .unwrap();
    let child = repo
        .create_item(
            project.id,
            "Done",
            item_titled("Only child", ItemType::Task, Some(parent.id)),
        )
        .await
        .unwrap();
    assert_eq!(child.status, "Done");

    let before = repo.get_item_version(parent.id).await.unwrap().unwrap();
    let completed = repo
        .check_and_update_parent_status(parent.id, "Done")
        .await
        .expect("db call");
    assert!(completed, "the only child is already Done");
    let after = repo.get_item_version(parent.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "check_and_update_parent_status must bump version: {before} -> {after}"
    );
}
