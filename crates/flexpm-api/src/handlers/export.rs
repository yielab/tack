use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use uuid::Uuid;

use flexpm_core::models::ItemFilter;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "json".to_string()
}

/// GET /api/projects/:id/export
///
/// Export a complete project with all its data (JSON or CSV format).
#[instrument(skip(state))]
pub async fn export_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    info!(project_id = %project_id, format = %query.format, "Exporting project");

    // Get project
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    // Get all items
    let filter = ItemFilter {
        status: None,
        item_type: None,
        priority: None,
        sprint_id: None,
        parent_id: None,
        tag: None,
        search: None,
        page: None,
        per_page: None,
    };
    let items = state.repo.list_items(project_id, &filter).await?;

    // Get all sprints
    let sprints = state.repo.list_sprints(project_id).await?;

    match query.format.as_str() {
        "json" => {
            let export_data = serde_json::json!({
                "project": project,
                "items": items,
                "sprints": sprints,
                "metadata": {
                    "exported_at": chrono::Utc::now().to_rfc3339(),
                    "version": env!("CARGO_PKG_VERSION"),
                    "total_items": items.len(),
                    "total_sprints": sprints.len(),
                }
            });

            let json_data = serde_json::to_string_pretty(&export_data)
                .map_err(|e| anyhow::anyhow!("Failed to serialize: {}", e))?;

            let filename = format!("{}-export.json", project.name.replace(' ', "-"));

            Ok((
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
                ],
                json_data,
            ).into_response())
        }
        "csv" => {
            let mut csv_output = String::from("id,title,type,status,priority,parent_id,created_at\n");

            for item in items {
                csv_output.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    item.id,
                    item.title.replace(',', " "),
                    item.item_type,
                    item.status,
                    item.priority,
                    item.parent_id.map(|id| id.to_string()).unwrap_or_default(),
                    item.created_at.to_rfc3339(),
                ));
            }

            let filename = format!("{}-export.csv", project.name.replace(' ', "-"));

            Ok((
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/csv"),
                    (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
                ],
                csv_output,
            ).into_response())
        }
        _ => Err(ApiError::BadRequest(format!(
            "Unsupported format: {}. Use 'json' or 'csv'",
            query.format
        ))),
    }
}

/// POST /api/projects/import
///
/// Import a project from JSON export.
/// Note: This is a simplified implementation that validates the JSON structure.
/// For production use, consider enhancing with full relationship preservation.
#[instrument(skip(state))]
pub async fn import_project(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Import endpoint called");

    // Validate basic structure
    let _project = payload.get("project")
        .ok_or_else(|| ApiError::BadRequest("Missing 'project' field in import data".to_string()))?;

    let _items = payload.get("items")
        .ok_or_else(|| ApiError::BadRequest("Missing 'items' field in import data".to_string()))?;

    // For now, return success with instructions
    // Full implementation would create project + items with ID mapping
    Ok(Json(serde_json::json!({
        "success": false,
        "message": "Import validation passed, but full import not yet implemented",
        "note": "Export format is valid. To import: 1) Create project via POST /api/projects, 2) Create items via POST /api/projects/:id/items",
        "exported_data_summary": {
            "has_project": payload.get("project").is_some(),
            "has_items": payload.get("items").is_some(),
            "has_sprints": payload.get("sprints").is_some(),
        }
    })))
}
