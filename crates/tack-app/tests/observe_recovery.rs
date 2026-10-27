//! The rule behind `POST /attempts/{attempt_id}/recovery-observation`: a process proven stopped
//! before it spawned requeues the request, the same observation again is a replay, and a fence
//! that is no longer current is refused.

use crate::common;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, ClaimRequestId, FencingToken, RecoveryKey, RunnerId};

use chrono::Duration;
use common::{RUNNER_ID, enqueue, world};
use tack_app::runs::claim::claim;
use tack_app::runs::observe_recovery::{Observation, RecoveryObserved, observe_recovery};
use tack_db::queries::{execution_attempts, execution_requests};
use tack_protocol::execution::{ExecutionState, RecoveryDisposition, RecoveryObservation};

const DETAILS: &str = r#"{"journal_state":"prepared","process_observed":false}"#;

async fn observe(
    world: &common::World,
    attempt_id: &str,
    fencing_token: FencingToken,
    recovery_key: &str,
    observation: RecoveryObservation,
) -> RecoveryObserved {
    observe_recovery(
        &world.repo,
        world.clock.now(),
        Observation {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(attempt_id),
            fencing_token,
            recovery_key: &RecoveryKey::new(recovery_key),
            observation,
            details_json: DETAILS,
        },
    )
    .await
    .expect("observe")
}

#[tokio::test]
async fn a_process_proven_stopped_requeues_the_request_and_replays_idempotently() {
    let world = world().await;
    let request_id = enqueue(&world, "recovery-key").await;
    let claimed = claim(
        &world.repo,
        world.clock.now(),
        &RunnerId::new(RUNNER_ID),
        &ClaimRequestId::new("claim-recovery"),
        Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");
    let attempt_id = claimed.lease.attempt_id.into_inner();
    let fence = claimed.lease.fencing_token;

    let RecoveryObserved::Recorded(applied) = observe(
        &world,
        &attempt_id,
        fence,
        "recovery-1",
        RecoveryObservation::ProcessStopped,
    )
    .await
    else {
        panic!("the observation is recorded");
    };
    assert_eq!(
        applied.disposition,
        RecoveryDisposition::SafePreSpawnRequeue
    );
    assert!(!applied.replayed);
    assert_eq!(
        execution_attempts::attempt_state(&world.repo, &attempt_id)
            .await
            .unwrap(),
        Some(ExecutionState::Lost)
    );
    assert_eq!(
        execution_requests::request_state(&world.repo, &request_id)
            .await
            .unwrap(),
        Some(ExecutionState::Queued)
    );

    let RecoveryObserved::Recorded(replayed) = observe(
        &world,
        &attempt_id,
        fence,
        "recovery-1",
        RecoveryObservation::ProcessStopped,
    )
    .await
    else {
        panic!("the replay is recorded");
    };
    assert!(replayed.replayed);
    assert_eq!(replayed.committed_at, applied.committed_at);
}

#[tokio::test]
async fn a_recovery_key_reused_with_a_different_observation_conflicts() {
    let world = world().await;
    enqueue(&world, "conflict-key").await;
    let claimed = claim(
        &world.repo,
        world.clock.now(),
        &RunnerId::new(RUNNER_ID),
        &ClaimRequestId::new("claim-conflict"),
        Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");
    let attempt_id = claimed.lease.attempt_id.into_inner();
    let fence = claimed.lease.fencing_token;
    observe(
        &world,
        &attempt_id,
        fence,
        "recovery-1",
        RecoveryObservation::ProcessStopped,
    )
    .await;

    let outcome = observe(
        &world,
        &attempt_id,
        fence,
        "recovery-1",
        RecoveryObservation::Ambiguous,
    )
    .await;

    assert_eq!(outcome, RecoveryObserved::IdempotencyConflict);
}

#[tokio::test]
async fn an_older_fence_is_stale_and_changes_nothing() {
    let world = world().await;
    enqueue(&world, "stale-key").await;
    let claimed = claim(
        &world.repo,
        world.clock.now(),
        &RunnerId::new(RUNNER_ID),
        &ClaimRequestId::new("claim-stale"),
        Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");
    let attempt_id = claimed.lease.attempt_id.into_inner();

    let outcome = observe(
        &world,
        &attempt_id,
        FencingToken::new(claimed.lease.fencing_token.get() + 1),
        "recovery-stale",
        RecoveryObservation::ProcessStopped,
    )
    .await;

    assert_eq!(outcome, RecoveryObserved::Stale);
    assert_eq!(
        execution_attempts::attempt_state(&world.repo, &attempt_id)
            .await
            .unwrap(),
        Some(ExecutionState::Leased)
    );
}
