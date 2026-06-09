use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use flexpm_core::models::CreateRole;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
pub async fn create_role(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateRole>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let role = state.repo.create_role(project_id, input).await?;
    Ok(Json(serde_json::to_value(role).unwrap()))
}

#[instrument(skip(state))]
pub async fn list_roles(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let roles = state.repo.list_roles(project_id).await?;
    Ok(Json(serde_json::to_value(roles).unwrap()))
}

#[instrument(skip(state))]
pub async fn assign_role(
    State(state): State<AppState>,
    Path((item_id, role_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    state.repo.assign_role_to_item(item_id, role_id).await?;
    Ok(Json(serde_json::json!({"assigned": true})))
}

#[instrument(skip(state))]
pub async fn remove_role(
    State(state): State<AppState>,
    Path((item_id, role_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    state.repo.remove_role_from_item(item_id, role_id).await?;
    Ok(Json(serde_json::json!({"removed": true})))
}

#[instrument(skip(state))]
pub async fn delete_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_role(id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("Role {id} not found")));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}
