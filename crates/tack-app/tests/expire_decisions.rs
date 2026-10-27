//! The fail-closed expiry of a decision nobody answered in time: by the sweep, and lazily when an
//! answer arrives after the deadline.

use crate::decisions;

use chrono::Duration;
use decisions::{
    attempt_and_request_state, clock, decision_state, pending_decision, running_attempt, seed_store,
};
use serde_json::json;
use tack_app::retention;
use tack_app::runs::expire_decisions::expire_decisions;
use tack_app::runs::resolve_decision::{ResolveOutcome, resolve_decision};
use tack_core::decision::DecisionResolvedBy;
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState::{Running, WaitingDecision};
use tack_test_support::setup_test_db;

#[tokio::test]
async fn the_sweep_expires_only_overdue_pending_decisions() {
    let repo = setup_test_db().await;
    let clock = clock();
    let item_id = seed_store(&repo, &clock).await;
    let attempt_id = running_attempt(&repo, &clock, &item_id, "sweep").await;
    let overdue = clock.now() - Duration::seconds(5);
    let future = clock.now() + Duration::seconds(5);
    let rows = [
        ("dec-overdue-1", Some(overdue), "expired"),
        ("dec-overdue-2", Some(overdue), "expired"),
        ("dec-future", Some(future), "pending"),
        ("dec-no-expiry", None, "pending"),
    ];
    for (decision_id, expires_at, _) in rows {
        pending_decision(&repo, &clock, &attempt_id, decision_id, expires_at).await;
    }

    let expired = expire_decisions(&repo, clock.now()).await.expect("sweep");

    assert_eq!(expired, 2);
    for (decision_id, _, expected_state) in rows {
        assert_eq!(
            decision_state(&repo, &attempt_id, decision_id).await,
            expected_state,
            "{decision_id}"
        );
    }
}

#[tokio::test]
async fn expiry_resumes_the_attempt_like_a_resolution_does() {
    let repo = setup_test_db().await;
    let clock = clock();
    let item_id = seed_store(&repo, &clock).await;
    let overdue = clock.now() - Duration::seconds(5);

    // Lazy expiry: the fail-closed answer to an overdue decision settles it.
    let lazy = running_attempt(&repo, &clock, &item_id, "expire-lazy").await;
    pending_decision(&repo, &clock, &lazy, "dec-late", Some(overdue)).await;
    assert_eq!(
        attempt_and_request_state(&repo, &lazy).await,
        (WaitingDecision, WaitingDecision)
    );
    let outcome = resolve_decision(
        &repo,
        clock.now(),
        &lazy,
        "dec-late",
        &json!({"option_id": "allow_once", "text": null}),
        &DecisionResolvedBy::operator("operator:local"),
    )
    .await
    .expect("late answer");
    assert_eq!(outcome, ResolveOutcome::Expired);
    assert_eq!(
        attempt_and_request_state(&repo, &lazy).await,
        (Running, Running)
    );

    // The periodic sweep settles the rest the same way, one attempt at a time: a still-pending
    // decision keeps its attempt waiting.
    let swept = running_attempt(&repo, &clock, &item_id, "expire-sweep").await;
    pending_decision(&repo, &clock, &swept, "dec-overdue", Some(overdue)).await;
    let kept = running_attempt(&repo, &clock, &item_id, "expire-kept").await;
    pending_decision(&repo, &clock, &kept, "dec-overdue", Some(overdue)).await;
    pending_decision(&repo, &clock, &kept, "dec-open", None).await;

    let expired = expire_decisions(&repo, clock.now()).await.expect("sweep");

    assert_eq!(expired, 2);
    assert_eq!(
        attempt_and_request_state(&repo, &swept).await,
        (Running, Running)
    );
    assert_eq!(
        attempt_and_request_state(&repo, &kept).await,
        (WaitingDecision, WaitingDecision)
    );
}

#[tokio::test]
async fn a_retention_pass_expires_the_overdue_decisions_and_leaves_the_rest() {
    let repo = setup_test_db().await;
    let clock = clock();
    let item_id = seed_store(&repo, &clock).await;
    let attempt_id = running_attempt(&repo, &clock, &item_id, "retention").await;
    pending_decision(
        &repo,
        &clock,
        &attempt_id,
        "dec-overdue",
        Some(clock.now() - Duration::seconds(5)),
    )
    .await;
    pending_decision(
        &repo,
        &clock,
        &attempt_id,
        "dec-future",
        Some(clock.now() + Duration::minutes(30)),
    )
    .await;

    let outcome = retention::tick(&repo, &clock, &retention::Settings::default())
        .await
        .expect("retention pass");

    assert_eq!(outcome.decisions_expired, 1);
    assert_eq!(
        decision_state(&repo, &attempt_id, "dec-overdue").await,
        "expired"
    );
    assert_eq!(
        decision_state(&repo, &attempt_id, "dec-future").await,
        "pending"
    );
}
