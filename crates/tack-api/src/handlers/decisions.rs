//! Operator decision-resolution repository/service/handler module.
//! Registered in `handlers.rs` and merged into the operator router in
//! `router.rs`'s `operator_execution_routes` — **before** `require_token`
//! is applied and **with** `inject_operator_principal` layered directly on
//! top, exactly like every other route that function merges.
//!
//! **Security boundary: runner may raise/read, never resolve.** This
//! module reads exactly one identity signal, the `x-tack-principal` header
//! (see [`principal`]) — never `Authorization`, so no runner bearer
//! credential can authenticate here even if presented. A runner may raise
//! and read its own attempt's decision (`handlers/runner_protocol.rs`),
//! but resolution lives on this structurally separate route family, mounted
//! behind `require_token` — proven in `tests/runner_protocol/decisions.rs`'s
//! `self_resolution_via_a_valid_runner_bearer_credential_is_denied_and_writes_nothing`.
//!
//! **Item status.** `execution_requests.status_map_policy_id` resolves to a
//! [`tack_core::workflow::StatusMapPolicy`], and `submit_completion` in
//! `handlers/runner_protocol.rs` is what acts on it. No function in this file
//! writes `items.status`, directly or indirectly: `resolve` never touches the
//! `items` table.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tack_app::runs::resolve_decision::ResolveOutcome;
use tack_app::{AppError, runs};
use tack_core::decision::DecisionResolvedBy;
use tack_db::Repository;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, ProtocolErrorEnvelope, StableErrorCode};
use utoipa::ToSchema;

use crate::middleware::constant_time_eq;

/// `docs/contracts/runner-v1/limits.json`'s `decision_answer_bytes_max`,
/// mirrored exactly (pinned against the live fixture by
/// `decision_answer_limit_matches_frozen_fixture` below) — the same limit
/// `handlers/runner_protocol.rs`'s `create_decision` documents as bounding
/// "an operator's decision *answer*". That card's own comment names this
/// exact limit as belonging to decision resolution, not runner-side
/// creation, which this module implements.
const DECISION_ANSWER_BYTES_MAX: u64 = 32_768;

/// State for this module's local router, constructed
/// from the shared API state the same way `operator_execution_routes`
/// constructs `executions::OperatorExecutionState` today.
#[derive(Clone)]
pub struct DecisionOperatorState {
    pub repo: Repository,
    pub clock: Arc<dyn Clock>,
    /// `TACK_EXECUTION_DECISION_TOKEN`. `None` means
    /// "not configured on this server" — the fail-closed default: nothing
    /// is ever compared against an unset token. See
    /// [`require_decision_token`].
    pub decision_token: Option<String>,
}

impl DecisionOperatorState {
    /// `decision_token` defaults to `None` (fail-closed) — a caller must
    /// opt in via [`Self::with_decision_token`] to ever allow a resolve
    /// through.
    pub fn with_clock(repo: Repository, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            decision_token: None,
        }
    }

    /// Builder, mirroring `runner_protocol::RunnerProtocolState::with_artifact_storage_root`'s
    /// established convention for an additive, post-construction config
    /// value.
    pub fn with_decision_token(mut self, token: Option<String>) -> Self {
        self.decision_token = token;
        self
    }
}

/// Static request-correlation id placeholder, matching
/// `executions::OPERATOR_REQUEST_ID`'s own convention — no per-request
/// correlation id is wired into these error envelopes yet.
const OPERATOR_REQUEST_ID: &str = "req_operator_decisions";

/// Builds the stable v1 error envelope via `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` so it can never drift from
/// `docs/contracts/runner-v1/errors/*.json`. `details` follows the per-code
/// shape documented in `docs/contracts/runner-v1/README.md`.
fn error(
    status: StatusCode,
    code: StableErrorCode,
    message: &str,
    details: Value,
) -> (StatusCode, Json<Value>) {
    let envelope = ProtocolErrorEnvelope::new(code, message, OPERATOR_REQUEST_ID, details);
    (
        status,
        Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
}

fn internal_error() -> (StatusCode, Json<Value>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        StableErrorCode::InternalError,
        "Could not resolve decision",
        json!({}),
    )
}

/// Header carrying the operator's `TACK_EXECUTION_DECISION_TOKEN` on a
/// decision-resolution request. Deliberately not
/// `Authorization` (already spoken for by the ordinary `TACK_API_TOKEN`
/// Bearer gate — this is a second, independent credential, not a
/// replacement for it) and deliberately a header, not a request-body field,
/// so it never ends up echoed into a JSON log line the way a body field might.
pub const DECISION_TOKEN_HEADER: &str = "x-tack-decision-token";

/// Resolving a decision releases whatever the harness/runner is blocked on
/// — a materially higher-privilege action than the ordinary operator
/// `x-tack-principal` gate covers. The safe default: an unconfigured
/// `TACK_EXECUTION_DECISION_TOKEN` always rejects, never "anyone holding
/// the ordinary API token can."
///
/// The error details carry `required_scope: "operator:decisions"`,
/// matching `docs/contracts/runner-v1/errors/forbidden.json`'s frozen
/// example — the real, separately-scoped credential that fixture's wording
/// calls for (see this file's module doc comment).
fn require_decision_token(
    state: &DecisionOperatorState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(expected) = &state.decision_token else {
        return Err(error(
            StatusCode::FORBIDDEN,
            StableErrorCode::Forbidden,
            "resolving a decision requires TACK_EXECUTION_DECISION_TOKEN to be configured on this server",
            json!({"required_scope": "operator:decisions"}),
        ));
    };
    let provided = headers
        .get(DECISION_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    match provided {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(error(
            StatusCode::FORBIDDEN,
            StableErrorCode::Forbidden,
            &format!("missing or invalid {DECISION_TOKEN_HEADER} header"),
            json!({"required_scope": "operator:decisions"}),
        )),
    }
}

/// Reads the operator principal `inject_operator_principal` (`middleware.rs`)
/// sets on every request once this router is mounted behind `require_token`.
/// Deliberately reads *only* this header — never `Authorization` — which is
/// what makes a runner bearer credential structurally powerless here (see
/// this file's module doc comment).
fn principal(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "An authenticated operator principal is required",
                json!({}),
            )
        })
}

pub fn routes(state: DecisionOperatorState) -> Router {
    Router::new()
        .route(
            "/attempts/{attempt_id}/decisions/{decision_id}/resolve",
            post(resolve_decision),
        )
        .route(
            "/attempts/{attempt_id}/decisions/{decision_id}/viewed",
            post(mark_decision_viewed),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------
// HTTP handler.
// ---------------------------------------------------------------------

/// `POST .../decisions/{decision_id}/resolve` response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResolveDecisionResponse {
    pub protocol_version: u32,
    pub decision_id: String,
    /// Always `"resolved"` on a 200 — an expired/not-found/conflicting
    /// decision is a distinct error response, never a 200 with a different
    /// state string, so a caller need not switch on this field.
    #[schema(example = "resolved")]
    pub state: String,
    /// The answer as submitted (or as stored, on a replay), echoed back as
    /// the operator sent it, extra keys included.
    #[schema(value_type = crate::openapi::DecisionAnswerSchema)]
    pub answer: Value,
    pub resolved_at: String,
    /// Who resolved the decision.
    #[schema(value_type = DecisionResolvedBy)]
    pub resolved_by: Value,
    /// `true` when this response is a byte-identical idempotent replay of an
    /// already-committed resolution rather than a fresh write.
    pub replayed: bool,
}

fn validate_answer(value: &Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let answer = value.get("answer").cloned().ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer is required",
            json!({"field": "answer"}),
        )
    })?;
    if !answer.is_object() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer must be an object",
            json!({"field": "answer"}),
        ));
    }
    let option_id_ok = answer
        .get("option_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    if !option_id_ok {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer.option_id is required and must be a non-empty string",
            json!({"field": "answer.option_id"}),
        ));
    }
    if let Some(text) = answer.get("text")
        && !text.is_null()
        && !text.is_string()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer.text must be a string or null",
            json!({"field": "answer.text"}),
        ));
    }
    let answer_bytes = serde_json::to_vec(&answer).unwrap_or_default().len() as u64;
    if answer_bytes > DECISION_ANSWER_BYTES_MAX {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            StableErrorCode::PayloadTooLarge,
            "The answer exceeds a protocol limit",
            json!({"limit": "decision_answer_bytes_max", "maximum": DECISION_ANSWER_BYTES_MAX}),
        ));
    }
    Ok(answer)
}

/// `POST /attempts/{attempt_id}/decisions/{decision_id}/viewed` — records
/// the first time an operator opened a decision. Idempotent: the first
/// timestamp wins. Needs a principal but no decision token: it resolves
/// nothing.
pub async fn mark_decision_viewed(
    State(state): State<DecisionOperatorState>,
    headers: HeaderMap,
    Path((attempt_id, decision_id)): Path<(AttemptId, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    principal(&headers)?;
    let viewed_at = runs::mark_decision_viewed::mark_decision_viewed(
        &state.repo,
        state.clock.now(),
        &attempt_id,
        &decision_id,
    )
    .await
    .map_err(|failure| match failure {
        AppError::NotFound => error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "The requested resource does not exist",
            json!({"resource": "decision"}),
        ),
        _ => internal_error(),
    })?;
    Ok(Json(json!({"viewed_at": viewed_at})))
}

/// `POST /attempts/{attempt_id}/decisions/{decision_id}/resolve` —
/// operator-only (see this file's module doc comment). Never reachable by a
/// runner bearer credential; never resolves an item's status directly.
pub async fn resolve_decision(
    State(state): State<DecisionOperatorState>,
    headers: HeaderMap,
    Path((attempt_id, decision_id)): Path<(AttemptId, String)>,
    body: Bytes,
) -> Result<Json<ResolveDecisionResponse>, (StatusCode, Json<Value>)> {
    require_decision_token(&state, &headers)?;
    let principal_id = principal(&headers)?;

    let value: Value = serde_json::from_slice(&body).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "Request body must be valid JSON",
            json!({"field": "body"}),
        )
    })?;
    let answer = validate_answer(&value)?;

    let resolved_by = DecisionResolvedBy::operator(principal_id);
    let outcome = runs::resolve_decision::resolve_decision(
        &state.repo,
        state.clock.now(),
        &attempt_id,
        &decision_id,
        &answer,
        &resolved_by,
    )
    .await
    .map_err(|err: AppError| {
        tracing::error!(
            attempt_id = %attempt_id,
            decision_id = %decision_id,
            error = %err,
            "decision resolve query failed"
        );
        internal_error()
    })?;

    match outcome {
        ResolveOutcome::NotFound => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "not_found",
                "decision resolve rejected"
            );
            Err(error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "The requested resource does not exist",
                json!({"resource": "decision"}),
            ))
        }
        ResolveOutcome::InvalidOption => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "invalid_option",
                "decision resolve rejected"
            );
            Err(error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "answer.option_id is not one of this decision's options",
                json!({"field": "answer.option_id"}),
            ))
        }
        ResolveOutcome::Expired => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "expired",
                "decision resolve rejected: fail-closed expiry"
            );
            Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::DecisionExpired,
                "The decision expired without a valid answer",
                json!({"decision_id": decision_id}),
            ))
        }
        ResolveOutcome::IdempotencyConflict => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "idempotency_conflict",
                "decision resolve rejected: conflicting replay"
            );
            Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::IdempotencyConflict,
                "The decision was already resolved with a different answer",
                json!({"decision_id": decision_id}),
            ))
        }
        ResolveOutcome::Resolved {
            resolved_at,
            resolved_by,
            answer,
            replayed,
        } => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "resolved",
                replayed,
                "decision resolved"
            );
            Ok(Json(ResolveDecisionResponse {
                protocol_version: 1,
                decision_id,
                state: "resolved".to_string(),
                answer,
                resolved_at,
                resolved_by,
                replayed,
            }))
        }
        ResolveOutcome::UnknownState(raw_state) => {
            tracing::error!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                state = %raw_state,
                "decision resolve: unrecognized stored state"
            );
            Err(internal_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_answer_limit_matches_frozen_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/limits.json"
        ))
        .expect("fixture parses");
        assert_eq!(
            fixture["decision_answer_bytes_max"].as_u64().unwrap(),
            DECISION_ANSWER_BYTES_MAX
        );
    }

    /// The attempt moves this module drives through
    /// `resume_attempt_after_decisions` (and its counterpart in
    /// `create_execution_decision`) are the lease owner's own edges of the
    /// frozen lifecycle, not an invention of the decision code.
    #[test]
    fn decision_state_moves_are_lease_owner_lifecycle_edges() {
        use tack_protocol::execution::{ExecutionState, TransitionActor, validate_transition};
        assert!(
            validate_transition(
                ExecutionState::Running,
                ExecutionState::WaitingDecision,
                TransitionActor::LeaseOwner,
            )
            .is_ok()
        );
        assert!(
            validate_transition(
                ExecutionState::WaitingDecision,
                ExecutionState::Running,
                TransitionActor::LeaseOwner,
            )
            .is_ok()
        );
    }

    #[test]
    fn principal_requires_a_non_empty_header() {
        let mut headers = HeaderMap::new();
        let (status, body) = principal(&headers).unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body.0["error"]["code"], "unauthorized");

        headers.insert("x-tack-principal", "".parse().unwrap());
        let (status, _) = principal(&headers).unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        headers.insert("x-tack-principal", "operator:local".parse().unwrap());
        assert_eq!(principal(&headers).unwrap(), "operator:local");
    }

    #[test]
    fn validate_answer_rejects_missing_empty_option_id_and_bad_text() {
        assert!(validate_answer(&json!({})).is_err());
        assert!(validate_answer(&json!({"answer": "not-an-object"})).is_err());
        assert!(validate_answer(&json!({"answer": {"option_id": ""}})).is_err());
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": 5}})).is_err()
        );
        assert!(validate_answer(&json!({"answer": {"option_id": "allow_once"}})).is_ok());
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": null}})).is_ok()
        );
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": "ok"}})).is_ok()
        );
    }
}
