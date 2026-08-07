# III-A0 handoff

- Base SHA / branch / final SHA: `1d71785` / `agent/iii-a0-contract` / branch tip (the
  integrator records the cherry-picked SHA because cherry-picking changes it).
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
  (pass); `mdbook build docs/book` (recorded below after execution).
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

