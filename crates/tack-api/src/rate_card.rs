//! The rate card's routes. The card and its pricing are
//! `tack_core::rate_card`; where it is kept is `tack_db::repo::rate_card`.

use axum::{Json, extract::State};
use tack_db::Repository;
use tracing::instrument;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

pub use tack_core::rate_card::{ModelPrice, RateCard, price};

/// The stored card, or `None` when the operator never entered one.
pub async fn load(repo: &Repository) -> Option<RateCard> {
    tack_db::queries::rate_card::stored_card(repo)
        .await
        .ok()
        .flatten()
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
    Json(load(&state.repo).await)
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
    state.repo.save_rate_card(&card).await?;
    Ok(Json(card))
}
