//! Local pull-runner engine.
//!
//! The journal is persisted before `HarnessAdapter::start`. Once a local
//! process might exist, any failed fence/report/recovery operation is treated
//! as ambiguous and quarantined rather than retried.

use async_trait::async_trait;
use std::collections::BTreeMap;
use tack_orch::execution::{
    AttemptId as DomainAttemptId, FencingToken as DomainFencingToken, ProtocolVersion,
    RecoveryDetails as DomainRecoveryDetails, RecoveryDisposition, RecoveryJournalState,
    RecoveryKey, RecoveryObservation, RecoveryObservationRequest, RecoveryObservationResponse,
    RunnerId as DomainRunnerId,
};
use thiserror::Error;

use super::{
    ActiveAttempt, AttemptId, AttemptState, CancellationReport, CancellationRequestId,
    ClaimRequest, ClaimResult, ClaimedWork, ClaimedWorkError, CompletionId, CompletionReport,
    EnrollmentRequest, EnrollmentResponse, HeartbeatRequest, ProtocolClientError, PullProtocol,
    RefreshRequest, RefreshResponse, RunnerSession, StartPhase, StartReport, Timestamp,
    journal::{AttemptJournal, JournalError, JournalState, OwnerOnlyJournal},
    workspace::{Workspace, WorkspaceError, WorkspaceManager, WorktreeProvisioner},
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessError {
    #[error("harness rejected this execution")]
    Rejected,
    #[error("harness process operation failed")]
    Process,
    #[error("harness recovery observation is unavailable")]
    RecoveryUnavailable,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRunHandle {
    pub process_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelObservation {
    ProcessStopped,
    AlreadyTerminal,
    Ambiguous,
}

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
    pub terminal_reason: String,
    pub final_checkpoint: Option<super::Checkpoint>,
    pub actual_execution: tack_orch::execution::ActualExecution,
    pub usage: tack_orch::execution::Usage,
}

impl HarnessOutcome {
    /// Workspace ownership belongs to the engine, not a harness adapter. Normalize
    /// the adapter outcome before it is projected into the transport report so the
    /// report has exactly one source for actual execution and usage facts.
    fn normalize_workspace_facts(mut self, workspace: &Workspace) -> Self {
        self.actual_execution.workspace_id =
            tack_orch::execution::WorkspaceId::new(workspace.id.as_str());
        self.actual_execution.base_revision = workspace.base_revision.clone();
        self
    }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunCycle {
    NoWork,
    Completed { attempt_id: AttemptId },
    Cancelled { attempt_id: AttemptId },
    Quarantined { attempt_id: AttemptId },
    RecoveryPending { attempt_id: AttemptId },
}

pub struct RunnerEngine<P, A, W> {
    protocol: P,
    adapter: A,
    journal: OwnerOnlyJournal,
    workspaces: WorkspaceManager<W>,
}

impl<P, A, W> RunnerEngine<P, A, W>
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
        Self {
            protocol,
            adapter,
            journal,
            workspaces,
        }
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
            ClaimResult::NoWork { .. } => Ok(RunCycle::NoWork),
            ClaimResult::Work(work) => self.run_claimed(session, *work).await,
        }
    }

    pub async fn recover(&self, session: &RunnerSession) -> Result<Vec<RunCycle>, EngineError> {
        let mut outcomes = Vec::new();
        for mut record in self.journal.unresolved()? {
            let observation = match self.adapter.reconcile(&record).await {
                Ok(observation) => observation,
                Err(_) => RecoveryObservation::Ambiguous,
            };
            outcomes.push(
                self.report_recovery_and_apply_disposition(session, &mut record, observation)
                    .await?,
            );
        }
        Ok(outcomes)
    }

    async fn run_claimed(
        &self,
        session: &RunnerSession,
        work: ClaimedWork,
    ) -> Result<RunCycle, EngineError> {
        let repository = work.workspace_repository()?;
        if work.lease.runner_id != session.runner_id {
            return Err(EngineError::RunnerMismatch);
        }

        let workspace = self.workspaces.plan(&work.lease, &repository)?;
        let mut record = AttemptJournal::prepared(&work.lease, workspace.journal());
        // This is the hard ownership boundary: no worktree or adapter method
        // with a local side effect is called before create+fsync succeeds.
        self.journal.persist_before_spawn(&record)?;
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
                },
            )
            .await?;
        self.workspaces.provision(&workspace, &repository).await?;

        let spec = ExecutionSpec {
            work: work.clone(),
            workspace,
        };
        self.adapter.validate(&spec).await?;
        let handle = self.adapter.start(&spec).await?;
        record.state = JournalState::ProcessObservedRunning;
        record.process_id = Some(handle.process_id.clone());
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
                },
            )
            .await
            .is_err()
        {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }

        let heartbeat = HeartbeatRequest {
            heartbeat_id: format!(
                "heartbeat:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            ),
            available_capacity: 0,
            active_attempts: vec![ActiveAttempt {
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                state: AttemptState::Running,
                journal_state: record.state,
                last_event_checkpoint: record.last_event_checkpoint.clone(),
            }],
        };
        let heartbeat = match self.protocol.heartbeat(session, heartbeat).await {
            Ok(response) => response,
            Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
        };
        let lease = heartbeat.lease_results.into_iter().find(|result| {
            result.attempt_id == record.attempt_id && result.fencing_token == record.fencing_token
        });
        let Some(lease) = lease else {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        };

        if lease.cancellation_requested {
            record.state = JournalState::CancellationRequested;
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
            match self
                .protocol
                .report_cancellation(session, report.clone())
                .await
            {
                Ok(response)
                    if response.protocol_version == ProtocolVersion::v1()
                        && response.attempt_id == report.attempt_id
                        && response.cancellation_request_id == report.cancellation_request_id
                        && response.state == AttemptState::Cancelled => {}
                Ok(_) | Err(_) => return self.report_or_retain_ambiguity(session, &record).await,
            }
            record.state = JournalState::Reported;
            self.journal.update(&record)?;
            let _ = self.workspaces.cleanup(&spec.workspace);
            return Ok(RunCycle::Cancelled {
                attempt_id: record.attempt_id,
            });
        }

        let outcome = match self.adapter.wait(&handle).await {
            Ok(outcome) => outcome,
            Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
        };
        if !matches!(
            outcome.terminal_state,
            AttemptState::Succeeded | AttemptState::Failed | AttemptState::Cancelled
        ) {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        let outcome = outcome.normalize_workspace_facts(&spec.workspace);
        let report = CompletionReport {
            completion_id: CompletionId::new(format!(
                "completion:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            )),
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            terminal_state: outcome.terminal_state,
            terminal_reason: outcome.terminal_reason,
            final_checkpoint: outcome.final_checkpoint,
            actual_execution: outcome.actual_execution,
            usage: outcome.usage,
        };
        if self
            .protocol
            .report_completion(session, report)
            .await
            .is_err()
        {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        record.state = JournalState::Reported;
        self.journal.update(&record)?;
        let _ = self.workspaces.cleanup(&spec.workspace);
        Ok(RunCycle::Completed {
            attempt_id: record.attempt_id,
        })
    }

    async fn quarantine_after_spawn(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        handle: &LocalRunHandle,
    ) -> Result<RunCycle, EngineError> {
        // Report before moving evidence out of restart scans. If delivery
        // fails, the journal remains unresolved for a later recovery pass.
        let outcome = self.report_or_retain_ambiguity(session, record).await;
        let _ = self.adapter.cancel(handle).await;
        outcome
    }

    async fn report_or_retain_ambiguity(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
    ) -> Result<RunCycle, EngineError> {
        let mut record = record.clone();
        self.report_recovery_and_apply_disposition(
            session,
            &mut record,
            RecoveryObservation::Ambiguous,
        )
        .await
    }

    async fn report_recovery_and_apply_disposition(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        observation: RecoveryObservation,
    ) -> Result<RunCycle, EngineError> {
        let request = Self::recovery_request(session, record, observation);
        let response = match self
            .protocol
            .observe_recovery(session, request.clone())
            .await
        {
            Ok(response)
                if response.attempt_id == request.attempt_id
                    && response.recovery_key == request.recovery_key =>
            {
                response
            }
            Ok(_) | Err(_) => {
                // An absent or mismatched acknowledgement cannot settle local
                // process evidence; leave the journal in the restart scan.
                return Ok(RunCycle::RecoveryPending {
                    attempt_id: record.attempt_id.clone(),
                });
            }
        };

        self.apply_recovery_disposition(record, observation, response)
    }

    fn apply_recovery_disposition(
        &self,
        record: &mut AttemptJournal,
        observation: RecoveryObservation,
        response: RecoveryObservationResponse,
    ) -> Result<RunCycle, EngineError> {
        let settled = match response.disposition {
            RecoveryDisposition::SafePreSpawnRequeue => {
                observation == RecoveryObservation::ProcessStopped
                    && record.state == JournalState::Prepared
                    && record.process_id.is_none()
            }
            RecoveryDisposition::AlreadyTerminal => {
                observation == RecoveryObservation::ProcessStopped
            }
            RecoveryDisposition::NeedsOperator => false,
        };
        if settled {
            record.state = JournalState::RecoveryObserved;
            self.journal.update(record)?;
            return Ok(RunCycle::Completed {
                attempt_id: record.attempt_id.clone(),
            });
        }

        // `needs_operator` is always durable quarantine. A disposition that
        // contradicts local evidence is treated just as conservatively.
        self.journal.quarantine(record)?;
        Ok(RunCycle::Quarantined {
            attempt_id: record.attempt_id.clone(),
        })
    }

    fn recovery_request(
        session: &RunnerSession,
        record: &AttemptJournal,
        observation: RecoveryObservation,
    ) -> RecoveryObservationRequest {
        RecoveryObservationRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: DomainRunnerId::new(session.runner_id.as_str()),
            attempt_id: DomainAttemptId::new(record.attempt_id.as_str()),
            fencing_token: DomainFencingToken(record.fencing_token.0),
            recovery_key: RecoveryKey::new(format!(
                "recovery:{}:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0,
                match observation {
                    RecoveryObservation::ProcessStopped => "process_stopped",
                    RecoveryObservation::ProcessRunning => "process_running",
                    RecoveryObservation::Ambiguous => "ambiguous",
                }
            )),
            observation,
            details: DomainRecoveryDetails {
                journal_state: Self::recovery_journal_state(record.state),
                process_observed: record.process_id.is_some(),
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        }
    }

    const fn recovery_journal_state(state: JournalState) -> RecoveryJournalState {
        match state {
            JournalState::Prepared => RecoveryJournalState::Prepared,
            JournalState::ProcessObservedRunning => RecoveryJournalState::ProcessObservedRunning,
            JournalState::CancellationRequested => RecoveryJournalState::CancellationRequested,
            JournalState::RecoveryObserved => RecoveryJournalState::RecoveryObserved,
            JournalState::Reported => RecoveryJournalState::Reported,
            JournalState::Quarantined => RecoveryJournalState::Quarantined,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use super::*;
    use crate::client::{
        AttemptLease, CancellationResponse, ClaimRequestId, ClaimResult, ClaimedWork, FencingToken,
        LeaseResult, ProtocolClientError, RunnerCredential, RunnerId, Timestamp,
    };
    use tack_orch::execution::{
        AttemptId as DomainAttemptId, AttemptSnapshot, ExecutionRequestSnapshot,
        RecoveryDisposition, RecoveryJournalState, RecoveryObservationRequest,
        RecoveryObservationResponse, RunnerId as DomainRunnerId,
    };

    #[derive(Clone)]
    struct RecoveryResponseConfig {
        disposition: RecoveryDisposition,
        replayed: bool,
        additional: BTreeMap<String, serde_json::Value>,
    }

    #[derive(Clone, Copy)]
    enum CancellationAckMismatch {
        None,
        Attempt,
        Request,
        State,
    }

    #[derive(Clone, Copy)]
    struct CancellationResponseConfig {
        replayed: bool,
        mismatch: CancellationAckMismatch,
    }

    #[derive(Clone)]
    struct FakeProtocol {
        claim: Arc<Mutex<Option<ClaimResult>>>,
        cancellation_requested: bool,
        stale_completion: bool,
        fail_running_start: bool,
        fail_cancellation_report: bool,
        start_reports: Arc<AtomicUsize>,
        reported_starts: Arc<Mutex<Vec<StartReport>>>,
        reported_heartbeats: Arc<Mutex<Vec<super::super::HeartbeatResponse>>>,
        completion_reports: Arc<AtomicUsize>,
        reported_completions: Arc<Mutex<Vec<CompletionReport>>>,
        cancellation_reports: Arc<AtomicUsize>,
        reported_cancellations: Arc<Mutex<Vec<CancellationReport>>>,
        cancellation_response: Arc<Mutex<CancellationResponseConfig>>,
        recovery_reports: Arc<AtomicUsize>,
        reported_recoveries: Arc<Mutex<Vec<RecoveryObservationRequest>>>,
        recovery_failures_remaining: Arc<AtomicUsize>,
        recovery_response: Arc<Mutex<RecoveryResponseConfig>>,
        refresh_requests: Arc<Mutex<Vec<RefreshRequest>>>,
    }

    #[async_trait]
    impl PullProtocol for FakeProtocol {
        async fn enroll(
            &self,
            _credential: &crate::EnrollmentCredential,
            _request: EnrollmentRequest,
        ) -> Result<EnrollmentResponse, ProtocolClientError> {
            Ok(EnrollmentResponse {
                session: session(),
                heartbeat_interval: std::time::Duration::from_secs(15),
                lease_duration: std::time::Duration::from_secs(60),
                server_time: Timestamp::new("2026-08-06T12:00:01Z"),
            })
        }

        async fn refresh(
            &self,
            _session: &RunnerSession,
            request: RefreshRequest,
        ) -> Result<RefreshResponse, ProtocolClientError> {
            self.refresh_requests
                .lock()
                .expect("fake protocol lock")
                .push(request);
            Ok(RefreshResponse {
                session: RunnerSession::new(
                    RunnerId::new("runner"),
                    RunnerCredential::new("rotated-never-log"),
                    Timestamp::new("2026-08-06T13:00:00Z"),
                ),
                accepted_at: Timestamp::new("2026-08-06T12:30:00Z"),
            })
        }

        async fn claim(
            &self,
            _session: &RunnerSession,
            _request: ClaimRequest,
        ) -> Result<ClaimResult, ProtocolClientError> {
            self.claim
                .lock()
                .expect("fake protocol lock")
                .take()
                .ok_or(ProtocolClientError::Rejected)
        }

        async fn heartbeat(
            &self,
            _session: &RunnerSession,
            request: HeartbeatRequest,
        ) -> Result<super::super::HeartbeatResponse, ProtocolClientError> {
            let active = request
                .active_attempts
                .into_iter()
                .next()
                .expect("active attempt");
            let response = super::super::HeartbeatResponse {
                heartbeat_id: request.heartbeat_id,
                accepted_at: Timestamp::new("2026-08-06T12:20:16Z"),
                lease_results: vec![LeaseResult {
                    attempt_id: active.attempt_id,
                    fencing_token: active.fencing_token,
                    lease_expires_at: Timestamp::new("2026-08-06T12:21:16Z"),
                    cancellation_requested: self.cancellation_requested,
                }],
            };
            self.reported_heartbeats
                .lock()
                .expect("fake protocol lock")
                .push(response.clone());
            Ok(response)
        }

        async fn report_start(
            &self,
            _session: &RunnerSession,
            report: StartReport,
        ) -> Result<(), ProtocolClientError> {
            self.start_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_starts
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if self.fail_running_start && report.phase == StartPhase::ProcessObservedRunning {
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
            self.completion_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_completions
                .lock()
                .expect("fake protocol lock")
                .push(_report.clone());
            if self.stale_completion {
                Err(ProtocolClientError::StaleLease)
            } else {
                Ok(())
            }
        }

        async fn report_cancellation(
            &self,
            _session: &RunnerSession,
            report: CancellationReport,
        ) -> Result<CancellationResponse, ProtocolClientError> {
            self.cancellation_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_cancellations
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if self.fail_cancellation_report {
                Err(ProtocolClientError::StaleLease)
            } else {
                let config = *self
                    .cancellation_response
                    .lock()
                    .expect("fake protocol lock");
                let mut response = CancellationResponse {
                    protocol_version: ProtocolVersion::v1(),
                    attempt_id: report.attempt_id,
                    cancellation_request_id: report.cancellation_request_id,
                    state: AttemptState::Cancelled,
                    replayed: config.replayed,
                    committed_at: Timestamp::new("2026-08-06T12:24:01Z"),
                };
                match config.mismatch {
                    CancellationAckMismatch::None => {}
                    CancellationAckMismatch::Attempt => {
                        response.attempt_id = AttemptId::new("wrong-attempt")
                    }
                    CancellationAckMismatch::Request => {
                        response.cancellation_request_id =
                            CancellationRequestId::new("wrong-cancel")
                    }
                    CancellationAckMismatch::State => response.state = AttemptState::Running,
                }
                Ok(response)
            }
        }

        async fn observe_recovery(
            &self,
            _session: &RunnerSession,
            report: RecoveryObservationRequest,
        ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
            self.recovery_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_recoveries
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            let remaining = self.recovery_failures_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.recovery_failures_remaining
                    .fetch_sub(1, Ordering::SeqCst);
                Err(ProtocolClientError::Transport)
            } else {
                let config = self
                    .recovery_response
                    .lock()
                    .expect("fake protocol lock")
                    .clone();
                let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
                    "../../../docs/contracts/runner-v1/recovery-observation.response.json"
                ))
                .expect("recovery response fixture");
                response.attempt_id = report.attempt_id;
                response.recovery_key = report.recovery_key;
                response.disposition = config.disposition;
                response.replayed = config.replayed;
                response.additional = config.additional;
                Ok(response)
            }
        }
    }

    #[derive(Clone)]
    struct FakeAdapter {
        expected_journal: PathBuf,
        start_after_journal: Arc<AtomicBool>,
        cancel_calls: Arc<AtomicUsize>,
        cancellation_evidence: CancellationEvidence,
        recovery_observation: RecoveryObservation,
        reconcile_fails: bool,
        completion_actual_execution: tack_orch::execution::ActualExecution,
    }

    #[async_trait]
    impl HarnessAdapter for FakeAdapter {
        async fn validate(&self, _spec: &ExecutionSpec) -> Result<(), HarnessError> {
            Ok(())
        }

        async fn start(&self, _spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
            self.start_after_journal
                .store(self.expected_journal.exists(), Ordering::SeqCst);
            Ok(LocalRunHandle {
                process_id: "fake-process".into(),
            })
        }

        async fn cancel(
            &self,
            _handle: &LocalRunHandle,
        ) -> Result<CancellationEvidence, HarnessError> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.cancellation_evidence.clone())
        }

        async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
            Ok(HarnessOutcome {
                terminal_state: AttemptState::Succeeded,
                terminal_reason: "completed".into(),
                final_checkpoint: None,
                actual_execution: self.completion_actual_execution.clone(),
                usage: usage(),
            })
        }

        async fn reconcile(
            &self,
            _journal: &AttemptJournal,
        ) -> Result<RecoveryObservation, HarnessError> {
            if self.reconcile_fails {
                Err(HarnessError::RecoveryUnavailable)
            } else {
                Ok(self.recovery_observation)
            }
        }
    }

    #[derive(Clone)]
    struct FakeWorktree {
        expected_journal: PathBuf,
        provision_after_journal: Arc<AtomicBool>,
    }

    #[async_trait]
    impl WorktreeProvisioner for FakeWorktree {
        async fn provision(
            &self,
            _workspace: &Workspace,
            _repository: &super::super::RepositorySpec,
        ) -> Result<(), WorkspaceError> {
            self.provision_after_journal
                .store(self.expected_journal.exists(), Ordering::SeqCst);
            Ok(())
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tack-runner-engine-{label}-{}", std::process::id()))
    }

    fn session() -> RunnerSession {
        RunnerSession::new(
            RunnerId::new("runner"),
            RunnerCredential::new("never-log"),
            Timestamp::new("2026-08-06T13:00:00Z"),
        )
    }

    fn capabilities() -> tack_orch::execution::RunnerCapabilities {
        serde_json::from_str(
            r#"{
                "runner_version":"test-runner",
                "reported_at":"2026-08-06T12:00:00Z",
                "labels":{},
                "concurrency":{"total":1,"available":1},
                "harnesses":[],
                "features":{
                    "cancel":{"support":"supported","reason":null},
                    "resume":{"support":"unsupported","reason":"no resume"},
                    "decisions":{"support":"supported","reason":null},
                    "artifacts":{"support":"supported","reason":null},
                    "usage":{"support":"advisory","reason":"partial"}
                },
                "limits":{"event_payload_bytes_max":65536,"artifact_content_bytes_max":52428800}
            }"#,
        )
        .expect("capabilities fixture")
    }

    fn actual_execution() -> tack_orch::execution::ActualExecution {
        serde_json::from_str(
            r#"{
                "harness_kind":"fake",
                "harness_version":"1.0.0",
                "model_provider":"test-provider",
                "model_id":"test-model",
                "model_observation_source":"harness_reported",
                "capability_snapshot":{
                    "cancel":{"support":"supported","reason":null},
                    "resume":{"support":"unsupported","reason":"no resume"},
                    "decisions":{"support":"supported","reason":null},
                    "artifacts":{"support":"supported","reason":null},
                    "usage":{"support":"advisory","reason":"partial"}
                },
                "workspace_id":"ws_617474656d7074",
                "base_revision":"revision",
                "started_at":"2026-08-06T12:20:00Z",
                "ended_at":"2026-08-06T12:25:00Z"
            }"#,
        )
        .expect("actual execution fixture")
    }

    fn usage() -> tack_orch::execution::Usage {
        serde_json::from_str(
            r#"{
                "tokens_in":{"value":1,"source":"measured"},
                "tokens_out":{"value":2,"source":"measured"},
                "duration_ms":{"value":3,"source":"measured"},
                "cost_usd":{"value":null,"source":"not_measured"}
            }"#,
        )
        .expect("usage fixture")
    }

    fn mismatched_actual_execution() -> tack_orch::execution::ActualExecution {
        let mut actual = actual_execution();
        actual.workspace_id = tack_orch::execution::WorkspaceId::new("ws_adapter_mismatch");
        actual.base_revision = "adapter-mismatch".into();
        actual
    }

    fn work() -> ClaimedWork {
        let claim: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/claim.response.json"
        ))
        .expect("claim fixture");
        let mut request: ExecutionRequestSnapshot =
            serde_json::from_value(claim["request"].clone()).expect("request fixture");
        let mut attempt: AttemptSnapshot =
            serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
        // Keep the production fixture's full shape while aligning the shared
        // facts with this focused engine fixture.
        request.repository.base_revision = "revision".into();
        attempt.attempt_id = DomainAttemptId::new("attempt");
        attempt.runner_id = DomainRunnerId::new("runner");
        attempt.base_revision = request.repository.base_revision.clone();
        ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(7),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
                expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
            },
            request,
            attempt,
        }
    }

    fn protocol(
        work: ClaimedWork,
        cancellation_requested: bool,
        stale_completion: bool,
    ) -> FakeProtocol {
        FakeProtocol {
            claim: Arc::new(Mutex::new(Some(ClaimResult::Work(Box::new(work))))),
            cancellation_requested,
            stale_completion,
            fail_running_start: false,
            fail_cancellation_report: false,
            start_reports: Arc::new(AtomicUsize::new(0)),
            reported_starts: Arc::new(Mutex::new(Vec::new())),
            reported_heartbeats: Arc::new(Mutex::new(Vec::new())),
            completion_reports: Arc::new(AtomicUsize::new(0)),
            reported_completions: Arc::new(Mutex::new(Vec::new())),
            cancellation_reports: Arc::new(AtomicUsize::new(0)),
            reported_cancellations: Arc::new(Mutex::new(Vec::new())),
            cancellation_response: Arc::new(Mutex::new(CancellationResponseConfig {
                replayed: false,
                mismatch: CancellationAckMismatch::None,
            })),
            recovery_reports: Arc::new(AtomicUsize::new(0)),
            reported_recoveries: Arc::new(Mutex::new(Vec::new())),
            recovery_failures_remaining: Arc::new(AtomicUsize::new(0)),
            recovery_response: Arc::new(Mutex::new(RecoveryResponseConfig {
                disposition: RecoveryDisposition::SafePreSpawnRequeue,
                replayed: false,
                additional: BTreeMap::new(),
            })),
            refresh_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn adapter(expected_journal: PathBuf) -> FakeAdapter {
        FakeAdapter {
            expected_journal,
            start_after_journal: Arc::new(AtomicBool::new(false)),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            cancellation_evidence: CancellationEvidence {
                observation: CancelObservation::ProcessStopped,
                observed_at: Timestamp::new("2026-08-06T12:24:00Z"),
                details: serde_json::Map::from_iter([
                    ("exit_code".into(), serde_json::json!(130)),
                    ("signal".into(), serde_json::json!("SIGTERM")),
                ]),
            },
            recovery_observation: RecoveryObservation::ProcessStopped,
            reconcile_fails: false,
            completion_actual_execution: actual_execution(),
        }
    }

    #[test]
    fn fixture_shaped_claim_preserves_snapshots_and_rejects_divergent_workspace_facts() {
        let work = work();
        assert_eq!(
            work.request
                .requested_model_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("opaque/model-alpha")
        );
        assert_eq!(
            work.attempt.request_id.as_str(),
            work.request.request_id.as_str()
        );
        assert_eq!(
            work.attempt.attempt_id.as_str(),
            work.lease.attempt_id.as_str()
        );
        let repository = work.workspace_repository().expect("matching claim facts");
        assert_eq!(repository.remote, "https://example.invalid/org/repo.git");
        assert_eq!(repository.base_revision, "revision");

        let mut revision_mismatch = work.clone();
        revision_mismatch.attempt.base_revision = "other-revision".into();
        assert!(matches!(
            revision_mismatch.workspace_repository(),
            Err(super::super::ClaimedWorkError::RepositoryRevisionMismatch)
        ));

        let mut lease_mismatch = work;
        lease_mismatch.lease.fencing_token = FencingToken(8);
        assert!(matches!(
            lease_mismatch.workspace_repository(),
            Err(super::super::ClaimedWorkError::AttemptLeaseMismatch)
        ));
    }

    fn claim_request() -> ClaimRequest {
        ClaimRequest {
            claim_request_id: ClaimRequestId::new("claim"),
            available_capacity: 1,
            wait: std::time::Duration::ZERO,
        }
    }

    #[tokio::test]
    async fn refresh_carries_capabilities_and_returns_expiring_session() {
        let root = temporary_root("refresh");
        let journal = OwnerOnlyJournal::new(&root);
        let protocol = protocol(work(), false, false);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(&root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        let response = engine
            .refresh(
                &session(),
                RefreshRequest {
                    runner_name: "runner".into(),
                    runner_version: "test-runner".into(),
                    rotate_credential: true,
                    capabilities: capabilities(),
                },
            )
            .await
            .expect("refresh");

        assert_eq!(
            response.session.credential_expires_at().as_str(),
            "2026-08-06T13:00:00Z"
        );
        assert_eq!(response.accepted_at.as_str(), "2026-08-06T12:30:00Z");
        let refreshes = protocol
            .refresh_requests
            .lock()
            .expect("fake protocol lock");
        assert_eq!(refreshes.len(), 1);
        assert!(refreshes[0].rotate_credential);
        assert_eq!(refreshes[0].capabilities.runner_version, "test-runner");
    }

    #[tokio::test]
    async fn cancellation_is_coordinated_after_journal_precedes_spawn() {
        let root = temporary_root("cancel");
        let journal = OwnerOnlyJournal::new(&root);
        let expected = journal.journal_path(&AttemptId::new("attempt"));
        let protocol = protocol(work(), true, false);
        let adapter = adapter(expected);
        let started = Arc::clone(&adapter.start_after_journal);
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let provisioned = Arc::new(AtomicBool::new(false));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(&root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::clone(&provisioned),
                },
            ),
        );

        let result = engine
            .run_once(&session(), claim_request())
            .await
            .expect("cycle");
        assert!(matches!(result, RunCycle::Cancelled { .. }));
        assert!(
            started.load(Ordering::SeqCst),
            "journal existed before adapter start"
        );
        assert!(
            provisioned.load(Ordering::SeqCst),
            "journal existed before worktree provision"
        );
        assert_eq!(protocol.start_reports.load(Ordering::SeqCst), 2);
        let start_reports = protocol.reported_starts.lock().expect("fake protocol lock");
        let preparing = start_reports
            .iter()
            .find(|report| report.phase == StartPhase::Preparing)
            .expect("preparing report");
        assert_eq!(
            preparing.workspace_id.as_ref().map(|id| id.as_str()),
            Some("ws_617474656d7074")
        );
        assert_eq!(preparing.base_revision.as_deref(), Some("revision"));
        assert_eq!(preparing.process_id, None);
        let running = start_reports
            .iter()
            .find(|report| report.phase == StartPhase::ProcessObservedRunning)
            .expect("running report");
        assert_eq!(
            running.workspace_id.as_ref().map(|id| id.as_str()),
            Some("ws_617474656d7074")
        );
        assert_eq!(running.base_revision.as_deref(), Some("revision"));
        assert_eq!(running.process_id.as_deref(), Some("fake-process"));
        let heartbeats = protocol
            .reported_heartbeats
            .lock()
            .expect("fake protocol lock");
        assert_eq!(heartbeats.len(), 1);
        assert_eq!(heartbeats[0].heartbeat_id, "heartbeat:attempt:7");
        assert_eq!(heartbeats[0].accepted_at.as_str(), "2026-08-06T12:20:16Z");
        assert_eq!(
            heartbeats[0].lease_results[0].lease_expires_at.as_str(),
            "2026-08-06T12:21:16Z"
        );
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        let cancellation_reports = protocol
            .reported_cancellations
            .lock()
            .expect("fake protocol lock");
        assert_eq!(cancellation_reports.len(), 1);
        let report = &cancellation_reports[0];
        assert_eq!(report.protocol_version.as_u16(), 1);
        assert_eq!(report.runner_id.as_str(), "runner");
        assert_eq!(report.attempt_id.as_str(), "attempt");
        assert_eq!(report.fencing_token.0, 7);
        assert_eq!(report.cancellation_request_id.as_str(), "cancel:attempt:7");
        assert_eq!(report.observation, CancelObservation::ProcessStopped);
        assert_eq!(report.observed_at.as_str(), "2026-08-06T12:24:00Z");
        assert_eq!(report.details["exit_code"], serde_json::json!(130));
        assert_eq!(report.details["signal"], serde_json::json!("SIGTERM"));
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn replayed_cancellation_ack_settles_stopped_evidence() {
        let root = temporary_root("replayed-cancellation");
        let journal = OwnerOnlyJournal::new(&root);
        let protocol = protocol(work(), true, false);
        protocol
            .cancellation_response
            .lock()
            .expect("fake protocol lock")
            .replayed = true;
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Cancelled { .. }
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::Reported
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn mismatched_cancellation_ack_is_never_cancelled_success() {
        for (label, mismatch) in [
            ("cancel-mismatch-attempt", CancellationAckMismatch::Attempt),
            ("cancel-mismatch-request", CancellationAckMismatch::Request),
            ("cancel-mismatch-state", CancellationAckMismatch::State),
        ] {
            let root = temporary_root(label);
            let journal = OwnerOnlyJournal::new(&root);
            let protocol = protocol(work(), true, false);
            protocol
                .cancellation_response
                .lock()
                .expect("fake protocol lock")
                .mismatch = mismatch;
            let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .run_once(&session(), claim_request())
                    .await
                    .expect("cycle"),
                RunCycle::Quarantined { .. }
            ));
            assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
            assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
            assert!(journal.unresolved().expect("scanned journal").is_empty());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn non_stopped_cancellation_evidence_skips_cancellation_transport() {
        for (label, observation) in [
            (
                "cancel-already-terminal",
                CancelObservation::AlreadyTerminal,
            ),
            ("cancel-ambiguous", CancelObservation::Ambiguous),
        ] {
            let root = temporary_root(label);
            let journal = OwnerOnlyJournal::new(&root);
            let protocol = protocol(work(), true, false);
            let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            adapter.cancellation_evidence.observation = observation;
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .run_once(&session(), claim_request())
                    .await
                    .expect("cycle"),
                RunCycle::Quarantined { .. }
            ));
            assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 0);
            assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
            assert!(journal.unresolved().expect("scanned journal").is_empty());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn stale_fence_quarantines_ambiguous_process_and_stops_reporting() {
        let root = temporary_root("stale");
        let journal = OwnerOnlyJournal::new(&root);
        let protocol = protocol(work(), false, true);
        let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        adapter.completion_actual_execution = mismatched_actual_execution();
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let provisioned = Arc::new(AtomicBool::new(false));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: provisioned,
                },
            ),
        );

        let result = engine
            .run_once(&session(), claim_request())
            .await
            .expect("cycle");
        assert!(matches!(result, RunCycle::Quarantined { .. }));
        assert_eq!(
            protocol.completion_reports.load(Ordering::SeqCst),
            1,
            "no retry after stale fence"
        );
        let completions = protocol
            .reported_completions
            .lock()
            .expect("fake protocol lock");
        assert_eq!(completions.len(), 1);
        assert_eq!(
            completions[0].actual_execution.workspace_id.as_str(),
            "ws_617474656d7074"
        );
        assert_eq!(completions[0].actual_execution.base_revision, "revision");
        assert_eq!(completions[0].usage.duration_ms.value, Some(3));
        assert_eq!(completions[0].terminal_state, AttemptState::Succeeded);
        assert_eq!(completions[0].terminal_reason, "completed");
        assert_eq!(completions[0].final_checkpoint, None);
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert!(journal.unresolved().expect("scan").is_empty());
        assert!(
            root.join("quarantine")
                .read_dir()
                .expect("quarantine")
                .next()
                .is_some()
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn restart_reports_unresolved_journal_observation_without_respawn() {
        let root = temporary_root("recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("persist prior journal");
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        let outcomes = engine.recover(&session()).await.expect("recover");
        assert!(matches!(outcomes.as_slice(), [RunCycle::Completed { .. }]));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        let recoveries = protocol
            .reported_recoveries
            .lock()
            .expect("fake protocol lock");
        assert_eq!(recoveries.len(), 1);
        assert_eq!(
            recoveries[0].recovery_key.as_str(),
            "recovery:attempt:7:process_stopped"
        );
        assert_eq!(recoveries[0].protocol_version.as_u16(), 1);
        assert_eq!(recoveries[0].runner_id.as_str(), "runner");
        assert_eq!(recoveries[0].attempt_id.as_str(), "attempt");
        assert_eq!(recoveries[0].fencing_token.0, 7);
        assert_eq!(
            recoveries[0].details.journal_state,
            RecoveryJournalState::Prepared
        );
        assert!(!recoveries[0].details.process_observed);
        assert!(recoveries[0].additional.is_empty());
        assert!(recoveries[0].details.additional.is_empty());
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::RecoveryObserved
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn needs_operator_response_durably_quarantines_stopped_pre_spawn_recovery() {
        let root = temporary_root("needs-operator-recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        protocol
            .recovery_response
            .lock()
            .expect("fake protocol lock")
            .disposition = RecoveryDisposition::NeedsOperator;
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        assert!(
            root.join("quarantine")
                .read_dir()
                .expect("quarantine")
                .next()
                .is_some(),
            "operator disposition moves evidence out of restart scans"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn replayed_already_terminal_response_settles_only_stopped_evidence() {
        let root = temporary_root("terminal-replay-recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        {
            let mut response = protocol
                .recovery_response
                .lock()
                .expect("fake protocol lock");
            response.disposition = RecoveryDisposition::AlreadyTerminal;
            response.replayed = true;
            response
                .additional
                .insert("future_response_field".into(), serde_json::json!(42));
        }
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::RecoveryObserved
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn already_terminal_response_quarantines_running_or_ambiguous_evidence() {
        for (label, running, reconcile_fails) in [
            ("terminal-running-recovery", true, false),
            ("terminal-ambiguous-recovery", false, true),
        ] {
            let root = temporary_root(label);
            let journal = OwnerOnlyJournal::new(&root);
            let lease = work().lease;
            let record = AttemptJournal::prepared(
                &lease,
                super::super::journal::WorkspaceJournal {
                    workspace_id: super::super::WorkspaceId::new("ws"),
                    path: root.join("workspaces/attempt"),
                    base_revision: "revision".into(),
                },
            );
            journal
                .persist_before_spawn(&record)
                .expect("prior journal");
            let protocol = protocol(work(), false, false);
            protocol
                .recovery_response
                .lock()
                .expect("fake protocol lock")
                .disposition = RecoveryDisposition::AlreadyTerminal;
            let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            adapter.reconcile_fails = reconcile_fails;
            if running {
                adapter.recovery_observation = RecoveryObservation::ProcessRunning;
            }
            let engine = RunnerEngine::new(
                protocol,
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .recover(&session())
                    .await
                    .expect("recovery")
                    .as_slice(),
                [RunCycle::Quarantined { .. }]
            ));
            assert!(journal.unresolved().expect("scanned journal").is_empty());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn safe_requeue_response_never_settles_post_spawn_stopped_evidence() {
        let root = temporary_root("safe-post-spawn-recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let mut record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        record.state = JournalState::ProcessObservedRunning;
        record.process_id = Some("former-process".into());
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol,
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn post_spawn_start_ack_failure_reports_ambiguity_and_quarantines() {
        let root = temporary_root("start-ack");
        let journal = OwnerOnlyJournal::new(&root);
        let mut protocol = protocol(work(), false, false);
        protocol.fail_running_start = true;
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(protocol.start_reports.load(Ordering::SeqCst), 2);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn cancellation_report_failure_reports_ambiguity_and_quarantines() {
        let root = temporary_root("cancel-report");
        let journal = OwnerOnlyJournal::new(&root);
        let mut protocol = protocol(work(), true, false);
        protocol.fail_cancellation_report = true;
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(&root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn failed_ambiguity_delivery_is_retried_on_restart_without_respawn() {
        let root = temporary_root("retry-recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        protocol
            .recovery_failures_remaining
            .store(1, Ordering::SeqCst);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let never_started = Arc::clone(&adapter.start_after_journal);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("first recovery")
                .as_slice(),
            [RunCycle::RecoveryPending { .. }]
        ));
        assert_eq!(journal.unresolved().expect("still scanned").len(), 1);
        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("second recovery")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 2);
        assert!(
            !never_started.load(Ordering::SeqCst),
            "recovery never respawns"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn running_recovery_observation_is_quarantined_not_completed() {
        let root = temporary_root("running-recovery");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        adapter.recovery_observation = RecoveryObservation::ProcessRunning;
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn duplicate_claim_for_quarantined_attempt_cannot_start_again() {
        let root = temporary_root("duplicate-quarantine");
        let journal = OwnerOnlyJournal::new(&root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        journal.quarantine(&record).expect("quarantine");
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let started = Arc::clone(&adapter.start_after_journal);
        let engine = RunnerEngine::new(
            protocol,
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(&root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine.run_once(&session(), claim_request()).await,
            Err(EngineError::Journal(JournalError::AlreadyExists))
        ));
        assert!(
            !started.load(Ordering::SeqCst),
            "duplicate claim did not start a process"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn post_spawn_journal_update_failure_reports_ambiguity_and_cancels() {
        let root = temporary_root("journal-update");
        let journal = OwnerOnlyJournal::new(&root);
        journal.fail_next_update_for_test();
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(&root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }
}
