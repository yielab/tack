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
    pub merge_sha: Option<String>,
    pub red_ci_at: Option<String>,
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

/// Mark a stored merged pull request as reverted, by PR `by_number` or, when
/// `None`, by a commit that has no pull request. Returns whether it was merged.
pub async fn mark_reverted(
    pool: &SqlitePool,
    repo: &str,
    number: i64,
    by_number: Option<i64>,
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

/// The merged pull requests of `repo` merged at or after `since` (RFC 3339).
pub async fn list_merged_since(
    pool: &SqlitePool,
    repo: &str,
    since: &str,
) -> Result<Vec<PullRequestRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT * FROM pull_requests WHERE repo = ? AND state = 'merged' AND merged_at >= ?",
    )
    .bind(repo)
    .bind(since)
    .fetch_all(pool)
    .await
}

/// Store the merge commit of a pull request that has none yet.
pub async fn set_merge_sha(
    pool: &SqlitePool,
    repo: &str,
    number: i64,
    merge_sha: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE pull_requests SET merge_sha = ? WHERE repo = ? AND number = ? AND merge_sha IS NULL",
    )
    .bind(merge_sha)
    .bind(repo)
    .bind(number)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Record when CI on a pull request's merge commit was first seen red.
pub async fn set_red_ci_at(
    pool: &SqlitePool,
    repo: &str,
    number: i64,
    at: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE pull_requests SET red_ci_at = ? WHERE repo = ? AND number = ? AND red_ci_at IS NULL",
    )
    .bind(at)
    .bind(repo)
    .bind(number)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

impl crate::Repository {
    /// Record that GitHub merged the pull request at `merged_at` into `merge_sha`: its state and
    /// times, then its merge commit. A reverted pull request and a merge commit already stored
    /// are left alone.
    pub async fn record_pull_request_merged(
        &self,
        repo: &str,
        number: i64,
        merged_at: &str,
        merge_sha: &str,
    ) -> Result<(), crate::StoreError> {
        observe(
            self.pool(),
            repo,
            number,
            "merged",
            Some(merged_at),
            Some(merged_at),
        )
        .await?;
        set_merge_sha(self.pool(), repo, number, merge_sha).await?;
        Ok(())
    }

    /// Record the pull request an attempt opened. A second call for the same attempt, or for a
    /// number already stored, changes nothing.
    pub async fn record_pull_request(
        &self,
        attempt_id: &str,
        repo: &str,
        number: i64,
        url: &str,
        opened_at: &str,
    ) -> Result<(), crate::StoreError> {
        Ok(insert(self.pool(), attempt_id, repo, number, url, opened_at).await?)
    }

    /// Write what GitHub reports for a stored pull request; whether a row changed.
    pub async fn observe_pull_request(
        &self,
        repo: &str,
        number: i64,
        state: &str,
        merged_at: Option<&str>,
        closed_at: Option<&str>,
    ) -> Result<bool, crate::StoreError> {
        Ok(observe(self.pool(), repo, number, state, merged_at, closed_at).await?)
    }

    /// Mark a stored merged pull request as reverted, by PR `by_number` or, when `None`, by a
    /// commit that has no pull request; whether it was merged.
    pub async fn mark_pull_request_reverted(
        &self,
        repo: &str,
        number: i64,
        by_number: Option<i64>,
    ) -> Result<bool, crate::StoreError> {
        Ok(mark_reverted(self.pool(), repo, number, by_number).await?)
    }

    /// Store the merge commit of a pull request that has none yet.
    pub async fn set_pull_request_merge_sha(
        &self,
        repo: &str,
        number: i64,
        merge_sha: &str,
    ) -> Result<bool, crate::StoreError> {
        Ok(set_merge_sha(self.pool(), repo, number, merge_sha).await?)
    }

    /// Record when CI on a pull request's merge commit was first seen red.
    pub async fn set_pull_request_red_ci_at(
        &self,
        repo: &str,
        number: i64,
        at: &str,
    ) -> Result<bool, crate::StoreError> {
        Ok(set_red_ci_at(self.pool(), repo, number, at).await?)
    }
}
