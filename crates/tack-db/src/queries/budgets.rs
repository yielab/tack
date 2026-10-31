use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::AssertSqlSafe;

use crate::repo::budgets::{
    COUNTED_TOKENS_SQL, DayRoom, ProjectBudgets, day_start, running_counted_join,
};
use crate::{Repository, StoreError};

/// The project's limits; `None` when it has no `project_budgets` row (no budget at all).
pub async fn project_budgets(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<ProjectBudgets>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT tokens_per_day, auto_merges_per_day, queue_items_max, provisional_tokens_per_run
         FROM project_budgets WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The token limit a run takes when neither the request nor its profile sets one: the project's
/// provisional limit, the default when it has no row, `None` when it is turned off.
pub async fn provisional_tokens(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(project_budgets(repo, project_id)
        .await?
        .unwrap_or_default()
        .provisional_tokens_per_run
        .filter(|n| *n > 0))
}

/// The project that owns a queued or running request, through its item, and the token limit its
/// snapshot carries (`None`: the run has none of its own). A provisional limit is not the run's
/// own: like no limit, it gives way to what is left of the day. `None` when the request is
/// unknown.
pub async fn project_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<(String, Option<i64>)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT i.project_id,
                CASE WHEN json_extract(r.request_snapshot, '$.metadata.limit_source') = 'provisional'
                     THEN NULL ELSE json_extract(r.request_snapshot, '$.budgets.tokens') END
         FROM execution_requests r JOIN items i ON i.id = r.item_id WHERE r.id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// Counted tokens ([`COUNTED_TOKENS_SQL`]) of the attempts the project started since 00:00 UTC,
/// an open one by what it has counted so far.
pub async fn spent_today(
    repo: &Repository,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<i64, StoreError> {
    // The fragments are constants of this crate; only values are bound.
    Ok(sqlx::query_scalar(AssertSqlSafe(format!(
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
    .fetch_one(repo.pool())
    .await?)
}

/// What each listed request's latest attempt has counted ([`COUNTED_TOKENS_SQL`]), an open one
/// by what it has counted so far. A request without an attempt is left out.
pub async fn counted_by_request(
    repo: &Repository,
    request_ids: &[String],
) -> Result<HashMap<String, i64>, StoreError> {
    // The fragments are constants of this crate; only values are bound.
    let rows: Vec<(String, i64)> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT r.id, {COUNTED_TOKENS_SQL} + COALESCE(live.counted, 0)
         FROM execution_attempts a
         {}
         JOIN execution_requests r ON r.id = a.request_id
         WHERE r.id IN (SELECT value FROM json_each(?))
           AND a.attempt_number = (SELECT MAX(attempt_number) FROM execution_attempts WHERE request_id = r.id)",
        running_counted_join()
    )))
    .bind(serde_json::json!(request_ids).to_string())
    .fetch_all(repo.pool())
    .await?;
    Ok(rows.into_iter().collect())
}

/// The project's token limit for today and what is spoken for; `None` when it has no daily
/// limit. The one owner of the day's room: admission and the Automation screen both read it.
pub async fn day_room(
    repo: &Repository,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<DayRoom>, StoreError> {
    let Some(limit) = project_budgets(repo, project_id)
        .await?
        .and_then(|budgets| budgets.tokens_per_day)
    else {
        return Ok(None);
    };
    let counted = spent_today(repo, project_id, now).await?;
    // The fragment is a constant of this crate; only `project_id` is bound.
    let held = crate::repo::execution::state::held();
    let (open_runs, reserved): (i64, i64) = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT COUNT(*), COALESCE(SUM(MAX(own - counted, 0)), 0) FROM (
            SELECT json_extract(r.request_snapshot, '$.budgets.tokens') AS own,
                   {COUNTED_TOKENS_SQL} + COALESCE(live.counted, 0) AS counted
            FROM execution_attempts a
            {}
            JOIN execution_requests r ON r.id = a.request_id
            JOIN items i ON i.id = r.item_id
            WHERE i.project_id = ?
              AND a.state IN ({held}))
         WHERE own IS NOT NULL",
        running_counted_join()
    )))
    .bind(project_id)
    .fetch_one(repo.pool())
    .await?;
    Ok(Some(DayRoom {
        limit,
        counted,
        reserved,
        open_runs,
        room: (limit - counted - reserved).max(0),
    }))
}

/// How many of the project's unfinished items wait for a human (show **Needs your review**), by
/// the board's own definition, so a pack left on an item already Done, or on an attempt a newer
/// run replaced, never fills the queue.
pub async fn review_queue_items(repo: &Repository, project_id: &str) -> Result<i64, StoreError> {
    // The fragment is a constant of this crate; only `project_id` is bound.
    Ok(sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM items WHERE project_id = ? AND completed_at IS NULL AND {}",
        crate::repo::items::needs_review_sql()
    )))
    .bind(project_id)
    .fetch_one(repo.pool())
    .await?)
}

/// The merges Tack made on its own (`trigger = 'auto'`) for the project since 00:00 UTC.
pub async fn auto_merges_today(
    repo: &Repository,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM tack_merges
         WHERE project_id = ? AND trigger = 'auto' AND merged_at IS NOT NULL
           AND julianday(merged_at) >= julianday(?)",
    )
    .bind(project_id)
    .bind(day_start(now))
    .fetch_one(repo.pool())
    .await?)
}
