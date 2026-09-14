//! Concurrent-duplicate-writer race proofs: two racing callers reporting
//! the same idempotency-scoped write (completion, requeue, transition,
//! heartbeat, recovery, cancellation, enqueue, event batch) must leave
//! exactly one authoritative writer and one replay, never two writers or a
//! lost update. Grouped together because their shape is the pattern under
//! test, not the individual operation each races.

use crate::common::execution_fixture::{
    Fixture, cancellation_input, completion_input, count_where, recovery_input,
};

use chrono::Duration;
use sqlx::Row;
use tack_db::repo::execution::{
    AttemptTransitionInput, AttemptTransitionPhase, AttemptTransitionResult,
    CancellationObservation, CompletionResult, EnqueueResult, EventApplyResult, EventBatch,
    HeartbeatBatchResult, HeartbeatLease, NewEvent, OperatorRequeueResult, RecoveryDisposition,
    RecoveryObservation, RecoveryObservationResult,
};

// Concurrency regressions: an independent audit of every transaction in this
// module found the same deferred-reader-upgrade deadlock hazard (SQLITE_LOCKED
// "database is deadlocked") in three more functions, each hit by a plausible
// duplicate/retry caller (a runner or operator resending an unacknowledged
// request). The fencing-token/state WHERE clauses on their writes guard the
// *result*, not the SQLite lock-upgrade race, so each needed the same
// BEGIN IMMEDIATE fix as redeem_enrollment_token and
// claim_execution_idempotent_with_snapshot. These tests assert both
// concurrent branches succeed at the sqlx level (a raw Err is a hard
// failure) and that exactly one is authoritative.
#[tokio::test]
async fn concurrent_duplicate_completions_have_one_committed_writer() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-concurrent-complete", "attempt-concurrent-complete")
        .await;
    let completion = completion_input(
        "attempt-concurrent-complete",
        fence,
        "completion-concurrent",
        None,
    );
    let (a, b) = tokio::join!(
        fx.complete_result(completion.clone()),
        fx.complete_result(completion),
    );
    let a = a.expect("first completion report must succeed at the sqlx level");
    let b = b.expect("second completion report must succeed at the sqlx level");

    let committed = count_where([&a, &b], |r| matches!(r, CompletionResult::Committed(_)));
    let replayed = count_where([&a, &b], |r| matches!(r, CompletionResult::Replayed(_)));
    assert_eq!(
        committed, 1,
        "exactly one duplicate report commits: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate report replays: {a:?} / {b:?}"
    );

    // Capacity is restored by the terminal transition exactly once, even
    // though both branches raced to report the same completion.
    assert_eq!(fx.capacity().await, 1, "capacity is restored exactly once");
}

#[tokio::test]
async fn concurrent_duplicate_requeues_have_one_writer() {
    let fx = Fixture::new().await;
    fx.enqueue(
        "request-concurrent-requeue",
        "key-concurrent-requeue",
        "same",
    )
    .await;
    let lease = fx
        .claim("attempt-concurrent-requeue", Duration::seconds(60))
        .await
        .unwrap();
    let recovery = recovery_input(
        "attempt-concurrent-requeue",
        lease.fencing_token,
        "recovery-concurrent-requeue",
        RecoveryObservation::Ambiguous,
    );
    assert!(matches!(
        fx.recover(recovery).await,
        RecoveryObservationResult::Applied(_)
    ));
    let (a, b) = tokio::join!(
        fx.operator_requeue_result("request-concurrent-requeue"),
        fx.operator_requeue_result("request-concurrent-requeue"),
    );
    let a = a.expect("first operator requeue must succeed at the sqlx level");
    let b = b.expect("second operator requeue must succeed at the sqlx level");

    assert!(
        matches!(
            (&a, &b),
            (
                OperatorRequeueResult::Requeued,
                OperatorRequeueResult::Replayed
            ) | (
                OperatorRequeueResult::Replayed,
                OperatorRequeueResult::Requeued
            )
        ),
        "exactly one duplicate requeue is authoritative: {a:?} / {b:?}"
    );

    assert_eq!(
        fx.request_state("request-concurrent-requeue").await,
        "queued"
    );

    // One audit from recover_attempt, one from the winning requeue; the
    // replayed duplicate must not write a second requeue audit.
    assert_eq!(
        fx.count("execution_recovery_audits", "attempt-concurrent-requeue")
            .await,
        2
    );
}

#[tokio::test]
async fn concurrent_duplicate_transitions_have_one_applied_writer() {
    let fx = Fixture::new().await;
    fx.enqueue(
        "request-concurrent-transition",
        "key-concurrent-transition",
        "same",
    )
    .await;
    let lease = fx
        .claim("attempt-concurrent-transition", Duration::seconds(60))
        .await
        .unwrap();
    let preparing = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-concurrent-transition",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Preparing,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: None,
    };
    let (a, b) = tokio::join!(
        fx.transition_result(preparing.clone()),
        fx.transition_result(preparing),
    );
    let a = a.expect("first transition report must succeed at the sqlx level");
    let b = b.expect("second transition report must succeed at the sqlx level");

    let applied = count_where([&a, &b], |r| {
        matches!(r, AttemptTransitionResult::Applied(_))
    });
    let replayed = count_where([&a, &b], |r| {
        matches!(r, AttemptTransitionResult::Replayed(_))
    });
    assert_eq!(
        applied, 1,
        "exactly one duplicate report applies: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate report replays: {a:?} / {b:?}"
    );

    let committed_at = |r: &AttemptTransitionResult| match r {
        AttemptTransitionResult::Applied(resp) | AttemptTransitionResult::Replayed(resp) => {
            resp.committed_at.clone()
        }
        other => panic!("expected applied/replayed, got {other:?}"),
    };
    assert_eq!(
        committed_at(&a),
        committed_at(&b),
        "both branches observe the single committed timestamp"
    );

    let row = sqlx::query(
        "SELECT state,prepared_at FROM execution_attempts WHERE id='attempt-concurrent-transition'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "preparing");
    assert!(row.get::<Option<String>, _>("prepared_at").is_some());
}

#[tokio::test]
async fn concurrent_duplicate_heartbeats_have_one_writer() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-concurrent-heartbeat",
            "attempt-concurrent-heartbeat",
        )
        .await;
    let leases = [HeartbeatLease {
        state: "running",
        journal_state: "running",
        ..fx.lease("attempt-concurrent-heartbeat", fence)
    }];
    let sent_at = fx.clock.now();
    let (a, b) = tokio::join!(
        fx.heartbeat_result(
            "heartbeat-concurrent",
            sent_at,
            0,
            &leases,
            Duration::seconds(30)
        ),
        fx.heartbeat_result(
            "heartbeat-concurrent",
            sent_at,
            0,
            &leases,
            Duration::seconds(30)
        ),
    );
    let a = a.expect("first heartbeat report must succeed at the sqlx level");
    let b = b.expect("second heartbeat report must succeed at the sqlx level");

    let accepted = count_where([&a, &b], |r| matches!(r, HeartbeatBatchResult::Accepted(_)));
    let replayed = count_where([&a, &b], |r| matches!(r, HeartbeatBatchResult::Replayed(_)));
    assert_eq!(
        accepted, 1,
        "exactly one duplicate heartbeat accepts: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate heartbeat replays: {a:?} / {b:?}"
    );

    let response = |r: &HeartbeatBatchResult| match r {
        HeartbeatBatchResult::Accepted(resp) | HeartbeatBatchResult::Replayed(resp) => resp.clone(),
        other => panic!("expected accepted/replayed, got {other:?}"),
    };
    assert_eq!(
        response(&a),
        response(&b),
        "both branches observe the single committed heartbeat response"
    );

    // Capacity is set exactly once, even though both branches raced to
    // report the same duplicate heartbeat.
    assert_eq!(fx.capacity().await, 0);
    assert_eq!(
        fx.count_by(
            "execution_heartbeat_replays",
            "heartbeat_id",
            "heartbeat-concurrent"
        )
        .await,
        1,
        "exactly one durable replay record is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_recoveries_have_one_writer() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-concurrent-recovery", "attempt-concurrent-recovery")
        .await;
    let input = recovery_input(
        "attempt-concurrent-recovery",
        fence,
        "recovery-concurrent",
        RecoveryObservation::Ambiguous,
    );
    let (a, b) = tokio::join!(fx.recover_result(input.clone()), fx.recover_result(input));
    let a = a.expect("first recovery report must succeed at the sqlx level");
    let b = b.expect("second recovery report must succeed at the sqlx level");

    let applied = count_where([&a, &b], |r| {
        matches!(r, RecoveryObservationResult::Applied(_))
    });
    let replayed = count_where([&a, &b], |r| {
        matches!(r, RecoveryObservationResult::Replayed(_))
    });
    assert_eq!(
        applied, 1,
        "exactly one duplicate recovery report applies: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate recovery report replays: {a:?} / {b:?}"
    );

    let response = |r: &RecoveryObservationResult| match r {
        RecoveryObservationResult::Applied(resp) | RecoveryObservationResult::Replayed(resp) => {
            resp.clone()
        }
        other => panic!("expected applied/replayed, got {other:?}"),
    };
    let applied_response = response(&a);
    assert_eq!(
        applied_response,
        response(&b),
        "both branches observe the single committed recovery response"
    );
    assert_eq!(
        applied_response.disposition,
        RecoveryDisposition::NeedsOperator
    );

    assert_eq!(
        fx.attempt_state("attempt-concurrent-recovery").await,
        "needs_operator"
    );

    // Capacity is restored by the recovery transition exactly once, even
    // though both branches raced to report the same duplicate observation.
    assert_eq!(fx.capacity().await, 1, "capacity is restored exactly once");
    assert_eq!(
        fx.count("execution_recovery_audits", "attempt-concurrent-recovery")
            .await,
        1,
        "exactly one durable audit record is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_cancellations_have_one_writer() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-concurrent-cancel", "attempt-concurrent-cancel")
        .await;
    fx.request_cancellation("request-concurrent-cancel").await;
    let input = cancellation_input(
        "attempt-concurrent-cancel",
        fence,
        "cancel-concurrent",
        fx.clock.now(),
    );
    let (a, b) = tokio::join!(
        fx.observe_cancellation_result(input.clone()),
        fx.observe_cancellation_result(input),
    );
    let a = a.expect("first cancellation report must succeed at the sqlx level");
    let b = b.expect("second cancellation report must succeed at the sqlx level");

    let cancelled = count_where([&a, &b], |r| {
        matches!(r, CancellationObservation::Cancelled(_))
    });
    let replayed = count_where([&a, &b], |r| {
        matches!(r, CancellationObservation::Replayed(_))
    });
    assert_eq!(
        cancelled, 1,
        "exactly one duplicate cancellation report is authoritative: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate cancellation report replays: {a:?} / {b:?}"
    );

    let response = |r: &CancellationObservation| match r {
        CancellationObservation::Cancelled(resp) | CancellationObservation::Replayed(resp) => {
            resp.clone()
        }
        other => panic!("expected cancelled/replayed, got {other:?}"),
    };
    assert_eq!(
        response(&a),
        response(&b),
        "both branches observe the single committed cancellation response"
    );

    assert_eq!(
        fx.attempt_state("attempt-concurrent-cancel").await,
        "cancelled"
    );

    // Capacity is restored by the terminal transition exactly once, even
    // though both branches raced to report the same duplicate observation.
    assert_eq!(fx.capacity().await, 1, "capacity is restored exactly once");
    assert_eq!(
        fx.count(
            "execution_cancellation_replays",
            "attempt-concurrent-cancel"
        )
        .await,
        1,
        "exactly one durable replay record is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_enqueues_have_one_authoritative_writer() {
    let fx = Fixture::new().await;
    let (a, b) = tokio::join!(
        fx.enqueue_result(
            "request-concurrent-enqueue",
            "key-concurrent-enqueue",
            "same"
        ),
        fx.enqueue_result(
            "request-concurrent-enqueue",
            "key-concurrent-enqueue",
            "same"
        ),
    );
    let a = a.expect("first enqueue report must succeed at the sqlx level");
    let b = b.expect("second enqueue report must succeed at the sqlx level");

    let created = count_where([&a, &b], |r| matches!(r, EnqueueResult::Created(_)));
    let replayed = count_where([&a, &b], |r| matches!(r, EnqueueResult::Replayed(_)));
    assert_eq!(
        created, 1,
        "exactly one duplicate enqueue creates: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate enqueue replays: {a:?} / {b:?}"
    );

    let id = |r: &EnqueueResult| match r {
        EnqueueResult::Created(id) | EnqueueResult::Replayed(id) => id.clone(),
        other => panic!("expected created/replayed, got {other:?}"),
    };
    assert_eq!(
        id(&a),
        id(&b),
        "both branches observe the single committed request id"
    );
    assert_eq!(id(&a), "request-concurrent-enqueue");
    assert_eq!(
        fx.count_by("execution_requests", "id", "request-concurrent-enqueue")
            .await,
        1,
        "exactly one execution request row is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_event_batches_have_one_writer() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-concurrent-events", "attempt-concurrent-events")
        .await;
    let events = vec![NewEvent {
        id: "event-row-concurrent",
        event_id: "event-concurrent",
        sequence: 1,
        source: "runner",
        kind: "log",
        payload: r#"{"line":"hello"}"#,
        occurred_at: fx.clock.now(),
    }];
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-concurrent-events",
        fencing_token: fence,
        previous_checkpoint: None,
        checkpoint: "checkpoint-concurrent",
    };
    let (a, b) = tokio::join!(
        fx.apply_events_result(batch.clone(), &events),
        fx.apply_events_result(batch, &events),
    );
    let a = a.expect("first event batch report must succeed at the sqlx level");
    let b = b.expect("second event batch report must succeed at the sqlx level");

    let fresh = count_where(
        [&a, &b],
        |r| matches!(r, EventApplyResult::Applied(result) if !result.replayed),
    );
    let replayed = count_where(
        [&a, &b],
        |r| matches!(r, EventApplyResult::Applied(result) if result.replayed),
    );
    assert_eq!(
        fresh, 1,
        "exactly one duplicate event batch applies fresh: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate event batch replays: {a:?} / {b:?}"
    );

    let result = |r: &EventApplyResult| match r {
        EventApplyResult::Applied(result) => result.clone(),
        other => panic!("expected applied, got {other:?}"),
    };
    let a_result = result(&a);
    let b_result = result(&b);
    assert_eq!(a_result.accepted_event_ids, b_result.accepted_event_ids);
    assert_eq!(a_result.duplicate_event_ids, b_result.duplicate_event_ids);
    assert_eq!(a_result.committed_checkpoint, b_result.committed_checkpoint);
    assert_eq!(
        a_result.accepted_event_ids,
        vec!["event-concurrent".to_string()]
    );
    assert_eq!(
        fx.count("execution_events", "attempt-concurrent-events")
            .await,
        1,
        "the event is persisted exactly once"
    );

    let checkpoint: String = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id='attempt-concurrent-events'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(checkpoint, "checkpoint-concurrent");
    assert_eq!(
        fx.count("execution_event_batch_replays", "attempt-concurrent-events")
            .await,
        1,
        "exactly one durable replay record is written"
    );
}
