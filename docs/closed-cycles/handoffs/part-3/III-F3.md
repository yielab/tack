# III-F3 handoff

- **Base SHA / branch / final SHA:** base `cbdd4a325a89df3f97bd8bc3009f51024df065fb`
  (`cbdd4a3`, tip of `plan/harness-agnostic-agent-fleet` at Wave 5 start — "docs: close
  out Wave 4 with the III-E6 handoff and accepted integration SHA") / `agent/iii-f3-models`
  / final SHA recorded in the commit containing this handoff (`git log -1` on this branch).
  Worked in an isolated worktree at
  `/home/ox/Sites/objetivosMios/.claude/worktrees/agent-adfee2aa7e239b814`.

- **Files changed (must equal ownership list):**
  - New: `crates/tack-orch/src/model_policy/mod.rs` — the pure precedence resolver
    (`ModelPolicyTier`, `ModelPolicySources`, `ResolvedModelPolicy`, `resolve_model_policy`).
  - New: `crates/tack-orch/src/model_policy/wiring.rs` — the live `tack-db`-backed
    caller (`parse_model_default_convention`, `resolve_request_model_policy`).
  - New: `crates/tack-orch/src/usage_provenance.rs` — model-provenance comparison
    (`compare_model_provenance`, `ModelProvenance`) and provenance-separated usage
    economics (`compute_runner_time_cost`, `build_usage_economics`, `derive_attempt_facts`,
    `RunnerTimeCost`, `UsageEconomics`, `AttemptFacts`).
  - New: `crates/tack-orch/tests/model_policy_test.rs` — the integration proof (see
    "Failure/adversarial case proved").
  - Modified: `crates/tack-orch/src/lib.rs` — exactly two lines added
    (`pub mod model_policy;`, `pub mod usage_provenance;`), alphabetically placed next
    to the existing four `pub mod` lines. No other line touched.
  - Modified: `crates/tack-db/src/repo/execution.rs` — exactly two new read-only methods
    appended (`fetch_agent_profile_limits`, `fetch_fleet_default_policy`), +31 lines,
    0 lines removed or changed elsewhere in the file. `repo.rs` (the module-declaration
    file) was **not** touched — `pub mod execution;` already existed.
  - New: this handoff.
  - `git status --porcelain` confirms exactly this: `M crates/tack-db/src/repo/execution.rs`,
    `M crates/tack-orch/src/lib.rs`, plus the new `model_policy/` directory,
    `usage_provenance.rs`, `tests/model_policy_test.rs`, and this handoff. Nothing under
    `crates/tack-api`, `crates/tack-runner`, `crates/tack-cli`, `frontend/**`,
    `docs/contracts/runner-v1/**`, `crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo.rs`,
    `crates/tack-api/src/router.rs`, `crates/tack-api/src/openapi.rs`,
    `crates/tack-api/src/handlers/mod.rs`, `docs/openapi.json`,
    `frontend/src/shared/api/schema.gen.ts`, root `Cargo.toml`, `.github/workflows/ci.yml`,
    `TODO.md`, or any other card's handoff was touched.

## Contract fixtures consumed

`docs/contracts/runner-v1/completion.request.json` (byte-pinned `usage`/`actual_execution`
shapes — `usage_provenance.rs`'s
`derive_attempt_facts_end_to_end_with_a_real_completion_fixture` test parses this exact
fixture via `include_str!` and asserts the derived facts against its real timestamps/values,
not a hand-typed stand-in) and `docs/contracts/runner-v1/capabilities.json` (indirectly, via
the existing `crate::execution::{ActualExecution, Usage, Measurement, MeasurementSource}`
types this card reuses verbatim — see "Behavior implemented"). No fixture was edited.
`cargo test -p tack-orch --test runner_contract` passes unmodified (18/18, confirmed below).

## Behavior implemented

### 1. Deterministic model-selection precedence (`model_policy/mod.rs`)

`resolve_model_policy(sources: &ModelPolicySources) -> ResolvedModelPolicy` walks a fixed
four-tier order — **request override → agent-profile default → project default → fleet
default → (nothing configured) auto-select** — and returns the first present tier's
`ModelSelector` (reused verbatim from `crate::scheduler::types::ModelSelector`, card III-E1;
not redeclared) plus which tier supplied it. Pure: no I/O, no clock, no database handle.

`ModelPolicySources` carries one `Option<ModelSelector>` per tier. `None` means "this tier
expressed no opinion" (fall through); `Some(ModelSelector::AutoSelect)` is a *different*,
real configuration — a tier explicitly pinning "always auto-select here" — that **stops**
the walk at that tier rather than falling through to a more general tier that might name a
concrete model. `a_tier_explicitly_configured_as_auto_select_stops_the_walk_there` proves
this distinction is load-bearing, not decorative.

### 2. Live wiring to real `agent_profiles.limits` / `agent_fleets.default_policy` (`model_policy/wiring.rs`)

`resolve_request_model_policy(repo, agent_profile_id, fleet_id, request_override)` fetches
each tier's configured default via two new read-only `tack-db` methods and resolves the
final policy. Where each tier's data actually comes from today:

- **Request override**: the caller already has `execution_requests.requested_model_provider`/
  `requested_model_id` in hand (nothing to fetch).
- **Agent profile default**: `parse_model_default_convention` reads an optional
  `{"default_model": {"provider": ..., "model_id": ...}}` (or `{"default_model": "auto"}`)
  key out of `agent_profiles.limits` (migration 042) — a JSON blob **already fully
  operator-settable today** via `POST /api/agent-profiles`'s existing `limits` field
  (`crates/tack-api/src/handlers/runner_admin.rs:create_profile`), confirmed by reading that
  handler before choosing this convention. No schema change; no route change.
- **Fleet default**: the identical convention read out of `agent_fleets.default_policy`
  (migration 039) — likewise already operator-settable via `POST /api/runner-fleets`
  (`create_fleet`, same file). The column's own name ("default policy") is a strong signal
  this is exactly what it was provisioned for.
- **Project default**: **no storage exists.** `projects` (migration 002) has only
  `vocabulary`/`workflow` JSON columns, both semantically owned by the workflow engine —
  reusing either would be a fragile hack, not a documented convention. This tier is fully
  expressed in the pure type (`ModelPolicySources.project_default`) so precedence stays
  complete and future-proof, but `resolve_request_model_policy` always passes `None` for it.
  See "Schema/API/contract change requested" below — this is exactly the "do not build on a
  stopgap without saying so" instruction in this card's brief, applied to project.

This is a **documented convention, not a second frozen contract** — the same posture card
III-E6 established for `execution_requests.metadata`'s `{"priority": ...}` key
(`crate::scheduler::wiring::priority_from_metadata`). `parse_model_default_convention` never
errors: malformed JSON, a missing key, wrong types, or a partial provider/model_id pair all
read as "no opinion," matching that precedent's own posture exactly.

### 3. Capability intersection before claim — proved, not reimplemented

This card does **not** duplicate or modify the runner-capability check. It already exists,
untouched, in `crate::scheduler::select::select_runner`/`evaluate_candidate` (card III-E1)
and is wired to live data by `crate::scheduler::wiring::choose_request_for_runner` (card
III-E6). Once a `ResolvedModelPolicy`'s selector is persisted as an `execution_requests`
row's `requested_model_provider`/`requested_model_id` (exactly what a future `POST
/executions` integration would do before enqueue — see "Schema/API/contract change requested"
for the exact call site), the existing claim path enforces "unavailable choice never leases"
automatically. `crates/tack-orch/tests/model_policy_test.rs` proves this end-to-end through
real repository methods — see "Failure/adversarial case proved."

### 4. Requested-vs-actual model provenance (`usage_provenance.rs`)

`compare_model_provenance(requested, actual_provider, actual_model_id) -> ModelProvenance`
returns one of three variants, comparing via `.as_str()` (never conflating the *requested*
namespace — `RequestedModelProvider`/`RequestedModelId` — with the *actual* namespace —
`ActualModelProvider`/`ActualModelId` — exactly as `evaluate_candidate` already does for
requested-vs-declared, per III.0's vocabulary rule):

- `Matched { provider, model_id }` — the attempt ran on exactly what was requested.
- `AutoSelectObserved { actual_provider, actual_model_id }` — the request allowed
  auto-selection and the attempt observed a concrete choice; distinct from `Matched` (nothing
  was requested to match) and from `Mismatched` (nothing was contradicted).
- `Mismatched { requested_provider, requested_model_id, actual_provider, actual_model_id }` —
  both sides carried in full, never silently reconciled. Proved visible both in Rust
  (`mismatch_carries_both_requested_and_actual_values`) and on the wire (same test asserts
  the serialized JSON has both `requested_model_id` and `actual_model_id` present
  simultaneously and unequal).

### 5. Runner time cost, structurally separate from model/token cost (`usage_provenance.rs`)

`UsageEconomics` carries two independently-provenanced dollar dimensions that are **never
summed**:

- `model_token_cost_usd_estimated: Measurement<f64>` — a verbatim pass-through of the
  completion's `Usage.cost_usd` (card B1's frozen wire type, matching
  `completion.request.json` exactly). Provenance: the harness/vendor's own self-report.
- `runner_time_cost: RunnerTimeCost { wall_clock_ms: Option<u64>, cost_usd_estimated:
  Measurement<f64> }` — this card's own derived dimension. `wall_clock_ms` is a fact the
  runner/API directly witnesses (`execution_attempts.started_at`/`ended_at`, migration 045,
  as opposed to the harness's own self-reported `Usage.duration_ms`) — always derivable once
  both timestamps exist, never itself `Measurement`-wrapped (there is no "estimated" wall
  clock). `cost_usd_estimated` stays `not_measured` unless a caller supplies an infra rate —
  **no such rate is stored anywhere in this schema today** (see "Schema/API/contract change
  requested"); `runner_rate_usd_per_hour` is always caller-supplied, never invented.

`present_usage_and_timestamps_produce_real_values` proves the two dimensions are genuinely
independent, not accidentally equal: 30 minutes of wall clock at a supplied $3.00/hour
produces `runner_time_cost.cost_usd_estimated = Some(1.5)`, asserted `!=` the harness's own
`model_token_cost_usd_estimated = Some(0.42)` in the same fixture.

### 6. One convenience "service handler": `derive_attempt_facts`

Takes the same raw column shapes `tack_db::repo::execution::AttemptListingRow` already
carries (`actual_execution`/`usage` as possibly-absent raw JSON text, plus started/ended
timestamps and the request's resolved requested provider/model) and returns
`AttemptFacts { model_provenance: Option<ModelProvenance>, usage_economics: UsageEconomics }`
in one call — the shape a future handler can wire directly to `GET
/executions/{id}/attempts`'s existing `AttemptSummary` (card III-E6,
`crates/tack-api/src/handlers/executions.rs`) without re-deriving anything. Malformed JSON in
either raw column (should not happen — both are written by this codebase's own completion
handler — but a raw `TEXT` column has no schema enforcement) is treated as "not yet
reported," never a panic (`derive_attempt_facts_treats_malformed_json_as_not_yet_reported`).

## Tests added and exact commands/results

- `cargo test -p tack-orch --lib model_policy` — **14 passed, 0 failed**: the exhaustive
  2^4 = 16-combination precedence table (all as one parametrized test,
  `every_presence_combination_resolves_to_the_pinned_precedence_order`), all-absent →
  auto-select, an explicit-auto tier stops the walk, 25x-repeated-call determinism, nonsense
  opaque ids (ASCII punctuation, unicode with combining/emoji code points, a 10,000-character
  id) round-tripping through both resolution and raw JSON unmodified, and the
  `parse_model_default_convention` unit suite (explicit default, explicit auto, unrecognised
  literal, missing key, malformed JSON, partial pair, wrong value types, nonsense ids inside
  the convention).
- `cargo test -p tack-orch --lib usage_provenance` — **9 passed, 0 failed**: matched/
  mismatched/auto-observed provenance (including the wire-visibility assertion), nonsense ids
  surviving a mismatch comparison, absent usage never serializing as zero (full structural
  JSON equality plus a literal `"0"`/`"0.0"` substring scan), the positive control proving
  real inputs produce real non-null/non-equal values, wall-clock-known-without-a-rate, malformed-JSON-is-not-yet-reported, and an end-to-end derivation against the real, byte-pinned
  `completion.request.json` fixture.
- `cargo test -p tack-orch --test model_policy_test` — **6 passed, 0 failed**: precedence
  resolution against real `agent_profiles`/`agent_fleets` rows (fleet default read from the
  real `default_policy` column; agent-profile default beats a fleet default; a request
  override beats both; no tier configured resolves to auto-select), plus the two
  load-bearing pipeline tests below.
- `cargo test -p tack-orch --test runner_contract` — **18/18**, unmodified (no fixture
  touched).
- `cargo test -p tack-orch --test scheduler_wiring_test` — **13/13**, unmodified (this
  card's changes to `tack-db`/`tack-orch` are additive; E1/E6's scheduler and its wiring are
  untouched and this suite proves it).
- `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed**, unmodified.
- `cargo test --workspace` — **1163 passed, 0 failed, 6 ignored**. Wave 4's accepted
  baseline (III-E6) was 1134 passed; 1134 + 29 = 1163 exactly — this card added 29 new Rust
  tests (14 `model_policy` lib + 9 `usage_provenance` lib + 6 `model_policy_test`
  integration) and changed no other test's pass/fail count.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean, workspace-wide; confirmed via `git status --porcelain` before
  and after `cargo fmt -p tack-orch -p tack-db` that only this card's own new/modified files
  needed reformatting (nothing unowned was touched).

## Failure/adversarial case proved

**"Unavailable choice never leases" — the card's own load-bearing safety claim** —
`crates/tack-orch/tests/model_policy_test.rs`:

- `a_fleet_default_model_the_runner_does_not_declare_never_leases`: a fleet's configured
  default model (via `agent_fleets.default_policy`'s `{"default_model": ...}` convention) is
  resolved through this card's own `resolve_request_model_policy`, persisted as the queued
  request's `requested_model_provider`/`requested_model_id` (exactly what a wired create-path
  would do), and run through the real, **entirely unmodified** claim path: E6's
  `choose_request_for_runner` (and its re-exported alias — proving there is only one, not a
  second drifting copy) returns `None`, and B2's
  `claim_execution_idempotent_with_snapshot` returns `Ok(None)`. The test then asserts the
  absence **directly against the database**, not just the function's return value:
  `SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?` is `0`, the request's
  `state` is still `'queued'`, and the runner's `available_capacity` is unchanged at `1`
  (proving the transaction's capacity reservation was rolled back, not partially consumed).
- **Proved load-bearing by reverting and watching it fail**, per `CLAUDE.md`'s discipline:
  I temporarily changed the fleet's configured default from an undeclared model
  (`opaque/UNAVAILABLE-model`) to the runner's actually-declared one
  (`opaque/model-alpha`) and reran just this test. It failed exactly as expected:
  ```
  thread 'a_fleet_default_model_the_runner_does_not_declare_never_leases' panicked:
  assertion `left == right` failed: an undeclared fleet-default model must never be chosen for a claim attempt
    left: Some("req-unavailable")
   right: None
  ```
  I then reverted the change and reran the full suite to confirm green again (see "Tests
  added" above). This is the literal "delete the guard, watch it fail, restore it" proof —
  applied here to the *wiring that feeds the unavailable model into the claim path*, since
  the actual gating logic lives in E1's `select.rs`, which this card does not own or touch.
- `a_fleet_default_model_the_runner_does_declare_leases_successfully` (the positive control):
  identical setup except the fleet's default *is* the declared model — `choose_request_for_runner`
  returns `Some(request_id)`, the claim returns `Some(ClaimedExecution)`, exactly one
  `execution_attempts` row now exists, the request's `state` is `'leased'`, and the runner's
  capacity is now `0`. This rules out the negative test being vacuously true because claiming
  never works in this harness for an unrelated reason.

**"Absent usage never serializes as zero"** —
`usage_provenance.rs#absent_usage_never_serializes_as_zero` asserts full structural JSON
equality against a literal expected value (`{"value": null, "source": "not_measured"}` for
both dollar dimensions, `"wall_clock_ms": null`), plus a belt-and-suspenders scan for the
literal substrings `":0"`/`":0.0"` anywhere in the serialized output.
`present_usage_and_timestamps_produce_real_values` and
`wall_clock_is_known_even_without_a_configured_rate` are the positive controls proving the
null-everywhere case is not simply a vacuous "always null" implementation.

**"Nonsense id round-trips"** — three independent test sites (precedence resolution,
JSON round-trip of the opaque wrapper types, and inside the `default_model` convention
parser) each feed `"totally-made-up-provider-9000"`, a string mixing Japanese characters,
`::`, emoji and a combining diacritic, and a 10,000-character id, and assert the exact
original bytes come back at every stage — never normalized, rejected, or coerced.

## Schema/API/contract change requested from another owner

Recorded here per III.2 rule 2, for a future integrator (this card intentionally does not
touch `router.rs`/`openapi.rs`/any handler's route registration):

1. **`projects` has no default-model-policy storage.** No column or JSON blob exists that is
   semantically appropriate to reuse (`vocabulary`/`workflow` both belong to the workflow
   engine). `ModelPolicySources.project_default` is fully modeled in the pure type and
   `resolve_request_model_policy` always passes `None` for it — this tier is real in the
   precedence order but inert until storage exists. A future migration batch (B2-successor,
   per III.3's ownership table) should add either a real `projects.default_model_policy TEXT`
   column or explicitly decide against a project tier; I did not choose between these, since
   both are schema/policy decisions outside this card's ownership.
2. **No runner infra cost-rate is stored anywhere.** `runner_time_cost.cost_usd_estimated`
   (`usage_provenance.rs`) is always `not_measured` unless a caller supplies
   `runner_rate_usd_per_hour` explicitly — there is no `agent_runners`/`agent_fleets` column
   or settings surface for an operator to configure one today. If a future card wants this
   dimension to ever read `estimated` in production, it needs a place to store that rate
   first; I did not invent one.
3. **Nothing in this codebase yet calls `resolve_request_model_policy` from a live HTTP path.**
   The natural integration point is `crates/tack-api/src/handlers/executions.rs`'s
   `create_execution` handler: when the client's `requested_model_provider`/`requested_model_id`
   are both `None`, call
   `tack_orch::model_policy::wiring::resolve_request_model_policy(&state.repo,
   Some(&input.agent_profile_id), fleet_id_if_selector_is_fleet, None)` and use its
   `.selector` (converted back to the two nullable wire fields — `AutoSelect` → both `None`,
   `Explicit { provider, model_id }` → both `Some`) in place of the client's own absent
   values, before the existing `NewExecutionRequest`/`enqueue_execution` call. I deliberately
   did **not** make this change myself: `create_execution` is a shared, actively-evolving
   handler (E6 already extended it once this cycle), this card's own brief says "route
   mounting requests go in your handoff for the Wave 5 integrator," and the acceptance bar
   ("unavailable choice never leases") is fully provable — and proved above — without it, by
   exercising the identical downstream repository/claim code directly. Wiring it in is now
   purely mechanical; the resolution and claim-time gating are both proved correct
   independently.
4. **`GET /api/executions/{id}/attempts`'s `AttemptSummary` (III-E6) does not yet expose
   `model_provenance`/`usage_economics`.** `derive_attempt_facts` (this card) is built to
   drop into that handler directly: call it with the row's `requested_model_provider`,
   `requested_model_id`, `actual_execution`, `usage`, `started_at`, `ended_at` (all already
   fetched by `list_attempts_for_request`) and an optional runner rate, and add its two output
   fields to the response DTO. I did not make this change — it touches a shared handler file
   and DTO, which is exactly the kind of change this card's brief reserves for the wave
   integrator (F4). See "What F4 needs" below for the precise shape.
5. **Model-profile (`model_profiles` table, migration 043) is not consulted by this card's
   precedence chain at all.** The card's own task list names four tiers — request override,
   agent profile, project, fleet — none of which is "a named, reusable model profile." I did
   not invent a fifth tier or fold `model_profiles` into one of the four; if a future design
   wants "pick a saved model profile by id" as an input to any tier (most naturally the
   request override, or as an alternative agent-profile representation), that is a scope
   decision for whoever owns that UI/API surface, not something I inferred silently.

## Known limitations or `not_measured` fields

- Every numbered item above.
- `runner_time_cost.cost_usd_estimated` is `not_measured` in every real call site in this
  repository today (no rate is ever supplied anywhere yet) — by design, not a bug; see item 2
  above.
- `model_token_cost_usd_estimated` is `not_measured` whenever a harness itself does not
  report `cost_usd` (the frozen `completion.request.json` fixture itself demonstrates this:
  its own `usage.cost_usd` is `{"value": null, "source": "not_measured"}`) — this card passes
  that provenance through verbatim, never upgrading or downgrading it.
- This module is not wired to any real HTTP call site yet (item 3 above) — every type and
  function here is built to make that wiring mechanical, but no handler in this repository
  constructs a `ModelPolicySources` or calls `resolve_request_model_policy`/
  `derive_attempt_facts` from live request data yet. That is explicitly the next integration
  step, per this card's own "route mounting requests go in your handoff" instruction.

## Secrets/logging review

- Neither `model_policy/mod.rs`, `model_policy/wiring.rs`, nor `usage_provenance.rs` contains
  a single `tracing::*!` call — all three are pure/read-only modules (the only I/O is two
  plain, unlogged `SELECT`s in the two new `tack-db` methods, matching the existing
  unlogged-read convention `fetch_fleet_concurrency`/`fetch_runner_scheduling_snapshot`
  already established in the same file). There is therefore no log output to redact.
- Every type introduced carries only ids, provider/model strings (opaque, never a credential
  — the same category of data `capabilities.json` and `completion.request.json` already
  expose on the wire), timestamps, and dollar/millisecond figures. No `PermissionPolicy`,
  `EnvironmentValue`, or `RunnerCredential`-adjacent type is referenced anywhere in this
  card's code.
- The two new `tack-db` methods (`fetch_agent_profile_limits`, `fetch_fleet_default_policy`)
  select only the `limits`/`default_policy` columns — no credential-shaped column exists on
  either `agent_profiles` or `agent_fleets` in the first place.

## Safe merge order and likely conflicts

- This branch never touched `crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo.rs`,
  `crates/tack-api/src/router.rs`, `crates/tack-api/src/openapi.rs`,
  `crates/tack-api/src/handlers/mod.rs`, `docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts`, `docs/contracts/runner-v1/**`,
  `.github/workflows/ci.yml`, `TODO.md`, root `Cargo.toml`, or any other card's handoff — no
  conflict expected there.
- `crates/tack-orch/src/lib.rs` gained exactly two alphabetically-placed `pub mod` lines.
  This card's own sibling Wave 5 branches (III-F1, III-F2, III-F5, per the worktree list
  active alongside this one) do not appear to need this same file based on their card
  descriptions (decisions/events-artifacts/retention modules, not model policy) — if one
  does add its own `pub mod` line here, it is a trivial adjacent-line merge, not a semantic
  conflict, exactly as III-E1's own single-line addition merged cleanly.
- `crates/tack-db/src/repo/execution.rs` gained two new methods appended after the existing
  `fetch_fleet_concurrency` method, before `list_runners`. This file is large (2601 lines
  after this change) and has been extended by multiple prior cards (B2, E6) via pure
  appends of new methods — the established safe pattern. A sibling card that also appends new
  read-only methods to this file should merge as an ordinary adjacent-append, not a semantic
  conflict.
- The next natural consumer is whichever card wires `create_execution` (item 3 above) and/or
  extends `AttemptSummary` (item 4) — most likely F4, the Wave 5 integrator, per the
  dependency graph (`F1 F2 F3 F5 → F4`).

## Checklist

- [x] No unowned files touched — confirmed via `git status --porcelain` (see "Files
      changed" above): exactly `crates/tack-orch/src/lib.rs` (two lines),
      `crates/tack-db/src/repo/execution.rs` (two new methods), the new `model_policy/`
      module, `usage_provenance.rs`, the new integration test, and this handoff.
- [x] No live secret: this card's code has no logging surface and touches no
      credential-shaped column or type (see "Secrets/logging review").
- [x] No panic stub: no `unimplemented!()`/`todo!()`/bare `panic!()` in non-test code;
      every "cannot resolve this" case (malformed convention JSON, malformed attempt JSON,
      an absent project-policy tier) returns a typed `None`/absence, never a crash.
- [x] No blind retry: this card's code contains no retry logic of any kind — every function
      is a single pure computation or a single read, by design.
