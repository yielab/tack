//! Where the rate card is kept (`app_meta`, key `rate_card`), and which model
//! a run is priced as.

use sqlx::SqlitePool;
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
