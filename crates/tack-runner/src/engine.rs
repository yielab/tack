//! Local pull-runner engine.
//!
//! The journal is persisted before `HarnessAdapter::start`. Once a local
//! process might exist, any failed fence/report/recovery operation is treated
//! as ambiguous and quarantined rather than retried.

use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;
use tack_protocol::execution::{ProtocolVersion, RecoveryObservation};
use thiserror::Error;
use tokio::sync::mpsc;

use super::{
    AttemptDataProtocol, AttemptId, AttemptState, CancellationReport, CancellationRequestId,
    ClaimRequest, ClaimResult, ClaimedWork, ClaimedWorkError, CompletionId, CompletionRequest,
    DecisionAnswer, DecisionOption, EnrollmentRequest, EnrollmentResponse, HeartbeatRequest,
    PendingTerminalReportKind, ProtocolClientError, PullProtocol, Recommendation, RefreshRequest,
    RefreshResponse, RunnerSession, StartPhase, StartReport, Timestamp, WorkspaceMode,
    journal::{AttemptJournal, JournalError, JournalState, OwnerOnlyJournal},
    workspace::{Workspace, WorkspaceError, WorkspaceManager, WorktreeProvisioner},
};

#[path = "engine/decisions.rs"]
mod decisions;
#[path = "engine/outcome.rs"]
mod outcome;
#[path = "engine/resume.rs"]
mod resume;

use outcome::{changed_config_path, runner_token_limit, token_limit_reached};

/// One thing a harness's own protocol asked the operator, read off a line of
/// its stdout by [`HarnessGrammar::signal`](crate::harness::local_process::HarnessGrammar::signal).
/// `kind`, `prompt`, `options` and `metadata` map one to one onto
/// `decision.create.request.json`; `vendor_id` does not cross the wire at
/// all — it is the harness's own correlation id (e.g. Claude Code's
/// `control_request.request_id`), carried back to
/// [`HarnessGrammar::answer`](crate::harness::local_process::HarnessGrammar::answer)
/// so the reply is addressed to the right pending question.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub vendor_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
    pub recommendation: Option<Recommendation>,
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// One step a running harness reported, for the attempt's timeline: what
/// kind of step it was (`agent.tool_call`, `agent.model_turn`) and a line
/// saying it. Read off a line of stdout by
/// [`HarnessGrammar::signal`](crate::harness::local_process::HarnessGrammar::signal)
/// and submitted as a runner event of that `kind` whose payload is `{"text": …}`,
/// plus `"usage"` when the step reports what it used: the server adds those up as
/// the attempt's running counted sum (`running_counted_join` in `tack-db`'s `budgets.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub kind: String,
    pub text: String,
    /// What this step used, when it says; the shape of the completion's usage.
    pub usage: Option<tack_protocol::execution::Usage>,
}

/// What one line of a harness's stdout means to the core, when the run is
/// listening for a pause-and-ask conversation. `Finished` is what lets the
/// core close the child's stdin, which is what lets a CLI that keeps
/// listening past its own terminal line actually exit. `Reply` is how a
/// grammar drives its own protocol's handshake (initialize, open a session,
/// send the prompt) from the replies it reads, before any question arrives:
/// write these bytes to the child's stdin as one line and keep listening.
/// `ProcessStarted` and `ProcessExited` tell the core that the harness
/// started, or finished with, a process group of its own (a tool call's),
/// so a cancellation can reach a group the main one does not contain.
/// `Progress` is a step worth showing on the attempt's timeline while the
/// run is still going.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamSignal {
    Question(Question),
    Progress(Progress),
    Reply(Vec<u8>),
    Finished,
    ProcessStarted { pgid: u32 },
    ProcessExited { pgid: u32 },
}

/// How often the runner's one ticker re-heartbeats every attempt it holds. Must stay comfortably under the server's
/// lease-grant window (`request_timeout_seconds_max` allows harness runs far
/// longer than any single lease) so a heartbeat lands well before the
/// previous one's grant would expire, including a missed cycle or two under
/// transient network trouble.
pub(crate) const LEASE_RENEWAL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

/// A decision this runner has asked the operator about and not yet answered.
struct PendingDecision {
    decision_id: String,
    question: Question,
    /// The poll cursor `decision.poll.request.json` calls `after`. The
    /// contract requires a timestamp, so it starts at the Unix epoch — every
    /// decision of this attempt — and then follows the server's own
    /// `next_after`, never this machine's clock.
    after: Timestamp,
}

/// How often a pending decision is polled for its answer. Much shorter than
/// [`LEASE_RENEWAL_INTERVAL`]: a person has just answered and is watching the
/// run for it to move.
const DECISION_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// How often the steps a running harness reported are sent to the server:
/// often enough that the timeline reads as live, seldom enough that a busy
/// agent costs a request every few seconds, not one per step.
const PROGRESS_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// The most steps one event batch carries; the server takes up to 100.
const PROGRESS_BATCH_MAX: usize = 50;

/// Where a fresh decision's poll cursor starts.
const POLL_FROM_THE_START: &str = "1970-01-01T00:00:00Z";

/// The mutable state one attempt's decision round trips share: the channel
/// pair to the running harness, whatever is currently pending an answer, and
/// the fixed deadline the attempt's own timeout set. Bundled so
/// `open_decision`/`advance_pending` take one seam, not four loose `&mut`s.
struct DecisionState<'a> {
    decisions: &'a mut Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
    pending: &'a mut Option<PendingDecision>,
    expires_at: chrono::DateTime<chrono::Utc>,
    /// The decisions the board already holds for this attempt when the runner
    /// took it back after a restart: a question asked again that is among them
    /// is not created a second time, it is waited for.
    known: &'a HashSet<String>,
}

/// The next question a running harness asks, or never resolves — used as a
/// [`tokio::select!`] branch that only actually waits on the channel when
/// there is one to wait on and nothing is already pending an answer, so a
/// harness that cannot ask, or one already mid-question, never wakes this
/// branch.
async fn next_question(
    decisions: &mut Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)>,
    already_pending: bool,
) -> Option<Question> {
    match decisions.as_mut() {
        Some((questions_rx, _)) if !already_pending => questions_rx.recv().await,
        _ => std::future::pending().await,
    }
}

/// The next step a running harness reports, or never resolves when it
/// reports none; `None` once the harness has stopped reporting.
async fn next_step(progress: &mut Option<mpsc::Receiver<Progress>>) -> Option<Option<Progress>> {
    match progress.as_mut() {
        Some(steps) => Some(steps.recv().await),
        None => std::future::pending().await,
    }
}

/// `Rejected` carries a `reason`.
///
/// Pre-spawn rejections have several genuinely distinct causes (wrong
/// harness kind, an unconfirmed auto-selected model, an unresolvable
/// binary, an unsupported provider, an unsupported provider/model
/// pairing, ...) that all reach this one variant. The reason is carried
/// across the trait boundary as a plain `String`, not a taxonomy of typed
/// sub-variants: `HarnessError` is a closed, deliberately small enum, and
/// widening it to a fourth *kind* of error was evaluated and rejected.
/// `Process`/`RecoveryUnavailable` are untouched: nothing needs an
/// analogous reason for them.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessError {
    #[error("harness rejected this execution: {reason}")]
    Rejected { reason: String },
    #[error("harness process operation failed")]
    Process,
    /// A file operation the harness needed failed; the kind says how, never the path.
    #[error("harness file operation failed: {0}")]
    Io(crate::error::IoKind),
    #[error("harness recovery observation is unavailable")]
    RecoveryUnavailable,
    #[error("this harness cannot take hold of a run that outlived the runner")]
    Unsupported,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Protocol(#[from] ProtocolClientError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Claim(#[from] ClaimedWorkError),
    #[error(transparent)]
    Harness(#[from] HarnessError),
    #[error("claimed lease belongs to another runner")]
    RunnerMismatch,
    #[error("harness outcome is not terminal")]
    NonTerminalOutcome,
    #[error("terminal report could not be encoded as canonical JSON")]
    TerminalReportSerialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRunHandle {
    pub process_id: String,
}

pub use tack_protocol::execution::CancelObservation;

/// Typed, non-secret evidence supplied by the harness after a cancellation
/// attempt. The adapter owns the observation time and details; the engine only
/// carries them to the cancellation transport unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct CancellationEvidence {
    pub observation: CancelObservation,
    pub observed_at: Timestamp,
    pub details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarnessOutcome {
    pub terminal_state: AttemptState,
    pub terminal_reason: serde_json::Value,
    pub final_checkpoint: Option<super::Checkpoint>,
    pub actual_execution: tack_protocol::execution::ActualExecution,
    pub usage: tack_protocol::execution::Usage,
}

#[derive(Debug, Clone)]
pub struct ExecutionSpec {
    pub work: ClaimedWork,
    pub workspace: Workspace,
}

/// Shared local process seam. It has no implementation in C3; C3 tests use a
/// fake adapter and later harness cards implement this contract in their own
/// files without changing engine ownership.
#[async_trait]
pub trait HarnessAdapter: Send + Sync {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;
    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError>;
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError>;
    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError>;
    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError>;

    /// Takes hold again of an agent that outlived the runner and that
    /// `reconcile` saw still running, from the claim the journal kept and the
    /// handle it recorded. Only an agent that lives outside the runner can
    /// answer: a child process died with it, so every local-process harness
    /// keeps this default.
    async fn resume(
        &self,
        _spec: &ExecutionSpec,
        _record: &AttemptJournal,
    ) -> Result<LocalRunHandle, HarnessError> {
        Err(HarnessError::Unsupported)
    }

    /// The channel pair for a run that can pause and ask: the engine
    /// receives [`Question`]s on the first half and posts back
    /// [`DecisionAnswer`]s on the second. `None` for a run that never asks —
    /// every harness today outside an `ask`-policy claude-code run. Taken
    /// once per handle; a second call returns `None` too.
    async fn decision_channels(
        &self,
        _handle: &LocalRunHandle,
    ) -> Option<(mpsc::Receiver<Question>, mpsc::Sender<DecisionAnswer>)> {
        None
    }

    /// What the run is confined to (`"bwrap"` or `"none"`); `None` for an
    /// adapter that does not say.
    async fn sandbox(&self, _handle: &LocalRunHandle) -> Option<String> {
        None
    }

    /// The steps a running harness reports as it goes, for the attempt's
    /// timeline. `None` for a harness that reports none. Taken once per
    /// handle, like [`Self::decision_channels`].
    async fn progress_channel(&self, _handle: &LocalRunHandle) -> Option<mpsc::Receiver<Progress>> {
        None
    }
}

/// An agent that outlived the runner, taken back by `recover` and ready to be
/// followed to its end. `recover` only prepares it: following can last hours,
/// so the caller runs [`RunnerEngine::follow_resumed`] where the run's lease is
/// heartbeated with every other, and where a restart's other journals do not
/// wait for it.
#[derive(Debug, Clone)]
pub struct ResumedRun {
    record: AttemptJournal,
    spec: ExecutionSpec,
    handle: LocalRunHandle,
    cancel: tokio::sync::watch::Receiver<bool>,
    known_decisions: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum RunCycle {
    /// A running agent `recover` took back; see [`ResumedRun`].
    Resumed(Box<ResumedRun>),
    /// Nothing to claim. `retry_after` is the server's own
    /// `claim.no-work.response.json` hint for when to ask again.
    NoWork {
        retry_after: std::time::Duration,
    },
    Completed {
        attempt_id: AttemptId,
    },
    Cancelled {
        attempt_id: AttemptId,
    },
    Quarantined {
        attempt_id: AttemptId,
    },
    RecoveryPending {
        attempt_id: AttemptId,
    },
    TerminalReportPending {
        attempt_id: AttemptId,
    },
}

pub struct RunnerEngine<P, A, W, C = crate::SystemClock> {
    protocol: P,
    adapter: A,
    journal: OwnerOnlyJournal,
    workspaces: WorkspaceManager<W>,
    clock: C,
    /// The events/decisions/artifacts transport
    /// (`AttemptDataProtocol`). Optional and defaulted to `None` by every
    /// existing constructor so no caller (`main.rs`
    /// aside, see [`Self::with_data_protocol`]) is forced to supply one —
    /// `crash_matrix.rs` and `h3_checkout.rs` keep
    /// compiling unchanged. When absent, the engine behaves exactly as
    /// before: no event/artifact submission is attempted.
    data_protocol: Option<Arc<dyn AttemptDataProtocol>>,
    /// The `[verify]` table and the process limits it runs under; `None`
    /// (and a disabled table) spawn nothing.
    verify: Option<(
        crate::config::VerifyConfig,
        crate::harness::process::ProcessLimits,
    )>,
    /// The `[git]` table; a disabled table pushes nothing.
    git: crate::config::GitConfig,
    /// The leases in flight, which the runner's ticker renews together.
    active: crate::active::ActiveAttempts,
}

impl<P, A, W> RunnerEngine<P, A, W, crate::SystemClock>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
{
    pub fn new(
        protocol: P,
        adapter: A,
        journal: OwnerOnlyJournal,
        workspaces: WorkspaceManager<W>,
    ) -> Self {
        Self::with_clock(protocol, adapter, journal, workspaces, crate::SystemClock)
    }
}

impl<P, A, W, C> RunnerEngine<P, A, W, C>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
    C: crate::Clock,
{
    /// Injects the local clock so heartbeat timestamps can be deterministic in
    /// tests without introducing lifecycle sleeps.
    pub fn with_clock(
        protocol: P,
        adapter: A,
        journal: OwnerOnlyJournal,
        workspaces: WorkspaceManager<W>,
        clock: C,
    ) -> Self {
        Self {
            protocol,
            adapter,
            journal,
            workspaces,
            clock,
            data_protocol: None,
            verify: None,
            git: crate::config::GitConfig::default(),
            active: crate::active::ActiveAttempts::new(1),
        }
    }

    /// Sets how many attempts this engine works at once.
    pub fn with_concurrency(mut self, total: u32) -> Self {
        self.active = crate::active::ActiveAttempts::new(total);
        self
    }

    /// Works against a registry the caller also holds, so something outside
    /// the engine (the Agents page) can read how many attempts are running.
    pub fn with_active(mut self, active: crate::active::ActiveAttempts) -> Self {
        self.active = active;
        self
    }

    /// The leases in flight.
    pub fn active(&self) -> &crate::active::ActiveAttempts {
        &self.active
    }

    /// Attaches the events/decisions/artifacts transport. A
    /// separate builder method, not a `new`/`with_clock` parameter, so every
    /// pre-existing construction site keeps
    /// compiling untouched; only `main.rs` (the one production wiring point)
    /// calls it today.
    pub fn with_data_protocol(mut self, data_protocol: Arc<dyn AttemptDataProtocol>) -> Self {
        self.data_protocol = Some(data_protocol);
        self
    }

    /// Attaches the verifier run after a succeeded attempt's evidence is
    /// captured. Nothing is spawned unless `config.enabled`.
    pub fn with_verify(
        mut self,
        config: crate::config::VerifyConfig,
        limits: crate::harness::process::ProcessLimits,
    ) -> Self {
        self.verify = Some((config, limits));
        self
    }

    /// Attaches the `[git]` table: when `push_branches` is on, a succeeded
    /// attempt's captured work is pushed to the remote it was fetched from.
    pub fn with_git(mut self, config: crate::config::GitConfig) -> Self {
        self.git = config;
        self
    }

    pub async fn enroll(
        &self,
        enrollment_credential: &crate::EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, EngineError> {
        Ok(self.protocol.enroll(enrollment_credential, request).await?)
    }

    pub async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, EngineError> {
        Ok(self.protocol.refresh(session, request).await?)
    }

    /// Performs one bounded claim/run/report cycle. The caller owns pacing;
    /// this avoids an untestable retry loop and means no work is never treated
    /// as a successful harness execution.
    pub async fn run_once(
        &self,
        session: &RunnerSession,
        claim: ClaimRequest,
    ) -> Result<RunCycle, EngineError> {
        match self.protocol.claim(session, claim).await? {
            ClaimResult::NoWork { retry_after, .. } => Ok(RunCycle::NoWork { retry_after }),
            ClaimResult::Work(work) => {
                let attempt_id = work.lease.attempt_id.clone();
                let cycle = self.run_claimed(session, *work).await;
                self.active.remove(&attempt_id);
                cycle
            }
        }
    }

    async fn run_claimed(
        &self,
        session: &RunnerSession,
        work: ClaimedWork,
    ) -> Result<RunCycle, EngineError> {
        let mut repository = work.workspace_repository()?;
        // A scratch request names no remote and no revision; its one commit
        // is made at provision time, so `HEAD` is the base evidence diffs against.
        let scratch = work.request.repository.kind == "scratch";
        if scratch {
            repository.base_revision = "HEAD".to_owned();
        }
        if work.lease.runner_id != session.runner_id {
            return Err(EngineError::RunnerMismatch);
        }

        let workspace = self.workspaces.plan(&work.lease, &repository)?;
        let mut record = AttemptJournal::prepared(&work.lease, workspace.journal());
        // The claim is kept so a restarted runner can follow an agent that is
        // still running. It carries references to secrets, never their values,
        // so a literal environment value is not written.
        let mut kept = work.clone();
        for entry in kept.request.environment.values_mut() {
            entry.value = None;
        }
        record.claimed_work = Some(kept);
        // This is the hard ownership boundary: no worktree or adapter method
        // with a local side effect is called before create+fsync succeeds.
        self.journal.persist_before_spawn(&record)?;
        let cancel = self.active.register(&record);
        self.protocol
            .report_start(
                session,
                StartReport {
                    attempt_id: record.attempt_id.clone(),
                    fencing_token: record.fencing_token,
                    phase: StartPhase::Preparing,
                    workspace_id: Some(workspace.id.clone()),
                    base_revision: Some(workspace.base_revision.clone()),
                    process_id: None,
                    environment_names: Vec::new(),
                    sandbox: None,
                },
            )
            .await?;
        let spec = ExecutionSpec {
            work: work.clone(),
            workspace,
        };
        let prepares = !scratch
            && matches!(
                repository.workspace_mode,
                WorkspaceMode::Clone | WorkspaceMode::LocalBranch
            );
        let install_skipped = prepares && crate::prepare::needs_no_install(&spec.work.request);
        // Everything from here to the first sight of the agent running ends
        // the attempt `failed` with its reason when it fails; only a failure
        // to reach the board above stays an engine error.
        let started = async {
            if scratch {
                self.workspaces.provision_scratch(&spec.workspace).await
            } else {
                self.workspaces
                    .provision(&spec.workspace, &repository)
                    .await
            }
            .map_err(|error| HarnessError::Rejected {
                reason: error.to_string(),
            })?;
            // Fixed here, before the harness can write anywhere: the commit
            // the base revision resolved to and where the branch is pushed.
            // They reach the journal with the update that follows the start.
            (record.base_commit, record.push_remote) =
                self.workspaces.provisioned(&spec.workspace).await;
            // The runner made this copy, so the runner installs what the agent
            // needs in it, unless the agent runs no command. A scratch folder has
            // no lockfile, `in_place` is the user's own folder and a docket run
            // is prepared by docket.
            if prepares && !install_skipped {
                let rejected = |reason: String| HarnessError::Rejected { reason };
                crate::prepare::prepare(
                    &spec.workspace.path,
                    &spec
                        .work
                        .request
                        .repository
                        .prepare
                        .clone()
                        .unwrap_or_default(),
                    &self
                        .workspaces
                        .state_dir()
                        .map_err(|e| rejected(e.to_string()))?,
                    std::time::Duration::from_secs(spec.work.request.timeout_seconds),
                )
                .await
                .map_err(|e| rejected(e.to_string()))?;
            }
            self.adapter.validate(&spec).await?;
            // What git will read as configuration is fixed here: the harness
            // runs next, and anything it changed in it is not the
            // repository's own.
            record.config_before = Some(crate::client::workspace::git::config_fingerprint(
                &spec.workspace.path,
            ));
            self.adapter.start(&spec).await
        }
        .await;
        let handle = match started {
            Ok(handle) => handle,
            Err(e) => return self.fail_before_spawn(session, &mut record, &spec, e).await,
        };
        record.state = JournalState::ProcessObservedRunning;
        record.process_id = Some(handle.process_id.clone());
        self.active.sync(&record);
        if self.journal.update(&record).is_err() {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        if self
            .protocol
            .report_start(
                session,
                StartReport {
                    attempt_id: record.attempt_id.clone(),
                    fencing_token: record.fencing_token,
                    phase: StartPhase::ProcessObservedRunning,
                    workspace_id: Some(spec.workspace.id.clone()),
                    base_revision: Some(spec.workspace.base_revision.clone()),
                    process_id: record.process_id.clone(),
                    environment_names: spec.work.request.environment.keys().cloned().collect(),
                    sandbox: self.adapter.sandbox(&handle).await,
                },
            )
            .await
            .is_err()
        {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        if install_skipped {
            let payload = serde_json::json!({"reason": crate::prepare::NO_INSTALL_REASON});
            self.submit_event(session, &mut record, "attempt.install_skipped", payload)
                .await;
        }

        if !self.renew_lease(session, &record).await {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        self.follow_to_end(session, record, &spec, handle, cancel, HashSet::new())
            .await
    }

    /// Sends one heartbeat for this attempt's lease and says whether the board
    /// still holds it to this runner and this fencing token. One answer covers
    /// every lease this runner holds: each cancellation it names goes to its
    /// own attempt, this one included.
    async fn renew_lease(&self, session: &RunnerSession, record: &AttemptJournal) -> bool {
        let request = self.heartbeat_request(session, record);
        let Ok(heartbeat) = self.protocol.heartbeat(session, request.clone()).await else {
            return false;
        };
        if heartbeat.protocol_version != ProtocolVersion::v1()
            || heartbeat.heartbeat_id != request.heartbeat_id
        {
            return false;
        }
        self.active.apply(&heartbeat.lease_results);
        heartbeat.lease_results.iter().any(|result| {
            result.attempt_id == record.attempt_id && result.fencing_token == record.fencing_token
        })
    }

    /// Everything from the first sight of the agent running to the terminal
    /// report: cancellation, the wait, evidence, the verifier, the publish and
    /// the report. A fresh run and a resumed one both end here, so there is one
    /// copy of it. `known_decisions` are the ids of the decisions the board
    /// already holds for this attempt, which only a resumed run has.
    async fn follow_to_end(
        &self,
        session: &RunnerSession,
        mut record: AttemptJournal,
        spec: &ExecutionSpec,
        handle: LocalRunHandle,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        known_decisions: HashSet<String>,
    ) -> Result<RunCycle, EngineError> {
        if *cancel.borrow_and_update() {
            record.state = JournalState::CancellationRequested;
            self.active.sync(&record);
            if self.journal.update(&record).is_err() {
                return self.quarantine_after_spawn(session, &record, &handle).await;
            }
            let evidence = match self.adapter.cancel(&handle).await {
                Ok(evidence) => evidence,
                Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
            };
            if evidence.observation != CancelObservation::ProcessStopped {
                // A terminal acknowledgement or an ambiguous adapter result
                // is not proof that this cancellation committed. Keep the
                // local record on the recovery/quarantine path instead of
                // fabricating a cancelled success.
                return self.report_or_retain_ambiguity(session, &record).await;
            }
            // Real evidence that a cancellation actually happened,
            // submitted through `AttemptDataProtocol` before the terminal
            // report — best-effort, see `submit_event`'s own doc comment.
            self.submit_cancellation_event(session, &mut record, &evidence)
                .await;
            // A partial diff is evidence too: read it before the workspace is
            // deleted below.
            let cancelled_reason = serde_json::json!({
                "observation": evidence.observation,
                "details": serde_json::Value::Object(evidence.details.clone()),
            });
            if let Some((scratch, staged)) = self
                .capture_attempt_evidence(
                    spec,
                    record.base_commit.as_deref(),
                    cancelled_reason,
                    serde_json::Value::Null,
                )
                .await
            {
                for artifact in &staged {
                    self.submit_staged_artifact(session, &record, artifact)
                        .await;
                }
                let _ = std::fs::remove_dir_all(scratch);
            }
            let report = CancellationReport {
                protocol_version: ProtocolVersion::v1(),
                runner_id: session.runner_id.clone(),
                cancellation_request_id: CancellationRequestId::new(format!(
                    "cancel:{}:{}",
                    record.attempt_id.as_str(),
                    record.fencing_token.0
                )),
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                observation: evidence.observation,
                observed_at: evidence.observed_at,
                details: evidence.details,
            };
            self.persist_pending_terminal_report(
                &mut record,
                PendingTerminalReportKind::Cancellation,
                &report,
            )?;
            let cycle = self
                .send_pending_terminal_report(session, &mut record)
                .await?;
            if matches!(cycle, RunCycle::Cancelled { .. }) {
                let _ = self.workspaces.cleanup(&spec.workspace);
            }
            return Ok(cycle);
        }

        let outcome = match self
            .wait_with_lease_renewal(
                session,
                &mut record,
                &handle,
                spec,
                &mut cancel,
                &known_decisions,
            )
            .await
        {
            Ok(outcome) => outcome,
            Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
        };
        if !matches!(
            outcome.terminal_state,
            AttemptState::Succeeded | AttemptState::Failed | AttemptState::Cancelled
        ) {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        let mut outcome = outcome.normalize_workspace_facts(&spec.workspace);
        // A changed git configuration ends the attempt `failed`: the evidence
        // diff is still captured for the person to see, but nothing is
        // verified or pushed from a repository the harness reconfigured.
        if let Some(path) = changed_config_path(
            record.config_before.as_deref().unwrap_or_default(),
            &crate::client::workspace::git::config_fingerprint(&spec.workspace.path),
        ) {
            outcome.terminal_state = AttemptState::Failed;
            outcome.terminal_reason = serde_json::json!({
                "code": "repository_config_changed",
                "path": path,
            });
        }
        let mut evidence_failure = None;
        let mut worktree_kept = false;
        // Read what the attempt changed before the workspace is deleted after
        // the report; the staged files ride `terminal_reason.artifacts`.
        let evidence_scratch = match self
            .capture_attempt_evidence(
                spec,
                record.base_commit.as_deref(),
                outcome.terminal_reason.clone(),
                outcome.evidence_usage(),
            )
            .await
        {
            Some((scratch, mut staged)) => {
                if outcome.terminal_state == AttemptState::Succeeded {
                    self.verify_attempt(session, &mut record, spec, &scratch, &mut staged)
                        .await;
                    if let Some(published) = self
                        .publish_attempt(
                            session,
                            &mut record,
                            spec,
                            &scratch,
                            &mut staged,
                            &mut outcome,
                        )
                        .await
                    {
                        outcome.actual_execution.additional.insert(
                            "git".to_owned(),
                            serde_json::to_value(&published).unwrap_or_default(),
                        );
                    }
                    // After the verifier, whose refusal of an empty change stays an
                    // event: the empty change is the headline, not that refusal.
                    Self::fail_without_change(spec, &scratch, &mut staged, &mut outcome);
                }
                if let Some(reason) = outcome.terminal_reason.as_object_mut() {
                    reason.insert("artifacts".to_owned(), serde_json::Value::Array(staged));
                }
                // A manifest that says `captured: false` because git could not
                // read the workspace means the files are unread; deleting the
                // workspace would delete them too. A provisioner that reads no
                // repository has nothing to lose.
                let manifest: serde_json::Value = std::fs::read(scratch.join("src/evidence.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default();
                if manifest["captured"] == serde_json::Value::Bool(false)
                    && manifest["reason"] == crate::evidence::EVIDENCE_REASON_GIT_UNREADABLE
                {
                    evidence_failure = Some(manifest["reason"].clone());
                    // A registered worktree is not moved (its gitdir pointer
                    // would break): it stays where it is, and is not cleaned up.
                    worktree_kept = spec.work.workspace_repository().is_ok_and(|repository| {
                        repository.workspace_mode == WorkspaceMode::LocalBranch
                    });
                    // An `in_place` folder is the user's own: never moved, and
                    // `cleanup` below only releases its lock.
                    let in_place = Self::in_repository(&spec.work)
                        || spec.work.workspace_repository().is_ok_and(|repository| {
                            repository.workspace_mode == WorkspaceMode::InPlace
                        });
                    let kept = if worktree_kept || in_place {
                        Ok(spec.workspace.path.clone())
                    } else {
                        self.workspaces.keep(&spec.workspace)
                    };
                    // A workspace that cannot be moved to quarantine stays
                    // where it is, and is not cleaned up: it holds the
                    // changes the evidence could not read.
                    let kept = kept.unwrap_or_else(|e| {
                        tracing::warn!(
                            attempt_id = %record.attempt_id.as_str(),
                            error = ?e,
                            "workspace could not be moved to quarantine; left in place"
                        );
                        worktree_kept = true;
                        spec.workspace.path.clone()
                    });
                    // A repository is the user's own: nothing of the run's is kept in it.
                    if !Self::in_repository(&spec.work)
                        && let Some(reason) = outcome.terminal_reason.as_object_mut()
                    {
                        reason.insert(
                            "workspace_kept_at".to_owned(),
                            serde_json::Value::String(kept.display().to_string()),
                        );
                    }
                }
                Some(scratch)
            }
            None => None,
        };
        // The runner's only call site for events and artifacts.
        // `outcome.terminal_reason` is the exact JSON the harness adapters
        // already produce (including the `artifact` key their `wait()`
        // implementations stage via `harness::artifact::ArtifactStager` —
        // see e.g. `codex.rs::stage_run_log`), so this reads real evidence,
        // never a fabricated summary. Runs before the completion report so
        // an operator inspecting the timeline after `succeeded` sees the
        // event/artifact already there.
        if let Some(scratch) = &evidence_scratch
            && let Ok(reason) =
                std::fs::read_to_string(scratch.join(crate::evidence::PLAN_INVALID_FILE))
        {
            self.submit_event(
                session,
                &mut record,
                "attempt.plan_invalid",
                serde_json::json!({ "reason": reason }),
            )
            .await;
        }
        if let Some(reason) = evidence_failure {
            self.submit_event(
                session,
                &mut record,
                "attempt.evidence_failed",
                serde_json::json!({ "reason": reason }),
            )
            .await;
        }
        self.submit_terminal_evidence(session, &mut record, &outcome)
            .await;
        if let Some(scratch) = evidence_scratch {
            let _ = std::fs::remove_dir_all(scratch);
        }
        let report = CompletionRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            completion_id: CompletionId::new(format!(
                "completion:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            )),
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            terminal_state: outcome.terminal_state.execution_state(),
            terminal_reason: outcome.terminal_reason,
            // NOT `outcome.final_checkpoint` (always `None` — no
            // adapter ever sets it). `complete_execution_result`'s own
            // compare-and-set requires this to equal the attempt row's
            // *current* `event_checkpoint` column exactly
            // (`crates/tack-db/src/repo/execution.rs`, the `UPDATE ... AND
            // event_checkpoint IS ?` guard) — and `submit_terminal_evidence`
            // above just moved that column from `NULL` to a real value by
            // submitting an event. Sending the adapter's stale `None` here
            // mismatches the row's now-current value and the update's
            // `rows_affected() != 1` branch turns every completion into a
            // `Conflict`, forever — proven the hard way against a live
            // server (`./scripts/smoke.sh`): the
            // event/artifact evidence reached
            // the server but the attempt's own completion never did.
            // `record.last_event_checkpoint` is exactly what the server
            // committed (or still `None` if nothing was ever submitted),
            // so it is always the value this compare-and-set expects.
            final_event_checkpoint: record.last_event_checkpoint.clone(),
            actual_execution: outcome.actual_execution,
            usage: outcome.usage,
        };
        self.persist_pending_terminal_report(
            &mut record,
            PendingTerminalReportKind::Completion,
            &report,
        )?;
        let cycle = self
            .send_pending_terminal_report(session, &mut record)
            .await?;
        if matches!(cycle, RunCycle::Completed { .. }) && !worktree_kept {
            let _ = self.workspaces.cleanup(&spec.workspace);
        }
        Ok(cycle)
    }

    fn heartbeat_request(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
    ) -> HeartbeatRequest {
        self.active.sync(record);
        let sent_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        HeartbeatRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            heartbeat_id: Self::heartbeat_id(
                record,
                &sent_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ),
            sent_at,
            available_capacity: self.active.free(),
            active_attempts: self.active.snapshot(),
        }
    }

    /// The logical heartbeat send time separates periodic sends for the same
    /// attempt/fence, while reusing the exact frozen payload preserves its
    /// idempotency ID for a retry.
    fn heartbeat_id(record: &AttemptJournal, sent_at: &str) -> String {
        let material = format!(
            "{}:{}:{sent_at}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        let opaque = material
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("hb_{opaque}")
    }

    /// Runs `self.adapter.wait(handle)` to completion, carrying any question
    /// the harness asks out to the operator through a decision, and sending
    /// the steps it reports to the attempt's timeline every
    /// [`PROGRESS_FLUSH_INTERVAL`] and once more when it ends.
    ///
    /// The lease is not renewed here: the runner's one ticker heartbeats every
    /// lease it holds each [`LEASE_RENEWAL_INTERVAL`], and flips `cancel` for
    /// an attempt whose answer asks for it. When it flips mid-run the harness
    /// is cancelled and the attempt ends `cancelled`.
    ///
    /// A question is polled for resolution on its own short timer, never past
    /// `spec`'s own `timeout_seconds`, the one deadline governing the whole
    /// attempt: a decision that outlives it is answered as the question's own
    /// deny option — same as a create that never succeeded or an answer
    /// channel that was dropped — so a harness waiting on stdin is never left
    /// waiting forever.
    async fn wait_with_lease_renewal(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        handle: &LocalRunHandle,
        spec: &ExecutionSpec,
        cancel: &mut tokio::sync::watch::Receiver<bool>,
        known_decisions: &HashSet<String>,
    ) -> Result<HarnessOutcome, HarnessError> {
        let prompt = spec
            .work
            .request
            .resolved_agent_profile
            .instructions
            .as_str();
        let timeout_seconds = i64::try_from(spec.work.request.timeout_seconds).unwrap_or(i64::MAX);
        let expires_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now())
            + chrono::Duration::seconds(timeout_seconds);

        let mut decisions = self.adapter.decision_channels(handle).await;
        let mut progress = self.adapter.progress_channel(handle).await;
        let mut steps: Vec<Progress> = Vec::new();
        let mut wait_future = self.adapter.wait(handle);
        let mut pending: Option<PendingDecision> = None;
        let mut stopped_by_cancel = false;
        let limit = runner_token_limit(&spec.work.request);
        let mut counted = 0u64;
        let mut stopped_by_limit = false;
        let mut flush = tokio::time::interval_at(
            tokio::time::Instant::now() + PROGRESS_FLUSH_INTERVAL,
            PROGRESS_FLUSH_INTERVAL,
        );
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let waiting_for_an_answer = pending.is_some();
            let mut state = DecisionState {
                decisions: &mut decisions,
                pending: &mut pending,
                expires_at,
                known: known_decisions,
            };
            tokio::select! {
                biased;
                outcome = &mut wait_future => {
                    // The read loop has ended, so whatever it reported is
                    // already in the channel.
                    if let Some(rest) = progress.as_mut() {
                        while let Ok(step) = rest.try_recv() {
                            steps.push(step);
                        }
                    }
                    self.submit_progress(session, record, &mut steps, prompt).await;
                    return outcome.map(|mut outcome| {
                        if stopped_by_cancel {
                            outcome.terminal_state = AttemptState::Cancelled;
                        }
                        if let (true, Some(limit)) = (stopped_by_limit, limit) {
                            token_limit_reached(&mut outcome, limit, counted);
                        }
                        outcome
                    });
                }
                Some(question) = next_question(state.decisions, state.pending.is_some()) => {
                    self.open_decision(session, record, &mut state, question, prompt).await;
                }
                Some(step) = next_step(&mut progress) => match step {
                    Some(step) => {
                        counted += step.usage.as_ref().and_then(tack_protocol::execution::Usage::counted).unwrap_or(0);
                        steps.push(step);
                        if limit.is_some_and(|limit| counted >= limit)
                            && !stopped_by_limit
                            && !stopped_by_cancel
                        {
                            stopped_by_limit = self.stop_at_limit(handle, record, counted).await;
                        }
                    }
                    None => progress = None,
                },
                _ = flush.tick(), if !steps.is_empty() => {
                    self.submit_progress(session, record, &mut steps, prompt).await;
                }
                Ok(()) = cancel.changed(), if !stopped_by_cancel && !stopped_by_limit => {
                    match self.adapter.cancel(handle).await {
                        Ok(evidence) => {
                            stopped_by_cancel =
                                evidence.observation == CancelObservation::ProcessStopped;
                        }
                        Err(error) => tracing::warn!(
                            %error,
                            attempt_id = record.attempt_id.as_str(),
                            "a cancellation asked mid-run could not reach the harness"
                        ),
                    }
                }
                () = tokio::time::sleep(DECISION_POLL_INTERVAL), if waiting_for_an_answer => {
                    self.advance_pending(session, record, &mut state).await;
                }
            }
        }
    }

    /// Stops the process through the cancel path once its steps have counted its limit;
    /// whether it stopped decides how the attempt ends.
    async fn stop_at_limit(
        &self,
        handle: &LocalRunHandle,
        record: &AttemptJournal,
        counted: u64,
    ) -> bool {
        match self.adapter.cancel(handle).await {
            Ok(evidence) => evidence.observation == CancelObservation::ProcessStopped,
            Err(error) => {
                tracing::warn!(
                    %error,
                    counted,
                    attempt_id = record.attempt_id.as_str(),
                    "the token limit was reached but the harness could not be stopped"
                );
                false
            }
        }
    }

    /// Whether the attempt works in the repository itself, with no copy made.
    fn in_repository(work: &ClaimedWork) -> bool {
        work.workspace_repository()
            .is_ok_and(|repository| repository.workspace_mode == WorkspaceMode::Repository)
    }

    fn workspace_from_journal(record: &AttemptJournal) -> Workspace {
        Workspace {
            attempt_id: record.attempt_id.clone(),
            id: record.workspace.workspace_id.clone(),
            path: record.workspace.path.clone(),
            base_revision: record.workspace.base_revision.clone(),
        }
    }
}

#[cfg(test)]
#[path = "engine/tests.rs"]
mod tests;
