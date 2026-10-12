//! Create a project and its items — or add items to a project — from one
//! outline file (JSON or YAML) or one CSV. Both formats become the same
//! [`Outline`] items, are checked whole by [`tack_core::outline::resolve`],
//! and are written in one transaction, so a refused or failed request leaves
//! nothing behind. `?dry_run=true` runs every check and reports what would be
//! created without writing.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateProject, ItemSource, Project, ProjectType};
use tack_core::outline::{
    Outline, OutlineError, OutlineItem, PlannedParent, ResolvedOutline, items_from_csv,
    parse_project_type, resolve,
};
use tack_core::workflow::workflow_for_type;

use crate::error::{ApiError, ApiResult};
use crate::handlers::export::structured_body;
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AddOutlineQuery {
    /// Check the file and report what would be created, writing nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NewProjectOutlineQuery {
    /// Check the file and report what would be created, writing nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// The new project's name. Overrides an outline's `project.name`;
    /// required for a CSV, which has no project of its own.
    pub name: Option<String>,
    /// The new project's type (default `custom`). Overrides an outline's
    /// `project.project_type`.
    pub project_type: Option<String>,
    /// Overrides an outline's `project.description`.
    pub description: Option<String>,
}

/// What an outline or CSV import created, or would create on a dry run.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OutlineResult {
    pub dry_run: bool,
    /// The project created; absent when adding to a project and on a dry run.
    pub project: Option<Project>,
    /// Items created (on a dry run, that would be created).
    pub created: usize,
    /// CSV rows with no title, which create nothing.
    pub skipped: usize,
    /// Dependencies created between the new items.
    pub dependencies: usize,
    /// The items in written order: parents before their children.
    pub items: Vec<OutlineResultItem>,
    /// What was read but not used as written: an unknown status or priority,
    /// a column with no field, a project block an add ignores.
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OutlineResultItem {
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    /// Absent on a dry run.
    pub id: Option<Uuid>,
    pub title: String,
    pub status: String,
    pub parent_id: Option<Uuid>,
}

/// Items read from a request body, before they are checked.
struct Read {
    items: Vec<OutlineItem>,
    skipped: usize,
    warnings: Vec<String>,
}

fn refused(e: OutlineError) -> ApiError {
    ApiError::BadRequest(e.to_string())
}

fn read_outline(headers: &HeaderMap, body: &[u8]) -> ApiResult<(Outline, Read)> {
    let raw = structured_body(headers, body, "outline")?;
    let mut outline: Outline = serde_json::from_value(raw)
        .map_err(|e| ApiError::BadRequest(format!("Invalid outline: {e}")))?;
    let items = std::mem::take(&mut outline.items);
    Ok((
        outline,
        Read {
            items,
            skipped: 0,
            warnings: Vec::new(),
        },
    ))
}

fn read_csv(headers: &HeaderMap, body: &[u8]) -> ApiResult<Read> {
    let media_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if media_type != "text/csv" {
        return Err(ApiError::UnsupportedMediaType(
            "CSV must be sent as text/csv".into(),
        ));
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| ApiError::BadRequest("CSV body is not valid UTF-8".into()))?;
    let csv = items_from_csv(text).map_err(refused)?;
    Ok(Read {
        items: csv.items,
        skipped: csv.skipped,
        warnings: csv.warnings,
    })
}

#[instrument(skip(state, headers, body))]
#[utoipa::path(
    post,
    path = "/api/projects/outline",
    tag = "import",
    params(NewProjectOutlineQuery),
    request_body(content = Outline, description = "An outline as JSON, or YAML when Content-Type mentions yaml"),
    responses(
        (status = 200, description = "The project and items created, or that would be on a dry run", body = OutlineResult),
        (status = 400, description = "The outline was refused; the message lists every problem", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_project_from_outline(
    State(state): State<AppState>,
    Query(query): Query<NewProjectOutlineQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<OutlineResult>> {
    let (outline, read) = read_outline(&headers, &body)?;
    let project = new_project(&query, outline.project.unwrap_or_default())?;
    into_new_project(&state, project, read, ItemSource::Outline, query.dry_run).await
}

#[instrument(skip(state, headers, body))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/outline",
    tag = "import",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        AddOutlineQuery,
    ),
    request_body(content = Outline, description = "An outline as JSON, or YAML when Content-Type mentions yaml; its project block is ignored"),
    responses(
        (status = 200, description = "The items created, or that would be on a dry run", body = OutlineResult),
        (status = 400, description = "The outline was refused; the message lists every problem", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn add_outline_to_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<AddOutlineQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<OutlineResult>> {
    let (outline, mut read) = read_outline(&headers, &body)?;
    if outline.project.is_some() {
        read.warnings
            .push("the project block is ignored when adding to an existing project".into());
    }
    into_existing_project(&state, project_id, read, ItemSource::Outline, query.dry_run).await
}

#[instrument(skip(state, headers, body))]
#[utoipa::path(
    post,
    path = "/api/projects/import-csv",
    tag = "import",
    params(NewProjectOutlineQuery),
    request_body(content = String, content_type = "text/csv", description = "CSV with a header row; `title` column required"),
    responses(
        (status = 200, description = "The project and items created, or that would be on a dry run", body = OutlineResult),
        (status = 400, description = "The CSV was refused; the message lists every problem", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_project_from_csv(
    State(state): State<AppState>,
    Query(query): Query<NewProjectOutlineQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<OutlineResult>> {
    let read = read_csv(&headers, &body)?;
    let project = new_project(&query, Default::default())?;
    into_new_project(&state, project, read, ItemSource::CsvImport, query.dry_run).await
}

/// `ItemSource::CsvImport` — a bare CSV carries no provenance of its own, so
/// every row is untrusted for dispatch purposes, same as the GitHub/Linear
/// importers.
#[instrument(skip(state, headers, body))]
#[utoipa::path(
    post,
    path = "/api/projects/{id}/import-csv",
    tag = "import",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        AddOutlineQuery,
    ),
    request_body(content = String, content_type = "text/csv", description = "CSV with a header row; `title` column required"),
    responses(
        (status = 200, description = "The items created, or that would be on a dry run", body = OutlineResult),
        (status = 400, description = "The CSV was refused; the message lists every problem", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn import_csv(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<AddOutlineQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<OutlineResult>> {
    let read = read_csv(&headers, &body)?;
    into_existing_project(
        &state,
        project_id,
        read,
        ItemSource::CsvImport,
        query.dry_run,
    )
    .await
}

/// The new project, from the query first and the outline's project block
/// second.
fn new_project(
    query: &NewProjectOutlineQuery,
    from_file: tack_core::outline::OutlineProject,
) -> ApiResult<CreateProject> {
    let pick = |q: &Option<String>, f: Option<String>| {
        q.clone()
            .or(f)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let name = pick(&query.name, from_file.name).ok_or_else(|| {
        ApiError::BadRequest(
            "a new project needs a name: set project.name in the outline or pass ?name=".into(),
        )
    })?;
    let project_type = match pick(&query.project_type, from_file.project_type) {
        Some(text) => parse_project_type(&text).map_err(ApiError::BadRequest)?,
        None => ProjectType::Custom,
    };
    let project = CreateProject {
        name,
        description: pick(&query.description, from_file.description),
        project_type,
        template: None,
        code_origin: None,
        repository: None,
        default_branch: None,
        workspace_mode: None,
    };
    project
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(project)
}

async fn into_new_project(
    state: &AppState,
    spec: CreateProject,
    read: Read,
    source: ItemSource,
    dry_run: bool,
) -> ApiResult<Json<OutlineResult>> {
    // `create_project` gives the project exactly this workflow, so the
    // outline is checked once, before the project exists.
    let resolved = resolve(&read.items, &workflow_for_type(&spec.project_type)).map_err(refused)?;
    let strangers: Vec<String> = resolved
        .items
        .iter()
        .filter_map(|p| match p.parent {
            Some(PlannedParent::Existing(id)) => Some(not_in_project(&p.input.title, id)),
            _ => None,
        })
        .collect();
    if !strangers.is_empty() {
        return Err(refused(OutlineError {
            problems: strangers,
        }));
    }
    if dry_run {
        return Ok(Json(preview(resolved, read)));
    }

    let project = state.repo.create_project(state.workspace_id, spec).await?;
    match write(state, project.id, resolved, read, source).await {
        Ok(mut result) => {
            result.project = Some(project);
            Ok(Json(result))
        }
        Err(e) => {
            // The transaction wrote nothing; the project row is the only
            // thing to take back.
            let _ = state.repo.delete_project(project.id).await;
            Err(e)
        }
    }
}

async fn into_existing_project(
    state: &AppState,
    project_id: Uuid,
    read: Read,
    source: ItemSource,
    dry_run: bool,
) -> ApiResult<Json<OutlineResult>> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    let resolved = resolve(&read.items, &project.workflow).map_err(refused)?;

    let mut belongs: HashMap<Uuid, bool> = HashMap::new();
    let mut strangers = Vec::new();
    for planned in &resolved.items {
        let Some(PlannedParent::Existing(id)) = planned.parent else {
            continue;
        };
        let ours = match belongs.get(&id) {
            Some(&ours) => ours,
            None => {
                let ours = state
                    .repo
                    .get_item(id)
                    .await?
                    .is_some_and(|item| item.project_id == project_id);
                belongs.insert(id, ours);
                ours
            }
        };
        if !ours {
            strangers.push(not_in_project(&planned.input.title, id));
        }
    }
    if !strangers.is_empty() {
        return Err(refused(OutlineError {
            problems: strangers,
        }));
    }
    if dry_run {
        return Ok(Json(preview(resolved, read)));
    }

    let result = write(state, project_id, resolved, read, source).await?;
    for item in &result.items {
        if let Some(item_id) = item.id {
            websocket::broadcast_event(
                state,
                BoardEvent::ItemCreated {
                    project_id,
                    item_id,
                    status: item.status.clone(),
                },
            );
        }
    }
    Ok(Json(result))
}

fn not_in_project(title: &str, id: Uuid) -> String {
    format!("\"{title}\" names parent {id}, which is not an item of this project")
}

async fn write(
    state: &AppState,
    project_id: Uuid,
    mut resolved: ResolvedOutline,
    read: Read,
    source: ItemSource,
) -> ApiResult<OutlineResult> {
    let refs: Vec<Option<String>> = resolved.items.iter().map(|p| p.reference.clone()).collect();
    let mut warnings = read.warnings;
    warnings.append(&mut resolved.warnings);
    let (items, dependencies) = state
        .repo
        .create_outline_items(project_id, resolved, source)
        .await?;
    info!(%project_id, created = items.len(), dependencies, "Outline written");
    Ok(OutlineResult {
        dry_run: false,
        project: None,
        created: items.len(),
        skipped: read.skipped,
        dependencies,
        items: items
            .into_iter()
            .zip(refs)
            .map(|(item, reference)| OutlineResultItem {
                reference,
                id: Some(item.id),
                title: item.title,
                status: item.status,
                parent_id: item.parent_id,
            })
            .collect(),
        warnings,
    })
}

fn preview(resolved: ResolvedOutline, read: Read) -> OutlineResult {
    let mut warnings = read.warnings;
    warnings.extend(resolved.warnings);
    let items = resolved
        .items
        .iter()
        .map(|p| OutlineResultItem {
            reference: p.reference.clone(),
            id: None,
            title: p.input.title.clone(),
            status: p.status.clone(),
            parent_id: match p.parent {
                Some(PlannedParent::Existing(id)) => Some(id),
                _ => None,
            },
        })
        .collect();
    OutlineResult {
        dry_run: true,
        project: None,
        created: resolved.items.len(),
        skipped: read.skipped,
        dependencies: resolved.blocks.len(),
        items,
        warnings,
    }
}
