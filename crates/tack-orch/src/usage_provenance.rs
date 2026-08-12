//! Requested-vs-actual model provenance and honest, provenance-separated
//! usage economics (`TODO.md` Part III, card III-F3).
//!
//! Two independent pure concerns, neither performing I/O:
//!
//! - [`compare_model_provenance`]: the request's resolved model (or "no
//!   model requested", i.e. auto-select) against the attempt's
//!   `ActualExecution` observation — visible, never silently reconciled
//!   (this card's acceptance bar: "requested/actual mismatch visible").
//! - [`build_usage_economics`]: keeps runner-observed wall-clock time cost
//!   structurally separate from the harness/vendor's own self-reported
//!   token/dollar usage — never summed into one opaque number (this card's
//!   task list: "runner time cost separate from model/token cost").
//!
//! Every dollar-valued field here is named `*_usd_estimated`, matching this
//! crate's own module-level convention (`crate::lib`'s "Money is always an
//! estimate"). Absent usage is `Measurement { value: None, source:
//! NotMeasured, .. }`, never a fabricated `0`/`0.0` — see this module's
//! `absent_usage_never_serializes_as_zero` test, which asserts the literal
//! JSON shape rather than trusting the type alone.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, Measurement, MeasurementSource,
    RequestedModelId, RequestedModelProvider, Usage,
};

/// The comparison between what an execution request asked for and what an
/// attempt actually ran on. All three variants carry the full observed
/// facts — never coalesced into a bare boolean "matched" flag, so a caller
/// (F4's frontend rendering, in particular) can show *both* sides of a
/// mismatch rather than just "something changed."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelProvenance {
    /// The attempt ran on exactly the requested provider/model.
    Matched { provider: String, model_id: String },
    /// The request allowed auto-selection (no explicit provider/model was
    /// ever resolved for it) and the attempt observed a concrete choice.
    /// Distinct from [`Self::Matched`] — nothing was requested to match
    /// against — and distinct from [`Self::Mismatched`] — nothing was
    /// contradicted, since nothing specific was asked for.
    AutoSelectObserved {
        actual_provider: String,
        actual_model_id: String,
    },
    /// The attempt ran on a provider and/or model different from what was
    /// explicitly requested. Both sides are carried in full.
    Mismatched {
        requested_provider: String,
        requested_model_id: String,
        actual_provider: String,
        actual_model_id: String,
    },
}

/// Compares a resolved request (`None` for auto-select — III.1.2's
/// nullable-pair shape) against an attempt's actually-observed model.
/// Compares via `.as_str()`, never by unwrapping into a shared type — the
/// *requested* namespace ([`RequestedModelProvider`]/[`RequestedModelId`])
/// and the *actual* namespace ([`ActualModelProvider`]/[`ActualModelId`])
/// stay textually distinct types all the way through (III.0's vocabulary
/// rule), exactly as `crate::scheduler::select::evaluate_candidate` already
/// does for requested-vs-declared.
pub fn compare_model_provenance(
    requested: Option<(&RequestedModelProvider, &RequestedModelId)>,
    actual_provider: &ActualModelProvider,
    actual_model_id: &ActualModelId,
) -> ModelProvenance {
    match requested {
        None => ModelProvenance::AutoSelectObserved {
            actual_provider: actual_provider.as_str().to_string(),
            actual_model_id: actual_model_id.as_str().to_string(),
        },
        Some((provider, model_id))
            if provider.as_str() == actual_provider.as_str()
                && model_id.as_str() == actual_model_id.as_str() =>
        {
            ModelProvenance::Matched {
                provider: actual_provider.as_str().to_string(),
                model_id: actual_model_id.as_str().to_string(),
            }
        }
        Some((provider, model_id)) => ModelProvenance::Mismatched {
            requested_provider: provider.as_str().to_string(),
            requested_model_id: model_id.as_str().to_string(),
            actual_provider: actual_provider.as_str().to_string(),
            actual_model_id: actual_model_id.as_str().to_string(),
        },
    }
}

fn not_measured() -> Measurement<f64> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: Default::default(),
    }
}

/// Runner-observed wall-clock time cost — a dimension with entirely
/// different provenance from the harness's own self-reported [`Usage`]:
///
/// - `wall_clock_ms` is a fact the runner/API directly witnesses (attempt
///   `started_at`/`ended_at`, `execution_attempts` columns, migration 045)
///   — always derivable once both are known, never itself wrapped in a
///   [`Measurement`] (there is no "estimated" wall clock; it either is or
///   is not known yet).
/// - `cost_usd_estimated` stays `not_measured` unless a caller supplies an
///   infra rate. **No such rate is stored anywhere in this schema today**
///   — see this card's handoff, "Schema/API/contract change requested."
///   `runner_rate_usd_per_hour` is therefore always caller-supplied, never
///   invented by this module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Every derived fact this card produces for one attempt, in one call —
/// the "repository/service handler" convenience this card owns. Takes the
/// same raw column shapes `tack_db::repo::execution::AttemptListingRow`
/// already carries (`actual_execution`/`usage` as raw JSON text, possibly
/// absent) plus the request's resolved requested provider/model, so a
/// caller (a future handler) can pass real row data straight through
/// without this module depending on `tack-db`'s row type directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptFacts {
    /// `None` only when the attempt has not yet reported `actual_execution`
    /// (still in flight) — distinct from a comparison result, which always
    /// has a matched/auto/mismatched answer once actual data exists.
    pub model_provenance: Option<ModelProvenance>,
    pub usage_economics: UsageEconomics,
}

/// Parses raw `execution_attempts` columns and produces [`AttemptFacts`].
/// Malformed JSON in `actual_execution_json`/`usage_json` (should not
/// happen — both are written by this codebase's own completion handler —
/// but a raw `TEXT` column has no schema enforcement) is treated the same
/// as "not yet reported," never a panic.
#[allow(clippy::too_many_arguments)]
pub fn derive_attempt_facts(
    requested_provider: Option<&str>,
    requested_model_id: Option<&str>,
    actual_execution_json: Option<&str>,
    usage_json: Option<&str>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> AttemptFacts {
    let actual: Option<ActualExecution> =
        actual_execution_json.and_then(|raw| serde_json::from_str(raw).ok());
    let usage: Option<Usage> = usage_json.and_then(|raw| serde_json::from_str(raw).ok());

    let model_provenance = actual.as_ref().map(|actual| {
        let requested = match (requested_provider, requested_model_id) {
            (Some(provider), Some(model_id)) => Some((
                RequestedModelProvider::new(provider),
                RequestedModelId::new(model_id),
            )),
            _ => None,
        };
        compare_model_provenance(
            requested
                .as_ref()
                .map(|(provider, model_id)| (provider, model_id)),
            &actual.model_provider,
            &actual.model_id,
        )
    });

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
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn actual_model(provider: &str, model_id: &str) -> (ActualModelProvider, ActualModelId) {
        (
            ActualModelProvider::new(provider),
            ActualModelId::new(model_id),
        )
    }

    #[test]
    fn matched_when_requested_equals_actual() {
        let requested = (
            RequestedModelProvider::new("anthropic"),
            RequestedModelId::new("opaque/sonnet"),
        );
        let (actual_provider, actual_model_id) = actual_model("anthropic", "opaque/sonnet");
        let provenance = compare_model_provenance(
            Some((&requested.0, &requested.1)),
            &actual_provider,
            &actual_model_id,
        );
        assert_eq!(
            provenance,
            ModelProvenance::Matched {
                provider: "anthropic".to_string(),
                model_id: "opaque/sonnet".to_string(),
            }
        );
    }

    /// The card's own acceptance bar: "requested/actual mismatch visible."
    /// Both sides must be present in the result, not silently reconciled to
    /// whichever the caller might expect.
    #[test]
    fn mismatch_carries_both_requested_and_actual_values() {
        let requested = (
            RequestedModelProvider::new("anthropic"),
            RequestedModelId::new("opaque/sonnet"),
        );
        let (actual_provider, actual_model_id) = actual_model("anthropic", "opaque/haiku");
        let provenance = compare_model_provenance(
            Some((&requested.0, &requested.1)),
            &actual_provider,
            &actual_model_id,
        );
        assert_eq!(
            provenance,
            ModelProvenance::Mismatched {
                requested_provider: "anthropic".to_string(),
                requested_model_id: "opaque/sonnet".to_string(),
                actual_provider: "anthropic".to_string(),
                actual_model_id: "opaque/haiku".to_string(),
            }
        );

        // And visible on the wire too — both sides present simultaneously,
        // not coalesced into one field.
        let json = serde_json::to_value(&provenance).expect("serialize");
        assert_eq!(json["requested_model_id"], "opaque/sonnet");
        assert_eq!(json["actual_model_id"], "opaque/haiku");
        assert_ne!(json["requested_model_id"], json["actual_model_id"]);
    }

    #[test]
    fn auto_select_observed_is_distinct_from_matched_and_mismatched() {
        let (actual_provider, actual_model_id) = actual_model("openai", "opaque/model-alpha");
        let provenance = compare_model_provenance(None, &actual_provider, &actual_model_id);
        assert_eq!(
            provenance,
            ModelProvenance::AutoSelectObserved {
                actual_provider: "openai".to_string(),
                actual_model_id: "opaque/model-alpha".to_string(),
            }
        );
    }

    /// Nonsense ids must appear verbatim on both sides of a mismatch —
    /// never normalized away.
    #[test]
    fn nonsense_ids_round_trip_through_a_mismatch_comparison() {
        let requested = (
            RequestedModelProvider::new("totally-made-up-provider-9000"),
            RequestedModelId::new("totally-made-up-model-9000"),
        );
        let (actual_provider, actual_model_id) = actual_model("openai", "opaque/model-alpha");
        let provenance = compare_model_provenance(
            Some((&requested.0, &requested.1)),
            &actual_provider,
            &actual_model_id,
        );
        assert_eq!(
            provenance,
            ModelProvenance::Mismatched {
                requested_provider: "totally-made-up-provider-9000".to_string(),
                requested_model_id: "totally-made-up-model-9000".to_string(),
                actual_provider: "openai".to_string(),
                actual_model_id: "opaque/model-alpha".to_string(),
            }
        );
    }

    /// The card's other acceptance bar: "absent usage never serializes as
    /// zero." Asserts the literal JSON shape, not just the Rust value, per
    /// CLAUDE.md's "assert the absence directly."
    #[test]
    fn absent_usage_never_serializes_as_zero() {
        let economics = build_usage_economics(None, None, None, None);
        let json = serde_json::to_value(&economics).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "model_token_cost_usd_estimated": {"value": null, "source": "not_measured"},
                "runner_time_cost": {
                    "wall_clock_ms": null,
                    "cost_usd_estimated": {"value": null, "source": "not_measured"}
                }
            })
        );
        // Literal-value sanity check on top of structural equality: no
        // numeric zero anywhere in the serialized economics.
        let raw = json.to_string();
        assert!(
            !raw.contains(":0"),
            "must never encode absent usage as 0: {raw}"
        );
        assert!(
            !raw.contains(":0.0"),
            "must never encode absent usage as 0.0: {raw}"
        );
    }

    /// The positive control for the test above: real inputs must actually
    /// produce real, non-null values — proving the null-everywhere case
    /// above is not simply a vacuous "always null" implementation.
    #[test]
    fn present_usage_and_timestamps_produce_real_values() {
        let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ended = DateTime::parse_from_rfc3339("2026-08-06T12:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let usage = Usage {
            tokens_in: Measurement {
                value: Some(1234),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            tokens_out: Measurement {
                value: Some(456),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            duration_ms: Measurement {
                value: Some(1_800_000),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            cost_usd: Measurement {
                value: Some(0.42),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        };
        let economics = build_usage_economics(Some(&usage), Some(started), Some(ended), Some(3.0));
        assert_eq!(economics.model_token_cost_usd_estimated.value, Some(0.42));
        assert_eq!(
            economics.model_token_cost_usd_estimated.source,
            MeasurementSource::Measured
        );
        assert_eq!(economics.runner_time_cost.wall_clock_ms, Some(1_800_000));
        // 30 minutes at $3.00/hour = $1.50, and this figure is an
        // independent estimate — it must never equal (or be silently
        // summed with) the harness's own $0.42 cost_usd.
        assert_eq!(
            economics.runner_time_cost.cost_usd_estimated.value,
            Some(1.5)
        );
        assert_eq!(
            economics.runner_time_cost.cost_usd_estimated.source,
            MeasurementSource::Estimated
        );
        assert_ne!(
            economics.runner_time_cost.cost_usd_estimated.value,
            economics.model_token_cost_usd_estimated.value
        );
    }

    /// A wall clock is derivable even with no rate configured — it must not
    /// collapse to `not_measured` just because the dollar estimate does.
    #[test]
    fn wall_clock_is_known_even_without_a_configured_rate() {
        let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ended = DateTime::parse_from_rfc3339("2026-08-06T12:00:05Z")
            .unwrap()
            .with_timezone(&Utc);
        let cost = compute_runner_time_cost(Some(started), Some(ended), None);
        assert_eq!(cost.wall_clock_ms, Some(5_000));
        assert_eq!(cost.cost_usd_estimated.value, None);
        assert_eq!(
            cost.cost_usd_estimated.source,
            MeasurementSource::NotMeasured
        );
    }

    #[test]
    fn derive_attempt_facts_treats_malformed_json_as_not_yet_reported() {
        let facts = derive_attempt_facts(
            Some("openai"),
            Some("opaque/model-alpha"),
            Some("not json"),
            Some("also not json"),
            None,
            None,
            None,
        );
        assert_eq!(facts.model_provenance, None);
        assert_eq!(
            facts.usage_economics.model_token_cost_usd_estimated.value,
            None
        );
    }

    #[test]
    fn derive_attempt_facts_end_to_end_with_a_real_completion_fixture() {
        let actual_execution_json =
            include_str!("../../../docs/contracts/runner-v1/completion.request.json");
        let completion: serde_json::Value =
            serde_json::from_str(actual_execution_json).expect("fixture JSON");
        let actual = completion["actual_execution"].to_string();
        let usage = completion["usage"].to_string();
        let facts = derive_attempt_facts(
            Some("openai"),
            Some("opaque/model-alpha"),
            Some(&actual),
            Some(&usage),
            Some(
                DateTime::parse_from_rfc3339("2026-08-06T12:20:05Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            Some(
                DateTime::parse_from_rfc3339("2026-08-06T12:25:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            None,
        );
        assert_eq!(
            facts.model_provenance,
            Some(ModelProvenance::Matched {
                provider: "openai".to_string(),
                model_id: "opaque/model-alpha".to_string(),
            })
        );
        assert_eq!(
            facts.usage_economics.model_token_cost_usd_estimated.value,
            None
        );
        assert_eq!(
            facts.usage_economics.model_token_cost_usd_estimated.source,
            MeasurementSource::NotMeasured
        );
        assert_eq!(
            facts.usage_economics.runner_time_cost.wall_clock_ms,
            Some(295_000)
        );
    }
}
