//! The store's shared conventions: `StoreError`, the state conversion, the
//! `queries` read functions and a command that raises `NotFound` / `Conflict`.

use crate::common::execution_fixture::Fixture;

use tack_db::StoreError;
use tack_db::queries::{execution_attempts, execution_decisions, execution_requests};
use tack_db::repo::execution::state;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

#[test]
fn a_store_error_names_what_it_is_and_wraps_sqlx() {
    assert_eq!(StoreError::NotFound.to_string(), "not found");
    assert_eq!(
        StoreError::Conflict("already_reviewed".into()).to_string(),
        "conflict: already_reviewed"
    );
    let wrapped: StoreError = sqlx::Error::RowNotFound.into();
    assert!(matches!(wrapped, StoreError::Sql(sqlx::Error::RowNotFound)));
}

#[test]
fn every_state_round_trips_through_its_column_text() {
    for state in [
        ExecutionState::Queued,
        ExecutionState::Leased,
        ExecutionState::Preparing,
        ExecutionState::Running,
        ExecutionState::WaitingDecision,
        ExecutionState::Succeeded,
        ExecutionState::Failed,
        ExecutionState::Cancelled,
        ExecutionState::Lost,
        ExecutionState::NeedsOperator,
    ] {
        assert_eq!(state::parse(state::text(state)).unwrap(), state);
    }
    let corrupt = state::parse("sleeping").unwrap_err();
    assert!(
        matches!(corrupt, StoreError::Sql(sqlx::Error::Decode(_))),
        "{corrupt:?}"
    );
}

#[tokio::test]
async fn the_queries_answer_from_the_rows() {
    let fx = Fixture::new().await;
    fx.ready_completion_attempt("request-q", "attempt-q").await;

    assert!(
        execution_requests::execution_request_exists(&fx.repo, "request-q")
            .await
            .unwrap()
    );
    assert!(
        !execution_requests::execution_request_exists(&fx.repo, "nobody")
            .await
            .unwrap()
    );

    assert_eq!(
        execution_attempts::attempt_state(&fx.repo, "attempt-q")
            .await
            .unwrap(),
        Some(ExecutionState::Leased)
    );
    assert_eq!(
        execution_attempts::attempt_state(&fx.repo, "nobody")
            .await
            .unwrap(),
        None
    );

    seed::execution_decision(&fx.repo, "attempt-q", "later", "2026-01-02T00:00:00Z").await;
    seed::execution_decision(&fx.repo, "attempt-q", "first", "2026-01-01T00:00:00Z").await;
    let decisions = execution_decisions::decisions_of_attempt(&fx.repo, "request-q", 1)
        .await
        .unwrap()
        .expect("attempt 1 exists");
    let order: Vec<&str> = decisions.iter().map(|d| d.decision_id.as_str()).collect();
    assert_eq!(order, ["first", "later"]);
    assert!(
        execution_decisions::decisions_of_attempt(&fx.repo, "request-q", 2)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_review_is_recorded_once_and_the_store_says_why_not() {
    let fx = Fixture::new().await;
    fx.ready_completion_attempt("request-r", "attempt-r").await;

    let missing = fx
        .repo
        .record_attempt_review("request-r", 9, "accepted", None, "2026-01-01T00:00:00Z")
        .await;
    assert!(matches!(missing, Err(StoreError::NotFound)), "{missing:?}");

    let attempt_id = fx
        .repo
        .record_attempt_review(
            "request-r",
            1,
            "accepted",
            Some("looks right"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
    assert_eq!(attempt_id, "attempt-r");

    let second = fx
        .repo
        .record_attempt_review("request-r", 1, "rejected", None, "2026-01-02T00:00:00Z")
        .await;
    assert!(
        matches!(&second, Err(StoreError::Conflict(reason)) if reason == "already_reviewed"),
        "{second:?}"
    );
}
