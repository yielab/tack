//! Fixture for `execution_repo`'s exhaustive execution-domain repository
//! tests: one runner ("runner-a"), one profile ("profile-a"), one item, and
//! a clock every test drives explicitly instead of reading real time. Its
//! short methods exist so a repeated call (claim, heartbeat, a
//! `SELECT COUNT(*)`) collapses to one line instead of rustfmt stacking
//! every argument of the underlying repository call one per line.

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use tack_db::repo::execution::{
    AttemptTransitionInput, AttemptTransitionResult, CancellationObservation,
    CancellationObservationInput, ClaimedExecution, Completion, CompletionResult,
    CredentialRotationResult, EnqueueResult, EnrollmentToken, EventApplyResult, EventBatch,
    ExecutionClock, HeartbeatBatchResult, HeartbeatLease, Lease, NewAgentProfile, NewArtifact,
    NewDecision, NewEvent, NewExecutionRequest, NewRunner, OperatorRequeueResult,
    RecoveryObservation, RecoveryObservationInput, RecoveryObservationResult,
    RedeemEnrollmentResult, RequestSelection,
};
use tack_db::{Repository, init_pool, migrations};

use super::{create_test_workspace, make_item, make_project, setup_test_db};

/// A shared, settable point in time implementing [`ExecutionClock`] — this
/// crate's execution repository takes its clock as `&dyn ExecutionClock`, so
/// the domain-agnostic `tack_test_support::ControllableClock` can't stand in
/// for it directly.
pub struct FakeClock(Mutex<DateTime<Utc>>);

#[allow(dead_code)] // not every tack-db test binary drives the clock forward
impl FakeClock {
    pub fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ))
    }

    pub fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }

    pub fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[allow(dead_code)] // not every tack-db test binary exercises the execution-domain repo
pub struct Fixture {
    pub repo: Repository,
    pub item_id: String,
    pub clock: FakeClock,
}

#[allow(dead_code)]
impl Fixture {
    /// One workspace, project, item, runner and agent profile on a shared
    /// in-memory database.
    pub async fn new() -> Self {
        Self::seeded(setup_test_db().await).await
    }

    /// Like [`Self::new`], on a file-backed database. SQLite's shared-cache
    /// mode (what a pooled `:memory:` database uses) imposes table-level
    /// locking that accidentally serializes a concurrent SELECT-then-INSERT
    /// gap a test may need to expose; a file-backed pool does not. Returns
    /// the `TempDir` guard alongside the fixture — dropping it deletes the
    /// database.
    pub async fn file_backed() -> (Self, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temporary directory");
        let db_path = dir.path().join("fixture.db");
        let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .expect("file-backed pool");
        migrations::run_all(&pool).await.expect("migrations");
        (Self::seeded(Repository::new(pool)).await, dir)
    }

    async fn seeded(repo: Repository) -> Self {
        let workspace = create_test_workspace(&repo).await;
        let project = make_project(&repo, workspace).await;
        let item = make_item(&repo, &project).await;
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
        Self {
            repo,
            item_id: item.id.to_string(),
            clock,
        }
    }

    /// A `NewExecutionRequest` against this fixture's item.
    pub fn request<'a>(
        &'a self,
        id: &'a str,
        key: &'a str,
        fingerprint: &'a str,
    ) -> NewExecutionRequest<'a> {
        request(id, &self.item_id, key, fingerprint)
    }

    /// Enqueues a request built by [`Self::request`].
    pub async fn enqueue(&self, id: &str, key: &str, fingerprint: &str) -> EnqueueResult {
        self.enqueue_result(id, key, fingerprint).await.unwrap()
    }

    /// Like [`Self::enqueue`], keeping the sqlx-level `Result` — for the
    /// concurrent-duplicate-enqueue test.
    pub async fn enqueue_result(
        &self,
        id: &str,
        key: &str,
        fingerprint: &str,
    ) -> Result<EnqueueResult, sqlx::Error> {
        self.repo
            .enqueue_execution(self.request(id, key, fingerprint), &self.clock)
            .await
    }

    /// Claims `attempt_id` for "runner-a", the fixture's sole runner.
    pub async fn claim(&self, attempt_id: &str, lease_duration: Duration) -> Option<Lease> {
        self.claim_result(attempt_id, lease_duration).await.unwrap()
    }

    /// Like [`Self::claim`], keeping the sqlx-level `Result` — for the
    /// concurrent-claimer tests, which assert both racing branches succeed
    /// at the sqlx level before checking which one won the lease.
    pub async fn claim_result(
        &self,
        attempt_id: &str,
        lease_duration: Duration,
    ) -> Result<Option<Lease>, sqlx::Error> {
        self.claim_raw_result(attempt_id, attempt_id, lease_duration)
            .await
            .map(|claim| claim.map(|claim| claim.lease))
    }

    /// Like [`Self::claim`], keeping the full `ClaimedExecution` (including
    /// its frozen request snapshot) and letting `claim_request_id` and
    /// `attempt_id` differ — for the handful of tests distinguishing claim
    /// replay (same `claim_request_id`) from a fresh attempt.
    pub async fn claim_raw(
        &self,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
    ) -> Option<ClaimedExecution> {
        self.claim_raw_result(claim_request_id, attempt_id, lease_duration)
            .await
            .unwrap()
    }

    async fn claim_raw_result(
        &self,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedExecution>, sqlx::Error> {
        self.repo
            .claim_execution_idempotent_with_snapshot(
                "runner-a",
                claim_request_id,
                attempt_id,
                lease_duration,
                &self.clock,
                RequestSelection::Naive,
            )
            .await
    }

    /// Enqueues `request_id` (as its own idempotency key) and claims it as
    /// `attempt_id` with a 60s lease; returns the fencing token most
    /// completion/recovery/cancellation tests key off of.
    pub async fn ready_completion_attempt(&self, request_id: &str, attempt_id: &str) -> i64 {
        self.enqueue(request_id, request_id, "same").await;
        self.claim(attempt_id, Duration::seconds(60))
            .await
            .unwrap()
            .fencing_token
    }

    /// A "runner-a" heartbeat for `leases`, sent now, with no missed count
    /// and the fixture's default 60s lease duration.
    pub async fn heartbeat(&self, id: &str, leases: &[HeartbeatLease<'_>]) -> HeartbeatBatchResult {
        self.heartbeat_full(id, self.clock.now(), 0, leases, Duration::seconds(60))
            .await
    }

    /// Like [`Self::heartbeat`], with every field a test may need to vary.
    pub async fn heartbeat_full(
        &self,
        id: &str,
        sent_at: DateTime<Utc>,
        missed: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
    ) -> HeartbeatBatchResult {
        self.heartbeat_result(id, sent_at, missed, leases, lease_duration)
            .await
            .unwrap()
    }

    /// Like [`Self::heartbeat_full`], keeping the sqlx-level `Result` — for
    /// the concurrent-duplicate-heartbeat test.
    pub async fn heartbeat_result(
        &self,
        id: &str,
        sent_at: DateTime<Utc>,
        missed: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
    ) -> Result<HeartbeatBatchResult, sqlx::Error> {
        self.repo
            .heartbeat_batch(
                "runner-a",
                id,
                sent_at,
                missed,
                leases,
                lease_duration,
                &self.clock,
            )
            .await
    }

    /// A `HeartbeatLease` for `attempt_id`/`fencing_token` in the common
    /// "leased, prepared, no checkpoint" shape; override one field with
    /// struct-update syntax (`HeartbeatLease { state: "running", ..fx.lease(a,
    /// f) }`).
    pub fn lease<'a>(&self, attempt_id: &'a str, fencing_token: i64) -> HeartbeatLease<'a> {
        HeartbeatLease {
            attempt_id,
            fencing_token,
            state: "leased",
            journal_state: "prepared",
            last_event_checkpoint: None,
        }
    }

    /// Issues an enrollment token for "runner-a" expiring in 5 minutes.
    pub async fn issue_token(&self, id: &str, token_hash: &str) {
        self.repo
            .issue_enrollment_token(
                EnrollmentToken {
                    id,
                    runner_id: "runner-a",
                    token_hash,
                    expires_at: self.clock.now() + Duration::minutes(5),
                },
                &self.clock,
            )
            .await
            .unwrap();
    }

    /// Redeems an enrollment token with the fixture's default runner
    /// version/labels/capacity, which the enrollment tests don't vary.
    pub async fn redeem(
        &self,
        token_hash: &str,
        credential_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> RedeemEnrollmentResult {
        self.repo
            .redeem_enrollment_token(
                token_hash,
                credential_hash,
                expires_at,
                "0.1",
                "Runner A",
                "{}",
                1,
                1,
                "{}",
                1,
                &self.clock,
            )
            .await
            .unwrap()
    }

    /// Applies an event batch, unwrapping the sqlx-level result.
    pub async fn apply_events(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
    ) -> EventApplyResult {
        self.apply_events_result(batch, events).await.unwrap()
    }

    /// Like [`Self::apply_events`], keeping the sqlx-level `Result` — for
    /// the concurrent-duplicate-event-batch test.
    pub async fn apply_events_result(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
    ) -> Result<EventApplyResult, sqlx::Error> {
        self.repo
            .append_execution_events_result(batch, events, &self.clock)
            .await
    }

    /// Requests cancellation of `request_id`.
    pub async fn request_cancellation(&self, request_id: &str) -> bool {
        self.repo
            .request_execution_cancellation(request_id, &self.clock)
            .await
            .unwrap()
    }

    /// Observes a cancellation, unwrapping the sqlx-level result.
    pub async fn observe_cancellation(
        &self,
        input: CancellationObservationInput<'_>,
    ) -> CancellationObservation {
        self.observe_cancellation_result(input).await.unwrap()
    }

    /// Like [`Self::observe_cancellation`], keeping the sqlx-level `Result`
    /// — for the concurrent-duplicate-cancellation test.
    pub async fn observe_cancellation_result(
        &self,
        input: CancellationObservationInput<'_>,
    ) -> Result<CancellationObservation, sqlx::Error> {
        self.repo.observe_cancellation(input, &self.clock).await
    }

    /// Reports a recovery observation, unwrapping the sqlx-level result.
    pub async fn recover(&self, input: RecoveryObservationInput<'_>) -> RecoveryObservationResult {
        self.recover_result(input).await.unwrap()
    }

    /// Like [`Self::recover`], keeping the sqlx-level `Result` — for the
    /// concurrent-duplicate-recovery test.
    pub async fn recover_result(
        &self,
        input: RecoveryObservationInput<'_>,
    ) -> Result<RecoveryObservationResult, sqlx::Error> {
        self.repo.recover_attempt(input, &self.clock).await
    }

    /// An operator requeue of `request_id` with the fixture's fixed
    /// client/operator/reason strings, which the requeue tests don't vary.
    pub async fn operator_requeue(&self, request_id: &str) -> OperatorRequeueResult {
        self.operator_requeue_result(request_id).await.unwrap()
    }

    /// Like [`Self::operator_requeue`], keeping the sqlx-level `Result` —
    /// for the concurrent-duplicate-requeue test.
    pub async fn operator_requeue_result(
        &self,
        request_id: &str,
    ) -> Result<OperatorRequeueResult, sqlx::Error> {
        self.repo
            .operator_requeue_needs_operator(
                request_id,
                "client-key",
                "operator-a",
                "reason-a",
                &self.clock,
            )
            .await
    }

    /// Applies an attempt transition, unwrapping the sqlx-level result.
    pub async fn transition(&self, input: AttemptTransitionInput<'_>) -> AttemptTransitionResult {
        self.transition_result(input).await.unwrap()
    }

    /// Like [`Self::transition`], keeping the sqlx-level `Result` — for the
    /// concurrent-duplicate-transition test.
    pub async fn transition_result(
        &self,
        input: AttemptTransitionInput<'_>,
    ) -> Result<AttemptTransitionResult, sqlx::Error> {
        self.repo
            .transition_attempt_with_facts(input, &self.clock)
            .await
    }

    /// Reports a completion, unwrapping the sqlx-level result.
    pub async fn complete(&self, completion: Completion<'_>) -> CompletionResult {
        self.complete_result(completion).await.unwrap()
    }

    /// Like [`Self::complete`], keeping the sqlx-level `Result` — for the
    /// concurrent-duplicate-completion test.
    pub async fn complete_result(
        &self,
        completion: Completion<'_>,
    ) -> Result<CompletionResult, sqlx::Error> {
        self.repo
            .complete_execution_result(completion, &self.clock)
            .await
    }

    /// The older, non-idempotency-tracked completion path (`true` if the
    /// attempt was still open to complete), for the one test proving it
    /// shares the fencing/terminal-lock invariants with [`Self::complete`].
    pub async fn complete_raw(&self, completion: Completion<'_>) -> bool {
        self.repo
            .complete_execution(completion, &self.clock)
            .await
            .unwrap()
    }

    /// Rotates "runner-a"'s credential, keeping the sqlx-level `Result` —
    /// for the concurrent-credential-rotation test, which races two rotation
    /// attempts against the same expected hash.
    pub async fn rotate_credential(
        &self,
        expected_hash: &str,
        new_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<CredentialRotationResult, sqlx::Error> {
        self.repo
            .rotate_runner_credential("runner-a", expected_hash, new_hash, expires_at, &self.clock)
            .await
    }

    /// Two clones of this fixture's repository, for a `tokio::join!` race —
    /// both share this fixture's one clock.
    pub fn pair(&self) -> (Repository, Repository) {
        (self.repo.clone(), self.repo.clone())
    }

    /// Records an artifact on `attempt_id`, `artifact_id` doubling as its
    /// row id (each test records at most one).
    pub async fn record_artifact(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        artifact_id: &str,
    ) -> bool {
        self.record_artifact_result(attempt_id, fencing_token, artifact_id)
            .await
            .unwrap()
    }

    /// Like [`Self::record_artifact`], keeping the sqlx-level `Result` — for
    /// the one test racing this write against a concurrent terminal
    /// transition, which asserts the write itself never errors.
    pub async fn record_artifact_result(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        artifact_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.repo
            .record_execution_artifact(
                "runner-a",
                attempt_id,
                fencing_token,
                new_artifact(artifact_id, artifact_id),
                &self.clock,
            )
            .await
    }

    /// Creates a decision on `attempt_id`, `decision_id` doubling as its row
    /// id (each test creates at most one).
    pub async fn create_decision(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        decision_id: &str,
    ) -> bool {
        self.create_decision_result(attempt_id, fencing_token, decision_id)
            .await
            .unwrap()
    }

    /// Like [`Self::create_decision`], keeping the sqlx-level `Result` — see
    /// [`Self::record_artifact_result`].
    pub async fn create_decision_result(
        &self,
        attempt_id: &str,
        fencing_token: i64,
        decision_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.repo
            .create_execution_decision(
                "runner-a",
                attempt_id,
                fencing_token,
                new_decision(decision_id, decision_id),
                &self.clock,
            )
            .await
    }

    /// `COUNT(*)` of rows in `table` whose `attempt_id` column equals
    /// `attempt_id` — the shape every replay/audit table in this domain
    /// shares.
    pub async fn count(&self, table: &str, attempt_id: &str) -> i64 {
        self.count_by(table, "attempt_id", attempt_id).await
    }

    /// `COUNT(*)` of rows in `table` whose `column` equals `value`, for the
    /// handful of tables keyed by something other than `attempt_id`.
    pub async fn count_by(&self, table: &str, column: &str, value: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE {column} = ?"
        )))
        .bind(value)
        .fetch_one(self.repo.pool())
        .await
        .unwrap()
    }

    /// `agent_runners.available_capacity` for "runner-a".
    pub async fn capacity(&self) -> i64 {
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    /// `execution_attempts.state` for `attempt_id`.
    pub async fn attempt_state(&self, attempt_id: &str) -> String {
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }

    /// `execution_requests.state` for `request_id`.
    pub async fn request_state(&self, request_id: &str) -> String {
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_one(self.repo.pool())
            .await
            .unwrap()
    }
}

/// A well-formed `NewExecutionRequest`: `id`, `item_id`, `key` and
/// `fingerprint` are the only fields that vary across tests, every other
/// field (selector, profile snapshot, budgets, ...) a fixed valid value.
#[allow(dead_code)] // exercised through Fixture::request by every execution_repo test
pub fn request<'a>(
    id: &'a str,
    item_id: &'a str,
    key: &'a str,
    fingerprint: &'a str,
) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"operator","subject_id":"test"}},"created_at":"2026-08-07T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"runner-a"}},"agent_profile_id":"profile-a","resolved_agent_profile":{{"name":"Profile A","instructions":"test","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{"limit":1}}}},"requested_harness_kind":"codex","requested_model_provider":"openai","requested_model_id":"opaque/model","repository":{{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":["shell"],"network":false}},"timeout_seconds":60,"budgets":{{"limit":1}},"status_map_policy_id":null,"environment":{{"MODE":{{"value":"test","secret_reference":null}}}},"metadata":{{"source":"test"}}}}"#
        )
        .into_boxed_str(),
    );
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: fingerprint,
        selector_kind: "exact_runner",
        selector_id: "runner-a",
        agent_profile_id: Some("profile-a"),
        agent_profile_snapshot: r#"{"name":"Profile A","instructions":"test","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"limit":1}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: Some("openai"),
        requested_model_id: Some("opaque/model"),
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":["shell"],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: r#"{"limit":1}"#,
        status_map_policy_id: None,
        environment: r#"{"MODE":{"value":"test","secret_reference":null}}"#,
        metadata: r#"{"source":"test"}"#,
        request_snapshot,
    }
}

/// A `NewArtifact` in the fixture's default "log" shape.
#[allow(dead_code)] // exercised through Fixture::record_artifact
pub fn new_artifact<'a>(row_id: &'a str, artifact_id: &'a str) -> NewArtifact<'a> {
    NewArtifact {
        id: row_id,
        artifact_id,
        kind: "log",
        name: "log",
        media_type: None,
        size_bytes: 0,
        sha256: "hash",
        content_disposition: None,
        content_reference: None,
        metadata: "{}",
    }
}

/// A `NewDecision` in the fixture's default "approval" shape.
#[allow(dead_code)] // exercised through Fixture::create_decision
pub fn new_decision<'a>(row_id: &'a str, decision_id: &'a str) -> NewDecision<'a> {
    NewDecision {
        id: row_id,
        decision_id,
        kind: "approval",
        prompt: "Continue?",
        options: "[]",
        metadata: "{}",
        expires_at: None,
    }
}

/// A `Completion` for "runner-a" with the fixture's default succeeded
/// terminal state/reason and result/usage JSON.
#[allow(dead_code)]
pub fn completion_input<'a>(
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

/// A `CancellationObservationInput` for "runner-a".
#[allow(dead_code)]
pub fn cancellation_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    cancellation_request_id: &'a str,
    observed_at: DateTime<Utc>,
) -> CancellationObservationInput<'a> {
    CancellationObservationInput {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        cancellation_request_id,
        observed_at,
        details: r#"{"detail":{"b":2,"a":1}}"#,
        observation: r#""process_stopped""#,
    }
}

#[allow(dead_code)]
pub fn expect_cancelled(
    obs: CancellationObservation,
) -> tack_db::repo::execution::CancellationResponse {
    match obs {
        CancellationObservation::Cancelled(r) => r,
        other => panic!("expected Cancelled, got {other:?}"),
    }
}

#[allow(dead_code)]
pub fn expect_replayed_cancellation(
    obs: CancellationObservation,
) -> tack_db::repo::execution::CancellationResponse {
    match obs {
        CancellationObservation::Replayed(r) => r,
        other => panic!("expected Replayed, got {other:?}"),
    }
}

/// A `RecoveryObservationInput` for "runner-a".
#[allow(dead_code)]
pub fn recovery_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    recovery_key: &'a str,
    observation: RecoveryObservation,
) -> RecoveryObservationInput<'a> {
    RecoveryObservationInput {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        recovery_key,
        observation,
        details: r#"{"journal_state":"prepared","process_observed":false,"observer":{"b":2,"a":1}}"#,
    }
}

/// How many of the two racing results in `items` satisfy `pred` — used by the
/// `concurrent_duplicate_*` tests to prove exactly one of a duplicate pair
/// committed while the other replayed.
#[allow(dead_code)]
pub fn count_where<T>(items: [&T; 2], pred: impl Fn(&T) -> bool) -> usize {
    items.into_iter().filter(|r| pred(r)).count()
}
