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
