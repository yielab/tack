# ADR 0087: the protocol is a crate, and the SQL has one home

**Status: PROPOSED 2026-10-20.** Executed, once accepted, by Phase 79 (`docs/plans/phase-79.md`).

**Decide:** that the runner-v1 contract becomes its own crate, `tack-protocol`, with no
dependency on anything else in the workspace, so the server and the runner share it and nothing
else; that every `sqlx` line of the server lives in `tack-db`, behind a `StoreError`, and the
state of an attempt is one Rust enum wherever it is read or written; that the rules of a run move
out of HTTP handlers into a `tack-app` crate of use cases that take ports, and the background
ticks are functions the binary schedules; and that a script, run by the pre-push hook, holds the
table of which crate may depend on which, so the tree cannot drift from this decision in silence.

**Why now:** an audit on 2026-10-20 measured the server. `tack-runner` — the process that holds
the operator's credentials and must stay small — compiles SQLite, because the types it shares
with the server sit in `tack-orch`, which depends on `tack-db`. The attempt's state machine
exists three times (typed in `tack-orch`, as string literals in `tack-db`, and called ad hoc from
handlers) for the same reason: `tack-db` cannot import the typed one. About 190 SQL queries sit in
`tack-api` beside the repository that exists to hold them, and `sqlx::Error` reaches the HTTP
layer. Eighteen types are kept as hand-written mirrors between crates because one crate cannot
derive the schema the other needs. The documentation says `tack-orch` "depends inward", and the
graph says the opposite.

**If you do nothing:** every change to a column is a search through two crates; every new
attempt state is written in three places and tested in one; the runner's binary and attack surface
carry a database it never opens; and the next person to add a handler copies the nearest one,
which has SQL in it.

## Decisions

| # | Decision | What a contributor sees |
|---|---|---|
| 1 | **The contract is a crate.** `tack-protocol` holds the runner-v1 types, the lifecycle, capabilities, usage provenance, the merge-readiness pack, the prepare step and one `Clock`. It depends on nothing in the workspace. `utoipa` is behind a feature, as in `tack-core`, so no crate needs a mirror type to document it | `use tack_protocol::execution::ExecutionState` from the runner and the server alike; the runner's `cargo tree` has no `sqlx` |
| 2 | **The SQL has one home.** `sqlx` is named only in `tack-db` (and `tack-test-support`), enforced by `cargo deny`. `tack-db` is organised by aggregate, with read models in `queries/`. Errors cross out as `StoreError { NotFound, Conflict, Sql }`. A handler calls a store method or a query function, never a query string | `grep sqlx crates/tack-api` finds nothing; a 404 is `StoreError::NotFound` mapped once |
| 3 | **One state enum.** The store converts at its edge; no literal state string exists outside the enum's own `FromStr`/`as_str` | A new state is added in one file and the compiler lists every `match` that must learn it |
| 4 | **Rules in use cases, ticks as functions.** `tack-app` holds `StartRun`, `ClaimAttempt`, `ReportEvents`, `Decide`, `Complete` and the ticks (`merge`, `auto_start`, `github_sync`) as `tick()`; they take ports (`RunStore`, `RunnerStore`, `Clock`). `tack-api` parses, calls, maps. `tack serve` composes and spawns | A rule is tested against an in-memory fake in milliseconds; a handler test asserts a status code and nothing more |
| 5 | **The graph is checked.** `scripts/check-crate-graph.sh` holds the allowed edges and runs in `.githooks/pre-push`. A new edge is a change to that table, reviewed like code | A forbidden `path` dependency fails the push with the edge's name |
| 6 | **No more crates than these.** Nine: the seven of today plus `tack-protocol` and `tack-app`. No client crate, no integrations crate, no runner split, no `.sql` migration files. Each would move files without changing a dependency set, a trust boundary or a direction | The crate list in `ARCHITECTURE.md` fits on one screen |

## Evidence

Measured on `develop` at `9c046fba`, 2026-10-20; the commands are in `docs/plans/phase-79.md` §1.

- `cargo tree -p tack-runner -e normal` lists `sqlx`, `sqlx-sqlite`, `libsqlite3-sys` and
  `tack-db`.
- `crates/tack-orch/src/execution/*.rs` and `usage_provenance.rs` import only `std`, `serde`,
  `chrono` and each other: the move is a `git mv`.
- 190 `sqlx` sites in `crates/tack-api/src` (78 `SELECT`); 258 in `crates/tack-api/tests`.
- 20 attempt-state string literals outside `tack-orch`; `mirror_request_state` copies the state
  into a second table.
- Three `Clock` traits with the identical signature `fn now(&self) -> DateTime<Utc>`.
- 18 structs named `*Schema`, `*Row`, `*Outcome`, `*Snapshot` whose doc comment says "mirror".
- 1 453 tests; 122 placed by `#[path]` next to their module, so they move with it.

## Consequences

- Internal paths change (`tack_orch::execution::*` → `tack_protocol::execution::*`); the JSON
  on the wire, `docs/openapi.json`'s shapes and `docs/contracts/runner-v1/*` do not. A deployed
  runner keeps working against a server from either side of the phase.
- The book's Developer Guide and `ARCHITECTURE.md` are rewritten in the phase's last PR; ADRs
  that cite old paths are history and stay as written.
- Six model agents in all; the rest is scripts. The plan says which is which, per step.
