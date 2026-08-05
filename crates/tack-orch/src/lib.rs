//! `tack-orch` — the control-plane orchestration client for Tack.
//!
//! Defines [`ControlPlane`], the trait every agent-fleet backend (docket today,
//! something else tomorrow) implements, plus the DTOs that cross the
//! Tack ⇄ control-plane boundary. Concrete adapters (`adapters::docket`, Wave 1)
//! and the reconciler poll loop (`reconciler`, Wave 1) build on top of this.
//!
//! # Dependency direction
//!
//! This crate depends inward on `tack-core` and `tack-db` only. **It must never
//! depend on `tack-api`.** `tack-api` depends on `tack-orch` — to spawn the
//! reconciler and to expose the `/api/control-planes`, `/api/fleet`, and
//! dispatch routes — not the other way around. If you're an agent reaching for
//! `tack-api` types from in here (e.g. to reuse a handler DTO), stop: define the
//! type here instead and let `tack-api` depend on it, or duplicate the small
//! shape rather than inverting the graph. See `TODO.md` §1.1 and
//! `docs/book/src/developer/orchestration.md`.
//!
//! # Money is always an estimate
//!
//! Every dollar-valued field in this crate is named `*_usd_estimated` (never
//! `*_usd` alone) — token counts are the primary, trustworthy measure; docket's
//! own driver does not report real spend (see `docket/core/dispatch.py`'s
//! `pod_gating_cost`), so any dollar figure downstream of it is derived, not
//! recorded. See `TODO.md` §0 rule 6.
//!
//! # Unknown enum values never fail a poll
//!
//! [`RunState`], [`RunSource`], [`TaskStatus`], and [`ApprovalState`] each carry
//! an `Unknown(String)` fallback variant with a hand-written `Deserialize` (a
//! plain `#[serde(other)]` only works on unit variants, and we need the
//! original string preserved so it can round-trip back out). A docket upgrade
//! that adds a new state must degrade to "shown as-is", never to a
//! deserialization error that kills the reconciler's poll loop.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod adapters;
pub mod reconciler;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong talking to a control plane.
#[derive(Debug, thiserror::Error)]
pub enum OrchError {
    /// Transport-level failure: connection refused, timeout, DNS, TLS, a non-2xx
    /// status the other variants don't more specifically describe, etc.
    #[error("control plane http error: {0}")]
    Http(String),

    /// The control plane rejected our credentials (401/403).
    #[error("control plane authentication failed")]
    Auth,

    /// The response body didn't parse into the DTO we expected — malformed
    /// JSON, a field of the wrong shape, a Prometheus line we couldn't tokenize.
    #[error("failed to decode control plane response: {0}")]
    Decode(String),

    /// The requested resource (run, task, approval, control plane) doesn't
    /// exist on the remote side.
    #[error("not found: {0}")]
    NotFound(String),

    /// The control plane is configured but not currently reachable (down,
    /// health check failing, apiVersion mismatch). Distinct from `Http` so
    /// callers can degrade a plane's displayed state without treating every
    /// single request failure as one.
    #[error("control plane unavailable: {0}")]
    Unavailable(String),

    /// The requested operation is gated behind a feature flag or missing
    /// configuration (e.g. `TACK_ORCH_ENABLE` unset, no approval token
    /// configured, a write method called against an adapter that only
    /// implements the read side).
    #[error("control plane feature disabled")]
    Disabled,

    /// docket's `pre_input` policy gate deliberately refused a dispatch —
    /// a transport *success* carrying a considered "no", not a transport
    /// failure. Card V1 verified live that a `block` verdict comes back as
    /// HTTP 400 naming the policy id that fired
    /// (`"task rejected by guardrail policy '<id>' at enqueue: <message>"`).
    /// `policy_id` is that id, parsed out once here so every caller gets a
    /// typed field instead of pattern-matching a prefix on a string (see
    /// TODO.md §2.1, card R1 — this variant replaces `adapters::docket`'s
    /// old `POLICY_BLOCK_PREFIX` string-matching workaround). `message` is
    /// docket's own text, kept verbatim for display.
    #[error("blocked by guardrail policy {policy_id:?}: {message}")]
    PolicyBlocked { policy_id: String, message: String },

    /// docket already resolved this approval before our decision reached it —
    /// `POST /approvals/{token}` returning HTTP 409, `approval.ApprovalNoop`
    /// server-side (e.g. granted moments earlier from the CLI, or expired
    /// past `APPROVAL_TIMEOUT`). Distinct from [`OrchError::NotFound`] (404 —
    /// no such token at all, or docket's `approval.ApprovalError` for an
    /// illegal state transition, which `serve.py` happens to report with the
    /// same status) so a caller building an approvals inbox (card D1) can
    /// render "someone already decided this" — a normal, expected race, not
    /// a hard error — and simply drop the stale row rather than surface a
    /// scary failure. `message` is docket's own text (e.g. `"Already
    /// granted: apr-..."`), kept verbatim for display.
    #[error("approval already decided: {0}")]
    AlreadyDecided(String),
}

// ---------------------------------------------------------------------------
// Remote state enums — the exact strings docket emits (TODO.md §1.2), each
// with an `Unknown(String)` fallback that round-trips.
// ---------------------------------------------------------------------------

/// Generates a fieldless enum over a fixed set of wire strings, plus an
/// `Unknown(String)` fallback, with hand-written `Serialize`/`Deserialize`
/// (via `String`) so an unrecognised value degrades instead of erroring — and
/// re-serializes back to the exact string it was read from.
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
            /// A value docket sent that this version of Tack doesn't recognise.
            /// Carries the original wire string verbatim so it can be shown
            /// as-is and re-serialized without loss.
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
    /// A dispatch run's lifecycle state. Verified against
    /// `docket/core/runs.py`'s `RunState` literal.
    pub enum RunState {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

remote_string_enum! {
    /// What triggered a dispatch run. Verified against `docket/core/runs.py`'s
    /// `RunSource` literal.
    pub enum RunSource {
        Cli => "cli",
        Webhook => "webhook",
        Schedule => "schedule",
        Sweep => "sweep",
        Mcp => "mcp",
    }
}

remote_string_enum! {
    /// A queued task's status within a pod's pipeline. Verified against the
    /// task state machine in `docket/core/dispatch.py` (`enqueue_task`,
    /// `_claim_next_task`, `TaskResult.status`).
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
    /// A pending-approval record's state. Verified against
    /// `docket/core/approval.py`'s `approval_grant`/`approval_deny`/
    /// `list_pending`.
    pub enum ApprovalState {
        Pending => "pending",
        Granted => "granted",
        Denied => "denied",
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------
//
// Field-name convention: every struct below mirrors the JSON shape of the
// docket endpoint it comes from. docket's `serve.py`-rendered endpoints
// (`/status.json`, `/runs`, `/runs/{id}`, `/approvals`) use camelCase; its
// trace/event records (`core/trace.py`) use snake_case — that split is real,
// not an inconsistency introduced here, so each struct's `#[serde(...)]`
// attributes follow the endpoint it actually came from rather than a single
// blanket convention. See the module doc and this crate's Wave-0 handoff note
// in TODO.md §6 for the endpoints that don't exist yet.

/// `GET /health` — liveness only. Format: `{"status":"ok","gateway":N}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    /// Always `"ok"` when the server answers at all — docket's `/health`
    /// has no unhealthy body, only "doesn't respond".
    pub status: String,
    /// The gateway service's own liveness bit: `1` = active, `0` = inactive.
    /// Kept as the wire integer rather than coerced to `bool` so this struct
    /// never silently disagrees with docket about what the byte means.
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
    /// sentinel (`serve.py`'s `_last_activity_or_never`), not empty/null.
    pub last_activity: String,
    /// Cumulative estimated spend for this agent. See the module-level note
    /// on money fields — docket's own driver reports no real cost, so this
    /// number is always an estimate even though docket's wire field is named
    /// `costUsd` without qualification.
    #[serde(rename = "costUsd")]
    pub cost_usd_estimated: f64,
    /// `None` when unset or `0`/empty on the docket side (see
    /// `_agent_record`'s `budget_raw` handling) — "no budget cap configured",
    /// not "budget is zero".
    pub budget_usd: Option<f64>,
}

/// One channel binding for a [`FleetAgent`]. Mirrors
/// `core/fleet.py`'s `agent_bindings()`: `[{channel, peerId}, ...]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetBinding {
    pub channel: String,
    pub peer_id: String,
}

/// `GET /status.json` — the fleet-wide snapshot. Mirrors `serve.py`'s
/// `build_status()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStatus {
    /// docket's `SERVE_API_VERSION` — bumped on any breaking contract change.
    /// Check this on every poll; a mismatch should degrade the plane rather
    /// than risk misparsing a shape that has silently changed underneath us
    /// (see TODO.md §5, "Known risks").
    pub api_version: String,
    pub timestamp: String,
    /// `"active"` or `"inactive"`.
    pub gateway: String,
    pub channels: Vec<String>,
    pub agents: Vec<FleetAgent>,
    /// Sum of every agent's `cost_usd_estimated`. See the module-level note
    /// on money fields.
    #[serde(rename = "totalCostUsd")]
    pub total_cost_usd_estimated: f64,
}

/// One parsed line out of `GET /metrics` (Prometheus text exposition format).
/// `name` is the bare metric name (e.g. `docket_agent_cost_usd`); `labels` is
/// the `{k="v", ...}` label set, if any; `value` is the trailing number.
/// Comment (`#`) lines never produce a sample. The parser that produces these
/// lives in `adapters::docket` (Wave 1, card A1) and is reused as-is by B3's
/// metrics ingestion — do not write a second one.
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
    /// docket's own id, formatted `run-<uuid>` — kept as an opaque string
    /// rather than parsed as a `Uuid`, since the `run-` prefix is part of the
    /// identifier docket's own APIs expect back (`GET /runs/{id}`).
    pub id: String,
    pub source: RunSource,
    pub project: String,
    pub state: RunState,
    /// Task ids this run actually touched — populated only once the run
    /// reaches a terminal state (`core/runs.py`'s `finish_run`).
    #[serde(default)]
    pub task_ids: Vec<String>,
    /// Exception text for a `failed` run; empty for every other state.
    #[serde(default)]
    pub error: String,
    /// ISO 8601 timestamp (`datetime.now(UTC).isoformat()` — offset form, not
    /// docket's other `...Z` convention). Kept as a raw string; parse at the
    /// call site if a typed timestamp is needed, rather than risk a poll-loop
    /// failure on a format this crate doesn't defensively handle.
    pub created: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// pids of any hop subprocess currently in flight for this run. Only ever
    /// meaningful on the machine actually running docket; mirrored here for
    /// display/audit, never acted on remotely.
    #[serde(default)]
    pub pids: Vec<i64>,
    /// The resolved pipeline variable namespace this run was dispatched with
    /// (only ever populated for a `webhook`-sourced run today).
    #[serde(default)]
    pub variables: serde_json::Value,
}

/// `GET /approvals` (one element of the `pending` array). Mirrors
/// `core/approval.py`'s approval record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteApproval {
    /// Formatted `apr-<uuid>` — see [`RemoteRun::id`]'s note on why this
    /// stays an opaque string.
    pub token: String,
    pub project: String,
    pub role: String,
    /// Human-readable description of the gated action, already redacted by
    /// docket before it reaches this struct.
    pub action: String,
    pub state: ApprovalState,
    pub created: String,
    /// Caller-supplied, stored verbatim by docket (`approval_create`'s
    /// `context` parameter). The one documented shape today is
    /// `{"taskId": "...", "pipelineIndex": 0}` from a dispatch-pipeline gate,
    /// but the field is an open dict on the docket side — kept as
    /// `serde_json::Value` rather than a typed struct so a caller with a
    /// different context shape (or none) still deserializes cleanly. Callers
    /// that want the dispatch correlation should look up `taskId` explicitly
    /// and treat its absence as "an uncorrelated approval" (see TODO.md §
    /// Wave 2, card B1), not as a parse error.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// `GET /tasks/{project}` (⚠️ does not exist yet — blocked on docket Phase 22,
/// see TODO.md §1.4). Field shape is inferred from the **queued task record**
/// docket already persists internally (`core/dispatch.py`'s `_normalize_task`/
/// `enqueue_task`) since that is the closest real precedent for what the new
/// endpoint is likely to expose — this is a best-effort projection, not a
/// verified contract, and must be re-checked against the real endpoint once
/// docket Phase 22 ships.
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
    /// `"operator"` in every task docket enqueues today; kept as a plain
    /// string (not an enum) since this is speculative pending the real
    /// endpoint, and a closed set here would be one more thing to get wrong.
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

/// Body for `POST /tasks/{project}` (⚠️ does not exist yet — blocked on
/// docket Phase 22; see TODO.md §1.4 and §Wave 3 card C1). `description` and
/// `priority` mirror `core/dispatch.py`'s `enqueue_task(project, description,
/// priority)`. `trusted` is **speculative**: it is not a parameter of
/// `enqueue_task` today (which derives trust from a fixed `source ==
/// "operator"` check), but TODO.md's Wave 3 card C2 requires imported items
/// to enqueue with `trusted: false` so docket's `pre_input` guardrail policy
/// evaluates them as untrusted, attacker-authored text — that requires the
/// future endpoint to accept trust as an explicit input. Re-verify this
/// field's name and presence once docket Phase 22 ships the real endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRemoteTask {
    pub description: String,
    /// `"high"` | `"normal"` | `"low"`; docket defaults to `"normal"` when
    /// omitted or unrecognised (`enqueue_task`'s own fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// `false` for any item whose text originated outside Tack's own UI/CLI
    /// (e.g. GitHub/Linear import) — see this struct's doc comment.
    #[serde(default)]
    pub trusted: bool,
}

/// One page of `GET /traces/{project}?since=` — the events themselves, plus
/// the remote's own resume cursor to send back as `since` on the next call.
///
/// **`next` is opaque.** Tack must never parse it, decode it, or reconstruct
/// it client-side — it is whatever the control plane minted, persisted
/// verbatim, and handed back unexamined. This DTO exists specifically so
/// that discipline is structural rather than a convention someone has to
/// remember: before this type existed, [`ControlPlane::traces`] had nowhere
/// to carry a remote-minted cursor back out, and `tack-orch`'s reconciler
/// reimplemented docket's own compound `"<ts>Z:<n>"` cursor algorithm
/// client-side to work around that gap — correct, but one silent algorithm
/// change away from quietly skipping or duplicating events (see TODO.md
/// §2.1, card R1, and the git history for `reconciler.rs`'s deleted
/// `next_trace_cursor`/`decode_trace_cursor`). That reconstruction is gone;
/// this field is the fix.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TracesPage {
    pub events: Vec<RemoteEvent>,
    /// `None` only if the remote genuinely didn't send one — defensive, not
    /// expected: docket always mints `next` on `GET /traces/{project}`
    /// (card V1, verified live). A caller with `next: None` should treat the
    /// cursor as unchanged rather than erroring the poll.
    #[serde(default)]
    pub next: Option<String>,
}

/// One trace/event record. Mirrors `core/trace.py`'s JSONL record shape
/// exactly, **including its snake_case field names** — trace events are the
/// one docket surface that is not camelCase (contrast every other DTO in this
/// file, which mirrors a `serve.py` JSON endpoint). Produced by `GET
/// /traces/{project}` (⚠️ does not exist yet — blocked on docket Phase 22; see
/// TODO.md §1.4 and §Wave 2 card B2).
///
/// `event_type` is deliberately a plain `String`, not an enum: docket's
/// `EVENT_TYPES` set (`core/trace.py`) is large, open to growth, and B2's
/// acceptance criteria requires unknown types to be **stored verbatim** —
/// exactly the property a plain string gives for free, without needing this
/// crate's `Unknown(String)` machinery at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEvent {
    /// `"YYYY-MM-DDTHH:MM:SSZ"` (`core/trace.py`'s `_now_iso()`).
    pub ts: String,
    pub project: String,
    pub session_id: String,
    pub agent_role: String,
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Only ever set on a handful of event types (e.g. `cost_charged`) — see
    /// the module-level note on money fields for why this is named
    /// `_estimated` even though docket's own field is the bare `cost_usd`.
    #[serde(rename = "cost_usd", default)]
    pub cost_usd_estimated: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// The ControlPlane trait
// ---------------------------------------------------------------------------

/// A control plane Tack can read fleet/run/approval/task state from and (from
/// Phase 35 on, gated behind `TACK_ORCH_ENABLE`) dispatch work to. `docket` is
/// the only implementor today (`adapters::docket::DocketAdapter`, Wave 1); the
/// trait exists so a second backend never has to touch the reconciler,
/// handlers, or frontend that consume it.
///
/// **No longer frozen.** TODO.md §1.1 specified these signatures exactly and
/// every Wave 1–3 card built against them verbatim; §2.1 (card R1,
/// 2026-08-05) lifted that freeze once it started forcing designs worse than
/// the churn it was meant to prevent (see [`traces`](Self::traces)'s return
/// type and [`OrchError::PolicyBlocked`] for the two concrete cases). Treat
/// the shape below as current, not eternal — change it again if the next
/// design genuinely needs to, and update every implementor/caller in the
/// same change, the way R1 did.
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str; // "docket"
    async fn health(&self) -> Result<Health, OrchError>;
    async fn status(&self) -> Result<FleetStatus, OrchError>;
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError>;
    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError>;
    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError>;
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError>;
    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError>;
    /// `since` is the opaque cursor a previous call's [`TracesPage::next`]
    /// returned (`None` to start from the beginning). The returned
    /// [`TracesPage::next`] must be persisted and passed back verbatim next
    /// time — never parsed, decoded, or recomputed by the caller.
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError>;
    // Phase 35+ — write side, gated behind TACK_ORCH_ENABLE.
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    /// Grant (`grant: true`) or deny (`grant: false`) a pending approval —
    /// `POST /approvals/{token}` (card D1, Wave 4, task 36.1). Success
    /// carries docket's own resulting [`ApprovalState`] (`Granted`/`Denied`;
    /// modeled as `Unknown` rather than assumed if docket's wording ever
    /// changes) — mirrors the real response body, `{"ok":true,"token":...,
    /// "state":...}` (card V1, verified live). The `channel` docket records
    /// alongside the decision in its hash-chained audit log (its P22-4) is
    /// **not** a parameter here — every caller of this trait is Tack itself,
    /// so `adapters::docket`'s implementation sends the fixed value `"tack"`
    /// (verified against `approval.APPROVAL_CHANNELS`, which already lists
    /// it) rather than threading a value no caller would ever vary through
    /// every layer above this trait.
    ///
    /// An already-decided token is [`OrchError::AlreadyDecided`], not a
    /// panic-worthy failure — see that variant's doc comment. An unknown
    /// token (or an illegal decision on one, which docket reports the same
    /// way) is [`OrchError::NotFound`].
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<ApprovalState, OrchError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// One "unrecognised value round-trips" test per enum (TODO.md's Wave-0
    /// acceptance criterion) — deserializes to `Unknown(original)` instead of
    /// erroring, and reserializes to exactly the same string.
    fn assert_unknown_round_trips<T>(wire_value: &str)
    where
        T: Serialize + for<'de> Deserialize<'de> + fmt::Debug + PartialEq + From<String>,
    {
        let json = format!("\"{wire_value}\"");
        let decoded: T = serde_json::from_str(&json).expect("unknown value must not error");
        assert_eq!(decoded, T::from(wire_value.to_string()));
        let re_encoded = serde_json::to_string(&decoded).expect("must reserialize");
        assert_eq!(
            re_encoded, json,
            "must round-trip to the exact original string"
        );
    }

    #[test]
    fn run_state_unknown_round_trips() {
        assert_unknown_round_trips::<RunState>("paused");
    }

    #[test]
    fn run_state_known_values_round_trip() {
        for wire in ["queued", "running", "succeeded", "failed", "cancelled"] {
            let json = format!("\"{wire}\"");
            let decoded: RunState = serde_json::from_str(&json).unwrap();
            assert!(!matches!(decoded, RunState::Unknown(_)));
            assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
        }
    }

    #[test]
    fn run_source_unknown_round_trips() {
        assert_unknown_round_trips::<RunSource>("telegram-bot");
    }

    #[test]
    fn task_status_unknown_round_trips() {
        assert_unknown_round_trips::<TaskStatus>("archived");
    }

    #[test]
    fn approval_state_unknown_round_trips() {
        assert_unknown_round_trips::<ApprovalState>("expired");
    }

    #[test]
    fn health_deserializes_from_docket_shape() {
        let h: Health = serde_json::from_str(r#"{"status":"ok","gateway":1}"#).unwrap();
        assert_eq!(h.status, "ok");
        assert_eq!(h.gateway, 1);
    }

    #[test]
    fn fleet_status_deserializes_from_docket_shape() {
        let raw = r#"{
            "apiVersion": "2",
            "timestamp": "2026-08-04T00:00:00Z",
            "gateway": "active",
            "channels": ["telegram"],
            "agents": [{
                "id": "proj-1",
                "name": "proj-1",
                "kind": "project",
                "scope": "project",
                "model": "claude-sonnet-5",
                "registered": true,
                "bindings": [{"channel": "telegram", "peerId": "12345"}],
                "lastActivity": "never",
                "costUsd": 1.5,
                "budgetUsd": null
            }],
            "totalCostUsd": 1.5
        }"#;
        let status: FleetStatus = serde_json::from_str(raw).unwrap();
        assert_eq!(status.api_version, "2");
        assert_eq!(status.total_cost_usd_estimated, 1.5);
        assert_eq!(status.agents.len(), 1);
        assert_eq!(status.agents[0].cost_usd_estimated, 1.5);
        assert_eq!(status.agents[0].budget_usd, None);
        assert_eq!(status.agents[0].bindings[0].peer_id, "12345");
    }

    #[test]
    fn remote_run_deserializes_from_docket_shape() {
        let raw = r#"{
            "id": "run-abc123",
            "source": "webhook",
            "project": "demo",
            "state": "running",
            "taskIds": [],
            "error": "",
            "created": "2026-08-04T00:00:00+00:00",
            "startedAt": "2026-08-04T00:00:01+00:00",
            "finishedAt": null,
            "pids": [],
            "variables": {}
        }"#;
        let run: RemoteRun = serde_json::from_str(raw).unwrap();
        assert_eq!(run.state, RunState::Running);
        assert_eq!(run.source, RunSource::Webhook);
        assert_eq!(run.finished_at, None);
    }

    #[test]
    fn remote_approval_deserializes_from_docket_shape() {
        let raw = r#"{
            "token": "apr-xyz",
            "project": "demo",
            "role": "lead",
            "action": "pod dispatch — task enqueue for 'demo': do the thing",
            "state": "pending",
            "created": "2026-08-04T00:00:00Z",
            "context": {"taskId": "task-1", "pipelineIndex": 0}
        }"#;
        let approval: RemoteApproval = serde_json::from_str(raw).unwrap();
        assert_eq!(approval.state, ApprovalState::Pending);
        assert_eq!(approval.context["taskId"], "task-1");
    }

    #[test]
    fn remote_event_deserializes_from_snake_case_trace_shape() {
        let raw = r#"{
            "ts": "2026-08-04T00:00:00Z",
            "project": "demo",
            "session_id": "agent:demo:task-1",
            "agent_role": "lead",
            "event_type": "some_future_event_type",
            "payload": {"text": "hi"},
            "cost_usd": 0.01,
            "duration_ms": 1200
        }"#;
        let event: RemoteEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(event.event_type, "some_future_event_type");
        assert_eq!(event.cost_usd_estimated, Some(0.01));
    }

    #[test]
    fn new_remote_task_omits_absent_priority() {
        let task = NewRemoteTask {
            description: "do the thing".to_string(),
            priority: None,
            trusted: false,
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("priority"));
        assert!(json.contains("\"trusted\":false"));
    }

    #[test]
    fn orch_error_variants_have_messages() {
        // Cheap smoke test that every variant constructs and displays without
        // panicking — mostly here to keep the match arms honest if a variant
        // is ever added without a message.
        let errors = [
            OrchError::Http("boom".into()),
            OrchError::Auth,
            OrchError::Decode("bad json".into()),
            OrchError::NotFound("run-1".into()),
            OrchError::Unavailable("connection refused".into()),
            OrchError::Disabled,
            OrchError::PolicyBlocked {
                policy_id: "prompt-injection".into(),
                message: "untrusted input matched a deny rule".into(),
            },
            OrchError::AlreadyDecided("Already granted: apr-1".into()),
        ];
        for e in errors {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn already_decided_display_names_the_docket_message() {
        let e = OrchError::AlreadyDecided("Already granted: apr-1".into());
        assert!(e.to_string().contains("Already granted: apr-1"));
    }

    #[test]
    fn policy_blocked_display_names_the_policy_id() {
        let e = OrchError::PolicyBlocked {
            policy_id: "prompt-injection".into(),
            message: "untrusted input matched a deny rule".into(),
        };
        let text = e.to_string();
        assert!(text.contains("prompt-injection"), "{text}");
        assert!(
            text.contains("untrusted input matched a deny rule"),
            "{text}"
        );
    }

    #[test]
    fn traces_page_round_trips_events_and_the_opaque_cursor() {
        // TracesPage's own `events` field is already-decoded `RemoteEvent`s —
        // the double-encoded-JSON-strings wire quirk is `adapters::docket`'s
        // problem to unwrap before it ever builds a `TracesPage`, not this
        // struct's. This test only exercises what this crate owns: the page
        // envelope and the opaque cursor field.
        let event = RemoteEvent {
            ts: "2026-08-05T00:00:00Z".to_string(),
            project: "demo".to_string(),
            session_id: "agent:demo:task-1".to_string(),
            agent_role: "lead".to_string(),
            event_type: "tool_call".to_string(),
            payload: serde_json::json!({}),
            cost_usd_estimated: None,
            duration_ms: None,
        };
        let page = TracesPage {
            events: vec![event],
            next: Some("2026-08-05T00:00:00Z:1".to_string()),
        };
        let json = serde_json::to_string(&page).unwrap();
        let decoded: TracesPage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, page);
    }

    #[test]
    fn traces_page_next_defaults_to_none_when_absent() {
        let page: TracesPage = serde_json::from_str(r#"{"events":[]}"#).unwrap();
        assert_eq!(page.next, None);
        assert!(page.events.is_empty());
    }
}
