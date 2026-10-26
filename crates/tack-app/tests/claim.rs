//! The rule behind `POST /claim`: the scheduler picks, the claim leases once per
//! `claim_request_id`, and a runner with nothing eligible gets no lease.

mod common;

use chrono::Duration;
use common::{RUNNER_ID, enqueue, world};
use tack_app::runs::claim::claim;
use tack_db::queries::{agent_runners, execution_requests};
use tack_protocol::execution::ExecutionState;

const LEASE: Duration = Duration::seconds(60);

#[tokio::test]
async fn no_queued_request_means_no_lease() {
    let world = world().await;

    let claimed = claim(&world.repo, &world.clock, RUNNER_ID, "claim-empty", LEASE)
        .await
        .expect("claim");

    assert!(claimed.is_none());
}

#[tokio::test]
async fn a_queued_request_is_leased_to_the_runner_and_uses_one_slot() {
    let world = world().await;
    let request_id = enqueue(&world, "one-key").await;

    let claimed = claim(&world.repo, &world.clock, RUNNER_ID, "claim-one", LEASE)
        .await
        .expect("claim")
        .expect("work");

    assert_eq!(claimed.lease.request_id, request_id);
    assert_eq!(claimed.lease.runner_id, RUNNER_ID);
    assert_eq!(
        execution_requests::request_state(&world.repo, &request_id)
            .await
            .unwrap(),
        Some(ExecutionState::Leased)
    );
    assert_eq!(
        agent_runners::available_capacity(&world.repo, RUNNER_ID)
            .await
            .unwrap(),
        Some(1)
    );
}

#[tokio::test]
async fn the_same_claim_request_id_returns_the_same_lease_and_takes_no_second_slot() {
    let world = world().await;
    enqueue(&world, "replay-key").await;
    enqueue(&world, "other-key").await;

    let first = claim(&world.repo, &world.clock, RUNNER_ID, "claim-replay", LEASE)
        .await
        .expect("claim")
        .expect("work");
    let replay = claim(&world.repo, &world.clock, RUNNER_ID, "claim-replay", LEASE)
        .await
        .expect("claim")
        .expect("work");

    assert_eq!(replay.lease.attempt_id, first.lease.attempt_id);
    assert_eq!(replay.lease.fencing_token, first.lease.fencing_token);
    assert_eq!(
        agent_runners::available_capacity(&world.repo, RUNNER_ID)
            .await
            .unwrap(),
        Some(1)
    );
}

#[tokio::test]
async fn a_runner_with_no_free_capacity_gets_no_lease() {
    let world = world().await;
    enqueue(&world, "a").await;
    enqueue(&world, "b").await;
    enqueue(&world, "c").await;
    for id in ["claim-a", "claim-b"] {
        claim(&world.repo, &world.clock, RUNNER_ID, id, LEASE)
            .await
            .expect("claim")
            .expect("work");
    }

    let third = claim(&world.repo, &world.clock, RUNNER_ID, "claim-c", LEASE)
        .await
        .expect("claim");

    assert!(third.is_none());
}
