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

/// Models observed for a harness/provider pair. Model IDs are deliberately
/// opaque: their punctuation and prefixes are not a compatibility contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCombination {
    pub model_provider: ModelProvider,
    pub model_ids: Vec<ModelId>,
    pub discovery: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_model_ids_and_additive_fields_round_trip() {
        let raw = r#"{
          "protocol_version":1,"runner_version":"0.1.0","reported_at":"2026-08-06T12:00:00Z",
          "labels":{},"concurrency":{"total":1,"available":1},"harnesses":[{
            "harness_kind":"codex","installed_version":"1","probe_error":null,
            "probed_at":"2026-08-06T12:00:00Z","model_combinations":[{
              "model_provider":"openai","model_ids":["opaque/model-alpha"],"discovery":"reported","future_combo":true
            }]
          }],"features":{
            "cancel":{"support":"supported","reason":null},"resume":{"support":"unsupported","reason":"no"},
            "decisions":{"support":"supported","reason":null},"artifacts":{"support":"supported","reason":null},
            "usage":{"support":"advisory","reason":"partial"}
          },"limits":{"event_payload_bytes_max":1,"artifact_content_bytes_max":2},"future_capability":{"nested":true}
        }"#;
        let parsed: RunnerCapabilities = serde_json::from_str(raw).expect("capabilities fixture");
        assert_eq!(
            parsed.harnesses[0].model_combinations[0].model_ids[0].as_str(),
            "opaque/model-alpha"
        );
        let round_trip = serde_json::to_value(parsed).expect("serialize");
        assert_eq!(round_trip["future_capability"]["nested"], true);
        assert_eq!(
            round_trip["harnesses"][0]["model_combinations"][0]["future_combo"],
            true
        );
    }
}
