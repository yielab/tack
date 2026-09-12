//! `tack-orch` — the control-plane orchestration client for Tack.
//!
//! Defines [`ControlPlane`], the trait every agent-fleet backend implements,
//! plus the DTOs that cross the Tack <-> control-plane boundary. Concrete
//! adapters (`adapters::docket`) and the reconciler poll loop (`reconciler`)
//! build on top of this.
//!
//! # Dependency direction
//!
//! This crate depends inward on `tack-core` and `tack-db` only and must
//! never depend on `tack-api`, which depends on it instead. A `tack-api`
//! type needed from in here should be defined in this crate instead — never
//! invert the graph.
//!
//! # Money is always an estimate
//!
//! Every dollar field is named `*_usd_estimated`, never `*_usd` alone —
//! docket's own driver does not report real spend.
//!
//! # Unknown wire values never fail a poll
//!
//! [`RunState`], [`RunSource`], [`TaskStatus`], and [`ApprovalState`] each
//! carry an `Unknown(String)` fallback, so a docket upgrade that adds a new
//! state degrades to "shown as-is" rather than killing the reconciler.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod adapters;
pub mod execution;
// A sibling of `execution`, not a submodule, since both are I/O-bearing
// background tasks — see `execution_retention`'s module doc.
pub mod execution_observability;
pub mod execution_retention;
pub mod model_policy;
pub mod reconciler;
pub mod scheduler;
pub mod usage_provenance;

/// Everything that can go wrong talking to a control plane.
#[derive(Debug, thiserror::Error)]
pub enum OrchError {
    /// Transport failure or a non-2xx not more specifically described below.
    #[error("control plane http error: {0}")]
    Http(String),

    /// The control plane rejected our credentials (401/403).
    #[error("control plane authentication failed")]
    Auth,

    /// The response body didn't parse into the expected DTO.
    #[error("failed to decode control plane response: {0}")]
    Decode(String),

    /// The requested run, task, approval, or control plane doesn't exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Configured but not reachable right now — distinct from `Http` so a
    /// caller can degrade a plane's state without treating every failed
    /// request as one.
    #[error("control plane unavailable: {0}")]
    Unavailable(String),

    /// Gated behind a feature flag or missing configuration.
    #[error("control plane feature disabled")]
    Disabled,

    /// docket's `pre_input` policy gate refused a dispatch — a transport
    /// *success* carrying a considered "no". `policy_id` is parsed out of
    /// docket's error text into a typed field.
    #[error("blocked by guardrail policy {policy_id:?}: {message}")]
    PolicyBlocked { policy_id: String, message: String },

    /// Already resolved before our decision reached it (HTTP 409) —
    /// distinct from [`OrchError::NotFound`] so a caller can drop the stale
    /// row as an expected race, not a hard error.
    #[error("approval already decided: {0}")]
    AlreadyDecided(String),

    /// The remote resource we tried to create already exists (HTTP 409) —
    /// distinct from [`OrchError::AlreadyDecided`] so a caller isn't forced
    /// to pattern-match a message to tell the two conflicts apart.
    #[error("already exists: {0}")]
    AlreadyExists(String),
}

/// Generates a fieldless enum over a fixed set of wire strings, plus an
/// `Unknown(String)` fallback with hand-written `Serialize`/`Deserialize` so
/// an unrecognised value degrades instead of erroring.
macro_rules! remote_string_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant,)+
            /// A value docket sent that this version of Tack doesn't
            /// recognise, kept verbatim so it can round-trip without loss.
            Unknown(String),
        }

        impl $name {
            /// The exact wire string this variant round-trips to/from.
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $wire,)+
                    $name::Unknown(s) => s.as_str(),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                match s.as_str() {
                    $($wire => $name::$variant,)+
                    _ => $name::Unknown(s),
                }
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                match s {
                    $($wire => $name::$variant,)+
                    other => $name::Unknown(other.to_string()),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                // `#[serde(other)]` only works on a unit fallback variant with
                // no payload; capturing the original string requires going
                // through `String` ourselves.
                let s = String::deserialize(deserializer)?;
                Ok(Self::from(s))
            }
        }
    };
}

remote_string_enum! {
    /// A dispatch run's lifecycle state (`docket/core/runs.py`'s `RunState`).
    pub enum RunState {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

remote_string_enum! {
    /// What triggered a dispatch run (`docket/core/runs.py`'s `RunSource`).
    pub enum RunSource {
        Cli => "cli",
        Webhook => "webhook",
        Schedule => "schedule",
        Sweep => "sweep",
        Mcp => "mcp",
    }
}

remote_string_enum! {
    /// A queued task's status within a pod's pipeline
    /// (`docket/core/dispatch.py`'s task state machine).
    pub enum TaskStatus {
        Pending => "pending",
        Running => "running",
        Done => "done",
        Failed => "failed",
        Blocked => "blocked",
        WaitingApproval => "waiting_approval",
    }
}

remote_string_enum! {
    /// A pending-approval record's state (`docket/core/approval.py`).
    pub enum ApprovalState {
        Pending => "pending",
        Granted => "granted",
        Denied => "denied",
    }
}

// Each struct mirrors its docket endpoint's JSON shape; `serve.py` uses
// camelCase, `core/trace.py`'s trace/event records use snake_case.

/// `GET /health` — liveness only. Format: `{"status":"ok","gateway":N}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    /// Always `"ok"` when the server answers at all.
    pub status: String,
    /// `1` = active, `0` = inactive — kept as the wire integer, not `bool`.
    pub gateway: u8,
}

/// One entry in [`FleetStatus::agents`] — a docket project agent or
/// specialist. Mirrors `serve.py`'s `_agent_record()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetAgent {
    pub id: String,
    pub name: String,
    /// `"project"` or `"specialist"`.
    pub kind: String,
    pub scope: String,
    pub model: String,
    pub registered: bool,
    pub bindings: Vec<FleetBinding>,
    /// RFC3339 timestamp, or the literal string `"never"` — docket's own
    /// sentinel, not empty/null.
    pub last_activity: String,
    /// docket's wire field is the bare `costUsd`; see the money-fields note.
    #[serde(rename = "costUsd")]
    pub cost_usd_estimated: f64,
    /// `None` means "no budget cap configured", not "budget is zero".
    pub budget_usd: Option<f64>,
}

/// One channel binding for a [`FleetAgent`] (`core/fleet.py`'s
/// `agent_bindings()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetBinding {
    pub channel: String,
    pub peer_id: String,
}

/// `GET /status.json` — the fleet-wide snapshot (`serve.py`'s
/// `build_status()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStatus {
    /// docket's `SERVE_API_VERSION`; a mismatch should degrade the plane.
    pub api_version: String,
    pub timestamp: String,
    /// `"active"` or `"inactive"`.
    pub gateway: String,
    pub channels: Vec<String>,
    pub agents: Vec<FleetAgent>,
    /// Sum of every agent's `cost_usd_estimated`.
    #[serde(rename = "totalCostUsd")]
    pub total_cost_usd_estimated: f64,
}

/// One parsed line out of `GET /metrics` (Prometheus text exposition
/// format). Comment (`#`) lines never produce a sample. The parser lives in
/// `adapters::prometheus` — do not write a second one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

/// `GET /runs` (one element of the `runs` array) / `GET /runs/{id}` (the body
/// directly). Mirrors `core/runs.py`'s run record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRun {
    /// Formatted `run-<uuid>` — kept opaque; docket's own APIs expect the
    /// prefix back verbatim.
    pub id: String,
    pub source: RunSource,
    pub project: String,
    pub state: RunState,
    /// Populated only once the run reaches a terminal state.
    #[serde(default)]
    pub task_ids: Vec<String>,
    /// Exception text for a `failed` run; empty otherwise.
    #[serde(default)]
    pub error: String,
    /// ISO 8601 offset form, not docket's other `...Z` convention. Kept raw;
    /// parse at the call site if a typed timestamp is needed.
    pub created: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// pids of any hop subprocess in flight — meaningful only on the
    /// machine running docket, mirrored here for display only.
    #[serde(default)]
    pub pids: Vec<i64>,
    /// Only ever populated for a `webhook`-sourced run today.
    #[serde(default)]
    pub variables: serde_json::Value,
}

/// `GET /approvals` (one element of the `pending` array). Mirrors
/// `core/approval.py`'s approval record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteApproval {
    /// Formatted `apr-<uuid>` — see [`RemoteRun::id`].
    pub token: String,
    pub project: String,
    pub role: String,
    /// Description of the gated action, already redacted by docket.
    pub action: String,
    pub state: ApprovalState,
    pub created: String,
    /// Caller-supplied, stored verbatim by docket — an open dict, kept as
    /// `serde_json::Value`. One documented shape is `{"taskId": "...",
    /// "pipelineIndex": 0}` from a dispatch-pipeline gate.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// `GET /tasks/{project}` — real wire shape, live-verified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTask {
    pub id: String,
    pub description: String,
    /// `"high"` | `"normal"` | `"low"`.
    pub priority: String,
    pub status: TaskStatus,
    pub created: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    /// `"operator"` today; a plain string, not an enum.
    pub source: String,
    #[serde(default)]
    pub reason: String,
    #[serde(rename = "costUsd", default)]
    pub cost_usd_estimated: f64,
    pub claim_id: Option<String>,
    pub claimed_at: Option<String>,
    pub approval_token: Option<String>,
    pub pending_approval_index: Option<i64>,
}

/// Body for `POST /tasks/{project}`. An imported item must enqueue with
/// `trusted: false` so docket's `pre_input` guardrail evaluates it as
/// untrusted, attacker-authored text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRemoteTask {
    pub description: String,
    /// `"high"` | `"normal"` | `"low"`; docket defaults to `"normal"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// `false` for any item whose text originated outside Tack's own UI/CLI.
    #[serde(default)]
    pub trusted: bool,
}

/// `POST /pods` request body, every field but `project` optional. `project`
/// is never derived from Tack's own project name — the caller names it so a
/// retry after a partial failure can reuse the same pod. `budget` is
/// unsuffixed (not `*_usd_estimated`): an operator-set ceiling, not a
/// derived spend figure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProvisionPodParams {
    pub project: String,
    #[serde(default)]
    pub path: String,
    pub blueprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    #[serde(
        rename = "verifyCmd",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub verify_cmd: String,
}

/// One pod member docket actually created — `POST /pods`'s `members[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvisionedPodMember {
    pub id: String,
    pub role: String,
    pub model: String,
}

/// `POST /pods`'s `201` success body. `ok` is not modeled — an unmodeled key
/// costs nothing.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProvisionedPod {
    pub project: String,
    pub blueprint: String,
    pub members: Vec<ProvisionedPodMember>,
}

/// One page of `GET /traces/{project}?since=`. **`next` is opaque** — Tack
/// must never parse, decode, or reconstruct it, only store whatever the
/// control plane minted and hand it back unexamined. A prior client-side
/// reconstruction of docket's cursor algorithm was one silent server-side
/// change away from quietly skipping or duplicating events.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TracesPage {
    pub events: Vec<RemoteEvent>,
    /// `None` only if the remote genuinely didn't send one; treat the
    /// cursor as unchanged rather than erroring the poll.
    #[serde(default)]
    pub next: Option<String>,
}

/// One trace/event record. Mirrors `core/trace.py`'s JSONL shape exactly,
/// **including its snake_case field names** — the one docket surface not
/// camelCase. `event_type` is a plain `String`, not an enum: docket's set
/// is large and open to growth, and an unknown type must round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEvent {
    /// `"YYYY-MM-DDTHH:MM:SSZ"`.
    pub ts: String,
    pub project: String,
    pub session_id: String,
    pub agent_role: String,
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Only set on a handful of event types (e.g. `cost_charged`).
    #[serde(rename = "cost_usd", default)]
    pub cost_usd_estimated: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

// A capability is a value the caller reads, never a provider check derived
// from `kind` — a grep for `kind === 'docket'` in `frontend/src` must stay
// empty.

/// Three-state support level for a capability a bare `bool` can't express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// No mechanism exists on this provider, in either direction.
    Unsupported,
    /// A mechanism exists but the provider may not honour it.
    Advisory,
    /// The provider does exactly what was asked.
    Supported,
}

/// How narrowly an adapter's event stream can be scoped. Not a ranking —
/// `Project` and `Run` are incomparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventScope {
    /// No event/trace stream exists on this provider at all.
    None,
    Run,
    Project,
    Plane,
}

/// How a caller learns about a decision (approval, deployment gate, …)
/// waiting on a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSupport {
    /// The provider has no concept of a decision blocking progress.
    None,
    /// A regular poll cadence is the only way to discover one.
    Poll,
    /// The provider can notify Tack the moment a decision opens.
    Push,
}

/// Where a usage/cost figure downstream of this adapter comes from — see
/// the crate doc's "Money is always an estimate" note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSupport {
    /// No token/cost figure exists for this provider.
    NotMeasured,
    /// The provider's own driver estimates and reports usage directly.
    FromProvider,
    /// A separate LLM gateway in front of the provider meters usage.
    FromGateway,
}

/// Whether a caller-supplied model identifier actually reaches the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSelection {
    /// The provider owns its own routing and may silently ignore it.
    Unsupported,
    /// The provider accepts a model hint but isn't guaranteed to use it.
    Advisory,
    /// The provider passes the identifier straight through, unexamined.
    Honoured,
}

/// Pairs a non-boolean capability's level with a human-readable reason, so a
/// UI can say *why*, not only *that*. `Serialize` only — nothing here
/// decodes a `Capabilities` back in from JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rated<T> {
    pub level: T,
    pub reason: &'static str,
}

impl<T> Rated<T> {
    pub const fn new(level: T, reason: &'static str) -> Self {
        Self { level, reason }
    }
}

/// What one control plane can actually do — derived from the adapter's own
/// configuration, never guessed from `kind`. Non-boolean fields are
/// [`Rated`]; boolean fields answer a plain yes/no.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Capabilities {
    /// Can this plane accept new work at all?
    pub dispatch: bool,
    /// Can an in-flight run be stopped?
    pub cancel: bool,
    pub pause: Rated<Support>,
    pub resume: Rated<Support>,
    pub event_scope: Rated<EventScope>,
    /// Can build/run artifacts be retrieved after the fact?
    pub artifacts: bool,
    pub decisions: Rated<DecisionSupport>,
    pub usage: Rated<UsageSupport>,
    pub model_selection: Rated<ModelSelection>,
    /// Does this plane expose a roster of available agent runtimes/models?
    pub runtimes: bool,
    /// Does this plane expose a plane-wide metrics scrape?
    pub plane_metrics: bool,
    /// Can this plane provision a fresh execution environment?
    pub provisioning: bool,
}

/// A control plane Tack can read fleet/run/approval/task state from and,
/// gated behind `TACK_ORCH_ENABLE`, dispatch work to. `docket` is the only
/// implementor today. **Not frozen** — update every implementor/caller in
/// the same change as any signature edit.
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str; // "docket"

    /// **Synchronous and does no I/O** — derived from the adapter's own
    /// static configuration, so a plane that's currently `unreachable`
    /// still reports honest capabilities.
    fn capabilities(&self) -> Capabilities;

    async fn health(&self) -> Result<Health, OrchError>;
    async fn status(&self) -> Result<FleetStatus, OrchError>;
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError>;
    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError>;
    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError>;
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError>;
    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError>;
    /// `since` is the opaque cursor a previous call's [`TracesPage::next`]
    /// returned (`None` to start from the beginning); persist the returned
    /// one and pass it back verbatim — never parsed or recomputed.
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError>;
    // Write side, gated behind TACK_ORCH_ENABLE.
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    /// Trigger a full pipeline dispatch for `project` — `POST
    /// /dispatch/{project}`, `vars` sent as the request body verbatim.
    /// Success carries the plane's own run id, distinct from
    /// [`ControlPlane::enqueue_task`]'s pod *task* id. **The id can come
    /// back before the dispatched work actually runs**: poll
    /// [`ControlPlane::get_run`] or [`ControlPlane::traces`] to learn
    /// whether it was accepted; `Err` here only means the plane refused to
    /// record it, never that the run later failed.
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    /// Grant (`grant: true`) or deny (`grant: false`) a pending approval —
    /// `POST /approvals/{token}`. An already-decided token is
    /// [`OrchError::AlreadyDecided`], not a panic-worthy failure; an
    /// unknown or illegally-transitioned one is [`OrchError::NotFound`].
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<ApprovalState, OrchError>;

    /// Provision a fresh pod from a blueprint — `POST /pods`. See
    /// [`ProvisionPodParams`]. **Provisions atomically: every member is
    /// created, or none are** — a failing call tears down whatever it
    /// started, so a caller never needs to (and cannot — no teardown
    /// route) undo it. [`OrchError::AlreadyExists`] (HTTP 409) is the one
    /// exception, raised before anything is touched.
    async fn provision_pod(&self, params: ProvisionPodParams) -> Result<ProvisionedPod, OrchError>;
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
