use crate::{Repository, StoreError};

/// The value stored under `key` in `app_meta`, or `None` when nothing is stored there.
pub async fn value(repo: &Repository, key: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
            .bind(key)
            .fetch_optional(repo.pool())
            .await?,
    )
}
