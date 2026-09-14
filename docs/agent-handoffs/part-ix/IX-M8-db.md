# IX-M8-db handoff

- Base SHA / branch / final SHA: base `7225c5e` (IX-M8-dedup, IX-M8-runner, IX-M8-orch
  integrated; 0 duplicate-test pairs), branch `agent/ix-m8-db`, final `afa755f`.
- Files changed (must equal ownership list): all under `crates/tack-db/tests/**` or
  `crates/tack-db/src/**/tests.rs`, no production file touched. `git diff --stat
  7225c5e...HEAD` shows exactly: `crates/tack-db/src/repo/templates/tests.rs`,
  `crates/tack-db/tests/migrations/item_source_migration.rs`,
  `crates/tack-db/tests/migrations/orch_metrics.rs`,
  `crates/tack-db/tests/migrations/orch_migrations.rs`,
  `crates/tack-db/tests/perf_test.rs`,
  `crates/tack-db/tests/repository/event_artifact_retention.rs`,
  `crates/tack-db/tests/repository/execution_repo.rs`,
  `crates/tack-db/tests/repository/execution_retention.rs`,
  `crates/tack-db/tests/repository/integration.rs`,
  `crates/tack-db/tests/repository/orch_repo.rs`,
  `crates/tack-db/tests/repository/status_update_checked.rs`,
  `crates/tack-db/tests/repository/version_concurrency.rs` — 12 files, no unowned file
  touched.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/` untouched; this crate's
  tests don't exercise the runner-v1 wire contract.
- Behavior implemented: none — test-only card. Every production file in `tack-db` is
  byte-identical to base (`git diff 7225c5e -- crates/tack-db/src` is empty except the
  moved-not-edited inline `#[cfg(test)]` boundary in `repo/templates.rs`, which is itself
  test code).
- Tests added and exact commands/results: no new behavior tests. Test count moved 189 →
  192 (+3): one 72-line, four-claim test in `templates/tests.rs` was split into four
  single-claim tests (rule 1) before this session started. `cargo nextest run --workspace
  -E 'package(tack-db)'`: `191 tests run: 191 passed, 1 skipped` (the 1 skipped is the
  `#[ignore]`d `perf_test`; the 192nd test — 191 + 1 — accounts for it). Stable across
  three consecutive `cargo test -p tack-db --tests` runs in this session (137/137 each
  time in the `repository` binary; no flakes).
- Failure/adversarial case proved: N/A — no behavior changed.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced.
- Secrets/logging review: N/A, no production code touched.
- Safe merge order and likely conflicts: this branch only touches files under
  `crates/tack-db/tests/**` and `crates/tack-db/src/repo/templates/tests.rs`. No other open
  IX-M8-<crate> card owns `tack-db`. Expect no conflicts against `develop`'s current tip
  beyond ordinary line-shift noise.
- Checklist: no unowned files (diff-stat above), no live secret (no production code
  touched), no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence |
|---|---|
| `duplicate-tests` stays at 0 for `tack-db` | `python3 scripts/maintainability.py duplicate-tests crates/tack-db` → `0 near-identical pairs across files` |
| `0064-fixed-waits.txt`/`list-fixed-waits.py` gains no `tack-db` line | `python3 scripts/list-fixed-waits.py` → 2 waits, both `tack-api`/`tack-orch`, none in `tack-db` |
| Workspace `measure --totals` test-line count goes down, not up | before `test=70749 tests=1410`, after `test=70336 tests=1413` (see *Measured numbers*) |
| `cargo llvm-cov -p tack-db --fail-under-lines 70`'s underlying number is not lowered by this card | before **69.89 %**, after **69.89 %** (byte-identical production code; see *Measured numbers*) |
| 9 of 16 test files now have every body ≤ 40 lines, every name ≤ 60 chars, file ≤ 1000 lines | `measure --json crates/tack-db` — see *Budget check* for the full per-file table |
| `orch_migrations.rs` bodies all ≤ 40 lines | `measure --json` → `test_fn_max_lines: 40` for that file |
| `execution_repo.rs` remains over budget on both body length and file length | **unmet, not "not worsened"** — 38 of its 42 tests still exceed 40 lines (25 exceed the 60-line hard ceiling), file is 4176 lines (was 4308); see *Budget check* |
| `cargo test -p tack-db --tests` is not flaky under repeated runs | run three times in this session, 137/137 passed each time |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

All commands run from `/tmp/ix-m8-db` with `CARGO_TARGET_DIR=/tmp/ix-m8-db-target`, except
the "before" numbers, measured against a `git worktree add --detach /tmp/ix-m8-db-base
7225c5e`.

- `python3 scripts/maintainability.py measure --totals` (workspace), before:
  `totals: prod=55599 (comments 11144) test=70749 ratio=1.272 tests=1410 sleeps_in_tests=26
  env_gated=0`
- Same, after (this branch's HEAD): `totals: prod=55599 (comments 11144) test=70336
  ratio=1.265 tests=1413 sleeps_in_tests=26 env_gated=0`
- `python3 scripts/maintainability.py measure --json crates/tack-db`, before: 16 test
  files, 189 tests, 11272 test lines (file+inline).
- Same, after: 16 test files, 192 tests, 10859 test lines (file+inline) — down 413 lines,
  up 3 tests (the pre-session templates split).
- `cargo llvm-cov -p tack-db --summary-only`, before (on the detached-worktree base):
  **TOTAL lines 3873, missed 1166, 69.89 %** (matches the card prompt's stated baseline
  exactly).
- Same, after: **TOTAL lines 3873, missed 1166, 69.89 %** — unchanged, as expected since no
  production line moved.
- `python3 scripts/maintainability.py duplicate-tests crates/tack-db`: `0 near-identical
  pairs across files` (both before my session's edits and after).
- `python3 scripts/list-fixed-waits.py`: 2 waits total, both outside `tack-db`
  (`tack-api/src/orch_runtime/tests.rs:347`, `tack-orch/tests/docket_live_test.rs:145`).

## What a stranger still cannot do

Nothing new — this card changes no behavior and no public surface. A stranger arriving at
`tack-db` after this card can read 9 of the crate's 16 test files (`item_source_migration.rs`,
`version_concurrency.rs`, `orch_metrics.rs`, `perf_test.rs`, `execution_retention.rs`,
`event_artifact_retention.rs`, `orch_repo.rs`, `orch_migrations.rs`'s bodies,
`status_update_checked.rs`, `integration.rs`) without meeting a test body over 40 lines or
a file over 1000 (`orch_migrations.rs`'s file length is the one exception among these, at
1190 lines — see below). They still cannot skim `execution_repo.rs` in one sitting: it is
4176 lines with 38 tests still over the 40-line body budget, 25 of those over the 60-line
hard ceiling, the largest at 174 lines (`m060_quarantines_all_nonterminal_malformed_legacy_snapshots`).
That file's own tests are individually well-factored already (most already call shared
fixture builders like `ready_repo`, `completion_input`, `cancellation_input`,
`recovery_input`) — its remaining size is concurrency and state-machine narrative, not
boilerplate duplication, and further reduction risks changing what each test actually
proves.

## Budget check

`python3 scripts/maintainability.py check --changed` (final tree, all commits staged):

```
✓ maintainability budgets hold (293 files checked)
```

(Ratchet-based: passes because every touched file is no worse than the committed baseline.
It does **not** mean the crate meets this card's stricter ≤40/≤60/≤1000 acceptance — see
below for the honest gap.)

`measure --totals` before/after: see *Measured numbers* — workspace `test` lines
70749 → 70336 (**down**, satisfying the acceptance bar), `tests` 1410 → 1413.

`measure --json crates/tack-db` per-file status, final tree — only files with something
still over budget:

| File | file_lines | max body | max name |
|---|---|---|---|
| `tests/migrations/orch_migrations.rs` | **1190** (>1000) | 40 (OK) | 57 (OK) |
| `tests/repository/execution_repo.rs` | **4176** (>1000) | **174** (>40, >60) | 60 (OK) |

Every other one of the 16 test files is fully within budget (file ≤ 1000, every body
≤ 40, every name ≤ 60). Named exclusions (`*_contract*` binaries, `tests/live/`) do not
apply to either of these two files — neither is a contract binary or under `tests/live/`.

**`orch_migrations.rs` (1190 lines, unmet on file length only):** every test body is now
≤ 40 lines and no name exceeds 60 chars. The file itself could only be brought under 1000
by deleting or splitting migration coverage — its own doc comment explains it proves
fresh-install, upgrade-in-place-per-checkpoint, FK enforcement, and (for the two table
rebuilds) row/field preservation, an empty `foreign_key_check`, old-PK uniqueness, and
crash-interrupted recovery, none of which is redundant with another file. Splitting the
file is explicitly forbidden by this card's own instructions (the sibling IX-M8-orch card
was rejected for exactly that and had to revert it). **Reported as unmet, not "not
worsened."**

**`execution_repo.rs` (4176 lines, unmet on both file length and body length):** this is
the file the card text itself flagged ahead of time as needing "every honest mechanism"
and possibly still ending over budget. What was done: extracted five crate-wide helpers
(`runner_capacity`, `attempt_state`, `request_state`, `count_where`,
`expect_cancelled`/`expect_replayed_cancellation`) that replaced ~40 call sites of
5-line raw-SQL/`matches!`-counting boilerplate with one-line calls, verified against the
full `nextest -E 'package(tack-db)'` suite after every substitution (no assertion, query,
or behavior changed — confirmed by diffing each substitution's SQL/predicate text against
the original before deleting it). This brought the file from 4308 to 4176 lines and moved
8 of its 42 tests under the 40-line body budget. The remaining 38 tests (25 of them over
the 60-line hard ceiling, up to 174 lines) are concurrency and state-machine proofs
already built on shared fixture helpers (`ready_repo`, `ready_completion_attempt`,
`completion_input`, `cancellation_input`, `recovery_input`, `claim_lease`, `request`) —
their remaining length is one narrative per test (seed → race two calls → assert exactly
one commits and one replays → assert the DB's terminal state), not duplicated setup.
Reducing them further would mean either extracting each test's unique race/assertion logic
into single-use helper functions (which does not reduce total complexity, only relocates
it, and risks obscuring which assertion belongs to which scenario) or genuinely trimming
assertions (forbidden — several of these are the sole proof of a fencing/idempotency
invariant). **Reported as unmet, not "not worsened":** 38 of 42 tests still exceed 40
lines; the file is 4176 of a 1000-line budget.

## What was removed

Per-file, in commit order (the first three commits predate this session and were left
untouched per the resuming instructions):

- `src/repo/templates/tests.rs` — **rule 1** (variants → rows / claims split apart):
  `seeds_three_construction_verticals_with_fields_and_workflows` (72 lines, four unrelated
  claims) split into `construction_seed_creates_four_templates_once_each`,
  `wood_frame_keeps_construction_vocabulary`, `wood_frame_has_stud_spacing_select_field`,
  `sip_panel_template_has_panel_count_number_field`, sharing new
  `seeded_construction_templates()`/`find_template()` helpers.
  `create_template_with_orchestration_round_trips_through_get_and_list` (67-char name) was
  renamed to `template_with_orchestration_round_trips_through_get_and_list` and its 50-line
  body cut to 35 by extracting `sample_orchestration()`/`create_with_orchestration()`.
- `tests/migrations/item_source_migration.rs` — **rule 1**: `update_item_never_changes_source`
  (42) extracted `create_untrusted_item()` shared setup;
  `upgrade_in_place_backfills_pre_migration_items_to_untrusted` (45) moved two inline
  rationale comments into a leading doc comment (not counted toward body length).
- `tests/repository/version_concurrency.rs` — **rule 1**:
  `check_and_update_parent_status_bumps_version` (59) extracted `item_titled()` to replace
  two near-identical `CreateItem` literals; body now 35.
- `tests/repository/status_update_checked.rs` — fixed a stray `#[tokio::test]` attribute
  the previous (cut-off) agent had left on a plain helper function (`assert_column_count`,
  which takes parameters and cannot be a test); not a size reduction, a compile fix. Also
  **rule 1**: extracted `titled_item()`, `fill_column()`, `assert_column_count()`,
  `assert_wip_limit_exceeded()`, `apply_checked()` to shrink four bodies that were 70, 32,
  etc. lines down to ≤ 33.
- `tests/migrations/orch_metrics.rs` — **rule 1** throughout: extracted
  `applied_migrations`/`assert_applied_in_order`/`assert_all_applied`,
  `assert_tables_missing`/`assert_tables_present`, `count_rows`, `daily_event_count` to
  shrink 4 oversized bodies (43–54 lines) to ≤ 32 each.
- `tests/perf_test.rs` — **rule 1**: extracted `seed_items()` and `measure_latencies()`
  from the one 69-line test, now 24.
- `tests/repository/execution_retention.rs` — **rule 1**: extracted `seed_stale_and_fresh_replays`,
  `insert_runner`, `file_backed_repo`, `insert_active_attempt`, `mark_attempt_succeeded`,
  `assert_remaining_event_ids` to shrink 4 bodies (42–56 lines) to ≤ 33.
- `tests/repository/event_artifact_retention.rs` — **rule 1**: extracted
  `set_content_reference`/`stored_content_reference`, `log_artifact`, `append_batch`,
  `remaining_event_ids`, `record_patch_artifact`, `remaining_artifact_ids`,
  `expire_and_delete_artifacts`, `purge_events`, `backdate_events`; moved the
  `artifact_delete_guard_skips_only_racily_resolved_row` case table from an inline `let
  cases = [...]` to a top-level `const GUARD_CASES` (same rule: shared-setup extraction,
  not a new claim). 5 bodies (50–77 lines) now ≤ 33.
- `tests/repository/orch_repo.rs` — **rule 1** throughout (largest file fix): extracted
  `create_plane`, `update_token`, `upsert_link`, `simple_link`, `task`, `batch_task`,
  `task_count`, `run`, `approval`, `simple_approval`, `agent_approval`,
  `correlated_approval`, `pending_with_context`. 11 bodies (42–79 lines) now ≤ 40; file
  itself dropped 1007 → 948 lines (**file-length acceptance now met**, was previously
  unmet at the card's stated starting measurement).
- `tests/migrations/orch_migrations.rs` — **rule 1**: extracted
  `assert_tables_missing`/`assert_tables_present`, `fk_violation_cases`, `insert_task_row`,
  `insert_run_attempt`, `run_fixtures_037`, `insert_orch_link_row`,
  `fetch_plane_config_secrets`, `insert_pre_038_approval`, `insert_orphaned_legacy_run`. 8
  bodies (42–60 lines) now ≤ 40. File length (1190) remains unmet — see *Budget check*.
- `tests/repository/integration.rs` — **rule 1** throughout: introduced struct-update-
  syntax builders `item_input`, `project_input`, `template_input`, `select_field`,
  `board_input`, `sprint_input`, plus `set_item`/`item_now`/`assert_sprint_id`/
  `status_update` wrappers for the repeated update-then-refetch pattern. 16 bodies
  (42–71 lines) now ≤ 40; file dropped 1285 → 981 lines (**file-length acceptance now
  met**, was previously unmet).
- `tests/repository/execution_repo.rs` — **rule 1**: extracted `runner_capacity`,
  `attempt_state`, `request_state`, `count_where`, `expect_cancelled`/
  `expect_replayed_cancellation`, replacing ~40 repeated raw-SQL/`matches!`-counting call
  sites. 8 of 42 bodies now ≤ 40; file dropped 4308 → 4176 lines. **Remaining 38 bodies and
  the file length itself are unmet** — see *Budget check* for why further reduction was
  not attempted this session.

No test was deleted outright and no assertion was weakened anywhere in this session — every
change above either extracted repeated setup/assertion code into a named helper (rule 1)
or split one test proving several claims into several single-claim tests (also rule 1, the
"variants → rows" reading applied to a whole test rather than one assertion block).

## Re-baselined?

`no`.

## Context spent

- Tokens read before the first edit (cold start): this session resumed a cut-off agent
  mid-card per the prompt's own "You are resuming" section, so the cold-start read was the
  resuming section itself (`git status`/`git log`/`git diff` against the inherited branch)
  rather than the full read-first list — the read-first list was consumed by the prior
  (cut-off) agent.
- Context size at handoff: not separately measured; this session ran to substantial length
  given the crate's 16-file, 42-test scope on the largest file alone.
- Files opened and not used: none — every file opened was one of the 16 test files this
  card owns.
- Read-list lines that were wrong: none identified.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
