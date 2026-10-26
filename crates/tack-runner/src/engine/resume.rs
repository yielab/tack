//! Taking an attempt back after a restart: the journal scan, the agent that
//! outlived the runner, the terminal-report outbox and the recovery report.

use std::collections::{BTreeMap, HashSet};

use tack_protocol::execution::{
    AttemptId as DomainAttemptId, FencingToken as DomainFencingToken, ProtocolVersion,
    RecoveryDetails as DomainRecoveryDetails, RecoveryDisposition, RecoveryJournalState,
    RecoveryKey, RecoveryObservation, RecoveryObservationRequest, RecoveryObservationResponse,
    RunnerId as DomainRunnerId,
};

use super::{
    CancelObservation, EngineError, ExecutionSpec, HarnessAdapter, LocalRunHandle,
    POLL_FROM_THE_START, ResumedRun, RunCycle, RunnerEngine,
};
use crate::client::{
    AttemptState, CancellationReport, CancellationRequestId, CompletionId, CompletionReport,
    CompletionResponse, DecisionPollReport, PendingTerminalReport, PendingTerminalReportKind,
    ProtocolClientError, PullProtocol, RunnerSession, Timestamp,
    journal::{AttemptJournal, JournalError, JournalState},
    workspace::WorktreeProvisioner,
};

impl<P, A, W, C> RunnerEngine<P, A, W, C>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
    C: crate::Clock,
{
    pub async fn recover(&self, session: &RunnerSession) -> Result<Vec<RunCycle>, EngineError> {
        let mut outcomes = Vec::new();
        for mut record in self.journal.unresolved()? {
            match (
                record.state == JournalState::TerminalReportPending,
                record.pending_terminal_report.is_some(),
            ) {
                (true, true) => {
                    let outcome = self
                        .send_pending_terminal_report(session, &mut record)
                        .await?;
                    if matches!(
                        outcome,
                        RunCycle::Completed { .. } | RunCycle::Cancelled { .. }
                    ) {
                        // A terminal replay may only clean up after its
                        // acknowledgement has been fsynced as `Reported`.
                        // `cleanup` revalidates this journal-derived path,
                        // marker, and deterministic attempt location.
                        let _ = self
                            .workspaces
                            .cleanup(&Self::workspace_from_journal(&record));
                    }
                    outcomes.push(outcome);
                    continue;
                }
                (true, false) | (false, true) => {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                (false, false) => {}
            }
            let observation = match self.adapter.reconcile(&record).await {
                Ok(observation) => observation,
                Err(_) => RecoveryObservation::Ambiguous,
            };
            // docket's task lives in its daemon, not in a child of this
            // runner: when it is still running and the journal kept the claim,
            // it is followed to its end as a fresh run is. Anything unproven
            // falls through to the recovery report below.
            if observation == RecoveryObservation::ProcessRunning
                && record.state == JournalState::ProcessObservedRunning
                && let Some(resumed) = self.resume_running_agent(session, &record).await
            {
                outcomes.push(RunCycle::Resumed(Box::new(resumed)));
                continue;
            }
            let outcome = self
                .report_recovery_and_apply_disposition(session, &mut record, observation)
                .await?;
            // A settled recovery owns the same cleanup duty as a
            // terminal replay. Without this the checkout a killed runner left
            // behind survives every restart, because nothing else ever revisits
            // that attempt's directory. A `Quarantined` or `RecoveryPending`
            // outcome deliberately keeps the checkout: it is the evidence an
            // operator needs, and the attempt is not settled.
            if matches!(outcome, RunCycle::Completed { .. }) {
                let _ = self
                    .workspaces
                    .cleanup(&Self::workspace_from_journal(&record));
            }
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Takes back an agent `reconcile` saw still running: the adapter hands
    /// back its handle, one heartbeat with the journal's fencing token proves
    /// the lease is still this runner's, and the lease is registered so the
    /// runner's one ticker carries it from here on. `None` when the journal kept
    /// no claim, the adapter cannot resume or the lease is no longer this
    /// runner's. The run is followed by [`Self::follow_resumed`].
    async fn resume_running_agent(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
    ) -> Option<ResumedRun> {
        let work = record.claimed_work.clone().filter(|work| {
            work.lease.attempt_id == record.attempt_id
                && work.lease.fencing_token == record.fencing_token
        })?;
        let spec = ExecutionSpec {
            work,
            workspace: Self::workspace_from_journal(record),
        };
        let handle = self.adapter.resume(&spec, record).await.ok()?;
        let cancel = self.active.register(record);
        if !self.renew_lease(session, record).await {
            self.active.remove(&record.attempt_id);
            return None;
        }
        let known_decisions = match self.data_protocol.as_ref() {
            Some(data_protocol) => data_protocol
                .poll_decisions(
                    session,
                    DecisionPollReport {
                        attempt_id: record.attempt_id.clone(),
                        fencing_token: record.fencing_token,
                        after: Timestamp::new(POLL_FROM_THE_START.to_owned()),
                    },
                )
                .await
                .map(|polled| {
                    polled
                        .decisions
                        .into_iter()
                        .map(|decision| decision.decision_id)
                        .collect()
                })
                .unwrap_or_default(),
            None => HashSet::new(),
        };
        Some(ResumedRun {
            record: record.clone(),
            spec,
            handle,
            cancel,
            known_decisions,
        })
    }

    /// Follows a [`ResumedRun`] to its terminal report, as a fresh run is
    /// followed after it starts. The attempt's deadline starts over here: the
    /// time it ran before the restart is not counted against it.
    pub async fn follow_resumed(
        &self,
        session: &RunnerSession,
        resumed: ResumedRun,
    ) -> Result<RunCycle, EngineError> {
        let attempt_id = resumed.record.attempt_id.clone();
        let cycle = self
            .follow_to_end(
                session,
                resumed.record,
                &resumed.spec,
                resumed.handle,
                resumed.cancel,
                resumed.known_decisions,
            )
            .await;
        self.active.remove(&attempt_id);
        cycle
    }

    pub(super) fn persist_pending_terminal_report<T: serde::Serialize>(
        &self,
        record: &mut AttemptJournal,
        kind: PendingTerminalReportKind,
        report: &T,
    ) -> Result<(), EngineError> {
        let canonical_json =
            serde_json::to_string(report).map_err(|_| EngineError::TerminalReportSerialization)?;
        let mut pending = record.clone();
        pending.state = JournalState::TerminalReportPending;
        pending.pending_terminal_report = Some(PendingTerminalReport {
            kind,
            canonical_json,
        });
        // `update` uses atomic replacement and fsync. Do not mutate the live
        // record until that durable intent succeeds.
        self.journal.update(&pending)?;
        *record = pending;
        Ok(())
    }

    pub(super) async fn send_pending_terminal_report(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
    ) -> Result<RunCycle, EngineError> {
        if record.runner_id != session.runner_id {
            return Err(EngineError::Journal(JournalError::Malformed));
        }
        let Some(pending) = record.pending_terminal_report.as_ref() else {
            return Err(EngineError::Journal(JournalError::Malformed));
        };
        match pending.kind {
            PendingTerminalReportKind::Completion => {
                let report: CompletionReport = serde_json::from_str(&pending.canonical_json)
                    .map_err(|_| EngineError::Journal(JournalError::Malformed))?;
                if report.protocol_version != ProtocolVersion::v1()
                    || report.runner_id != session.runner_id
                    || report.attempt_id != record.attempt_id
                    || report.fencing_token != record.fencing_token
                    || report.completion_id
                        != CompletionId::new(format!(
                            "completion:{}:{}",
                            record.attempt_id.as_str(),
                            record.fencing_token.0
                        ))
                    || report.actual_execution.workspace_id.as_str()
                        != record.workspace.workspace_id.as_str()
                    || report.actual_execution.base_revision != record.workspace.base_revision
                    || !matches!(
                        report.terminal_state,
                        AttemptState::Succeeded | AttemptState::Failed | AttemptState::Cancelled
                    )
                {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                let acknowledged = matches!(
                    self.protocol.report_completion(session, report.clone()).await,
                    Ok(CompletionResponse {
                        protocol_version,
                        attempt_id,
                        completion_id,
                        state,
                        ..
                    }) if protocol_version == ProtocolVersion::v1()
                        && attempt_id == report.attempt_id
                        && completion_id == report.completion_id
                        && state == report.terminal_state
                );
                if !acknowledged {
                    return Ok(RunCycle::TerminalReportPending {
                        attempt_id: record.attempt_id.clone(),
                    });
                }
                self.acknowledge_pending_terminal_report(
                    record,
                    RunCycle::Completed {
                        attempt_id: record.attempt_id.clone(),
                    },
                )
            }
            PendingTerminalReportKind::Cancellation => {
                let report: CancellationReport = serde_json::from_str(&pending.canonical_json)
                    .map_err(|_| EngineError::Journal(JournalError::Malformed))?;
                if report.protocol_version != ProtocolVersion::v1()
                    || report.runner_id != session.runner_id
                    || report.attempt_id != record.attempt_id
                    || report.fencing_token != record.fencing_token
                    || report.cancellation_request_id
                        != CancellationRequestId::new(format!(
                            "cancel:{}:{}",
                            record.attempt_id.as_str(),
                            record.fencing_token.0
                        ))
                    || report.observation != CancelObservation::ProcessStopped
                {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                let acknowledged = matches!(
                    self.protocol.report_cancellation(session, report.clone()).await,
                    Ok(response)
                        if response.protocol_version == ProtocolVersion::v1()
                            && response.attempt_id == report.attempt_id
                            && response.cancellation_request_id == report.cancellation_request_id
                            && response.state == AttemptState::Cancelled
                );
                if !acknowledged {
                    return Ok(RunCycle::TerminalReportPending {
                        attempt_id: record.attempt_id.clone(),
                    });
                }
                self.acknowledge_pending_terminal_report(
                    record,
                    RunCycle::Cancelled {
                        attempt_id: record.attempt_id.clone(),
                    },
                )
            }
        }
    }

    fn acknowledge_pending_terminal_report(
        &self,
        record: &mut AttemptJournal,
        cycle: RunCycle,
    ) -> Result<RunCycle, EngineError> {
        let mut acknowledged = record.clone();
        acknowledged.pending_terminal_report = None;
        acknowledged.state = JournalState::Reported;
        // If the post-ack write fails, retain the in-memory and on-disk
        // pending payload for an exact replay on restart.
        if self.journal.update(&acknowledged).is_err() {
            return Ok(RunCycle::TerminalReportPending {
                attempt_id: record.attempt_id.clone(),
            });
        }
        *record = acknowledged;
        Ok(cycle)
    }

    pub(super) async fn quarantine_after_spawn(
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

    pub(super) async fn report_or_retain_ambiguity(
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
            Err(ProtocolClientError::StaleLease) => {
                // `stale_lease` is the one answer that settles this rather
                // than leaving it open: the server has no active lease
                // under this attempt id, runner id and fencing token at
                // all, and (this fencing token being fixed for the life of
                // an attempt) never will again. That is different in kind
                // from an absent reply or a transport failure, which say
                // nothing about whether the attempt exists. Quarantine it
                // exactly like an operator-facing disposition: the record
                // leaves the restart scan, and its checkout is left on disk
                // as the evidence an operator would need.
                self.journal.quarantine(record)?;
                return Ok(RunCycle::Quarantined {
                    attempt_id: record.attempt_id.clone(),
                });
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
            // Pending terminal payloads are handled before recovery mapping.
            JournalState::TerminalReportPending => RecoveryJournalState::Reported,
            JournalState::RecoveryObserved => RecoveryJournalState::RecoveryObserved,
            JournalState::Reported => RecoveryJournalState::Reported,
            JournalState::Quarantined => RecoveryJournalState::Quarantined,
        }
    }
}
