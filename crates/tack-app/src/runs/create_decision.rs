use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use tack_db::repo::execution::NewDecision;
use tack_db::{Repository, queries};
use tack_protocol::execution::{
    AttemptId, DecisionOption, DecisionRecommendation, ExecutionState, FencingToken, RunnerId,
    TransitionActor, validate_transition,
};
use uuid::Uuid;

use crate::AppError;

/// A question an attempt asks the person while it waits.
pub struct DecisionRequest<'a> {
    pub runner_id: &'a RunnerId,
    pub attempt_id: &'a AttemptId,
    pub fencing_token: FencingToken,
    /// The runner's own id for the question; the same id again is a replay.
    pub decision_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub options: &'a [DecisionOption],
    /// What the harness says about the question; shown, never branched on.
    pub metadata: &'a Map<String, Value>,
    pub recommendation: Option<&'a DecisionRecommendation>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionCreated {
    /// The decision is stored, now or by an earlier identical call.
    Created {
        /// The decision's own state, as stored.
        state: String,
        created_at: String,
    },
    /// The fence is not the attempt's current one, or its lease has ended.
    Stale,
    /// The attempt is in a state from which it cannot wait on a decision.
    Ineligible { state: ExecutionState },
    /// `decision_id` was already used with different content.
    IdempotencyConflict,
}

/// Opens a decision on an attempt and parks the attempt, and its request, in `waiting_decision`.
/// An attempt may open one while it is running or already waiting on another; any other state,
/// a stale fence or an ended lease refuses it. Idempotent on `decision_id`: the same content
/// again returns the stored decision, other content under the same id is an idempotency conflict
/// and overwrites nothing.
pub async fn create_decision(
    repo: &Repository,
    now: DateTime<Utc>,
    request: DecisionRequest<'_>,
) -> Result<DecisionCreated, AppError> {
    // A wrong runner or fence (`Stale`) is told apart from an attempt that cannot wait
    // (`Ineligible`) before the write, which collapses both into one answer.
    let Some(lease) = queries::execution_attempts::attempt_lease(
        repo,
        request.attempt_id,
        request.runner_id,
        request.fencing_token,
    )
    .await?
    else {
        return Ok(DecisionCreated::Stale);
    };
    if lease.lease_expires_at <= now.to_rfc3339() {
        return Ok(DecisionCreated::Stale);
    }
    // Eligibility is the frozen lifecycle's: a decision may open while the attempt already waits
    // on one, or while its lease owner may park it there, which the store then does in the same
    // transaction as the insert.
    let may_open = lease.state == ExecutionState::WaitingDecision
        || validate_transition(
            lease.state,
            ExecutionState::WaitingDecision,
            TransitionActor::LeaseOwner,
        )
        .is_ok();
    if !may_open {
        return Ok(DecisionCreated::Ineligible { state: lease.state });
    }

    let row_id = format!("dec_row_{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            request.runner_id,
            request.attempt_id,
            request.fencing_token,
            NewDecision {
                id: &row_id,
                decision_id: request.decision_id,
                kind: request.kind,
                prompt: request.prompt,
                options: request.options,
                metadata: request.metadata,
                expires_at: request.expires_at,
                recommendation: request.recommendation,
            },
            now,
        )
        .await?;
    if !written {
        return Ok(DecisionCreated::Stale);
    }

    // The store inserts with `ON CONFLICT (attempt_id, decision_id) DO NOTHING`, so a reused
    // `decision_id` with different content would otherwise report success without writing the
    // new content. Read the committed row back and treat a difference as an idempotency
    // conflict; an exact replay answers with the row's own state and creation time.
    let stored = queries::execution_decisions::decision_of_attempt(
        repo,
        request.attempt_id.as_str(),
        request.decision_id,
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let stored_options: Option<Vec<DecisionOption>> = serde_json::from_str(&stored.options).ok();
    let stored_metadata: Option<Map<String, Value>> = serde_json::from_str(&stored.metadata).ok();
    let stored_recommendation: Option<DecisionRecommendation> = stored
        .recommendation
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok());
    let requested_expires_at = request.expires_at.map(|value| value.to_rfc3339());
    // A settled decision no longer holds the call it was asked with, so a replay of the create
    // that carries it is the same request, not a different one.
    let mut requested_metadata = request.metadata.clone();
    if stored.state != "pending" {
        requested_metadata.remove("call");
    }
    if stored.kind != request.kind
        || stored.prompt != request.prompt
        || stored_options.as_deref() != Some(request.options)
        || stored_metadata.as_ref() != Some(&requested_metadata)
        || stored_recommendation.as_ref() != request.recommendation
        || stored.expires_at != requested_expires_at
    {
        return Ok(DecisionCreated::IdempotencyConflict);
    }
    Ok(DecisionCreated::Created {
        state: stored.state,
        created_at: stored.created_at,
    })
}
