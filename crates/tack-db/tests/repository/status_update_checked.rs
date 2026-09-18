//! Sequential correctness tests for `Repository::update_item_status_checked`
//! — the atomic WIP-limit-check-then-write that
//! replaced `dispatcher::apply_mapped_status`'s old, racy
//! `count_items_by_status` + `update_item` pair. These are deterministic,
//! single-writer checks of the method's own behaviour (applied vs.
//! rejected, timestamp bookkeeping, the `None`-on-vanished-item case); the
//! genuinely concurrent reproduction of the race this method fixes lives in
//! an HTTP-level race test, since it needs the full
//! dispatch flow to drive real concurrent HTTP requests.

use crate::common::{self, create_test_workspace, make_project};
use tack_core::CoreError;
use tack_core::models::{CreateItem, ItemType, Priority};
use tack_db::repo::items::StatusUpdateOutcome;
use uuid::Uuid;

/// Scrum's "In Progress" column (`wip_limit: Some(5)`) is used throughout —
/// see `tack_core::workflow::scrum_workflow`.
const TARGET: &str = "In Progress";
const WIP_LIMIT: usize = 5;

#[tokio::test]
async fn applies_transition_under_limit() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.status, "Backlog");

    let outcome = repo
        .update_item_status_checked(item.id, project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call")
        .expect("item exists");

    match outcome {
        StatusUpdateOutcome::Applied(updated) => {
            assert_eq!(updated.status, TARGET);
        }
        StatusUpdateOutcome::Rejected(e) => panic!("expected Applied, got Rejected({e})"),
    }

    let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, TARGET);
}

fn titled_item(title: String) -> CreateItem {
    CreateItem {
        title,
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
    }
}

/// Fills `status` with `count` freshly created items, asserting each lands there.
async fn fill_column(repo: &tack_db::Repository, project_id: Uuid, status: &str, count: usize) {
    for i in 0..count {
        let filler = repo
            .create_item(project_id, status, titled_item(format!("Filler {i}")))
            .await
            .unwrap();
        assert_eq!(filler.status, status);
    }
}

async fn assert_column_count(
    repo: &tack_db::Repository,
    project_id: Uuid,
    status: &str,
    want: i64,
) {
    assert_eq!(
        repo.count_items_by_status(project_id, status)
            .await
            .unwrap(),
        want
    );
}

fn assert_wip_limit_exceeded(outcome: StatusUpdateOutcome, column: &str, limit: usize) {
    match outcome {
        StatusUpdateOutcome::Rejected(CoreError::WipLimitExceeded {
            column: c,
            limit: l,
            current,
        }) => {
            assert_eq!(c, column);
            assert_eq!(l, limit);
            assert_eq!(current, limit);
        }
        StatusUpdateOutcome::Rejected(other) => panic!("expected WipLimitExceeded, got {other}"),
        StatusUpdateOutcome::Applied(_) => panic!("expected Rejected, the column is already full"),
    }
}

#[tokio::test]
async fn rejects_at_limit_leaves_item_and_count_untouched() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    fill_column(&repo, project.id, TARGET, WIP_LIMIT).await;
    assert_column_count(&repo, project.id, TARGET, WIP_LIMIT as i64).await;

    let item = common::make_item(&repo, &project).await;
    let outcome = repo
        .update_item_status_checked(item.id, project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call")
        .expect("item exists");
    assert_wip_limit_exceeded(outcome, TARGET, WIP_LIMIT);

    // The item itself was left exactly where it started — no partial write — and the
    // column's count didn't move either.
    let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, "Backlog");
    assert_column_count(&repo, project.id, TARGET, WIP_LIMIT as i64).await;
}

#[tokio::test]
async fn status_with_no_limit_always_applies() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    // "Done" has no wip_limit in scrum_workflow().
    for i in 0..50 {
        let item = repo
            .create_item(project.id, "Backlog", titled_item(format!("Item {i}")))
            .await
            .unwrap();
        let outcome = repo
            .update_item_status_checked(item.id, project.id, "Done", None, &project.workflow)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(outcome, StatusUpdateOutcome::Applied(_)));
    }
    assert_eq!(
        repo.count_items_by_status(project.id, "Done")
            .await
            .unwrap(),
        50
    );
}

async fn apply_checked(
    repo: &tack_db::Repository,
    item_id: Uuid,
    project_id: Uuid,
    status: &str,
    category: tack_core::workflow::StatusCategory,
    workflow: &tack_core::workflow::WorkflowConfig,
) -> tack_core::models::Item {
    let outcome = repo
        .update_item_status_checked(item_id, project_id, status, Some(category), workflow)
        .await
        .unwrap()
        .unwrap();
    match outcome {
        StatusUpdateOutcome::Applied(i) => *i,
        StatusUpdateOutcome::Rejected(e) => panic!("unexpected rejection: {e}"),
    }
}

#[tokio::test]
async fn status_category_stamps_started_and_completed_at() {
    use tack_core::workflow::StatusCategory;
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert!(item.started_at.is_none());
    assert!(item.completed_at.is_none());

    // Entering an InProgress-category status stamps started_at.
    let updated = apply_checked(
        &repo,
        item.id,
        project.id,
        TARGET,
        StatusCategory::InProgress,
        &project.workflow,
    )
    .await;
    assert!(updated.started_at.is_some());
    assert!(updated.completed_at.is_none());

    // Entering a Done-category status stamps completed_at, keeps started_at.
    let done = apply_checked(
        &repo,
        item.id,
        project.id,
        "Done",
        StatusCategory::Done,
        &project.workflow,
    )
    .await;
    assert!(done.started_at.is_some());
    assert!(done.completed_at.is_some());
}

#[tokio::test]
async fn unknown_item_id_returns_none_not_error() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    // The count+limit check has nothing to do with a specific item id, so
    // this only fails at the final reload — proving a vanished item (e.g.
    // deleted concurrently) surfaces as `None`, not a decode/row error.
    let outcome = repo
        .update_item_status_checked(Uuid::new_v4(), project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call itself must not fail");
    assert!(outcome.is_none());
}
