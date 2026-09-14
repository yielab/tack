# III-H4 handoff

**What this card changes, in plain language.** When two credential-rotation
requests for the same runner land close together, the one that loses now
learns "someone beat you, retry" instead of "your credential is dead." Before
this fix, the loser's very first check — resolving its bearer token to an
identity — ran a plain lookup that finds nothing once the winner's rotation
has already overwritten the row, so it failed exactly like a genuinely
invalid credential would: `401 unauthorized`, non-retryable. A runner client
that reasonably treats `401` as fatal could stop itself over a harmless race
it happened to lose. Now, specifically for a `/refresh` request that was
itself asking to rotate, that same "not recognized" failure is answered
`409 conflict` (`retryable: true`), matching the outcome the code already
gave when the failure surfaced one step later, in the rotation's
compare-and-set. Proven live in the fixed process: rotate once, then present
the pre-rotation credential again asking to rotate — `409 conflict`, nothing
written; the same replay against the unfixed code reliably reproduces the
CI-reported `401`.

- **Base SHA / branch / final SHA:** base `0e2da46` (tip of `develop` at
  dispatch; matches the board's `84fabf1` plus its own recording commit — no
  real drift), branch `agent/iii-h4-credential-rotation-race`. Final SHA:
  uncommitted at the time of writing (commit requested by the user, not yet
  performed as of this handoff's authoring — see "Checklist" below for the
  intended commit).

- **Files changed (all within Owns):**
  - `crates/tack-api/src/handlers/runner_protocol.rs` — **touched only inside
    and immediately above `refresh()`**, roughly lines 636–730 in the final
    file (the doc comment block right before `refresh`, a new private
    function `reclassify_refresh_auth_error`, and one call-site change
    inside `refresh` itself: `runner_auth::authenticate(...).await?` becomes
    `runner_auth::authenticate(...).await.map_err(|error|
    reclassify_refresh_auth_error(error, &body))?`). Nothing else in this
    2300+ line file was touched — not `enroll()`, not any attempt-scoped
    handler, not the router/mod wiring. **III-H7 works in `enroll()` in the
    same file; this card never opens that function.**
  - `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs` — added a
    private `const CREDENTIAL_NOT_RECOGNIZED_MESSAGE`, renamed the literal
    string `authenticate()` already returned to use that const (no behavior
    change), and added one new public function,
    `is_credential_not_recognized(&Value) -> bool`, plus one unit test for
    it. `authenticate()`'s signature, return type, and behavior for its
    other ~16 call sites are unchanged — verified by the full `c2_handlers_test`
    and `wave2_gate` suites passing unmodified.
  - `crates/tack-api/tests/c2_handlers_test.rs` — added one new test,
    `refresh_rotation_with_already_superseded_credential_returns_conflict_not_unauthorized`,
    directly after the existing (evidence) test at line ~1640. The existing
    test itself is byte-for-byte unchanged.

- **Contract fixtures consumed:** `docs/contracts/runner-v1/errors/conflict.json`
  (`{"error":{"code":"conflict","retryable":true,"details":{}}}`) — the
  remapped response reuses exactly the message and shape the existing
  `CredentialRotationResult::HashMismatch` branch already emits against this
  same fixture; no fixture edit was needed or made.

- **Behavior implemented.** `authenticate()` resolves a bearer credential
  with a single `SELECT ... WHERE credential_hash = ?`. Once a rotation
  commits, the old hash is gone — overwritten in place, not archived — so a
  request still carrying it fails that `SELECT` exactly like a credential
  that was never valid at all; the two cases are genuinely indistinguishable
  by a second query, which is the same ambiguity the pre-existing
  `HashMismatch` branch already accepted rather than resolved (see its
  comment in `refresh()`). The fix extends that accepted policy one step
  earlier: `refresh()` now inspects `authenticate()`'s failure, and if it is
  specifically the "not recognized" case *and* the request body says
  `rotate_credential: true`, the response becomes `409 conflict` instead of
  `401 unauthorized`. Every other `authenticate()` failure (missing header,
  revoked, inactive, expired) and every non-rotating `/refresh` are
  unaffected — those are not the ambiguous case. The `rotate_credential`
  peek reads the raw, not-yet-validated body directly (cheap, no side
  effects); the real, schema-validated parse still runs unchanged afterward
  on the success path.

- **Tests added and exact commands/results.**
  - `cargo test -p tack-api --test c2_handlers_test refresh_rotation_with_already_superseded_credential_returns_conflict_not_unauthorized -- --exact`
    → `1 passed`, and reproduced across 5 repeated runs (fully deterministic,
    no lock/sleep involved).
  - Reverting the fix (`git stash push` on the two source files, test file
    untouched) and rerunning the same command → fails reliably:
    `assertion left == right failed ... left: 401 right: 409`, body
    `{"error":{"code":"unauthorized","message":"The runner credential is not
    recognized","retryable":false}}`. Re-applied (`git stash pop`) and
    reran green. This is the load-bearing proof requested by the card.
  - Full scope: `cargo test -p tack-api` → all 20 test binaries green (no
    count regressions; the new test adds one to `c2_handlers_test`'s total,
    33→34).
  - `cargo test -p tack-api --test openapi_contract` → 5/5, no drift (this
    change adds no new route, field, or status code to any documented
    response shape — `409 conflict` on `/refresh` was already a documented
    possible outcome via the `HashMismatch` branch).
  - `cargo test -p tack-orch --test runner_contract` → 18/18, unaffected
    (this card touches no `runner-v1` fixture).
  - `cargo fmt --check` and `cargo clippy -p tack-api --all-targets -- -D
    warnings` → clean.

- **Failure/adversarial case proved.** The pre-existing evidence test,
  `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten`
  (`c2_handlers_test.rs:1719`), was **not touched, muted, or ignored** — it
  still exists verbatim and still passes. It is worth recording plainly what
  was found investigating it: run locally (in-memory SQLite, 5 repeated
  runs, both with and without this fix applied), it passes either way —
  it does not reproduce the bug in this environment, exactly as its own
  comment and the card's evidence section already say ("does not reproduce
  locally... only CI's contention opens the window"). Its lock-based
  construction forces both requests to unblock at the same instant, but
  which one's `authenticate()` runs first relative to the other's full
  commit is still scheduler-dependent — locally it consistently lands on
  the already-correctly-handled `HashMismatch` ordering, not the
  authenticate-level one this card fixes. Rather than fight that
  non-determinism with a deeper test-only synchronization hook, the new test
  reproduces the *identical code path* deterministically and without any
  timing dependency at all: the server cannot tell "lost a real concurrent
  race" from "presented an already-superseded credential" apart — they are
  the same `SELECT` finding no row — so rotating once and then replaying the
  pre-rotation credential exercises the exact defect on every run. Both
  tests now exist; the new one is the one that actually gates this fix in
  an uncontended environment (e.g. this machine, likely most CI runners
  most of the time), and the original remains as the more realistic
  concurrent-load reproduction for whenever contention is high enough to
  hit it.

- **Schema/API/contract change requested from another owner:** none. The
  fix stays entirely within the ambiguity `HashMismatch` already accepted;
  no new column, table, or contract fixture was needed. (Considered and
  rejected: persisting the previous credential hash to disambiguate "race"
  from "stale" more precisely — would need a migration, and `migrations.rs`
  is a B2-owned chokepoint this card must not touch. Not requested because
  the existing `HashMismatch` precedent already establishes that this
  ambiguity is acceptable, not merely tolerated for lack of a better
  option.)

- **Request to the `tack-runner` owner (not implemented here, per Owns):**
  none required by this fix — the server-side change alone resolves the
  card's acceptance (loser gets `409 conflict`, correctly retryable, instead
  of `401`). If a runner client currently treats `401` from `/refresh` as
  fatal-and-stop, that was already a policy worth checking under the
  pre-existing `HashMismatch` branch (which has returned `409` for a lost
  rotation race since B2), so this card believes there is nothing new to
  request. Flagging anyway per the card's instruction to check, not assume:
  the runner's `/refresh` retry policy was not read as part of this card
  (out of `Owns`); if it turns out to special-case `401` as terminal, it
  should also treat this endpoint's `409` as retryable, which the existing
  `conflict.json` fixture's `retryable: true` already documents system-wide.

- **Known limitations or `not_measured` fields:** the remap is scoped to
  `/refresh`'s "not recognized" authenticate failure only when
  `rotate_credential: true`. A *non-rotating* refresh that loses the same
  underlying race (runner A refreshes without rotating while a concurrent
  rotation elsewhere invalidates its credential mid-flight) still gets
  `401`. This is deliberate — a non-rotating request has no CAS-based
  precedent to extend, and conflating "your credential became stale for
  any reason" with "conflict, retry" more broadly was judged out of this
  card's evidence and scope, not silently limited.

- **Secrets/logging review:** no new logging was added. `authenticate()`
  already logs only `runner_id` on success and nothing on failure (its
  error responses carry no credential material); this card's new function
  reads the already-constructed error `Value` and never touches or logs the
  raw or hashed credential. `logs_never_contain_raw_credentials_only_ids`
  (existing test) passes unmodified.

- **Safe merge order and likely conflicts:** this card's edits to
  `runner_protocol.rs` are confined to the block immediately before and
  inside `refresh()` (see "Files changed" above for exact scope); III-H7's
  work is in `enroll()`, elsewhere in the same file. A merge/rebase of
  either onto the other should not conflict on hunks, only (at most) on
  adjacent-line context if III-H7 also touches the file's shared `use`
  block — worth a diff scan, not expected to be substantive. `runner_auth.rs`
  changes are additive (new const + new fn + new test) with one literal
  substituted for a const it now names — should not conflict with anything
  III-H7 needs there.

- **Checklist:** no unowned files touched (`TODO.md`, migrations, router,
  OpenAPI, other handoffs all untouched); no live secret in tests or logs;
  no panic stub / `unimplemented!()`; no blind retry; the existing failing
  evidence test was not muted, deleted, retried, or `#[ignore]`d.

## Proposed status-board row text (for the wave integrator; not applied here)

III-H4 done (uncommitted at handoff time; branch
`agent/iii-h4-credential-rotation-race`). A losing credential-rotation
request now gets `409 conflict` (retryable) instead of `401 unauthorized`,
whichever of the two points it fails at (`authenticate`'s lookup or the
rotation's own CAS — both now agree). Reproduced with a new deterministic,
non-timing-dependent test (`refresh_rotation_with_already_superseded_credential_returns_conflict_not_unauthorized`);
proven load-bearing by reverting the fix and watching it fail. The
pre-existing CI-evidence test remains, unmodified and still green, but
still does not reproduce locally (confirmed again on this machine, 5/5
green both with and without the fix) — its lock-based race construction is
scheduler-order-dependent and this card did not add a deeper test-only
synchronization hook, judging the deterministic replay test sufficient
proof of the same code path. Gates: `c2_handlers_test` 34/34,
`openapi_contract` 5/5 (no drift), `runner_contract` 18/18 (untouched),
clippy `-D warnings` and fmt clean on the full workspace.
