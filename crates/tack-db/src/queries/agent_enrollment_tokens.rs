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
