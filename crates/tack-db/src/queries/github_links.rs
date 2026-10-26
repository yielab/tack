use uuid::Uuid;

use crate::{Repository, StoreError};

/// Every repository some item is linked to, each once.
pub async fn linked_repos(repo: &Repository) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar("SELECT DISTINCT repo FROM github_links")
        .fetch_all(repo.pool())
        .await?)
}

/// Every `(item_id, issue_number, synced_at)` linked to `github_repo`; `synced_at` is `None`
/// for a link the inbound poll has never touched.
pub async fn links_of_repo(
    repo: &Repository,
    github_repo: &str,
) -> Result<Vec<(Uuid, i64, Option<String>)>, StoreError> {
    Ok(crate::repo::github_links::list_links_for_repo(repo.pool(), github_repo).await?)
}
