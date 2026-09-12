# III-G1 handoff

- **Base SHA / branch / final SHA:** base `5c6842f` (Wave 5 accepted, per the Part III
  status board); branch `agent/iii-g1-docket-bridge`; final SHA recorded at commit time
  (see `git log` on this branch — this handoff is written before the commit that
  carries it, per the card's workflow).

- **Files changed (must equal ownership list):**
  - `crates/tack-orch/src/adapters/legacy_bridge.rs` (new) — the compatibility
    decision, label/policy, provider-scoped id, and the pure `SchedulingOwner`
    decision function, with unit tests.
  - `crates/tack-orch/src/adapters/mod.rs` — one line declaring the new module.
  - `crates/tack-api/src/dispatcher.rs` — the legacy Docket dispatch write path (owned
    per the Wave-6 ownership table's "existing `orch_*`, Docket adapter/reconciler"
    row): added the one-scheduling-owner guard before any HTTP call to docket.
  - `crates/tack-db/src/repo/orch.rs` — three new functions:
    `has_active_execution_request_for_item` (read-only, dual-dispatch guard),
    `reconcile_stale_orch_tasks`, `reconcile_stale_orch_approvals` (local-only
    staleness sweeps).
  - `crates/tack-api/tests/g1_dual_dispatch_test.rs` (new) — collision tests across
    the two planes.
  - `crates/tack-db/tests/g1_stale_reconcile_test.rs` (new) — stale-row reconciliation
    tests.
  - This handoff.

  No `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `openapi.rs`,
  generated schema, `TODO.md`, or `.github/workflows/ci.yml` edits. No new migration
  numbers. `handlers/orch.rs`/`sprint_dispatch.rs`/`handlers/items.rs` (the three other
  files that exhaustively match `DispatchOutcome`) were **not** touched — the guard
  deliberately reuses the existing `ApiError::Conflict` shape instead of adding a new
  `DispatchOutcome` variant, specifically to avoid forcing edits into those unowned
  files. See "Behavior implemented" below.

- **Contract fixtures consumed:** none. No `docs/contracts/runner-v1/` fixture was
  read, added, or edited — this card's guard reads `execution_requests.state` as a
  plain string (`succeeded`/`failed`/`cancelled` as the terminal set, matching
  `tack_orch::execution::types::ExecutionState::is_terminal` by value, not by
  importing or depending on that type), never a runner-v1 wire payload.

## Decision: maintain (not export, not deprecate)

Full evidence and reasoning is in `crates/tack-orch/src/adapters/legacy_bridge.rs`'s
module doc ("Decision: maintain" section) — summarized here:

- The bridge is live-verified against a real `docket serve` (card V1), wired into
  auto-dispatch, sprint DAG-ordered dispatch, an approvals inbox, agent-fleet status,
  and economics — none of which runner-v1 replaces today.
- It carries substantial regression coverage (`docket_adapter_test.rs`,
  `docket_wire_contract_test.rs`, `docket_tick_contract_test.rs`, plus a dozen more in
  `tack-api`/`tack-db`) that "deprecate" would either delete or need to reproduce.
- "Export" (migrate `orch_tasks` rows into `execution_attempts`) was considered and
  rejected: docket's own capability snapshot (`cancel: false`, `artifacts: false`,
  `model_selection: Unsupported`) means an `execution_attempts` row built from a
  Docket-origin task would have to invent values for fields the runner-v1 contract
  requires to be measured or explicitly typed-absent — a structural-zero violation of
  this cycle's own rules, not a real export.
- `TACK_ORCH_ENABLE` already makes Docket infrastructurally optional (off by default,
  reconciler never spawns, `orch_*` routes 409 `orchestration_disabled` when unset).

**Explicit compatibility label:** `legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL =
"legacy-docket:maintained-bridge-v1"`, with a paired human-readable
`LEGACY_DOCKET_COMPATIBILITY_POLICY` string. Neither is wired into an HTTP response by
this card (see "Schema/API/contract change requested" below).

## Behavior implemented

**Dual-dispatch prevention (one direction of two).** `tack_db::repo::orch::Repository::
has_active_execution_request_for_item(item_id)` is a read-only query against
`execution_requests` — active means "state not in `{succeeded, failed, cancelled}`",
the same set `ExecutionState::is_terminal` defines, so `lost`/`needs_operator` both
still count as active (neither is safely redispatchable without operator/recovery
action). `tack_api::dispatcher::dispatch_item` calls this before any HTTP call to
docket and before the existing `orch_tasks` idempotency read; if true, it returns
`Err(ApiError::Conflict(...))` — the same shape the function already returns for its
concurrent-dispatch lock case, chosen specifically so **no other file's exhaustive
`DispatchOutcome` match needed editing**. Proven with a real router test that reverts
the guard and shows the mocked-but-otherwise-successful docket dispatch would have
gone through (see "Tests added" below).

**The reverse direction is not implemented — see "Schema/API/contract change
requested" below.** `tack-api::handlers::executions::create_execution` (`POST
/api/executions`) belongs to another card's file ownership and has no knowledge of
`orch_tasks`. This is proven, not asserted, by
`g1_dual_dispatch_test.rs::creating_a_runner_v1_request_does_not_yet_check_for_an_active_docket_task`.

**Stale row reconciliation.** `reconcile_stale_orch_tasks(stale_before)` marks
`orch_tasks` rows `remote_status = 'stale'` when they are currently "active"
(`pending`/`running`/`waiting_approval` — `dispatcher::ACTIVE_TASK_STATUSES`'s exact
set), `dispatched_at` predates the cutoff, and the item's linked control plane has
been `unreachable` with `last_seen_at` also predating the cutoff — i.e. a sustained
outage, not a plane mid-recovery. `'stale'` is deliberately outside
`ACTIVE_TASK_STATUSES`, so a swept row is immediately redispatchable through the
existing "anything not active is terminal, redispatch is safe" rule — no separate
unblock logic was needed. `reconcile_stale_orch_approvals(stale_before)` does the
same for `orch_approvals`, moving `pending` → `expired`. **Both are local-only — no
HTTP call to docket** — specifically so they cannot perturb
`docket_tick_contract_test.rs`'s pinned per-tick request sequence (confirmed: that
test, and `docket_wire_contract_test.rs`/`docket_adapter_test.rs`, are unmodified and
still green — 55 tests total, unchanged pass count).

**Neither sweep is wired to a scheduled task by this card.** `server.rs` is not named
in this card's ownership, and this follows the exact precedent card B3 set for
`spawn_retention_sweep` (built, tested, "not yet spawned from `server.rs`," the exact
one-block addition documented for the integrator). See "Schema/API/contract change
requested" for the wiring this needs.

**Provider-scoped ids / normalized-attempt projection.** `legacy_bridge::
provider_scoped_task_id` namespaces a docket `remote_task_id` as
`docket:<remote_task_id>`. `LegacyAttemptProjection` is a read-only, in-memory mapping
from `OrchTask` to `{provider_scoped_id, item_id, remote_status, scheduling_owner:
LegacyDocket}` — a presentation shape for a future operator surface, not a write into
`execution_attempts` and not consulted by any scheduler.

## Tests added and exact commands/results

```
cargo test --workspace
  1304 passed, 0 failed (was 1289 at Wave 5 close; +15 this card: 5 legacy_bridge unit
  tests, 4 g1_dual_dispatch_test, 6 g1_stale_reconcile_test)
cargo clippy --workspace --all-targets -- -D warnings   clean
cargo fmt --check                                       clean
cargo test -p tack-orch --test runner_contract           18/18 (46 fixtures byte-pinned, unchanged)
cargo test -p tack-api --test wave2_gate                 5/5
cargo test -p tack-api --test openapi_contract            5/5, no drift
cargo test -p tack-orch --test docket_adapter_test \
  --test docket_tick_contract_test \
  --test docket_wire_contract_test                       37 + 5 + 13 = 55/55, unchanged
                                                           (the "legacy golden test" —
                                                           confirmed byte-identical
                                                           behavior, no fixture edits)
```

New test files: `crates/tack-orch/src/adapters/legacy_bridge.rs` (5 `#[cfg(test)]`
unit tests, in-module), `crates/tack-api/tests/g1_dual_dispatch_test.rs` (4 tests),
`crates/tack-db/tests/g1_stale_reconcile_test.rs` (6 tests).

## Failure/adversarial case proved

Each guard below was proved load-bearing by reverting it and watching the test fail,
then restoring it and re-confirming green:

- **Dual-dispatch guard** (`dispatcher.rs`): temporarily replaced the
  `has_active_execution_request_for_item` check with `if false { ... }` (dead code,
  guard never fires). `dispatch_refuses_when_item_has_active_runner_v1_request` then
  failed with `left: 200, right: 409` — the item was dispatched to the (mocked,
  intentionally-succeeding) docket server and a real `orch_tasks` row was created,
  proving the test does not merely tautologically 409 for an unrelated reason.
  Restored; green again.
- **Stale-sweep plane-health selectivity** (`repo/orch.rs`): temporarily removed
  `cp.health = 'unreachable' AND` from `reconcile_stale_orch_tasks`'s WHERE clause.
  `task_on_healthy_plane_is_never_marked_stale` (whose fixture uses a synthetic
  `health = 'healthy'` + old `last_seen_at` combination specifically to isolate this
  column from the `last_seen_at` cutoff) failed with `left: 1, right: 0` — the healthy
  plane's task was wrongly swept. Restored; green again.

## Schema/API/contract change requested from another owner

1. **Mirror dual-dispatch guard in `tack-api::handlers::executions::
   create_execution`.** Before creating a new `execution_requests` row, check whether
   the item has an active `orch_tasks` row (`is_active_task_status`,
   `dispatcher::ACTIVE_TASK_STATUSES`) and refuse (409, matching this card's shape) if
   so. Owned by whichever card owns `handlers/executions.rs`
   (`router.rs`/`handlers/mod.rs`'s C5-then-E6/F4 lineage, per `TODO.md`'s III.3
   table). Proven as a real, currently-reachable gap by
   `g1_dual_dispatch_test.rs::creating_a_runner_v1_request_does_not_yet_check_for_an_active_docket_task`
   — that test asserts today's behavior (`201`/`200` succeeds) and is written to fail
   loudly (wrong status code) the moment this gap closes, at which point it should be
   updated to assert the new refusal rather than deleted.
2. **Wire `reconcile_stale_orch_tasks`/`reconcile_stale_orch_approvals` into a
   scheduled task at boot**, mirroring `spawn_retention_sweep`'s own not-yet-wired
   precedent (card B3) — a background loop in `server.rs` (not owned by this card)
   calling both functions on an interval, gated the same way `TACK_ORCH_ENABLE`
   already gates the reconciler (no plane data exists to reconcile when orch is
   disabled, so this is naturally a no-op then, but should still be explicit).
   Suggested config, also requesting `config.rs` (not owned by this card): a
   `TACK_ORCH_STALE_RECONCILE_DAYS` (default: reuse `TACK_ORCH_EVENT_RETENTION_DAYS`'s
   default of 90, or a shorter dedicated default — operator judgment) threshold and an
   interval, analogous to `TACK_EXECUTION_RETENTION_INTERVAL_SECS`'s shape.
3. **Surface `LEGACY_DOCKET_COMPATIBILITY_LABEL`/`_POLICY` in an API response** — no
   current `handlers/orch.rs` route returns a compatibility-label field. A natural
   home is `GET /api/control-planes/{id}` or `GET /api/fleet`'s response envelope.
   Owned by whoever next touches `handlers/orch.rs`'s response DTOs.
4. **III-G3 (operator docs)** should quote `LEGACY_DOCKET_COMPATIBILITY_POLICY`
   verbatim rather than re-derive Docket's compatibility story from source.

## Known limitations or `not_measured` fields

- The dual-dispatch guard is one-directional (runner-v1 → blocks Docket). The reverse
  is a documented, proven-open gap (request 1 above), not an oversight — see
  `legacy_bridge.rs`'s "One scheduling owner" section for why this asymmetry was
  judged acceptable for this card's file ownership rather than papered over.
- The staleness sweeps are built and tested but not wired to run automatically (request
  2 above) — same posture Wave 2's `spawn_retention_sweep` shipped in before its
  integrator wired it.
- `'stale'`/`'expired'` are new string values in `orch_tasks.remote_status` /
  `orch_approvals.state` — both columns already store arbitrary, unvalidated
  docket-origin strings by design (`repo/orch.rs`'s own module doc, "every
  remote-state string column stores whatever docket sent, unvalidated"), so this adds
  a Tack-originated value to an already-open string space rather than requiring a
  schema change.
- `LegacyAttemptProjection`/`provider_scoped_task_id` are built and unit-tested but not
  yet consulted by any handler — no route renders them today (see request 3).

## Secrets/logging review

No new logging was added in this card's changes beyond the existing `#[instrument]`
macros already on `Repository` methods (which log only ids/counts, matching this
repo's existing discipline — no docket token, no item title/description, no prompt
content anywhere in the new code). The dual-dispatch guard's `ApiError::Conflict`
message includes only `item_id` (a UUID). No new secret, credential, or token path was
touched — `has_active_execution_request_for_item`/`reconcile_stale_orch_*` read/write
only `state`/`remote_status`/timestamps, never `execution_requests.environment` (which
is where secret references would live) or any docket token column.

## Safe merge order and likely conflicts

- No conflicts expected with G2/G3/G4 (adversarial tests, docs, CI — disjoint files).
- Should merge cleanly onto the Wave 6 integration branch alongside sibling cards;
  this card's only shared-adjacent file is `crates/tack-orch/src/adapters/mod.rs`
  (one new `pub mod` line, low collision risk).
- **Recommend merging G1 before any card that touches `handlers/executions.rs`**, so
  that card's owner sees this handoff's "Schema/API/contract change requested" item 1
  before deciding whether to implement the mirror guard in the same pass.

## Checklist

- [x] No unowned files edited (`router.rs`, `openapi.rs`, `migrations.rs`,
      `handlers/executions.rs`, `handlers/orch.rs`, `sprint_dispatch.rs`,
      `handlers/items.rs`, `TODO.md`, `.github/workflows/ci.yml`, contract fixtures —
      none touched).
- [x] No live secret in any test or fixture (wiremock only; no real docket instance).
- [x] No panic stub / `unimplemented!()` / hidden fake success anywhere in this diff.
- [x] No blind retry — the dual-dispatch guard fails closed (`Err`, nothing written);
      the stale sweep is a deterministic, idempotent `UPDATE ... WHERE`, safely
      re-runnable every tick with no double-application risk (a row already `'stale'`
      no longer matches the `WHERE remote_status IN (...)` clause).

## Proposed status-board row text (G5 applies, does not self-apply)

> 6 — Legacy bridge and release | III-G1 · G2 · G3 · G4 · G5 | 57 | **III-G1
> complete** — branch `agent/iii-g1-docket-bridge`, base `5c6842f`. Decision:
> **maintain** the legacy Docket bridge (evidence: live-verified, wired into
> auto-dispatch/sprint-dispatch/approvals/economics, `orch_*` untouched for four
> waves as designated). Added the one-scheduling-owner dual-dispatch guard
> (`Repository::has_active_execution_request_for_item`, `dispatcher.rs`) —
> runner-v1-active blocks legacy Docket dispatch, proven load-bearing; the mirror
> direction (Docket-active blocking a new `execution_requests` row) is a
> documented, proven-open gap requested from the `handlers/executions.rs` owner.
> Added local-only stale-row reconciliation for `orch_tasks`/`orch_approvals`
> (`reconcile_stale_orch_tasks`/`_approvals`), not yet wired to a scheduled task
> (same not-yet-spawned posture B3's retention sweep shipped in). Added an explicit
> compatibility label (`legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL`), not yet
> surfaced over HTTP. 1304 workspace tests (was 1289; +15), clippy/fmt clean,
> `runner_contract` 18/18 unchanged, `wave2_gate` 5/5, `openapi_contract` 5/5
> no-drift, and all three docket golden tests (`docket_adapter_test`,
> `docket_tick_contract_test`, `docket_wire_contract_test`, 55 tests total)
> unchanged and green. See `III-G1.md` for the full evidence, the three requested
> follow-ups, and the proven-load-bearing reverts.
