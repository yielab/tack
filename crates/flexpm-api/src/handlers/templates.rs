use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use flexpm_core::models::*;
use flexpm_db::repo;
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ListTemplatesQuery {
    pub project_type: Option<ProjectType>,
}

/// POST /api/templates - Create a new project template
#[instrument(skip(state))]
pub async fn create_template(
    State(state): State<AppState>,
    Json(data): Json<CreateProjectTemplate>,
) -> Result<Json<ProjectTemplate>, StatusCode> {
    repo::templates::create_template(state.pool(), data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create template");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/templates - List all project templates
#[instrument(skip(state))]
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

#[derive(Debug, Deserialize)]
pub struct CreateProjectFromTemplate {
    pub name: String,
    pub description: Option<String>,
}

/// POST /api/projects/from-template/:id - Create a project from a template
#[instrument(skip(state))]
pub async fn create_project_from_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
    Json(data): Json<CreateProjectFromTemplate>,
) -> Result<Json<Project>, StatusCode> {
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

    let mut project = repo::projects::create_project(state.pool(), workspace_id, project_data)
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

    repo::projects::update_project(state.pool(), project.id, update_data)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update project with template config");
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
            grouping: board_template.grouping.as_ref().and_then(|g| parse_grouping_from_string(g)),
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
    repo::projects::get_project(state.pool(), project.id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch created project");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Helper: Get or create default workspace
async fn get_or_create_default_workspace(
    pool: &sqlx::SqlitePool,
) -> Result<Uuid, StatusCode> {
    // Try to get first workspace
    let existing = sqlx::query!("SELECT id FROM workspaces LIMIT 1")
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

        sqlx::query!(
            "INSERT INTO workspaces (id, name, description, default_vocabulary, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            id.to_string(),
            "Default Workspace",
            "Auto-created default workspace",
            "{}",
            now.to_rfc3339(),
            now.to_rfc3339()
        )
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
