//! The project folder (ADR 0025): a project's plan kept in files beside the code. Six routes read
//! it, preview it and apply it. The plan compares the folder, the board and the last revision;
//! the apply writes it in one transaction and leaves two revisions.

use std::collections::BTreeMap;

use axum::Extension;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::{ItemSource, Project, UpdateProject};
use tack_core::outline::bundle::sync::{
    Change, Conflict, Merged, Prefer, Three, Verb, diff, merge, without_board_keys,
};
use tack_core::outline::bundle::{Plan, check};
use tack_core::outline::folder;
use tack_core::outline::key::key_of;
use tack_core::outline::{Outline, OutlineItem};
use tack_core::principal::{Principal, PrincipalKind};
use tack_db::repo::project_file::{Removals, RevisionRow, RevisionWrite, Target, body_hash};

use crate::error::{ApiError, ApiResult};
use crate::extract::{Json, Path, Query, RawBody};
use crate::handlers::export::bundle_of;
use crate::handlers::outline::{
    OwnedBoard, board_of, current_settings, failed, refused, to_write, visible_items,
};
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// The project folder as one object: each path relative to `.tack/` and its text.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Folder {
    pub files: BTreeMap<String, String>,
}

/// One revision of a project's plan, without its body.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RevisionSummary {
    pub number: i64,
    /// `board` (an export), `file` (an import) or `revert`.
    pub origin: String,
    pub hash: String,
    pub created_at: String,
}

impl From<RevisionRow> for RevisionSummary {
    fn from(row: RevisionRow) -> Self {
        Self {
            number: row.number,
            origin: row.origin,
            hash: row.hash,
            created_at: row.created_at,
        }
    }
}

/// What the board changed since the project folder's last revision.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderStatus {
    pub last_revision: Option<RevisionSummary>,
    pub board_hash: String,
    pub changes: Vec<Change>,
}

/// A removal and what it deletes with it, one line each.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Removal {
    pub section: String,
    pub key: String,
    pub name: String,
    pub cost: Vec<String>,
}

/// What an apply would do; nothing is written.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderPlan {
    pub plan_hash: String,
    /// The last revision, which the board and the folder are compared with.
    pub base: Option<RevisionSummary>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
    pub removals: Vec<Removal>,
    pub warnings: Vec<String>,
    /// False while a conflict has no side to take.
    pub applies: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderApplied {
    pub revision: RevisionSummary,
    pub changes: Vec<Change>,
    pub removals: Vec<Removal>,
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlanQuery {
    /// Go back to this revision instead of reading a folder from the body.
    pub revision: Option<i64>,
    /// Settle every conflict with the folder's side or the board's.
    pub prefer: Option<Prefer>,
    /// The hash the preview answered; an apply needs it.
    pub plan_hash: Option<String>,
}

/// A plan as a revision keeps it: the configuration and each item's plan, without what the board
/// keeps for itself, every section in key order, as canonical JSON.
fn body_text(outline: &Outline) -> String {
    let plan = Outline {
        teams: Vec::new(),
        items: outline
            .items
            .iter()
            .map(|item| OutlineItem {
                reference: None,
                status: None,
                priority: None,
                assignee: None,
                sprint: None,
                due_date: None,
                origin: None,
                ..item.clone()
            })
            .collect(),
        ..outline.clone()
    };
    let mut value = serde_json::to_value(&plan).unwrap_or_default();
    for section in ["types", "fields", "profiles", "roles", "sprints", "items"] {
        if let Some(Value::Array(rows)) = value.get_mut(section) {
            rows.sort_by_key(|row| {
                let text = |name: &str| row.get(name).and_then(Value::as_str);
                text("key").map_or_else(
                    || key_of(text("name").or(text("title")).unwrap_or_default()),
                    str::to_owned,
                )
            });
        }
    }
    value.to_string()
}

fn outline_of(body: &str) -> ApiResult<Outline> {
    serde_json::from_str(body)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("a revision's body is unreadable: {e}")))
}

async fn project_of(state: &AppState, id: Uuid) -> ApiResult<Project> {
    state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))
}

/// Keeps the board's plan as a revision when it differs from the last one.
pub(crate) async fn record_board(
    state: &AppState,
    project_id: Uuid,
    board: &Outline,
) -> ApiResult<()> {
    state
        .repo
        .record_revision(project_id, "board", &body_text(board))
        .await?;
    Ok(())
}

/// GET /api/projects/{id}/folder
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/folder",
    tag = "folder",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project folder as the board has it; a revision when the board changed since the last", body = Folder),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn export_folder(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<Folder>> {
    let project = project_of(&state, project_id).await?;
    let items = visible_items(&state, project_id).await?;
    let board = bundle_of(&state, &project, &items).await?;
    record_board(&state, project_id, &board).await?;
    Ok(Json(Folder {
        files: folder::write(&board),
    }))
}

/// GET /api/projects/{id}/folder/status
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/folder/status",
    tag = "folder",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "What the board changed since the last revision", body = FolderStatus),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn folder_status(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<FolderStatus>> {
    let project = project_of(&state, project_id).await?;
    let items = visible_items(&state, project_id).await?;
    let board = body_text(&bundle_of(&state, &project, &items).await?);
    let last = state.repo.last_revision(project_id).await?;
    let changes = match &last {
        Some(last) => diff(&outline_of(&last.body)?, &outline_of(&board)?),
        None => Vec::new(),
    };
    Ok(Json(FolderStatus {
        last_revision: last.map(Into::into),
        board_hash: body_hash(&board),
        changes,
    }))
}

/// GET /api/projects/{id}/folder/revisions
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/folder/revisions",
    tag = "folder",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The revisions, newest first", body = Vec<RevisionSummary>),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn list_revisions(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<Vec<RevisionSummary>>> {
    project_of(&state, project_id).await?;
    let rows = state.repo.list_revisions(project_id).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// GET /api/projects/{id}/folder/revisions/{n}
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{id}/folder/revisions/{n}",
    tag = "folder",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("n" = i64, Path, description = "The revision's number"),
    ),
    responses(
        (status = 200, description = "The project folder as that revision had it", body = Folder),
        (status = 404, description = "Project or revision not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_revision(
    State(state): State<AppState>,
    Path((project_id, number)): Path<(Uuid, i64)>,
) -> ApiResult<Json<Folder>> {
    project_of(&state, project_id).await?;
    let row = state
        .repo
        .revision(project_id, number)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Revision {number} not found")))?;
    Ok(Json(Folder {
        files: folder::write(&outline_of(&row.body)?),
    }))
}

/// A plan against the live board: what an apply would write, and what it would take out.
struct Planned {
    project: Project,
    owned: OwnedBoard,
    base: Option<RevisionRow>,
    revert: bool,
    /// The board's plan as a revision keeps it.
    board_text: String,
    merged: Outline,
    changes: Vec<Change>,
    conflicts: Vec<Conflict>,
    removals: Vec<Removal>,
    removed: Removals,
    warnings: Vec<String>,
    plan: Plan,
    hash: String,
    applies: bool,
}

/// The id of the board's row a removal names, by its section and key.
fn id_of(owned: &OwnedBoard, section: &str, key: &str) -> Option<String> {
    match section {
        "profiles" => owned
            .profiles
            .iter()
            .find(|p| p.key == key && !p.builtin)
            .map(|p| p.id.clone()),
        "roles" => owned
            .roles
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.id.clone()),
        "sprints" => owned
            .sprints
            .iter()
            .find(|s| s.key == key)
            .map(|s| s.id.clone()),
        "fields" => owned
            .fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.id.to_string()),
        "items" => owned
            .items
            .iter()
            .find(|i| i.key == key)
            .map(|i| i.id.to_string()),
        "types" => Some(key.to_owned()),
        _ => None,
    }
}

fn uuid_of(id: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| ApiError::Internal(anyhow::anyhow!("{id}: {e}")))
}

/// What `plan` and `apply` share: the folder (or the revision) compared with the board and the
/// last revision, checked whole, with each removal turned into its cost, or into a thing kept.
async fn planned(
    state: &AppState,
    project_id: Uuid,
    body: &[u8],
    query: &PlanQuery,
) -> ApiResult<Planned> {
    let project = project_of(state, project_id).await?;
    let owned = board_of(state, &project).await?;
    let board = bundle_of(state, &project, &owned.items).await?;
    let board_text = body_text(&board);
    let base = state.repo.last_revision(project_id).await?;
    let base_outline = base.as_ref().map(|r| outline_of(&r.body)).transpose()?;
    let (file, revert) = match query.revision {
        Some(number) => {
            let row = state
                .repo
                .revision(project_id, number)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Revision {number} not found")))?;
            (outline_of(&row.body)?, true)
        }
        None => {
            let sent: Folder = serde_json::from_slice(body).map_err(|_| {
                ApiError::BadRequest(
                    "send the project folder as {\"files\": {path: text}}, or ask for a revision"
                        .into(),
                )
            })?;
            let outline = folder::read(&sent.files)
                .map_err(|problems| ApiError::BadRequest(problems.join("; ")))?;
            (outline, false)
        }
    };
    let Merged {
        outline: mut merged,
        changes,
        conflicts,
    } = merge(
        &Three {
            base: base_outline.as_ref(),
            board: &board,
            file: &file,
        },
        query.prefer,
    );

    let mut problems = Vec::new();
    let mut shown = Vec::with_capacity(changes.len());
    let (mut removals, mut removed) = (Vec::new(), Removals::default());
    for change in changes {
        if change.verb != Verb::Remove {
            shown.push(change);
            continue;
        }
        let id = id_of(&owned, change.section, &change.key).unwrap_or_default();
        match state.repo.usage_of(project_id, change.section, &id).await? {
            Ok(cost) => {
                match change.section {
                    "profiles" => removed.profiles.push(id),
                    "items" => removed.items.push(uuid_of(&id)?),
                    "roles" => removed.roles.push(uuid_of(&id)?),
                    "fields" => removed.fields.push(uuid_of(&id)?),
                    "sprints" => removed.sprints.push(uuid_of(&id)?),
                    _ => {}
                }
                removals.push(Removal {
                    section: change.section.to_owned(),
                    key: change.key.clone(),
                    name: change.name.clone(),
                    cost,
                });
                shown.push(change);
            }
            Err(reason) if change.section == "items" => {
                if let Some(item) = board
                    .items
                    .iter()
                    .find(|i| i.key.as_deref() == Some(change.key.as_str()))
                {
                    merged.items.push(without_board_keys(item));
                }
                shown.push(Change {
                    verb: Verb::Keep,
                    reason: Some(reason),
                    ..change
                });
            }
            Err(reason) => problems.push(format!(
                "{} \"{}\" stays: {reason}",
                change.section, change.name
            )),
        }
    }

    let mut plan = check(&merged, &owned.borrow()).map_err(refused)?;
    if !problems.is_empty() {
        return Err(ApiError::BadRequest(problems.join("; ")));
    }
    let warnings = std::mem::take(&mut plan.warnings);
    let applies = conflicts.is_empty() || query.prefer.is_some();
    let lines: Vec<String> = removals
        .iter()
        .map(|r| format!("{}/{}", r.section, r.key))
        .collect();
    let prefer = query.prefer.map_or("", |p| match p {
        Prefer::File => "file",
        Prefer::Board => "board",
    });
    let hash = body_hash(&format!(
        "{}\n{}\n{}\n{prefer}",
        body_text(&merged),
        body_hash(&board_text),
        lines.join("\n")
    ));
    Ok(Planned {
        project,
        owned,
        base,
        revert,
        board_text,
        merged,
        changes: shown,
        conflicts,
        removals,
        removed,
        warnings,
        plan,
        hash,
        applies,
    })
}

/// POST /api/projects/{id}/folder/plan
#[instrument(skip(state, body))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/folder/plan",
    tag = "folder",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        PlanQuery,
    ),
    request_body(content = Folder, description = "The project folder's files; none with `revision`"),
    responses(
        (status = 200, description = "What an apply would do; nothing is written", body = FolderPlan),
        (status = 400, description = "The folder was refused; the message lists every problem", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project or revision not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn plan_folder(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<PlanQuery>,
    RawBody(body): RawBody,
) -> ApiResult<Json<FolderPlan>> {
    let planned = planned(&state, project_id, &body, &query).await?;
    Ok(Json(FolderPlan {
        plan_hash: planned.hash,
        base: planned.base.map(Into::into),
        changes: planned.changes,
        conflicts: planned.conflicts,
        removals: planned.removals,
        warnings: planned.warnings,
        applies: planned.applies,
    }))
}

/// POST /api/projects/{id}/folder/apply
#[instrument(skip(state, body))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/folder/apply",
    tag = "folder",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        PlanQuery,
    ),
    request_body(content = Folder, description = "The project folder's files; none with `revision`"),
    responses(
        (status = 200, description = "The plan was written, in one transaction, and left a revision", body = FolderApplied),
        (status = 400, description = "The folder was refused, or a conflict has no side", body = crate::openapi::ErrorEnvelope),
        (status = 403, description = "An agent may preview the project folder; a person applies it", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project or revision not found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "The project changed since the preview", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn apply_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<PlanQuery>,
    RawBody(body): RawBody,
) -> ApiResult<Json<FolderApplied>> {
    if principal.kind != PrincipalKind::Operator {
        return Err(ApiError::Forbidden(
            "An agent may preview the project folder; a person applies it".into(),
        ));
    }
    let planned = planned(&state, project_id, &body, &query).await?;
    let Some(sent) = query.plan_hash.as_deref() else {
        return Err(ApiError::BadRequest(
            "an apply needs the plan_hash of its preview".into(),
        ));
    };
    if sent != planned.hash {
        return Err(ApiError::Conflict(
            "The project changed since the preview; preview again".into(),
        ));
    }
    if !planned.applies {
        return Err(ApiError::BadRequest(
            "the plan has a conflict; choose prefer=file or prefer=board".into(),
        ));
    }

    let Planned {
        project,
        owned,
        revert,
        board_text,
        merged,
        changes,
        removals,
        removed,
        plan,
        ..
    } = planned;
    let target = Target::Existing(project_id);
    let current = current_settings(&state, &target, &plan).await?;
    let mut write = to_write(target, &owned.borrow(), plan, ItemSource::Outline, current)?;
    write.update = merged.project.as_ref().map(|file| UpdateProject {
        name: file.name.clone().filter(|name| *name != project.name),
        description: file
            .description
            .clone()
            .filter(|text| project.description.as_ref() != Some(text)),
        vocabulary: file
            .vocabulary
            .clone()
            .filter(|words| *words != project.vocabulary),
        ..Default::default()
    });
    let gone = removed.items.clone();
    write.removals = removed;
    write.revisions = Some(RevisionWrite { board: board_text });
    let written = state.repo.write_project_file(write).await.map_err(failed)?;

    websocket::broadcast_event(&state, BoardEvent::BoardConfigUpdated { project_id });
    for item in &written.items {
        websocket::broadcast_event(
            &state,
            BoardEvent::ItemCreated {
                project_id,
                item_id: item.id,
                status: item.status.clone(),
            },
        );
    }
    for id in &written.updated {
        if let Some(item) = owned.items.iter().find(|item| item.id == *id) {
            websocket::broadcast_event(
                &state,
                BoardEvent::ItemUpdated {
                    project_id,
                    item_id: *id,
                    old_status: Some(item.status.clone()),
                    new_status: item.status.clone(),
                },
            );
        }
    }
    for item_id in gone {
        websocket::broadcast_event(
            &state,
            BoardEvent::ItemDeleted {
                project_id,
                item_id,
            },
        );
    }

    // What the apply wrote is kept as the board stored it, read back as an export reads it: the
    // folder's own text (a description's last newline, a type left to its default) would differ
    // from the board at once, and every later plan would see a change nobody made.
    let after = bundle_of(&state, &project, &visible_items(&state, project_id).await?).await?;
    let origin = if revert { "revert" } else { "file" };
    let revision = match state
        .repo
        .record_revision(project_id, origin, &body_text(&after))
        .await?
    {
        Some(row) => row,
        None => state
            .repo
            .last_revision(project_id)
            .await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("no revision after an apply")))?,
    };
    Ok(Json(FolderApplied {
        revision: revision.into(),
        changes,
        removals,
    }))
}
