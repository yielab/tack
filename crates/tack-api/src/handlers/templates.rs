use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use tack_core::models::*;
use tack_core::workflow::WorkflowConfig;
use tack_db::repo;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListTemplatesQuery {
    pub project_type: Option<ProjectType>,
}

/// Validate a template's `orchestration` block before it's stored (Phase 37,
/// card D3, task 37.1 — TODO.md's "validate, don't trust" requirement).
/// `workflow` must be the workflow *this template will actually create*
/// (`create_template` resolves the same `data.workflow` /
/// `simple_workflow()` fallback `repo::templates::create_template` uses,
/// before calling here — see that call site), not any live project's
/// workflow: a template's `status_map` describes transitions for boards this
/// template will produce in the future, which may not exist yet.
///
/// Two checks, deliberately not three:
///
/// 1. **`status_map`** — every named status must exist in `workflow`. Reuses
///    `handlers::orch::validate_status_map` (card A4's `orch-link` validator,
///    TODO.md §1.3) via a field-for-field conversion into `orch::StatusMap`,
///    rather than a second copy of the same three lines of logic — TODO.md's
///    explicit instruction for this card, precedent: A4 built exactly this
///    for `PUT /orch-link` already.
/// 2. **`pipeline_yaml`**, if inline text is supplied, must at least parse as
///    YAML. This is deliberately *not* a check against docket's pipeline
///    schema (step ids, gate/rework edges, variable-name rules, duplicate-id
///    detection, …) — that real validator is `docket pipeline validate`
///    (`core.pipeline.validate_pipeline` in
///    `~/Sites/rack-cli/src/docket/core/pipeline.py`), reachable only as a
///    local CLI subcommand as of 2026-08-05: `serve.py` has no HTTP route
///    for it (verified by reading every `do_GET`/`do_POST` branch), and
///    shelling out to a local `docket` binary from `tack-api` would be
///    wrong even if one were installed — Tack's server talks to every
///    control plane over HTTP (`tack-orch::ControlPlane`), never a local
///    process, and a control plane is not necessarily on the same host as
///    the API server. Reimplementing docket's schema in Rust instead is
///    exactly the class of mistake TODO.md already paid down once (B2's
///    client-side cursor reimplementation, undone by R1) — a second,
///    hand-maintained copy of docket's `PipelineSpec` would drift the first
///    time docket adds a step kind or a gate variant.
///
///    So this is the one check Tack *can* make honestly without drifting:
///    "is this text YAML at all" — not "is this a valid docket pipeline."
///    The gap is recorded upstream: `~/Sites/rack-cli/ROADMAP.md` Phase 22,
///    new card **P22-8 — `pipeline validate` over HTTP** (see TODO.md §6
///    "D3" for the full writeup). Once that route exists, this function
///    should call it instead of `serde_yaml`'s bare parse.
fn validate_template_orchestration(
    orch: &TemplateOrchestration,
    workflow: &WorkflowConfig,
) -> ApiResult<()> {
    let status_map = crate::handlers::orch::StatusMap {
        dispatch_from: orch.status_map.dispatch_from.clone(),
        on_running: orch.status_map.on_running.clone(),
        on_waiting_approval: orch.status_map.on_waiting_approval.clone(),
        on_succeeded: orch.status_map.on_succeeded.clone(),
        on_failed: orch.status_map.on_failed.clone(),
        on_cancelled: orch.status_map.on_cancelled.clone(),
    };
    crate::handlers::orch::validate_status_map(&status_map, workflow)?;

    if let Some(yaml) = &orch.pipeline_yaml {
        serde_yaml::from_str::<serde_yaml::Value>(yaml).map_err(|e| {
            ApiError::BadRequest(format!(
                "orchestration.pipeline_yaml is not valid YAML: {e} \
                 (note: this only checks it parses as YAML, not that it is a \
                 valid docket pipeline — docket has no HTTP endpoint for that \
                 check yet; see TODO.md §6 \"D3\")"
            ))
        })?;
    }

    Ok(())
}

/// POST /api/templates - Create a new project template
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/templates",
    tag = "templates",
    request_body = tack_core::models::CreateProjectTemplate,
    responses(
        (status = 200, description = "Template created", body = tack_core::models::ProjectTemplate),
        (status = 400, description = "orchestration validation error (unknown status_map name, or invalid pipeline_yaml)", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "Validation error (workflow shape, custom field options)", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_template(
    State(state): State<AppState>,
    Json(data): Json<CreateProjectTemplate>,
) -> ApiResult<Json<ProjectTemplate>> {
    // Validate workflow shape if provided
    if let Some(ref wf) = data.workflow {
        wf.validate().map_err(|e| {
            tracing::warn!(error = %e, "Template workflow validation failed");
            ApiError::Unprocessable(format!("workflow: {e}"))
        })?;
    }

    // Validate custom field options: Select/MultiSelect require at least one option
    if let Some(ref fields) = data.custom_fields {
        for f in fields {
            if matches!(
                f.field_type,
                CustomFieldType::Select | CustomFieldType::MultiSelect
            ) {
                let has_options = f.options.as_ref().map(|o| !o.is_empty()).unwrap_or(false);
                if !has_options {
                    tracing::warn!(field_name = %f.name, "Select field missing options");
                    return Err(ApiError::Unprocessable(format!(
                        "custom_fields: {:?} is a select field but has no options",
                        f.name
                    )));
                }
            }
        }
    }

    // Validate the orchestration block, if present, against the workflow
    // *this template will actually create* — mirroring
    // `repo::templates::create_template`'s own `data.workflow.unwrap_or_else
    // (simple_workflow)` fallback so the two never resolve to a different
    // "effective" workflow (TODO.md §6 "D3": "validate the map against the
    // workflow the template will actually create, not against whatever
    // project happens to be applying it").
    if let Some(ref orch) = data.orchestration {
        let effective_workflow = data
            .workflow
            .clone()
            .unwrap_or_else(tack_core::workflow::simple_workflow);
        validate_template_orchestration(orch, &effective_workflow)?;
    }

    Ok(Json(
        repo::templates::create_template(state.pool(), data).await?,
    ))
}

/// GET /api/templates - List all project templates
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/templates",
    tag = "templates",
    params(
        ListTemplatesQuery,
    ),
    responses(
        (status = 200, description = "Templates (optionally filtered by project type)", body = Vec<tack_core::models::ProjectTemplate>),
    ),
)]
pub async fn list_templates(
    State(state): State<AppState>,
    Query(params): Query<ListTemplatesQuery>,
) -> Result<Json<Vec<ProjectTemplate>>, StatusCode> {
    repo::templates::list_templates(state.pool(), params.project_type)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list templates");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/templates/:id - Get a specific template
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/templates/{id}",
    tag = "templates",
    params(
        ("id" = Uuid, Path, description = "Template ID"),
    ),
    responses(
        (status = 200, description = "The template", body = tack_core::models::ProjectTemplate),
        (status = 404, description = "Template not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProjectTemplate>, StatusCode> {
    repo::templates::get_template(state.pool(), id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, template_id = %id, "Failed to get template");
            StatusCode::NOT_FOUND
        })
}

/// DELETE /api/templates/:id - Delete a template (user-created only)
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/templates/{id}",
    tag = "templates",
    params(
        ("id" = Uuid, Path, description = "Template ID"),
    ),
    responses(
        (status = 204, description = "Template deleted"),
    ),
)]
pub async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    repo::templates::delete_template(state.pool(), id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!(error = %e, template_id = %id, "Failed to delete template");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct CreateProjectFromTemplate {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 2_000, message = "description too long (max 2 000 chars)"))]
    pub description: Option<String>,
}

/// The actual "build a project from a template" work — extracted (card D4,
/// 2026-08-05) so `handlers::provisioning::create_project_with_pod` can
/// reuse the exact same project-creation path (workflow/vocabulary/custom
/// fields/boards) rather than a second, driftable copy of it, before going
/// on to provision a pod and write the `orch_links` row. `pub(crate)`, not
/// `pub` — this is an internal seam between two handler modules, not part
/// of the public HTTP surface.
///
/// **`template.orchestration` is still inert here** — this function only
/// ever creates the Tack project itself. Turning `orchestration` into a
/// live `orch_links` row needs a `control_plane_id` pointing at one
/// specific, already-registered docket instance, which callers of *this*
/// function may not have yet (the plain `POST /api/projects/from-template/
/// {id}` endpoint below never does); `handlers::provisioning` is the one
/// caller that reads `template.orchestration` for its own defaults, after
/// this function returns, not by changing anything in here.
pub(crate) async fn build_project_from_template(
    state: &AppState,
    template_id: Uuid,
    data: CreateProjectFromTemplate,
) -> Result<Project, StatusCode> {
    data.validate()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;

    // Get the template
    let template = repo::templates::get_template(state.pool(), template_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, template_id = %template_id, "Template not found");
            StatusCode::NOT_FOUND
        })?;

    // Get default workspace (or create one if needed)
    let workspace_id = get_or_create_default_workspace(state.pool()).await?;

    // Create project with template configuration
    let project_data = CreateProject {
        name: data.name,
        description: data.description,
        project_type: template.project_type,
        template: None, // Already applied
    };

    let mut project = state
        .repo
        .create_project(workspace_id, project_data)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create project from template");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Apply template vocabulary and workflow
    project.vocabulary = template.vocabulary.clone();
    project.workflow = template.workflow.clone();

    // Update project with template config
    let update_data = UpdateProject {
        name: None,
        description: None,
        vocabulary: Some(template.vocabulary.clone()),
        workflow: Some(template.workflow.clone()),
        archived: None,
    };

    state
        .repo
        .update_project(project.id, update_data)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update project with template config");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::error!("Failed to find project after update");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Create custom fields from template
    for field_def in &template.custom_fields {
        let field_data = CreateCustomField {
            name: field_def.name.clone(),
            field_type: field_def.field_type.clone(),
            description: field_def.description.clone(),
            required: Some(field_def.required),
            default_value: field_def.default_value.clone(),
            options: field_def.options.clone(),
            validation: field_def.validation.clone(),
        };

        repo::custom_fields::create_field(state.pool(), project.id, field_data)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, field_name = %field_def.name, "Failed to create custom field from template");
                // Continue even if some fields fail
            })
            .ok();
    }

    // Create boards from template
    for (idx, board_template) in template.default_boards.iter().enumerate() {
        let board_data = CreateBoard {
            name: board_template.name.clone(),
            description: board_template.description.clone(),
            filters: board_template.filters.clone(),
            grouping: board_template
                .grouping
                .as_ref()
                .and_then(|g| parse_grouping_from_string(g)),
            is_default: Some(idx == 0), // First board is default
        };

        repo::boards::create_board(state.pool(), project.id, board_data)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, board_name = %board_template.name, "Failed to create board from template");
                // Continue even if some boards fail
            })
            .ok();
    }

    // Return the final project
    state
        .repo
        .get_project(project.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch created project");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::error!("Failed to find project after creation");
            StatusCode::NOT_FOUND
        })
}

/// POST /api/projects/from-template/:id - Create a project from a template
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/from-template/{id}",
    tag = "templates",
    params(
        ("id" = Uuid, Path, description = "Template ID"),
    ),
    request_body = CreateProjectFromTemplate,
    responses(
        (status = 200, description = "Project created from template", body = tack_core::models::Project),
        (status = 404, description = "Template not found", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_project_from_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
    Json(data): Json<CreateProjectFromTemplate>,
) -> Result<Json<Project>, StatusCode> {
    build_project_from_template(&state, template_id, data)
        .await
        .map(Json)
}

// ─── Save project as template ────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SaveAsTemplateRequest {
    pub name: String,
    pub description: Option<String>,
}

/// POST /api/projects/:id/save-as-template — Snapshot a project's configuration as a reusable template
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/save-as-template",
    tag = "templates",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    request_body = SaveAsTemplateRequest,
    responses(
        (status = 200, description = "Template snapshot created", body = tack_core::models::ProjectTemplate),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn save_project_as_template(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(data): Json<SaveAsTemplateRequest>,
) -> Result<Json<ProjectTemplate>, StatusCode> {
    let project = state
        .repo
        .get_project(project_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to get project");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Snapshot custom field definitions (strip per-project ids so they become template-level)
    let field_defs = repo::custom_fields::list_fields_for_project(state.pool(), project_id)
        .await
        .unwrap_or_default();

    let now = chrono::Utc::now();
    let custom_fields: Vec<CustomFieldDefinition> = field_defs
        .into_iter()
        .map(|f| CustomFieldDefinition {
            id: Uuid::new_v4(),
            project_id: None,
            name: f.name,
            field_type: f.field_type,
            description: f.description,
            required: f.required,
            default_value: f.default_value,
            options: f.options,
            validation: f.validation,
            created_at: now,
            updated_at: now,
        })
        .collect();

    // Snapshot boards — columns are derived from workflow statuses
    let boards = repo::boards::list_boards(state.pool(), project_id)
        .await
        .unwrap_or_default();

    let status_names: Vec<String> = project
        .workflow
        .statuses
        .iter()
        .map(|s| s.name.clone())
        .collect();

    let default_boards: Vec<BoardTemplate> = boards
        .into_iter()
        .map(|b| BoardTemplate {
            name: b.name,
            description: b.description,
            columns: status_names
                .iter()
                .map(|s| BoardColumn {
                    status: s.clone(),
                    wip_limit: None,
                    collapsed: false,
                })
                .collect(),
            filters: b.filters,
            grouping: b.grouping.map(grouping_to_string),
        })
        .collect();

    let template_data = CreateProjectTemplate {
        name: data.name,
        description: data.description,
        project_type: project.project_type,
        vocabulary: Some(project.vocabulary),
        workflow: Some(project.workflow),
        custom_fields: if custom_fields.is_empty() {
            None
        } else {
            Some(custom_fields)
        },
        default_boards: if default_boards.is_empty() {
            None
        } else {
            Some(default_boards)
        },
        // Deliberately not derived from the source project's live
        // `orch_link`. Unlike vocabulary/workflow —
        // which describe *this* project's shape and transfer cleanly to any
        // future project created from the resulting template —
        // `orch_links.control_plane_id`/`remote_project` point at one
        // specific, already-registered docket instance and one specific
        // remote project string. Copying them into a template would make
        // every *future* project created from it silently point at
        // someone else's docket pod the moment orchestration is wired up.
        // A template's orchestration block can only be set explicitly, via
        // `POST /api/templates`.
        orchestration: None,
    };

    repo::templates::create_template(state.pool(), template_data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to save project as template");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn grouping_to_string(g: BoardGrouping) -> String {
    match g {
        BoardGrouping::Status => "status".to_string(),
        BoardGrouping::Priority => "priority".to_string(),
        BoardGrouping::ItemType => "item_type".to_string(),
        BoardGrouping::Sprint => "sprint".to_string(),
        BoardGrouping::Assignee => "assignee".to_string(),
        BoardGrouping::CustomField(id) => format!("custom_field:{id}"),
    }
}

/// Helper: Get or create default workspace
async fn get_or_create_default_workspace(pool: &sqlx::SqlitePool) -> Result<Uuid, StatusCode> {
    // Try to get first workspace
    #[derive(sqlx::FromRow)]
    struct WorkspaceId {
        id: String,
    }

    let existing = sqlx::query_as::<_, WorkspaceId>("SELECT id FROM workspaces LIMIT 1")
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to query workspaces");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some(row) = existing {
        Ok(Uuid::parse_str(&row.id).unwrap())
    } else {
        // Create default workspace
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO workspaces (id, name, description, default_vocabulary, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind("Default Workspace")
        .bind("Auto-created default workspace")
        .bind("{}")
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create default workspace");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(id)
    }
}

/// Helper: Parse grouping from string (for templates)
fn parse_grouping_from_string(s: &str) -> Option<BoardGrouping> {
    match s {
        "status" => Some(BoardGrouping::Status),
        "priority" => Some(BoardGrouping::Priority),
        "item_type" => Some(BoardGrouping::ItemType),
        "sprint" => Some(BoardGrouping::Sprint),
        "assignee" => Some(BoardGrouping::Assignee),
        _ if s.starts_with("custom_field:") => {
            let field_id_str = s.trim_start_matches("custom_field:");
            Uuid::parse_str(field_id_str)
                .ok()
                .map(BoardGrouping::CustomField)
        }
        _ => None,
    }
}
