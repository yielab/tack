//! Requested-vs-actual model provenance and the attempt's wall-clock time.
//! Two independent pure concerns, neither performing I/O:
//! [`model_provenance`] surfaces the model the request named next to the one
//! the attempt reports, visible, never silently reconciled;
//! [`wall_clock_ms`] is the time the runner itself witnessed, kept apart from
//! the harness's self-reported token usage. Nothing here is priced.
//!
//! Absent usage is `Measurement { value: None, source: NotMeasured, .. }`,
//! never `0`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::ActualExecution;

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

/// Runner-observed wall-clock time: a fact the runner/API directly witnesses
/// (attempt start/end), never wrapped in a `Measurement` since there is no
/// "estimated" wall clock. Missing timestamps give `None`.
pub fn wall_clock_ms(
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
) -> Option<u64> {
    let (start, end) = (started_at?, ended_at?);
    let millis = end.signed_duration_since(start).num_milliseconds();
    // Clamp rather than panic/underflow on a clock that somehow reports
    // `ended_at` before `started_at` — that is a data anomaly for whoever
    // ingests these facts to investigate, not grounds for this pure function
    // to produce nonsense or crash.
    Some(u64::try_from(millis).unwrap_or(0))
}

/// Every derived fact this module produces for one attempt, in one call.
/// Takes the same raw column shapes `AttemptListingRow` already carries
/// (`actual_execution` as raw JSON text, possibly absent) so a caller can pass
/// real row data through without this module depending on `tack-db`'s row
/// type directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptFacts {
    /// `None` only when the attempt has not yet reported `actual_execution`
    /// (still in flight).
    pub model_provenance: Option<ModelProvenance>,
    /// `None` until the attempt has both started and ended.
    pub wall_clock_ms: Option<u64>,
}

/// Parses raw `execution_attempts` columns into [`AttemptFacts`]. Malformed
/// JSON (a raw `TEXT` column has no schema enforcement) is treated as "not
/// yet reported," never a panic.
pub fn derive_attempt_facts(
    requested_model: Option<&str>,
    actual_execution_json: Option<&str>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
) -> AttemptFacts {
    let actual: Option<ActualExecution> =
        actual_execution_json.and_then(|raw| serde_json::from_str(raw).ok());

    AttemptFacts {
        model_provenance: actual
            .as_ref()
            .map(|actual| model_provenance(requested_model, actual)),
        wall_clock_ms: wall_clock_ms(started_at, ended_at),
    }
}

#[cfg(test)]
#[path = "usage_provenance/tests.rs"]
mod tests;
