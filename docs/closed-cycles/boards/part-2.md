> Archived, no-longer-current board. The live board is `TODO.md` — see its "Which board is live" table.

# Part II — Agnostic Control Plane (Phases 39–49)

Executable task board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Agnostic Control Plane*.
The full plan, with a verification command per item and the reasoning behind every
decision, is **[docs/plans/agnostic-control-plane.md](docs/plans/agnostic-control-plane.md)**.
Read both before picking up a card.

**Own numbering namespace.** Sections here are `II.0` … `II.6`. Part I's numbers are cited
by number from Rust doc comments across the workspace and must not move.

**The thesis every design decision must respect:** competitors start from the fleet and
bolt on a board. Tack starts from the work item and adds the fleet. That is why Tack can
say *"this feature cost 4.2M tokens across 3 runs and 2 reworks"* and a fleet dashboard
cannot — it has no concept of a feature.

**The test this cycle has to pass:** *can an adapter be written for a provider with no
pods, no roles, no hops, no approval store and no policy engine, without touching the
trait?* Today it cannot: of thirteen `ControlPlane` methods, only `kind()` and `get_run()`
survive unchanged, three are pure docket, and eight carry docket-shaped DTOs.

## Status board — Part II

| Wave | Cards | Phases | Status |
|---|---|---|---|
| A — Oracle (blocking) | O1 · O2 · O3 | 39 | implemented and verified in the current working tree; unreleased |
| B — Foundations (parallel) | G1 · G2 · G3 · G4 · G5a · G5b | 40, 41, 42 | implemented in the current working tree; unreleased; concurrency/schema acceptance reopened by the architecture audit |
| C — The reshape (blocking, single owner) | T1 · T2 · T3 · T4 · T5 · T6 | 43 | **superseded — do not start; preserved below for history** |
| D — The second adapter | N1 · N2 · N3 · N4 · N5 | 44, 45 | **superseded — do not start; GitHub Actions is not an agent-harness proof** |
| E — Independent tracks | M1 · M2 · W1 · H1 · S1 · S2 | 46, 47, 48, 49 | **frozen — useful concepts are re-scoped into Part III; do not start from these cards** |

**Supersession note (2026-08-06).** Nothing below was deleted: it remains the decision
history that explains the current code and migrations. Part III replaces the central
boundary. Tack will own scheduling and durable execution requests; a pull-based
`tack-runner` will launch agent harnesses. Docket becomes an optional legacy bridge, and
GitHub Actions returns to being a CI/integration concern rather than the proof of an agent
harness abstraction.

---

## II.0 Rules of engagement (additions to §0)

Part I's ten rules stand unchanged. These are additional, and each one exists because the
planning read found the failure it prevents.

1. **Tack never runs agents and never proxies model traffic.** Always a client of a control
   plane. It never holds a vendor key on the request path, never implements routing or
   fallback. If a solution needs Tack to spawn an agent process, or model traffic to pass
   through the Tack process, it is out. Tack configures and reads a gateway; it never
   becomes one.
2. **Model identifiers are opaque strings.** Store the identifier plus the id of the
   gateway that understands it. **Never parse, map, normalise or classify one.** Tack
   classifies work items; the gateway classifies models. No tier abstraction under any
   name — docket removed `economy`/`standard`/`premium` in 0.2.0 and accepts them nowhere.
3. **No docket noun crosses the ACL.** If the UI says "pod," the anti-corruption layer has
   leaked. Provider specifics live in `provider_metadata` and a per-adapter UI fragment.
   `rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/` must return 0 by the
   end of Wave C.
4. **One `ALTER` per migration name.** `migrations.rs` runs each statement individually
   **with no wrapping transaction**, and records the migration name only after every
   statement succeeds. A five-`ALTER` migration failing on statement three records nothing;
   the next boot re-runs statement one, hits SQLite's `duplicate column name`, and **the
   server never boots again** — with no down-migration. Both existing `ALTER` migrations
   (029, 030) are deliberately single statements. This is a brick-the-install rule, not a
   style preference.
5. **Every new secret column is added to `remote_backup.rs::scrub_snapshot_secrets` in the
   same commit**, before the trailing `VACUUM`. That function's own doc comment states the
   rule. A secret column added without it ships credentials inside every downloadable
   backup.
6. **A capability is a value, never a provider check.** `rg -n "kind === 'docket'"` in
   `frontend/src` must return 0. A disabled control names its reason from
   `capabilities().<field>.reason`.
7. **Two meters are never one number.** Runner minutes and token spend are separate bills
   from separate vendors. Show each with its source label; never aggregate them into a
   single "cost" figure. At realistic volumes the runner is low single-digit percent of the
   total, so a combined number would hide the meter that matters.
8. **"Not measured" is never "0".** `orch_tasks.tokens_in`/`tokens_out` are written as
   literal `0` by `dispatcher.rs` and updated by nothing — every token figure in the app is
   a structural zero today. Card M2 builds the first real path; until then, and wherever no
   source exists, render "not measured".
9. **Handoff note:** finish by appending to §II.6.

---

## II.1 Shared contracts

Wave C defines them; every later card consumes them verbatim. Unlike Part I, the trait is
**not** frozen after its wave — §2.1 lifted that freeze and the reasoning still holds.
Change it if a real design need shows up, and update every implementor and caller in the
same change.

### II.1.1 The target `ControlPlane` trait

```rust
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    /// The ONLY input to the reachability verdict. The adapter decides what
    /// "reachable" means and owns its own expected-version check.
    async fn health(&self) -> Result<PlaneHealth, OrchError>;

    async fn runtimes(&self) -> Result<Vec<Runtime>, OrchError>;
    async fn plane_metrics(&self) -> Result<Vec<MetricSample>, OrchError>;

    /// Returns a rich ack, so no caller ever needs a read-back call.
    async fn dispatch(&self, t: &DispatchTarget, r: DispatchRequest)
        -> Result<DispatchAck, OrchError>;

    async fn list_runs(&self, t: &DispatchTarget) -> Result<Vec<RunStatus>, OrchError>;
    async fn run_status(&self, h: &RunHandle) -> Result<RunStatus, OrchError>;

    /// SCOPED. docket serves events per project and cannot serve them per run;
    /// GitHub Actions can only serve them per run.
    async fn events(&self, scope: &EventScope, cursor: Option<&str>)
        -> Result<EventPage, OrchError>;

    /// First-class, so the reconciler never reaches into provider_metadata.
    fn correlation_keys(&self, record: &CorrelatableRecord) -> Vec<String>;

    async fn artifacts(&self, h: &RunHandle) -> Result<Vec<Artifact>, OrchError>;
    async fn pending_decisions(&self) -> Result<Vec<Decision>, OrchError>;
    async fn resolve_decision(&self, id: &str, a: DecisionAnswer)
        -> Result<DecisionState, OrchError>;
    async fn usage(&self, h: &RunHandle) -> Result<Option<Usage>, OrchError>;
    async fn cancel(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn pause(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn resume(&self, h: &RunHandle) -> Result<(), OrchError>;
}
```

Gone from the trait: `status()`, `metrics()` (renamed `plane_metrics`), `list_tasks()`,
`get_run()` (dead — zero production call sites), `enqueue_task()`, `dispatch(project,
vars)` (dead — returns `Disabled` unconditionally), `decide_approval()`,
`provision_pod()`, `traces()`.

**Four corrections that are not negotiable**, each forced by real code:

- **`events` is scoped, not per-run.** `RemoteEvent` carries no run id and
  `persist_events` says so outright: *"docket's trace payload carries no run_id, only
  session_id… Left unset rather than guessing."* A per-run `events()` is **unimplementable**
  for docket — every event currently ingested would be dropped. The cursor store keeps its
  `(control_plane_id, remote_project)` key for `Project` scope and gains a run-scoped key.
- **`dispatch` returns `DispatchAck`.** `dispatcher.rs` today makes a **second** call to
  `list_tasks` purely to recover `remote_status` and `approval_token`. Delete `list_tasks`
  without widening the ack and `approval_token` becomes permanently `null` while every
  approval-gated dispatch takes the `on_running` branch. **The OpenAPI drift gate cannot
  catch this** — the field still exists, only its value dies.
- **`RunState` is a normalized closed enum on the trait**, never in `provider_metadata`.
  See §II.1.3.
- **`plane_metrics()` stays.** docket's `/metrics` is plane-wide with no run or project
  dimension, and `GET /api/projects/{id}/orch-policy` is built entirely from it including a
  server-computed `denial_rate`. A per-adapter UI fragment cannot produce a number the
  server already committed to in the spec.

### II.1.2 `Capabilities`

```rust
pub struct Capabilities {
    pub dispatch: bool,
    pub cancel: bool,
    pub pause: Support,             // Unsupported | Advisory | Supported
    pub resume: Support,
    pub event_scope: EventScope,    // None | Run | Project | Plane
    pub artifacts: bool,
    pub decisions: DecisionSupport, // None | Poll | Push
    pub usage: UsageSupport,        // NotMeasured | FromProvider | FromGateway
    pub model_selection: ModelSelection, // Unsupported | Advisory | Honoured
    pub runtimes: bool,
    pub plane_metrics: bool,
    pub provisioning: bool,
}
```

Every non-boolean field carries a `reason: &'static str` for the disabled case. **Model
selection is the acceptance test for the whole mechanism:** docket owns its routing and may
ignore an externally supplied model (`Unsupported`); a GitHub Actions adapter forwards it
verbatim (`Honoured`). The UI must render three different things, never a picker that
silently does nothing.

Two ad-hoc capability bits already exist and are retired into this struct:
`PendingApprovalListResponse.grant_available`, and `useAgentActivityMap`'s `orchAvailable()`
used as a dispatch gate — the latter really means *"orchestration is on"*, not *"this
provider can dispatch"*, and is wrong the moment there are two providers.

### II.1.3 Normalized `RunState`, and the per-adapter mapping table

`Queued | Running | Blocked | Succeeded | Failed | Cancelled | TimedOut | Unknown(String)`,
keeping the existing `remote_string_enum!` round-trip discipline.

| Provider value | Normalized |
|---|---|
| docket `queued` / `running` / `succeeded` / `failed` / `cancelled` | the same five — **docket is byte-identical** |
| GHA status `queued`, `pending`, `requested` | `Queued` |
| GHA status `in_progress` | `Running` |
| GHA status `waiting`, conclusion `action_required` | `Blocked` **and raises a `Decision`** — a deployment gate is a human waiting, not a terminal state |
| GHA conclusion `success` | `Succeeded` |
| GHA conclusion `failure`, `startup_failure` | `Failed` |
| GHA conclusion `cancelled` | `Cancelled` |
| GHA conclusion `timed_out` | `TimedOut` |
| GHA conclusion `skipped`, `neutral`, `stale` | `Cancelled` — documented as "the provider decided not to run it" |

Why this cannot live in `provider_metadata`: `orch_store.rs`'s
`reconcile_terminal_status_map` is the **only** place a finishing agent moves a card, and it
matches three string literals. GitHub has nine conclusions; seven would fall through
`_ => return` with no error, no log and no event, leaving cards permanently in "In
Progress".

`StatusMap` gains `on_blocked` and `on_timed_out`, **both optional**. Absent means fall back
to `on_waiting_approval` and `on_failed`, so every `status_map` already saved in a user's
database behaves exactly as it does today. Assert that with a test.

### II.1.4 Verified external facts

Checked against vendor documentation during planning. Do not re-derive; do re-verify if a
card's behaviour disagrees.

| Fact | Value | Consequence |
|---|---|---|
| `POST .../actions/workflows/{id}/dispatches` on github.com | `200` with `{workflow_run_id, run_url, html_url}` | fast path only |
| The same endpoint on **GHES 3.17** | `204 No Content`, empty body | **correlation must not depend on the dispatch response** |
| Cancel | `POST .../runs/{id}/cancel` → `202` | `cancel: true` |
| Pause / suspend / hold | **no endpoint exists** | `pause: Unsupported` |
| Force cancel | `POST .../runs/{id}/force-cancel` → `202` | provider extra, not a trait method |
| Re-run | `POST .../runs/{id}/rerun` → `201` | maps to `orch_tasks.attempt` / `run_attempt` |
| Run logs | `GET .../runs/{id}/logs` → `302`, link expires in **1 minute** | not an event stream — do not build on it |
| Log/artifact retention | default 90 days; public 1–90, private 1–400 | bounded derived history |
| Job limits | GitHub-hosted **6 h**, self-hosted **5 days**, run 35 days incl. waiting | the HITL ceiling |
| Pending deployments | `GET`/`POST .../runs/{id}/pending_deployments`, `{environment_ids, state, comment}` | Actions' decision store |
| Webhook headers | `X-GitHub-Event`, `X-GitHub-Delivery` (GUID), `X-Hub-Signature-256` | dedupe key + HMAC |
| Claude Code hooks | `PreToolUse`/`PostToolUse` are **synchronous and block the tool call**; default `command` timeout **600 s**, per-hook `timeout`; exit 2 blocks | the HITL mechanism |

---

## II.2 File-ownership map

One owner per file per wave. Anything not listed is free to create.

| File | Owner | Wave |
|---|---|---|
| `crates/tack-orch/tests/**` (oracles + golden) | **O1** | A — O2/O3 add files, never edit O1's |
| `.github/workflows/ci.yml` | **O3** | A — batch every gate for the cycle in one edit |
| `crates/tack-db/src/migrations.rs` | **G5** | B — **all migrations for the cycle route through G5**, one `ALTER` each (§II.0 rule 4) |
| `crates/tack-orch/src/lib.rs` (trait + DTOs) | G1 → **T1/T2** | B → C |
| `crates/tack-orch/src/adapters/registry.rs` | G1 | B |
| `crates/tack-orch/src/adapters/github_actions.rs` | G1 (stub) → **N2** | B → D |
| `crates/tack-orch/src/adapters/docket.rs` | **T2** | C |
| `crates/tack-orch/src/reconciler.rs` | **T3** | C |
| `crates/tack-api/src/orch_store.rs` | G1 → T4 | B → C |
| `crates/tack-api/src/dispatcher.rs` | T4 | C |
| `crates/tack-api/src/remote_backup.rs` | **G2** | B — scrub block only |
| `crates/tack-api/src/middleware.rs`, `router.rs` | **G4** | B — **batch every route and CORS change for the cycle**; later waves hand routes to G4's successor |
| `crates/tack-api/src/handlers/orch.rs` | T4 → N1 | C → D |
| `crates/tack-api/src/handlers/ingest.rs` (new) | **N3** | D |
| `crates/tack-api/src/handlers/webhooks.rs` (new) | **W1** | E |
| `crates/tack-cli/src/{client,mcp}.rs` | G4 | B |
| `crates/tack-core/src/models.rs` | T4 | C — ACL removal only |
| `crates/tack-core/src/model_policy.rs` (new) | M1 | E |
| `frontend/src/shared/orch/**` (new) | G1 → T6 | B → C |
| `frontend/src/features/fleet/**` | T6 | C |
| `frontend/src/features/approvals/**` | N1 | D |
| `docs/examples/**` (new) | N4, H1 | D, E |

**Merge order within a wave:** whoever finishes first merges first; the rest rebase. Cards
were scoped so only `migrations.rs`, `router.rs` and `lib.rs` can genuinely conflict, and
each has a single named owner.

---

## II.3 Wave plan

> **Historical/superseded sequence.** Waves C–E below must not be dispatched; Part III is
> the active agent board. The diagram remains to explain the work already present in the
> unreleased tree and the decisions recorded in Part II.

```text
Wave A  (Phase 39, blocking, 3 cards)          O1 ── O2 ── O3
                                                     │
Wave B  (Phases 40–42, 5 cards, parallel)  G1 G2 G3 G4 G5
                                                     │
Wave C  (Phase 43, SUPERSEDED)           T1 → T2 → T3 → T4 → T5 → T6
                                                     │
Wave D  (Phases 44–45, SUPERSEDED)            N1 N2 N3 N4 N5
                                                     │
Wave E  (Phases 46–49, FROZEN)                M1 M2 W1 H1 S1 S2
```

**Wave C is deliberately sequential and single-owner.** It is the one breaking change in
the cycle; splitting it across parallel agents would produce exactly the churn §2.1's
freeze was invented to prevent, on a trait with no external consumers to protect.

**Wave A blocks everything.** Nine of thirteen trait methods currently have no test
asserting what leaves the process — only four of the 37 `DocketAdapter` tests check an
outgoing request. Starting Wave C without Wave A is a blind refactor.

---

## Wave A — Phase 39, the regression oracle (blocking)

### O1 — Tick-level contract oracle

**Tasks:** 39.1 · **Files:** `crates/tack-orch/tests/docket_tick_contract_test.rs`,
`crates/tack-orch/tests/golden/**` · **Depends on:** —

1. Drive a full `reconcile_once` **plus the whole persist phase** against a `wiremock`
   docket **and** an in-memory SQLite. Both patterns already exist — copy
   `tests/ingestion_test.rs` and `tests/traces_ingestion_test.rs`; `sqlx` is already a
   dev-dep.
2. Snapshot **two** artifacts to `tests/golden/`: (i) the **ordered** list of HTTP requests
   the tick issued — method, path, sorted query, which headers were present, canonicalised
   body; (ii) the resulting rows of `orch_runs`, `orch_approvals`, `orch_events`,
   `orch_metrics`, `orch_trace_cursors`, deterministically sorted.
3. Five scenarios: cold start, warm cursor, **rewound cursor**, a plane with 0 linked
   projects, a plane with 3.
4. `UPDATE_GOLDEN=1` regenerates, mirroring the existing `UPDATE_OPENAPI=1` pattern in
   `crates/tack-api/tests/openapi_contract.rs`.

**Why ordered-and-counted, not a set:** the refactor that defeats a set is *"implement the
reshaped `events()` as a straight delegation to the same `/traces/{project}?since=`
request, then re-scope `reconcile_once` to iterate active runs"*. With 3 linked projects
and 0 active runs — the steady state, and exactly what a per-method fixture set looks like
— the tick issues **zero** trace calls where it issued three. Trace ingestion silently
stops for every user and a method-level golden is byte-identical.

**Why the row snapshot:** the refactor that defeats requests-only is *dropping the
`occurred_at < retention_cutoff` guard*. Not an adapter concern, so no request changes; but
a rewound cursor then resurrects rows already rolled into `orch_events_daily` and purged,
and the next `rollup_and_purge_orch_events` counts their cost **a second time**. Silent
money corruption, green CI.

**Acceptance:** `cargo test -p tack-orch --test docket_tick_contract_test` passes, and
`UPDATE_GOLDEN=1 cargo test -p tack-orch --test docket_tick_contract_test && git diff
--exit-code crates/tack-orch/tests/golden/` exits 0 on an unmodified tree. Then prove it is
real: locally re-scope the poll loop as described above and confirm the test fails.

### O2 — Per-method wire oracle + pinned event id

**Tasks:** 39.2, 39.3 · **Files:** `crates/tack-orch/tests/docket_wire_contract_test.rs`,
`crates/tack-orch/tests/golden/**` · **Depends on:** O1 (golden dir layout)

1. Request transcript plus decoded result for **all thirteen** current methods. Today only
   four tests check an outgoing request: `enqueue_task_sends_the_trusted_flag_on_the_wire`,
   `decide_approval_grant_sends_channel_tack_…`,
   `provision_pod_sends_the_full_request_shape_on_the_wire`,
   `unauthenticated_routes_never_send_authorization_header`. The other 33 assert decoding.
2. Pinned-literal event id:
   `assert_eq!(derive_event_id(<fixed uuid>, "proj", &<fixed event>).to_string(), "<literal>")`.
   The namespace constant carries a *"must never change once any deployment has ingested a
   single event"* warning, and the existing determinism test only proves determinism
   **within one build** — it would not catch a changed separator, field order, or namespace,
   which re-inserts every previously-ingested event under a fresh id on the first poll after
   upgrade.

**Acceptance:** both tests pass; flipping one byte of the namespace constant locally fails
the pinned test.

### O3 — CI gates and the missing coverage floor

**Tasks:** 39.4 · **Files:** `.github/workflows/ci.yml` · **Depends on:** O1, O2

1. A `golden drift` step in the `rust` job, mirroring the OpenAPI gate exactly.
2. `cargo llvm-cov -p tack-orch --fail-under-lines 70` in the `coverage` job. **There is no
   `tack-orch` floor today** — the job floors `tack-core >= 85`, `tack-db >= 70`,
   `tack-api >= 70` and does not name `tack-orch`, which makes the adapter the
   least-guarded code in the workspace and the code this cycle rewrites most.

**Acceptance:** a deliberately mutated golden fails CI; `cargo llvm-cov -p tack-orch
--fail-under-lines 70` passes locally.

---

## Wave B — Phases 40–42, foundations (parallel)

### G1 — Capability model + one adapter registry + GHA stub

**Tasks:** 40.1, 40.4, 40.5, 40.7 · **Files:** `crates/tack-orch/src/lib.rs`,
`crates/tack-orch/src/adapters/{mod,registry,github_actions}.rs`,
`crates/tack-api/src/{orch_store,dispatcher}.rs`,
`crates/tack-api/src/handlers/{orch,provisioning}.rs`,
`frontend/src/shared/orch/capabilities.ts` · **Depends on:** G2 (the `config`/`secrets`
columns the registry signature needs)

1. `Capabilities` per §II.1.2, plus `fn capabilities(&self) -> Capabilities` on the trait.
   `DocketAdapter::capabilities()` returns the **verified** truth, including
   `pause: Unsupported` with a reason naming `docket profile <pod-id> --resume`.
2. `tack_orch::adapters::registry::build(kind, base_url, config, secrets)`. It must live in
   `tack-orch` — `crates/tack-orch/Cargo.toml` forbids depending on `tack-api`.
3. Replace **all four** duplicated `match row.kind.as_str()` sites. They are not
   equivalent and must keep their different failure behaviour: `orch_store.rs` warns and
   `continue`s (batch, reconciler); `dispatcher.rs`, `handlers/orch.rs` and
   `handlers/provisioning.rs` each error, each mapping `RowNotFound` to `NotFound`
   separately. Two of them carry a comment saying the duplication is deliberate and *"if a
   third caller ever needs this, that's the point to actually share it."* There are four.
4. `adapters::github_actions` **compile-only stub**: `kind()` and `capabilities()`
   truthfully filled in, every other method `unimplemented!()`, never registered. Its only
   job is to make *"both adapters compile against the trait"* a Wave C gate rather than a
   Wave D discovery.
5. Surface capabilities on `GET /api/control-planes/{id}` and `GET /api/fleet`; every gated
   frontend control reads them. Retire `grant_available` and the
   `orchAvailable()`-as-dispatch-gate.

**Acceptance:** `rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` returns **0**;
`rg -n "kind === 'docket'|grant_available" frontend/src` returns **0**;
`cargo test -p tack-api --test orch_reconciler_wiring_test` passes unchanged; a Vitest test
asserts a disabled control renders a reason string **sourced from the capability**, not a
hard-coded literal.

### G2 — Plane config + secrets + `unconfigured` health + backup scrub

**Tasks:** 40.2, 40.3 · **Files:** `crates/tack-api/src/remote_backup.rs`,
`crates/tack-api/src/orch_store.rs`, `frontend/src/features/fleet/api.ts` ·
**Depends on:** G5 (migrations 032, 033)

1. `control_planes.config` (provider configuration JSON) and `control_planes.secrets`
   (write-only credentials). A GitHub Actions plane needs `{owner, repo, workflow_file, ref,
   api_base}` plus **two** secrets — an API credential and a webhook secret — and today
   `control_planes` has only `base_url` and one `token`.
2. **Same commit** adds the `control_planes.secrets` block to
   `remote_backup.rs::scrub_snapshot_secrets`, before the trailing `VACUUM` (§II.0 rule 5).
   Follow the shape of the existing `control_planes.token` block: null the column, keep the
   row, guard on `sqlite_master` so a pre-migration snapshot still works.
3. `health = 'unconfigured'`. A restored backup has `secrets IS NULL`, and `orch_store.rs`
   currently `continue`s past a failed adapter construction with only a `warn!` — **the
   plane vanishes from polling invisibly.** Benign for docket (its token is optional and the
   adapter only adds the header when present), fatal and silent for any plane whose
   credentials are required. Widens the closed `ControlPlaneHealth` union in the frontend.

**Acceptance:** a new `scrub_removes_control_plane_secrets_from_snapshot` test in the shape
of the existing token one asserts the column is nulled and the row survives; an
unconfigured plane reports `unconfigured`, not `unknown`; `HEALTH_LABEL` covers five states.

### G3 — Optimistic concurrency

**Tasks:** 41.1, 41.2 · **Files:** `crates/tack-db/src/repo/{items,orch}.rs`,
`crates/tack-api/src/handlers/{items,orch,projects}.rs` · **Depends on:** G5 (migrations
034–036)

1. Repo layer bumps `version` on **every** `UPDATE` — including
   `update_item_status_checked` and `check_and_update_parent_status`, not just
   `update_item`.
2. `ETag` on `GET`, `If-Match` on `PATCH`/`PUT` for items, orch-links and control-planes.
   `412` on mismatch. **An absent `If-Match` behaves exactly as today**, so nothing breaks.

**Acceptance — corrected 2026-08-06, the original criterion here was weak and the
adversarial pass proved it:** the gate is the two **sequential** tests,
`patch_with_a_stale_if_match_is_rejected_with_412_and_the_standard_envelope` and
`patch_with_an_if_match_for_a_different_item_is_rejected`. Each captures an ETag, lets a
write land, then replays the now-stale ETag and requires `412`. No concurrency, no
scheduler dependence; both fail 100% of the time against an `If-Match` that is parsed and
ignored.

The originally-named criterion — two concurrent PATCHes yielding one `200` and one `412` —
detected that mutation only **5 times in 15 runs**, because it leaves
`claim_item_version`'s atomic `UPDATE ... WHERE version = ?` in place and two racers sharing
one still-valid version reproduce the expected shape by coincidence. It stays in the suite
as a property test of the compare-and-swap layer, but it is **not** the gate. See §II.6's
"Adversarial verification — Wave B" for the full finding.

### G4 — CORS, routes, and the MCP write path

**Tasks:** 41.3, 41.4, 41.5, 40.6 · **Files:** `crates/tack-api/src/{router,middleware}.rs`,
`crates/tack-cli/src/{client,mcp}.rs`, `crates/tack-api/src/handlers/items.rs`,
`docs/book/src/developer/orchestration.md` · **Depends on:** G3

1. **CORS has no `expose_headers` call at all** — a browser can read no non-safelisted
   response header today. Add `expose_headers([ETAG])`, and add `if-match` and
   **`x-tack-approval-token`** to the allow-list. The approval-token omission is a
   pre-existing bug: the decide call works only because production is same-origin via
   `embed-spa`, and is already broken on any cross-origin `TACK_ALLOWED_ORIGINS` path.
   There is no CORS test in the repo — this card ships the first one.
2. `tack-cli`'s client `request()` **cannot set a header at all**, so every MCP write is
   unconditionally last-write-wins. Add header support and send `If-Match` from
   `update_item` and `move_item`. The agent-versus-human race is precisely the one G3
   exists for, and the agent path is the unprotected one. Note `mcp.rs` asserts
   `tools.len() == 8`; it moves in step with any tool change.
3. **Fix the auto-dispatch gate.** `handlers/items.rs` gates auto-dispatch on
   `state.config.orch_enable`, not `effective_orch_enabled` — so it **ignores the UI toggle
   today**, contradicting §0 rule 8. With a GitHub Actions plane that means a workflow
   dispatched automatically while the UI reports orchestration off. A behaviour change to a
   shipped feature: note it in `CHANGELOG.md`.
4. Document the writers that bypass HTTP: the reconciler calls
   `dispatcher::apply_mapped_status` directly with no request and no `If-Match` — **the
   largest single mutator of `items.status` is outside this control by design** — and
   `propagate_parent_completion` mutates a *parent* on a child's PATCH, so a parent's ETag
   changes with no caller having touched it. Both are correct; both must be written down so
   a client does not conclude `412` is a total ordering.

**Acceptance:** `cargo test -p tack-api
preflight_allows_if_match_and_approval_token_and_exposes_etag`; `cargo test -p tack-cli
mcp_update_item_sends_if_match`; `mdbook build docs/book`.

### G5 — All migrations for the cycle, one `ALTER` each

**Tasks:** 40.2, 41.1, 42.1, 42.2, 42.3, 42.4 · **Files:**
`crates/tack-db/src/migrations.rs` · **Depends on:** —

**This card owns `migrations.rs` for the entire cycle.** Later waves request migrations
through it rather than editing directly — the same chokepoint discipline `router.rs` had in
Part I.

1. 032 `control_planes.config`, 033 `control_planes.secrets`, 034 `items.version`,
   035 `orch_links.version`, 036 `control_planes.version` — **one statement each**
   (§II.0 rule 4).
2. **037, rebuild `orch_runs`.** SQLite's 12-step procedure, this table only: create with
   `PRIMARY KEY (control_plane_id, external_run_id, run_attempt)` and `correlation_id TEXT
   UNIQUE`, `INSERT … SELECT` copying `run_id` into `external_run_id` with
   `run_attempt = 1`, drop, rename, recreate `idx_orch_runs_plane_state`,
   `PRAGMA foreign_key_check`. **Why it cannot be an `ALTER`:** `run_id` is a *global* PK
   with `control_plane_id` outside the key, so minting a placeholder on the Tack
   correlation id and later "backfilling" the provider id inserts a **second** row under a
   different PK — `ON CONFLICT(run_id)` cannot merge two different primary keys, and you
   get two rows per run forever.
3. **038, rebuild `orch_approvals`.** `control_plane_id` becomes nullable — a
   hook-originated decision comes from a run that may never have been dispatched through a
   registered plane, and today the column is `NOT NULL REFERENCES control_planes(id)`. Adds
   `kind`, `external_id`, `provider_metadata`. **`token` stays the PK and the URL segment**
   — renaming a column that is in a user's database buys nothing.
4. **Half-applied-rebuild guard:** `run_all` refuses to boot if both `orch_runs` and
   `orch_runs_new` exist, with an error naming the backup endpoint, rather than re-running
   `DROP TABLE`.
5. Release note: this upgrade rewrites two tables; take a backup first.
6. Later migrations, added as their cards land: 039–041 (ingest), 042–043 (model policy),
   044–049 (GitHub links).

**Acceptance:** a seeded DB at 036 upgrades with identical row counts, per-row field
equality, an empty `PRAGMA foreign_key_check`, and the old PK's uniqueness still enforced;
a deliberately half-applied state refuses to boot with a named error;
`cargo test -p tack-db --test orch_migrations_test` green.

---

## Wave C — Phase 43, the reshape (superseded; do not start)

### T1 — DTOs and the `RunState` mapping table

**Tasks:** 43.1 · **Files:** `crates/tack-orch/src/lib.rs` · **Depends on:** Wave A, G1

Every DTO from §II.1.1, each with `provider_metadata: serde_json::Value` for provider
extras. `RunState` per §II.1.3, keeping the `remote_string_enum!` `Unknown(String)`
discipline — a provider upgrade that adds a state must degrade to "shown as-is", never to a
deserialization error that kills the poll loop.

**Acceptance:** `cargo test -p tack-orch run_state_normalization_table` — table-driven over
**every** documented GitHub status and conclusion and every docket state. A mapping that
silently falls through fails it.

### T2 — The trait, and `DocketAdapter` rewritten

**Tasks:** 43.2 · **Files:** `crates/tack-orch/src/{lib,adapters/docket}.rs` ·
**Depends on:** T1

`provision_pod` leaves the trait for a provider-specific route.

**Acceptance:** `cargo test -p tack-orch --test docket_tick_contract_test && git diff
--exit-code crates/tack-orch/tests/golden/` — **the golden must not move.** Plus
`cargo build -p tack-orch` proving both adapters compile against the new shape.

### T3 — Reconciler restructure

**Tasks:** 43.3 · **Files:** `crates/tack-orch/src/reconciler.rs` · **Depends on:** T2

1. `evaluate` consumes **only** `health()`. Today `reachable = health_ok && status_ok`; if
   `runtimes()` inherited that role, every GitHub plane would go `unreachable` on tick one —
   listing runners needs repo-admin and an `actions:write` credential gets 403 — and the
   backoff would then pin it there.
2. `EXPECTED_API_VERSION` moves out of the reconciler into the docket adapter;
   `PlaneHealth` carries `api_version` and `version_ok`. The adapter decides what
   "reachable" means, so **docket keeps requiring both `/health` and `/status.json`** and
   its behaviour does not move.
3. Correlation moves to `correlation_keys()`. Today `persist_runs` reads
   `RemoteRun.task_ids` and `persist_approvals` reads `RemoteApproval.context.taskId` —
   both docket-only. Pushing them into `provider_metadata` would make the *generic*
   reconciler do `md["context"]["taskId"]`, which is the exact coupling this cycle removes.
4. Per-scope event polling; `FetchOutcome` fields renamed to match.

**Acceptance:** the tick golden is unchanged, **and** the existing state-machine verdict
assertions still pass. Renaming `evaluate`'s input fields is fine; **if an asserted verdict
has to change, docket's behaviour moved and the change is wrong.**

### T4 — `tack-api` onto the new trait, and the `tack-core` ACL removal

**Tasks:** 43.4, 43.5, 43.6 · **Files:** `crates/tack-api/src/{orch_store,dispatcher,
sprint_dispatch}.rs`, `crates/tack-api/src/handlers/{orch,provisioning,economics}.rs`,
`crates/tack-core/src/models.rs` · **Depends on:** T3

1. `dispatcher.rs`'s read-back call disappears — `DispatchAck` carries `state` and
   `pending_decision_id`.
2. `StatusMap` gains `on_blocked` and `on_timed_out`, both optional, both falling back.
3. Move `OrchBlueprint` and `TemplateOrchestration` out of `tack-core` into
   `tack-api::handlers::provisioning`. `project_templates.orchestration` is already `TEXT`,
   so `tack-core` keeps only an opaque `serde_json::Value`. **No migration.**

**Acceptance:** `cargo test -p tack-api dispatch_ack_carries_the_approval_token_without_a_second_call`
— asserts `approval_token != null` on a `waiting_approval` dispatch **and** that wiremock
saw exactly one docket request. This is the cycle's canonical green-CI-broken-product
regression: the OpenAPI gate cannot catch it because the field survives, only its value
dies. Plus `cargo test -p tack-api a_status_map_saved_before_this_release_behaves_identically`
and `rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/` returning **0**.

### T5 — Contract regeneration and the breaking-change notes

**Tasks:** 43.7 · **Files:** `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
`CHANGELOG.md` · **Depends on:** T4

Per decision D3 the 21 orchestration operations are **broken with notice** — no aliases.
`CHANGELOG.md` names every changed operation. Fix the stale `utoipa` annotations while
here: every orch handler still documents *"404, Orchestration disabled (TACK_ORCH_ENABLE
unset)"* and the spec's `orchestration` tag still says *"Every route is disabled — 404"*,
but the code has returned **409** with `error.code: "orchestration_disabled"` since card E1.

**Acceptance:** `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`, then
`npm --prefix frontend run gen:api`, then `git diff --exit-code docs/openapi.json
frontend/src/shared/api/schema.gen.ts` passes on the committed result.

### T6 — Frontend neutral shapes + lazy docket fragment

**Tasks:** 43.8 · **Files:** `frontend/src/features/fleet/**`,
`frontend/src/shared/{orch,dispatch,agentActivity}/**` · **Depends on:** T5

1. The eight hand-written wire-boundary `api.ts` files each carry the contract *"When the
   real endpoint lands (or changes), reconciling means editing THIS FILE ONLY."* No
   component reads a raw wire field — **that is the whole refactor seam; use it.**
2. `shared/orch/providers/docket/` as a **lazy** fragment
   (`lazy(() => import(...))`, the pattern already used for every route) so the 30 KB
   gzipped entry-bundle CI gate is unaffected.
3. `Pod health` → `Health`; `Roster` → `Runtimes`; docket-specific cells move into the
   fragment. `architecture.test.ts` forbids cross-feature imports, so anything shared lands
   in `shared/`.

**Acceptance:** `rg -n "Pod health|Roster|Burn vs budget" frontend/src` returns **0**;
`npm --prefix frontend run build` passes the bundle-size gate; the existing
`FleetPage.test.tsx` column assertions are updated in the same commit.

---

## Wave D — Phases 44–45, the second adapter (superseded; do not start)

### N1 — Unified decision inbox

**Tasks:** 44.1–44.4 · **Files:** `crates/tack-api/src/handlers/orch.rs`,
`crates/tack-api/src/router.rs` (via G4's successor), `frontend/src/features/approvals/**` ·
**Depends on:** Wave C, G5 (migration 038)

Four kinds: `ApprovalOfIrreversibleAction`, `PlanAwaitingReview`, `OpenQuestion`,
`WorkOrderAmbiguity`. Routes move to `/api/decisions` and `/api/decisions/{id}` (D3, no
alias). **`TACK_ORCH_APPROVAL_TOKEN` keeps its exact meaning** — resolving a decision stays
higher-privilege than editing a card. With one shared secret there is no per-user actor, so
the audit row records the **surface** and the UI never renders a name it does not have.

**Acceptance:** the four kinds render distinctly; the existing approval-token gating tests
pass verbatim in behaviour; `resolving_a_decision_records_an_unattributed_audit_row`.

### N2 — The GitHub Actions adapter

**Tasks:** 45.1 · **Files:** `crates/tack-orch/src/adapters/github_actions.rs`,
`crates/tack-orch/tests/github_actions_adapter_test.rs` · **Depends on:** Wave C

1. `health` uses a cheap authenticated `GET /repos/{o}/{r}` — **never** the runner list,
   which needs repo-admin and whose 403 would pin every plane at `unreachable`.
2. `events` is `EventScope::Run`, derived from `GET .../runs/{id}/jobs` steps, cursor =
   highest `(job_id, step_number)`. **Logs are not used at all** — a `302` expiring in one
   minute over an archive deleted on a retention schedule is not an event stream.
3. `pending_decisions` / `resolve_decision` map to `pending_deployments`.
   `pause`/`resume` return `Unsupported`.
4. Raw `reqwest` — no `octocrab`, per the crate's own rule against a second HTTP client.
5. Fixtures for **both** dispatch responses: `200` with `workflow_run_id` (github.com) and
   `204` empty (GHES 3.17).

**Acceptance:** a golden request transcript in O2's shape; every capability the adapter
declares is exercised by a test.

### N3 — Bind and ingest endpoints

**Tasks:** 45.2, 45.3, 45.4, 45.5 · **Files:** `crates/tack-api/src/handlers/ingest.rs`,
`crates/tack-api/src/{config,router}.rs` · **Depends on:** N2, G5 (migrations 039–041)

1. **Correlation does not depend on the dispatch response** (GHES returns 204). Tack mints
   `tack_run_id` and passes it as a **non-secret** workflow input; the workflow's first step
   calls `POST /api/fleet/runs/bind` with it plus `${{ github.run_id }}`.
2. **The nonce is single-use.** A workflow input is caller-supplied, so anyone with
   `actions:write` could forge one and post events into another item's timeline. `bind`
   verifies the nonce was minted **by Tack, for that plane, and is still unbound**, and
   consumes it.
3. `bind` returns the **per-run credential** (D5): scoped to one correlation id, expiring
   with the run, able to append events and raise decisions for that run and **nothing
   else** — it can never resolve a decision and never edit a card. It is returned in a
   response body, so it never appears in a log. **Workflow inputs are visible in the run's
   UI and logs**; no vendor key, gateway key or API token ever travels as one.
   `TACK_ORCH_RUN_BOOTSTRAP_TOKEN` gates the exchange and, like
   `TACK_ORCH_APPROVAL_TOKEN` and unlike `TACK_API_TOKEN`, **unset means refuse**.
4. `POST /api/fleet/runs/{correlation_id}/events` — idempotent on a caller-supplied
   `event_id` behind the partial unique index; rejects events for a terminal run.
5. Migration 039's `orch_events.source` vocabulary is **three values**, not two. The claim
   that `run_id` is always NULL and `id` is always a UUIDv5 is true only of the reconciler's
   path: `orch_store.rs` writes `new_v4()` with `run_id: Some(..)` for
   `status_map_skipped_human_override`, and `dispatcher.rs` and the auto-dispatch hook write
   `new_v4()` with `run_id: None`. So `'poll' | 'push' | 'local'`, with existing rows
   backfilled to `'local'` keyed on `event_type`.
6. **Auth wiring: own sub-router, outside both existing gates.** A run must not need
   `TACK_API_TOKEN`, and toggling orchestration off must not `409` an in-flight handshake
   and re-label every live run "not instrumented". **Not** a fourth entry in the
   `path().ends_with(...)` exemption list — that is a suffix match and would exempt any
   future path ending in the same string.

**Acceptance:** `bind_rejects_a_forged_nonce_a_reused_nonce_and_a_nonce_from_another_plane`
(three 403s, zero run rows); `posting_the_same_event_batch_twice_yields_one_row_per_event`;
`existing_locally_minted_events_backfill_to_local_not_poll`;
`no_other_route_became_exempt` enumerating the router.

### N4 — Reference workflow, hook, and the instrumentation honesty rules

**Tasks:** 45.6, 45.7 · **Files:** `docs/examples/github-actions/**`,
`docs/book/src/user-guide/**`, `crates/tack-api/src/handlers/ingest.rs` ·
**Depends on:** N3

1. A copy-pasteable workflow and `PostToolUse` hook. Documents honestly that **fork-PR runs
   receive no secrets**, cannot bind, and will correctly render "not instrumented".
2. **"Not instrumented" must never be confused with "waiting on a human."** A run parked on
   a required-reviewer environment sits in `waiting` for up to 35 days and is exactly what
   the decision inbox exists to surface — suppress the timer whenever the run is `waiting`
   or a decision is open against it.
3. A run that goes `queued → cancelled` without ever starting never arms an
   `in_progress`-based timer, so the reaper runs off a **dispatch-time deadline** and leaves
   an event explaining why the card stopped. Without this the card sits in "In Progress"
   forever: an unbound run lands with `item_id: None`, `upsert_runs` skips it, and
   `reconcile_terminal_status_map` never runs.

**Acceptance:** `a_waiting_run_is_never_labelled_not_instrumented`;
`a_run_cancelled_before_starting_is_reaped_and_leaves_an_event`; `mdbook build docs/book`.

### N5 — GitHub Actions in the UI

**Tasks:** 45.8 · **Files:** `frontend/src/features/settings/orchestrationSettings/**`,
`frontend/src/shared/orch/providers/github-actions/**` · **Depends on:** N2, T6

A second Kind option (today the selector hard-codes `<option value="docket">docket</option>`
as its only value, with `docket` also defaulted twice and rendered raw per row), its own
config form, and a **lazy** provider fragment.

**Acceptance:** a Vitest test asserts two Kind options and that selecting each renders a
different config form; the bundle-size gate still passes.

---

## Wave E — Phases 46–49, independent tracks (frozen; do not start)

### M1 — Model policy

**Tasks:** 46.1–46.4, 46.7 · **Files:** `crates/tack-core/src/model_policy.rs`,
`crates/tack-api/src/handlers/orch.rs`, `frontend/src/features/{settings,item-detail}/**` ·
**Depends on:** Wave D, G5 (migrations 042, 043)

1. Resolution order: **item override → item-type default → project default →
   control-plane default.** Pure, no I/O, in `tack-core`.
2. It **never parses, maps, normalises or classifies the identifier** (§II.0 rule 2). This
   also removes the staleness problem permanently — a new model needs no Tack release.
3. Every response carries the **resolved** value and its **provenance** — *"sonnet, from
   project default."* A policy whose provenance is invisible is a policy nobody trusts.
4. `capabilities().model_selection` respected with all three values live: docket
   `Unsupported`, GitHub Actions `Honoured`. **This is the capability-negotiation
   acceptance test for the whole cycle.**
5. `orch_links.harness` — one plain config field beside the workflow name. **No harness
   trait, no capability matrix, no plugin layer.** The gates, labels, plan template and
   review rubric live in GitHub and in the repo, so they are harness-independent already.

**Acceptance:** all sixteen presence combinations resolve correctly; a nonsense identifier
like `"zzz/not-a-model:v9"` resolves and round-trips unchanged; three capability values
render three different controls; `rg -n "trait Harness|HarnessRegistry" crates/` returns 0.

### M2 — Gateway config and the first real measurement

**Tasks:** 46.5, 46.6 · **Files:** `crates/tack-api/src/handlers/{orch,economics}.rs`,
`crates/tack-api/src/remote_backup.rs` · **Depends on:** M1

1. Per-project gateway: base URL, an optional **server-side read-only spend-query
   credential**, nothing else. Tack never sends a key into a run — the run gets its own from
   a repo secret the operator sets. A gateway credential in `app_meta` must also join
   `SENSITIVE_META_KEYS`.
2. **Gateway unreachable ⇒ dispatch refuses.** A new `gateway_unreachable` outcome (HTTP
   200, branch on `outcome` like the existing six) and no run starts. A run that silently
   bypasses the gateway is unmeasured and uncapped — exactly the "shows zero, actually spent
   money" failure the numeric-honesty rules exist to prevent. Queuing is rejected: Tack has
   no queue and no replay logic anywhere, deliberately.
3. Roll pushed telemetry and docket's own `cost_charged` events into `orch_tasks`. Where no
   source exists: **"not measured"**, never `0` (§II.0 rule 8).

**Acceptance:** `gateway_secret_is_write_only_and_scrubbed_from_a_backup`;
`pushed_usage_events_roll_up_into_orch_tasks`; a project with no measurement source renders
"not measured" and never `$0.00` or `0 tokens`.

> **Read this before starting M2.** Every token figure in the app is a structural zero
> today, so the whole numeric surface has **never been exercised against real data**. When
> this card lands, every number becomes non-zero at once and a units error — input counted
> as output, cumulative as delta, per-attempt summed across retries — would ship looking
> entirely plausible. **Hand-verify against one real run's provider figures before letting
> the Economics page render the new values.** CI cannot do this check for you.

### W1 — Inbound GitHub webhooks and echo suppression

**Tasks:** 47.1–47.4 · **Files:** `crates/tack-api/src/handlers/webhooks.rs`,
`crates/tack-api/src/{router,github_sync}.rs`, `crates/tack-api/src/handlers/items.rs` ·
**Depends on:** N3 (the sub-router pattern)

1. `POST /api/webhooks/github/{control_plane_id}`, verifying `X-Hub-Signature-256` with the
   `hmac`/`sha2`/`hex` crates already present — `webhook.rs` has `sign` and **no `verify`**,
   and nothing in the codebase reads a signature today. Reuse `constant_time_eq`.
2. Dedupe on `X-GitHub-Delivery`, purged by the retention sweep.
3. `workflow_run`, `workflow_job`, `deployment_review`. **Polling stays as the
   reconciliation backstop**, so a missed delivery self-heals — the pull-based design was
   chosen precisely so no queue or replay logic is needed.
4. **Echo suppression, three layers**, because any one alone is defeatable: a
   `ChangeOrigin` tag so a webhook-driven write never re-fires `maybe_sync_github`; a
   `github_links.state_hash` backstop for when the tag is lost across a process boundary;
   and dropping deliveries whose `sender.id` is the identity Tack pushes as. **`ItemSource`
   cannot serve here** — it is written once at creation and `update_item` has no code path
   that touches the column.

**Acceptance:** a bad, a missing, and a wrong-plane signature each `401` with zero writes;
a replayed delivery GUID changes no row; a dropped delivery is recovered by the next poll;
and `a_webhook_driven_status_change_produces_no_outbound_push` asserts the mock GitHub
received **zero** requests — a naive implementation loops and never terminates cleanly.

### H1 — Intervention without pause

**Tasks:** 48.1–48.4 · **Files:** `crates/tack-api/src/handlers/{ingest,orch}.rs`,
`docs/examples/hooks/**` · **Depends on:** N3, N1

A `PreToolUse` hook runs **synchronously and blocks the tool call until it returns**, so it
can post a decision request and long-poll for the verdict. That makes the decision inbox
the mechanism that supplies **intervention**, not just visibility — on a provider with no
pause API at all.

1. `POST /api/fleet/runs/{correlation_id}/decisions` (raise) and
   `GET …/decisions/{id}/verdict?wait=<secs>` (bounded), both on the run credential.
   **Resolution stays on `/api/decisions/{id}` behind `TACK_ORCH_APPROVAL_TOKEN`** — the run
   credential can raise a decision and can never answer one.
2. **The ceiling:** the reference hook sets `timeout: 600` **explicitly** rather than
   relying on the default, and requests `wait=540`, leaving headroom for the round trip. A
   hook that outlives its own timeout is killed and its verdict lost. 540 s is far under the
   6 h GitHub-hosted job cap, so the wait can never be what kills a job. Minutes, not hours,
   deliberately.
3. **Expiry is fail-closed**: deny, hook exits 2, tool call blocked, `orch_events` row
   records the expiry and its reason.
4. **Where the item lands:** `on_blocked` if the project's `status_map` sets it, otherwise
   the item does not move and the decision is recorded as expired. Never silently "done".
5. The operator guide names the cost: the wait is **paid idle runner time**, so decisions
   belong at genuine gates, not per tool call.

**Acceptance:** `a_run_credential_cannot_resolve_its_own_decision`;
`an_expired_decision_returns_deny_and_writes_an_audit_row`;
`an_expired_decision_never_moves_an_item_to_a_done_status`.

### S1 — Bi-directional issue sync

**Tasks:** 49.1–49.4 · **Files:** `crates/tack-db/src/repo/github_links.rs`,
`crates/tack-api/src/handlers/{items,webhooks,import_github}.rs` · **Depends on:** W1,
G5 (migrations 044–049)

1. `github_links` gains `host` (default `'github.com'`), `node_id`, `last_synced_at`,
   `remote_updated_at`, `state_hash` — **one `ALTER` each** — plus a **non-unique** reverse
   index on `(host, repo, issue_number)`. A unique index could fail on a user's existing
   duplicates, and a failed statement in this migration runner bricks the boot loop;
   uniqueness is enforced in the repo layer, which logs when it finds more than one. Today
   there is no reverse index at all and no `get_link_by_issue`, so an inbound event for an
   issue has no lookup path.
2. **Credential precedence, decided and written down:** `TACK_GITHUB_TOKEN` /
   `TACK_GITHUB_API_BASE` remain the fallback for import and issue push; a control plane's
   own credentials win where a plane is involved. Two token sources with different scopes
   exist today and the rule was never stated.
3. Inbound `issues` / `issue_comment` applied **through `tack-core`** so workflow rules
   hold, with `ItemSource::Github` preserved so the trust boundary is not laundered.
4. Outbound item → issue create: **per project, opt-in, off by default.**

**Acceptance:** `an_issue_edited_on_github_updates_the_item_without_bypassing_the_workflow_engine`
asserts an illegal transition is **rejected, not forced**;
`item_create_does_not_touch_github_unless_the_project_opted_in`;
`plane_credentials_win_over_the_global_github_token`.

### S2 — PR, checks, merge evidence, decision mirroring, and retry

**Tasks:** 49.5–49.8 · **Files:** `crates/tack-api/src/{github_sync,webhook}.rs`,
`crates/tack-api/src/handlers/webhooks.rs`, `docs/GITHUB-SYNC.md` · **Depends on:** S1

1. PR opened → item moves and the PR is linked; checks running → verifying; check failed →
   failed with the run link; PR merged → done with **evidence** (SHA, run URL, artifacts)
   persisted as an `orch_events` row.
2. A blocking decision appears in Tack's inbox **and** as a comment plus a label on the
   issue; resolving on either side reflects on the other.
3. **Retry and rate limits.** Today the push is `tokio::spawn`'d and never awaited, with
   zero retry and no persisted failure record — the only failure surface is a `warn!` line,
   unlike the auto-dispatch hook beside it which writes an `auto_dispatch_failed` event.
   Import likewise only *observes* `x-ratelimit-remaining` and never acts on it: no
   `Retry-After`, no `x-ratelimit-reset`, no backoff. Add a bounded retry honouring both,
   and record failures as `orch_events`. **No new dependency** — `tower` is already a
   workspace dep with `features = ["full"]`.
4. Rewrite `docs/GITHUB-SYNC.md` for v2; it currently lists inbound sync, comment
   mirroring, per-project tokens and manual linking as explicitly out of scope.

**Acceptance:** `a_merged_pr_completes_the_item_with_sha_run_url_and_artifacts` asserts all
three evidence fields non-empty;
`resolving_on_github_resolves_in_tack_and_vice_versa_without_an_echo`;
`a_rate_limited_push_retries_and_records_a_failure_event`.

---

## II.4 Cross-cutting acceptance for the cycle

An operator registers a GitHub Actions control plane beside a docket one, dispatches the
same sprint to either, sees per-item token cost from both, resolves a blocking decision
raised from inside a running workflow, and watches a merged PR complete the card with its
SHA and run URL — with **every control the provider cannot support visibly disabled and its
reason named**, and with **docket's behaviour provably unchanged throughout**.

Mechanically, at the end of the cycle:

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
cargo llvm-cov -p tack-orch --fail-under-lines 70
git diff --exit-code crates/tack-orch/tests/golden/ docs/openapi.json
cd frontend && npm run type-check && npm run test && npm run lint:tokens && npm run build
make e2e
rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/          # 0
rg -n "kind === 'docket'" frontend/src                          # 0
rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/ # 0
rg -n "Pod health|Roster|Burn vs budget" frontend/src            # 0
```

---

## II.5 Known risks

1. **The Wave C golden becomes unfalsifiable the moment it legitimately changes.** Some
   change will plausibly *require* it to move — say the reshaped reconciler issues
   `GET /health` before `GET /status.json`. At that moment there is no way to distinguish
   "legitimately changed" from "we broke it", and the temptation is to regenerate and move
   on. **Rule, not intention:** the golden may only change in a commit that changes nothing
   else, whose message states the behavioural difference and why it is safe, and whose diff
   is read line by line. **More than two such commits in Wave C means the reshape is not
   preserving behaviour — stop.**
2. **The trait is designed against one real implementor.** G1's stub makes "both adapters
   compile" a Wave C gate, but a stub cannot discover what a real adapter discovers. Three
   such discoveries were already made by reading the API during planning (`health` must not
   touch the runner list; there is no plane-wide usage figure; `waiting` is a decision, not
   a state). If Wave D forces a trait change, **D3's single break becomes two breaks in
   consecutive releases.**
3. **G5's rebuilds are the only irreversible step in the cycle.** No transactions, no
   down-migrations. The guard refuses to boot rather than compounding damage, and the
   release note says to back up — but "we told them to" is not a recovery plan. Ship G5 in a
   release of its own, not bundled.
4. **Everything good about the GitHub Actions adapter depends on the target repo being
   instrumented** — a workflow, a hook, and a secret the repo owner must add. D5 cuts three
   secrets to one, which is the single biggest lever available, and N4 ships a
   copy-pasteable reference. But Tack cannot make a stranger's repo instrument itself, and
   an adapter that *technically* proves the trait while *practically* proving nothing about
   the product claim is a real outcome.
5. **M2 turns on a measurement that has always read zero.** See the callout on that card.

---

## II.6 Handoff notes

One section per card, appended on completion: what you changed, what you discovered, and
anything the next wave must know. Same discipline as §6 — record corrections to this file
inline rather than leaving a stale instruction standing, and say plainly when something was
read from source versus verified live.

**Retroactive, all nine subsections below — rule II.0.9 wasn't followed once this wave.**
No card in Wave A or Wave B appended its own note before its session ended; the gap is
what this whole entry exists to close. Everything below was reconstructed on 2026-08-06 by
reading the working tree directly — `git status --porcelain`/`git diff --stat` for the file
inventory, the tests and their golden output, the migrations, the adapter and handler code,
`.github/workflows/ci.yml` — not written live by the cards' own agents, so design
deliberation that never made it into a comment, a test name, or a commit message is gone.
**Verified live in this pass** (run 2026-08-06): `cargo test --workspace` (48 `test result:
ok` blocks, 0 `FAILED`), `cargo clippy --workspace --all-targets -- -D warnings` (clean),
`cargo fmt --all -- --check` (clean), `mdbook build docs/book` (succeeds), plus each card's
own named acceptance test run in isolation where cited below. Everything else — rationale,
why a value is what it is — is read from source comments and test names, not independently
re-derived; where a specific acceptance-bar step was *not* re-run, that's said outright
rather than implied.

### O1 — Tick-level contract oracle — 2026-08-05

**Files:** `crates/tack-orch/tests/docket_tick_contract_test.rs` (1252 lines) and ten golden
files under `tests/golden/tick/` (five scenarios × `.requests.json`/`.rows.json`).

**What's built.** Drives a full `reconcile_once` plus the whole persist phase through the
real `spawn_one` tick loop (not a bare unit call) against `wiremock` + an in-memory SQLite,
across five scenarios — cold start, warm cursor, rewound cursor, zero linked projects, three
linked projects — and snapshots (A) the ordered HTTP request transcript and (B) the
resulting rows of all five `orch_*` tables. Both are compared byte-for-byte against a golden
file; normalisation only ever touches values Tack itself mints at runtime (wall-clock
timestamps, the random `control_planes.id`, and the ids derived/generated from it) — never a
value that came off the wire, which is the property that makes the golden meaningful at all.
Verified live: `cargo test -p tack-orch --test docket_tick_contract_test` — 5/5 pass.

**The load-bearing gap (fact for whoever commits the golden dir).** The plan's own
verification command, `git diff --exit-code crates/tack-orch/tests/golden/`, is **vacuous
right now**. `git status --porcelain` shows the entire `tests/golden/` tree as `??`
(untracked) — confirmed directly. `git diff` never inspects untracked paths, so this command
exits `0` regardless of whether `UPDATE_GOLDEN=1` changed a single byte; today it proves
nothing. This isn't a flaw in the oracle's design — the golden mechanism itself works, see
below — it's that the safety net around it isn't wired to git's plumbing until someone
actually commits the directory. O3's CI step has the identical problem; repeated there too
so it isn't missed by a reader who only opens one card.

**The merge-order break, from O1's side.** Migration 037 (card G5b) renamed
`orch_runs.run_id` to `external_run_id` and added `run_attempt`. This oracle's
`snapshot_rows` originally ran `SELECT * FROM orch_runs ORDER BY run_id` through a generic
row-dumper that only decoded `TEXT`/`REAL` columns — both broke, and all five scenarios
panicked with `no such column: run_id` (the `ORDER BY`) once the rename landed, and would
have panicked again on the first `orch_runs` row regardless (the `INTEGER` `run_attempt`
column, decoded as `Option<String>`, is rejected outright by sqlx's SQLite driver rather than
coerced). Fixed by ordering on `external_run_id` and adding an `i64` decode branch for
`run_attempt` — done during the Wave B final verification pass below, not by an O1-owned
session, since Wave A had no active owner left by the time migration 037 landed underneath
it. `snapshot_rows`'s own comment (lines 730–738) now documents this for the next person who
reshapes `orch_runs`.

**Determinism, proved by copy, not by git** (the command above can't do it): running
`UPDATE_GOLDEN=1` twice in a row against the fixed oracle produced an empty `diff -r` on the
second pass.

**Blast radius confirmed confined.** The only fields that moved across the fix are exactly
what migrations 037/038 touched: `orch_runs` gains `correlation_id`/`run_attempt` and renames
`run_id`→`external_run_id`; `orch_approvals` gains `external_id`/`kind`/`provider_metadata`.
No `.requests.json` transcript changed and `orch_events`/`orch_metrics`/`orch_trace_cursors`
did not change at all.

**Unverifiable claim, flagged rather than assumed true.** The module doc's "Proving the
oracle is real" section (lines 129–142) says the manual mutation-and-revert proof — link zero
projects instead of three inside
`three_linked_projects_issues_three_per_project_calls_each`, confirm the test fails against
the real (three-project) golden, revert — was "Done once, by hand," and points the reader "to
this card's handoff note in TODO.md § II.6 for the exact transcript captured." **No such
transcript existed anywhere in this file before this entry.** O1 never wrote one, consistent
with the wave-wide gap this whole section exists to close. I did not re-run that manual proof
myself in this reconstruction pass (it means deliberately mutating a test file, which is out
of scope for a handoff note); this is a genuine unverified claim, not a confirmed one.
Whoever next touches this file should either produce the transcript for real or correct the
doc comment's claim that one exists.

**For Wave C:** T2/T3 must re-run this oracle after every trait/reconciler change — the
merge-order break above is the concrete proof of what happens when a schema-owning card and
this oracle land out of step, exactly the failure mode §II.5 risk 1 warns about in the
abstract.

### O2 — Per-method wire oracle + pinned event id — 2026-08-05

**Files:** `crates/tack-orch/tests/docket_wire_contract_test.rs` (655 lines) + thirteen
golden files under `tests/golden/wire/`, one per `ControlPlane` method.

**Verified directly:** every one of the thirteen current trait methods has a golden file
(`kind`, `health`, `status`, `metrics`, `list_runs`, `get_run`, `list_approvals`,
`list_tasks`, `traces`, `enqueue_task`, `dispatch`, `decide_approval`, `provision_pod`).
`dispatch.json` holds `"requests": []` and `{"outcome":"err","error":"control plane feature
disabled"}` — the absence of a request is itself the golden, matching the module doc's claim
that this method is captured "not skipped" so a future wiring-up of dispatch fails the golden
the moment it starts issuing a request. `grep -rn "Bearer" crates/tack-orch/tests/golden/wire/`
returns nothing — header **values** never reach a golden file, only names, exactly the
acceptance bar.

**Discovery worth flagging plainly.** Task 39.3 ("pinned-literal event id") does **not** live
in this file, despite what a reader of only the card's **Files:** line would expect. It's
`derive_event_id_matches_the_pinned_literal`, a unit test inside
`crates/tack-orch/src/reconciler.rs`'s own `#[cfg(test)] mod tests` — necessarily so, since
`derive_event_id` is a private function `reconciler.rs` never exports, and this file only
exercises `DocketAdapter` through the public trait. Pinned value for the fixed input is the
literal `4808170d-9797-561e-8fbb-dd8e9b94a9fe`; its own doc comment states plainly that a
future failure should almost always be fixed by reverting whatever changed the derivation,
never by updating the literal — updating it silently re-keys every already-ingested event in
every deployed database. **Wave C's T3 (reconciler restructure) owns `reconciler.rs` next and
needs to know this pinned test lives there** before moving or deleting anything in that file.

**Not independently re-verified:** flipping one byte of the namespace constant to confirm the
pinned test actually fails (the card's own acceptance bar). Read the test and its doc
comment, and the test passes in the full suite run, but I did not perform the mutate-and-
revert exercise myself in this pass.

### O3 — CI gates and the missing coverage floor — 2026-08-05

**Files:** `.github/workflows/ci.yml` (diff read directly via `git diff`).

**Added:** a "tack-orch golden drift gate" step in the `rust` job — `UPDATE_GOLDEN=1`
against both O1's and O2's test binaries, then `git diff --exit-code
crates/tack-orch/tests/golden/` — placed immediately after the existing OpenAPI drift step,
same shape. **Added:** `cargo llvm-cov -p tack-orch --fail-under-lines 70` in the `coverage`
job, with the job's header comment updated from "tack-core >= 85%, tack-db/tack-api >= 70%"
to name tack-orch too. Confirmed there was genuinely no `tack-orch` coverage line before this
diff — it was the least-guarded crate in the workspace, as the card's own brief said.

**Repeating O1's finding here because this card owns the consequence.** The golden-drift step
as written is not wrong, but it is currently inert: `crates/tack-orch/tests/golden/` is
entirely untracked (`git status --porcelain` — `??`), and `git diff --exit-code` against an
untracked path always exits `0`. CI would go green on this exact step today regardless of
what `UPDATE_GOLDEN=1` produced, silently. This is a precondition gap, not a logic bug in the
step — it becomes a real gate the moment somebody `git add`s the golden directory, and not a
moment before, no matter how correct the step's own commands are. Whoever lands that commit
should run the step once against a deliberately broken adapter first, by hand, to see it
actually fail red before trusting it in CI.

**Not verified live in this pass:** `cargo llvm-cov -p tack-orch --fail-under-lines 70`
itself — llvm-cov is a separate, slower toolchain step, skipped for time in a reconstruction
pass. `cargo test --workspace`, `clippy --all-targets -- -D warnings`, and `fmt --all --
--check` were all run and are clean, but that says nothing about line-coverage percentage.
Recommend Wave C's first real CI run be watched on this specific gate rather than assumed
green.

### G1 — Capability model + one adapter registry + GHA stub — 2026-08-05

**Files, per `git diff --stat`:** `crates/tack-orch/src/lib.rs` (+253 lines — `Support`,
`EventScope`, `DecisionSupport`, `UsageSupport`, `ModelSelection`, `Rated<T>`, `Capabilities`,
and `fn capabilities()` added to the trait), `adapters/mod.rs`, two new files
`adapters/registry.rs` (181 lines) and `adapters/github_actions.rs` (229 lines), plus
`orch_store.rs`/`dispatcher.rs`/`handlers/orch.rs`/`handlers/provisioning.rs` updated to call
the registry instead of hand-matching `kind`.

**`Capabilities` matches §II.1.2 field-for-field**, verified against `lib.rs` directly: two
plain bools plus `artifacts`/`runtimes`/`plane_metrics`/`provisioning`, and six `Rated<T>`
fields pairing a level with a `&'static str` reason. `Rated` is deliberately `Serialize`-only
— its own doc comment explains a borrowed `&'static str` reason has no safe `Deserialize`,
and nothing in this crate ever needs to decode one back in from the wire.

**`DocketAdapter::capabilities()` read line-by-line:** `dispatch: true` (goes through
`enqueue_task`, not the trait's own dead `dispatch()` method — a deliberate naming trap the
adapter's own comment calls out explicitly, "`false` here would misreport what docket can do
to satisfy the name of one dead trait method"), `cancel: false`, `pause`/`resume:
Unsupported` naming `docket profile <pod-id> --resume`, `event_scope: Project`, `artifacts:
false`, `decisions: Poll`, `usage: FromProvider`, `model_selection: Unsupported`,
`runtimes`/`plane_metrics`/`provisioning: true`. Every field's comment cites either
`serve.py`'s route table or an earlier "verified live" note (card D4 for `provisioning`), not
a guess. `docket_capabilities_match_the_verified_facts` (`lib.rs:1129`) asserts all of it and
is the exact test the adversarial check below targeted.

**`GithubActionsAdapter`:** `kind()`/`capabilities()` are real and checked against §II.1.4's
verified-external-facts table (pause/resume `Unsupported` — "no endpoint exists"; usage
`NotMeasured` — runner minutes, not tokens; model_selection `Honoured` — a dispatched
workflow receives inputs verbatim); every other trait method is `unimplemented!()`. Confirmed
**not** registered in `registry::build` — deliberately: selecting it today would panic the
reconciler's first poll rather than fail construction with an honest
`RegistryError::UnknownKind`, so the registry's one match arm is still just `"docket"`.

**`registry::build` is now the single caller for all four previously-duplicated
`match row.kind.as_str()` sites** — verified by grep that `orch_store.rs`, `dispatcher.rs`,
`handlers/orch.rs`, and `handlers/provisioning.rs` all import and call
`tack_orch::adapters::registry::{self, RegistryError}`. Each caller keeps its own failure
behavior on purpose (batch `continue`+`warn!` in the reconciler vs. a typed HTTP error in
each handler) — the registry only unifies construction, never what a caller does when
construction fails.

**Acceptance-grep caveat, found by actually running it, not assumed.**
`rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` is not literally `0` as the card's
acceptance bar states — it returns exactly one hit, `handlers/alexa.rs:423`,
`match payload.request.kind.as_str()`. Read the surrounding code: this is Alexa's own
request-kind dispatch (`LaunchRequest`/`IntentRequest`/…), unrelated to control planes, and
confirmed pre-existing — last touched by commit `3ae012e` ("close the WIP-limit race..."),
zero diff on that file in this cycle. The four real duplicated sites are genuinely gone; the
acceptance command as literally worded will fail a naive re-run and needs narrowing (anchor
on `row.kind`, or exclude `alexa.rs`) or a documented exception, or the next person to run it
chases a false alarm.

**Two retirements the card promised, both still open — found by grep, not assumed closed:**

1. `PendingApprovalListResponse.grant_available` is still declared server-side in
   `handlers/orch.rs` and still appears in generated `frontend/src/shared/api/schema.gen.ts`.
   `frontend/src/features/approvals/api.ts`'s own doc comment explains why the *client* side
   stopped reading it (a server-secret-configured flag, not a provider capability — no
   correct home for it in `Capabilities`, and the UI now always renders the decide controls
   and lets a real `403` answer instead of guessing) — but the DTO field itself was never
   deleted. Wave C's T4 inherits `handlers/orch.rs` next and should delete it or document why
   it stays (see also "Wave B final verification," below, which found the same thing).
2. `useAgentActivityMap.orchAvailable()` used as a dispatch gate is **also still live**:
   `frontend/src/features/board/Board.tsx:410` still reads
   `dispatchAvailable={agentActivity.orchAvailable()}`. Not a silent leftover — both
   `useAgentActivityMap.ts`'s doc comment on `orchAvailable` and `DispatchCardMenu.tsx`'s doc
   comment on its `available` prop name this explicitly as "WRONG" and "found [by] card G1,"
   and explain why it wasn't fixed here: `Board.tsx`/`BoardColumnView.tsx`/`ItemCard.tsx`,
   which thread the prop down, sit outside this card's file ownership
   (`frontend/src/shared/orch/**` is what G1 owns), and the real signal
   (`Capabilities.dispatch`) isn't reachable from a hook fed only a bare `projectId` without a
   second network call judged out of scope. Left as a flagged, **not-closed** gap — worth
   naming here since it's the second of exactly the two ad-hoc capability bits §II.1.2 says
   this struct retires, and only one of the two frontend call sites was actually touched.

`frontend/src/shared/orch/capabilities.ts` and `CapabilityNote.tsx` (both new, both G1's)
exist and are what a disabled control is meant to read its reason from going forward —
confirmed present and referenced by `DispatchCardMenu`/`OrchestrationPanel`/
`ProvisioningWizard` via grep, not re-read line-by-line in this pass.

### G2 — Plane config + secrets + `unconfigured` health + backup scrub — 2026-08-05

**Files:** `crates/tack-api/src/remote_backup.rs`, `crates/tack-api/src/orch_store.rs`,
frontend fleet files (`format.ts`, `api.ts`, `HealthChip.tsx`, `FleetRow.tsx` + tests).
`control_planes.config`/`control_planes.secrets` columns themselves are migrations 032/033
(card G5a's territory, below) — this card is their consumer, not their author.

**Secrets scrub, verified directly in `remote_backup.rs`.** `scrub_snapshot_secrets` guards on
`pragma_table_info('control_planes')` for a `secrets` column (so a pre-033 snapshot still
works, per the card's own acceptance bar), runs
`UPDATE control_planes SET secrets = NULL WHERE secrets IS NOT NULL`, and this happens
**before** the trailing `VACUUM` — read the statement order directly, not inferred from the
doc comment. Test `scrub_removes_control_plane_secrets_from_snapshot` asserts the secret
bytes are physically absent from the post-`VACUUM` file, not just that the column reads
`NULL` — the stronger of the two checks the card's acceptance bar allows, and the one this
same test's own comment explains is necessary ("a test that only checked `secrets IS NULL`
would still pass against an implementation that forgot the VACUUM").

**`unconfigured` health, verified in `orch_store.rs`.** `mark_unconfigured` writes
`health = "unconfigured"` (best-effort — a persist failure is logged, never propagated,
matching the batch loop's existing "one plane's problem never aborts the others" discipline)
when `list_registered` can't even build an adapter for a row. Frontend `ControlPlaneHealth`
widened to five states (`'healthy' | 'degraded' | 'unreachable' | 'unknown' |
'unconfigured'`); `isStale()` treats it as stale; `HEALTH_LABEL`/`HEALTH_TONE` both give it
its own copy ("Missing credentials") rather than collapsing into `degraded`'s — a dedicated
test asserts the label differs from `degraded`'s and that all five states are covered.

**Minor attribution note, not treated as a defect.** The doc comment on `mark_unconfigured`
labels itself "Card G1 (TODO.md)" even though this exact behavior is G2's own listed
acceptance bar (tasks 40.2/40.3). Plausible reading: it's attributed to G1 because the
trigger is `list_registered`'s registry-construction-failure path, which is G1's replacement
of the old match statement — not necessarily a real misattribution. Noted in case it signals
two agents working the same seam without comparing notes; not chased further here.

### G3 — Optimistic concurrency — 2026-08-05

**Files:** `crates/tack-db/src/repo/items.rs`, `crates/tack-api/src/handlers/items.rs`, new
test files `crates/tack-db/tests/version_concurrency_test.rs` and
`crates/tack-api/tests/item_concurrency_test.rs`.

**Verified in `repo/items.rs`:** every `UPDATE items SET ...` statement — all field-specific
branches inside `update_item`, both timestamp side-effect branches, `update_item_status_
checked`, and `check_and_update_parent_status` — carries `version = version + 1` in the same
statement. Not just the obvious `update_item` path; the two easy-to-miss ones the card's own
brief called out by name are both covered. `claim_item_version` (the CAS primitive) is a
single statement, `UPDATE items SET version = version + 1, ... WHERE id = ? AND version = ?`
— SQLite's writer serialization, not application-level locking, is what makes "exactly one of
two racers wins" true.

**`handlers/items.rs`:** `item_etag(id, version)` and `check_if_match` implement the
`ETag`/`If-Match` contract. Confirmed the documented "absent `If-Match` behaves exactly as
before this card" bar matches the code — `check_if_match` returns `Ok(None)` ("proceed")
before ever reading `current_version` when no header is present.

**Verified live:** `item_concurrency_test.rs` — 7/7 pass, including
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` and the
higher-fanout variant. (**Naming note:** the card's acceptance bar names this test
`concurrent_patch_with_the_same_if_match_yields_one_200_and_one_412` — singular "patch,"
present-tense "yields." The test as written is
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` — the same
test, cosmetic naming drift, but worth knowing if anyone greps for the exact acceptance-bar
string and comes up empty.) `version_concurrency_test.rs` (repo-layer, 4 tests) is also
green.

**See the Adversarial verification subsection below for a real caveat on this exact headline
test.** An independent adversarial mutation of `check_if_match` (neutering the top-level
comparison while leaving the CAS intact) found the two-racer test flaky against that specific
regression — passed 10/15, failed 5/15 across reruns — while the sequential single-request
tests in the same file caught the identical mutation 100% of the time. The mechanism itself
still works (the sequential tests prove `If-Match` really is enforced); the specific test
named in this card's own acceptance bar is not a reliable solo gate for "someone silently
disabled the comparison." Read that subsection before leaning on this test alone.

### G4 — CORS, routes, and the MCP write path — 2026-08-05

**Files:** `crates/tack-api/src/router.rs`, `crates/tack-cli/src/client.rs` (+256),
`crates/tack-cli/src/mcp.rs` (+217), `crates/tack-api/src/handlers/items.rs` (shared with
G3), `docs/book/src/developer/orchestration.md` (+71 lines, new "Concurrency control"
section).

**CORS, verified in `router.rs`:** `allow_headers` now includes `header::IF_MATCH` and
`header::HeaderName::from_static(orch::APPROVAL_TOKEN_HEADER)` (reusing the handler's own
constant rather than a hand-copied literal, so the two can't drift apart), and
`.expose_headers([header::ETAG])` is added where no `expose_headers` call existed at all
before. New `crates/tack-api/tests/cors_test.rs` (2 tests, both green): one confirms the
preflight allows both headers and exposes `ETag`, the other confirms an arbitrary header is
still rejected.

**MCP write path:** `tack-cli`'s HTTP client gained `patch_if_match`; `mcp.rs`'s
`update_item`/`move_item` tools now `GET` first to read the `ETag`, then `PATCH` with it as
`If-Match`, surfacing a `412` as a distinct "you raced, re-read and retry" tool error rather
than the generic `{status}: {message}` shape. `assert_eq!(tools.len(), 8)` still passes — no
tool was added or removed, only their write behavior changed. Verified live:
`mcp_update_item_sends_if_match` passes.

**Auto-dispatch gate fix:** `handlers/items.rs`'s `maybe_auto_dispatch` now calls
`effective_orch_enabled` instead of the raw `state.config.orch_enable`, closing the gap where
a server started with `TACK_ORCH_ENABLE=1` kept auto-dispatching after an operator turned
orchestration off in Settings. CHANGELOG.md documents it as a behavior change.
**Attribution wrinkle, stated plainly rather than silently resolved one way:** both the
in-code doc comment (`items.rs`, on `maybe_auto_dispatch`) and the CHANGELOG.md entry label
this fix "card G3," but the plan (this file, task 41.3) assigns "fix the auto-dispatch gate"
to **this card, G4**. `handlers/items.rs` is legitimately shared between G3 (ETag/If-Match,
tasks 41.1–41.2) and G4 (this fix, task 41.3) per both cards' own **Files:** lines, so either
attribution is plausible from the file-ownership map alone — I can't determine from the tree
which agent actually wrote it, only that the plan's task assignment and the shipped labels
disagree. Nothing is broken here, only a label; noted so nobody downstream treats the
CHANGELOG's card attribution as authoritative over the plan's own task assignment.

**Documented, not built:** the two writers that bypass `If-Match` entirely — the
reconciler's `apply_mapped_status` call (no `HeaderMap`, no request in flight; the single
largest mutator of `items.status`) and `propagate_parent_completion` (a child's `PATCH` can
bump a *parent's* `version` with no caller having named that row) — both written up in the
new orchestration.md section, framed explicitly as "correct, not a gap," with the warning
that a `412` proves a concurrent write on *that* row only, never a total ordering over every
writer in the system.

**Verified live:** `preflight_allows_if_match_and_approval_token_and_exposes_etag` and
`mcp_update_item_sends_if_match` both pass in isolation; `mdbook build docs/book` succeeds
(see this section's close, below).

### G5a — Wave B additive columns (migrations 032–036) — 2026-08-05

**File:** `crates/tack-db/src/migrations.rs` — this card and G5b together own the file for
the whole cycle, per §II.2.

Five migrations, each verified as exactly one `ALTER TABLE ... ADD COLUMN` statement: 032
`control_planes.config TEXT NOT NULL DEFAULT '{}'`, 033 `control_planes.secrets TEXT`
(nullable — `NULL` means "nothing stored," distinct from 032's own `NOT NULL DEFAULT '{}'`
shape, where an empty-but-present config is the default for every pre-existing row), 034
`items.version INTEGER NOT NULL DEFAULT 1`, 035 `orch_links.version INTEGER NOT NULL DEFAULT
1`, 036 `control_planes.version INTEGER NOT NULL DEFAULT 1`. `DEFAULT 1`, not `0`, on every
version column — a pre-existing row has been written exactly once already (its own
`INSERT`), so `1` is the correct starting point for a client's already-cached `ETag` to
still match right after the migration runs.

033 shipped genuinely inert on purpose: its own comment states the column "MUST STAY UNUSED
... until card G2 adds a `control_planes.secrets` block to
`remote_backup.rs::scrub_snapshot_secrets`" — sequencing enforced by comment and card
discipline, not by the schema itself. Confirmed G2 did land that block (see above) before
anything could plausibly have written to the column for real.

**Not independently re-run:** the acceptance bar's "a seeded DB at 036 upgrades with
identical row counts..." scenario as its own isolated step — covered generally by the green
`cargo test --workspace` pass, but not re-verified as a standalone exercise in this
reconstruction.

### G5b — Table rebuilds (migrations 037–038), the irreversible step — 2026-08-05

Same file as G5a. These are the two migrations §II.0 rule 4 explicitly permits to break the
one-`ALTER`-per-name rule, because neither change is expressible as an `ALTER` at all (see
`migrations.rs`'s own section comment, lines 844–892, for the primary-key argument in full).

**037** rebuilds `orch_runs` around the widened primary key `(control_plane_id,
external_run_id, run_attempt)` plus a new `correlation_id TEXT UNIQUE`, following SQLite's
12-step procedure (`PRAGMA foreign_keys=OFF` → `CREATE ..._new` → an explicit column-for-
column `INSERT ... SELECT` → `DROP` → `RENAME` → recreate `idx_orch_runs_plane_state` →
`PRAGMA foreign_key_check` → `PRAGMA foreign_keys=ON`). Verified the `INSERT` copies every
one of the pre-existing columns (`control_plane_id, item_id, remote_project, source, state,
started_at, ended_at, error, created_at, updated_at`, plus positionally mapping old `run_id`
into `external_run_id`) — nothing silently dropped; `run_attempt` backfills to `1`,
`correlation_id` to `NULL`.

**038** rebuilds `orch_approvals`: `control_plane_id` loses `NOT NULL` (a hook-raised decision
can predate any registered plane), `kind`/`external_id`/`provider_metadata` are added.
`token` deliberately keeps its role as primary key and URL segment. Verified the `INSERT`
backfills `kind = 'approval'` for every existing row (every one of them was, by construction,
exactly that shape before this cycle) and `external_id`/`provider_metadata` to `NULL`/`'{}'`.

**`guard_against_half_applied_rebuild`, verified in `run_all`:** refuses to boot if both a
rebuild's original table and its `_new` staging sibling exist simultaneously — the one state
a crash between `INSERT` and `DROP` could leave behind — naming the backup endpoint in the
error rather than letting a naive retry replay `CREATE ..._new` / `DROP` against whatever
survived. This runs **before** `apply_migrations` touches anything, which is what makes it a
real guard rather than a log line after the fact. CHANGELOG.md carries the operator-facing
backup warning, attributed "(Phase 42, card G5b.)"

**This is the migration the O1 merge-order break traces back to** — restated once more here
since it's this card's own change: the rename of `run_id`→`external_run_id` and the new
`run_attempt` column are exactly what broke `docket_tick_contract_test.rs`'s raw
`SELECT * FROM orch_runs ORDER BY run_id` and its `TEXT`-only generic decoder. That oracle's
`snapshot_rows` now carries an explicit comment naming migration 037 and card G5b as the
reason it orders on `external_run_id` instead — cross-referenced in both directions now,
which is what §II.5 risk 1 and O1's own fix (above) both ask for going forward: a schema
change and its golden regeneration landing in the same session, not two sessions apart.

**Verified live:** `cargo test --workspace` is green including `orch_migrations_test.rs`
(part of the 48/48 `ok` blocks in the full run — not isolated separately in this pass) and
both oracle files under `tack-orch`.

### Wave B final verification — 2026-08-06

Not a card — a verification pass over the six landed cards (G1, G2, G3, G4, G5a, G5b)
plus a re-check of the four files four adversarial checks had already mutated-and-restored
(`adapters/docket.rs`, `remote_backup.rs`, `handlers/items.rs`, `migrations.rs`). All four
read clean: `pause`/`resume` both correctly `Unsupported`, the `control_planes.secrets`
scrub `UPDATE` is present and runs before `VACUUM`, the auto-dispatch gate genuinely reads
`effective_orch_enabled`, and both `INSERT ... SELECT`s in migrations 037/038 carry every
source column. No adversarial residue found; nothing hand-edited back.

**Found and fixed: the primary oracle (O1's `docket_tick_contract_test.rs`) did not
survive G5b's rebuild.** `cargo test --workspace` reported success only because the
`tee | tail` pipeline used to capture it masked cargo's real exit code — the actual run had
`docket_tick_contract_test`'s all five scenarios panicking with `no such column: run_id`.
Migration 037 renamed `orch_runs.run_id` to `external_run_id`; the repo layer
(`tack-db/src/repo/orch.rs`) aliases it back for its own typed queries, but this oracle's
`snapshot_rows` does a raw `SELECT * FROM orch_runs ORDER BY run_id`, which sees the
physical column and fails at prepare time regardless of row count. Fixed the `ORDER BY` to
`external_run_id`, then hit a second failure: `orch_runs.run_attempt` (new, `INTEGER`)
can't decode through the generic `fetch_table` helper's `Option<String>` fallback, so it
gained the same `value`-style type-aware branch `orch_metrics.value` already has. With both
fixed, all 5 tick scenarios and all 13 wire scenarios pass. Regenerated golden and confirmed
the diff against the pre-fix (stale) golden is confined to exactly what migrations 037/038
touched — `orch_runs` gains `correlation_id`/`run_attempt` and renames `run_id` to
`external_run_id`; `orch_approvals` gains `external_id`/`kind`/`provider_metadata` — no
`.requests.json` transcript changed and no other table changed. A second `UPDATE_GOLDEN=1`
run after that diff was empty (determinism proven). **Correction to this file's own
framing:** the FINAL VERIFICATION brief for this pass said "Card G5b legitimately moved
[the golden]" as an already-completed fact, sourced from "G5b's report" — no such report
exists in this section (it was empty before this entry), and the golden actually found in
the tree was still pre-037/038. Whoever ran G5b evidently never re-ran the oracle against
it. Files touched beyond the two fixes: `crates/tack-orch/tests/docket_tick_contract_test.rs`
(owned by O1 per §II.2, edited here only because Wave A has no active owner left and the
oracle was unusable) and the two `.rows.json` goldens it regenerated.

**Found and fixed: one clippy failure.** `crates/tack-orch/src/lib.rs`'s
`rated_serializes_level_and_reason_together` (card G1's own test) borrowed a `Rated` it
didn't need to (`clippy::needless_borrows_for_generic_args`, deny-by-default under
`-D warnings`). One-line fix, `rustfmt`'d, re-verified clean.

**Open item, not fixed here — deliberate, not accidental, but the acceptance grep still
fails literally.** `rg -n "grant_available" frontend/src` returns one hit, in the
*generated* `schema.gen.ts`, because `handlers/orch.rs`'s `PendingApprovalListResponse`
still declares the field server-side. `frontend/src/features/approvals/api.ts` documents
why it was left: it's a server-secret-configured flag, not a provider capability, so there
was no correct home for it in `Capabilities`, and the frontend now always renders
decide controls and lets a real 403 answer instead of guessing — which satisfies rule 6's
intent (no hard-coded gate) without touching the wire shape. Whoever picks this up next
(T4, since it touches `handlers/orch.rs`) should decide whether to delete the field outright
(it has zero remaining readers anywhere in `frontend/src`) or leave it — either way, note
it explicitly rather than let the grep keep silently failing.

**Not Wave B's problem, confirmed pre-existing.** Three frontend Vitest failures
(`client.test.ts`'s `requestBlob`, `panels.test.tsx`'s `DataPanel`, `GlobalSettings.test.tsx`'s
backup download — all `URL.createObjectURL`/`Blob` jsdom-environment mismatches) are in
files with zero changes in this cycle's diff (`git status --porcelain` clean on all three).
Left alone.

**Status board corrected above** — Wave A and B were both still marked "not started"
despite being fully landed; no card had appended a handoff note here either, despite §II.0
rule 9. Wave C: the golden is now verified deterministic and confined to the intended
tables: build on it with confidence, but re-run the tick oracle yourself after T2/T3 rather
than trusting a stale copy — this entry is proof that "should have been regenerated" and
"was regenerated" are not the same claim.

*(cards before this pass did not append their own handoff notes — see the correction above)*

---

### Adversarial verification — Wave B — 2026-08-06

Four independent sessions, each handed one card's shipped code and one hand-picked plausible
regression to inject, told to mutate, run the real test suite, record what caught it (or
didn't), then restore the file byte-for-byte and confirm `cargo test --workspace` is green
again. **All four verdicts: CAUGHT** — none reports a gap that let a real regression through
silently — but each surfaces a genuine weakness worth carrying into Wave C, and one of the
four (C) is a sharper finding than "caught" alone conveys.

**A — lying capability (card G1).** Flipped `DocketAdapter::capabilities().pause.level` from
`Unsupported` to `Supported`. Caught by exactly one test in the entire workspace —
`docket_capabilities_match_the_verified_facts` (`lib.rs:1143`) — confirmed by running the
full suite before and after the mutation, not just the targeted test; nothing at the API,
wire-contract, or frontend-mock layer independently notices a lying capability. Diagnostic
quality is mixed within the same test: the `pause.level`/`resume.level` assertions that
actually fired are bare `assert_eq!`s with no message (the panic text never says "pause" — a
reader needs the source line to learn which field lied), while the neighboring
`reason`-string assertions two lines below carry a real explanatory message
(`"pause's reason must name the docket CLI remedy, got: {:?}"`). A capability whose *reason
text* lies would be caught, and caught clearly; a capability whose *level* lies is caught but
silently as to which field. Cheap fix, not applied here: give the level assertions the same
`format!` treatment the reason checks already have.

**B — unscrubbed secret (card G2).** Commented out the
`UPDATE control_planes SET secrets = NULL ...` while leaving the trailing `VACUUM` intact —
exactly the "forgot the scrub, still ran VACUUM" shape the test's own comment names as the
case a weaker `secrets IS NULL` check would miss. Caught cleanly by
`scrub_removes_control_plane_secrets_from_snapshot`, which asserts the literal secret
substring is physically absent from the snapshot's raw bytes, not just that the column reads
`NULL` — the strong version of the check, and it fired before the weaker null-count
assertion later in the same test ever got a chance to be the only thing standing between this
regression and a green suite. All 18 sibling tests in the module were unaffected. No
diagnostic complaint here: the failure message names the exact leaked string and states
where it was found.

**C — ignored If-Match (card G3) — the sharpest finding of the four.** Neutered the
*comparison* half of `check_if_match` (`if false && provided != item_etag(...)`) while
leaving the underlying atomic `claim_item_version` CAS untouched. Caught overall —
`patch_with_a_stale_if_match_is_rejected_with_412...` and
`patch_with_an_if_match_for_a_different_item_is_rejected` (both sequential, no concurrency)
fail 100% across 5 reruns each, with a clear `left: 200 / right: 412`. **But the test this
card's own acceptance bar names as the headline check** —
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` — **is flaky
against this exact mutation**: 10/15 passes, 5/15 failures (≈33% detection) across reruns in
isolation, because two racers sharing one still-valid version can coincidentally reproduce
the "one 200, one 412" shape via the untouched CAS alone, independent of whether `If-Match`
itself gates anything. The higher-fanout (N=6) variant is more reliable but still not
deterministic (7/10 failures over 10 reruns). Recommended fix, not applied here: stop relying
on two racers colliding on the same live version — force one racer's held ETag to be
known-stale relative to a write guaranteed to land first (the way the two deterministic
sequential tests already do), or inject a forced yield between the version read and the
claim to open the race window on every run rather than leaving it to scheduler luck. This
means G3's own named acceptance test is not, by itself, a trustworthy gate for the exact bug
class it was written to catch — see G3's subsection above, which points back here.

**D — lossy rebuild (card G5b).** Dropped `item_id` from both the column list and the
`SELECT` of migration 037's `orch_runs` rebuild, while leaving `item_id` in the target
`CREATE TABLE` unchanged — a column that survives in the schema but silently loses its data
on upgrade. Caught by `test_migration_037_rebuild_preserves_every_row_and_field_equality`,
which does genuine per-row, per-field equality across all 13 columns (not a row-count check)
and would have caught a silent drop of nearly any column, not only `item_id`. The specific
assertion that fired is a bare `assert_eq!` with no message (`left: None,
right: Some("961a5e85-...")`, no field name anywhere in the output) — roughly 11 of the
test's 13 field assertions share that gap, though the neighboring `run_attempt`/
`correlation_id` checks and a later `cli_run.item_id` check do carry explanatory strings. The
oracle itself is sound; a reviewer reading only CI output, not the test source, cannot tell
which column broke.

**The procedure worked as designed, twice over (worth recording as a positive result, not a
delay).** All four sessions' first attempt aborted at their own setup check —
`cargo test --workspace` was already red before any mutation was applied, because the tree
still carried the merge-order break documented under O1/G5b above
(`docket_tick_contract_test.rs` querying a column migration 037 had already renamed). Every
session correctly refused to draw a verdict against a baseline that wasn't green, rather than
mutating anyway and reporting a result that would have meant nothing against a tree that was
already broken for an unrelated reason. Once that break was fixed (see O1's subsection and
"Wave B final verification," above), all four re-ran clean and reached the verdicts recorded
here. This is the adversarial harness behaving exactly as intended under a real failure, not
friction to route around next time.

---
