# ADR 0064 rollout step 4 — regroup tack-db's test binaries

Mechanical regrouping only: no test body, assertion, test name, or `#[test]`/
`#[tokio::test]` attribute changed. What moved is file layout, module
declarations, and the `use`/path lines each file needs to keep reaching
`tests/common/mod.rs` from its new location.

## Grouping

`crates/tack-db/tests/` goes from 12 files (12 independent binaries, each its
own link) to 3 binaries: `repository.rs`, `migrations.rs`, and `perf_test.rs`
(untouched, stays its own binary — it is `#[ignore]`d, seeds 50k rows, and is
selected on its own; folding it into a group would link it on every run of
that group for nothing).

**`tests/repository.rs`** — the CRUD/query/concurrency side, exactly the
seven files suggested by the card, moved under `tests/repository/` with the
`_test` suffix dropped:

| Old file | New module |
|---|---|
| `integration_test.rs` | `repository/integration.rs` |
| `execution_repo_test.rs` | `repository/execution_repo.rs` |
| `orch_repo_test.rs` | `repository/orch_repo.rs` |
| `status_update_checked_test.rs` | `repository/status_update_checked.rs` |
| `version_concurrency_test.rs` | `repository/version_concurrency.rs` |
| `execution_retention_test.rs` | `repository/execution_retention.rs` |
| `f2_event_artifact_retention_test.rs` | `repository/event_artifact_retention.rs` |

**`tests/migrations.rs`** — the schema side, the four suggested files under
`tests/migrations/`:

| Old file | New module |
|---|---|
| `orch_migrations_test.rs` | `migrations/orch_migrations.rs` |
| `orch_metrics_test.rs` | `migrations/orch_metrics.rs` |
| `item_source_migration_test.rs` | `migrations/item_source_migration.rs` |
| `g1_stale_reconcile_test.rs` | `migrations/stale_reconcile.rs` |

Followed the suggested split as given rather than moving `stale_reconcile`
into `repository.rs` despite it exercising `Repository::reconcile_stale_orch_*`
rather than a schema migration — its own doc comment frames it as closing a
gap in what keeps `orch_tasks`/`orch_approvals` in sync with a reachable
control plane, which reads closer to "schema/reconciliation lifecycle" than
to the CRUD/concurrency tests in `repository.rs`. Not strong enough a
disagreement to override the card's explicit grouping.

Only `f2_event_artifact_retention_test.rs` and `g1_stale_reconcile_test.rs`
carried board-card prefixes; both are renamed to their subject
(`event_artifact_retention`, `stale_reconcile`). The other ten files already
carried their own subject name, so only the `_test` suffix was dropped —
consistent with the `runner_contract.rs` submodule names this pattern is
copied from (`domain.rs`, `fakes.rs`, `fixtures.rs`, …), none of which carry a
`_test` suffix either.

## `execution_repo_test.rs` — not split

Checked for existing seams (`grep -n '^mod \|^// ===\|^// ---'`) before
touching it: no `mod` blocks, no comment-banner sections — just 61 flat
`#[tokio::test]` functions, unlike `integration_test.rs` (which does have
`// ─── X Tests ───` banners for every entity) or
`f2_event_artifact_retention_test.rs` (three `// ---` banners). Its own test
names cluster into obvious topics by prefix (`claim_*`, `cancellation_*`,
`completion_*`, `concurrent_*`, `enqueue_*`, `heartbeat_*`, `recovery_*`, …),
but the card is explicit that only seams the file *already has* license a
split, not ones invented from naming conventions. Moved whole, as
`repository/execution_repo.rs` (4,339 lines, unchanged).

## The `use crate::common` pattern

Every file compiled `mod common;` (→ `tests/common/mod.rs`) into its own
binary before. Each moved file now reaches the shared harness via
`use crate::common::{...}`, since `common` is declared once at the new
binary's root (`repository.rs`/`migrations.rs`). Where a file also calls
`common::some_fn(...)` in fully-qualified form somewhere in its body (rather
than only the destructured names), the import is `use crate::common::{self,
some_fn, ...};` — that keeps the module path itself in scope so the
qualified call sites don't need editing, since those are test-body text.

## `tests/common/mod.rs` — changed, with reason

Touched `create_test_workspace`'s and `make_project`'s/`make_item`'s
`#[allow(dead_code)]` and the comment explaining them, because the *reason*
those attributes exist changed under regrouping, not because the functions
did:

- Before: 12 binaries, one file's disuse of a helper made it dead code in
  *that* file's copy of `common` while live in every other file's copy. The
  comment named `orch_metrics_test.rs` as the one binary that never creates a
  workspace.
- After: only 3 binaries share `common`. Verified directly — temporarily
  stripped every `#[allow(dead_code)]` from `common/mod.rs` and ran
  `cargo clippy -p tack-db --all-targets -- -D warnings`: `create_test_workspace`
  produced no warning anywhere (every binary, including `perf_test`, calls
  it), while `make_project` and `make_item` both failed to compile
  `perf_test` with "function ... is never used" — `perf_test.rs` seeds its
  50k rows with a raw bulk `INSERT` and never builds a `Project`/`Item`
  through the shared helpers. Removed the now-unneeded attribute from
  `create_test_workspace`; kept it on `make_project`/`make_item` with the
  comment updated to name `perf_test` (not `orch_metrics.rs`) as the reason.

This is the one change to `tests/common/mod.rs` beyond incidental
`cargo fmt` reordering; no signature changed.

## Cross-reference comments fixed (in scope only)

Filenames that moved are cited by comment in several other files. Fixed the
ones inside `crates/tack-db/tests/` (my own scope): `event_artifact_retention.rs`
(3 references to `execution_retention_test.rs`/`execution_repo_test.rs`),
`execution_retention.rs` (1 to `execution_repo_test.rs`), `orch_repo.rs` (1 to
`orch_metrics_test.rs`), `orch_migrations.rs` (3, to `integration_test.rs`
and `orch_metrics_test.rs` twice), and `common/mod.rs` (1, see above).

**Found but not fixed — outside scope.** The same filenames are cited by
comment in `crates/tack-db/src/repo/execution.rs`, `crates/tack-db/src/repo/items.rs`
(both `src/`, not `tests/`), `crates/tack-api/src/handlers/orch.rs`,
`crates/tack-api/src/handlers/runner_protocol/retention.rs`,
`crates/tack-api/src/execution_runtime.rs`, several `crates/tack-api/tests/*`
and `crates/tack-orch/tests/*` files, and a number of `docs/agent-handoffs/`
entries and `docs/adr/0060-*`/`docs/book/src/*`. The card's scope is
`crates/tack-db/tests/` only — four other agents are working in other crates
concurrently — so none of those were touched. The `docs/agent-handoffs/`
and `docs/adr` ones are historical record and shouldn't be rewritten anyway;
the `src/` and `tack-api`/`tack-orch` ones are now stale pointers to
`execution_repo_test.rs`, `execution_retention_test.rs`, `orch_metrics_test.rs`,
`integration_test.rs`, and `f2_event_artifact_retention_test.rs`, which no
longer exist under those names. Whoever owns those crates should fix them
(a plain rename, same shape as the ones done here) — full list is the
`grep -rn` this handoff was written from:

```
grep -rn "execution_repo_test\.rs\|f2_event_artifact_retention_test\.rs\|g1_stale_reconcile_test\.rs\|orch_migrations_test\.rs\|orch_metrics_test\.rs\|item_source_migration_test\.rs\|integration_test\.rs\|orch_repo_test\.rs\|status_update_checked_test\.rs\|version_concurrency_test\.rs\|execution_retention_test\.rs\|perf_test\.rs" crates/ docs/
```

## Before / after

```
cargo nextest list --workspace -E 'package(tack-db)' | wc -l
```

- Before (branched from `develop` at `db113b8`): **199**
- After: **199**

File count: 12 files in `crates/tack-db/tests/` (12 independent binaries) →
3 top-level binaries (`repository.rs`, `migrations.rs`, `perf_test.rs`
unchanged) + 11 of the original files preserved as submodules under
`tests/repository/` and `tests/migrations/` (renamed per the table above).
Zero files deleted; every test that existed still exists, under the same
name, in the binary its group implies.

## Verification run

```
./scripts/check-comments.sh                                                  # pass
cargo fmt --all --check                                                      # pass
cargo clippy --workspace --all-targets -- -D warnings                        # pass, 0 warnings
cargo nextest run --workspace                                                # 1392 passed, 6 skipped, 0 failed
cargo nextest list --workspace -E 'package(tack-db)' | wc -l                  # 199
cargo nextest run --workspace --run-ignored ignored-only -E 'test(list_items_p95)'  # 1 passed
```

## Note on `nextest list`'s plain-text output and `perf_test`

`cargo nextest list --workspace -E 'package(tack-db)'` (plain text) prints no
line at all for `list_items_p95_under_100ms_at_50k` — before or after this
change (verified against the pre-change tree too, same command). The JSON
form (`--message-format json`) does report it: `tack-db::perf_test` with 1
testcase. This is the same "1,399 vs 1,392 + 6 skipped, unreconciled"
discrepancy ADR 0064 already flags for `cargo test` vs `nextest`, not
something this regrouping introduced — the 199 baseline and the 199 result
were produced by the identical command on both sides of the change, and the
perf test's own dedicated run (last line above) proves it still executes.

## File-backed vs. in-memory DB — untouched

Per CLAUDE.md's standing rule, every test that builds its own file-backed
pool (`init_pool("sqlite://{path}?mode=rwc")` — the four sites in
`execution_repo.rs`, `execution_retention.rs`, `event_artifact_retention.rs`,
and `orch_migrations.rs`) was moved character-for-character; none was
switched to the shared in-memory `common::setup_test_db()` harness. Verified
by content diff, not just visual inspection: `git show
db113b8:crates/tack-db/tests/execution_repo_test.rs` (etc.) `diff`'d against
each new file's post-`git mv` content before any edit was applied — the only
difference at that point was line 1 (`mod common;` vs. absent) in every case.
