//! The rules behind `POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`: one winner
//! among racing answers, and the attempt resuming once its last pending decision is settled.

mod decisions;

use decisions::{
    attempt_and_request_state, clock, decision_state, pending_decision, running_attempt, seed_store,
};
use serde_json::json;
use tack_app::runs::resolve_decision::{ResolveOutcome, resolve_decision};
use tack_core::decision::DecisionResolvedBy;
use tack_protocol::execution::ExecutionState::{Running, WaitingDecision};
use tack_test_support::{seed, setup_file_test_db, setup_test_db};

#[tokio::test]
async fn resolving_the_last_pending_decision_resumes_attempt_and_request() {
    let repo = setup_test_db().await;
    let clock = clock();
    let item_id = seed_store(&repo, &clock).await;
    let attempt_id = running_attempt(&repo, &clock, &item_id, "resume").await;
    pending_decision(&repo, &clock, &attempt_id, "dec-a", None).await;
    pending_decision(&repo, &clock, &attempt_id, "dec-b", None).await;
    assert_eq!(
        attempt_and_request_state(&repo, &attempt_id).await,
        (WaitingDecision, WaitingDecision)
    );
    let operator = DecisionResolvedBy::operator("operator:local");

    let first = resolve_decision(
        &repo,
        &clock,
        &attempt_id,
        "dec-a",
        &json!({"option_id": "allow_once", "text": null}),
        &operator,
    )
    .await
    .expect("first answer");
    assert!(matches!(
        first,
        ResolveOutcome::Resolved {
            replayed: false,
            ..
        }
    ));
    assert_eq!(
        attempt_and_request_state(&repo, &attempt_id).await,
        (WaitingDecision, WaitingDecision),
        "one answer of two leaves the attempt waiting"
    );

    resolve_decision(
        &repo,
        &clock,
        &attempt_id,
        "dec-b",
        &json!({"option_id": "deny", "text": null}),
        &operator,
    )
    .await
    .expect("second answer");
    assert_eq!(
        attempt_and_request_state(&repo, &attempt_id).await,
        (Running, Running)
    );
}

/// Two operators answer the same decision while a third writer holds the store's write lock. Both
/// answers queue behind it and run back to back once it lets go. Reading the decision and writing
/// the answer in one `BEGIN IMMEDIATE` transaction is what makes the second one see the first's
/// answer; with a plain `BEGIN` both read `pending` and the second overwrote the first.
#[tokio::test]
async fn concurrent_resolves_serialize_to_exactly_one_winner() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let repo = setup_file_test_db(&dir.path().join("resolve-race.db")).await;
    let clock = clock();
    let item_id = seed_store(&repo, &clock).await;
    let attempt_id = running_attempt(&repo, &clock, &item_id, "race").await;
    pending_decision(&repo, &clock, &attempt_id, "dec-race", None).await;

    let lock = seed::agent_runner_row_held(&repo, decisions::RUNNER_ID).await;
    let allow = json!({"option_id": "allow_once", "text": null});
    let deny = json!({"option_id": "deny", "text": null});
    let op_a = DecisionResolvedBy::operator("operator-a");
    let op_b = DecisionResolvedBy::operator("operator-b");
    let resolve_a = resolve_decision(&repo, &clock, &attempt_id, "dec-race", &allow, &op_a);
    let resolve_b = resolve_decision(&repo, &clock, &attempt_id, "dec-race", &deny, &op_b);
    let release = async {
        for _ in 0..512 {
            tokio::task::yield_now().await;
        }
        lock.release().await;
    };

    let (outcome_a, outcome_b, ()) = tokio::join!(resolve_a, resolve_b, release);
    let outcomes = [
        outcome_a.expect("resolve A must not error under BEGIN IMMEDIATE"),
        outcome_b.expect("resolve B must not error under BEGIN IMMEDIATE"),
    ];

    assert_eq!(
        decision_state(&repo, &attempt_id, "dec-race").await,
        "resolved"
    );
    // Exactly one of the two calls won the write; the other observed the by-then-committed
    // resolution and reported a conflict, never a silent second write.
    let resolved = outcomes
        .iter()
        .filter(|o| {
            matches!(
                o,
                ResolveOutcome::Resolved {
                    replayed: false,
                    ..
                }
            )
        })
        .count();
    let conflicts = outcomes
        .iter()
        .filter(|o| matches!(o, ResolveOutcome::IdempotencyConflict))
        .count();
    assert_eq!(
        resolved, 1,
        "exactly one racer must land the fresh write: {outcomes:?}"
    );
    assert_eq!(
        conflicts, 1,
        "the loser must observe idempotency_conflict, not silently succeed: {outcomes:?}"
    );
}
