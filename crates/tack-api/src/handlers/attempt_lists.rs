//! Operator-facing discovery routes for what one attempt produced: artifact
//! manifests and the decisions raised against it. Both are read-only GETs,
//! scoped by the same operator-facing `(request_id, attempt_number)` pair
//! `list_execution_attempt_events` already uses — reusing
//! `executions::OperatorExecutionState` rather than introducing a third
//! state type for two handlers that need nothing beyond `repo`.
//!
//! Listing a decision is deliberately **not** gated behind
//! `TACK_EXECUTION_DECISION_TOKEN` — that token protects *resolving* a
//! decision (`handlers::decisions::require_decision_token`), a privileged
//! write. Reading what decisions exist is an ordinary operator read, same
//! gate as every other route merged in `router.rs#operator_execution_routes`.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};
use utoipa::ToSchema;

use super::executions::OperatorExecutionState;

/// Static request-correlation id placeholder, matching
/// `executions::OPERATOR_REQUEST_ID`'s own convention — no per-request
/// correlation id is wired into these error envelopes yet.
const OPERATOR_REQUEST_ID: &str = "req_operator_attempt_lists";

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

async fn execution_request_exists(
    state: &OperatorExecutionState,
    request_id: &str,
) -> Result<bool, (StatusCode, Json<Value>)> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_requests WHERE id = ?)")
        .bind(request_id)
        .fetch_one(state.repo.pool())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not verify execution",
                json!({}),
            )
        })
}

// ─── Artifacts ──────────────────────────────────────────────────────────────

/// One `execution_artifacts` manifest row as an operator sees it. Never the
/// raw `content_reference` storage path, which is an internal server-side
/// detail — `content_verified` is the presence/absence of that reference,
/// the same manifest-vs-content-verified distinction
/// `GET .../artifacts/{artifact_id}/content` itself enforces with its own
/// `404`-vs-`409` split.
#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactSummary {
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    pub media_type: Option<String>,
    pub size_bytes: i64,
    pub content_verified: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactListResponse {
    pub protocol_version: u32,
    pub data: Vec<ArtifactSummary>,
}

/// `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts` —
/// every artifact manifested for this attempt, oldest first. Returns `404`
/// naming which resource is missing (`execution_request` vs
/// `execution_attempt`), mirroring `list_execution_attempt_events`'s own
/// not-found split.
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/artifacts",
    tag = "execution-operator",
    params(
        ("request_id" = String, Path, description = "Execution request ID (opaque)"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "Every artifact manifested for this attempt, oldest first (may be empty)", body = ArtifactListResponse),
        (status = 404, description = "not_found (execution_request or execution_attempt)", body = super::executions::RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempt_artifacts(
    State(state): State<OperatorExecutionState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> Result<Json<ArtifactListResponse>, (StatusCode, Json<Value>)> {
    if !execution_request_exists(&state, &request_id).await? {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    }
    let artifacts = state
        .repo
        .list_execution_artifacts_for_attempt_number(&request_id, attempt_number)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list artifacts",
                json!({}),
            )
        })?;
    let Some(artifacts) = artifacts else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Attempt does not exist",
            json!({"resource": "execution_attempt"}),
        ));
    };
    let data = artifacts
        .into_iter()
        .map(|artifact| ArtifactSummary {
            artifact_id: artifact.artifact_id,
            kind: artifact.kind,
            name: artifact.name,
            media_type: artifact.media_type,
            size_bytes: artifact.size_bytes,
            content_verified: artifact.content_reference.is_some(),
            created_at: artifact.created_at,
        })
        .collect();
    Ok(Json(ArtifactListResponse {
        protocol_version: 1,
        data,
    }))
}

pub fn artifact_routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route(
            "/executions/{request_id}/attempts/{attempt_number}/artifacts",
            get(list_execution_attempt_artifacts),
        )
        .with_state(state)
}

// ─── Decisions ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DecisionOptionSummary {
    pub option_id: String,
    pub label: String,
}

/// One `execution_decisions` row as an operator sees it — every column the
/// table carries, options/metadata/answer/resolved_by parsed out of their
/// raw JSON-string storage. `answer`/`resolved_by` stay untyped `Value`:
/// the wire shape they carry is documented by `handlers::decisions`'
/// `ResolveDecisionRequest`/`ResolveDecisionResponseSchema`, which this
/// module does not duplicate.
#[derive(Debug, Serialize, ToSchema)]
pub struct DecisionSummary {
    pub decision_id: String,
    pub attempt_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<DecisionOptionSummary>,
    pub metadata: Value,
    pub expires_at: Option<String>,
    pub state: String,
    pub answer: Option<Value>,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DecisionListResponse {
    pub protocol_version: u32,
    pub data: Vec<DecisionSummary>,
}

/// `GET /api/executions/{request_id}/attempts/{attempt_number}/decisions` —
/// every decision raised against this attempt, oldest first, including
/// `pending`/`resolved`/`expired` states alike. Listing carries no
/// `TACK_EXECUTION_DECISION_TOKEN` gate — see this module's header comment.
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/decisions",
    tag = "execution-operator",
    params(
        ("request_id" = String, Path, description = "Execution request ID (opaque)"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "Every decision raised against this attempt, oldest first (may be empty)", body = DecisionListResponse),
        (status = 404, description = "not_found (execution_request or execution_attempt)", body = super::executions::RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempt_decisions(
    State(state): State<OperatorExecutionState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> Result<Json<DecisionListResponse>, (StatusCode, Json<Value>)> {
    if !execution_request_exists(&state, &request_id).await? {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    }
    let decisions = state
        .repo
        .list_execution_decisions_for_attempt_number(&request_id, attempt_number)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list decisions",
                json!({}),
            )
        })?;
    let Some(decisions) = decisions else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Attempt does not exist",
            json!({"resource": "execution_attempt"}),
        ));
    };
    let data = decisions
        .into_iter()
        .map(|decision| DecisionSummary {
            decision_id: decision.decision_id,
            attempt_id: decision.attempt_id,
            kind: decision.kind,
            prompt: decision.prompt,
            options: serde_json::from_str::<Vec<DecisionOptionSummary>>(&decision.options)
                .unwrap_or_default(),
            metadata: serde_json::from_str::<Value>(&decision.metadata).unwrap_or(Value::Null),
            expires_at: decision.expires_at,
            state: decision.state,
            answer: decision
                .answer
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            resolved_at: decision.resolved_at,
            resolved_by: decision
                .resolved_by
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            created_at: decision.created_at,
            updated_at: decision.updated_at,
        })
        .collect();
    Ok(Json(DecisionListResponse {
        protocol_version: 1,
        data,
    }))
}

pub fn decision_routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route(
            "/executions/{request_id}/attempts/{attempt_number}/decisions",
            get(list_execution_attempt_decisions),
        )
        .with_state(state)
}
