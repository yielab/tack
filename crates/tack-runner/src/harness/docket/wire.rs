//! The shapes of the answers docket's operator API sends and this harness reads, as docket's
//! `operator-api.spec.md` writes them. A field the spec says may be absent or null is an
//! `Option` (or a `Vec` that defaults to empty); every other field is required, so an answer
//! that is not what the spec describes is an error here and not a value invented later.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// docket's `{"error": "<sentence>"}`.
#[derive(Debug, Deserialize)]
pub(super) struct ErrorBody {
    pub error: Option<String>,
}

/// `GET /status.json`. `apiVersion` is a string on the wire; [`super::probe`] turns it into the
/// number it compares.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusAnswer {
    pub api_version: String,
    pub version: Option<String>,
    #[serde(default)]
    pub agents: Vec<Agent>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Agent {
    pub model: Option<String>,
}

/// `GET /pods`.
#[derive(Debug, Deserialize)]
pub(super) struct PodList {
    pub pods: Vec<PodRow>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PodRow {
    pub name: String,
}

/// `GET /pods/<pod>`: the keys Tack reads of it. `readOnly` is absent from an older docket.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodReport {
    pub profile: Profile,
    pub read_only: Option<Vec<String>>,
}

/// The verdict object of a pod's profile; `violations` is empty unless the profile is
/// `lights-out`.
#[derive(Debug, Deserialize)]
pub(super) struct Profile {
    pub name: String,
    pub violations: Vec<Violation>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Violation {
    pub requirement: String,
    pub fix: String,
}

/// `POST /tasks/<pod>` that created or found a task.
#[derive(Debug, Deserialize)]
pub(super) struct Enqueued {
    pub task: String,
}

/// Token counts as docket writes them for a hop, a task or one model of a task. `input`
/// includes the cache reads and writes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Tokens {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached: Option<u64>,
    pub cache_write: Option<u64>,
}

/// A task's `usage`: its own totals, which docket writes when the task settles, and the same
/// per model.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TaskUsage {
    #[serde(flatten)]
    pub total: Tokens,
    #[serde(default)]
    pub by_model: BTreeMap<String, Tokens>,
}

/// `GET /tasks/<pod>/<id>`: the keys Tack reads of the task record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TaskView {
    pub status: String,
    /// Why a failed or blocked task ended; empty or absent for any other.
    pub reason: Option<String>,
    pub failure_kind: Option<String>,
    pub blocked_reason: Option<String>,
    /// Absent for a task that ran in place.
    pub worktree: Option<Worktree>,
    pub usage: Option<TaskUsage>,
    #[serde(default)]
    pub hops: Vec<TaskHop>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Worktree {
    pub branch: String,
    pub base_commit: Option<String>,
}

/// A hop of the task record. Every key after `model` is absent from a hop recorded by an older
/// docket, and docket adds keys in minor versions, so none is required and none is refused.
#[derive(Debug, Deserialize)]
pub(super) struct TaskHop {
    pub role: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub verdict: Option<String>,
    #[serde(default, rename = "promptManifest")]
    pub prompt_manifest: Option<PromptManifest>,
    #[serde(default)]
    pub artifact: Option<HopArtifact>,
}

/// The prompts a hop's turn was composed from (`promptManifest`), in composition order, and the
/// model's family and class (its `profile` is not kept).
#[derive(Debug, Default, Deserialize)]
pub(super) struct PromptManifest {
    #[serde(default)]
    pub layers: Vec<PromptLayer>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub class: Option<String>,
}

/// One prompt of a manifest. `sha256` is the hash of its text as the prompt's file holds it and
/// `tokens` the compiler's estimate, not a measurement.
#[derive(Debug, Deserialize)]
pub(super) struct PromptLayer {
    pub layer: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub tokens: Option<u64>,
}

/// What a hop's `handoff` call said: the keys Tack reads of the hop's `artifact`. A list docket
/// has nothing for is absent.
#[derive(Debug, Default, Deserialize)]
pub(super) struct HopArtifact {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub verdict: Option<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub questions: Vec<String>,
}

/// `GET /tasks/<pod>/<id>/diff`.
#[derive(Debug, Deserialize)]
pub(super) struct TaskDiff {
    pub untracked: Vec<String>,
}

/// `GET /traces/<pod>`: each event is a JSON document in a string.
#[derive(Debug, Deserialize)]
pub(super) struct TracePage {
    pub events: Vec<String>,
    pub next: Option<String>,
}

/// One trace event: its type and the payload that type carries. Any other type docket writes is
/// [`TraceEvent::Other`].
#[derive(Debug)]
pub(super) enum TraceEvent {
    HopStarted(HopEvent),
    HopFinished(HopEvent),
    EscalationAsked(EscalationEvent),
    EscalationResolved(EscalationEvent),
    EscalationExpired(EscalationEvent),
    EscalationDecided(EscalationEvent),
    Other,
}

/// A line of the trace as docket writes it, before its payload is read as the type names.
#[derive(Deserialize)]
struct TraceLine {
    event_type: String,
    #[serde(default)]
    payload: serde_json::Value,
}

impl TraceEvent {
    /// The event a line of the trace page is.
    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        let TraceLine {
            event_type,
            payload,
        } = serde_json::from_str(line)?;
        Ok(match event_type.as_str() {
            "hop_started" => Self::HopStarted(serde_json::from_value(payload)?),
            "hop_finished" => Self::HopFinished(serde_json::from_value(payload)?),
            "escalation_asked" => Self::EscalationAsked(serde_json::from_value(payload)?),
            "escalation_resolved" => Self::EscalationResolved(serde_json::from_value(payload)?),
            "escalation_expired" => Self::EscalationExpired(serde_json::from_value(payload)?),
            "escalation_decided" => Self::EscalationDecided(serde_json::from_value(payload)?),
            _ => Self::Other,
        })
    }

    /// The task the event is about, when it is about one Tack follows.
    pub fn task(&self) -> Option<&str> {
        match self {
            Self::HopStarted(hop) | Self::HopFinished(hop) => Some(&hop.task),
            Self::EscalationAsked(event)
            | Self::EscalationResolved(event)
            | Self::EscalationExpired(event)
            | Self::EscalationDecided(event) => Some(&event.task),
            Self::Other => None,
        }
    }
}

/// The payload of `hop_started` and `hop_finished`; the keys after `cycle` are the finished
/// hop's.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HopEvent {
    pub task: String,
    pub role: String,
    pub model: Option<String>,
    pub cycle: Option<u64>,
    pub ok: Option<bool>,
    pub verdict: Option<String>,
    pub gate: Option<String>,
    pub error: Option<String>,
    pub usage: Option<Tokens>,
    pub duration_ms: Option<u64>,
}

/// The payload of the `escalation_*` events: which escalation, and for a resolution the option
/// chosen.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EscalationEvent {
    pub task: String,
    pub escalation: Option<String>,
    pub option_id: Option<String>,
    pub prompt: Option<String>,
}

/// `GET /escalations`. Each record is read on its own, so one Tack cannot read is reported and
/// the rest still are.
#[derive(Debug, Deserialize)]
pub(super) struct EscalationList {
    pub escalations: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum EscalationState {
    Pending,
    Expired,
    /// Resolved, decided by a policy or any other state past waiting.
    #[serde(other)]
    Settled,
}

/// An `EscalationView`: the keys Tack reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Escalation {
    pub id: String,
    pub kind: String,
    pub state: EscalationState,
    pub seq: u64,
    pub prompt: Option<String>,
    pub options: Vec<EscalationOption>,
    pub recommendation: Option<EscalationRecommendation>,
    pub requested_schema: Option<RequestedSchema>,
    /// Null for a question docket never expires.
    pub expires_at: Option<DateTime<Utc>>,
    pub step: Option<String>,
    pub role: Option<String>,
    pub policy: Option<String>,
    pub level: Option<String>,
    pub affordances: Affordances,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EscalationOption {
    pub id: String,
    pub label: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    pub estimated_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EscalationRecommendation {
    pub option_id: String,
    pub rationale: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// The JSON Schema of the answer's `content`: the property names and which are required.
#[derive(Debug, Default, Deserialize)]
pub(super) struct RequestedSchema {
    #[serde(default)]
    pub properties: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub required: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Affordances {
    pub accepts_text: bool,
}
