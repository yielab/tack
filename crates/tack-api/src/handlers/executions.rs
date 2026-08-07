//! Card-local operator execution handlers. C5 owns their global-router wiring.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_db::{
    Repository,
    repo::execution::{EnqueueResult, ExecutionClock, NewExecutionRequest, OperatorRequeueResult},
};
use tack_orch::execution::ExecutionRequestSnapshot;
use uuid::Uuid;

/// State for C1's card-local router. C5 can construct this from the shared API
/// state when it performs the one permitted global-router integration.
#[derive(Clone)]
pub struct OperatorExecutionState {
    pub repo: Repository,
    pub clock: Arc<dyn ExecutionClock>,
}

impl OperatorExecutionState {
    pub fn with_clock(repo: Repository, clock: Arc<dyn ExecutionClock>) -> Self {
        Self { repo, clock }
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

#[derive(Clone)]
struct FixedExecutionClock(DateTime<Utc>);

impl ExecutionClock for FixedExecutionClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn principal(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "An authenticated operator principal is required",
            )
        })
}

fn canonical_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect(),
        ),
        value => value,
    }
}

fn canonical_string(value: Value) -> Result<String, (StatusCode, Json<Value>)> {
    serde_json::to_string(&canonical_json(value)).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Request cannot be canonicalized",
        )
    })
}

fn stable_request_id(scope: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(scope.as_bytes());
    hasher.update([0]);
    hasher.update(key.as_bytes());
    format!("exec_{}", hex::encode(hasher.finalize()))
}

fn fingerprint(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateExecution {
    pub item_id: Uuid,
    pub idempotency_key: String,
    pub selector_kind: String,
    pub selector_id: String,
    pub agent_profile_id: String,
    pub requested_harness_kind: String,
    #[serde(default)]
    pub requested_model_provider: Option<String>,
    #[serde(default)]
    pub requested_model_id: Option<String>,
    pub agent_profile_snapshot: Value,
    pub repository_snapshot: Value,
    pub permission_policy: Value,
    pub budgets: Value,
    pub environment: Value,
    pub metadata: Value,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub status_map_policy_id: Option<String>,
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
    headers: HeaderMap,
    Json(input): Json<CreateExecution>,
) -> HandlerResult {
    let authenticated_principal = principal(&headers)?;
    let idempotency_scope = format!("operator:{authenticated_principal}");
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
    let existing_snapshot: Option<String> = sqlx::query_scalar(
        "SELECT request_snapshot FROM execution_requests WHERE idempotency_scope=? AND idempotency_key=?",
    )
    .bind(&idempotency_scope)
    .bind(&input.idempotency_key)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Could not load idempotency replay"))?;
    // An exact retry must be allowed to reach B2's durable replay record even
    // if a mutable runner status changed after the original create.
    if existing_snapshot.is_none() && input.selector_kind == "exact_runner" {
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
    let (request_id, created_at) = match existing_snapshot {
        Some(snapshot) => {
            let value: Value = serde_json::from_str(&snapshot).map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "idempotency_conflict",
                    "Stored request snapshot is invalid",
                )
            })?;
            let object = value.as_object().ok_or_else(|| {
                error(
                    StatusCode::CONFLICT,
                    "idempotency_conflict",
                    "Stored request snapshot is invalid",
                )
            })?;
            let request_id = object
                .get("request_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    error(
                        StatusCode::CONFLICT,
                        "idempotency_conflict",
                        "Stored request snapshot is invalid",
                    )
                })?;
            let created_at = object
                .get("created_at")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    error(
                        StatusCode::CONFLICT,
                        "idempotency_conflict",
                        "Stored request snapshot is invalid",
                    )
                })?;
            let created_at = DateTime::parse_from_rfc3339(created_at)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|_| {
                    error(
                        StatusCode::CONFLICT,
                        "idempotency_conflict",
                        "Stored request snapshot is invalid",
                    )
                })?;
            (request_id.to_owned(), created_at)
        }
        None => (
            stable_request_id(&idempotency_scope, &input.idempotency_key),
            state.clock.now(),
        ),
    };
    let item_id = input.item_id.to_string();
    let selector = match input.selector_kind.as_str() {
        "exact_runner" => json!({"kind":"exact_runner","runner_id":input.selector_id}),
        "fleet" => json!({"kind":"fleet","fleet_id":input.selector_id}),
        _ => unreachable!("selector kind was validated"),
    };
    let snapshot_value = json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": input.idempotency_key,
        "created_by": {"source":"operator_api","subject_id":authenticated_principal},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": input.agent_profile_id,
        "resolved_agent_profile": input.agent_profile_snapshot,
        "requested_harness_kind": input.requested_harness_kind,
        "requested_model_provider": input.requested_model_provider,
        "requested_model_id": input.requested_model_id,
        "repository": input.repository_snapshot,
        "permission_policy": input.permission_policy,
        "timeout_seconds": input.timeout_seconds,
        "budgets": input.budgets,
        "status_map_policy_id": input.status_map_policy_id,
        "environment": input.environment,
        "metadata": input.metadata,
    });
    let typed_snapshot: ExecutionRequestSnapshot =
        serde_json::from_value(snapshot_value).map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Execution request snapshot is incomplete or invalid",
            )
        })?;
    let snapshot_value = serde_json::to_value(typed_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Execution request snapshot is invalid",
        )
    })?;
    let request_snapshot = canonical_string(snapshot_value.clone())?;
    let request_fingerprint = fingerprint(&request_snapshot);
    let root = snapshot_value.as_object().ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Execution request snapshot is invalid",
        )
    })?;
    let serialized_field = |name: &str| canonical_string(root[name].clone());
    let agent_profile_snapshot = serialized_field("resolved_agent_profile")?;
    let repository_snapshot = serialized_field("repository")?;
    let permission_policy = serialized_field("permission_policy")?;
    let budgets = serialized_field("budgets")?;
    let environment = serialized_field("environment")?;
    let metadata = serialized_field("metadata")?;
    let timeout_seconds = i64::try_from(input.timeout_seconds).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "timeout_seconds is out of range",
        )
    })?;
    let request_clock = FixedExecutionClock(created_at);
    let result = state
        .repo
        .enqueue_execution(
            NewExecutionRequest {
                id: &request_id,
                item_id: &item_id,
                idempotency_scope: &idempotency_scope,
                idempotency_key: &input.idempotency_key,
                request_fingerprint: &request_fingerprint,
                selector_kind: &input.selector_kind,
                selector_id: &input.selector_id,
                agent_profile_id: Some(&input.agent_profile_id),
                agent_profile_snapshot: &agent_profile_snapshot,
                requested_harness_kind: Some(&input.requested_harness_kind),
                requested_model_provider: input.requested_model_provider.as_deref(),
                requested_model_id: input.requested_model_id.as_deref(),
                repository_snapshot: &repository_snapshot,
                permission_policy: &permission_policy,
                timeout_seconds: Some(timeout_seconds),
                budgets: &budgets,
                status_map_policy_id: input.status_map_policy_id.as_deref(),
                environment: &environment,
                metadata: &metadata,
                request_snapshot: &request_snapshot,
            },
            &request_clock,
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
    pub recovery_key: String,
    pub reason: String,
}
pub async fn requeue_needs_operator(
    State(state): State<OperatorExecutionState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    Json(input): Json<RecoveryConfirmation>,
) -> HandlerResult {
    let actor = principal(&headers)?;
    let reason_fingerprint = fingerprint(&input.reason);
    let result = state
        .repo
        .operator_requeue_needs_operator(
            &request_id,
            &input.recovery_key,
            &actor,
            &reason_fingerprint,
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Could not requeue execution",
            )
        })?;
    match result {
        OperatorRequeueResult::Requeued | OperatorRequeueResult::Replayed => Ok(Json(
            json!({"protocol_version":1,"request_id":request_id,"state":"queued","recovered_from":"needs_operator","replayed":matches!(result, OperatorRequeueResult::Replayed)}),
        )),
        OperatorRequeueResult::Conflict => Err(error(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "The recovery key was used with a different confirmation",
        )),
        OperatorRequeueResult::InvalidTransition | OperatorRequeueResult::NotFound => Err(error(
            StatusCode::CONFLICT,
            "invalid_transition",
            "Only authoritatively recovered needs_operator attempts may be requeued",
        )),
    }
}
