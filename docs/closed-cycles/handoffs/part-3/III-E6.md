# III-E6 handoff

- **Base SHA / branch / final SHA:** base `f0d4ac24fdd763150a9ec0fd31e1332a745402bc` (`f0d4ac2`,
  tip of `plan/harness-agnostic-agent-fleet` at Wave 4 start — "docs: mark E3/E4 landed on the
  Wave 4 status board", already including E1–E5) / `agent/iii-e6-integration` / final SHA
  `8a6e6139d27610712460145a2b766b96fda35bc1` (`8a6e613`), worked in an isolated worktree at
  `/tmp/tack-iii-e6`. Five commits, in order: `755ccae` (scheduler wiring), `f625358` (new
  routes + OpenAPI typing), `9b51fc2` (frontend schema regen), `b6f1f35` (CLI E2E),
  `8a6e613` (live-capability wiring + UI E2E).

- **Files changed:** `git diff --stat f0d4ac2..HEAD` — 25 files, +6665/-2203. Per the card's
  own charter ("scheduler service wiring, route/spec/generated updates, and cross-surface
  E2E"), not a fixed pre-declared list:
  - **Scheduler wiring:** `crates/tack-db/src/repo/execution.rs` (new `RequestSelection`
    enum + three new read-only methods, `claim_execution_idempotent_with_snapshot` signature
    change — see "Behavior implemented"), `crates/tack-orch/Cargo.toml` (+`sqlx` — already a
    workspace dependency, no new crate), `crates/tack-orch/src/scheduler/mod.rs` (+`pub mod
    wiring;` + re-export), `crates/tack-orch/src/scheduler/wiring.rs` (new),
    `crates/tack-orch/tests/scheduler_wiring_test.rs` (new),
    `crates/tack-api/src/handlers/runner_protocol.rs` (claim handler calls the wiring).
  - **Mechanical call-site updates** for the `RequestSelection` signature change (existing
    tests, unowned by me but required to compile — `RequestSelection::Naive` preserves their
    exact original behavior): `crates/tack-api/tests/runner_vertical_slice/repository_crash.rs`,
    `crates/tack-db/tests/execution_repo_test.rs`.
  - **Fixture realism fixes**, required because the real scheduler now checks harness/model
    eligibility these fixtures never exercised: `crates/tack-api/tests/wave2_gate.rs`,
    `crates/tack-api/tests/c2_handlers_test.rs`, `crates/tack-api/tests/c5_integration_test.rs`.
  - **New routes:** `crates/tack-api/src/handlers/executions.rs` (+`list_execution_attempts`,
    `list_execution_attempt_events`, + typed response DTOs for every handler in the file),
    `crates/tack-api/src/handlers/runner_admin.rs` (+`list_runners`, + typed DTOs for every
    handler). `router.rs`/`handlers/mod.rs` needed **no changes at all** — both new routes
    register inside `executions::routes()`/`runner_admin::routes()`'s own `Router::new()`
    chains, already merged into the operator router by the existing C5 wiring.
  - **OpenAPI:** `crates/tack-api/src/openapi.rs` (removed the manual `OperatorApiDoc`
    fragment; every operator handler now carries its own `#[utoipa::path(...)]`, listed
    directly in `ApiDoc`), `docs/openapi.json` (regenerated,
    `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`).
  - **New tests:** `crates/tack-api/tests/e6_routes_test.rs`,
    `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`, `frontend/e2e/scheduler-e2e.spec.ts`.
  - **Frontend generated client:** `frontend/src/shared/api/schema.gen.ts`
    (`npm run gen:api`).
  - **Frontend, outside my nominal charter but required to make the acceptance bar
    checkable at all** (see "A resolved contract ambiguity" below):
    `frontend/src/shared/execution/api.ts` (+`runnersApi.list()`, `RunnerSummary`/
    `RunnerListResult` types — additive, no existing export changed shape),
    `frontend/src/shared/execution/index.ts` (+2 type re-exports),
    `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` (live capability fetch +
    adapter, replacing the hardcoded `[]` default — the `capabilities` prop override every
    existing test uses is fully preserved), `crates/tack-cli/Cargo.toml`/`Cargo.lock`
    (+`chrono` dev-dependency, already a workspace dependency elsewhere),
    `frontend/e2e/helpers.ts` (+`enrollRunner`/`claimOnce`/`createModelProfile`, additive,
    following E4's own established per-feature-helper convention).
  - **Not touched:** `crates/tack-db/src/migrations.rs` (no migration — every field used
    already exists), `.github/workflows/ci.yml`, `docs/adr/**`, `docs/contracts/runner-v1/**`,
    any other card's handoff, `crates/tack-orch/src/scheduler/{select,batch,types}.rs` (E1's
    core algorithm — extended via a new sibling module only), any file under
    `frontend/src/features/fleet/**` (E3's), `frontend/src/shared/runWithAgent/shared.ts` or
    `ExecutionTimeline.tsx` (untouched — see "Known limitations").

## Contract fixtures consumed

`docs/contracts/runner-v1/capabilities.json`/`limits.json` (via reuse of E1's
`SchedulingPolicy::default()`, unmodified) and `lifecycle-transitions.json` (the ten-state
vocabulary, unchanged). No fixture was edited; `cargo test -p tack-orch --test runner_contract`
passes unmodified (18/18, confirmed below).

## Behavior implemented

### 1. Scheduler wiring (task 1)

`tack_orch::scheduler::wiring::choose_request_for_runner(repo, runner_id, now, policy)` is
the new integration seam: fetches a runner's own scheduling snapshot and every
selector-eligible queued request via two new read-only `tack-db` methods
(`fetch_runner_scheduling_snapshot`, `list_eligible_queued_requests`), builds
`RunnerCandidate`/`SchedulingRequest` values, and calls E1's `select_runner`/`schedule`
completely unmodified. `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler
calls it before the fenced claim transaction and passes the result into
`claim_execution_idempotent_with_snapshot` via a new `RequestSelection` enum:
`Naive` (the old `ORDER BY created_at LIMIT 1`, preserved byte-for-byte for every pre-existing
test that doesn't care about scheduling) or `Scheduled(Option<&str>)` (the real path — `Some`
names the chosen request, re-validated inside the transaction before leasing; `None` reports
`no work` without ever falling back to naive selection, which would silently undo the
scheduler's rejection). **The fenced claim write itself is byte-identical to before** — same
`BEGIN IMMEDIATE`, same compare-and-set, same replay handling; only the middle "which request"
decision changed.

Two gaps E1's handoff explicitly left open are resolved:

- **`agent_fleets.concurrency_limit`** (migration 039, previously unenforced anywhere) is now
  honored via `fleet_is_saturated`, a pre-filter in `wiring.rs` that excludes a fleet-selector
  request from ever reaching the pure scheduler if the fleet's aggregate in-use capacity
  (`SUM(total_capacity - available_capacity)` across members) has met or exceeded its limit.
  This intentionally does **not** touch `scheduler/select.rs`/`batch.rs` (E1's owned files,
  respected per this card's own instruction) — the pure scheduler has no notion of a
  cross-runner ceiling and isn't the right layer to add one to.
- **No `priority` column exists on `execution_requests`.** `wiring.rs#priority_from_metadata`
  reads an optional `{"priority": "low"|"normal"|"high"}` key from
  `execution_requests.metadata` (case-insensitive), defaulting to `Normal` (pure FIFO) for a
  missing key, wrong type, or unrecognised value. This is a convention this card introduces
  and documents — not a contract binding on any other card — and is proven load-bearing in
  `scheduler_wiring_test.rs#high_priority_metadata_wins_over_an_older_normal_priority_request`.

### 2. A genuine deadlock found and fixed: heartbeat freshness for a runner's first claim

`agent_runners.last_heartbeat_at` is set **only** by the runner-v1 `/heartbeat` batch, which
reports *active-attempt lease renewal* — a runner with zero active attempts (true of every
runner polling for its very first claim, since enroll/refresh never set this column) has it
`NULL`. E1's pure scheduler treats a missing heartbeat as unconditionally stale. Wiring these
two facts together naively would mean **no runner could ever get its first piece of work,
ever** — discovered by running `wave2_gate_claim_start_stream_complete_and_survive_restart`
against the newly-wired scheduler and watching a previously-passing claim fail.

Resolved by falling back to the capability snapshot's own `reported_at` when
`last_heartbeat_at` is `NULL`: enroll/refresh already requires the runner to attest "as of
this instant, I am alive and this is what I support" — the same liveness claim a heartbeat
makes, under a different operation name — and both are still subject to
`policy.max_heartbeat_age`, so a runner that neither heartbeats nor refreshes for 60+ seconds
still goes stale. Proved both directions in
`scheduler_wiring_test.rs`:
`a_freshly_enrolled_runner_with_no_heartbeat_yet_can_still_claim_its_first_request` and
`a_never_heartbeated_runner_with_a_stale_capability_report_is_still_rejected`.

### 3. New routes (task 2)

- `GET /api/runners[?fleet_id=]` — every enrolled runner's state, capacity, parsed
  `labels`/`capability_snapshot`, and current fleet roster (`fleet_ids`, via
  `agent_fleet_members`). Closes the gap E2, E3 and E5 each independently hit and named
  "Gap 1."
- `GET /api/executions/{id}/attempts` and `GET /api/executions/{id}/attempts/{n}/events` —
  real `execution_attempts`/`execution_events` data (an empty list is an honest "not yet,"
  distinct from a 404 on an unknown id/attempt number). Closes E2's Gap 2 / E4's / E5's
  identically-worded requests.

Both are backed by new read-only `tack-db` methods (`list_runners`,
`list_attempts_for_request`, `list_events_for_attempt_number`) — no write path touched, no
migration needed (every column already exists).

### 4. OpenAPI typing (task 3)

Every handler in `executions.rs`/`runner_admin.rs` (not just the two new ones) now has a real
`#[derive(Serialize, ToSchema)]` response DTO and a `#[utoipa::path(...)]` annotation,
matching the convention every other handler module already follows (`handlers::orch`,
`handlers::items`, …). The hand-built `OperatorApiDoc` fragment (which typed every body as
free-form JSON — the direct cause of the `{}` schemas E2/E3/E4/E5 each independently flagged)
is deleted; these handlers are listed directly in `ApiDoc`'s `paths(...)`/
`components(schemas(...))`. A doc-only `RunnerV1ErrorEnvelope`/`RunnerV1Error` pair documents
the real runner-v1 stable-error envelope these routes return (distinct from this crate's
generic `ErrorEnvelope`) — deliberately defined inside `executions.rs` itself, not
`openapi.rs`, because `c1_handlers_test.rs`/`c2_handlers_test.rs` load that file standalone
via `#[path]`, where a `crate::openapi` import would not resolve.

`docs/openapi.json` regenerated via `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`; `frontend/src/shared/api/schema.gen.ts` regenerated via `npm run gen:api`.
Both are drift-checked clean (see "Tests" below).

### 5. A resolved contract ambiguity (III.2 rule 13): the UI could not submit anything the scheduler could ever claim

While building the UI E2E scenarios, I found that `RunWithAgentModal.tsx`
(`frontend/src/shared/runWithAgent/`) defaulted its `capabilities` prop to `[]` at every real
call site — E2's Gap 1 (`GET /runners`) didn't exist when E4 built it. Per E4's own documented
design, a **specific** model choice is a falsifiable claim and is hard-blocked with no
capability data; **Auto** is never blocked (a legal request shape per III.1.2). The
consequence: the only request shape any operator could ever submit through the real, landed
UI was `Auto`. Independently, E1's scheduler unconditionally rejects `ModelSelector::AutoSelect`
(no capability field attests a harness safely accepts an unspecified model — confirmed still
true; I did not change this). **Combined, these two individually-correct Wave-4 designs meant
no execution request submitted through the real UI could ever be claimed by any runner** — an
integration gap between two cards, invisible to either one in isolation, exactly the kind of
thing this card exists to catch.

**The smallest fix:** `GET /runners` now exists (this card, task 2). I wired it into
`RunWithAgentModal.tsx` as a live fetch used whenever a caller doesn't inject fixture
`capabilities` (every real call site) — a small adapter (`runnerSummaryToCapabilities`)
reconciles the one real shape difference (`RunnerCapabilities` nests `protocol_version`/
`runner_version` inside the report; `GET /runners` reports them as sibling columns, matching
`EmbeddedCapabilitySnapshot`'s own documented reasoning). This is additive: the `capabilities`
prop override every existing E4 test (`RunWithAgentModal.test.tsx`) relies on is fully
preserved (the live fetch is skipped whenever the prop is provided), so no existing test
needed to change.

**A real bug this surfaced immediately:** the adapter's first version treated any
successfully-`JSON.parse`d `capability_snapshot` as valid — but a runner whose enrollment was
created (`POST /runners/enrollment`) and never completed (no `/enroll` call) has the schema
default `capability_snapshot='{}'`, which parses as valid JSON with no `harnesses` field.
Running the pre-existing `a11y.spec.ts` suite (unmodified) against the persistent e2e.db (which
accumulates exactly this kind of row across runs) crashed
`capabilities.ts#harnessProbeStatus` with `TypeError: snapshot.harnesses is not iterable`.
Fixed by validating structural completeness (`Array.isArray(snapshot.harnesses)` plus
`concurrency`/`limits` presence) before accepting a row, treating an incomplete snapshot the
same as "no data" (III.2 rule 7) rather than a fabricated empty capability. The full a11y
suite (35 tests) is green after the fix — this is the adversarial proof that the check is
load-bearing, not the passing suite in isolation.

## Tests added and exact commands/results

- `cargo test -p tack-orch --lib scheduler` — **29 passed, 0 failed** (E1's original 24 +
  this card's 5: `priority_from_metadata_reads_the_documented_convention`,
  `fleet_saturation_reads_the_snapshot_honestly`,
  `unknown_runner_state_maps_to_the_conservative_revoked_reading`, and two
  `build_scheduling_request` malformed-row skip tests).
- `cargo test -p tack-orch --test scheduler_wiring_test` — **13 passed, 0 failed**: healthy
  match, mismatched/undeclared model rejected, no-declared-harnesses rejected, per-runner
  capacity saturation, high-priority-wins, FIFO-within-tier, saturated fleet blocks a
  fleet-selector request, unsaturated fleet allows a member, stale-heartbeat rejection,
  fresh-enrollment-no-heartbeat-yet succeeds, stale-capability-report-no-heartbeat rejected,
  no-queued-work is a clean `None`, unknown-runner-id is a clean `None`.
- `cargo test -p tack-orch --test runner_contract` — **18/18**, unmodified (no fixture
  touched).
- `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed** (all 5, including the
  main lifecycle test, now pass *through the real scheduler*, not the naive match it was
  originally written against — fixtures updated to declare a real, matching capability so the
  gate remains a genuine proof of "claim, start, stream, complete, survive restart" rather
  than accidentally exercising a code path the scheduler bypasses).
- `cargo test -p tack-api --test c2_handlers_test` — **23 passed, 0 failed** (same fixture
  realism fix).
- `cargo test -p tack-api --test c5_integration_test` — **7 passed, 0 failed** (same fix).
- `cargo test -p tack-api --test e6_routes_test` — **6 passed, 0 failed**: `GET /runners`
  returns real capability/capacity data, fleet-id filter, requires operator auth; attempts
  empty-before/populated-after a real claim; attempts 404 on unknown request; events reflect a
  real reported batch and 404 on an unknown attempt number.
- `cargo test -p tack-api --test openapi_contract` — **5 passed, 0 failed**, including
  `openapi_spec_matches_committed_file` (drift-clean).
- `cargo test --workspace` — **1134 passed, 0 failed, 6 ignored** (Wave 4's own accepted
  baseline before this card: 1105 passed. 1105 + 29 = 1134 exactly: this card added 29 new
  Rust tests — 5 scheduler unit + 13 wiring integration + 6 route + 5 CLI E2E — and changed no
  other test's pass/fail count).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean, workspace-wide (appropriate for the integrator per this card's
  own instructions) — confirmed via `git diff --stat` that only files this card touched
  needed reformatting; nothing unowned was reformatted.
- `cargo test -p tack-cli --test e6_scheduler_e2e_test -- --test-threads=1` — **5 passed, 0
  failed**: healthy claim observed via `tack execution get`; a saturated runner leaves a
  second request `queued`; an exact-runner selector excludes a capacity-free bystander; an
  undeclared model is created but never claimed; a changed-payload idempotency replay
  surfaces `idempotency_conflict` through the CLI's exit code. Every operator action shells
  out to the real `tack` binary (`env!("CARGO_BIN_EXE_tack")`) against a real `tack serve`
  subprocess (real SQLite, real production router); runner-side actions use direct HTTP (the
  CLI has no runner-protocol commands, by design — that's `tack-runner`'s job).
- Frontend: `npm run type-check` (`tsc -b`) — clean. `npx vitest run` — **653 passed, 0
  failed** across 77 files (Wave 4's own baseline, unchanged count — confirms the capability
  wiring didn't regress anything, since every existing test injecting fixture `capabilities`
  never triggers the new live fetch). `npm run lint:tokens` — 0/0, clean. `npm run build` —
  clean.
- `npx playwright test --project=chromium` (full suite) — **62 passed, 0 failed**, including
  this card's 4 new `scheduler-e2e.spec.ts` scenarios and all 35 a11y scans (the a11y crash
  described above, now fixed).
- `npx playwright test --project=chromium --project=firefox` (full suite, both engines that
  work in this sandbox) — **83 passed, 41 skipped** (a11y's own by-design chromium-only skip),
  **0 failed**.
- `npx playwright test` (all 3 configured engines) — **webkit fails to launch in this sandbox**
  (`error while loading shared libraries: libwoff2dec.so.1.0.2: cannot open shared object
  file`) — confirmed as a pre-existing sandbox/environment limitation, not a regression: every
  webkit test fails identically, including specs this card never touched
  (`smoke.spec.ts`, `table.spec.ts`, `journey.spec.ts`). No `sudo` available in this worktree
  to install the missing system library. Chromium + Firefox are fully green; webkit's actual
  pass/fail status against this branch could not be established in this environment and should
  be re-verified in CI or a machine with full system dependencies.

## Failure/adversarial case proved

- **The heartbeat-fallback fix is load-bearing, not decorative**: proved by constructing the
  exact real-world state (a runner that just enrolled, zero heartbeats, a queued matching
  request) and confirming the claim succeeds only with the fallback present — the CI-visible
  regression this fix prevents is "every runner's first claim ever fails," which
  `wave2_gate.rs` would have caught on its own the moment the naive fallback was removed
  without this fix (this is literally how I discovered it: wiring the scheduler broke that
  pre-existing, previously-green test).
- **The `{}` capability-snapshot crash** (see "Behavior implemented" §5) was caught by running
  an *existing, unmodified* test suite (`a11y.spec.ts`) against realistic accumulated state,
  not a test written to specifically target the bug — the strongest kind of adversarial proof
  available for a UI regression.
- **Fleet concurrency enforcement**: `a_saturated_fleet_concurrency_limit_blocks_a_fleet_selector_request`
  constructs a fleet at its exact limit (one member fully consumed, `concurrency_limit=1`) and
  proves a *different*, otherwise-fully-eligible member is still rejected — not just "no
  fleet ever schedules," which would be a vacuous pass.
- **Exact-runner exclusivity** (proved at all three layers — Rust integration, CLI, UI):
  an otherwise identically-capable bystander runner, polling with valid credentials and free
  capacity, gets nothing when a different runner is named.
- **Unsupported-model rejection proved at two independent layers**: server-side (CLI test —
  the request is created, but never claimed, forever) and now client-side too (UI test — the
  live capability gate blocks submission entirely, with a named "Unsupported" reason and a
  disabled button) — the second layer only exists because of the capability-wiring fix in §5.

## Schema/API/contract change requested from another owner

Recorded for Wave 5 (or a later integrator), per III.2 rule 2:

1. **`agent_fleet_members` still has no write route through any API surface** — E3's own
   flagged gap, not newly introduced here. I deliberately did not add one: it is out of this
   card's "route/spec/generated updates" scope as I read it (a genuinely new write capability,
   not wiring an existing gap), and every scenario this card's acceptance bar names is provable
   via the exact-runner selector, which exercises the identical downstream scheduler
   eligibility code. **Consequence:** no CLI/UI E2E in this repository can yet prove
   fleet-*selector* routing end-to-end through a live HTTP surface (fleet-*membership*
   eligibility, including `concurrency_limit`, is proven at the Rust integration level instead
   — `scheduler_wiring_test.rs`). A future card adding this route gets a natural, symmetrical
   CLI/UI E2E extension for free (swap `--runner`/`Exact runner` for `--fleet`/`Fleet` in the
   existing test bodies).
2. **`GET /runner-fleets/{id}`** (single fleet + full roster) still doesn't exist — `GET
   /runners?fleet_id=` (this card) is the only membership-roster read path today.
3. **Model-profile `enabled` toggle** — still no route (E3's flagged gap); `ModelProfilesPanel`
   still cannot disable a profile.
4. **`POST /executions/{id}/cancel`'s `"state":"cancellation_requested"` wire inconsistency**
   (E2's Gap 4) — I typed this response (`CancellationRequestedResponse`) but did not change
   its actual value; still not a real `ExecutionState` member. Left as-is per "stay surgical."
5. **Attempts/events frontend wiring** — `GET /executions/{id}/attempts`/`.../events` now
   exist and are tested (this card), but `store.ts#attemptsFor`'s `AttemptAvailability` union
   still only has the `not_available` variant, and `ExecutionTimeline.tsx` still shows that
   placeholder. Wiring them in is now purely mechanical (the route and its shape are proven);
   I did not do it because it ripples into `store.test.ts`/`ExecutionTimeline.test.tsx` and is
   a larger scope than this card's own acceptance bar requires. A natural F4 task.
6. **Model resolution/provenance (III-F3)** — untouched, as instructed.
   `resolveDefaultProvenance()` still returns the typed `not_available` placeholder.
7. **A real `priority` column** — this card's `metadata`-convention fallback
   (`{"priority": "low"|"normal"|"high"}`) is a documented stopgap, not a frozen contract. A
   future B2-successor migration batch should consider a real column and can deprecate the
   convention at that point.
8. **`ModelSelector::AutoSelect` remains unconditionally unschedulable** (E1's own v1 decision,
   confirmed still correct and unchanged by this card) — now with a visible, understood UI
   consequence (§5 above) rather than a silent one. If a future capability field ever attests
   safe auto-select support, `crates/tack-orch/src/scheduler/select.rs`'s
   `ModelSelector::AutoSelect` arm remains the one place to change, per E1's own note.

## Known limitations or `not_measured` fields

- Everything in the numbered list above.
- Webkit's pass/fail status in this sandbox is `not_measured` — the launch failure is an
  environment gap (missing system library, no `sudo` available to install it), not a
  demonstrated app-level result either way. Chromium and Firefox are both fully green.
- `RunnerSummary`/`AttemptSummary`/`EventSummary` (the new typed DTOs) are hand-derived to
  match the real handlers field-for-field, same as every other type in
  `frontend/src/shared/execution/api.ts` — not imported from the newly-generated
  `schema.gen.ts`, per this card's own explicit scope boundary ("you do not need to rewrite
  E2/E3/E4/E5's hand-typed DTOs").
- `GET /runners` has no pagination — acceptable at current expected fleet sizes, not
  documented as urgent, flagged here so it isn't rediscovered as a surprise.

## Secrets/logging review

- `tack_orch::scheduler::wiring` has no `tracing::*!` call of any kind — identical posture to
  E1's own module (pure, synchronous-shaped computation over already-fetched rows; the only
  I/O is the plain `SELECT`s in the two new `tack-db` read methods, which log nothing beyond
  what `#[instrument]` already does elsewhere in that file for methods with an `input`/`clock`
  argument — these new methods take neither, so no `#[instrument]` was needed).
  `RunnerSchedulingSnapshot`/`QueuedRequestForScheduling`/`RunnerListingRow`/
  `AttemptListingRow`/`EventListingRow` carry no credential-shaped field — capacity numbers,
  labels, ids, timestamps, and the runner's own self-reported (non-secret) capability JSON,
  the same category of data the frozen `capabilities.json` fixture itself exposes on the wire.
- `GET /runners`' response never includes `credential_hash`/`credential_expires_at`/
  `credential_rotated_at` — `RunnerListingRow`'s own SQL `SELECT` list omits them entirely
  (not filtered after the fact — never fetched).
- The CLI E2E test (`e6_scheduler_e2e_test.rs`) never logs a raw credential — the runner
  bearer credential returned by `/enroll` is held only in a local `String` variable, used
  exactly once per test as an `Authorization: Bearer` header, never printed via `{:?}` or
  otherwise. The one operator secret in scope (the enrollment token) is captured from the
  CLI's own `--json` stdout and used immediately, matching the existing CLI's own documented
  "one-time reveal" contract (E5's handoff) — never written to a file, never logged.
- The UI E2E test (`scheduler-e2e.spec.ts`) and `helpers.ts#enrollRunner` hold the runner
  credential in a local TypeScript variable for the same single-use purpose; Playwright's own
  trace/video capture (off by default locally, `on-first-retry` in CI) is the only place it
  could theoretically appear on disk, identical to every other credential-touching spec
  already in this suite (`EnrollmentPanel.tsx`'s own e2e coverage).

## Safe merge order and likely conflicts

- This branch never touched `crates/tack-db/src/migrations.rs`, `.github/workflows/ci.yml`,
  `docs/adr/**`, `docs/contracts/runner-v1/**`, or any other card's handoff file — no conflict
  expected there.
- `crates/tack-api/src/router.rs`/`handlers/mod.rs` were **not touched** (both new routes
  register inside `executions.rs`/`runner_admin.rs`'s own `Router::new()` chains, already
  merged by C5's existing wiring) — nothing here should conflict with a future card that
  *does* need to touch `router.rs`.
- `frontend/src/shared/execution/{api,index}.ts` and
  `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` were touched outside this card's
  nominal file list (see "A resolved contract ambiguity"); every change is additive (new
  exports, a new internal resource, no existing exported signature changed shape) and every
  pre-existing test in both files' owning cards (E2, E4) passes unmodified — a future card
  extending either file should not expect a conflict beyond an ordinary adjacent-line merge.
- This is intended to be the final Wave 4 card — no further Wave 4 branches should be rebasing
  onto this one. Wave 5 cards (F1–F5) should branch from this integration's accepted SHA once
  recorded in `TODO.md`.

## Checklist

- [x] No unowned files touched outside what's justified above (migrations, CI, ADRs,
      contracts, other handoffs all untouched; every frontend file outside this card's literal
      charter is justified in "A resolved contract ambiguity").
- [x] No live secret committed, logged, or reachable via `argv`/`ps`/trace (see "Secrets/logging
      review").
- [x] No panic stub / `unimplemented!()` / fake success — every genuinely-missing capability
      (fleet-membership write, single-fleet detail, model-profile toggle, attempts/events
      frontend wiring, model provenance) is a named, typed gap in this handoff and/or the
      product itself, never a placeholder standing in for success.
- [x] No blind retry — the scheduler's `Scheduled(None)` path reports `no work` once and stops;
      nothing in this card's new code retries a failed operation automatically.
