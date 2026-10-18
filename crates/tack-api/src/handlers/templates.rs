use axum::response::IntoResponse;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use tack_core::models::*;
use tack_core::outline::bundle::{Parts, ProfileRow, to_bundle};
use tack_core::outline::{
    BundleAutoStart, BundleAutonomy, BundleBudgets, BundleDomain, BundleField, BundleStatus,
    BundleWorkflow, Outline, OutlineProject,
};
use tack_core::workflow::{StatusCategory, StatusDef};
use tack_db::repo;
use tack_db::repo::autonomy as store;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use crate::handlers::outline::create_from_bundle;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListTemplatesQuery {
    pub project_type: Option<ProjectType>,
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
        (status = 422, description = "Validation error (workflow shape, custom field options)", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_template(
    State(state): State<AppState>,
    Json(data): Json<CreateProjectTemplate>,
) -> ApiResult<Json<ProjectTemplate>> {
    data.validate()
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;

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

/// The actual "build a project from a template" work — extracted from the
/// route handler below so the project-creation path
/// (workflow/vocabulary/custom fields/boards) has exactly one
/// implementation. `pub(crate)`, not `pub`: not part of the public HTTP
/// surface.
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

    // The template is a project file without items: the same code an import
    // runs makes the project, its fields, profiles, roles and automation.
    let mut statuses: Vec<&StatusDef> = template.workflow.statuses.iter().collect();
    statuses.sort_by_key(|s| s.order);
    let outline = Outline {
        project: Some(OutlineProject {
            name: Some(data.name),
            project_type: Some(template.project_type.to_string()),
            description: data.description,
            vocabulary: Some(template.vocabulary.clone()),
            workflow: Some(BundleWorkflow {
                preset: None,
                statuses: statuses
                    .into_iter()
                    .map(|s| BundleStatus {
                        name: s.name.clone(),
                        category: match s.category {
                            StatusCategory::Todo => "todo",
                            StatusCategory::InProgress => "in_progress",
                            StatusCategory::Done => "done",
                        }
                        .to_string(),
                        wip_limit: s.wip_limit,
                    })
                    .collect(),
                transitions: template.workflow.transitions.clone(),
            }),
        }),
        fields: template
            .custom_fields
            .iter()
            .map(|f| BundleField {
                name: f.name.clone(),
                field_type: serde_json::to_value(&f.field_type)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                description: f.description.clone(),
                required: f.required,
                default: f.default_value.clone(),
                options: f.options.clone().unwrap_or_default(),
                validation: f.validation.clone(),
            })
            .collect(),
        profiles: template.profiles.clone(),
        roles: template.roles.clone(),
        sprints: Vec::new(),
        automation: template.automation.clone(),
        items: Vec::new(),
    };
    // The first workspace of the board (made when there is none) owns it.
    let workspace_id = get_or_create_default_workspace(state.pool()).await?;
    let scoped = AppState {
        workspace_id,
        ..state.clone()
    };
    let (project, _) = create_from_bundle(&scoped, outline, ItemSource::Outline)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, template_id = %template_id, "Failed to create project from template");
            e.into_response().status()
        })?;

    // A template keeps its workflow's type, which a file's statuses cannot say.
    state
        .repo
        .update_project(
            project.id,
            UpdateProject {
                workflow: Some(template.workflow.clone()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to apply template workflow");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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

    // The project's roles, the profiles they use and its automation settings,
    // written the way an export writes them.
    let internal = |e: sqlx::Error| {
        tracing::error!(error = %e, project_id = %project_id, "Failed to read project for template");
        StatusCode::INTERNAL_SERVER_ERROR
    };
    let profiles: Vec<ProfileRow> = state
        .repo
        .list_agent_profile_rows()
        .await
        .map_err(internal)?;
    let roles = state.repo.list_roles(project_id).await.map_err(internal)?;
    let sprints = state
        .repo
        .list_sprints(project_id)
        .await
        .map_err(internal)?;
    let fleets = state.repo.fleet_names().await.map_err(internal)?;
    let autonomy = store::get_settings(state.pool(), &project_id.to_string())
        .await
        .map_err(internal)?
        .map(|s| BundleAutonomy {
            level: Some(s.level.to_string()),
            domains: s
                .domains
                .into_iter()
                .map(|d| BundleDomain {
                    name: d.name,
                    paths: d.path_globs,
                    item_types: d.item_types,
                    level: d.level.to_string(),
                })
                .collect(),
            verify_require: Some(s.verify_require),
            verify_loop_max_attempts: Some(s.verify_loop_max_attempts),
            budgets: Some(BundleBudgets {
                tokens_per_day: s.budgets.tokens_per_day,
                usd_per_day: s.budgets.usd_per_day,
                auto_merges_per_day: s.budgets.auto_merges_per_day,
                queue_items_max: s.budgets.queue_items_max,
            }),
            merge_on_done: Some(s.merge_on_done),
        });
    let bundle = to_bundle(&Parts {
        project: &project,
        fields: &[],
        profiles: &profiles,
        roles: &roles,
        sprints: &sprints,
        autonomy,
        auto_start: BundleAutoStart::from_project(
            project.auto_start_enabled,
            project.auto_start_status.clone(),
            project.auto_start_max_open,
        ),
        items: &[],
        item_roles: &[],
        item_fields: &[],
        dependencies: &[],
        fleets: &fleets,
    });

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
        roles: bundle.roles,
        profiles: bundle.profiles,
        automation: bundle.automation,
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
