use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use flexpm_core::models::*;
use flexpm_db::repo;
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;

use crate::AppState;

/// POST /api/projects/:id/boards - Create a new board for a project
#[instrument(skip(state))]
pub async fn create_board(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(data): Json<CreateBoard>,
) -> Result<Json<Board>, StatusCode> {
    // Verify project exists
    repo::projects::get_project(state.pool(), project_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    repo::boards::create_board(state.pool(), project_id, data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to create board");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/projects/:id/boards - List all boards for a project
#[instrument(skip(state))]
pub async fn list_boards(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<Board>>, StatusCode> {
    repo::boards::list_boards(state.pool(), project_id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to list boards");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/boards/:id - Get a specific board
#[instrument(skip(state))]
pub async fn get_board(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Board>, StatusCode> {
    repo::boards::get_board(state.pool(), id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, board_id = %id, "Failed to get board");
            StatusCode::NOT_FOUND
        })
}

/// PATCH /api/boards/:id - Update a board
#[instrument(skip(state))]
pub async fn update_board(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(data): Json<UpdateBoard>,
) -> Result<Json<Board>, StatusCode> {
    repo::boards::update_board(state.pool(), id, data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, board_id = %id, "Failed to update board");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// DELETE /api/boards/:id - Delete a board
#[instrument(skip(state))]
pub async fn delete_board(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    repo::boards::delete_board(state.pool(), id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!(error = %e, board_id = %id, "Failed to delete board");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Serialize)]
pub struct BoardViewResponse {
    pub board: Board,
    pub columns: Vec<BoardColumnWithItems>,
}

#[derive(Debug, Serialize)]
pub struct BoardColumnWithItems {
    pub name: String,
    pub items: Vec<Item>,
    pub wip_limit: Option<usize>,
    pub wip_exceeded: bool,
}

/// GET /api/boards/:id/view - Get board state with items grouped and filtered
#[instrument(skip(state))]
pub async fn get_board_view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BoardViewResponse>, StatusCode> {
    // Get the board
    let board = repo::boards::get_board(state.pool(), id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, board_id = %id, "Board not found");
            StatusCode::NOT_FOUND
        })?;

    // Get the project to access workflow
    let project = repo::projects::get_project(state.pool(), board.project_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %board.project_id, "Project not found");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Get all items for the project
    let mut items = repo::items::list_items(state.pool(), board.project_id, Default::default())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %board.project_id, "Failed to fetch items");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Apply board filters if present
    if let Some(_filters) = &board.filters {
        // TODO: Apply filters to items
        // For now, we show all items
    }

    // Group items based on board grouping
    let columns = match &board.grouping {
        Some(BoardGrouping::Status) | None => {
            // Group by status (default Kanban view)
            group_by_status(items, &project.workflow)
        }
        Some(BoardGrouping::Priority) => {
            // Group by priority
            group_by_priority(items)
        }
        Some(BoardGrouping::ItemType) => {
            // Group by item type
            group_by_item_type(items)
        }
        Some(BoardGrouping::Sprint) => {
            // Group by sprint
            group_by_sprint(state.pool(), board.project_id, items).await?
        }
        Some(BoardGrouping::Assignee) => {
            // Group by assignee (not yet implemented)
            vec![]
        }
        Some(BoardGrouping::CustomField(field_id)) => {
            // Group by custom field
            group_by_custom_field(state.pool(), *field_id, items).await?
        }
    };

    Ok(Json(BoardViewResponse { board, columns }))
}

/// Helper: Group items by status
fn group_by_status(items: Vec<Item>, workflow: &WorkflowConfig) -> Vec<BoardColumnWithItems> {
    let mut columns = Vec::new();

    for status in &workflow.statuses {
        let status_items: Vec<Item> = items
            .iter()
            .filter(|item| item.status == status.name)
            .cloned()
            .collect();

        let wip_exceeded = if let Some(limit) = status.wip_limit {
            status_items.len() > limit
        } else {
            false
        };

        columns.push(BoardColumnWithItems {
            name: status.name.clone(),
            items: status_items,
            wip_limit: status.wip_limit,
            wip_exceeded,
        });
    }

    columns
}

/// Helper: Group items by priority
fn group_by_priority(items: Vec<Item>) -> Vec<BoardColumnWithItems> {
    let priorities = vec!["critical", "high", "medium", "low", "none"];
    let mut columns = Vec::new();

    for priority in priorities {
        let priority_items: Vec<Item> = items
            .iter()
            .filter(|item| item.priority.to_string() == priority)
            .cloned()
            .collect();

        columns.push(BoardColumnWithItems {
            name: priority.to_string(),
            items: priority_items,
            wip_limit: None,
            wip_exceeded: false,
        });
    }

    columns
}

/// Helper: Group items by item type
fn group_by_item_type(items: Vec<Item>) -> Vec<BoardColumnWithItems> {
    let types = vec!["epic", "feature", "task", "subtask", "bug", "requirement"];
    let mut columns = Vec::new();

    for item_type in types {
        let type_items: Vec<Item> = items
            .iter()
            .filter(|item| item.item_type.to_string() == item_type)
            .cloned()
            .collect();

        if !type_items.is_empty() {
            columns.push(BoardColumnWithItems {
                name: item_type.to_string(),
                items: type_items,
                wip_limit: None,
                wip_exceeded: false,
            });
        }
    }

    columns
}

/// Helper: Group items by sprint
async fn group_by_sprint(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
    items: Vec<Item>,
) -> Result<Vec<BoardColumnWithItems>, StatusCode> {
    let sprints = repo::sprints::list_sprints(pool, project_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch sprints");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut columns = Vec::new();

    // Add backlog (no sprint)
    let backlog_items: Vec<Item> = items
        .iter()
        .filter(|item| item.sprint_id.is_none())
        .cloned()
        .collect();

    columns.push(BoardColumnWithItems {
        name: "Backlog".to_string(),
        items: backlog_items,
        wip_limit: None,
        wip_exceeded: false,
    });

    // Add columns for each sprint
    for sprint in sprints {
        let sprint_items: Vec<Item> = items
            .iter()
            .filter(|item| item.sprint_id == Some(sprint.id))
            .cloned()
            .collect();

        columns.push(BoardColumnWithItems {
            name: sprint.name.clone(),
            items: sprint_items,
            wip_limit: None,
            wip_exceeded: false,
        });
    }

    Ok(columns)
}

/// Helper: Group items by custom field value
async fn group_by_custom_field(
    pool: &sqlx::SqlitePool,
    field_id: Uuid,
    items: Vec<Item>,
) -> Result<Vec<BoardColumnWithItems>, StatusCode> {
    // Get the field definition
    let _field = repo::custom_fields::get_field(pool, field_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, field_id = %field_id, "Custom field not found");
            StatusCode::NOT_FOUND
        })?;

    // Get all values for this field across all items
    let mut value_groups: std::collections::HashMap<String, Vec<Item>> = std::collections::HashMap::new();

    for item in items {
        if let Ok(field_value) = repo::custom_fields::get_field_value(pool, item.id, field_id).await {
            let value_str = field_value.value.to_string();
            value_groups.entry(value_str).or_default().push(item);
        } else {
            // Items without this field value go to "Unset"
            value_groups.entry("Unset".to_string()).or_default().push(item);
        }
    }

    // Convert to columns
    let columns: Vec<BoardColumnWithItems> = value_groups
        .into_iter()
        .map(|(name, items)| BoardColumnWithItems {
            name,
            items,
            wip_limit: None,
            wip_exceeded: false,
        })
        .collect();

    Ok(columns)
}
