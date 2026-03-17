use flexpm_core::models::*;
use sqlx::SqlitePool;
use tracing::instrument;
use uuid::Uuid;

/// Create a new board for a project
#[instrument(skip(pool))]
pub async fn create_board(
    pool: &SqlitePool,
    project_id: Uuid,
    data: CreateBoard,
) -> Result<Board, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let filters = data.filters.map(|f| f.to_string());
    let grouping = data.grouping.map(|g| match g {
        BoardGrouping::Status => "status".to_string(),
        BoardGrouping::Priority => "priority".to_string(),
        BoardGrouping::ItemType => "item_type".to_string(),
        BoardGrouping::Sprint => "sprint".to_string(),
        BoardGrouping::Assignee => "assignee".to_string(),
        BoardGrouping::CustomField(field_id) => format!("custom_field:{}", field_id),
    });

    // If this is marked as default, unset other defaults first
    if data.is_default.unwrap_or(false) {
        sqlx::query!(
            "UPDATE boards SET is_default = 0 WHERE project_id = ?",
            project_id.to_string()
        )
        .execute(pool)
        .await?;
    }

    sqlx::query(
        "INSERT INTO boards
         (id, project_id, name, description, filters, grouping, is_default, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id.to_string())
    .bind(project_id.to_string())
    .bind(&data.name)
    .bind(&data.description)
    .bind(filters)
    .bind(grouping)
    .bind(data.is_default.unwrap_or(false) as i32)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;

    get_board(pool, id).await
}

/// Get a board by ID
#[instrument(skip(pool))]
pub async fn get_board(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Board, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, project_id, name, description, filters, grouping, is_default, created_at, updated_at
         FROM boards
         WHERE id = ?",
        id.to_string()
    )
    .fetch_one(pool)
    .await?;

    let filters = row.filters.and_then(|f| serde_json::from_str(&f).ok());
    let grouping = row.grouping.and_then(|g| parse_grouping(&g));

    Ok(Board {
        id: Uuid::parse_str(&row.id).unwrap(),
        project_id: Uuid::parse_str(&row.project_id).unwrap(),
        name: row.name,
        description: row.description,
        filters,
        grouping,
        is_default: row.is_default != 0,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
    })
}

/// Get default board for a project
#[instrument(skip(pool))]
pub async fn get_default_board(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<Option<Board>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id, project_id, name, description, filters, grouping, is_default, created_at, updated_at
         FROM boards
         WHERE project_id = ? AND is_default = 1
         LIMIT 1",
        project_id.to_string()
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let filters = row.filters.and_then(|f| serde_json::from_str(&f).ok());
        let grouping = row.grouping.and_then(|g| parse_grouping(&g));

        Ok(Some(Board {
            id: Uuid::parse_str(&row.id).unwrap(),
            project_id: Uuid::parse_str(&row.project_id).unwrap(),
            name: row.name,
            description: row.description,
            filters,
            grouping,
            is_default: row.is_default != 0,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
        }))
    } else {
        Ok(None)
    }
}

/// List all boards for a project
#[instrument(skip(pool))]
pub async fn list_boards(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<Vec<Board>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, project_id, name, description, filters, grouping, is_default, created_at, updated_at
         FROM boards
         WHERE project_id = ?
         ORDER BY is_default DESC, name ASC",
        project_id.to_string()
    )
    .fetch_all(pool)
    .await?;

    let boards = rows.into_iter().map(|row| {
        let filters = row.filters.and_then(|f| serde_json::from_str(&f).ok());
        let grouping = row.grouping.and_then(|g| parse_grouping(&g));

        Board {
            id: Uuid::parse_str(&row.id).unwrap(),
            project_id: Uuid::parse_str(&row.project_id).unwrap(),
            name: row.name,
            description: row.description,
            filters,
            grouping,
            is_default: row.is_default != 0,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
        }
    }).collect();

    Ok(boards)
}

/// Update a board
#[instrument(skip(pool))]
pub async fn update_board(
    pool: &SqlitePool,
    id: Uuid,
    data: UpdateBoard,
) -> Result<Board, sqlx::Error> {
    let now = chrono::Utc::now();

    // If setting as default, unset other defaults for this project first
    if data.is_default == Some(true) {
        let board = get_board(pool, id).await?;
        sqlx::query!(
            "UPDATE boards SET is_default = 0 WHERE project_id = ? AND id != ?",
            board.project_id.to_string(),
            id.to_string()
        )
        .execute(pool)
        .await?;
    }

    if let Some(name) = &data.name {
        sqlx::query!("UPDATE boards SET name = ?, updated_at = ? WHERE id = ?",
            name, now.to_rfc3339(), id.to_string())
            .execute(pool).await?;
    }

    if let Some(description) = &data.description {
        sqlx::query!("UPDATE boards SET description = ?, updated_at = ? WHERE id = ?",
            description, now.to_rfc3339(), id.to_string())
            .execute(pool).await?;
    }

    if let Some(filters) = &data.filters {
        let filters_str = filters.to_string();
        sqlx::query!("UPDATE boards SET filters = ?, updated_at = ? WHERE id = ?",
            filters_str, now.to_rfc3339(), id.to_string())
            .execute(pool).await?;
    }

    if let Some(grouping) = &data.grouping {
        let grouping_str = match grouping {
            BoardGrouping::Status => "status".to_string(),
            BoardGrouping::Priority => "priority".to_string(),
            BoardGrouping::ItemType => "item_type".to_string(),
            BoardGrouping::Sprint => "sprint".to_string(),
            BoardGrouping::Assignee => "assignee".to_string(),
            BoardGrouping::CustomField(field_id) => format!("custom_field:{}", field_id),
        };
        sqlx::query!("UPDATE boards SET grouping = ?, updated_at = ? WHERE id = ?",
            grouping_str, now.to_rfc3339(), id.to_string())
            .execute(pool).await?;
    }

    if let Some(is_default) = data.is_default {
        sqlx::query!("UPDATE boards SET is_default = ?, updated_at = ? WHERE id = ?",
            is_default as i32, now.to_rfc3339(), id.to_string())
            .execute(pool).await?;
    }

    get_board(pool, id).await
}

/// Delete a board
#[instrument(skip(pool))]
pub async fn delete_board(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM boards WHERE id = ?", id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Helper to parse grouping string
fn parse_grouping(s: &str) -> Option<BoardGrouping> {
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
