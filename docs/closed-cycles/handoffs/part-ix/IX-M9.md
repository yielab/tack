# IX-M9 handoff

- Base SHA / branch: `d8fd3ce` (develop tip, IX-M8-cli integrated, IX-M8 closed) /
  `agent/ix-m9`.
- Files changed: `scripts/maintainability.py`, `scripts/maintainability-baseline.json`
  (regenerated), `docs/TESTING.md`, `CLAUDE.md`, `docs/plans/human-maintainability.md`,
  `.github/workflows/ci.yml` (one stale comment describing the old ratchet-only
  mechanism), this handoff. No production or test file touched; no behavior change.
- Owns: `BUDGETS`/`EXCLUSIONS`/`check` in `scripts/maintainability.py`, the baseline, the
  numbers in `docs/TESTING.md`/`CLAUDE.md`/the plan's §1 table.

## What changed in the script

- `BUDGETS`: `test_fn_max_lines` 60 → 40, `src_comment_share` 0.35 → 0.30. `test_file_lines`
  (1000), `test_name_max_chars` (60) and the two preamble budgets are unchanged, per card.
- `check` no longer requires "over budget AND worse than baseline" everywhere. A file is a
  hard failure the moment it is over budget, **unless** its `(file, BUDGETS key)` pair is
  listed in the new `EXCLUSIONS` dict, each entry carrying a one-line reason — an excluded
  pair falls back to the old ratchet (ok only if not worse than baseline).
- `check --changed` gained the per-card budget (plan §2.3, never implemented before this
  card): sums `tests`/`test_lines` deltas against the baseline across changed files and
  fails past 15 tests / 600 test lines. This needed the baseline to start carrying each
  file's `tests`/`test_lines` counts (not only the `BUDGETS` keys), added in `cmd_baseline`.
- `changed_files()` now passes `--ignored=matching` to `git status` so a leftover
  `crates/*/tests/scratch_*.rs` is visible to `check --changed` (proved by the gate below)
  while the separate tracked-scratch failure (via `git ls-files`, in bare `check`) is what
  actually keeps one from ever being committed — the plain `--ignored` flag was tried first
  and failed silently: git collapses a whole new untracked directory to one line unless
  `matching` is asked for explicitly.
- Ratio ceiling: unchanged mechanism (baseline's `test_to_prod_ratio` + 0.005 tolerance),
  now pinned to today's measured ratio via the baseline regen below — no BUDGETS constant
  needed since the check was already baseline-relative.

## Measured numbers

`measure --totals`, before (`d8fd3ce`) and after (script/docs only, no Rust touched):
```
before: totals: prod=55602 (comments 11144) test=70098 ratio=1.261 tests=1407 sleeps_in_tests=24 env_gated=0
after:  totals: prod=55602 (comments 11144) test=70098 ratio=1.261 tests=1407 sleeps_in_tests=24 env_gated=0
```
Unchanged, as expected — this card is a ratchet-mechanism and budget-number change, not a
size-reduction card. Ratio ceiling locked at **1.261** (baseline regenerated on this tree).

`check` with an **empty** `EXCLUSIONS` (i.e. the hard caps alone, bare, no `--changed`):
**89 failures.** `len(EXCLUSIONS)` in the committed script: **89.** Equal, as required —
every failure the new hard caps would otherwise raise is named, with a reason.

`python3 scripts/maintainability.py check` (bare, real `EXCLUSIONS`): green, 295 files.
`check --changed`: green, 0 files (clean tree).

## Exclusion list (89 entries in `scripts/maintainability.py::EXCLUSIONS`)

Full per-file detail lives in the script; grouped here by why:

| Group | Entries | Basis |
|---|---:|---|
| Named plan/TODO.md exclusions (`wave2_gate.rs`, `openapi_contract`, `runner_contract` ×6, `docket_wire_contract_test`, `docket_live_test`) | 12 | TODO.md "Named exclusions, carried from plan §7" — unrelated to size budgets, kept by design |
| Test-body/file-length lines IX-M8's own handoffs report as unmet, cited by file and number | 27 | `IX-M8-api.md` (24: 13 bodies + 6 files + `remote_backup/tests.rs` + `crud.rs` + a 1-line integration drift on `control_plane/resource.rs`, 40→41), `IX-M8-db.md` (2: `orch_migrations.rs`, `execution_repo.rs` ×2 keys), `IX-M8-orch.md` (1: `reconciler/tests.rs`, kept as one file after reverting a forbidden split), `IX-M8-runner.md` (3: `engine/tests.rs`, `harness/claude_code/tests.rs` ×2 keys) |
| **Findings — not recorded by any IX-M8 handoff** (below) | 50 | comment-share tightened 0.35→0.30 by this card, and fixed-wait counts, in crates IX-M8 never re-measured for either |

## Findings

The 50 unrecorded entries are two rule families IX-M8's crate cards were never asked to
touch (their acceptance lines were test body/name/file length only):

- **`src_comment_share` (33 files)** — `tack-orch` 17, `tack-runner` 7, `tack-api` 6,
  `tack-cli` 2, `tack-db` 1. All sit between 30 % and 74 %; several (`adapters/mod.rs` 74 %,
  `adapters/registry.rs` 61 %, `local_enrollment.rs` 62 %) are well past even the *old* 35 %
  budget and were only ever green because nothing had touched them since the last baseline
  (unchanged, so "not worse"). M6 closed the crate-*wide* comment share to 19.8–20.0 %;
  no card re-walked individual files against a 30 % per-file cap, because that cap did not
  exist until this one. Candidate for an `M10-comments` pass, scoped like M6 was.
- **`test_sleeps` (13 files)** — `tack-cli` 4, `tack-runner` 5, `tack-orch` 2, `tack-api` 1
  (plus the already-named `docket_live_test.rs`, counted above). Pre-existing fixed waits
  `0064-fixed-waits.txt` already tracks; no IX-M8 card owned removing them outside the files
  it split.
- **`tack-core` (3 files, `test_name_max_chars`)** — `dependency/tests.rs`, `models/tests.rs`,
  `workflow/tests.rs`. `tack-core` was never carded in Part IX-M8 at all (orch, runner, api,
  db, cli, dedup only) — a real gap, not a decision.
- **`tack-desktop` (2 files, `test_name_max_chars`)** — `paths.rs`, `tray.rs`. IX-M8-cli's
  named `tack-desktop` exception covered only `supervisor.rs`/`supervisor/tests.rs`.
- **One drift**: `tack-api/tests/orchestration/control_plane/resource.rs` — IX-M8-api's own
  table reports this file met at exactly 40 after its work; it now measures 41. One line
  grew somewhere in a later merge (not this card's). Excluded here rather than blocking the
  tree, but worth a one-line look before the next `tack-api` card.

None of the above changes behavior, a contract, or a test's assertions — this card is the
mechanism and the number, not the fix.

## Gate

```
python3 scripts/maintainability.py check             -> green, 295 files
python3 scripts/maintainability.py check --changed    -> green, 0 files
scratch_m9.rs (16 tests, gitignored) under crates/tack-core/tests/ -> check --changed
  correctly failed ("changed files add 16 tests and 80 test lines"), then deleted
./scripts/check-comments.sh && ./scripts/check-test-hygiene.sh -> both green
python3 scripts/maintainability.py duplicate-tests crates -> 0 pairs
cargo fmt --all --check -> clean
bash -n .githooks/pre-push -> ok; both pre-push and ci.yml still call `check`/`check --changed`
```

## Re-baselined?

Yes — `scripts/maintainability-baseline.json` regenerated via `python3
scripts/maintainability.py baseline` (295 files, ratio 1.261), the card's own step 5. Not a
size-reduction re-baseline; this run only re-recorded the current tree's numbers (plus the
new `tests`/`test_lines` per-file fields the per-card budget needs) under the new script.
