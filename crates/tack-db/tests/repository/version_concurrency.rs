//! `items.version` (migration 034) backs `handlers::items`'s `ETag`/`If-Match`
//! support. `update_item_atomically` is the write path that must bump it.

use crate::common::{self, create_test_workspace, make_item, make_project};
use tack_core::models::UpdateItem;

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
