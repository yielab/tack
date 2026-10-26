use crate::{Repository, StoreError};

/// A task waiting in a status, as auto-start picks from them.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WaitingItem {
    pub id: String,
    pub priority: String,
    pub sort_order: i32,
    pub version: i64,
}

/// The tasks of the project in `status` that have at least one role.
pub async fn waiting_with_a_role(
    repo: &Repository,
    project_id: &str,
    status: &str,
) -> Result<Vec<WaitingItem>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT id, priority, sort_order, version FROM items
         WHERE project_id = ? AND status = ?
           AND EXISTS (SELECT 1 FROM item_roles ir WHERE ir.item_id = items.id)",
    )
    .bind(project_id)
    .bind(status)
    .fetch_all(repo.pool())
    .await?)
}

/// The id and status of one task.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ItemStatus {
    pub id: String,
    pub status: String,
}

/// The status of every task of the project.
pub async fn statuses_of_project(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<ItemStatus>, StoreError> {
    Ok(
        sqlx::query_as("SELECT id, status FROM items WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(repo.pool())
            .await?,
    )
}
