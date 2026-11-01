use crate::extract::{Json, Path};
use axum::Extension;
use axum::extract::State;
use tack_core::principal::{Capability, Principal, may, missing_scope};
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
    Extension(principal): Extension<Principal>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<CreateDependency>,
) -> ApiResult<Json<serde_json::Value>> {
    // The middleware checked the source item's project; the other end must be one the caller
    // may change too, or a credential could hang its own item on a project it cannot touch.
    if !principal.is_operator()
        && let Some(target) = state.repo.get_item(input.target_item_id).await?
        && !may(&principal, target.project_id, Capability::Act)
    {
        return Err(ApiError::Forbidden(missing_scope(
            target.project_id,
            Capability::Act,
        )));
    }
    let dep = state
        .repo
        .create_dependency_by(item_id, input, Some(&principal.label()))
        .await?;
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
    Path((item_id, dep_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_dependency(item_id, dep_id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("Dependency {dep_id} not found")));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}
