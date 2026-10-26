use crate::{Repository, StoreError};

/// One runner's lifecycle columns.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunnerLifecycle {
    pub state: String,
    pub revoked_at: Option<String>,
}

/// How many more attempts the runner can take, or `None` when no runner has this id.
pub async fn available_capacity(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = ?")
            .bind(runner_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// The runner's state and when it was revoked, or `None` when no runner has this id.
pub async fn runner_lifecycle(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<RunnerLifecycle>, StoreError> {
    Ok(
        sqlx::query_as("SELECT state, revoked_at FROM agent_runners WHERE id = ?")
            .bind(runner_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}
