use crate::{Repository, StoreError};

/// What a runner's bearer credential resolves to: enough to decide whether the
/// request may proceed, never the credential itself.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunnerCredentialRow {
    pub id: String,
    pub state: String,
    pub revoked_at: Option<String>,
    pub credential_expires_at: Option<String>,
}

/// The runner whose stored credential hash is `credential_hash`, or `None`.
pub async fn runner_by_credential_hash(
    repo: &Repository,
    credential_hash: &str,
) -> Result<Option<RunnerCredentialRow>, StoreError> {
    Ok(sqlx::query_as::<_, RunnerCredentialRow>(
        "SELECT id, state, revoked_at, credential_expires_at FROM agent_runners WHERE credential_hash = ? LIMIT 1",
    )
    .bind(credential_hash)
    .fetch_optional(repo.pool())
    .await?)
}

/// A runner's name and stored credential hash, as a test checks them after a
/// route changed them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunnerIdentityRow {
    pub name: String,
    pub credential_hash: String,
}

/// The name and credential hash of runner `runner_id`, or `None` when there is none.
pub async fn runner_identity(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<RunnerIdentityRow>, StoreError> {
    Ok(sqlx::query_as::<_, RunnerIdentityRow>(
        "SELECT name, credential_hash FROM agent_runners WHERE id = ?",
    )
    .bind(runner_id)
    .fetch_optional(repo.pool())
    .await?)
}

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

/// Whether this runner exists, is active and has not been revoked.
pub async fn runner_is_active(repo: &Repository, runner_id: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ? AND state = 'active' AND revoked_at IS NULL)",
    )
    .bind(runner_id)
    .fetch_one(repo.pool())
    .await?)
}
