# III-H8 handoff

- **Base SHA / branch / final SHA:** base `0e2da46` on `develop` (the board names `84fabf1`
  as Wave 8's base; `0e2da46` is a docs-only commit recording III-H5's merge that landed on
  `develop` on top of it — branched from `develop`'s actual tip per the card brief). Branch
  `agent/iii-h8-fleet-write-route`. Final SHA: recorded at commit time (this handoff is
  written before the commit that carries it; see the branch for the actual head).

- **Files changed (must equal ownership list):**
  - `crates/tack-db/src/repo/execution.rs` — new `AddFleetMemberOutcome` enum and
    `Repository::add_fleet_member` / `Repository::remove_fleet_member` (the repository half
    of the card).
  - `crates/tack-api/src/handlers/runner_admin.rs` — new `AddFleetMember` request DTO,
    `FleetMemberResponse` response DTO, `add_fleet_member`/`remove_fleet_member` handlers, and
    two new routes (`POST`/`DELETE .../members[/{runner_id}]`) inside this file's own
    `routes()` — no edit to `router.rs` itself, which already mounts
    `runner_admin::routes(operator_state)` as a whole.
  - `crates/tack-api/src/openapi.rs` — registered the two new handlers and two new DTOs in
    the existing "harness-agnostic runner fleet" section of the `OpenApi` derive. This file
    is normally C5/E6/F4-only per §III.3; the card brief explicitly tasked this card with the
    regeneration, so it is done here and flagged for integrator review below.
  - `docs/openapi.json` — regenerated via `UPDATE_OPENAPI=1 cargo test -p tack-api --test
    openapi_contract`; not hand-edited.
  - `frontend/src/shared/api/schema.gen.ts` — regenerated via `cd frontend && npm run
    gen:api`; not hand-edited. `frontend/node_modules` did not exist in this worktree before
    this card (fresh checkout); ran `npm install` first to get `openapi-typescript` — no
    `package.json`/`package-lock.json` change resulted, so this is not a dependency change.
  - `crates/tack-api/tests/h8_fleet_membership_test.rs` — new, card-owned test file (the
    card's own tests, not touching any other card's test file).
  - `docs/agent-handoffs/part-iii/III-H8.md` — this handoff.

- **Contract fixtures consumed:** none. This card adds an operator-surface CRUD route; it
  does not touch `docs/contracts/runner-v1/**` (the runner-facing wire contract), and no
  fixture in that directory describes fleet membership. `runner_contract` stays 18/18
  unchanged (see Test results).

- **Behavior implemented:** `agent_fleet_members` (`crates/tack-db/src/migrations.rs`,
  migration 041) has existed since card B2 as a live scheduling **read** input — the claim
  path (`Repository::claim_execution_idempotent_with_snapshot`,
  `fetch_runner_scheduling_snapshot`, `fetch_fleet_concurrency`) already joins it to resolve a
  `selector_kind = "fleet"` request onto any of the fleet's runners — but nothing could ever
  write to it. §III.6 requires selecting "an exact runner **or fleet**"; the fleet half was
  therefore undemonstrable end to end (flagged standing since E6, restated by H2's Wave 8
  escalation list because it is release-relevant).
  - `POST /api/runner-fleets/{fleet_id}/members` (body `{"runner_id": "..."}`) adds a runner
    to a fleet's roster. Idempotent: re-adding a runner already in the fleet returns `200`
    with `state: "already_member"` rather than a conflict — repopulating a fleet with an
    already-present runner is the expected operator workflow (e.g. reconciling a roster from
    a config file), not an error. Returns `404 not_found` (with `details.resource` set to
    `"fleet"` or `"runner"`) if either side does not exist, checked explicitly before the
    insert so the caller gets a precise reason rather than a generic foreign-key failure.
  - `DELETE /api/runner-fleets/{fleet_id}/members/{runner_id}` removes a runner from a
    fleet's roster. Returns `404 not_found` if the pair was never a member — a no-op delete is
    reported honestly, not silently accepted as success.
  - Both routes are mounted on the existing operator router (`/api`, behind `require_token`)
    inside `runner_admin::routes()`, alongside the pre-existing `create_fleet`/`list_fleets`/
    `list_runners` routes — no new auth surface, no change to the runner-protocol
    (`/api/runner/v1`) side.
  - `GET /api/runners?fleet_id=` (III-E6) already read `agent_fleet_members` for its roster
    filter; it now reflects real writes instead of always returning nothing for any
    `fleet_id` an operator could name.

- **Tests added and exact commands/results:**
  - `cargo test -p tack-api --test h8_fleet_membership_test` → **2 passed, 0 failed.**
    - `fleet_targeted_request_schedules_onto_a_populated_member_and_not_a_non_member` — the
      acceptance claim itself, driven through the real production router
      (`tack_api::router::build_router`, exactly what `tack serve` mounts), not a card-local
      stand-in: creates a fleet over the API, adds one enrolled runner (`member`) to it,
      leaves a second enrolled runner (`outsider`) out, creates a `selector_kind: "fleet"`
      execution request, has `outsider` claim first (asserts `lease: null` — a non-member gets
      no work) and directly re-reads `execution_requests.state` (`"queued"`) plus a `COUNT(*)`
      on `execution_attempts` for that request (`0`) to prove the outsider's claim attempt
      wrote nothing, then has `member` claim and asserts the returned `request.request_id`
      matches and the persisted `execution_attempts` row's `runner_id` is `member`'s id. Also
      covers the idempotent re-add (`state: "already_member"`) and a real `DELETE` with a
      direct `SELECT EXISTS(...)` against `agent_fleet_members` proving the row is gone.
    - `adding_a_member_to_a_nonexistent_fleet_or_runner_is_rejected_and_writes_nothing` — both
      404 branches (`fleet`, `runner`), plus a `SELECT COUNT(*) FROM agent_fleet_members`
      asserting `0` rows after both rejected attempts, and a `DELETE` of a membership that
      never existed asserting `404 not_found` rather than a silent no-op `200`.
  - Reverted the fix once to prove the primary test load-bearing (rule from CLAUDE.md/`/gate`):
    temporarily replaced `add_fleet_member`'s insert with a no-op that unconditionally
    returned `Added` — `fleet_targeted_request_schedules_onto_a_populated_member_and_not_a_non_member`
    failed at the direct `agent_fleet_members` existence assertion (`"membership row was not
    persisted"`), the second test still passed (it never depends on the insert succeeding).
    Restored the real implementation; both tests green again.
  - `cargo fmt --check` — clean.
  - `cargo clippy -p tack-db -p tack-api --tests -- -D warnings` — clean.
  - `cargo test -p tack-db` — 22 + 5 + 4 passed (execution/status/version test files), 0
    failed, 1 ignored (pre-existing perf test, unrelated).
  - `cargo test -p tack-api` — every test binary green; `h8_fleet_membership_test` included as
    2 of the total. No pre-existing test regressed.
  - `cargo test -p tack-orch --test runner_contract` — 18/18, unchanged (this card touches no
    contract fixture).
  - `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract` — 5/5, including
    `openapi_spec_matches_committed_file`; regenerated `docs/openapi.json` committed alongside
    the source change, never hand-edited.
  - `cd frontend && npm run type-check` — clean (no frontend feature code required a change;
    the new schema types are additive and unused so far — no fleet-membership UI exists yet).
  - `cd frontend && npx vitest run` — 726/726 passed, 85 files (unaffected by this card's
    change; run to confirm the regenerated `schema.gen.ts` didn't break anything downstream).
  - `cargo test --workspace` — **1370 passed, 0 failed** (Wave 8's most recent recorded
    baseline, at the III-H5 merge, was 1368/0 — exactly +2, matching this card's two new
    tests and nothing else moving).

- **Failure/adversarial case proved:** a runner that is enrolled and eligible on every other
  axis (harness/model pairing, live capabilities) but was never added to the fleet a request
  targets gets `lease: null` on claim — proved against real persisted state (`queued`
  unchanged, zero attempt rows), not just the response shape. Adding a member to a
  nonexistent fleet or a nonexistent runner is rejected with `404 not_found` and leaves zero
  rows in `agent_fleet_members` (direct `COUNT(*)`, not just a status-code check). Removing a
  membership that never existed is `404 not_found`, not a silent success.

- **Schema/API/contract change requested from another owner:** none. No migration was
  needed — `agent_fleets`/`agent_runners`/`agent_fleet_members` (migrations 039-041) already
  exist from card B2.

- **Known limitations or `not_measured` fields:** no frontend UI exists yet to drive these
  routes (the existing `frontend/src/features/fleet/runnerFleet/FleetsPanel.tsx` reads
  `GET /api/runner-fleets`/`GET /api/runners` but has no membership-editing affordance) — out
  of this card's scope (API + repository + OpenAPI regeneration only, per `Owns`); a future
  card can wire the UI now that the write route exists. No bulk/replace-roster endpoint was
  added — only single add/remove — matching the granularity of every other write route in
  `runner_admin.rs` (one runner/profile/token per call).

- **Secrets/logging review:** no new logging was added by this card (no `tracing::` call
  sites touched). The new handlers carry only `fleet_id`/`runner_id` (opaque ids, no
  credential material) through error envelopes and responses — consistent with every other
  handler in `runner_admin.rs`. No secret, credential, prompt body or query string is present
  anywhere in the new code path.

- **Safe merge order and likely conflicts:** low conflict risk — this card's only shared
  chokepoint touches (`openapi.rs`, `docs/openapi.json`, `schema.gen.ts`) are additive
  (new entries appended to existing lists/sections), not restructuring. The remaining open
  Wave 8 cards (III-H4: credential-rotation race; III-H6: engine submits events/decisions/
  artifacts; III-H7: duplicate `runner_name` enrollment) touch `runner_protocol.rs` and the
  engine, not `runner_admin.rs` or the fleet-membership tables — no expected overlap. If
  another card also regenerated `docs/openapi.json`/`schema.gen.ts` concurrently, the
  integrator should re-run `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`
  once on the merged tree rather than hand-merging the generated JSON/TS.

- **Checklist:** no unowned files touched (router.rs itself was not edited; migrations.rs was
  not touched — no new migration was needed); no live secret in any test or fixture; no
  `unimplemented!()` or panic stub; no blind retry; every "writes nothing" claim is backed by
  a direct row-count/state read, and the primary test was proven load-bearing by reverting the
  fix once.

## Proposed status-board line (for the wave integrator; not applied here)

III-H8 delivered on `agent/iii-h8-fleet-write-route` (base `0e2da46`): the fleet-membership
write route now exists — `POST`/`DELETE /api/runner-fleets/{fleet_id}/members[/{runner_id}]`
— populating `agent_fleet_members` (migration 041, previously read-only since B2). Proved
live against the real production router: an enrolled runner added to a fleet receives a
fleet-targeted (`selector_kind: "fleet"`) claim; a runner never added to that fleet does not,
confirmed by both the claim response and direct persisted-state reads. `docs/openapi.json`
and `frontend/src/shared/api/schema.gen.ts` regenerated via the documented commands, not
hand-edited (openapi_contract 5/5 drift-free). `runner_contract` 18/18 unchanged (no fixture
touched). §III.6's "exact runner or fleet" selection claim is now demonstrable for the fleet
half; remaining Wave 8 items (H4, H6, H7) are unaffected by this card.
