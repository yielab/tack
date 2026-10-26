use crate::{Repository, StoreError};

/// The stored hash of enrollment token `token_id`, or `None` when there is none.
pub async fn token_hash(repo: &Repository, token_id: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT token_hash FROM agent_enrollment_tokens WHERE id = ?")
            .bind(token_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// The stored hash of the newest enrollment token issued for `runner_id`, or `None` when
/// none was.
pub async fn latest_token_hash_of_runner(
    repo: &Repository,
    runner_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT token_hash FROM agent_enrollment_tokens WHERE runner_id=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(runner_id)
    .fetch_optional(repo.pool())
    .await?)
}
