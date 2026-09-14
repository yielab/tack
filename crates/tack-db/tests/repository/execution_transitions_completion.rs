//! Attempt lifecycle transitions (preparing/running) and completion
//! reporting, including the idempotency/terminal-lock invariants they share.

use crate::common::execution_fixture::{Fixture, completion_input, count_where};

use chrono::Duration;
use sqlx::Row;
use tack_db::repo::execution::{
    AttemptTransitionInput, AttemptTransitionPhase, AttemptTransitionResult, Completion,
    CompletionResult, EventBatch, NewEvent,
};

#[tokio::test]
async fn attempt_start_transitions_are_idempotent_and_freeze_facts() {
    let fx = Fixture::new().await;
    fx.enqueue("request-start", "key-start", "same").await;
    let lease = fx
        .claim("attempt-start", Duration::seconds(60))
        .await
        .unwrap();

    let preparing = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-start",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Preparing,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: None,
    };
    let prepared_at = match fx.transition(preparing.clone()).await {
        AttemptTransitionResult::Applied(response) => response.committed_at,
        result => panic!("expected preparation to apply, got {result:?}"),
    };
    fx.clock.advance(Duration::seconds(1));
    assert!(matches!(
        fx.transition(preparing.clone()).await,
        AttemptTransitionResult::Replayed(response) if response.committed_at == prepared_at
    ));
    assert_eq!(
        fx.transition(AttemptTransitionInput {
            workspace_id: "workspace-changed",
            ..preparing
        })
        .await,
        AttemptTransitionResult::Conflict
    );

    let running = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-start",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Running,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: Some("process-1"),
    };
    let started_at = match fx.transition(running.clone()).await {
        AttemptTransitionResult::Applied(response) => response.committed_at,
        result => panic!("expected start to apply, got {result:?}"),
    };
    fx.clock.advance(Duration::seconds(1));
    assert!(matches!(
        fx.transition(AttemptTransitionInput {
            phase: AttemptTransitionPhase::Preparing,
            process_id: None,
            ..running.clone()
        })
        .await,
        AttemptTransitionResult::Replayed(response)
            if response.state == "preparing" && response.committed_at == prepared_at
    ));
    assert!(matches!(
        fx.transition(running.clone()).await,
        AttemptTransitionResult::Replayed(response) if response.committed_at == started_at
    ));
    assert_eq!(
        fx.transition(AttemptTransitionInput {
            process_id: Some("process-changed"),
            ..running
        })
        .await,
        AttemptTransitionResult::Conflict
    );

    let row = sqlx::query(
        "SELECT state,workspace_id,base_revision,process_id,prepared_at,started_at FROM execution_attempts WHERE id='attempt-start'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "running");
    assert_eq!(row.get::<String, _>("workspace_id"), "workspace-1");
    assert_eq!(row.get::<String, _>("base_revision"), "abc123");
    assert_eq!(row.get::<String, _>("process_id"), "process-1");
    assert_eq!(row.get::<String, _>("prepared_at"), prepared_at);
    assert_eq!(row.get::<String, _>("started_at"), started_at);
}

#[tokio::test]
async fn attempt_start_rejects_wrong_order_and_stale_authority() {
    let fx = Fixture::new().await;
    fx.enqueue("request-order", "key-order", "same").await;
    let lease = fx
        .claim("attempt-order", Duration::seconds(5))
        .await
        .unwrap();

    let running = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-order",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Running,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: Some("process-1"),
    };
    assert_eq!(
        fx.transition(running.clone()).await,
        AttemptTransitionResult::Conflict
    );
    assert_eq!(
        fx.transition(AttemptTransitionInput {
            fencing_token: lease.fencing_token + 1,
            ..running.clone()
        })
        .await,
        AttemptTransitionResult::Stale
    );
    fx.clock.advance(Duration::seconds(6));
    assert_eq!(
        fx.transition(AttemptTransitionInput {
            phase: AttemptTransitionPhase::Preparing,
            process_id: None,
            ..running
        })
        .await,
        AttemptTransitionResult::Stale
    );

    let row = sqlx::query(
        "SELECT state,workspace_id,base_revision,process_id,prepared_at,started_at FROM execution_attempts WHERE id='attempt-order'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert!(row.get::<Option<String>, _>("workspace_id").is_none());
    assert!(row.get::<Option<String>, _>("base_revision").is_none());
    assert!(row.get::<Option<String>, _>("process_id").is_none());
    assert!(row.get::<Option<String>, _>("prepared_at").is_none());
    assert!(row.get::<Option<String>, _>("started_at").is_none());
}

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
async fn completion_response_loss_replays_authoritative_response() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-loss", "attempt-completion-loss")
        .await;
    let completion = completion_input("attempt-completion-loss", fence, "completion-loss", None);
    let committed = fx.complete(completion.clone()).await;
    let CompletionResult::Committed(committed) = committed else {
        panic!("first completion must commit");
    };
    let replayed = fx.complete(completion).await;
    let CompletionResult::Replayed(replayed) = replayed else {
        panic!("lost response retry must replay");
    };
    assert_eq!(replayed, committed);
}

#[tokio::test]
async fn completion_replay_foreign_fence_is_stale() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-stale", "attempt-completion-stale")
        .await;
    let completion = completion_input("attempt-completion-stale", fence, "completion-stale", None);
    fx.complete(completion.clone()).await;
    let foreign = Completion {
        fencing_token: fence + 1,
        ..completion
    };
    assert_eq!(fx.complete(foreign).await, CompletionResult::Stale);
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_json() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-completion-canonical",
            "attempt-completion-canonical",
        )
        .await;
    let completion = completion_input(
        "attempt-completion-canonical",
        fence,
        "completion-canonical",
        None,
    );
    fx.complete(completion.clone()).await;
    let canonical_retry = Completion {
        actual_execution: r#"{"result":{"a":1,"b":2}}"#,
        usage: r#"{"input_tokens":1,"output_tokens":2}"#,
        ..completion
    };
    assert!(matches!(
        fx.complete(canonical_retry).await,
        CompletionResult::Replayed(_)
    ));
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_terminal_reason() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-reason", "attempt-completion-reason")
        .await;
    let completion = Completion {
        terminal_reason: r#"{"detail":{"b":2,"a":1},"code":"done"}"#,
        ..completion_input(
            "attempt-completion-reason",
            fence,
            "completion-reason",
            None,
        )
    };
    fx.complete(completion.clone()).await;
    let semantic_retry = Completion {
        terminal_reason: r#"{"code":"done","detail":{"a":1,"b":2}}"#,
        ..completion
    };
    assert!(matches!(
        fx.complete(semantic_retry).await,
        CompletionResult::Replayed(_)
    ));
}

// Defect 1 regression: distinguishes the benign, retryable `Conflict` (a lost
// optimistic-concurrency compare-and-set, before any replay row exists) from
// the non-retryable `IdempotencyConflict` (the same completion_id — an
// idempotency-scoped key — reused with different content once a replay row
// exists). Collapsing these into one variant is exactly how a runner ends up
// told to retry a request that can never succeed.
#[tokio::test]
async fn completion_conflict_before_replay_row_is_benign() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-conflict", "attempt-completion-conflict")
        .await;
    let base = completion_input(
        "attempt-completion-conflict",
        fence,
        "completion-conflict",
        None,
    );
    // No replay row exists yet: this is a lost compare-and-set against the
    // live attempt row (final_event_checkpoint doesn't match), not a reused
    // idempotency key. Benign/retryable.
    let wrong_checkpoint = Completion {
        final_event_checkpoint: Some("not-current"),
        ..base
    };
    assert_eq!(
        fx.complete(wrong_checkpoint).await,
        CompletionResult::Conflict
    );
    assert_eq!(
        fx.attempt_state("attempt-completion-conflict").await,
        "leased"
    );
}

// Paired with `completion_conflict_before_replay_row_is_benign` above: once a
// replay row exists, the same reused completion_id (idempotency-scoped key)
// with different content is the non-retryable `IdempotencyConflict` instead.
#[tokio::test]
async fn completion_changed_after_replay_is_idempotency_conflict() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-conflict", "attempt-completion-conflict")
        .await;
    let base = completion_input(
        "attempt-completion-conflict",
        fence,
        "completion-conflict",
        None,
    );
    fx.complete(base.clone()).await;
    let changed_fields = Completion {
        terminal_reason: "changed",
        ..base.clone()
    };
    let changed_checkpoint = Completion {
        final_event_checkpoint: Some("changed"),
        ..base
    };
    assert_eq!(
        fx.complete(changed_fields).await,
        CompletionResult::IdempotencyConflict
    );
    assert_eq!(
        fx.complete(changed_checkpoint).await,
        CompletionResult::IdempotencyConflict
    );
    let completion_id: String = sqlx::query_scalar(
        "SELECT completion_id FROM execution_attempts WHERE id = 'attempt-completion-conflict'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(completion_id, "completion-conflict");
}

#[tokio::test]
async fn completion_replay_corrupt_response_fails_closed() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-corrupt", "attempt-completion-corrupt")
        .await;
    let completion = completion_input(
        "attempt-completion-corrupt",
        fence,
        "completion-corrupt",
        None,
    );
    fx.complete(completion.clone()).await;
    sqlx::query("UPDATE execution_completion_replays SET response = '{}' WHERE attempt_id = 'attempt-completion-corrupt'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    assert!(
        fx.repo
            .complete_execution_result(completion, &fx.clock)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn completion_replay_restores_capacity_once_and_never_above_cap() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-capacity", "attempt-completion-capacity")
        .await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let completion = completion_input(
        "attempt-completion-capacity",
        fence,
        "completion-capacity",
        None,
    );
    fx.complete(completion.clone()).await;
    fx.complete(completion).await;
    assert_eq!(fx.capacity().await, 1);

    let second_fence = fx
        .ready_completion_attempt("request-completion-once", "attempt-completion-once")
        .await;
    let second = completion_input(
        "attempt-completion-once",
        second_fence,
        "completion-once",
        None,
    );
    fx.complete(second.clone()).await;
    let after_commit = fx.capacity().await;
    fx.complete(second).await;
    let after_replay = fx.capacity().await;
    assert_eq!(after_commit, 1);
    assert_eq!(after_replay, 1);
}

#[tokio::test]
async fn completion_replay_insert_failure_rolls_back_terminal() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-completion-rollback", "attempt-completion-rollback")
        .await;
    sqlx::query("CREATE TRIGGER fail_completion_replay BEFORE INSERT ON execution_completion_replays BEGIN SELECT RAISE(ABORT, 'forced completion replay failure'); END")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let completion = completion_input(
        "attempt-completion-rollback",
        fence,
        "completion-rollback",
        None,
    );
    let error = fx
        .repo
        .complete_execution_result(completion, &fx.clock)
        .await;
    assert!(error.is_err());
    assert_eq!(
        fx.attempt_state("attempt-completion-rollback").await,
        "leased"
    );
    assert_eq!(
        fx.request_state("request-completion-rollback").await,
        "leased"
    );
    assert_eq!(fx.capacity().await, 0);
}

#[tokio::test]
async fn completion_replay_is_idempotent_and_terminal_lock_holds() {
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
    fx.apply_events(batch, &[event]).await;
    let base = Completion {
        runner_id: "runner-a",
        attempt_id: "attempt-a",
        fencing_token: 1,
        completion_id: "complete-1",
        final_event_checkpoint: Some("checkpoint-1"),
        terminal_state: "succeeded",
        terminal_reason: "completed",
        actual_execution: "{}",
        usage: "{}",
    };
    assert!(fx.complete_raw(base.clone()).await);
    assert!(
        fx.complete_raw(base.clone()).await,
        "same completion is replay-safe"
    );
    let reopen = Completion {
        completion_id: "complete-2",
        terminal_state: "failed",
        terminal_reason: "changed",
        ..base
    };
    assert!(
        !fx.complete_raw(reopen).await,
        "terminal attempt cannot reopen"
    );
}
