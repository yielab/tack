# Regroup tack-api's orchestration test binaries

ADR 0064, rollout step 4, `tack-api`'s orchestration slice. Base: `develop` at `db113b8`.
Branch: `agent/regroup-api-orchestration`.

## Scope

Moved these 15 files into one new binary, `crates/tack-api/tests/orchestration.rs` +
`crates/tack-api/tests/orchestration/`:

```
orch_test.rs                     orch_agent_activity_test.rs   orch_approvals_test.rs
orch_broadcast_test.rs           orch_budget_policy_test.rs    orch_dispatch_test.rs
orch_reconciler_wiring_test.rs   orch_settings_test.rs         orch_terminal_status_test.rs
auto_dispatch_test.rs            auto_dispatch_gate_test.rs    sprint_dispatch_test.rs
g1_dual_dispatch_test.rs         templates_orchestration_test.rs  h8_fleet_membership_test.rs
```

No other file under `crates/tack-api/tests/` was touched, and `tests/common/mod.rs` was not
edited.

## Shape

Two levels of nesting, not one. Every one of the 15 files defines its own private
`req`/`body_json`/`app_with_state`/`create_project`/`orch_config`-style helpers, and those
names collide across files pulled into the same subject (e.g. six of the fifteen define their
own `app_with_state`). Flattening two colliding files into one module would either rename a
helper (out of scope — the card allows moving files, module declarations and use/path fixes,
not renaming test-support functions) or silently shadow one copy with another. Nesting each
original file as its own leaf module under a subject module keeps every private helper
module-scoped, exactly as it was crate-scoped before, with zero renames:

```
tests/orchestration.rs                  (mod common; + 6 subject mods)
tests/orchestration/
  control_plane.rs        control_plane/{resource.rs, settings.rs}
  dispatch.rs             dispatch/{item.rs, dual_scheduling.rs}
  auto_dispatch.rs        auto_dispatch/{hook.rs, gate.rs, sprint.rs}
  reporting.rs            reporting/{agent_activity.rs, budget_policy.rs, approvals.rs}
  reconciler.rs           reconciler/{wiring.rs, terminal_status.rs, broadcast.rs}
  fleet_templates.rs      fleet_templates/{fleet_membership.rs, templates.rs}
```

Groupings and why:

- **`control_plane`** (`orch_test.rs` → `resource.rs`, `orch_settings_test.rs` →
  `settings.rs`): administering the control-plane resource itself — CRUD, token
  write-only/never-echoed, `orch-link` save validation, `/api/fleet` reachability — plus the
  runtime enable/disable toggle. Both are "configure orchestration," not "use it."
- **`dispatch`** (`orch_dispatch_test.rs` → `item.rs`, `g1_dual_dispatch_test.rs` →
  `dual_scheduling.rs`): the single-item dispatch endpoint and the guard that keeps it from
  colliding with the neutral runner-v1 scheduling plane on the same item.
- **`auto_dispatch`** (`auto_dispatch_test.rs` → `hook.rs`, `auto_dispatch_gate_test.rs` →
  `gate.rs`, `sprint_dispatch_test.rs` → `sprint.rs`): dispatch triggered by something other
  than a direct call to the dispatch endpoint — the PATCH-driven hook, its enablement gate,
  and the sprint-level batch endpoint.
- **`reporting`** (`orch_agent_activity_test.rs` → `agent_activity.rs`,
  `orch_budget_policy_test.rs` → `budget_policy.rs`, `orch_approvals_test.rs` →
  `approvals.rs`): read-facing endpoints reporting what orchestration already did.
- **`reconciler`** (`orch_reconciler_wiring_test.rs` → `wiring.rs`,
  `orch_terminal_status_test.rs` → `terminal_status.rs`, `orch_broadcast_test.rs` →
  `broadcast.rs`): the reconciler/store seam exercised directly, never through HTTP —
  polling a real docket, terminal-state status-map application, and broadcast-on-change.
- **`fleet_templates`** (`h8_fleet_membership_test.rs` → `fleet_membership.rs`,
  `templates_orchestration_test.rs` → `templates.rs`): the two places orchestration's shape
  is written/validated outside the dispatch path — fleet roster membership feeding
  scheduling, and a template's `orchestration` block validated at save time.

Files named after board cards (`g1_`, `h8_`) were renamed to their subject
(`dual_scheduling.rs`, `fleet_membership.rs`); each file's own `//!` doc comment (already
free of card citations) moved with it unchanged.

## Private `setup`-equivalents: what moved to `common::`, what stayed private

No file in this group of 15 has a function literally named `setup`
(`grep -n '^\(async \)\?fn setup' <files>` — zero hits). What exists instead:

- **9 of the 15** already called `common::test_app()` / `common::test_app_with_config(...)`
  and declared `mod common;`. That declaration doesn't resolve the same way once the file is
  a nested submodule (a root test file's `mod common;` finds `tests/common.rs`; a submodule
  three levels deep would instead look for e.g.
  `tests/orchestration/control_plane/resource/common.rs`, which doesn't exist). Replaced with
  `use crate::common;` in each, which keeps every existing `common::test_app(...)` call site
  byte-for-byte unchanged: `resource.rs`, `agent_activity.rs`, `approvals.rs`,
  `budget_policy.rs`, `item.rs`, `sprint.rs`, `templates.rs` (7 files — `use crate::common;`
  kept and used).
- **2 of those 9** (`settings.rs`, `dual_scheduling.rs`, formerly `orch_settings_test.rs` and
  `g1_dual_dispatch_test.rs`) declared `mod common;` but never actually called anything
  through it — both build their own `app_with_state` inline instead. `mod common;` doesn't
  trigger `unused_imports`, so this was already dead before the move and nobody noticed;
  `use crate::common;` does trigger it, and `cargo clippy --workspace --all-targets -- -D
  warnings` failed on exactly these two until the now-provably-unused import was deleted.
  Not a test-body change — it's the same "path fix" every other file needed, landing on
  "delete" instead of "translate" because the thing it pointed at was never used.
- **6 files never used `common::` at all** (`orch_broadcast_test.rs`,
  `orch_reconciler_wiring_test.rs`, `orch_terminal_status_test.rs`, `auto_dispatch_test.rs`,
  `auto_dispatch_gate_test.rs`, `h8_fleet_membership_test.rs`) — each builds its own
  `Repository`/`AppState`/`sqlx::SqlitePool` directly because it needs something
  `common::test_app_with_config` doesn't hand back (a bare `Repository`, the full `AppState`,
  or the raw pool). No candidate for replacement.
- **Kept, deliberately, not touched**: at least six files define their own
  `async fn app_with_state(config: AppConfig) -> (Router, AppState[, Uuid])`, a near-clone of
  `common::test_app_with_config` that additionally hands back `AppState` so a test can reach
  the repo directly (e.g. to simulate the reconciler writing a health outcome) without going
  through HTTP. `resource.rs` even carries its own doc comment saying so ("Like
  `common::test_app_with_config`, but also hands back the `AppState`..."), and
  `dual_scheduling.rs` carries one explaining why it keeps a private copy instead of sharing
  one ("a private copy avoids coupling this file's tests to that file's helper signatures
  changing later"). None of these were unified into a shared `common::` helper: doing so
  would add a new function to `tests/common/mod.rs`, which the card reserves — three
  concurrent agents are regrouping other subjects in the same directory, and a shared-file
  edit is exactly the collision surface it asked to route around instead. Flagging it here
  as a real, later candidate: `common::test_app_with_state(config) -> (Router, AppState,
  Uuid)`, if a fourth file needing it shows up.

## Before/after counts

```
$ cargo nextest list --workspace -E 'package(tack-api)' | wc -l
469        # before (recorded before any file was moved) and after (recorded again post-move)

$ ls crates/tack-api/tests/*.rs | wc -l
36         # before
22         # after (drop of 14 — the 15 moved files minus the 1 new tests/orchestration.rs)
```

## Verification run

```
$ ./scripts/check-comments.sh
✓ no board archaeology in crates/

$ cargo fmt --all --check
(clean)

$ cargo clippy --workspace --all-targets -- -D warnings
(clean, after the two unused-import fixes above)

$ cargo nextest run --workspace
Summary [  17.400s] 1392 tests run: 1392 passed, 6 skipped
```

## Found, not fixed

- Five files outside this group have doc comments citing the old filenames by name and will
  go stale: `crates/tack-orch/src/adapters/legacy_bridge.rs` (cites `orch_dispatch_test.rs`,
  `orch_reconciler_wiring_test.rs`, `auto_dispatch_test.rs`, `sprint_dispatch_test.rs`,
  `orch_approvals_test.rs`, `g1_dual_dispatch_test.rs`), `crates/tack-api/src/orch_runtime.rs`
  (cites `orch_settings_test.rs`), and three sibling test files —
  `crates/tack-api/tests/wip_limit_race_test.rs`, `crates/tack-api/tests/provisioning_test.rs`,
  `crates/tack-api/tests/economics_test.rs` — that cite `orch_dispatch_test.rs`,
  `sprint_dispatch_test.rs`, `orch_approvals_test.rs`, `templates_orchestration_test.rs` or
  `orch_budget_policy_test.rs` in a "mirrors X's helpers" comment. None of these five files
  are in this card's ownership list (and three are plausibly owned by the two concurrent
  regrouping agents), so they were left alone rather than edited out of scope. Whoever lands
  last across all the concurrent regrouping cards should grep for the old filenames across
  `crates/` once everything merges.
- `.config/nextest.toml`'s only per-binary override targets `binary(e6_scheduler_e2e_test)`
  (a `tack-cli` binary, unrelated to this group) — confirmed it needed no change.
