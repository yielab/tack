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

/// A capability snapshot embedded inside `enrollment.request.json` or
/// `refresh.request.json`, distinct from a standalone [`RunnerCapabilities`]
/// report (`capabilities.json`).
///
/// This is a different wire shape, not a loosened copy of the standalone
/// one — each field's strictness follows directly from what the two
/// embedding fixtures actually contain:
///
/// - `runner_version` and `protocol_version` have **no field here at all**.
///   Both are present only as *siblings* of `capabilities` in the enclosing
///   enrollment/refresh envelope, never nested inside it, so there is
///   nothing on the wire to default or make optional — a field for either
///   would just always be absent. This is why [`RunnerCapabilities`] (whose
///   `runner_version` is required and has no `serde(default)`, by design —
///   see its own doc comment) cannot parse this shape, and why widening
///   that field there was rejected in favor of this additive type.
/// - `concurrency` and `labels` stay structurally required/typed, matching
///   what `validate_capability_payload` in
///   `crates/tack-api/src/handlers/runner_protocol.rs` already enforces by
///   hand: it errors on a missing or malformed `concurrency`, and rejects a
///   non-object `labels` or non-string label value. Reusing [`Concurrency`]
///   here gives that same shape a real type instead of a second hand-rolled
///   check.
/// - `harnesses` and `features` stay permissive. `refresh.request.json`'s
///   example reports `"harnesses": []` and `"features": {}`, while
///   `enrollment.request.json`'s reports a populated harness list and full
///   per-feature support statements — so `features` is opaque
///   `serde_json::Value` rather than [`FeatureCapabilities`] (whose five
///   support fields are all required, correctly, for the terminal
///   `capability_snapshot` use at completion) and `harnesses` defaults to
///   empty rather than requiring the full [`HarnessCapability`] list.
/// - `reported_at` and `limits` appear, identically shaped, in both
///   fixtures, so they stay required and reuse [`CapabilityLimits`] rather
///   than being loosened without evidence.
/// - Unrecognised keys are preserved via `serde(flatten)`, matching every
///   other additive type in this module.
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

    /// `RunnerCapabilities` cannot parse either embedded snapshot: both
    /// fixtures omit `runner_version` (a sibling field in the enclosing
    /// envelope, not nested under `capabilities`), which `RunnerCapabilities`
    /// requires. `EmbeddedCapabilitySnapshot` is the type for this shape;
    /// this test proves it parses enrollment's full example and refresh's
    /// sparse one (`"harnesses": []`, `"features": {}`) unchanged, and that
    /// unknown keys still survive a round trip.
    #[test]
    fn embedded_capability_snapshot_parses_full_and_sparse_fixtures() {
        let enrollment: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/enrollment.request.json"
        ))
        .expect("enrollment fixture JSON");
        let enrollment_capabilities = enrollment["capabilities"].clone();
        assert!(
            serde_json::from_value::<RunnerCapabilities>(enrollment_capabilities.clone()).is_err(),
            "enrollment's embedded snapshot omits runner_version and must not parse as \
             RunnerCapabilities"
        );
        let parsed_enrollment: EmbeddedCapabilitySnapshot =
            serde_json::from_value(enrollment_capabilities.clone())
                .expect("enrollment embedded capabilities");
        assert_eq!(
            serde_json::to_value(&parsed_enrollment).expect("serialize enrollment capabilities"),
            enrollment_capabilities,
            "enrollment's full embedded snapshot must round-trip exactly"
        );
        assert_eq!(parsed_enrollment.harnesses.len(), 1);
        assert_eq!(parsed_enrollment.features["cancel"]["support"], "supported");

        let refresh: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/refresh.request.json"
        ))
        .expect("refresh fixture JSON");
        let refresh_capabilities = refresh["capabilities"].clone();
        assert!(
            serde_json::from_value::<RunnerCapabilities>(refresh_capabilities.clone()).is_err(),
            "refresh's embedded snapshot omits runner_version and must not parse as \
             RunnerCapabilities"
        );
        let parsed_refresh: EmbeddedCapabilitySnapshot =
            serde_json::from_value(refresh_capabilities.clone())
                .expect("refresh embedded capabilities");
        assert_eq!(
            serde_json::to_value(&parsed_refresh).expect("serialize refresh capabilities"),
            refresh_capabilities,
            "refresh's sparse embedded snapshot must round-trip exactly"
        );
        assert!(parsed_refresh.harnesses.is_empty());
        assert_eq!(parsed_refresh.features, serde_json::json!({}));

        let mut with_future_field = enrollment_capabilities.clone();
        with_future_field["future_capability_field"] = serde_json::json!({"nested": true});
        let parsed_additive: EmbeddedCapabilitySnapshot =
            serde_json::from_value(with_future_field.clone()).expect("parse additive field");
        assert_eq!(
            serde_json::to_value(parsed_additive).expect("serialize additive field"),
            with_future_field,
            "unrecognised keys on an embedded snapshot must survive a round trip"
        );
    }
}
