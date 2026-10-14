//! The operator's rate card: what a million tokens of each model cost, and what
//! a CI minute and a human hour cost. Tack prices; prill and the harnesses do
//! not, and a harness's own `cost_usd` is never read. The metrics page and the
//! scheduler's dollar budget both price through [`price`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Dollars per million tokens of one model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
