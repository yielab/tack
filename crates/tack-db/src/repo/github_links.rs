//! Item ↔ GitHub issue links: status and comments sync both ways over them.

use crate::StoreError;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Create or replace the GitHub link for an item.
pub(crate) async fn set_link(
    pool: &SqlitePool,
    item_id: Uuid,
    repo: &str,
    issue_number: i64,
) -> Result<(), StoreError> {
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

/// Remove the GitHub link for an item, if any. Not an error if it was
/// already absent — matches `rm -f`, not `rm`.
pub(crate) async fn remove_link(pool: &SqlitePool, item_id: Uuid) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM github_links WHERE item_id = ?")
        .bind(item_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch the `(repo, issue_number)` linked to an item, if any.
pub(crate) async fn get_link(
    pool: &SqlitePool,
    item_id: Uuid,
) -> Result<Option<(String, i64)>, StoreError> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT repo, issue_number FROM github_links WHERE item_id = ?")
            .bind(item_id.to_string())
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

impl crate::Repository {
    /// Record the `updated_at` of the GitHub issue an item's link was last synced against.
    pub async fn mark_github_link_synced(
        &self,
        item_id: Uuid,
        synced_at: &str,
    ) -> Result<(), crate::StoreError> {
        sqlx::query("UPDATE github_links SET synced_at = ? WHERE item_id = ?")
            .bind(synced_at)
            .bind(item_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
