use axum::extract::{Path, State};
use axum::Json;
use tracing::instrument;
use uuid::Uuid;

use flexpm_core::models::CreateComment;

use crate::error::ApiResult;
use crate::router::AppState;

#[instrument(skip(state))]
pub async fn create_comment(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<CreateComment>,
) -> ApiResult<Json<serde_json::Value>> {
    let comment = state.repo.create_comment(item_id, input).await?;
    Ok(Json(serde_json::to_value(comment).unwrap()))
}

#[instrument(skip(state))]
pub async fn list_comments(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let comments = state.repo.list_comments(item_id).await?;
    Ok(Json(serde_json::to_value(comments).unwrap()))
}
