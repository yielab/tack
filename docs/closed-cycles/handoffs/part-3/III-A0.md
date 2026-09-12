# III-A0 handoff

- Base SHA / branch / card SHA / accepted Wave 0 integration SHA: `1d71785` /
  `agent/iii-a0-contract` / `ce50316` / `f042085`.
- Files changed: `TODO.md` Part III Wave 0 status row;
  `docs/adr/0050-runner-control-plane.md`; `docs/contracts/runner-v1/**`;
  `docs/agent-handoffs/part-iii/{README,III-A0}.md`.
- Contract fixtures consumed: none; this card creates the v1 authority.
- Behavior implemented: documentation-only contract freeze for scheduler/process
  ownership, enrollment/revocation/rotation, protocol compatibility, payload limits,
  lifecycle validation, canonical success exchanges and stable error envelopes.
- Tests added and exact commands/results: `python -m json.tool` was not used to edit files;
  all JSON was parsed with `jq empty` (pass); a local validation script checked that every
  ordered state pair appears exactly once in allow/deny (pass), error fixtures equal
  `protocol.json`'s stable-code set (pass), and no JSON field is named bare `provider`
  (pass); `mdbook build docs/book` passed on the integrated tree and emitted no `error` or
  `broken` link diagnostic (the CI broken-link gate).
- Failure/adversarial case proved: terminal states have no outbound transition; stale
  fencing tokens have a stable non-retryable error; a self-transition is denied even though
  byte-equivalent endpoint replay remains idempotent; revoked runner credentials cannot be
  used as lease authority.
- Schema/API/contract change requested from another owner: B1/B4/C5 must consume these
  fixtures verbatim. B4 should add executable fixture-conformance tests rather than copy
  shapes. C5 owns route/OpenAPI integration. Phase 57 owns widening protocol compatibility.
- Known limitations or `not_measured` fields: protocol v1 accepts only version 1; the
  completion fixture intentionally represents cost as `{value:null,source:"not_measured"}`;
  artifact content storage is bounded but its backend is deferred.
- Secrets/logging review: all credential examples are visibly invalid and prefixed
  `example_`; the ADR forbids credentials, prompt bodies, query strings and complete
  environment values in logs.
- Safe merge order and likely conflicts: merge before B1/B4 and before any route/schema
  authoring. Only the wave integrator may conflict on the Part III Wave 0 status row.
- Checklist: no unowned source files, no live secret, no panic stub, no blind retry.

## Baseline inventory

The named baseline `1d71785` contains 82 paths: 46 previously tracked modifications and
36 previously untracked files. It preserves all Part II work in one reviewable commit:

- CI/release/planning: `.github/workflows/ci.yml`, `CHANGELOG.md`, `README.md`, `TODO.md`,
  `docs/plans/agnostic-control-plane.md`, `docs/book/src/{roadmap.md,developer/orchestration.md}`.
- Generated contract: `docs/openapi.json`.
- `tack-orch`: `src/lib.rs`, `src/reconciler.rs`, four adapter files, two contract tests and
  all 23 `tests/golden/{tick,wire}/**` fixtures.
- `tack-db`: `src/migrations.rs`, `src/repo/{items,orch}.rs`, the expanded orchestration
  migration suite and `version_concurrency_test.rs`.
- `tack-api`: dispatcher/runtime/store/router/OpenAPI plus item/orchestration/provisioning/
  backup handlers and the new CORS, item concurrency and auto-dispatch gate tests.
- `tack-cli`: `src/{client,mcp}.rs`.
- Frontend: approvals, fleet, orchestration settings, agent-activity and dispatch modules
  and focused tests, plus the four new `frontend/src/shared/orch/**` files.

The independent baseline audit recorded Wave 0 inputs rather than hiding them: split CAS
claims/payload writes, incomplete capability-based dispatch gating, stale generated
frontend schema, migration rebuild crash states, and three cross-realm Blob/object-URL
Vitest failures. `cargo test --workspace` passed from the baseline with local mock-server
socket permission; `cargo fmt --all --check` passed; the frontend suite passed 475/478 and
the three failures are card A4's explicit starting fixtures.

## Wave 0 integration acceptance

The wave integrator accepted `f042085d585adfdd8386a2120c7429649883e5df` as the exact
Wave 1 branch point after the combined tree passed:

- `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --all -- --check`;
- 60 frontend test files / 482 tests, TypeScript production build with seven emitted fonts,
  and the design-token lint;
- the complete Playwright matrix: Chromium 49, Firefox 14 and WebKit 14 enabled tests
  passed; 70 non-Chromium a11y/API cases skipped by the existing project policy;
- OpenAPI source/drift tests, migration crash/retry tests, trust redirect and split-origin
  WebSocket adversarial tests, zero docket-golden drift, fixture parsing/lifecycle coverage,
  and `mdbook build docs/book`.

## Additive v1 recovery-observation amendment

- Added canonical `recovery-observation.request.json` and response fixtures for the separately
  authenticated `POST /api/runner/v1/attempts/{attempt_id}/recovery-observation` operation.
  They carry the C3 recovery key, process observation, and non-secret journal evidence, and
  return one authoritative disposition: `safe_pre_spawn_requeue`, `needs_operator`, or
  `already_terminal`.
- `protocol.json` now expressly permits and names additive authenticated operations that leave
  every existing v1 operation's fields, enum meanings, limits, and semantics unchanged.
  `lifecycle-transitions.json` records authoritative recovery disposition and replay rules;
  the existing state-pair rules remain unchanged. ADR 0050 records the route and safety rule.
- Required downstream work: B1 adds shared DTOs; B2 persists/replays recovery outcomes; C2
  implements the fenced route; C3 consumes disposition instead of treating all acknowledgements
  alike; B4 adds fixture conformance. Stable existing errors cover malformed input, stale fence,
  idempotency conflict, revoked runner, invalid transition and internal failures; no error enum
  change is required.

## Accept/start fixture freeze (integrator-authorized cross-card sync with B4)

- Gap: every state-changing runner operation had a paired `*.request.json`/`*.response.json`
  fixture except `accept` and `start`, even though `lifecycle-transitions.json` has always named
  `leased -> preparing` and `preparing -> running` as `lease_owner`-only transitions needing a
  wire mechanism, and `protocol.json`'s `additive_operations` never listed them either. Confirmed
  an editorial omission, not a design choice: III-C2's handoff (gap #5) independently designed
  and shipped the wire shape against B2's `transition_attempt_with_facts` /
  `AttemptTransitionInput` / `AttemptTransitionResponse` and C2's own
  `/attempts/{id}/accept` / `/attempts/{id}/start` handlers, but explicitly flagged that no
  frozen fixture pair backed it and asked A0/D5 to fold it into a future revision.
- Verified the shape directly against the live handler
  (`crates/tack-api/src/handlers/runner_protocol.rs`, `transition_attempt` ~lines 788-856,
  `accept_attempt`/`start_attempt` ~lines 756-786) and `crates/tack-api/tests/c2_handlers_test.rs`
  (the `accept_body`/`start_body` literals and the full-lifecycle test), not from any brief's
  prose. Confirmed: request is `{protocol_version, runner_id, attempt_id, fencing_token,
  workspace_id, base_revision}`, with `start` additionally requiring a non-empty `process_id`
  (rejected `invalid_request` when absent or empty — see `transition_attempt`'s explicit
  `AttemptTransitionPhase::Running` check); response is `{protocol_version, attempt_id, state,
  replayed, committed_at}` with `state` = `"preparing"`/`"running"` respectively, in exactly the
  key order the handler's `json!(...)` construction emits.
- Added `accept.request.json`, `accept.response.json`, `start.request.json`,
  `start.response.json` under `docs/contracts/runner-v1/`, matching the two-space-indent,
  single-trailing-newline style of the most recently added fixture pair
  (`recovery-observation.{request,response}.json`) rather than the older double-trailing-newline
  files, and reusing the same example `runr_`/`att_`/`ws_` ids and base-revision hex already used
  throughout the fixture set. Did not add accept/start to `protocol.json`'s
  `additive_operations`: that section is reserved for genuinely new operations layered onto v1
  after the fact (as `recovery_observation` was in the amendment above); accept/start are core
  canonical lifecycle operations that were always implied by `lifecycle-transitions.json`, so
  filing them there would misrepresent them as a later addition. Updated `README.md`'s operation
  list to name all twelve canonical exchanges and to state explicitly that heartbeat's
  `active_attempts[].state` is a reconciliation report only, never an authoritative transition
  driver, per `lifecycle-transitions.json`'s own recovery-observation `authority` clause.
- Did not change any existing fixture's bytes; only the four new files were added and
  `README.md`'s prose was extended.
- **Cross-card sync, integrator-authorized:** adding four fixtures moved the frozen fixture count
  from 42 to 46, which necessarily broke B4's `crates/tack-orch/tests/runner_contract.rs`
  byte-pinned `FROZEN_FIXTURE_FNV1A64` table and its `paths.len() == 42` assertion. Per this
  card's explicit authorization to keep the tree green, made exactly that minimal sync in
  `crates/tack-orch/tests/runner_contract/fixtures.rs`: inserted
  `("accept.request.json", 0x7c41_cf4c_5a0c_50a0)` and
  `("accept.response.json", 0x9e9d_72b5_565a_783d)` before `artifact.request.json` (alphabetical
  order), appended `("start.request.json", 0x26b7_1d95_5b02_3895)` and
  `("start.response.json", 0x9f07_b1b7_473e_75a3)` after `refresh.response.json`, and changed the
  `assert_eq!(paths.len(), 42, ...)` to `46`. No other line of B4's harness (its four `tests/`
  modules) was touched — this is recorded here and in `III-B4.md` precisely so a later ownership
  audit does not read this as a rule-2 violation.
- Verification: `cargo test -p tack-orch --test runner_contract` — 18 passed, 0 failed (new
  count/hashes accepted). `cargo test -p tack-orch --lib` — 104 passed, 0 failed (another agent
  was concurrently editing `crates/tack-orch/src/execution/{capabilities,mod,types}.rs`; this
  card read but did not touch those files, and the run was clean). `jq empty` on all four new
  fixtures — valid. `cargo fmt -p tack-orch -- --check` — clean. `git diff --check` — clean.
  `git status --porcelain` — only `crates/tack-orch/tests/runner_contract/fixtures.rs`,
  `docs/contracts/runner-v1/README.md` (modified) and the four new fixture files (untracked)
  belong to this card; every other modified/untracked path belongs to other cards' concurrent
  work in the same shared checkout (C1/C2/C5 in `tack-api`, B1/B2 in `tack-db`/`tack-orch/src`,
  their handoffs, and generated OpenAPI/schema output) and was left untouched.
- Schema/API/contract change requested from another owner: none opened by this sync; it closes
  III-C2's gap #5 follow-up request.
