use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{ItemBrief, UpsertItemBrief};

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

async fn require_item(state: &AppState, item_id: Uuid) -> ApiResult<()> {
    state
        .repo
        .get_item(item_id)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} not found")))
}

/// GET /api/items/:item_id/brief
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{item_id}/brief",
    tag = "briefs",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "The item's brief", body = tack_core::models::ItemBrief),
        (status = 404, description = "Item not found, or it has no brief", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_brief(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<ItemBrief>> {
    require_item(&state, item_id).await?;
    let brief = state
        .repo
        .get_item_brief(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} has no brief")))?;
    Ok(Json(brief))
}

/// PUT /api/items/:item_id/brief
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/items/{item_id}/brief",
    tag = "briefs",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    request_body = tack_core::models::UpsertItemBrief,
    responses(
        (status = 200, description = "The brief as stored", body = tack_core::models::ItemBrief),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_brief(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<UpsertItemBrief>,
) -> ApiResult<Json<ItemBrief>> {
    require_item(&state, item_id).await?;
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(state.repo.upsert_item_brief(item_id, input).await?))
}

/// DELETE /api/items/:item_id/brief
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{item_id}/brief",
    tag = "briefs",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 204, description = "Brief deleted"),
        (status = 404, description = "Item not found, or it has no brief", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn delete_brief(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_item(&state, item_id).await?;
    if state.repo.delete_item_brief(item_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Item {item_id} has no brief")))
    }
}
