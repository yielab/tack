//! The rule behind `POST /api/executions/{id}/requeue`: only a recovered `needs_operator` run goes
//! back to the queue, once per confirmation, and the recovery is audited.

use crate::support;

use chrono::Utc;
use support::{board, full_request, start};
use tack_app::AppError;
use tack_app::runs::requeue::requeue;
use tack_db::queries;
use tack_protocol::SystemClock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

#[tokio::test]
async fn only_needs_operator_can_be_requeued_and_recovery_is_audited() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let request_id = started.request_id;
    let now = Utc::now().to_rfc3339();
    seed::execution_attempt(
        &board.repo,
        seed::AttemptSeed {
            id: "attempt-1",
            request_id: &request_id,
            runner_id: "runner-active",
            state: ExecutionState::Lost,
            at: &now,
            usage: None,
            actual_execution: None,
        },
    )
    .await;
    let denied = requeue(
        &board.repo,
        &SystemClock,
        &request_id,
        "operator-recovery-1",
        "operator reviewed",
        "operator-1",
    )
    .await;
    match denied {
        Err(AppError::InvalidTransition { from, to }) => {
            assert_eq!(from, "lost");
            assert_eq!(to, "queued");
        }
        other => panic!("expected an invalid transition, got {other:?}"),
    }

    seed::execution_attempt_state(&board.repo, "attempt-1", ExecutionState::NeedsOperator).await;
    seed::execution_request_state(&board.repo, &request_id, ExecutionState::NeedsOperator).await;
    seed::execution_recovery_audit(
        &board.repo,
        "attempt-1",
        "runner-recovery-1",
        "needs_operator",
        &now,
    )
    .await;
    let allowed = requeue(
        &board.repo,
        &SystemClock,
        &request_id,
        "operator-recovery-1",
        "operator reviewed",
        "operator-1",
    )
    .await
    .unwrap();
    assert!(!allowed.replayed);
    assert_eq!(
        queries::execution_requests::request_state(&board.repo, &request_id)
            .await
            .unwrap(),
        Some(ExecutionState::Queued)
    );
    assert_eq!(
        queries::execution_attempts::operator_requeues_of_attempt(&board.repo, "attempt-1")
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn the_same_confirmation_replays_and_another_reason_under_the_key_conflicts() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let request_id = started.request_id;
    let now = Utc::now().to_rfc3339();
    seed::execution_attempt(
        &board.repo,
        seed::AttemptSeed {
            id: "attempt-1",
            request_id: &request_id,
            runner_id: "runner-active",
            state: ExecutionState::NeedsOperator,
            at: &now,
            usage: None,
            actual_execution: None,
        },
    )
    .await;
    seed::execution_request_state(&board.repo, &request_id, ExecutionState::NeedsOperator).await;
    seed::execution_recovery_audit(
        &board.repo,
        "attempt-1",
        "runner-recovery-1",
        "needs_operator",
        &now,
    )
    .await;
    let confirm = |reason: &'static str| {
        requeue(
            &board.repo,
            &SystemClock,
            &request_id,
            "operator-recovery-1",
            reason,
            "operator-1",
        )
    };
    assert!(!confirm("operator reviewed").await.unwrap().replayed);
    assert!(confirm("operator reviewed").await.unwrap().replayed);
    let conflict = confirm("another reason").await;
    assert!(
        matches!(conflict, Err(AppError::Conflict(_))),
        "{conflict:?}"
    );
}

#[tokio::test]
async fn a_run_with_no_attempt_cannot_be_requeued_and_names_an_unknown_state() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let denied = requeue(
        &board.repo,
        &SystemClock,
        &started.request_id,
        "key",
        "reason",
        "operator-1",
    )
    .await;
    match denied {
        Err(AppError::InvalidTransition { from, .. }) => assert_eq!(from, "unknown"),
        other => panic!("expected an invalid transition, got {other:?}"),
    }
}
