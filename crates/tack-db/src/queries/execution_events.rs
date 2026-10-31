use crate::{Repository, StoreError};

/// How many events an attempt has stored.
pub async fn event_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// How many stored events of an attempt carry this `event_id`: 0 or 1.
pub async fn event_count_with_id(
    repo: &Repository,
    attempt_id: &str,
    event_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = ?",
    )
    .bind(attempt_id)
    .bind(event_id)
    .fetch_one(repo.pool())
    .await?)
}
