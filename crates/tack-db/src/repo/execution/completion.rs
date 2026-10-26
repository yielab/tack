//! Completion: an attempt moves through its states, ends with a result or a cancellation, and replays of either answer the same.

use chrono::{DateTime, Utc};
use sqlx::Row;
use tack_protocol::Clock;
use tracing::instrument;

use super::decisions::end_attempt_decisions;
use super::shared::{canonical_json, mirror_request_state, stamp, terminal};
use super::state;
use crate::{Repository, StoreError};
use tack_protocol::execution::ExecutionState;

#[derive(Debug, Clone)]
pub struct Completion<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub completion_id: &'a str,
    pub final_event_checkpoint: Option<&'a str>,
    pub terminal_state: &'a str,
    pub terminal_reason: &'a str,
    pub actual_execution: &'a str,
    pub usage: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptTransitionPhase {
    Preparing,
    Running,
}

#[derive(Debug, Clone)]
pub struct AttemptTransitionInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub phase: AttemptTransitionPhase,
    pub workspace_id: &'a str,
    pub base_revision: &'a str,
    pub process_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptTransitionResponse {
    pub attempt_id: String,
    pub state: String,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptTransitionResult {
    Applied(AttemptTransitionResponse),
    Replayed(AttemptTransitionResponse),
    Conflict,
    Stale,
}

#[derive(Debug, Clone)]
pub struct CancellationObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub cancellation_request_id: &'a str,
    pub observed_at: DateTime<Utc>,
    pub details: &'a str,
    pub observation: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationResponse {
    pub attempt_id: String,
    pub cancellation_request_id: String,
    pub state: String,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationObservation {
    Cancelled(CancellationResponse),
    Replayed(CancellationResponse),
    Conflict,
    Stale,
    AlreadyTerminal { state: String },
    Ambiguous { state: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub completion_id: String,
    pub terminal_state: String,
    pub final_event_checkpoint: Option<String>,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionResult {
    Committed(CompletionResponse),
    Replayed(CompletionResponse),
    /// The stored replay fingerprint for this `(attempt_id, completion_id)`
    /// key differs from the new request's fingerprint: the same
    /// idempotency-scoped key was reused with different content. This can
    /// never succeed by retrying with the same key — the wire mapping is the
    /// non-retryable `idempotency_conflict` stable error, never `conflict`.
    IdempotencyConflict,
    /// No matching replay row: either the attempt is already terminal under
    /// a different completion (no authoritative historical response to
    /// replay, e.g. a pre-M055 terminal attempt) or a concurrent writer won
    /// the compare-and-set. The wire mapping is the retryable `conflict`
    /// stable error.
    Conflict,
    Stale,
}

fn canonical_json_or_string(value: &str) -> serde_json::Value {
    match serde_json::from_str(value) {
        Ok(value) => canonical_json(value),
        Err(_) => serde_json::Value::String(value.into()),
    }
}

fn completion_fingerprint(completion: &Completion<'_>) -> Result<String, StoreError> {
    let actual_execution: serde_json::Value = serde_json::from_str(completion.actual_execution)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let usage: serde_json::Value = serde_json::from_str(completion.usage)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": completion.runner_id,
        "attempt_id": completion.attempt_id,
        "fencing_token": completion.fencing_token,
        "completion_id": completion.completion_id,
        "final_event_checkpoint": completion.final_event_checkpoint,
        "terminal_state": completion.terminal_state,
        "terminal_reason": canonical_json_or_string(completion.terminal_reason),
        "actual_execution": canonical_json(actual_execution),
        "usage": canonical_json(usage),
    }))
    .map_err(|error| StoreError::malformed(error.to_string()))
}

fn replay_response(serialized: &str) -> Result<CompletionResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::malformed("invalid completion replay response"))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::malformed(format!("invalid completion replay response field: {field}"))
            })
    };
    let final_event_checkpoint = match object.get("final_event_checkpoint") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        _ => {
            return Err(StoreError::malformed(
                "invalid completion replay response field: final_event_checkpoint",
            ));
        }
    };
    Ok(CompletionResponse {
        completion_id: required("completion_id")?,
        terminal_state: required("terminal_state")?,
        final_event_checkpoint,
        committed_at: required("committed_at")?,
    })
}

fn cancellation_fingerprint(
    input: &CancellationObservationInput<'_>,
) -> Result<String, StoreError> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let observation: serde_json::Value = serde_json::from_str(input.observation)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    if observation != serde_json::Value::String("process_stopped".into()) {
        return Err(StoreError::malformed(
            "cancellation observation must be JSON string process_stopped",
        ));
    }
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "cancellation_request_id": input.cancellation_request_id,
        "observed_at": input.observed_at.to_rfc3339(),
        "details": canonical_json(details),
        "observation": canonical_json(observation),
    }))
    .map_err(|error| StoreError::malformed(error.to_string()))
}

fn cancellation_response(serialized: &str) -> Result<CancellationResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| StoreError::malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::malformed("invalid cancellation replay response"))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::malformed(format!(
                    "invalid cancellation replay response field: {field}"
                ))
            })
    };
    Ok(CancellationResponse {
        attempt_id: required("attempt_id")?,
        cancellation_request_id: required("cancellation_request_id")?,
        state: required("state")?,
        committed_at: required("committed_at")?,
    })
}

impl Repository {
    pub async fn complete_execution_result(
        &self,
        completion: Completion<'_>,
        clock: &dyn Clock,
    ) -> Result<CompletionResult, StoreError> {
        if !terminal(completion.terminal_state) {
            return Ok(CompletionResult::Conflict);
        }
        let now = stamp(clock);
        let fingerprint = completion_fingerprint(&completion)?;
        // Serialize duplicate completion reports (e.g. a runner retrying an
        // unacknowledged terminal POST) before either transaction reads this
        // attempt row. Two deferred readers can otherwise deadlock while
        // both try to upgrade to writers. Mirrors the redeem_enrollment_token
        // and claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT request_id,runner_id,state,lease_expires_at,event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.runner_id)
        .bind(completion.fencing_token)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_completion_replays WHERE attempt_id=? AND completion_id=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.completion_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                // Same (attempt_id, completion_id) idempotency-scoped key,
                // different content: this can never succeed by retrying.
                tx.commit().await?;
                return Ok(CompletionResult::IdempotencyConflict);
            }
            let response: String = row.get("response");
            let response = replay_response(&response)?;
            tx.commit().await?;
            return Ok(CompletionResult::Replayed(response));
        }
        let state: String = row.get("state");
        let expires: String = row.get("lease_expires_at");
        if terminal(&state) {
            // Pre-M055 terminal attempts have no authoritative response to
            // replay. This is a distinct completion_id from whichever one
            // terminated the attempt, not a reused idempotency key with
            // different content, so it is the benign/retryable `Conflict`.
            tx.commit().await?;
            return Ok(CompletionResult::Conflict);
        }
        if expires <= now {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let updated = sqlx::query("UPDATE execution_attempts SET state=?,completion_id=?,terminal_reason=?,actual_execution=?,usage=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at > ?")
            .bind(completion.terminal_state).bind(completion.completion_id).bind(completion.terminal_reason).bind(completion.actual_execution).bind(completion.usage).bind(&now).bind(&now).bind(completion.attempt_id).bind(completion.runner_id).bind(completion.fencing_token).bind(completion.final_event_checkpoint).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            // Defensive: near-unreachable under the BEGIN IMMEDIATE this
            // function already opens, but a lost compare-and-set is a
            // benign/retryable `Conflict`, not an idempotency-key reuse.
            tx.rollback().await?;
            return Ok(CompletionResult::Conflict);
        }
        end_attempt_decisions(&mut tx, completion.attempt_id, &now).await?;
        sqlx::query("UPDATE execution_requests SET state=?,updated_at=? WHERE id=?")
            .bind(completion.terminal_state)
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(runner).execute(&mut *tx).await?;
        let response = CompletionResponse {
            completion_id: completion.completion_id.into(),
            terminal_state: completion.terminal_state.into(),
            final_event_checkpoint: completion.final_event_checkpoint.map(str::to_owned),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "completion_id": response.completion_id,
            "terminal_state": response.terminal_state,
            "final_event_checkpoint": response.final_event_checkpoint,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_completion_replays(attempt_id,completion_id,fingerprint,response,committed_at) VALUES(?,?,?,?,?)").bind(completion.attempt_id).bind(completion.completion_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CompletionResult::Committed(response))
    }

    /// Stores what a runner reported when it started the agent (`report` is JSON text) on the
    /// attempt it holds under `fencing_token`. Nothing is written for an attempt another runner
    /// or fence holds.
    pub async fn record_start_report(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fencing_token: i64,
        report: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE execution_attempts SET start_report=? \
             WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(report)
        .bind(attempt_id)
        .bind(runner_id)
        .bind(fencing_token)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Persist the two runner start acknowledgements with natural idempotency.
    /// Workspace facts are immutable once preparation is accepted; observing
    /// a process is the only transition that may add `process_id`. The
    /// request follows its attempt into `preparing` and `running` in the
    /// same transaction.
    pub async fn transition_attempt_with_facts(
        &self,
        input: AttemptTransitionInput<'_>,
        clock: &dyn Clock,
    ) -> Result<AttemptTransitionResult, StoreError> {
        if input.workspace_id.is_empty()
            || input.base_revision.is_empty()
            || matches!(input.phase, AttemptTransitionPhase::Preparing)
                && input.process_id.is_some()
            || matches!(input.phase, AttemptTransitionPhase::Running)
                && (input.process_id.is_none() || input.process_id.is_some_and(str::is_empty))
        {
            return Ok(AttemptTransitionResult::Conflict);
        }

        let now = stamp(clock);
        // Serialize duplicate transition acknowledgements for the same
        // attempt (e.g. a runner retrying an unacknowledged preparing or
        // running report) before either transaction reads this attempt
        // row. Two deferred readers can otherwise deadlock while both try to
        // upgrade to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT a.state,a.workspace_id,a.base_revision,a.process_id,a.prepared_at,a.started_at \
             FROM execution_attempts a JOIN agent_runners r ON r.id=a.runner_id \
             WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? \
             AND a.lease_expires_at>? AND r.state='active' AND r.revoked_at IS NULL",
        )
        .bind(input.attempt_id)
        .bind(input.runner_id)
        .bind(input.fencing_token)
        .bind(&now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(AttemptTransitionResult::Stale);
        };

        let state = state::parse(&row.get::<String, _>("state"))?;
        let workspace_id: Option<String> = row.get("workspace_id");
        let base_revision: Option<String> = row.get("base_revision");
        let process_id: Option<String> = row.get("process_id");
        let prepared_at: Option<String> = row.get("prepared_at");
        let started_at: Option<String> = row.get("started_at");
        let exact_workspace = workspace_id.as_deref() == Some(input.workspace_id)
            && base_revision.as_deref() == Some(input.base_revision);

        match input.phase {
            AttemptTransitionPhase::Preparing if exact_workspace && prepared_at.is_some() => {
                tx.commit().await?;
                let Some(committed_at) = prepared_at else {
                    return Ok(AttemptTransitionResult::Conflict);
                };
                return Ok(AttemptTransitionResult::Replayed(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: state::text(ExecutionState::Preparing).into(),
                        committed_at,
                    },
                ));
            }
            AttemptTransitionPhase::Running
                if exact_workspace
                    && process_id.as_deref() == input.process_id
                    && started_at.is_some() =>
            {
                tx.commit().await?;
                let Some(committed_at) = started_at else {
                    return Ok(AttemptTransitionResult::Conflict);
                };
                return Ok(AttemptTransitionResult::Replayed(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: state::text(ExecutionState::Running).into(),
                        committed_at,
                    },
                ));
            }
            AttemptTransitionPhase::Preparing if state == ExecutionState::Leased => {
                let updated = sqlx::query(
                    "UPDATE execution_attempts SET state='preparing',workspace_id=?,base_revision=?,prepared_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND state='leased' AND lease_expires_at>?",
                )
                .bind(input.workspace_id)
                .bind(input.base_revision)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id)
                .bind(input.runner_id)
                .bind(input.fencing_token)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() != 1 {
                    tx.rollback().await?;
                    return Ok(AttemptTransitionResult::Stale);
                }
                mirror_request_state(&mut tx, input.attempt_id, ExecutionState::Preparing, &now)
                    .await?;
                tx.commit().await?;
                return Ok(AttemptTransitionResult::Applied(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: state::text(ExecutionState::Preparing).into(),
                        committed_at: now,
                    },
                ));
            }
            AttemptTransitionPhase::Running
                if state == ExecutionState::Preparing && exact_workspace =>
            {
                let updated = sqlx::query(
                    "UPDATE execution_attempts SET state='running',process_id=?,started_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND state='preparing' \
                     AND workspace_id=? AND base_revision=? AND lease_expires_at>?",
                )
                .bind(input.process_id)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id)
                .bind(input.runner_id)
                .bind(input.fencing_token)
                .bind(input.workspace_id)
                .bind(input.base_revision)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() != 1 {
                    tx.rollback().await?;
                    return Ok(AttemptTransitionResult::Stale);
                }
                mirror_request_state(&mut tx, input.attempt_id, ExecutionState::Running, &now)
                    .await?;
                tx.commit().await?;
                return Ok(AttemptTransitionResult::Applied(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: state::text(ExecutionState::Running).into(),
                        committed_at: now,
                    },
                ));
            }
            _ => {}
        }

        tx.commit().await?;
        Ok(AttemptTransitionResult::Conflict)
    }

    pub async fn observe_cancellation(
        &self,
        input: CancellationObservationInput<'_>,
        clock: &dyn Clock,
    ) -> Result<CancellationObservation, StoreError> {
        let now = stamp(clock);
        let fingerprint = cancellation_fingerprint(&input)?;
        // Serialize duplicate cancellation observations for the same attempt
        // (e.g. a runner retrying an unacknowledged cancellation report)
        // before either transaction reads this attempt/request row. Two
        // deferred readers can otherwise deadlock while both try to upgrade
        // to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.lease_expires_at,r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=?")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_cancellation_replays WHERE attempt_id=? AND cancellation_request_id=?")
            .bind(input.attempt_id).bind(input.cancellation_request_id).fetch_optional(&mut *tx).await? {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(CancellationObservation::Conflict);
            }
            let response: String = row.get("response");
            let response = cancellation_response(&response)?;
            tx.commit().await?;
            return Ok(CancellationObservation::Replayed(response));
        }
        let request: String = row.get("request_id");
        let owner: String = row.get("runner_id");
        let state: String = row.get("state");
        if terminal(&state) {
            tx.commit().await?;
            return Ok(CancellationObservation::AlreadyTerminal { state });
        }
        if !state::is_live(&state) {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let expires: String = row.get("lease_expires_at");
        if expires <= now {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        }
        let cancellation_requested_at: Option<String> = row.get("cancellation_requested_at");
        if cancellation_requested_at.is_none() {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let updated = sqlx::query("UPDATE execution_attempts SET state='cancelled',completion_id=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at>?")
            .bind(input.cancellation_request_id).bind(&now).bind(&now).bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        end_attempt_decisions(&mut tx, input.attempt_id, &now).await?;
        sqlx::query("UPDATE execution_requests SET state='cancelled',updated_at=? WHERE id=?")
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(owner).execute(&mut *tx).await?;
        let response = CancellationResponse {
            attempt_id: input.attempt_id.into(),
            cancellation_request_id: input.cancellation_request_id.into(),
            state: "cancelled".into(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "cancellation_request_id": response.cancellation_request_id,
            "state": response.state,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_cancellation_replays(attempt_id,cancellation_request_id,state,fingerprint,response,created_at) VALUES(?,?, 'cancelled',?,?,?)")
            .bind(input.attempt_id).bind(input.cancellation_request_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CancellationObservation::Cancelled(response))
    }

    #[instrument(skip(self, clock))]
    pub async fn complete_execution(
        &self,
        completion: Completion<'_>,
        clock: &dyn Clock,
    ) -> Result<bool, StoreError> {
        Ok(matches!(
            self.complete_execution_result(completion, clock).await?,
            CompletionResult::Committed(_) | CompletionResult::Replayed(_)
        ))
    }

    /// Records the cancellation request. When nobody holds the run any more
    /// — its latest attempt is `needs_operator`, or its lease has expired —
    /// there is no runner to observe the request, so the attempt and the
    /// request end `cancelled` at once: that is the person's decision. A run
    /// somebody holds only gets `cancellation_requested_at`, which its runner
    /// reads on the next heartbeat.
    #[instrument(skip(self, clock))]
    pub async fn request_execution_cancellation(
        &self,
        request_id: &str,
        clock: &dyn Clock,
    ) -> Result<bool, StoreError> {
        let now = stamp(clock);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let applied = apply_cancellation(&mut tx, request_id, &now).await?;
        tx.commit().await?;
        Ok(applied)
    }

    /// [`Self::request_execution_cancellation`], answering what became of the request in the
    /// same transaction: ended at once, waiting for its runner, already terminal, or unknown.
    #[instrument(skip(self, clock))]
    pub async fn cancel_execution_request(
        &self,
        request_id: &str,
        clock: &dyn Clock,
    ) -> Result<CancellationOutcome, StoreError> {
        let now = stamp(clock);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let applied = apply_cancellation(&mut tx, request_id, &now).await?;
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
                .bind(request_id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(match (applied, state) {
            (_, None) => CancellationOutcome::NotFound,
            (false, Some(_)) => CancellationOutcome::AlreadyTerminal,
            (true, Some(state)) if state::parse(&state)? == ExecutionState::Cancelled => {
                CancellationOutcome::Ended
            }
            (true, Some(_)) => CancellationOutcome::Requested,
        })
    }
}

/// What a cancellation request did to an execution request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    /// Nobody held the run: it ended `cancelled` at once.
    Ended,
    /// A runner holds the run: the request is recorded and the runner reports the outcome.
    Requested,
    /// The request had already reached a terminal state.
    AlreadyTerminal,
    NotFound,
}

/// Marks the request for cancellation and ends it at once when nobody holds it. `false` when the
/// request is unknown or already terminal.
async fn apply_cancellation(
    tx: &mut sqlx::SqliteConnection,
    request_id: &str,
    now: &str,
) -> Result<bool, StoreError> {
    let result = sqlx::query("UPDATE execution_requests SET cancellation_requested_at = COALESCE(cancellation_requested_at, ?), updated_at = ? WHERE id = ? AND state NOT IN ('succeeded','failed','cancelled')")
        .bind(now).bind(now).bind(request_id).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Ok(false);
    }
    let latest = sqlx::query("SELECT id,runner_id,state,lease_expires_at FROM execution_attempts WHERE request_id=? ORDER BY attempt_number DESC LIMIT 1")
        .bind(request_id).fetch_optional(&mut *tx).await?;
    if let Some(latest) = latest {
        let state: String = latest.get("state");
        let expired = latest.get::<String, _>("lease_expires_at").as_str() <= now;
        let active = state::is_live(&state);
        if state::is_needs_operator(&state) || (active && expired) {
            let attempt: String = latest.get("id");
            sqlx::query("UPDATE execution_attempts SET state='cancelled',ended_at=?,updated_at=? WHERE id=?")
                .bind(now).bind(now).bind(&attempt).execute(&mut *tx).await?;
            end_attempt_decisions(tx, &attempt, now).await?;
            sqlx::query("UPDATE execution_requests SET state='cancelled',updated_at=? WHERE id=?")
                .bind(now)
                .bind(request_id)
                .execute(&mut *tx)
                .await?;
            // A `needs_operator` attempt already gave its slot back.
            if active {
                let runner: String = latest.get("runner_id");
                sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?")
                    .bind(now).bind(&runner).execute(&mut *tx).await?;
            }
        }
    }
    Ok(true)
}
