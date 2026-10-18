//! A project's daily budgets (`project_budgets`, migration 087) and the counts
//! they are compared with. A project with no row has no budget.

use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, SqlitePool};

/// One project's limits; `None` is no limit.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ProjectBudgets {
    pub tokens_per_day: Option<i64>,
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
        "SELECT tokens_per_day, auto_merges_per_day, queue_items_max
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
        "INSERT INTO project_budgets (project_id, tokens_per_day, auto_merges_per_day, queue_items_max)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(project_id) DO UPDATE SET
           tokens_per_day = excluded.tokens_per_day,
           auto_merges_per_day = excluded.auto_merges_per_day,
           queue_items_max = excluded.queue_items_max",
    )
    .bind(project_id)
    .bind(budgets.tokens_per_day)
    .bind(budgets.auto_merges_per_day)
    .bind(budgets.queue_items_max)
    .execute(pool)
    .await?;
    Ok(())
}

/// The project that owns a queued or running request, through its item, and the
/// token limit its snapshot carries (`None`: the run has none of its own).
pub async fn project_of_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, Option<i64>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.project_id, json_extract(r.request_snapshot, '$.budgets.tokens')
         FROM execution_requests r JOIN items i ON i.id = r.item_id WHERE r.id = ?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
}

/// A queued run with no limit of its own, in a project with a daily limit, runs
/// with what is left of the day: writes `tokens` as its limit in the snapshot's
/// two copies of `budgets` and in the row's own columns, and `metadata.limit_source`
/// as `day`. A request that has a limit of its own, or is no longer queued, is left alone.
pub async fn limit_to_day_room(
    pool: &SqlitePool,
    request_id: &str,
    tokens: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE execution_requests SET
           request_snapshot = json_set(request_snapshot,
             '$.budgets.tokens', ?1,
             '$.resolved_agent_profile.budgets.tokens', ?1,
             '$.metadata.limit_source', 'day'),
           budgets = json_set(budgets, '$.tokens', ?1),
           metadata = json_set(metadata, '$.limit_source', 'day')
         WHERE id = ?2 AND state = 'queued'
           AND json_extract(request_snapshot, '$.budgets.tokens') IS NULL",
    )
    .bind(tokens)
    .bind(request_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// A counted token is a new one: the attempt's input without what the cache
/// served, plus its output. Over `execution_attempts a`; `tokens_in` already
/// includes cache reads, and an attempt with no cache figure counts whole. An
/// attempt that reported neither figure counts 0 here and is *unmeasured* to
/// whoever needs the difference. The one owner of this sum in SQL: the day's
/// spend and the Factory totals read it; `Usage::counted` is the same rule in Rust.
pub const COUNTED_TOKENS_SQL: &str = "(COALESCE(json_extract(a.usage, '$.tokens_in.value')
             - COALESCE(json_extract(a.usage, '$.cache_read_tokens'), 0), 0)
           + COALESCE(json_extract(a.usage, '$.tokens_out.value'), 0))";

/// The input tokens the prompt cache served, over `execution_attempts a`:
/// shown beside [`COUNTED_TOKENS_SQL`], never part of it.
pub const CACHE_READ_SQL: &str = "COALESCE(json_extract(a.usage, '$.cache_read_tokens'), 0)";

/// The attempt states in which a run is open: it holds its place in the day.
const OPEN_ATTEMPT_STATES: &str = "'leased', 'preparing', 'running', 'waiting_decision'";

/// Joins, to `execution_attempts a`, `live.counted`: what an open attempt has
/// counted so far, which is [`COUNTED_TOKENS_SQL`] over the `usage` its progress
/// lines carry (an attempt's own `usage` is written only when it ends). The one
/// owner of the attempt's running counted sum: the day's spend and its reservation read it.
fn running_counted_join() -> String {
    format!(
        "LEFT JOIN (SELECT attempt_id, SUM({COUNTED_TOKENS_SQL}) AS counted FROM (
             SELECT e.attempt_id AS attempt_id, json_extract(e.payload, '$.usage') AS usage
             FROM execution_events e
             WHERE json_type(e.payload, '$.usage') = 'object'
               AND e.attempt_id IN
                 (SELECT id FROM execution_attempts WHERE state IN ({OPEN_ATTEMPT_STATES}))) a
           GROUP BY attempt_id) live ON live.attempt_id = a.id"
    )
}

/// Counted tokens ([`COUNTED_TOKENS_SQL`]) of the attempts the project started
/// since 00:00 UTC, an open one by what it has counted so far.
pub async fn spent_today(
    pool: &SqlitePool,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    // The fragments are constants of this crate; only values are bound.
    sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COALESCE(SUM({COUNTED_TOKENS_SQL} + COALESCE(live.counted, 0)), 0)
         FROM execution_attempts a
         {}
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ? AND julianday(a.created_at) >= julianday(?)",
        running_counted_join()
    )))
    .bind(project_id)
    .bind(day_start(now))
    .fetch_one(pool)
    .await
}

/// A project's token limit for today and what is already spoken for. An open run
/// is an attempt that is leased, preparing, running or waiting for a decision; it
/// reserves its own limit less what it has counted so far (its progress lines carry
/// the usage; an agent that reports none leaves its whole limit reserved). A run with
/// no limit of its own reserves nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayRoom {
    pub limit: i64,
    /// [`spent_today`].
    pub counted: i64,
    pub reserved: i64,
    /// The open runs that reserve something.
    pub open_runs: i64,
    /// `limit - counted - reserved`, never below 0: what a run admitted now may use.
    pub room: i64,
}

/// The project's [`DayRoom`]; `None` when it has no token limit per day. The one
/// owner of the day's room: admission and the Automation screen both read it.
pub async fn day_room(
    pool: &SqlitePool,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<DayRoom>, sqlx::Error> {
    let Some(limit) = get(pool, project_id)
        .await?
        .and_then(|budgets| budgets.tokens_per_day)
    else {
        return Ok(None);
    };
    let counted = spent_today(pool, project_id, now).await?;
    // The fragment is a constant of this crate; only `project_id` is bound.
    let (open_runs, reserved): (i64, i64) = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT COUNT(*), COALESCE(SUM(MAX(own - counted, 0)), 0) FROM (
            SELECT json_extract(r.request_snapshot, '$.budgets.tokens') AS own,
                   {COUNTED_TOKENS_SQL} + COALESCE(live.counted, 0) AS counted
            FROM execution_attempts a
            {}
            JOIN execution_requests r ON r.id = a.request_id
            JOIN items i ON i.id = r.item_id
            WHERE i.project_id = ?
              AND a.state IN ({OPEN_ATTEMPT_STATES}))
         WHERE own IS NOT NULL",
        running_counted_join()
    )))
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(Some(DayRoom {
        limit,
        counted,
        reserved,
        open_runs,
        room: (limit - counted - reserved).max(0),
    }))
}

/// What waits for a human: the project's unfinished items that show **Needs
/// your review**, by the board's own definition, so a pack left on an item
/// already Done, or on an attempt a newer run replaced, never fills the queue.
pub async fn queue_items(pool: &SqlitePool, project_id: &str) -> Result<i64, sqlx::Error> {
    // The fragment is a constant of this crate; only `project_id` is bound.
    sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM items WHERE project_id = ? AND completed_at IS NULL AND {}",
        super::items::NEEDS_REVIEW_SQL
    )))
    .bind(project_id)
    .fetch_one(pool)
    .await
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
