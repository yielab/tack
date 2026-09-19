//! Runner capability snapshots and support declarations.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{HarnessKind, ModelId, ModelProvider, ProtocolVersion};

/// The three support levels fixed by runner protocol v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Advisory,
}

/// A capability value coupled to the reason supplied by the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityValue {
    pub support: CapabilitySupport,
    /// `null` is meaningful fixture data: preserve it instead of silently
    /// omitting the key during a round trip.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Per-feature support statements reported by a runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCapabilities {
    pub cancel: CapabilityValue,
    pub resume: CapabilityValue,
    pub decisions: CapabilityValue,
    pub artifacts: CapabilityValue,
    pub usage: CapabilityValue,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Current and total execution capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concurrency {
    pub total: u32,
    pub available: u32,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A provider's own catalog quote for one model — never invented for a
/// vendor that publishes nothing (ADR 0063 decision 7): an unpublished field
/// stays `None`, never a default or zero. `price`/`modality` are recorded
/// exactly as the provider's catalog shapes them (ADR 0063 decision 5)
/// rather than normalized — vendor catalogs use mutually incompatible
/// shapes for both, and normalizing would silently falsify most of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modality: Option<serde_json::Value>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Models observed for a harness/provider pair. Model IDs are deliberately
/// opaque: their punctuation and prefixes are not a compatibility contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCombination {
    pub model_provider: ModelProvider,
    pub model_ids: Vec<ModelId>,
    pub discovery: String,
    /// Per-model price, context window and modality (ADR 0063 decision 5).
    /// A model absent from this map is one the catalog said nothing about,
    /// not a claim of zero. Absent as a whole field (a pre-metadata runner)
    /// defaults to an empty map and is omitted again on re-serialization, so
    /// an older `capabilities.json` round-trips unchanged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_metadata: BTreeMap<ModelId, ModelMetadata>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// One installed harness and the models it can report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapability {
    pub harness_kind: HarnessKind,
    pub installed_version: String,
    pub probe_error: Option<String>,
    pub probed_at: DateTime<Utc>,
    #[serde(default)]
    pub model_combinations: Vec<ModelCombination>,
    /// Whether this harness accepts an **operator-specified opaque model**
    /// forwarded verbatim by its adapter — a claim about the adapter's own
    /// invocation contract (a bad model fails with the harness's own error,
    /// never a fabricated one), not about which models exist, so adapters
    /// with no enumeration (claude-code, codex) can attest it honestly even
    /// with empty `model_combinations`. The scheduler treats only
    /// `Supported` as schedulable — `Advisory` is rejected like
    /// `Unsupported`; `None` behaves as if the field did not exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_passthrough: Option<CapabilityValue>,
    /// Whether **this harness** can pause a run and ask the operator before
    /// it acts, for a request whose `permission_policy.approvals` is `ask`.
    /// A runner's top-level `features.decisions` only says whether the
    /// protocol path exists on this runner at all; which harness can
    /// actually use it differs per adapter (claude-code's grammar keeps
    /// stdin open and answers a `can_use_tool` question, codex/docket/
    /// opencode never open one), so the scheduler reads this field, not
    /// `features.decisions`, to admit or refuse an `ask` request. Same
    /// absence/support rule as `model_passthrough`: only `Supported`
    /// counts, `Advisory` and an absent attestation both mean "not
    /// attested" and are treated like `Unsupported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<CapabilityValue>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Maximum payload values the runner says it can handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    pub event_payload_bytes_max: u64,
    pub artifact_content_bytes_max: u64,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A complete point-in-time runner capability report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerCapabilities {
    /// Standalone capability reports carry the protocol version; embedded
    /// enrollment/refresh capability snapshots inherit it from the enclosing
    /// protocol message and therefore omit this member. Keep that distinction
    /// on the wire rather than materializing a field on re-serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<ProtocolVersion>,
    pub runner_version: String,
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    pub features: FeatureCapabilities,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A capability snapshot embedded inside `enrollment.request.json` or
/// `refresh.request.json` — a different wire shape than the standalone
/// [`RunnerCapabilities`] report, not a loosened copy of it.
///
/// `runner_version`/`protocol_version` have no field here: both are only
/// *siblings* of `capabilities` in the enclosing envelope. `harnesses`/
/// `features` stay permissive — `refresh.request.json` reports `[]`/`{}`
/// while `enrollment.request.json` reports populated data — so `features`
/// is opaque `serde_json::Value` rather than [`FeatureCapabilities`].
/// Unrecognised keys round-trip via `serde(flatten)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedCapabilitySnapshot {
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    #[serde(default)]
    pub features: serde_json::Value,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
#[path = "capabilities/tests.rs"]
mod tests;
