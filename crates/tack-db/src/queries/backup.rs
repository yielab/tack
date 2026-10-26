use crate::{Repository, StoreError};

/// How many items the database holds: the figure a backup's manifest records.
pub async fn item_count(repo: &Repository) -> Result<u64, StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(repo.pool())
        .await?;
    Ok(count as u64)
}
