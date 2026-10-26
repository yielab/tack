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
