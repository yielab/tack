//! Recovery observations and the operator-requeue path they can unblock.

use crate::common::execution_fixture::{Fixture, recovery_input};

use chrono::Duration;
use tack_db::repo::execution::{
    EnrollmentToken, NewRunner, OperatorRequeueResult, RecoveryDisposition, RecoveryObservation,
    RecoveryObservationInput, RecoveryObservationResult,
};

#[tokio::test]
async fn recovery_stopped_without_start_requeues_fence_and_replays() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-safe", "attempt-recovery-safe")
        .await;
    let input = recovery_input(
        "attempt-recovery-safe",
        fence,
        "recovery-safe",
        RecoveryObservation::ProcessStopped,
    );
    let applied = fx.recover(input.clone()).await;
    let RecoveryObservationResult::Applied(applied) = applied else {
        panic!("current-fence stopped process with no start must requeue");
    };
    assert_eq!(
        applied.disposition,
        RecoveryDisposition::SafePreSpawnRequeue
    );
    let semantic_retry = RecoveryObservationInput {
        details: r#"{"observer":{"a":1,"b":2},"process_observed":false,"journal_state":"prepared"}"#,
        ..input.clone()
    };
    let replayed = fx.recover(semantic_retry).await;
    let RecoveryObservationResult::Replayed(replayed) = replayed else {
        panic!("canonical recovery retry must replay");
    };
    assert_eq!(replayed, applied);
    let changed = RecoveryObservationInput {
        details: r#"{"journal_state":"prepared","process_observed":true,"observer":"changed"}"#,
        ..input
    };
    assert_eq!(
        fx.recover(changed).await,
        RecoveryObservationResult::Conflict
    );
    assert_eq!(fx.attempt_state("attempt-recovery-safe").await, "lost");
    assert_eq!(fx.request_state("request-recovery-safe").await, "queued");
}

#[tokio::test]
async fn recovery_running_ambiguous_and_spawn_evidence_need_operator() {
    let fx = Fixture::new().await;
    sqlx::query(
        "UPDATE agent_runners SET total_capacity = 4, available_capacity = 4 WHERE id = 'runner-a'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let stopped_fence = fx
        .ready_completion_attempt("request-recovery-started", "attempt-recovery-started")
        .await;
    let running_fence = fx
        .ready_completion_attempt("request-recovery-running", "attempt-recovery-running")
        .await;
    let ambiguous_fence = fx
        .ready_completion_attempt("request-recovery-ambiguous", "attempt-recovery-ambiguous")
        .await;
    let post_spawn_fence = fx
        .ready_completion_attempt("request-recovery-post-spawn", "attempt-recovery-post-spawn")
        .await;
    sqlx::query(
        "UPDATE execution_attempts SET started_at = ? WHERE id = 'attempt-recovery-started'",
    )
    .bind(fx.clock.now().to_rfc3339())
    .execute(fx.repo.pool())
    .await
    .unwrap();
    for (attempt_id, fence, key, observation) in [
        (
            "attempt-recovery-started",
            stopped_fence,
            "recovery-started",
            RecoveryObservation::ProcessStopped,
        ),
        (
            "attempt-recovery-running",
            running_fence,
            "recovery-running",
            RecoveryObservation::ProcessRunning,
        ),
        (
            "attempt-recovery-ambiguous",
            ambiguous_fence,
            "recovery-ambiguous",
            RecoveryObservation::Ambiguous,
        ),
    ] {
        assert!(matches!(
            fx.recover(recovery_input(attempt_id, fence, key, observation)).await,
            RecoveryObservationResult::Applied(response)
                if response.disposition == RecoveryDisposition::NeedsOperator
        ));
    }
    let post_spawn = RecoveryObservationInput {
        details: r#"{"journal_state":"spawned","process_observed":false}"#,
        ..recovery_input(
            "attempt-recovery-post-spawn",
            post_spawn_fence,
            "recovery-post-spawn",
            RecoveryObservation::ProcessStopped,
        )
    };
    assert!(matches!(
        fx.recover(post_spawn).await,
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    assert_eq!(
        fx.count_by("execution_attempts", "state", "needs_operator")
            .await,
        4
    );
}

#[tokio::test]
async fn recovery_foreign_fence_is_stale() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-stale", "attempt-recovery-stale")
        .await;
    assert_eq!(
        fx.recover(recovery_input(
            "attempt-recovery-stale",
            fence + 1,
            "recovery-foreign",
            RecoveryObservation::Ambiguous,
        ))
        .await,
        RecoveryObservationResult::Stale
    );
}

#[tokio::test]
async fn recovery_terminal_replay_is_idempotent_and_rejects_changes() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-stale", "attempt-recovery-stale")
        .await;
    sqlx::query(
        "UPDATE execution_attempts SET state = 'succeeded' WHERE id = 'attempt-recovery-stale'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let terminal = recovery_input(
        "attempt-recovery-stale",
        fence,
        "recovery-terminal",
        RecoveryObservation::Ambiguous,
    );
    assert!(matches!(
        fx.recover(terminal.clone()).await,
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::AlreadyTerminal
    ));
    assert!(matches!(
        fx.recover(terminal.clone()).await,
        RecoveryObservationResult::Replayed(response)
            if response.disposition == RecoveryDisposition::AlreadyTerminal
    ));
    let changed_terminal = RecoveryObservationInput {
        details: r#"{"journal_state":"prepared","process_observed":true}"#,
        ..terminal
    };
    assert_eq!(
        fx.recover(changed_terminal).await,
        RecoveryObservationResult::Conflict
    );
}

#[tokio::test]
async fn recovery_after_runner_revoked_is_stale() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-stale", "attempt-recovery-stale")
        .await;
    fx.repo.revoke_runner("runner-a", &fx.clock).await.unwrap();
    assert_eq!(
        fx.recover(recovery_input(
            "attempt-recovery-stale",
            fence,
            "recovery-revoked",
            RecoveryObservation::Ambiguous,
        ))
        .await,
        RecoveryObservationResult::Stale
    );
}

#[tokio::test]
async fn pending_runner_token_issue_fails_on_mismatched_capacity() {
    let fx = Fixture::new().await;
    let bad = fx
        .repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: "pending",
                name: "pending",
                credential_hash: "ignored",
                labels: "{}",
                total_capacity: 1,
                available_capacity: 2,
                capability_snapshot: "{}",
                protocol_version: 1,
            },
            EnrollmentToken {
                id: "token",
                runner_id: "other",
                token_hash: "hash",
                expires_at: fx.clock.now(),
            },
            &fx.clock,
        )
        .await;
    assert!(
        bad.is_err(),
        "mismatched/expired/invalid capacity token issue fails before writes"
    );
}

#[tokio::test]
async fn operator_requeue_clears_cancellation_and_is_idempotent() {
    let fx = Fixture::new().await;
    fx.enqueue("request-r", "key-r", "same").await;
    let lease = fx.claim("attempt-r", Duration::seconds(60)).await.unwrap();
    let recovery = recovery_input(
        "attempt-r",
        lease.fencing_token,
        "recovery-r",
        RecoveryObservation::Ambiguous,
    );
    assert!(matches!(
        fx.recover(recovery).await,
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    sqlx::query("UPDATE execution_requests SET cancellation_requested_at='x' WHERE id='request-r'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let capacity = fx.capacity().await;
    assert_eq!(
        fx.operator_requeue("request-r").await,
        OperatorRequeueResult::Requeued
    );
    assert_eq!(
        fx.operator_requeue("request-r").await,
        OperatorRequeueResult::Replayed
    );
    let cleared: Option<String> = sqlx::query_scalar(
        "SELECT cancellation_requested_at FROM execution_requests WHERE id='request-r'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert!(cleared.is_none());
    assert_eq!(fx.count("execution_recovery_audits", "attempt-r").await, 2);
    assert_eq!(fx.capacity().await, capacity);
}

#[tokio::test]
async fn operator_requeue_rejects_without_authoritative_recovery() {
    let fx = Fixture::new().await;
    fx.enqueue("request-r", "key-r", "same").await;
    fx.claim("attempt-r", Duration::seconds(60)).await.unwrap();
    sqlx::query("UPDATE execution_attempts SET state='needs_operator' WHERE id='attempt-r'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET state='needs_operator' WHERE id='request-r'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(
        fx.operator_requeue("request-r").await,
        OperatorRequeueResult::InvalidTransition
    );
    assert_eq!(fx.request_state("request-r").await, "needs_operator");
    assert_eq!(fx.capacity().await, 0);
}
