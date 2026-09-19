use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateItem, Item, ItemFilter, UpdateItem};
use tack_db::repo::items::AtomicItemUpdateOutcome;

use crate::error::{ApiError, ApiResult};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// A version-derived `ETag`, not a content hash: the counter changes on
/// every write, and no body has to be hashed. Quoted per RFC 7232 so a
/// client only ever needs to echo the string back verbatim via `If-Match`
/// — no quote-stripping or parsing on either side, which is also why the
/// value embeds the id: a client that (incorrectly) sends back an `ETag`
/// fetched for a different item can never accidentally collide with this
/// one's version counter.
fn item_etag(id: Uuid, version: i64) -> String {
    format!("\"{id}-{version}\"")
}

/// A stale `If-Match` never becomes an [`ApiError`] — this
/// hand-builds the exact `{"error": {"status", "message"}}` envelope
/// `ApiError`'s `IntoResponse` impl produces, rather than adding a new
/// variant to `error.rs`. A client that already parses that
/// shape for every other 4xx from this API needs no special case for 412.
fn precondition_failed(message: String) -> Response {
    let body = serde_json::json!({
        "error": {
            "status": StatusCode::PRECONDITION_FAILED.as_u16(),
            "message": message,
        }
    });
    (StatusCode::PRECONDITION_FAILED, Json(body)).into_response()
}

/// Converts a matching `If-Match` into the version guarded by the repository
/// transaction.  It deliberately does *not* claim a version: doing so before
/// status/WIP validation would leave a rejected PATCH with a moved ETag.
fn expected_version_from_if_match(
    id: Uuid,
    headers: &HeaderMap,
    current_version: i64,
) -> Result<Option<i64>, String> {
    let Some(if_match) = headers.get(header::IF_MATCH) else {
        return Ok(None);
    };
    let provided = if_match.to_str().unwrap_or("");
    if provided != item_etag(id, current_version) {
        return Err(format!(
            "item {id} was modified since it was fetched (If-Match did not match); \
             refresh and retry"
        ));
    }
    Ok(Some(current_version))
}

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/items",
    tag = "items",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    request_body = tack_core::models::CreateItem,
    responses(
        (status = 200, description = "Item created", body = tack_core::models::Item),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_item(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateItem>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Get project to find initial status from workflow
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let initial_status = project.workflow.initial_status().map_err(ApiError::Core)?;

    let item = state
        .repo
        .create_item(project_id, &initial_status, input)
        .await?;

    // Broadcast WebSocket event
    websocket::broadcast_event(
        &state,
        BoardEvent::ItemCreated {
            project_id,
            item_id: item.id,
            status: item.status.clone(),
        },
    );

    if let Some(wh) = &state.webhook {
        wh.fire(
            "item.created",
            serde_json::json!({
                "event": "item.created",
                "timestamp": Utc::now().to_rfc3339(),
                "project_id": project_id,
                "item": &item,
            }),
        );
    }

    Ok(Json(serde_json::to_value(item).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/items",
    tag = "items",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
        tack_core::models::ItemFilter,
    ),
    responses(
        (status = 200, description = "Paginated items", body = crate::openapi::PaginatedItems),
    ),
)]
pub async fn list_items(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(filter): Query<ItemFilter>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.list_items(project_id, &filter).await?;
    let total = state.repo.count_items(project_id, &filter).await?;
    Ok(Json(serde_json::json!({
        "data": items,
        "total": total,
        "page": filter.effective_page(),
        "per_page": filter.effective_per_page(),
    })))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{id}",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "Item with roles and dependencies; carries an ETag header for a later conditional PATCH", body = crate::openapi::ItemDetail,
            headers(("ETag" = String, description = "Version-derived entity tag; echo verbatim in If-Match when updating this item"))
        ),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_item(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Response> {
    let snapshot = state
        .repo
        .get_item_snapshot(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

    // Also fetch roles and dependencies for the detail view
    let roles = state.repo.get_roles_for_item(id).await?;
    let deps = state.repo.list_dependencies_for_item(id).await?;

    let mut response = Json(serde_json::json!({
        "item": snapshot.item,
        "roles": roles,
        "dependencies": deps,
    }))
    .into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&item_etag(id, snapshot.version)).expect("etag is ascii"),
    );
    Ok(response)
}

#[instrument(skip(state))]
#[utoipa::path(
    patch,
    path = "/api/items/{id}",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
        ("If-Match" = Option<String>, Header, description = "Optional ETag from GET /api/items/{id}; a stale or malformed value returns 412 and writes nothing"),
    ),
    request_body = tack_core::models::UpdateItem,
    responses(
        (status = 200, description = "Updated item; carries the ETag for the exact returned snapshot", body = tack_core::models::Item,
            headers(("ETag" = String, description = "Version-derived entity tag for the returned item snapshot"))
        ),
        (status = 400, description = "Invalid transition / validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
        (status = 412, description = "If-Match did not match the current item version — nothing was written", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UpdateItem>,
) -> ApiResult<Response> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // This snapshot is the version the browser is allowed to mutate.  The
    // repository carries it into the same transaction as every field/WIP
    // change; no pre-claim can leave a rejected request with a bumped ETag.
    let snapshot = state
        .repo
        .get_item_snapshot(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;
    let expected_version = match expected_version_from_if_match(id, &headers, snapshot.version) {
        Ok(expected_version) => expected_version,
        Err(message) => return Ok(precondition_failed(message)),
    };
    let project = state
        .repo
        .get_project(snapshot.item.project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Project not found".into()))?;

    let (item, version, old_status) = match state
        .repo
        .update_item_atomically(id, input, &project.workflow, expected_version)
        .await?
    {
        AtomicItemUpdateOutcome::Updated {
            item,
            version,
            old_status,
        } => (*item, version, old_status),
        AtomicItemUpdateOutcome::NotFound => {
            return Err(ApiError::NotFound(format!("Item {id} not found")));
        }
        AtomicItemUpdateOutcome::PreconditionFailed => {
            return Ok(precondition_failed(format!(
                "item {id} was modified since it was fetched (If-Match did not match); refresh and retry"
            )));
        }
        AtomicItemUpdateOutcome::Rejected(error) => return Err(error.into()),
    };

    // Broadcast WebSocket event
    websocket::broadcast_event(
        &state,
        BoardEvent::ItemUpdated {
            project_id: item.project_id,
            item_id: item.id,
            old_status: Some(old_status.clone()),
            new_status: item.status.clone(),
        },
    );

    if let Some(wh) = &state.webhook {
        wh.fire(
            "item.updated",
            serde_json::json!({
                "event": "item.updated",
                "timestamp": Utc::now().to_rfc3339(),
                "project_id": item.project_id,
                "item": &item,
            }),
        );
    }

    // Auto-propagate parent status when all siblings reach Done
    propagate_parent_completion(&state, &item, &old_status).await;

    // Push the open/closed state back to a linked GitHub issue.
    maybe_sync_github(&state, &item, &old_status).await;

    let mut response = Json(serde_json::to_value(item).unwrap()).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&item_etag(id, version)).expect("etag is ascii"),
    );
    Ok(response)
}

/// Best-effort, fire-and-forget GitHub push: when a linked item crosses the
/// Done boundary, close (or reopen) its GitHub issue. No-op unless a
/// `TACK_GITHUB_TOKEN` is configured and the item has a `github_links` row.
pub(crate) async fn maybe_sync_github(state: &AppState, item: &Item, old_status: &str) {
    let Some(token) = state.config.github_token.clone() else {
        return;
    };
    if item.status == old_status {
        return;
    }
    let Ok(Some((repo, number))) = state.repo.get_github_link(item.id).await else {
        return;
    };
    let Ok(Some(proj)) = state.repo.get_project(item.project_id).await else {
        return;
    };
    let Some(closed) = crate::github_sync::state_change(
        proj.workflow.is_done_status(old_status),
        proj.workflow.is_done_status(&item.status),
    ) else {
        return;
    };

    let base = state.config.github_api_base.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::github_sync::push_issue_state(&base, &token, &repo, number, closed).await
        {
            tracing::warn!(repo = %repo, issue = number, error = %e, "GitHub status push failed");
        }
    });
}

/// Best-effort: when `item` just moved into a Done-category status, mark its
/// parent as done if every sibling is now complete. Errors are ignored.
pub(crate) async fn propagate_parent_completion(state: &AppState, item: &Item, old_status: &str) {
    if let Some(parent_id) = item.parent_id
        && item.status != old_status
        && let Ok(Some(proj)) = state.repo.get_project(item.project_id).await
        && proj.workflow.is_done_status(&item.status)
        && let Some(done_status) = proj.workflow.find_first_done_status()
        && let Ok(all_done) = state.repo.siblings_all_done(parent_id, done_status).await
        && tack_core::workflow::WorkflowConfig::should_complete_parent(all_done)
    {
        let _ = state
            .repo
            .check_and_update_parent_status(parent_id, done_status)
            .await;
    }
}

#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{id}",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    // Get item before deleting to get project_id for event
    let item = state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

    let deleted = state.repo.delete_item(id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("Item {id} not found")));
    }

    // Broadcast WebSocket event
    websocket::broadcast_event(
        &state,
        BoardEvent::ItemDeleted {
            project_id: item.project_id,
            item_id: id,
        },
    );

    if let Some(wh) = &state.webhook {
        wh.fire(
            "item.deleted",
            serde_json::json!({
                "event": "item.deleted",
                "timestamp": Utc::now().to_rfc3339(),
                "project_id": item.project_id,
                "item_id": id,
            }),
        );
    }

    Ok(Json(serde_json::json!({"deleted": true})))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/items/tree",
    tag = "items",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Item hierarchy (parents with nested children)", body = Vec<tack_core::models::Item>),
    ),
)]
pub async fn get_item_tree(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.get_item_tree(project_id).await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/search",
    tag = "search",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
        SearchParams,
    ),
    responses(
        (status = 200, description = "Matching items", body = Vec<tack_core::models::Item>),
    ),
)]
pub async fn search_items(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.search_items(project_id, &params.q).await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    pub q: String,
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/search",
    tag = "search",
    params(
        SearchParams,
    ),
    responses(
        (status = 200, description = "Matching items across all projects", body = Vec<tack_core::models::Item>),
    ),
)]
pub async fn search_items_global(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state
        .repo
        .search_items_global(state.workspace_id, &params.q)
        .await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}
