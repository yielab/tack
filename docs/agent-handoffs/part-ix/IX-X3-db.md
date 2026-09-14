# IX-X3-db handoff

Base `bf3958f`, branch `agent/ix-x3-db`. Scope: `execution_repo.rs`, `orch_migrations.rs`.

## Method

Added `tests/common/execution_fixture.rs`: a `Fixture` struct (repo, item, runner
"runner-a", profile "profile-a", clock) with short methods (`claim`, `heartbeat`, `lease`,
`enqueue`, `recover`, `transition`, `complete`, `count`, ...) replacing 6-8-line
multi-argument calls rustfmt had stacked one-per-line; each has a `_result` sibling keeping
the sqlx `Result` for the concurrent-duplicate tests. Rewrote every test onto it, split 6
two-claim tests into 11 (below), extracted `insert_legacy_request`/`set_legacy_snapshot`
inside the largest body (`m060_quarantines_...`) to collapse its repeated raw-SQL blocks.

`orch_migrations.rs` (1189 lines, bodies already ≤40): natural split points exist but would
relocate shared helpers for a file meeting every other budget — left as one file, per the
card's own fallback.

## Before / after

| | before | after |
|---|---|---|
| `execution_repo.rs` lines | 4176 | 2900 |
| tests in file | 61 | 69 |
| bodies over 40 lines | 38 | 20 |
| max body | 174 | 131 |
| `orch_migrations.rs` lines | 1189 | 1189 (untouched) |
| new `execution_fixture.rs` | — | 728 |
| `llvm-cov -p tack-db` TOTAL lines | 3873, missed 1166, **69.89%** | same |

+8 tests, entirely from the 6 splits below. No deletion, no assertion changed (Deletions: none).

## Splits (two-claims-in-one-test rule)

- `event_replay_changed_payload_is_idempotency_conflict` → itself +
  `event_replay_out_of_order_checkpoint_is_benign_conflict`
- `claim_fence_replay_and_terminal_state_are_atomic` → `claim_second_claimer_blocked_
  stale_heartbeat_writes_nothing` + `event_replay_is_idempotent_under_one_fence` +
  `completion_replay_is_idempotent_and_terminal_lock_holds`
- `token_guards_and_operator_requeue_are_idempotent` → `pending_runner_token_issue_fails_
  on_mismatched_capacity` + `operator_requeue_clears_cancellation_and_is_idempotent`
- `recovery_foreign_revoked_and_terminal_are_not_recovered` → `recovery_foreign_fence_
  is_stale` + `recovery_terminal_replay_is_idempotent_and_rejects_changes` +
  `recovery_after_runner_revoked_is_stale`
- `completion_and_idempotency_conflicts_are_distinguished` → `completion_conflict_before_
  replay_row_is_benign` + `completion_changed_after_replay_is_idempotency_conflict`
- `structured_events_and_cancellation_observation_replay` → `structured_events_replay_is_
  applied_once` + `cancellation_observation_replays_after_first_success`

## Unmet (rewritten in `maintainability.py` `EXCLUSIONS`)

- `execution_repo.rs` `test_file_lines`: 2900 (budget 1000) — still a multi-race narrative.
- `execution_repo.rs` `test_fn_max_lines`: 20 of 69 bodies exceed 40 (up to 131 — `m060_...`,
  `enqueue_rejects_malformed_...`, `attempt_start_transitions_...`, the
  `concurrent_duplicate_*` family, recovery/heartbeat narratives): each is one race/protocol
  proof; further extraction risks obscuring which assertion is which.
- `orch_migrations.rs` `test_file_lines`: 1189 (budget 1000), untouched — see Method.

**Finding:** the inherited exclusion said "38 of 42 bodies" — real denominator was **61**
(`git show bf3958f:...execution_repo.rs | grep -c '#\[tokio::test\]'`). Corrected.

## Gate (final run, all green)

```
maintainability.py baseline && check      → ✓ 296 files checked
check-comments.sh / check-test-hygiene.sh / duplicate-tests → ✓ / ✓ / 0 pairs, all 7 crates
cargo fmt --all --check (+ tack-desktop)  → clean
cargo clippy --workspace --all-targets    → clean
cargo nextest -E 'package(tack-db)'       → 199 run: 199 passed, 1 skipped
3x cargo test -p tack-db --tests -q       → no FAILED/panicked
cargo llvm-cov -p tack-db --summary-only  → 3873 lines, missed 1166, 69.89%
```

Note: baseline once ran while the fixture file was untracked (ratio read 1.238 not 1.251,
`git ls-files` skipped its 728 lines) — re-baselined post-commit, gate re-ran clean.

## Commits

`23e34f3` test: fixture + rewrite · `c2bbe52` chore: rebaseline + correct counts ·
`48cd5ae` docs: this handoff · `8e11503` chore: re-baseline post-tracking
