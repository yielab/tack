//! Card-local operator execution handlers. C5 owns their global-router wiring.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tack_db::{
    Repository,
    repo::execution::{EnqueueResult, NewExecutionRequest, SystemExecutionClock},
};
use uuid::Uuid;

/// State for C1's card-local router. C5 can construct this from the shared API
/// state when it performs the one permitted global-router integration.
#[derive(Clone)]
pub struct OperatorExecutionState {
    pub repo: Repository,
    pub clock: Arc<SystemExecutionClock>,
}

impl OperatorExecutionState {
    pub fn new(repo: Repository) -> Self {
        Self {
            repo,
            clock: Arc::new(SystemExecutionClock),
        }
    }
}

type HandlerResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(
            json!({"error":{"code":code,"message":message,"request_id":"operator" ,"retryable":false,"details":{}}}),
        ),
    )
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateExecution {
    pub item_id: Uuid,
    pub idempotency_key: String,
    pub selector_kind: String,
    pub selector_id: String,
    #[serde(default)]
    pub agent_profile_id: Option<String>,
    #[serde(default)]
    pub requested_harness_kind: Option<String>,
    #[serde(default)]
    pub requested_model_provider: Option<String>,
    #[serde(default)]
    pub requested_model_id: Option<String>,
    #[serde(default = "empty_object")]
    pub agent_profile_snapshot: Value,
    #[serde(default = "empty_object")]
    pub repository_snapshot: Value,
    #[serde(default = "empty_object")]
    pub permission_policy: Value,
    #[serde(default = "empty_object")]
    pub budgets: Value,
    #[serde(default = "empty_object")]
    pub environment: Value,
    #[serde(default = "empty_object")]
    pub metadata: Value,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
    #[serde(default)]
    pub status_map_policy_id: Option<String>,
}
fn empty_object() -> Value {
    json!({})
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/executions", post(create_execution).get(list_executions))
        .route("/executions/{request_id}", get(get_execution))
        .route(
            "/executions/{request_id}/cancel",
            post(request_cancellation),
        )
        .route(
            "/executions/{request_id}/requeue",
            post(requeue_needs_operator),
        )
        .with_state(state)
}

pub async fn create_execution(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateExecution>,
) -> HandlerResult {
    if state
        .repo
        .get_item(input.item_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not verify item",
            )
        })?
        .is_none()
    {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Item does not exist",
        ));
    }
    if input.selector_kind == "exact_runner" {
        let eligible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ? AND state = 'active' AND revoked_at IS NULL)")
            .bind(&input.selector_id).fetch_one(state.repo.pool()).await
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not verify runner"))?;
        if !eligible {
            return Err(error(
                StatusCode::CONFLICT,
                "runner_revoked",
                "The selected runner is unavailable or revoked",
            ));
        }
    }
    if !matches!(input.selector_kind.as_str(), "exact_runner" | "fleet") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "selector_kind must be exact_runner or fleet",
        ));
    }
    let fingerprint = serde_json::to_string(&input).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Request cannot be canonicalized",
        )
    })?;
    let request_id = format!("exec_{}", Uuid::new_v4());
    let item_id = input.item_id.to_string();
    let snapshot = serde_json::to_string(&input.agent_profile_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Profile snapshot cannot be serialized",
        )
    })?;
    let repository = serde_json::to_string(&input.repository_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Repository snapshot cannot be serialized",
        )
    })?;
    let policy = serde_json::to_string(&input.permission_policy).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Permission policy cannot be serialized",
        )
    })?;
    let budgets = serde_json::to_string(&input.budgets).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Budgets cannot be serialized",
        )
    })?;
    let environment = serde_json::to_string(&input.environment).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Environment cannot be serialized",
        )
    })?;
    let metadata = serde_json::to_string(&input.metadata).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Metadata cannot be serialized",
        )
    })?;
    let result = state
        .repo
        .enqueue_execution(
            NewExecutionRequest {
                id: &request_id,
                item_id: &item_id,
                idempotency_scope: "operator:item",
                idempotency_key: &input.idempotency_key,
                request_fingerprint: &fingerprint,
                selector_kind: &input.selector_kind,
                selector_id: &input.selector_id,
                agent_profile_id: input.agent_profile_id.as_deref(),
                agent_profile_snapshot: &snapshot,
                requested_harness_kind: input.requested_harness_kind.as_deref(),
                requested_model_provider: input.requested_model_provider.as_deref(),
                requested_model_id: input.requested_model_id.as_deref(),
                repository_snapshot: &repository,
                permission_policy: &policy,
                timeout_seconds: input.timeout_seconds,
                budgets: &budgets,
                status_map_policy_id: input.status_map_policy_id.as_deref(),
                environment: &environment,
                metadata: &metadata,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not enqueue execution",
            )
        })?;
    match result {
        EnqueueResult::Created(id) => Ok(Json(
            json!({"protocol_version":1,"request_id":id,"state":"queued","replayed":false}),
        )),
        EnqueueResult::Replayed(id) => Ok(Json(
            json!({"protocol_version":1,"request_id":id,"state":"queued","replayed":true}),
        )),
        EnqueueResult::Conflict => Err(error(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "The idempotency key was used with a different request",
        )),
    }
}

pub async fn list_executions(State(state): State<OperatorExecutionState>) -> HandlerResult {
    let rows = sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at FROM execution_requests ORDER BY created_at DESC")
        .fetch_all(state.repo.pool()).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not list executions"))?;
    let data: Vec<Value> = rows.into_iter().map(|row| json!({"request_id":row.get::<String,_>("id"),"item_id":row.get::<String,_>("item_id"),"state":row.get::<String,_>("state"),"cancellation_requested_at":row.get::<Option<String>,_>("cancellation_requested_at"),"created_at":row.get::<String,_>("created_at")})).collect();
    Ok(Json(json!({"protocol_version":1,"data":data})))
}

pub async fn get_execution(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> HandlerResult {
    let row = sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at FROM execution_requests WHERE id = ?")
        .bind(&request_id).fetch_optional(state.repo.pool()).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not load execution"))?;
    let Some(row) = row else {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Execution request does not exist",
        ));
    };
    Ok(Json(
        json!({"protocol_version":1,"request_id":row.get::<String,_>("id"),"item_id":row.get::<String,_>("item_id"),"state":row.get::<String,_>("state"),"cancellation_requested_at":row.get::<Option<String>,_>("cancellation_requested_at"),"created_at":row.get::<String,_>("created_at")}),
    ))
}

pub async fn request_cancellation(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> HandlerResult {
    let changed = state
        .repo
        .request_execution_cancellation(&request_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not request cancellation",
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Execution is missing or already terminal",
        ));
    }
    Ok(Json(
        json!({"protocol_version":1,"request_id":request_id,"state":"cancellation_requested"}),
    ))
}

#[derive(Deserialize)]
pub struct RecoveryConfirmation {
    pub reason: String,
}
pub async fn requeue_needs_operator(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
    Json(input): Json<RecoveryConfirmation>,
) -> HandlerResult {
    let mut tx = state.repo.pool().begin().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Could not begin recovery",
        )
    })?;
    let attempt = sqlx::query("SELECT id, runner_id, state FROM execution_attempts WHERE request_id = ? ORDER BY attempt_number DESC LIMIT 1").bind(&request_id).fetch_optional(&mut *tx).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not load attempt"))?;
    let Some(attempt) = attempt else {
        return Err(error(
            StatusCode::CONFLICT,
            "invalid_transition",
            "Only needs_operator attempts may be requeued",
        ));
    };
    if attempt.get::<String, _>("state") != "needs_operator" {
        return Err(error(
            StatusCode::CONFLICT,
            "invalid_transition",
            "Only needs_operator attempts may be requeued",
        ));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE execution_requests SET state = 'queued', cancellation_requested_at = NULL, updated_at = ? WHERE id = ?").bind(&now).bind(&request_id).execute(&mut *tx).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not requeue execution"))?;
    sqlx::query("INSERT INTO execution_events (id, attempt_id, event_id, sequence, source, kind, payload, occurred_at, created_at) VALUES (?, ?, ?, COALESCE((SELECT MAX(sequence) + 1 FROM execution_events WHERE attempt_id = ?), 1), 'operator', 'requeue_confirmed', ?, ?, ?)")
        .bind(Uuid::new_v4().to_string()).bind(attempt.get::<String,_>("id")).bind(Uuid::new_v4().to_string()).bind(attempt.get::<String,_>("id")).bind(json!({"reason":input.reason}).to_string()).bind(&now).bind(&now).execute(&mut *tx).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not audit recovery"))?;
    tx.commit().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Could not commit recovery",
        )
    })?;
    Ok(Json(
        json!({"protocol_version":1,"request_id":request_id,"state":"queued","recovered_from":"needs_operator"}),
    ))
}
