# Orchestration Architecture

This chapter covers the Agent-Factory Control Center: the `tack-orch` crate, the
`ControlPlane` trait, the reconciler's poll loop, and the schema behind it. It assumes
you've read the [Architecture Overview](README.md) and the [Crate Tour](crate-tour.md)
for the four original crates — this one adds a fifth.

For the *why* behind this feature and the full multi-phase plan, see
[Roadmap → Agent-Factory Control Center](../roadmap.md#next--agent-factory-control-center-phases-3338-august-2026).
This page documents what's actually implemented today (Phase 33, read-only), not the
aspirational end state.

## The one-line architecture

Tack holds desired state, an external agent-fleet backend executes it, and a
reconciler closes the loop by polling. As shipped today, only the pull half of that
loop exists:

```text
┌─────────────────────────── Tack (control center) ────────────────────────────┐
│  Fleet view          GET /api/fleet                                          │
│  tack-api            GET/POST /api/control-planes, /api/control-planes/{id}  │
│                       GET/PUT  /api/projects/{id}/orch-link                  │
│                                                                               │
│  tack-orch::reconciler    one tokio task per registered plane                │
│      poll /health + /status.json  ──►  healthy/degraded/unreachable          │
│                                         + control_planes.* (tack-db)         │
│                                                                               │
│  tack-orch::ControlPlane (trait)                                            │
│      └── adapters::docket::DocketAdapter   (the only implementor today)     │
└───────────────────────────────────┬───────────────────────────────────────────┘
                                    │ HTTP, Bearer on authenticated routes only
┌───────────────────────────────────▼───────────────────────────────────────────┐
│  docket serve            /health  /status.json  /metrics  (unauthenticated)  │
│                           /runs  /runs/{id}  /approvals  (Bearer)            │
└─────────────────────────────────────────────────────────────────────────────┘
```

Everything above the line is real and shipped. Dispatch (Tack → docket, the push
half of the loop — `POST /api/items/{id}/dispatch` and friends) does not exist yet;
neither does docket's own `POST /tasks/{project}` that it would depend on. See
[What's implemented vs. not](#whats-implemented-vs-not) for the precise boundary.

## The `ControlPlane` trait, and why it exists

`crates/tack-orch/src/lib.rs` defines:

```rust
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
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<Vec<RemoteEvent>, OrchError>;
    // Write side — implemented by DocketAdapter as Err(OrchError::Disabled) until dispatch ships.
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<(), OrchError>;
}
```

**Tack is a factory control center, not a docket-specific dashboard.** Nothing in the
reconciler, the API handlers, or the frontend imports `DocketAdapter` directly — they
only ever hold an `Arc<dyn ControlPlane>`. `docket` is the only backend implemented
today (`adapters::docket::DocketAdapter`), but the trait is the seam a second backend
(GitHub Actions, Temporal, a different agent-fleet tool entirely) would implement
without touching the reconciler, the handlers, or the Fleet view at all. If you're
reaching for `DocketAdapter` by name outside `tack-orch::adapters` or a control-plane
construction site, you're probably working against the trait, not with it.

`DocketAdapter::new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, OrchError>`
returns `Result` rather than panicking, because `base_url` frequently comes from a
user-typed value in a DB row — a bad URL is a normal, expected failure mode, not a
crash. `token: None` is a fully-supported configuration: every unauthenticated docket
route still works, and calling an authenticated route without a token just gets
docket's real `401` (mapped to `OrchError::Auth`).

### `OrchError`

```rust
pub enum OrchError {
    Http(String),        // transport-level failure
    Auth,                 // 401/403
    Decode(String),       // malformed response body
    NotFound(String),     // resource doesn't exist on the remote side
    Unavailable(String),  // plane configured but not currently reachable
    Disabled,              // gated behind a flag/config or a not-yet-implemented write method
}
```

### Remote enums never fail a poll on an unrecognised value

`RunState`, `RunSource`, `TaskStatus`, and `ApprovalState` are generated by a
`remote_string_enum!` macro (`lib.rs`) rather than a plain `#[derive(Deserialize)]`.
Each gets a hand-written `Serialize`/`Deserialize` (via `String`, not
`#[serde(other)]` — that attribute only supports a payload-less fallback, and the
original string needs to survive) plus an `Unknown(String)` variant that round-trips
byte-for-byte. A docket upgrade that adds a new run state or task status degrades to
"shown as-is" on the next poll, never to a `Decode` error that kills that plane's poll
loop. If you add a new remote-state enum, use the macro rather than hand-rolling
another one.

## Money: `*_usd_estimated`, always

Every dollar-valued field in `tack-orch`, the repository layer, and the API DTOs is
named with an `_usd_estimated` suffix (`cost_usd_estimated` on `orch_tasks`,
`FleetEntry`, `FleetAgent`, `RemoteEvent`, …), even where the upstream wire field is
the bare `costUsd`/`cost_usd` — the `#[serde(rename = "...")]` maps the name, not the
meaning. `orch_links.budget_usd` is the one deliberate exception: it's a cap a human
typed in, not a number derived from token counts, so it's never suffixed. See the
[user guide's explanation](../user-guide/orchestration.md#why-every-dollar-figure-says-estimated)
for the user-facing framing of this same rule.

## The reconciler

`crates/tack-orch/src/reconciler.rs` runs one `tokio` task per registered control
plane. Each tick is three strictly separated phases, enforced by the types rather than
by convention:

1. **Fetch** (`reconcile_once`) — every HTTP call the tick needs. No database handle
   is reachable from this phase at all; `reconcile_once` and everything it calls never
   receive a store or pool.
2. **Decide** (`HealthTracker::observe`) — a pure, synchronous state transition over
   the fetch result. No I/O.
3. **Persist** (`spawn_one`'s single `store.record_health(...).await` call) — one
   short write, invoked once per tick, strictly after phase 1's `.await` has already
   resolved.

Because phase 1 has fully completed before phase 3 begins, there is no window where a
write transaction is open while an HTTP request to docket is in flight — this is the
concrete mechanism behind the first non-negotiable below.

### Health state machine

```rust
pub const DEGRADED_AFTER_FAILURES: i64 = 3;
pub const UNREACHABLE_AFTER_FAILURES: i64 = 10;
pub const MAX_BACKOFF_SECS: u64 = 300;
```

- **healthy → degraded** after 3 consecutive `/health`+`/status.json` failures.
- **degraded → unreachable** after 10 consecutive failures.
- **Recovery is immediate.** A single successful poll resets `consecutive_failures` to
  zero and re-evaluates state from scratch, regardless of how long the prior outage
  ran.
- An `apiVersion` mismatch (`FleetStatus.api_version`'s major component differs from
  `EXPECTED_API_VERSION`, currently `"2"`) is an **independent** signal from
  reachability: `HealthTracker::observe` takes the more severe of the two
  (`HealthState` derives `Ord` with `Healthy < Degraded < Unreachable` specifically so
  this "max of two signals" logic is a one-line `.max()`). A plane that's both
  unreachable *and* version-mismatched still reports `unreachable`, never silently
  downgraded.
- Poll backoff doubles with each consecutive failure, capped at `MAX_BACKOFF_SECS`
  (`backoff_secs`); backoff only kicks in once a poll has actually failed.
- Log severity only fires on a *transition* — entering degraded/unreachable logs
  `warn`, recovering logs `info`, an unchanged repeat failure logs `debug`. A
  sustained outage produces at most two `warn` lines no matter how long it lasts.

### Jitter is deterministic, not `rand`

`jittered_secs(plane_id, tick, base_secs)` hashes `(plane_id, tick)` with
`std::collections::hash_map::DefaultHasher` and maps the result to a ±20% fraction
applied to `base_secs`. This is deliberate, not a placeholder: `rand` is not a
workspace dependency, and a deterministic function keeps the schedule reproducible in
tests without threading a seeded RNG through the whole call chain. It still gives the
real anti-stampede property — different plane IDs land on different offsets within the
same base interval, so N registered planes don't all wake up on the same tick.

### Panic isolation

A panic inside a poll — a bug in a `poll_*` function, a misbehaving adapter — must
never take down another plane's task, or surface in a user request. `spawn_one` gets
this by wrapping each tick's fetch phase in its own `tokio::spawn`: a panic there
becomes a `JoinError` the outer (non-panicking) loop catches, logs, and treats as a
failed poll. The loop keeps ticking; no other plane's task is affected; nothing here
ever makes an *inbound* HTTP call, so there's no way for this to touch a live user
request at all.

### Extending the poll loop: adding a `poll_*` step

`reconcile_once` builds a `FetchOutcome` as a flat struct-of-results, one field per
HTTP call. Adding a new poll step (a future `poll_runs`, `poll_approvals`,
`poll_traces`, `poll_metrics`, or anything else) is exactly three edits, and nothing
else in the file changes:

```rust
// 1. Add a field to FetchOutcome:
struct FetchOutcome {
    health: Result<Health, OrchError>,
    status: Result<FleetStatus, OrchError>,
    runs: Result<Vec<RemoteRun>, OrchError>,       // <- new
}

// 2. Add your own fetch-only poll_* fn, same shape as poll_health/poll_status
//    (module-private, no DB access — HTTP call only):
async fn poll_runs(
    control_plane: &Arc<dyn ControlPlane>,
    project: Option<&str>,
) -> Result<Vec<RemoteRun>, OrchError> {
    control_plane.list_runs(project).await
}

// 3. Add one line inside reconcile_once's struct literal:
async fn reconcile_once(control_plane: &Arc<dyn ControlPlane>) -> PollEvaluation {
    let fetched = FetchOutcome {
        health: poll_health(control_plane).await,
        status: poll_status(control_plane).await,
        runs: poll_runs(control_plane, project).await,   // <- new
    };
    evaluate(&fetched)
}
```

Your own persistence call (e.g. `store.upsert_orch_runs(...)`) does **not** go inside
`reconcile_once` — add it in `spawn_one`'s loop, as its own short call placed *after*
`store.record_health(...).await`, keeping the same fetch-then-persist separation.
**Do not let a new poll step's failure influence `evaluate`'s reachability verdict** —
`evaluate` deliberately reads only `.health`/`.status`; every other field is a
data-ingestion concern with its own success/failure handling, independent of plane
health.

### Persistence interface

The reconciler depends on a narrow trait, not `tack_db::Repository` directly:

```rust
#[async_trait::async_trait]
pub trait ControlPlaneStore: Send + Sync {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError>;
    async fn record_health(&self, control_plane_id: Uuid, record: &HealthRecord) -> Result<(), OrchError>;
}
```

The reason it's an abstraction rather than the concrete repository: turning a
`control_planes` row into a live `Arc<dyn ControlPlane>` needs both a real database
read *and* dispatch on `kind` to a concrete adapter constructor, and `tack-orch` has
no reason to depend on `tack-db`'s concrete `Repository` type beyond what it already
needs for the trait's own signatures.

The real implementation, `RepoControlPlaneStore`, lives in
`crates/tack-api/src/orch_store.rs` — deliberately kept out of `server.rs`,
`router.rs`, `config.rs`, and `handlers/orch.rs`, since `tack-api` is the one crate
that already depends on both `tack-db` (for `Repository`) and `tack-orch` (for the
adapter and the trait), and this glue has exactly one reason to change: a new
control-plane `kind` needs a new adapter, or the persistence mapping shifts.
`list_registered` never fails the whole poll cycle over one bad row — an unknown
`kind`, a token-lookup error, or an adapter that fails to construct (an unparsable
`base_url`) each just skip that one plane (logged at `warn`) rather than aborting
every other registered plane's polling too. Only a failure to read the
`control_planes` table at all surfaces as an `Err`.

`server.rs` spawns it, gated behind `config.orch_enable`:

```rust
if config.orch_enable {
    let store: Arc<dyn reconciler::ControlPlaneStore> =
        Arc::new(RepoControlPlaneStore::new(state.repo.clone()));
    let handles = reconciler::spawn_reconcilers(
        true,
        store,
        reconciler::ReconcilerConfig { poll_secs: config.orch_poll_secs },
    ).await;
    // ...
}
```

`spawn_reconcilers(enabled, ..)` is itself the mechanism behind "off by default": with
`enabled: false` it returns an empty `Vec` **without even calling
`store.list_registered()`** — not just "spawns nothing," but "never queries for what
it would have spawned."

## The Prometheus text parser

`crates/tack-orch/src/adapters/prometheus.rs` — a small, dependency-free parser for
docket's `/metrics` endpoint (Prometheus text-exposition format): `pub fn
parse(input: &str) -> Vec<MetricSample>`. It never errors and never panics; a
malformed line is dropped, not treated as a whole-document failure. It lives under
`adapters::` rather than at the crate root because `tack-orch/src/lib.rs` is frozen
(see below) and adding a new top-level module there requires an edit to a file no
single Wave-1 card owned — the public path is
`tack_orch::adapters::prometheus::parse`, not `tack_orch::prometheus::parse`. Any
future consumer of `/metrics` (a metrics-ingestion feature, a `GET /api/metrics`
handler) should reuse this parser rather than writing a second one.

## Crate layout

```text
crates/tack-orch/
├── Cargo.toml         Depends on tack-core, tack-db, tokio, reqwest (the
│                       workspace's existing client — no second one), serde,
│                       serde_json, thiserror, async-trait, chrono, uuid,
│                       tracing. Dev-dep: wiremock.
└── src/
    ├── lib.rs          ControlPlane trait, OrchError, remote-state enums,
    │                    every DTO (Health, FleetStatus, FleetAgent,
    │                    RemoteRun, RemoteApproval, RemoteTask, NewRemoteTask,
    │                    RemoteEvent, MetricSample). Frozen after Wave 1 —
    │                    every later addition consumes this surface verbatim
    │                    rather than renaming a field.
    ├── reconciler.rs   Poll loop, health state machine, ControlPlaneStore
    │                    trait, spawn_reconcilers/spawn_one.
    └── adapters/
        ├── mod.rs       pub mod docket; pub mod prometheus;
        ├── docket.rs    DocketAdapter — the ControlPlane impl for docket.
        └── prometheus.rs  /metrics text-exposition parser.
```

**Dependency direction is inward-only and enforced by review, not by the compiler:**
`tack-orch` depends on `tack-core` and `tack-db`; it must never depend on `tack-api`.
`tack-api` depends on `tack-orch` (`crates/tack-api/Cargo.toml`) to spawn the
reconciler and expose the control-plane/fleet routes — never the reverse. If you're
adding code to `tack-orch` and find yourself reaching for a `tack-api` handler DTO,
stop: define the type in `tack-orch` and let `tack-api` depend on it, or duplicate the
small shape, rather than inverting the graph.

`crates/tack-api/src/orch_store.rs` is where the two halves actually meet — see
"Persistence interface" above.

## The database layer

`crates/tack-db/src/repo/orch.rs` is the repository module for every table below.
Notable design points:

- **The stored control-plane token never leaves this layer in a read DTO.**
  `get_control_plane_token(id) -> Result<Option<String>, sqlx::Error>` is
  doc-commented "INTERNAL ONLY" — every other read (`get_control_plane`,
  `list_control_planes`) returns a DTO with no `token` field at all, only
  `token_set: bool`. This is a compile-time guarantee, not a runtime check: there is
  no field to accidentally serialize.
- `UpdateControlPlane.token: Option<Option<String>>` gives the API layer tri-state
  `PATCH` semantics: `None` = field absent (leave stored token untouched), `Some(None)`
  = explicit `null` (clear it), `Some(Some(t))` = set/replace.
- Batch upserts (`upsert_orch_tasks`/`_runs`/`_events`/`_approvals`) each take a slice
  and use **one transaction per call**, not one per row — a poll tick that upserts 50
  task rows opens exactly one write transaction. An empty slice is a documented no-op:
  `Ok(())` without opening a transaction at all.
- `orch_runs.item_id` and `orch_approvals.item_id` are write-once through the upsert
  path (`ON CONFLICT DO UPDATE SET item_id = COALESCE(excluded.item_id, item_id)`): a
  poll that doesn't yet know an item's attribution can never clobber an attribution a
  previous poll already learned.
- `remote_status`/`state`/`source`/`event_type` columns are plain `String` — this
  layer does no matching against `RunState`/`TaskStatus`/etc. An unrecognised value
  round-trips byte-for-byte, the same "never fail on an unknown value" discipline as
  the enums in `tack-orch`.

## Migrations 019–024

Landed in `crates/tack-db/src/migrations.rs` following the `018_github_links`
precedent — a `const [&str; N]` slice of statements per migration, registered in the
migration list. Every foreign key to `items`/`projects`/`control_planes` is `ON DELETE
CASCADE`. Timestamps are RFC3339 `TEXT`; UUIDs are `TEXT`, matching every other table
in the database.

| Migration | Table | Key columns |
|---|---|---|
| 019 | `control_planes` | `id` PK, `name`, `kind` (default `'docket'`), `base_url`, `token` (nullable, write-only over the API), `api_version`, `health` (default `'unknown'`), `last_seen_at`, `consecutive_failures`. No FKs — root of the graph. |
| 020 | `orch_links` | `project_id` PK (FK → `projects`, one link per project — mirrors the one-row-per-item shape of `github_links`), `control_plane_id` (FK → `control_planes`), `remote_project`, `pipeline_file`, `blueprint`, `auto_dispatch`, `budget_usd` (a cap, deliberately not `_estimated`), `status_map` (JSON `TEXT`, default `'{}'`). |
| 021 | `orch_tasks` | **Composite PK `(item_id, remote_task_id)`** — not a single-column key, because an item can be redispatched and each dispatch gets its own row. `remote_run_id` (indexed, not a hard FK — a task can exist before its run is mirrored), `remote_status` (default `'pending'`), `attempt` (default 1), `tokens_in`/`tokens_out` (the primary measure), `cost_usd_estimated` (derived, nullable), `dispatched_at`, `trusted` (default 1; imported items will set 0). |
| 022 | `orch_runs` | `run_id` PK, `control_plane_id` (FK), `item_id` (nullable FK — `NULL` means "mirrored, unattributed," the normal case until dispatch exists), `remote_project`, `source` (default `'cli'`), `state` (default `'queued'`), `started_at`, `ended_at`, `error`. |
| 023 | `orch_events` | Append-only telemetry. `id` PK (caller-assigned — a trace event has no natural key from docket's side), `control_plane_id` (FK), `item_id` (nullable FK), `run_id` (`TEXT`, no hard FK), `event_type` (raw string, stored verbatim including types Tack doesn't yet recognise), `payload` (JSON `TEXT`). Two indexes: `(item_id, occurred_at)` for an item's timeline, `(occurred_at)` for the future retention sweep. |
| 024 | `orch_approvals` | `token` PK (docket's own approval token — a correlation id, not a credential), `control_plane_id` (FK), `item_id` (nullable FK — `NULL` means uncorrelated, must still surface in a fleet-wide inbox), `remote_task_id` (no hard FK, correlation only), `agent`, `action`, `state` (default `'pending'`), `requested_at`, `decided_at`. |

Indexes beyond the primary/foreign keys: `orch_links.control_plane_id`,
`orch_tasks.remote_run_id`, `orch_runs(control_plane_id, state)`,
`orch_events(item_id, occurred_at)`, `orch_events(occurred_at)`,
`orch_approvals.item_id`, `orch_approvals.state`.

`orch_metrics` — mentioned as a possible seventh table in earlier planning notes — is
**not** part of migrations 019–024 and does not exist yet. Nothing in the current
crate layout references it. If a future migration adds it, it needs its own number
(025+) and its own repository functions in `repo/orch.rs`, following the same
`New*` input struct + batch `upsert_*` shape as every other entity there.

## The API surface

Every route below is registered once, in a dedicated `orch_routes()` sub-router in
`crates/tack-api/src/router.rs`, and carries a single middleware layer —
`orch::require_orch_enabled` — rather than a per-handler check:

```rust
pub async fn require_orch_enabled(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.orch_enable {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(next.run(req).await)
}
```

With `TACK_ORCH_ENABLE` unset, every route below 404s — indistinguishable from not
existing. The ordinary Bearer-token gate (`require_token`) wraps this sub-router too,
so both gates apply when both are configured.

| Method | Path | Notes |
|---|---|---|
| `POST` | `/api/control-planes` | Register a control plane. `token` write-only; response never echoes it. |
| `GET` | `/api/control-planes` | List every registered plane (no tokens). |
| `GET` | `/api/control-planes/{id}` | One plane. |
| `PATCH` | `/api/control-planes/{id}` | `token: Option<Option<String>>` tri-state — absent leaves it, `null` clears it, a string replaces it. |
| `DELETE` | `/api/control-planes/{id}` | Deregister. |
| `GET` | `/api/projects/{id}/orch-link` | Returns `{linked: false, link: null}` for an unlinked project — `200`, not `404` (matches the `settings.rs` "not configured yet" precedent, not "resource not found"). |
| `PUT` | `/api/projects/{id}/orch-link` | Create/replace the project's link. Validates every `status_map` status name against the project's live `WorkflowConfig`; an unknown name is `400`. |
| `GET` | `/api/fleet` | The Fleet view's aggregate — see below. |

`router.rs` also carries commented placeholders, one line each, for every route later
phases of this feature will add (`/metrics`, `/items/{id}/dispatch`,
`/sprints/{id}/dispatch`, `/approvals/{token}`), at the exact insertion point — the
intent is that `router.rs` gets touched once per addition, not restructured.

### `GET /api/fleet`

One row per Tack project that has an `orch_links` row (not one row per control plane),
joining the link, its control plane's reconciler-observed health, and cost/token/
approval sums from `orch_tasks`/`orch_approvals` for that project's items:

```json
{
  "rows": [
    {
      "project_id": "uuid", "project_name": "string",
      "control_plane_id": "uuid", "control_plane_name": "string", "control_plane_kind": "docket",
      "remote_project": "string",
      "health": "unknown | healthy | degraded | unreachable",
      "last_seen_at": "RFC3339 | null", "consecutive_failures": 0, "api_version": "string | null",
      "gateway": "active | inactive | unknown",
      "roster": [{ "id": "string", "name": "string", "role": "string", "model": "string" }],
      "last_activity_at": "RFC3339 | null",
      "auto_dispatch": false, "blueprint": "string | null", "budget_usd": 50.0,
      "tokens_in": 0, "tokens_out": 0,
      "cost_usd_estimated": 0.0,
      "pricing_snapshot_at": "string | null",
      "pending_approval_count": 0
    }
  ]
}
```

`cost_usd_estimated` is `null` whenever `health == "unreachable"` — never coerced to
`0` — so a stale row is representable as stale rather than as a confident zero.
`Some(0.0)` means the plane is reachable and genuinely has nothing mirrored yet.
`tokens_in`/`tokens_out` are always a plain (never-null) sum: staleness is signalled
through `health`, not through nullability, on those two fields specifically.
`pending_approval_count` is scoped **per project** (an inner join on
`orch_approvals.item_id → items.project_id`) — an uncorrelated approval (`item_id
IS NULL`) is excluded from every row here and is meant to surface in a fleet-wide
approvals inbox instead, once that exists.

Every handler in `handlers/orch.rs` reads Tack's own database, populated out-of-band
by the reconciler. **No handler in this file makes an outbound HTTP call** — a docket
outage can only ever leave `health`/`last_seen_at` stale; it can never turn into a
`500` on a user's request, because the two are architecturally decoupled (the
reconciler is the only thing that ever talks to docket, entirely off the request
path).

## The four non-negotiables

These are enforced by the shape of the code, not just documented as a convention —
each one below names where.

1. **Never hold a SQLite write transaction across an HTTP call to docket.** The
   reconciler's fetch → decide → persist phase separation (above) makes this
   structurally true: the persist phase doesn't have a `ControlPlane` handle to await
   on even if it wanted one. Every repository batch-upsert function also does its own
   HTTP-free single-transaction write — none of them call out to a control plane.
2. **Status changes go through the workflow engine, never raw SQL.** WIP limits,
   explicit transitions, `started_at`/`completed_at`, and parent auto-propagation must
   all still fire on an orchestration-driven status change, exactly as they do on a
   user drag. As shipped today there's no code path that writes an item's status from
   this feature yet (dispatch, the feature that will need this, hasn't landed) — but
   the save-time guard is already in place one layer up: `PUT /orch-link`'s
   `status_map` validates every named status against the project's live
   `WorkflowConfig` before it's ever stored, so a typo can't silently become a
   no-op transition later.
3. **Everything is off by default.** `TACK_ORCH_ENABLE` unset ⇒ `spawn_reconcilers`
   returns before even querying for registered planes, and `require_orch_enabled`
   404s every route in `orch_routes()`. Both are tested
   (`crates/tack-orch/src/reconciler.rs`'s
   `disabled_orchestration_spawns_no_tasks_and_never_queries_the_store`;
   `crates/tack-api/tests/orch_test.rs`'s `every_orch_route_404s_when_disabled`).
4. **The stored control-plane token never leaves the repository layer.** Every read
   DTO — in `repo/orch.rs` and in `handlers/orch.rs`'s `ControlPlaneResponse` — exposes
   `token_set: bool` and nothing else; the only function that returns the real value,
   `get_control_plane_token`, is doc-commented internal-only and is called from
   exactly one place (`orch_store.rs`, to construct a live adapter). See the [user
   guide](../user-guide/orchestration.md#a-known-gap-the-control-plane-token-is-not-yet-scrubbed-from-backups)
   for a related, currently-open gap: this discipline covers the API, but the local
   database backup endpoint does not yet scrub `control_planes.token` the way it
   already scrubs the cloud-backup secret key.

## Adding a new control-plane backend

1. Implement `ControlPlane` for your type in a new `adapters::<name>` module,
   returning `OrchError::Disabled` for any write method you don't support yet.
2. Add a `match` arm for your `kind` string in `RepoControlPlaneStore::list_registered`
   (`crates/tack-api/src/orch_store.rs`) that constructs your adapter from a
   `control_planes` row's `base_url`/token.
3. Nothing in `reconciler.rs`, `handlers/orch.rs`, or the Fleet frontend needs to
   change — they all consume `Arc<dyn ControlPlane>`.

## What's implemented vs. not

Implemented, tested, and live behind `TACK_ORCH_ENABLE`: control-plane
registration/CRUD, project↔control-plane links with `status_map` validation, the
reconciler's health polling (`/health` + `/status.json` only) and state machine, and
the `GET /api/fleet` aggregate.

Not implemented — present only as honest placeholders on the wire, or not present at
all:

- **`gateway` is always `"unknown"`.** `control_planes` has no persisted gateway
  column, and the reconciler doesn't poll or store one — `FleetStatus.gateway` exists
  in the DTO but nothing writes it anywhere yet. Also worth knowing independent of
  this: docket's own `gateway_active()` is hardcoded to return `false` in the current
  docket version (no daemon gateway exists any more), so even a plane that *is* being
  polled for this would read `"inactive"` universally today, not just in Tack's
  placeholder.
- **`roster` is always `[]`.** No agent-roster table exists in migrations 019–024.
- **`pricing_snapshot_at` is always `null`.** No pricing-snapshot mechanism exists
  anywhere in the codebase yet.
- **Dispatch does not exist.** No `POST /api/items/{id}/dispatch`, no
  `POST /api/sprints/{id}/dispatch`, no code path that writes to docket at all. The
  `ControlPlane` trait's write methods (`enqueue_task`, `dispatch`,
  `decide_approval`) are all implemented by `DocketAdapter` today as
  `Err(OrchError::Disabled)`.
- **`GET /tasks/{project}` and `GET /traces/{project}` don't exist on docket at all**
  in the version this integration targets — confirmed by reading docket's own route
  table, not inferred. `ControlPlane::list_tasks`/`traces` will return `NotFound`
  against any real docket instance until that changes upstream. This is expected, not
  a bug — treat a `NotFound` from either method as "not available yet," not as the
  plane being broken.
- **Run, approval, and trace mirroring into `orch_runs`/`orch_approvals`/`orch_events`
  is not wired up.** The tables and the repository's batch-upsert functions for them
  exist (per the schema above), but nothing in the reconciler calls them yet — the
  poll loop currently only executes the two steps in the diagram above.

## Testing this feature

- `cargo test -p tack-orch` — the `ControlPlane`/DTO unit tests in `lib.rs`, the
  `DocketAdapter` integration tests against `wiremock` fixtures in
  `tests/docket_adapter_test.rs` (captured from a real `docket serve`, plus
  deliberately malformed/unknown-enum fixtures), the Prometheus parser's own unit
  tests, and the reconciler's state-machine tests in `reconciler.rs` (driven by a fake
  `ControlPlane`/`ControlPlaneStore`, no real network or database).
- `cargo test -p tack-db` — migration table-existence/upgrade-in-place/FK-orphan tests
  (`tests/orch_migrations_test.rs`) and the repository's CRUD/idempotency tests
  (`tests/orch_repo_test.rs`), both against in-memory SQLite.
- `cargo test -p tack-api` — the route-gating and token-leak tests
  (`tests/orch_test.rs`), including `every_orch_route_404s_when_disabled` and a suite
  that asserts the literal token string never appears in any response body.

None of these require a live docket instance — the adapter tests run against
`wiremock`, and everything downstream of it runs against an in-memory database and a
hand-written fake `ControlPlane`.
