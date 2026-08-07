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
use uuid::Uuid;

use super::executions::OperatorExecutionState;

type HandlerResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

/// C5 replaces this non-secret sentinel with the request correlation ID once it
/// mounts these card-local routes in the global API router.
const OPERATOR_REQUEST_ID: &str = "req_operator";

fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(
            json!({"error":{"code":code,"message":message,"request_id":OPERATOR_REQUEST_ID,"retryable":false,"details":{}}}),
        ),
    )
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
        Err(_) => Err(error(
            StatusCode::CONFLICT,
            "conflict",
            "Fleet name already exists",
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
            "internal_error",
            "Could not list fleets",
        )
    })?;
    let data: Vec<Value> = rows.into_iter().map(|r| -> Result<Value, (StatusCode, Json<Value>)> {
        let policy = serde_json::from_str::<Value>(&r.get::<String,_>("default_policy")).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Fleet policy is corrupt"))?;
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
                "internal_error",
                "Could not revoke runner",
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Runner does not exist",
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
            "invalid_request",
            "enrollment_lifetime_seconds must be positive",
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
                "invalid_request",
                "enrollment_lifetime_seconds is out of range",
            )
        })?;
    let expires_at = now.checked_add_signed(enrollment_lifetime).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "enrollment_lifetime_seconds is out of range",
        )
    })?;
    let labels = serde_json::to_string(&input.labels).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "labels cannot be serialized",
        )
    })?;
    let capabilities = serde_json::to_string(&input.capability_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "capability_snapshot cannot be serialized",
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
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "pending runner enrollment is invalid",
            )
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
                "internal_error",
                "Could not revoke enrollment token",
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "Enrollment token is missing, consumed, or already revoked",
        ));
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
            "invalid_request",
            "tool_policy cannot be serialized",
        )
    })?;
    let limits = serde_json::to_string(&input.limits).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "limits cannot be serialized",
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
        Err(_) => Err(error(
            StatusCode::CONFLICT,
            "conflict",
            "Agent profile name already exists",
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
            "internal_error",
            "Could not list agent profiles",
        )
    })?;
    let data: Vec<Value> = rows.into_iter().map(|r| -> Result<Value, (StatusCode, Json<Value>)> {
        let policy = serde_json::from_str::<Value>(&r.get::<String,_>("tool_policy")).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Agent profile policy is corrupt"))?;
        let limits = serde_json::from_str::<Value>(&r.get::<String,_>("limits")).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Agent profile limits are corrupt"))?;
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
        Err(_) => Err(error(
            StatusCode::CONFLICT,
            "conflict",
            "Model profile name already exists",
        )),
    }
}
pub async fn list_model_profiles(State(state): State<OperatorExecutionState>) -> HandlerResult {
    let rows=sqlx::query("SELECT id,name,model_provider,model_id,config_reference,enabled FROM model_profiles ORDER BY name").fetch_all(state.repo.pool()).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"internal_error","Could not list model profiles"))?;
    Ok(Json(
        json!({"protocol_version":1,"data":rows.into_iter().map(|r| json!({"model_profile_id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"model_provider":r.get::<String,_>("model_provider"),"model_id":r.get::<String,_>("model_id"),"config_reference":r.get::<Option<String>,_>("config_reference"),"enabled":r.get::<i64,_>("enabled") != 0})).collect::<Vec<_>>() }),
    ))
}
