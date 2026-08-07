mod common;

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        Completion, EnqueueResult, EventBatch, ExecutionClock, NewAgentProfile, NewEvent,
        NewExecutionRequest, NewRunner,
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
    }
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
            &[event.clone()],
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
