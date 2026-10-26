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
