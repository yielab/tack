//! `items.version` (migration 034) is the
//! optimistic-concurrency counter `handlers::items`'s `ETag`/`If-Match`
//! support is built on. A counter that only moves on the obvious write path
//! is worse than none — a caller would trust a stale `ETag` computed from a
//! version some other write silently left behind — so every place
//! `crates/tack-db/src/repo/items.rs` runs `UPDATE items` must bump it.
//! `Repository::update_item`, `update_item_status_checked`, and
//! `check_and_update_parent_status` are the three write paths that touch it;
//! this file drives all three directly and asserts the counter actually
//! moved, plus the atomic compare-and-swap `claim_item_version` gives the
//! HTTP layer its concurrency guard.

use crate::common::{self, create_test_workspace, make_item, make_project};
use tack_core::models::{CreateItem, ItemType, Priority, UpdateItem};
use tack_core::workflow::StatusCategory;

/// Scrum's "In Progress" column has `wip_limit: Some(5)` — comfortably above
/// the single item this test moves into it.
const TARGET: &str = "In Progress";

#[tokio::test]
async fn a_freshly_created_item_starts_at_version_one() {
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
async fn version_increments_on_every_item_update() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    // ── update_item ─────────────────────────────────────────────────────
    let item = make_item(&repo, &project).await;
    let before = repo.get_item_version(item.id).await.unwrap().unwrap();
    repo.update_item(
        item.id,
        UpdateItem {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .await
    .expect("db call")
    .expect("item exists");
    let after = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "update_item must bump version: {before} -> {after}"
    );

    // ── update_item_status_checked ──────────────────────────────────────
    let item2 = make_item(&repo, &project).await;
    let before2 = repo.get_item_version(item2.id).await.unwrap().unwrap();
    repo.update_item_status_checked(
        item2.id,
        project.id,
        TARGET,
        Some(StatusCategory::InProgress),
        &project.workflow,
    )
    .await
    .expect("db call")
    .expect("item exists");
    let after2 = repo.get_item_version(item2.id).await.unwrap().unwrap();
    assert!(
        after2 > before2,
        "update_item_status_checked must bump version: {before2} -> {after2}"
    );

    // ── check_and_update_parent_status ──────────────────────────────────
    let parent = repo
        .create_item(
            project.id,
            "Backlog",
            CreateItem {
                title: "Parent".into(),
                description: None,
                item_type: Some(ItemType::Epic),
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
        .unwrap();
    let child = repo
        .create_item(
            project.id,
            "Done",
            CreateItem {
                title: "Only child".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: Some(parent.id),
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
        .unwrap();
    assert_eq!(child.status, "Done");

    let before3 = repo.get_item_version(parent.id).await.unwrap().unwrap();
    let completed = repo
        .check_and_update_parent_status(parent.id, "Done")
        .await
        .expect("db call");
    assert!(completed, "the only child is already Done");
    let after3 = repo.get_item_version(parent.id).await.unwrap().unwrap();
    assert!(
        after3 > before3,
        "check_and_update_parent_status must bump version: {before3} -> {after3}"
    );
}

#[tokio::test]
async fn claim_item_version_succeeds_exactly_once_for_a_given_expected_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let v1 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(v1, 1);

    // First claim against the true current version succeeds and bumps it.
    let claimed = repo.claim_item_version(item.id, v1).await.expect("db call");
    assert!(claimed, "claiming the true current version must succeed");
    let v2 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(v2, v1 + 1);

    // A second claim reusing the now-stale `v1` must fail — this is the
    // exact shape of a caller that lost a concurrent race.
    let stale_claim = repo.claim_item_version(item.id, v1).await.expect("db call");
    assert!(
        !stale_claim,
        "claiming an already-superseded version must fail, not silently re-apply"
    );
    // And the failed claim must not itself have moved the counter.
    let v3 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(
        v3, v2,
        "a failed claim must be a no-op, not a partial write"
    );
}

#[tokio::test]
async fn claim_item_version_against_an_unknown_item_returns_false_not_an_error() {
    let repo = common::setup_test_db().await;
    let claimed = repo
        .claim_item_version(uuid::Uuid::new_v4(), 1)
        .await
        .expect("db call must not error for a missing row");
    assert!(!claimed);
}
