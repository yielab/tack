# IV-A4 handoff

- Base SHA / branch / final SHA: base `cace43f0ac02ac189339b554c78024d6fcbfa35d` (develop tip
  at worktree creation, includes IV-A1/A2/A3), branch `agent/iv-a4-zero-touch-enrollment`, final
  SHA — see the last commit on this branch.
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/runner_admin.rs`
  (extraction only, route behavior unchanged — see below), `crates/tack-cli/src/local_enrollment.rs`
  (new), `crates/tack-cli/src/local_runner.rs` (minimal additive hook into
  `serve_with_embedded_runner`, as the card's own Tasks require), `crates/tack-cli/src/main.rs`
  (one line, `mod local_enrollment;`, alongside the existing `mod local_runner;` — required
  because `local_enrollment` follows IV-A3's own established pattern of declaring these modules
  from the binary crate root rather than `lib.rs`), plus `crates/tack-api/tests/c1_handlers_test.rs`
  (two new tests directly exercising the new `provision_local_runner` function — see "Tests
  added" below for why this was necessary, not optional), and this handoff. Nothing else touched.
- Contract fixtures consumed: none. This card never touches `docs/contracts/runner-v1/` — the
  redemption side of enrollment is completely unmodified; only the *admin* side (creating a
  pending runner and minting its one-time token) gained an in-process caller.

## Behavior implemented

**Task 1 — extraction (`runner_admin.rs`).** `create_pending_runner`'s validation, id
generation, hashing and repo call were moved into a new `pub async fn provision_pending_runner
(repo: &Repository, clock: &dyn ExecutionClock, input: CreatePendingRunner) -> Result
<CreatePendingRunnerResponse, ProvisionError>`. `ProvisionError` is a new plain enum (one
variant per validation/repo failure the original handler distinguished). The axum handler
`create_pending_runner` is now a thin wrapper: call `provision_pending_runner`, then match each
`ProvisionError` variant back to the exact `(StatusCode, StableErrorCode, message, details)`
tuple the original inline code produced — verified byte-identical by the existing route tests
passing untouched (see "Tests added").

**Task 1b — the in-process entry point (`runner_admin.rs`).** A second new function,
`pub async fn provision_local_runner(database_url: &str) -> Result<CreatePendingRunnerResponse,
ProvisionLocalRunnerError>`, opens its own short-lived `SqlitePool` against `database_url` (via
`tack_db::init_pool`, the same function `tack_api::server`'s boot sequence calls), builds a
`Repository` and a `SystemExecutionClock`, and calls `provision_pending_runner` with a fixed
single-local-runner shape: `name: "local-<fresh uuid>"` (so repeated self-provisioning, e.g. a
retry after a crash between provisioning and storing the session, can never collide with an
earlier still-pending runner), `total_capacity`/`available_capacity: 1`, default labels/
capability_snapshot/protocol_version/lifetime (the same defaults `CreatePendingRunner`'s serde
defaults already use). This lives in `tack-api`, not `tack-cli`, by deliberate choice — see
"Design decision" below.

**Task 2 — the in-process caller (`local_enrollment.rs`, new).** `pub async fn
self_provision(database_url: &str) -> anyhow::Result<EnrollmentCredential>` calls
`runner_admin::provision_local_runner`, logs `runner_id`/`token_id`/`expires_at` (never the raw
token) at `info`, and returns the raw one-time token wrapped in `EnrollmentCredential` (whose
`Debug`/`Display` are unconditionally redacted). `pub fn has_stored_session(state_dir: &Path) ->
bool` checks `state_dir.join("session.json").is_file()`.

**Task 3 — wiring (`local_runner.rs`).** `serve_with_embedded_runner` no longer calls
`runner_config.require_enrollment_credential()?` before starting the server (that line is
deleted — it is what made manual enrollment mandatory). Instead, after the server signals
readiness and the runner config's `api_base_url` is pointed at the real bound address, a new
`ensure_runner_credential(&mut runner_config, &server_config)` runs:
1. a manually configured `enrollment_credential` wins unconditionally and is left untouched;
2. otherwise, if `local_enrollment::has_stored_session(&runner_config.state_dir)` is true, the
   config's credential is set to a placeholder (see "Design decision — the placeholder
   credential" below) and `self_provision` is **not called at all** — no new pending runner, no
   new one-time token minted;
3. otherwise, `local_enrollment::self_provision(&server_config.database_url)` is called and its
   result becomes `runner_config.enrollment_credential`.

If step 3 fails, `server_task.abort()` is called and the error is returned — the server never
runs without a runner it was asked to have, matching the same "fail loud" discipline
`supervise` already applies once both tasks are running.

**Task 4 — loopback gating.** `ensure_runner_credential` runs after `ensure_loopback` has
already passed inside `serve_with_embedded_runner` (structurally — it is only ever reached
after that check), so self-provisioning inherits the loopback-only rule rather than
re-deriving it. No new gate was added; see "Loopback/gating proof" below for how this is shown,
not just asserted.

**Task 5 — owner-only credential storage.** Unmodified: `transport.rs::store_session` already
writes `session.json` via `write_owner_only` (mode `0600` on Unix). Confirmed on a real run
(see "Loopback/gating" and "Which role executed what" below): `-rw------- 1 ox ox 196 ...
session.json`.

## Design decision — where `provision_local_runner`'s database access lives

The card's Task 2 offered two framings for how `local_enrollment.rs` reaches state: reuse what
the server built, or construct an equivalent repo/clock handle. Reusing the server's state would
need a new seam in `crates/tack-api/src/server.rs` (not owned by this card, and `serve_with_ready`
does not hand its `Repository` back to the caller) — not done, per the card's own instruction to
escalate rather than edit that file.

Constructing an equivalent handle is what happens, but the pool-opening code lives in
`tack-api`'s `runner_admin.rs` (`provision_local_runner`), not in `tack-cli`'s
`local_enrollment.rs`, for two concrete reasons:
1. **`CLAUDE.md`'s own architecture map states `tack-cli` is "HTTP only, never opens the DB."**
   Giving `local_enrollment.rs` a direct `tack_db::init_pool` call would make that false.
   `provision_local_runner` keeps that boundary intact the same way `tack_api::serve_with_ready`
   already does: `tack-cli` calls one library function that happens to open a database, exactly
   as it already does for the server role.
2. **`crates/tack-cli/Cargo.toml` and root `Cargo.lock` are owned by IV-A3 (then IV-A5 for one
   subcommand arm), not this card.** `tack_db::Repository`/`ExecutionClock`/`SystemExecutionClock`
   are not currently reachable from `tack-cli` without a new direct dependency on `tack_db`
   (they are used by `tack-api` internally but not re-exported at its crate root). Since
   `tack-api` already depends on `tack_db`, adding `provision_local_runner` there avoids a
   Cargo.toml change this card does not own. `local_enrollment.rs` only ever imports
   `tack_api::handlers::runner_admin` and `tack_runner::EnrollmentCredential`, both already
   reachable through `tack-cli`'s existing dependencies.

This was not flagged as a `server.rs` escalation because it did not need one — no new seam into
`server.rs` was required once the pool-opening logic moved one file over, inside a file this
card already owns for a different reason (the extraction).

## Design decision — the placeholder credential for a stored session (discovered gap)

The card's Task 3 says a stored session should be reused "unchanged (don't self-provision,
don't touch the config's credential)". While implementing this literally, I found that
`tack_runner::bootstrap::build_runtime` (in `crates/tack-runner/src/bootstrap.rs`, owned by
IV-A1, not this card) calls `config.require_enrollment_credential()?` as its **first
statement**, unconditionally, before it ever inspects `state_dir` — its own doc comment says
so explicitly ("Fails fast on a missing enrollment credential before any filesystem side effect
... "). The actual "prefer a stored session over the credential" logic lives entirely inside
`transport.rs::establish_session`, which `build_runtime` does not reach until well after this
check. Concretely: **today, even without this card, restarting the standalone `tack-runner`
binary (or `tack runner start`) against an already-enrolled `state_dir` with no
`--enrollment-token`/`TACK_RUNNER_ENROLLMENT_TOKEN` fails immediately** with
`MissingEnrollmentCredential`, never reaching the stored session at all. This predates IV-A4 and
is not introduced by it; `bootstrap_entrypoint.rs`'s own
`build_runtime_fails_fast_on_a_missing_enrollment_credential` test documents the same
unconditional check (its fixture happens to use a fresh `state_dir`, so it never exercised the
"session exists" case either).

Given this, a literal "leave the credential untouched" implementation would make `bootstrap::
run` fail on every second start — the opposite of the acceptance criterion. Since
`bootstrap.rs`/`config.rs` are owned by IV-A1 and not editable here, `ensure_runner_credential`
instead sets the credential to a fixed placeholder
(`local_enrollment::stored_session_placeholder()`, the string
`"stored-session-on-disk-no-token-needed"`) whenever a stored session is found, and does **not**
call `self_provision`. This is safe by construction, not just in practice:
- `EnrollmentCredential`'s `Debug`/`Display` are unconditionally `"[REDACTED]"` regardless of
  content, so the placeholder is exactly as safe to hold as a real token.
- On the path this card's acceptance actually exercises — a valid stored session — `transport.rs
  ::establish_session` calls `refresh()` first and only reads `enrollment_credential` if that
  refresh is rejected by the server. The live proof below confirms the placeholder is never
  transmitted: round 2's log shows a `POST /api/runner/v1/refresh` and **no** `POST
  /api/runner/v1/enroll` at all.
- If the stored session were ever rejected (revoked, expired), `establish_session` would fall
  through to `require_enrollment_credential()` and attempt to enroll with the placeholder, which
  the server would correctly reject — a loud failure, not a silent one, which is the same
  outcome a real stale token would produce.

**Recommendation for whoever owns `bootstrap.rs` next:** `build_runtime`'s credential check
could instead be deferred into `establish_session`'s fallback branch (or check `state_dir` for a
session before requiring a credential), which would remove the need for this placeholder
entirely and fix the same restart friction for the standalone binary, not just the embedded
case. Not done here — out of this card's ownership.

## Discovered gap — embedded-process tracing filter (not fixed, not blocking)

`tack_api::server::init_tracing`'s default `EnvFilter` (used whenever `RUST_LOG` is unset) is
`"tack_api={level},tack_db={level},tack_core={level},tower_http=debug"` — it does not include
`tack` (the binary crate `local_runner.rs`/`local_enrollment.rs` compile into) or `tack_runner`.
Confirmed live: my own `self_provision` info line (target `tack::local_enrollment`) never
appears with default settings, and neither would any of `tack_runner::transport`'s own existing
"runner enrolled"/"runner session resumed" lines — this affects every embedded-runner log line
from `tack-runner`'s and `tack-cli`'s own code, not just this card's addition, and predates it
(IV-A3 wired the embedded runner into the same process; nothing in IV-A3 changed this filter
either). Setting `RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info`
makes it visible (verified). Not fixed here: `server.rs` is not owned by this card and this is a
pre-existing default that predates IV-A3 exercising it. Worth a line in IV-A6's operator docs or
a small follow-up card broadening the default filter now that non-`tack-api` code runs in the
same process.

## Tests added and exact commands/results

- `crates/tack-cli/src/local_enrollment.rs`, `#[cfg(test)] mod tests` (3 tests, deterministic,
  no network/DB): `has_stored_session_is_false_for_an_empty_directory`,
  `has_stored_session_is_true_once_session_json_exists`,
  `has_stored_session_is_false_when_state_dir_does_not_exist_yet`.
- `crates/tack-cli/src/local_runner.rs`, `#[cfg(test)] mod tests` (3 new `ensure_runner_credential`
  tests, on top of IV-A3's existing 5):
  - `ensure_runner_credential_leaves_a_manual_credential_untouched` — a manual credential plus a
    deliberately unparseable `database_url` still succeeds and the credential is byte-identical
    to what was configured.
  - `ensure_runner_credential_reuses_a_stored_session_without_self_provisioning` — a stored
    `session.json` plus the same unparseable `database_url` still succeeds (proving
    `self_provision`, which would fail against that URL, was never called) and the credential
    becomes `Some(_)` (the placeholder).
  - `ensure_runner_credential_attempts_self_provisioning_when_nothing_else_is_available` — no
    manual credential, no stored session, same unparseable `database_url` → `Err`. Paired with
    the previous test, this proves the branch selection is real: the identical
    `database_url` is untouched in one case and reached-and-failed in the other.
- `crates/tack-api/tests/c1_handlers_test.rs` (2 new tests). **Why these were necessary, not
  optional:** this file loads `runner_admin.rs` via `#[path]` as its own module tree (a
  pre-existing pattern for direct-handler-call testing, not a router). In that recompiled unit,
  `cargo clippy -D warnings` flagged `provision_local_runner`/`ProvisionLocalRunnerError` as
  dead code the first time I ran it, because nothing in that specific test binary called them
  yet (the real `tack-api` library target has no such warning — dead-code analysis there
  correctly treats `pub` items as externally reachable). Rather than suppress the lint, I added:
  - `provision_local_runner_writes_through_its_own_pool_to_the_same_database` — opens a real
    file-backed sqlite DB, runs migrations (mirroring the server's own boot sequence), calls
    `provision_local_runner` against that same `database_url`, and confirms the resulting
    `runner_id` is visible via a **second**, already-open pool against the same file — the exact
    property `provision_local_runner`'s own doc comment claims (WAL mode makes a second pool
    against the same file safe).
  - `provision_local_runner_fails_on_an_unparseable_database_url` — negative-space companion,
    confirms the `ProvisionLocalRunnerError::Pool` variant on a bad URL.
- Commands and results (via `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/IV-A4`):
  - `cargo fmt -p tack-api -p tack-cli` → clean (reformatted `runner_admin.rs`'s new imports;
    no semantic change).
  - `cargo clippy -p tack-api -p tack-cli --all-targets -- -D warnings` → clean.
  - `cargo test -p tack-api` → all targets pass, including `c1_handlers_test.rs`: `9 passed` (7
    pre-existing + 2 new), 0 failed.
  - `cargo test -p tack-cli` → lib target `57 passed` (unchanged from IV-A3 — `local_enrollment`/
    `local_runner` are binary-only modules), `main.rs` unit-test target `11 passed` (5
    pre-existing `local_runner` tests + 3 new `has_stored_session` + 3 new
    `ensure_runner_credential`), `cli_test.rs` `11 passed`, `e6_scheduler_e2e_test.rs` `5 passed`
    (unaffected). 0 failures.
  - `cargo build --workspace` → clean.
  - `cargo test -p tack-api -p tack-cli` (combined final run) → every target `ok`, 0 failed, 0
    ignored, across all listed test binaries.

## Failure/adversarial case proved

- **Extraction did not change the route.** `c1_handlers_test.rs`'s pre-existing
  `enrollment_token_is_returned_once_hash_only_and_revoke_or_redeem_blocks_reuse` (hits
  `create_pending_runner` through the real HTTP router, asserting status codes, the exact
  response shape, and that the raw token cannot be recovered from any other endpoint) passed
  **untouched** — no edit to that test was needed or made.
- **Second start really does skip self-provisioning, not just "happen not to see a call."**
  `ensure_runner_credential_reuses_a_stored_session_without_self_provisioning` proves absence
  directly: it uses a `database_url` that fails at parse time (`tack_db::init_pool` never even
  attempts a connection), and the function still returns `Ok`. If self-provisioning were
  wrongly invoked on this path, the test would see `Err` — proven by its sibling test
  (`ensure_runner_credential_attempts_self_provisioning_when_nothing_else_is_available`), which
  uses the identical bogus URL and does see `Err`, showing the pair is not tautological.
- **Live, adversarial-shaped run:** see "Which role executed what" below — round 2's server log
  contains zero `POST /api/runner/v1/enroll` requests (only `refresh`), proving no second token
  was minted or redeemed, and `GET /api/runners` stayed at exactly 1 row across both rounds.

## Schema/API/contract change requested from another owner

None to `docs/contracts/runner-v1/` or `docs/openapi.json` — this card never touches the wire
contract. Two non-blocking findings recorded above for whoever next owns the relevant files:
`bootstrap.rs`'s unconditional credential check (IV-A1's file) and `init_tracing`'s default
filter (owned by IV-A2 for the readiness signal only; the tracing filter itself has no explicit
owner in the Part IV ownership table).

## Known limitations or `not_measured` fields

- **Binary-size delta: not independently measured, and reasoned to be effectively zero.**
  `git diff --stat Cargo.lock crates/tack-cli/Cargo.toml crates/tack-api/Cargo.toml` shows no
  changes — this card adds no new external dependency to either crate; every type it uses
  (`Repository`, `ExecutionClock`, `SystemExecutionClock`, `uuid`, `thiserror`) was already
  linked into both `tack-api` and (transitively, via `tack-api`) `tack-cli` before this card.
  The added code is a few hundred lines of logic reusing existing types, not a new dependency
  edge. A full release-profile build-twice comparison (IV-A3's method) was judged not worth the
  build time for a change with no new linked crate.
- The two discovered gaps above (bootstrap.rs's credential precondition, and the tracing
  filter) are real but pre-existing and out of this card's ownership — worked around, not fixed,
  and explicitly flagged for the owners of those files.

## Secrets/logging review

- Grepped for how `transport.rs` and `runner_admin.rs` already redact: `EnrollmentCredential`'s
  `Debug`/`Display` are unconditionally `"[REDACTED]"` (unchanged by this card); the raw token
  in `runner_admin.rs` is called `raw_token`/`enrollment_token` and is placed **only** in
  `CreatePendingRunnerResponse::enrollment_token`, matching the pre-existing handler exactly —
  the extraction did not add a new place the raw value is copied to.
- `local_enrollment::self_provision` logs `runner_id`, `token_id`, `expires_at` at `info` and
  never the raw token — confirmed by reading the tracing call, not just by convention.
- Live-run proof (positive control + negative assertion, see below): round 1's combined
  server+runner log genuinely contains the runner id (`grep -q "$RUNNER_ID"` succeeds — proving
  the log capture mechanism can show real content) and genuinely contains **zero** matches for
  the raw token's known shape (`enr_<uuid>`, the exact format `runner_admin.rs` generates via
  `format!("enr_{}", Uuid::new_v4())`). Round 2's log is checked the same way and is also clean.
- `ensure_runner_credential`'s stored-session placeholder string
  (`"stored-session-on-disk-no-token-needed"`) was also grepped for in both round logs: zero
  matches (expected — it is never transmitted, and would be redacted even if it were).

## Which role executed what for the live run (Part IV addition)

One process, one binary, `tack serve --with-runner`, run twice against the same `state_dir` and
database file. Full transcript below; script preserved at
`/tmp/claude-1000/-home-ox-Sites-objetivosMios/b3baec54-86ac-4990-a4f2-730beab1cbc7/scratchpad/iv-a4-live-proof.sh`
(not part of this repo — a throwaway proof harness, reusing `scripts/smoke.sh`'s fake-harness
shim pattern exactly as IV-A3's own proof did):

- **Round 1 (fresh, empty `state_dir`, no `TACK_RUNNER_ENROLLMENT_TOKEN`, no manual enrollment
  call of any kind):** the embedded runner **self-provisioned** — its own log shows `POST
  /api/runner/v1/enroll` → `200`, and the server's own handler log line reads `runner enrolled
  runner_id=runr_1a5dc296-...`. This is the real runner-v1 HTTP redemption path, not a bypass:
  the same `/enroll` route a manually-issued token would hit, driven by
  `tack_runner::bootstrap::run` exactly as any remote runner would drive it.
- **Round 2 (same `state_dir`, now holding `session.json` from round 1, same database file, no
  token):** the embedded runner **reused the stored session** — its log shows `POST
  /api/runner/v1/refresh` → `200` (`"runner capability refresh accepted ... rotated=false"`) and
  **no** `/enroll` request at all. `GET /api/runners` reports the same single row
  (`runr_1a5dc296-...`) both before and after round 2.
- In both rounds the attempt that actually ran was claimed and executed by the **embedded**
  runner (the same process as the server, via `POST /api/runner/v1/claim` and
  `/api/runner/v1/heartbeat` from that runner's own credential) — never a separate/standalone
  runner process.

```
== PROOF A: zero-touch enrollment — fresh state_dir, no token, one command ==
   PASS server up on 3595 with zero manual enrollment
   PASS self-provisioned runner runr_1a5dc296-7e6b-45ef-9d24-36d97374c13c reached active with a heartbeat, with no token ever configured
   PASS GET /api/runners shows exactly 1 runner after round 1
   PASS execution request exec_e451... queued against the self-provisioned runner
   PASS PROOF A: attempt succeeded end-to-end with zero manual enrollment

== secret hygiene on round 1's combined log (server + embedded runner, one process) ==
   PASS positive control: the log CAN and DOES show a real id (runr_1a5dc296-...) — proves the absence check below is not vacuous
   PASS no raw enrollment token (enr_<uuid> shape) ever appeared in the log

== PROOF B: second start reuses the stored session, no second runner ==
   PASS server up again on round 2
   PASS round 2 reused the SAME runner id (runr_1a5dc296-...), active with a fresh heartbeat
   PASS GET /api/runners STILL shows exactly 1 runner after round 2 (no second runner created)
   PASS execution request exec_dc85... queued against the reused runner
   PASS PROOF B: attempt succeeded on round 2 via the reused stored session
   PASS round 2's log contains no raw enrollment token, consistent with self-provisioning being skipped

== session.json permissions ==
   PASS session.json is owner-only (mode 600)

ALL PROOFS PASSED
```

Directly grepped confirmation of the real protocol calls (not just the summary above), from the
raw logs:
```
round1.log: POST /api/runner/v1/enroll -> 200; "runner enrolled runner_id=runr_1a5dc296-..."
round2.log: POST /api/runner/v1/refresh -> 200; "runner capability refresh accepted runner_id=runr_1a5dc296-... rotated=false"
round2.log: zero occurrences of "runner/v1/enroll"
```

`session.json` on disk after round 1: `-rw------- 1 ox ox 196 ... session.json` (mode `600`,
confirmed with `stat -c '%a'`, not assumed from the write path alone).

## Loopback/gating proof (Part IV addition, named explicitly)

Not re-derived — inherited by construction. `ensure_runner_credential` (and therefore
self-provisioning) is only ever called from inside `serve_with_embedded_runner`, after
`ensure_loopback(&server_config)?` has already returned `Ok`; there is no other call site. This
is the same reasoning IV-A3 used for its own guard placement, unchanged by this card. IV-A3's
own `embedded_runner_refuses_non_loopback_bind` test (`crates/tack-cli/src/local_runner.rs`)
still exercises `ensure_loopback` directly and is untouched by this diff — `cargo test -p
tack-cli` confirms it still passes. This card added no new loopback-related test because it
added no new loopback-related code path: self-provisioning cannot run before that guard has
already passed.

## Safe merge order and likely conflicts

No conflicts expected against `develop`. `runner_admin.rs`'s diff is additive (two new
functions, one enum, and the existing handler body replaced by a call into the first new
function) — no other in-flight card touches that file per §IV.2. `local_runner.rs`'s diff
touches the same function IV-A3 wrote (`serve_with_embedded_runner`) and adds one new private
function (`ensure_runner_credential`) plus new tests; no other card is expected to touch this
file in this wave. `main.rs`'s only change is one new `mod` line — IV-A5 is expected to add a
subcommand arm to the same file; a conflict there, if any, is a trivial two-line merge, not a
semantic one, matching what the card's own instructions anticipated.

## Checklist

- No unowned files touched: `runner_admin.rs` (owned, extraction only), `local_enrollment.rs`
  (new, owned), `local_runner.rs` (owned per the card's explicit carve-out), `main.rs` (one
  `mod` line, same reasoning as the `local_runner.rs` carve-out), `c1_handlers_test.rs` (test
  file for the crate whose extraction this card owns), this handoff. `bootstrap.rs`, `config.rs`
  (tack-runner) and `server.rs`, `init_tracing` (tack-api) were read and reasoned about but
  **not edited** — both discovered gaps are recorded above instead.
- No live secret introduced or logged: see "Secrets/logging review" above, backed by a live,
  format-aware grep against real log output from a real run, not just code inspection.
- No panic stub: every fallible call in the new code propagates via `?` or `.map_err`; no
  `unwrap()`/`expect()` outside test code.
- No blind retry: `provision_local_runner` performs one pool open and one provisioning call; no
  retry loop was added anywhere in this diff.
