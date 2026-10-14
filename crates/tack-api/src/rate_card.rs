//! The operator's rate card: what a million tokens of each model cost, and what
//! a CI minute and a human hour cost. It lives in `app_meta` under `rate_card`.
//! Tack prices; prill and the harnesses do not, and a harness's own `cost_usd`
//! is never read.

use std::collections::BTreeMap;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

const KEY: &str = "rate_card";

/// Dollars per million tokens of one model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RateCard {
    /// The date the prices were read, as the operator wrote it.
    pub as_of: String,
    /// Price per model string, exactly as a run names its model.
    pub models: BTreeMap<String, ModelPrice>,
    pub ci_minute: f64,
    pub human_hour: f64,
}

/// What `tokens_in` and `tokens_out` of `model` cost under `card`; `None` when
/// the card does not list the model.
pub fn price(card: &RateCard, model: &str, tokens_in: i64, tokens_out: i64) -> Option<f64> {
    let price = card.models.get(model)?;
    Some(
        (tokens_in as f64 * price.input_per_mtok + tokens_out as f64 * price.output_per_mtok)
            / 1_000_000.0,
    )
}

/// The stored card, or `None` when the operator never entered one.
pub async fn load(pool: &sqlx::SqlitePool) -> Option<RateCard> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|raw| serde_json::from_str(&raw).ok())
}

/// GET /api/settings/rate-card
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/settings/rate-card",
    tag = "settings",
    responses((status = 200, description = "The rate card, or null when none was entered", body = Option<RateCard>)),
)]
pub async fn get_rate_card(State(state): State<AppState>) -> Json<Option<RateCard>> {
    Json(load(state.pool()).await)
}

/// PUT /api/settings/rate-card
#[instrument(skip(state, card))]
#[utoipa::path(
    put,
    path = "/api/settings/rate-card",
    tag = "settings",
    request_body = RateCard,
    responses(
        (status = 200, description = "The saved rate card", body = RateCard),
        (status = 422, description = "A price is negative or not a number", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_rate_card(
    State(state): State<AppState>,
    Json(card): Json<RateCard>,
) -> ApiResult<Json<RateCard>> {
    let prices = card
        .models
        .values()
        .flat_map(|p| [p.input_per_mtok, p.output_per_mtok])
        .chain([card.ci_minute, card.human_hour]);
    if prices.into_iter().any(|p| !p.is_finite() || p < 0.0) {
        return Err(ApiError::Unprocessable(
            "every price must be a number that is not negative".into(),
        ));
    }
    let value = serde_json::to_string(&card).map_err(|e| ApiError::Internal(e.into()))?;
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(KEY)
    .bind(value)
    .execute(state.pool())
    .await?;
    Ok(Json(card))
}
