//! Cancellation observations, and artifacts/decisions recorded against an
//! attempt (including the lease-expiry and concurrent-terminal races).

use crate::common::execution_fixture::{
    Fixture, cancellation_input, completion_input, expect_cancelled, expect_replayed_cancellation,
    recovery_input,
};

use chrono::Duration;
use tack_db::repo::execution::{
    CancellationObservation, CancellationObservationInput, RecoveryObservation,
};

#[tokio::test]
async fn cancellation_observation_replays_after_first_success() {
    let fx = Fixture::new().await;
    fx.enqueue("request-z", "key-z", "same").await;
    let lease = fx.claim("attempt-z", Duration::seconds(60)).await.unwrap();
    assert!(fx.request_cancellation("request-z").await);
    let cancel = cancellation_input("attempt-z", lease.fencing_token, "cancel-z", fx.clock.now());
    assert!(matches!(
        fx.observe_cancellation(cancel.clone()).await,
        CancellationObservation::Cancelled(_)
    ));
    assert!(matches!(
        fx.observe_cancellation(cancel).await,
        CancellationObservation::Replayed(_)
    ));
}

#[tokio::test]
async fn cancellation_response_loss_replays_after_time_advance() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-cancellation-loss", "attempt-cancellation-loss")
        .await;
    assert!(fx.request_cancellation("request-cancellation-loss").await);
    let input = cancellation_input(
        "attempt-cancellation-loss",
        fence,
        "cancellation-loss",
        fx.clock.now(),
    );
    let first = expect_cancelled(fx.observe_cancellation(input.clone()).await);
    fx.clock.advance(Duration::seconds(61));
    let semantic_retry = CancellationObservationInput {
        details: r#"{"detail":{"a":1,"b":2}}"#,
        observation: r#""process_stopped""#,
        ..input
    };
    let replay = fx.observe_cancellation(semantic_retry).await;
    assert_eq!(expect_replayed_cancellation(replay), first);
}

#[tokio::test]
async fn cancellation_changed_same_id_conflicts_without_write() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-cancellation-conflict",
            "attempt-cancellation-conflict",
        )
        .await;
    fx.request_cancellation("request-cancellation-conflict")
        .await;
    let input = cancellation_input(
        "attempt-cancellation-conflict",
        fence,
        "cancellation-conflict",
        fx.clock.now(),
    );
    fx.observe_cancellation(input.clone()).await;
    let changed = CancellationObservationInput {
        details: r#"{"detail":"changed"}"#,
        ..input
    };
    assert_eq!(
        fx.observe_cancellation(changed).await,
        CancellationObservation::Conflict
    );
    let completion_id: String = sqlx::query_scalar(
        "SELECT completion_id FROM execution_attempts WHERE id = 'attempt-cancellation-conflict'",
    )
    .fetch_one(fx.repo.pool())
    .await
    .unwrap();
    assert_eq!(completion_id, "cancellation-conflict");
    assert_eq!(fx.capacity().await, 1);
}

#[tokio::test]
async fn cancellation_foreign_fence_is_stale_before_replay_lookup() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-cancellation-stale", "attempt-cancellation-stale")
        .await;
    fx.request_cancellation("request-cancellation-stale").await;
    let input = cancellation_input(
        "attempt-cancellation-stale",
        fence,
        "cancellation-stale",
        fx.clock.now(),
    );
    fx.observe_cancellation(input.clone()).await;
    let foreign = CancellationObservationInput {
        fencing_token: fence + 1,
        ..input
    };
    assert_eq!(
        fx.observe_cancellation(foreign).await,
        CancellationObservation::Stale
    );
}

#[tokio::test]
async fn cancellation_corrupt_replay_response_fails_closed() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-cancellation-corrupt",
            "attempt-cancellation-corrupt",
        )
        .await;
    fx.request_cancellation("request-cancellation-corrupt")
        .await;
    let input = cancellation_input(
        "attempt-cancellation-corrupt",
        fence,
        "cancellation-corrupt",
        fx.clock.now(),
    );
    fx.observe_cancellation(input.clone()).await;
    sqlx::query("UPDATE execution_cancellation_replays SET response = '{}' WHERE attempt_id = 'attempt-cancellation-corrupt'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    assert!(
        fx.repo
            .observe_cancellation(input, &fx.clock)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_requires_exact_process_stopped_observation() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-cancellation-observation",
            "attempt-cancellation-observation",
        )
        .await;
    fx.request_cancellation("request-cancellation-observation")
        .await;
    let invalid = CancellationObservationInput {
        observation: r#""process_running""#,
        ..cancellation_input(
            "attempt-cancellation-observation",
            fence,
            "cancellation-observation",
            fx.clock.now(),
        )
    };
    assert!(
        fx.repo
            .observe_cancellation(invalid, &fx.clock)
            .await
            .is_err()
    );
    assert_eq!(
        fx.attempt_state("attempt-cancellation-observation").await,
        "leased"
    );
    assert_eq!(
        fx.count(
            "execution_cancellation_replays",
            "attempt-cancellation-observation"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn cancellation_capacity_restore_is_capped_and_replay_safe() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-cancellation-cap", "attempt-cancellation-cap")
        .await;
    fx.request_cancellation("request-cancellation-cap").await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-cap",
        fence,
        "cancellation-cap",
        fx.clock.now(),
    );
    fx.observe_cancellation(input.clone()).await;
    fx.observe_cancellation(input).await;
    assert_eq!(fx.capacity().await, 1);
}

#[tokio::test]
async fn recovery_capacity_release_is_capped_and_replay_safe() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-cap", "attempt-recovery-cap")
        .await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(fx.repo.pool())
    .await
    .unwrap();
    let input = recovery_input(
        "attempt-recovery-cap",
        fence,
        "recovery-cap",
        RecoveryObservation::Ambiguous,
    );
    fx.recover(input.clone()).await;
    fx.recover(input).await;
    assert_eq!(fx.capacity().await, 1);
}

#[tokio::test]
async fn cancellation_replay_insert_failure_rolls_back_terminal() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-cancellation-rollback",
            "attempt-cancellation-rollback",
        )
        .await;
    fx.request_cancellation("request-cancellation-rollback")
        .await;
    sqlx::query("CREATE TRIGGER fail_cancellation_replay BEFORE INSERT ON execution_cancellation_replays BEGIN SELECT RAISE(ABORT, 'forced cancellation replay failure'); END")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-rollback",
        fence,
        "cancellation-rollback",
        fx.clock.now(),
    );
    assert!(
        fx.repo
            .observe_cancellation(input, &fx.clock)
            .await
            .is_err()
    );
    assert_eq!(
        fx.attempt_state("attempt-cancellation-rollback").await,
        "leased"
    );
    assert_eq!(
        fx.request_state("request-cancellation-rollback").await,
        "leased"
    );
    assert_eq!(fx.capacity().await, 0);
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
async fn recovery_replay_insert_failure_rolls_back_lifecycle() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt("request-recovery-rollback", "attempt-recovery-rollback")
        .await;
    sqlx::query("CREATE TRIGGER fail_recovery_replay BEFORE INSERT ON execution_recovery_audits BEGIN SELECT RAISE(ABORT, 'forced recovery replay failure'); END")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let input = recovery_input(
        "attempt-recovery-rollback",
        fence,
        "recovery-rollback",
        RecoveryObservation::Ambiguous,
    );
    assert!(fx.repo.recover_attempt(input, &fx.clock).await.is_err());
    assert_eq!(
        fx.attempt_state("attempt-recovery-rollback").await,
        "leased"
    );
    assert_eq!(
        fx.request_state("request-recovery-rollback").await,
        "leased"
    );
    assert_eq!(fx.capacity().await, 0);
}

#[tokio::test]
async fn cancellation_on_terminal_or_missing_request_is_not_success() {
    let fx = Fixture::new().await;
    let fence = fx
        .ready_completion_attempt(
            "request-cancellation-outcomes",
            "attempt-cancellation-outcomes",
        )
        .await;
    let ambiguous_input = cancellation_input(
        "attempt-cancellation-outcomes",
        fence,
        "cancellation-ambiguous",
        fx.clock.now(),
    );
    assert_eq!(
        fx.observe_cancellation(ambiguous_input).await,
        CancellationObservation::Ambiguous {
            state: "leased".into()
        }
    );
    sqlx::query("UPDATE execution_attempts SET state = 'succeeded' WHERE id = 'attempt-cancellation-outcomes'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    let terminal_input = cancellation_input(
        "attempt-cancellation-outcomes",
        fence,
        "cancellation-terminal",
        fx.clock.now(),
    );
    assert_eq!(
        fx.observe_cancellation(terminal_input).await,
        CancellationObservation::AlreadyTerminal {
            state: "succeeded".into()
        }
    );
}

#[tokio::test]
async fn artifact_and_decision_reject_lease_expiry_equality() {
    let fx = Fixture::new().await;
    fx.enqueue("request-boundary", "key-boundary", "same").await;
    let lease = fx
        .claim("attempt-boundary", Duration::seconds(60))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE execution_attempts SET state='running', lease_expires_at=? WHERE id='attempt-boundary'",
    )
    .bind(fx.clock.now().to_rfc3339())
    .execute(fx.repo.pool())
    .await
    .unwrap();

    assert!(
        !fx.record_artifact("attempt-boundary", lease.fencing_token, "artifact-boundary")
            .await
    );
    assert!(
        !fx.create_decision("attempt-boundary", lease.fencing_token, "decision-boundary")
            .await
    );
    let artifact_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    let decision_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_decisions")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(artifact_count, 0);
    assert_eq!(decision_count, 0);
}

// Defect 2 regression: `record_execution_artifact` and
// `create_execution_decision` must not run their eligibility SELECT and
// their INSERT as two separate un-transacted statements — a concurrent
// terminal transition could commit between them and let the write land
// against an attempt that had already gone terminal.
//
// Uses a file-backed database, not this suite's shared in-memory
// `ready_repo()` harness: SQLite's shared-cache mode (what a pooled
// `:memory:` database uses) imposes table-level locking that blocks a
// plain SELECT behind any pending write, which accidentally serializes
// the SELECT-then-INSERT gap this test needs to expose on both fixed and
// unfixed code. Fully awaits opening `BEGIN IMMEDIATE` and the terminal
// UPDATE before either writer is even constructed, so the hold is
// unconditionally in place before the race starts; `tokio::join!` alone
// would only control poll order, not which SQL reaches SQLite first.
#[tokio::test]
async fn artifact_and_decision_reject_concurrently_terminal_attempt() {
    let (fx, _dir) = Fixture::file_backed().await;
    let fence = fx
        .ready_completion_attempt("request-race-terminal", "attempt-race-terminal")
        .await;
    sqlx::query("UPDATE execution_attempts SET state='running' WHERE id='attempt-race-terminal'")
        .execute(fx.repo.pool())
        .await
        .unwrap();

    // Fully establish the held write lock and the terminal UPDATE *before*
    // either writer starts, so there is no race for who reaches the SQLite
    // engine first.
    let mut manual_tx = fx.repo.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let terminal_now = fx.clock.now().to_rfc3339();
    sqlx::query(
        "UPDATE execution_attempts SET state='succeeded', ended_at=?, updated_at=? WHERE id='attempt-race-terminal'",
    )
    .bind(&terminal_now)
    .bind(&terminal_now)
    .execute(&mut *manual_tx)
    .await
    .unwrap();

    let artifact_write = fx.record_artifact_result("attempt-race-terminal", fence, "artifact-race");
    let decision_write = fx.create_decision_result("attempt-race-terminal", fence, "decision-race");
    // Poll (bounded, not a fixed wait) until both writers have checked a
    // connection out of the pool — each has begun its own `BEGIN IMMEDIATE`
    // and is blocked on this lock — before releasing it.
    let idle_before_writers = fx.repo.pool().num_idle();
    let delayed_release = async {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while fx.repo.pool().num_idle() > idle_before_writers.saturating_sub(2) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "writers never both blocked on the lock"
            );
            tokio::task::yield_now().await;
        }
        manual_tx.commit().await.unwrap();
    };

    let (artifact_accepted, decision_accepted, _) =
        tokio::join!(artifact_write, decision_write, delayed_release);
    let artifact_accepted =
        artifact_accepted.expect("artifact write must succeed at the sqlx level");
    let decision_accepted =
        decision_accepted.expect("decision write must succeed at the sqlx level");

    assert!(
        !artifact_accepted,
        "artifact must not be recorded once the attempt has gone terminal concurrently"
    );
    assert!(
        !decision_accepted,
        "decision must not be recorded once the attempt has gone terminal concurrently"
    );
    assert_eq!(
        fx.count("execution_artifacts", "attempt-race-terminal")
            .await,
        0,
        "no artifact row may exist"
    );
    assert_eq!(
        fx.count("execution_decisions", "attempt-race-terminal")
            .await,
        0,
        "no decision row may exist"
    );
    assert_eq!(fx.attempt_state("attempt-race-terminal").await, "succeeded");
}
