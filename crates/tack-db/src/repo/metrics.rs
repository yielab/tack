//! Factory metrics: the raw aggregates behind `GET /api/projects/{id}/metrics/factory`,
//! read straight from rows of one project. `since` is an RFC 3339 instant stored
//! rows are compared against with `julianday` (`created_at >= since`); `None` is all time.
//! No `cost_usd` is read anywhere here.

use sqlx::{AssertSqlSafe, Row};

use super::budgets::{CACHE_READ_SQL, COUNTED_TOKENS_SQL};
use crate::{Repository, StoreError};

/// Counted tokens (`COUNTED_TOKENS_SQL`) of attempts, split into implementation
/// and rework, with the cache reads that were shown and not counted.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenSplit {
    pub tokens: i64,
    pub cache_read: i64,
    pub attempts: i64,
    /// Attempts whose usage carries no token value.
    pub unmeasured: i64,
}

/// Attempt and decision counts plus token usage in the window.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttemptTotals {
    pub attempts: i64,
    pub decisions: i64,
    /// Attempts with at least one escalation the agent's own policy settled, which no person saw.
    pub decided_alone: i64,
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

/// Attempts, decisions and attempts that decided alone in the window, and the token split. An attempt is
/// rework when its request's `metadata.rework_of` names another request, or when
/// it is not the request's first attempt; every other attempt is implementation.
pub async fn attempt_totals(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<AttemptTotals, StoreError> {
    // The fragments are constants of this crate; only values are bound.
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT CASE WHEN json_extract(r.metadata, '$.rework_of') IS NOT NULL
                          AND json_extract(r.metadata, '$.rework_of') != r.id THEN 'rework'
                     WHEN a.attempt_number = 1 THEN 'implementation'
                     ELSE 'rework' END AS bucket,
                COUNT(*) AS attempts,
                COALESCE(SUM({COUNTED_TOKENS_SQL}), 0) AS tokens,
                COALESCE(SUM({CACHE_READ_SQL}), 0) AS cache_read,
                COALESCE(SUM(json_extract(a.usage, '$.tokens_in.value') IS NULL AND json_extract(a.usage, '$.tokens_out.value') IS NULL), 0) AS unmeasured
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND (? IS NULL OR julianday(a.created_at) >= julianday(?))
         GROUP BY bucket"
    )))
        .bind(project_id)
        .bind(since)
        .bind(since)
        .fetch_all(repo.pool())
        .await?;
    let mut totals = AttemptTotals::default();
    for row in rows {
        let split = TokenSplit {
            tokens: row.get("tokens"),
            cache_read: row.get("cache_read"),
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
    .fetch_one(repo.pool())
    .await?;
    totals.decided_alone = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND (? IS NULL OR julianday(a.created_at) >= julianday(?))
           AND EXISTS (SELECT 1 FROM execution_events e
                       WHERE e.attempt_id = a.id AND e.kind = ?)",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .bind(tack_protocol::event_kind::ESCALATION_DECIDED)
    .fetch_one(repo.pool())
    .await?;
    Ok(totals)
}

/// Resolved decisions created in the window.
pub async fn decision_waits(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<WaitRow>, StoreError> {
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
    .fetch_all(repo.pool())
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
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<Vec<WaitRow>, StoreError> {
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
    .fetch_all(repo.pool())
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
    repo: &Repository,
    project_id: &str,
    media_type: &str,
    since: Option<&str>,
) -> Result<MrpTotals, StoreError> {
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
    .fetch_one(repo.pool())
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
    repo: &Repository,
    project_id: &str,
    media_type: &str,
    since: Option<&str>,
) -> Result<Vec<Option<String>>, StoreError> {
    Ok(sqlx::query_scalar(
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
    .fetch_all(repo.pool())
    .await?)
}

/// Pull requests opened in the window, by their current state.
pub async fn pull_request_totals(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<PullRequestTotals, StoreError> {
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
    .fetch_one(repo.pool())
    .await?;
    Ok(PullRequestTotals {
        opened: row.get("opened"),
        merged: row.get("merged"),
        closed: row.get("closed"),
        reverted: row.get("reverted"),
    })
}

/// Requests with an attempt in the window, by how they began: rework (`metadata.rework_of`
/// names another request), by itself (`metadata.started_by = auto_start`), or by a person.
#[derive(Debug, Clone, Copy, Default)]
pub struct StartTotals {
    pub person: i64,
    pub auto: i64,
    pub rework: i64,
}

/// Merges recorded in the window, by what triggered them.
#[derive(Debug, Clone, Copy, Default)]
pub struct MergeTriggers {
    pub button: i64,
    pub done: i64,
    pub auto: i64,
}

pub async fn runs_by_start(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<StartTotals, StoreError> {
    let rows = sqlx::query(
        "SELECT CASE WHEN json_extract(r.metadata, '$.rework_of') IS NOT NULL
                          AND json_extract(r.metadata, '$.rework_of') != r.id THEN 'rework'
                     WHEN json_extract(r.metadata, '$.started_by') = 'auto_start' THEN 'auto'
                     ELSE 'person' END AS bucket,
                COUNT(DISTINCT r.id) AS runs
         FROM execution_requests r
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND EXISTS (
             SELECT 1 FROM execution_attempts a
             WHERE a.request_id = r.id AND (? IS NULL OR julianday(a.created_at) >= julianday(?)))
         GROUP BY bucket",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_all(repo.pool())
    .await?;
    let mut totals = StartTotals::default();
    for row in rows {
        let runs = row.get("runs");
        match row.get::<String, _>("bucket").as_str() {
            "rework" => totals.rework = runs,
            "auto" => totals.auto = runs,
            _ => totals.person = runs,
        }
    }
    Ok(totals)
}

pub async fn merges_by_trigger(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<MergeTriggers, StoreError> {
    let rows = sqlx::query(
        "SELECT trigger, COUNT(*) AS merges FROM tack_merges
         WHERE project_id = ? AND merged_at IS NOT NULL
           AND (? IS NULL OR julianday(merged_at) >= julianday(?))
         GROUP BY trigger",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_all(repo.pool())
    .await?;
    let mut totals = MergeTriggers::default();
    for row in rows {
        let merges = row.get("merges");
        match row.get::<String, _>("trigger").as_str() {
            "button" => totals.button = merges,
            "done" => totals.done = merges,
            _ => totals.auto = merges,
        }
    }
    Ok(totals)
}

/// Of the items merged in the window, how many had their newest merged attempt begin by
/// itself while none of the item's requests ever had a resolved decision.
/// Returns `(lights_off, merged_items)`.
pub async fn lights_off(
    repo: &Repository,
    project_id: &str,
    since: Option<&str>,
) -> Result<(i64, i64), StoreError> {
    let row = sqlx::query(
        "WITH newest AS (
             SELECT i.id AS item_id, r.metadata AS metadata,
                    ROW_NUMBER() OVER (PARTITION BY i.id ORDER BY t.merged_at DESC) AS rn
             FROM tack_merges t
             JOIN execution_attempts a ON a.id = t.attempt_id
             JOIN execution_requests r ON r.id = a.request_id
             JOIN items i ON i.id = r.item_id
             WHERE t.project_id = ? AND t.merged_at IS NOT NULL
               AND (? IS NULL OR julianday(t.merged_at) >= julianday(?)))
         SELECT COUNT(*) AS merged_items,
                COALESCE(SUM(json_extract(metadata, '$.started_by') = 'auto_start'
                    AND NOT EXISTS (
                        SELECT 1 FROM execution_decisions d
                        JOIN execution_attempts a2 ON a2.id = d.attempt_id
                        JOIN execution_requests r2 ON r2.id = a2.request_id
                        WHERE r2.item_id = newest.item_id AND d.resolved_at IS NOT NULL)), 0) AS lights_off
         FROM newest WHERE rn = 1",
    )
    .bind(project_id)
    .bind(since)
    .bind(since)
    .fetch_one(repo.pool())
    .await?;
    Ok((row.get("lights_off"), row.get("merged_items")))
}
