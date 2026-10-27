//! The rule behind `POST /heartbeat`: a beat is idempotent on its id and content, and a lease
//! the runner no longer holds is refused.

use crate::common;
use tack_protocol::execution::ExecutionState;

use chrono::{DateTime, Duration, Utc};
use common::{RUNNER_ID, enqueue, world};
use tack_app::runs::claim::claim;
use tack_app::runs::heartbeat::{Beat, Heartbeat, ReportedLease, heartbeat};
use tack_protocol::Clock;

async fn beat(
    world: &common::World,
    sent_at: DateTime<Utc>,
    heartbeat_id: &str,
    available_capacity: i64,
    leases: &[ReportedLease],
) -> Heartbeat {
    heartbeat(
        &world.repo,
        &world.clock,
        Beat {
            runner_id: RUNNER_ID,
            heartbeat_id,
            sent_at,
            available_capacity,
            leases,
            lease_duration: Duration::seconds(60),
        },
    )
    .await
    .expect("heartbeat")
}

#[tokio::test]
async fn the_same_beat_again_returns_the_first_answer_and_other_content_conflicts() {
    let world = world().await;
    enqueue(&world, "replay-key").await;
    claim(
        &world.repo,
        &world.clock,
        RUNNER_ID,
        "claim-replay",
        Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");

    let sent_at = world.clock.now();
    let Heartbeat::Accepted(first) = beat(&world, sent_at, "hb-replay", 1, &[]).await else {
        panic!("the first beat is accepted");
    };
    world.clock.0.advance(Duration::seconds(5));
    let Heartbeat::Replayed(replay) = beat(&world, sent_at, "hb-replay", 1, &[]).await else {
        panic!("the same beat again is a replay");
    };
    assert_eq!(replay.accepted_at, first.accepted_at);

    let conflicting = beat(&world, sent_at, "hb-replay", 2, &[]).await;
    assert_eq!(conflicting, Heartbeat::IdempotencyConflict);
}

#[tokio::test]
async fn a_lease_with_the_wrong_fence_is_stale() {
    let world = world().await;
    enqueue(&world, "stale-key").await;
    let claimed = claim(
        &world.repo,
        &world.clock,
        RUNNER_ID,
        "claim-stale",
        Duration::seconds(60),
    )
    .await
    .expect("claim")
    .expect("work");
    let lease = ReportedLease {
        attempt_id: claimed.lease.attempt_id.clone(),
        fencing_token: claimed.lease.fencing_token + 1,
        state: ExecutionState::Leased,
        journal_state: "prepared".into(),
        last_event_checkpoint: None,
    };

    let outcome = beat(&world, world.clock.now(), "hb-stale", 1, &[lease]).await;

    assert_eq!(outcome, Heartbeat::StaleLease(claimed.lease.attempt_id));
}
