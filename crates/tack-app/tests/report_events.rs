//! The rule behind `POST /attempts/{attempt_id}/events`: a batch lands once per checkpoint, the
//! same batch again is a replay, other events under a used checkpoint are an idempotency
//! conflict, a batch that does not continue from the stream is a conflict, and a stale fence
//! writes nothing.

use crate::common;
use tack_protocol::execution::{AttemptId, Checkpoint, EventRecord, FencingToken, RunnerId};

use common::{RUNNER_ID, running_attempt, world};
use serde_json::{Value, json};
use tack_app::runs::report_events::{EventReport, EventsReported, report_events};
use tack_db::queries::{execution_attempts, execution_events};
use tack_protocol::Clock;

async fn report(
    world: &common::World,
    attempt_id: &str,
    fence: FencingToken,
    previous: Option<&str>,
    checkpoint: &str,
    event_id: &str,
    payload: Value,
) -> EventsReported {
    report_events(
        &world.repo,
        world.clock.now(),
        EventReport {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(attempt_id),
            fencing_token: fence,
            previous_checkpoint: previous.map(Checkpoint::new).as_ref(),
            checkpoint: &Checkpoint::new(checkpoint),
            events: &[EventRecord {
                event_id: event_id.into(),
                sequence: 1,
                occurred_at: world.clock.now(),
                source: "runner".into(),
                kind: "log".into(),
                payload,
            }],
        },
    )
    .await
    .expect("report")
}

#[tokio::test]
async fn a_batch_lands_once_and_the_same_batch_again_is_a_replay() {
    let world = world().await;
    let run = running_attempt(&world, "events-replay").await;

    let EventsReported::Applied(first) = report(
        &world,
        &run.attempt_id,
        run.fence,
        None,
        "cp-1",
        "evt-1",
        json!({"note": "original"}),
    )
    .await
    else {
        panic!("the first batch lands");
    };
    assert_eq!(first.accepted_event_ids, ["evt-1"]);
    assert!(!first.replayed);

    let EventsReported::Applied(replay) = report(
        &world,
        &run.attempt_id,
        run.fence,
        None,
        "cp-1",
        "evt-1",
        json!({"note": "original"}),
    )
    .await
    else {
        panic!("an identical resend is a replay");
    };
    assert!(replay.replayed);
    assert_eq!(replay.committed_checkpoint, Checkpoint::new("cp-1"));
    assert_eq!(
        execution_events::event_count_with_id(&world.repo, &run.attempt_id, "evt-1")
            .await
            .unwrap(),
        1,
        "a replayed batch does not duplicate its events"
    );
}

#[tokio::test]
async fn other_events_under_a_used_checkpoint_conflict_for_good_and_a_misplaced_batch_conflicts_to_retry()
 {
    let world = world().await;
    let run = running_attempt(&world, "events-conflict").await;
    report(
        &world,
        &run.attempt_id,
        run.fence,
        None,
        "cp-1",
        "evt-1",
        json!({"note": "original"}),
    )
    .await;

    let changed = report(
        &world,
        &run.attempt_id,
        run.fence,
        None,
        "cp-1",
        "evt-1",
        json!({"note": "CHANGED"}),
    )
    .await;
    assert_eq!(changed, EventsReported::IdempotencyConflict);

    // Claims to start from nothing although the stream is already at cp-1: a reordered delivery.
    let reordered = report(
        &world,
        &run.attempt_id,
        run.fence,
        None,
        "cp-2",
        "evt-reordered",
        json!({}),
    )
    .await;
    assert_eq!(reordered, EventsReported::Conflict);

    assert_eq!(
        execution_attempts::event_checkpoint(&world.repo, &run.attempt_id)
            .await
            .unwrap()
            .as_deref(),
        Some("cp-1"),
        "neither batch moved the checkpoint"
    );
    assert_eq!(
        execution_events::event_count_of_attempt(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_fence_that_is_not_current_writes_nothing() {
    let world = world().await;
    let run = running_attempt(&world, "events-stale").await;

    let stale = report(
        &world,
        &run.attempt_id,
        FencingToken::new(run.fence.get() + 1),
        None,
        "cp-1",
        "evt-1",
        json!({}),
    )
    .await;
    assert_eq!(stale, EventsReported::Stale);
    assert_eq!(
        execution_events::event_count_of_attempt(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        0
    );
}
