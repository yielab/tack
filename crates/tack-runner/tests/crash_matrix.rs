use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tack_runner::{
    EnrollmentCredential,
    client::{
        AttemptId, AttemptLease, AttemptState, CancelObservation, CancellationReport, ClaimRequest,
        ClaimRequestId, ClaimResult, ClaimedWork, CompletionReport, EngineError, EnrollmentRequest,
        EnrollmentResponse, FencingToken, HarnessAdapter, HarnessError, HarnessOutcome,
        HeartbeatRequest, HeartbeatResponse, JournalError, JournalState, LeaseResult,
        LocalRunHandle, OwnerOnlyJournal, ProtocolClientError, PullProtocol, RecoveryObservation,
        RecoveryReport, RepositorySpec, RunCycle, RunnerCredential, RunnerEngine, RunnerId,
        RunnerSession, StartPhase, StartReport, Timestamp, Workspace, WorkspaceError, WorkspaceId,
        WorkspaceJournal, WorkspaceManager, WorktreeProvisioner,
    },
};

static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

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
            .map(ClaimResult::Work)
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
            lease_results: vec![LeaseResult {
                attempt_id: active.attempt_id,
                fencing_token: active.fencing_token,
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
        _report: CompletionReport,
    ) -> Result<(), ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("completion".into());
        evidence.completion_reports += 1;
        if self.failure == FailurePoint::Completion {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(())
        }
    }

    async fn report_cancellation(
        &self,
        _session: &RunnerSession,
        _report: CancellationReport,
    ) -> Result<(), ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("cancellation_observation".into());
        evidence.cancellation_reports += 1;
        if self.failure == FailurePoint::Cancellation {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(())
        }
    }

    async fn observe_recovery(
        &self,
        _session: &RunnerSession,
        report: RecoveryReport,
    ) -> Result<(), ProtocolClientError> {
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
            Ok(())
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

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancelObservation, HarnessError> {
        self.evidence.lock().expect("process evidence").cancels += 1;
        Ok(CancelObservation::ProcessStopped)
    }

    async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        Ok(HarnessOutcome {
            terminal_state: AttemptState::Succeeded,
            terminal_reason: "completed".into(),
            final_checkpoint: None,
        })
    }

    async fn reconcile(
        &self,
        _journal: &tack_runner::client::AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        self.evidence.lock().expect("process evidence").reconciles += 1;
        Ok(self.recovery.clone())
    }
}

#[derive(Clone)]
struct FakeWorktree {
    fail: bool,
    provisions: Arc<AtomicUsize>,
}

impl FakeWorktree {
    fn succeeds() -> Self {
        Self {
            fail: false,
            provisions: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn fails() -> Self {
        Self {
            fail: true,
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
        if self.fail {
            Err(WorkspaceError::Io)
        } else {
            Ok(())
        }
    }
}

fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tack-c4-{label}-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::SeqCst)
    ))
}

fn work() -> ClaimedWork {
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
        repository: RepositorySpec {
            remote: "https://example.invalid/repository.git".into(),
            base_revision: "0123456789abcdef".into(),
        },
    }
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runner-crash"),
        RunnerCredential::new("test-secret-never-log"),
    )
}

fn claim() -> ClaimRequest {
    ClaimRequest {
        claim_request_id: ClaimRequestId::new("claim-crash"),
        available_capacity: 1,
        wait: Duration::ZERO,
    }
}

fn remove_test_root(path: &PathBuf) {
    if path.exists() {
        std::fs::remove_dir_all(path).expect("remove test root");
    }
}

#[tokio::test]
async fn after_claim_before_spawn_failure_recovers_as_process_stopped_without_respawn() {
    let root = root("before-spawn");
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessStopped);
    let failed_worktree = FakeWorktree::fails();
    let journal = OwnerOnlyJournal::new(&root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), failed_worktree.clone()),
    );

    assert!(engine.run_once(&session(), claim()).await.is_err());
    assert_eq!(adapter.evidence.lock().expect("process evidence").starts, 0);
    assert_eq!(failed_worktree.provisions.load(Ordering::SeqCst), 1);
    let unresolved = journal.unresolved().expect("unresolved journal");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].state, JournalState::Prepared);

    let recovery = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = recovery.recover(&session()).await.expect("recover");
    assert!(matches!(outcomes.as_slice(), [RunCycle::Completed { .. }]));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 0, "recovery cannot respawn the harness");
    assert_eq!(process.reconciles, 1);
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::ProcessStopped]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "start:preparing")
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:ProcessStopped")
    );
    drop(evidence);
    remove_test_root(&root);
}

#[tokio::test]
async fn after_spawn_before_ack_failure_is_audited_ambiguous_and_never_retried() {
    let root = root("spawn-before-ack");
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(&root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("quarantine result");
    assert!(matches!(result, RunCycle::Quarantined { .. }));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 1);
    assert_eq!(process.cancels, 1);
    drop(process);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "start:process_observed_running")
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:Ambiguous")
    );
    drop(evidence);
    remove_test_root(&root);
}

#[tokio::test]
async fn completion_ack_failure_quarantines_once_with_ambiguous_recovery_evidence() {
    let root = root("completion");
    let protocol = FakeProtocol::new(work(), FailurePoint::Completion, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(&root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("quarantine result");
    assert!(matches!(result, RunCycle::Quarantined { .. }));
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.completion_reports, 1,
        "completion is never blind-retried"
    );
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(evidence.events.iter().any(|event| event == "completion"));
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:Ambiguous")
    );
    drop(evidence);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    remove_test_root(&root);
}

#[tokio::test]
async fn cancellation_ack_failure_never_claims_terminal_and_records_ambiguity() {
    let root = root("cancellation");
    let protocol = FakeProtocol::new(work(), FailurePoint::Cancellation, true);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(&root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("quarantine result");
    assert!(matches!(result, RunCycle::Quarantined { .. }));
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(evidence.cancellation_reports, 1);
    assert_eq!(evidence.completion_reports, 0);
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "cancellation_observation")
    );
    drop(evidence);
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 1);
    assert_eq!(
        process.cancels, 2,
        "initial stop plus best-effort quarantine stop"
    );
    drop(process);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    remove_test_root(&root);
}

#[tokio::test]
async fn failed_ambiguity_report_stays_pending_then_restart_quarantines_without_respawn() {
    let root = root("ambiguity-report-retry");
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    protocol.fail_recovery_reports(1);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(&root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    assert!(matches!(
        engine
            .run_once(&session(), claim())
            .await
            .expect("pending result"),
        RunCycle::RecoveryPending { .. }
    ));
    assert_eq!(
        journal.unresolved().expect("pending local evidence").len(),
        1,
        "failed report keeps the journal eligible for restart recovery"
    );
    assert_eq!(adapter.evidence.lock().expect("process evidence").starts, 1);

    let restarted = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = restarted
        .recover(&session())
        .await
        .expect("restart recovery");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));

    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 1,
        "restart recovery must never launch again"
    );
    assert_eq!(process.reconciles, 1);
    assert_eq!(process.cancels, 1, "only the original post-spawn stop runs");
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![
            RecoveryObservation::Ambiguous,
            RecoveryObservation::Ambiguous
        ],
        "the ambiguity report is retried exactly once after its failed delivery"
    );
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "start:process_observed_running")
            .count(),
        1
    );
    drop(evidence);
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
        "server-acknowledged ambiguity is preserved as local quarantine evidence"
    );
    remove_test_root(&root);
}

#[tokio::test]
async fn process_running_recovery_observation_reports_ambiguity_and_quarantines_without_spawn() {
    let root = root("process-running-recovery");
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessRunning);
    let journal = OwnerOnlyJournal::new(&root);
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

    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = engine
        .recover(&session())
        .await
        .expect("recover running process");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 0,
        "recovery must not spawn a second process"
    );
    assert_eq!(process.reconciles, 1);
    assert_eq!(
        process.cancels, 0,
        "no local handle exists to cancel on restart"
    );
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(
        !evidence
            .events
            .iter()
            .any(|event| event == "recovery:ProcessRunning"),
        "a running process must never be reported as a safe completed recovery"
    );
    drop(evidence);
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
            .is_some()
    );
    remove_test_root(&root);
}

#[tokio::test]
async fn reoffered_quarantined_attempt_is_rejected_before_a_second_spawn() {
    let root = root("quarantined-reoffer");
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(&root);
    let first = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    assert!(matches!(
        first
            .run_once(&session(), claim())
            .await
            .expect("first quarantine"),
        RunCycle::Quarantined { .. }
    ));

    *protocol.work.lock().expect("claim lock") = Some(work());
    let restarted = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    assert!(matches!(
        restarted.run_once(&session(), claim()).await,
        Err(EngineError::Journal(JournalError::AlreadyExists))
    ));

    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 1,
        "reoffered quarantined work cannot relaunch"
    );
    assert_eq!(process.cancels, 1);
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "claim_committed")
            .count(),
        2,
        "the server reoffer reached the runner but stopped at local evidence"
    );
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "start:preparing")
            .count(),
        1,
        "the rejected reoffer never starts preparation or a process"
    );
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    drop(evidence);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    assert!(
        root.join("quarantine")
            .read_dir()
            .expect("quarantine")
            .next()
            .is_some()
    );
    remove_test_root(&root);
}
