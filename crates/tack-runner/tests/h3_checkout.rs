//! A claimed attempt reaches a real harness process with its own
//! checkout of a **real** git repository.
//!
//! `crash_matrix.rs` proves the engine's lifecycle against a fake
//! provisioner: it answers "does the engine call provisioning at the right
//! moment", not "does a checkout exist when the harness starts". This file
//! answers the second question end to end — real `git`, real repository, real
//! child process — because that gap was invisible to every test
//! that used a fake provisioner (an empty directory satisfied all of them).

use std::{
    path::{Path, PathBuf},
    process::Command as SyncCommand,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tack_orch::execution::{
    AttemptId as DomainAttemptId, AttemptSnapshot, ExecutionRequestSnapshot, ProtocolVersion,
    RecoveryObservationRequest, RecoveryObservationResponse, RunnerId as DomainRunnerId,
};
use tack_runner::{
    EnrollmentCredential,
    client::{
        AttemptId, AttemptLease, AttemptState, CancelObservation, CancellationEvidence,
        CancellationReport, CancellationResponse, ClaimRequest, ClaimRequestId, ClaimResult,
        ClaimedWork, CompletionReport, CompletionResponse, EnrollmentRequest, EnrollmentResponse,
        FencingToken, HarnessAdapter, HarnessError, HarnessOutcome, HeartbeatRequest,
        HeartbeatResponse, LeaseResult, LocalRunHandle, OwnerOnlyJournal, ProtocolClientError,
        PullProtocol, RecoveryObservation, RunCycle, RunnerCredential, RunnerEngine, RunnerId,
        RunnerSession, StartReport, Timestamp, WorkspaceManager,
        workspace::git::{CHECKOUT_MARKER, GitWorktreeProvisioner},
    },
};

mod common;
use common::temp_dir as temp_root;
use common::usage;

/// Resolves `git` to an absolute path instead of relying on `PATH`.
///
/// Not paranoia: `harness::claude_code`'s discovery test overwrites the
/// process-wide `PATH` for the duration of its assertion, and the Rust test
/// harness runs tests on many threads, so any concurrently-running test that
/// resolves a bare program name can observe that empty `PATH` and fail with a
/// spurious "binary not found". This was found as a one-in-many-runs failure
/// of `a_checkout_of_a_different_revision_is_never_reused` during
/// `cargo test --workspace` — a shared-state hazard from a concurrently
/// running test overwriting `PATH`.
fn git_program() -> PathBuf {
    for candidate in [
        "/usr/bin/git",
        "/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

fn run_git(directory: &Path, args: &[&str]) -> String {
    let output = SyncCommand::new(git_program())
        .current_dir(directory)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real repository with a distinctive file, so "the harness saw the
/// checkout" can be asserted on content rather than on the mere existence of
/// a directory.
fn source_repository() -> (tempfile::TempDir, String) {
    let path_dir = temp_root("source");
    let path = path_dir.path();
    run_git(path, &["-c", "init.defaultBranch=main", "init", "--quiet"]);
    run_git(path, &["config", "user.email", "runner@example.invalid"]);
    run_git(path, &["config", "user.name", "Tack Runner Test"]);
    std::fs::write(path.join("PLAN.md"), "the item to work on\n").expect("write");
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "--quiet", "-m", "plan"]);
    let commit = run_git(path, &["rev-parse", "HEAD"]);
    (path_dir, commit)
}

/// What the harness process observed from inside its own working directory.
#[derive(Debug, Default, Clone)]
struct HarnessObservation {
    process_id: String,
    head: String,
    file: String,
    working_directory: String,
}

/// A minimal but genuinely real harness: it spawns an actual child process in
/// the attempt's workspace and records what that process could see. Nothing
/// here is mocked — if the workspace is not a checkout, the child fails and
/// the recorded observation is empty.
#[derive(Clone, Default)]
struct RealProcessAdapter {
    observed: Arc<Mutex<HarnessObservation>>,
}

#[async_trait]
impl HarnessAdapter for RealProcessAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        let output = SyncCommand::new("/bin/sh")
            .current_dir(&spec.workspace.path)
            .arg("-c")
            .arg("printf '%s\\n%s\\n%s' \"$(git rev-parse HEAD)\" \"$(cat PLAN.md)\" \"$(pwd)\"")
            .output()
            .map_err(|_| HarnessError::Process)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut lines = stdout.lines();
        let process_id = format!("h3-{}", std::process::id());
        *self.observed.lock().expect("observation") = HarnessObservation {
            process_id: process_id.clone(),
            head: lines.next().unwrap_or_default().to_owned(),
            file: lines.next().unwrap_or_default().to_owned(),
            working_directory: lines.next().unwrap_or_default().to_owned(),
        };
        Ok(LocalRunHandle { process_id })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new("2026-08-19T12:00:01Z"),
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
        Ok(RecoveryObservation::ProcessStopped)
    }
}

#[derive(Clone)]
struct FakeProtocol {
    work: Arc<Mutex<Option<ClaimedWork>>>,
    cancellation_requested: bool,
    completions: Arc<Mutex<Vec<CompletionReport>>>,
}

impl FakeProtocol {
    fn new(work: ClaimedWork, cancellation_requested: bool) -> Self {
        Self {
            work: Arc::new(Mutex::new(Some(work))),
            cancellation_requested,
            completions: Arc::default(),
        }
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
        let active = request
            .active_attempts
            .into_iter()
            .next()
            .expect("active attempt");
        Ok(HeartbeatResponse {
            protocol_version: ProtocolVersion::v1(),
            heartbeat_id: request.heartbeat_id,
            accepted_at: Timestamp::new("2026-08-19T12:00:01Z"),
            lease_results: vec![LeaseResult {
                attempt_id: active.attempt_id,
                fencing_token: active.fencing_token,
                lease_expires_at: Timestamp::new("2026-08-19T12:01:00Z"),
                cancellation_requested: self.cancellation_requested,
            }],
        })
    }

    async fn report_start(
        &self,
        _session: &RunnerSession,
        _report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        Ok(())
    }

    async fn report_completion(
        &self,
        _session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        self.completions
            .lock()
            .expect("completions")
            .push(report.clone());
        Ok(CompletionResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: report.attempt_id,
            completion_id: report.completion_id,
            state: report.terminal_state,
            replayed: false,
            committed_at: Timestamp::new("2026-08-19T12:00:02Z"),
        })
    }

    async fn report_cancellation(
        &self,
        _session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        Ok(CancellationResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: report.attempt_id,
            cancellation_request_id: report.cancellation_request_id,
            state: AttemptState::Cancelled,
            replayed: false,
            committed_at: Timestamp::new("2026-08-19T12:00:02Z"),
        })
    }

    async fn observe_recovery(
        &self,
        _session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/recovery-observation.response.json"
        ))
        .expect("recovery fixture");
        response.attempt_id = report.attempt_id;
        response.recovery_key = report.recovery_key;
        Ok(response)
    }
}

fn actual_execution() -> tack_orch::execution::ActualExecution {
    serde_json::from_str(
        r#"{
            "harness_kind":"fake", "harness_version":"1.0.0",
            "model": "test-model", "model_source": "reported",
            "capability_snapshot":{
                "cancel":{"support":"supported","reason":null},
                "resume":{"support":"unsupported","reason":"no resume"},
                "decisions":{"support":"unsupported","reason":"no decisions"},
                "artifacts":{"support":"unsupported","reason":"no artifacts"},
                "usage":{"support":"advisory","reason":"partial"}
            },
            "workspace_id":"ws_placeholder",
            "base_revision":"placeholder",
            "started_at":"2026-08-19T12:00:00Z", "ended_at":"2026-08-19T12:01:00Z"
        }"#,
    )
    .expect("actual execution")
}

/// The frozen claim fixture, retargeted at a repository that actually exists
/// on this machine. Only the repository coordinates and the ids change — the
/// snapshot shape stays the contract's.
fn work(attempt: &str, remote: &Path, revision: &str) -> ClaimedWork {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(fixture["request"].clone()).expect("request snapshot");
    let mut snapshot: AttemptSnapshot =
        serde_json::from_value(fixture["attempt"].clone()).expect("attempt snapshot");
    request.repository.remote = remote.to_string_lossy().into_owned();
    request.repository.base_revision = revision.to_owned();
    snapshot.attempt_id = DomainAttemptId::new(attempt);
    snapshot.runner_id = DomainRunnerId::new("runner-h3");
    snapshot.base_revision = revision.to_owned();
    ClaimedWork {
        claim_request_id: ClaimRequestId::new(format!("claim-{attempt}")),
        lease: AttemptLease {
            attempt_id: AttemptId::new(attempt),
            runner_id: RunnerId::new("runner-h3"),
            fencing_token: FencingToken(7),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-19T12:00:00Z"),
            expires_at: Timestamp::new("2026-08-19T12:01:00Z"),
        },
        request,
        attempt: snapshot,
    }
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runner-h3"),
        RunnerCredential::new("test-secret-never-log"),
        Timestamp::new("2026-08-19T13:00:00Z"),
    )
}

fn claim(attempt: &str) -> ClaimRequest {
    ClaimRequest {
        claim_request_id: ClaimRequestId::new(format!("claim-{attempt}")),
        available_capacity: 1,
        wait: Duration::ZERO,
    }
}

/// Every test in this file but one runs against a real `git` checkout; this
/// is that wiring, shared so each test states only what varies: the
/// protocol, the adapter, the journal, and the workspace root.
fn git_engine<A: HarnessAdapter>(
    protocol: FakeProtocol,
    adapter: A,
    journal: OwnerOnlyJournal,
    workspaces: &Path,
) -> RunnerEngine<FakeProtocol, A, GitWorktreeProvisioner> {
    RunnerEngine::new(
        protocol,
        adapter,
        journal,
        WorkspaceManager::new(
            workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    )
}

/// Drives an attempt into the "killed after checkout, before anything
/// terminal is reported" state that both restart tests below resume from,
/// and asserts the checkout that a restart must later decide about is really
/// there. Shared because it is not what either test is proving — what each
/// proves is what its own restart does next.
async fn crash_before_terminal_report(
    label: &str,
    source: &Path,
    commit: &str,
    journal: &OwnerOnlyJournal,
    workspaces: &Path,
) -> PathBuf {
    let killed = git_engine(
        FakeProtocol::new(work(label, source, commit), false),
        FailingStartAdapter {
            reconcile_is_ambiguous: false,
        },
        journal.clone(),
        workspaces,
    );
    assert!(killed.run_once(&session(), claim(label)).await.is_err());
    let orphan = std::fs::read_dir(workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("the interrupted attempt left a checkout");
    assert!(
        orphan.join(CHECKOUT_MARKER).exists(),
        "the interrupted attempt had already completed its checkout"
    );
    orphan
}

#[tokio::test]
async fn a_claimed_attempt_reaches_the_harness_in_its_own_checkout() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("run");
    let root = root_dir.path();
    let adapter = RealProcessAdapter::default();
    let engine = git_engine(
        FakeProtocol::new(work("attempt-h3", source, &commit), false),
        adapter.clone(),
        OwnerOnlyJournal::new(root.join("journal")),
        &root.join("workspaces"),
    );

    let cycle = engine
        .run_once(&session(), claim("attempt-h3"))
        .await
        .expect("the claimed attempt runs");
    assert!(matches!(cycle, RunCycle::Completed { .. }));

    let observed = adapter.observed.lock().expect("observation").clone();
    assert_eq!(
        observed.head, commit,
        "the harness process must start inside a checkout of the claimed revision"
    );
    assert_eq!(
        observed.file, "the item to work on",
        "the harness process must be able to read the repository's files"
    );
    assert!(!observed.process_id.is_empty());
    assert!(
        observed.working_directory.contains("workspaces"),
        "the harness ran outside the runner's workspace root: {}",
        observed.working_directory
    );
}

/// The complement of the test above: without a real provisioner, the
/// identical run cannot reach the harness at all. Reverting the fix is
/// therefore proven to break the claim, rather than assumed to.
#[tokio::test]
async fn without_a_provisioner_the_attempt_never_reaches_the_harness() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("unavailable");
    let root = root_dir.path();
    let adapter = RealProcessAdapter::default();
    let engine = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-gap", source, &commit), false),
        adapter.clone(),
        OwnerOnlyJournal::new(root.join("journal")),
        WorkspaceManager::new(
            root.join("workspaces"),
            tack_runner::client::UnavailableWorktreeProvisioner,
        ),
    );

    let error = engine
        .run_once(&session(), claim("attempt-h3-gap"))
        .await
        .expect_err("provisioning is unavailable, so the attempt cannot run");
    assert!(format!("{error}").contains("worktree provisioning is not configured"));
    assert!(
        adapter
            .observed
            .lock()
            .expect("observation")
            .head
            .is_empty(),
        "no harness process may start without a checkout"
    );
}

#[tokio::test]
async fn a_completed_attempt_leaves_no_checkout_behind() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("cleanup");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let engine = git_engine(
        FakeProtocol::new(work("attempt-h3-done", source, &commit), false),
        RealProcessAdapter::default(),
        OwnerOnlyJournal::new(root.join("journal")),
        &workspaces,
    );

    engine
        .run_once(&session(), claim("attempt-h3-done"))
        .await
        .expect("the claimed attempt runs");

    let remaining: Vec<_> = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert!(
        remaining.is_empty(),
        "a completed attempt must leave no checkout behind: {remaining:?}"
    );
}

#[tokio::test]
async fn a_cancelled_attempt_leaves_no_checkout_behind() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("cancelled");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let engine = git_engine(
        FakeProtocol::new(work("attempt-h3-cancel", source, &commit), true),
        RealProcessAdapter::default(),
        OwnerOnlyJournal::new(root.join("journal")),
        &workspaces,
    );

    let cycle = engine
        .run_once(&session(), claim("attempt-h3-cancel"))
        .await
        .expect("the claimed attempt is cancelled");
    assert!(matches!(cycle, RunCycle::Cancelled { .. }));

    let remaining: Vec<_> = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert!(
        remaining.is_empty(),
        "a cancelled attempt must leave no checkout behind: {remaining:?}"
    );
}

/// Crash recovery: a runner killed after provisioning leaves a checkout on
/// disk with an unresolved journal record. The restart must resolve the
/// attempt and remove that checkout — the "no unusable worktree survives a
/// kill" invariant, proven at the engine level rather than inside the
/// provisioner.
#[tokio::test]
async fn a_checkout_left_by_a_killed_runner_is_removed_by_the_restart() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("recovery");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let journal = OwnerOnlyJournal::new(root.join("journal"));
    let orphan =
        crash_before_terminal_report("attempt-h3-killed", source, &commit, &journal, &workspaces)
            .await;
    assert_eq!(journal.unresolved().expect("journal").len(), 1);

    let restarted = git_engine(
        FakeProtocol::new(work("attempt-h3-killed", source, &commit), false),
        RealProcessAdapter::default(),
        journal.clone(),
        &workspaces,
    );
    let outcomes = restarted.recover(&session()).await.expect("recovery");
    assert_eq!(outcomes.len(), 1);
    assert!(
        !orphan.exists(),
        "the restart must not leave the killed attempt's checkout behind"
    );
    assert!(journal.unresolved().expect("journal").is_empty());
}

/// The deliberate other half of the rule above: an attempt the runner could
/// not settle is *quarantined*, and its checkout is the evidence an operator
/// will be asked to look at. Deleting it would destroy that evidence, so the
/// cleanup on recovery must be conditional, not unconditional.
#[tokio::test]
async fn a_quarantined_attempt_keeps_its_checkout_as_evidence() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("quarantine");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let journal = OwnerOnlyJournal::new(root.join("journal"));
    let orphan = crash_before_terminal_report(
        "attempt-h3-quarantine",
        source,
        &commit,
        &journal,
        &workspaces,
    )
    .await;

    let restarted = git_engine(
        FakeProtocol::new(work("attempt-h3-quarantine", source, &commit), false),
        FailingStartAdapter {
            reconcile_is_ambiguous: true,
        },
        journal.clone(),
        &workspaces,
    );
    let outcomes = restarted.recover(&session()).await.expect("recovery");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));
    assert!(
        orphan.join(CHECKOUT_MARKER).exists(),
        "a quarantined attempt's checkout must survive for the operator"
    );
}

/// Fails to start, and optionally cannot say what happened afterwards. The
/// second mode is how a quarantine is reached: the engine treats a reconcile
/// error as `Ambiguous`, which no disposition can settle.
struct FailingStartAdapter {
    reconcile_is_ambiguous: bool,
}

#[async_trait]
impl HarnessAdapter for FailingStartAdapter {
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
        Err(HarnessError::Process)
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Err(HarnessError::Process)
    }

    async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        Err(HarnessError::Process)
    }

    async fn reconcile(
        &self,
        _journal: &tack_runner::client::AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        if self.reconcile_is_ambiguous {
            Err(HarnessError::RecoveryUnavailable)
        } else {
            Ok(RecoveryObservation::ProcessStopped)
        }
    }
}

/// Does the work of an attempt (writes one file) and, when `reconfigure` is
/// set, plants what would run the repository's code in the runner: a
/// `post-checkout` hook and a `core.fsmonitor` script, each of which leaves a
/// marker file if git ever runs it.
struct RepositoryWritingAdapter {
    reconfigure: bool,
    markers: PathBuf,
}

#[async_trait]
impl HarnessAdapter for RepositoryWritingAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        let workspace = &spec.workspace.path;
        std::fs::write(workspace.join("work.txt"), "the harness wrote this\n")
            .map_err(|_| HarnessError::Process)?;
        if self.reconfigure {
            let hook = workspace.join(".git/hooks/post-checkout");
            let monitor = workspace.join(".git/fsmonitor.sh");
            let marker = |name: &str| self.markers.join(name).display().to_string();
            std::fs::write(&hook, format!("#!/bin/sh\necho ran > {}\n", marker("hook")))
                .map_err(|_| HarnessError::Process)?;
            std::fs::write(
                &monitor,
                format!("#!/bin/sh\necho ran > {}\n", marker("fsmonitor")),
            )
            .map_err(|_| HarnessError::Process)?;
            for script in [&hook, &monitor] {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755))
                    .map_err(|_| HarnessError::Process)?;
            }
            run_git(
                workspace,
                &["config", "core.fsmonitor", monitor.to_str().expect("utf-8")],
            );
        }
        Ok(LocalRunHandle {
            process_id: "h3-writer".to_owned(),
        })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Err(HarnessError::Process)
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
        Ok(RecoveryObservation::ProcessStopped)
    }
}

/// An evidence channel that takes nothing: evidence capture and the push are
/// switched on by its presence, and these tests read the bare remote and the
/// completion report instead.
struct NoUploads;

#[async_trait]
impl tack_runner::client::AttemptDataProtocol for NoUploads {
    async fn submit_events(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::EventBatchReport,
    ) -> Result<tack_runner::client::EventBatchResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn create_decision(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::DecisionCreateReport,
    ) -> Result<tack_runner::client::DecisionCreateResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn poll_decisions(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::DecisionPollReport,
    ) -> Result<tack_runner::client::DecisionPollResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn submit_artifact_manifest(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::ArtifactManifestReport,
    ) -> Result<Vec<tack_runner::client::ArtifactUploadGrant>, ProtocolClientError> {
        Ok(Vec::new())
    }

    async fn put_artifact_content(
        &self,
        _session: &RunnerSession,
        _fencing_token: FencingToken,
        _grant: &tack_runner::client::ArtifactUploadGrant,
        _media_type: Option<&str>,
        _content: Vec<u8>,
    ) -> Result<(), ProtocolClientError> {
        Ok(())
    }
}

fn bare_clone(source: &Path, into: &Path, name: &str) -> PathBuf {
    let bare = into.join(name);
    run_git(
        into,
        &[
            "clone",
            "--bare",
            "--quiet",
            source.to_str().expect("utf-8"),
            bare.to_str().expect("utf-8"),
        ],
    );
    bare
}

/// Runs one attempt that writes a file (and, when `reconfigure`, plants hooks
/// and a config script) against a bare remote that accepts pushes. Returns the
/// bare remote, the marker directory and the completion the runner reported.
async fn run_pushing_attempt(
    label: &str,
    reconfigure: bool,
) -> (tempfile::TempDir, PathBuf, PathBuf, CompletionReport) {
    let (source_dir, commit) = source_repository();
    let root_dir = temp_root(label);
    let remote = bare_clone(source_dir.path(), root_dir.path(), "remote.git");
    let markers = root_dir.path().join("markers");
    std::fs::create_dir_all(&markers).expect("markers");
    let protocol = FakeProtocol::new(work(label, &remote, &commit), false);
    let completions = protocol.completions.clone();
    let engine = git_engine(
        protocol,
        RepositoryWritingAdapter {
            reconfigure,
            markers: markers.clone(),
        },
        OwnerOnlyJournal::new(root_dir.path().join("journal")),
        &root_dir.path().join("workspaces"),
    )
    .with_data_protocol(Arc::new(NoUploads))
    .with_git(tack_runner::config::GitConfig {
        push_branches: true,
        ..Default::default()
    });
    engine
        .run_once(&session(), claim(label))
        .await
        .expect("the attempt runs");
    let report = completions.lock().expect("completions")[0].clone();
    (root_dir, remote, markers, report)
}

fn remote_branches(remote: &Path) -> String {
    run_git(
        remote,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/tack",
        ],
    )
}

#[tokio::test]
async fn a_harness_that_rewrites_git_config_ends_repository_config_changed_and_pushes_nothing() {
    let (_root, remote, markers, report) = run_pushing_attempt("rcc", true).await;
    assert!(
        !markers.join("hook").exists() && !markers.join("fsmonitor").exists(),
        "git ran code the harness planted in the repository"
    );
    assert_eq!(report.terminal_state, AttemptState::Failed);
    assert_eq!(report.terminal_reason["code"], "repository_config_changed");
    assert!(
        report.terminal_reason["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(".git/config")),
        "{}",
        report.terminal_reason
    );
    assert_eq!(remote_branches(&remote), "", "nothing may be pushed");
}

#[tokio::test]
async fn a_run_that_changes_no_git_config_still_pushes() {
    let (_root, remote, markers, report) = run_pushing_attempt("noconfig", false).await;
    assert_eq!(report.terminal_state, AttemptState::Succeeded);
    assert!(!markers.join("hook").exists());
    assert_eq!(remote_branches(&remote).lines().count(), 1);
}

#[tokio::test]
async fn a_pushurl_the_agent_sets_does_not_redirect_the_push() {
    let (source_dir, commit) = source_repository();
    let root_dir = temp_root("pushurl");
    let remote = bare_clone(source_dir.path(), root_dir.path(), "remote.git");
    let evil = bare_clone(source_dir.path(), root_dir.path(), "evil.git");
    let path = root_dir.path().join("attempt-pushurl");
    std::fs::create_dir_all(&path).expect("attempt directory");
    std::fs::write(path.join(".tack-attempt"), "attempt-pushurl").expect("marker");
    let workspace = tack_runner::client::Workspace {
        attempt_id: AttemptId::new("attempt-pushurl"),
        id: tack_runner::client::WorkspaceId::new("ws_pushurl"),
        path,
        base_revision: commit.clone(),
    };
    let provisioner = GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600));
    let repository = tack_runner::client::RepositorySpec {
        remote: remote.display().to_string(),
        base_revision: commit,
        workspace_mode: tack_runner::client::WorkspaceMode::Clone,
        repository_path: None,
    };
    use tack_runner::client::WorktreeProvisioner;
    provisioner
        .provision(&workspace, &repository)
        .await
        .expect("checkout");
    std::fs::write(workspace.path.join("work.txt"), "work\n").expect("write");
    let evil_url = evil.display().to_string();
    run_git(
        &workspace.path,
        &["config", "remote.origin.pushurl", &evil_url],
    );
    run_git(&workspace.path, &["config", "remote.origin.url", &evil_url]);
    let evidence = provisioner
        .capture_evidence(&workspace, &[], None, ("Tack <t@localhost>", "work"))
        .await
        .expect("evidence")
        .expect("a git provisioner captures");
    provisioner
        .publish_branch(
            &workspace,
            "tack/abc-a1",
            true,
            Some(&remote.display().to_string()),
            &evidence.captured_commit,
            &evidence.head_commit,
        )
        .await
        .expect("published")
        .expect("a git provisioner publishes");
    assert_eq!(remote_branches(&remote), "tack/abc-a1");
    assert_eq!(remote_branches(&evil), "");
}

/// An evidence channel that keeps what the runner uploads, by artifact name.
#[derive(Clone, Default)]
struct RecordedUploads {
    names: Arc<Mutex<std::collections::HashMap<String, String>>>,
    contents: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl RecordedUploads {
    fn named(&self, name: &str) -> Option<Vec<u8>> {
        let names = self.names.lock().expect("names");
        let id = names.iter().find(|(_, value)| *value == name)?.0.clone();
        self.contents.lock().expect("contents").get(&id).cloned()
    }
}

#[async_trait]
impl tack_runner::client::AttemptDataProtocol for RecordedUploads {
    async fn submit_events(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::EventBatchReport,
    ) -> Result<tack_runner::client::EventBatchResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn create_decision(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::DecisionCreateReport,
    ) -> Result<tack_runner::client::DecisionCreateResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn poll_decisions(
        &self,
        _session: &RunnerSession,
        _report: tack_runner::client::DecisionPollReport,
    ) -> Result<tack_runner::client::DecisionPollResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn submit_artifact_manifest(
        &self,
        _session: &RunnerSession,
        report: tack_runner::client::ArtifactManifestReport,
    ) -> Result<Vec<tack_runner::client::ArtifactUploadGrant>, ProtocolClientError> {
        let mut names = self.names.lock().expect("names");
        Ok(report
            .artifacts
            .into_iter()
            .map(|item| {
                names.insert(item.artifact_id.clone(), item.name);
                tack_runner::client::ArtifactUploadGrant {
                    artifact_id: item.artifact_id,
                    state: "pending".to_owned(),
                    method: "PUT".to_owned(),
                    path: "/unused".to_owned(),
                    expires_at: None,
                }
            })
            .collect())
    }

    async fn put_artifact_content(
        &self,
        _session: &RunnerSession,
        _fencing_token: FencingToken,
        grant: &tack_runner::client::ArtifactUploadGrant,
        _media_type: Option<&str>,
        content: Vec<u8>,
    ) -> Result<(), ProtocolClientError> {
        self.contents
            .lock()
            .expect("contents")
            .insert(grant.artifact_id.clone(), content);
        Ok(())
    }
}

/// What the fake harness does to the workspace before it reports success.
#[derive(Clone, Copy)]
enum Work {
    /// Moves onto a local branch called `main` and commits `work.txt` there.
    CommitOnMain,
    /// Leaves `work.txt` uncommitted.
    WriteFile,
    /// Rewrites `prill.yaml` and the files that decide how the work is checked.
    EditChecks,
}

struct WorkAdapter(Work);

#[async_trait]
impl HarnessAdapter for WorkAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        let workspace = &spec.workspace.path;
        let write = |name: &str, text: &str| {
            let path = workspace.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, text).expect("write");
        };
        match self.0 {
            Work::CommitOnMain => {
                run_git(workspace, &["checkout", "--quiet", "-b", "main"]);
                write("work.txt", "committed on a branch the agent named main\n");
                run_git(workspace, &["add", "work.txt"]);
                run_git(
                    workspace,
                    &[
                        "-c",
                        "user.name=agent",
                        "-c",
                        "user.email=agent@example.invalid",
                        "commit",
                        "--quiet",
                        "-m",
                        "agent work",
                    ],
                );
            }
            Work::WriteFile => write("work.txt", "the harness wrote this\n"),
            Work::EditChecks => {
                write("prill.yaml", "sandbox:\n  image: evil\n");
                write("package.json", "{\"scripts\":{\"test\":\"true\"}}\n");
                write(".github/workflows/ci.yml", "on: push\n");
            }
        }
        Ok(LocalRunHandle {
            process_id: "h3-work".to_owned(),
        })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Err(HarnessError::Process)
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
        Ok(RecoveryObservation::ProcessStopped)
    }
}

struct Ran {
    _root: tempfile::TempDir,
    remote: PathBuf,
    base: String,
    uploads: RecordedUploads,
    report: CompletionReport,
}

/// One pushing attempt against a bare remote whose `main` also holds
/// `committed` (path, text) pairs. `by_name` asks for the base as `main`
/// rather than as the commit id. `verify` is the runner's `[verify]` table.
async fn run_work(
    label: &str,
    work_done: Work,
    committed: &[(&str, &str)],
    by_name: bool,
    verify: impl FnOnce(&Path) -> Option<tack_runner::config::VerifyConfig>,
) -> Ran {
    let (source_dir, _) = source_repository();
    for (name, text) in committed {
        let path = source_dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("write");
    }
    run_git(source_dir.path(), &["add", "."]);
    run_git(
        source_dir.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "checks"],
    );
    let commit = run_git(source_dir.path(), &["rev-parse", "HEAD"]);
    let root_dir = temp_root(label);
    let remote = bare_clone(source_dir.path(), root_dir.path(), "remote.git");
    let revision = if by_name { "main" } else { commit.as_str() };
    let protocol = FakeProtocol::new(work(label, &remote, revision), false);
    let completions = protocol.completions.clone();
    let uploads = RecordedUploads::default();
    let mut engine = git_engine(
        protocol,
        WorkAdapter(work_done),
        OwnerOnlyJournal::new(root_dir.path().join("journal")),
        &root_dir.path().join("workspaces"),
    )
    .with_data_protocol(Arc::new(uploads.clone()))
    .with_git(tack_runner::config::GitConfig {
        push_branches: true,
        ..Default::default()
    });
    if let Some(config) = verify(root_dir.path()) {
        engine = engine.with_verify(
            config,
            tack_runner::harness::process::ProcessLimits::new(
                1 << 20,
                1 << 20,
                Duration::from_secs(60),
            ),
        );
    }
    engine
        .run_once(&session(), claim(label))
        .await
        .expect("the attempt runs");
    let report = completions.lock().expect("completions")[0].clone();
    Ran {
        _root: root_dir,
        remote,
        base: commit,
        uploads,
        report,
    }
}

fn script(directory: &Path, name: &str, text: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{text}")).expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
    path
}

fn verifier(
    kind: tack_runner::config::VerifyKind,
    program: &Path,
) -> tack_runner::config::VerifyConfig {
    tack_runner::config::VerifyConfig {
        enabled: true,
        kind,
        program: program.display().to_string(),
        args: Vec::new(),
        timeout_seconds: 60,
        sign_key: None,
    }
}

#[tokio::test]
async fn a_branch_the_agent_names_like_the_base_does_not_move_the_evidence_base() {
    let ran = run_work("samename", Work::CommitOnMain, &[], true, |_| None).await;
    assert_eq!(ran.report.terminal_state, AttemptState::Succeeded);
    let branch = remote_branches(&ran.remote);
    assert_eq!(branch.lines().count(), 1, "the work is pushed");
    assert_eq!(
        run_git(&ran.remote, &["diff", "--name-only", &ran.base, &branch]),
        "work.txt"
    );
    let patch = ran.uploads.named("changes.patch").expect("a patch");
    assert!(String::from_utf8_lossy(&patch).contains("work.txt"));
}

#[tokio::test]
async fn a_tree_that_moves_after_verification_is_never_pushed() {
    let ran = run_work("amended", Work::WriteFile, &[], false, |root| {
        let pack = std::fs::canonicalize(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/contracts/mrp-v1/fixtures/ready.json"
        ))
        .expect("fixture");
        // The pack is written first; then the agent's lingering process
        // rewrites HEAD, as a process the harness left behind could.
        let program = script(
            root,
            "verify.sh",
            &format!(
                "cp {} \"$6\"\ncd \"$4\" && git -c user.name=agent -c user.email=agent@example.invalid commit --quiet --amend --allow-empty -m moved\n",
                pack.display()
            ),
        );
        Some(verifier(tack_runner::config::VerifyKind::Command, &program))
    })
    .await;
    assert_eq!(ran.report.terminal_state, AttemptState::Failed);
    assert_eq!(
        ran.report.terminal_reason["code"],
        "tree_changed_after_verification"
    );
    assert_eq!(remote_branches(&ran.remote), "", "nothing may be pushed");
}

/// A prill that records the configuration it was handed and prints an empty pack.
fn recording_prill(root: &Path) -> tack_runner::config::VerifyConfig {
    let program = script(
        root,
        "prill.sh",
        &format!(
            "while [ $# -gt 0 ]; do\n  if [ \"$1\" = --evidence ]; then cp \"$2\" {}; fi\n  shift\ndone\necho '{{}}'\n",
            root.join("received-evidence").display()
        ),
    );
    verifier(tack_runner::config::VerifyKind::Prill, &program)
}

#[tokio::test]
async fn prill_reads_the_configuration_of_the_base_commit_not_the_agents() {
    let base_config = "sandbox:\n  image: registry.example/verify@sha256:base\n";
    let root = std::sync::Mutex::new(PathBuf::new());
    let ran = run_work(
        "prillconfig",
        Work::EditChecks,
        &[("prill.yaml", base_config)],
        false,
        |dir| {
            *root.lock().expect("root") = dir.to_path_buf();
            Some(recording_prill(dir))
        },
    )
    .await;
    assert_eq!(ran.report.terminal_state, AttemptState::Succeeded);
    // prill reads `prill.yaml` at the commit the evidence names: that commit
    // must be the pinned base, whose file is the base's, not the agent's edit.
    let received: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.lock().expect("root").join("received-evidence"))
            .expect("prill was handed the evidence"),
    )
    .expect("json");
    assert_eq!(received["baseCommit"], ran.base.as_str());
    let at_base = run_git(&ran.remote, &["show", &format!("{}:prill.yaml", ran.base)]);
    assert_eq!(at_base.trim_end(), base_config.trim_end());
}

#[tokio::test]
async fn a_change_to_how_the_work_is_checked_is_named_in_the_pack() {
    let ran = run_work("checkedfiles", Work::EditChecks, &[], false, |dir| {
        Some(recording_prill(dir))
    })
    .await;
    let pack: serde_json::Value =
        serde_json::from_slice(&ran.uploads.named("mrp.json").expect("a pack")).expect("json");
    let reasons: Vec<String> = pack["risk"]["reasons"]
        .as_array()
        .expect("reasons")
        .iter()
        .map(|reason| reason.as_str().unwrap_or_default().to_owned())
        .collect();
    let section = "The agent changed how it is checked";
    for path in ["package.json", ".github/workflows/ci.yml"] {
        assert!(
            reasons
                .iter()
                .any(|reason| reason.starts_with(section) && reason.contains(path)),
            "{path} is missing from {reasons:?}"
        );
    }
}
