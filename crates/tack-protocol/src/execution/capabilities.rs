//! Runner capability snapshots and support declarations.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{HarnessKind, ProtocolVersion};

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

/// The key under which a harness that speaks a versioned API (docket) reports it, beside
/// `installed_version`, which holds only the program's own version. It rides in
/// `additional`, so an older runner or reader that does not know it loses nothing.
const API_VERSION_KEY: &str = "api_version";

/// One installed harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapability {
    pub harness_kind: HarnessKind,
    pub installed_version: String,
    pub probe_error: Option<String>,
    pub probed_at: DateTime<Utc>,
    /// Whether **this harness** can pause a run and ask the operator before
    /// it acts, for a request whose `permission_policy.approvals` is `ask`.
    /// A runner's top-level `features.decisions` only says whether the
    /// protocol path exists on this runner at all; which harness can
    /// actually use it differs per adapter (claude-code's grammar keeps
    /// stdin open and answers a `can_use_tool` question, codex/docket/
    /// opencode never open one), so the scheduler reads this field, not
    /// `features.decisions`, to admit or refuse an `ask` request. Only
    /// `Supported` counts, `Advisory` and an absent attestation both mean
    /// "not attested" and are treated like `Unsupported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<CapabilityValue>,
    /// Whether a request must name a model for this harness, or may leave
    /// the choice to it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requires_model: bool,
    /// Why a request must name a model; present only when it must.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_model_reason: Option<String>,
    /// The models the agent offers. `None` means the runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelsReport>,
    /// The model the agent itself would use, only when the agent names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Whether the agent is signed in. `None` means the runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<SignInReport>,
    /// Where the agent's own documentation covers its models and its login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocsLinks>,
    /// The most autonomy the harness's teams allow, `"L3"`; absent when the
    /// runner did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy_ceiling: Option<String>,
    /// Each team the harness runs, with what keeps it from running lights-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<Vec<TeamReport>>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl HarnessCapability {
    /// The API version the harness answers, when it has one: `"5"`, not `"API 5"`.
    pub fn api_version(&self) -> Option<&str> {
        self.additional.get(API_VERSION_KEY)?.as_str()
    }

    /// Records the API version the harness answers; see [`Self::api_version`].
    pub fn set_api_version(&mut self, version: &str) {
        self.additional
            .insert(API_VERSION_KEY.to_owned(), serde_json::json!(version));
    }
}

/// One team a harness runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamReport {
    pub name: String,
    pub profile: String,
    #[serde(default)]
    pub violations: Vec<TeamViolation>,
    /// The folders the team's sandbox may read besides its own work, as docket resolved them
    /// (its machine-wide grants narrowed by the team's own setting); empty when there are none.
    /// Absent when the harness did not say, which is not the same as none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<Vec<String>>,
}

/// One thing that makes a team need a person during a run, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamViolation {
    pub requirement: String,
    pub fix: String,
}

/// Where a harness's model ids came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelListSource {
    /// Asked of the agent itself.
    Listed,
    /// Names Tack suggests because the agent cannot list its own.
    Suggested,
    /// No ids; `reason` says why.
    None,
}

/// The models a harness offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsReport {
    pub source: ModelListSource,
    pub ids: Vec<String>,
    pub reason: Option<String>,
}

/// Whether a harness has a login it can run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInState {
    SignedIn,
    SignedOut,
    Unknown,
}

/// A harness's login state, and the command that fixes a missing one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInReport {
    pub state: SignInState,
    pub detail: String,
    pub command: Option<String>,
}

/// The agent's own documentation pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsLinks {
    pub models: String,
    pub sign_in: String,
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
