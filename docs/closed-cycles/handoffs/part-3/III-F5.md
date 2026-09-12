# III-F5 handoff

- **Base SHA / branch / final SHA:** base `cbdd4a325a89df3f97bd8bc3009f51024df065fb`
  (`cbdd4a3`, tip of `plan/harness-agnostic-agent-fleet` at Wave 4 close — "docs: close
  out Wave 4 with the III-E6 handoff and accepted integration SHA") / branch
  `agent/iii-f5-retention`, worked in an isolated worktree. Final SHA: this branch's
  `HEAD` after the single commit that includes this file (the exact hash is
  necessarily unknowable *before* that commit exists, since this file's own content
  is part of what the commit hashes — see `git log -1 agent/iii-f5-retention` for the
  authoritative value; do not trust a hard-coded guess here over that).

## Files changed (must equal ownership list)

Per the card's own charter ("execution retention/metrics/health modules,
startup/shutdown wiring assigned at integration, soak tests"):

- **New, wholly owned by this card:**
  - `crates/tack-orch/src/execution_retention.rs` — cancellable retention sweep:
    `ExecutionRetentionStore` trait, `RepoExecutionRetentionStore` (real
    `tack_db::Repository`-backed impl), `RetentionClock`/`SystemClock`, `spawn_execution_retention_sweep`.
  - `crates/tack-orch/src/execution_observability.rs` — health watch: `ExecutionFleetSnapshot`,
    `ExecutionObservabilityStore`/`RepoExecutionObservabilityStore`, `evaluate_alerts`,
    `ObservabilityClock`/`SystemClock`, `spawn_execution_health_watch`.
  - `crates/tack-api/src/execution_runtime.rs` — `ExecutionRuntime` start/stop wrapper
    (real join on `stop()`, unlike `OrchRuntime`).
  - `crates/tack-db/tests/execution_retention_test.rs` — real, file-backed-DB proof for
    the new repository methods (6 tests).
  - `crates/tack-orch/tests/execution_retention_prod_test.rs` — production-runtime proof:
    real spawned tasks + real `Repository` + injected clock (2 tests).
- **Modified, additive only:**
  - `crates/tack-db/src/repo/execution.rs` — new `PurgeStats`/`ExecutionFleetSnapshotRow`
    types + `purge_stale_execution_replays`/`purge_stale_terminal_execution_events`/
    `execution_fleet_snapshot` methods, appended at the end of the existing
    `impl Repository` block. No existing method's signature or behavior changed.
    (Not on the forbidden list — only `repo/mod.rs` is.)
  - `crates/tack-orch/src/lib.rs` — two new `pub mod` lines (`execution_retention`,
    `execution_observability`). No existing line changed.
  - `crates/tack-api/src/lib.rs` — one new `pub mod execution_runtime;` line.
  - `crates/tack-api/src/config.rs` — 5 new `AppConfig` fields + defaults + env parsing
    (`execution_retention_enable/_days/_interval_secs`, `execution_health_enable/_interval_secs`).
    No existing field touched.
  - `crates/tack-api/src/server.rs` — see "Behavior implemented" #4 below for the exact
    diff; two small, additive blocks.
  - `CLAUDE.md` — 5 new config-table rows appended after `TACK_ORCH_APPROVAL_TOKEN`;
    no other row touched.
- **Not touched:** `crates/tack-api/src/router.rs`, `openapi.rs`, `handlers/mod.rs`,
  `crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo/mod.rs`,
  `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
  `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`, root
  `Cargo.toml`/`Cargo.lock`, any other card's handoff, anything under `frontend/`.

## Contract fixtures consumed

None. This card's `Owns` line (retention/metrics/health) touches no runner-v1 wire
shape — it operates entirely on server-side rows after they're already durable. No
fixture in `docs/contracts/runner-v1/` was read, referenced, or needed.

## Behavior implemented

### 1. Retention: what rolls up, what purges, and why not both everywhere

`crates/tack-db/src/repo/execution.rs` gets three new methods:

- **`purge_stale_execution_replays(cutoff, batch_size)`** — deletes stale rows from
  the six idempotency/replay bookkeeping tables (`execution_claim_replays`,
  `execution_heartbeat_replays`, `execution_cancellation_replays`,
  `execution_event_batch_replays`, `execution_completion_replays`,
  `execution_recovery_audits`), one bounded `BEGIN IMMEDIATE` transaction per batch
  per table, looping until nothing older than `cutoff` remains. This is **purge, not
  roll-up**, and is documented as exactly that: every row in these six tables exists
  solely to answer "have I already processed this exact retried write?" for a
  fencing/lease/heartbeat window measured in seconds to low minutes (III.1.5). Past a
  90-day cutoff there is no future question these rows could answer — no aggregate is
  lost by deleting them outright.
- **`purge_stale_terminal_execution_events(cutoff, batch_size)`** — deletes
  `execution_events` rows once both (a) `occurred_at < cutoff` and (b) the owning
  attempt is genuinely terminal (`succeeded`/`failed`/`cancelled` — the same three
  states `ExecutionState::is_terminal()` names). Deliberately **excludes**
  `lost`/`needs_operator`: both remain actionable/ambiguous per III.1.1 and are this
  same card's own observability targets — an attempt an operator might still requeue
  or investigate must never have its event history silently swept.
- **`execution_fleet_snapshot(now, event_window)`** — five independent read-only
  queries (no transaction needed): runner state counts, request state counts, stale
  lease count + oldest age, `needs_operator` count + oldest age, event-ingestion count
  in a trailing window.

**The card's own instruction flagged that a rollup table for `execution_events`
"probably does not exist," and it doesn't** — the execution domain has no equivalent
of `orch_events_daily`. I was told I may not add a migration. I chose the
**purge-only** path for `execution_events` (not the "leave it untouched" path),
because the practical unbounded-growth risk is real (every runner-reported tool
call/status event lands here) and the instruction explicitly sanctions delete-only as
long as it is *labeled honestly, not called a roll-up* — which this code and its doc
comments do throughout. **This is a deliberate trade this card is flagging, not
hiding**: real per-day/per-kind event counts are lost once purged. See "Schema/API/
contract change requested" below for the exact migration that removes this trade.

The six replay/bookkeeping tables needed no such judgment call — they never carried
aggregate value in the first place, so purging them is simply correct, not a
compromise.

### 2. Cancellable, joinable background tasks (not the orch precedent's shape)

`tack_orch::reconciler::spawn_retention_sweep` (the existing orch-domain precedent)
has **no cancellation signal at all** — dropping its `JoinHandle` is the only way to
stop it — and computes its cutoff from `Utc::now()` directly, making it untestable
against injected time. Both `spawn_execution_retention_sweep` and
`spawn_execution_health_watch` (this card) fix both for the execution domain:

- A `RetentionClock`/`ObservabilityClock` trait makes "now" injectable; production
  uses `SystemClock`, tests use a fixed/`Mutex`-backed fake.
- Each spawn function takes a `tokio::sync::watch::Receiver<bool>` stop signal, raced
  against the inter-tick sleep via `tokio::select!` and checked at the top of every
  loop iteration — mirroring `reconciler::spawn_one`'s own cancellation shape exactly
  (a purge batch or a snapshot query is already a short, bounded, independently-
  committed unit of work, so a stop signal is observed between ticks, never
  mid-transaction).
- Both are gated by their own `enabled: bool` — `false` returns `None` immediately
  without ever touching the store (same off-by-default-when-disabled contract as
  `spawn_reconcilers`/`spawn_retention_sweep`).

`crates/tack-api/src/execution_runtime.rs::ExecutionRuntime` wraps both into one
`start()`/`stop()` pair. **`stop()` genuinely joins**: it sends the stop signal, then
`.await`s each spawned `JoinHandle` to completion before returning — the load-bearing
difference from `OrchRuntime::stop`, whose own doc comment says it "does not block
waiting for the tasks to actually exit." This card's acceptance bar ("shutdown joins
task") required the stronger guarantee, so `ExecutionRuntime` could not simply reuse
`OrchRuntime`'s shape despite the superficial similarity.

### 3. Alerts: count > 0 is the whole condition, logged only on transition

`execution_observability::evaluate_alerts` is pure (no I/O): a stale lease or a
`needs_operator` request is *always* worth surfacing — both are, by definition
(III.1.1), states nothing else in the system resolves on its own. There is no
"acceptable number to stay quiet about" threshold. `spawn_execution_health_watch`
logs a `warn!` only on the **transition into** an alert condition (and an `info!` on
the transition back out), matching `reconciler.rs`'s own documented "logs backoff at
warn without spam" convention — a sustained stuck condition produces one warn on
onset, not one every tick, no matter how long it lasts. A `debug!`-level snapshot
line is emitted every tick regardless.

### 4. `server.rs` wiring (applied — conflict-free against this branch)

Two small, additive blocks. Exact diff for the integrator to re-apply after a rebase
if it doesn't merge cleanly:

```rust
// --- before `// Build router` / `let app = build_router(state);` ---
let execution_runtime = crate::execution_runtime::ExecutionRuntime::new();
execution_runtime
    .start(state.repo.clone(), (&config).into())
    .await;

// Build router
let app = build_router(state);
```

```rust
// --- immediately after `axum::serve(...).with_graceful_shutdown(...).await?;` ---
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

execution_runtime.stop().await;

info!("Server shut down gracefully");
Ok(())
```

Both blocks are self-contained: no `AppState` field was added (the runtime handle is
a local variable in `serve()`, not stored on `AppState` — `AppState` is defined in
`router.rs`, which this card must not edit, and nothing in the current HTTP surface
needs to toggle this at runtime the way `PUT /api/settings/orchestration` does for
`OrchRuntime`). `state.repo.clone()` is taken *before* `state` is moved into
`build_router(state)`, so ordering matters if this is re-applied by hand.

## Tests added and exact commands/results

- `cargo test -p tack-orch --lib execution_` — **12 passed, 0 failed** (fake-store
  unit tests, both modules): disabled-spawns-nothing (×2), enabled-calls-with-correct-
  cutoff-and-batch-size, shutdown-joins-and-no-further-purge, failing-purge-logged-
  and-retried-not-panicked (retention); no-alert/single-stale/single-needs-operator/
  both-independently, bounded-id-free-label-set, disabled-spawns-nothing, shutdown-
  joins-and-no-further-snapshot, transition-logging-does-not-crash (observability).
- `cargo test -p tack-db --test execution_retention_test` — **6 passed, 0 failed**:
  cross-cutoff purge correctness across all six replay tables; batch-bound proof (12
  rows / batch_size 5 → 3 batches, 0 remaining); terminal-only event purge (active
  attempt's events survive regardless of age; terminal attempt's stale event alone is
  gone); snapshot correctness (runner/request state counts, stale-lease detection
  excluding terminal attempts, `needs_operator` age, windowed event count) — every
  numeric assertion is an exact expected value, not a range or "some rows remain";
  two independent concurrency races (replays table, events table) against a
  file-backed database.
- `cargo test -p tack-orch --test execution_retention_prod_test` — **2 passed, 0
  failed**: the real spawned retention task purges a real stale row through the real
  `RepoExecutionRetentionStore`/`Repository`/SQLite file, with `stop()` proven to
  genuinely join (a row inserted *after* `.await`ing the handle is never purged); the
  real spawned health-watch task's real snapshot (captured via a spy wrapping the
  real store, not a fake) reports the real seeded stale lease and `needs_operator`
  row.
- `cargo test -p tack-api --lib execution_runtime` — **3 passed, 0 failed**: start-
  then-stop leaves nothing running and `stop()` returning at all is part of the proof
  (both tasks' `tokio::select!` loops must have observed the signal); disabled config
  starts no tasks; `start()` is idempotent while already running.
- `cargo build` (whole workspace) — clean.
- `cargo test --workspace` — **1157 passed, 0 failed, 6 ignored**. Wave 4's own
  accepted baseline was 1134 passed; 1134 + 23 = 1157 exactly (12 + 6 + 2 + 3 = 23 new
  tests above), confirming no other test's pass/fail count moved.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean workspace-wide; confirmed via `git diff --stat` /
  `cargo fmt --check`'s own file list that only files this card touched needed
  reformatting (`crates/tack-db/tests/execution_retention_test.rs`,
  `crates/tack-orch/src/execution_retention.rs`,
  `crates/tack-orch/tests/execution_retention_prod_test.rs` — formatted individually
  with `rustfmt`, not a blanket `cargo fmt` over the whole tree).
- `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed**, unmodified.
- `cargo test -p tack-orch --test runner_contract` — **18 passed, 0 failed**,
  unmodified (no fixture touched — this card has none to touch).

## Failure/adversarial case proved

- **`BEGIN IMMEDIATE` is load-bearing for both new purge methods, proved by reverting
  and watching it fail, per CLAUDE.md's own instruction — done twice, once per
  method:**
  - `purge_stale_execution_replays`: temporarily changed
    `self.pool().begin_with("BEGIN IMMEDIATE")` to `self.pool().begin()` at its call
    site; `concurrent_purges_never_deadlock_against_a_file_backed_database` failed
    immediately at iteration 0 with `error returned from database: (code: 5) database
    is locked`. Restored; the test passes consistently again (run afterward, see
    above).
  - `purge_stale_terminal_execution_events`: same revert-and-watch-fail cycle against
    `concurrent_event_purges_never_deadlock_against_a_file_backed_database` —
    identical `database is locked` failure at iteration 0, then a clean pass after
    restoring.
  - Both races run against a genuinely file-backed, WAL, `mode=rwc` database (the
    in-memory shared-cache pool would mask this — see the test file's own module doc,
    which cites `execution_repo_test.rs`'s own documented reasoning for the identical
    choice) and assert **exact** accounting (`left.rows_purged + right.rows_purged ==
    20`, table count `== 0` after), not merely "no error."
- **"Shutdown joins task" proved three separate times, each a real `.await` on the
  `JoinHandle`, not a signal-and-return:** `execution_retention.rs`'s
  `shutdown_joins_the_task_and_no_further_purge_happens_after`,
  `execution_observability.rs`'s equivalent, and — the strongest version —
  `execution_retention_prod_test.rs`'s production-runtime test, which inserts a fresh
  stale row *after* the join completes and asserts it is never purged (nothing left
  running to purge it).
- **Bounded batches proved with an observation, not a comment:**
  `purge_stale_execution_replays_respects_the_batch_bound` inserts 12 stale rows at
  `batch_size=5` and asserts `batches_run == 3` (5+5+2) and `rows_purged == 12` —
  the exact transaction count, not just "eventually all rows are gone."
- **The `execution_fleet_snapshot`'s stale-lease scoping is proved adversarially, not
  just positively:** the snapshot test seeds a *second* attempt whose lease is also
  expired but whose state is already terminal, and asserts `stale_lease_count == 1`
  (not 2) — proving the terminal-state exclusion is real, not vacuous.
- **The bounded-label-set guarantee is proved structurally:**
  `snapshot_label_set_is_bounded_and_id_free` populates every known runner/request
  state and asserts the map sizes stay at exactly 3 and 10 respectively, plus a
  shape-based check that no key looks UUID-like — this is the concrete proof behind
  the card's "no prompt/model contents in metric labels... never label by attempt id"
  charter.

## Schema/API/contract change requested from another owner

1. **`execution_events_daily` migration (recommended, not required for this card's
   own tests to pass — see "Behavior implemented" #1 for why purge-only was chosen
   instead for now).** Mirrors `orch_events`/`orch_events_daily`'s proven shape
   (`crates/tack-db/src/migrations.rs` migrations 025/026 and
   `repo/orch.rs::rollup_and_purge_orch_events`), deliberately **not** keyed by
   `attempt_id` (an unbounded, ever-growing dimension — the same problem the purge is
   meant to solve, just relocated) but by `(day, source, kind)`, mirroring how
   `orch_events_daily` itself deliberately drops per-item granularity in favor of a
   bounded, fleet-wide daily total:

   ```sql
   CREATE TABLE IF NOT EXISTS execution_events_daily (
       id TEXT PRIMARY KEY NOT NULL,
       day TEXT NOT NULL,
       source TEXT NOT NULL,
       kind TEXT NOT NULL,
       event_count INTEGER NOT NULL DEFAULT 0,
       created_at TEXT NOT NULL DEFAULT (datetime('now')),
       updated_at TEXT NOT NULL DEFAULT (datetime('now')),
       UNIQUE(day, source, kind)
   )
   ```
   ```sql
   CREATE INDEX IF NOT EXISTS idx_execution_events_daily_day ON execution_events_daily(day)
   ```

   Once this lands, `purge_stale_terminal_execution_events` should be replaced with a
   `rollup_and_purge_terminal_execution_events` that inserts/upserts the day/source/kind
   aggregate in the *same* `BEGIN IMMEDIATE` transaction as the delete, exactly
   mirroring `rollup_and_purge_orch_events`'s own atomicity argument (a crash between
   an aggregate write and its delete must be impossible, not merely bounded).

2. **No HTTP route exposes `execution_fleet_snapshot` yet.** This card was told not
   to touch `router.rs`/`openapi.rs`/`handlers/mod.rs`, so "stale lease and
   `needs_operator` are observable" is satisfied at the Rust level (a real, tested
   repository method + a real spawned background task that logs alerts from it) but
   **not yet at the HTTP/UI level**. A future card (or the wave integrator) gets a
   small, mechanical addition: a `GET /api/execution-fleet/health` (or similar)
   handler calling `state.repo.execution_fleet_snapshot(Utc::now(),
   Duration::hours(1))` and returning `ExecutionFleetSnapshotRow` as JSON — the query
   itself, its correctness, and its cardinality guarantee are already fully proven by
   this card's tests; only the route registration is missing. **This is the most
   important gap for F4 to know about — see "What F4 can rely on" below.**

## Known limitations or `not_measured` fields

- `execution_events_daily` does not exist — see request #1 above. Until it lands,
  90-day-old terminal-attempt event history is purged with no aggregate trace, by
  design and clearly labeled as such throughout the code (never called a "roll-up").
- No HTTP route surfaces `execution_fleet_snapshot` — see request #2. The frontend
  cannot yet render a stale-lease/`needs_operator` dashboard from a live endpoint;
  it *can* be built the moment that route lands, against already-tested repository
  logic.
- `events_ingested_in_window` is a coarse ingestion-rate signal (a single count in a
  caller-chosen trailing window), not a rate/derivative — sufficient for "is
  something still happening" but not for a throughput graph.
- Retention/health defaults are **on** (`TACK_EXECUTION_RETENTION_ENABLE`/
  `TACK_EXECUTION_HEALTH_ENABLE` both default `true`), a deliberate departure from
  `TACK_ORCH_ENABLE`'s off-by-default precedent — justified in `config.rs`'s own doc
  comments (no outbound calls, no new API surface, pure local-row hygiene) but
  flagged here explicitly in case a reviewer disagrees with the default direction.
- `ExecutionRuntime` is not stored on `AppState` and therefore cannot be toggled at
  runtime the way orchestration can via `PUT /api/settings/orchestration` — only a
  full restart changes its enabled state today. Adding a runtime toggle would need
  `router.rs`'s owner to add an `AppState` field, which this card does not do.

## Secrets/logging review

- Every SQL query added to `repo/execution.rs` selects only ids, states, and
  timestamps — no `prompt`, `agent_profile_snapshot`, `metadata`, or any other
  free-text/JSON-blob column is ever read by `purge_stale_execution_replays`,
  `purge_stale_terminal_execution_events`, or `execution_fleet_snapshot`.
- `ExecutionFleetSnapshot`'s only two "many-valued" fields
  (`runner_state_counts`/`request_state_counts`) are `BTreeMap<String, i64>` populated
  exclusively via `GROUP BY state` against a 3-value and a 10-value closed vocabulary
  — structurally incapable of being keyed by an id, and proved so in
  `snapshot_label_set_is_bounded_and_id_free`.
- No `tracing::*!` call anywhere in `execution_retention.rs`/`execution_observability.rs`
  ever includes a raw row, a JSON blob, or an id-shaped collection — every log field
  is a plain count, an `Option<i64>` age, or a fixed enum-like string ("execution
  retention sweep stopping", etc.). `debug!`'s `runner_state_counts =
  ?snapshot.runner_state_counts` logs the *whole bounded map* (3–13 entries, ever),
  which is the deliberate point: a bounded map is safe to log wholesale precisely
  because it can never grow.
- No credential, query string, or full environment value is read, held, or logged
  anywhere in this card's new code — none of it touches `agent_runners.credential_hash`,
  `credential_expires_at`, or any enrollment-token table's `token_hash` column.

## Safe merge order and likely conflicts

- This branch never touched `router.rs`, `openapi.rs`, `handlers/mod.rs`,
  `migrations.rs`, `repo/mod.rs`, `docs/openapi.json`, `schema.gen.ts`,
  `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`, root
  `Cargo.toml`/`Cargo.lock`, or any other card's handoff — no conflict expected there.
- `crates/tack-db/src/repo/execution.rs` was only *appended to* (three new methods
  and their supporting types added at the very end of the file, after
  `create_execution_decision`) — a future card that also appends to this file's end
  should expect an ordinary adjacent-addition merge, not a structural conflict. No
  existing method's body changed.
- `crates/tack-api/src/server.rs` is the one genuinely "high-traffic" file this card
  touched (per its own charter's warning). Both blocks are self-contained and placed
  at natural seams (immediately before `build_router`, immediately after
  `axum::serve(...).await?`) rather than interleaved with the orchestration-reconciler
  block above them — a concurrent card also editing `server.rs`'s boot sequence should
  see a clean textual merge unless it inserts *between* the same two lines this card
  did.
- `crates/tack-orch/src/lib.rs` and `crates/tack-api/src/lib.rs` each gained exactly
  one/two new `pub mod` line(s) with no reordering of existing ones — low conflict
  risk even against another card adding its own new module in parallel.
- `CLAUDE.md`'s config table gained 5 rows after the existing
  `TACK_ORCH_APPROVAL_TOKEN` row; a concurrent card also appending config rows there
  should merge cleanly as long as it doesn't insert at the exact same line.
- F4 (frontend) should branch after this card **and** after request #2 above lands
  (the HTTP route) to build any live stale-lease/`needs_operator` UI — today, F4 has
  nothing new to wire from this card beyond what already existed.

## What F4 (frontend) can rely on

**Not yet an HTTP-observable surface** — this is the most important thing for F4 to
know. This card delivers:

- A fully tested, correct `Repository::execution_fleet_snapshot(now, event_window)`
  returning exact runner/queue/lease/`needs_operator`/event counts (see "Tests
  added").
- A real background task (`spawn_execution_health_watch`, wired into `server.rs` via
  `ExecutionRuntime`, on by default) that logs `warn!`-level alerts server-side when a
  stale lease or `needs_operator` request exists, and `info!` when it clears.

**What does not exist yet**: any `GET` route returning this snapshot as JSON. F4
cannot build a live dashboard from this card alone — it needs request #2's small,
mechanical route addition first (the query and its correctness are already proven;
only wiring is missing). Until then, the only place a stale lease or `needs_operator`
request is visible is the server's own structured logs.

## Checklist

- [x] No unowned files touched (see "Files changed" — every touched file not newly
      created by this card is justified there; every forbidden file listed in the
      card's boundaries is confirmed untouched via `git status`/`git diff --stat`
      against base SHA `cbdd4a3`).
- [x] No live secret committed, logged, or reachable (see "Secrets/logging review").
- [x] No panic stub / `unimplemented!()` / fake success — the one genuinely-missing
      capability (a real `execution_events` roll-up table) is a named, typed gap with
      an exact requested migration, never a placeholder standing in for success; the
      HTTP-route gap is likewise named, not silently absent.
- [x] No blind retry — a failed purge or snapshot query is logged at `warn!` and
      retried on the *next scheduled tick* (bounded by `sweep_interval_secs`/
      `check_interval_secs`), never retried in a tight loop.
