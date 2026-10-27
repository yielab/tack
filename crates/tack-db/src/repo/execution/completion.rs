//! Completion: an attempt moves through its states, ends with a result or a cancellation, and replays of either answer the same.

use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::{instrument, warn};

use super::super::items::{AtomicItemUpdateOutcome, update_item_in};
use super::decisions::end_attempt_decisions;
use super::enqueue::{
    EnqueueResult, NewExecutionRequest, enqueue_in, validate_execution_request_snapshot,
};
use super::shared::{mirror_request_state, terminal, token_column};
use super::state::{self, Move};
use crate::canonical_json;
use crate::{Repository, StoreError};
use tack_core::models::UpdateItem;
use tack_core::workflow::WorkflowConfig;
use tack_protocol::execution::{
    self, ActualExecution, AttemptId, CancellationRequestId, Checkpoint, CompletionId,
    ExecutionRequestId, ExecutionState, FencingToken, RunnerId, Usage, WorkspaceId,
};
use uuid::Uuid;

/// What a successful completion sets going, written in the completion's own transaction: either
/// the attempt ends with all of it or with none of it, and the runner's retry finds the rest.
#[derive(Debug, Default)]
pub struct FollowUps<'a> {
    /// The board move the project maps a finished run to.
    pub item_update: Option<ItemUpdate<'a>>,
    /// The run that goes back to the agent with what the verifier asked it to fix.
    pub rework: Option<Rework<'a>>,
}

/// A move of one item, validated against `workflow` as any other edit of it is.
#[derive(Debug)]
pub struct ItemUpdate<'a> {
    pub item_id: Uuid,
    pub update: UpdateItem,
    pub workflow: &'a WorkflowConfig,
}

/// A follow-up run to enqueue. `created_at` is the instant the request's snapshot names.
#[derive(Debug)]
pub struct Rework<'a> {
    pub request: NewExecutionRequest<'a>,
    pub created_at: DateTime<Utc>,
}

/// What the [`FollowUps`] did; empty unless the completion was committed. A move the workflow
/// refuses is an outcome, not an error: the completion stands and the item stays where it is.
#[derive(Debug, Default)]
pub struct FollowUpsOutcome {
    pub item_update: Option<AtomicItemUpdateOutcome>,
    pub rework: Option<EnqueueResult>,
}

#[derive(Debug, Clone)]
pub struct Completion<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    pub completion_id: &'a CompletionId,
    pub final_event_checkpoint: Option<&'a Checkpoint>,
    pub terminal_state: ExecutionState,
    /// An object the harness adapter fills (`code`, `message`, and whatever else that harness
    /// can say, such as the staged artifacts). Its keys are open, so it stays a JSON object: the
    /// board stores and shows it and branches on only the few keys it names where it reads them.
    pub terminal_reason: &'a serde_json::Map<String, serde_json::Value>,
    pub actual_execution: &'a ActualExecution,
    pub usage: &'a Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptTransitionPhase {
    Preparing,
    Running,
}

#[derive(Debug, Clone)]
pub struct AttemptTransitionInput<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    pub phase: AttemptTransitionPhase,
    pub workspace_id: &'a WorkspaceId,
    pub base_revision: &'a str,
    pub process_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptTransitionResponse {
    pub attempt_id: AttemptId,
    pub state: ExecutionState,
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
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    pub cancellation_request_id: &'a CancellationRequestId,
    pub observed_at: DateTime<Utc>,
    pub details: &'a str,
    pub observation: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationResponse {
    pub attempt_id: AttemptId,
    pub cancellation_request_id: CancellationRequestId,
    pub state: ExecutionState,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationObservation {
    Cancelled(CancellationResponse),
    Replayed(CancellationResponse),
    Conflict,
    Stale,
    AlreadyTerminal { state: ExecutionState },
    Ambiguous { state: ExecutionState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub completion_id: CompletionId,
    pub terminal_state: ExecutionState,
    pub final_event_checkpoint: Option<Checkpoint>,
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

fn encode(value: &impl serde::Serialize) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|error| StoreError::unwritable(error))
}

fn canonical_value(value: &impl serde::Serialize) -> Result<serde_json::Value, StoreError> {
    serde_json::to_value(value)
        .map(canonical_json)
        .map_err(|error| StoreError::unwritable(error))
}

fn completion_fingerprint(completion: &Completion<'_>) -> Result<String, StoreError> {
    serde_json::to_string(&serde_json::json!({
        "runner_id": completion.runner_id.as_str(),
        "attempt_id": completion.attempt_id.as_str(),
        "fencing_token": completion.fencing_token.get(),
        "completion_id": completion.completion_id.as_str(),
        "final_event_checkpoint": completion.final_event_checkpoint.map(Checkpoint::as_str),
        "terminal_state": completion.terminal_state,
        "terminal_reason": canonical_value(completion.terminal_reason)?,
        "actual_execution": canonical_value(completion.actual_execution)?,
        "usage": canonical_value(completion.usage)?,
    }))
    .map_err(|error| StoreError::unwritable(error))
}

fn replay_response(serialized: &str) -> Result<CompletionResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| StoreError::corrupt("execution_completion_replays", "response", error))?;
    let object = value.as_object().ok_or_else(|| {
        StoreError::corrupt(
            "execution_completion_replays",
            "response",
            "invalid completion replay response",
        )
    })?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::corrupt(
                    "execution_completion_replays",
                    "response",
                    format!("invalid completion replay response field: {field}"),
                )
            })
    };
    let final_event_checkpoint = match object.get("final_event_checkpoint") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(Checkpoint::new(value.clone())),
        _ => {
            return Err(StoreError::corrupt(
                "execution_completion_replays",
                "response",
                "invalid completion replay response field: final_event_checkpoint",
            ));
        }
    };
    Ok(CompletionResponse {
        completion_id: CompletionId::new(required("completion_id")?),
        terminal_state: state::parse(
            "execution_completion_replays",
            "response",
            &required("terminal_state")?,
        )?,
        final_event_checkpoint,
        committed_at: required("committed_at")?,
    })
}

fn cancellation_fingerprint(
    input: &CancellationObservationInput<'_>,
) -> Result<String, StoreError> {
    let details: serde_json::Value =
        serde_json::from_str(input.details).map_err(|error| StoreError::unwritable(error))?;
    let observation: serde_json::Value =
        serde_json::from_str(input.observation).map_err(|error| StoreError::unwritable(error))?;
    if observation != serde_json::Value::String("process_stopped".into()) {
        return Err(StoreError::unwritable(
            "cancellation observation must be JSON string process_stopped",
        ));
    }
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id.as_str(),
        "attempt_id": input.attempt_id.as_str(),
        "fencing_token": input.fencing_token.get(),
        "cancellation_request_id": input.cancellation_request_id.as_str(),
        "observed_at": input.observed_at.to_rfc3339(),
        "details": canonical_json(details),
        "observation": canonical_json(observation),
    }))
    .map_err(|error| StoreError::unwritable(error))
}

fn cancellation_response(serialized: &str) -> Result<CancellationResponse, StoreError> {
    let value: serde_json::Value = serde_json::from_str(serialized).map_err(|error| {
        StoreError::corrupt("execution_cancellation_replays", "response", error)
    })?;
    let object = value.as_object().ok_or_else(|| {
        StoreError::corrupt(
            "execution_cancellation_replays",
            "response",
            "invalid cancellation replay response",
        )
    })?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                StoreError::corrupt(
                    "execution_cancellation_replays",
                    "response",
                    format!("invalid cancellation replay response field: {field}"),
                )
            })
    };
    Ok(CancellationResponse {
        attempt_id: AttemptId::new(required("attempt_id")?),
        cancellation_request_id: CancellationRequestId::new(required("cancellation_request_id")?),
        state: state::parse(
            "execution_cancellation_replays",
            "response",
            &required("state")?,
        )?,
        committed_at: required("committed_at")?,
    })
}

impl Repository {
    pub async fn complete_execution_result(
        &self,
        completion: Completion<'_>,
        now: DateTime<Utc>,
    ) -> Result<CompletionResult, StoreError> {
        Ok(self
            .complete_execution_with_follow_ups(completion, FollowUps::default(), now)
            .await?
            .0)
    }

    /// [`Self::complete_execution_result`], and when it commits, the `follow_ups` in the same
    /// transaction. A follow-up that fails with a store error undoes the completion with it (the
    /// runner reports again); one the rules refuse is in the outcome and the completion stands.
    #[instrument(skip(self, completion, follow_ups))]
    pub async fn complete_execution_with_follow_ups(
        &self,
        completion: Completion<'_>,
        follow_ups: FollowUps<'_>,
        now: DateTime<Utc>,
    ) -> Result<(CompletionResult, FollowUpsOutcome), StoreError> {
        let none = FollowUpsOutcome::default;
        if !completion.terminal_state.is_terminal() {
            return Ok((CompletionResult::Conflict, none()));
        }
        let now = now.to_rfc3339();
        let fencing_token = token_column(completion.fencing_token)?;
        let fingerprint = completion_fingerprint(&completion)?;
        let (terminal_reason, actual_execution, usage) = (
            encode(completion.terminal_reason)?,
            encode(completion.actual_execution)?,
            encode(completion.usage)?,
        );
        // A follow-up run Tack built wrong is dropped with a warning rather than failing the
        // completion for good: the runner would only report the same thing again.
        let rework = follow_ups.rework.and_then(|rework| {
            match validate_execution_request_snapshot(&rework.request) {
                Ok(snapshot) => Some((rework, snapshot)),
                Err(error) => {
                    warn!(attempt_id = %completion.attempt_id, %error, "rework: the follow-up request is invalid; not enqueued");
                    None
                }
            }
        });
        // Serialize duplicate completion reports (e.g. a runner retrying an
        // unacknowledged terminal POST) before either transaction reads this
        // attempt row. Two deferred readers can otherwise deadlock while
        // both try to upgrade to writers. Mirrors the redeem_enrollment_token
        // and claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT request_id,runner_id,state,lease_expires_at,event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(completion.attempt_id.as_str())
        .bind(completion.runner_id.as_str())
        .bind(fencing_token)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok((CompletionResult::Stale, none()));
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_completion_replays WHERE attempt_id=? AND completion_id=?",
        )
        .bind(completion.attempt_id.as_str())
        .bind(completion.completion_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                // Same (attempt_id, completion_id) idempotency-scoped key,
                // different content: this can never succeed by retrying.
                tx.commit().await?;
                return Ok((CompletionResult::IdempotencyConflict, none()));
            }
            let response: String = row.get("response");
            let response = replay_response(&response)?;
            tx.commit().await?;
            return Ok((CompletionResult::Replayed(response), none()));
        }
        let state: String = row.get("state");
        let expires: String = row.get("lease_expires_at");
        if terminal(&state) {
            // Pre-M055 terminal attempts have no authoritative response to
            // replay. This is a distinct completion_id from whichever one
            // terminated the attempt, not a reused idempotency key with
            // different content, so it is the benign/retryable `Conflict`.
            tx.commit().await?;
            return Ok((CompletionResult::Conflict, none()));
        }
        if expires <= now {
            tx.commit().await?;
            return Ok((CompletionResult::Stale, none()));
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let guard = Move::Finish(completion.terminal_state).guard();
        let updated = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE execution_attempts SET state=?,completion_id=?,terminal_reason=?,actual_execution=?,usage=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ? AND {guard} AND lease_expires_at > ?")))
            .bind(state::text(completion.terminal_state)).bind(completion.completion_id.as_str()).bind(terminal_reason).bind(actual_execution).bind(usage).bind(&now).bind(&now).bind(completion.attempt_id.as_str()).bind(completion.runner_id.as_str()).bind(fencing_token).bind(completion.final_event_checkpoint.map(Checkpoint::as_str)).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            // Defensive: near-unreachable under the BEGIN IMMEDIATE this
            // function already opens, but a lost compare-and-set is a
            // benign/retryable `Conflict`, not an idempotency-key reuse.
            tx.rollback().await?;
            return Ok((CompletionResult::Conflict, none()));
        }
        end_attempt_decisions(&mut tx, completion.attempt_id, &now).await?;
        sqlx::query("UPDATE execution_requests SET state=?,updated_at=? WHERE id=?")
            .bind(state::text(completion.terminal_state))
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(runner).execute(&mut *tx).await?;
        let response = CompletionResponse {
            completion_id: completion.completion_id.clone(),
            terminal_state: completion.terminal_state,
            final_event_checkpoint: completion.final_event_checkpoint.cloned(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "completion_id": response.completion_id.as_str(),
            "terminal_state": state::text(response.terminal_state),
            "final_event_checkpoint": response.final_event_checkpoint.as_ref().map(Checkpoint::as_str),
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_completion_replays(attempt_id,completion_id,fingerprint,response,committed_at) VALUES(?,?,?,?,?)").bind(completion.attempt_id.as_str()).bind(completion.completion_id.as_str()).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        let mut done = FollowUpsOutcome::default();
        if let Some(move_item) = follow_ups.item_update {
            done.item_update = Some(
                update_item_in(
                    &mut tx,
                    move_item.item_id,
                    move_item.update,
                    move_item.workflow,
                    None,
                )
                .await?,
            );
        }
        if let Some((rework, snapshot)) = rework {
            let created_at = rework.created_at.to_rfc3339();
            done.rework = Some(enqueue_in(&mut tx, &rework.request, &snapshot, &created_at).await?);
        }
        tx.commit().await?;
        Ok((CompletionResult::Committed(response), done))
    }

    /// Stores what a runner reported when it started the agent (`report` is JSON text) on the
    /// attempt it holds under `fencing_token`. Nothing is written for an attempt another runner
    /// or fence holds.
    pub async fn record_start_report(
        &self,
        runner_id: &RunnerId,
        attempt_id: &AttemptId,
        fencing_token: FencingToken,
        report: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE execution_attempts SET start_report=? \
             WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(report)
        .bind(attempt_id.as_str())
        .bind(runner_id.as_str())
        .bind(token_column(fencing_token)?)
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
        now: DateTime<Utc>,
    ) -> Result<AttemptTransitionResult, StoreError> {
        if input.workspace_id.as_str().is_empty()
            || input.base_revision.is_empty()
            || matches!(input.phase, AttemptTransitionPhase::Preparing)
                && input.process_id.is_some()
            || matches!(input.phase, AttemptTransitionPhase::Running)
                && (input.process_id.is_none() || input.process_id.is_some_and(str::is_empty))
        {
            return Ok(AttemptTransitionResult::Conflict);
        }

        let now = now.to_rfc3339();
        let fencing_token = token_column(input.fencing_token)?;
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
        .bind(input.attempt_id.as_str())
        .bind(input.runner_id.as_str())
        .bind(fencing_token)
        .bind(&now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(AttemptTransitionResult::Stale);
        };

        let state = state::parse(
            "execution_attempts",
            "state",
            &row.get::<String, _>("state"),
        )?;
        let workspace_id: Option<String> = row.get("workspace_id");
        let base_revision: Option<String> = row.get("base_revision");
        let process_id: Option<String> = row.get("process_id");
        let prepared_at: Option<String> = row.get("prepared_at");
        let started_at: Option<String> = row.get("started_at");
        let exact_workspace = workspace_id.as_deref() == Some(input.workspace_id.as_str())
            && base_revision.as_deref() == Some(input.base_revision);

        match input.phase {
            AttemptTransitionPhase::Preparing if exact_workspace && prepared_at.is_some() => {
                tx.commit().await?;
                let Some(committed_at) = prepared_at else {
                    return Ok(AttemptTransitionResult::Conflict);
                };
                return Ok(AttemptTransitionResult::Replayed(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.clone(),
                        state: ExecutionState::Preparing,
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
                        attempt_id: input.attempt_id.clone(),
                        state: ExecutionState::Running,
                        committed_at,
                    },
                ));
            }
            AttemptTransitionPhase::Preparing if state == ExecutionState::Leased => {
                let (set, guard) = (Move::Prepare.set(), Move::Prepare.guard());
                let updated = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE execution_attempts SET {set},workspace_id=?,base_revision=?,prepared_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND {guard} AND lease_expires_at>?"
                )))
                .bind(input.workspace_id.as_str())
                .bind(input.base_revision)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id.as_str())
                .bind(input.runner_id.as_str())
                .bind(fencing_token)
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
                        attempt_id: input.attempt_id.clone(),
                        state: ExecutionState::Preparing,
                        committed_at: now,
                    },
                ));
            }
            AttemptTransitionPhase::Running
                if state == ExecutionState::Preparing && exact_workspace =>
            {
                let (set, guard) = (Move::Start.set(), Move::Start.guard());
                let updated = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE execution_attempts SET {set},process_id=?,started_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND {guard} \
                     AND workspace_id=? AND base_revision=? AND lease_expires_at>?"
                )))
                .bind(input.process_id)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id.as_str())
                .bind(input.runner_id.as_str())
                .bind(fencing_token)
                .bind(input.workspace_id.as_str())
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
                        attempt_id: input.attempt_id.clone(),
                        state: ExecutionState::Running,
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
        now: DateTime<Utc>,
    ) -> Result<CancellationObservation, StoreError> {
        let now = now.to_rfc3339();
        let fencing_token = token_column(input.fencing_token)?;
        let fingerprint = cancellation_fingerprint(&input)?;
        // Serialize duplicate cancellation observations for the same attempt
        // (e.g. a runner retrying an unacknowledged cancellation report)
        // before either transaction reads this attempt/request row. Two
        // deferred readers can otherwise deadlock while both try to upgrade
        // to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.lease_expires_at,r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=?")
            .bind(input.attempt_id.as_str()).bind(input.runner_id.as_str()).bind(fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_cancellation_replays WHERE attempt_id=? AND cancellation_request_id=?")
            .bind(input.attempt_id.as_str()).bind(input.cancellation_request_id.as_str()).fetch_optional(&mut *tx).await? {
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
        let state = state::parse(
            "execution_attempts",
            "state",
            &row.get::<String, _>("state"),
        )?;
        if state.is_terminal() {
            tx.commit().await?;
            return Ok(CancellationObservation::AlreadyTerminal { state });
        }
        if !execution::lease_held().contains(&state) {
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
        let (set, guard) = (
            Move::Finish(ExecutionState::Cancelled).set(),
            Move::Finish(ExecutionState::Cancelled).guard(),
        );
        let updated = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE execution_attempts SET {set},completion_id=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND {guard} AND lease_expires_at>?")))
            .bind(input.cancellation_request_id.as_str()).bind(&now).bind(&now).bind(input.attempt_id.as_str()).bind(input.runner_id.as_str()).bind(fencing_token).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        end_attempt_decisions(&mut tx, input.attempt_id, &now).await?;
        sqlx::query(state::sql(
            "UPDATE execution_requests SET state={cancelled},updated_at=? WHERE id=?",
        ))
        .bind(&now)
        .bind(request)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(owner).execute(&mut *tx).await?;
        let response = CancellationResponse {
            attempt_id: input.attempt_id.clone(),
            cancellation_request_id: input.cancellation_request_id.clone(),
            state: ExecutionState::Cancelled,
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id.as_str(),
            "cancellation_request_id": response.cancellation_request_id.as_str(),
            "state": response.state,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_cancellation_replays(attempt_id,cancellation_request_id,state,fingerprint,response,created_at) VALUES(?,?,?,?,?,?)")
            .bind(input.attempt_id.as_str()).bind(input.cancellation_request_id.as_str()).bind(state::text(ExecutionState::Cancelled)).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CancellationObservation::Cancelled(response))
    }

    #[instrument(skip(self))]
    pub async fn complete_execution(
        &self,
        completion: Completion<'_>,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        Ok(matches!(
            self.complete_execution_result(completion, now).await?,
            CompletionResult::Committed(_) | CompletionResult::Replayed(_)
        ))
    }

    /// Records the cancellation request. When nobody holds the run any more
    /// — its latest attempt is `needs_operator`, or its lease has expired —
    /// there is no runner to observe the request, so the attempt and the
    /// request end `cancelled` at once: that is the person's decision. A run
    /// somebody holds only gets `cancellation_requested_at`, which its runner
    /// reads on the next heartbeat.
    #[instrument(skip(self))]
    pub async fn request_execution_cancellation(
        &self,
        request_id: &ExecutionRequestId,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let applied = apply_cancellation(&mut tx, request_id.as_str(), &now).await?;
        tx.commit().await?;
        Ok(applied)
    }

    /// [`Self::request_execution_cancellation`], answering what became of the request in the
    /// same transaction: ended at once, waiting for its runner, already terminal, or unknown.
    #[instrument(skip(self))]
    pub async fn cancel_execution_request(
        &self,
        request_id: &ExecutionRequestId,
        now: DateTime<Utc>,
    ) -> Result<CancellationOutcome, StoreError> {
        let now = now.to_rfc3339();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let applied = apply_cancellation(&mut tx, request_id.as_str(), &now).await?;
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
                .bind(request_id.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(match (applied, state) {
            (_, None) => CancellationOutcome::NotFound,
            (false, Some(_)) => CancellationOutcome::AlreadyTerminal,
            (true, Some(state))
                if state::parse("execution_requests", "state", &state)?
                    == ExecutionState::Cancelled =>
            {
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
    let result = sqlx::query(state::sql("UPDATE execution_requests SET cancellation_requested_at = COALESCE(cancellation_requested_at, ?), updated_at = ? WHERE id = ? AND state NOT IN ({terminal})"))
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
            let attempt = AttemptId::new(latest.get::<String, _>("id"));
            let (set, guard) = (Move::Cancel.set(), Move::Cancel.guard());
            sqlx::query(state::sql(&format!(
                "UPDATE execution_attempts SET {set},ended_at=?,updated_at=? WHERE id=? AND {guard}"
            )))
            .bind(now)
            .bind(now)
            .bind(attempt.as_str())
            .execute(&mut *tx)
            .await?;
            end_attempt_decisions(tx, &attempt, now).await?;
            sqlx::query(state::sql(&format!(
                "UPDATE execution_requests SET {set},updated_at=? WHERE id=? AND {guard}"
            )))
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
