use std::collections::HashMap;

use crate::extract::{Json, Path, Query};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_app::change_sets::{self, Mode, Proposal};
use tack_app::events::{DomainEvent, Fact};
use tack_app::parents::{self, Subject};
use tack_core::change_set::{CreateOp, ItemRef, Operation, Origin};
use tack_core::children;
use tack_core::hierarchy::Hierarchy;
use tack_core::models::{ChildrenProgress, CreateItem, Item, ItemCard, ItemFilter, UpdateItem};
use tack_core::principal::Principal;
use tack_db::queries;
use tack_db::repo::items::{AtomicItemUpdateOutcome, ItemError};
use tack_protocol::execution::StableErrorCode;

use crate::error::{ApiError, ApiResult};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

impl From<ItemError> for ApiError {
    fn from(error: ItemError) -> Self {
        match error {
            ItemError::Store(failure) => ApiError::Store(failure),
            ItemError::Refused(rule) => ApiError::Core(rule),
        }
    }
}

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

/// How far each parent's children are, by parent, under the project's workflow.
async fn progress_by_parent(
    state: &AppState,
    project_id: Uuid,
) -> ApiResult<HashMap<Uuid, ChildrenProgress>> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    Ok(queries::children::of_project(&state.repo, project_id)
        .await?
        .into_iter()
        .filter_map(|(parent, kids)| {
            children::progress(&kids, &project.workflow).map(|progress| (parent, progress))
        })
        .collect())
}

/// An item of the tree, with how far its children are when it has any.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemNode {
    #[serde(flatten)]
    pub item: Item,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children_progress: Option<ChildrenProgress>,
}

/// A stale `If-Match` answers 412 with the same `{"error": {status, message, code}}` envelope
/// every other failure of this API has.
fn precondition_failed(message: String) -> Response {
    ApiError::Coded {
        code: StableErrorCode::PreconditionFailed,
        message,
        field: None,
    }
    .into_response()
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
        (status = 200, description = "Item created", body = tack_core::models::ItemCard),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "The parent breaks the tree rule: a group under work (group_under_work), a tree deeper than five levels (tree_too_deep) or a parent of another project (parent_in_other_project)", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_item(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<tack_core::principal::Principal>,
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

    // A new task has no run history yet, so its card is the bare item.
    let mut item = ItemCard::from(
        state
            .repo
            .create_item(project_id, &initial_status, input)
            .await?,
    );
    (item.created_by, item.updated_by) = state
        .repo
        .record_item_writer(item.id, &principal.label(), true)
        .await?;

    state.events().emit(DomainEvent::new(
        project_id,
        &principal,
        Fact::ItemCreated {
            item_id: item.id,
            status: item.status.clone(),
        },
    ));

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
    let mut items = state.repo.list_items(project_id, &filter).await?;
    let mut progress = progress_by_parent(&state, project_id).await?;
    for card in &mut items {
        card.children_progress = progress.remove(&card.item.id);
    }
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
    let mut snapshot = state
        .repo
        .get_item_snapshot(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;
    snapshot.item.children_progress = progress_by_parent(&state, snapshot.item.project_id)
        .await?
        .remove(&id);

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
        (status = 200, description = "Updated item; carries the ETag for the exact returned snapshot", body = tack_core::models::ItemCard,
            headers(("ETag" = String, description = "Version-derived entity tag for the returned item snapshot"))
        ),
        (status = 400, description = "Invalid transition / validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
        (status = 412, description = "If-Match did not match the current item version — nothing was written", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "The tree rule refuses the parent or the sprint: group_under_work, parent_loop, tree_too_deep, parent_in_other_project, group_in_sprint", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_item(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<tack_core::principal::Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    from_mcp: Option<axum::Extension<crate::middleware::FromMcp>>,
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

    let (mut item, version, old_status) = match state
        .repo
        .update_item_atomically(id, input, &project.workflow, &Hierarchy, expected_version)
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

    (item.created_by, item.updated_by) = state
        .repo
        .record_item_writer(item.id, &principal.label(), false)
        .await?;

    let over_mcp = from_mcp.is_some_and(|axum::Extension(mark)| mark.0);
    after_update(&state, &principal, &project, &item, &old_status, over_mcp).await;

    let mut response = Json(serde_json::to_value(item).unwrap()).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&item_etag(id, version)).expect("etag is ascii"),
    );
    Ok(response)
}

/// Everything that follows a saved change to an item, the person's route and a change set's
/// alike: the board's event channel (and with it the WebSocket) hears of it, a
/// closed recurring task makes the next one, a linked GitHub issue follows, and into Done the
/// finished run is merged when the project asks for it. `over_mcp` says the
/// change came over MCP, which the merge holds back for a person.
pub(crate) async fn after_update(
    state: &AppState,
    principal: &Principal,
    project: &tack_core::models::Project,
    item: &ItemCard,
    old_status: &str,
    over_mcp: bool,
) {
    state.events().emit(DomainEvent::item_changed(
        item.project_id,
        principal,
        item.id,
        old_status,
        &item.status,
    ));

    // A recurring task that was just closed makes the next one.
    create_next_recurrence(state, principal, &project.workflow, item, old_status).await;

    // Push the open/closed state back to a linked GitHub issue.
    maybe_sync_github(state, item, old_status).await;

    // Into Done: merge the finished run when the project asks for it.
    if item.status != old_status
        && project.workflow.is_done_status(&item.status)
        && !project.workflow.is_done_status(old_status)
    {
        // In the background: the move answers at once, and the merge (or why it
        // did not happen) lands on the item's activity.
        let (app, item_id, project_id) = (
            state.clone(),
            item.id.to_string(),
            item.project_id.to_string(),
        );
        state.tasks.spawn(async move {
            tack_app::merge::on_done(
                &app.repo,
                &tack_protocol::SystemClock,
                &app.ports(),
                &item_id,
                &project_id,
                over_mcp,
            )
            .await;
        });
    }
}

/// When a task with a `recurrence` enters Done from a status that was not Done and
/// no task made from it is unfinished, create the next one (with the same roles)
/// and note it on the closed one. Errors are ignored: the close has already happened.
async fn create_next_recurrence(
    state: &AppState,
    principal: &Principal,
    workflow: &tack_core::workflow::WorkflowConfig,
    item: &Item,
    old_status: &str,
) {
    use tack_core::workflow::StatusCategory;
    let Some(recurrence) = item.recurrence else {
        return;
    };
    if !workflow.is_done_status(&item.status) || workflow.is_done_status(old_status) {
        return;
    }
    let Ok(made) =
        tack_db::queries::items::statuses_of_recurrences(&state.repo, &item.id.to_string()).await
    else {
        return;
    };
    if made.iter().any(|status| !workflow.is_done_status(status)) {
        return;
    }
    let Some(first_todo) = workflow
        .statuses
        .iter()
        .filter(|s| s.category == StatusCategory::Todo)
        .min_by_key(|s| s.order)
        .map(|s| s.name.clone())
        .or_else(|| workflow.initial_status().ok())
    else {
        return;
    };
    let from = item.due_date.unwrap_or_else(Utc::now).date_naive();
    let due = tack_core::recurrence::next_due(from, recurrence);
    let input = tack_core::recurrence::clone_for_next(item, due);
    let Ok(next) = state
        .repo
        .create_item(item.project_id, &first_todo, input)
        .await
    else {
        return;
    };
    if let Ok(roles) = state.repo.get_roles_for_item(item.id).await {
        for role in roles {
            let _ = state.repo.assign_role_to_item(next.id, role.id).await;
        }
    }
    let _ = state
        .repo
        .create_system_comment(
            item.id,
            format!("Created the next one, due {due}"),
            tack_core::models::CommentType::System,
        )
        .await;
    state.events().emit(DomainEvent::new(
        next.project_id,
        principal,
        Fact::ItemCreated {
            item_id: next.id,
            status: next.status.clone(),
        },
    ));
}

/// Best-effort, fire-and-forget GitHub push: when a linked item crosses the
/// Done boundary, close (or reopen) its GitHub issue. No-op unless a token
/// resolves (the item's project's own `github_token_ref`, else
/// `TACK_GITHUB_TOKEN` — see `GitHub::token_for_project`) and the
/// item has a `github_links` row.
pub(crate) async fn maybe_sync_github(state: &AppState, item: &Item, old_status: &str) {
    if item.status == old_status {
        return;
    }
    let Ok(Some((repo, number))) = state.repo.get_github_link(item.id).await else {
        return;
    };
    let Ok(Some(proj)) = state.repo.get_project(item.project_id).await else {
        return;
    };
    let Some(token) = state.github_token(item.project_id).await else {
        return;
    };
    let Some(closed) = tack_app::github_sync::state_change(
        proj.workflow.is_done_status(old_status),
        proj.workflow.is_done_status(&item.status),
    ) else {
        return;
    };

    let base = state.github().api_base;
    state.tasks.spawn(async move {
        if let Err(e) =
            tack_app::github_sync::push_issue_state(&base, &token, &repo, number, closed).await
        {
            tracing::warn!(repo = %repo, issue = number, error = %e, "GitHub status push failed");
        }
    });
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
    axum::Extension(principal): axum::Extension<Principal>,
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

    state.events().emit(DomainEvent::new(
        item.project_id,
        &principal,
        Fact::ItemDeleted { item_id: id },
    ));

    Ok(Json(serde_json::json!({"deleted": true})))
}

/// Body of `PUT /api/items/{id}/github-link` and the response of
/// `GET /api/items/{id}/github-link` — the one new type this route family
/// needs.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct GithubLinkBody {
    /// "owner/repo" or a full GitHub URL — parsed the same way
    /// `POST /api/projects/{id}/import-github` parses its own `repo` field.
    pub repo: String,
    pub issue_number: i64,
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{id}/github-link",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "The item's current GitHub link", body = GithubLinkBody),
        (status = 404, description = "Item not found, or not linked", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_item_github_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GithubLinkBody>> {
    state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;
    let (repo, issue_number) = state
        .repo
        .get_github_link(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} has no GitHub link")))?;
    Ok(Json(GithubLinkBody { repo, issue_number }))
}

/// Manually links an item to a GitHub issue so status and comments sync both ways — the
/// same `github_links` row `POST /api/projects/{id}/import-github` writes
/// for every item it creates, settable here for an item that was never
/// imported.
#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/items/{id}/github-link",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    request_body = GithubLinkBody,
    responses(
        (status = 204, description = "Linked"),
        (status = 400, description = "Invalid repo or issue number", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_item_github_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<GithubLinkBody>,
) -> ApiResult<StatusCode> {
    state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;
    let (owner, repo_name) =
        super::import_github::parse_github_repo(&input.repo).ok_or_else(|| {
            ApiError::BadRequest(format!(
                "Invalid GitHub repo '{}'. Use 'owner/repo' or a full GitHub URL.",
                input.repo
            ))
        })?;
    if input.issue_number < 1 {
        return Err(ApiError::BadRequest("issue_number must be >= 1".into()));
    }
    state
        .repo
        .set_github_link(id, &format!("{owner}/{repo_name}"), input.issue_number)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes an item's manual (or imported) GitHub link. `204` even when the
/// item had no link — matches `rm -f`, not `rm`.
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{id}/github-link",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 204, description = "Unlinked (or was already unlinked)"),
        (status = 404, description = "Item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn delete_item_github_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;
    state.repo.remove_github_link(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, Serialize, Validate, utoipa::ToSchema)]
pub struct CreateSubtaskFromPlan {
    #[validate(length(min = 1, message = "title must not be empty"))]
    pub title: String,
    #[validate(length(max = 50_000, message = "description too long (max 50 000 chars)"))]
    pub description: Option<String>,
    #[validate(length(max = 50, message = "at most 50 acceptance criteria"))]
    pub acceptance: Vec<String>,
    /// Indexes of entries in this list that block this one.
    #[serde(default)]
    pub depends_on: Vec<u32>,
    /// The item type, when the project has it; else the parent's when that is work, else the default work type.
    #[serde(default, rename = "type")]
    pub item_type: Option<String>,
    /// A project role, by name.
    #[serde(default)]
    pub role: Option<String>,
    /// A team, by name; the one profile that names it goes into the item's run settings.
    #[serde(default)]
    pub team: Option<String>,
    /// A profile, by id: any profile, with a team or without, goes into the item's run settings.
    /// It wins over `team`.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// The index of an earlier entry this one is a sub-item of; absent means a sub-item of the item.
    #[serde(default)]
    pub parent: Option<u32>,
}

#[derive(Debug, Deserialize, Validate, Serialize, utoipa::ToSchema)]
pub struct CreateSubtasksFromPlanRequest {
    /// The plan artifact the entries came from; absent means a file the person uploaded.
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[validate(length(min = 1, message = "subtasks list must not be empty"))]
    pub subtasks: Vec<CreateSubtaskFromPlan>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreatedSubtask {
    pub id: Uuid,
    pub title: String,
    /// The role name the project does not have; the item has no role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_role: Option<String>,
    /// The type the project does not have; the item has its parent's type when that is work, else the default work type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_type: Option<String>,
    /// The team that no profile, or more than one, names; the item's run settings name no profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_team: Option<String>,
    /// The profile id that no profile has; the item's run settings name no profile from it, and
    /// its `team` is still tried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_profile: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateSubtasksFromPlanResponse {
    pub created: Vec<CreatedSubtask>,
}

/// The entries' indexes must point inside the list, a parent before its child, and the blockers
/// must not loop; checked before anything is written.
fn check_plan_links(subtasks: &[CreateSubtaskFromPlan]) -> Result<(), ApiError> {
    let ids: Vec<Uuid> = subtasks.iter().map(|_| Uuid::new_v4()).collect();
    let mut graph = tack_core::dependency::DependencyGraph::new();
    for (idx, entry) in subtasks.iter().enumerate() {
        if entry.parent.is_some_and(|parent| parent as usize >= idx) {
            return Err(ApiError::BadRequest(format!(
                "subtasks[{idx}]: parent must be the index of an earlier entry"
            )));
        }
        for &blocker in &entry.depends_on {
            let blocker = blocker as usize;
            if blocker >= subtasks.len() || blocker == idx {
                return Err(ApiError::BadRequest(format!(
                    "subtasks[{idx}]: depends_on {blocker} is not another entry of the list"
                )));
            }
            graph
                .validate_new_edge(ids[blocker], ids[idx])
                .map_err(ApiError::Core)?;
            graph.add_edge(
                ids[blocker],
                ids[idx],
                tack_core::models::DependencyType::Blocks,
            );
        }
    }
    Ok(())
}

#[instrument(skip(state, input))]
#[utoipa::path(
    post,
    path = "/api/items/{id}/subtasks-from-plan",
    tag = "items",
    params(
        ("id" = Uuid, Path, description = "Parent Item ID"),
    ),
    request_body = CreateSubtasksFromPlanRequest,
    responses(
        (status = 200, description = "Subtasks created", body = CreateSubtasksFromPlanResponse),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Parent item not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_subtasks_from_plan(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<tack_core::principal::Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateSubtasksFromPlanRequest>,
) -> ApiResult<Json<CreateSubtasksFromPlanResponse>> {
    // Validate request
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    for (idx, subtask_input) in input.subtasks.iter().enumerate() {
        subtask_input
            .validate()
            .map_err(|e| ApiError::BadRequest(format!("subtasks[{}]: {}", idx, e)))?;
    }
    check_plan_links(&input.subtasks)?;

    // Get parent item
    let parent = state
        .repo
        .get_item(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {id} not found")))?;

    // The entries are a change set of creates under the parent, applied at once: the one path
    // every change to the board goes through.
    let operations = input
        .subtasks
        .iter()
        .map(|entry| {
            Operation::Create(CreateOp {
                title: entry.title.clone(),
                description: entry.description.clone(),
                acceptance: entry.acceptance.clone(),
                depends_on: entry.depends_on.iter().map(|&i| ItemRef::New(i)).collect(),
                item_type: entry.item_type.clone(),
                role: entry.role.clone(),
                team: entry.team.clone(),
                profile_id: entry.profile_id.clone(),
                parent: Some(entry.parent.map_or(ItemRef::Existing(id), ItemRef::New)),
            })
        })
        .collect();
    let proposed = change_sets::propose(
        &state.repo,
        &tack_protocol::SystemClock,
        state.events().as_ref(),
        Proposal {
            project: parent.project_id,
            principal: &principal,
            origin: Origin::Run,
            origin_ref: input.artifact_id.clone(),
            reason: format!("The plan for \"{}\" was accepted", parent.title),
            idempotency_key: None,
            anchor: Some(id),
            artifact_id: input.artifact_id.clone(),
            operations,
            mode: Mode::Now,
        },
    )
    .await
    .map_err(|error| match error {
        tack_app::AppError::NotFound => {
            ApiError::NotFound(format!("Project {} not found", parent.project_id))
        }
        other => other.into(),
    })?;

    crate::handlers::change_sets::announce(&state, &proposed.change_set, &proposed.applied_now)
        .await;

    let created = proposed
        .change_set
        .ops
        .into_iter()
        .filter_map(|op| op.result)
        .map(|result| CreatedSubtask {
            id: result.item.unwrap_or_default(),
            title: result.title.unwrap_or_default(),
            unknown_role: result.unknown_role,
            unknown_type: result.unknown_type,
            unresolved_team: result.unresolved_team,
            unknown_profile: result.unknown_profile,
        })
        .collect();

    // The parent's card is the one the open boards refresh to show its new children; the
    // children themselves were announced above.
    websocket::broadcast_event(
        &state,
        BoardEvent::ItemUpdated {
            project_id: parent.project_id,
            item_id: id,
            old_status: Some(parent.status.clone()),
            new_status: parent.status.clone(),
        },
    );

    Ok(Json(CreateSubtasksFromPlanResponse { created }))
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
        (status = 200, description = "Every item of the project, parents first, each parent with how far its children are", body = Vec<ItemNode>),
    ),
)]
pub async fn get_item_tree(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut progress = progress_by_parent(&state, project_id).await?;
    let nodes: Vec<ItemNode> = state
        .repo
        .get_item_tree(project_id)
        .await?
        .into_iter()
        .map(|item| ItemNode {
            children_progress: progress.remove(&item.id),
            item,
        })
        .collect();
    Ok(Json(serde_json::to_value(nodes).unwrap()))
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

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ParentsQuery {
    /// An item that exists: the answer leaves out the item and everything under it.
    pub item: Option<Uuid>,
    /// The type key of an item about to be made.
    #[serde(rename = "type")]
    pub item_type: Option<String>,
}

/// An item that can be the parent of another.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ParentOption {
    pub id: Uuid,
    pub title: String,
    /// The key of its type in the project's catalog.
    #[serde(rename = "type")]
    pub item_type: String,
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/parents",
    tag = "items",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
        ParentsQuery,
    ),
    responses(
        (status = 200, description = "Every item the tree rule admits as the parent, in tree order", body = Vec<ParentOption>),
        (status = 400, description = "Neither or both of item and type were given", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "The project, the item or the type is not there", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn list_parents(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<ParentsQuery>,
) -> ApiResult<Json<Vec<ParentOption>>> {
    let subject = match (query.item, query.item_type) {
        (Some(item), None) => Subject::Item(item),
        (None, Some(key)) => Subject::NewOfType(key),
        _ => {
            return Err(ApiError::BadRequest(
                "give either item (a move) or type (a new item)".into(),
            ));
        }
    };
    let choices = parents::admitted(&state.repo, project_id, subject).await?;
    Ok(Json(
        choices
            .into_iter()
            .map(|choice| ParentOption {
                id: choice.id,
                title: choice.title,
                item_type: choice.item_type,
            })
            .collect(),
    ))
}
