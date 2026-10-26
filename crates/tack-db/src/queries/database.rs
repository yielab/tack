//! Questions about the database file itself rather than one table.

use crate::{Repository, StoreError};

/// How many migrations the database has applied.
pub async fn migrations_applied(repo: &Repository) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(repo.pool())
        .await?)
}

/// The size of the database in bytes (page count times page size).
pub async fn size_bytes(repo: &Repository) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
    )
    .fetch_one(repo.pool())
    .await?)
}

/// How many rows `table` holds. The name is a literal of the caller's, never user input.
pub async fn row_count(repo: &Repository, table: &'static str) -> Result<i64, StoreError> {
    let query = format!("SELECT COUNT(*) FROM {table}");
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(repo.pool())
        .await?)
}
