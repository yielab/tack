//! Work runs and a group is planned (ADR 0036, decision 12): a run on a group is refused unless its
//! profile plans, auto-start starts a group's work and never the group, and an item takes the
//! profile of the nearest ancestor that names one.

use crate::support;
use serde_json::json;
use support::{Board, add_profile, bare_request, board, start};
use tack_app::auto_start;
use tack_core::models::{CreateItem, CreateRole, UpdateRole};
use tack_protocol::SystemClock;
use tack_protocol::execution::StableErrorCode;
use tack_test_support::seed;
use uuid::Uuid;

async fn profile_of_kind(board: &Board, id: &str, kind: &str) {
    seed::agent_profile_with_policy(&board.repo, id, id, kind, &json!({})).await;
}

async fn item(board: &Board, title: &str, item_type: &str, parent: Option<Uuid>) -> Uuid {
    board
        .repo
        .create_item(
            board.project_id,
            "To Do",
            CreateItem {
                title: title.into(),
                item_type: Some(item_type.into()),
                parent_id: parent,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .id
}

async fn requests_of(board: &Board, item: Uuid) -> usize {
    tack_test_support::probe::execution_requests_of_item(&board.repo, &item.to_string())
        .await
        .unwrap()
        .len()
}

/// A role that names `profile`, given to `item`.
async fn give_role(board: &Board, item: Uuid, name: &str, profile: &str) {
    let role = board
        .repo
        .create_role(
            board.project_id,
            CreateRole {
                name: name.into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();
    board
        .repo
        .update_role(
            role.id,
            UpdateRole {
                name: None,
                color: None,
                icon: None,
                agent_profile_id: Some(Some(profile.into())),
            },
        )
        .await
        .unwrap();
    board.repo.assign_role_to_item(item, role.id).await.unwrap();
}

fn request_with(item: Uuid, profile: &str, key: &str) -> tack_app::runs::start_run::RunRequest {
    let mut request = bare_request(item, key);
    request.agent_profile_id = Some(profile.into());
    request.selector = None;
    request
}

#[tokio::test]
async fn an_implementer_on_an_epic_is_refused_and_creates_no_request() {
    let board = board().await;
    profile_of_kind(&board, "implementer", "implementer").await;
    let epic = item(&board, "Checkout", "epic", None).await;

    let refusal = start(&board, request_with(epic, "implementer", "k1"))
        .await
        .unwrap_err();

    assert_eq!(refusal.status(), 409);
    assert_eq!(refusal.code, StableErrorCode::GroupNotRunnable);
    assert_eq!(
        refusal.message,
        "Checkout is an epic: it holds work and is not run itself. Run its items, or plan it."
    );
    assert_eq!(requests_of(&board, epic).await, 0);
}

#[tokio::test]
async fn a_planner_or_a_researcher_runs_an_epic_and_any_profile_runs_work() {
    let board = board().await;
    profile_of_kind(&board, "planner", "planner").await;
    profile_of_kind(&board, "researcher", "researcher").await;
    profile_of_kind(&board, "implementer", "implementer").await;
    let epic = item(&board, "Checkout", "epic", None).await;
    let task = item(&board, "Pay", "task", Some(epic)).await;

    for profile in ["planner", "researcher"] {
        let started = start(&board, request_with(epic, profile, profile)).await;
        assert!(started.is_ok(), "{profile}: {started:?}");
    }
    assert_eq!(requests_of(&board, epic).await, 2);

    let started = start(&board, request_with(task, "implementer", "work")).await;
    assert!(started.is_ok(), "{started:?}");

    // A task with an item under it is a group too, and is planned, not run.
    make_child(&board, task).await;
    let refusal = start(&board, request_with(task, "implementer", "again"))
        .await
        .unwrap_err();
    assert_eq!(refusal.code, StableErrorCode::GroupNotRunnable);
}

async fn make_child(board: &Board, parent: Uuid) {
    item(board, "Receipt", "task", Some(parent)).await;
}

#[tokio::test]
async fn an_item_with_no_profile_of_its_own_takes_the_one_its_epic_names() {
    let board = board().await;
    profile_of_kind(&board, "builder", "implementer").await;
    let epic = item(&board, "Checkout", "epic", None).await;
    give_role(&board, epic, "Builder", "builder").await;
    let task = item(&board, "Pay", "task", Some(epic)).await;

    let started = start(&board, bare_request(task, "inherits")).await.unwrap();

    let stored = support::snapshot(&board.repo, &started).await;
    assert_eq!(stored["agent_profile_id"], "builder");
    assert_eq!(stored["metadata"]["profile_source"], "role:Builder");
}

#[tokio::test]
async fn auto_start_starts_the_tasks_of_an_epic_that_has_a_role_and_never_the_epic() {
    let board = board().await;
    add_profile(&board.repo, "builder", "{}", "{}").await;
    let project = board.project_id.to_string();
    seed::project_auto_start(&board.repo, &project, "To Do", "codex").await;
    seed::project_auto_start_max_open(&board.repo, &project, 10).await;
    let epic = item(&board, "Checkout", "epic", None).await;
    give_role(&board, epic, "Builder", "builder").await;
    let pay = item(&board, "Pay", "task", Some(epic)).await;
    let refund = item(&board, "Refund", "task", Some(epic)).await;
    // Another top-level task with nobody named: it waits for no one.
    let alone = item(&board, "Alone", "task", None).await;

    let outcome = auto_start::tick(&board.repo, &SystemClock).await.unwrap();

    assert_eq!(outcome.started, 2);
    assert_eq!(requests_of(&board, pay).await, 1);
    assert_eq!(requests_of(&board, refund).await, 1);
    assert_eq!(requests_of(&board, epic).await, 0);
    assert_eq!(requests_of(&board, alone).await, 0);
}
