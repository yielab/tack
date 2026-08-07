//! Local pull-runner engine.
//!
//! The journal is persisted before `HarnessAdapter::start`. Once a local
//! process might exist, any failed fence/report/recovery operation is treated
//! as ambiguous and quarantined rather than retried.

use async_trait::async_trait;
use thiserror::Error;

use super::{
    ActiveAttempt, AttemptId, AttemptState, CancellationReport, CancellationRequestId,
    ClaimRequest, ClaimResult, ClaimedWork, CompletionId, CompletionReport, EnrollmentRequest,
    EnrollmentResponse, HeartbeatRequest, ProtocolClientError, PullProtocol, RecoveryReport,
    RunnerSession, StartPhase, StartReport,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelObservation {
    ProcessStopped,
    AlreadyTerminal,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservation {
    ProcessRunning,
    ProcessStopped,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessOutcome {
    pub terminal_state: AttemptState,
    pub terminal_reason: String,
    pub final_checkpoint: Option<super::Checkpoint>,
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
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancelObservation, HarnessError>;
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
            ClaimResult::Work(work) => self.run_claimed(session, work).await,
        }
    }

    pub async fn recover(&self, session: &RunnerSession) -> Result<Vec<RunCycle>, EngineError> {
        let mut outcomes = Vec::new();
        for mut record in self.journal.unresolved()? {
            let observation = match self.adapter.reconcile(&record).await {
                Ok(observation) => observation,
                Err(_) => {
                    outcomes.push(self.report_or_retain_ambiguity(session, &record).await?);
                    continue;
                }
            };
            if !matches!(observation, RecoveryObservation::ProcessStopped) {
                outcomes.push(self.report_or_retain_ambiguity(session, &record).await?);
                continue;
            }
            let report = RecoveryReport {
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                observation,
            };
            if self
                .protocol
                .observe_recovery(session, report)
                .await
                .is_err()
            {
                // The server has not acknowledged the safe observation, so
                // keep the original unresolved evidence for a retry.
                outcomes.push(RunCycle::RecoveryPending {
                    attempt_id: record.attempt_id,
                });
                continue;
            }
            record.state = JournalState::RecoveryObserved;
            self.journal.update(&record)?;
            outcomes.push(RunCycle::Completed {
                attempt_id: record.attempt_id,
            });
        }
        Ok(outcomes)
    }

    async fn run_claimed(
        &self,
        session: &RunnerSession,
        work: ClaimedWork,
    ) -> Result<RunCycle, EngineError> {
        if work.lease.runner_id != session.runner_id {
            return Err(EngineError::RunnerMismatch);
        }

        let workspace = self.workspaces.plan(&work.lease, &work.repository)?;
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
                    process_id: None,
                },
            )
            .await?;
        self.workspaces
            .provision(&workspace, &work.repository)
            .await?;

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
            let observation = match self.adapter.cancel(&handle).await {
                Ok(observation) => observation,
                Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
            };
            let report = CancellationReport {
                cancellation_request_id: CancellationRequestId::new(format!(
                    "cancel:{}:{}",
                    record.attempt_id.as_str(),
                    record.fencing_token.0
                )),
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                observation,
            };
            if self
                .protocol
                .report_cancellation(session, report)
                .await
                .is_err()
            {
                return self.quarantine_after_spawn(session, &record, &handle).await;
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
        let report = CompletionReport {
            completion_id: CompletionId::new(format!(
                "completion:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            )),
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            outcome,
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
        let delivered = self
            .protocol
            .observe_recovery(
                session,
                RecoveryReport {
                    attempt_id: record.attempt_id.clone(),
                    fencing_token: record.fencing_token,
                    observation: RecoveryObservation::Ambiguous,
                },
            )
            .await
            .is_ok();
        if !delivered {
            return Ok(RunCycle::RecoveryPending {
                attempt_id: record.attempt_id.clone(),
            });
        }
        self.journal.quarantine(record)?;
        Ok(RunCycle::Quarantined {
            attempt_id: record.attempt_id.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use super::*;
    use crate::client::{
        AttemptLease, ClaimRequestId, ClaimResult, ClaimedWork, FencingToken, LeaseResult,
        ProtocolClientError, RepositorySpec, RunnerCredential, RunnerId, Timestamp,
    };

    #[derive(Clone)]
    struct FakeProtocol {
        claim: Arc<Mutex<Option<ClaimResult>>>,
        cancellation_requested: bool,
        stale_completion: bool,
        fail_running_start: bool,
        fail_cancellation_report: bool,
        start_reports: Arc<AtomicUsize>,
        completion_reports: Arc<AtomicUsize>,
        cancellation_reports: Arc<AtomicUsize>,
        recovery_reports: Arc<AtomicUsize>,
        recovery_failures_remaining: Arc<AtomicUsize>,
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
            Ok(super::super::HeartbeatResponse {
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
            self.start_reports.fetch_add(1, Ordering::SeqCst);
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
            if self.stale_completion {
                Err(ProtocolClientError::StaleLease)
            } else {
                Ok(())
            }
        }

        async fn report_cancellation(
            &self,
            _session: &RunnerSession,
            _report: CancellationReport,
        ) -> Result<(), ProtocolClientError> {
            self.cancellation_reports.fetch_add(1, Ordering::SeqCst);
            if self.fail_cancellation_report {
                Err(ProtocolClientError::StaleLease)
            } else {
                Ok(())
            }
        }

        async fn observe_recovery(
            &self,
            _session: &RunnerSession,
            _report: RecoveryReport,
        ) -> Result<(), ProtocolClientError> {
            self.recovery_reports.fetch_add(1, Ordering::SeqCst);
            let remaining = self.recovery_failures_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.recovery_failures_remaining
                    .fetch_sub(1, Ordering::SeqCst);
                Err(ProtocolClientError::Transport)
            } else {
                Ok(())
            }
        }
    }

    #[derive(Clone)]
    struct FakeAdapter {
        expected_journal: PathBuf,
        start_after_journal: Arc<AtomicBool>,
        cancel_calls: Arc<AtomicUsize>,
        recovery_observation: RecoveryObservation,
        reconcile_fails: bool,
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
        ) -> Result<CancelObservation, HarnessError> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
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
            _journal: &AttemptJournal,
        ) -> Result<RecoveryObservation, HarnessError> {
            if self.reconcile_fails {
                Err(HarnessError::RecoveryUnavailable)
            } else {
                Ok(self.recovery_observation.clone())
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
        RunnerSession::new(RunnerId::new("runner"), RunnerCredential::new("never-log"))
    }

    fn work() -> ClaimedWork {
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
            repository: RepositorySpec {
                remote: "https://example.invalid/repo.git".into(),
                base_revision: "revision".into(),
            },
        }
    }

    fn protocol(
        work: ClaimedWork,
        cancellation_requested: bool,
        stale_completion: bool,
    ) -> FakeProtocol {
        FakeProtocol {
            claim: Arc::new(Mutex::new(Some(ClaimResult::Work(work)))),
            cancellation_requested,
            stale_completion,
            fail_running_start: false,
            fail_cancellation_report: false,
            start_reports: Arc::new(AtomicUsize::new(0)),
            completion_reports: Arc::new(AtomicUsize::new(0)),
            cancellation_reports: Arc::new(AtomicUsize::new(0)),
            recovery_reports: Arc::new(AtomicUsize::new(0)),
            recovery_failures_remaining: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn adapter(expected_journal: PathBuf) -> FakeAdapter {
        FakeAdapter {
            expected_journal,
            start_after_journal: Arc::new(AtomicBool::new(false)),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            recovery_observation: RecoveryObservation::ProcessStopped,
            reconcile_fails: false,
        }
    }

    fn claim_request() -> ClaimRequest {
        ClaimRequest {
            claim_request_id: ClaimRequestId::new("claim"),
            available_capacity: 1,
            wait: std::time::Duration::ZERO,
        }
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
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn stale_fence_quarantines_ambiguous_process_and_stops_reporting() {
        let root = temporary_root("stale");
        let journal = OwnerOnlyJournal::new(&root);
        let protocol = protocol(work(), false, true);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
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
