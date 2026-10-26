use crate::{Repository, StoreError};

/// The value kept under `key` in the `app_meta` key/value table, `None` when nothing is kept
/// there.
pub async fn app_meta_value(repo: &Repository, key: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
            .bind(key)
            .fetch_optional(repo.pool())
            .await?,
    )
}
