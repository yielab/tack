//! Card-local operator fleet, runner, and profile handlers. C5 owns wiring.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use super::executions::OperatorExecutionState;

type HandlerResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(
            json!({"error":{"code":code,"message":message,"request_id":"operator","retryable":false,"details":{}}}),
        ),
    )
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/runner-fleets", post(create_fleet).get(list_fleets))
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

pub async fn create_fleet(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateFleet>,
) -> HandlerResult {
    let id = format!("fleet_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
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
    let now = Utc::now().to_rfc3339();
    let changed = sqlx::query("UPDATE agent_runners SET state='revoked', revoked_at=COALESCE(revoked_at, ?), updated_at=? WHERE id=?").bind(&now).bind(&now).bind(&runner_id).execute(state.repo.pool()).await.map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"internal_error","Could not revoke runner"))?;
    if changed.rows_affected() == 0 {
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
pub async fn create_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateProfile>,
) -> HandlerResult {
    let id = format!("ap_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let result=sqlx::query("INSERT INTO agent_profiles (id,name,instructions,tool_policy,limits,created_at,updated_at) VALUES (?,?,?,?,?,?,?)").bind(&id).bind(&input.name).bind(&input.instructions).bind(input.tool_policy.to_string()).bind(input.limits.to_string()).bind(&now).bind(&now).execute(state.repo.pool()).await;
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
    let now = Utc::now().to_rfc3339();
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
