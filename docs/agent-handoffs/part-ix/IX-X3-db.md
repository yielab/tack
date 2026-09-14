# IX-X3-db handoff

Base `bf3958f`, branch `agent/ix-x3-db`. Scope: `crates/tack-db/tests/repository/execution_repo.rs`
and `crates/tack-db/tests/migrations/orch_migrations.rs` — shrink over-budget test files by
introducing a shared fixture, per `scripts/maintainability.py`'s `EXCLUSIONS`.

## Method

Added `crates/tack-db/tests/common/execution_fixture.rs`: a `Fixture` struct (repo, item,
runner "runner-a", profile "profile-a", clock) with short methods (`claim`, `heartbeat`,
`lease`, `enqueue`, `recover`, `transition`, `complete`, `count`, ...) replacing the
6-8-line multi-argument calls rustfmt had stacked one-per-line. Each has a `_result`
sibling keeping the sqlx-level `Result` for the concurrent-duplicate tests that assert
both racing branches succeed before checking which one won. Rewrote every test in
`execution_repo.rs` onto it. Split 6 tests that asserted two-or-more unrelated claims into
11 (rule: "a test that asserts two unrelated claims may become two tests") — see list
below. Extracted two local helpers (`insert_legacy_request`, `set_legacy_snapshot`) inside
`m060_quarantines_all_nonterminal_malformed_legacy_snapshots`, the file's largest body, to
collapse its two repeated raw-SQL blocks into one-line calls.

`orch_migrations.rs` (1189 lines, `test_fn_max_lines` already met): natural split points
exist (its own section comments: fresh install / upgrade-in-place / FK enforcement /
redispatch / 032-036 additive / 037-038 rebuilds) but splitting would relocate shared
helpers (`table_exists`, `column_exists`, `insert_control_plane`, `seed_item`) into
`tests/common` for a file already meeting every other budget — left as one file per the
card's "otherwise leave it and say so."

## Before / after

| | before | after |
|---|---|---|
| `execution_repo.rs` lines | 4176 | 2900 |
| tests in that file | 61 | 69 |
| bodies over 40 lines | 38 | 20 |
| max body | 174 | 131 |
| `orch_migrations.rs` lines | 1189 | 1189 (untouched) |
| new `execution_fixture.rs` | — | 728 |
| `cargo llvm-cov -p tack-db --summary-only` TOTAL lines | 3873, missed 1166, **69.89%** | 3873, missed 1166, **69.89%** |

Test count is +8, entirely from the 6 splits below — no test added net new coverage
intent, no test deleted.

## Splits (two-claims-in-one-test rule)

- `event_replay_changed_payload_is_idempotency_conflict` → itself +
  `event_replay_out_of_order_checkpoint_is_benign_conflict`
- `claim_fence_replay_and_terminal_state_are_atomic` →
  `claim_second_claimer_blocked_stale_heartbeat_writes_nothing` +
  `event_replay_is_idempotent_under_one_fence` +
  `completion_replay_is_idempotent_and_terminal_lock_holds`
- `token_guards_and_operator_requeue_are_idempotent` →
  `pending_runner_token_issue_fails_on_mismatched_capacity` +
  `operator_requeue_clears_cancellation_and_is_idempotent`
- `recovery_foreign_revoked_and_terminal_are_not_recovered` →
  `recovery_foreign_fence_is_stale` +
  `recovery_terminal_replay_is_idempotent_and_rejects_changes` +
  `recovery_after_runner_revoked_is_stale`
- `completion_and_idempotency_conflicts_are_distinguished` →
  `completion_conflict_before_replay_row_is_benign` +
  `completion_changed_after_replay_is_idempotency_conflict`
- `structured_events_and_cancellation_observation_replay` →
  `structured_events_replay_is_applied_once` +
  `cancellation_observation_replays_after_first_success`

## Deletions

None.

## Unmet (updated in `scripts/maintainability.py` `EXCLUSIONS`)

- `execution_repo.rs` `test_file_lines`: 2900 (budget 1000) — still a multi-race
  state-machine narrative; splitting by operation would fragment the shared `Fixture`
  usage without reducing total complexity.
- `execution_repo.rs` `test_fn_max_lines`: 20 of 69 bodies still exceed 40 (up to 131:
  `m060_quarantines_all_nonterminal_malformed_legacy_snapshots`,
  `enqueue_rejects_malformed_and_contradictory_snapshots`,
  `attempt_start_transitions_are_idempotent_and_freeze_facts`, the `concurrent_duplicate_*`
  family, and the recovery/heartbeat multi-step narratives) — each is one race or
  multi-step-protocol proof; further extraction risks obscuring which assertion belongs to
  which scenario.
- `orch_migrations.rs` `test_file_lines`: 1189 (budget 1000), untouched — see Method.

## Finding

The `EXCLUSIONS` entry this card inherited said "38 of 42 bodies" — the true original
denominator was **61** tests (`git show bf3958f:.../execution_repo.rs | grep -c
'#\[tokio::test\]'`), not 42. Corrected in the rewritten entry.

## Gate (final run, all green)

```
python3 scripts/maintainability.py baseline && python3 scripts/maintainability.py check
  → ✓ maintainability budgets hold (295 files checked)
./scripts/check-comments.sh          → ✓ no board archaeology
./scripts/check-test-hygiene.sh      → ✓ tests take their temporary paths from a guard
python3 scripts/maintainability.py duplicate-tests crates
  → 0 near-identical pairs across files, all 7 crates
cargo fmt --all -- --check (+ tack-desktop separately) → clean
cargo clippy --workspace --all-targets -- -D warnings  → clean
cargo nextest run --workspace -E 'package(tack-db)'
  → 199 tests run: 199 passed, 1 skipped
3x `cargo test -p tack-db --tests -q | grep FAILED|panicked` → nothing printed
cargo llvm-cov -p tack-db --summary-only → TOTAL 3873 lines, missed 1166, 69.89%
```

No commits made this session (uncommitted in the worktree; report says why: not asked to
commit).
