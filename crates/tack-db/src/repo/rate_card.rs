//! Where the rate card is kept (`app_meta`, key `rate_card`), and which model
//! a run is priced as.

use sqlx::{AssertSqlSafe, SqlitePool};
use tack_core::rate_card::RateCard;

const KEY: &str = "rate_card";

/// The model a run is priced as, over `execution_attempts a` joined to
/// `execution_requests r`: the model the agent reported, else the one the run
/// asked for. A run on the agent's default that reported none is not priced.
pub const RUN_MODEL_SQL: &str =
    "COALESCE(json_extract(a.actual_execution, '$.model'), r.requested_model)";

/// The stored card, or `None` when the operator never entered one.
pub async fn load(pool: &SqlitePool) -> Result<Option<RateCard>, sqlx::Error> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
}

pub async fn save(pool: &SqlitePool, card: &RateCard) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(card).expect("a rate card serializes");
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// One model's tokens in a repository and team, over the attempts counted.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SpendRow {
    /// `None` for a run that reported no model and asked for none.
    pub model: Option<String>,
    /// The repository of the pull request the attempts opened.
    pub repo: Option<String>,
    /// The docket team of the attempts' resolved agent profile.
    pub team: Option<String>,
    /// Every input token, cache reads and writes included: what the rate card prices.
    pub tokens_in: i64,
    /// Of `tokens_in`, the ones the prompt cache served: shown, never counted.
    pub cache_read: i64,
    pub tokens_out: i64,
    pub attempts: i64,
}

/// The project's attempts created since `since` (all time when `None`), by
/// model, pull-request repository and team. An attempt that carries
/// `usage.by_model` counts once per model in it, with its cache reads and
/// writes in `tokens_in`; any other counts under [`RUN_MODEL_SQL`] with its
/// `tokens_in` and `tokens_out`.
pub async fn spend_by_model(
    pool: &SqlitePool,
    project: &str,
    since: Option<&str>,
) -> Result<Vec<SpendRow>, sqlx::Error> {
    // The fragments are constants of this crate; only values are bound.
    let from = "FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         JOIN items i ON i.id = r.item_id
         LEFT JOIN pull_requests p ON p.attempt_id = a.id";
    let team =
        "json_extract(r.request_snapshot, '$.resolved_agent_profile.tool_policy.docket.team')";
    let filter = "i.project_id = ? AND (? IS NULL OR julianday(a.created_at) >= julianday(?))";
    sqlx::query_as(AssertSqlSafe(format!(
        "SELECT model, repo, team,
                COALESCE(SUM(tokens_in), 0) AS tokens_in,
                COALESCE(SUM(cache_read), 0) AS cache_read,
                COALESCE(SUM(tokens_out), 0) AS tokens_out,
                COUNT(*) AS attempts
         FROM (
           SELECT m.key AS model, p.repo AS repo, {team} AS team,
                  COALESCE(json_extract(m.value, '$.input'), 0)
                    + COALESCE(json_extract(m.value, '$.cache_read'), 0)
                    + COALESCE(json_extract(m.value, '$.cache_write'), 0) AS tokens_in,
                  COALESCE(json_extract(m.value, '$.cache_read'), 0) AS cache_read,
                  COALESCE(json_extract(m.value, '$.output'), 0) AS tokens_out
           {from}, json_each(a.usage, '$.by_model') m
           WHERE json_type(a.usage, '$.by_model') = 'object' AND {filter}
           UNION ALL
           SELECT {model} AS model, p.repo AS repo, {team} AS team,
                  COALESCE(json_extract(a.usage, '$.tokens_in.value'), 0) AS tokens_in,
                  {cache_read} AS cache_read,
                  COALESCE(json_extract(a.usage, '$.tokens_out.value'), 0) AS tokens_out
           {from}
           WHERE json_type(a.usage, '$.by_model') IS NOT 'object' AND {filter}
         )
         GROUP BY model, repo, team",
        model = RUN_MODEL_SQL,
        cache_read = super::budgets::CACHE_READ_SQL
    )))
    .bind(project)
    .bind(since)
    .bind(since)
    .bind(project)
    .bind(since)
    .bind(since)
    .fetch_all(pool)
    .await
}

impl crate::Repository {
    /// Keep `card` as the rate card, replacing the one stored.
    pub async fn save_rate_card(&self, card: &RateCard) -> Result<(), crate::StoreError> {
        Ok(save(self.pool(), card).await?)
    }
}
