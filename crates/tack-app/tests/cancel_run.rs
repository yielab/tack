//! The rule behind `POST /api/executions/{id}/cancel`: a run somebody holds is only marked and its
//! runner stops it; a run nobody holds ends at once; one that is over cannot be cancelled.

mod support;

use chrono::{Duration, Utc};
use support::{board, full_request, start};
use tack_app::AppError;
use tack_app::runs::cancel_run::{Cancellation, cancel_run};
use tack_db::queries;
use tack_protocol::SystemClock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

async fn state_of(board: &support::Board, request_id: &str) -> ExecutionState {
    queries::execution_requests::request_state(&board.repo, request_id)
        .await
        .unwrap()
        .expect("request exists")
}

#[tokio::test]
async fn a_queued_run_is_marked_for_cancellation_and_not_ended() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let outcome = cancel_run(&board.repo, &SystemClock, &started.request_id)
        .await
        .unwrap();
    assert_eq!(outcome, Cancellation::Requested);
    assert_eq!(
        state_of(&board, &started.request_id).await,
        ExecutionState::Queued
    );
}

#[tokio::test]
async fn a_run_nobody_holds_ends_at_once() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    let now = Utc::now();
    seed::execution_request_state(
        &board.repo,
        &started.request_id,
        ExecutionState::NeedsOperator,
    )
    .await;
    seed::execution_attempt(
        &board.repo,
        seed::AttemptSeed {
            id: "attempt-1",
            request_id: &started.request_id,
            runner_id: "runner-active",
            state: ExecutionState::NeedsOperator,
            at: &now.to_rfc3339(),
            usage: None,
            actual_execution: None,
        },
    )
    .await;
    let outcome = cancel_run(&board.repo, &SystemClock, &started.request_id)
        .await
        .unwrap();
    assert_eq!(outcome, Cancellation::Ended);
    assert_eq!(
        state_of(&board, &started.request_id).await,
        ExecutionState::Cancelled
    );
    assert_eq!(
        queries::execution_attempts::attempt_state(&board.repo, "attempt-1")
            .await
            .unwrap(),
        Some(ExecutionState::Cancelled)
    );
}

#[tokio::test]
async fn a_run_whose_lease_expired_ends_at_once_and_one_whose_lease_holds_does_not() {
    let board = board().await;
    let now = Utc::now();
    let held = start(&board, full_request(board.item_id)).await.unwrap();
    seed::execution_request_state(&board.repo, &held.request_id, ExecutionState::Running).await;
    seed::execution_attempt(
        &board.repo,
        seed::AttemptSeed {
            id: "attempt-held",
            request_id: &held.request_id,
            runner_id: "runner-active",
            state: ExecutionState::Running,
            at: &now.to_rfc3339(),
            usage: None,
            actual_execution: None,
        },
    )
    .await;
    seed::execution_attempt_lease_expires_at(
        &board.repo,
        "attempt-held",
        &(now + Duration::minutes(5)).to_rfc3339(),
    )
    .await;
    assert_eq!(
        cancel_run(&board.repo, &SystemClock, &held.request_id)
            .await
            .unwrap(),
        Cancellation::Requested
    );
    assert_eq!(
        state_of(&board, &held.request_id).await,
        ExecutionState::Running
    );

    let mut other = full_request(board.item_id);
    other.idempotency_key = "second".into();
    let lapsed = start(&board, other).await.unwrap();
    seed::execution_request_state(&board.repo, &lapsed.request_id, ExecutionState::Running).await;
    seed::execution_attempt(
        &board.repo,
        seed::AttemptSeed {
            id: "attempt-lapsed",
            request_id: &lapsed.request_id,
            runner_id: "runner-active",
            state: ExecutionState::Running,
            at: &(now - Duration::minutes(10)).to_rfc3339(),
            usage: None,
            actual_execution: None,
        },
    )
    .await;
    seed::execution_attempt_lease_expires_at(
        &board.repo,
        "attempt-lapsed",
        &(now - Duration::minutes(5)).to_rfc3339(),
    )
    .await;
    assert_eq!(
        cancel_run(&board.repo, &SystemClock, &lapsed.request_id)
            .await
            .unwrap(),
        Cancellation::Ended
    );
    assert_eq!(
        state_of(&board, &lapsed.request_id).await,
        ExecutionState::Cancelled
    );
}

#[tokio::test]
async fn a_run_that_is_over_cannot_be_cancelled_and_an_unknown_one_is_not_found() {
    let board = board().await;
    let started = start(&board, full_request(board.item_id)).await.unwrap();
    seed::execution_request_state(&board.repo, &started.request_id, ExecutionState::Succeeded)
        .await;
    let over = cancel_run(&board.repo, &SystemClock, &started.request_id).await;
    assert!(matches!(over, Err(AppError::Conflict(_))), "{over:?}");

    let unknown = cancel_run(&board.repo, &SystemClock, "exec_missing").await;
    assert!(matches!(unknown, Err(AppError::NotFound)), "{unknown:?}");
}
