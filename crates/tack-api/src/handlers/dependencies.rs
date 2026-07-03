use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::CreateDependency;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/items/{item_id}/dependencies",
    tag = "dependencies",
    params(
        ("item_id" = Uuid, Path, description = "Source item ID"),
    ),
    request_body = tack_core::models::CreateDependency,
    responses(
        (status = 200, description = "Dependency created", body = tack_core::models::Dependency),
        (status = 400, description = "Cycle detected or duplicate", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_dependency(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<CreateDependency>,
) -> ApiResult<Json<serde_json::Value>> {
    let dep = state.repo.create_dependency(item_id, input).await?;
    Ok(Json(serde_json::to_value(dep).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{item_id}/dependencies",
    tag = "dependencies",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "Dependency edges for the item", body = Vec<tack_core::models::Dependency>),
    ),
)]
pub async fn list_dependencies(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deps = state.repo.list_dependencies_for_item(item_id).await?;
    Ok(Json(serde_json::to_value(deps).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{item_id}/dependencies/{dep_id}",
    tag = "dependencies",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("dep_id" = Uuid, Path, description = "Dependency ID"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Dependency not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn delete_dependency(
    State(state): State<AppState>,
    Path((_item_id, dep_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_dependency(dep_id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("Dependency {dep_id} not found")));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}
