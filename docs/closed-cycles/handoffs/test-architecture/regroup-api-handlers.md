# Regroup tack-api's handler and security test binaries

ADR 0064 rollout step 4, `tack-api` slice. Base: `develop` at `db113b8`. Branch:
`agent/regroup-api-handlers`.

## What moved

**`tests/handlers.rs` + `tests/handlers/`** (7 submodules, from 7 files):

| New file | From | Subject |
|---|---|---|
| `handlers/crud.rs` | `api_test.rs` | CRUD/lifecycle across every operator handler (health, token gate, body limits, validation, vocabulary/workflow, backup/restore, embedded SPA, custom fields, board filter, item update/delete, sprints, roles, comments, dependencies, search, export, provenance, GitHub push sync) |
| `handlers/executions_runner_admin.rs` | `c1_handlers_test.rs` | `handlers/executions.rs` + `handlers/runner_admin.rs`, loaded via `#[path]` directly (whitebox, not through the global router) |
| `handlers/production_router.rs` | `c5_integration_test.rs` | Same lifecycle through the real, fully wired production router |
| `handlers/operator_read_routes.rs` | `e6_routes_test.rs` | `GET /api/runners`, `GET /api/executions/{id}/attempts(+events)` |
| `handlers/economics.rs` | `economics_test.rs` | `GET /api/economics/summary`, `GET /api/economics/items` |
| `handlers/item_concurrency.rs` | `item_concurrency_test.rs` | Optimistic concurrency (`ETag`/`If-Match`) on items |
| `handlers/provisioning.rs` | `provisioning_test.rs` | `POST /api/templates/{id}/provision` and its rollback behavior |

**`tests/security.rs` + `tests/security/`** (5 submodules, from 5 files):

| New file | From | Subject |
|---|---|---|
| `security/trust_boundary.rs` | `trust_boundary_test.rs` | Bearer-token boundary: path lookalikes, CSP, WebSocket handshake auth |
| `security/cors.rs` | `cors_test.rs` | CORS preflight coverage |
| `security/chaos_recovery.rs` | `g2_chaos_security_test.rs` | Chaos/fencing/security/recovery adversarial suite |
| `security/board_drag_wip_race.rs` | `board_drag_wip_race_test.rs` | WIP-limit race on the ordinary board-drag PATCH path |
| `security/wip_limit_race.rs` | `wip_limit_race_test.rs` | WIP-limit race on the sprint-dispatch path |

Files renamed by subject per the card's instruction — `c1_`, `c5_`, `e6_`, `g2_` board-card
prefixes are gone from every filename and from every doc-comment cross-reference between
two files this card touched (`c1_handlers_test.rs` ↔ `c5_integration_test.rs`,
`board_drag_wip_race_test.rs` ↔ `wip_limit_race_test.rs`). Cross-references to files
**outside** this card's scope (`c2_handlers_test.rs`, `wave2_gate.rs`,
`f6a_artifact_wiring_test.rs`, `orch_dispatch_test.rs`, `sprint_dispatch_test.rs`) were left
as-is — those files are owned by the other two concurrent regrouping agents and I don't
know their eventual names.

Two files outside this card's scope still reference the old filenames and will read stale
until whoever regroups them updates the cross-reference: `auto_dispatch_test.rs:216`
("the same pattern `api_test.rs`'s GitHub...") and `f6b_model_wiring_test.rs:29` ("same
shape as `wave2_gate.rs`/`e6_routes_test.rs`"). Not fixed here — touching those files was
out of scope.

## Before/after counts

```
$ cargo nextest list --workspace -E 'package(tack-api)' | wc -l
469   # before (db113b8) and after (this branch) — identical
```

```
$ ls crates/tack-api/tests/*.rs | wc -l
36   # before (git show db113b8:crates/tack-api/tests | grep '\.rs$' | wc -l)
26   # after — dropped by 10 (removed 12 top-level files, added 2: handlers.rs, security.rs)
```

```
$ cargo nextest run --workspace
Summary [ ~19s ] 1392 tests run: 1392 passed, 6 skipped
```

63 binaries after (was 65 at `db113b8`; two binaries — `handlers`, `security` — replaced
twelve).

## Private `fn setup` — replaced vs. kept

Two files (`c1_handlers_test.rs`/`executions_runner_admin.rs`,
`c5_integration_test.rs`/`production_router.rs`) declared `mod common;` but never called
anything from it — dead declarations, presumably copy-pasted boilerplate. Removed (module
declaration only; nothing that ran changed).

**Replaced** with `use crate::common;` (qualified call sites kept, e.g. `common::test_app()`)
or `use crate::common::test_app;` (unqualified call sites kept) — no behavior change, just
where the module comes from now that the crate root, not each file, owns `mod common;`:

- `crud.rs` (was `api_test.rs`) — `common::test_app`, `test_app_with_config`,
  `test_app_with_file_db`, all already used.
- `economics.rs` — `common::test_app` (one call site, the orch-disabled-by-default case;
  every other test in the file uses its own `app_with_state`, kept — see below).
- `security/trust_boundary.rs`, `security/cors.rs` — same swap.

**Kept, not forced onto `common::`** — each differs in a way that matters:

- `executions_runner_admin.rs::setup()` — builds a router from `executions::routes(...)
  .merge(runner_admin::routes(...))` directly (whitebox: the two source files are loaded
  as inline modules via `#[path]`), not the full `build_router`, and seeds a runner +
  agent profile the shared helper doesn't. Not equivalent to any `common::` function.
- `production_router.rs::setup(config)` / `app_state(...)` — returns
  `(Router, Repository, SqlitePool, Uuid, String)` and keeps the raw pool so tests can
  simulate a restart by rebuilding a fresh router/`AppState` around the *same* pool
  (`common::test_app*` returns only `(Router, Uuid)` — no pool handle). The file's own
  comment already documents why.
- `operator_read_routes.rs::setup()` — builds the full production router but also returns
  `Repository` for direct-DB fixture setup and uses its own `OPERATOR_TOKEN`; `common::`
  has no variant that returns a `Repository` alongside the router.
- `economics.rs::app_with_state`, `provisioning.rs::app_with_state`,
  `item_concurrency.rs::app_with_state`, `security/board_drag_wip_race.rs::app_with_state`,
  `security/wip_limit_race.rs::app_with_state` — all return `(Router, AppState)`, not
  `(Router, Uuid)`, specifically so tests can assert against `state.repo` directly (the
  house rule for "writes nothing / rejects before X" claims: assert the absence directly,
  not just a status code). `item_concurrency.rs`'s own comment says it mirrors
  `board_drag_wip_race.rs`'s helper on purpose — five near-identical copies, kept, because
  none of them is reducible to what `common::` currently exposes.
- `security/chaos_recovery.rs` — `app_in_memory`/`app_file_backed`/`build_app` take a
  `storage_dir` and a `database_url` string `common::` doesn't plumb through, and the
  file's own doc comment says the self-contained shape is deliberate (mirrors
  `wave2_gate.rs`/`f6a_artifact_wiring_test.rs`, both out of scope for this card).

**Finding, not fixed:** `AppState`-returning `app_with_state`/`build_app` helpers are
duplicated across five files now living in two different binaries. A `common::` helper
that returns `(Router, AppState)` (or exposes `.repo` some other way) would let all five
collapse onto the shared file — but `tests/common/mod.rs` was explicitly off-limits this
card ("stop and report it instead"). Flagging for whoever owns that file next.

## The file-backed vs. in-memory question

Checked deliberately, per the card's instruction: **neither `board_drag_wip_race.rs` nor
`wip_limit_race.rs` builds a file-backed pool.** Both use `init_pool("sqlite::memory:")`
via their own local `app_with_state`, same as every other in-memory test in this crate.
This is not a weaker proof than a file-backed DB: `tack_db::init_pool` always requests
`max_connections(5)`, and sqlx parses `"sqlite::memory:"` with `shared_cache = true` and a
pool-scoped unique in-memory filename (`sqlx-sqlite`'s `options/parse.rs`), so the pool's
five connections share one real, contended, shared-cache database — not five independent
empty ones. `g2_chaos_security_test.rs`/`chaos_recovery.rs` is the file in this crate that
*does* need genuine file-backed connections (two of its tests, for reasons its own doc
comment gives), and it already builds one locally; that file, and its file-backed helper,
were moved verbatim with no change to either helper. Nothing was normalized onto the
shared `common::` helper for any of the three race-adjacent files.

## Race-test tally

`cargo nextest run --workspace -E 'test(/wip_limit|board_drag/)'`, ten consecutive runs
(4 tests selected each time: both race tests × 2 test fns matching each pattern):

```
run 1:  4 passed, 1394 skipped
run 2:  4 passed, 1394 skipped
run 3:  4 passed, 1394 skipped
run 4:  4 passed, 1394 skipped
run 5:  4 passed, 1394 skipped
run 6:  4 passed, 1394 skipped
run 7:  4 passed, 1394 skipped
run 8:  4 passed, 1394 skipped
run 9:  4 passed, 1394 skipped
run 10: 4 passed, 1394 skipped
```

40/40 passed, 0 failures.

## Gate

```
./scripts/check-comments.sh                                    # clean
cargo fmt --all --check                                        # clean (after one `cargo fmt --all`
                                                                 #  to alphabetize the new mod declarations)
cargo clippy --workspace --all-targets -- -D warnings           # clean
cargo nextest run --workspace                                   # 1392 passed, 6 skipped
cargo nextest list --workspace -E 'package(tack-api)' | wc -l   # 469
ls crates/tack-api/tests/*.rs | wc -l                            # 26 (was 36)
```

## Not touched

`tests/common/mod.rs`, `.config/nextest.toml`, `.github/`, `Cargo.toml`, and every
`tests/*.rs` file outside the 12 named in scope (the other two concurrent regrouping
agents own those).

## Context spent

Read the ADR, all 12 target files in full, `tests/common/mod.rs`, the two existing
`#[path]`-grouped precedents (`runner_vertical_slice.rs`, `runner_orch::runner_contract.rs`),
and the sqlx-sqlite `options/parse.rs` source to verify the in-memory shared-cache claim
above rather than assume it.
