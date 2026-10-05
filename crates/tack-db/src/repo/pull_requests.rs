//! The pull request an attempt opened on GitHub, and what became of it. One row
//! per attempt; a repo's PR number maps to at most one attempt. `state` is one of
//! `open`, `merged`, `closed`, `reverted`.

use sqlx::SqlitePool;

/// One stored pull request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PullRequestRow {
    pub attempt_id: String,
    pub repo: String,
    pub number: i64,
    pub url: String,
    pub state: String,
    pub opened_at: String,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
    pub reverted_by_number: Option<i64>,
}

/// Record the pull request an attempt opened. A second call for the same
/// attempt, or for a number already stored, changes nothing.
pub async fn insert(
    pool: &SqlitePool,
    attempt_id: &str,
    repo: &str,
    number: i64,
    url: &str,
    opened_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO pull_requests (attempt_id, repo, number, url, state, opened_at)
         VALUES (?, ?, ?, ?, 'open', ?)",
    )
    .bind(attempt_id)
    .bind(repo)
    .bind(number)
    .bind(url)
    .bind(opened_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// The pull request an attempt opened, if any.
pub async fn get_for_attempt(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<PullRequestRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM pull_requests WHERE attempt_id = ?")
        .bind(attempt_id)
        .fetch_optional(pool)
        .await
}

/// Write what GitHub reports for a stored pull request. Returns whether a row
/// changed: an unchanged report, an unknown PR and a reverted PR all write
/// nothing.
pub async fn observe(
    pool: &SqlitePool,
    repo: &str,
    number: i64,
    state: &str,
    merged_at: Option<&str>,
    closed_at: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE pull_requests SET state = ?, merged_at = ?, closed_at = ?
         WHERE repo = ? AND number = ? AND state != 'reverted'
           AND (state IS NOT ? OR merged_at IS NOT ? OR closed_at IS NOT ?)",
    )
    .bind(state)
    .bind(merged_at)
    .bind(closed_at)
    .bind(repo)
    .bind(number)
    .bind(state)
    .bind(merged_at)
    .bind(closed_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Mark a stored merged pull request as reverted by PR `by_number`. Returns
/// whether it was one.
pub async fn mark_reverted(
    pool: &SqlitePool,
    repo: &str,
    number: i64,
    by_number: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE pull_requests SET state = 'reverted', reverted_by_number = ?
         WHERE repo = ? AND number = ? AND state = 'merged'",
    )
    .bind(by_number)
    .bind(repo)
    .bind(number)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
