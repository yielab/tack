use crate::{Repository, StoreError};

/// The stored content reference of the attempt's newest artifact of `media_type` that has one,
/// or `None` when it has none.
pub async fn latest_content_reference(
    repo: &Repository,
    attempt_id: &str,
    media_type: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts \
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(media_type)
    .fetch_optional(repo.pool())
    .await?)
}
