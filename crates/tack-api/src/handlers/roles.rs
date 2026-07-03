use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::CreateRole;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/roles",
    tag = "roles",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    request_body = tack_core::models::CreateRole,
    responses(
        (status = 200, description = "Role created", body = tack_core::models::Role),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
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
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/roles",
    tag = "roles",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Roles for the project", body = Vec<tack_core::models::Role>),
    ),
)]
pub async fn list_roles(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let roles = state.repo.list_roles(project_id).await?;
    Ok(Json(serde_json::to_value(roles).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/items/{item_id}/roles/{role_id}",
    tag = "roles",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("role_id" = Uuid, Path, description = "Role ID"),
    ),
    responses(
        (status = 200, description = "Role assigned to item", body = serde_json::Value),
    ),
)]
pub async fn assign_role(
    State(state): State<AppState>,
    Path((item_id, role_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    state.repo.assign_role_to_item(item_id, role_id).await?;
    Ok(Json(serde_json::json!({"assigned": true})))
}

#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{item_id}/roles/{role_id}",
    tag = "roles",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("role_id" = Uuid, Path, description = "Role ID"),
    ),
    responses(
        (status = 200, description = "Role removed from item", body = serde_json::Value),
    ),
)]
pub async fn remove_role(
    State(state): State<AppState>,
    Path((item_id, role_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    state.repo.remove_role_from_item(item_id, role_id).await?;
    Ok(Json(serde_json::json!({"removed": true})))
}

#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/roles/{id}",
    tag = "roles",
    params(
        ("id" = Uuid, Path, description = "Role ID"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Role not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
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
