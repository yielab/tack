//! Card-local operator fleet, runner, and profile handlers. C5 owns wiring.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use chrono::Duration;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_db::repo::execution::{EnrollmentToken, NewAgentProfile, NewRunner};
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};
use uuid::Uuid;

use super::executions::OperatorExecutionState;

type HandlerResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

/// C5 replaces this non-secret sentinel with the request correlation ID once it
/// mounts these card-local routes in the global API router.
const OPERATOR_REQUEST_ID: &str = "req_operator";

/// Builds the stable v1 error envelope via B1's `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` (`StableErrorCode::retryable`) so it
/// can never drift from `docs/contracts/runner-v1/errors/*.json`. `details`
/// must follow the per-code shape documented in
/// `docs/contracts/runner-v1/README.md`.
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

/// Whether a `sqlx::Error` from an INSERT is a unique-constraint violation on
/// a caller-chosen name, as opposed to any other database-level fault. Used
/// so name collisions map to `conflict` and everything else maps to
/// `internal_error`, instead of collapsing every insert failure into one code.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|db_error| db_error.is_unique_violation())
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/runner-fleets", post(create_fleet).get(list_fleets))
        .route("/runners/enrollment", post(create_pending_runner))
        .route(
            "/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
            post(revoke_enrollment_token),
        )
        .route("/runners/{runner_id}/revoke", post(revoke_runner))
        .route("/agent-profiles", post(create_profile).get(list_profiles))
        .route(
            "/model-profiles",
            post(create_model_profile).get(list_model_profiles),
        )
        .with_state(state)
}

#[derive(Deserialize)]
pub struct CreateFleet {
    pub name: String,
    #[serde(default)]
    pub concurrency_limit: Option<i64>,
    #[serde(default = "empty")]
    pub default_policy: Value,
}
#[derive(Deserialize)]
pub struct CreateProfile {
    pub name: String,
    pub instructions: String,
    #[serde(default = "empty")]
    pub tool_policy: Value,
    #[serde(default = "empty")]
    pub limits: Value,
}
#[derive(Deserialize)]
pub struct CreateModelProfile {
    pub name: String,
    pub model_provider: String,
    pub model_id: String,
    #[serde(default)]
    pub config_reference: Option<String>,
}
fn empty() -> Value {
    json!({})
}

#[derive(Deserialize)]
pub struct CreatePendingRunner {
    pub name: String,
    #[serde(default = "empty")]
    pub labels: Value,
    pub total_capacity: i64,
    pub available_capacity: i64,
    #[serde(default = "empty")]
    pub capability_snapshot: Value,
    #[serde(default = "protocol_v1")]
    pub protocol_version: i64,
    #[serde(default = "default_enrollment_lifetime_seconds")]
    pub enrollment_lifetime_seconds: i64,
}

fn protocol_v1() -> i64 {
    1
}

fn default_enrollment_lifetime_seconds() -> i64 {
    60 * 60
}

fn token_hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

pub async fn create_fleet(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateFleet>,
) -> HandlerResult {
    let id = format!("fleet_{}", Uuid::new_v4());
    let now = state.clock.now().to_rfc3339();
    let inserted = sqlx::query("INSERT INTO agent_fleets (id,name,concurrency_limit,default_policy,created_at,updated_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(&input.name).bind(input.concurrency_limit).bind(input.default_policy.to_string()).bind(&now).bind(&now).execute(state.repo.pool()).await;
    match inserted {
        Ok(_) => Ok(Json(
            json!({"protocol_version":1,"fleet_id":id,"name":input.name}),
        )),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Fleet name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create fleet",
            json!({}),
        )),
    }
}
pub async fn list_fleets(State(state): State<OperatorExecutionState>) -> HandlerResult {
    let rows = sqlx::query(
        "SELECT id,name,concurrency_limit,default_policy FROM agent_fleets ORDER BY name",
    )
    .fetch_all(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list fleets",
            json!({}),
        )
    })?;
    let data: Vec<Value> = rows.into_iter().map(|r| -> Result<Value, (StatusCode, Json<Value>)> {
        let policy = serde_json::from_str::<Value>(&r.get::<String,_>("default_policy")).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Fleet policy is corrupt",
                json!({}),
            )
        })?;
        Ok(json!({"fleet_id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"concurrency_limit":r.get::<Option<i64>,_>("concurrency_limit"),"default_policy":policy}))
    }).collect::<Result<_, _>>()?;
    Ok(Json(json!({"protocol_version":1,"data":data})))
}
pub async fn revoke_runner(
    State(state): State<OperatorExecutionState>,
    Path(runner_id): Path<String>,
) -> HandlerResult {
    let changed = state
        .repo
        .revoke_runner(&runner_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not revoke runner",
                json!({}),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner does not exist",
            json!({"resource": "runner"}),
        ));
    }
    Ok(Json(
        json!({"protocol_version":1,"runner_id":runner_id,"state":"revoked"}),
    ))
}

/// Creates a pending runner and stores only a SHA-256 enrollment-token hash.
/// The raw token is deliberately emitted once here and is never readable from
/// metadata, list, revocation, or runner responses.
pub async fn create_pending_runner(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreatePendingRunner>,
) -> HandlerResult {
    if input.enrollment_lifetime_seconds <= 0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "enrollment_lifetime_seconds must be positive",
            json!({"field": "enrollment_lifetime_seconds"}),
        ));
    }
    let runner_id = format!("runr_{}", Uuid::new_v4());
    let token_id = format!("ent_{}", Uuid::new_v4());
    let raw_token = format!("enr_{}", Uuid::new_v4());
    let now = state.clock.now();
    let enrollment_lifetime =
        Duration::try_seconds(input.enrollment_lifetime_seconds).ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "enrollment_lifetime_seconds is out of range",
                json!({"field": "enrollment_lifetime_seconds"}),
            )
        })?;
    let expires_at = now.checked_add_signed(enrollment_lifetime).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "enrollment_lifetime_seconds is out of range",
            json!({"field": "enrollment_lifetime_seconds"}),
        )
    })?;
    let labels = serde_json::to_string(&input.labels).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "labels cannot be serialized",
            json!({"field": "labels"}),
        )
    })?;
    let capabilities = serde_json::to_string(&input.capability_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "capability_snapshot cannot be serialized",
            json!({"field": "capability_snapshot"}),
        )
    })?;
    let enrollment_token_hash = token_hash(&raw_token);
    state
        .repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: &runner_id,
                name: &input.name,
                credential_hash: "pending:no-credential",
                labels: &labels,
                total_capacity: input.total_capacity,
                available_capacity: input.available_capacity,
                capability_snapshot: &capabilities,
                protocol_version: input.protocol_version,
            },
            EnrollmentToken {
                id: &token_id,
                runner_id: &runner_id,
                token_hash: &enrollment_token_hash,
                expires_at,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|err| match err {
            // The repo layer reports out-of-bounds capacity/expiry via
            // `sqlx::Error::Protocol` — a genuine client input problem.
            sqlx::Error::Protocol(_) => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "Pending runner capacity or enrollment window is invalid",
                json!({"field": "total_capacity"}),
            ),
            _ if is_unique_violation(&err) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Runner name already exists",
                json!({}),
            ),
            _ => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not create pending runner",
                json!({}),
            ),
        })?;
    Ok(Json(json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "token_id": token_id,
        "enrollment_token": raw_token,
        "expires_at": expires_at.to_rfc3339(),
    })))
}

pub async fn revoke_enrollment_token(
    State(state): State<OperatorExecutionState>,
    Path((runner_id, token_id)): Path<(String, String)>,
) -> HandlerResult {
    let changed = state
        .repo
        .revoke_enrollment_token_by_id(&runner_id, &token_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not revoke enrollment token",
                json!({}),
            )
        })?;
    if !changed {
        // `revoke_enrollment_token_by_id` also returns false for a token
        // that exists but was already consumed; disambiguate that genuine
        // conflict from a token id that never existed instead of reporting
        // `not_found` for both.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_enrollment_tokens WHERE runner_id = ? AND id = ?)",
        )
        .bind(&runner_id)
        .bind(&token_id)
        .fetch_one(state.repo.pool())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not verify enrollment token",
                json!({}),
            )
        })?;
        return Err(if exists {
            error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Enrollment token was already consumed",
                json!({}),
            )
        } else {
            error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Enrollment token does not exist",
                json!({"resource": "enrollment_token"}),
            )
        });
    }
    Ok(Json(
        json!({"protocol_version":1,"runner_id":runner_id,"token_id":token_id,"state":"revoked"}),
    ))
}
pub async fn create_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateProfile>,
) -> HandlerResult {
    let id = format!("ap_{}", Uuid::new_v4());
    let tool_policy = serde_json::to_string(&input.tool_policy).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "tool_policy cannot be serialized",
            json!({"field": "tool_policy"}),
        )
    })?;
    let limits = serde_json::to_string(&input.limits).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "limits cannot be serialized",
            json!({"field": "limits"}),
        )
    })?;
    let result = state
        .repo
        .create_agent_profile(
            NewAgentProfile {
                id: &id,
                name: &input.name,
                instructions: &input.instructions,
                tool_policy: &tool_policy,
                limits: &limits,
            },
            state.clock.as_ref(),
        )
        .await;
    match result {
        Ok(_) => Ok(Json(
            json!({"protocol_version":1,"agent_profile_id":id,"name":input.name}),
        )),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Agent profile name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create agent profile",
            json!({}),
        )),
    }
}
pub async fn list_profiles(State(state): State<OperatorExecutionState>) -> HandlerResult {
    let rows = sqlx::query(
        "SELECT id,name,instructions,tool_policy,limits FROM agent_profiles ORDER BY name",
    )
    .fetch_all(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list agent profiles",
            json!({}),
        )
    })?;
    let data: Vec<Value> = rows.into_iter().map(|r| -> Result<Value, (StatusCode, Json<Value>)> {
        let policy = serde_json::from_str::<Value>(&r.get::<String,_>("tool_policy")).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Agent profile policy is corrupt",
                json!({}),
            )
        })?;
        let limits = serde_json::from_str::<Value>(&r.get::<String,_>("limits")).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Agent profile limits are corrupt",
                json!({}),
            )
        })?;
        Ok(json!({"agent_profile_id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"instructions":r.get::<String,_>("instructions"),"tool_policy":policy,"limits":limits}))
    }).collect::<Result<_, _>>()?;
    Ok(Json(json!({"protocol_version":1,"data":data})))
}
pub async fn create_model_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateModelProfile>,
) -> HandlerResult {
    let id = format!("mp_{}", Uuid::new_v4());
    let now = state.clock.now().to_rfc3339();
    let result=sqlx::query("INSERT INTO model_profiles (id,name,model_provider,model_id,config_reference,created_at,updated_at) VALUES (?,?,?,?,?,?,?)").bind(&id).bind(&input.name).bind(&input.model_provider).bind(&input.model_id).bind(&input.config_reference).bind(&now).bind(&now).execute(state.repo.pool()).await;
    match result {
        Ok(_) => Ok(Json(
            json!({"protocol_version":1,"model_profile_id":id,"name":input.name,"model_provider":input.model_provider,"model_id":input.model_id}),
        )),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Model profile name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create model profile",
            json!({}),
        )),
    }
}
pub async fn list_model_profiles(State(state): State<OperatorExecutionState>) -> HandlerResult {
    let rows=sqlx::query("SELECT id,name,model_provider,model_id,config_reference,enabled FROM model_profiles ORDER BY name").fetch_all(state.repo.pool()).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list model profiles",
            json!({}),
        )
    })?;
    Ok(Json(
        json!({"protocol_version":1,"data":rows.into_iter().map(|r| json!({"model_profile_id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"model_provider":r.get::<String,_>("model_provider"),"model_id":r.get::<String,_>("model_id"),"config_reference":r.get::<Option<String>,_>("config_reference"),"enabled":r.get::<i64,_>("enabled") != 0})).collect::<Vec<_>>() }),
    ))
}
