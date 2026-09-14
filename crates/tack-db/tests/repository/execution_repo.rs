use crate::common::execution_fixture::{
    FakeClock, Fixture, cancellation_input, completion_input, count_where, expect_cancelled,
    expect_replayed_cancellation, recovery_input, request,
};

use chrono::Duration;
use sqlx::Row;
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        AttemptTransitionInput, AttemptTransitionPhase, AttemptTransitionResult,
        CancellationObservation, CancellationObservationInput, Completion, CompletionResult,
        CredentialRotationResult, EnqueueResult, EnrollmentToken, EventApplyResult, EventBatch,
        HeartbeatBatchResult, HeartbeatLease, NewEvent, NewRunner, OperatorRequeueResult,
        RecoveryDisposition, RecoveryObservation, RecoveryObservationInput,
        RecoveryObservationResult, RedeemEnrollmentResult, RequestSelection,
    },
};

#[tokio::test]
async fn enrollment_token_is_single_use_and_revocation_fails_closed() {
    let fx = Fixture::new().await;
    sqlx::query("UPDATE agent_runners SET state = 'pending_enrollment' WHERE id = 'runner-a'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    fx.issue_token("token-1", "token-hash").await;
    let expires = fx.clock.now() + Duration::days(1);
    assert_eq!(
        fx.redeem("token-hash", "credential-hash", expires).await,
        RedeemEnrollmentResult::Redeemed("runner-a".into())
    );
    assert_eq!(
        fx.redeem("token-hash", "other", expires).await,
        RedeemEnrollmentResult::InvalidOrExpired
    );
    fx.issue_token("token-2", "revoked").await;
    assert!(
        fx.repo
            .revoke_enrollment_token("revoked", &fx.clock)
            .await
            .unwrap()
    );
    assert_eq!(
        fx.redeem("revoked", "other", expires).await,
        RedeemEnrollmentResult::InvalidOrExpired
    );
}

#[tokio::test]
async fn concurrent_enrollment_redemption_has_one_winner() {
    let fx = Fixture::new().await;
    fx.repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: "runner-concurrent-enrollment",
                name: "Pending runner",
                credential_hash: "ignored-for-pending",
                labels: "{}",
                total_capacity: 1,
                available_capacity: 1,
                capability_snapshot: "{}",
                protocol_version: 1,
            },
            EnrollmentToken {
                id: "token-concurrent-enrollment",
                runner_id: "runner-concurrent-enrollment",
                token_hash: "hash-concurrent-enrollment",
                expires_at: fx.clock.now() + Duration::minutes(5),
            },
            &fx.clock,
        )
        .await
        .unwrap();
    let expires = fx.clock.now() + Duration::hours(1);
    let redeem = || fx.redeem("hash-concurrent-enrollment", "credential-hash", expires);
    let (left, right) = tokio::join!(redeem(), redeem());
    let results = [left, right];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, RedeemEnrollmentResult::Redeemed(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, RedeemEnrollmentResult::InvalidOrExpired))
            .count(),
        1
    );
}

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

#[tokio::test]
async fn enqueue_rejects_malformed_and_contradictory_snapshots() {
    let fx = Fixture::new().await;
    let mut missing = fx.request("snapshot-missing", "snapshot-missing", "same");
    missing.request_snapshot = Box::leak(
        missing
            .request_snapshot
            .replace("\"metadata\":{\"source\":\"test\"}", "")
            .replace(",,", ",")
            .replace(",}", "}")
            .into_boxed_str(),
    );
    assert!(fx.repo.enqueue_execution(missing, &fx.clock).await.is_err());

    let mut malformed = fx.request("snapshot-malformed", "snapshot-malformed", "same");
    malformed.request_snapshot = "{not-json";
    assert!(
        fx.repo
            .enqueue_execution(malformed, &fx.clock)
            .await
            .is_err()
    );

    let mut contradictory = fx.request("snapshot-cross", "snapshot-cross", "same");
    contradictory.request_snapshot = Box::leak(
        contradictory
            .request_snapshot
            .replace(
                "\"request_id\":\"snapshot-cross\"",
                "\"request_id\":\"other\"",
            )
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(contradictory, &fx.clock)
            .await
            .is_err()
    );

    let mut wrong_created_at = fx.request("snapshot-clock", "snapshot-clock", "same");
    wrong_created_at.request_snapshot = Box::leak(
        wrong_created_at
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert!(
        fx.repo
            .enqueue_execution(wrong_created_at, &fx.clock)
            .await
            .is_err()
    );

    for (suffix, from, to) in [
        (
            "created-by",
            r#""created_by":{"source":"operator","subject_id":"test"}"#,
            r#""created_by":"operator""#,
        ),
        (
            "profile-policy",
            r#""tool_policy":{"mode":"safe"}"#,
            r#""tool_policy":"safe""#,
        ),
        ("budgets", r#""budgets":{"limit":1}"#, r#""budgets":1"#),
        ("repository-kind", r#""kind":"git""#, r#""kind":1"#),
        (
            "metadata",
            r#""metadata":{"source":"test"}"#,
            r#""metadata":false"#,
        ),
        (
            "environment",
            r#""value":"test","secret_reference":null"#,
            r#""value":"test","secret_reference":"secret://mode""#,
        ),
    ] {
        let id = Box::leak(format!("snapshot-{suffix}").into_boxed_str());
        let mut invalid = fx.request(id, id, "same");
        invalid.request_snapshot =
            Box::leak(invalid.request_snapshot.replace(from, to).into_boxed_str());
        assert!(
            fx.repo.enqueue_execution(invalid, &fx.clock).await.is_err(),
            "{suffix}"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// Inserts a legacy (pre-`request_snapshot`) execution request row, every
/// column but `id`/`key`/`state` fixed at a valid placeholder.
async fn insert_legacy_request(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    key: &str,
    state: &str,
) {
    sqlx::query("INSERT INTO execution_requests(id,item_id,idempotency_scope,idempotency_key,request_fingerprint,state,selector_kind,selector_id,agent_profile_snapshot,repository_snapshot,permission_policy,created_at,updated_at) VALUES(?, 'legacy-item', 'legacy', ?, 'legacy-fingerprint', ?, 'exact_runner', 'runner-a', '{}', '{}', '{}', '2026-08-07T12:00:00Z', '2026-08-07T12:00:00Z')")
        .bind(id)
        .bind(key)
        .bind(state)
        .execute(conn)
        .await
        .unwrap();
}

/// Overwrites a legacy row's `request_snapshot` once the column exists.
async fn set_legacy_snapshot(pool: &sqlx::SqlitePool, id: &str, key: &str, snapshot: &str) {
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot=? WHERE id=? AND idempotency_key=?",
    )
    .bind(snapshot)
    .bind(id)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn m060_quarantines_all_nonterminal_malformed_legacy_snapshots() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "052_execution_report_replays")
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    for (id, key, state) in [
        ("legacy-request", "legacy-key", "queued"),
        ("legacy-partial", "legacy-partial-key", "leased"),
        ("legacy-malformed", "legacy-malformed-key", "running"),
        ("legacy-terminal", "legacy-terminal-key", "succeeded"),
        ("legacy-created-at", "legacy-created-at-key", "queued"),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            "queued",
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            "queued",
        ),
        ("legacy-valid", "legacy-valid-key", "queued"),
    ] {
        insert_legacy_request(&mut connection, id, key, state).await;
    }
    drop(connection);
    migrations::run_up_to(&pool, "058_execution_recovery_replay_response")
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{\"created_by\":{},\"selector\":{},\"repository\":{}}' WHERE id = 'legacy-partial'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{not-json' WHERE id IN ('legacy-malformed', 'legacy-terminal')")
        .execute(&pool)
        .await
        .unwrap();
    type Mutator = fn(&str) -> String;
    let mutations: [(&str, &str, Mutator); 4] = [
        ("legacy-created-at", "legacy-created-at-key", |s| {
            s.replace("2026-08-07T12:00:00Z", "2026-08-07 12:00:00")
        }),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            |s| s.replace("\"timeout_seconds\":60", "\"timeout_seconds\":-1"),
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            |s| s.replace("\"timeout_seconds\":60", "\"timeout_seconds\":60.5"),
        ),
        ("legacy-valid", "legacy-valid-key", |s| s.to_owned()),
    ];
    for (id, key, mutate) in mutations {
        let snapshot = mutate(request(id, "legacy-item", key, "same").request_snapshot);
        set_legacy_snapshot(&pool, id, key, &snapshot).await;
    }
    migrations::run_up_to(&pool, "059_quarantine_legacy_execution_request_snapshots")
        .await
        .unwrap();
    let m059_cohort: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, state FROM execution_requests WHERE id LIKE 'legacy-%timeout' OR id = 'legacy-created-at' OR id = 'legacy-valid' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        m059_cohort,
        vec![
            ("legacy-created-at".into(), "queued".into()),
            ("legacy-fractional-timeout".into(), "queued".into()),
            ("legacy-negative-timeout".into(), "queued".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    migrations::run_all(&pool).await.unwrap();
    let after: Vec<(String, String)> =
        sqlx::query_as("SELECT id, state FROM execution_requests ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        after,
        vec![
            ("legacy-created-at".into(), "needs_operator".into()),
            ("legacy-fractional-timeout".into(), "needs_operator".into()),
            ("legacy-malformed".into(), "needs_operator".into()),
            ("legacy-negative-timeout".into(), "needs_operator".into()),
            ("legacy-partial".into(), "needs_operator".into()),
            ("legacy-request".into(), "needs_operator".into()),
            ("legacy-terminal".into(), "succeeded".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    let repo = Repository::new(pool);
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-a",
            name: "Runner A",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-legacy-valid",
            "attempt-legacy-valid",
            Duration::seconds(60),
            &clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap()
        .expect("the later valid queued row remains claimable");
    assert_eq!(claimed.lease.request_id, "legacy-valid");
}

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
async fn old_schema_upgrades_to_all_ten_execution_tables() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .unwrap();
    migrations::run_all(&pool).await.unwrap();
    for table in [
        "agent_fleets",
        "agent_runners",
        "agent_fleet_members",
        "agent_profiles",
        "model_profiles",
        "execution_requests",
        "execution_attempts",
        "execution_events",
        "execution_artifacts",
        "execution_decisions",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{table} missing after upgrade");
    }
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations WHERE name BETWEEN '039_agent_fleets' AND '048_execution_decisions'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(migration_count, 10);
}

#[tokio::test]
async fn enqueue_is_idempotent_and_conflicting_reuse_is_rejected() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into())
    );
    assert_eq!(
        fx.enqueue("request-c", "key-a", "different").await,
        EnqueueResult::Conflict
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(fx.repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn enqueue_replay_compares_frozen_snapshot_before_current_time() {
    let fx = Fixture::new().await;
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Created("request-a".into())
    );
    fx.clock.advance(Duration::seconds(1));
    assert_eq!(
        fx.enqueue("request-a", "key-a", "same").await,
        EnqueueResult::Replayed("request-a".into()),
        "an exact retry uses the original frozen snapshot rather than the new clock"
    );
    let mut changed_timestamp = fx.request("request-a", "key-a", "same");
    changed_timestamp.request_snapshot = Box::leak(
        changed_timestamp
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert_eq!(
        fx.repo
            .enqueue_execution(changed_timestamp, &fx.clock)
            .await
            .unwrap(),
        EnqueueResult::Conflict,
        "same idempotency key with a changed frozen timestamp is not an exact replay"
    );
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

// Defect 3 regression: two concurrent or retried credential rotations from
// the same runner both authenticate against the same still-valid old hash
// (that is the whole point of a rotation race — neither has learned the
// other's new hash yet). Without a compare-and-set against the hash that was
// actually authenticated, last-writer-wins would silently discard one
// rotation's result, leaving its caller holding a credential the server no
// longer accepts and with no way to recover short of a fresh operator-issued
// enrollment token. `rotate_runner_credential` must let exactly one of two
// concurrent rotations against the same expected hash win.
#[tokio::test]
async fn concurrent_credential_rotations_have_exactly_one_winner() {
    let fx = Fixture::new().await;
    // `Fixture::new` registers "runner-a" with credential_hash "hash-only".
    let expires = fx.clock.now() + Duration::days(30);
    let (a, b) = tokio::join!(
        fx.rotate_credential("hash-only", "hash-rotated-by-left", expires),
        fx.rotate_credential("hash-only", "hash-rotated-by-right", expires),
    );
    let a = a.expect("first rotation must succeed at the sqlx level");
    let b = b.expect("second rotation must succeed at the sqlx level");

    let rotated = count_where([&a, &b], |r| {
        matches!(r, CredentialRotationResult::Rotated(_))
    });
    let mismatched = count_where([&a, &b], |r| {
        matches!(r, CredentialRotationResult::HashMismatch)
    });
    assert_eq!(
        rotated, 1,
        "exactly one concurrent rotation against the same expected hash wins: {a:?} / {b:?}"
    );
    assert_eq!(
        mismatched, 1,
        "the other concurrent rotation observes its expected hash no longer matches: {a:?} / {b:?}"
    );

    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(fx.repo.pool())
            .await
            .unwrap();
    // Whichever branch won, the stored hash must be exactly that branch's new
    // hash — never the loser's, and never some third, corrupted value.
    let expected = if matches!(a, CredentialRotationResult::Rotated(_)) {
        "hash-rotated-by-left"
    } else {
        "hash-rotated-by-right"
    };
    assert_eq!(
        stored_hash, expected,
        "stored hash must match whichever branch's CredentialRotationResult::Rotated fired"
    );

    // A retry against the now-stale original hash (e.g. a naive client that
    // didn't observe either response and blindly retries with what it still
    // believes is current) must not be able to rotate again.
    let stale_retry = fx
        .rotate_credential("hash-only", "hash-rotated-by-stale-retry", expires)
        .await
        .unwrap();
    assert_eq!(stale_retry, CredentialRotationResult::HashMismatch);
    let stored_hash_after_retry: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(fx.repo.pool())
            .await
            .unwrap();
    assert_eq!(
        stored_hash_after_retry, expected,
        "a stale-hash retry must not overwrite the winning rotation"
    );
}
