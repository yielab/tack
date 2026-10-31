use crate::{Repository, StoreError};

/// How many completions of an attempt are remembered for replay.
pub async fn replay_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_completion_replays WHERE attempt_id = ?",
        )
        .bind(attempt_id)
        .fetch_one(repo.pool())
        .await?,
    )
}
