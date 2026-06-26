use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateItem, Item, ItemFilter, UpdateItem};

use crate::error::{ApiError, ApiResult};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

#[instrument(skip(state))]
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
pub async fn list_items(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(filter): Query<ItemFilter>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.list_items(project_id, &filter).await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}

#[instrument(skip(state))]
pub async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let item = state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

    // Also fetch roles and dependencies for the detail view
    let roles = state.repo.get_roles_for_item(id).await?;
    let deps = state.repo.list_dependencies_for_item(id).await?;

    Ok(Json(serde_json::json!({
        "item": item,
        "roles": roles,
        "dependencies": deps,
    })))
}

#[instrument(skip(state))]
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateItem>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Get old item state for status change detection
    let old_item = state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

    let old_status = old_item.status.clone();

    // If status is being changed, validate the transition
    if let Some(ref new_status) = input.status {
        let project = state
            .repo
            .get_project(old_item.project_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Project not found".into()))?;

        // Validate transition
        project
            .workflow
            .validate_transition(&old_item.status, new_status)?;

        // Check WIP limit for target column
        let count = state
            .repo
            .count_items_by_status(old_item.project_id, new_status)
            .await? as usize;
        project.workflow.check_wip_limit(new_status, count)?;
    }

    let item = state
        .repo
        .update_item(id, input)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

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

    Ok(Json(serde_json::to_value(item).unwrap()))
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
pub async fn get_item_tree(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.get_item_tree(project_id).await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}

#[instrument(skip(state))]
pub async fn search_items(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = state.repo.search_items(project_id, &params.q).await?;
    Ok(Json(serde_json::to_value(items).unwrap()))
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchParams {
    pub q: String,
}

#[instrument(skip(state))]
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
