# ADR 0060: The Docket control plane stays a maintained, optional legacy bridge

- Status: accepted
- Date: 2026-08-31
- Supersedes: nothing. **Reaffirms** ADR 0050's "Docket is optional and may exist only
  as `legacy-docket`" with the measured evidence ADR 0050 did not have at the time, and
  formalizes the "Decision: maintain" already argued informally in
  `crates/tack-orch/src/adapters/legacy_bridge.rs`'s module doc.
- Contract: `docs/contracts/runner-v1/` — unchanged by this decision.

## Context

Tack integrated against exactly one agent-fleet backend, Docket: a `ControlPlane`
trait, a reconciler/health machine, a Docket adapter, a fleet-wide Approvals inbox, a
`ControlPlanesManager`, and 10 dedicated tables. ADR 0050 then made the native
pull-based runner-v1 execution domain (`execution_requests`/`execution_attempts`, the
protocol `tack-runner` speaks) the plan of record, and named Docket optional —
`legacy-docket`. Both models still coexist in the schema, in `tack-orch`, and in the
UI, and a reader who lands on either without that history cannot tell which is current.
This ADR answers the question ADR 0050 left informal: given what Docket now measurably
costs, should Tack keep it as-is, gate it behind a default-off build flag, or schedule
its deletion.

## Measurement

Every number below is from a command run against this tree, not an estimate.

**Schema.** A naive `grep -c 'orch_'` over `crates/tack-db/src/migrations.rs` counts
11 — but that count is wrong, because it includes `orch_runs_new` and
`orch_approvals_new`, the transient staging-table names migrations 037/038 use during
their rebuild-in-place (`DROP ... IF EXISTS`, copy, `DROP TABLE orch_runs`,
`ALTER TABLE orch_runs_new RENAME TO orch_runs`); those names never exist in a
fully-migrated database. The real count comes from migrating a fresh in-memory database
and reading `sqlite_master`:

```
sqlite::memory:  →  tack_db::migrations::run_all()  →
  SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'
```

Result: **48 tables total; 10 are Docket-specific** — `control_planes`, `orch_links`,
`orch_tasks`, `orch_runs`, `orch_events`, `orch_approvals`, `orch_metrics`,
`orch_events_daily`, `orch_metrics_daily`, `orch_trace_cursors` (20.8% of the schema).
The runner-v1 execution domain owns a disjoint 17: `agent_fleets`, `agent_runners`,
`agent_fleet_members`, `agent_profiles`, `model_profiles`, `execution_requests`,
`execution_attempts`, `execution_events`, `execution_artifacts`, `execution_decisions`,
`agent_enrollment_tokens`, and five `execution_*_replays`/`execution_recovery_audits`
idempotency tables. Every Docket table's only inbound FK root is `control_planes`; none
of the 17 runner-v1 tables reference or are referenced by any Docket table.

**Backend code**, split by reading each file's module doc rather than assumed from its
directory (`tack-orch` is not all legacy — the runner-v1 execution domain lives there
too, see `crates/tack-orch/src/execution*`, `scheduler/`, `model_policy/`,
`usage_provenance.rs`, none of which is Docket):

| Crate | Docket-specific (prod) | Docket-specific (tests) | Runner-v1 (prod, for scale) |
|---|---:|---:|---:|
| `tack-orch` | 6,690 (`adapters/**` 1,774 + `lib.rs` 1,198 + `reconciler.rs` 3,718) | 2,916 (`docket_*_test.rs` × 3) | 5,013 |
| `tack-api` | 7,177 (`handlers/orch.rs` 2,829 + `handlers/provisioning.rs` 652 + `dispatcher.rs` 702 + `orch_store.rs` 801 + `orch_runtime.rs` 582 + `sprint_dispatch.rs` 576 + `handlers/economics.rs` 1,035) | 4,289 (9 `orch_*_test.rs` files) | not measured here (out of scope) |
| `tack-db` | 0 new files (schema only, see above) | 3,062 (`orch_metrics_test.rs`, `orch_migrations_test.rs`, `orch_repo_test.rs`) | — |

`tack-orch/src` totals 11,703 lines (`find crates/tack-orch/src -name '*.rs' \| xargs wc
-l`); with its `tests/` directory the crate is 18,799 — the "~19k lines" this card's
brief cites, confirmed. Of that, **57%** (6,690/11,703) is Docket; the rest is the
load-bearing runner-v1 domain this ADR does not touch.

`crates/tack-orch/src/adapters/**` — the one directory this card owns for gating — is
1,774 lines: `docket.rs` 779, `github_actions.rs` 226 (a compile-only, never-registered
second-adapter stub kept only so a second `ControlPlane` impl typechecks),
`legacy_bridge.rs` 249, `prometheus.rs` 315 (a text-exposition parser consumed only by
`docket.rs`'s `metrics()`), `registry.rs` 181, `mod.rs` 24. Grepping every `.rs` file in
`crates/` confirms `DocketAdapter`/`adapters::docket` is referenced only from inside
`adapters/` itself; every caller in `tack-api` (`orch_store.rs`, `dispatcher.rs`,
`handlers/provisioning.rs`, `handlers/orch.rs`) goes through
`adapters::registry::build` and never imports `DocketAdapter` directly — the registry
really is the one choke point its own module doc claims.

**Frontend.** Docket has six feature directories, not the two ("Approvals inbox",
"ControlPlanesManager") this card's context paragraph names — the other four
(`economics`, `provisioning`, and the *project-level* `settings/orchestration`) were
found by tracing which files import the same `isOrchDisabled`/`orchAvailable` gating
helper and which backend routes they call:

| Directory | Confirmed by |
|---|---|
| `features/fleet/{FleetPage,FleetRow,HealthChip,api,format}.*` (excl. `runnerFleet/`) | `api.ts`'s own header: "Part II's per-project Docket control-plane roster"; `runnerFleet/FleetsPanel.tsx`'s header explicitly disclaims importing from it |
| `features/approvals/**` | fleet-wide Docket approvals inbox named in this card's own context |
| `features/economics/**` | `handlers/economics.rs` reads `orch_tasks`/`orch_events` exclusively, gated by `orch_routes()`'s `require_orch_enabled` layer — no `execution_requests`/`execution_attempts` reference anywhere in that handler |
| `features/provisioning/**` | `ProvisioningWizard.tsx` literally instructs the operator to "Register a control plane (e.g. a running docket instance)... via POST /api/control-planes" |
| `features/settings/orchestration/**` (project tab) | `OrchestrationPanel.tsx` imports `orchestrationApi`/`OrchLink`; `BudgetPanel.tsx`'s own comment: "docket's budget..." |
| `features/settings/orchestrationSettings/**` (global section) | hosts `ControlPlanesManager.tsx`, named in this card's context |

`find <those 6 dirs> -maxdepth 1 -type f -name '*.ts*' | xargs wc -l`: **7,330 lines
across 46 files.** `frontend/src/shared/orch/{capabilities.ts,CapabilityNote.tsx}` (395
lines) looks Docket-scoped by name but is **not** — `shared/execution/api.ts`,
`shared/execution/capabilities.ts`, `shared/agentActivity/useAgentActivityMap.ts`, and
`shared/dispatch/DispatchCardMenu.tsx` (all runner-v1) import it too; it is a genuinely
shared capability-gate primitive and is excluded from every number above.

The Sidebar (`shared/ui/Sidebar.tsx`) always renders Fleet, Approvals, Economics and
Provision nav entries; only Fleet carries a small "Off" badge when the orchestration
probe reports disabled. Nothing hides the other three today, `TACK_ORCH_ENABLE` unset
or not — the confusion this ADR was asked to resolve is real and is a **UI** problem,
not a binary-size or schema problem (see Decision).

**Config.** Four `TACK_ORCH_*` variables in `docs/CONFIG.md` (lines 34-37):
`TACK_ORCH_ENABLE` (default `false`), `TACK_ORCH_POLL_SECS`,
`TACK_ORCH_EVENT_RETENTION_DAYS`, `TACK_ORCH_APPROVAL_TOKEN`. No row changes are needed
by this ADR — see Decision.

**Binary size — the adapter compiled out.** `crates/tack-orch/src/adapters/mod.rs`'s
`docket`/`github_actions`/`prometheus` module declarations and `registry.rs`'s
`"docket"` match arm were commented out, `cargo build --release -p tack-cli` (this
repo's `[profile.release]`: `lto = true`, `codegen-units = 1`, `opt-level = "z"`,
`strip = true`) was run before and after, and the change was reverted — not committed;
`git status` on this branch is clean of it.

| Build | `target/release/tack` size |
|---|---:|
| Baseline (Docket compiled in) | 18,622,360 bytes |
| Docket adapter compiled out | 18,479,256 bytes |
| **Delta** | **143,104 bytes ≈ 140 KiB ≈ 0.77%** |

Both builds omit `--features embed-spa` (no `frontend/dist/` was built for this
measurement, isolating the Rust-only delta from frontend bundle size, which is not
separately measured here — `not_measured`). The delta excludes `reconciler.rs` (3,718
lines, the single largest Docket file) and `lib.rs`'s `ControlPlane` trait/DTOs (1,198
lines): neither lives under `adapters/**`, so neither is in this card's gating
ownership, and both stay compiled in under either build. `reqwest` — usually a Rust
binary's largest single dependency once TLS is pulled in — is **not** removable by this
gate either: `crates/tack-orch/Cargo.toml`'s own comment says it is "reused, not
duplicated" from `tack-api`'s webhook delivery code, and `tack-api/src/{webhook.rs,
github_sync.rs, handlers/import_linear.rs, handlers/import_github.rs}` all depend on it
unconditionally. The measured 0.77% is consistent with what remains once TLS is
excluded from the comparison: struct/enum definitions, JSON (de)serialization glue, and
a handful of HTTP call sites — real code, but not code whose absence a user would feel.

**Release packaging (found while measuring, not anticipated going in).** `Makefile`
line 11, `docs/DEPLOYMENT-GUIDE.md` lines 57/65/76, and
`.github/workflows/release.yml` line 149 all hard-code
`cargo build --release ... --features embed-spa` with **no other feature flag**. A
default-off Docket cargo feature, exactly as this card's "gate" option specifies,
would silently drop Docket from the next official release binary unless all three of
those (none owned by this card, none owned by `V-B2` at all) were updated in the same
change.

## Decision

**Keep Docket as a maintained, optional legacy bridge — no code change.** This
reaffirms ADR 0050's "Docket is optional and may exist only as `legacy-docket`" and
`adapters/legacy_bridge.rs`'s own "Decision: maintain", now with the measurements above
behind it rather than only the reasoning that module doc already gave (live-verified
regression coverage across `docket_adapter_test.rs`/`docket_wire_contract_test.rs`/
`docket_tick_contract_test.rs` plus nine `orch_*_test.rs` files in `tack-api` and three
in `tack-db`; DAG-ordered sprint dispatch and a `pre_input` guardrail-policy engine that
runner-v1 does not replace; `TACK_ORCH_ENABLE` already off by default at the
infrastructure level — unset, the reconciler never spawns and every `orch_*`/`/api/fleet`
route 404s).

**Rejected: gate behind a default-off cargo feature + config flag.**
Costed from the measurements above:
- The only code this card owns for gating (`adapters/**`) is 1,774 of the 6,690
  Docket-specific lines in `tack-orch` — the other 4,916 (`reconciler.rs` + `lib.rs`'s
  `ControlPlane` trait/DTOs) are out of this card's ownership and would stay compiled
  in regardless, so the *reachable* binary saving is capped near the measured 143,104
  bytes (0.77%) — not the much larger number a reader might expect from "6,690 lines
  gated out."
- Meeting this card's own acceptance bar — "a default build exposes no docket concept
  in the UI" — requires touching four frontend feature directories
  (`economics`, `provisioning`, project-level `settings/orchestration`) this card's
  context paragraph never named, plus `Sidebar.tsx` and `app/routes.tsx`, neither of
  which is "the control-plane and approvals UI routes." A gate that stops at the two
  named surfaces leaves Economics and Provision fully visible by default — worse than
  no gate, because it would read as solved when it is not.
- A **default-off** cargo feature (this card's own spec) silently drops Docket from the
  next official release unless `Makefile`, `docs/DEPLOYMENT-GUIDE.md`, and
  `.github/workflows/release.yml` are updated in the same change — three files this
  card does not own, making a same-PR fix impossible without exceeding scope, and a
  follow-up PR a window for exactly the silent regression this ADR should not create.
- None of this is disqualifying on its own; together they mean "gate" is a
  multi-directory, multi-crate, CI-touching change disguised by its context paragraph as
  a two-file one. It is better scoped as its own future card that owns the full
  surface this measurement exposed — see Consequences.

**Rejected: schedule deletion.** `adapters/legacy_bridge.rs`'s own module doc already
evaluated and rejected "export" (rewriting Docket rows into the `execution_requests`/
`execution_attempts` shape): Docket's own `DocketAdapter::capabilities()` reports
`cancel: false`, `artifacts: false`, `model_selection: Unsupported`, and an
`execution_attempts` row asserts fields (fencing token, isolated workspace identity, a
capability snapshot) Docket's wire protocol has no source data for — forcing an export
would mean inventing values for fields this codebase's own rules require to be
measured or explicitly `not_measured`. Deletion proper would need, at minimum: a
migration plan for the 10 tables above (currently zero rows in most installs, since
`TACK_ORCH_ENABLE` defaults off, but no migration may assume that of an installed
`tack.db`); removal of 6,690 `tack-orch` + 7,177 `tack-api` lines and their 10,267 lines
of regression tests; removal of the 7,330-line, six-directory frontend surface; and
updating every one of the 234 doc-comment citations of `TODO.md` section numbers
project-wide that this repo's own `CLAUDE.md` already flags as a reason deletion here
is not a `rm`. Nothing measured above shows Docket costing enough — in schema surface
(21%, cleanly FK-isolated), binary size (0.77%), or maintenance risk (already optional,
already tested) — to justify that migration now, against real, live-verified capability
(DAG dispatch, guardrails) runner-v1 does not yet have.

## Consequences

- No code, schema, or `docs/CONFIG.md` row changes ship with this ADR. `git diff` on
  `crates/tack-db/src/migrations.rs` is empty, as required.
- The "two fleet concepts" confusion this card was raised to resolve is **real** and is
  **not** solved by this decision — it was never a binary-size or schema problem; it is
  entirely a UI-visibility problem (Sidebar always renders Fleet/Approvals/Economics/
  Provision; only Fleet gets an "Off" badge, and even that doesn't hide the page).
  **Recommended follow-up**, not executed here: a card that owns the full six-directory
  frontend surface plus `Sidebar.tsx` and `app/routes.tsx`, extending the *existing*
  `orchAvailable()`/`isOrchDisabled()` runtime probe (already wired into every one of
  these pages) so a default `TACK_ORCH_ENABLE=false` install hides all four nav entries
  and their routes, not just badges one of them. This is materially cheaper and lower-risk
  than a new build-time flag: no second frontend build, no Rust `cfg` feature, no
  `Makefile`/CI change, and it uses a pattern already proven in this codebase rather than
  introducing one.
- `LEGACY_DOCKET_COMPATIBILITY_LABEL` (`adapters/legacy_bridge.rs`) remains the one
  stable, quotable string for this decision; this ADR is now the second place (after
  that module doc) it is written down, and the more visible one for future contributors
  and audits.
- If a future measurement shows Docket's cost rising — new tables, a widening gap
  between its test suite and its actual use, or a security/maintenance burden distinct
  from raw size — this decision should be revisited with fresh numbers, not with this
  ADR's numbers re-cited unchanged.
