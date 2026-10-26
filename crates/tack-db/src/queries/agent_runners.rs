use crate::{Repository, StoreError};

/// Whether this runner exists, is active and has not been revoked.
pub async fn runner_is_active(repo: &Repository, runner_id: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ? AND state = 'active' AND revoked_at IS NULL)",
    )
    .bind(runner_id)
    .fetch_one(repo.pool())
    .await?)
}
