mod common;

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        ClaimReplayResult, Completion, CompletionResult, EnqueueResult, EnrollmentToken,
        EventApplyResult, EventBatch, ExecutionClock, HeartbeatBatchResult, HeartbeatLease,
        NewAgentProfile, NewEvent, NewExecutionRequest, NewRunner, RecoveryClassification,
        RedeemEnrollmentResult,
    },
};

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ))
    }
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}

#[tokio::test]
async fn enrollment_token_is_single_use_expiry_and_revocation_fail_closed() {
    let (repo, _, clock) = ready_repo().await;
    sqlx::query("UPDATE agent_runners SET state = 'pending_enrollment' WHERE id = 'runner-a'")
        .execute(repo.pool())
        .await
        .unwrap();
    repo.issue_enrollment_token(
        EnrollmentToken {
            id: "token-1",
            runner_id: "runner-a",
            token_hash: "token-hash",
            expires_at: clock.now() + Duration::minutes(5),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        repo.redeem_enrollment_token(
            "token-hash",
            "credential-hash",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::Redeemed("runner-a".into())
    );
    assert_eq!(
        repo.redeem_enrollment_token(
            "token-hash",
            "other",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );
    repo.issue_enrollment_token(
        EnrollmentToken {
            id: "token-2",
            runner_id: "runner-a",
            token_hash: "revoked",
            expires_at: clock.now() + Duration::minutes(5),
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(
        repo.revoke_enrollment_token("revoked", &clock)
            .await
            .unwrap()
    );
    assert_eq!(
        repo.redeem_enrollment_token(
            "revoked",
            "other",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );
}

#[tokio::test]
async fn claim_request_replay_returns_the_original_lease() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let first = repo
        .claim_execution_idempotent(
            "runner-a",
            "claim-a",
            "attempt-a",
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let replay = repo
        .claim_execution_idempotent(
            "runner-a",
            "claim-a",
            "attempt-b",
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(first, replay);
    assert!(matches!(replay, ClaimReplayResult::Lease(_)));
}

#[tokio::test]
async fn heartbeat_replays_and_recovery_is_audited_once() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-a", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .unwrap();
    let leases = [HeartbeatLease {
        attempt_id: "attempt-a",
        fencing_token: lease.fencing_token,
    }];
    assert!(matches!(
        repo.heartbeat_batch(
            "runner-a",
            "hb-1",
            0,
            &leases,
            Duration::seconds(60),
            &clock
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Accepted(_)
    ));
    assert!(matches!(
        repo.heartbeat_batch(
            "runner-a",
            "hb-1",
            0,
            &leases,
            Duration::seconds(60),
            &clock
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Replayed(_)
    ));
    clock.advance(Duration::seconds(61));
    assert!(
        repo.recover_attempt(
            "attempt-a",
            "recover-1",
            RecoveryClassification::SafePreSpawnRequeue,
            "proven not spawned",
            &clock
        )
        .await
        .unwrap()
    );
    assert!(
        repo.recover_attempt(
            "attempt-a",
            "recover-1",
            RecoveryClassification::SafePreSpawnRequeue,
            "proven not spawned",
            &clock
        )
        .await
        .unwrap()
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-a'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 1);
}

#[tokio::test]
async fn structured_events_and_cancellation_observation_replay() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-z", &item_id, "key-z", "same"), &clock)
        .await
        .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-z", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .unwrap();
    let event = NewEvent {
        id: "row-z",
        event_id: "event-z",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-z",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-z",
    };
    let first = repo
        .append_execution_events_result(batch.clone(), std::slice::from_ref(&event), &clock)
        .await
        .unwrap();
    let EventApplyResult::Applied(first) = first else {
        panic!("first event batch must apply");
    };
    assert_eq!(first.accepted_event_ids, vec!["event-z"]);
    let replay = repo
        .append_execution_events_result(batch, &[event], &clock)
        .await
        .unwrap();
    let EventApplyResult::Applied(replay) = replay else {
        panic!("matching event batch must replay");
    };
    assert_eq!(replay.accepted_event_ids, vec!["event-z"]);
    assert!(replay.replayed);
    assert!(
        repo.request_execution_cancellation("request-z", &clock)
            .await
            .unwrap()
    );
    assert_eq!(
        repo.observe_cancellation(
            "runner-a",
            "attempt-z",
            lease.fencing_token,
            "cancel-z",
            &clock
        )
        .await
        .unwrap(),
        tack_db::repo::execution::CancellationObservation::Cancelled { replayed: false }
    );
    assert_eq!(
        repo.observe_cancellation(
            "runner-a",
            "attempt-z",
            lease.fencing_token,
            "cancel-z",
            &clock
        )
        .await
        .unwrap(),
        tack_db::repo::execution::CancellationObservation::Cancelled { replayed: true }
    );
}

#[tokio::test]
async fn event_replay_canonicalizes_equivalent_json_payloads() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-canonical", &item_id, "key-canonical", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = repo
        .claim_execution(
            "runner-a",
            "attempt-canonical",
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap()
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
        occurred_at: clock.now(),
    };
    let replay = NewEvent {
        id: "row-canonical-retry",
        payload: r#"{"z":0,"outer":{"a":1,"b":2}}"#,
        ..initial
    };
    assert!(matches!(
        repo.append_execution_events_result(batch.clone(), &[initial], &clock)
            .await
            .unwrap(),
        EventApplyResult::Applied(result) if !result.replayed
    ));
    assert!(matches!(
        repo.append_execution_events_result(batch, &[replay], &clock)
            .await
            .unwrap(),
        EventApplyResult::Applied(result) if result.replayed
    ));
}

#[tokio::test]
async fn event_replay_changed_payload_is_conflict_and_does_not_write() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-conflict", &item_id, "key-conflict", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = repo
        .claim_execution(
            "runner-a",
            "attempt-conflict",
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap()
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
        occurred_at: clock.now(),
    };
    let changed = NewEvent {
        id: "row-conflict-retry",
        payload: r#"{"state":"changed"}"#,
        ..initial
    };
    repo.append_execution_events_result(batch.clone(), &[initial], &clock)
        .await
        .unwrap();
    assert_eq!(
        repo.append_execution_events_result(batch, &[changed], &clock)
            .await
            .unwrap(),
        EventApplyResult::ReplayConflict
    );
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let checkpoint: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_count, 1);
    assert_eq!(checkpoint.as_deref(), Some("checkpoint-conflict"));
}

#[tokio::test]
async fn event_replay_foreign_fence_is_stale_and_does_not_write() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-stale", &item_id, "key-stale", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-stale", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .unwrap();
    let result = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-stale",
                fencing_token: lease.fencing_token + 1,
                previous_checkpoint: None,
                checkpoint: "checkpoint-stale",
            },
            &[NewEvent {
                id: "row-stale",
                event_id: "event-stale",
                sequence: 1,
                source: "runner",
                kind: "progress",
                payload: "{}",
                occurred_at: clock.now(),
            }],
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(result, EventApplyResult::Stale);
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-stale'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn token_guards_and_operator_requeue_are_idempotent() {
    let (repo, item_id, clock) = ready_repo().await;
    let bad = repo
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
                expires_at: clock.now(),
            },
            &clock,
        )
        .await;
    assert!(
        bad.is_err(),
        "mismatched/expired/invalid capacity token issue fails before writes"
    );
    repo.enqueue_execution(request("request-r", &item_id, "key-r", "same"), &clock)
        .await
        .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-r", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE execution_attempts SET state='needs_operator' WHERE id='attempt-r'")
        .execute(repo.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET state='needs_operator',cancellation_requested_at='x' WHERE id='request-r'").execute(repo.pool()).await.unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    use tack_db::repo::execution::OperatorRequeueResult;
    assert_eq!(
        repo.operator_requeue_needs_operator(
            "request-r",
            "client-key",
            "operator-a",
            "reason-a",
            &clock
        )
        .await
        .unwrap(),
        OperatorRequeueResult::Requeued
    );
    assert_eq!(
        repo.operator_requeue_needs_operator(
            "request-r",
            "client-key",
            "operator-a",
            "reason-a",
            &clock
        )
        .await
        .unwrap(),
        OperatorRequeueResult::Replayed
    );
    let cleared: Option<String> = sqlx::query_scalar(
        "SELECT cancellation_requested_at FROM execution_requests WHERE id='request-r'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(cleared.is_none());
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-r'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 1);
    let after: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, after);
    let _ = lease;
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn ready_repo() -> (Repository, String, FakeClock) {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await;
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
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-a",
            name: "Profile A",
            instructions: "test",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .unwrap();
    (repo, item.id.to_string(), clock)
}

fn request<'a>(
    id: &'a str,
    item_id: &'a str,
    key: &'a str,
    fingerprint: &'a str,
) -> NewExecutionRequest<'a> {
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: fingerprint,
        selector_kind: "exact_runner",
        selector_id: "runner-a",
        agent_profile_id: Some("profile-a"),
        agent_profile_snapshot: "{}",
        requested_harness_kind: Some("codex"),
        requested_model_provider: Some("openai"),
        requested_model_id: Some("opaque/model"),
        repository_snapshot: "{}",
        permission_policy: "{}",
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot: r#"{"created_by":{"source":"operator","subject_id":"test"},"selector":{"kind":"exact_runner","runner_id":"runner-a"},"repository":{}}"#,
    }
}

async fn ready_completion_attempt(
    repo: &Repository,
    item_id: &str,
    clock: &FakeClock,
    request_id: &str,
    attempt_id: &str,
) -> i64 {
    repo.enqueue_execution(request(request_id, item_id, request_id, "same"), clock)
        .await
        .unwrap();
    repo.claim_execution("runner-a", attempt_id, Duration::seconds(60), clock)
        .await
        .unwrap()
        .unwrap()
        .fencing_token
}

fn completion_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    completion_id: &'a str,
    final_event_checkpoint: Option<&'a str>,
) -> Completion<'a> {
    Completion {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        completion_id,
        final_event_checkpoint,
        terminal_state: "succeeded",
        terminal_reason: "completed",
        actual_execution: r#"{"result":{"b":2,"a":1}}"#,
        usage: r#"{"output_tokens":2,"input_tokens":1}"#,
    }
}

#[tokio::test]
async fn heartbeat_rejects_false_free_capacity_while_a_lease_is_active() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-free",
        "attempt-heartbeat-free",
    )
    .await;
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-free",
            1,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-free",
                fencing_token: fence,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Conflict
    );
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 0);
}

#[tokio::test]
async fn heartbeat_multi_lease_replay_is_canonical_and_authoritative() {
    let (repo, item_id, clock) = ready_repo().await;
    sqlx::query(
        "UPDATE agent_runners SET total_capacity = 2, available_capacity = 2 WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let first_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-multi-a",
        "attempt-heartbeat-multi-a",
    )
    .await;
    let second_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-multi-b",
        "attempt-heartbeat-multi-b",
    )
    .await;
    let leases = [
        HeartbeatLease {
            attempt_id: "attempt-heartbeat-multi-a",
            fencing_token: first_fence,
        },
        HeartbeatLease {
            attempt_id: "attempt-heartbeat-multi-b",
            fencing_token: second_fence,
        },
    ];
    let accepted = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-multi",
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("multi-lease heartbeat must be accepted");
    };
    assert_eq!(accepted.leases.len(), 2);
    let replayed = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-multi",
            0,
            &[leases[1].clone(), leases[0].clone()],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("canonical lease ordering must replay");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_exact_replay_after_clock_advance_returns_original_fields() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-time",
        "attempt-heartbeat-time",
    )
    .await;
    let leases = [HeartbeatLease {
        attempt_id: "attempt-heartbeat-time",
        fencing_token: fence,
    }];
    let accepted = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-time",
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("first heartbeat must be accepted");
    };
    clock.advance(Duration::seconds(61));
    let replayed = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-time",
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("exact heartbeat retry must replay after lease expiry");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_changed_same_id_conflicts_without_write() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-conflict",
        "attempt-heartbeat-conflict",
    )
    .await;
    let leases = [HeartbeatLease {
        attempt_id: "attempt-heartbeat-conflict",
        fencing_token: fence,
    }];
    repo.heartbeat_batch(
        "runner-a",
        "heartbeat-conflict",
        0,
        &leases,
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap();
    let before: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    clock.advance(Duration::seconds(1));
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-conflict",
            1,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Conflict
    );
    let after: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn heartbeat_stale_lease_returns_typed_stale_result() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-stale",
        "attempt-heartbeat-stale",
    )
    .await;
    clock.advance(Duration::seconds(61));
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-stale",
            1,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-stale",
                fencing_token: fence,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::StaleLease("attempt-heartbeat-stale".into())
    );
}

#[tokio::test]
async fn heartbeat_replay_insert_failure_rolls_back_all_updates() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-rollback",
        "attempt-heartbeat-rollback",
    )
    .await;
    sqlx::query("CREATE TRIGGER fail_heartbeat_replay BEFORE INSERT ON execution_heartbeat_replays BEGIN SELECT RAISE(ABORT, 'forced heartbeat replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    let result = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-rollback",
            0,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-rollback",
                fencing_token: fence,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await;
    assert!(result.is_err());
    let last_heartbeat: Option<String> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM execution_attempts WHERE id = 'attempt-heartbeat-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let runner_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let replay_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_heartbeat_replays WHERE heartbeat_id = 'heartbeat-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(last_heartbeat.is_none());
    assert!(runner_heartbeat.is_none());
    assert_eq!(replay_count, 0);
}

#[tokio::test]
async fn completion_response_loss_replays_authoritative_response() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-loss",
        "attempt-completion-loss",
    )
    .await;
    let completion = completion_input("attempt-completion-loss", fence, "completion-loss", None);
    let committed = repo
        .complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let CompletionResult::Committed(committed) = committed else {
        panic!("first completion must commit");
    };
    let replayed = repo
        .complete_execution_result(completion, &clock)
        .await
        .unwrap();
    let CompletionResult::Replayed(replayed) = replayed else {
        panic!("lost response retry must replay");
    };
    assert_eq!(replayed, committed);
}

#[tokio::test]
async fn completion_replay_foreign_fence_is_stale() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-stale",
        "attempt-completion-stale",
    )
    .await;
    let completion = completion_input("attempt-completion-stale", fence, "completion-stale", None);
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let foreign = Completion {
        fencing_token: fence + 1,
        ..completion
    };
    assert_eq!(
        repo.complete_execution_result(foreign, &clock)
            .await
            .unwrap(),
        CompletionResult::Stale
    );
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_json() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
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
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let canonical_retry = Completion {
        actual_execution: r#"{"result":{"a":1,"b":2}}"#,
        usage: r#"{"input_tokens":1,"output_tokens":2}"#,
        ..completion
    };
    assert!(matches!(
        repo.complete_execution_result(canonical_retry, &clock)
            .await
            .unwrap(),
        CompletionResult::Replayed(_)
    ));
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_terminal_reason() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-reason",
        "attempt-completion-reason",
    )
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
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let semantic_retry = Completion {
        terminal_reason: r#"{"code":"done","detail":{"a":1,"b":2}}"#,
        ..completion
    };
    assert!(matches!(
        repo.complete_execution_result(semantic_retry, &clock)
            .await
            .unwrap(),
        CompletionResult::Replayed(_)
    ));
}

#[tokio::test]
async fn completion_changed_fields_or_checkpoint_conflict_without_write() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-conflict",
        "attempt-completion-conflict",
    )
    .await;
    let base = completion_input(
        "attempt-completion-conflict",
        fence,
        "completion-conflict",
        None,
    );
    let wrong_checkpoint = Completion {
        final_event_checkpoint: Some("not-current"),
        ..base.clone()
    };
    assert_eq!(
        repo.complete_execution_result(wrong_checkpoint, &clock)
            .await
            .unwrap(),
        CompletionResult::Conflict
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-completion-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(state, "leased");
    repo.complete_execution_result(base.clone(), &clock)
        .await
        .unwrap();
    let changed_fields = Completion {
        terminal_reason: "changed",
        ..base.clone()
    };
    let changed_checkpoint = Completion {
        final_event_checkpoint: Some("changed"),
        ..base
    };
    assert_eq!(
        repo.complete_execution_result(changed_fields, &clock)
            .await
            .unwrap(),
        CompletionResult::Conflict
    );
    assert_eq!(
        repo.complete_execution_result(changed_checkpoint, &clock)
            .await
            .unwrap(),
        CompletionResult::Conflict
    );
    let completion_id: String = sqlx::query_scalar(
        "SELECT completion_id FROM execution_attempts WHERE id = 'attempt-completion-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(completion_id, "completion-conflict");
}

#[tokio::test]
async fn completion_replay_corrupt_response_fails_closed() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-corrupt",
        "attempt-completion-corrupt",
    )
    .await;
    let completion = completion_input(
        "attempt-completion-corrupt",
        fence,
        "completion-corrupt",
        None,
    );
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_completion_replays SET response = '{}' WHERE attempt_id = 'attempt-completion-corrupt'")
        .execute(repo.pool())
        .await
        .unwrap();
    assert!(
        repo.complete_execution_result(completion, &clock)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn completion_replay_restores_capacity_once_and_never_above_cap() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-capacity",
        "attempt-completion-capacity",
    )
    .await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let completion = completion_input(
        "attempt-completion-capacity",
        fence,
        "completion-capacity",
        None,
    );
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    repo.complete_execution_result(completion, &clock)
        .await
        .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 1);

    let second_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-once",
        "attempt-completion-once",
    )
    .await;
    let second = completion_input(
        "attempt-completion-once",
        second_fence,
        "completion-once",
        None,
    );
    repo.complete_execution_result(second.clone(), &clock)
        .await
        .unwrap();
    let after_commit: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    repo.complete_execution_result(second, &clock)
        .await
        .unwrap();
    let after_replay: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(after_commit, 1);
    assert_eq!(after_replay, 1);
}

#[tokio::test]
async fn completion_replay_insert_failure_rolls_back_terminal_transition() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-rollback",
        "attempt-completion-rollback",
    )
    .await;
    sqlx::query("CREATE TRIGGER fail_completion_replay BEFORE INSERT ON execution_completion_replays BEGIN SELECT RAISE(ABORT, 'forced completion replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    let error = repo
        .complete_execution_result(
            completion_input(
                "attempt-completion-rollback",
                fence,
                "completion-rollback",
                None,
            ),
            &clock,
        )
        .await;
    assert!(error.is_err());
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-completion-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id = 'request-completion-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(attempt_state, "leased");
    assert_eq!(request_state, "leased");
    assert_eq!(capacity, 0);
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
    let (repo, item_id, clock) = ready_repo().await;
    assert_eq!(
        repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Created("request-a".into())
    );
    assert_eq!(
        repo.enqueue_execution(request("request-b", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Replayed("request-a".into())
    );
    assert_eq!(
        repo.enqueue_execution(request("request-c", &item_id, "key-a", "different"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Conflict
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn claim_fence_replay_and_terminal_state_are_atomic() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-a", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .expect("first claim");
    assert_eq!(lease.fencing_token, 1);
    assert!(
        repo.claim_execution("runner-a", "attempt-b", Duration::seconds(60), &clock)
            .await
            .unwrap()
            .is_none(),
        "capacity/request state prevent a second valid lease"
    );
    assert!(
        !repo
            .heartbeat_execution("runner-a", "attempt-a", 999, Duration::seconds(60), &clock)
            .await
            .unwrap(),
        "stale fence writes nothing"
    );
    let event = NewEvent {
        id: "row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    assert!(
        repo.append_execution_events(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                previous_checkpoint: None,
                checkpoint: "checkpoint-1"
            },
            std::slice::from_ref(&event),
            &clock
        )
        .await
        .unwrap()
    );
    assert!(
        repo.append_execution_events(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                previous_checkpoint: None,
                checkpoint: "checkpoint-1"
            },
            &[event],
            &clock
        )
        .await
        .unwrap(),
        "same checkpoint is a replay"
    );
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_events")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(event_count, 1);
    assert!(
        repo.complete_execution(
            Completion {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                completion_id: "complete-1",
                final_event_checkpoint: Some("checkpoint-1"),
                terminal_state: "succeeded",
                terminal_reason: "completed",
                actual_execution: "{}",
                usage: "{}"
            },
            &clock
        )
        .await
        .unwrap()
    );
    assert!(
        repo.complete_execution(
            Completion {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                completion_id: "complete-1",
                final_event_checkpoint: Some("checkpoint-1"),
                terminal_state: "succeeded",
                terminal_reason: "completed",
                actual_execution: "{}",
                usage: "{}"
            },
            &clock
        )
        .await
        .unwrap(),
        "same completion is replay-safe"
    );
    assert!(
        !repo
            .complete_execution(
                Completion {
                    runner_id: "runner-a",
                    attempt_id: "attempt-a",
                    fencing_token: 1,
                    completion_id: "complete-2",
                    final_event_checkpoint: Some("checkpoint-1"),
                    terminal_state: "failed",
                    terminal_reason: "changed",
                    actual_execution: "{}",
                    usage: "{}"
                },
                &clock
            )
            .await
            .unwrap(),
        "terminal attempt cannot reopen"
    );
}

#[tokio::test]
async fn concurrent_claimers_receive_exactly_one_valid_lease() {
    let (repo, item_id, clock) = ready_repo().await;
    // Capacity must not be the reason the second claimer loses: both can enter
    // the scheduler, but the request's queued → leased compare-and-set wins once.
    sqlx::query("UPDATE agent_runners SET total_capacity = 2, available_capacity = 2")
        .execute(repo.pool())
        .await
        .unwrap();
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let left = repo.clone();
    let right = repo.clone();
    let (first, second) = tokio::join!(
        left.claim_execution("runner-a", "attempt-a", Duration::seconds(60), &clock),
        right.claim_execution("runner-a", "attempt-b", Duration::seconds(60), &clock),
    );
    let leases = [first.unwrap(), second.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        leases.len(),
        1,
        "two claimers cannot receive valid leases for one request"
    );
}

#[tokio::test]
async fn foreign_keys_and_expiry_recovery_fail_closed() {
    let (repo, item_id, clock) = ready_repo().await;
    let orphan = repo
        .enqueue_execution(request("orphan", "missing-item", "orphan-key", "x"), &clock)
        .await;
    assert!(orphan.is_err(), "request item FK is enforced");
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = repo
        .claim_execution("runner-a", "attempt-a", Duration::seconds(5), &clock)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !repo
            .classify_expired_attempt("attempt-a", "queued", &clock)
            .await
            .unwrap(),
        "ambiguous work never auto-requeues"
    );
    clock.advance(Duration::seconds(6));
    assert!(
        repo.classify_expired_attempt("attempt-a", "needs_operator", &clock)
            .await
            .unwrap()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
        .bind(&lease.attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state, "needs_operator");
}

#[tokio::test]
async fn queue_and_history_indexes_are_used() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let queue_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT id FROM execution_requests WHERE state = 'queued' ORDER BY created_at")
        .fetch_all(repo.pool()).await.unwrap();
    assert!(queue_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_requests_queue")
    }));
    let lease = repo
        .claim_execution("runner-a", "attempt-a", Duration::seconds(60), &clock)
        .await
        .unwrap()
        .unwrap();
    let event = NewEvent {
        id: "row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    repo.append_execution_events(
        EventBatch {
            runner_id: "runner-a",
            attempt_id: "attempt-a",
            fencing_token: lease.fencing_token,
            previous_checkpoint: None,
            checkpoint: "checkpoint-1",
        },
        &[event],
        &clock,
    )
    .await
    .unwrap();
    let history_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT * FROM execution_events WHERE attempt_id = 'attempt-a' ORDER BY sequence")
        .fetch_all(repo.pool()).await.unwrap();
    assert!(history_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_events_timeline")
    }));
}
