use crate::repo::pull_requests::PullRequestRow;
use crate::{Repository, StoreError};

/// The pull request an attempt opened, `None` when it opened none.
pub async fn pull_request_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<PullRequestRow>, StoreError> {
    Ok(crate::repo::pull_requests::get_for_attempt(repo.pool(), attempt_id).await?)
}

/// The merged pull requests of `github_repo` merged at or after `since` (RFC 3339).
pub async fn merged_pull_requests_since(
    repo: &Repository,
    github_repo: &str,
    since: &str,
) -> Result<Vec<PullRequestRow>, StoreError> {
    Ok(crate::repo::pull_requests::list_merged_since(repo.pool(), github_repo, since).await?)
}

/// Each repository the project's tasks opened pull requests in, by name.
pub async fn repos_of_project(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT p.repo FROM pull_requests p
         JOIN execution_attempts a ON a.id = p.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? ORDER BY p.repo",
    )
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?)
}
