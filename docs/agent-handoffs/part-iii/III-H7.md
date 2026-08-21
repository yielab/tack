# III-H7 handoff

**What this card changes, in plain language.** Before it, a second runner
enrolling on the same host as a first, both left at their default
configuration, could not join the fleet at all — the server crashed with an
unhandled 500 instead of enrolling it or explaining why not. The cause: the
runner's own self-reported name (which defaults to the same value,
`TACK_RUNNER_ID`, for any two default-configured runners) was silently
overwriting the distinct name the operator had already assigned each pending
runner, so the second runner's enrollment collided with the first's on a
database uniqueness rule nobody meant to enforce there. Now the operator's
assigned name is the one that sticks — the runner's self-report is accepted
as required protocol shape but never allowed to compete with it — so two (or
more) default-configured runners enroll cleanly side by side. As a
defense-in-depth measure, if this code path ever does hit a genuine name
collision (from a cause not reachable today), it now returns the same typed,
documented `conflict` outcome the sibling admin route already uses, instead
of a raw 500.

- **Base SHA / branch / final SHA:** base `develop` at `0e2da46` (matches the
  board's stated Wave 8 base, `84fabf1`, plus the docs-only commit already on
  `develop`'s tip — no real drift). Branch `agent/iii-h7-duplicate-enrollment`.
  Final SHA: uncommitted at the time of writing — no commit was requested to
  be reported here; see the housekeeping line in the report for actual
  commit status if the caller committed after this was written.
- **Files changed (all within Owns):**
  - `crates/tack-api/src/handlers/runner_protocol.rs`:
    - `fn enroll` (the whole function, currently lines 586–664 after this
      change) — the `.map_err(...)` on the `redeem_enrollment_token` call
      now classifies `sqlx::Error` via the new `is_unique_violation` helper:
      a unique-constraint violation becomes `StableErrorCode::Conflict` /
      HTTP 409 with message "A runner with this name is already enrolled";
      everything else stays the pre-existing `internal_error`. No other line
      in `enroll` changed.
    - New private fn `is_unique_violation(&sqlx::Error) -> bool`, placed
      directly after `fn internal_error` (currently lines 466–474) — a local
      twin of `runner_admin.rs::is_unique_violation` (that one is private to
      its own module, so this is a duplicate, not a shared import).
    - **Nothing in `fn refresh` or the credential-rotation helpers changed.**
      III-H4 (branch `agent/iii-h4-credential-rotation-race`, commit
      `33dbd09`, not merged as of this branch's base) owns
      `reclassify_refresh_auth_error` and the `refresh()` call site above it,
      plus `is_credential_not_recognized` and its test in
      `handlers/runner_protocol/runner_auth.rs`. This card never touched
      `runner_auth.rs` and never touched any line inside `refresh()`. The
      integrator should diff both branches against `runner_protocol.rs` to
      confirm the two edits land in disjoint line ranges (`enroll`/
      `is_unique_violation` here vs. `refresh`/`reclassify_refresh_auth_error`
      there) before merging both.
  - `crates/tack-db/src/repo/execution.rs`, `fn redeem_enrollment_token`
    (currently lines ~1997–2039): the `runner_name` parameter is renamed
    `_runner_name` (still accepted and still passed by the handler, per the
    protocol's required field, but no longer read); the `UPDATE agent_runners
    ... SET ...` statement no longer sets `name=?` and no longer binds
    `runner_name`/`_runner_name`. A doc comment above the function explains
    why. No other statement, branch, or return value in the function
    changed.
  - `crates/tack-api/tests/c2_handlers_test.rs`: one new test,
    `duplicate_self_reported_runner_name_enrolls_both_runners`, inserted
    after `full_runner_protocol_lifecycle_enroll_through_completion` (before
    `fn request_created_before`). No existing test in this file was modified.

## Contract fixtures consumed

- `docs/contracts/runner-v1/errors/conflict.json` — the typed-error shape
  the defensive branch now returns (`code: "conflict"`, `retryable: true`),
  matching the same fixture `runner_admin.rs::create_pending_runner` and
  several other `runner_protocol.rs` handlers already use. No fixture was
  edited; none needed to be — `conflict` already existed and this card uses
  it exactly as documented.
- `docs/contracts/runner-v1/enrollment.request.json` /
  `enrollment.response.json` — unchanged; `runner_name` remains a required
  request field (still validated present) and the response shape is
  untouched. No contract edit was needed because this card changes what the
  server *does with* an already-valid field, not the wire shape itself.

## Behavior implemented

1. **Defect 2 (root cause, fixed):** `redeem_enrollment_token`'s `UPDATE`
   no longer writes the enroll body's self-reported `runner_name` into
   `agent_runners.name`. The operator-assigned name from
   `create_pending_runner_and_issue_token` is authoritative and untouched by
   enrollment. Two runners that self-report the identical name (the
   `TACK_RUNNER_ID` default case) now both enroll successfully, each keeping
   its own operator-assigned name.
2. **Defect 1 (defense-in-depth):** the `enroll` handler's error mapping on
   the `redeem_enrollment_token` call classifies `sqlx::Error` the same way
   `runner_admin.rs::create_pending_runner` already does for its INSERT: a
   unique-constraint violation maps to the frozen `conflict` outcome (HTTP
   409, `StableErrorCode::Conflict`); every other database fault stays
   `internal_error` (HTTP 500). After defect 2's fix, `name` is the only
   `UNIQUE` column on `agent_runners` and it is no longer touched by this
   statement, so this branch is not reachable through today's public API —
   it is kept as the same defensive posture the sibling admin route already
   relies on, documented as such in the code comment, in case a future
   migration adds another unique column this statement writes.

## Tests added and exact commands/results

- `cargo test -p tack-api --test c2_handlers_test duplicate_self_reported_runner_name_enrolls_both_runners`
  → `ok. 1 passed; 0 failed`.
- New test creates two pending runners with **distinct operator-assigned
  names** and distinct enrollment tokens, then sends two `/enroll` requests
  that differ **only in `enrollment_token`** (the exact III-H2 repro shape)
  and both self-report the **identical** `runner_name`
  (`"default-runner-id"`, standing in for two default-configured runners
  sharing one `TACK_RUNNER_ID`). Asserts: both return `200`, the two
  returned `runner_id`s differ, and each row's stored `name` column still
  equals its operator-assigned value (`operator-assigned-a` /
  `operator-assigned-b`) — the self-reported name never lands in storage.
- `cargo test -p tack-api` (whole crate): `ok`, no failures, 100% of the
  suite files passed (see Test results below for the count comparison).
- `cargo test -p tack-db`: `ok`, no failures.
- `cargo test -p tack-orch --test runner_contract`: `18 passed; 0 failed`
  (unchanged from the board's H5 baseline — no fixture touched).
- `cargo test -p tack-api --test wave2_gate`: `5 passed; 0 failed`.
- `cargo test -p tack-api --test openapi_contract`: `5 passed; 0 failed`,
  spec drift-free (this card added no route and no response shape to any
  route that is wired into the global OpenAPI spec — the card-local
  `runner_protocol` router stays unregistered per C5's plan, unchanged by
  this card).
- `cargo fmt --check`: clean. `cargo clippy --workspace -- -D warnings`:
  clean.
- `cargo test --workspace`: **1369 passed, 0 failed** — exactly +1 over the
  board's recorded H5-merge baseline (1368/0), the one test this card added.
  No test count dropped anywhere in the workspace.

## Failure/adversarial case proved

Reverted the fix twice to prove the new test is load-bearing, not
decorative:

1. Reverting only `crates/tack-db/src/repo/execution.rs` (the name-overwrite
   removal), keeping the handler's new error classification: the second
   enrollment now returns **409 `conflict`** (`"A runner with this name is
   already enrolled"`) instead of 200 — test fails on
   `assert_eq!(status_b, StatusCode::OK, ...)`. This proves the handler's
   defensive classification correctly recognizes and types the exact
   collision defect 2 causes, even though it no longer fires in the shipped
   code.
2. Reverting both changed files back to `develop`'s tip: the second
   enrollment reproduces the **exact original bug** — `500`,
   `{"code":"internal_error","message":"Could not redeem enrollment
   token",...}` — byte-for-byte the failure mode III-H2 originally reported.
   Restoring both files returns the test to green with no other change.

## Schema/API/contract change requested from another owner

None. No migration, router, or OpenAPI change was needed for this fix.

## Known limitations or `not_measured` fields

- The defensive `conflict` branch added in `enroll`'s error mapping is not
  independently exercised by a test that reaches it through the public API
  — after defect 2's fix, `name` is the only `UNIQUE` column this statement
  writes, and it no longer writes it, so there is no reachable trigger left
  to test honestly. This is disclosed rather than hidden: writing a test
  that reaches an unreachable branch (e.g. by bypassing the public
  creation path to manufacture illegal DB state) would not prove anything
  the code will ever actually do. The classification helper itself
  (`is_unique_violation`) is a direct copy of `runner_admin.rs`'s own,
  already-precedented logic (SHA-256/`sqlx`'s `is_unique_violation()` on the
  underlying database error) — its correctness rests on that precedent
  matching, not on a new test.
- This card does not change what happens if an *operator* races two
  `create_pending_runner` calls with the same assigned name — that path was
  already typed (409) before this card, unchanged here.

## Secrets/logging review

- The new `tracing::warn!("enrollment rejected: runner name already in
  use")` on the defensive-conflict branch carries no runner id, no token, no
  name, and no other field — it is a static string, satisfying "logs carry
  ids only" (nothing to redact because nothing beyond a fixed message is
  logged here; there was no id available yet at this point in enrollment
  since the row update failed).
- No other log statement was added or changed. The pre-existing
  `tracing::info!(runner_id = %runner_id, "runner enrolled")` and
  `tracing::warn!("enrollment token rejected: invalid, expired, or already
  used")` are unchanged.

## Safe merge order and likely conflicts

- **Shares `crates/tack-api/src/handlers/runner_protocol.rs` with III-H4.**
  This card's only edits are inside `fn enroll` and the new
  `is_unique_violation` helper (placed right after `fn internal_error`).
  III-H4's edits (per the coordination note this card was given) are
  confined to `fn refresh` and a private `reclassify_refresh_auth_error`
  function placed just above it, plus its own file
  `handlers/runner_protocol/runner_auth.rs` (untouched by this card). The
  two changes should not textually overlap; a plain 3-way merge should
  succeed without conflict markers. If it doesn't, the integrator should
  treat any conflict as a signal that one of the two "confined to" claims
  (this one or III-H4's) was inaccurate, not silently resolve it either way.
- No conflict expected with any other Wave 8 card — none of III-H5, H6, H8
  touch `runner_protocol.rs` or `crates/tack-db/src/repo/execution.rs`'s
  `redeem_enrollment_token`.

## Checklist

- [x] No unowned files touched (`runner_protocol.rs`'s enrollment path,
      `execution.rs`'s `redeem_enrollment_token`, this card's own test file
      and handoff only).
- [x] No live secret logged (see Secrets/logging review).
- [x] No panic stub, no `unimplemented!()`.
- [x] No blind retry introduced.
