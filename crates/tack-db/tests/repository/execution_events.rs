//! Structured event batch replay under a claimed attempt's fence.

use crate::common::execution_fixture::Fixture;

use chrono::Duration;
use tack_db::repo::execution::{EventApplyResult, EventBatch, NewEvent};

#[tokio::test]
async fn structured_events_replay_is_applied_once() {
    let fx = Fixture::new().await;
    fx.enqueue("request-z", "key-z", "same").await;
    let lease = fx.claim("attempt-z", Duration::seconds(60)).await.unwrap();
    let event = NewEvent {
        id: "row-z",
        event_id: "event-z",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: fx.clock.now(),
    };
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-z",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-z",
    };
    let first = fx
        .apply_events(batch.clone(), std::slice::from_ref(&event))
        .await;
    let EventApplyResult::Applied(first) = first else {
        panic!("first event batch must apply");
    };
    assert_eq!(first.accepted_event_ids, vec!["event-z"]);
    let replay = fx.apply_events(batch, &[event]).await;
    let EventApplyResult::Applied(replay) = replay else {
        panic!("matching event batch must replay");
    };
    assert_eq!(replay.accepted_event_ids, vec!["event-z"]);
    assert!(replay.replayed);
}

#[tokio::test]
async fn event_replay_canonicalizes_equivalent_json_payloads() {
    let fx = Fixture::new().await;
    fx.enqueue("request-canonical", "key-canonical", "same")
        .await;
    let lease = fx
        .claim("attempt-canonical", Duration::seconds(60))
        .await
        .unwrap();
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-canonical",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-canonical",
    };
    let initial = NewEvent {
        id: "row-canonical",
        event_id: "event-canonical",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: r#"{"outer":{"b":2,"a":1},"z":0}"#,
        occurred_at: fx.clock.now(),
    };
    let replay = NewEvent {
        id: "row-canonical-retry",
        payload: r#"{"z":0,"outer":{"a":1,"b":2}}"#,
        ..initial
    };
    assert!(matches!(
        fx.apply_events(batch.clone(), &[initial]).await,
        EventApplyResult::Applied(result) if !result.replayed
    ));
    assert!(matches!(
        fx.apply_events(batch, &[replay]).await,
        EventApplyResult::Applied(result) if result.replayed
    ));
}

// Defect 1 regression: a reused (attempt_id, checkpoint) idempotency-scoped
// key with different content must be the non-retryable `IdempotencyConflict`
// — never the benign, retryable `Conflict` a runner is expected to retry
// forever against a request that can never succeed. Paired with
// `event_replay_out_of_order_checkpoint_is_benign_conflict` below, which
// proves the genuinely benign case (a stale `previous_checkpoint`) stays
// `Conflict` and is not swept into this variant.
#[tokio::test]
async fn event_replay_changed_payload_is_idempotency_conflict() {
    let fx = Fixture::new().await;
    fx.enqueue("request-conflict", "key-conflict", "same").await;
    let lease = fx
        .claim("attempt-conflict", Duration::seconds(60))
        .await
        .unwrap();
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-conflict",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-conflict",
    };
    let initial = NewEvent {
        id: "row-conflict",
        event_id: "event-conflict",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: r#"{"state":"original"}"#,
        occurred_at: fx.clock.now(),
    };
    let changed = NewEvent {
        id: "row-conflict-retry",
        payload: r#"{"state":"changed"}"#,
        ..initial
    };
    fx.apply_events(batch.clone(), &[initial]).await;
    // Same checkpoint (idempotency-scoped key), different event payload:
    // fingerprint mismatch. Non-retryable.
    assert_eq!(
        fx.apply_events(batch, &[changed]).await,
        EventApplyResult::IdempotencyConflict
    );
    assert_eq!(fx.count("execution_events", "attempt-conflict").await, 1);
    let checkpoint: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-conflict'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(checkpoint.as_deref(), Some("checkpoint-conflict"));
}

// Contrast case for `event_replay_changed_payload_is_idempotency_conflict`
// above: a fresh checkpoint whose `previous_checkpoint` claim no longer
// matches the attempt's actual stream position is a benign out-of-order
// resync, not an idempotency-key reuse — retryable, and must stay `Conflict`
// rather than `IdempotencyConflict`.
#[tokio::test]
async fn event_replay_out_of_order_checkpoint_is_benign_conflict() {
    let fx = Fixture::new().await;
    fx.enqueue("request-conflict", "key-conflict", "same").await;
    let lease = fx
        .claim("attempt-conflict", Duration::seconds(60))
        .await
        .unwrap();
    let out_of_order = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-conflict",
        fencing_token: lease.fencing_token,
        previous_checkpoint: Some("not-the-current-checkpoint"),
        checkpoint: "checkpoint-conflict-2",
    };
    let next = NewEvent {
        id: "row-conflict-next",
        event_id: "event-conflict-next",
        sequence: 2,
        source: "runner",
        kind: "progress",
        payload: r#"{"state":"next"}"#,
        occurred_at: fx.clock.now(),
    };
    assert_eq!(
        fx.apply_events(out_of_order, &[next]).await,
        EventApplyResult::Conflict
    );
    assert_eq!(
        fx.count("execution_events", "attempt-conflict").await,
        0,
        "out-of-order batch must not write"
    );
}

#[tokio::test]
async fn event_replay_foreign_fence_is_stale_and_does_not_write() {
    let fx = Fixture::new().await;
    fx.enqueue("request-stale", "key-stale", "same").await;
    let lease = fx
        .claim("attempt-stale", Duration::seconds(60))
        .await
        .unwrap();
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-stale",
        fencing_token: lease.fencing_token + 1,
        previous_checkpoint: None,
        checkpoint: "checkpoint-stale",
    };
    let event = NewEvent {
        id: "row-stale",
        event_id: "event-stale",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: fx.clock.now(),
    };
    assert_eq!(
        fx.apply_events(batch, &[event]).await,
        EventApplyResult::Stale
    );
    assert_eq!(fx.count("execution_events", "attempt-stale").await, 0);
}

// Continues the fencing story from `claim_second_claimer_blocked_stale_
// heartbeat_writes_nothing` one layer up the protocol: once claimed
// (fencing token 1 on a fresh attempt), event replay is idempotent under
// that fence. Paired with `completion_replay_is_idempotent_and_
// terminal_lock_holds` below for the completion side of the same story.
#[tokio::test]
async fn event_replay_is_idempotent_under_one_fence() {
    let fx = Fixture::new().await;
    fx.enqueue("request-a", "key-a", "same").await;
    fx.claim("attempt-a", Duration::seconds(60)).await;
    let event = NewEvent {
        id: "row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: fx.clock.now(),
    };
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-a",
        fencing_token: 1,
        previous_checkpoint: None,
        checkpoint: "checkpoint-1",
    };
    assert!(matches!(
        fx.apply_events(batch.clone(), std::slice::from_ref(&event)).await,
        EventApplyResult::Applied(result) if !result.replayed
    ));
    assert!(
        matches!(
            fx.apply_events(batch, &[event]).await,
            EventApplyResult::Applied(result) if result.replayed
        ),
        "same checkpoint is a replay"
    );
    assert_eq!(fx.count("execution_events", "attempt-a").await, 1);
}
