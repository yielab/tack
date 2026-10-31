use crate::{Repository, StoreError};

/// How many heartbeat answers are remembered for replay, across every runner.
pub async fn replay_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_heartbeat_replays")
            .fetch_one(repo.pool())
            .await?,
    )
}
