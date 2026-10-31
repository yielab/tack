use uuid::Uuid;

use crate::{Repository, StoreError};

/// Every repository some item is linked to, each once.
pub async fn linked_repos(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar("SELECT DISTINCT repo FROM github_links")
        .fetch_all(repo.pool())
        .await?)
}

/// Every `(item_id, issue_number, synced_at)` linked to `github_repo` — what the inbound poll
/// needs to compute `since` (the newest `synced_at`) and to map a returned issue number back to
/// its item. `synced_at` is `None` for a link the poll has never touched.
pub async fn links_of_repo(
    repo: &Repository,
    github_repo: &str,
) -> Result<Vec<(Uuid, i64, Option<String>)>, StoreError> {
    let rows: Vec<(String, i64, Option<String>)> =
        sqlx::query_as("SELECT item_id, issue_number, synced_at FROM github_links WHERE repo = ?")
            .bind(github_repo)
            .fetch_all(repo.pool())
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
