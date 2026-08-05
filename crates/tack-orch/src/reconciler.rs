//! The orchestration reconciler: one background `tokio` task per registered
//! control plane, polling it on an interval and driving the
//! `healthy` → `degraded` → `unreachable` state machine (TODO.md §Wave 1,
//! card A2 / task 33.6).
//!
//! # The three-phase shape, and why it's not just discipline
//!
//! TODO.md §0 rule 5 is a hard requirement: **never hold a SQLite write
//! transaction across an HTTP call to docket.** This module enforces that by
//! construction rather than by convention — each poll tick has three
//! strictly separated phases, run in this order, with no phase able to see
//! into the next:
//!
//! 1. **Fetch** ([`reconcile_once`]) — every HTTP call this poll needs. No
//!    database access happens anywhere in this phase; it isn't even possible,
//!    because [`reconcile_once`] and everything it calls never receive a
//!    store or pool handle. It returns a plain, ownable [`PollEvaluation`].
//! 2. **Decide** ([`HealthTracker::observe`]) — a pure, synchronous state
//!    transition over the fetch result. No I/O of any kind.
//! 3. **Persist** ([`spawn_one`]'s single `store.record_health(...).await`
//!    call) — one short write, invoked exactly once per tick, strictly after
//!    phase 1 has fully completed. Nothing in this phase awaits an HTTP call.
//!
//! Because phase 1 has already finished (its `.await` has resolved) before
//! phase 3 begins, there is no window in which a write is open while an HTTP
//! request is in flight — not "we were careful", but "the types don't let
//! you interleave them; phase 3 doesn't have a `ControlPlane` handle to await
//! on even if someone wanted to."
//!
//! # Extending this for Wave 2 (read before adding `poll_traces`)
//!
//! [`reconcile_once`] builds a [`FetchOutcome`] as an explicit, flat list of
//! steps — one field per `poll_*` call. Each Wave-2 card adds **exactly one
//! field** to [`FetchOutcome`], **exactly one `poll_*` function**
//! (module-private, following [`poll_health`]/[`poll_status`]'s shape), and
//! **exactly one line** inside [`reconcile_once`]'s struct literal. B1 (runs +
//! approvals), B3 (metrics), and B2 (traces) have all landed this way.
//! Concretely, B3's addition looked like:
//!
//! ```ignore
//! // 1. A field on FetchOutcome:
//! struct FetchOutcome {
//!     health: Result<Health, OrchError>,
//!     status: Result<FleetStatus, OrchError>,
//!     runs: Vec<(String, Result<Vec<RemoteRun>, OrchError>)>,
//!     approvals: Result<Vec<RemoteApproval>, OrchError>,
//!     metrics: Result<Vec<MetricSample>, OrchError>,       // landed (B3)
//! }
//!
//! // 2. A fetch-only poll_* fn, same shape as poll_health/poll_status. Note
//! //    this is *not* a raw-text-plus-parse step: DocketAdapter::metrics()
//! //    (card A1) already calls adapters::prometheus::parse internally and
//! //    returns Vec<MetricSample> directly, so poll_metrics is exactly as
//! //    trivial as poll_health/poll_status:
//! async fn poll_metrics(control_plane: &Arc<dyn ControlPlane>) -> Result<Vec<MetricSample>, OrchError> {
//!     control_plane.metrics().await
//! }
//!
//! // 3. One line in reconcile_once's struct literal:
//! let fetched = FetchOutcome {
//!     health: poll_health(control_plane).await,
//!     status: poll_status(control_plane).await,
//!     runs: poll_runs(control_plane, projects).await,
//!     approvals: poll_approvals(control_plane).await,
//!     metrics: poll_metrics(control_plane).await,          // landed (B3)
//! };
//! ```
//!
//! **The deviation B1 made to A2's original recipe, still in effect:** the
//! recipe as A2 first wrote it had `reconcile_once` return only
//! `evaluate(&fetched)` (a [`PollEvaluation`]), discarding `fetched` entirely
//! — that works for a step whose only consumer is `evaluate`, but ingestion
//! data has to reach [`spawn_one`]'s persistence phase, and `evaluate` never
//! carries data forward, only a verdict. `reconcile_once` returns
//! `(PollEvaluation, FetchOutcome)` — the tuple's second element is the raw,
//! unevaluated fetch, there specifically so a later phase can read fields
//! `evaluate` doesn't touch. `metrics` is read off that `FetchOutcome` half in
//! [`spawn_one`] (`persist_metrics`, added by B3), the same way B1's
//! `persist_runs`/`persist_approvals` already do, and it is **not** visible
//! to `evaluate` (nor should it be — a `/metrics` scrape failure must not
//! affect plane health any more than a `/runs` one does).
//!
//! Your own persistence (e.g. `store.upsert_runs(...)`) does **not** belong
//! inside [`reconcile_once`] — add it in [`spawn_one`]'s loop, as its own
//! short call placed after `store.record_health(...).await`, following the
//! exact same fetch-then-persist separation. Do **not** let a
//! runs/approvals/traces/metrics poll failure influence [`evaluate`]'s
//! reachability verdict — only `/health` and `/status.json` decide plane
//! health; the other steps are data-ingestion concerns with their own
//! success/failure handling. See TODO.md §6, the A2/B1/B3 handoff notes, for
//! the worked examples this doc comment is kept in sync with.
//!
//! One more thing B1 added that isn't purely mechanical: `poll_runs` needs a
//! project list (docket's `/runs` is filtered by `?project=`, one call per
//! *linked* project, not fleet-wide), and that list comes from the database
//! (`orch_links`), not from the control plane. That fetch
//! (`store.list_linked_projects(id)`) happens in [`spawn_one`], **before**
//! the panic-isolated `reconcile_once` call — it's a single short read, not
//! held open across any HTTP `.await`, so it doesn't violate §0 rule 5, but
//! it does mean the project list a given tick uses can be one tick stale
//! relative to a concurrent `orch_links` edit. That's an accepted
//! staleness window (bounded by `TACK_ORCH_POLL_SECS`), not a bug — see the
//! B1 handoff note in TODO.md §6. `poll_metrics` needs no such per-project
//! fan-out — docket's `/metrics` is fleet-wide, one call per plane, exactly
//! like `/health`/`/status.json`.
//!
//! # Trace cursor (card B2, task 34.4)
//!
//! **The cursor is compound, not a bare timestamp.** docket's
//! `GET /traces/{project}?since=` (`serve.py`'s `_traces_page`) mints
//! `"<ts>Z:<n>"`: `ts` is second-granularity and its filter is *inclusive*
//! (`ts >= since`), so a bare "last event's ts" cursor would redeliver every
//! event sharing that exact second on the next poll; `n` is how many of
//! that second's events have already been delivered, letting docket skip
//! exactly those on re-query. Verified by reading `serve.py`'s
//! `_decode_trace_cursor`/`_traces_page` directly (not guessed, not read
//! from docket's stale `ROADMAP.md`).
//!
//! **The frozen `ControlPlane::traces` signature (§1.1) returns
//! `Vec<RemoteEvent>` only — it has no way to carry docket's minted `next`
//! cursor back out**, and this module cannot widen it (`lib.rs` is frozen
//! after Wave 0, owned by W0-A). [`next_trace_cursor`] solves this by
//! reimplementing docket's exact anchor/count algorithm client-side, over
//! the ordered events a poll actually returned plus the cursor that was
//! sent as `since` — both already in hand, no extra I/O. This is provably
//! equivalent to reading docket's own `next` and discarding it: the events
//! `traces()` returns are already `_traces_page`'s post-trim `lines`, so
//! the same anchor/count computation over the same data produces the same
//! answer. See [`next_trace_cursor`]'s doc comment for the mirrored steps
//! and [`decode_trace_cursor`] for the compound-token parse.
//!
//! **`orch_events.id` has no natural key to upsert on.** A trace event is a
//! position in a JSONL stream, not an entity with a stable id — docket's
//! payload carries no monotonic sequence number or byte offset (confirmed
//! by reading `core/trace.py::trace_event`'s writer directly: `ts`,
//! `project`, `session_id`, `agent_role`, `event_type`, `payload`, and two
//! optional fields, nothing else). [`derive_event_id`] is therefore a
//! UUIDv5 (namespace + name, no randomness) over every field of the event
//! plus `control_plane_id`/`remote_project` — the *same* source event
//! always produces the *same* id, so re-ingesting an overlapping cursor
//! window is a no-op row-count-wise (`upsert_orch_events`'s
//! `ON CONFLICT(id)`), not a duplicate.
//!
//! **Retention composition — the sharpest edge in this card.** B3's
//! retention sweep ([`spawn_retention_sweep`], below) rolls `orch_events`
//! rows older than the cutoff into `orch_events_daily` and deletes them. A
//! lost/rewound cursor can re-deliver an event whose row was already rolled
//! up and purged; because its id is content-derived, re-ingesting it would
//! `INSERT` a fresh row indistinguishable from a brand-new event to the
//! *next* sweep, which would then roll its count in a second time —
//! silently double-counting real cost/token totals. [`persist_events`]
//! guards against this at ingest time: an event whose `occurred_at`
//! already predates `now - retention_days` (the same formula
//! [`spawn_retention_sweep`] uses for its own cutoff) is dropped, not
//! inserted. The cost is not (re-)counting a handful of events at the
//! extreme edge of a pathological rewind — strictly better than corrupting
//! a total. See [`persist_events`]'s doc comment for the full argument and
//! `TODO.md` §6's B2 handoff for why a schema-based "purge watermark" was
//! considered and rejected in favor of this simpler, schema-free check.
//!
//! # Retention sweep (card B3, tasks 34.6/34.7) — a separate background task
//!
//! Unlike the per-plane `poll_*`/`persist_*` steps above, the retention sweep
//! ([`spawn_retention_sweep`]) is not part of any plane's `spawn_one` loop —
//! it operates fleet-wide across `orch_events`/`orch_metrics`, independent of
//! which (if any) planes are currently registered. It uses its own narrow
//! trait, [`RetentionStore`], rather than growing [`ControlPlaneStore`] with
//! unrelated fleet-wide concerns. The actual rollup-then-purge SQL (and its
//! crash-safety argument) lives in `tack_db::Repository::rollup_and_purge_orch_events`/
//! `rollup_and_purge_orch_metrics` — this module only schedules it.
//!
//! **Not yet spawned from `server.rs`.** `server.rs` isn't in this card's file
//! ownership (TODO.md §2 lists it as A2's), so [`spawn_retention_sweep`] is
//! built, tested, and ready, but nothing currently calls it at boot. See the
//! TODO.md §6 B3 handoff for the exact one-block addition needed — it mirrors
//! the existing reconciler spawn block immediately above it in `server.rs`.
//!
//! # Jitter
//!
//! Interval jitter (±20%) is derived deterministically from the plane's
//! [`uuid::Uuid`] plus a per-plane tick counter, hashed with
//! [`std::collections::hash_map::DefaultHasher`] — **not** the `rand` crate,
//! which is not a workspace dependency and this card was told not to add.
//! This keeps the schedule reproducible in tests while still spreading N
//! planes' poll times so they don't stampede the gateway in lockstep.
//!
//! # Panic isolation
//!
//! A panic inside a single poll (a bug in a future `poll_*` fn, a bad
//! adapter, anything) must not take down that plane's loop, let alone any
//! other plane's, and must never be visible to a user request (this module
//! makes no HTTP calls *inbound* to Tack at all — it only calls *out* to a
//! control plane). [`spawn_one`] gets this for free from `tokio::spawn`'s
//! unwind boundary: each poll tick runs inside its own spawned task, and a
//! panic there surfaces as a `JoinError` to the (non-panicking) outer loop,
//! which logs it and treats the tick as a failed poll — the loop itself
//! never panics, so it keeps ticking.
//!
//! # Persistence interface
//!
//! [`ControlPlaneStore`] is a narrow trait, not `tack_db::Repository`
//! directly, even though `tack-orch` already depends on `tack-db` (see
//! `Cargo.toml`) and `crates/tack-db/src/repo/orch.rs` (card A3) landed with
//! everything this module needs on the persistence side
//! (`list_control_planes`, `update_control_plane_health`,
//! `get_control_plane_token`). The reason it's still an abstraction: turning
//! a `tack_db::repo::orch::ControlPlane` row into a live `Arc<dyn
//! ControlPlane>` requires dispatching on `kind` to a concrete adapter
//! (today only `"docket"` → `DocketAdapter`), and that adapter
//! (`adapters::docket`, card A1) had not landed yet when this file was
//! written. Seeing the real repo shape did let the trait below match A3's
//! signatures closely (same field meaning, `i64` failure counts, borrowed
//! `Option<&str>` for `api_version`) so the eventual glue is a thin,
//! mechanical wrapper. See the TODO.md §6 A2 handoff note for exactly what's
//! left to wire.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tack_db::repo::orch::{NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    ControlPlane, FleetStatus, Health, MetricSample, OrchError, RemoteApproval, RemoteEvent,
    RemoteRun,
};

// ---------------------------------------------------------------------------
// Health state machine
// ---------------------------------------------------------------------------

/// Consecutive `/health` + `/status.json` failures before a plane is shown
/// `degraded`. Recovery is immediate on a single success — see
/// [`HealthTracker::observe`].
pub const DEGRADED_AFTER_FAILURES: i64 = 3;

/// Consecutive failures before a plane is shown `unreachable`.
pub const UNREACHABLE_AFTER_FAILURES: i64 = 10;

/// Poll backoff never waits longer than this, regardless of how long a plane
/// has been failing.
pub const MAX_BACKOFF_SECS: u64 = 300;

/// docket's `SERVE_API_VERSION` as verified by W0-A against `serve.py`
/// (TODO.md §6, W0-A handoff). Compared against [`FleetStatus::api_version`]
/// on every poll — see [`evaluate`] and the module doc's apiVersion policy.
pub const EXPECTED_API_VERSION: &str = "2";

/// A control plane's health as the reconciler sees it. Column values in
/// `control_planes.health` are these variants' [`HealthState::as_str`]
/// output verbatim (`"healthy"` / `"degraded"` / `"unreachable"`) — the
/// column also allows `"unknown"` as its pre-first-poll default, which this
/// enum deliberately has no variant for: nothing in this module ever writes
/// `"unknown"`, only a fresh row's DEFAULT does.
///
/// Variant order is significant: `derive(PartialOrd, Ord)` ranks
/// `Healthy < Degraded < Unreachable`, which [`evaluate`] and
/// [`HealthTracker::observe`] rely on to combine two independent signals
/// (reachability and apiVersion match) by taking the more severe one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unreachable,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Degraded => "degraded",
            HealthState::Unreachable => "unreachable",
        }
    }
}

impl std::fmt::Display for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Severity to log a health transition at. Anything that doesn't change
/// state (a poll that fails for the 7th time in a row while already
/// `unreachable`, say) logs at `debug`, not `warn` — this is what makes the
/// "logs backoff at warn without spam" acceptance criterion true: warn-level
/// logging only fires *on a transition*, so a sustained outage produces at
/// most two warns (entering `degraded`, entering `unreachable`) no matter
/// how long it lasts, plus one `info` on recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogSeverity {
    Warn,
    Info,
}

/// What [`HealthTracker::observe`] decided this tick, ready to persist and log.
#[derive(Debug, Clone)]
struct HealthTransition {
    state: HealthState,
    consecutive_failures: i64,
    /// `Some(now)` on a successful poll; `None` on a failed one — mirrors
    /// `update_control_plane_health`'s contract exactly (A3): `None` means
    /// "leave the stored `last_seen_at` untouched", not "clear it".
    last_seen_at: Option<DateTime<Utc>>,
    log: Option<LogSeverity>,
}

/// In-memory per-plane state driving the health state machine. One instance
/// lives for the lifetime of a plane's `tokio` task ([`spawn_one`]) — it is
/// not itself persisted; [`HealthTransition`]/[`HealthRecord`] are the
/// persisted projection of it after each tick.
#[derive(Debug, Clone)]
struct HealthTracker {
    consecutive_failures: i64,
    state: HealthState,
    version_mismatch: bool,
}

impl HealthTracker {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            state: HealthState::Healthy,
            version_mismatch: false,
        }
    }

    /// Feed one poll's outcome into the state machine.
    ///
    /// `reachable` is true iff both required Wave-1 calls (`/health`,
    /// `/status.json`) succeeded. `version_mismatch` is an independent
    /// signal (see [`evaluate`]): a plane can be fully reachable and still
    /// show `degraded` because it's running a docket version this Tack
    /// doesn't understand. Recovery is immediate — a single `reachable`
    /// poll resets `consecutive_failures` to zero and re-evaluates state
    /// from scratch, regardless of how deep the prior outage was.
    fn observe(
        &mut self,
        reachable: bool,
        version_mismatch: bool,
        now: DateTime<Utc>,
    ) -> HealthTransition {
        let prev_state = self.state;
        let prev_version_mismatch = self.version_mismatch;

        self.consecutive_failures = if reachable {
            0
        } else {
            self.consecutive_failures.saturating_add(1)
        };

        let reachability_state = if self.consecutive_failures < DEGRADED_AFTER_FAILURES {
            HealthState::Healthy
        } else if self.consecutive_failures < UNREACHABLE_AFTER_FAILURES {
            HealthState::Degraded
        } else {
            HealthState::Unreachable
        };

        // A version mismatch never reports healthy, but never overrides a
        // worse reachability-driven state either — take the more severe of
        // the two independent signals.
        let version_floor = if version_mismatch {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };
        let new_state = reachability_state.max(version_floor);

        self.state = new_state;
        self.version_mismatch = version_mismatch;

        let last_seen_at = reachable.then_some(now);

        let log = if new_state != prev_state {
            Some(if new_state > prev_state {
                LogSeverity::Warn
            } else {
                LogSeverity::Info
            })
        } else if version_mismatch && !prev_version_mismatch {
            Some(LogSeverity::Warn)
        } else {
            None
        };

        HealthTransition {
            state: new_state,
            consecutive_failures: self.consecutive_failures,
            last_seen_at,
            log,
        }
    }
}

/// Poll interval grows exponentially with consecutive failures (doubling
/// each time), capped at [`MAX_BACKOFF_SECS`]. `consecutive_failures <= 0`
/// (healthy) returns `base_secs` unchanged — backoff only kicks in once a
/// poll has actually failed.
pub fn backoff_secs(consecutive_failures: i64, base_secs: u64) -> u64 {
    let base = base_secs.max(1);
    if consecutive_failures <= 0 {
        return base;
    }
    // Cap the exponent well below 64 so the shift can't overflow; the
    // result saturates to MAX_BACKOFF_SECS long before this matters.
    let exp = consecutive_failures.min(20) as u32;
    let multiplier = 1u64.checked_shl(exp).unwrap_or(u64::MAX);
    base.saturating_mul(multiplier).min(MAX_BACKOFF_SECS)
}

/// Deterministic ±20% jitter, so N planes sharing the same `TACK_ORCH_POLL_SECS`
/// don't all wake up on the same tick and stampede the gateway. Derived from
/// a hash of `(plane_id, tick)` via `DefaultHasher` — `rand` is not a
/// workspace dependency and this card was told not to add one (TODO.md §6,
/// card A2's requirements). Deterministic on purpose: it keeps scheduling
/// reproducible in tests and doesn't need a seeded RNG threaded through.
fn jittered_secs(plane_id: &Uuid, tick: u64, base_secs: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    plane_id.hash(&mut hasher);
    tick.hash(&mut hasher);
    let h = hasher.finish();

    // Map the hash to a jitter fraction in [-0.20, 0.20].
    let bucket = (h % 4001) as i64 - 2000; // -2000..=2000
    let jitter_frac = bucket as f64 / 10000.0; // -0.20..=0.20

    let base = base_secs.max(1) as f64;
    let jittered = base * (1.0 + jitter_frac);
    jittered.round().max(1.0) as u64
}

// ---------------------------------------------------------------------------
// Fetch phase — HTTP calls only, no database access anywhere in this
// section (§0 rule 5; see the module doc's "three-phase shape").
// ---------------------------------------------------------------------------

async fn poll_health(control_plane: &Arc<dyn ControlPlane>) -> Result<Health, OrchError> {
    control_plane.health().await
}

async fn poll_status(control_plane: &Arc<dyn ControlPlane>) -> Result<FleetStatus, OrchError> {
    control_plane.status().await
}

/// `GET /runs?project=`, one call per linked project (docket has no
/// fleet-wide runs listing — see TODO.md §1.4). `projects` comes from
/// [`ControlPlaneStore::list_linked_projects`], fetched in [`spawn_one`]
/// before this runs. Each project's own result is kept independent so one
/// project's failure never drops another's runs for this tick (card B1).
async fn poll_runs(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
) -> Vec<(String, Result<Vec<RemoteRun>, OrchError>)> {
    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        let result = control_plane.list_runs(Some(project)).await;
        out.push((project.clone(), result));
    }
    out
}

/// `GET /approvals` — fleet-wide, not per-project (card B1). A failure here
/// must never influence [`evaluate`]'s reachability verdict: a docket that's
/// up but whose `/approvals` route errors is a degraded *feature*, not a
/// degraded *plane*.
async fn poll_approvals(
    control_plane: &Arc<dyn ControlPlane>,
) -> Result<Vec<RemoteApproval>, OrchError> {
    control_plane.list_approvals().await
}

/// `GET /metrics` — fleet-wide, not per-project, exactly like `/health`/
/// `/status.json` (card B3, task 34.3). `ControlPlane::metrics()` (the
/// `DocketAdapter` impl, card A1) already fetches the raw Prometheus text and
/// parses it via `adapters::prometheus::parse` internally — this function is
/// not a second parsing step, just the same thin HTTP-call wrapper every
/// other `poll_*` fn is. A failure here must never influence [`evaluate`]'s
/// reachability verdict, same as `poll_approvals`.
async fn poll_metrics(
    control_plane: &Arc<dyn ControlPlane>,
) -> Result<Vec<MetricSample>, OrchError> {
    control_plane.metrics().await
}

/// One linked project's `/traces?since=` result for this tick, paired with
/// the exact cursor that was actually sent as `since` — carried alongside
/// the result rather than re-derived at persist time from a separately
/// threaded map, so [`persist_events`] can never pair a result with the
/// wrong "previous cursor" even if a future edit reorders when cursors are
/// read. `since` is `None` for a project that has never been polled before
/// (no stored row yet) — docket treats an absent/empty `since` as "from the
/// beginning", so the very first poll for a newly-linked project mirrors
/// its entire trace history, same as `poll_runs`'s first-poll behavior for
/// CLI-dispatched runs.
type TracesPollResult = (String, Option<String>, Result<Vec<RemoteEvent>, OrchError>);

/// `GET /traces/{project}?since=`, one call per linked project — docket has
/// no fleet-wide trace listing, same shape as [`poll_runs`] (card B2, task
/// 34.4). `cursors` is this tick's starting cursor per project, resolved by
/// [`spawn_one`] via `ControlPlaneStore::list_trace_cursors` before the
/// fetch phase begins — the same "DB read outside the panic-isolation
/// boundary" pattern B1 established for `list_linked_projects` (see the
/// module doc). Each project's own result is kept independent so one
/// project's failure never blocks another's traces for this tick.
async fn poll_traces(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
    cursors: &HashMap<String, String>,
) -> Vec<TracesPollResult> {
    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        let since = cursors.get(project).cloned();
        let result = control_plane.traces(project, since.as_deref()).await;
        out.push((project.clone(), since, result));
    }
    out
}

/// Every HTTP call one poll tick needs, gathered as a flat struct-of-results
/// so one failing call never blocks the others from being attempted. Wave 2
/// cards append fields here — see the module doc for the exact recipe.
struct FetchOutcome {
    health: Result<Health, OrchError>,
    status: Result<FleetStatus, OrchError>,
    runs: Vec<(String, Result<Vec<RemoteRun>, OrchError>)>,
    approvals: Result<Vec<RemoteApproval>, OrchError>,
    metrics: Result<Vec<MetricSample>, OrchError>,
    traces: Vec<TracesPollResult>,
}

/// Fetch phase for one poll tick. No database access happens in this
/// function or anything it calls — `projects`/`trace_cursors` are supplied
/// by the caller ([`spawn_one`]), already resolved before this is invoked.
/// Returns both the evaluated verdict (used for health) and the raw fetch
/// (used for runs/approvals/traces persistence) — see the module doc's "one
/// deviation from the recipe" note for why this isn't just
/// `PollEvaluation`.
async fn reconcile_once(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
    trace_cursors: &HashMap<String, String>,
) -> (PollEvaluation, FetchOutcome) {
    let fetched = FetchOutcome {
        health: poll_health(control_plane).await,
        status: poll_status(control_plane).await,
        runs: poll_runs(control_plane, projects).await,
        approvals: poll_approvals(control_plane).await,
        metrics: poll_metrics(control_plane).await,
        traces: poll_traces(control_plane, projects, trace_cursors).await,
    };
    let evaluation = evaluate(&fetched);
    (evaluation, fetched)
}

/// Decide phase's input-independent half: turns a [`FetchOutcome`] into a
/// plain verdict. Deliberately reads only `.health`/`.status` — a
/// runs/approvals/traces/metrics fetch failure (once Wave 2 adds those
/// fields) must never affect plane reachability; those are data-ingestion
/// concerns, not health.
struct PollEvaluation {
    reachable: bool,
    version_mismatch: bool,
    observed_api_version: Option<String>,
    detail: String,
}

/// apiVersion policy (TODO.md §5's risk table, and card A2's requirement 5):
/// a "mismatch" is a difference in the **major** version component — the
/// substring before the first `.`, or the whole string if there is no `.`.
/// docket's version scheme today is a bare incrementing integer (`"2"`), so
/// in practice this is currently an exact-string comparison; the `.`-split
/// is there so a future move to a dotted scheme (`"2.1"` vs `"3.0"`) degrades
/// only on the part that actually signals a breaking contract change, not on
/// every patch bump.
fn major_version(v: &str) -> &str {
    v.split('.').next().unwrap_or(v)
}

fn evaluate(outcome: &FetchOutcome) -> PollEvaluation {
    let health_ok = outcome.health.is_ok();
    let status_ok = outcome.status.is_ok();
    let reachable = health_ok && status_ok;

    let (version_mismatch, observed_api_version) = match &outcome.status {
        Ok(status) => {
            let mismatch =
                major_version(&status.api_version) != major_version(EXPECTED_API_VERSION);
            (mismatch, Some(status.api_version.clone()))
        }
        Err(_) => (false, None),
    };

    let detail = if !reachable {
        let mut parts = Vec::new();
        if let Err(e) = &outcome.health {
            parts.push(format!("health: {e}"));
        }
        if let Err(e) = &outcome.status {
            parts.push(format!("status: {e}"));
        }
        parts.join("; ")
    } else if version_mismatch {
        format!(
            "apiVersion mismatch: Tack expects major version {}, control plane reports {}",
            major_version(EXPECTED_API_VERSION),
            observed_api_version.as_deref().unwrap_or("?")
        )
    } else {
        String::new()
    };

    PollEvaluation {
        reachable,
        version_mismatch,
        observed_api_version,
        detail,
    }
}

// ---------------------------------------------------------------------------
// Persistence interface
// ---------------------------------------------------------------------------

/// A control plane the reconciler should be polling, with its live adapter
/// already constructed. Building this from a `control_planes` DB row (kind
/// dispatch → concrete adapter, e.g. `DocketAdapter`) is `ControlPlaneStore`
/// implementors' job, not this module's — see the module doc's persistence
/// section.
#[derive(Clone)]
pub struct RegisteredPlane {
    pub id: Uuid,
    pub control_plane: Arc<dyn ControlPlane>,
}

/// What to persist after one poll tick. Field shapes mirror
/// `tack_db::Repository::update_control_plane_health`'s parameters exactly
/// (`i64` failure count, `Option<DateTime<Utc>>` with `None` = "don't
/// touch") so a `ControlPlaneStore` impl backed by A3's repo is a direct
/// pass-through.
#[derive(Debug, Clone)]
pub struct HealthRecord {
    pub health: HealthState,
    pub consecutive_failures: i64,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub api_version: Option<String>,
}

/// The narrow persistence interface the reconciler needs. Deliberately not
/// `tack_db::Repository` directly — see the module doc's "Persistence
/// interface" section for why, and TODO.md §6 (A2 handoff) for how to wire
/// a real implementation once `adapters::docket` (A1) lands.
///
/// Card B1 (Wave 2) added the four methods below `record_health`. Each is a
/// thin, mechanical pass-through to a single `tack_db::repo::orch` function
/// (`list_orch_links_for_plane`, `find_orch_task_by_remote_task_id`,
/// `upsert_orch_runs`, `upsert_orch_approvals`) — the same shape
/// `record_health` already has against `update_control_plane_health`. No
/// correlation or business logic belongs in an implementor of this trait;
/// that lives in [`spawn_one`]'s persistence phase (`persist_runs`/
/// `persist_approvals`), which is the whole reason this trait stays narrow
/// rather than growing into `tack_db::Repository` by another name.
#[async_trait::async_trait]
pub trait ControlPlaneStore: Send + Sync {
    /// Every control plane currently registered, each with a live adapter
    /// ready to poll.
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError>;

    /// Persist one poll tick's outcome for a single plane.
    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError>;

    /// Distinct `remote_project` names linked to this control plane
    /// (`orch_links.remote_project`) — what `poll_runs` needs to build its
    /// per-project `/runs?project=` calls. Order is not significant.
    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError>;

    /// Look up the Tack item a docket `remote_task_id` was dispatched for
    /// (`orch_tasks.remote_task_id` → `orch_tasks.item_id`), if any. `Ok(None)`
    /// means "no such task is known to Tack" — not an error; a run or
    /// approval correlating against it stays unattributed for this tick
    /// (card B1's "must not error" requirement for CLI-dispatched work).
    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError>;

    /// Batch upsert into `orch_runs` (`ON CONFLICT(run_id)`, idempotent —
    /// see `tack_db::repo::orch::upsert_orch_runs`). A `None` `item_id` on a
    /// `NewOrchRun` never clobbers a previously-learned attribution; the
    /// repo layer's `COALESCE` guarantees that, not this trait.
    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError>;

    /// Batch upsert into `orch_approvals` (`ON CONFLICT(token)`, idempotent
    /// — see `tack_db::repo::orch::upsert_orch_approvals`). Same
    /// never-unlearn-an-attribution guarantee as [`Self::upsert_runs`].
    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError>;

    /// Batch insert into `orch_metrics` (append-only — see
    /// `tack_db::repo::orch::upsert_orch_metrics`'s doc comment for why a
    /// metric sample has no natural key to conflict on, unlike every other
    /// method on this trait). Card B3 (Wave 2, metrics ingestion).
    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError>;

    // ── Card B2 (Wave 2, trace ingestion, task 34.4) ──
    //
    // Same thin-pass-through discipline as every method above: no cursor
    // arithmetic, no event-id derivation, no retention-age filtering here —
    // all of that lives in `next_trace_cursor`/`derive_event_id`/
    // `persist_events` in this module. An implementor's job is exactly
    // "read/write these rows", nothing more.

    /// Every stored resume cursor for this plane's linked projects, keyed by
    /// `remote_project` (`tack_db::repo::orch::list_trace_cursors`). A
    /// project absent from the map has never been polled (or its cursor was
    /// never advanced) — [`poll_traces`] treats that as `since: None`
    /// ("from the beginning"), not an error.
    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError>;

    /// Persist the resume cursor for one `(control_plane_id, remote_project)`
    /// pair after a poll (`tack_db::repo::orch::set_trace_cursor`) — see the
    /// module doc's "Trace cursor" section for why the cursor lives keyed
    /// this way rather than as a column on `orch_links`.
    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError>;

    /// Batch upsert into `orch_events` (`ON CONFLICT(id)`, idempotent — see
    /// `tack_db::repo::orch::upsert_orch_events` and [`derive_event_id`],
    /// which is what makes the same source event always produce the same
    /// `id`).
    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError>;
}

// ---------------------------------------------------------------------------
// Persist phase — runs/approvals ingestion and item correlation (card B1)
// ---------------------------------------------------------------------------
//
// Everything below is called from spawn_one's persistence phase, strictly
// after the fetch phase (reconcile_once) has already completed — see the
// module doc. Correlation itself (mapping a docket-side id to a Tack item)
// lives here, not in the ControlPlaneStore trait or its implementors: the
// store's job is mechanical CRUD, this module's job is deciding what to CRUD.

/// A run or approval attributes to whichever task id, out of a candidate
/// list, is the first one Tack actually knows about. `None` means none of
/// the candidates correlate — the normal, expected state for a
/// docket-CLI-dispatched run (empty `task_ids`) and must not be treated as
/// an error (card B1's requirement).
async fn correlate_remote_task(
    store: &dyn ControlPlaneStore,
    candidates: impl IntoIterator<Item = &str>,
) -> Option<Uuid> {
    for remote_task_id in candidates {
        match store.find_item_for_remote_task(remote_task_id).await {
            Ok(Some(item_id)) => return Some(item_id),
            Ok(None) => continue,
            Err(e) => {
                warn!(
                    remote_task_id = %remote_task_id,
                    error = %e,
                    "failed to correlate remote task id to an item; treating as uncorrelated for this candidate"
                );
                continue;
            }
        }
    }
    None
}

/// `RemoteApproval::context`'s one documented shape is
/// `{"taskId": "...", "pipelineIndex": 0}` (TODO.md §1.4), but `context` is
/// an open dict on docket's side — a missing or non-string `taskId` is an
/// uncorrelated approval, not a parse error.
fn extract_task_id(context: &serde_json::Value) -> Option<String> {
    context.get("taskId")?.as_str().map(str::to_string)
}

/// Parses one of docket's two observed ISO 8601 timestamp conventions
/// (`...+00:00` from `core/runs.py`, `...Z` from `core/approval.py` — see
/// TODO.md §6's A1 handoff note) into a `DateTime<Utc>`. `None` in, `None`
/// out; a malformed string in also degrades to `None` rather than failing
/// the whole poll — a run/approval with an unparseable timestamp still gets
/// its other fields mirrored.
fn parse_optional_rfc3339(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Batch-correlates and upserts one tick's runs. `runs` is
/// `FetchOutcome::runs` verbatim: one `(remote_project, Result<..>)` pair per
/// linked project polled this tick. A project whose poll failed is logged
/// and skipped — it does not block the other projects' runs from landing,
/// and it never touches plane health (only `.health`/`.status` do that; see
/// `evaluate`).
async fn persist_runs(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    runs: &[(String, Result<Vec<RemoteRun>, OrchError>)],
) {
    let mut new_runs = Vec::new();

    for (project, result) in runs {
        match result {
            Ok(remote_runs) => {
                for run in remote_runs {
                    let item_id =
                        correlate_remote_task(store, run.task_ids.iter().map(String::as_str)).await;
                    new_runs.push(NewOrchRun {
                        run_id: run.id.clone(),
                        item_id,
                        // `run.project` (docket's own field on the record) rather
                        // than the queried `project` string — they should always
                        // agree since we always pass `Some(project)`, but the
                        // record's own field is the more authoritative source.
                        remote_project: run.project.clone(),
                        source: run.source.as_str().to_string(),
                        state: run.state.as_str().to_string(),
                        started_at: parse_optional_rfc3339(run.started_at.as_deref()),
                        ended_at: parse_optional_rfc3339(run.finished_at.as_deref()),
                        error: (!run.error.is_empty()).then(|| run.error.clone()),
                    });
                }
            }
            Err(e) => {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to poll runs for a linked project this tick; will retry next tick"
                );
            }
        }
    }

    if !new_runs.is_empty() {
        let count = new_runs.len();
        if let Err(e) = store.upsert_runs(control_plane_id, &new_runs).await {
            warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored runs");
        } else {
            debug!(control_plane_id = %control_plane_id, count, "mirrored runs upserted");
        }
    }
}

/// Batch-correlates and upserts one tick's approvals. `approvals` is
/// `FetchOutcome::approvals` verbatim — a fleet-wide poll, not per-project.
/// A record whose `created` timestamp doesn't parse is skipped with a
/// warning (`requested_at` is a required column) rather than aborting the
/// whole batch; every other approval in the same poll still lands.
async fn persist_approvals(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    approvals: &Result<Vec<RemoteApproval>, OrchError>,
) {
    let remote_approvals = match approvals {
        Ok(a) => a,
        Err(e) => {
            debug!(
                control_plane_id = %control_plane_id,
                error = %e,
                "failed to poll approvals this tick; will retry next tick"
            );
            return;
        }
    };

    let mut new_approvals = Vec::with_capacity(remote_approvals.len());

    for approval in remote_approvals {
        let task_id = extract_task_id(&approval.context);
        let item_id = match &task_id {
            Some(task_id) => correlate_remote_task(store, std::iter::once(task_id.as_str())).await,
            None => None,
        };

        let Some(requested_at) = parse_optional_rfc3339(Some(&approval.created)) else {
            warn!(
                token = %approval.token,
                created = %approval.created,
                "skipping approval with an unparseable created timestamp"
            );
            continue;
        };

        new_approvals.push(NewOrchApproval {
            token: approval.token.clone(),
            item_id,
            remote_task_id: task_id,
            // `role` is docket's name for who the gate is asking — mapped
            // onto the `agent` column, which is what the fleet-wide
            // approvals inbox (Wave 4, D1) actually displays.
            agent: Some(approval.role.clone()),
            action: Some(approval.action.clone()),
            state: approval.state.as_str().to_string(),
            requested_at,
            // RemoteApproval carries no decided_at — /approvals only ever
            // returns the still-`pending` set (see docket_adapter's
            // `ApprovalsResponse` doc comment); a real decision is Wave 3.
            decided_at: None,
        });
    }

    if !new_approvals.is_empty() {
        let count = new_approvals.len();
        if let Err(e) = store
            .upsert_approvals(control_plane_id, &new_approvals)
            .await
        {
            warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored approvals");
        } else {
            debug!(control_plane_id = %control_plane_id, count, "mirrored approvals upserted");
        }
    }
}

/// Persists one tick's `/metrics` scrape (card B3). No correlation is needed
/// — metrics aren't attributed to an item — so, unlike `persist_runs`/
/// `persist_approvals`, this is a straight translation from
/// `FetchOutcome::metrics` to a batch of `NewOrchMetric`. A poll failure is
/// logged and skipped, never propagated into the health verdict already
/// decided in the fetch phase (the same rule every other `persist_*` fn
/// here follows).
async fn persist_metrics(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    metrics: &Result<Vec<MetricSample>, OrchError>,
) {
    let samples = match metrics {
        Ok(s) => s,
        Err(e) => {
            debug!(
                control_plane_id = %control_plane_id,
                error = %e,
                "failed to poll metrics this tick; will retry next tick"
            );
            return;
        }
    };

    if samples.is_empty() {
        return;
    }

    let new_metrics: Vec<NewOrchMetric> = samples
        .iter()
        .map(|s| NewOrchMetric {
            name: s.name.clone(),
            labels: s.labels.clone(),
            value: s.value,
        })
        .collect();

    let count = new_metrics.len();
    if let Err(e) = store.upsert_metrics(control_plane_id, &new_metrics).await {
        warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored metrics");
    } else {
        debug!(control_plane_id = %control_plane_id, count, "mirrored metrics upserted");
    }
}

// ---------------------------------------------------------------------------
// Trace ingestion (card B2, task 34.4) — event-id derivation, cursor
// reconstruction, and persistence. See the module doc's "Trace cursor"
// section for the full argument; the functions below are its implementation.
// ---------------------------------------------------------------------------

/// Fixed namespace for [`derive_event_id`]'s UUIDv5 derivation. The exact
/// bytes are an arbitrary (but permanently fixed) 16-byte constant — ASCII
/// spelling "tack-orch-events" is a mnemonic, not a meaningful namespace
/// URL. Changing these bytes would silently re-mint a different id for
/// every previously-ingested event, defeating the entire point of a
/// deterministic id, so this must never change once any real deployment has
/// ingested a single trace event.
const ORCH_EVENT_ID_NAMESPACE: Uuid = Uuid::from_bytes(*b"tack-orch-events");

/// Derives `orch_events.id` as a pure function of the source docket trace
/// event, so the *same* event ingested on two different polls — an
/// overlapping cursor window, a rewound/lost cursor, a restart — always
/// produces the *same* row. `upsert_orch_events`'s `ON CONFLICT(id) DO
/// UPDATE` then makes re-ingestion a no-op row-count-wise, which is exactly
/// this card's acceptance bar.
///
/// docket's trace records (`core/trace.py::trace_event`, confirmed by
/// reading the writer directly, not guessed) carry no monotonic sequence
/// number or byte offset — every field written is `ts`, `project`,
/// `session_id`, `agent_role`, `event_type`, `payload`, and two optional
/// fields (`cost_usd`, `duration_ms`). So option 1 in TODO.md's preference
/// order (`(control_plane_id, remote_project, seq)`) is unavailable; this
/// is option 2 — UUIDv5 (namespace + name, deterministic, no randomness)
/// over `control_plane_id`, `remote_project`, and every field of the event.
///
/// `payload` is a `serde_json::Value`; this crate never enables
/// `serde_json`'s `preserve_order` feature (see `Cargo.toml` — no
/// `indexmap` in the dependency tree), so `Value::Object` is backed by a
/// `BTreeMap` and always serializes with its keys in sorted order. TODO.md's
/// "canonicalised form, stable field order" requirement falls out of the
/// workspace's existing `serde_json` configuration for free, not a bespoke
/// canonicalizer.
///
/// Every field is joined with `\u{1}` (a control character no real docket
/// field is going to contain) so that, e.g., an empty `session_id` followed
/// by `"x"` can never hash the same as a non-empty `session_id` of `"x"`
/// preceded by nothing — naive string concatenation without a delimiter
/// would not have that property.
///
/// **A caveat worth naming, not hiding:** two *genuinely distinct* docket
/// events that happen to be byte-for-byte identical across every field this
/// function reads (same second-granularity `ts`, same session, same role,
/// same type, same payload, same cost/duration) hash to the same id and
/// collapse into one row. Given a payload usually carries something
/// turn-specific (a tool command, a token count), this is vanishingly
/// unlikely in practice — and if it ever happened, the two events were
/// already indistinguishable to any consumer of this table.
fn derive_event_id(control_plane_id: Uuid, remote_project: &str, event: &RemoteEvent) -> Uuid {
    const SEP: char = '\u{1}';
    let payload = serde_json::to_string(&event.payload).unwrap_or_default();
    let canonical = format!(
        "{control_plane_id}{SEP}{remote_project}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{payload}{SEP}{:?}{SEP}{:?}",
        event.ts,
        event.session_id,
        event.agent_role,
        event.event_type,
        event.cost_usd_estimated,
        event.duration_ms,
    );
    Uuid::new_v5(&ORCH_EVENT_ID_NAMESPACE, canonical.as_bytes())
}

/// Mirrors `serve.py`'s `_decode_trace_cursor` exactly: a compound
/// `"<ts>Z:<n>"` token (ts ends in `Z`, tail is all digits) decodes to
/// `(ts, n)`; anything else — a bare hand-typed timestamp, or an empty
/// string — decodes to `(raw, 0)`. The trailing-`Z` requirement is
/// load-bearing: a timestamp itself contains colons
/// (`"...T12:34:56Z"`), so without it `"...T12:34:56"` would misparse as a
/// compound token with a bogus count. Returns `("", 0)` for an empty input
/// (the "never polled before" starting state).
fn decode_trace_cursor(raw: &str) -> (&str, i64) {
    if raw.is_empty() {
        return ("", 0);
    }
    if let Some((ts, tail)) = raw.rsplit_once(':')
        && ts.ends_with('Z')
        && !tail.is_empty()
        && tail.bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = tail.parse::<i64>()
    {
        return (ts, n);
    }
    (raw, 0)
}

/// Reconstructs docket's own `next` cursor value purely from the ordered
/// batch of events a poll actually returned, plus the cursor that was sent
/// as `since` for that call — see the module doc's "Trace cursor" section
/// for why this reimplements `serve.py`'s `_traces_page` algorithm
/// client-side instead of reading docket's own `next` field. A pure
/// function, safe to unit-test byte-for-byte against hand-computed cases.
///
/// Mirrors `_traces_page` exactly:
/// - no events at all → the cursor doesn't move (`previous`, unchanged).
/// - no event in the batch has a non-empty `ts` → doesn't move either
///   (advancing past `previous` without a usable anchor risks a full
///   replay on the next poll).
/// - otherwise: the anchor is the last event (from the end) with a
///   non-empty `ts`; the count is how many events, counting backward from
///   the anchor, share that exact `ts` (a contiguous trailing same-second
///   run); if the anchor's `ts` equals `previous`'s decoded ts, the
///   previous count carries forward and adds to this run (the same-second
///   boundary spans two polls) — otherwise the run starts fresh at 0.
fn next_trace_cursor(previous: Option<&str>, events: &[RemoteEvent]) -> Option<String> {
    let (prev_ts, prev_count) = decode_trace_cursor(previous.unwrap_or(""));

    if events.is_empty() {
        return previous.map(str::to_string);
    }

    let anchor = events
        .iter()
        .enumerate()
        .rev()
        .find(|(_, e)| !e.ts.is_empty());
    let Some((anchor_idx, anchor_event)) = anchor else {
        return previous.map(str::to_string);
    };
    let anchor_ts = anchor_event.ts.as_str();

    let mut count_at_anchor = 0i64;
    for event in events[..=anchor_idx].iter().rev() {
        if event.ts != anchor_ts {
            break;
        }
        count_at_anchor += 1;
    }
    if anchor_ts == prev_ts {
        count_at_anchor += prev_count;
    }

    Some(format!("{anchor_ts}:{count_at_anchor}"))
}

/// Extracts the trailing `<suffix>` from docket's `session_id` convention
/// `"agent:<project>:<suffix>"` (`core/dispatch.py`'s `enqueue_task`/hop
/// execution, confirmed by reading the writer directly) as a candidate
/// `orch_tasks.remote_task_id` to correlate against — the same "try, and
/// treat a miss as normal" shape [`persist_approvals`] already uses for
/// `context.taskId`. `<suffix>` is the real task id for a task-dispatched
/// session, but docket also uses this convention for non-task sessions
/// (`"agent:<project>:dispatch"` for a bare project-level dispatch,
/// `core/pod.py`'s own project-key session) — [`correlate_remote_task`] on
/// the result simply won't find a matching `orch_tasks` row for those,
/// which is not an error, so no special-casing is needed here beyond
/// parsing the string.
fn session_id_task_id(session_id: &str) -> Option<String> {
    session_id
        .strip_prefix("agent:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(_project, suffix)| suffix.to_string())
}

/// Batch-derives, correlates, and upserts one tick's trace events, then
/// advances (or leaves untouched) each project's cursor. `traces` is
/// `FetchOutcome::traces` verbatim. A project whose poll failed is logged
/// and skipped — same as [`persist_runs`]/[`persist_approvals`] — and never
/// touches plane health (only `.health`/`.status` do that; see
/// [`evaluate`]).
///
/// **Retention composition** (see the module doc's "Trace cursor" section
/// for the full argument): an event whose `occurred_at` already predates
/// `now - retention_days` — the same cutoff formula
/// [`spawn_retention_sweep`] uses — is dropped, not inserted. Without this,
/// a lost/rewound cursor could resurrect a raw row for an event that was
/// already rolled into `orch_events_daily` and purged; because
/// `orch_events.id` is content-derived rather than server-generated, that
/// resurrection is indistinguishable from a brand-new event to the next
/// sweep, which would then roll its count in a *second* time. Dropping it
/// here instead means the only cost is not (re-)counting a handful of
/// events at the extreme edge of a pathological rewind — never a corrupted
/// total.
async fn persist_events(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    traces: &[TracesPollResult],
    retention_days: u32,
) {
    let retention_cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

    for (project, since, result) in traces {
        let events = match result {
            Ok(events) => events,
            Err(e) => {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to poll traces for a linked project this tick; will retry next tick"
                );
                continue;
            }
        };

        let next_cursor = next_trace_cursor(since.as_deref(), events);

        let mut new_events = Vec::with_capacity(events.len());
        let mut dropped_stale = 0u32;
        for event in events {
            let Some(occurred_at) = parse_optional_rfc3339(Some(&event.ts)) else {
                warn!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    ts = %event.ts,
                    "skipping trace event with an unparseable ts"
                );
                continue;
            };
            if occurred_at < retention_cutoff {
                dropped_stale += 1;
                continue;
            }

            let id = derive_event_id(control_plane_id, project, event);
            let item_id = match session_id_task_id(&event.session_id) {
                Some(task_id) => {
                    correlate_remote_task(store, std::iter::once(task_id.as_str())).await
                }
                None => None,
            };

            new_events.push(NewOrchEvent {
                id,
                item_id,
                // docket's trace payload carries no run_id, only session_id
                // (itself derived from task_id, not run_id — see
                // `session_id_task_id`'s doc). Left unset rather than
                // guessing at a session_id → run_id lookup this store
                // doesn't expose.
                run_id: None,
                event_type: event.event_type.clone(),
                payload: event.payload.clone(),
                occurred_at,
            });
        }

        if dropped_stale > 0 {
            warn!(
                control_plane_id = %control_plane_id,
                project = %project,
                dropped_stale,
                retention_days,
                "dropped trace events older than the retention cutoff instead of \
                 resurrecting an already-purged row"
            );
        }

        if !new_events.is_empty() {
            let count = new_events.len();
            if let Err(e) = store.upsert_events(control_plane_id, &new_events).await {
                warn!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to persist mirrored trace events"
                );
            } else {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    count,
                    "mirrored trace events upserted"
                );
            }
        }

        if let Some(next) = &next_cursor
            && Some(next.as_str()) != since.as_deref()
            && let Err(e) = store
                .set_trace_cursor(control_plane_id, project, next)
                .await
        {
            warn!(
                control_plane_id = %control_plane_id,
                project = %project,
                error = %e,
                "failed to persist trace cursor"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Retention sweep (card B3, tasks 34.6/34.7) — a separate background task,
// not per-plane. See the module doc's "Retention sweep" section for how this
// relates to the per-plane poll/persist machinery above.
// ---------------------------------------------------------------------------

/// Default retention window, matching `AppConfig::orch_event_retention_days`'s
/// own default (`crates/tack-api/src/config.rs`). Only a fallback for callers
/// that don't have the configured value handy (e.g. a quick manual sweep) —
/// [`spawn_retention_sweep`] takes `retention_days` as an explicit parameter
/// so the real configured value can be threaded in without this crate
/// depending on `tack-api`'s config type.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Rows processed per sweep transaction — see
/// `tack_db::Repository::rollup_and_purge_orch_events`'s doc comment for why
/// this is bounded rather than one transaction for the whole backlog.
pub const RETENTION_BATCH_SIZE: i64 = 500;

/// Outcome of rolling up and purging one table's stale rows. Mirrors
/// `tack_db::repo::orch::RollupStats` field-for-field so a [`RetentionStore`]
/// impl backed by the real repo layer is a direct pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollupOutcome {
    pub rows_purged: i64,
    pub batches_run: i64,
}

/// The narrow persistence interface the retention sweep needs. Deliberately
/// separate from [`ControlPlaneStore`]: retention operates fleet-wide across
/// `orch_events`/`orch_metrics`, independent of which planes are currently
/// registered, and needs none of `ControlPlaneStore`'s
/// adapter-construction/per-plane machinery.
#[async_trait::async_trait]
pub trait RetentionStore: Send + Sync {
    /// Roll every `orch_events` row older than `cutoff` into `orch_events_daily`
    /// and delete the raw rows, batched. See
    /// `tack_db::Repository::rollup_and_purge_orch_events`'s doc comment for
    /// the atomicity argument this method's implementors must preserve: the
    /// aggregate write and the delete for a given batch must commit together,
    /// not as two independently-committed steps.
    async fn rollup_and_purge_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError>;

    /// Same contract as [`Self::rollup_and_purge_events`], for `orch_metrics` /
    /// `orch_metrics_daily`.
    async fn rollup_and_purge_metrics(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError>;
}

/// Spawn the retention sweep, or don't — the same off-by-default contract as
/// [`spawn_reconcilers`] (TODO.md §0 rule 8): `enabled = false` returns `None`
/// immediately without calling `store` at all.
///
/// **Not yet wired into `server.rs`.** That file isn't in this card's file
/// ownership (TODO.md §2 lists it as A2's, and this card's own scope note
/// says "own only your files... if you need a change in a file you don't
/// own, write it in your handoff note — do not edit it") — see the TODO.md
/// §6 B3 handoff for the exact one-block addition needed to call this at
/// boot, mirroring the existing reconciler spawn block.
///
/// Runs both tables' sweeps back-to-back on one ticker, `sweep_interval_secs`
/// apart, starting immediately (no initial delay, same as
/// [`spawn_one`]'s poll loop). A failure in either sweep is logged and
/// retried next cycle — it never panics the task or stops the ticker.
pub fn spawn_retention_sweep(
    enabled: bool,
    store: Arc<dyn RetentionStore>,
    retention_days: u32,
    sweep_interval_secs: u64,
) -> Option<tokio::task::JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = sweep_interval_secs.max(1);
        loop {
            let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

            match store
                .rollup_and_purge_events(cutoff, RETENTION_BATCH_SIZE)
                .await
            {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "orch_events retention sweep rolled up and purged stale rows"
                ),
                Ok(_) => debug!("orch_events retention sweep: nothing stale to purge"),
                Err(e) => {
                    warn!(error = %e, "orch_events retention sweep failed; will retry next cycle")
                }
            }

            match store
                .rollup_and_purge_metrics(cutoff, RETENTION_BATCH_SIZE)
                .await
            {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "orch_metrics retention sweep rolled up and purged stale rows"
                ),
                Ok(_) => debug!("orch_metrics retention sweep: nothing stale to purge"),
                Err(e) => {
                    warn!(error = %e, "orch_metrics retention sweep failed; will retry next cycle")
                }
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    }))
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

/// Reconciler configuration. `poll_secs` is the base interval before backoff
/// and jitter are applied. `event_retention_days` (card B2) must match
/// `TACK_ORCH_EVENT_RETENTION_DAYS` — it feeds [`persist_events`]'s
/// retention-composition guard, which needs the *same* cutoff
/// [`spawn_retention_sweep`] uses, not an independently-configured one (a
/// mismatch here would either resurrect purged rows or drop events the
/// sweep hasn't purged yet).
#[derive(Debug, Clone, Copy)]
pub struct ReconcilerConfig {
    pub poll_secs: u64,
    pub event_retention_days: u32,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            poll_secs: 10,
            event_retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

/// Spawn one reconciler task per registered control plane, or none at all.
///
/// This is the single gate the "off by default" non-negotiable (TODO.md §0
/// rule 8 / §4 cross-cutting acceptance) depends on: when `enabled` is
/// `false`, this returns immediately with an empty `Vec` **without calling
/// `store.list_registered()` at all** — not just "spawns nothing", but "does
/// not even query for what it would have spawned". See the unit test
/// `disabled_orchestration_spawns_no_tasks_and_never_queries_the_store` for
/// the assertion.
///
/// `enabled` is read from `TACK_ORCH_ENABLE` by the caller (`tack-api`'s
/// `server.rs`) — this function takes a plain `bool` rather than reading the
/// environment itself so it stays testable without env-var mutation.
pub async fn spawn_reconcilers(
    enabled: bool,
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
) -> Vec<tokio::task::JoinHandle<()>> {
    if !enabled {
        return Vec::new();
    }

    let planes = match store.list_registered().await {
        Ok(planes) => planes,
        Err(e) => {
            warn!(error = %e, "failed to list registered control planes; orchestration reconciler not started");
            return Vec::new();
        }
    };

    planes
        .into_iter()
        .map(|plane| spawn_one(plane, Arc::clone(&store), config))
        .collect()
}

/// One `tokio` task per plane, looping: fetch → decide → persist → sleep.
/// See the module doc for the panic-isolation and phase-separation
/// rationale.
fn spawn_one(
    plane: RegisteredPlane,
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let RegisteredPlane { id, control_plane } = plane;
        let mut tracker = HealthTracker::new();
        let mut tick: u64 = 0;

        loop {
            tick += 1;

            // Which projects to poll /runs?project= for this tick. A single
            // short DB read, not held open across any HTTP .await below —
            // see the module doc's note on why this lives here rather than
            // inside the panic-isolated fetch phase. A failure here just
            // means this tick mirrors no runs (not an error, not a health
            // signal); it never blocks health/status/approvals polling.
            let projects = match store.list_linked_projects(id).await {
                Ok(projects) => projects,
                Err(e) => {
                    warn!(
                        control_plane_id = %id,
                        error = %e,
                        "failed to list linked projects; skipping run ingestion this tick"
                    );
                    Vec::new()
                }
            };

            // Which cursor to start each linked project's /traces poll from
            // this tick (card B2). Same pattern and same accepted staleness
            // window as the `projects` read just above — a single short DB
            // read, not held open across any HTTP .await below. A failure
            // here just means every project starts this tick's traces poll
            // fresh (`since: None`, i.e. "from the beginning") rather than
            // resuming — safe (content-derived event ids make re-ingestion
            // idempotent) if wasteful, and never a health signal.
            let trace_cursors = match store.list_trace_cursors(id).await {
                Ok(cursors) => cursors,
                Err(e) => {
                    warn!(
                        control_plane_id = %id,
                        error = %e,
                        "failed to list trace cursors; polling traces from the beginning this tick"
                    );
                    HashMap::new()
                }
            };

            // Fetch phase, isolated: a panic anywhere inside reconcile_once
            // (this poll or a future poll_* a Wave-2 card adds) surfaces as
            // a JoinError here rather than unwinding this loop — this task
            // keeps ticking, and no other plane's task is affected either.
            let cp = Arc::clone(&control_plane);
            let poll_result =
                tokio::spawn(async move { reconcile_once(&cp, &projects, &trace_cursors).await })
                    .await;

            let now = Utc::now();
            let (evaluation, fetched) = match poll_result {
                Ok((eval, fetched)) => (eval, fetched),
                Err(join_err) => {
                    error!(
                        control_plane_id = %id,
                        error = %join_err,
                        "control-plane poll panicked; treating this tick as a failed poll"
                    );
                    let panic_err = || OrchError::Unavailable("poll task panicked".to_string());
                    (
                        PollEvaluation {
                            reachable: false,
                            version_mismatch: false,
                            observed_api_version: None,
                            detail: format!("poll task panicked: {join_err}"),
                        },
                        FetchOutcome {
                            health: Err(panic_err()),
                            status: Err(panic_err()),
                            runs: Vec::new(),
                            approvals: Err(panic_err()),
                            metrics: Err(panic_err()),
                            traces: Vec::new(),
                        },
                    )
                }
            };

            // Decide phase: pure, synchronous.
            let transition =
                tracker.observe(evaluation.reachable, evaluation.version_mismatch, now);

            // Persist phase: one short call, strictly after the fetch above
            // has already completed.
            let record = HealthRecord {
                health: transition.state,
                consecutive_failures: transition.consecutive_failures,
                last_seen_at: transition.last_seen_at,
                api_version: evaluation.observed_api_version.clone(),
            };
            if let Err(e) = store.record_health(id, &record).await {
                warn!(control_plane_id = %id, error = %e, "failed to persist control-plane health");
            }

            // Persist phase (card B1): runs/approvals ingestion. Strictly
            // after record_health, strictly after the fetch phase above has
            // already completed — no HTTP call is in flight during either
            // of these. Failures here are logged and skipped, never
            // propagated into the health verdict already decided above.
            persist_runs(store.as_ref(), id, &fetched.runs).await;
            persist_approvals(store.as_ref(), id, &fetched.approvals).await;
            persist_metrics(store.as_ref(), id, &fetched.metrics).await;
            persist_events(
                store.as_ref(),
                id,
                &fetched.traces,
                config.event_retention_days,
            )
            .await;

            match transition.log {
                Some(LogSeverity::Warn) => warn!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    consecutive_failures = transition.consecutive_failures,
                    detail = %evaluation.detail,
                    "control plane health degraded"
                ),
                Some(LogSeverity::Info) => info!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    "control plane recovered"
                ),
                None => debug!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    consecutive_failures = transition.consecutive_failures,
                    "control plane poll (no state change)"
                ),
            }

            let base = if transition.consecutive_failures > 0 {
                backoff_secs(transition.consecutive_failures, config.poll_secs)
            } else {
                config.poll_secs
            };
            let sleep_for = jittered_secs(&id, tick, base);
            tokio::time::sleep(Duration::from_secs(sleep_for)).await;
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApprovalState, NewRemoteTask, RemoteApproval, RemoteEvent, RemoteRun, RemoteTask,
        RunSource, RunState, TaskStatus,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // -- Health state machine (pure; no ControlPlane needed) --------------

    #[test]
    fn health_state_transitions_at_3_and_10_failures() {
        let mut t = HealthTracker::new();
        let now = Utc::now();

        // 1 and 2 failures: still healthy.
        assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
        assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
        // 3rd failure: degraded.
        let tr = t.observe(false, false, now);
        assert_eq!(tr.state, HealthState::Degraded);
        assert_eq!(tr.consecutive_failures, 3);

        // 4..=9 stay degraded.
        for _ in 4..=9 {
            assert_eq!(t.observe(false, false, now).state, HealthState::Degraded);
        }
        // 10th failure: unreachable.
        let tr = t.observe(false, false, now);
        assert_eq!(tr.state, HealthState::Unreachable);
        assert_eq!(tr.consecutive_failures, 10);
    }

    #[test]
    fn health_recovers_immediately_on_a_single_success() {
        let mut t = HealthTracker::new();
        let now = Utc::now();
        for _ in 0..15 {
            t.observe(false, false, now);
        }
        assert_eq!(t.state, HealthState::Unreachable);

        let tr = t.observe(true, false, now);
        assert_eq!(tr.state, HealthState::Healthy);
        assert_eq!(tr.consecutive_failures, 0);
        assert_eq!(tr.last_seen_at, Some(now));
    }

    #[test]
    fn last_seen_at_is_none_on_a_failed_poll_so_the_store_leaves_it_untouched() {
        let mut t = HealthTracker::new();
        let tr = t.observe(false, false, Utc::now());
        assert_eq!(tr.last_seen_at, None);
    }

    #[test]
    fn warn_logging_is_suppressed_during_a_sustained_outage() {
        // This is the exact suppression logic spawn_one's loop uses to
        // decide whether to `tracing::warn!` — a real sustained outage
        // (docket down for an hour, say) must not spam the log at every
        // tick. Across a long failure streak, `warn` should only fire on
        // the two severity transitions (entering degraded, entering
        // unreachable), never on every one of the (here) 30 failed polls.
        let mut t = HealthTracker::new();
        let now = Utc::now();
        let mut warns = 0;
        for _ in 0..30 {
            if t.observe(false, false, now).log == Some(LogSeverity::Warn) {
                warns += 1;
            }
        }
        assert_eq!(
            warns, 2,
            "expected exactly 2 warns (healthy->degraded, degraded->unreachable) across a 30-failure streak"
        );

        // Recovery logs once at `info`, not `warn`.
        let tr = t.observe(true, false, now);
        assert_eq!(tr.log, Some(LogSeverity::Info));
    }

    #[test]
    fn backoff_is_capped_at_five_minutes() {
        assert_eq!(backoff_secs(100, 10), MAX_BACKOFF_SECS);
        assert_eq!(backoff_secs(1_000_000, 10), MAX_BACKOFF_SECS);
    }

    #[test]
    fn backoff_grows_with_consecutive_failures_and_resets_when_healthy() {
        let a = backoff_secs(1, 10);
        let b = backoff_secs(2, 10);
        let c = backoff_secs(3, 10);
        assert!(
            a < b && b < c,
            "backoff should strictly increase: {a} < {b} < {c}"
        );
        assert_eq!(backoff_secs(0, 10), 10, "no backoff while healthy");
    }

    #[test]
    fn jitter_stays_within_20_percent_of_base() {
        let id = Uuid::new_v4();
        for tick in 0..200 {
            let j = jittered_secs(&id, tick, 100);
            assert!(
                (80..=120).contains(&j),
                "tick {tick} produced {j}, expected within [80, 120] for base 100"
            );
        }
    }

    #[test]
    fn jitter_varies_so_planes_do_not_stampede_in_lockstep() {
        // Different plane ids polled on the same tick should not all land
        // on the exact same jittered interval.
        let base = 100;
        let values: std::collections::HashSet<u64> = (0..20)
            .map(|_| jittered_secs(&Uuid::new_v4(), 1, base))
            .collect();
        assert!(
            values.len() > 1,
            "expected jitter to differ across plane ids on the same tick, got a single value {values:?}"
        );
    }

    #[test]
    fn jitter_never_produces_a_zero_or_negative_sleep() {
        let id = Uuid::new_v4();
        for tick in 0..50 {
            assert!(jittered_secs(&id, tick, 1) >= 1);
        }
    }

    // -- apiVersion policy --------------------------------------------------

    fn sample_status(api_version: &str) -> FleetStatus {
        FleetStatus {
            api_version: api_version.to_string(),
            timestamp: "2026-08-04T00:00:00Z".to_string(),
            gateway: "active".to_string(),
            channels: vec![],
            agents: vec![],
            total_cost_usd_estimated: 0.0,
        }
    }

    #[test]
    fn evaluate_is_reachable_and_matched_when_health_and_status_succeed() {
        let outcome = FetchOutcome {
            health: Ok(Health {
                status: "ok".into(),
                gateway: 1,
            }),
            status: Ok(sample_status(EXPECTED_API_VERSION)),
            runs: Vec::new(),
            approvals: Ok(Vec::new()),
            metrics: Ok(Vec::new()),
            traces: Vec::new(),
        };
        let eval = evaluate(&outcome);
        assert!(eval.reachable);
        assert!(!eval.version_mismatch);
        assert_eq!(
            eval.observed_api_version.as_deref(),
            Some(EXPECTED_API_VERSION)
        );
    }

    #[test]
    fn evaluate_is_unreachable_when_health_call_fails() {
        let outcome = FetchOutcome {
            health: Err(OrchError::Unavailable("connection refused".into())),
            status: Ok(sample_status(EXPECTED_API_VERSION)),
            runs: Vec::new(),
            approvals: Ok(Vec::new()),
            metrics: Ok(Vec::new()),
            traces: Vec::new(),
        };
        assert!(!evaluate(&outcome).reachable);
    }

    #[test]
    fn evaluate_is_unreachable_when_status_call_fails() {
        let outcome = FetchOutcome {
            health: Ok(Health {
                status: "ok".into(),
                gateway: 1,
            }),
            status: Err(OrchError::Decode("malformed json".into())),
            runs: Vec::new(),
            approvals: Ok(Vec::new()),
            metrics: Ok(Vec::new()),
            traces: Vec::new(),
        };
        let eval = evaluate(&outcome);
        assert!(!eval.reachable);
        assert!(
            !eval.version_mismatch,
            "an unparseable status has no apiVersion to compare"
        );
    }

    #[test]
    fn evaluate_flags_a_major_api_version_mismatch_but_stays_reachable() {
        let outcome = FetchOutcome {
            health: Ok(Health {
                status: "ok".into(),
                gateway: 1,
            }),
            status: Ok(sample_status("3")),
            runs: Vec::new(),
            approvals: Ok(Vec::new()),
            metrics: Ok(Vec::new()),
            traces: Vec::new(),
        };
        let eval = evaluate(&outcome);
        assert!(eval.reachable, "the HTTP calls themselves succeeded");
        assert!(eval.version_mismatch);
        assert_eq!(eval.observed_api_version.as_deref(), Some("3"));
        assert!(eval.detail.contains("apiVersion mismatch"));
    }

    #[test]
    fn evaluate_ignores_a_minor_version_difference() {
        // "2.1" vs expected "2" (or a future "2.0"): same major, not a
        // mismatch — see major_version's doc comment.
        let outcome = FetchOutcome {
            health: Ok(Health {
                status: "ok".into(),
                gateway: 1,
            }),
            status: Ok(sample_status("2.1")),
            runs: Vec::new(),
            approvals: Ok(Vec::new()),
            metrics: Ok(Vec::new()),
            traces: Vec::new(),
        };
        assert!(!evaluate(&outcome).version_mismatch);
    }

    #[test]
    fn version_mismatch_forces_at_least_degraded_even_while_reachability_is_healthy() {
        let mut t = HealthTracker::new();
        let tr = t.observe(true, true, Utc::now());
        assert_eq!(tr.state, HealthState::Degraded);
        assert_eq!(tr.log, Some(LogSeverity::Warn));
    }

    #[test]
    fn version_mismatch_does_not_downgrade_an_already_unreachable_plane() {
        let mut t = HealthTracker::new();
        let now = Utc::now();
        for _ in 0..10 {
            t.observe(false, false, now);
        }
        assert_eq!(t.state, HealthState::Unreachable);
        // Reachable again but with a version mismatch on this same tick:
        // reachability resets failures to 0 (healthy floor), but the
        // mismatch keeps it at degraded, not unreachable and not healthy.
        let tr = t.observe(true, true, now);
        assert_eq!(tr.state, HealthState::Degraded);
    }

    // -- Fake ControlPlane, for reconcile_once / spawn tests ---------------

    /// A `ControlPlane` whose `health`/`status`/`list_runs`/`list_approvals`
    /// responses are scripted; every other method (unused this wave)
    /// returns `Disabled`. Used to drive `reconcile_once`/`spawn_reconcilers`
    /// without a real adapter.
    struct FakeControlPlane {
        health_calls: AtomicUsize,
        healthy: bool,
        panic_on_health: bool,
        runs: Vec<RemoteRun>,
        approvals: Vec<RemoteApproval>,
        approvals_should_fail: bool,
        metrics: Vec<MetricSample>,
        metrics_should_fail: bool,
        /// Keyed by the `project`/`since` pair `traces()` was called with —
        /// lets a single test script different responses per project without
        /// needing per-project fakes.
        traces: std::collections::HashMap<(String, Option<String>), Vec<RemoteEvent>>,
        traces_should_fail: bool,
    }

    impl FakeControlPlane {
        fn healthy() -> Self {
            Self {
                health_calls: AtomicUsize::new(0),
                healthy: true,
                panic_on_health: false,
                runs: Vec::new(),
                approvals: Vec::new(),
                approvals_should_fail: false,
                metrics: Vec::new(),
                metrics_should_fail: false,
                traces: std::collections::HashMap::new(),
                traces_should_fail: false,
            }
        }

        fn panics_on_health() -> Self {
            Self {
                panic_on_health: true,
                ..Self::healthy()
            }
        }

        fn with_runs(runs: Vec<RemoteRun>) -> Self {
            Self {
                runs,
                ..Self::healthy()
            }
        }

        fn with_approvals(approvals: Vec<RemoteApproval>) -> Self {
            Self {
                approvals,
                ..Self::healthy()
            }
        }

        fn healthy_with_failing_approvals() -> Self {
            Self {
                approvals_should_fail: true,
                ..Self::healthy()
            }
        }

        fn with_metrics(metrics: Vec<MetricSample>) -> Self {
            Self {
                metrics,
                ..Self::healthy()
            }
        }

        fn healthy_with_failing_metrics() -> Self {
            Self {
                metrics_should_fail: true,
                ..Self::healthy()
            }
        }

        /// `traces()` for `(project, since)` returns `events`. Call multiple
        /// times to script more than one project/cursor combination.
        fn with_traces(
            mut self,
            project: &str,
            since: Option<&str>,
            events: Vec<RemoteEvent>,
        ) -> Self {
            self.traces
                .insert((project.to_string(), since.map(str::to_string)), events);
            self
        }

        fn healthy_with_failing_traces() -> Self {
            Self {
                traces_should_fail: true,
                ..Self::healthy()
            }
        }
    }

    #[async_trait::async_trait]
    impl ControlPlane for FakeControlPlane {
        fn kind(&self) -> &'static str {
            "fake"
        }

        async fn health(&self) -> Result<Health, OrchError> {
            self.health_calls.fetch_add(1, Ordering::SeqCst);
            if self.panic_on_health {
                panic!("simulated adapter bug");
            }
            if self.healthy {
                Ok(Health {
                    status: "ok".into(),
                    gateway: 1,
                })
            } else {
                Err(OrchError::Unavailable("connection refused".into()))
            }
        }

        async fn status(&self) -> Result<FleetStatus, OrchError> {
            Ok(sample_status(EXPECTED_API_VERSION))
        }

        async fn metrics(&self) -> Result<Vec<crate::MetricSample>, OrchError> {
            if self.metrics_should_fail {
                Err(OrchError::Unavailable("metrics endpoint down".into()))
            } else {
                Ok(self.metrics.clone())
            }
        }

        async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
            Ok(self.runs.clone())
        }

        async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
            Err(OrchError::Disabled)
        }

        async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
            if self.approvals_should_fail {
                Err(OrchError::Unavailable("approvals endpoint down".into()))
            } else {
                Ok(self.approvals.clone())
            }
        }

        async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
            Err(OrchError::Disabled)
        }

        async fn traces(
            &self,
            project: &str,
            since: Option<&str>,
        ) -> Result<Vec<RemoteEvent>, OrchError> {
            if self.traces_should_fail {
                return Err(OrchError::Unavailable("traces endpoint down".into()));
            }
            let key = (project.to_string(), since.map(str::to_string));
            Ok(self.traces.get(&key).cloned().unwrap_or_default())
        }

        async fn enqueue_task(
            &self,
            _project: &str,
            _task: NewRemoteTask,
        ) -> Result<String, OrchError> {
            Err(OrchError::Disabled)
        }

        async fn dispatch(
            &self,
            _project: &str,
            _vars: serde_json::Value,
        ) -> Result<String, OrchError> {
            Err(OrchError::Disabled)
        }

        async fn decide_approval(&self, _token: &str, _grant: bool) -> Result<(), OrchError> {
            Err(OrchError::Disabled)
        }
    }

    // Silence "unused" on ApprovalState/RunState/TaskStatus imports, which
    // exist only so this module compiles against the exact same import set
    // future poll_* fns (Wave 2) will need; keep them imported here as a
    // living example rather than trimming and re-adding later.
    #[allow(dead_code)]
    fn _uses_remote_enums(_a: ApprovalState, _b: RunState, _c: TaskStatus) {}

    #[tokio::test]
    async fn panic_in_a_poll_is_isolated_by_the_task_boundary() {
        let cp: Arc<dyn ControlPlane> = Arc::new(FakeControlPlane::panics_on_health());
        // This is exactly the isolation spawn_one relies on: reconcile_once
        // runs inside its own tokio::spawn, so a panic inside it surfaces as
        // a JoinError to the caller instead of unwinding the caller's stack.
        let result =
            tokio::spawn(async move { reconcile_once(&cp, &[], &HashMap::new()).await }).await;
        assert!(
            result.is_err(),
            "the panic must surface as a JoinError, not propagate"
        );
    }

    // -- Persistence-side fake, for spawn_reconcilers tests -----------------

    struct FakeStore {
        planes: Vec<RegisteredPlane>,
        list_called: AtomicBool,
        health_records: Mutex<Vec<(Uuid, HealthRecord)>>,
        /// `orch_links.remote_project` stand-in — what `list_linked_projects`
        /// returns for every plane (this fake doesn't scope by
        /// `control_plane_id`; one plane per test is enough for these tests).
        linked_projects: Vec<String>,
        /// `orch_tasks.remote_task_id -> item_id` stand-in for
        /// `find_item_for_remote_task`.
        known_tasks: std::collections::HashMap<String, Uuid>,
        upserted_runs: Mutex<Vec<(Uuid, Vec<NewOrchRun>)>>,
        upserted_approvals: Mutex<Vec<(Uuid, Vec<NewOrchApproval>)>>,
        upserted_metrics: Mutex<Vec<(Uuid, Vec<NewOrchMetric>)>>,
        /// `orch_trace_cursors` stand-in, seeded via [`Self::with_trace_cursor`]
        /// and mutated by `set_trace_cursor` — a `Mutex` (not the read-only
        /// `linked_projects`/`known_tasks` shape) because the reconciler
        /// itself writes to it every tick.
        trace_cursors: Mutex<std::collections::HashMap<String, String>>,
        upserted_events: Mutex<Vec<(Uuid, Vec<NewOrchEvent>)>>,
    }

    impl FakeStore {
        fn new(planes: Vec<RegisteredPlane>) -> Self {
            Self {
                planes,
                list_called: AtomicBool::new(false),
                health_records: Mutex::new(Vec::new()),
                linked_projects: Vec::new(),
                known_tasks: std::collections::HashMap::new(),
                upserted_runs: Mutex::new(Vec::new()),
                upserted_approvals: Mutex::new(Vec::new()),
                upserted_metrics: Mutex::new(Vec::new()),
                trace_cursors: Mutex::new(std::collections::HashMap::new()),
                upserted_events: Mutex::new(Vec::new()),
            }
        }

        fn with_linked_projects(mut self, projects: Vec<String>) -> Self {
            self.linked_projects = projects;
            self
        }

        fn with_known_task(mut self, remote_task_id: &str, item_id: Uuid) -> Self {
            self.known_tasks.insert(remote_task_id.to_string(), item_id);
            self
        }

        fn with_trace_cursor(self, remote_project: &str, cursor: &str) -> Self {
            self.trace_cursors
                .lock()
                .unwrap()
                .insert(remote_project.to_string(), cursor.to_string());
            self
        }
    }

    #[async_trait::async_trait]
    impl ControlPlaneStore for FakeStore {
        async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
            self.list_called.store(true, Ordering::SeqCst);
            Ok(self.planes.clone())
        }

        async fn record_health(
            &self,
            control_plane_id: Uuid,
            record: &HealthRecord,
        ) -> Result<(), OrchError> {
            self.health_records
                .lock()
                .unwrap()
                .push((control_plane_id, record.clone()));
            Ok(())
        }

        async fn list_linked_projects(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<Vec<String>, OrchError> {
            Ok(self.linked_projects.clone())
        }

        async fn find_item_for_remote_task(
            &self,
            remote_task_id: &str,
        ) -> Result<Option<Uuid>, OrchError> {
            Ok(self.known_tasks.get(remote_task_id).copied())
        }

        async fn upsert_runs(
            &self,
            control_plane_id: Uuid,
            runs: &[NewOrchRun],
        ) -> Result<(), OrchError> {
            self.upserted_runs
                .lock()
                .unwrap()
                .push((control_plane_id, runs.to_vec()));
            Ok(())
        }

        async fn upsert_approvals(
            &self,
            control_plane_id: Uuid,
            approvals: &[NewOrchApproval],
        ) -> Result<(), OrchError> {
            self.upserted_approvals
                .lock()
                .unwrap()
                .push((control_plane_id, approvals.to_vec()));
            Ok(())
        }

        async fn upsert_metrics(
            &self,
            control_plane_id: Uuid,
            metrics: &[NewOrchMetric],
        ) -> Result<(), OrchError> {
            self.upserted_metrics
                .lock()
                .unwrap()
                .push((control_plane_id, metrics.to_vec()));
            Ok(())
        }

        async fn list_trace_cursors(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<HashMap<String, String>, OrchError> {
            Ok(self.trace_cursors.lock().unwrap().clone())
        }

        async fn set_trace_cursor(
            &self,
            _control_plane_id: Uuid,
            remote_project: &str,
            cursor: &str,
        ) -> Result<(), OrchError> {
            self.trace_cursors
                .lock()
                .unwrap()
                .insert(remote_project.to_string(), cursor.to_string());
            Ok(())
        }

        async fn upsert_events(
            &self,
            control_plane_id: Uuid,
            events: &[NewOrchEvent],
        ) -> Result<(), OrchError> {
            self.upserted_events
                .lock()
                .unwrap()
                .push((control_plane_id, events.to_vec()));
            Ok(())
        }
    }

    fn healthy_plane(id: Uuid) -> RegisteredPlane {
        RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy()),
        }
    }

    #[tokio::test]
    async fn disabled_orchestration_spawns_no_tasks_and_never_queries_the_store() {
        let store = Arc::new(FakeStore::new(vec![healthy_plane(Uuid::new_v4())]));
        let handles = spawn_reconcilers(false, store.clone(), ReconcilerConfig::default()).await;
        assert!(handles.is_empty());
        assert!(
            !store.list_called.load(Ordering::SeqCst),
            "a disabled reconciler must not even query for registered planes"
        );
    }

    #[tokio::test]
    async fn enabled_orchestration_spawns_one_task_per_registered_plane() {
        let store = Arc::new(FakeStore::new(vec![
            healthy_plane(Uuid::new_v4()),
            healthy_plane(Uuid::new_v4()),
            healthy_plane(Uuid::new_v4()),
        ]));
        let handles = spawn_reconcilers(
            true,
            store.clone(),
            ReconcilerConfig {
                poll_secs: 1,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(handles.len(), 3);
        for h in handles {
            h.abort();
        }
    }

    #[tokio::test]
    async fn a_running_plane_task_persists_health_after_its_first_tick() {
        let id = Uuid::new_v4();
        let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
        let handles = spawn_reconcilers(
            true,
            store.clone(),
            ReconcilerConfig {
                poll_secs: 1,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(handles.len(), 1);

        // The loop polls immediately on start (no artificial initial
        // delay — deliberately different from the due-soon/backup
        // schedulers, see the A2 handoff note), so a short real-time wait
        // is enough to observe the first persisted record.
        tokio::time::sleep(Duration::from_millis(300)).await;
        for h in handles {
            h.abort();
        }

        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
        );
    }

    #[tokio::test]
    async fn store_error_listing_planes_yields_no_tasks_not_a_panic() {
        struct FailingStore;
        #[async_trait::async_trait]
        impl ControlPlaneStore for FailingStore {
            async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
                Err(OrchError::Unavailable("db unreachable".into()))
            }
            async fn record_health(
                &self,
                _id: Uuid,
                _record: &HealthRecord,
            ) -> Result<(), OrchError> {
                Ok(())
            }
            async fn list_linked_projects(
                &self,
                _control_plane_id: Uuid,
            ) -> Result<Vec<String>, OrchError> {
                Ok(Vec::new())
            }
            async fn find_item_for_remote_task(
                &self,
                _remote_task_id: &str,
            ) -> Result<Option<Uuid>, OrchError> {
                Ok(None)
            }
            async fn upsert_runs(
                &self,
                _control_plane_id: Uuid,
                _runs: &[NewOrchRun],
            ) -> Result<(), OrchError> {
                Ok(())
            }
            async fn upsert_approvals(
                &self,
                _control_plane_id: Uuid,
                _approvals: &[NewOrchApproval],
            ) -> Result<(), OrchError> {
                Ok(())
            }
            async fn upsert_metrics(
                &self,
                _control_plane_id: Uuid,
                _metrics: &[NewOrchMetric],
            ) -> Result<(), OrchError> {
                Ok(())
            }
            async fn list_trace_cursors(
                &self,
                _control_plane_id: Uuid,
            ) -> Result<HashMap<String, String>, OrchError> {
                Ok(HashMap::new())
            }
            async fn set_trace_cursor(
                &self,
                _control_plane_id: Uuid,
                _remote_project: &str,
                _cursor: &str,
            ) -> Result<(), OrchError> {
                Ok(())
            }
            async fn upsert_events(
                &self,
                _control_plane_id: Uuid,
                _events: &[NewOrchEvent],
            ) -> Result<(), OrchError> {
                Ok(())
            }
        }

        let handles =
            spawn_reconcilers(true, Arc::new(FailingStore), ReconcilerConfig::default()).await;
        assert!(handles.is_empty());
    }

    // -- Card B1: runs + approvals ingestion --------------------------------

    fn sample_run(id: &str, project: &str, task_ids: Vec<String>) -> RemoteRun {
        RemoteRun {
            id: id.to_string(),
            source: RunSource::Cli,
            project: project.to_string(),
            state: RunState::Succeeded,
            task_ids,
            error: String::new(),
            created: "2026-08-04T19:50:43.129083+00:00".to_string(),
            started_at: Some("2026-08-04T19:50:43.129674+00:00".to_string()),
            finished_at: Some("2026-08-04T19:50:43.130194+00:00".to_string()),
            pids: Vec::new(),
            variables: serde_json::json!({}),
        }
    }

    fn sample_approval(token: &str, context: serde_json::Value) -> RemoteApproval {
        RemoteApproval {
            token: token.to_string(),
            project: "demo".to_string(),
            role: "implementer".to_string(),
            action: "pod dispatch — task enqueue".to_string(),
            state: ApprovalState::Pending,
            created: "2026-08-04T19:50:50Z".to_string(),
            context,
        }
    }

    /// Runs `spawn_one`'s loop (via `spawn_reconcilers`) for one tick against
    /// a `FakeStore` (whose plane list is set via [`FakeStore::new`]), then
    /// aborts the task and returns the store for assertions. Every B1 test
    /// below follows this same "one tick, then inspect what got upserted"
    /// shape.
    async fn run_one_tick(store: Arc<FakeStore>) -> Arc<FakeStore> {
        let store_dyn: Arc<dyn ControlPlaneStore> = store.clone();
        let handles = spawn_reconcilers(
            true,
            store_dyn,
            ReconcilerConfig {
                poll_secs: 60,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(handles.len(), 1, "expected exactly one plane registered");
        tokio::time::sleep(Duration::from_millis(300)).await;
        for h in handles {
            h.abort();
        }
        store
    }

    #[tokio::test]
    async fn approvals_poll_failure_leaves_plane_health_untouched() {
        let id = Uuid::new_v4();
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy_with_failing_approvals()),
        };
        let store = Arc::new(FakeStore::new(vec![plane]));
        let store = run_one_tick(store).await;

        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
            "a /approvals failure must not degrade plane health: {records:?}"
        );
    }

    #[tokio::test]
    async fn a_correlated_run_lands_with_the_right_item_id() {
        let id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let run = sample_run("run-1", "demo", vec!["task-1".to_string()]);
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
        };
        let store = Arc::new(
            FakeStore::new(vec![plane])
                .with_linked_projects(vec!["demo".to_string()])
                .with_known_task("task-1", item_id),
        );
        let store = run_one_tick(store).await;

        let upserted = store.upserted_runs.lock().unwrap();
        let (_, runs) = upserted
            .iter()
            .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
            .expect("expected at least one upserted run");
        let run = runs.iter().find(|r| r.run_id == "run-1").expect("run-1");
        assert_eq!(run.item_id, Some(item_id));
        assert_eq!(run.remote_project, "demo");
        assert_eq!(run.source, "cli");
        assert_eq!(run.state, "succeeded");
    }

    #[tokio::test]
    async fn an_uncorrelated_run_lands_with_item_id_none_and_does_not_error() {
        let id = Uuid::new_v4();
        // Empty task_ids: the normal shape of a run dispatched from
        // docket's own CLI, not through Tack (card B1's "must not error"
        // requirement).
        let run = sample_run("run-cli-only", "demo", vec![]);
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_runs.lock().unwrap();
        let (_, runs) = upserted
            .iter()
            .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
            .expect("expected the CLI-dispatched run to still be mirrored");
        let run = runs
            .iter()
            .find(|r| r.run_id == "run-cli-only")
            .expect("run-cli-only");
        assert_eq!(run.item_id, None);

        // And plane health must be entirely unaffected by this.
        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
        );
    }

    #[tokio::test]
    async fn a_correlated_approval_lands_with_the_right_item_id() {
        let id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let approval = sample_approval(
            "apr-1",
            serde_json::json!({"taskId": "task-1", "pipelineIndex": 2}),
        );
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
        };
        let store = Arc::new(FakeStore::new(vec![plane]).with_known_task("task-1", item_id));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_approvals.lock().unwrap();
        let (_, approvals) = upserted
            .iter()
            .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
            .expect("expected at least one upserted approval");
        let approval = approvals
            .iter()
            .find(|a| a.token == "apr-1")
            .expect("apr-1");
        assert_eq!(approval.item_id, Some(item_id));
        assert_eq!(approval.remote_task_id.as_deref(), Some("task-1"));
        assert_eq!(approval.agent.as_deref(), Some("implementer"));
        assert_eq!(approval.state, "pending");
    }

    #[tokio::test]
    async fn an_uncorrelated_approval_lands_with_item_id_none_and_still_surfaces() {
        let id = Uuid::new_v4();
        // No "taskId" in context at all — an approval Tack cannot attribute
        // to any item. Per card B1 this must still persist (item_id: NULL),
        // not be dropped, since it's exactly the kind of approval most
        // likely to silently block a fleet.
        let approval = sample_approval("apr-uncorrelated", serde_json::json!({}));
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
        };
        let store = Arc::new(FakeStore::new(vec![plane]));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_approvals.lock().unwrap();
        let (_, approvals) = upserted
            .iter()
            .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
            .expect("expected the uncorrelated approval to still be mirrored");
        let approval = approvals
            .iter()
            .find(|a| a.token == "apr-uncorrelated")
            .expect("apr-uncorrelated");
        assert_eq!(approval.item_id, None);
        assert_eq!(approval.remote_task_id, None);
    }

    #[test]
    fn extract_task_id_handles_missing_and_non_string_taskid() {
        assert_eq!(
            extract_task_id(&serde_json::json!({"taskId": "task-1", "pipelineIndex": 0})),
            Some("task-1".to_string())
        );
        assert_eq!(extract_task_id(&serde_json::json!({})), None);
        assert_eq!(
            extract_task_id(&serde_json::json!({"taskId": 42})),
            None,
            "a non-string taskId is treated as uncorrelated, not a parse error"
        );
        assert_eq!(extract_task_id(&serde_json::json!(null)), None);
    }

    #[test]
    fn parse_optional_rfc3339_accepts_both_docket_timestamp_conventions() {
        // core/runs.py's `+00:00` offset form.
        assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:43.129083+00:00")).is_some());
        // core/approval.py's `Z` form.
        assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:50Z")).is_some());
        // Malformed input degrades to None rather than panicking/erroring.
        assert_eq!(parse_optional_rfc3339(Some("not-a-timestamp")), None);
        assert_eq!(parse_optional_rfc3339(None), None);
    }

    // -- Card B3: metrics ingestion -----------------------------------------

    fn sample_metric(name: &str, value: f64) -> MetricSample {
        let mut labels = std::collections::BTreeMap::new();
        labels.insert("agent".to_string(), "demo-lead".to_string());
        MetricSample {
            name: name.to_string(),
            labels,
            value,
        }
    }

    #[tokio::test]
    async fn metrics_land_via_upsert_metrics_on_a_successful_poll() {
        let id = Uuid::new_v4();
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::with_metrics(vec![
                sample_metric("docket_agents_total", 3.0),
                sample_metric("docket_agent_cost_usd", 1.5),
            ])),
        };
        let store = Arc::new(FakeStore::new(vec![plane]));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_metrics.lock().unwrap();
        let (_, metrics) = upserted
            .iter()
            .find(|(cp_id, metrics)| *cp_id == id && !metrics.is_empty())
            .expect("expected at least one upserted metric batch");
        assert_eq!(metrics.len(), 2);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "docket_agents_total" && m.value == 3.0)
        );
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "docket_agent_cost_usd" && m.value == 1.5)
        );
    }

    #[tokio::test]
    async fn metrics_poll_failure_leaves_plane_health_untouched_and_persists_nothing() {
        let id = Uuid::new_v4();
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy_with_failing_metrics()),
        };
        let store = Arc::new(FakeStore::new(vec![plane]));
        let store = run_one_tick(store).await;

        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
            "a /metrics failure must not degrade plane health: {records:?}"
        );

        let upserted = store.upserted_metrics.lock().unwrap();
        assert!(
            upserted.iter().all(|(_, m)| m.is_empty()),
            "a failed metrics poll must not persist anything: {upserted:?}"
        );
    }

    // -- Card B2: trace ingestion ---------------------------------------------

    fn sample_event(session_id: &str, ts: &str, event_type: &str) -> RemoteEvent {
        RemoteEvent {
            ts: ts.to_string(),
            project: "demo".to_string(),
            session_id: session_id.to_string(),
            agent_role: "lead".to_string(),
            event_type: event_type.to_string(),
            payload: serde_json::json!({"tool": "bash", "command": "cargo test"}),
            cost_usd_estimated: Some(0.0021),
            duration_ms: Some(842),
        }
    }

    #[test]
    fn derive_event_id_is_deterministic_for_the_same_source_event() {
        let cp_id = Uuid::new_v4();
        let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
        let id1 = derive_event_id(cp_id, "demo", &event);
        let id2 = derive_event_id(cp_id, "demo", &event);
        assert_eq!(
            id1, id2,
            "the same source event must always derive the same id"
        );
    }

    #[test]
    fn derive_event_id_differs_when_any_field_differs() {
        let cp_id = Uuid::new_v4();
        let base = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
        let base_id = derive_event_id(cp_id, "demo", &base);

        let mut different_ts = base.clone();
        different_ts.ts = "2026-08-04T19:52:28Z".to_string();
        assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_ts));

        let mut different_payload = base.clone();
        different_payload.payload = serde_json::json!({"tool": "bash", "command": "cargo build"});
        assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_payload));

        assert_ne!(
            base_id,
            derive_event_id(cp_id, "other-project", &base),
            "the same event on a different remote_project must derive a different id"
        );
        assert_ne!(
            base_id,
            derive_event_id(Uuid::new_v4(), "demo", &base),
            "the same event on a different control plane must derive a different id"
        );
    }

    #[test]
    fn derive_event_id_ignores_field_boundaries_not_field_content() {
        // Naive delimiter-free concatenation would hash "a" + "bc" the same
        // as "ab" + "c" — the \u{1} separator in derive_event_id must
        // prevent that. Two events differing only in where a boundary falls
        // between session_id and agent_role must derive different ids.
        let cp_id = Uuid::new_v4();
        let mut a = sample_event("agent:demo:ab", "2026-08-04T19:52:27Z", "tool_call");
        a.agent_role = "c".to_string();
        let mut b = sample_event("agent:demo:a", "2026-08-04T19:52:27Z", "tool_call");
        b.agent_role = "bc".to_string();
        assert_ne!(
            derive_event_id(cp_id, "demo", &a),
            derive_event_id(cp_id, "demo", &b)
        );
    }

    #[test]
    fn decode_trace_cursor_parses_compound_and_bare_forms() {
        assert_eq!(decode_trace_cursor(""), ("", 0));
        assert_eq!(
            decode_trace_cursor("2026-08-04T19:52:27Z:3"),
            ("2026-08-04T19:52:27Z", 3)
        );
        // A bare, hand-typed ISO timestamp with no trailing Z:<n> — count
        // defaults to 0, and critically the *whole* input is treated as the
        // ts (not misparsed by splitting on one of the timestamp's own
        // colons).
        assert_eq!(
            decode_trace_cursor("2026-08-04T19:52:27"),
            ("2026-08-04T19:52:27", 0)
        );
        // Non-digit tail after the last colon: not a compound cursor.
        assert_eq!(
            decode_trace_cursor("2026-08-04T19:52:27Z:abc"),
            ("2026-08-04T19:52:27Z:abc", 0)
        );
    }

    #[test]
    fn next_trace_cursor_does_not_move_when_there_are_no_events() {
        assert_eq!(
            next_trace_cursor(Some("2026-08-04T19:52:27Z:1"), &[]),
            Some("2026-08-04T19:52:27Z:1".to_string())
        );
        assert_eq!(next_trace_cursor(None, &[]), None);
    }

    #[test]
    fn next_trace_cursor_does_not_move_when_no_event_has_a_usable_ts() {
        let event = sample_event("agent:demo:task-1", "", "tool_call");
        assert_eq!(
            next_trace_cursor(Some("2026-08-04T19:52:27Z:1"), &[event]),
            Some("2026-08-04T19:52:27Z:1".to_string())
        );
    }

    #[test]
    fn next_trace_cursor_counts_the_trailing_same_second_run() {
        let events = vec![
            sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
            sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "session_start"),
            sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "tool_result"),
        ];
        // Two events share the last (anchor) second; the first event's
        // different second doesn't extend that run.
        assert_eq!(
            next_trace_cursor(None, &events),
            Some("2026-08-04T19:52:40Z:2".to_string())
        );
    }

    #[test]
    fn next_trace_cursor_carries_the_previous_count_forward_across_the_same_second_boundary() {
        // The previous poll already delivered 2 events at :40Z (encoded in
        // `previous`); this poll's page starts mid-way through that same
        // second and delivers 1 more before rolling into a new second.
        let events = vec![
            sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "tool_result"),
            sample_event("agent:demo:task-1", "2026-08-04T19:52:41Z", "session_end"),
        ];
        assert_eq!(
            next_trace_cursor(Some("2026-08-04T19:52:40Z:2"), &events),
            Some("2026-08-04T19:52:41Z:1".to_string()),
            "the anchor second changed, so the run resets — the :2 from the \
             previous poll must not leak into the new second"
        );

        // Same starting cursor, but this page never leaves the :40Z second —
        // the previous count must carry forward and keep growing.
        let events_same_second = vec![sample_event(
            "agent:demo:task-1",
            "2026-08-04T19:52:40Z",
            "tool_result",
        )];
        assert_eq!(
            next_trace_cursor(Some("2026-08-04T19:52:40Z:2"), &events_same_second),
            Some("2026-08-04T19:52:40Z:3".to_string())
        );
    }

    #[test]
    fn next_trace_cursor_matches_a_real_docket_servers_minted_cursor() {
        // Captured live 2026-08-05 (card V1) against an isolated `docket serve`
        // instance (see TODO.md §6, V1 handoff) — a project with exactly one
        // trace session file, 3 events at :00Z then 2 at :01Z, polled four
        // times with docket's real `next` cursor fed back in as the next
        // poll's `since`. Every `next` value below is docket's own
        // `_traces_page`-minted string, read verbatim off the wire
        // (`curl .../traces/curstest?since=...`), not hand-computed — this is
        // the first time this reconstruction has been checked against a real
        // server rather than only unit-tested against hand-built cases
        // (B2's "still open" item in TODO.md §6).
        fn ev(n: i64, ts: &str) -> RemoteEvent {
            RemoteEvent {
                ts: ts.to_string(),
                project: "curstest".to_string(),
                session_id: "agent:curstest:only-session".to_string(),
                agent_role: "lead".to_string(),
                event_type: "tool_call".to_string(),
                payload: serde_json::json!({"n": n}),
                cost_usd_estimated: None,
                duration_ms: None,
            }
        }

        // Poll 1: since="" -> docket returned all 5 events (3 at :00Z, 2 at
        // :01Z) and minted next="2026-08-05T12:00:01Z:2".
        let poll1_events = vec![
            ev(1, "2026-08-05T12:00:00Z"),
            ev(2, "2026-08-05T12:00:00Z"),
            ev(3, "2026-08-05T12:00:00Z"),
            ev(4, "2026-08-05T12:00:01Z"),
            ev(5, "2026-08-05T12:00:01Z"),
        ];
        assert_eq!(
            next_trace_cursor(None, &poll1_events),
            Some("2026-08-05T12:00:01Z:2".to_string())
        );

        // Poll 2: since="2026-08-05T12:00:01Z:2" -> docket returned no new
        // events (nothing had been appended yet) and left next unchanged.
        assert_eq!(
            next_trace_cursor(Some("2026-08-05T12:00:01Z:2"), &[]),
            Some("2026-08-05T12:00:01Z:2".to_string())
        );

        // Poll 3: 2 more events appended at the SAME second (:01Z) after
        // poll 1 already advanced past it with count=2 — exactly the
        // "off-by-one hides here" case the card called out. docket returned
        // only the 2 new events (not the original 5) and minted
        // next="2026-08-05T12:00:01Z:4" — the previous count (2) carried
        // forward and added to this page's trailing run (2), not reset to 2.
        let poll3_events = vec![ev(6, "2026-08-05T12:00:01Z"), ev(7, "2026-08-05T12:00:01Z")];
        assert_eq!(
            next_trace_cursor(Some("2026-08-05T12:00:01Z:2"), &poll3_events),
            Some("2026-08-05T12:00:01Z:4".to_string())
        );

        // Poll 4: one more event at a new second (:02Z) -> the run resets;
        // docket minted next="2026-08-05T12:00:02Z:1", not ":5" (the prior
        // second's count must not leak across the boundary).
        let poll4_events = vec![ev(8, "2026-08-05T12:00:02Z")];
        assert_eq!(
            next_trace_cursor(Some("2026-08-05T12:00:01Z:4"), &poll4_events),
            Some("2026-08-05T12:00:02Z:1".to_string())
        );
    }

    #[test]
    fn session_id_task_id_parses_the_agent_project_suffix_convention() {
        assert_eq!(
            session_id_task_id("agent:demo:task-90e465a8"),
            Some("task-90e465a8".to_string())
        );
        // docket also mints this convention for non-task sessions
        // (core/dispatch.py's bare dispatch session, core/pod.py's project
        // key) — parsing still succeeds, correlation against orch_tasks is
        // just expected to miss, which is not this function's concern.
        assert_eq!(
            session_id_task_id("agent:demo:dispatch"),
            Some("dispatch".to_string())
        );
        assert_eq!(session_id_task_id("not-the-agent-convention"), None);
        assert_eq!(session_id_task_id(""), None);
    }

    #[tokio::test]
    async fn a_correlated_trace_event_lands_with_the_right_item_id() {
        let id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
                "demo",
                None,
                vec![event],
            )),
        };
        let store = Arc::new(
            FakeStore::new(vec![plane])
                .with_linked_projects(vec!["demo".to_string()])
                .with_known_task("task-1", item_id),
        );
        let store = run_one_tick(store).await;

        let upserted = store.upserted_events.lock().unwrap();
        let (_, events) = upserted
            .iter()
            .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
            .expect("expected at least one upserted event");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].item_id, Some(item_id));
        assert_eq!(events[0].event_type, "tool_call");
    }

    #[tokio::test]
    async fn an_uncorrelated_trace_event_lands_with_item_id_none_and_does_not_error() {
        let id = Uuid::new_v4();
        // "dispatch" is docket's own non-task session suffix (see
        // session_id_task_id's doc) — never correlates to any orch_tasks
        // row, and must not be treated as an error.
        let event = sample_event(
            "agent:demo:dispatch",
            "2026-08-04T19:52:27Z",
            "session_start",
        );
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
                "demo",
                None,
                vec![event],
            )),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_events.lock().unwrap();
        let (_, events) = upserted
            .iter()
            .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
            .expect("expected the uncorrelated event to still be mirrored");
        assert_eq!(events[0].item_id, None);

        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
        );
    }

    #[tokio::test]
    async fn an_unrecognised_event_type_is_stored_verbatim() {
        let id = Uuid::new_v4();
        let event = sample_event(
            "agent:demo:task-1",
            "2026-08-04T19:52:40Z",
            "some_future_event_type_v3",
        );
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
                "demo",
                None,
                vec![event],
            )),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
        let store = run_one_tick(store).await;

        let upserted = store.upserted_events.lock().unwrap();
        let (_, events) = upserted
            .iter()
            .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
            .expect("expected the event to still be mirrored");
        assert_eq!(events[0].event_type, "some_future_event_type_v3");
    }

    #[tokio::test]
    async fn traces_poll_failure_leaves_plane_health_untouched_and_persists_nothing() {
        let id = Uuid::new_v4();
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy_with_failing_traces()),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
        let store = run_one_tick(store).await;

        let records = store.health_records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
            "a /traces failure must not degrade plane health: {records:?}"
        );

        let upserted = store.upserted_events.lock().unwrap();
        assert!(
            upserted.iter().all(|(_, e)| e.is_empty()),
            "a failed traces poll must not persist anything: {upserted:?}"
        );
    }

    #[tokio::test]
    async fn a_successful_traces_poll_advances_the_stored_cursor() {
        let id = Uuid::new_v4();
        let events = vec![
            sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
            sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "session_start"),
        ];
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, events)),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
        let store = run_one_tick(store).await;

        let cursors = store.trace_cursors.lock().unwrap();
        assert_eq!(
            cursors.get("demo").map(String::as_str),
            Some("2026-08-04T19:52:40Z:1")
        );
    }

    #[tokio::test]
    async fn a_stored_cursor_is_used_as_since_on_the_next_poll() {
        let id = Uuid::new_v4();
        // FakeControlPlane's traces() is keyed by the exact (project, since)
        // pair it was called with — seeding it *only* for
        // since = Some("2026-08-04T19:52:27Z:1") proves poll_traces reads
        // the stored cursor and sends it, rather than always polling with
        // since = None.
        let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "tool_result");
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
                "demo",
                Some("2026-08-04T19:52:27Z:1"),
                vec![event],
            )),
        };
        let store = Arc::new(
            FakeStore::new(vec![plane])
                .with_linked_projects(vec!["demo".to_string()])
                .with_trace_cursor("demo", "2026-08-04T19:52:27Z:1"),
        );
        let store = run_one_tick(store).await;

        let upserted = store.upserted_events.lock().unwrap();
        let (_, events) = upserted
            .iter()
            .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
            .expect(
                "expected an event — only present if poll_traces actually sent \
                 the stored cursor as `since`",
            );
        assert_eq!(events[0].event_type, "tool_result");
    }

    #[tokio::test]
    async fn an_event_older_than_the_retention_cutoff_is_not_persisted() {
        let id = Uuid::new_v4();
        // Well outside even a 1-day retention window — simulates a rewound
        // cursor re-delivering an event that was already rolled up and
        // purged by B3's retention sweep (see the module doc's "Trace
        // cursor" / retention-composition section).
        let stale_event = sample_event("agent:demo:task-1", "2020-01-01T00:00:00Z", "tool_call");
        let plane = RegisteredPlane {
            id,
            control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
                "demo",
                None,
                vec![stale_event],
            )),
        };
        let store =
            Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));

        let store_dyn: Arc<dyn ControlPlaneStore> = store.clone();
        let handles = spawn_reconcilers(
            true,
            store_dyn,
            ReconcilerConfig {
                poll_secs: 60,
                event_retention_days: 1,
            },
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        for h in handles {
            h.abort();
        }

        let upserted = store.upserted_events.lock().unwrap();
        assert!(
            upserted.iter().all(|(_, e)| e.is_empty()),
            "an event older than the retention cutoff must never be (re-)inserted: {upserted:?}"
        );
    }

    // -- Card B3: retention sweep --------------------------------------------

    /// A [`RetentionStore`] whose two rollup methods are scripted and every
    /// call recorded, for driving [`spawn_retention_sweep`] without a real
    /// database.
    struct FakeRetentionStore {
        events_outcome: RollupOutcome,
        metrics_outcome: RollupOutcome,
        events_calls: Mutex<Vec<DateTime<Utc>>>,
        metrics_calls: Mutex<Vec<DateTime<Utc>>>,
    }

    impl FakeRetentionStore {
        fn new() -> Self {
            Self {
                events_outcome: RollupOutcome::default(),
                metrics_outcome: RollupOutcome::default(),
                events_calls: Mutex::new(Vec::new()),
                metrics_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl RetentionStore for FakeRetentionStore {
        async fn rollup_and_purge_events(
            &self,
            cutoff: DateTime<Utc>,
            _batch_size: i64,
        ) -> Result<RollupOutcome, OrchError> {
            self.events_calls.lock().unwrap().push(cutoff);
            Ok(self.events_outcome)
        }

        async fn rollup_and_purge_metrics(
            &self,
            cutoff: DateTime<Utc>,
            _batch_size: i64,
        ) -> Result<RollupOutcome, OrchError> {
            self.metrics_calls.lock().unwrap().push(cutoff);
            Ok(self.metrics_outcome)
        }
    }

    #[tokio::test]
    async fn disabled_retention_sweep_spawns_nothing_and_never_calls_the_store() {
        let store = Arc::new(FakeRetentionStore::new());
        let handle = spawn_retention_sweep(false, store.clone(), 90, 1);
        assert!(handle.is_none());

        // Give a would-be sweep a chance to run, if it wrongly had.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(store.events_calls.lock().unwrap().is_empty());
        assert!(store.metrics_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn enabled_retention_sweep_calls_both_rollups_with_a_cutoff_derived_from_retention_days()
    {
        let store = Arc::new(FakeRetentionStore::new());
        let retention_days = 90u32;
        let before_spawn = Utc::now();
        let handle = spawn_retention_sweep(true, store.clone(), retention_days, 1);
        assert!(handle.is_some());

        tokio::time::sleep(Duration::from_millis(200)).await;
        handle.unwrap().abort();

        let events_calls = store.events_calls.lock().unwrap();
        let metrics_calls = store.metrics_calls.lock().unwrap();
        assert!(
            !events_calls.is_empty(),
            "expected at least one events rollup call"
        );
        assert!(
            !metrics_calls.is_empty(),
            "expected at least one metrics rollup call"
        );

        // The cutoff passed must be ~ (now - retention_days), not some other
        // arbitrary value — bounds-check against the window this test ran in.
        let expected_floor = before_spawn - chrono::Duration::days(retention_days as i64 + 1);
        let expected_ceiling = Utc::now() - chrono::Duration::days(retention_days as i64 - 1);
        for cutoff in events_calls.iter().chain(metrics_calls.iter()) {
            assert!(
                *cutoff > expected_floor && *cutoff < expected_ceiling,
                "cutoff {cutoff} not within the expected ~{retention_days}-day-ago window"
            );
        }
    }

    #[tokio::test]
    async fn a_rollup_failure_is_logged_and_does_not_stop_the_sweep_ticker() {
        struct AlwaysFailingRetentionStore {
            calls: Mutex<usize>,
        }

        #[async_trait::async_trait]
        impl RetentionStore for AlwaysFailingRetentionStore {
            async fn rollup_and_purge_events(
                &self,
                _cutoff: DateTime<Utc>,
                _batch_size: i64,
            ) -> Result<RollupOutcome, OrchError> {
                *self.calls.lock().unwrap() += 1;
                Err(OrchError::Unavailable("db unreachable".into()))
            }

            async fn rollup_and_purge_metrics(
                &self,
                _cutoff: DateTime<Utc>,
                _batch_size: i64,
            ) -> Result<RollupOutcome, OrchError> {
                Err(OrchError::Unavailable("db unreachable".into()))
            }
        }

        let store = Arc::new(AlwaysFailingRetentionStore {
            calls: Mutex::new(0),
        });
        // A short sweep interval so several ticks happen inside the test's wait.
        let handle = spawn_retention_sweep(true, store.clone(), 90, 1).unwrap();
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        handle.abort();

        assert!(
            *store.calls.lock().unwrap() >= 2,
            "the ticker must keep retrying after a failed sweep, not stop"
        );
    }
}
