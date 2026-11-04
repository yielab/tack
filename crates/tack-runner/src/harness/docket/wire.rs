//! The shapes of the answers docket's operator API sends and this harness reads, as docket's
//! `operator-api.spec.md` writes them. A field the spec says may be absent or null is an
//! `Option` (or a `Vec` that defaults to empty); every other field is required, so an answer
//! that is not what the spec describes is an error here and not a value invented later.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

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

/// `GET /health`: the features this docket answers to; an older one sends none.
#[derive(Debug, Deserialize)]
pub(super) struct HealthAnswer {
    #[serde(default)]
    pub features: Vec<String>,
}

/// `GET /pods`.
#[derive(Debug, Deserialize)]
pub(super) struct PodList {
    pub pods: Vec<PodRow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodRow {
    pub name: String,
    /// How many tasks the pod holds in each state; absent from a docket that sends none.
    pub queue: Option<PodQueue>,
    /// `codebase` or `workdir`: where the team works. Absent from a docket before that key.
    pub workspace_kind: Option<String>,
    /// Whether the team claims no task now (`podPause`). Absent from a docket before that key.
    pub paused: Option<bool>,
    /// Why, while `paused`; the key is absent while it runs.
    pub paused_reason: Option<String>,
}

/// The counts of a row's `queue` that Tack reads. A state with no count is zero.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodQueue {
    #[serde(default)]
    pub pending: u64,
    #[serde(default)]
    pub running: u64,
    #[serde(default)]
    pub waiting_approval: u64,
    #[serde(default)]
    pub waiting_input: u64,
}

/// `GET /pods/<pod>`: the keys Tack reads of it. `readOnly` is absent from an older docket.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodReport {
    pub profile: Profile,
    pub read_only: Option<Vec<String>>,
    /// Where the team's configuration of record lives, inside its repository; `""` for a team that
    /// works in a folder (`GET /pods/<pod>` carries no `codebase`; the list does). Absent from a
    /// docket before that was possible.
    pub repo_config_dir: Option<String>,
    /// `codebase` or `workdir`, the same key the row of the list carries. Absent from a docket
    /// before that key; `repoConfigDir` then says where the team works.
    pub workspace_kind: Option<String>,
    /// The team's members. Absent from a docket that sends none.
    pub members: Option<Vec<PodMember>>,
    /// What the team's agents may reach: `none` or `open`.
    pub network: Option<PodNetwork>,
    /// The shape the team was provisioned with. Absent from a docket that does not send it (the
    /// first of the two asks of ADR 0032).
    pub blueprint: Option<String>,
    pub pipeline: Option<PodPipeline>,
    /// The same pair the team's row of the list carries (`podPause`).
    pub paused: Option<bool>,
    pub paused_reason: Option<String>,
    /// The folder a team that works in one writes in (`podWorkDir`). Absent for a team in a
    /// repository and from a docket before the key.
    pub work_dir: Option<String>,
    /// What a task on the team hands back (`podContract`): a list, `branch` or `report` first and
    /// `records` after it when the team keeps record sets. Kept as it came and read on its own, so
    /// a shape Tack cannot read costs the contract's `delivers` and never the team. Absent from a
    /// docket before that key.
    pub delivers: Option<Value>,
    /// Every method a task may name (`podContract`). Each is kept as it came and read on its own,
    /// so a method Tack cannot read costs the list and never the team.
    pub methods: Option<Vec<Value>>,
    /// What the team's work still needs (`podContract`), kept as `methods` is.
    pub needs: Option<Vec<Value>>,
    /// What the team may spend and has (`podResume`), kept as it came and read on its own so a
    /// budget Tack cannot read costs the budget and never the team.
    pub budget: Option<Value>,
    /// How the team reads the web (`webRead`), kept as it came and read on its own so a mode Tack
    /// has no word for costs the sentence and never the team. Absent from a docket before it.
    pub web_read: Option<Value>,
}

/// `GET /pods/<pod>/memory`: the entries, each kept as it came and read on its own so an entry
/// Tack cannot read costs that entry and never the team's others.
#[derive(Debug, Deserialize)]
pub(super) struct MemoryList {
    pub entries: Vec<Value>,
}

/// One entry of a team's memory. `status` is the one the query named, so it is not read.
#[derive(Debug, Deserialize)]
pub(super) struct MemoryItem {
    pub id: String,
    pub kind: String,
    pub subject: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub provenance: std::collections::BTreeMap<String, String>,
    /// `{"exit": n}`, `{"ok": bool}` or `{}`.
    #[serde(default)]
    pub evidence: Option<MemoryProof>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MemoryProof {
    pub exit: Option<i64>,
    pub ok: Option<bool>,
}

/// `budget` of a team's report.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodBudget {
    /// The tokens the team may spend in the window; `null` when it has no limit.
    pub tokens: Option<u64>,
    #[serde(default)]
    pub used: u64,
    /// How long a window lasts: `none` (it never restarts), `day` or `week`.
    pub window: String,
    /// When the current window began; absent for one that never restarts.
    pub window_start: Option<String>,
}

/// One entry of `methods`: its name, where the team has it from, whether a task that names none
/// goes through it, the roles its steps need that the team has no member for, and why its file
/// does not load when it does not.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodMethod {
    pub name: String,
    pub source: String,
    pub default: bool,
    #[serde(default)]
    pub missing_roles: Vec<String>,
    pub error: Option<String>,
}

/// `webRead` of a team's report: docket's own sentence for how the team reads the web.
#[derive(Debug, Deserialize)]
pub(super) struct PodWebRead {
    pub mode: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub fix: String,
}

/// One entry of `needs`.
#[derive(Debug, Deserialize)]
pub(super) struct PodNeed {
    pub kind: String,
    pub name: String,
    pub fix: String,
    pub satisfied: bool,
    /// Whether the work cannot go on without it; absent from a docket before `webRead`.
    #[serde(default)]
    pub blocking: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PodMember {
    pub role: String,
    pub model: Option<PodMemberModel>,
    /// What the member's role reaches, as docket worked it out. Kept as it came and read on its
    /// own ([`MemberReach`]), so a shape Tack cannot read costs the member's reach and never the
    /// team. Absent from a docket before that key.
    pub capabilities: Option<Value>,
}

/// What a member's `capabilities` says that Tack reads: the kinds of tool the role has, the
/// tools it holds, the hosts its `fetch` reads and the record sets it can write. `records.query`
/// and any other key are not read.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MemberReach {
    pub kinds: Option<Vec<String>>,
    /// The tool names the role holds; docket lists `fetch` only while it offers it.
    pub tools: Option<Vec<String>>,
    /// The hosts the member's `fetch` reads (`podFetch`); `*` is any public host. Sent only with
    /// `fetch`.
    pub fetch_domains: Option<Vec<String>>,
    pub records: Option<MemberRecords>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MemberRecords {
    pub write: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PodMemberModel {
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PodNetwork {
    pub mode: String,
    /// Whether a jail holds the team to `mode`; `false` leaves egress open whatever `mode` says.
    /// Absent from a docket that does not send it.
    pub enforced: Option<bool>,
}

/// `pipeline` of a pod's report: `source` is docket's; `steps` is the second ask of ADR 0032 and
/// is absent from a docket that does not send it. Each step is kept as it came and read on its
/// own, so a step Tack cannot read costs the steps and never the team.
#[derive(Debug, Deserialize)]
pub(super) struct PodPipeline {
    pub steps: Option<Vec<Value>>,
}

/// One entry of `pipeline.steps`: its id, its kind, who does it, what checks it and where a failed
/// check sends the work back.
#[derive(Debug, Deserialize)]
pub(super) struct PodStep {
    pub id: String,
    pub kind: String,
    pub role: Option<String>,
    /// The kind of gate that decides the step: `mechanical`, `verdict`, `approval` or `external`.
    pub gate: Option<String>,
    /// Where the work goes back to, as docket lists it: each route has its labels (`when`, not
    /// read), the step it goes to and how many times. Empty or absent for a step that sends
    /// nothing back.
    #[serde(default)]
    pub rework: Vec<PodRework>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PodRework {
    pub to: String,
    pub max: Option<u32>,
}

/// `GET /pods/<pod>/records`: the sets the team declares. `problems` (documents that do not
/// validate) is docket's to show.
#[derive(Debug, Deserialize)]
pub(super) struct RecordList {
    #[serde(default)]
    pub records: Vec<RecordSet>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RecordSet {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// How many rows the set holds in all, tasks included.
    #[serde(default)]
    pub rows: u64,
    /// `pod` (a row's key is unique across the team) or `task` (unique within one task). Absent
    /// from a docket that cannot read a task's rows by `?task=`: that absence is the only thing
    /// that decides how the rows of a task are read.
    #[serde(default)]
    pub scope: Option<String>,
}

/// `GET /pods/<pod>/records/<set>`: the rows are objects keyed by column.
#[derive(Debug, Deserialize)]
pub(super) struct RecordRows {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<Value>,
    /// Whether docket stopped at the limit; it sends a flag, and a count would mean the same.
    #[serde(default)]
    pub cut: Value,
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TaskUsage {
    #[serde(flatten)]
    pub total: Tokens,
    #[serde(default)]
    pub by_model: BTreeMap<String, Tokens>,
}

/// `GET /tasks/<pod>/<id>`: the keys Tack reads of the task record.
#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Worktree {
    pub branch: String,
    pub base_commit: Option<String>,
}

/// A hop of the task record. Every key after `model` is absent from a hop recorded by an older
/// docket, and docket adds keys in minor versions, so none is required and none is refused.
#[derive(Debug, Clone, Deserialize)]
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
    /// `hopTimes`: the instants (ISO 8601 UTC) of a hop that ran an agent turn and the rework
    /// cycle it ran in. A command, gate, wait or resolution hop, and a hop recorded before the
    /// feature, carry none of the three.
    #[serde(default, rename = "startedAt")]
    pub started_at: Option<String>,
    #[serde(default, rename = "finishedAt")]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub cycle: Option<u32>,
    /// `budgetOutcome`: the hop stopped because a step's own budget ran out, not because the
    /// model settled. Absent for any hop that is not that, and from a docket before the feature.
    #[serde(default)]
    pub budget: Option<BudgetOutcome>,
    /// `capabilityMissing`: the gap in the team's setup that stopped the hop, absent for any hop
    /// that met none and from a docket before the feature.
    #[serde(default, rename = "capabilityMissing")]
    pub capability_missing: Option<CapabilityMissing>,
    /// The hop's own raw stop text. For a budget stop this already carries docket's own
    /// `"(source: <source>; raise <key>)"` tail (`agent/loop_types.py::limit_origin`): Tack reads
    /// that tail for where to raise the limit instead of a table of its own, so a key docket
    /// renames or retires (ADR 0038 decision 5) never goes stale on this side. Empty when the hop
    /// wrote none.
    #[serde(default)]
    pub error: String,
}

/// What stopped a hop under `budgetOutcome`: the bound that ran out, the limit and what was used
/// of it, who set the limit (`step`, `role`, `pod`, `default`, or `task` for a task-wide token
/// cut; any other value is docket naming a source Tack does not know yet), and whether the
/// stopped hop's work stayed on the task's branch. `bound` is one of `rounds`, `toolCalls`,
/// `tokens` or `time`, kept as a string rather than a closed enum so a fifth docket adds later
/// still deserializes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BudgetOutcome {
    pub bound: String,
    pub limit: u64,
    pub used: u64,
    pub source: String,
    pub tree_changed: bool,
    /// The one key that raises this bound (docket ADR 0038 decision 5, planned as P59-6, not yet
    /// on the wire): `timeout`, `turnTimeoutS`, `maxTokens`, `budgetTokens`, or a machine variable
    /// for `source: default`. Absent from a docket before the field; Tack then falls back to
    /// parsing the same key out of the hop's own stop text (`error`), and failing that, to a
    /// source-only sentence.
    #[serde(default)]
    pub raise: Option<String>,
}

/// The prompts a hop's turn was composed from (`promptManifest`), in composition order, and the
/// model's family and class (its `profile` is not kept).
#[derive(Debug, Clone, Default, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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

/// What docket names as missing from the team's setup: `tool`, `records` or `fetch` (a string, so
/// a kind docket adds later still reads), the name of what is missing and the change that gives it.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct CapabilityMissing {
    pub kind: String,
    pub name: String,
    pub fix: String,
}

/// What a hop's `handoff` call said: the keys Tack reads of the hop's `artifact`. A list docket
/// has nothing for is absent.
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct HopArtifact {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub verdict: Option<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub questions: Vec<String>,
    /// The files the hop's work changed; docket keeps them for a hop that failed and still kept
    /// its work (an Implementer stopped by a budget, its transport or a gap in the team's setup).
    #[serde(default, alias = "filesChanged")]
    pub files_changed: Vec<String>,
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

    /// The escalation an `escalation_*` event is about.
    pub fn escalation(&self) -> Option<&str> {
        match self {
            Self::EscalationAsked(event)
            | Self::EscalationResolved(event)
            | Self::EscalationExpired(event)
            | Self::EscalationDecided(event) => event.escalation.as_deref(),
            _ => None,
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
    /// The approval of a call that leaves the machine (a `send`); absent otherwise.
    #[serde(default)]
    pub outward: bool,
    /// Which of the questions one consult asked this is (`consultParts`): `{1, 1}` for a consult
    /// that asked one, absent on every other kind and from a docket before the feature.
    #[serde(default)]
    pub part: Option<EscalationPart>,
}

/// One question of a consult that asked several, 1-based.
#[derive(Debug, Clone, Copy, Deserialize)]
pub(super) struct EscalationPart {
    pub index: u32,
    pub count: u32,
}

/// `GET /escalations/<id>`: the view, and for an `outward` approval that is still pending the
/// whole call as text, credentials removed. No other route carries the call.
#[derive(Debug, Deserialize)]
pub(super) struct EscalationDetail {
    #[serde(flatten)]
    pub view: Escalation,
    pub call: Option<String>,
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

/// What `POST /pods/<pod>/apply` answers when the folder would run commands on this machine
/// (`409`) and when it was applied (`200`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApplyAnswer {
    /// The code to send back as `accept` once a person has read `runs`. Absent from an answer
    /// that is not a request for one (a folder that changed since the code was given).
    pub review_code: Option<String>,
    /// What the folder would run, one entry each.
    #[serde(default)]
    pub runs: Vec<ApplyRun>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApplyRun {
    pub kind: String,
    pub name: String,
    /// The exact text the person is asked to allow.
    pub runs: String,
}
