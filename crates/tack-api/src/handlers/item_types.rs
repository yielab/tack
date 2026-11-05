//! A project's item types: the catalog Settings → Types edits. The rules (a level changes only
//! while no item uses the type, one default work type, a used type is merged before it goes) are
//! `tack_core::item_types`'s; the store applies them in one transaction.

use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use tack_core::item_types::{CreateItemType, ItemTypeDef, ItemTypeUse, UpdateItemType};
use tack_db::queries;
use tack_db::repo::item_types::ItemTypeError;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::router::AppState;

impl From<ItemTypeError> for ApiError {
    fn from(error: ItemTypeError) -> Self {
        match error {
            ItemTypeError::Store(failure) => ApiError::Store(failure),
            ItemTypeError::Refused(rule) => ApiError::Core(rule),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteTypeQuery {
    /// The type the items of the deleted one move to; required while items use it.
    pub merge_into: Option<String>,
}

/// The type `key` as the catalog holds it now.
fn named(types: Vec<ItemTypeDef>, key: &str) -> ApiResult<ItemTypeDef> {
    types
        .into_iter()
        .find(|t| t.key == key)
        .ok_or_else(|| ApiError::NotFound(format!("Item type {key} not found")))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/types",
    tag = "item types",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project's types in display order, each with how many items use it", body = Vec<ItemTypeUse>),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn list_item_types(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<Vec<ItemTypeUse>>> {
    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    Ok(Json(
        queries::item_types::in_use(&state.repo, project_id).await?,
    ))
}

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/types",
    tag = "item types",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = CreateItemType,
    responses(
        (status = 200, description = "The type, last in order and never the default", body = ItemTypeDef),
        (status = 400, description = "The key or name is not valid, or the key is used", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_item_type(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateItemType>,
) -> ApiResult<Json<ItemTypeDef>> {
    let types = state.repo.create_item_type(project_id, &input).await?;
    Ok(Json(named(types, &input.key)?))
}

#[instrument(skip(state))]
#[utoipa::path(
    patch,
    path = "/api/projects/{id}/types/{key}",
    tag = "item types",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("key" = String, Path, description = "The type's key"),
    ),
    request_body = UpdateItemType,
    responses(
        (status = 200, description = "The type after the change", body = ItemTypeDef),
        (status = 400, description = "The change breaks a rule of the catalog", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project or type not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "The level cannot change while items use the type (type_in_use)", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_item_type(
    State(state): State<AppState>,
    Path((project_id, key)): Path<(Uuid, String)>,
    Json(input): Json<UpdateItemType>,
) -> ApiResult<Json<ItemTypeDef>> {
    let types = state
        .repo
        .update_item_type(project_id, &key, &input)
        .await?;
    Ok(Json(named(types, &key)?))
}

#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/projects/{id}/types/{key}",
    tag = "item types",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("key" = String, Path, description = "The type's key"),
        DeleteTypeQuery,
    ),
    responses(
        (status = 204, description = "The type is deleted and its items moved to merge_into"),
        (status = 400, description = "The default work type, or a merge into another level, cannot be done", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project or type not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Items use the type and merge_into is not given (type_in_use)", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn delete_item_type(
    State(state): State<AppState>,
    Path((project_id, key)): Path<(Uuid, String)>,
    Query(query): Query<DeleteTypeQuery>,
) -> ApiResult<StatusCode> {
    state
        .repo
        .delete_item_type(project_id, &key, query.merge_into.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
