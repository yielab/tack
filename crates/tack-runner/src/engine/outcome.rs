//! What an attempt ends with: the limit and no-change verdicts, the evidence
//! captured, verified and published, the events and artifacts submitted, and
//! the failure reported when the agent never started.

use std::collections::BTreeMap;

use tack_protocol::execution::{ExecutionState, ProtocolVersion};

use super::decisions::scrub_with_prompt;
use super::{
    CancellationEvidence, EngineError, ExecutionSpec, HarnessAdapter, HarnessError, HarnessOutcome,
    PROGRESS_BATCH_MAX, Progress, RunCycle, RunnerEngine,
};
use crate::client::{
    ArtifactManifestItem, ArtifactManifestReport, AttemptState, Checkpoint, CompletionId,
    CompletionRequest, EventBatchRequest, EventRecord, PendingTerminalReportKind, PullProtocol,
    RunnerSession, WorkspaceMode,
    journal::AttemptJournal,
    workspace::{Workspace, WorkspaceError, WorktreeProvisioner},
};

/// The token limit the runner itself holds a run to: the snapshot's `budgets.tokens`
/// (`run_limits` in the server wrote it), unless docket is the agent, which stops its own
/// task there (`maxTokens` in its task body). A harness that reports no usage while it runs
/// is never stopped by it: its tokens are measured, not enforced.
pub(super) fn runner_token_limit(
    request: &tack_protocol::execution::ExecutionRequestSnapshot,
) -> Option<u64> {
    if request.requested_harness_kind.as_str() == crate::harness::docket::DESCRIPTOR.kind {
        return None;
    }
    request
        .budgets
        .get("tokens")
        .and_then(serde_json::Value::as_u64)
        .filter(|tokens| *tokens > 0)
}

/// The attempt a run's limit stopped ends failed with the limit's sentence, which the
/// attempt's panel prints as its reason; what the harness staged stays beside it.
pub(super) fn token_limit_reached(outcome: &mut HarnessOutcome, limit: u64, counted: u64) {
    outcome.terminal_state = AttemptState::Failed;
    let mut reason = match std::mem::take(&mut outcome.terminal_reason) {
        serde_json::Value::Object(reason) => reason,
        _ => serde_json::Map::new(),
    };
    reason.insert("reason".to_owned(), "token_limit".into());
    reason.insert("code".to_owned(), "token_limit".into());
    reason.insert(
        "message".to_owned(),
        format!(
            "the run reached its token limit ({})",
            tack_core::run_defaults::token_count(i64::try_from(limit).unwrap_or(i64::MAX))
        )
        .into(),
    );
    reason.insert("counted_tokens".to_owned(), counted.into());
    outcome.terminal_reason = serde_json::Value::Object(reason);
}

/// The attempt whose agent reported success and delivered no change ends failed, with the
/// sentence its panel prints as the reason; what the harness reported stays beside it. A
/// harness that left files uncommitted says how many in `untracked_files`.
pub(super) fn no_change_delivered(outcome: &mut HarnessOutcome) {
    outcome.terminal_state = AttemptState::Failed;
    let mut reason = match std::mem::take(&mut outcome.terminal_reason) {
        serde_json::Value::Object(reason) => reason,
        _ => serde_json::Map::new(),
    };
    let mut message =
        "The agent reported success but delivered no change to review (the branch equals its base)"
            .to_owned();
    match reason
        .get("untracked_files")
        .and_then(serde_json::Value::as_u64)
    {
        Some(0) | None => {}
        Some(1) => message.push_str("; 1 file was not committed and is not in the evidence"),
        Some(count) => message.push_str(&format!(
            "; {count} files were not committed and are not in the evidence"
        )),
    }
    reason.insert("reason".to_owned(), "no_change".into());
    reason.insert("code".to_owned(), "no_change".into());
    reason.insert("message".to_owned(), message.into());
    outcome.terminal_reason = serde_json::Value::Object(reason);
}

impl HarnessOutcome {
    /// The usage the evidence carries: the measured counters, plus the
    /// agent's version and the model it ran, which the verifier reads from
    /// the same place.
    pub fn evidence_usage(&self) -> serde_json::Value {
        let mut usage = serde_json::to_value(&self.usage).unwrap_or(serde_json::Value::Null);
        if let Some(object) = usage.as_object_mut() {
            object.insert(
                "harness_version".to_owned(),
                serde_json::json!(self.actual_execution.harness_version),
            );
            object.insert(
                "model".to_owned(),
                serde_json::json!(self.actual_execution.model),
            );
        }
        usage
    }

    /// Workspace ownership belongs to the engine, not a harness adapter. Normalize
    /// the adapter outcome before it is projected into the transport report so the
    /// report has exactly one source for actual execution and usage facts.
    pub(super) fn normalize_workspace_facts(mut self, workspace: &Workspace) -> Self {
        self.actual_execution.workspace_id =
            tack_protocol::execution::WorkspaceId::new(workspace.id.as_str());
        self.actual_execution.base_revision = workspace.base_revision.clone();
        self
    }
}

/// The first config file whose line differs between two
/// [`crate::client::workspace::git::config_fingerprint`]s, or `None` when they agree.
pub(super) fn changed_config_path(before: &str, after: &str) -> Option<String> {
    if before.is_empty() || before == after {
        return None;
    }
    let after_lines: Vec<&str> = after.lines().collect();
    before
        .lines()
        .find(|line| !after_lines.contains(line))
        .or_else(|| {
            after_lines
                .iter()
                .copied()
                .find(|line| !before.lines().any(|b| b == *line))
        })
        .map(|line| line.split('\t').next().unwrap_or(line).to_owned())
}

impl<P, A, W, C> RunnerEngine<P, A, W, C>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
    C: crate::Clock,
{
    // -----------------------------------------------------------------
    // Events, decisions and artifacts.
    //
    // The methods below are the call sites: the steps a harness reports
    // while it runs, one runner-sourced event per terminal outcome
    // (completion or cancellation) and per step after it (verify, push),
    // each numbered on from the attempt's last, plus a best-effort upload of whatever artifact an adapter already staged
    // locally (`terminal_reason.artifact`, each harness adapter's own
    // convention — see e.g. `harness::codex::CodexGrammar::stage_run_log`).
    //
    // Submission is deliberately best-effort: a transport failure here is
    // logged (ids only) and never turns a real harness result into a failed
    // attempt or blocks the terminal report. It does not participate in the
    // crash-safe replay `send_pending_terminal_report` implements for
    // completion/cancellation — a restart does not resend a lost event or
    // artifact. That is a known limitation, not hidden here.
    // -----------------------------------------------------------------

    pub(super) async fn submit_terminal_evidence(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        outcome: &HarnessOutcome,
    ) {
        self.submit_event(
            session,
            record,
            "attempt.terminal",
            outcome.terminal_reason.clone(),
        )
        .await;
        // `artifact` is the harness's own run log; `artifacts` is the list the
        // engine adds (the evidence files).
        let single = outcome.terminal_reason.get("artifact");
        let listed = outcome
            .terminal_reason
            .get("artifacts")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten();
        for artifact in single.into_iter().chain(listed) {
            self.submit_staged_artifact(session, record, artifact).await;
        }
    }

    /// Runs the configured verifier over a succeeded attempt whose evidence
    /// was captured and stages its pack. A failure submits one
    /// `attempt.verify_failed` event; it never changes the terminal state.
    pub(super) async fn verify_attempt(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        spec: &ExecutionSpec,
        scratch: &std::path::Path,
        staged: &mut Vec<serde_json::Value>,
    ) {
        let Some((config, limits)) = self.verify.as_ref().filter(|(config, _)| config.enabled)
        else {
            if spec.work.request.verify == Some(true) {
                let payload =
                    serde_json::json!({"reason": "this runner has no verifier configured"});
                self.submit_event(session, record, "attempt.verify_skipped", payload)
                    .await;
            }
            return;
        };
        if spec.work.request.verify == Some(false) {
            return;
        }
        let captured = std::fs::read(scratch.join("src/evidence.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<crate::evidence::AttemptEvidence>(&bytes).ok()
            })
            .is_some_and(|evidence| evidence.captured);
        if !captured {
            return;
        }
        // A command verifier reads a work tree, and a repository has none.
        if config.kind == crate::config::VerifyKind::Command && Self::in_repository(&spec.work) {
            let payload = serde_json::json!({
                "exit_code": null,
                "reason": "this run made no working copy for a command verifier to read; use kind = \"prill\"",
            });
            self.submit_event(session, record, "attempt.verify_failed", payload)
                .await;
            return;
        }
        // The verifier may run for its whole `timeout_seconds`; the runner's
        // ticker renews the lease meanwhile.
        // Where the board will serve the run's steps once they are uploaded: the artifact's id
        // is a function of the attempt, the fencing token and the digest, so the link is known
        // before the upload.
        let trajectory_uri = staged
            .iter()
            .find(|artifact| artifact["name"] == crate::evidence::TRAJECTORY_FILE)
            .and_then(|artifact| artifact["sha256"].as_str())
            .map(|sha256| {
                format!(
                    "{}/api/executions/{}/attempts/{}/artifacts/{}/content",
                    self.board_url.as_deref().unwrap_or_default(),
                    spec.work.request.request_id.as_str(),
                    spec.work.lease.attempt_number,
                    Self::artifact_id(record, sha256)
                )
            });
        let result = crate::verify::run(
            config,
            limits,
            &spec.workspace,
            scratch,
            &self.process,
            trajectory_uri.as_deref(),
        )
        .await;
        match result {
            Ok(artifacts) => staged.extend(artifacts),
            Err(error) => {
                let payload =
                    serde_json::json!({"exit_code": error.exit_code, "reason": error.reason});
                self.submit_event(session, record, "attempt.verify_failed", payload)
                    .await;
            }
        }
    }

    /// Pushes the branch of a succeeded attempt whose captured patch is
    /// non-empty, and records it in `evidence.json`. A failed push submits one
    /// `attempt.push_failed` event and logs a warning; it never changes the
    /// terminal state.
    pub(super) async fn publish_attempt(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        spec: &ExecutionSpec,
        scratch: &std::path::Path,
        staged: &mut [serde_json::Value],
        outcome: &mut HarnessOutcome,
    ) -> Option<crate::evidence::PublishedBranch> {
        // `in_place` never commits or branches in the user's repository.
        if spec
            .work
            .workspace_repository()
            .is_ok_and(|repository| repository.workspace_mode == WorkspaceMode::InPlace)
        {
            return None;
        }
        // A `local_branch` attempt always keeps its branch in the user's
        // repository; the push is a separate, doubly-opt-in step: the request
        // asks (`push_after_run`) and this runner's `[git] push_branches` is on.
        let in_repository = Self::in_repository(&spec.work);
        let local_branch = in_repository
            || spec
                .work
                .workspace_repository()
                .is_ok_and(|repository| repository.workspace_mode == WorkspaceMode::LocalBranch);
        let push = if local_branch {
            let asked = spec
                .work
                .request
                .repository
                .additional
                .get("push_after_run")
                == Some(&serde_json::Value::Bool(true));
            if asked && !self.git.push_branches {
                let payload = serde_json::json!({"reason": "this runner does not push branches"});
                self.submit_event(session, record, "attempt.push_skipped", payload)
                    .await;
            }
            asked && self.git.push_branches
        } else {
            if !self.git.push_branches {
                if spec.work.request.push_branch == Some(true) {
                    let payload =
                        serde_json::json!({"reason": "this runner does not push branches"});
                    self.submit_event(session, record, "attempt.push_skipped", payload)
                        .await;
                }
                return None;
            }
            if spec.work.request.push_branch == Some(false) {
                return None;
            }
            true
        };
        let evidence_path = scratch.join("src/evidence.json");
        let mut evidence = std::fs::read(&evidence_path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<crate::evidence::AttemptEvidence>(&bytes).ok()
            })
            .filter(|evidence| {
                evidence.captured && evidence.patch.as_ref().is_some_and(|p| p.size_bytes > 0)
            })?;
        // The commit the capture made, and the `HEAD` it was made on. A run
        // that recorded neither has nothing that was verified to push.
        let recorded =
            std::fs::read_to_string(scratch.join(crate::evidence::CAPTURED_COMMIT_FILE)).ok()?;
        let mut recorded = recorded.lines();
        let (captured, head) = (recorded.next()?, recorded.next()?);
        let item = spec.work.request.item_id.as_str();
        let branch = format!(
            "{}{}-a{}",
            self.git.branch_prefix,
            item.split('-').next().unwrap_or(item),
            spec.work.lease.attempt_number
        );
        // A push to a slow remote can outlast the lease; the runner's ticker
        // renews it meanwhile.
        let remote = record.push_remote.clone();
        let result = if in_repository {
            // A commit in the repository cannot change after it was verified.
            self.workspaces
                .publish_range(&spec.workspace, &branch, push, remote.as_deref(), captured)
                .await
        } else {
            self.workspaces
                .publish_branch(
                    &spec.workspace,
                    &branch,
                    push,
                    remote.as_deref(),
                    captured,
                    head,
                )
                .await
        };
        let published = match result {
            Ok(Some(published)) if published.head_commit == captured => published,
            // What would be pushed is not what was verified: the attempt
            // fails, and the evidence says why.
            Ok(Some(_)) | Err(WorkspaceError::TreeChanged) => {
                outcome.terminal_state = AttemptState::Failed;
                outcome.terminal_reason =
                    serde_json::json!({ "code": "tree_changed_after_verification" });
                evidence.terminal_reason = outcome.terminal_reason.clone();
                Self::restage_evidence(&evidence_path, staged, &evidence);
                return None;
            }
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    failure = %error,
                    "the attempt's branch could not be pushed"
                );
                let payload = serde_json::json!({"branch": branch, "reason": error.to_string()});
                self.submit_event(session, record, "attempt.push_failed", payload)
                    .await;
                return None;
            }
        };
        // `evidence.json` was staged before the push; stage it again with the
        // branch, keeping the manifest entry's size and digest true to the file.
        evidence.branch = serde_json::to_value(&published).ok();
        // The work now ends at the branch's commit, with nothing left
        // uncommitted on top of it: the patch is exactly base..head.
        evidence.head_commit = Some(published.head_commit.clone());
        evidence.worktree_dirty = Some(false);
        Self::restage_evidence(&evidence_path, staged, &evidence);
        Some(published)
    }

    /// A run that may change the repository, whose agent reported success and whose captured
    /// evidence holds no change, ends failed: nothing reached review. A run that is read-only
    /// (`ExecutionRequestSnapshot::expects_change`), or that staged a plan, delivered what it
    /// was asked for and stays succeeded.
    pub(super) fn fail_without_change(
        spec: &ExecutionSpec,
        scratch: &std::path::Path,
        staged: &mut [serde_json::Value],
        outcome: &mut HarnessOutcome,
    ) {
        if outcome.terminal_state != AttemptState::Succeeded
            || !spec.work.request.expects_change()
            || staged
                .iter()
                .any(|entry| entry.get("kind").and_then(|kind| kind.as_str()) == Some("plan"))
        {
            return;
        }
        let evidence_path = scratch.join("src/evidence.json");
        let Some(mut evidence) = std::fs::read(&evidence_path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<crate::evidence::AttemptEvidence>(&bytes).ok()
            })
            .filter(|evidence| {
                evidence.captured
                    && evidence.files.is_empty()
                    && evidence
                        .patch
                        .as_ref()
                        .is_none_or(|patch| patch.size_bytes == 0)
            })
        else {
            return;
        };
        no_change_delivered(outcome);
        evidence.terminal_reason = outcome.terminal_reason.clone();
        Self::restage_evidence(&evidence_path, staged, &evidence);
    }

    /// Writes `evidence` over the staged `evidence.json`, keeping the
    /// manifest entry's size and digest true to the file.
    fn restage_evidence(
        evidence_path: &std::path::Path,
        staged: &mut [serde_json::Value],
        evidence: &crate::evidence::AttemptEvidence,
    ) {
        let bytes = serde_json::to_vec_pretty(evidence).unwrap_or_default();
        let _ = std::fs::write(evidence_path, &bytes);
        if let Some(entry) = staged
            .iter_mut()
            .find(|entry| entry.get("name").and_then(|name| name.as_str()) == Some("evidence.json"))
        {
            let written = entry
                .get("staged_path")
                .and_then(|path| path.as_str())
                .is_some_and(|path| std::fs::write(path, &bytes).is_ok());
            if written {
                entry["size_bytes"] = bytes.len().into();
                entry["sha256"] = crate::harness::sha256::sha256_hex(&bytes).into();
            }
        }
    }

    /// Stages the evidence of one attempt, or nothing when no transport is
    /// attached to upload it.
    pub(super) async fn capture_attempt_evidence(
        &self,
        spec: &ExecutionSpec,
        base_commit: Option<&str>,
        terminal_reason: serde_json::Value,
        usage: serde_json::Value,
    ) -> Option<(std::path::PathBuf, Vec<serde_json::Value>)> {
        self.data_protocol.as_ref()?;
        // A run in the repository itself ended where its harness says, or
        // where it began when it made no commit.
        let head = Self::in_repository(&spec.work).then(|| {
            terminal_reason["head_commit"]
                .as_str()
                .or(base_commit)
                .unwrap_or_default()
                .to_owned()
        });
        let harness_kind = serde_json::to_value(&spec.work.request.requested_harness_kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let message = format!(
            "Tack attempt {} on item {}",
            spec.work.lease.attempt_id.as_str(),
            spec.work.request.item_id.as_str()
        );
        Some(
            crate::evidence::capture(
                &self.workspaces,
                &spec.workspace,
                harness_kind,
                terminal_reason,
                usage,
                spec.work.request.brief.as_ref(),
                base_commit,
                (&self.git.author, &message),
                head.as_deref(),
            )
            .await,
        )
    }

    pub(super) async fn submit_cancellation_event(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        evidence: &CancellationEvidence,
    ) {
        let payload = serde_json::json!({
            "observation": evidence.observation,
            "details": serde_json::Value::Object(evidence.details.clone()),
        });
        self.submit_event(session, record, "attempt.cancelled", payload)
            .await;
    }

    /// Submits one runner-sourced event carrying `payload`. `event_id` is
    /// derived deterministically from `(attempt_id, fencing_token, kind)` —
    /// never random or clock-based — so a caller that retries this exact
    /// submission (e.g. after a transient transport error) reuses the
    /// identical id instead of manufacturing a duplicate under a new
    /// identity. `docs/contracts/runner-v1/event-batch.request.json`'s own
    /// idempotency rule is a unique `(attempt_id, event_id)`; the server is
    /// the authority on treating a resend of the same id as a no-op
    /// (`duplicate_event_ids`), proved from this side in
    /// `submitting_the_same_terminal_event_twice_is_idempotent` below.
    pub(super) async fn submit_event(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        kind: &str,
        payload: serde_json::Value,
    ) {
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        let sent_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        let event_id = Self::event_id(record, kind);
        let checkpoint = Checkpoint::new(format!(
            "chk:{}:{}:{kind}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        ));
        let report = EventBatchRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            previous_checkpoint: record.last_event_checkpoint.clone(),
            checkpoint: checkpoint.clone(),
            events: vec![EventRecord {
                event_id,
                sequence: i64::try_from(record.events_submitted).unwrap_or(i64::MAX),
                occurred_at: sent_at,
                source: "runner".to_owned(),
                kind: kind.to_owned(),
                payload,
            }],
        };
        match data_protocol.submit_events(session, report).await {
            Ok(response) => {
                record.last_event_checkpoint = Some(response.committed_checkpoint);
                record.events_submitted += 1;
                // Best-effort: the event already reached the server; a local
                // journal-write failure here only affects this runner's own
                // `last_event_checkpoint` bookkeeping (used for a future
                // resumed batch), not the event's durability on the server.
                let _ = self.journal.update(record);
            }
            Err(_error) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "event submission failed"
                );
            }
        }
    }

    /// Submits the steps a running harness reported since the last flush,
    /// each a runner event of its own `kind` whose payload is `{"text": …}`
    /// (and `"usage"` when the step has one), numbered on from the attempt's last event. Best effort, like every
    /// event: a batch the server does not take is logged and dropped, never
    /// retried, so a step can be missing from the timeline but never shown
    /// twice. `prompt` is scrubbed from each text as it is from a decision.
    pub(super) async fn submit_progress(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        steps: &mut Vec<Progress>,
        prompt: &str,
    ) {
        let steps = std::mem::take(steps);
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        for chunk in steps.chunks(PROGRESS_BATCH_MAX) {
            let occurred_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
            let first = record.events_submitted;
            let checkpoint = Checkpoint::new(format!(
                "chk:{}:{}:progress:{first}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            ));
            let events = chunk
                .iter()
                .zip(first..)
                .map(|(step, sequence)| EventRecord {
                    event_id: Self::event_id(record, &format!("progress:{sequence}")),
                    sequence: i64::try_from(sequence).unwrap_or(i64::MAX),
                    occurred_at,
                    source: "runner".to_owned(),
                    kind: step.kind.clone(),
                    payload: {
                        let mut payload =
                            serde_json::json!({ "text": scrub_with_prompt(&step.text, prompt) });
                        if let Some(usage) = &step.usage {
                            payload["usage"] = serde_json::to_value(usage)
                                .expect("usage is plain data and always serializes");
                        }
                        payload
                    },
                })
                .collect();
            let report = EventBatchRequest {
                protocol_version: ProtocolVersion::v1(),
                runner_id: session.runner_id.clone(),
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                previous_checkpoint: record.last_event_checkpoint.clone(),
                checkpoint: checkpoint.clone(),
                events,
            };
            match data_protocol.submit_events(session, report).await {
                Ok(response) => {
                    record.last_event_checkpoint = Some(response.committed_checkpoint);
                    record.events_submitted += chunk.len() as u64;
                    let _ = self.journal.update(record);
                }
                Err(_error) => {
                    tracing::warn!(
                        attempt_id = record.attempt_id.as_str(),
                        steps = chunk.len(),
                        "progress submission failed; these steps are left off the timeline"
                    );
                }
            }
        }
    }

    fn event_id(record: &AttemptJournal, kind: &str) -> String {
        let material = format!(
            "{}:{}:{kind}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        format!("evt_{}", Self::hex(material.as_bytes()))
    }

    fn artifact_id(record: &AttemptJournal, sha256: &str) -> String {
        let material = format!(
            "{}:{}:{sha256}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        format!("art_{}", Self::hex(material.as_bytes()))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Best-effort upload of a locally staged artifact. `artifact` is the
    /// exact JSON object the harness adapters already produce at
    /// `terminal_reason.artifact` (`kind`, `name`, `media_type`, `sha256`,
    /// `staged_path`, ...) via `harness::artifact::ArtifactStager` — this
    /// method is the first thing in the tree that ever reads it back out.
    /// The file's bytes and size are read fresh from `staged_path` rather
    /// than trusted from the JSON, mirroring `ArtifactStager`'s own rule
    /// that a checksum/size must come from bytes actually handled, never a
    /// value merely reported.
    pub(super) async fn submit_staged_artifact(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        artifact: &serde_json::Value,
    ) {
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        let (Some(kind), Some(name), Some(media_type), Some(sha256), Some(staged_path)) = (
            artifact.get("kind").and_then(|value| value.as_str()),
            artifact.get("name").and_then(|value| value.as_str()),
            artifact.get("media_type").and_then(|value| value.as_str()),
            artifact.get("sha256").and_then(|value| value.as_str()),
            artifact.get("staged_path").and_then(|value| value.as_str()),
        ) else {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "staged artifact JSON is missing an expected field; not uploaded"
            );
            return;
        };
        let content = match std::fs::read(staged_path) {
            Ok(bytes) => bytes,
            Err(_) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "staged artifact could not be read for upload"
                );
                return;
            }
        };
        let artifact_id = Self::artifact_id(record, sha256);
        let manifest = ArtifactManifestReport {
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            artifacts: vec![ArtifactManifestItem {
                artifact_id: artifact_id.clone(),
                kind: kind.to_owned(),
                name: name.to_owned(),
                media_type: Some(media_type.to_owned()),
                size_bytes: content.len() as u64,
                sha256: sha256.to_owned(),
                content_disposition: Some("inline_upload".to_owned()),
                metadata: Default::default(),
            }],
        };
        let grants = match data_protocol
            .submit_artifact_manifest(session, manifest)
            .await
        {
            Ok(grants) => grants,
            Err(_error) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "artifact manifest submission failed"
                );
                return;
            }
        };
        let Some(grant) = grants
            .into_iter()
            .find(|grant| grant.artifact_id == artifact_id)
        else {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "server granted no upload for the submitted artifact"
            );
            return;
        };
        if data_protocol
            .put_artifact_content(
                session,
                record.fencing_token,
                &grant,
                Some(media_type),
                content,
            )
            .await
            .is_err()
        {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "artifact content upload failed"
            );
        }
    }

    /// Turns a harness rejection that arrived after the attempt was already
    /// announced as `preparing` into a reported failure, through the same
    /// durable outbox every other terminal report goes through.
    ///
    /// Propagating it as an error instead leaves the server holding an
    /// attempt in `preparing` under a lease nothing will heartbeat or
    /// complete, and the journal holding a `prepared` record with no process
    /// behind it — a hang only a restart's recovery scan resolves. The
    /// rejection is settled: the adapter refused before any process existed,
    /// so the report is `failed` with that reason.
    ///
    /// Four things end here, each with its own text: a refusal at `validate`,
    /// a refusal at `start`, any other harness error before the agent was
    /// seen running (its text), and a workspace that could not be prepared
    /// (its text). Only a failure to reach the board stays an [`EngineError`].
    ///
    /// The report says nothing ran, in the contract's own vocabulary: no
    /// harness version observed, the model marked as requested, or the agent's own default, every
    /// feature `unsupported`, every usage figure `not_measured`.
    pub(super) async fn fail_before_spawn(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        spec: &ExecutionSpec,
        error: HarnessError,
    ) -> Result<RunCycle, EngineError> {
        use tack_protocol::execution as domain;

        // A refusal's own words; any other error's text, which never carries
        // a secret.
        let reason = match error {
            HarnessError::Rejected { reason } => reason,
            other => other.to_string(),
        };

        let now = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        let request = &spec.work.request;
        let unexercised = || domain::CapabilityValue {
            support: domain::CapabilitySupport::Unsupported,
            reason: Some("the agent refused this run before it started".to_owned()),
            additional: BTreeMap::new(),
        };
        fn not_measured<T>() -> domain::Measurement<T> {
            domain::Measurement {
                value: None,
                source: domain::MeasurementSource::NotMeasured,
                additional: BTreeMap::new(),
            }
        }
        let (model, model_source) = match request.requested_model.as_ref() {
            Some(model) => (Some(model.clone()), crate::harness::ModelSource::Requested),
            None => (None, crate::harness::ModelSource::AgentDefault),
        };

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
            terminal_state: ExecutionState::Failed,
            terminal_reason: serde_json::json!({
                "code": "harness_rejected",
                "message": reason,
            }),
            final_event_checkpoint: record.last_event_checkpoint.clone(),
            actual_execution: domain::ActualExecution {
                harness_kind: request.requested_harness_kind.clone(),
                harness_version: String::new(),
                model,
                model_source: model_source.as_str().to_owned(),
                capability_snapshot: domain::FeatureCapabilities {
                    cancel: unexercised(),
                    resume: unexercised(),
                    decisions: unexercised(),
                    artifacts: unexercised(),
                    usage: unexercised(),
                    additional: BTreeMap::new(),
                },
                workspace_id: domain::WorkspaceId::new(record.workspace.workspace_id.as_str()),
                base_revision: record.workspace.base_revision.clone(),
                started_at: now,
                ended_at: now,
                additional: BTreeMap::new(),
            },
            usage: domain::Usage {
                tokens_in: not_measured(),
                tokens_out: not_measured(),
                duration_ms: not_measured(),
                cost_usd: not_measured(),
                additional: BTreeMap::new(),
            },
        };
        tracing::warn!(
            attempt_id = %record.attempt_id.as_str(),
            "harness rejected the execution before it started; reporting the attempt as failed"
        );
        self.persist_pending_terminal_report(
            record,
            PendingTerminalReportKind::Completion,
            &report,
        )?;
        let cycle = self.send_pending_terminal_report(session, record).await?;
        if matches!(cycle, RunCycle::Completed { .. }) {
            let _ = self.workspaces.cleanup(&spec.workspace);
        }
        Ok(cycle)
    }
}
