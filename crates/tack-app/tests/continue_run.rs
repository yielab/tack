//! `continue_run`: a new execution request that follows the same docket task a stopped run left
//! behind (Phase 87, decisions 5 and 6), instead of posting a new one.

use crate::support;
use tack_protocol::Clock;

use serde_json::{Value, json};
use support::{board, full_request, start};
use tack_app::runs::continue_run::continue_run;
use tack_protocol::SystemClock;
use tack_protocol::execution::{ExecutionState, StableErrorCode};
use tack_test_support::seed::{self, AttemptSeed};

/// A full request under its own idempotency key, so several can be started on the same item
/// without replaying one another.
fn keyed_request(item_id: uuid::Uuid, key: &str) -> tack_app::runs::start_run::RunRequest {
    let mut request = full_request(item_id);
    request.idempotency_key = key.into();
    request
}

/// Plants the newest attempt of `request_id` as a budget stop that offers (or refuses) Continue:
/// the docket team and task its evidence names, whether `next.continue` says `possible`, and the
/// tokens it counted when it stopped.
async fn leave_budget_stop(
    board: &support::Board,
    request_id: &str,
    pod: &str,
    task: &str,
    possible: bool,
    why: Option<&str>,
    used: (u64, u64),
) {
    seed::execution_attempt(
        &board.repo,
        AttemptSeed {
            id: &format!("attempt-{request_id}"),
            request_id,
            runner_id: "runner-active",
            state: ExecutionState::Failed,
            at: "2026-01-01T00:00:00Z",
            usage: Some(
                &json!({
                    "tokens_in": {"value": used.0, "source": "measured"},
                    "tokens_out": {"value": used.1, "source": "measured"},
                })
                .to_string(),
            ),
            // `additional` is `#[serde(flatten)]` on `ActualExecution`, so docket's own `team`
            // key sits at the top level of the stored JSON, never nested (a real bug this test
            // once matched instead of catching: Phase 87's live proof, docs/closed-cycles/plans/phase-87.md).
            actual_execution: Some(&json!({"team": pod}).to_string()),
        },
    )
    .await;
    let mut next = json!({"possible": possible});
    if let Some(why) = why {
        next["why"] = json!(why);
    }
    seed::execution_attempt_ended(
        &board.repo,
        &format!("attempt-{request_id}"),
        ExecutionState::Failed,
        &json!({"code": "step_budget", "task": task, "next": {"continue": next}}).to_string(),
    )
    .await;
}

async fn stopped_at(
    board: &support::Board,
    key: &str,
    pod: &str,
    task: &str,
    possible: bool,
    why: Option<&str>,
    used: (u64, u64),
) -> String {
    let mut request = keyed_request(board.item_id, key);
    request.budgets = json!({"tokens": 1000});
    let stopped = start(board, request).await.unwrap();
    leave_budget_stop(board, &stopped.request_id, pod, task, possible, why, used).await;
    stopped.request_id
}

async fn snapshot_of(repo: &tack_db::Repository, request_id: &str) -> Value {
    let raw = tack_db::queries::execution_requests::request_snapshot(repo, request_id)
        .await
        .expect("read snapshot")
        .expect("request exists");
    serde_json::from_str(&raw).expect("snapshot json")
}

#[tokio::test]
async fn continue_run_follows_the_same_task_with_what_the_limit_left() {
    let board = board().await;
    let source = stopped_at(
        &board,
        "stopped-1",
        "saas-dev",
        "task-0001",
        true,
        None,
        (700, 100),
    )
    .await;

    let started = continue_run(&board.repo, SystemClock.now(), &source)
        .await
        .unwrap();
    assert!(!started.replayed);
    assert_ne!(started.request_id, source);

    let snapshot = snapshot_of(&board.repo, &started.request_id).await;
    assert_eq!(snapshot["metadata"]["continues"], json!(source));
    assert_eq!(
        snapshot["metadata"]["continue_task"],
        json!("saas-dev/task-0001")
    );
    assert_eq!(
        snapshot["metadata"]["continue_baseline_usage"],
        json!({"input": 700, "output": 100})
    );
    // 1000 was the source run's own limit; it had counted 700 + 100 = 800 against it.
    assert_eq!(snapshot["budgets"]["tokens"], json!(200));
}

#[tokio::test]
async fn continue_run_is_idempotent_on_the_source_request() {
    let board = board().await;
    let source = stopped_at(
        &board,
        "stopped-replay",
        "saas-dev",
        "task-0002",
        true,
        None,
        (0, 0),
    )
    .await;

    let first = continue_run(&board.repo, SystemClock.now(), &source)
        .await
        .unwrap();
    let second = continue_run(&board.repo, SystemClock.now(), &source)
        .await
        .unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(first.request_id, second.request_id);
}

#[tokio::test]
async fn continue_run_is_refused_when_its_own_ending_never_offered_it() {
    let board = board().await;
    let source = stopped_at(
        &board,
        "stopped-no",
        "saas-dev",
        "task-0003",
        false,
        Some("The step changed nothing; the task may be unclear. Reword it before running again."),
        (500, 0),
    )
    .await;

    let refusal = continue_run(&board.repo, SystemClock.now(), &source)
        .await
        .unwrap_err();
    assert_eq!(refusal.status(), 422);
    assert_eq!(refusal.code, StableErrorCode::Unprocessable);
    assert_eq!(refusal.details["field"], "continues");
    assert_eq!(refusal.details["reason"], "continues_not_offered");
    assert_eq!(
        refusal.message,
        "The step changed nothing; the task may be unclear. Reword it before running again."
    );
}

#[tokio::test]
async fn continue_run_is_refused_when_it_names_no_request() {
    let board = board().await;
    let refusal = continue_run(&board.repo, SystemClock.now(), "exec_does-not-exist")
        .await
        .unwrap_err();
    assert_eq!(refusal.status(), 404);
    assert_eq!(refusal.details["field"], "continues");
    assert_eq!(refusal.details["reason"], "continues_not_found");
}

#[tokio::test]
async fn continue_run_is_refused_when_nothing_is_left_of_the_limit() {
    let board = board().await;
    // The source's own limit is 1000 (`stopped_at`): counted all the way to its last token.
    let source = stopped_at(
        &board,
        "stopped-spent",
        "saas-dev",
        "task-0004",
        true,
        None,
        (900, 100),
    )
    .await;

    let refusal = continue_run(&board.repo, SystemClock.now(), &source)
        .await
        .unwrap_err();
    assert_eq!(refusal.status(), 422);
    assert_eq!(refusal.details["reason"], "continues_no_tokens_left");
}
