use uuid::Uuid;

use crate::{Repository, StoreError};

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
