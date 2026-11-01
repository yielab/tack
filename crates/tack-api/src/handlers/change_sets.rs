//! Change sets (ADR 0034, decisions 3 and 4): what an agent proposes to change on a board. A
//! project lists them, a person applies all of one or some of its operations (editing as they
//! go) or rejects it, and an agent holding the scope `propose` proposes one. The rules live in
//! `tack_core::change_set` and `tack_app::change_sets`; this file extracts, calls, announces
//! what changed to the board's listeners, and answers.

use axum::Extension;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use tack_app::change_sets::{self, Application, Mode, Proposal};
use tack_app::events::{DomainEvent, Fact};
use tack_core::change_set::{ChangeSet, OpState, Operation, Origin};
use tack_core::principal::Principal;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query};
use crate::handlers::items::after_update;
use crate::router::AppState;

/// What proposes a change set.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProposeChangeSet {
    /// Why the change is wanted, in a sentence a person reads before deciding.
    pub reason: String,
    /// Where it came from; absent means an MCP tool call.
    #[serde(default)]
    pub origin: Option<Origin>,
    /// The run or thread it came from.
    #[serde(default)]
    pub origin_ref: Option<String>,
    /// The same key on a project answers the first change set again.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    pub operations: Vec<Operation>,
}

/// A change set as it stands after it was proposed.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProposedChangeSet {
    pub change_set: ChangeSet,
    /// False when the idempotency key had been used: this is the first answer again.
    pub made: bool,
    /// The positions of the operations that applied at once; the rest wait for a person.
    pub applied_now: Vec<usize>,
}

/// An operation to put in place of a waiting one.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OperationEdit {
    pub index: usize,
    pub operation: Operation,
}

/// What a person applies.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ApplyChangeSet {
    /// The positions to apply; absent means everything that waits.
    #[serde(default)]
    pub operations: Option<Vec<usize>>,
    /// Edits to waiting operations, made before they apply.
    #[serde(default)]
    pub edits: Vec<OperationEdit>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RejectChangeSet {
    /// Why the waiting operations are turned down.
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditOperation {
    pub operation: Operation,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListParams {
    /// Comma-separated statuses: `waiting`, `applied`, `rejected`, `partly`. Absent lists all.
    pub status: Option<String>,
    /// At most this many, the newest first (default 100, at most 500).
    pub limit: Option<u32>,
}

/// Tells the board's listeners what the operations at `applied` did, as the change set's
/// proposer did them, and runs what follows a move the way the person's own route does.
pub(crate) async fn announce(state: &AppState, set: &ChangeSet, applied: &[usize]) {
    let Ok(Some(project)) = state.repo.get_project(set.project).await else {
        return;
    };
    let principal = Principal::from_label(&set.principal).unwrap_or_else(|| {
        tracing::warn!(change_set = %set.id, principal = %set.principal, "events: the proposer's label names no principal");
        Principal::system()
    });
    for &index in applied {
        let Some(op) = set.ops.get(index) else {
            continue;
        };
        let Some(result) = op.result.as_ref() else {
            continue;
        };
        let Some(item_id) = result.item else { continue };
        let Ok(Some(snapshot)) = state.repo.get_item_snapshot(item_id).await else {
            continue;
        };
        let item = snapshot.item;
        match (&op.operation, &result.moved_from) {
            (Operation::Create(_), _) => state.events().emit(DomainEvent::new(
                set.project,
                &principal,
                Fact::ItemCreated {
                    item_id,
                    status: item.status.clone(),
                },
            )),
            (Operation::Move { .. }, Some(from)) => {
                after_update(
                    state,
                    &principal,
                    &project,
                    &item,
                    from,
                    set.origin == Origin::Mcp,
                )
                .await;
            }
            _ => state.events().emit(DomainEvent::item_changed(
                set.project,
                &principal,
                item_id,
                &item.status,
                &item.status,
            )),
        }
    }
}

/// The positions that went from waiting to applied between two readings of a change set.
fn newly_applied(before: &ChangeSet, after: &ChangeSet) -> Vec<usize> {
    after
        .ops
        .iter()
        .enumerate()
        .filter(|(index, op)| {
            op.state == OpState::Applied
                && before
                    .ops
                    .get(*index)
                    .is_some_and(|old| old.state == OpState::Waiting)
        })
        .map(|(index, _)| index)
        .collect()
}

/// GET /api/projects/{project_id}/change-sets
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/change-sets",
    tag = "change-sets",
    params(("project_id" = Uuid, Path, description = "Project ID"), ListParams),
    responses(
        (status = 200, description = "The project's change sets, the newest first", body = Vec<ChangeSet>),
        (status = 400, description = "A status that is not one", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn list_change_sets(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Vec<ChangeSet>>> {
    let statuses = change_sets::parse_statuses(params.status.as_deref().unwrap_or(""))?;
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    Ok(Json(
        state
            .repo
            .list_change_sets(project_id, &statuses, limit)
            .await?,
    ))
}

/// POST /api/projects/{project_id}/change-sets
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/change-sets",
    tag = "change-sets",
    params(("project_id" = Uuid, Path, description = "Project ID")),
    request_body = ProposeChangeSet,
    responses(
        (status = 200, description = "Kept; what the project's rung allows has applied, the rest waits", body = ProposedChangeSet),
        (status = 400, description = "Refused whole, with every reason; nothing was kept", body = crate::openapi::ErrorEnvelope),
        (status = 403, description = "The credential lacks the scope `propose`", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn propose_change_set(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<ProposeChangeSet>,
) -> ApiResult<Json<ProposedChangeSet>> {
    let proposed = change_sets::propose(
        &state.repo,
        &tack_protocol::SystemClock,
        Proposal {
            project: project_id,
            principal: &principal,
            origin: input.origin.unwrap_or(Origin::Mcp),
            origin_ref: input.origin_ref,
            reason: input.reason,
            idempotency_key: input.idempotency_key,
            anchor: None,
            artifact_id: None,
            operations: input.operations,
            mode: Mode::Rung,
        },
    )
    .await
    .map_err(|error| match error {
        tack_app::AppError::NotFound => {
            ApiError::NotFound(format!("Project {project_id} not found"))
        }
        other => other.into(),
    })?;
    announce(&state, &proposed.change_set, &proposed.applied_now).await;
    Ok(Json(ProposedChangeSet {
        change_set: proposed.change_set,
        made: proposed.made,
        applied_now: proposed.applied_now,
    }))
}

/// GET /api/change-sets/{id}
#[utoipa::path(
    get,
    path = "/api/change-sets/{id}",
    tag = "change-sets",
    params(("id" = Uuid, Path, description = "Change set ID")),
    responses(
        (status = 200, description = "The change set with each operation's state and result", body = ChangeSet),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_change_set(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ChangeSet>> {
    state
        .repo
        .get_change_set(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("Change set {id} not found")))
}

/// POST /api/change-sets/{id}/apply
#[utoipa::path(
    post,
    path = "/api/change-sets/{id}/apply",
    tag = "change-sets",
    params(("id" = Uuid, Path, description = "Change set ID")),
    request_body = ApplyChangeSet,
    responses(
        (status = 200, description = "Applied in one transaction", body = ChangeSet),
        (status = 400, description = "Refused whole, with every reason; the change set is as it was", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "An operation chosen is not waiting", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn apply_change_set(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<ApplyChangeSet>,
) -> ApiResult<Json<ChangeSet>> {
    let before = state
        .repo
        .get_change_set(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Change set {id} not found")))?;
    let after = change_sets::apply(
        &state.repo,
        &tack_protocol::SystemClock,
        id,
        &principal,
        Application {
            operations: input.operations,
            edits: input
                .edits
                .into_iter()
                .map(|edit| (edit.index, edit.operation))
                .collect(),
        },
    )
    .await?;
    announce(&state, &after, &newly_applied(&before, &after)).await;
    Ok(Json(after))
}

/// POST /api/change-sets/{id}/reject
#[utoipa::path(
    post,
    path = "/api/change-sets/{id}/reject",
    tag = "change-sets",
    params(("id" = Uuid, Path, description = "Change set ID")),
    request_body = RejectChangeSet,
    responses(
        (status = 200, description = "What waited is turned down", body = ChangeSet),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Nothing waits in it", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn reject_change_set(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<RejectChangeSet>,
) -> ApiResult<Json<ChangeSet>> {
    Ok(Json(
        change_sets::reject(
            &state.repo,
            &tack_protocol::SystemClock,
            id,
            &principal,
            &input.reason,
        )
        .await?,
    ))
}

/// PATCH /api/change-sets/{id}/operations/{index}
#[utoipa::path(
    patch,
    path = "/api/change-sets/{id}/operations/{index}",
    tag = "change-sets",
    params(
        ("id" = Uuid, Path, description = "Change set ID"),
        ("index" = usize, Path, description = "Position of the operation"),
    ),
    request_body = EditOperation,
    responses(
        (status = 200, description = "The operation is replaced; it is checked when applied", body = ChangeSet),
        (status = 404, description = "Not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "The operation is not waiting, or the edit changes its kind", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input))]
pub async fn edit_operation(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, usize)>,
    Json(input): Json<EditOperation>,
) -> ApiResult<Json<ChangeSet>> {
    Ok(Json(
        change_sets::edit_operation(&state.repo, id, index, input.operation).await?,
    ))
}
