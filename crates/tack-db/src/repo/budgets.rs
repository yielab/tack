//! A project's daily budgets (`project_budgets`, migration 087) and the counts
//! they are compared with. A project with no row has no budget.

use crate::repo::execution::state;
use chrono::{DateTime, Utc};

/// One project's limits; `None` is no limit.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ProjectBudgets {
    pub tokens_per_day: Option<i64>,
    pub auto_merges_per_day: Option<i64>,
    pub queue_items_max: Option<i64>,
    /// The token limit of a run whose profile sets none; `None` turns it off.
    pub provisional_tokens_per_run: Option<i64>,
}

/// The provisional limit of a project that has not changed it: the cap the SWE-Effi
/// benchmark gives one software task.
pub const DEFAULT_PROVISIONAL_TOKENS: i64 = 2_000_000;

impl Default for ProjectBudgets {
    /// A project with no row: no daily limits, the default provisional limit.
    fn default() -> Self {
        Self {
            tokens_per_day: None,
            auto_merges_per_day: None,
            queue_items_max: None,
            provisional_tokens_per_run: Some(DEFAULT_PROVISIONAL_TOKENS),
        }
    }
}

/// 00:00 UTC of `now`'s day, as the RFC 3339 text the tables store.
pub(crate) fn day_start(now: DateTime<Utc>) -> String {
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists")
        .and_utc()
        .to_rfc3339()
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

/// Joins, to `execution_attempts a`, `live.counted`: what an open attempt has
/// counted so far, which is [`COUNTED_TOKENS_SQL`] over the `usage` its progress
/// lines carry (an attempt's own `usage` is written only when it ends). The one
/// owner of the attempt's running counted sum: the day's spend and its reservation read it.
pub(crate) fn running_counted_join() -> String {
    let held = state::held();
    format!(
        "LEFT JOIN (SELECT attempt_id, SUM({COUNTED_TOKENS_SQL}) AS counted FROM (
             SELECT e.attempt_id AS attempt_id, json_extract(e.payload, '$.usage') AS usage
             FROM execution_events e
             WHERE json_type(e.payload, '$.usage') = 'object'
               AND e.attempt_id IN
                 (SELECT id FROM execution_attempts WHERE state IN ({held}))) a
           GROUP BY attempt_id) live ON live.attempt_id = a.id"
    )
}

/// A project's token limit for today and what is already spoken for. An open run
/// is an attempt that is leased, preparing, running or waiting for a decision; it
/// reserves its own limit less what it has counted so far (its progress lines carry
/// the usage; an agent that reports none leaves its whole limit reserved). A run with
/// no limit of its own reserves nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayRoom {
    pub limit: i64,
    /// What `queries::budgets::spent_today` counts.
    pub counted: i64,
    pub reserved: i64,
    /// The open runs that reserve something.
    pub open_runs: i64,
    /// `limit - counted - reserved`, never below 0: what a run admitted now may use.
    pub room: i64,
}

impl crate::Repository {
    /// Replace the project's limits, creating the row when it has none.
    pub async fn put_project_budgets(
        &self,
        project_id: &str,
        budgets: &ProjectBudgets,
    ) -> Result<(), crate::StoreError> {
        sqlx::query(
            "INSERT INTO project_budgets (project_id, tokens_per_day, auto_merges_per_day, queue_items_max,
                                          provisional_tokens_per_run)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(project_id) DO UPDATE SET
               tokens_per_day = excluded.tokens_per_day,
               auto_merges_per_day = excluded.auto_merges_per_day,
               queue_items_max = excluded.queue_items_max,
               provisional_tokens_per_run = excluded.provisional_tokens_per_run",
        )
        .bind(project_id)
        .bind(budgets.tokens_per_day)
        .bind(budgets.auto_merges_per_day)
        .bind(budgets.queue_items_max)
        .bind(budgets.provisional_tokens_per_run)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A queued run with no limit of its own, in a project with a daily limit, runs with what is
    /// left of the day: writes `tokens` as its limit in the snapshot's two copies of `budgets`
    /// and in the row's own columns, and `metadata.limit_source` as `day`. A run under the
    /// provisional limit keeps the smaller of the two, and its source says which. A request that
    /// has a limit of its own, or is no longer queued, is left alone.
    pub async fn limit_request_to_day_room(
        &self,
        request_id: &str,
        tokens: i64,
    ) -> Result<(), crate::StoreError> {
        // Every expression reads the row as it was before the update.
        sqlx::query(
            state::sql("UPDATE execution_requests SET
               request_snapshot = json_set(request_snapshot,
                 '$.budgets.tokens', MIN(?1, COALESCE(json_extract(request_snapshot, '$.budgets.tokens'), ?1)),
                 '$.resolved_agent_profile.budgets.tokens',
                   MIN(?1, COALESCE(json_extract(request_snapshot, '$.budgets.tokens'), ?1)),
                 '$.metadata.limit_source',
                   CASE WHEN ?1 < COALESCE(json_extract(request_snapshot, '$.budgets.tokens'), ?1 + 1)
                        THEN 'day' ELSE 'provisional' END),
               budgets = json_set(budgets, '$.tokens',
                 MIN(?1, COALESCE(json_extract(request_snapshot, '$.budgets.tokens'), ?1))),
               metadata = json_set(metadata, '$.limit_source',
                 CASE WHEN ?1 < COALESCE(json_extract(request_snapshot, '$.budgets.tokens'), ?1 + 1)
                      THEN 'day' ELSE 'provisional' END)
             WHERE id = ?2 AND state = {queued}
               AND (json_extract(request_snapshot, '$.budgets.tokens') IS NULL
                    OR json_extract(request_snapshot, '$.metadata.limit_source') = 'provisional')"),
        )
        .bind(tokens)
        .bind(request_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
