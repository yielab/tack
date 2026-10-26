//! Requested-vs-actual model provenance and honest, provenance-separated
//! usage economics. Two independent pure concerns, neither performing I/O:
//! [`model_provenance`] surfaces the model the request named next to the one
//! the attempt reports, visible, never silently reconciled;
//! [`build_usage_economics`] keeps runner-observed wall-clock
//! time cost structurally separate from the harness's self-reported
//! token/dollar usage, never summed into one opaque number.
//!
//! Every dollar field is named `*_usd_estimated`. Absent usage is
//! `Measurement { value: None, source: NotMeasured, .. }`, never `0`/`0.0`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::{ActualExecution, Measurement, MeasurementSource, Usage};

/// What an execution request asked for vs. what an attempt ran on: both
/// model strings in full, and who named the served one (`reported`,
/// `requested` or `agent_default`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelProvenance {
    /// The model the request named; `null` when it named none.
    pub requested: Option<String>,
    /// The model the attempt ran on; `null` when nothing named one.
    pub served: Option<String>,
    /// Who named `served`: `reported` (the agent's own output), `requested`
    /// (only the request) or `agent_default` (neither).
    pub source: String,
}

/// Pairs the request's model with the attempt's reported one, unmodified.
pub fn model_provenance(requested: Option<&str>, actual: &ActualExecution) -> ModelProvenance {
    ModelProvenance {
        requested: requested.map(str::to_owned),
        served: actual.model.clone(),
        source: actual.model_source.clone(),
    }
}

fn not_measured() -> Measurement<f64> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: Default::default(),
    }
}

/// Runner-observed wall-clock time cost — entirely different provenance
/// from the harness's self-reported [`Usage`]. `wall_clock_ms` is a fact the
/// runner/API directly witnesses (attempt start/end), never wrapped in a
/// [`Measurement`] since there is no "estimated" wall clock. No infra rate
/// is stored anywhere in this schema today, so `cost_usd_estimated` stays
/// `not_measured` unless a caller supplies `runner_rate_usd_per_hour`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RunnerTimeCost {
    pub wall_clock_ms: Option<u64>,
    pub cost_usd_estimated: Measurement<f64>,
}

/// Computes [`RunnerTimeCost`] from an attempt's observed start/end times
/// and an optional infra rate. Never fabricates a value: missing timestamps
/// leave `wall_clock_ms: None`; a missing rate leaves `cost_usd_estimated`
/// `not_measured` — it is never assumed to be `0.0`.
pub fn compute_runner_time_cost(
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> RunnerTimeCost {
    let wall_clock_ms = match (started_at, ended_at) {
        (Some(start), Some(end)) => {
            let millis = end.signed_duration_since(start).num_milliseconds();
            // Clamp rather than panic/underflow on a clock that somehow
            // reports `ended_at` before `started_at` — that is a data
            // anomaly for whoever ingests these facts to investigate, not
            // grounds for this pure function to produce nonsense or crash.
            Some(u64::try_from(millis).unwrap_or(0))
        }
        _ => None,
    };
    let cost_usd_estimated = match (wall_clock_ms, runner_rate_usd_per_hour) {
        (Some(ms), Some(rate)) => Measurement {
            value: Some((ms as f64 / 3_600_000.0) * rate),
            source: MeasurementSource::Estimated,
            additional: Default::default(),
        },
        _ => not_measured(),
    };
    RunnerTimeCost {
        wall_clock_ms,
        cost_usd_estimated,
    }
}

/// Two independently-provenanced dollar dimensions, deliberately never
/// summed into one figure — see this module's doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UsageEconomics {
    /// Pass-through of `Usage.cost_usd` — the harness/vendor's own
    /// self-reported dollar figure, provenance (`measured` / `estimated` /
    /// `not_measured`) preserved verbatim, never reinterpreted.
    pub model_token_cost_usd_estimated: Measurement<f64>,
    /// This module's own derived dimension — see [`RunnerTimeCost`].
    pub runner_time_cost: RunnerTimeCost,
}

/// Builds [`UsageEconomics`] from a completion's optional [`Usage`] report
/// and an attempt's observed start/end times. `usage: None` (the attempt
/// has not completed, or the harness reported none at all) yields
/// `model_token_cost_usd_estimated`'s `not_measured` — never a fabricated
/// zero.
pub fn build_usage_economics(
    usage: Option<&Usage>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> UsageEconomics {
    let model_token_cost_usd_estimated = usage
        .map(|usage| usage.cost_usd.clone())
        .unwrap_or_else(not_measured);
    UsageEconomics {
        model_token_cost_usd_estimated,
        runner_time_cost: compute_runner_time_cost(started_at, ended_at, runner_rate_usd_per_hour),
    }
}

/// Every derived fact this module produces for one attempt, in one call.
/// Takes the same raw column shapes `AttemptListingRow` already carries
/// (`actual_execution`/`usage` as raw JSON text, possibly absent) so a
/// caller can pass real row data through without this module depending on
/// `tack-db`'s row type directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptFacts {
    /// `None` only when the attempt has not yet reported `actual_execution`
    /// (still in flight).
    pub model_provenance: Option<ModelProvenance>,
    pub usage_economics: UsageEconomics,
}

/// Parses raw `execution_attempts` columns into [`AttemptFacts`]. Malformed
/// JSON (a raw `TEXT` column has no schema enforcement) is treated as "not
/// yet reported," never a panic.
#[allow(clippy::too_many_arguments)]
pub fn derive_attempt_facts(
    requested_model: Option<&str>,
    actual_execution_json: Option<&str>,
    usage_json: Option<&str>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> AttemptFacts {
    let actual: Option<ActualExecution> =
        actual_execution_json.and_then(|raw| serde_json::from_str(raw).ok());
    let usage: Option<Usage> = usage_json.and_then(|raw| serde_json::from_str(raw).ok());

    let model_provenance = actual
        .as_ref()
        .map(|actual| model_provenance(requested_model, actual));

    AttemptFacts {
        model_provenance,
        usage_economics: build_usage_economics(
            usage.as_ref(),
            started_at,
            ended_at,
            runner_rate_usd_per_hour,
        ),
    }
}

#[cfg(test)]
#[path = "usage_provenance/tests.rs"]
mod tests;
