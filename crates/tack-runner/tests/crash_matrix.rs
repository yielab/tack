//! Crash-recovery matrix: for each point a runner attempt can crash (before
//! spawn, after spawn, mid-report, mid-recovery, on reoffer), proves the
//! engine's next `run_once`/`recover` call reaches the one correct outcome
//! without a duplicate spawn or a duplicate terminal report.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tack_protocol::execution::{
    AttemptId as DomainAttemptId, AttemptSnapshot, ExecutionRequestSnapshot, ProtocolVersion,
    RecoveryObservationRequest, RecoveryObservationResponse, RunnerId as DomainRunnerId,
};
use tack_runner::{
    EnrollmentCredential,
    client::{
        AttemptId, AttemptLease, AttemptState, CancelObservation, CancellationEvidence,
        CancellationReport, CancellationResponse, ClaimRequest, ClaimRequestId, ClaimResult,
        ClaimedWork, CompletionReport, CompletionResponse, EngineError, EnrollmentRequest,
        EnrollmentResponse, FencingToken, HarnessAdapter, HarnessError, HarnessOutcome,
        HeartbeatRequest, HeartbeatResponse, JournalError, JournalState, LeaseResult,
        LocalRunHandle, OwnerOnlyJournal, ProtocolClientError, PullProtocol, RecoveryObservation,
        RepositorySpec, RunCycle, RunnerCredential, RunnerEngine, RunnerId, RunnerSession,
        StartPhase, StartReport, Timestamp, Workspace, WorkspaceError, WorkspaceId,
        WorkspaceJournal, WorkspaceManager, WorktreeProvisioner,
    },
};

mod common;
use common::temp_dir as root;
mod usage_fixture;
use usage_fixture::usage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailurePoint {
    None,
    PreparingAck,
    ProcessStartAck,
    Heartbeat,
    Completion,
    Cancellation,
}

#[derive(Debug, Default)]
struct ProtocolEvidence {
    events: Vec<String>,
    completion_reports: usize,
    cancellation_reports: usize,
    recovery_reports: Vec<RecoveryObservation>,
}

#[derive(Clone)]
struct FakeProtocol {
    work: Arc<Mutex<Option<ClaimedWork>>>,
    failure: FailurePoint,
    cancellation_requested: bool,
    evidence: Arc<Mutex<ProtocolEvidence>>,
    recovery_failures_remaining: Arc<AtomicUsize>,
}

impl FakeProtocol {
    fn new(work: ClaimedWork, failure: FailurePoint, cancellation_requested: bool) -> Self {
        Self {
            work: Arc::new(Mutex::new(Some(work))),
            failure,
            cancellation_requested,
            evidence: Arc::new(Mutex::new(ProtocolEvidence::default())),
            recovery_failures_remaining: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn event(&self, event: impl Into<String>) {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .events
            .push(event.into());
    }

    fn fail_recovery_reports(&self, count: usize) {
        self.recovery_failures_remaining
            .store(count, Ordering::SeqCst);
    }

    fn has_event(&self, name: &str) -> bool {
        self.event_count(name) > 0
    }

    fn event_count(&self, name: &str) -> usize {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .events
            .iter()
            .filter(|event| event.as_str() == name)
            .count()
    }

    fn recovery_reports(&self) -> Vec<RecoveryObservation> {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .recovery_reports
            .clone()
    }

    fn completion_reports(&self) -> usize {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .completion_reports
    }

    fn cancellation_reports(&self) -> usize {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .cancellation_reports
    }
}

#[async_trait]
impl PullProtocol for FakeProtocol {
    async fn enroll(
        &self,
        _enrollment_credential: &EnrollmentCredential,
        _request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn refresh(
        &self,
        _session: &RunnerSession,
        _request: tack_runner::client::RefreshRequest,
    ) -> Result<tack_runner::client::RefreshResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn claim(
        &self,
        _session: &RunnerSession,
        _request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError> {
        self.event("claim_committed");
        self.work
            .lock()
            .expect("claim lock")
            .take()
            .map(|work| ClaimResult::Work(Box::new(work)))
            .ok_or(ProtocolClientError::Rejected)
    }

    async fn heartbeat(
        &self,
        _session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError> {
        self.event("heartbeat");
        if self.failure == FailurePoint::Heartbeat {
            return Err(ProtocolClientError::Transport);
        }
        let active = request
            .active_attempts
            .into_iter()
            .next()
            .expect("active attempt");
        Ok(HeartbeatResponse {
            protocol_version: ProtocolVersion::v1(),
            heartbeat_id: request.heartbeat_id,
            accepted_at: Timestamp::new("2026-08-07T12:00:01Z"),
            lease_results: vec![LeaseResult {
                attempt_id: active.attempt_id,
                fencing_token: active.fencing_token,
                lease_expires_at: Timestamp::new("2026-08-07T12:01:00Z"),
                cancellation_requested: self.cancellation_requested,
            }],
        })
    }

    async fn report_start(
        &self,
        _session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        let (event, fails) = match report.phase {
            StartPhase::Preparing => (
                "start:preparing",
                self.failure == FailurePoint::PreparingAck,
            ),
            StartPhase::ProcessObservedRunning => (
                "start:process_observed_running",
                self.failure == FailurePoint::ProcessStartAck,
            ),
        };
        self.event(event);
        if fails {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(())
        }
    }

    async fn report_completion(
        &self,
        _session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("completion".into());
        evidence.completion_reports += 1;
        if self.failure == FailurePoint::Completion {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(CompletionResponse {
                protocol_version: ProtocolVersion::v1(),
                attempt_id: report.attempt_id,
                completion_id: report.completion_id,
                state: report.terminal_state,
                replayed: false,
                committed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            })
        }
    }

    async fn report_cancellation(
        &self,
        _session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("cancellation_observation".into());
        evidence.cancellation_reports += 1;
        if self.failure == FailurePoint::Cancellation {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(CancellationResponse {
                protocol_version: ProtocolVersion::v1(),
                attempt_id: report.attempt_id,
                cancellation_request_id: report.cancellation_request_id,
                state: AttemptState::Cancelled,
                replayed: false,
                committed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            })
        }
    }

    async fn observe_recovery(
        &self,
        _session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence
            .events
            .push(format!("recovery:{:?}", report.observation));
        evidence.recovery_reports.push(report.observation);
        drop(evidence);
        if self
            .recovery_failures_remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            Err(ProtocolClientError::Transport)
        } else {
            let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
                "../../../docs/contracts/runner-v1/recovery-observation.response.json"
            ))
            .expect("recovery fixture");
            response.attempt_id = report.attempt_id;
            response.recovery_key = report.recovery_key;
            Ok(response)
        }
    }
}

#[derive(Debug, Default)]
struct ProcessEvidence {
    starts: usize,
    cancels: usize,
    reconciles: usize,
}

#[derive(Clone)]
struct FakeAdapter {
    evidence: Arc<Mutex<ProcessEvidence>>,
    recovery: RecoveryObservation,
}

impl FakeAdapter {
    fn new(recovery: RecoveryObservation) -> Self {
        Self {
            evidence: Arc::new(Mutex::new(ProcessEvidence::default())),
            recovery,
        }
    }

    fn starts(&self) -> usize {
        self.evidence.lock().expect("process evidence").starts
    }

    fn cancels(&self) -> usize {
        self.evidence.lock().expect("process evidence").cancels
    }

    fn reconciles(&self) -> usize {
        self.evidence.lock().expect("process evidence").reconciles
    }
}

#[async_trait]
impl HarnessAdapter for FakeAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        self.evidence.lock().expect("process evidence").starts += 1;
        Ok(LocalRunHandle {
            process_id: "fake-process".into(),
        })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        self.evidence.lock().expect("process evidence").cancels += 1;
        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            details: serde_json::Map::new(),
        })
    }

    async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        Ok(HarnessOutcome {
            terminal_state: AttemptState::Succeeded,
            terminal_reason: serde_json::json!({"code":"completed"}),
            final_checkpoint: None,
            actual_execution: actual_execution(),
            usage: usage(),
        })
    }

    async fn reconcile(
        &self,
        _journal: &tack_runner::client::AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        self.evidence.lock().expect("process evidence").reconciles += 1;
        Ok(self.recovery)
    }
}

#[derive(Clone)]
struct FakeWorktree {
    hang: bool,
    provisions: Arc<AtomicUsize>,
}

impl FakeWorktree {
    fn succeeds() -> Self {
        Self {
            hang: false,
            provisions: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A checkout that never finishes: the runner is killed inside it.
    fn hangs() -> Self {
        Self {
            hang: true,
            provisions: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl WorktreeProvisioner for FakeWorktree {
    async fn provision(
        &self,
        _workspace: &Workspace,
        _repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        self.provisions.fetch_add(1, Ordering::SeqCst);
        if self.hang {
            std::future::pending().await
        } else {
            Ok(())
        }
    }
}

fn actual_execution() -> tack_protocol::execution::ActualExecution {
    serde_json::from_str(
        r#"{
            "harness_kind":"fake", "harness_version":"1.0.0",
            "model": "test-model", "model_source": "reported",
            "capability_snapshot":{
                "cancel":{"support":"supported","reason":null},
                "resume":{"support":"unsupported","reason":"no resume"},
                "decisions":{"support":"supported","reason":null},
                "artifacts":{"support":"supported","reason":null},
                "usage":{"support":"advisory","reason":"partial"}
            },
            "workspace_id":"ws_617474656d70742d6372617368",
            "base_revision":"0123456789abcdef",
            "started_at":"2026-08-07T12:00:00Z", "ended_at":"2026-08-07T12:01:00Z"
        }"#,
    )
    .expect("actual execution")
}

fn work() -> ClaimedWork {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(fixture["request"].clone()).expect("request snapshot");
    let mut attempt: AttemptSnapshot =
        serde_json::from_value(fixture["attempt"].clone()).expect("attempt snapshot");
    request.repository.base_revision = "0123456789abcdef".into();
    attempt.attempt_id = DomainAttemptId::new("attempt-crash");
    attempt.runner_id = DomainRunnerId::new("runner-crash");
    attempt.base_revision = request.repository.base_revision.clone();
    ClaimedWork {
        claim_request_id: ClaimRequestId::new("claim-crash"),
        lease: AttemptLease {
            attempt_id: AttemptId::new("attempt-crash"),
            runner_id: RunnerId::new("runner-crash"),
            fencing_token: FencingToken(7),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-07T12:00:00Z"),
            expires_at: Timestamp::new("2026-08-07T12:01:00Z"),
        },
        request,
        attempt,
    }
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runner-crash"),
        RunnerCredential::new("test-secret-never-log"),
        Timestamp::new("2026-08-07T13:00:00Z"),
    )
}

fn claim() -> ClaimRequest {
    ClaimRequest {
        claim_request_id: ClaimRequestId::new("claim-crash"),
        available_capacity: 1,
        wait: Duration::ZERO,
    }
}

/// One engine wired to the given fakes, rooted at `root/workspaces`. Every
/// test builds at least one of these; several build two, to simulate a
/// process restart picking up where a crashed engine left off.
fn engine(
    protocol: &FakeProtocol,
    adapter: &FakeAdapter,
    journal: &OwnerOnlyJournal,
    root: &Path,
    worktree: FakeWorktree,
) -> RunnerEngine<FakeProtocol, FakeAdapter, FakeWorktree> {
    RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), worktree),
    )
}

/// Kills the runner for real: the attempt is run until its provisioning has
/// begun (the journal then holds `Prepared`), and its task is dropped there.
async fn crash_while_provisioning(
    protocol: &FakeProtocol,
    adapter: &FakeAdapter,
    journal: &OwnerOnlyJournal,
    root: &Path,
) {
    let worktree = FakeWorktree::hangs();
    let crashed = engine(protocol, adapter, journal, root, worktree.clone());
    let running = tokio::spawn(async move { crashed.run_once(&session(), claim()).await });
    while worktree.provisions.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    running.abort();
    let _ = running.await;
}

/// A quarantine leaves nothing left to recover and a dated record of why.
fn assert_quarantine_recorded(root: &Path, journal: &OwnerOnlyJournal) {
    assert!(
        journal
            .unresolved()
            .expect("post-quarantine scan")
            .is_empty()
    );
    assert!(
        root.join("quarantine")
            .read_dir()
            .expect("quarantine")
            .next()
            .is_some(),
        "quarantine directory holds evidence of the terminal decision"
    );
}

/// The setup shared by every scenario where the harness process was
/// observed starting but its start acknowledgment never reached the server —
/// the shape both `failed_ambiguity_report_retries_once_then_quarantines`
/// and `reoffered_quarantined_attempt_rejected_before_second_spawn` restart
/// from.
fn ambiguous_ack_scenario(
    label: &str,
) -> (
    tempfile::TempDir,
    FakeProtocol,
    FakeAdapter,
    OwnerOnlyJournal,
) {
    let root_dir = root(label);
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root_dir.path());
    (root_dir, protocol, adapter, journal)
}

/// A single `run_once` against a fresh engine wired to `worktree: succeeds`
/// — the first-attempt shape shared by every test whose crash happens after
/// the worktree provisions cleanly.
async fn attempt_run_once(
    protocol: &FakeProtocol,
    adapter: &FakeAdapter,
    journal: &OwnerOnlyJournal,
    root: &Path,
) -> Result<RunCycle, EngineError> {
    engine(protocol, adapter, journal, root, FakeWorktree::succeeds())
        .run_once(&session(), claim())
        .await
}

/// Restarts against the same journal and asserts the restart's only outcome
/// is a quarantine — the shape shared by every crash this file restarts from.
async fn recover_and_expect_quarantine(
    protocol: &FakeProtocol,
    adapter: &FakeAdapter,
    journal: &OwnerOnlyJournal,
    root: &Path,
) {
    let restarted = engine(protocol, adapter, journal, root, FakeWorktree::succeeds());
    let outcomes = restarted
        .recover(&session())
        .await
        .expect("restart recovery");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));
}

#[tokio::test]
async fn before_spawn_worktree_crash_recovers_without_respawn() {
    let root_dir = root("before-spawn");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessStopped);
    let journal = OwnerOnlyJournal::new(root);

    crash_while_provisioning(&protocol, &adapter, &journal, root).await;
    assert_eq!(adapter.starts(), 0);
    let unresolved = journal.unresolved().expect("unresolved journal");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].state, JournalState::Prepared);

    let recovered = engine(
        &protocol,
        &adapter,
        &journal,
        root,
        FakeWorktree::succeeds(),
    );
    let outcomes = recovered.recover(&session()).await.expect("recover");
    assert!(matches!(outcomes.as_slice(), [RunCycle::Completed { .. }]));
    assert_eq!(adapter.starts(), 0, "recovery cannot respawn the harness");
    assert_eq!(adapter.reconciles(), 1);
    assert_eq!(
        protocol.recovery_reports(),
        vec![RecoveryObservation::ProcessStopped]
    );
    assert!(protocol.has_event("start:preparing"));
    assert!(protocol.has_event("recovery:ProcessStopped"));
}

#[tokio::test]
async fn two_unresolved_journals_after_a_crash_are_both_recovered() {
    let root_dir = root("two-journals");
    let root = root_dir.path();
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessStopped);
    let journal = OwnerOnlyJournal::new(root);
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);

    // Two attempts held leases when the runner crashed.
    for attempt in ["attempt-crash", "attempt-crash-2"] {
        let mut crashed_work = work();
        crashed_work.lease.attempt_id = AttemptId::new(attempt);
        crashed_work.attempt.attempt_id = DomainAttemptId::new(attempt);
        let crashed = FakeProtocol::new(crashed_work, FailurePoint::None, false);
        crash_while_provisioning(&crashed, &adapter, &journal, root).await;
    }
    assert_eq!(journal.unresolved().expect("unresolved").len(), 2);

    let recovered = engine(
        &protocol,
        &adapter,
        &journal,
        root,
        FakeWorktree::succeeds(),
    );
    let outcomes = recovered.recover(&session()).await.expect("recover");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Completed { .. }, RunCycle::Completed { .. }]
    ));
    assert_eq!(adapter.starts(), 0, "recovery cannot respawn the harness");
    assert_eq!(adapter.reconciles(), 2);
    assert_eq!(protocol.event_count("recovery:ProcessStopped"), 2);
    assert!(journal.unresolved().expect("unresolved").is_empty());
}

#[tokio::test]
async fn spawn_ack_loss_quarantines_as_ambiguous_without_retry() {
    let (root_dir, protocol, adapter, journal) = ambiguous_ack_scenario("spawn-before-ack");
    let root = root_dir.path();

    let result = attempt_run_once(&protocol, &adapter, &journal, root)
        .await
        .expect("quarantine result");
    assert!(matches!(result, RunCycle::Quarantined { .. }));
    assert_eq!(adapter.starts(), 1);
    assert_eq!(adapter.cancels(), 1);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    assert_eq!(
        protocol.recovery_reports(),
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(protocol.has_event("start:process_observed_running"));
    assert!(protocol.has_event("recovery:Ambiguous"));
}

struct TerminalLossCase {
    label: &'static str,
    failure: FailurePoint,
    cancellation_requested: bool,
    completion_reports: usize,
    cancellation_reports: usize,
    cancels: usize,
    event: &'static str,
}

async fn assert_terminal_loss(case: TerminalLossCase) {
    let root_dir = root(case.label);
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), case.failure, case.cancellation_requested);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let runner = engine(
        &protocol,
        &adapter,
        &journal,
        root,
        FakeWorktree::succeeds(),
    );

    let result = runner
        .run_once(&session(), claim())
        .await
        .expect("terminal outbox result");
    assert!(
        matches!(result, RunCycle::TerminalReportPending { .. }),
        "{}",
        case.label
    );
    assert_eq!(
        protocol.completion_reports(),
        case.completion_reports,
        "{}",
        case.label
    );
    assert_eq!(
        protocol.cancellation_reports(),
        case.cancellation_reports,
        "{}",
        case.label
    );
    assert!(protocol.recovery_reports().is_empty(), "{}", case.label);
    assert!(protocol.has_event(case.event), "{}", case.label);
    assert_eq!(adapter.starts(), 1, "{}", case.label);
    assert_eq!(adapter.cancels(), case.cancels, "{}", case.label);
    assert_eq!(
        journal.unresolved().expect("journal scan").len(),
        1,
        "{}",
        case.label
    );
}

#[tokio::test]
async fn terminal_report_loss_keeps_attempt_pending_without_resend() {
    let cases = [
        TerminalLossCase {
            label: "completion",
            failure: FailurePoint::Completion,
            cancellation_requested: false,
            completion_reports: 1,
            cancellation_reports: 0,
            cancels: 0,
            event: "completion",
        },
        TerminalLossCase {
            label: "cancellation",
            failure: FailurePoint::Cancellation,
            cancellation_requested: true,
            completion_reports: 0,
            cancellation_reports: 1,
            cancels: 1,
            event: "cancellation_observation",
        },
    ];

    for case in cases {
        assert_terminal_loss(case).await;
    }
}

#[tokio::test]
async fn failed_ambiguity_report_retries_once_then_quarantines() {
    let (root_dir, protocol, adapter, journal) = ambiguous_ack_scenario("ambiguity-report-retry");
    let root = root_dir.path();
    protocol.fail_recovery_reports(1);

    assert!(matches!(
        attempt_run_once(&protocol, &adapter, &journal, root)
            .await
            .expect("pending result"),
        RunCycle::RecoveryPending { .. }
    ));
    assert_eq!(
        journal.unresolved().expect("pending local evidence").len(),
        1,
        "failed report keeps the journal eligible for restart recovery"
    );
    assert_eq!(adapter.starts(), 1);

    recover_and_expect_quarantine(&protocol, &adapter, &journal, root).await;
    assert_eq!(
        adapter.starts(),
        1,
        "restart recovery must never launch again"
    );
    assert_eq!(adapter.reconciles(), 1);
    assert_eq!(
        adapter.cancels(),
        1,
        "only the original post-spawn stop runs"
    );
    assert_eq!(
        protocol.recovery_reports(),
        vec![RecoveryObservation::Ambiguous; 2],
        "the ambiguity report is retried exactly once after its failed delivery"
    );
    assert_eq!(protocol.event_count("start:process_observed_running"), 1);
    assert_quarantine_recorded(root, &journal);
}

#[tokio::test]
async fn running_process_recovery_quarantines_without_respawn() {
    let root_dir = root("process-running-recovery");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessRunning);
    let journal = OwnerOnlyJournal::new(root);
    let lease = work().lease;
    journal
        .persist_before_spawn(&tack_runner::client::AttemptJournal::prepared(
            &lease,
            WorkspaceJournal {
                workspace_id: WorkspaceId::new("ws_crash"),
                path: root.join("workspaces/attempt-crash"),
                base_revision: "0123456789abcdef".into(),
            },
        ))
        .expect("persist prior journal");

    recover_and_expect_quarantine(&protocol, &adapter, &journal, root).await;
    assert_eq!(
        adapter.starts(),
        0,
        "recovery must not spawn a second process"
    );
    assert_eq!(adapter.reconciles(), 1);
    assert_eq!(
        adapter.cancels(),
        0,
        "no local handle exists to cancel on restart"
    );
    assert_eq!(
        protocol.recovery_reports(),
        vec![RecoveryObservation::ProcessRunning]
    );
    assert!(
        protocol.has_event("recovery:ProcessRunning"),
        "a running process is preserved as an audited recovery fact"
    );
    assert_quarantine_recorded(root, &journal);
}

#[tokio::test]
async fn reoffered_quarantined_attempt_rejected_before_second_spawn() {
    let (root_dir, protocol, adapter, journal) = ambiguous_ack_scenario("quarantined-reoffer");
    let root = root_dir.path();
    assert!(matches!(
        attempt_run_once(&protocol, &adapter, &journal, root)
            .await
            .expect("first quarantine"),
        RunCycle::Quarantined { .. }
    ));

    *protocol.work.lock().expect("claim lock") = Some(work());
    assert!(matches!(
        attempt_run_once(&protocol, &adapter, &journal, root).await,
        Err(EngineError::Journal(JournalError::AlreadyExists))
    ));

    assert_eq!(
        adapter.starts(),
        1,
        "reoffered quarantined work cannot relaunch"
    );
    assert_eq!(adapter.cancels(), 1);
    assert_eq!(
        protocol.event_count("claim_committed"),
        2,
        "the server reoffer reached the runner but stopped at local evidence"
    );
    assert_eq!(
        protocol.event_count("start:preparing"),
        1,
        "the rejected reoffer never starts preparation or a process"
    );
    assert_eq!(
        protocol.recovery_reports(),
        vec![RecoveryObservation::Ambiguous]
    );
    assert_quarantine_recorded(root, &journal);
}

// ---- cancelling a harness that starts process groups of its own ------------

/// The shared fake harness in its `spawn_detached` mode, behind a grammar that
/// reads the `process_started <pgid>` line the way docket's grammar reads its
/// own event. `announces` is whether the fake prints the line at all.
struct DetachingGrammar {
    announces: bool,
    /// Whether the descriptor reports progress, which is what sends the
    /// attempt down the line-reading wait docket takes.
    progress: bool,
    pidfile: std::path::PathBuf,
    /// The groups the wait reported, as the grammar's `report` saw them.
    seen: std::sync::Arc<std::sync::Mutex<Option<tack_runner::harness::process::ProcessGroups>>>,
}

static DETACHING: tack_runner::harness::local_process::HarnessDescriptor =
    tack_runner::harness::local_process::HarnessDescriptor {
        kind: "detaching-fake",
        program: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/harness/fixtures/fake_harness.sh"
        ),
        model_selection: tack_runner::harness::local_process::ModelSelection::Optional,
        inherited_env: &["PATH"],
        probe_notes: &[],
        credential_note: "",
        reports_process_groups: true,
        cancels_over_api: false,
        reports_progress: false,
        sandbox_binds: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    };

static DETACHING_PROGRESS: tack_runner::harness::local_process::HarnessDescriptor =
    tack_runner::harness::local_process::HarnessDescriptor {
        kind: "detaching-progress-fake",
        program: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/harness/fixtures/fake_harness.sh"
        ),
        model_selection: tack_runner::harness::local_process::ModelSelection::Optional,
        inherited_env: &["PATH"],
        probe_notes: &[],
        credential_note: "",
        reports_process_groups: true,
        cancels_over_api: false,
        reports_progress: true,
        sandbox_binds: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    };

impl tack_runner::harness::local_process::HarnessGrammar for DetachingGrammar {
    fn descriptor(&self) -> &'static tack_runner::harness::local_process::HarnessDescriptor {
        if self.progress {
            &DETACHING_PROGRESS
        } else {
            &DETACHING
        }
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        tack_runner::harness::local_process::HarnessGrammar::capabilities(
            &tack_runner::harness::claude_code::ClaudeCodeGrammar,
        )
    }

    fn invocation(
        &self,
        _run: &tack_runner::harness::local_process::RunContext<'_>,
    ) -> Result<tack_runner::harness::local_process::Invocation, HarnessError> {
        Ok(tack_runner::harness::local_process::Invocation {
            env: [
                ("TACK_FAKE_HARNESS_MODE", "spawn_detached".to_owned()),
                (
                    "TACK_FAKE_HARNESS_ANNOUNCE",
                    if self.announces { "1" } else { "0" }.to_owned(),
                ),
                (
                    "TACK_FAKE_HARNESS_PIDFILE",
                    self.pidfile.display().to_string(),
                ),
            ]
            .map(|(name, value)| (name.to_owned(), value))
            .into(),
            ..Default::default()
        })
    }

    fn report(
        &self,
        run: &tack_runner::harness::local_process::RunContext<'_>,
        result: &tack_runner::harness::process::ProcessResult,
    ) -> tack_runner::harness::local_process::RunReport {
        *self.seen.lock().expect("groups lock") = Some(result.groups);
        tack_runner::harness::local_process::HarnessGrammar::report(
            &tack_runner::harness::claude_code::ClaudeCodeGrammar,
            run,
            result,
        )
    }

    fn signal(
        &self,
        _run: &tack_runner::harness::local_process::RunContext<'_>,
        line: &str,
    ) -> Option<tack_runner::client::engine::StreamSignal> {
        let pgid = line.strip_prefix("process_started ")?.parse().ok()?;
        Some(tack_runner::client::engine::StreamSignal::ProcessStarted { pgid })
    }
}

/// Starts the attempt, waits until its detached grandchild exists, cancels,
/// and returns the evidence and the grandchild's pid. The caller reaps the
/// grandchild if the cancellation left it.
async fn cancel_with_a_detached_grandchild(announces: bool) -> (CancellationEvidence, u32) {
    use tack_runner::client::engine::ExecutionSpec;
    use tack_runner::harness::local_process::LocalProcessHarness;
    use tack_runner::harness::process::ProcessLimits;

    let dir = root("detached");
    let pidfile = dir.path().join("grandchild.pid");
    let harness = LocalProcessHarness::discover(
        DetachingGrammar {
            announces,
            progress: false,
            pidfile: pidfile.clone(),
            seen: Default::default(),
        },
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        tack_runner::SecretStore::file(dir.path().join("secrets.json")),
    )
    .expect("the fake harness script is locatable");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let mut work = work();
    work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new(DETACHING.kind);
    let spec = ExecutionSpec {
        work,
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt-crash"),
            id: WorkspaceId::new("ws_detached"),
            path: workspace,
            base_revision: "0123456789abcdef".into(),
        },
    };

    let handle = harness.start(&spec).await.expect("start");
    let grandchild = {
        let mut found = None;
        for _ in 0..200 {
            if let Some(pid) = std::fs::read_to_string(&pidfile)
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
            {
                found = Some(pid);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        found.expect("the fake harness started its detached grandchild")
    };
    assert!(tack_runner::harness::process::process_alive(grandchild));
    let evidence = harness.cancel(&handle).await.expect("cancel");
    (evidence, grandchild)
}

async fn gone_within(pid: u32, within: Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    while std::time::Instant::now() < deadline {
        if !tack_runner::harness::process::process_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

/// The attempt times out with the fake never exiting on its own: the group it
/// announced must be gone and the wait must say so. `progress` picks the wait
/// path: the line-reading one (what docket takes) or the plain one.
async fn time_out_with_an_announced_detached_group(
    progress: bool,
) -> (
    tack_runner::harness::process::ProcessGroups,
    Option<u32>,
    HarnessOutcome,
) {
    use tack_runner::client::engine::ExecutionSpec;
    use tack_runner::harness::local_process::LocalProcessHarness;
    use tack_runner::harness::process::ProcessLimits;

    let dir = root("timeout-groups");
    let pidfile = dir.path().join("grandchild.pid");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let harness = LocalProcessHarness::discover(
        DetachingGrammar {
            announces: true,
            progress,
            pidfile: pidfile.clone(),
            seen: seen.clone(),
        },
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        tack_runner::SecretStore::file(dir.path().join("secrets.json")),
    )
    .expect("the fake harness script is locatable");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let mut work = work();
    let descriptor = if progress {
        &DETACHING_PROGRESS
    } else {
        &DETACHING
    };
    work.request.requested_harness_kind =
        tack_protocol::execution::HarnessKind::new(descriptor.kind);
    work.request.timeout_seconds = 3;
    let spec = ExecutionSpec {
        work,
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt-crash"),
            id: WorkspaceId::new("ws_timeout_groups"),
            path: workspace,
            base_revision: "0123456789abcdef".into(),
        },
    };
    let handle = harness.start(&spec).await.expect("start");
    let outcome = harness.wait(&handle).await.expect("wait");
    let grandchild = std::fs::read_to_string(&pidfile)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    let groups = seen
        .lock()
        .expect("groups lock")
        .expect("the wait reported its groups");
    (groups, grandchild, outcome)
}

/// Docket reports progress, so its attempts take the line-reading wait, which
/// reads the announcement and kills the group on timeout. The plain wait never
/// reads a line, so a group is never announced to it: the second row records
/// that as it is (nothing tracked, the group survives), and no harness can
/// reach it while one that announces groups also reports progress.
#[tokio::test]
async fn timeout_stops_every_group_the_harness_announced() {
    use tack_runner::harness::process::ProcessGroups;
    let reached = ProcessGroups {
        tracked: 1,
        killed: 1,
        survived: 0,
    };
    for (path, progress, expected, stopped) in [
        ("line-reading wait", true, reached, true),
        ("plain wait", false, ProcessGroups::default(), false),
    ] {
        let (groups, grandchild, outcome) =
            time_out_with_an_announced_detached_group(progress).await;
        let grandchild = grandchild.expect("the fake harness started its detached grandchild");
        let gone = gone_within(grandchild, Duration::from_secs(10)).await;
        // Reap it before asserting: this test started it.
        let _ = std::process::Command::new("kill")
            .arg(grandchild.to_string())
            .status();
        assert_eq!(outcome.terminal_state, AttemptState::Failed, "{path}");
        assert_eq!(outcome.terminal_reason["code"], "timeout", "{path}");
        assert_eq!(
            outcome.terminal_reason["message"], "The run's time ran out after 3 seconds",
            "{path}"
        );
        assert_eq!(
            gone, stopped,
            "{path}: the announced group after the timeout"
        );
        assert_eq!(groups, expected, "{path}");
    }
}

#[tokio::test]
async fn cancel_reaches_a_detached_group_the_harness_announced() {
    let (evidence, grandchild) = cancel_with_a_detached_grandchild(true).await;
    assert_eq!(evidence.observation, CancelObservation::ProcessStopped);
    assert!(
        gone_within(grandchild, Duration::from_secs(10)).await,
        "the announced group must be dead after the cancellation"
    );
    assert_eq!(
        evidence.details["groups"],
        serde_json::json!({"tracked": 1, "killed": 1, "survived": 0})
    );
}

/// The honest `Advisory` case: nothing announced the group, so nothing
/// reaches it and the evidence claims nothing about it.
#[tokio::test]
async fn cancel_cannot_reach_a_detached_group_nobody_announced() {
    let (evidence, grandchild) = cancel_with_a_detached_grandchild(false).await;
    assert_eq!(evidence.observation, CancelObservation::ProcessStopped);
    let survived = tack_runner::harness::process::process_alive(grandchild);
    // Reap it before asserting: this test started it.
    let _ = std::process::Command::new("kill")
        .arg(grandchild.to_string())
        .status();
    assert!(survived, "a group nobody announced is not reached");
    assert_eq!(
        evidence.details["groups"],
        serde_json::json!({"tracked": 0, "killed": 0, "survived": 0})
    );
}

// ---- a harness ends when its process ends -----------------------------------

/// `/bin/sh -c <script>` behind the real Claude Code grammar's reading of the
/// transcript, so what the process prints and how it exits is the whole input.
struct ShellGrammar {
    script: &'static str,
    pidfile: std::path::PathBuf,
}

static SHELL: tack_runner::harness::local_process::HarnessDescriptor =
    tack_runner::harness::local_process::HarnessDescriptor {
        kind: "shell-fake",
        program: "/bin/sh",
        model_selection: tack_runner::harness::local_process::ModelSelection::Optional,
        inherited_env: &["PATH"],
        probe_notes: &[],
        credential_note: "",
        reports_process_groups: false,
        cancels_over_api: false,
        reports_progress: false,
        sandbox_binds: &[],
        readiness: None,
        models_docs: "",
        sign_in_docs: "",
    };

impl tack_runner::harness::local_process::HarnessGrammar for ShellGrammar {
    fn descriptor(&self) -> &'static tack_runner::harness::local_process::HarnessDescriptor {
        &SHELL
    }

    fn capabilities(&self) -> tack_protocol::execution::FeatureCapabilities {
        tack_runner::harness::local_process::HarnessGrammar::capabilities(
            &tack_runner::harness::claude_code::ClaudeCodeGrammar,
        )
    }

    fn invocation(
        &self,
        _run: &tack_runner::harness::local_process::RunContext<'_>,
    ) -> Result<tack_runner::harness::local_process::Invocation, HarnessError> {
        Ok(tack_runner::harness::local_process::Invocation {
            args: vec!["-c".to_owned(), self.script.to_owned()],
            env: [(
                "TACK_FAKE_PIDFILE".to_owned(),
                self.pidfile.display().to_string(),
            )]
            .into(),
            ..Default::default()
        })
    }

    fn report(
        &self,
        run: &tack_runner::harness::local_process::RunContext<'_>,
        result: &tack_runner::harness::process::ProcessResult,
    ) -> tack_runner::harness::local_process::RunReport {
        tack_runner::harness::local_process::HarnessGrammar::report(
            &tack_runner::harness::claude_code::ClaudeCodeGrammar,
            run,
            result,
        )
    }
}

/// Starts the shell script as an attempt and waits for it, returning the
/// outcome and the pid the script wrote to its pidfile, if it did.
async fn run_shell_harness(script: &'static str) -> (HarnessOutcome, Option<u32>) {
    use tack_runner::client::engine::ExecutionSpec;
    use tack_runner::harness::local_process::LocalProcessHarness;
    use tack_runner::harness::process::ProcessLimits;

    let dir = root("shell");
    let pidfile = dir.path().join("child.pid");
    let harness = LocalProcessHarness::discover(
        ShellGrammar {
            script,
            pidfile: pidfile.clone(),
        },
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(60)),
        dir.path().join("staging"),
        tack_runner::SecretStore::file(dir.path().join("secrets.json")),
    )
    .expect("/bin/sh is locatable");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let mut work = work();
    work.request.requested_harness_kind = tack_protocol::execution::HarnessKind::new(SHELL.kind);
    let spec = ExecutionSpec {
        work,
        workspace: Workspace {
            attempt_id: AttemptId::new("attempt-crash"),
            id: WorkspaceId::new("ws_shell"),
            path: workspace,
            base_revision: "0123456789abcdef".into(),
        },
    };
    let handle = harness.start(&spec).await.expect("start");
    let outcome = harness.wait(&handle).await.expect("wait");
    let child = std::fs::read_to_string(&pidfile)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    (outcome, child)
}

/// The harness exits 1; a child it left behind holds stdout open and would
/// print a success line two seconds later.
#[tokio::test]
async fn a_child_outliving_a_failed_harness_is_killed_and_cannot_claim_success() {
    let started = std::time::Instant::now();
    let (outcome, child) = run_shell_harness(
        r#"(sleep 2; printf '%s\n' '{"type":"result","is_error":false}') &
echo $! > "$TACK_FAKE_PIDFILE"
exit 1"#,
    )
    .await;
    assert_eq!(outcome.terminal_state, AttemptState::Failed);
    assert_eq!(outcome.terminal_reason["reason"], "exit_status");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the attempt must end with the harness, not with the child's stdout"
    );
    let child = child.expect("the harness wrote its child's pid");
    assert!(
        gone_within(child, Duration::from_secs(6)).await,
        "the child must be dead within the grace"
    );
}

#[tokio::test]
async fn the_first_terminal_line_decides_and_a_later_one_is_ignored() {
    let rows = [
        (
            r#"printf '%s\n' '{"type":"result","is_error":true}' '{"type":"result","is_error":false}'"#,
            AttemptState::Failed,
        ),
        (
            r#"printf '%s\n' '{"type":"result","is_error":false}'"#,
            AttemptState::Succeeded,
        ),
    ];
    for (script, expected) in rows {
        let (outcome, _) = run_shell_harness(script).await;
        assert_eq!(outcome.terminal_state, expected, "{script}");
    }
}

/// The plain wait never reads a line, so it can never kill a group it was
/// told about: a harness that announces its groups must report progress,
/// which is what sends its attempts down the line-reading wait.
#[test]
fn a_harness_that_announces_groups_takes_the_line_reading_wait() {
    for descriptor in tack_runner::harness::DESCRIPTORS {
        assert!(
            !descriptor.reports_process_groups || descriptor.reports_progress,
            "{} announces process groups but would take the plain wait",
            descriptor.kind
        );
    }
}
