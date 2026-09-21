use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{Comment, CreateComment};

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/items/{item_id}/comments",
    tag = "comments",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    request_body = tack_core::models::CreateComment,
    responses(
        (status = 200, description = "Comment created", body = tack_core::models::Comment),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_comment(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Json(input): Json<CreateComment>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let comment = state.repo.create_comment(item_id, input).await?;
    maybe_sync_github(&state, item_id, &comment).await;
    Ok(Json(serde_json::to_value(comment).unwrap()))
}

/// Best-effort, fire-and-forget GitHub push: mirror a newly created user
/// comment onto its item's linked GitHub issue, and store the returned id
/// so the inbound poll never mirrors it back in. No-op unless a token
/// resolves (the item's project's own `github_token_ref`, else
/// `TACK_GITHUB_TOKEN` — see `github_sync::github_token_for_project`) and the
/// item has a `github_links` row. Only ever called for a comment created
/// through this handler, so a comment mirrored in from GitHub (created
/// directly via the db repo by `github_sync::poll_once`) is never a
/// candidate here.
async fn maybe_sync_github(state: &AppState, item_id: Uuid, comment: &Comment) {
    let Ok(Some((repo, number))) = state.repo.get_github_link(item_id).await else {
        return;
    };
    let Ok(Some(item)) = state.repo.get_item(item_id).await else {
        return;
    };
    let Some(token) = crate::github_sync::github_token_for_project(state, item.project_id).await
    else {
        return;
    };
    let base = state.config.github_api_base.clone();
    let body = comment.content.clone();
    let comment_id = comment.id;
    let db = state.repo.clone();
    tokio::spawn(async move {
        match crate::github_sync::push_issue_comment(&base, &token, &repo, number, &body).await {
            Ok(github_id) => {
                if let Err(error) = db.set_comment_github_id(comment_id, github_id).await {
                    tracing::warn!(comment_id = %comment_id, %error, "storing GitHub comment id failed");
                }
            }
            Err(error) => {
                tracing::warn!(repo = %repo, issue = number, %error, "GitHub comment push failed");
            }
        }
    });
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{item_id}/comments",
    tag = "comments",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "Comments on the item", body = Vec<tack_core::models::Comment>),
    ),
)]
pub async fn list_comments(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let comments = state.repo.list_comments(item_id).await?;
    Ok(Json(serde_json::to_value(comments).unwrap()))
}
