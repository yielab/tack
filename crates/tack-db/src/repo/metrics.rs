//! Factory metrics: the raw aggregates behind `GET /api/projects/{id}/metrics/factory`,
//! read straight from rows of one project. `since` is an RFC 3339 instant stored
//! rows are compared against with `julianday` (`created_at >= since`); `None` is all time.
//! No `cost_usd` is read anywhere here.

use sqlx::{Row, SqlitePool};

/// Tokens (in + out) of attempts, split into implementation and rework.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenSplit {
    pub tokens: i64,
    pub attempts: i64,
    /// Attempts whose usage carries no token value.
    pub unmeasured: i64,
}

/// Attempt and decision counts plus token usage in the window.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttemptTotals {
    pub attempts: i64,
    pub decisions: i64,
    pub implementation: TokenSplit,
    pub rework: TokenSplit,
}

/// The stored RFC 3339 strings of one human wait; the caller subtracts them.
#[derive(Debug, Clone)]
pub struct WaitRow {
    pub viewed_at: Option<String>,
    pub created_at: String,
    pub resolved_at: String,
}

/// Counts of the pack and its review in the window.
#[derive(Debug, Clone, Copy, Default)]
pub struct MrpTotals {
    pub produced: i64,
    pub reviewed: i64,
    pub accepted: i64,
}

/// Pull request counts for requests opened in the window.
#[derive(Debug, Clone, Copy, Default)]
pub struct PullRequestTotals {
    pub opened: i64,
    pub merged: i64,
    pub closed: i64,
    pub reverted: i64,
}

/// Attempts and decisions in the window, and the token split. An attempt is
/// rework when its request's `metadata.rework_of` names another request, or when
/// it is not the request's first attempt; every other attempt is implementation.
pub async fn attempt_totals(
    pool: &SqlitePool,
    project_id: &str,
    since: Option<&str>,
) -> Result<AttemptTotals, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT CASE WHEN json_extract(r.metadata, '$.rework_of') IS NOT NULL
                          AND json_extract(r.metadata, '$.rework_of') != r.id THEN 'rework'
                     WHEN a.attempt_number = 1 THEN 'implementation'
                     ELSE 'rework' END AS bucket,
                COUNT(*) AS attempts,
                COALESCE(SUM(COALESCE(json_extract(a.usage, '$.tokens_in.value'), 0) + COALESCE(json_extract(a.usage, '$.tokens_out.value'), 0)), 0) AS tokens,
                COALESCE(SUM(json_extract(a.usage, '$.tokens_in.value') IS NULL AND json_extract(a.usage, '$.tokens_out.value') IS NULL), 0) AS unmeasured
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND (? IS NULL OR julianday(a.created_at) >= julianday(?))
         GROUP BY bucket",
    )
        .bind(project_id)
        .bind(since)
        .bind(since)
        .fetch_all(pool)
        .await?;
    let mut totals = AttemptTotals::default();
    for row in rows {
        let split = TokenSplit {
            tokens: row.get("tokens"),
            attempts: row.get("attempts"),
            unmeasured: row.get("unmeasured"),
        };
        totals.attempts += split.attempts;
        match row.get::<String, _>("bucket").as_str() {
            "implementation" => totals.implementation = split,
            _ => totals.rework = split,
        }
    }
    totals.decisions = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_decisions d
         JOIN execution_attempts a ON a.id = d.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND (? IS NULL OR julianday(d.created_at) >= julianday(?))",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(totals)
}

/// Resolved decisions created in the window.
pub async fn decision_waits(
    pool: &SqlitePool,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<WaitRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT d.viewed_at, d.created_at, d.resolved_at FROM execution_decisions d
         JOIN execution_attempts a ON a.id = d.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND d.resolved_at IS NOT NULL
           AND (? IS NULL OR julianday(d.created_at) >= julianday(?))",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| WaitRow {
            viewed_at: r.get("viewed_at"),
            created_at: r.get("created_at"),
            resolved_at: r.get("resolved_at"),
        })
        .collect())
}

/// Reviewed packs whose artifact was created in the window; `created_at` is the
/// artifact's, `resolved_at` the review's.
pub async fn mrp_waits(
    pool: &SqlitePool,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<WaitRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT m.viewed_at, x.created_at, m.reviewed_at AS resolved_at FROM mrp_reviews m
         JOIN execution_artifacts x ON x.attempt_id = m.attempt_id AND x.artifact_id = m.artifact_id
         JOIN execution_attempts a ON a.id = m.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND m.reviewed_at IS NOT NULL
           AND (? IS NULL OR julianday(x.created_at) >= julianday(?))",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| WaitRow {
            viewed_at: r.get("viewed_at"),
            created_at: r.get("created_at"),
            resolved_at: r.get("resolved_at"),
        })
        .collect())
}

/// Attempts that produced a pack in the window, how many were reviewed and
/// how many of those accepted.
pub async fn mrp_totals(
    pool: &SqlitePool,
    project_id: &str,
    media_type: &str,
    since: Option<&str>,
) -> Result<MrpTotals, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(DISTINCT x.attempt_id) AS produced,
                COUNT(DISTINCT CASE WHEN m.verdict IS NOT NULL THEN x.attempt_id END) AS reviewed,
                COUNT(DISTINCT CASE WHEN m.verdict = 'accept' THEN x.attempt_id END) AS accepted
         FROM execution_artifacts x
         JOIN execution_attempts a ON a.id = x.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         LEFT JOIN mrp_reviews m ON m.attempt_id = x.attempt_id AND m.artifact_id = x.artifact_id
         WHERE i.project_id = ? AND x.media_type = ? AND (? IS NULL OR julianday(x.created_at) >= julianday(?))",
    )
    .bind(project_id)
    .bind(media_type)
    .bind(since)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(MrpTotals {
        produced: row.get("produced"),
        reviewed: row.get("reviewed"),
        accepted: row.get("accepted"),
    })
}

/// Every pack artifact in the window: its `content_reference`, whose bytes carry
/// the verifier's `usage`. `None` where the content was never uploaded.
pub async fn mrp_pack_references(
    pool: &SqlitePool,
    project_id: &str,
    media_type: &str,
    since: Option<&str>,
) -> Result<Vec<Option<String>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT x.content_reference FROM execution_artifacts x
         JOIN execution_attempts a ON a.id = x.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND x.media_type = ? AND (? IS NULL OR julianday(x.created_at) >= julianday(?))
         ORDER BY x.created_at, x.id",
    )
    .bind(project_id)
    .bind(media_type)
    .bind(since)
    .bind(since)
    .fetch_all(pool)
    .await
}

/// Pull requests opened in the window, by their current state.
pub async fn pull_request_totals(
    pool: &SqlitePool,
    project_id: &str,
    since: Option<&str>,
) -> Result<PullRequestTotals, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS opened,
                COALESCE(SUM(p.state = 'merged'), 0) AS merged,
                COALESCE(SUM(p.state = 'closed'), 0) AS closed,
                COALESCE(SUM(p.state = 'reverted'), 0) AS reverted
         FROM pull_requests p
         JOIN execution_attempts a ON a.id = p.attempt_id
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND (? IS NULL OR julianday(p.opened_at) >= julianday(?))",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(PullRequestTotals {
        opened: row.get("opened"),
        merged: row.get("merged"),
        closed: row.get("closed"),
        reverted: row.get("reverted"),
    })
}
