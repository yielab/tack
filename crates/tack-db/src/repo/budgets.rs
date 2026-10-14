//! A project's daily budgets (`project_budgets`, migration 087) and the three
//! counts they are compared with. A project with no row has no budget.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

/// The media type of a Merge-Readiness Pack artifact.
const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

/// One project's limits; `None` is no limit.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ProjectBudgets {
    pub tokens_per_day: Option<i64>,
    pub usd_per_day: Option<f64>,
    pub auto_merges_per_day: Option<i64>,
    pub queue_items_max: Option<i64>,
}

/// 00:00 UTC of `now`'s day, as the RFC 3339 text the tables store.
fn day_start(now: DateTime<Utc>) -> String {
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists")
        .and_utc()
        .to_rfc3339()
}

pub async fn get(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Option<ProjectBudgets>, sqlx::Error> {
    sqlx::query_as(
        "SELECT tokens_per_day, usd_per_day, auto_merges_per_day, queue_items_max
         FROM project_budgets WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn put(
    pool: &SqlitePool,
    project_id: &str,
    budgets: &ProjectBudgets,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_budgets (project_id, tokens_per_day, usd_per_day, auto_merges_per_day, queue_items_max)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(project_id) DO UPDATE SET
           tokens_per_day = excluded.tokens_per_day,
           usd_per_day = excluded.usd_per_day,
           auto_merges_per_day = excluded.auto_merges_per_day,
           queue_items_max = excluded.queue_items_max",
    )
    .bind(project_id)
    .bind(budgets.tokens_per_day)
    .bind(budgets.usd_per_day)
    .bind(budgets.auto_merges_per_day)
    .bind(budgets.queue_items_max)
    .execute(pool)
    .await?;
    Ok(())
}

/// The project that owns a queued or running request, through its item.
pub async fn project_of_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT i.project_id FROM execution_requests r JOIN items i ON i.id = r.item_id WHERE r.id = ?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
}

/// Tokens (in plus out, as the runners measured them) of the attempts the
/// project started since 00:00 UTC.
pub async fn spent_today(
    pool: &SqlitePool,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(COALESCE(json_extract(a.usage, '$.tokens_in.value'), 0)
                           + COALESCE(json_extract(a.usage, '$.tokens_out.value'), 0)), 0)
         FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND julianday(a.created_at) >= julianday(?)",
    )
    .bind(project_id)
    .bind(day_start(now))
    .fetch_one(pool)
    .await
}

/// What waits for a human: packs nobody has given a verdict, plus held merge
/// rows. A pack whose attempt already has a merge row is counted once, there.
pub async fn queue_items(pool: &SqlitePool, project_id: &str) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT
           (SELECT COUNT(DISTINCT a.id)
              FROM execution_attempts a
              JOIN execution_requests r ON r.id = a.request_id
              JOIN items i ON i.id = r.item_id
              JOIN execution_artifacts x ON x.attempt_id = a.id AND x.media_type = ?
             WHERE i.project_id = ?
               AND NOT EXISTS (SELECT 1 FROM mrp_reviews m WHERE m.attempt_id = a.id AND m.verdict IS NOT NULL)
               AND NOT EXISTS (SELECT 1 FROM tack_merges t WHERE t.attempt_id = a.id))
         + (SELECT COUNT(*) FROM tack_merges t
             WHERE t.project_id = ? AND t.held_reason IS NOT NULL AND t.merged_at IS NULL)
           AS waiting",
    )
    .bind(MRP_MEDIA_TYPE)
    .bind(project_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(row.get("waiting"))
}

/// Merges Tack made on its own (`trigger = 'auto'`) since 00:00 UTC.
pub async fn auto_merges_today(
    pool: &SqlitePool,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM tack_merges
         WHERE project_id = ? AND trigger = 'auto' AND merged_at IS NOT NULL
           AND julianday(merged_at) >= julianday(?)",
    )
    .bind(project_id)
    .bind(day_start(now))
    .fetch_one(pool)
    .await
}
