use crate::{Repository, StoreError};

/// How many migrations this database has applied: the schema version a backup records and a
/// restore compares against the running binary.
pub async fn applied_migration_count(repo: &Repository) -> Result<u32, StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(repo.pool())
        .await?;
    Ok(count as u32)
}
