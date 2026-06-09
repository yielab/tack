use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;

use flexpm_core::models::CreateDependency;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
pub async fn create_dependency(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<CreateDependency>,
) -> ApiResult<Json<serde_json::Value>> {
    let dep = state.repo.create_dependency(item_id, input).await?;
    Ok(Json(serde_json::to_value(dep).unwrap()))
}

#[instrument(skip(state))]
pub async fn list_dependencies(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deps = state.repo.list_dependencies_for_item(item_id).await?;
    Ok(Json(serde_json::to_value(deps).unwrap()))
}

#[instrument(skip(state))]
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
