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
- Tests run on the rebased current C4 branch: `cargo test -p tack-api --test
  runner_vertical_slice` (5 passed) and `cargo test -p tack-runner --test
  crash_matrix` (7 passed). `cargo clippy -p tack-api --test
  runner_vertical_slice -- -D warnings`, `cargo clippy -p tack-runner --test
  crash_matrix -- -D warnings`, `cargo fmt --all -- --check`, and `git diff
  --check` passed.

## Covered boundaries

- Before claim commit: an SQLite abort before attempt insertion rolls back the
  request lease, runner capacity, attempt row, and fence allocation; retry
  starts at fence 1.
- Event persistence: abort after the first of a two-event batch, and separately
  before the checkpoint update, leave no rows and no checkpoint. The successful
  structured event result then replays authoritatively without duplicate rows.
- Completion and cancellation intent: injected database faults leave both
  attempt/request state non-terminal or cancellation unrequested; the retry is
  safe. Completion proves its attempt/request state transaction rolls back as a
  unit. Runner response-loss cases retain a durable terminal outbox rather than
  fabricating an ambiguity/quarantine path.
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

## Remaining scope boundary

- The C4-owned matrix uses repository seams and runner fakes; C2/C5 still own
  authenticated route-level restart verification. Enrollment concurrency and
  heartbeat replay are B2 repository contracts but do not yet have dedicated
  C4 fault-injection cases in this focused suite.
- C4 is not a C5 router test: it has no production authenticated route,
  endpoint-envelope, or API/runner restart-through-router coverage. The runner
  tests use an explicit fake protocol/process seam; API audit assertions depend
  on the B2 recovery and operator-requeue integration above.

- Stale-assumption review: the canonical request snapshot, idempotent claim API,
  audited recovery outcome, structured event result, typed terminal acknowledgements,
  cancellation evidence, C3 outbox, and C3 response fields are all used directly.
  The runner matrix's `claim_committed` label denotes only the fake protocol handoff;
  the repository suite provides the actual SQLite transaction coverage.
