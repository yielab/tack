# III-C4 handoff

- Base SHA / branch / final SHA: `f14019b` / `agent/iii-c4-crash-matrix` /
  the commit containing this handoff. The branch was rebased onto C3 through
  `1b6dede`; C3 source files in ancestry are not C4-owned changes.
- C4 commits: `fbc2b12` adds the repository crash matrix, `e9ee884` injects
  both partial-event and checkpoint-update faults, `3ece27f` adds runner
  process-boundary tests, and `54bce23` adds recovery-report retry,
  ProcessRunning, and quarantined-reoffer coverage.
- Files owned: `crates/tack-api/tests/runner_vertical_slice.rs`,
  `crates/tack-api/tests/runner_vertical_slice/repository_crash.rs`,
  `crates/tack-runner/tests/crash_matrix.rs`, and this handoff. No production
  API, database, runner, router, fixture, or contract source was modified by
  C4.
- Tests run on the current C4 branch: `cargo test -p tack-api --test
  runner_vertical_slice` (5 passed) and `cargo test -p tack-runner --test
  crash_matrix` (7 passed). At the `54bce23` runner-test change, `cargo fmt
  --all -- --check`, `cargo clippy -p tack-runner --test crash_matrix -- -D
  warnings`, and `git diff --check` also passed.

## Covered boundaries

- Before claim commit: an SQLite abort before attempt insertion rolls back the
  request lease, runner capacity, attempt row, and fence allocation; retry
  starts at fence 1.
- Event persistence: abort after the first of a two-event batch, and separately
  before the checkpoint update, leave no rows and no checkpoint. The successful
  batch then replays without duplicate rows.
- Completion and cancellation intent: injected database faults leave both
  attempt/request state non-terminal or cancellation unrequested; the retry is
  safe. Completion proves its attempt/request state transaction rolls back as a
  unit.
- Before local spawn: a worktree-provisioning failure leaves a prepared journal;
  restart reports `ProcessStopped` without a harness start.
- After spawn before start acknowledgement, completion acknowledgement, and
  cancellation acknowledgement: the runner records an ambiguity observation,
  quarantines acknowledged evidence, cancels best-effort, and does not blindly
  re-report the failed mutation.
- Failed ambiguity delivery remains `RecoveryPending` with an unresolved local
  journal. A restart retries it exactly once, then quarantines server-acknowledged
  evidence without another spawn. `ProcessRunning` is reported as ambiguity and
  quarantined, never returned as a safe completed recovery. A server reoffer of
  the same quarantined attempt fails with `JournalError::AlreadyExists` before
  preparation or another spawn.

## Remaining B2-rebase / integration gaps

- The repository suite is deliberately still pre-amendment. Its request helper
  lacks B2 migration 053's required canonical `request_snapshot`; it will need
  a fixture-complete immutable snapshot before rebasing. Rebased claim cases
  must exercise `claim_execution_idempotent_with_snapshot` and claim-replay
  responses, not the legacy lease-only claim method.
- `expired_post_claim_ambiguity_never_grants_a_second_fence` currently calls
  legacy `classify_expired_attempt`. After rebase it must instead use B2's
  audited `recover_attempt(NeedsOperator)` path and assert request state,
  capacity normalization, and one recovery audit. It must never encode the old
  attempt-only classification behavior as an acceptance result.
- Event cases must move to B2's structured event-batch result and assert the
  accepted/duplicate/checkpoint response identity on replay, in addition to
  the existing transaction-fault assertions. Completion must assert its typed
  replay response, terminal details, and capacity restoration exactly once.
- Cancellation coverage currently stops at cancellation-request persistence.
  Rebased C4 must inject cancellation observation/replay faults and assert its
  idempotency identity, payload/result preservation, request/attempt terminal
  states, and capacity release once.
- Enrollment/revocation remains absent from the C4 repository matrix. Once B2's
  final token/requeue API is present, add fault/race tests for pending-runner
  creation plus hash-only token issuance, one winner for concurrent redemption,
  revoke-before-redeem, and no raw token in rows, error bodies, or audit data.
- C4 is not a C5 router test: it has no production authenticated route,
  endpoint-envelope, or API/runner restart-through-router coverage. The runner
  tests use an explicit fake protocol/process seam; API audit assertions depend
  on the B2 recovery and operator-requeue integration above.

- Stale-assumption review: no correction was required on the current branch.
  The runner matrix's `claim_committed` label denotes only the fake protocol
  handoff, not a database assertion; the repository suite provides that actual
  SQLite transaction coverage. The legacy expiry-classification test is retained
  solely as a documented B2-rebase gap, not as proof of final recovery semantics.
