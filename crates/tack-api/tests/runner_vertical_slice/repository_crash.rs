use std::sync::Mutex;
use tack_protocol::Clock;
use tack_protocol::execution::{
    AttemptId, ClaimRequestId, ExecutionRequestId, FencingToken, RecoveryKey, RunnerId,
};

use crate::common;
use chrono::{DateTime, Duration, Utc};
use tack_core::models::{CreateItem, CreateProject, ItemType, Priority, ProjectType};
use tack_db::{
    Repository, StoreError, queries,
    repo::execution::{
        ClaimedExecution, Completion, EnrollmentToken, EventApplyResult, EventBatch,
        HeartbeatBatchResult, HeartbeatLease, NewAgentProfile, NewEvent, NewExecutionRequest,
        NewRunner, RecoveryDisposition, RecoveryObservation, RecoveryObservationInput,
        RecoveryObservationResult, RedeemEnrollmentResult, RequestSelection,
    },
};
use tack_protocol::execution::ExecutionState;
use tack_test_support::{create_test_workspace, ids, seed, setup_test_db};

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .expect("fixed timestamp")
                .with_timezone(&Utc),
        ))
    }
}

impl Clock for FakeClock {
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
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: "Crash Matrix".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
                code_origin: None,
                repository: None,
                default_branch: None,
                workspace_mode: None,
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
                recurrence: None,
                recurrence_of: None,
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
                source_artifact_id: None,
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
        clock.now(),
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
        clock.now(),
    )
    .await
    .expect("profile");
    Fixture {
        repo,
        item_id: item.id.to_string(),
        clock,
    }
}

fn request<'a>(item_id: &'a str, request_snapshot: &'a str) -> NewExecutionRequest<'a> {
    NewExecutionRequest {
        id: "request-crash",
        item_id,
        idempotency_scope: "crash-matrix",
        idempotency_key: "request-1",
        request_fingerprint: "request-1-fingerprint",
        selector_kind: "exact_runner",
        selector_id: "runner-crash",
        agent_profile_id: Some("profile-crash"),
        agent_profile_snapshot: r#"{"name":"Crash Matrix Profile","instructions":"exercise failure boundaries","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
        requested_harness_kind: Some("mock"),
        requested_model: None,
        repository_snapshot: r#"{"kind":"git","remote":"https://example.invalid/repository.git","base_revision":"0123456789abcdef","subdirectory":null}"#,
        permission_policy: r#"{"tools":[],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

fn request_snapshot(item_id: &str, clock: &FakeClock) -> String {
    serde_json::json!({
        "request_id": "request-crash",
        "item_id": item_id,
        "idempotency_key": "request-1",
        "agent_profile_id": "profile-crash",
        "requested_harness_kind": "mock",
        "requested_model": null,
        "created_by": {"source": "test", "subject_id": "crash-matrix"},
        "created_at": clock.now().to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": "runner-crash"},
        "resolved_agent_profile": {
            "name": "Crash Matrix Profile", "instructions": "exercise failure boundaries",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "repository": {
            "kind": "git", "remote": "https://example.invalid/repository.git",
            "base_revision": "0123456789abcdef", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": {}
    })
    .to_string()
}

async fn enqueue(fixture: &Fixture) {
    let snapshot = request_snapshot(&fixture.item_id, &fixture.clock);
    fixture
        .repo
        .enqueue_execution(request(&fixture.item_id, &snapshot), fixture.clock.now())
        .await
        .expect("enqueue");
}

/// The state the fixed `request-crash` request is in.
async fn request_state(fixture: &Fixture) -> ExecutionState {
    queries::execution_requests::request_state(&fixture.repo, "request-crash")
        .await
        .expect("request state")
        .expect("request row")
}

/// The state the fixed `attempt-crash` attempt is in.
async fn attempt_state(fixture: &Fixture) -> ExecutionState {
    queries::execution_attempts::attempt_state(&fixture.repo, "attempt-crash")
        .await
        .expect("attempt state")
        .expect("attempt row")
}

/// `claim_execution_idempotent_with_snapshot` against the fixed `runner-crash`, varying
/// only the claim/attempt ids so a test can claim more than once without hard-coding the
/// other four arguments at every call site.
async fn try_claim(
    fixture: &Fixture,
    claim_request_id: &str,
    attempt_id: &str,
) -> Result<Option<ClaimedExecution>, StoreError> {
    fixture
        .repo
        .claim_execution_idempotent_with_snapshot(
            &RunnerId::new("runner-crash"),
            &ClaimRequestId::new(claim_request_id),
            &AttemptId::new(attempt_id),
            Duration::seconds(60),
            fixture.clock.now(),
            RequestSelection::Naive,
        )
        .await
}

#[tokio::test]
async fn claim_crash_rolls_back_then_retry_keeps_fence_at_one() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    seed::failure_trigger(
        &fixture.repo,
        "inject_claim_crash",
        "execution_attempts",
        "INSERT",
        None,
        "injected claim crash",
    )
    .await;

    let crashed = try_claim(&fixture, "claim-crash", "attempt-crash").await;
    assert!(crashed.is_err(), "fault injection must reach claim commit");

    let capacity = queries::agent_runners::available_capacity(&fixture.repo, "runner-crash")
        .await
        .expect("runner capacity");
    let attempts =
        queries::execution_attempts::attempt_count_of_request(&fixture.repo, "request-crash")
            .await
            .expect("attempt count");
    assert_eq!(request_state(&fixture).await, ExecutionState::Queued);
    assert_eq!(capacity, Some(1));
    assert_eq!(attempts, 0);

    seed::drop_trigger(&fixture.repo, "inject_claim_crash").await;
    let lease = try_claim(&fixture, "claim-after-crash", "attempt-after-retry")
        .await
        .expect("claim retry")
        .expect("lease after retry");
    assert_eq!(
        lease.lease.fencing_token,
        FencingToken::new(1),
        "rollback cannot burn a fence"
    );
}

/// Records a recovery observation for the fixed `attempt-crash`/fence 1.
async fn recover(
    fixture: &Fixture,
    observation: RecoveryObservation,
    recovery_key: &'static str,
    details: &'static str,
) -> RecoveryObservationResult {
    fixture
        .repo
        .recover_attempt(
            RecoveryObservationInput {
                runner_id: &RunnerId::new("runner-crash"),
                attempt_id: &AttemptId::new("attempt-crash"),
                fencing_token: FencingToken::new(1),
                recovery_key: &RecoveryKey::new(recovery_key),
                observation,
                details,
            },
            fixture.clock.now(),
        )
        .await
        .expect("record recovery")
}

#[tokio::test]
async fn ambiguous_recovery_needs_operator_and_blocks_reclaim() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let recovery = recover(
        &fixture,
        RecoveryObservation::Ambiguous,
        "recovery:attempt-crash:1:ambiguous",
        r#"{"journal_state":"process_observed_running","process_observed":true}"#,
    )
    .await;
    assert!(matches!(
        recovery,
        RecoveryObservationResult::Applied(ref response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    assert_eq!(attempt_state(&fixture).await, ExecutionState::NeedsOperator);
    assert!(
        try_claim(
            &fixture,
            "claim-invalid-second-fence",
            "attempt-invalid-second-fence"
        )
        .await
        .expect("second claim query")
        .is_none(),
        "ambiguous side effects must never be blind-retried"
    );
    let fences =
        queries::execution_attempts::fence_count_of_request(&fixture.repo, "request-crash")
            .await
            .expect("fence count");
    assert_eq!(fences, 1);
}

/// Fixed two-event batch shared by the event-batch tests below; only the crash trigger
/// (or its absence) varies between them.
fn crash_events(fixture: &Fixture) -> [NewEvent<'static>; 2] {
    [
        NewEvent {
            id: "event-row-1",
            event_id: "event-1",
            sequence: 1,
            source: "runner",
            kind: "progress",
            payload: r#"{"phase":"spawn"}"#,
            occurred_at: fixture.clock.now(),
        },
        NewEvent {
            id: "event-row-2",
            event_id: "event-2",
            sequence: 2,
            source: "runner",
            kind: "progress",
            payload: r#"{"phase":"running"}"#,
            occurred_at: fixture.clock.now(),
        },
    ]
}

fn crash_event_batch() -> EventBatch<'static> {
    EventBatch {
        runner_id: ids::runner("runner-crash"),
        attempt_id: ids::attempt("attempt-crash"),
        fencing_token: FencingToken::new(1),
        previous_checkpoint: None,
        checkpoint: ids::checkpoint("checkpoint-1"),
    }
}

/// The `attempt-crash` event checkpoint and how many events are stored.
async fn event_checkpoint_state(fixture: &Fixture) -> (Option<String>, i64) {
    let checkpoint = queries::execution_attempts::event_checkpoint(&fixture.repo, "attempt-crash")
        .await
        .expect("checkpoint state");
    let events = queries::execution_events::event_count_of_attempt(&fixture.repo, "attempt-crash")
        .await
        .expect("event count");
    (checkpoint, events)
}

/// One crash-injection point for the event-batch fault matrix: the trigger that fires there,
/// removed once the rolled-back state has been checked.
struct EventBatchFault {
    label: &'static str,
    trigger: &'static str,
    table: &'static str,
    event: &'static str,
    when: Option<&'static str>,
    message: &'static str,
}

const EVENT_BATCH_FAULTS: &[EventBatchFault] = &[
    EventBatchFault {
        label: "second_event_insert",
        trigger: "inject_second_event_crash",
        table: "execution_events",
        event: "INSERT",
        when: Some("NEW.sequence = 2"),
        message: "injected second event crash",
    },
    EventBatchFault {
        label: "checkpoint_update",
        trigger: "inject_checkpoint_crash",
        table: "execution_attempts",
        event: "UPDATE OF event_checkpoint",
        when: None,
        message: "injected checkpoint crash",
    },
];

async fn assert_event_batch_fault_rolls_back(fault: &EventBatchFault) {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    seed::failure_trigger(
        &fixture.repo,
        fault.trigger,
        fault.table,
        fault.event,
        fault.when,
        fault.message,
    )
    .await;
    let events = crash_events(&fixture);
    assert!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, fixture.clock.now())
            .await
            .is_err(),
        "{}",
        fault.label
    );
    let (checkpoint, event_count) = event_checkpoint_state(&fixture).await;
    assert_eq!(checkpoint, None, "{}", fault.label);
    assert_eq!(event_count, 0, "{}", fault.label);
    seed::drop_trigger(&fixture.repo, fault.trigger).await;
}

#[tokio::test]
async fn event_batch_fault_leaves_no_partial_write() {
    for fault in EVENT_BATCH_FAULTS {
        assert_event_batch_fault_rolls_back(fault).await;
    }
}

#[tokio::test]
async fn event_batch_replay_writes_events_exactly_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let events = crash_events(&fixture);
    assert!(matches!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, fixture.clock.now())
            .await
            .expect("first report"),
        EventApplyResult::Applied(ref result) if !result.replayed
    ));
    assert!(matches!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, fixture.clock.now())
            .await
            .expect("replay"),
        EventApplyResult::Applied(ref result) if result.replayed
    ));
    let (_, count) = event_checkpoint_state(&fixture).await;
    assert_eq!(count, 2);
}

fn completion_crash() -> Completion<'static> {
    Completion {
        runner_id: ids::runner("runner-crash"),
        attempt_id: ids::attempt("attempt-crash"),
        fencing_token: FencingToken::new(1),
        completion_id: ids::completion("completion-1"),
        final_event_checkpoint: None,
        terminal_state: tack_protocol::execution::ExecutionState::Succeeded,
        terminal_reason: "completed",
        actual_execution: "{}",
        usage: "{}",
    }
}

#[tokio::test]
async fn completion_crash_rolls_back_attempt_and_request() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    seed::failure_trigger(
        &fixture.repo,
        "inject_completion_crash",
        "execution_requests",
        "UPDATE OF state",
        Some("NEW.state = 'succeeded'"),
        "injected completion crash",
    )
    .await;

    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), fixture.clock.now())
            .await
            .is_err()
    );
    let completion_id =
        queries::execution_attempts::completion_id_of_attempt(&fixture.repo, "attempt-crash")
            .await
            .expect("terminal state");
    assert_eq!(attempt_state(&fixture).await, ExecutionState::Leased);
    assert_eq!(completion_id, None);
    assert_eq!(request_state(&fixture).await, ExecutionState::Leased);
}

#[tokio::test]
async fn completion_replay_is_idempotent_and_terminal_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), fixture.clock.now())
            .await
            .expect("completion")
    );
    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), fixture.clock.now())
            .await
            .expect("completion replay")
    );
    let terminal_rows = queries::execution_attempts::count_completed_with(
        &fixture.repo,
        "completion-1",
        ExecutionState::Succeeded,
    )
    .await
    .expect("terminal count");
    assert_eq!(terminal_rows, 1);
}

/// Requests cancellation of the fixed `request-crash` request.
async fn request_cancellation(fixture: &Fixture) -> Result<bool, StoreError> {
    fixture
        .repo
        .request_execution_cancellation(
            &ExecutionRequestId::new("request-crash"),
            fixture.clock.now(),
        )
        .await
}

/// The request-crash row's `(state, cancellation_requested_at)`.
async fn cancellation_state(fixture: &Fixture) -> (ExecutionState, Option<String>) {
    let detail = queries::execution_requests::request_detail(&fixture.repo, "request-crash")
        .await
        .expect("cancellation state")
        .expect("request row");
    (detail.state, detail.cancellation_requested_at)
}

#[tokio::test]
async fn cancellation_crash_retries_without_false_terminal_state() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    seed::failure_trigger(
        &fixture.repo,
        "inject_cancel_crash",
        "execution_requests",
        "UPDATE OF cancellation_requested_at",
        None,
        "injected cancellation crash",
    )
    .await;

    assert!(request_cancellation(&fixture).await.is_err());
    let (state, cancelled_at) = cancellation_state(&fixture).await;
    assert_eq!(state, ExecutionState::Leased);
    assert_eq!(cancelled_at, None);

    seed::drop_trigger(&fixture.repo, "inject_cancel_crash").await;
    assert!(
        request_cancellation(&fixture)
            .await
            .expect("cancellation retry")
    );
    let (state, cancelled_at) = cancellation_state(&fixture).await;
    assert_eq!(state, ExecutionState::Leased);
    assert!(
        cancelled_at.is_some(),
        "requesting cancellation must not pretend the attempt is terminal"
    );
}

fn pending_enroll_runner() -> NewRunner<'static> {
    NewRunner {
        id: "runner-enroll-crash",
        name: "Pending crash runner",
        credential_hash: "ignored-for-pending",
        labels: "{}",
        total_capacity: 1,
        available_capacity: 1,
        capability_snapshot: "{}",
        protocol_version: 1,
    }
}

fn pending_enroll_token(clock: &FakeClock) -> EnrollmentToken<'static> {
    EnrollmentToken {
        id: "token-enroll-crash",
        runner_id: "runner-enroll-crash",
        token_hash: "hash:enroll-crash",
        expires_at: clock.now() + Duration::minutes(5),
    }
}

/// Redeems the fixed `hash:enroll-crash` enrollment token.
async fn redeem(fixture: &Fixture) -> Result<RedeemEnrollmentResult, tack_db::StoreError> {
    fixture
        .repo
        .redeem_enrollment_token(
            "hash:enroll-crash",
            "credential-hash-only",
            fixture.clock.now() + Duration::hours(1),
            "test-runner",
            "Enrolled crash runner",
            "{}",
            1,
            1,
            "{}",
            1,
            fixture.clock.now(),
        )
        .await
}

#[tokio::test]
async fn enrollment_redeem_has_one_winner_and_keeps_hash_only_token() {
    let fixture = fixture().await;
    fixture
        .repo
        .create_pending_runner_and_issue_token(
            pending_enroll_runner(),
            pending_enroll_token(&fixture.clock),
            fixture.clock.now(),
        )
        .await
        .expect("pending runner and token");
    let (left, right) = tokio::join!(redeem(&fixture), redeem(&fixture));
    let results = [
        left.expect("left redemption"),
        right.expect("right redemption"),
    ];
    let winners = results
        .iter()
        .filter(|r| matches!(r, RedeemEnrollmentResult::Redeemed(_)))
        .count();
    assert_eq!(
        winners, 1,
        "only one concurrent redemption can consume the token"
    );

    let metadata = fixture
        .repo
        .enrollment_token_metadata("runner-enroll-crash", "token-enroll-crash")
        .await
        .expect("token metadata")
        .expect("token row");
    assert!(metadata.consumed_at.is_some());
    let token_rows =
        queries::agent_enrollment_tokens::count_with_hash(&fixture.repo, "hash:enroll-crash")
            .await
            .expect("token rows");
    assert_eq!(token_rows, 1);
}

fn crash_heartbeat_lease() -> [HeartbeatLease<'static>; 1] {
    [HeartbeatLease {
        attempt_id: ids::attempt("attempt-crash"),
        fencing_token: FencingToken::new(1),
        state: ExecutionState::Running,
        journal_state: "process_observed_running",
        last_event_checkpoint: None,
    }]
}

/// `heartbeat_batch` against the fixed `runner-crash`/`hb-crash-1` lease, varying only the
/// observed timestamp — keeping call sites to two arguments dodges rustfmt exploding the
/// underlying 7-argument call onto one line per argument.
async fn crash_heartbeat(
    fixture: &Fixture,
    observed_at: DateTime<Utc>,
) -> Result<HeartbeatBatchResult, StoreError> {
    fixture
        .repo
        .heartbeat_batch(
            &RunnerId::new("runner-crash"),
            "hb-crash-1",
            observed_at,
            0,
            &crash_heartbeat_lease(),
            Duration::seconds(60),
            fixture.clock.now(),
        )
        .await
}

#[tokio::test]
async fn heartbeat_crash_rolls_back_and_writes_no_replay_row() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    seed::failure_trigger(
        &fixture.repo,
        "inject_heartbeat_crash",
        "execution_attempts",
        "UPDATE OF last_heartbeat_at",
        None,
        "injected heartbeat crash",
    )
    .await;
    assert!(
        crash_heartbeat(&fixture, fixture.clock.now())
            .await
            .is_err()
    );
    let replays = queries::execution_heartbeat_replays::replay_count(&fixture.repo)
        .await
        .expect("no failed replay row");
    assert_eq!(replays, 0);
}

#[tokio::test]
async fn heartbeat_replay_is_idempotent_then_conflicts_on_new_state() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let now = fixture.clock.now();
    let accepted = crash_heartbeat(&fixture, now).await.expect("heartbeat");
    let replay = crash_heartbeat(&fixture, now)
        .await
        .expect("heartbeat replay");
    assert!(matches!(accepted, HeartbeatBatchResult::Accepted(_)));
    assert!(matches!(replay, HeartbeatBatchResult::Replayed(_)));
    let conflict = crash_heartbeat(&fixture, now + Duration::seconds(1))
        .await
        .expect("heartbeat replay conflict");
    assert!(matches!(conflict, HeartbeatBatchResult::Conflict));
    let capacity = queries::agent_runners::available_capacity(&fixture.repo, "runner-crash")
        .await
        .expect("capacity after replay");
    assert_eq!(
        capacity,
        Some(0),
        "replay must not restore or double-write capacity"
    );
}
