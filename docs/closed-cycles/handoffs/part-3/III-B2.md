# III-B2 handoff

- Base SHA / branch / final SHA: `f042085` / `agent/iii-b2-execution-store` /
  reported with the final commit (a commit cannot contain its own content-addressed SHA).
- Files changed (must equal ownership list): `crates/tack-db/src/migrations.rs`,
  `crates/tack-db/src/repo/execution.rs`, `crates/tack-db/src/repo.rs` (one module export),
  `crates/tack-db/tests/execution_repo_test.rs`, this handoff.
- Contract fixtures consumed: `runner-v1` lifecycle, limits, protocol, claim, event-batch,
  completion, capability and stable-error fixtures. IDs remain opaque text; no fixture changed.
- Behavior implemented: additive migrations 039–048 for the ten neutral tables; transactional
  idempotent enqueue; eligible exact-runner/fleet claim with request state, fence, attempt and
  capacity in one transaction; fake-clock heartbeat/expiry classification; fenced event batches,
  completion replay, cancellation request, artifact/decision metadata writes; queue/history
  indexes.
- Tests added and exact commands/results: `cargo test -p tack-db --test execution_repo_test`
  (6 passed); `cargo fmt -p tack-db -- --check`; `cargo test -p tack-db` (121 passed,
  1 performance test intentionally ignored); `cargo clippy -p tack-db -- -D warnings`.
- Failure/adversarial case proved: orphan item FK is rejected; changed idempotency payload is a
  conflict; stale fence cannot heartbeat; event/completion replay is idempotent; terminal state
  cannot reopen; expiry permits only `lost`/`needs_operator`, never automatic requeue; query
  plans select the queue/timeline indexes.
- Schema/API/contract change requested from another owner: C1/C2 must translate their frozen
  protocol DTOs into these opaque repository inputs. No route/OpenAPI/fixture edits made.
- Known limitations or `not_measured` fields: profiles/snapshots and actual execution are
  persisted opaque JSON at this layer; validation/serialization authority remains B1/A0/C5.
- Secrets/logging review: runner credential is accepted only as an already-derived hash; no
  credential, prompt, metadata or environment value is logged by this module.
- Safe merge order and likely conflicts: merge after A3 (already in base); no other Wave 1 card
  may modify migrations or `repo.rs`. B1/B3/B4 are otherwise independent.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Wave 2 persistence amendment

- Base / branch / final SHA: `f14019b` / `agent/iii-b2-wave2-amend` / reported with commit.
- Added migrations 049–051: runner credential metadata plus hash-only, single-use,
  expiry/revocation-aware enrollment tokens; durable `(runner_id, claim_request_id)` replay
  records; and a recovery-audit table reserved for the C4 recovery transaction.
- Added atomic token issue/redeem/revoke and the strong
  `claim_execution_idempotent_with_snapshot` API. Its one transaction includes replay lookup,
  capacity reservation, request transition, attempt insert and replay-key insert; replay returns
  the original lease and stored immutable request fields. The existing lease-only method remains
  as a compatibility wrapper.
- Hardened report paths: event/artifact/decision/completion require an unexpired lease; terminal
  completion restores runner capacity in its successful terminal transaction only, so replay
  cannot restore it twice.
- Focused verification: `cargo test -p tack-db --test execution_repo_test` (8 passed),
  `cargo fmt -p tack-db`, `cargo clippy -p tack-db --test execution_repo_test -- -D warnings`.
- Remaining blocker, deliberately not guessed: the requested all-in-one heartbeat batch and C4
  recovery transition/audit APIs still need their dedicated repository methods and fault tests.
  The tables are present but C2/C4 must not treat their absence as a completed recovery protocol.

### Reporting/recovery follow-up

- Added migration 052 and atomic `heartbeat_batch`: runner capacity/heartbeat, every fenced
  lease renewal and cancellation flags commit with a durable runner+heartbeat-id replay record;
  a stale lease rolls the whole transaction back.
- Added `recover_attempt` with a keyed durable audit. `SafePreSpawnRequeue` is accepted only
  for expired attempts with no `started_at`; all ambiguous/post-spawn recovery must request
  `NeedsOperator`. Each successful recovery normalizes capacity and writes exactly one audit.
- New fake-clock test proves heartbeat replay and recovery audit idempotency (focused DB suite:
  9 passed; Clippy and format passed).
- Still required before C2/C4 treats the repository as feature-complete: structured event-batch
  accepted-vs-duplicate result and durable cancellation-observation replay APIs. Existing event
  checkpoint and completion-id persistence remain safe compatibility paths, but do not expose
  the richer protocol result yet.

### Runner-admin follow-up

- Added transactionally-created pending runner plus token issuance, runner/token-id-scoped token
  metadata and revocation, and runner revocation that revokes outstanding tokens. Redemption now
  requires `pending_enrollment` and writes the full identity/capability exchange atomically.
- Added idempotent audited operator requeue for `needs_operator`; its key includes recovery key,
  actor and reason fingerprint and it never increments capacity.
- Focused `execution_repo_test` passed 10 tests; format, focused Clippy and diff checks passed.

### Final hardening follow-up

- Migration 053 stores the canonical immutable runner-v1 request snapshot. Claim fresh/replay
  reads the same JSON fallibly and fails closed for the legacy `{}` default rather than silently
  reconstructing a lossy payload.
- Enrollment redemption now atomically consumes its hash-only token and persists runner name,
  version, labels, capacity, capabilities, protocol, credential hash and expiry; revocation is
  never cleared by enrollment.
- Added structured event checkpoint results and durable cancellation observation replay, with
  focused fake-clock repository coverage. Final focused command: `cargo test -p tack-db --test
  execution_repo_test` (10 passed), plus `cargo fmt -p tack-db` and focused Clippy.

### Event replay correction

- Migration 054 persists each event-batch replay keyed by `(attempt_id, checkpoint)` with an
  authenticated request fingerprint and the authoritative accepted/duplicate response. The
  strong event API now returns `Applied`, `ReplayConflict`, or `Stale`; runner/fence/lease
  validation happens before a replay record may be used.
- Payload JSON is recursively canonicalized before fingerprinting, so object-key order does not
  turn an equivalent retry into a conflict. Malformed replay rows fail closed with a database
  protocol error. A legacy pre-054 attempt whose checkpoint already advanced but has no replay
  row is a `ReplayConflict`; no response is fabricated from incomplete historical state.
- Focused coverage proves canonical retries, changed-payload conflict with no extra write, and
  foreign-fence stale rejection.

### Completion replay correction

- Migration 055's completion replay record is now authoritative: `Completion` carries the
  required final event checkpoint, and the terminal compare-and-set validates attempt, runner,
  fence, active lease and that checkpoint in one statement. Completion fingerprints bind every
  immutable request field (including runner/fence/checkpoint) and recursively canonicalize the
  structured terminal-reason, execution and usage JSON.
- Authentication precedes replay lookup. A matching authenticated terminal retry returns the
  parsed stored response; malformed stored data fails closed. A stale or foreign runner/fence
  cannot read a response. Pre-055 terminal attempts with no durable response are `Conflict`,
  even if their historical completion ID matches: the repository deliberately does not invent a
  response from incomplete state.
- Focused tests cover response-loss replay, foreign-fence rejection, semantic JSON replay,
  changed fields/checkpoint conflicts, corrupt response handling, exactly-once capped capacity,
  and rollback when persisting the replay record fails. The boolean compatibility completion API
  delegates to the hardened typed path.

### Heartbeat replay correction

- Migration 056 adds the durable canonical request fingerprint to heartbeat replay rows. The
  heartbeat response now preserves its ID, acceptance time, and every renewed
  attempt/fence/expiry/cancellation flag; exact retry returns those original typed fields even
  after the clock has advanced. Same-ID input changes return `Conflict`; malformed stored
  responses fail closed.
- Heartbeat capacity is checked against all active unexpired reservations, not merely the
  submitted leases. A runner cannot report free capacity while it still holds a lease, and the
  persisted value is constrained to `total_capacity - active_reservations`.
- Pre-056 replay rows have the empty migration default fingerprint and therefore return
  `Conflict`; their incomplete historical response is not treated as authoritative. Focused
  coverage includes false-free capacity, multi-lease canonical replay, stale lease, response
  loss after clock advance, changed-input no-write, and replay-insert rollback.
- Heartbeats now also bind a frozen client `sent_at` timestamp into that fingerprint. Existing
  pre-`sent_at` fingerprints therefore conflict rather than replaying. Capacity reservations
  count every unresolved active attempt even after its lease expires: expiry alone cannot make a
  slot available; a recovery/terminal transition must release it.
- Each heartbeat active-attempt entry now also freezes and persists its reported `state`,
  `journal_state`, and `last_event_checkpoint`; any same-ID mutation of one of those fields is a
  `Conflict` rather than a replay.

### Cancellation observation correction

- Migration 057 gives cancellation replay records a canonical fingerprint and authoritative
  response. `CancellationObservationInput` binds runner/fence, cancellation ID, `observed_at`,
  and canonical JSON `details` plus `observation`; the response records attempt, ID, state and
  committed timestamp. Authentication occurs before replay lookup, so a foreign/stale fence
  cannot obtain a replay response, while an authenticated terminal retry can.
- The typed outcome separates `Cancelled` and `Replayed` from `Conflict`, `Stale`,
  `AlreadyTerminal`, and `Ambiguous`; terminal or unsupported/missing-cancellation states are
  never reported as a successful cancellation. Capacity restoration is capped at total capacity.
- Pre-057 cancellation rows have empty fingerprint/response defaults and return `Conflict` on a
  retry rather than fabricating a response. Focused coverage includes response-loss replay after
  time advance, semantic JSON replay, changed-input no-write, foreign fence, corrupt response,
  capped capacity, transaction rollback, and terminal/ambiguous classifications.
- Cancellation `observation` is intentionally constrained to the exact JSON string
  `"process_stopped"`; any other JSON value fails before opening a transaction or changing state.

### Recovery observation v1

- Migration 058 makes recovery audits durable replay records with a fingerprint and response.
  `recover_attempt` now accepts a runner/attempt/fence/key observation, authenticates an active,
  non-revoked owning runner before lookup, and returns typed applied/replayed/conflict/stale
  results with the original committed timestamp.
- Only `process_stopped` with server `started_at` absent plus validated details
  `journal_state="prepared"` and `process_observed=false` can safely mark the attempt `lost`
  and requeue its request. Started, spawned, running, process-observed, or ambiguous evidence is
  conservatively `needs_operator`. Terminal attempts create an auditable/replayable
  `already_terminal` disposition with no lifecycle or capacity mutation.
- Pre-058 audit records have the empty migration-default fingerprint/response and return
  `Conflict`; their historical audit is not fabricated into a recovery response. Focused tests
  cover every disposition, semantic replay and changed input, foreign/revoked access, capped
  release, and rollback of lifecycle, capacity, and audit together.

### Request snapshot hardening

- Enqueue now parses the complete runner-v1 frozen execution request shape before opening a
  transaction: required nested objects and scalar types, RFC3339 `created_at`, selector/profile,
  repository, policy, budget, environment, and metadata fields are validated and cross-checked
  against every normalized request column. The snapshot `created_at` must be the same injected
  clock instant written to the row, and the validated JSON is what is persisted.
- Migration 059 quarantines queued legacy M053-default `{}` snapshots as `needs_operator` while
  preserving the row and all original data. It never invents a `created_by` or a replacement
  immutable snapshot. Claim continues to fail closed for any incomplete persisted snapshot.
- Focused coverage uses full fixtures and proves removed, malformed, cross-field, and clock
  mismatch snapshots create no row; it upgrades through migrations 049–058 then proves M059
  quarantines the legacy queued record without data loss.
- Exact enqueue retries compare the canonical immutable snapshot stored with the original request
  before the fresh-request clock check. They therefore replay after time advances, while a
  changed `created_at` (or any other frozen field) is a conflict. Fresh inserts still require
  `created_at` to match the injected clock.
- Migration 060 expands quarantine to every nonterminal malformed or partial snapshot, including
  pre-existing three-key cohorts, without altering terminal historical rows. Snapshot validation
  now requires typed `created_by`, object-valued budgets/metadata/profile policies, a string
  repository kind, and exactly one literal value or secret reference for each environment entry.
  It also rejects SQLite-lenient timestamps lacking RFC3339 `T`/offset syntax and negative or
  fractional root/profile timeouts, so those rows cannot remain queued and starve valid work.

### Legacy boundary cleanup

- The unsafe public single-lease heartbeat, boolean event append, expiry-classification, and
  lease-only claim compatibility paths are removed. C2 and C4 must use `heartbeat_batch`,
  `append_execution_events_result`, `recover_attempt`, and
  `claim_execution_idempotent_with_snapshot`; their typed outcomes preserve the authenticated
  fence, durable replay, frozen snapshot, and conservative-recovery semantics that the old
  shortcuts could bypass.
- Artifact and decision creation require `lease_expires_at > now`; equality is expired and
  creates no row. Operator requeue additionally requires a durable authoritative
  `needs_operator` recovery audit for that exact attempt, so a manually corrupted lifecycle
  state cannot release or requeue work.
- Capacity restoration remains capped by `total_capacity` and is owned by terminal/recovery
  transitions only. Recovering to `needs_operator` releases one reservation; the subsequent
  operator requeue only changes request state and cannot release capacity again.

### Runner start-transition integration amendment

- Migration 061 adds durable `prepared_at` and `process_id` facts to execution attempts. The
  typed `transition_attempt_with_facts` repository API is the sole C2 seam for accepting the
  frozen `preparing` and `running` acknowledgements without route-owned lifecycle SQL.
- The transition authenticates runner, attempt and fence against an active, non-revoked runner
  and a strictly unexpired lease. Preparation freezes workspace and base revision; start may
  only add a non-empty process ID after matching preparation. Exact natural retries return the
  original committed timestamp, while changed facts, wrong ordering and stale authority cannot
  write.
- Focused coverage proves both transitions, response-loss replay after clock advance, immutable
  fact conflicts, wrong-order rejection, stale fence/expiry rejection, and no-write behavior.

### Concurrent enrollment redemption amendment

- Enrollment redemption now begins with an immediate SQLite write transaction. This serializes
  consumers before reading the single-use token and prevents two deferred readers from
  deadlocking while both upgrade to writers. Concurrent callers deterministically receive one
  authoritative redemption and one invalid/expired result; raw token material remains absent
  from storage.

### Concurrent claim serialization amendment

- Base / branch / final SHA: reported with the amending commit.
- Files changed (must equal ownership list): `crates/tack-db/src/repo/execution.rs`,
  `crates/tack-db/tests/execution_repo_test.rs`, this handoff.
- An independent verifier found the enrollment-redemption deadlock's twin, unfixed, in
  `claim_execution_idempotent_with_snapshot` — the function this card's own acceptance criterion
  (TODO.md:9469, "concurrent claimers produce one lease") depends on. It opened the same
  deferred `self.pool().begin()`, read the runner's capacity row, then wrote it; two concurrent
  claimants for one runner reproduced `SqliteError { code: 6, message: "database is deadlocked"
  }` 25/25 times. `claim_execution_idempotent_with_snapshot` now begins with
  `BEGIN IMMEDIATE`, matching the enrollment fix exactly; every exit path (replay hit, capacity
  exhausted, no eligible work, lost request compare-and-set, success) was re-checked to still
  commit or roll back explicitly. Its signature and return type are unchanged, per III-C1/III-C2
  contract freeze.
- The acceptance test itself did not prove the criterion it claimed to:
  `concurrent_claimers_receive_exactly_one_valid_lease` caught `Err(_)` from either concurrent
  branch and silently fell back to a sequential retry. Since the deadlock fired on effectively
  every run, the test quietly stopped exercising concurrency and passed on the sequential
  fallback path instead — the "one lease" criterion was unproven. It has been rewritten: a raw
  `sqlx::Error` from either branch is now a hard failure with no retry/swallow, and it
  additionally asserts the winner's fencing token, the loser's well-typed `None` (not a second
  lease), and coherent end-state (`available_capacity`, request state, attempt-row count). A
  second permanent test, `concurrent_claimers_deadlock_fix_holds_under_load`, runs the same
  shape 25 times in one process and demands zero sqlx-level errors throughout.
- Load-bearing proof, exact numbers: the rewritten test run 25/25 times against the fixed code —
  zero failures. The same test, run 25/25 times in a disposable `git worktree` (never by
  reverting the real tree) against the pre-fix function body, failed 25/25 times with the
  identical `database is deadlocked` signature reported above. The worktree was removed after
  verification.
- Inspecting this card's remaining transaction sites for the same shape (SELECT then write on a
  shared row inside a deferred transaction) found it was not isolated to enrollment and claim.
  `complete_execution_result`, `operator_requeue_needs_operator`, and
  `transition_attempt_with_facts` all guard their writes with fencing-token/state predicates —
  those predicates protect the correctness of the eventual write, but not SQLite's lock-upgrade
  race, which is orthogonal to the `WHERE` clause. Concurrent, plausible duplicate/retry callers
  (a runner or operator resending an unacknowledged report) were stress-tested directly against
  each function and reproduced the same deadlock signature reliably (multiple hits per 15 runs
  on every one of the three). All three were genuinely broken, not merely theoretical, and all
  three received the identical `BEGIN IMMEDIATE` fix with the same exit-path audit. Permanent
  regression tests (`concurrent_duplicate_completion_reports_have_one_committed_writer`,
  `concurrent_duplicate_operator_requeues_have_one_authoritative_writer`,
  `concurrent_duplicate_transition_reports_have_one_applied_writer`) each assert both concurrent
  branches succeed at the sqlx level and exactly one is authoritative; each was confirmed to
  reproduce the deadlock before its fix and pass reliably (20/20) after.
- Tests added and exact commands/results: `cargo test -p tack-db --test execution_repo_test`
  (54 passed); `cargo test -p tack-db` (169 passed across lib + all integration suites, 1
  performance test intentionally ignored); `cargo test -p tack-api --test c1_handlers_test`
  (4 passed) and `cargo test -p tack-api --test runner_vertical_slice` (7 passed), proving the
  functions' consumers are unaffected; `cargo clippy -p tack-db -- -D warnings` (clean);
  `cargo fmt -p tack-db -- --check` (clean).
- Failure/adversarial case proved: every exit path of all four amended functions (enrollment
  redemption, claim, completion, operator requeue, attempt transition) still commits or rolls
  back explicitly under `BEGIN IMMEDIATE`; no transaction is left open on any return path. The
  rewritten claim test and its load-bearing loop prove the deadlock is gone, not merely masked,
  by first proving it reproduces before the fix.
- Schema/API/contract change requested from another owner: none. No migration, route, or DTO
  changed; `claim_execution_idempotent_with_snapshot`'s signature and return type are untouched.
- Known limitations or `not_measured` fields: this amendment does not re-audit every transaction
  in the module for the same table-level lock-upgrade hazard — only the three sites an
  independent verifier flagged as suspicious were inspected and stress-tested. A systemic audit
  of the remaining `self.pool().begin()` call sites was judged out of this card's scope.
- Secrets/logging review: no change; no credential, prompt, or environment value is logged by
  any amended function.
- Safe merge order and likely conflicts: no migration or shared-file change; safe to merge
  independently of concurrent Wave work in `crates/tack-api/**`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

### Systemic deadlock audit — the remaining seven sites

- Base / branch / final SHA: reported with the amending commit.
- Files changed (must equal ownership list): `crates/tack-db/src/repo/execution.rs`,
  `crates/tack-db/tests/execution_repo_test.rs`, this handoff.
- The prior amendment explicitly left the remaining `self.pool().begin()` sites in this module
  unaudited. This amendment audits all seven: `create_pending_runner_and_issue_token` (~1053),
  `revoke_runner` (~1084), `heartbeat_batch` (~1315), `recover_attempt` (~1407),
  `observe_cancellation` (~1527), `enqueue_execution` (~1787), and
  `append_execution_events_result` (~1834). Each was stress-tested with a realistic concurrent
  caller (a runner/operator/client retrying an unacknowledged request, or two duplicate
  submissions of the same idempotency key) before any code changed, per the standing instruction
  not to trust inspection for this hazard.
- Two sites are write-first — their opening statement inside the transaction is an `INSERT`/
  `UPDATE`, not a `SELECT` — so there is no intervening point where the transaction holds only a
  SHARED lock and must later race another connection to upgrade it. Both were stress-tested
  anyway: `create_pending_runner_and_issue_token` (two concurrent enrollments for different
  runners, 100 concurrent calls across 50 iterations) and `revoke_runner` (two concurrent
  duplicate revokes of the same runner, 100 concurrent calls across 50 iterations) produced
  **0/100 deadlocks** each. Both are left unchanged; the difference from the five broken sites is
  structural (write-first vs. read-then-write), not incidental, so this is not "inspection
  overriding a stress test" — the stress test is what settled it.
- The remaining five all open with a `SELECT` against a row a concurrent duplicate caller also
  reads, then later write it, and all five reproduced the deadlock under a direct, realistic
  duplicate-caller stress test before any fix was applied:

  | Site (function, ~line) | Realistic concurrent caller | Deadlocks before fix |
  |---|---|---|
  | `heartbeat_batch` (~1315) | runner retries an unacknowledged heartbeat POST (same `heartbeat_id`) | 17/40 |
  | `recover_attempt` (~1407) | runner/reconciler retries an unacknowledged recovery report (same `recovery_key`) | 8/40 |
  | `observe_cancellation` (~1527) | runner retries an unacknowledged cancellation report (same `cancellation_request_id`) | 5/40 |
  | `enqueue_execution` (~1787) | client retries an unacknowledged enqueue POST (same idempotency key) | 2/40 |
  | `append_execution_events_result` (~1834) | runner retries an unacknowledged event-batch POST (same checkpoint) | 11/40 |

  Three of these (`heartbeat_batch`, `append_execution_events_result`, `observe_cancellation`)
  sit directly on the Wave 2 hot path that III-C2 is wiring into HTTP endpoints; the fifth
  (`enqueue_execution`) is III-C1's write path. All five now begin with `BEGIN IMMEDIATE`,
  identical in form and comment to the five sites fixed by the prior amendments. Every exit path
  of all five — every early `Stale`/`Conflict`/`Ambiguous`/`AlreadyTerminal`/`ReplayConflict`
  return, every replay hit, and the successful-write tail — was re-checked to still commit or
  roll back explicitly; none left a transaction open on any path. No function's public signature
  or return type changed.
- Five new permanent regression tests were added, one per fixed site, matching the established
  style (`concurrent_duplicate_completion_reports_have_one_committed_writer` and its two
  siblings): `concurrent_duplicate_heartbeats_have_one_authoritative_writer`,
  `concurrent_duplicate_recovery_observations_have_one_authoritative_writer`,
  `concurrent_duplicate_cancellation_observations_have_one_authoritative_writer`,
  `concurrent_duplicate_enqueues_have_one_authoritative_writer`, and
  `concurrent_duplicate_event_batches_have_one_authoritative_writer`. Each calls the same
  duplicate input concurrently via `tokio::join!`, treats a raw `sqlx::Error` from either branch
  as a hard `.expect(...)` failure (no retry, no swallow), asserts exactly one branch is
  authoritative (`Accepted`/`Applied`/`Cancelled`/`Created`, matched by discriminant) and the
  other replays with the identical stored response, and asserts the coherent end state: capacity
  restored/consumed exactly once, exactly one durable replay/audit row, and (for the event-batch
  test) the event persisted exactly once and the checkpoint advanced exactly once.
- Load-bearing proof, exact numbers: the fixed code was run through the full
  `execution_repo_test` suite (including a 20-iteration bash loop invoking all five new tests
  each pass, i.e. 100 total executions per test) with **zero failures**. Each fix was then proven
  load-bearing independently in a disposable `git worktree` under
  `/tmp/claude-1000/-home-ox-Sites-objetivosMios/7ef1baa9-ab8a-4721-b176-069584c82435/scratchpad`
  (never by reverting the real working tree): the worktree's `execution.rs` and
  `execution_repo_test.rs` were overwritten with the fixed working-tree versions, then, one site
  at a time, that one site's `BEGIN IMMEDIATE` was reverted back to the pre-fix `begin()` body
  (comment removed, everything else — including the other four fixes — left intact) and its new
  test run 20 times:

  | Site | Control result (pre-fix body, in worktree) |
  |---|---|
  | `heartbeat_batch` | 17/20 runs failed with `SqliteError { code: 6, message: "database is deadlocked" }` |
  | `recover_attempt` | 9/20 runs failed, identical signature |
  | `observe_cancellation` | 10/20 runs failed, identical signature |
  | `enqueue_execution` | 6/20 runs failed, identical signature |
  | `append_execution_events_result` | 9/20 runs failed, identical signature |

  A sample failure was inspected directly to confirm the signature rather than trusting a grep
  match: `second heartbeat report must succeed at the sqlx level: Database(SqliteError { code: 6,
  message: "database is deadlocked" })`. The worktree was removed after verification
  (`git worktree remove --force`).
- Tests added and exact commands/results: `cargo test -p tack-db --test execution_repo_test`
  (59 passed, up from 54); `cargo test -p tack-db` (174 passed across lib + all integration
  suites, 1 performance test intentionally ignored, up from 169); `cargo test -p tack-api --test
  c1_handlers_test` (7 passed) and `cargo test -p tack-api --test runner_vertical_slice`
  (7 passed), proving the functions' consumers are unaffected; `cargo clippy -p tack-db --
  -D warnings` (clean); `cargo fmt -p tack-db -- --check` (clean); `git diff --check` (clean).
- Failure/adversarial case proved: every exit path of all five newly amended functions still
  commits or rolls back explicitly under `BEGIN IMMEDIATE`, confirmed by the full suite passing
  and by the 100-execution zero-failure loop. Each fix was proven load-bearing by first
  reproducing the identical deadlock signature on its exact pre-fix function body in isolation,
  not merely by the fix "seeming reasonable."
- Schema/API/contract change requested from another owner: none. No migration, route, or DTO
  changed; none of the five functions' signatures or return types were touched.
- Known limitations or `not_measured` fields: **`execution.rs` is now fully audited** — every
  `self.pool().begin()` call site in this module (all 12: the 5 fixed by the prior amendments,
  the 5 fixed here, and the 2 confirmed safe here) has been either stress-tested and fixed, or
  stress-tested and left alone with recorded evidence. No open transaction-serialization question
  remains in this file. Read-only inspection of the two out-of-scope repositories this card must
  not edit found the same read-then-write-inside-`begin()` shape in `crates/tack-db/src/repo/
  orch.rs`: the retention-sweep rollup loops for `orch_events`→`orch_events_daily` (~1529) and
  `orch_metrics`→`orch_metrics_daily` (~1646) both `SELECT` a candidate batch then `INSERT`/
  `UPDATE`/`DELETE` it inside the same deferred transaction, structurally identical to the five
  sites fixed here. This is inspection only — not stress-tested, since `orch.rs` is frozen until
  card G1 and out of this card's ownership — but it is flagged here for that future owner rather
  than fixed. The other five `orch.rs` sites (~543, ~806, ~952, ~1093, ~1386) are write-first
  batch-insert loops, structurally the same shape as this amendment's two confirmed-safe sites, so
  they are lower suspicion but were not stress-tested. `crates/tack-db/src/repo/items.rs` has
  exactly one `self.pool().begin()` site (~196), not inspected further.
- Secrets/logging review: no change; no credential, prompt, or environment value is logged by any
  amended function.
- Safe merge order and likely conflicts: no migration or shared-file change; safe to merge
  independently of concurrent Wave work in `crates/tack-api/**`, `crates/tack-orch/**`, and
  `crates/tack-runner/**`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

### Three-review fix-up: idempotency-conflict split, decision/artifact transaction, credential-rotation CAS

- Base / branch / final SHA: reported with the amending commit; branch `plan/harness-agnostic-agent-fleet`.
- Files changed (must equal ownership list): `crates/tack-db/src/repo/execution.rs`,
  `crates/tack-db/tests/execution_repo_test.rs`, this handoff.
- Three independent reviews each found a genuine defect in this file. All three are fixed here.

**Defect 1 — collapsed idempotency-conflict/conflict caused unbounded retry.**
`append_execution_events_result`'s and `complete_execution_result`'s replay checks each had two
structurally different causes collapsed into one `ReplayConflict`/`Conflict` variant: a stored
replay fingerprint that differs from the new request's fingerprint (the same idempotency-scoped
key reused with different content — can never succeed by retrying) versus a benign out-of-order
resync or a defensive lost compare-and-set (genuinely retryable). C2's handler mapped the
collapsed variant to the retryable `conflict` stable error unconditionally, so a runner that
reused an idempotency key with different content was told to retry forever. `EventApplyResult`
and `CompletionResult` each gained an `IdempotencyConflict` variant (non-retryable,
`idempotency_conflict` on the wire) alongside the existing `Conflict` (retryable, `conflict` on
the wire), following the multi-variant precedent `CancellationObservation` already set in this
file. Every fingerprint-mismatch return site (`append_execution_events_result` ~L1882 pre-amend;
`complete_execution_result` ~L986 pre-amend) now returns `IdempotencyConflict`; every benign
out-of-order/lost-race site (checkpoint-already-advanced-with-no-replay-row, previous_checkpoint
mismatch, and both functions' defensive optimistic-concurrency-UPDATE-affected-zero-rows branches)
still returns `Conflict`, each now commented with which case it is and why.

Defect 1 regression tests: `event_replay_changed_payload_is_idempotency_conflict_and_does_not_write`
(renamed from `..._is_conflict_...`) now asserts `IdempotencyConflict` for the reused-checkpoint
case and, in the same test, adds a contrasting out-of-order batch asserting the benign `Conflict`
is preserved and distinct. `completion_conflict_and_idempotency_conflict_distinguish_causes_without_write`
(renamed from `completion_changed_fields_or_checkpoint_conflict_without_write`) keeps its
pre-replay-row `Conflict` assertion (lost compare-and-set) and changes its two post-replay-row,
same-completion-id/changed-fields assertions to `IdempotencyConflict`. Both scenarios were already
exercised by the pre-existing tests, but the tests only asserted a single collapsed variant —
which is exactly how a caller-visible distinction never got made.

**Defect 2 — decision/artifact writes had no transaction.**
`create_execution_decision` and `record_execution_artifact` ran their eligibility
`SELECT EXISTS(...)` and their `INSERT ... ON CONFLICT DO NOTHING` as two separate un-transacted
statements against `self.pool()` — the only two methods in this file that never opened a
transaction, which is exactly why the standing `begin()`-call-site audits in the amendments above
never flagged them. Both now open `BEGIN IMMEDIATE` (matching every other method in this file) and
run the check and the insert inside it; every exit path (ineligible → commit + `false`, eligible →
insert + commit + `true`) was checked to still commit explicitly.

Defect 2 regression test: `artifact_and_decision_cannot_land_against_concurrently_terminal_attempt`.
This test deliberately does **not** use the suite's shared in-memory `ready_repo()` harness — a
first version built on it passed 20/20 on both the fixed and the unfixed code, because pooling
several connections against one `:memory:` database requires SQLite shared-cache mode, which
imposes its own table-level locking where a plain `SELECT` blocks behind *any* pending write to
the same table, accidentally serializing the exact gap this defect needs to expose. A genuine
file-backed database (the same `sqlite:...?mode=rwc` + WAL setup production uses) does not have
that property — a reader is never blocked by an uncommitted writer — which is the locking model
this defect actually lives under. The test also does not rely on `tokio::join!` poll order to
establish "the concurrent writer got there first" (that too proved non-deterministic, ~25%
spurious failures on both fixed and unfixed code in an earlier version, because `join!` polling
order does not control which connection's SQL reaches SQLite first). Instead it *fully awaits*
opening a manual `BEGIN IMMEDIATE` and running a terminal `UPDATE` before either writer is even
constructed, then races both writers against a delayed commit of that held transaction.

Defect 2 load-bearing proof, exact numbers: proven in a disposable `git worktree` under
`/tmp/claude-1000/-home-ox-Sites-objetivosMios/7ef1baa9-ab8a-4721-b176-069584c82435/scratchpad`
(never by reverting the real working tree; removed after verification via
`git worktree remove --force`). The worktree's `execution.rs`/`execution_repo_test.rs` were
overwritten with the fixed working-tree versions, then `record_execution_artifact` and
`create_execution_decision` alone were reverted to their pre-fix, non-transactional bodies. The
new test then ran **20/20 failures** in the worktree (pre-fix), each with the identical signature
`panicked ... artifact must not be recorded once the attempt has gone terminal concurrently`, and
**20/20 passes** against the real, fixed working tree.

**Defect 3 — credential rotation had no compare-and-set.**
C2's `refresh` endpoint (`crates/tack-api/src/handlers/runner_protocol.rs`, `rotate_credential`
branch) writes `agent_runners.credential_hash` directly through `Repository::pool()` with no
predicate on the currently-authenticated hash, so two concurrent or retried rotations — which both
necessarily authenticate against the same still-valid old hash — silently last-writer-wins; the
loser's caller is left holding/caching a credential the server has already discarded, recoverable
only via a fresh operator-issued enrollment token. Added `Repository::rotate_runner_credential`
(`runner_id`, `expected_credential_hash`, `new_credential_hash`, `credential_expires_at`, clock) →
`Result<CredentialRotationResult, sqlx::Error>`, where `CredentialRotationResult` is
`Rotated(RotatedCredential { runner_id, credential_expires_at, rotated_at })` or `HashMismatch`.
The UPDATE runs inside `BEGIN IMMEDIATE` with
`WHERE id=? AND state='active' AND revoked_at IS NULL AND credential_hash=?`, i.e. the
compare-and-set is against the hash the caller actually authenticated with, not merely "still
active". C2's existing direct-`pool()` call path in `refresh` is untouched — C2 must adopt this
method in its own amendment; it is not deleted or redirected here.

Defect 3 regression test: `concurrent_credential_rotations_have_exactly_one_winner` — two
concurrent `rotate_runner_credential` calls against the same runner and the same
`expected_credential_hash` but different new hashes; asserts exactly one `Rotated` and one
`HashMismatch`, that the stored `credential_hash` matches exactly the winner's new hash, and that
a subsequent retry against the now-stale original hash also gets `HashMismatch` without
overwriting the winner.

Defect 3 load-bearing proof: the CAS predicate (`AND credential_hash=?`) was temporarily removed
in the real working tree (restored immediately after) and the test run 20 times: **20/20
failures**, each `left: 2 / right: 1` on the "exactly one rotation wins" assertion (both branches
reported `Rotated`) — the precise last-writer-wins symptom the defect describes. With the
predicate restored: **20/20 passes**.

- Tests added and exact commands/results: `cargo test -p tack-db --test execution_repo_test`
  (61 passed, up from 59); `cargo test -p tack-db` (176 passed across lib + all integration
  suites, 1 performance test intentionally ignored, up from 174); `cargo clippy -p tack-db --
  -D warnings` (clean); `cargo fmt -p tack-db -- --check` (clean); `git diff --check` (clean).
- Failure/adversarial case proved: see the three load-bearing proofs above — each fix was shown to
  reproduce its exact described symptom on the pre-fix code and to be clean on the fixed code, not
  merely inspected and assumed correct.
- Schema/API/contract change requested from another owner: **card C2 must change to compile again.**
  Splitting `EventApplyResult::ReplayConflict` and `CompletionResult::Conflict` breaks the
  exhaustive `match` in `crates/tack-api/src/handlers/runner_protocol.rs` by design (confirmed:
  `cargo build -p tack-api` currently fails with `E0599` on
  `EventApplyResult::ReplayConflict` at `runner_protocol.rs:961` and `E0004` non-exhaustive
  `CompletionResult` at `runner_protocol.rs:1462`). C2 must make three changes.
- First, at `runner_protocol.rs:961` (`append_events` handler): replace the single
  `EventApplyResult::ReplayConflict => Err(protocol_error(StatusCode::CONFLICT,
  StableErrorCode::Conflict, ...))` arm with two arms — `EventApplyResult::IdempotencyConflict =>
  Err(protocol_error(StatusCode::CONFLICT, StableErrorCode::IdempotencyConflict, "...", ...))` and
  `EventApplyResult::Conflict => Err(protocol_error(StatusCode::CONFLICT, StableErrorCode::Conflict,
  "...", ...))` (the existing message/details can stay on the `Conflict` arm). Delete the now-stale
  comment at `runner_protocol.rs:956-960` claiming B2 "collapses" the two cases, and
  update/replace `c2_handlers_test.rs`'s
  `event_checkpoint_conflict_response_carries_contract_correct_retryable_true` comment block
  (~L1237-1247) which currently documents the collapsed behavior as intentional — it was a real
  gap, not an intentional simplification. Consider adding a sibling test that drives the
  reused-checkpoint/changed-payload path and asserts `idempotency_conflict` + `retryable: false`,
  mirroring how `IdempotencyConflict` is already handled at `runner_protocol.rs:1110` for enqueue.
- Second, at `runner_protocol.rs:1462` (`submit_completion` handler): add
  `CompletionResult::IdempotencyConflict => Err(protocol_error(StatusCode::CONFLICT,
  StableErrorCode::IdempotencyConflict, "...", json!({"attempt_id": attempt_id})))` alongside the
  existing `CompletionResult::Conflict` arm at line 1479. **Do not** add a catch-all `_ =>` arm —
  the compiler enforcing exhaustiveness here is exactly the point.
- Third and separately, adopt `Repository::rotate_runner_credential` in the `refresh` handler's
  `rotate_credential` branch (`runner_protocol.rs` ~499-523) in place of the direct
  `sqlx::query("UPDATE agent_runners SET credential_hash=... WHERE id=? AND state='active' AND
  revoked_at IS NULL")` call, passing the principal's just-authenticated hash as
  `expected_credential_hash`, and mapping `CredentialRotationResult::HashMismatch` to the existing
  `revoked_error()` (or a new, more precise stable error — a hash mismatch is not exactly "revoked"
  and callers may want to distinguish "rotate again" from "re-enroll"; that's C2's call, not
  dictated here). This card does not touch `runner_protocol.rs` per the ownership boundary — only
  the method is provided.
- Known limitations or `not_measured` fields: this amendment does not re-run the full
  systemic-`begin()`-audit methodology against `create_execution_decision`/
  `record_execution_artifact` beyond the one load-bearing race proven above (concurrent completion
  racing the two writers); cancellation- and recovery-driven terminal transitions were not
  separately stress-tested against the two writers, though they share the identical
  `BEGIN IMMEDIATE`-serialization mechanism and the same `state NOT IN (...)`/`state IN (...)`
  eligibility predicate, so the same proof generalizes structurally. `rotate_runner_credential`
  intentionally does not add an idempotency-key/replay-dedup table (unlike every other mutating
  path in this protocol) — the defect brief asked only for the missing compare-and-set; a
  same-hash, same-new-hash exact retry after a network-lost response would currently get
  `HashMismatch` rather than a replayed success, same as the retryable branches elsewhere in this
  file before their own replay tables were added. Flagged for a future owner, not fixed here, since
  it is not what either review defect described.
- Secrets/logging review: `rotate_runner_credential` accepts and returns credential material only
  as hashes (`expected_credential_hash`, `new_credential_hash`), never a raw credential; nothing new
  is logged by any of the three fixes.
- Safe merge order and likely conflicts: no migration or shared-file change. `tack-api` will not
  compile until C2's amendment lands — that is expected, not a regression to work around here (see
  the schema/contract note above). No conflict with concurrent Wave work in `crates/tack-orch/**`
  or `docs/contracts/**`, which this amendment does not touch.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
