use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateItem, Item, ItemFilter, UpdateItem};
use tack_db::repo::orch::NewOrchEvent;

use crate::dispatcher::{self, DispatchOutcome};
use crate::error::{ApiError, ApiResult};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

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
        (status = 200, description = "Item with roles and dependencies", body = crate::openapi::ItemDetail),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
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
#[utoipa::path(
    patch,
    path = "/api/items/{id}",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    request_body = tack_core::models::UpdateItem,
    responses(
        (status = 200, description = "Updated item", body = tack_core::models::Item),
        (status = 400, description = "Invalid transition / validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut input): Json<UpdateItem>,
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

        // Resolve the target status category so the repo can maintain
        // started_at / completed_at as the item crosses category boundaries.
        input.status_category = project
            .workflow
            .statuses
            .iter()
            .find(|s| &s.name == new_status)
            .map(|s| s.category.clone());
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

    // Auto-dispatch to the linked control plane, if configured (card C2 /
    // task 35.5). Off unless TACK_ORCH_ENABLE is set and the project's
    // orch_link has auto_dispatch on (§0 rules 5 and 8).
    maybe_auto_dispatch(&state, &item, &old_status).await;

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

/// Best-effort: when `orch_links.auto_dispatch` is on and `item` just
/// entered one of the link's `status_map.dispatch_from` statuses, dispatch
/// it automatically via `dispatcher::dispatch_item` (card C1), passing the
/// item's own **persisted** trust value (`item.source.is_trusted()` —
/// `tack_core::models::ItemSource`, migration 029) rather than inferring
/// anything at dispatch time (TODO.md's C2 card is explicit that inference
/// is the failure mode to avoid: a deleted `github_links` row or a
/// forgotten future import path would silently flip an item back to
/// trusted).
///
/// **Fires at most once per status entry, not on every edit.** Runs off the
/// request path (`tokio::spawn`, the same shape `maybe_sync_github` already
/// uses) so a slow or unreachable control plane can never turn a card move
/// into a slow or failing PATCH — TODO.md's card text: "a dispatch failure
/// logs, records an event, and never fails the user's PATCH." Two
/// independent guards make "don't dispatch on every update" hold:
///
/// 1. **`item.status == old_status` short-circuits before any DB or HTTP
///    call.** An item edited while it is already sitting in a
///    `dispatch_from` status (title tweak, priority change, etc.) never
///    reaches `dispatch_item` at all, because its status didn't change.
/// 2. **`dispatch_item`'s own idempotency guard** (a process-wide per-item
///    lock plus an `orch_tasks` "already in flight" check — see that
///    module's doc comment) is the second, belt-and-suspenders layer: two
///    genuine status changes into the same `dispatch_from` status in quick
///    succession (or a concurrent manual dispatch) still produce exactly
///    one task, not two.
///
/// **Visibility on failure.** A transport/config error (`Err`) or a
/// `pre_input` policy block (`DispatchOutcome::Blocked`) is not silently
/// swallowed: both are logged via `tracing::warn!` *and* recorded as an
/// `orch_events` row (`auto_dispatch_failed` / `auto_dispatch_blocked`) on
/// the item, the same table `dispatcher::apply_mapped_status` already uses
/// for `status_map_rejected` — so a failed auto-dispatch shows up wherever
/// that event history is surfaced (the item's Agent Activity tab, card B5),
/// not just in server logs nobody is watching. Every other outcome
/// (`NoDispatchPolicy`, `NotEligible`, `AlreadyInFlight`, `Success`) is
/// expected, uninteresting background behavior and is not separately
/// recorded here — `Success` already gets its own `orch_tasks` row and, for
/// `on_running`/`on_waiting_approval`, its own status_map bookkeeping
/// inside `dispatch_item` itself.
pub(crate) async fn maybe_auto_dispatch(state: &AppState, item: &Item, old_status: &str) {
    if !state.config.orch_enable {
        return;
    }
    if item.status == old_status {
        return;
    }
    let Ok(Some(link)) = state.repo.get_orch_link(item.project_id).await else {
        return;
    };
    if !link.auto_dispatch {
        return;
    }

    let state = state.clone();
    let item_id = item.id;
    let trusted = item.source.is_trusted();
    let control_plane_id = link.control_plane_id;

    tokio::spawn(async move {
        match dispatcher::dispatch_item(&state, item_id, trusted).await {
            Ok(DispatchOutcome::Blocked { policy_id, message }) => {
                tracing::warn!(
                    item_id = %item_id,
                    policy_id = %policy_id,
                    message = %message,
                    "auto-dispatch blocked by control-plane policy"
                );
                record_auto_dispatch_event(
                    &state,
                    item_id,
                    control_plane_id,
                    "auto_dispatch_blocked",
                    &message,
                    Some(&policy_id),
                )
                .await;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(item_id = %item_id, error = %e, "auto-dispatch failed");
                record_auto_dispatch_event(
                    &state,
                    item_id,
                    control_plane_id,
                    "auto_dispatch_failed",
                    &e.to_string(),
                    None,
                )
                .await;
            }
        }
    });
}

/// Best-effort: record an `auto_dispatch_failed`/`auto_dispatch_blocked`
/// `orch_events` row so a failed auto-dispatch is visible somewhere a human
/// (or card B5's Agent Activity UI) can find it, not just in server logs.
/// Never panics or propagates — an audit-trail write failing must not turn
/// an already-logged background failure into a crashed background task.
///
/// `policy_id` is `Some` only for `auto_dispatch_blocked` (a typed
/// `OrchError::PolicyBlocked`, card R1) — `auto_dispatch_failed` covers
/// every other, non-policy failure and has no policy id to carry.
async fn record_auto_dispatch_event(
    state: &AppState,
    item_id: Uuid,
    control_plane_id: Uuid,
    event_type: &str,
    message: &str,
    policy_id: Option<&str>,
) {
    let event = NewOrchEvent {
        id: Uuid::new_v4(),
        item_id: Some(item_id),
        run_id: None,
        event_type: event_type.to_string(),
        payload: serde_json::json!({ "message": message, "policy_id": policy_id }),
        occurred_at: Utc::now(),
    };
    if let Err(e) = state
        .repo
        .upsert_orch_events(control_plane_id, std::slice::from_ref(&event))
        .await
    {
        tracing::warn!(
            item_id = %item_id,
            error = %e,
            "failed to record auto-dispatch event"
        );
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
