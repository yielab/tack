//! Create a project and its items — or add items to a project — from one
//! outline file (JSON or YAML) or one CSV. Both formats become the same
//! [`Outline`] items, are checked whole by [`tack_core::outline::resolve`],
//! and are written in one transaction, so a refused or failed request leaves
//! nothing behind. `?dry_run=true` runs every check and reports what would be
//! created without writing.

use crate::extract::{Json, Path, Query, RawBody};
use std::collections::HashMap;

use axum::extract::State;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use uuid::Uuid;
use validator::Validate;

use serde_json::Value;
use tack_core::autonomy::{Domain, Level};
use tack_core::item_types::preset;
use tack_core::models::{
    CreateCustomField, CreateProject, CreateRole, CreateSprint, CustomFieldType, EstimateUnit,
    ItemSource, Project, ProjectType, UpdateCustomField, UpdateProject,
};
use tack_core::outline::bundle::{
    Board, Named, Plan, ProfileRow, SectionSummary, Upsert, check, workflow_of,
};
use tack_core::outline::missing::{Missing, missing};
use tack_core::outline::{
    BundleAutomation, BundleProfile, Outline, OutlineError, PlannedParent, items_from_csv,
    parse_due_date, parse_project_type,
};
use tack_core::vocabulary::vocabulary_for_type;
use tack_db::queries;
use tack_db::repo::autonomy::Settings;
use tack_db::repo::project_file::{
    FieldWrite, ProjectWrite, RoleWrite, SprintWrite, Target, WriteError,
};

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
    /// What the file expects that no computer reports: a team (with docket's command that makes
    /// it) or an agent (with how to install it). Nothing is created from it; empty for a CSV and
    /// for a file that names no team and no agent.
    pub missing: Vec<Missing>,
    /// The names of the item types the items' words added to the project, as work: a word the
    /// project's types lack is added rather than refused.
    pub new_types: Vec<String>,
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

/// What `outline` expects of teams and agents that no active runner reports.
async fn missing_of(state: &AppState, outline: &Outline) -> ApiResult<Vec<Missing>> {
    if outline.teams.is_empty() && outline.profiles.is_empty() && outline.automation.is_none() {
        return Ok(Vec::new());
    }
    let (mut teams, mut harnesses) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    for harness in crate::handlers::teams::reported_harnesses(state).await? {
        for team in harness.capability.teams.unwrap_or_default() {
            if !team.is_of_agent(&harness.kind) {
                teams.insert(team.name);
            }
        }
        harnesses.insert(harness.kind);
    }
    Ok(missing(outline, &teams, &harnesses))
}

fn refused(e: OutlineError) -> ApiError {
    ApiError::BadRequest(e.to_string())
}

fn read_outline(headers: &HeaderMap, body: &[u8]) -> ApiResult<Read> {
    let raw = structured_body(headers, body, "outline")?;
    let outline: Outline = serde_json::from_value(raw)
        .map_err(|e| ApiError::BadRequest(format!("Invalid outline: {e}")))?;
    Ok(Read {
        outline,
        skipped: 0,
        warnings: Vec::new(),
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
    RawBody(body): RawBody,
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
    RawBody(body): RawBody,
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
    RawBody(body): RawBody,
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
    RawBody(body): RawBody,
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
/// empty board, then written in the file's order, the project with it, in one
/// transaction. Templates come through here too.
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
        item_types: &preset(&spec.project_type),
        new_project: true,
    };
    let plan = check(&read.outline, &board).map_err(refused)?;
    let missing = missing_of(state, &read.outline).await?;
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
        let result = OutlineResult {
            missing,
            ..preview(plan, read, &source)
        };
        return Ok((None, result));
    }

    let target = Target::New {
        workspace_id: state.workspace_id,
        project: spec,
    };
    let (project_id, result) = write(state, target, &board, plan, read, source).await?;
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("no project made")))?;
    Ok((Some(project), OutlineResult { missing, ..result }))
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
    let owned = board_of(state, &project).await?;
    let board = owned.borrow();
    let plan = check(&read.outline, &board).map_err(refused)?;
    let missing = missing_of(state, &read.outline).await?;

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
        return Ok(Json(OutlineResult {
            missing,
            ..preview(plan, read, &source)
        }));
    }

    let (_, result) = write(
        state,
        Target::Existing(project_id),
        &board,
        plan,
        read,
        source,
    )
    .await?;
    let result = OutlineResult { missing, ..result };
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

/// What a project's file is checked against, owning what [`Board`] borrows.
pub(crate) struct OwnedBoard {
    workflow: tack_core::workflow::WorkflowConfig,
    fields: Vec<tack_core::models::CustomFieldDefinition>,
    profiles: Vec<ProfileRow>,
    roles: Vec<Named>,
    sprints: Vec<Named>,
    fleets: Vec<(String, String)>,
    level: Level,
    auto_start: bool,
    item_types: Vec<tack_core::item_types::ItemTypeDef>,
}

impl OwnedBoard {
    pub(crate) fn borrow(&self) -> Board<'_> {
        Board {
            workflow: &self.workflow,
            fields: &self.fields,
            profiles: &self.profiles,
            roles: &self.roles,
            sprints: &self.sprints,
            fleets: &self.fleets,
            harness_kinds: &HARNESS_KINDS,
            level: self.level,
            auto_start: self.auto_start,
            item_types: &self.item_types,
            new_project: false,
        }
    }
}

/// What a file is checked against for this project: what its board has today.
pub(crate) async fn board_of(state: &AppState, project: &Project) -> ApiResult<OwnedBoard> {
    let fields = state.repo.list_fields_for_project(project.id).await?;
    let roles = state
        .repo
        .list_roles(project.id)
        .await?
        .into_iter()
        .map(|r| Named {
            id: r.id.to_string(),
            key: r.key,
            name: r.name,
        })
        .collect();
    let sprints = state
        .repo
        .list_sprints(project.id)
        .await?
        .into_iter()
        .map(|s| Named {
            id: s.id.to_string(),
            key: s.key,
            name: s.name,
        })
        .collect();
    let profiles = state.repo.list_agent_profile_rows().await?;
    let fleets = state.repo.fleet_names().await?;
    let level = queries::autonomy::project_settings(&state.repo, &project.id.to_string())
        .await?
        .map_or(Level::L3, |s| s.level);
    Ok(OwnedBoard {
        workflow: project.workflow.clone(),
        fields,
        profiles,
        roles,
        sprints,
        fleets,
        level,
        auto_start: project.auto_start_enabled,
        item_types: project.item_types.clone(),
    })
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

fn uuid_of(id: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(id).map_err(|e| ApiError::Internal(anyhow::anyhow!("{id}: {e}")))
}

/// What a write that failed answers: the section the store failed in, or the board's own rule.
fn failed(failure: WriteError) -> ApiError {
    match failure {
        WriteError::Section { section, source } => ApiError::SectionFailed { section, source },
        WriteError::Refused(rule) => ApiError::Core(rule),
        WriteError::Store(source) => ApiError::Store(source),
    }
}

/// The file's profiles (made new or changed by name) as rows to write, and the map of every
/// profile name to its id, the file's first, the board's after.
fn profiles_of(
    board: &Board<'_>,
    rows: &[Upsert<BundleProfile>],
) -> (Vec<ProfileRow>, HashMap<String, String>) {
    let mut profile_ids: HashMap<String, String> = board
        .profiles
        .iter()
        .map(|p| (p.name.trim().to_string(), p.id.clone()))
        .collect();
    let mut written = Vec::with_capacity(rows.len());
    for row in rows {
        let (id, key, value, old) = match row {
            Upsert::Create { key, value } => {
                (format!("ap_{}", Uuid::new_v4()), key.clone(), value, None)
            }
            Upsert::Update { id, value } => {
                let old = board.profiles.iter().find(|p| &p.id == id);
                let key = old.map(|o| o.key.clone()).unwrap_or_default();
                (id.clone(), key, value, old)
            }
        };
        let empty = |v: &Value| v.as_object().is_some_and(serde_json::Map::is_empty);
        let kind = value
            .kind
            .as_deref()
            .map(|k| k.trim().to_string())
            .or_else(|| old.map(|o| o.kind.clone()))
            .unwrap_or_else(|| "custom".into());
        let profile = ProfileRow {
            id: id.clone(),
            key,
            name: value.name.trim().to_string(),
            kind: kind.clone(),
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
            egress_mode: value
                .egress_mode
                .as_deref()
                .map(|mode| mode.trim().to_owned())
                .or_else(|| old.map(|o| o.egress_mode.clone()))
                .unwrap_or_else(|| {
                    tack_protocol::execution::EgressMode::default_for_kind(&kind)
                        .as_str()
                        .to_owned()
                }),
            builtin: false,
        };
        profile_ids.insert(profile.name.clone(), id);
        written.push(profile);
    }
    (written, profile_ids)
}

/// The automation section: the project's own keys, and its autonomy settings overlaid on
/// `current` (the settings the project has today).
fn automation_of(
    auto: &BundleAutomation,
    profile_ids: &HashMap<String, String>,
    current: Settings,
) -> ApiResult<(UpdateProject, Option<Settings>)> {
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

    let Some(asked) = &auto.autonomy else {
        return Ok((update, None));
    };
    let mut settings = current;
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
    Ok((update, Some(settings)))
}

/// A checked plan as the one write the store makes, in the file's order: vocabulary and
/// workflow (a new project only), types, fields, profiles, roles, sprints, automation, then
/// the items with their roles and field values. `current` is the project's autonomy settings
/// today, which the file's overlay.
fn to_write(
    target: Target,
    board: &Board<'_>,
    mut plan: Plan,
    source: ItemSource,
    current: Settings,
) -> ApiResult<ProjectWrite> {
    let update = match &target {
        Target::New { project, .. } => {
            let mut vocabulary = vocabulary_for_type(&project.project_type);
            vocabulary.extend(plan.vocabulary.take().unwrap_or_default());
            Some(UpdateProject {
                vocabulary: Some(vocabulary),
                workflow: Some(board.workflow.clone()),
                ..Default::default()
            })
        }
        Target::Existing(_) => None,
    };

    let mut fields = Vec::with_capacity(plan.fields.len());
    for row in &plan.fields {
        fields.push(match row {
            Upsert::Create { key, value: f } => FieldWrite::Create(CreateCustomField {
                key: Some(key.clone()),
                name: f.name.trim().to_string(),
                field_type: parsed::<CustomFieldType>(&f.field_type)?,
                description: f.description.clone(),
                required: Some(f.required),
                default_value: f.default.clone(),
                options: (!f.options.is_empty()).then(|| f.options.clone()),
                validation: f.validation.clone(),
            }),
            Upsert::Update { id, value } => FieldWrite::Update {
                id: uuid_of(id)?,
                change: UpdateCustomField {
                    name: Some(value.name.trim().to_string()),
                    description: value.description.clone(),
                    required: Some(value.required),
                    default_value: value.default.clone(),
                    options: (!value.options.is_empty()).then(|| value.options.clone()),
                    validation: value.validation.clone(),
                },
            },
        });
    }

    let (profiles, profile_ids) = profiles_of(board, &plan.profiles);

    let mut roles = Vec::with_capacity(plan.roles.len());
    for row in &plan.roles {
        let (key, value, existing) = match row {
            Upsert::Create { key, value } => (Some(key.clone()), value, None),
            Upsert::Update { id, value } => (None, value, Some(uuid_of(id)?)),
        };
        roles.push(RoleWrite {
            existing,
            role: CreateRole {
                key,
                name: value.name.trim().to_string(),
                color: value.color.clone(),
                icon: value.icon.clone(),
            },
            profile: value
                .profile
                .as_deref()
                .and_then(|name| profile_ids.get(name.trim()))
                .cloned(),
        });
    }

    let mut sprints = Vec::with_capacity(plan.sprints.len());
    for row in &plan.sprints {
        let (key, value, existing) = match row {
            Upsert::Create { key, value } => (Some(key.clone()), value, None),
            Upsert::Update { id, value } => (None, value, Some(uuid_of(id)?)),
        };
        sprints.push(SprintWrite {
            existing,
            sprint: CreateSprint {
                key,
                name: value.name.trim().to_string(),
                goal: value.goal.clone(),
                start_date: value.start.as_deref().and_then(parse_due_date),
                end_date: value.end.as_deref().and_then(parse_due_date),
            },
            close: value.status.as_deref().map(str::trim) == Some("closed"),
        });
    }

    let (automation, autonomy) = match &plan.automation {
        Some(auto) => {
            let (update, autonomy) = automation_of(auto, &profile_ids, current)?;
            (Some(update), autonomy)
        }
        None => (None, None),
    };

    for planned in &mut plan.items.items {
        planned.input.estimate_unit = planned.estimate_unit.as_deref().map(unit_of);
    }
    let names = |rows: Vec<(String, Uuid)>| rows.into_iter().collect::<HashMap<_, _>>();
    Ok(ProjectWrite {
        target,
        update,
        types: plan.types,
        fields,
        profiles,
        roles,
        sprints,
        automation,
        autonomy,
        items: plan.items,
        source,
        field_ids: names(
            board
                .fields
                .iter()
                .map(|f| (f.name.trim().to_string(), f.id))
                .collect(),
        ),
        role_ids: names(
            board
                .roles
                .iter()
                .map(|r| Ok((r.name.trim().to_string(), uuid_of(&r.id)?)))
                .collect::<ApiResult<_>>()?,
        ),
        sprint_ids: names(
            board
                .sprints
                .iter()
                .map(|s| Ok((s.name.trim().to_string(), uuid_of(&s.id)?)))
                .collect::<ApiResult<_>>()?,
        ),
    })
}

/// Write a checked plan: one call to the store, which writes every section or none. The new
/// project's id (or the existing one's) comes back with what the answer reports.
async fn write(
    state: &AppState,
    target: Target,
    board: &Board<'_>,
    mut plan: Plan,
    read: Read,
    source: ItemSource,
) -> ApiResult<(Uuid, OutlineResult)> {
    let refs: Vec<Option<String>> = plan
        .items
        .items
        .iter()
        .map(|p| p.reference.clone())
        .collect();
    let sections = if matches!(source, ItemSource::CsvImport) {
        Vec::new()
    } else {
        std::mem::take(&mut plan.sections)
    };
    let mut warnings = read.warnings;
    warnings.extend(std::mem::take(&mut plan.warnings));
    let new_types = std::mem::take(&mut plan.new_types);
    let asks_autonomy = plan
        .automation
        .as_ref()
        .is_some_and(|auto| auto.autonomy.is_some());
    let current = match &target {
        Target::Existing(id) if asks_autonomy => {
            queries::autonomy::project_settings(&state.repo, &id.to_string())
                .await?
                .ok_or_else(|| ApiError::NotFound("Project not found".into()))?
        }
        _ => Settings::default(),
    };

    let written = state
        .repo
        .write_project_file(to_write(target, board, plan, source, current)?)
        .await
        .map_err(failed)?;
    let (project_id, items, dependencies) =
        (written.project_id, written.items, written.dependencies);
    info!(%project_id, created = items.len(), dependencies, "Outline written");
    Ok((
        project_id,
        OutlineResult {
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
            missing: Vec::new(),
            new_types,
            warnings,
        },
    ))
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
        missing: Vec::new(),
        new_types: plan.new_types,
        warnings,
    }
}
