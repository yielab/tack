use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use flexpm_core::models::{Item, ItemFilter, UpdateProject};

use crate::error::ApiError;
use crate::router::AppState;
use crate::handlers::websocket::{self, BoardEvent};

/// Board state response with items grouped by status
#[derive(Debug, Serialize)]
pub struct BoardState {
    pub project_id: Uuid,
    pub columns: Vec<BoardColumn>,
    pub total_items: usize,
}

/// A single board column with its items
#[derive(Debug, Serialize)]
pub struct BoardColumn {
    pub status: String,
    pub category: String,
    pub wip_limit: Option<usize>,
    pub order: i32,
    pub items: Vec<Item>,
    pub item_count: usize,
    pub wip_exceeded: bool,
}

/// Request to update board configuration
#[derive(Debug, Deserialize)]
pub struct UpdateBoardConfig {
    pub columns: Option<Vec<BoardColumnConfig>>,
}

#[derive(Debug, Deserialize)]
pub struct BoardColumnConfig {
    pub status: String,
    pub wip_limit: Option<usize>,
}

/// GET /api/projects/:id/board
///
/// Returns the current board state with items grouped by status column.
/// Items are ordered by sort_order within each column.
#[instrument(skip(state))]
pub async fn get_board(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<BoardState>, ApiError> {
    info!(project_id = %project_id, "Fetching board state");

    // Get the project to access workflow configuration
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    // Get all items for the project
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

    let all_items = state
        .repo
        .list_items(project_id, &filter)
        .await?;

    debug!(item_count = all_items.len(), "Fetched all items for board");

    // Group items by status
    let mut items_by_status: HashMap<String, Vec<Item>> = HashMap::new();
    for item in all_items.iter() {
        items_by_status
            .entry(item.status.clone())
            .or_default()
            .push(item.clone());
    }

    // Build columns from workflow configuration
    let mut columns: Vec<BoardColumn> = project
        .workflow
        .statuses
        .iter()
        .map(|status_def| {
            let mut items = items_by_status
                .get(&status_def.name)
                .cloned()
                .unwrap_or_default();

            // Sort items by sort_order
            items.sort_by_key(|item| item.sort_order);

            let item_count = items.len();
            let wip_exceeded = if let Some(limit) = status_def.wip_limit {
                item_count >= limit
            } else {
                false
            };

            BoardColumn {
                status: status_def.name.clone(),
                category: format!("{:?}", status_def.category).to_lowercase(),
                wip_limit: status_def.wip_limit,
                order: status_def.order,
                items,
                item_count,
                wip_exceeded,
            }
        })
        .collect();

    // Ensure columns are sorted by order
    columns.sort_by_key(|col| col.order);

    let total_items = all_items.len();

    info!(
        project_id = %project_id,
        columns = columns.len(),
        total_items,
        "Board state retrieved successfully"
    );

    Ok(Json(BoardState {
        project_id,
        columns,
        total_items,
    }))
}

/// PATCH /api/projects/:id/board
///
/// Update board configuration (WIP limits, column settings).
/// Note: Column status names and order are managed via project workflow configuration.
#[instrument(skip(state))]
pub async fn update_board_config(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<UpdateBoardConfig>,
) -> Result<StatusCode, ApiError> {
    info!(project_id = %project_id, "Updating board configuration");

    // Get current project
    let mut project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    // Update WIP limits if provided
    if let Some(column_configs) = payload.columns {
        for config in column_configs {
            if let Some(status) = project
                .workflow
                .statuses
                .iter_mut()
                .find(|s| s.name == config.status)
            {
                status.wip_limit = config.wip_limit;
                debug!(
                    status = %config.status,
                    wip_limit = ?config.wip_limit,
                    "Updated WIP limit for column"
                );
            } else {
                return Err(ApiError::BadRequest(format!(
                    "Status '{}' not found in workflow",
                    config.status
                )));
            }
        }

        // Update project with new workflow using UpdateProject struct
        let update = UpdateProject {
            name: None,
            description: None,
            vocabulary: None,
            workflow: Some(project.workflow.clone()),
            archived: None,
        };

        state
            .repo
            .update_project(project_id, update)
            .await?;

        // Broadcast WebSocket event
        websocket::broadcast_event(&state, BoardEvent::BoardConfigUpdated {
            project_id,
        });

        info!(project_id = %project_id, "Board configuration updated successfully");
    }

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_column_wip_exceeded() {
        let column = BoardColumn {
            status: "In Progress".to_string(),
            category: "in_progress".to_string(),
            wip_limit: Some(3),
            order: 0,
            items: vec![],
            item_count: 5,
            wip_exceeded: true,
        };

        assert!(column.wip_exceeded);
        assert_eq!(column.wip_limit, Some(3));
    }
}
