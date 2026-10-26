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

/// The stored pull request `repo#number`, `None` when Tack stored none.
pub async fn pull_request(
    repo: &Repository,
    repository: &str,
    number: i64,
) -> Result<Option<PullRequestRow>, StoreError> {
    Ok(
        sqlx::query_as("SELECT * FROM pull_requests WHERE repo = ? AND number = ?")
            .bind(repository)
            .bind(number)
            .fetch_optional(repo.pool())
            .await?,
    )
}
