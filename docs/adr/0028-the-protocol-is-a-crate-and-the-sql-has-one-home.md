# ADR 0028: the protocol is a crate, and the SQL has one home

**Status: ACCEPTED 2026-10-26 by the maintainer. Proposed 2026-10-20, revised 2026-10-25 after an
audit of the first draft, built 2026-10-26 (the build ran on the maintainer's go-ahead; the
acceptance was recorded the same day, after it).** Executed by Phase 79
(`docs/plans/phase-79.md`, §8 lists what the execution found, including the one `Host` trait
through which `tack-app` calls back into `tack-api`).

**Decide:** that the runner-v1 contract becomes its own crate, `tack-protocol`, with no
dependency on anything else in the workspace, so the server and the runner share it and nothing
else; that every `sqlx` line of the server lives in `tack-db`, behind a `StoreError`, and the
state of an attempt is one Rust enum wherever it is read or written; that the rules of a run move
out of HTTP handlers into a `tack-app` crate of use cases that take the store and a clock, and
every background loop is a function the binary schedules; and that a script, run by the pre-push
hook, holds the table of which crate may depend on which, so the tree cannot drift from this
decision in silence.

**Why now:** an audit on 2026-10-20 measured the server. `tack-runner` — the process that holds
the operator's credentials and must stay small — compiles SQLite, because the types it shares
with the server sit in `tack-orch`, which depends on `tack-db`. The attempt's state machine
exists three times (typed in `tack-orch`, as string literals in `tack-db`, and called ad hoc from
handlers) for the same reason: `tack-db` cannot import the typed one. About 200 SQL queries sit in
`tack-api` beside the repository that exists to hold them, and `sqlx::Error` reaches the HTTP
layer. Eight types are kept as hand-written mirrors because one crate cannot derive the schema
the other needs. The documentation says `tack-orch` "depends inward", and the graph says the
opposite.

**If you do nothing:** every change to a column is a search through two crates; every new
attempt state is written in three places and tested in one; the runner's binary and attack surface
carry a database it never opens; and the next person to add a handler copies the nearest one,
which has SQL in it.

## Decisions

| # | Decision | What a contributor sees |
|---|---|---|
| 1 | **The contract is a crate.** `tack-protocol` holds the runner-v1 types, the lifecycle, capabilities, usage provenance, the merge-readiness pack, the prepare step and one `Clock`. It depends on nothing in the workspace. `utoipa` is behind a feature, as in `tack-core`, so no crate needs a mirror type to document it | `use tack_protocol::execution::ExecutionState` from the runner and the server alike; the runner's `cargo tree` has no `sqlx` |
| 2 | **The SQL has one home.** `sqlx` is named only in `tack-db` (and `tack-test-support`), enforced by `cargo deny`. `tack-db` keeps its per-aggregate modules, splits `repo/execution.rs` by lifecycle stage, and holds read models in `queries/`. Errors cross out as `StoreError { NotFound, Conflict, Sql }`. A handler calls a store method or a query function, never a query string and never a pool | `grep sqlx crates/tack-api crates/tack-orch` finds nothing; a 404 is `StoreError::NotFound` mapped once |
| 3 | **One state enum.** The store converts at its edge; no literal state string exists outside the enum's own `FromStr`/`as_str` and the one store file that calls them | A new state is added in one file and the compiler lists every `match` that must learn it |
| 4 | **Rules in use cases, loops as functions.** `tack-app` holds the runs use cases and every tick (`merge`, `auto_start`, `github_sync`, `retention`, `observability`) as `tick()`; a use case takes `&Repository`, `&dyn Clock` and plain values and returns `AppError`. There are no store traits and no in-memory fake: the one store is SQLite, every test already runs it in memory in milliseconds, and a fake would hide the transactions and fencing the store exists for. `tack-api` parses, calls, maps. `tack serve` composes and spawns | A rule is tested in `tack-app` against `setup_test_db()` with no HTTP; a handler test asserts a status code and nothing more |
| 5 | **The graph is checked.** `scripts/check-crate-graph.sh` holds the allowed normal edges (dev-dependencies on `tack-test-support` are exempt) and runs in `.githooks/pre-push`. A new edge is a change to that table, reviewed like code | A forbidden `path` dependency fails the push with the edge's name |
| 6 | **No more crates than these, and `tack-orch` becomes pure.** Nine workspace members: the seven of today plus `tack-protocol` and `tack-app` (`tack-desktop` stays outside the workspace). `tack-orch` keeps the pure scheduler and model policy; its wiring and its two ticks move to `tack-app`, so it no longer depends on `tack-db`. No client crate, no integrations crate, no runner split, no `.sql` migration files | The crate list in `ARCHITECTURE.md` fits on one screen, and every crate is one layer |

## Evidence

Measured on `develop` at `9c046fba` (2026-10-20) and re-measured on `d130d103` (2026-10-25); the
commands are in `docs/plans/phase-79.md` §1.

- `cargo tree -p tack-runner -e normal` lists `sqlx`, `sqlx-sqlite`, `libsqlite3-sys` and
  `tack-db`.
- The modules that move to `tack-protocol` import only `std`, `serde`, `chrono` and each other:
  the move is a `git mv`.
- 197 `sqlx` sites in `crates/tack-api/src`; 31 test files under `crates/tack-api/tests` name
  `sqlx::`; `crates/tack-orch/src` names it in two `wiring.rs` files.
- 20 attempt-state string literals outside `tack-protocol`, some of them a second enum,
  `RecoveryDisposition`; `mirror_request_state` copies the state into a second table.
- Three `Clock` traits with the identical signature `fn now(&self) -> DateTime<Utc>`.
- 8 structs named `*Schema` in `tack-api` whose doc comment says "mirror".
- Every `tack-db` and `tack-api` test runs on `sqlite::memory:`; the slowest test binary finishes
  in about 6 s (`.config/nextest.toml`).
- 13 `tokio::spawn` sites in `tack-api`, 4 of them interval loops.
- 1 453 tests; 122 placed by `#[path]` next to their module, so they move with it.

## Consequences

- Internal paths change (`tack_orch::execution::*` → `tack_protocol::execution::*`); the JSON
  on the wire, `docs/openapi.json`'s shapes and `docs/contracts/runner-v1/*` do not. A deployed
  runner keeps working against a server from either side of the phase.
- The book's Developer Guide and `ARCHITECTURE.md` are rewritten in the phase's last PR; ADRs
  that cite old paths are history and stay as written.
- Seven scripts and twenty-four model tasks, none with more than ~1 400 lines of source in
  scope, each with a list a script made and a command that says when it is done. The plan says
  which is which, per task.
- A port over the store is not written in this phase. The day a second store exists, the use
  cases' `&Repository` parameter becomes a trait, and that is a small ADR of its own.
