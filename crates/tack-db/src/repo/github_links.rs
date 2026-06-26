//! Item ↔ GitHub issue links for push-only status sync.

use sqlx::SqlitePool;
use uuid::Uuid;

/// Create or replace the GitHub link for an item.
pub async fn set_link(
    pool: &SqlitePool,
    item_id: Uuid,
    repo: &str,
    issue_number: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO github_links (item_id, repo, issue_number)
         VALUES (?, ?, ?)
         ON CONFLICT(item_id) DO UPDATE SET repo = excluded.repo, issue_number = excluded.issue_number",
    )
    .bind(item_id.to_string())
    .bind(repo)
    .bind(issue_number)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch the `(repo, issue_number)` linked to an item, if any.
pub async fn get_link(
    pool: &SqlitePool,
    item_id: Uuid,
) -> Result<Option<(String, i64)>, sqlx::Error> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT repo, issue_number FROM github_links WHERE item_id = ?")
            .bind(item_id.to_string())
            .fetch_optional(pool)
            .await?;
    Ok(row)
}
