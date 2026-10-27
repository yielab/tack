//! Create a project and its items — or add items to a project — from one
//! outline file (JSON or YAML) or one CSV. Both formats become the same
//! [`Outline`] items, are checked whole by [`tack_core::outline::resolve`],
//! and are written in one transaction, so a refused or failed request leaves
//! nothing behind. `?dry_run=true` runs every check and reports what would be
//! created without writing.

use std::collections::HashMap;
use tack_protocol::{Clock, SystemClock};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use uuid::Uuid;
use validator::Validate;

use serde_json::Value;
use tack_core::autonomy::{Domain, Level};
use tack_core::models::{
    CreateCustomField, CreateProject, CreateRole, CreateSprint, CustomFieldType, EstimateUnit,
    ItemSource, Project, ProjectType, SprintStatus, UpdateCustomField, UpdateProject, UpdateRole,
    UpdateSprint,
};
use tack_core::outline::bundle::{
    Board, Plan, ProfileRow, SectionSummary, Upsert, check, workflow_of,
};
use tack_core::outline::{
    BundleAutomation, BundleProfile, Outline, OutlineError, PlannedParent, items_from_csv,
    parse_due_date, parse_project_type,
};
use tack_core::vocabulary::vocabulary_for_type;
use tack_db::StoreError;
use tack_db::queries;

use crate::error::{ApiError, ApiResult};
use crate::handlers::export::structured_body;
use crate::handlers::runner_admin::HARNESS_KINDS;
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
    /// What each section of a project file does, in the file's order; empty for a CSV.
    pub sections: Vec<SectionSummary>,
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

/// A file read from a request body, before it is checked.
struct Read {
    outline: Outline,
    skipped: usize,
    warnings: Vec<String>,
}

fn refused(e: OutlineError) -> ApiError {
    ApiError::BadRequest(e.to_string())
}

fn read_outline(headers: &HeaderMap, body: &[u8]) -> ApiResult<Read> {
    let raw = structured_body(headers, body, "outline")?;
    let outline: Outline = serde_json::from_value(raw)
        .map_err(|e| ApiError::BadRequest(format!("Invalid outline: {e}")))?;
    let mut warnings = Vec::new();
    if outline
        .automation
        .as_ref()
        .and_then(|auto| auto.autonomy.as_ref())
        .and_then(|autonomy| autonomy.budgets.as_ref())
        .is_some_and(|budgets| budgets.usd_per_day.is_some())
    {
        warnings.push("Dollars per day is no longer a limit; this value is ignored.".to_owned());
    }
    Ok(Read {
        outline,
        skipped: 0,
        warnings,
    })
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
    let text = std::str::from_utf8(body).map_err(|failure| {
        ApiError::BadRequest(format!("CSV body is not valid UTF-8: {failure}"))
    })?;
    let csv = items_from_csv(text).map_err(refused)?;
    Ok(Read {
        outline: Outline {
            items: csv.items,
            ..Default::default()
        },
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
    let read = read_outline(&headers, &body)?;
    let project = new_project(&query, read.outline.project.clone().unwrap_or_default())?;
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
    request_body(content = Outline, description = "An outline as JSON, or YAML when Content-Type mentions yaml; its project block is read for nothing but a warning"),
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
    let read = read_outline(&headers, &body)?;
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
    let (project, mut result) = create_new(state, spec, read, source, dry_run).await?;
    result.project = project;
    Ok(Json(result))
}

/// A new project from a project file: the file is checked whole against an
/// empty board, then written in the file's order. Any failure deletes the
/// project again. Templates come through here too.
pub(crate) async fn create_from_bundle(
    state: &AppState,
    outline: Outline,
    source: ItemSource,
) -> Result<(Project, OutlineResult), ApiError> {
    let spec = new_project(
        &NewProjectOutlineQuery::default(),
        outline.project.clone().unwrap_or_default(),
    )?;
    let read = Read {
        outline,
        skipped: 0,
        warnings: Vec::new(),
    };
    let (project, result) = create_new(state, spec, read, source, false).await?;
    let project = project.ok_or_else(|| ApiError::Internal(anyhow::anyhow!("no project made")))?;
    Ok((project, result))
}

async fn create_new(
    state: &AppState,
    spec: CreateProject,
    read: Read,
    source: ItemSource,
    dry_run: bool,
) -> ApiResult<(Option<Project>, OutlineResult)> {
    let workflow =
        workflow_of(read.outline.project.as_ref(), &spec.project_type).map_err(refused)?;
    let profiles = state.repo.list_agent_profile_rows().await?;
    let fleets = state.repo.fleet_names().await?;
    let board = Board {
        workflow: &workflow,
        fields: &[],
        profiles: &profiles,
        roles: &[],
        sprints: &[],
        fleets: &fleets,
        harness_kinds: &HARNESS_KINDS,
        level: Level::L3,
        auto_start: false,
    };
    let plan = check(&read.outline, &board).map_err(refused)?;
    let strangers: Vec<String> = plan
        .items
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
        return Ok((None, preview(plan, read, &source)));
    }

    let project = state.repo.create_project(state.workspace_id, spec).await?;
    match write(state, &project, true, &board, plan, read, source).await {
        Ok(result) => {
            let project = state.repo.get_project(project.id).await?.unwrap_or(project);
            Ok((Some(project), result))
        }
        Err(e) => {
            // Items are one transaction; the rows of the other sections go
            // with the project.
            let _ = state.repo.delete_project(project.id).await;
            Err(match e {
                ApiError::SectionFailed {
                    section, source, ..
                } => ApiError::SectionFailed {
                    section,
                    kept: false,
                    source,
                },
                other => other,
            })
        }
    }
}

async fn into_existing_project(
    state: &AppState,
    project_id: Uuid,
    mut read: Read,
    source: ItemSource,
    dry_run: bool,
) -> ApiResult<Json<OutlineResult>> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    if read.outline.project.take().is_some() {
        read.warnings.push(
            "the project's name, workflow and vocabulary are kept; change them under Settings"
                .into(),
        );
    }
    let fields = state.repo.list_fields_for_project(project_id).await?;
    let roles: Vec<(Uuid, String)> = state
        .repo
        .list_roles(project_id)
        .await?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();
    let sprints: Vec<(Uuid, String)> = state
        .repo
        .list_sprints(project_id)
        .await?
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    let profiles = state.repo.list_agent_profile_rows().await?;
    let fleets = state.repo.fleet_names().await?;
    let level = tack_db::queries::autonomy::project_settings(&state.repo, &project_id.to_string())
        .await?
        .map_or(Level::L3, |s| s.level);
    let board = Board {
        workflow: &project.workflow,
        fields: &fields,
        profiles: &profiles,
        roles: &roles,
        sprints: &sprints,
        fleets: &fleets,
        harness_kinds: &HARNESS_KINDS,
        level,
        auto_start: project.auto_start_enabled,
    };
    let plan = check(&read.outline, &board).map_err(refused)?;

    let mut belongs: HashMap<Uuid, bool> = HashMap::new();
    let mut strangers = Vec::new();
    for planned in &plan.items.items {
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
        return Ok(Json(preview(plan, read, &source)));
    }

    let result = write(state, &project, false, &board, plan, read, source).await?;
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

/// The unit word of an item's estimate, as the data model holds it.
fn unit_of(word: &str) -> EstimateUnit {
    match word.trim() {
        "story_points" => EstimateUnit::StoryPoints,
        "hours" => EstimateUnit::Hours,
        "days" => EstimateUnit::Days,
        other => EstimateUnit::Custom(other.to_string()),
    }
}

fn parsed<T: serde::de::DeserializeOwned>(word: &str) -> ApiResult<T> {
    serde_json::from_value(Value::String(word.trim().to_string()))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("\"{word}\": {e}")))
}

/// Names the section a failed store call belonged to; what the file wrote before it stays.
fn at<E: Into<StoreError>>(section: &'static str) -> impl Fn(E) -> ApiError {
    move |source| ApiError::SectionFailed {
        section,
        kept: true,
        source: source.into(),
    }
}

fn uuid_of(id: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| ApiError::Internal(anyhow::anyhow!("{id}: {e}")))
}

/// Write the file's profiles (made new or changed by name); the map of every profile name to its id,
/// the file's first, the board's after.
async fn write_profiles(
    state: &AppState,
    board: &Board<'_>,
    rows: &[Upsert<BundleProfile>],
) -> ApiResult<HashMap<String, String>> {
    let repo = &state.repo;
    let mut profile_ids: HashMap<String, String> = board
        .profiles
        .iter()
        .map(|p| (p.name.trim().to_string(), p.id.clone()))
        .collect();
    for row in rows {
        let (id, value, old) = match row {
            Upsert::Create(value) => (format!("ap_{}", Uuid::new_v4()), value, None),
            Upsert::Update { id, value } => (
                id.clone(),
                value,
                board.profiles.iter().find(|p| &p.id == id),
            ),
        };
        let empty = |v: &Value| v.as_object().is_some_and(serde_json::Map::is_empty);
        let profile = ProfileRow {
            id: id.clone(),
            name: value.name.trim().to_string(),
            kind: value
                .kind
                .as_deref()
                .map(|k| k.trim().to_string())
                .or_else(|| old.map(|o| o.kind.clone()))
                .unwrap_or_else(|| "custom".into()),
            instructions: match old {
                Some(o) if value.instructions.is_empty() => o.instructions.clone(),
                _ => value.instructions.clone(),
            },
            tool_policy: match old {
                Some(o) if empty(&value.tool_policy) => o.tool_policy.clone(),
                _ => value.tool_policy.clone(),
            },
            limits: match old {
                Some(o) if empty(&value.limits) => o.limits.clone(),
                _ => value.limits.clone(),
            },
            fleet_id: value
                .fleet
                .as_deref()
                .and_then(|name| {
                    board
                        .fleets
                        .iter()
                        .find(|(_, n)| n.trim() == name.trim())
                        .map(|(id, _)| id.clone())
                })
                .or_else(|| old.and_then(|o| o.fleet_id.clone())),
            delivers: value
                .delivers
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_owned)
                .or_else(|| old.and_then(|o| o.delivers.clone())),
            builtin: false,
        };
        repo.write_agent_profile(&profile, SystemClock.now())
            .await
            .map_err(at("profiles"))?;
        profile_ids.insert(profile.name.clone(), id);
    }
    Ok(profile_ids)
}

/// Write the automation section: the project's own keys, then its autonomy settings overlaid on today's.
async fn write_automation(
    state: &AppState,
    project_id: Uuid,
    auto: &BundleAutomation,
    profile_ids: &HashMap<String, String>,
) -> ApiResult<()> {
    let repo = &state.repo;
    let mut update = UpdateProject {
        repository: auto.repository.clone().map(Some),
        default_branch: auto.default_branch.clone().map(Some),
        prepare_command: auto.prepare.clone().map(Some),
        push_after_run: auto.push_after_run,
        default_harness: auto
            .default_harness
            .clone()
            .map(|h| Some(h.trim().to_string())),
        default_profile_id: auto
            .default_profile
            .as_deref()
            .and_then(|name| profile_ids.get(name.trim()))
            .cloned()
            .map(Some),
        default_model: auto.default_model.clone().map(Some),
        on_finish_status: auto.on_finish_status.clone().map(Some),
        definition_of_done: auto.definition_of_done.clone().map(Some),
        ..Default::default()
    };
    if let Some(origin) = &auto.code_origin {
        update.code_origin = Some(parsed(origin)?);
    }
    if let Some(mode) = &auto.workspace_mode {
        update.workspace_mode = Some(parsed(mode)?);
    }
    if let Some(start) = &auto.auto_start {
        update.auto_start_enabled = start.enabled;
        update.auto_start_status = start.status.clone().map(Some);
        update.auto_start_max_open = start.max_open;
    }
    repo.update_project(project_id, update)
        .await
        .map_err(at("automation"))?;

    if let Some(asked) = &auto.autonomy {
        let id = project_id.to_string();
        let mut settings = queries::autonomy::project_settings(repo, &id)
            .await
            .map_err(at("automation"))?
            .ok_or_else(|| ApiError::NotFound("Project not found".into()))?;
        if let Some(level) = &asked.level {
            settings.level = level.parse().map_err(ApiError::BadRequest)?;
        }
        if !asked.domains.is_empty() {
            settings.domains = asked
                .domains
                .iter()
                .map(|d| {
                    Ok(Domain {
                        name: d.name.trim().to_string(),
                        path_globs: d.paths.clone(),
                        item_types: d.item_types.clone(),
                        level: d.level.parse().map_err(ApiError::BadRequest)?,
                    })
                })
                .collect::<ApiResult<_>>()?;
        }
        if let Some(require) = &asked.verify_require {
            settings.verify_require = require.clone();
        }
        if let Some(attempts) = asked.verify_loop_max_attempts {
            settings.verify_loop_max_attempts = attempts;
        }
        if let Some(budgets) = &asked.budgets {
            let now = &mut settings.budgets;
            now.tokens_per_day = budgets.tokens_per_day.or(now.tokens_per_day);
            now.auto_merges_per_day = budgets.auto_merges_per_day.or(now.auto_merges_per_day);
            now.queue_items_max = budgets.queue_items_max.or(now.queue_items_max);
        }
        if let Some(merge) = asked.merge_on_done {
            settings.merge_on_done = merge;
        }
        repo.put_autonomy_settings(&id, &settings)
            .await
            .map_err(at("automation"))?;
    }
    Ok(())
}

/// Write a checked plan in the file's order: vocabulary and workflow (a new
/// project only), fields, profiles, roles, sprints, automation, then the
/// items with their roles and field values. An error names its section.
async fn write(
    state: &AppState,
    project: &Project,
    is_new: bool,
    board: &Board<'_>,
    mut plan: Plan,
    read: Read,
    source: ItemSource,
) -> ApiResult<OutlineResult> {
    let repo = &state.repo;
    let project_id = project.id;

    if is_new {
        let mut vocabulary = vocabulary_for_type(&project.project_type);
        vocabulary.extend(plan.vocabulary.take().unwrap_or_default());
        repo.update_project(
            project_id,
            UpdateProject {
                vocabulary: Some(vocabulary),
                workflow: Some(board.workflow.clone()),
                ..Default::default()
            },
        )
        .await
        .map_err(at("project"))?;
    }

    let mut field_ids: HashMap<String, Uuid> = board
        .fields
        .iter()
        .map(|f| (f.name.trim().to_string(), f.id))
        .collect();
    for row in &plan.fields {
        match row {
            Upsert::Create(f) => {
                let created = repo
                    .create_field(
                        project_id,
                        CreateCustomField {
                            name: f.name.trim().to_string(),
                            field_type: parsed::<CustomFieldType>(&f.field_type)?,
                            description: f.description.clone(),
                            required: Some(f.required),
                            default_value: f.default.clone(),
                            options: (!f.options.is_empty()).then(|| f.options.clone()),
                            validation: f.validation.clone(),
                        },
                    )
                    .await
                    .map_err(at("fields"))?;
                field_ids.insert(created.name.trim().to_string(), created.id);
            }
            Upsert::Update { id, value } => {
                repo.update_field(
                    uuid_of(id)?,
                    UpdateCustomField {
                        name: None,
                        description: value.description.clone(),
                        required: Some(value.required),
                        default_value: value.default.clone(),
                        options: (!value.options.is_empty()).then(|| value.options.clone()),
                        validation: value.validation.clone(),
                    },
                )
                .await
                .map_err(at("fields"))?;
            }
        }
    }

    let profile_ids = write_profiles(state, board, &plan.profiles).await?;
    let mut role_ids: HashMap<String, Uuid> = board
        .roles
        .iter()
        .map(|(id, n)| (n.trim().to_string(), *id))
        .collect();
    for row in &plan.roles {
        let (value, existing) = match row {
            Upsert::Create(value) => (value, None),
            Upsert::Update { id, value } => (value, Some(uuid_of(id)?)),
        };
        let profile = value
            .profile
            .as_deref()
            .and_then(|name| profile_ids.get(name.trim()))
            .cloned();
        let id = match existing {
            Some(id) => {
                repo.update_role(
                    id,
                    UpdateRole {
                        name: None,
                        color: value.color.clone(),
                        icon: value.icon.clone(),
                        agent_profile_id: profile.map(Some),
                    },
                )
                .await
                .map_err(at("roles"))?;
                id
            }
            None => {
                let role = repo
                    .create_role(
                        project_id,
                        CreateRole {
                            name: value.name.trim().to_string(),
                            color: value.color.clone(),
                            icon: value.icon.clone(),
                        },
                    )
                    .await
                    .map_err(at("roles"))?;
                if profile.is_some() {
                    repo.update_role(
                        role.id,
                        UpdateRole {
                            name: None,
                            color: None,
                            icon: None,
                            agent_profile_id: Some(profile),
                        },
                    )
                    .await
                    .map_err(at("roles"))?;
                }
                role.id
            }
        };
        role_ids.insert(value.name.trim().to_string(), id);
    }

    let mut sprint_ids: HashMap<String, Uuid> = board
        .sprints
        .iter()
        .map(|(id, n)| (n.trim().to_string(), *id))
        .collect();
    for row in &plan.sprints {
        let (value, existing) = match row {
            Upsert::Create(value) => (value, None),
            Upsert::Update { id, value } => (value, Some(uuid_of(id)?)),
        };
        let start = value.start.as_deref().and_then(parse_due_date);
        let end = value.end.as_deref().and_then(parse_due_date);
        let id = match existing {
            Some(id) => {
                let old = repo.get_sprint(id).await.map_err(at("sprints"))?;
                repo.update_sprint(
                    id,
                    UpdateSprint {
                        name: value.name.trim().to_string(),
                        goal: value
                            .goal
                            .clone()
                            .or_else(|| old.as_ref().and_then(|o| o.goal.clone())),
                        start_date: start.or_else(|| old.as_ref().and_then(|o| o.start_date)),
                        end_date: end.or_else(|| old.as_ref().and_then(|o| o.end_date)),
                    },
                )
                .await
                .map_err(at("sprints"))?;
                id
            }
            None => {
                repo.create_sprint(
                    project_id,
                    CreateSprint {
                        name: value.name.trim().to_string(),
                        goal: value.goal.clone(),
                        start_date: start,
                        end_date: end,
                    },
                )
                .await
                .map_err(at("sprints"))?
                .id
            }
        };
        if value.status.as_deref().map(str::trim) == Some("closed") {
            repo.update_sprint_status(id, SprintStatus::Closed)
                .await
                .map_err(at("sprints"))?;
        }
        sprint_ids.insert(value.name.trim().to_string(), id);
    }

    if let Some(auto) = &plan.automation {
        write_automation(state, project_id, auto, &profile_ids).await?;
    }
    let mut extras = Vec::with_capacity(plan.items.items.len());
    for planned in &mut plan.items.items {
        if let Some(sprint) = planned.sprint.as_deref() {
            planned.input.sprint_id = sprint_ids.get(sprint.trim()).copied();
        }
        planned.input.estimate_unit = planned.estimate_unit.as_deref().map(unit_of);
        extras.push((planned.roles.clone(), planned.fields.clone()));
    }
    let refs: Vec<Option<String>> = plan
        .items
        .items
        .iter()
        .map(|p| p.reference.clone())
        .collect();
    let sections = if matches!(source, ItemSource::CsvImport) {
        Vec::new()
    } else {
        plan.sections
    };
    let mut warnings = read.warnings;
    warnings.extend(plan.warnings);
    let (items, dependencies) = repo
        .create_outline_items(project_id, plan.items, source)
        .await
        .map_err(at("items"))?;
    for (item, (roles, fields)) in items.iter().zip(extras) {
        for role in roles.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
            if let Some(&role_id) = role_ids.get(role) {
                repo.assign_role_to_item(item.id, role_id)
                    .await
                    .map_err(at("roles"))?;
            }
        }
        for (name, value) in fields {
            if let Some(&field_id) = field_ids.get(name.trim()) {
                repo.set_field_value(item.id, field_id, value)
                    .await
                    .map_err(at("fields"))?;
            }
        }
    }
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
        sections,
        warnings,
    })
}

fn preview(plan: Plan, read: Read, source: &ItemSource) -> OutlineResult {
    let mut warnings = read.warnings;
    warnings.extend(plan.warnings);
    let items = plan
        .items
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
        created: plan.items.items.len(),
        skipped: read.skipped,
        dependencies: plan.items.blocks.len(),
        items,
        sections: if matches!(source, ItemSource::CsvImport) {
            Vec::new()
        } else {
            plan.sections
        },
        warnings,
    }
}
