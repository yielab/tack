use uuid::Uuid;

use crate::{Repository, StoreError};

/// The ids of the projects that start their own work: auto-start on, not archived, and a
/// status named to take tasks from.
pub async fn auto_start_project_ids(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM projects WHERE auto_start_enabled = 1 AND archived = 0 AND auto_start_status IS NOT NULL",
    )
    .fetch_all(repo.pool())
    .await?)
}

/// The id of the project of this name in the workspace, when there is one.
pub async fn project_id_named(
    repo: &Repository,
    workspace_id: Uuid,
    name: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT id FROM projects WHERE workspace_id = ? AND name = ?")
            .bind(workspace_id.to_string())
            .bind(name)
            .fetch_optional(repo.pool())
            .await?,
    )
}
