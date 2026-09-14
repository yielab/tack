//! Claiming an execution request into a leased attempt, and the heartbeat
//! protocol that keeps a lease alive.

use crate::common::execution_fixture::{Fixture, recovery_input, request};

use chrono::Duration;
use sqlx::Row;
use tack_db::repo::execution::{
    EventApplyResult, EventBatch, HeartbeatBatchResult, HeartbeatLease, NewEvent,
    RecoveryDisposition, RecoveryObservation, RecoveryObservationResult,
};

#[tokio::test]
async fn claim_request_replay_returns_the_original_lease() {
    let fx = Fixture::new().await;
    fx.enqueue("request-a", "key-a", "same").await;
    let first = fx
        .claim_raw("claim-a", "attempt-a", Duration::seconds(60))
        .await;
    let replay = fx
        .claim_raw("claim-a", "attempt-b", Duration::seconds(60))
        .await;
    assert_eq!(first, replay);
    assert!(matches!(replay, Some(claim) if claim.request_snapshot["request_id"] == "request-a"));
}

#[tokio::test]
async fn claim_second_claimer_blocked_stale_heartbeat_writes_nothing() {
    let fx = Fixture::new().await;
    fx.enqueue("request-a", "key-a", "same").await;
    let lease = fx
        .claim("attempt-a", Duration::seconds(60))
        .await
        .expect("first claim");
    assert_eq!(lease.fencing_token, 1);
    assert!(
        fx.claim("attempt-b", Duration::seconds(60)).await.is_none(),
        "capacity/request state prevent a second valid lease"
    );
    assert_eq!(
        fx.heartbeat("heartbeat-stale", &[fx.lease("attempt-a", 999)])
            .await,
        HeartbeatBatchResult::StaleLease("attempt-a".into()),
        "stale fence writes nothing"
    );
}

#[tokio::test]
async fn concurrent_claimers_receive_exactly_one_valid_lease() {
    let fx = Fixture::new().await;
    // Capacity must not be the reason the second claimer loses: both can enter
    // the scheduler, but the request's queued → leased compare-and-set wins once.
    sqlx::query("UPDATE agent_runners SET total_capacity = 2, available_capacity = 2")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    fx.enqueue("request-a", "key-a", "same").await;
    let (first, second) = tokio::join!(
        fx.claim_result("attempt-a", Duration::seconds(60)),
        fx.claim_result("attempt-b", Duration::seconds(60)),
    );
    // BEGIN IMMEDIATE now serializes claimants at the write lock instead of
    // letting two deferred readers race to upgrade. There is no legitimate
    // reason for either branch to fail at the sqlx level any more, so a raw
    // Err here (SQLITE_LOCKED "database is deadlocked" without BEGIN
    // IMMEDIATE, observed on ~100% of unfixed runs) is a hard test failure
    // -- no retry fallback, no swallowing.
    let first = first.expect("first claimant must succeed at the sqlx level");
    let second = second.expect("second claimant must succeed at the sqlx level");

    let leases: Vec<_> = [first.clone(), second.clone()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(
        leases.len(),
        1,
        "exactly one concurrent claimant receives a valid lease for one request"
    );
    let winner = leases.into_iter().next().unwrap();
    assert_eq!(winner.request_id, "request-a");
    assert_eq!(winner.attempt_number, 1);
    assert_eq!(
        winner.fencing_token, 1,
        "the sole winner takes the first fencing token"
    );

    // The losing branch is not a swallowed error and not a second lease: it
    // is the typed "no eligible work" outcome (None), because the winner
    // already consumed the queued → leased compare-and-set.
    assert!(
        matches!((&first, &second), (Some(_), None) | (None, Some(_))),
        "the losing claimant must observe a well-typed no-work result, not a second lease: {first:?} / {second:?}"
    );

    // Capacity end-state must be coherent: only the winner's reservation
    // survives. If the loser's capacity decrement (or its now-rolled-back
    // request CAS) leaked outside its transaction, this would read 0 instead
    // of 1 even though total_capacity started at 2.
    assert_eq!(
        fx.capacity().await,
        1,
        "only the winning claim consumes a capacity slot"
    );
    assert_eq!(fx.request_state("request-a").await, "leased");
    assert_eq!(
        fx.count_by("execution_attempts", "request_id", "request-a")
            .await,
        1,
        "exactly one attempt row exists for the request"
    );
}

#[tokio::test]
async fn concurrent_claimers_deadlock_fix_holds_under_load() {
    // Regression proof for the claim_execution_idempotent_with_snapshot
    // BEGIN IMMEDIATE fix: run the same concurrent-claim shape many times in
    // one process and demand zero sqlx-level errors and exactly one lease
    // per iteration. A single flaky iteration means the serialization
    // regressed back to the deferred-reader deadlock.
    for i in 0..25 {
        let fx = Fixture::new().await;
        sqlx::query("UPDATE agent_runners SET total_capacity = 2, available_capacity = 2")
            .execute(fx.repo.pool())
            .await
            .unwrap();
        fx.enqueue(&format!("request-{i}"), &format!("key-{i}"), "same")
            .await;
        let attempt_a = format!("attempt-{i}-a");
        let attempt_b = format!("attempt-{i}-b");
        let (first, second) = tokio::join!(
            fx.claim_result(&attempt_a, Duration::seconds(60)),
            fx.claim_result(&attempt_b, Duration::seconds(60)),
        );
        let first = first.unwrap_or_else(|e| panic!("iteration {i}: first claimant errored: {e}"));
        let second =
            second.unwrap_or_else(|e| panic!("iteration {i}: second claimant errored: {e}"));
        let leases = [first, second].into_iter().flatten().count();
        assert_eq!(leases, 1, "iteration {i}: expected exactly one lease");
    }
}

#[tokio::test]
async fn foreign_keys_and_expiry_recovery_fail_closed() {
    let fx = Fixture::new().await;
    let orphan = fx
        .repo
        .enqueue_execution(
            request("orphan", "missing-item", "orphan-key", "x"),
            &fx.clock,
        )
        .await;
    assert!(orphan.is_err(), "request item FK is enforced");
    fx.enqueue("request-a", "key-a", "same").await;
    let lease = fx.claim("attempt-a", Duration::seconds(5)).await.unwrap();
    fx.clock.advance(Duration::seconds(6));
    assert!(matches!(
        fx.recover(recovery_input(
            "attempt-a",
            lease.fencing_token,
            "recovery-expiry",
            RecoveryObservation::Ambiguous,
        ))
        .await,
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    assert_eq!(fx.attempt_state(&lease.attempt_id).await, "needs_operator");
}

#[tokio::test]
async fn queue_and_history_indexes_are_used() {
    let fx = Fixture::new().await;
    fx.enqueue("request-a", "key-a", "same").await;
    let queue_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT id FROM execution_requests WHERE state = 'queued' ORDER BY created_at")
        .fetch_all(fx.repo.pool()).await.unwrap();
    assert!(queue_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_requests_queue")
    }));
    let lease = fx.claim("attempt-a", Duration::seconds(60)).await.unwrap();
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
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-1",
    };
    assert!(matches!(
        fx.apply_events(batch, &[event]).await,
        EventApplyResult::Applied(_)
    ));
    let history_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT * FROM execution_events WHERE attempt_id = 'attempt-a' ORDER BY sequence")
        .fetch_all(fx.repo.pool()).await.unwrap();
    assert!(history_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_events_timeline")
    }));
}

#[tokio::test]
async fn heartbeat_replays_and_recovery_is_audited_once() {
    let fx = Fixture::new().await;
    fx.enqueue("request-a", "key-a", "same").await;
    let lease = fx.claim("attempt-a", Duration::seconds(60)).await.unwrap();
    let leases = [fx.lease("attempt-a", lease.fencing_token)];
    assert!(matches!(
        fx.heartbeat("hb-1", &leases).await,
        HeartbeatBatchResult::Accepted(_)
    ));
    assert!(matches!(
        fx.heartbeat("hb-1", &leases).await,
        HeartbeatBatchResult::Replayed(_)
    ));
    fx.clock.advance(Duration::seconds(61));
    let recovery = recovery_input(
        "attempt-a",
        lease.fencing_token,
        "recover-1",
        RecoveryObservation::ProcessStopped,
    );
    assert!(matches!(
        fx.repo
            .recover_attempt(recovery.clone(), &fx.clock)
            .await
            .unwrap(),
        RecoveryObservationResult::Applied(_)
    ));
    assert!(matches!(
        fx.repo.recover_attempt(recovery, &fx.clock).await.unwrap(),
        RecoveryObservationResult::Replayed(_)
    ));
    assert_eq!(fx.count("execution_recovery_audits", "attempt-a").await, 1);
}

#[tokio::test]
async fn heartbeat_rejects_false_free_capacity_during_active_lease() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-heartbeat-free", "attempt-heartbeat-free")
        .await;
    let leases = [fx.lease("attempt-heartbeat-free", fence)];
    assert_eq!(
        fx.heartbeat_full(
            "heartbeat-free",
            fx.clock.now(),
            1,
            &leases,
            Duration::seconds(60)
        )
        .await,
        HeartbeatBatchResult::Conflict
    );
    assert_eq!(fx.capacity().await, 0);
}

#[tokio::test]
async fn heartbeat_multi_lease_replay_is_canonical_and_authoritative() {
    let fx = Fixture::new().await;
    sqlx::query(
        "UPDATE agent_runners SET total_capacity = 2, available_capacity = 2 WHERE id = 'runner-a'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let first_fence = fx
        .ready_completion_attempt("request-heartbeat-multi-a", "attempt-heartbeat-multi-a")
        .await;
    let second_fence = fx
        .ready_completion_attempt("request-heartbeat-multi-b", "attempt-heartbeat-multi-b")
        .await;
    let leases = [
        fx.lease("attempt-heartbeat-multi-a", first_fence),
        fx.lease("attempt-heartbeat-multi-b", second_fence),
    ];
    let accepted = fx.heartbeat("heartbeat-multi", &leases).await;
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("multi-lease heartbeat must be accepted");
    };
    assert_eq!(accepted.leases.len(), 2);
    let reordered = [leases[1].clone(), leases[0].clone()];
    let replayed = fx.heartbeat("heartbeat-multi", &reordered).await;
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("canonical lease ordering must replay");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_exact_replay_after_clock_advance_returns_fields() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-heartbeat-time", "attempt-heartbeat-time")
        .await;
    let leases = [fx.lease("attempt-heartbeat-time", fence)];
    let sent_at = fx.clock.now();
    let accepted = fx
        .heartbeat_full("heartbeat-time", sent_at, 0, &leases, Duration::seconds(60))
        .await;
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("first heartbeat must be accepted");
    };
    fx.clock.advance(Duration::seconds(61));
    let replayed = fx
        .heartbeat_full("heartbeat-time", sent_at, 0, &leases, Duration::seconds(60))
        .await;
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("exact heartbeat retry must replay after lease expiry");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_frozen_request_mutations_conflict_without_write() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-heartbeat-conflict", "attempt-heartbeat-conflict")
        .await;
    let leases = [fx.lease("attempt-heartbeat-conflict", fence)];
    let sent_at = fx.clock.now();
    fx.heartbeat_full(
        "heartbeat-conflict",
        sent_at,
        0,
        &leases,
        Duration::seconds(60),
    )
    .await;
    let before: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    fx.clock.advance(Duration::seconds(1));
    assert_eq!(
        fx.heartbeat_full(
            "heartbeat-conflict",
            sent_at + Duration::seconds(1),
            0,
            &leases,
            Duration::seconds(60),
        )
        .await,
        HeartbeatBatchResult::Conflict
    );
    for mutated_leases in [
        [HeartbeatLease {
            state: "running",
            ..leases[0].clone()
        }],
        [HeartbeatLease {
            journal_state: "spawned",
            ..leases[0].clone()
        }],
        [HeartbeatLease {
            last_event_checkpoint: Some("checkpoint-mutated"),
            ..leases[0].clone()
        }],
    ] {
        assert_eq!(
            fx.heartbeat_full(
                "heartbeat-conflict",
                sent_at,
                0,
                &mutated_leases,
                Duration::seconds(60)
            )
            .await,
            HeartbeatBatchResult::Conflict
        );
    }
    let after: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn heartbeat_stale_lease_returns_typed_stale_result() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-heartbeat-stale", "attempt-heartbeat-stale")
        .await;
    fx.clock.advance(Duration::seconds(61));
    let leases = [fx.lease("attempt-heartbeat-stale", fence)];
    assert_eq!(
        fx.heartbeat("heartbeat-stale", &leases).await,
        HeartbeatBatchResult::StaleLease("attempt-heartbeat-stale".into())
    );
}

#[tokio::test]
async fn expired_unrecovered_attempt_blocks_capacity_and_claims() {
    let fx = Fixture::new().await;
    fx.ready_completion_attempt("request-heartbeat-expired", "attempt-heartbeat-expired")
        .await;
    fx.clock.advance(Duration::seconds(61));
    assert_eq!(
        fx.heartbeat_full(
            "heartbeat-expired",
            fx.clock.now(),
            1,
            &[],
            Duration::seconds(60)
        )
        .await,
        HeartbeatBatchResult::Conflict
    );
    assert_eq!(fx.capacity().await, 0);
    let mut blocked = fx.request("request-heartbeat-blocked", "key-heartbeat-blocked", "same");
    blocked.request_snapshot = Box::leak(
        blocked
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", &fx.clock.now().to_rfc3339())
            .into_boxed_str(),
    );
    fx.repo.enqueue_execution(blocked, &fx.clock).await.unwrap();
    assert!(
        fx.claim("attempt-heartbeat-blocked", Duration::seconds(60))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn heartbeat_replay_insert_failure_rolls_back_all_updates() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-heartbeat-rollback", "attempt-heartbeat-rollback")
        .await;
    sqlx::query("CREATE TRIGGER fail_heartbeat_replay BEFORE INSERT ON execution_heartbeat_replays BEGIN SELECT RAISE(ABORT, 'forced heartbeat replay failure'); END")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let leases = [fx.lease("attempt-heartbeat-rollback", fence)];
    let result = fx
        .repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-rollback",
            fx.clock.now(),
            0,
            &leases,
            Duration::seconds(60),
            &fx.clock,
        )
        .await;
    assert!(result.is_err());
    let last_heartbeat: Option<String> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM execution_attempts WHERE id = 'attempt-heartbeat-rollback'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    let runner_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(fx.repo.pool())
            .await
            .unwrap();
    assert!(last_heartbeat.is_none());
    assert!(runner_heartbeat.is_none());
    assert_eq!(
        fx.count_by(
            "execution_heartbeat_replays",
            "heartbeat_id",
            "heartbeat-rollback"
        )
        .await,
        0
    );
}
