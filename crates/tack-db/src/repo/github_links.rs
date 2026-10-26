//! Item ↔ GitHub issue links: status and comments sync both ways over them.

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

/// Remove the GitHub link for an item, if any. Not an error if it was
/// already absent — matches `rm -f`, not `rm`.
pub async fn remove_link(pool: &SqlitePool, item_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM github_links WHERE item_id = ?")
        .bind(item_id.to_string())
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

/// List every `(item_id, issue_number, synced_at)` linked to one repo — what
/// the inbound poll needs to compute `since` (the max `synced_at` across the
/// set) and to map a returned issue number back to its item. `synced_at` is
/// `None` for a link the poll has never touched.
pub async fn list_links_for_repo(
    pool: &SqlitePool,
    repo: &str,
) -> Result<Vec<(Uuid, i64, Option<String>)>, sqlx::Error> {
    let rows: Vec<(String, i64, Option<String>)> =
        sqlx::query_as("SELECT item_id, issue_number, synced_at FROM github_links WHERE repo = ?")
            .bind(repo)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(item_id, issue_number, synced_at)| {
            Uuid::parse_str(&item_id)
                .ok()
                .map(|item_id| (item_id, issue_number, synced_at))
        })
        .collect())
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
