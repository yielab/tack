# IX-X2 handoff

Base `3e63030`, branch `agent/ix-x2`, final `999ad72`. Files: the 12 sleep files, 5
name files, `tests/orchestration/control_plane/resource.rs`, `tack-test-support`
(new `poll.rs` + wiring), `scripts/maintainability.py` + its baseline.

## Budget check

`python3 scripts/maintainability.py check --changed` → `✓ maintainability budgets
hold (20 files checked)`. Bare `check` also green (295 files). `measure --totals`:
prod=55604 test=70046 ratio=1.26 tests=1407 sleeps_in_tests=3 (all
`docket_live_test.rs`, the named exclusion — was 24 workspace-wide before).

## What was removed

- 21 fixed/looping sleeps (class 8) across the 12 named files → bounded polls via
  `tack_test_support::poll_until`/`poll_until_sync`, or dropped outright in
  `execution_observability`/`execution_retention`'s two disabled-sweep tests, where
  the assertion is already synchronously true (no task is ever spawned) — reverting
  to the `sleep` there would be superstition, not coverage.
- 3 duplicate local `poll_until<T>` fns (tack-cli embedded-runner tests) → one
  `use tack_test_support::poll_until_sync as poll_until;` each.
- 8 test names over 60 chars (class 2) → shortened, same claim.
- `patch_token_field_is_tri_state`'s 41-line body (class 1: table → rows already in
  place; extracted the row-builder itself) → 15 lines via a `patch_token_cases()` fn.
- 19 EXCLUSIONS entries (12 sleeps, 5 names, 2 fn-lines) — all now unconditionally 0
  or ≤ budget, not just ratcheted.

## Re-baselined?

Yes. `git diff scripts/maintainability-baseline.json | grep '^[-+] '` shows every
changed entry moving down (test_sleeps to 0, name/body lengths down) and the
workspace totals improving (`test_sleeps` 24→3, `test_to_prod_ratio` 1.261→1.26).
Nothing went up.

## Gate tails

fmt/clippy/nextest/hygiene/comments/duplicate-tests/list-fixed-waits: all clean.
nextest (the 6 affected packages): 1221 passed, 7 skipped, repeatedly. tack-desktop
has no `--lib` target (bin-only); `cargo test --bin tack-desktop -q paths`/`tray`/
bare: 5, 5, 34 passed.

**Finding, not fixed:** `cargo test -p tack-api -p tack-orch -p tack-runner --lib
--tests` (run 3x per the card) fails intermittently, always
`server::tests::a_configured_log_file_receives_the_lines_that_are_logged` ("it
holds: \"\""). Root cause (confirmed by instrumenting `try_init`'s return value):
three tests in `tack-api/src/server/tests.rs` reach `serve_inner`→`init_tracing`,
which `try_init()`s a *process-global* subscriber. Only the log-file test's doc
comment claims exclusivity ("must stay the only test in this binary that does"),
but `persisted_enable_pref_never_auto_starts_non_loopback_bind` and
`serve_with_ready_signals_the_real_bound_address` trigger it too; whichever of the
three wins leaves the other two writing to a subscriber with no file layer —
pre-existing, not new. This card's now-instant orch_runtime tests shift scheduling
enough to expose it far more often (0/28 on base `3e63030` vs. reproducible here;
isolated to `orch_runtime/tests.rs` by reverting only that file and rerunning). Real
fix (a `tracing::subscriber::set_default` guard, or stopping the other two from
calling `serve_inner`) touches `server.rs` production code — outside this card's
scope. nextest (one process per test, the repo's actual runner) never hits it.
Recommend a follow-up card.
