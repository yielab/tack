# Repoint stale file-name comments in tack-api's test tree

Base: `develop` at `88cb4a4`. Branch: `agent/fix-stale-pointers-api-tests`. Scope:
`crates/tack-api/tests/` only — three concurrent agents own `crates/tack-api/src/`,
`crates/tack-orch/`, `crates/tack-db/`.

## Method

`comm -23` between every `.rs` basename cited in a comment anywhere under `crates/`
and every `.rs` basename actually tracked by git surfaced 32 dead names workspace-wide.
Filtered to sites inside `crates/tack-api/tests/` gave **37 comment lines** across
**24 files** citing **28 distinct dead names**. Every destination was taken from
`/var/tmp/tack-rename-map.txt` (49 renames from git's own rename detection across the
two regroup commits) — none guessed. Two names cited from tack-api comments
(`orch_repo_test.rs`, `f2_event_artifact_retention_test.rs`) point into `tack-db`;
one (`ingestion_test.rs`) points into `tack-orch` and wasn't in the rename map at all
(see below). Fixing the tack-api-side comment text is in scope regardless of which
crate the target lives in — only editing files outside `tack-api/tests/` is not.

## Split: repoint vs. rewrite

**All 37 sites were repointed**, none rewritten to drop the pointer. Every citation
found was a genuine "the same technique/shape/helper as file X" cross-reference where
naming the actual file is more useful to a future reader than restating the idea in
prose — none was a bare stand-in for an idea that had drifted from what the file
actually contains (the case CLAUDE.md's "keep the knowledge, drop the pointer" rule
targets). Every repointed destination is qualified with its subject-module directory
(`orchestration/dispatch/item.rs`, not bare `item.rs`) per the card's instruction, since
several new basenames now collide across subject modules (e.g. two `wiring.rs`).
Cross-crate pointers are qualified with the full crate-relative path
(`crates/tack-db/tests/repository/execution_repo.rs`).

## Sites needing real care

Two doc-comment blocks explain a file being loaded via `#[path]` "the same way" another
file loads its own handlers — these are the load-bearing kind CLAUDE.md calls out, where
the filename is part of the explanation, not decoration:

- `runner_protocol/decisions.rs:2-3` and `runner_protocol/lifecycle.rs:5` both cite the
  old `c1_handlers_test.rs` for "loads its own handlers via `#[path]`, the same
  technique". Verified before repointing: `handlers/executions_runner_admin.rs` (the
  rename target) still opens with `#[path = "../../src/handlers/executions.rs"]` /
  `#[path = "../../src/handlers/runner_admin.rs"]`, so the claim holds. Repointed to
  `handlers/executions_runner_admin.rs`.

Everything else was an ordinary "mirrors file X's helper" or "see file X" pointer. For
each one crossing a subject-module boundary (about half the 37), I read the destination
file and confirmed the specific claim still holds — the named helper, test function, or
behavior still exists there — before repointing rather than trusting the rename map
alone. Two are worth flagging:

- `orchestration/reconciler/broadcast.rs:10` cited `orch_repo_test.rs`/`ingestion_test.rs`
  for "the DB-state half of this proof." `orch_repo_test.rs` renamed cleanly to
  `crates/tack-db/tests/repository/orch_repo.rs`. `ingestion_test.rs` is **not** in the
  rename map — git's rename detection didn't find a 1:1 match because the tack-orch
  regroup split its 576 lines across two new files (`ingestion/runs.rs`,
  `ingestion/support.rs`) rather than moving it whole. Repointed to
  `crates/tack-orch/tests/ingestion/runs.rs` after confirming it's the one that still
  ingests runs/approvals end-to-end into a real `Repository` (the DB-state assertions
  the comment refers to); `support.rs` is fixtures only, no assertions of its own.
- `orchestration/control_plane/settings.rs:160` cites `orch_reconciler_wiring_test.rs`
  for exercising `server.rs`'s boot-time `orch_runtime.start()` call. The renamed file
  (`orchestration/reconciler/wiring.rs`) was a 100%-identical git rename (no content
  change), and reading it shows it calls `spawn_reconcilers` directly against a
  hand-built `AppConfig` — it never actually invokes `server.rs` or `AppState`. The
  claim was already this loose before the regroup (the rename didn't touch the file), so
  it isn't a regression from this reorganisation; repointed the name only, didn't
  rewrite the claim, since auditing pre-existing claim accuracy is outside this card's
  "fix the dead pointers" scope. Flagging here per the card's instruction rather than
  silently tightening it.

## Proof the diff is comments-only

```
$ git diff -U0 -- crates/tack-api/tests | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' \
    | grep -vE '^[+-][[:space:]]*(//|///|//!)'
(no output — every added/removed line starts with //, ///, or //!)
```

24 files changed, 62 insertions(+), 48 deletions(-) — the delta is from wrapping some
longer repointed names across an extra comment line, not new sentences.

## Verification

```
./scripts/check-comments.sh                                    # clean
cargo fmt --all --check                                        # clean
cargo clippy --workspace --all-targets -- -D warnings           # clean (only an
                                                                 #  unrelated proc-macro-error2
                                                                 #  future-incompat notice)
cargo nextest run --workspace                                   # 1392 passed, 6 skipped
cargo nextest list --workspace -E 'package(tack-api)' | wc -l   # 469 (unchanged)
```

Final scope check — every basename still cited in a `crates/tack-api/tests/` comment
now resolves to a tracked file:

```
$ grep -rhE '^[[:space:]]*(//|///|//!)' --include='*.rs' crates/tack-api/tests \
    | grep -oE '[A-Za-z0-9_.-]+\.rs' | sort -u | comm -23 - /tmp/known.txt
(no output)
```

## Not touched

`wave2_gate.rs` and `runner_vertical_slice.rs` kept their identity (comments inside them
were fixed, per the card's instruction, but neither file was moved or restructured).
Nothing outside `crates/tack-api/tests/` was edited.
