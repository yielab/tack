use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_core::{
    models::{CreateItem, CreateProject, ItemType, Priority, ProjectType},
    vocabulary,
};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        Completion, EventBatch, ExecutionClock, NewAgentProfile, NewEvent, NewExecutionRequest,
        NewRunner,
    },
};
use uuid::Uuid;

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .expect("fixed timestamp")
                .with_timezone(&Utc),
        ))
    }

    fn advance(&self, duration: Duration) {
        *self.0.lock().expect("clock lock") += duration;
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

struct Fixture {
    repo: Repository,
    item_id: String,
    clock: FakeClock,
}

async fn fixture() -> Fixture {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace_id = Uuid::new_v4();
    let default_vocabulary =
        serde_json::to_string(&vocabulary::default_vocabulary()).expect("vocabulary JSON");
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Crash Matrix', ?)",
    )
    .bind(workspace_id.to_string())
    .bind(default_vocabulary)
    .execute(repo.pool())
    .await
    .expect("workspace");
    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: "Crash Matrix".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_owned();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Crash-boundary work".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: Some(Priority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item");
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-crash",
            name: "Crash Matrix Runner",
            credential_hash: "test-hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-crash",
            name: "Crash Matrix Profile",
            instructions: "exercise failure boundaries",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("profile");
    Fixture {
        repo,
        item_id: item.id.to_string(),
        clock,
    }
}

fn request<'a>(item_id: &'a str) -> NewExecutionRequest<'a> {
    NewExecutionRequest {
        id: "request-crash",
        item_id,
        idempotency_scope: "crash-matrix",
        idempotency_key: "request-1",
        request_fingerprint: "request-1-fingerprint",
        selector_kind: "exact_runner",
        selector_id: "runner-crash",
        agent_profile_id: Some("profile-crash"),
        agent_profile_snapshot: "{}",
        requested_harness_kind: Some("mock"),
        requested_model_provider: None,
        requested_model_id: None,
        repository_snapshot: "{}",
        permission_policy: "{}",
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
    }
}

async fn enqueue(fixture: &Fixture) {
    fixture
        .repo
        .enqueue_execution(request(&fixture.item_id), &fixture.clock)
        .await
        .expect("enqueue");
}

async fn claim(fixture: &Fixture) {
    fixture
        .repo
        .claim_execution(
            "runner-crash",
            "attempt-crash",
            Duration::seconds(60),
            &fixture.clock,
        )
        .await
        .expect("claim query")
        .expect("lease");
}

#[tokio::test]
async fn crash_before_claim_commit_rolls_back_request_capacity_and_fence() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    sqlx::query(
        "CREATE TRIGGER inject_claim_crash BEFORE INSERT ON execution_attempts BEGIN \
         SELECT RAISE(ABORT, 'injected claim crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");

    let crashed = fixture
        .repo
        .claim_execution(
            "runner-crash",
            "attempt-crash",
            Duration::seconds(60),
            &fixture.clock,
        )
        .await;
    assert!(crashed.is_err(), "fault injection must reach claim commit");

    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = 'request-crash'")
            .fetch_one(fixture.repo.pool())
            .await
            .expect("request state");
    let capacity: i64 = sqlx::query_scalar(
        "SELECT available_capacity FROM agent_runners WHERE id = 'runner-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("capacity");
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts")
        .fetch_one(fixture.repo.pool())
        .await
        .expect("attempt count");
    assert_eq!(request_state, "queued");
    assert_eq!(capacity, 1);
    assert_eq!(attempts, 0);

    sqlx::query("DROP TRIGGER inject_claim_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    let lease = fixture
        .repo
        .claim_execution(
            "runner-crash",
            "attempt-after-retry",
            Duration::seconds(60),
            &fixture.clock,
        )
        .await
        .expect("claim retry")
        .expect("lease after retry");
    assert_eq!(lease.fencing_token, 1, "rollback cannot burn a fence");
}

#[tokio::test]
async fn expired_post_claim_ambiguity_never_grants_a_second_fence() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    claim(&fixture).await;
    fixture.clock.advance(Duration::seconds(61));

    assert!(
        fixture
            .repo
            .classify_expired_attempt("attempt-crash", "needs_operator", &fixture.clock)
            .await
            .expect("classify expiry")
    );
    let state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = 'attempt-crash'")
            .fetch_one(fixture.repo.pool())
            .await
            .expect("attempt state");
    assert_eq!(state, "needs_operator");
    assert!(
        fixture
            .repo
            .claim_execution(
                "runner-crash",
                "attempt-invalid-second-fence",
                Duration::seconds(60),
                &fixture.clock,
            )
            .await
            .expect("second claim query")
            .is_none(),
        "ambiguous side effects must never be blind-retried"
    );
    let fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("fence count");
    assert_eq!(fences, 1);
}

#[tokio::test]
async fn crash_during_event_batch_rolls_back_rows_and_checkpoint_then_replays_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    claim(&fixture).await;
    sqlx::query(
        "CREATE TRIGGER inject_event_crash BEFORE INSERT ON execution_events BEGIN \
         SELECT RAISE(ABORT, 'injected event crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");
    let event = NewEvent {
        id: "event-row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: r#"{"phase":"spawn"}"#,
        occurred_at: fixture.clock.now(),
    };
    let batch = || EventBatch {
        runner_id: "runner-crash",
        attempt_id: "attempt-crash",
        fencing_token: 1,
        previous_checkpoint: None,
        checkpoint: "checkpoint-1",
    };

    assert!(
        fixture
            .repo
            .append_execution_events(batch(), std::slice::from_ref(&event), &fixture.clock)
            .await
            .is_err()
    );
    let row = sqlx::query(
        "SELECT event_checkpoint, (SELECT COUNT(*) FROM execution_events) AS event_count \
         FROM execution_attempts WHERE id = 'attempt-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("checkpoint state");
    assert_eq!(row.get::<Option<String>, _>("event_checkpoint"), None);
    assert_eq!(row.get::<i64, _>("event_count"), 0);

    sqlx::query("DROP TRIGGER inject_event_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    assert!(
        fixture
            .repo
            .append_execution_events(batch(), std::slice::from_ref(&event), &fixture.clock)
            .await
            .expect("first report")
    );
    assert!(
        fixture
            .repo
            .append_execution_events(batch(), std::slice::from_ref(&event), &fixture.clock)
            .await
            .expect("replay")
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_events")
        .fetch_one(fixture.repo.pool())
        .await
        .expect("event count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn crash_during_completion_rolls_back_attempt_and_request_then_replays_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    claim(&fixture).await;
    sqlx::query(
        "CREATE TRIGGER inject_completion_crash BEFORE UPDATE OF state ON execution_requests \
         WHEN NEW.state = 'succeeded' BEGIN SELECT RAISE(ABORT, 'injected completion crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");
    let completion = || Completion {
        runner_id: "runner-crash",
        attempt_id: "attempt-crash",
        fencing_token: 1,
        completion_id: "completion-1",
        terminal_state: "succeeded",
        terminal_reason: "completed",
        actual_execution: "{}",
        usage: "{}",
    };

    assert!(
        fixture
            .repo
            .complete_execution(completion(), &fixture.clock)
            .await
            .is_err()
    );
    let row = sqlx::query(
        "SELECT a.state AS attempt_state, a.completion_id, r.state AS request_state \
         FROM execution_attempts a JOIN execution_requests r ON r.id = a.request_id \
         WHERE a.id = 'attempt-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("terminal state");
    assert_eq!(row.get::<String, _>("attempt_state"), "leased");
    assert_eq!(row.get::<Option<String>, _>("completion_id"), None);
    assert_eq!(row.get::<String, _>("request_state"), "leased");

    sqlx::query("DROP TRIGGER inject_completion_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    assert!(
        fixture
            .repo
            .complete_execution(completion(), &fixture.clock)
            .await
            .expect("completion")
    );
    assert!(
        fixture
            .repo
            .complete_execution(completion(), &fixture.clock)
            .await
            .expect("completion replay")
    );
    let terminal_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts WHERE completion_id = 'completion-1' AND state = 'succeeded'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("terminal count");
    assert_eq!(terminal_rows, 1);
}

#[tokio::test]
async fn crash_during_cancellation_request_is_retryable_without_false_terminal_state() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    claim(&fixture).await;
    sqlx::query(
        "CREATE TRIGGER inject_cancel_crash BEFORE UPDATE OF cancellation_requested_at \
         ON execution_requests BEGIN SELECT RAISE(ABORT, 'injected cancellation crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");

    assert!(
        fixture
            .repo
            .request_execution_cancellation("request-crash", &fixture.clock)
            .await
            .is_err()
    );
    let row = sqlx::query(
        "SELECT state, cancellation_requested_at FROM execution_requests WHERE id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("cancellation state");
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert_eq!(
        row.get::<Option<String>, _>("cancellation_requested_at"),
        None
    );

    sqlx::query("DROP TRIGGER inject_cancel_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    assert!(
        fixture
            .repo
            .request_execution_cancellation("request-crash", &fixture.clock)
            .await
            .expect("cancellation retry")
    );
    let row = sqlx::query(
        "SELECT state, cancellation_requested_at FROM execution_requests WHERE id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("cancellation state after retry");
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert!(
        row.get::<Option<String>, _>("cancellation_requested_at")
            .is_some(),
        "requesting cancellation must not pretend the attempt is terminal"
    );
}
