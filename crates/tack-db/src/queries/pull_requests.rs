use crate::repo::pull_requests::PullRequestRow;
use crate::{Repository, StoreError};

/// The pull request an attempt opened, `None` when it opened none.
pub async fn pull_request_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<PullRequestRow>, StoreError> {
    Ok(
        sqlx::query_as("SELECT * FROM pull_requests WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// The merged pull requests of `github_repo` merged at or after `since` (RFC 3339).
pub async fn merged_pull_requests_since(
    repo: &Repository,
    github_repo: &str,
    since: &str,
) -> Result<Vec<PullRequestRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT * FROM pull_requests WHERE repo = ? AND state = 'merged' AND merged_at >= ?",
    )
    .bind(github_repo)
    .bind(since)
    .fetch_all(repo.pool())
    .await?)
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
