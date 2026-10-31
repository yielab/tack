# Phase 79: the crates say what depends on what, and the SQL lives in one place

**Executes:** [ADR 0028](../../adr/0028-the-protocol-is-a-crate-and-the-sql-has-one-home.md)
(accepted 2026-10-26; proposed 2026-10-20, revised 2026-10-25). **Built 2026-10-26**; merged into
`develop` with Phases 80 to 82 on 2026-10-31 (PR #66).

**Done when:** `tack-runner` compiles without `sqlx` or `libsqlite3-sys` in its dependency tree;
no file under `crates/tack-api/src`, `crates/tack-orch/src` or `crates/tack-app/src` names
`sqlx`; the state of an attempt is one Rust enum used by every crate; no `loop` over a
`tokio::time::interval` lives in `tack-api`; `scripts/check-crate-graph.sh` passes and runs from
`.githooks/pre-push`; the full suite passes once, on the merged tree, with no test duplicated
across old and new locations.

**Status:** built (2026-10-26), every task of §5 committed; the full suite
(1 505 tests) and `.githooks/pre-push` pass on the merged tree. What the execution found is in §8.
What the audit changed is in §7; the two companion guides in Spanish
([`phase-79-guia.md`](phase-79-guia.md), [`phase-79-desde-cero.md`](phase-79-desde-cero.md))
still describe the first draft's ports and fakes and are teaching material, not instructions.

This plan is written for whoever executes it, person or agent. §4 is the work in order; §5 is
the task list an agent is handed, one row at a time. An agent is handed its row of §5 and the
recipe section it names, never this whole file.

## 1. What was measured (2026-10-25, `phase-79-architecture` = `develop` at `d130d103` + docs)

Every number below came from a command; re-run it before trusting it.

| Fact | Number | Command |
|---|---|---|
| `sqlx` query sites in `tack-api/src` | 197 (by file in §5, task group 2) | `grep -rcE 'sqlx::query\|query_as\|query_scalar\|\.fetch_(one\|all\|optional)' crates/tack-api/src --include='*.rs' \| grep -v ':0'` |
| `sqlx` in `tack-orch/src` | 2 files: `scheduler/wiring.rs`, `model_policy/wiring.rs` (4 `sqlx::Error` returns) | `grep -rln 'sqlx' crates/tack-orch/src` |
| `sqlx::` in `tack-api/tests` | 31 files; the biggest `handlers/executions_runner_admin.rs` (70) | `grep -rc 'sqlx::' crates/tack-api/tests --include='*.rs' \| grep -v ':0' \| sort -t: -k2 -rn` |
| Attempt-state literals outside tests, db + api | 20; some belong to `RecoveryDisposition`, a second enum, not to `ExecutionState` | `grep -rnE '"(leased\|preparing\|waiting_decision\|needs_operator)"' crates/tack-db/src crates/tack-api/src --include='*.rs' \| grep -v tests` |
| `ExecutionState` has `as_str`/`FromStr` | no; serde `snake_case` only | `grep -n 'FromStr\|fn as_str' crates/tack-orch/src/execution/types.rs` |
| `tack-runner` pulls `tack-db`, `sqlx`, `libsqlite3-sys` | yes, via `tack-orch` | `cargo tree -p tack-runner -e normal --prefix none \| grep -E '^(sqlx\|libsqlite3-sys\|tack-db) '` |
| Modules moving to `tack-protocol` import I/O or `tack_core` | no; one line, `execution/types.rs:283`, names `tack_core::prepare`, which moves too | `grep -n 'tack_core\|tack_db\|sqlx' crates/tack-orch/src/execution/*.rs crates/tack-orch/src/usage_provenance.rs crates/tack-core/src/{mrp,prepare}.rs` |
| Root re-exports in `tack-orch/src/lib.rs` | none; every use is `tack_orch::<module>::` | `grep -n '^pub use' crates/tack-orch/src/lib.rs` |
| Equivalent `Clock` traits (`fn now(&self) -> DateTime<Utc>`) | 3: `ExecutionClock` (db), `RetentionClock`, `ObservabilityClock` (orch); the runner's uses `SystemTime` and stays | `grep -rn 'trait .*Clock' crates --include='*.rs'` |
| Hand-kept doc mirrors in `tack-api` | 5 `*Schema` types in `handlers/executions.rs` (removed in 1.1) and 3 in `openapi.rs` that mirror `ResolveDecisionResponse`, whose `answer`/`resolved_by` are untyped `Value` (they go with D6); the earlier "18" counted real `*Snapshot`/`*Outcome` types | `grep -rhoE 'pub (struct\|enum) \w+Schema\b' crates/tack-api/src \| sort -u` |
| `Repository` methods | 27 forwarding wrappers in `repo.rs`; 188 `pub async fn` in `repo/*.rs`, 18 of them free functions taking a pool | `grep -c 'pub async fn' crates/tack-db/src/repo.rs crates/tack-db/src/repo/*.rs` |
| `repo/execution.rs` | 4 087 lines, 57 methods, 28 transactions | `wc -l`, `grep -c '    pub async fn'`, `grep -c 'BEGIN IMMEDIATE\|\.begin()'` |
| `tokio::spawn` in `tack-api/src` | 13; **4 are ticks** (`server.rs`: github poll, auto-start, backup, hourly sweep); the rest are websocket send/recv, fire-and-forget in `comments`/`items`, `execution_runtime`, `prill_store`, `webhook` | `grep -rn 'tokio::spawn' crates/tack-api/src --include='*.rs' \| grep -v tests` |
| Handlers longer than 25 lines | `runner_protocol.rs` 18 of 19, `executions.rs` 14 of 14, `decisions.rs` 3 of 4, `local_runner.rs` 7 of 9 | awk over `pub async fn` boundaries |
| Path edges `cargo metadata` reports | 12 normal + 6 `dev` (every crate → `tack-test-support`) | `cargo metadata --format-version 1 --no-deps \| jq -r '.packages[] \| .name as $n \| .dependencies[] \| select(.path != null) \| [$n, .name, (.kind // "normal")] \| @tsv'` |
| Test speed | the slowest binary in the suite finishes in ~6 s; every db and api test runs on `sqlite::memory:` with all migrations | `.config/nextest.toml`, `docs/TESTING.md` §layers |
| Frontend: features importing `shared/runWithAgent`, `shared/execution`, `shared/agents` | 7 features (agents 13 files, settings 4, item-detail 2, board, list, sprints, table) | `grep -rlE "shared/(runWithAgent\|execution\|agents)" frontend/src/features \| cut -d/ -f3 \| sort \| uniq -c` |
| Frontend: `shared/` files importing `features/` | 0 real imports; 3 files (`state/itemEvents.ts`, `state/executionContext.tsx`, `agents/FirstRunBanner.tsx`) only named `features/` in comments, which the grep also counts | `grep -rl "features/" frontend/src/shared --include='*.ts*'` |
| `cli/src/main.rs`, `runner/src/engine.rs` | 3 054 and 2 611 lines | `wc -l` |
| `decision-token` at the repo root | untracked and not ignored | `git check-ignore decision-token` (exits 1) |
| `tack.db*`, `tack.toml`, `logs/`, `storage/` | all already ignored | `git check-ignore -v tack.db tack.toml logs storage` |

## 2. Rules every task follows

### Script or model

A step goes to a **script** when `cargo check` or `npm test` can tell whether it is right: moving
files, renaming paths, listing sites. A step goes to a **model** when someone has to choose a
name, a grouping or a transaction boundary. Every model task gets: a list of `file:line` made by
a script, the one recipe section it needs, and a "done when" that is a command.

### The size of a task

One agent, one task of §5, and never more than **~800 lines of source in scope** plus the tests
those lines reach. A handler file above that is split into two tasks by handler name, and each
task's list names only its handlers' lines. A file is visited once: the agent that takes a
file out of `sqlx` also extracts its use cases or gives its loop a `tick()` in the same run,
because it has already read the file (§5). An agent that finds itself reading a file not on its
list has the wrong list: it stops and reports, it does not widen. A task is finished when its
"done when" command passes; the agent never runs the whole suite and never runs
`.githooks/pre-push`.

### Conventions first, then the fan-out

The fan-out starts with two **conventions tasks** (§5: `C1` for the store, `C2` for the
application crate): they write the shared types and one worked example of each kind the recipes
name. Every later task
copies that example. No two agents invent the same thing.

### Tests: nothing coexists

1. A test moves with its code, in the same commit: `git mv foo.rs` and `git mv foo/tests.rs`
   together; the `#[path = "foo/tests.rs"]` line follows the file.
2. When a new test covers a behaviour, the old test that covered it is deleted in the same task,
   and the task's commit message names both.
3. Every test keeps running on `sqlite::memory:` through `tack_test_support::setup_test_db()`.
   There is no in-memory fake of the store and no contract suite between two stores: the one
   store is SQLite, and its tests already take milliseconds (§1).
4. While working, run the test binaries the change reaches, by name:
   `cargo nextest run --workspace -E 'binary(<name>)'`. `rdeps(tack-db)` is nearly the whole
   suite and is not a working filterset. The full suite runs once per phase, on the merged tree.
5. Never `cargo test`; never `-p` (see `.config/nextest.toml`).

### The graph is checked, not described

From phase 0 on, `scripts/check-crate-graph.sh` holds a table of allowed `path` dependencies
among **normal** dependencies and fails on any other edge; `dev-dependencies` on
`tack-test-support` are exempt by the script, not by the table. Each phase tightens the table in
the same PR that makes the tightening true. `scripts/gen-deny-toml.sh` gains a `bans.deny` entry
with `wrappers` for `sqlx` and for `axum` when task E2 makes both true.

### Agents

- Worktree from `develop`, never from `main`; `PATH=$HOME/.cargo/bin:$PATH`;
  `CARGO_TARGET_DIR` under `/var/tmp/tack-agent-targets/<id>`; `--build-jobs 4 --test-threads 4`;
  `renice` applied from outside.
- Sequential tasks reuse one worktree and one `CARGO_TARGET_DIR`; only tasks that run abreast
  get their own, and at most three target directories exist at once. A fresh target directory is
  a full build of the workspace, which costs more than most tasks.
- No GUI, no headed browser; never the live board, `~/.docket`, `~/.config/tack/runner.toml` or a
  `tack.db`.
- No `Co-Authored-By`; the change writes its own `CHANGELOG.md` line under `[Unreleased]`; code
  comments carry no phase, task or ADR numbers (`scripts/check-comments.sh`); the User and
  Administrator guides carry no source paths.

## 3. Where the tree ends up

Seven crates today, nine after. Two are new; none is renamed.

| Crate | Layer | Holds | Normal deps | Why it is a crate |
|---|---|---|---|---|
| `tack-protocol` **(new)** | contract | the runner-v1 types (`execution/`), `lifecycle::validate_transition`, capabilities, `usage_provenance`, `mrp`, `prepare`, one `Clock`; `utoipa` behind an `openapi` feature | none in the workspace | the server and the runner share it and nothing else; it is the trust boundary |
| `tack-core` | domain | planning and runs policy, pure | protocol | pure logic |
| `tack-orch` | domain | `scheduler/` and `model_policy/` **pure** (their `wiring.rs` leave in phase 3) | protocol, core | the runs domain that is not protocol; pure after phase 3 |
| `tack-db` | adapter | every `sqlx` line in the server; `repo/<aggregate>/`, `queries/`, `StoreError`, migrations; the attempt state converted at its edge | protocol, core | the only crate allowed to name `sqlx` |
| `tack-app` **(new)** | application | the runs use cases (`runs/`), **every** tick (`auto_start`, `merge`, `github_sync`, `remote_backup`, `due_soon`, `retention`, `observability`) as `async fn tick()`, the scheduler's and model policy's wiring, the embedded runner's one module, `AppError`. It may name `reqwest` (its ticks talk to GitHub and the backup store); never `axum` or `sqlx` | protocol, core, orch, db | the only place that composes planning, runs and the store |
| `tack-api` | adapter | axum routes, auth, websocket, OpenAPI, DTOs, `ApiError` | protocol, core, db, app | the only crate allowed to name `axum`; it names `tack-db` because `AppState.repo` is the `Repository` and `ApiError` wraps `StoreError` |
| `tack-runner` | process | unchanged | protocol, core | never `tack-db` again |
| `tack-cli` | binary | `serve.rs` composition (builds the `Repository`, spawns the ticks), subcommands, MCP | api, app, runner, db, protocol, core | the composition root |
| `tack-test-support` | test | `setup_test_db()`, seeds, `FakeClock`, fixtures | protocol, core, db | shared by every crate's tests as a dev-dependency |

Allowed normal edges after the phase (the table `check-crate-graph.sh` ends with):

```
tack-protocol     -> (none)
tack-core         -> tack-protocol
tack-orch         -> tack-protocol tack-core
tack-db           -> tack-protocol tack-core
tack-app          -> tack-protocol tack-core tack-orch tack-db
tack-api          -> tack-protocol tack-core tack-db tack-app
tack-runner       -> tack-protocol tack-core
tack-cli          -> tack-api tack-app tack-runner tack-db tack-protocol tack-core
tack-test-support -> tack-protocol tack-core tack-db
```

Use cases take `&Repository` and `&dyn Clock` and plain arguments; they return
`Result<T, AppError>`. There are no store traits: a port is added the day a second implementation
of the store exists, not before.

Not in this phase, on purpose: a `tack-client` crate; an `integrations` crate; a split of the
runner; migrations as `.sql` files; dropping the mirrored request state in `execution_requests`
(a schema change; the typed enum will show whether it ever diverges); folding the pure
`tack-orch` into `tack-core` (possible once it is pure; a separate, small decision).

## 4. The phases

Each phase is one or more PRs into `develop`; the full suite and `.githooks/pre-push` run once on
each phase's merged tree. Do them in order.

### Phase 0 — the token file and the graph check

Script, all of it:

```bash
printf '/decision-token\n' >> .gitignore
git check-ignore decision-token                       # prints "decision-token", exit 0

cat > scripts/check-crate-graph.sh <<'EOF'
#!/usr/bin/env bash
# Fails when a workspace crate has a normal dependency on one this table does not allow.
# dev-dependencies are exempt: every crate may test with tack-test-support.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.."
declare -A ALLOW=(
  [tack-core]=""
  [tack-db]="tack-core"
  [tack-orch]="tack-core tack-db"
  [tack-api]="tack-core tack-db tack-orch"
  [tack-runner]="tack-core tack-orch"
  [tack-cli]="tack-api tack-runner tack-orch"
  [tack-test-support]="tack-core tack-db"
)
if [ "${1:-}" = "--print" ]; then for k in "${!ALLOW[@]}"; do echo "$k -> ${ALLOW[$k]:-(none)}"; done | sort; exit 0; fi
fail=0
while IFS=$'\t' read -r from to; do
  case " ${ALLOW[$from]-} " in *" $to "*) ;; *) echo "forbidden edge: $from -> $to"; fail=1;; esac
done < <(cargo metadata --format-version 1 --no-deps \
  | jq -r '.packages[] | .name as $n | .dependencies[] | select(.path != null and .kind == null) | [$n, .name] | @tsv')
exit $fail
EOF
chmod +x scripts/check-crate-graph.sh
scripts/check-crate-graph.sh                           # no output, exit 0: the table IS today's graph
# .githooks/pre-push: add `scripts/check-crate-graph.sh` right after the clippy line.
```

Model: none. Verify: `git check-ignore decision-token && scripts/check-crate-graph.sh && .githooks/pre-push`.

### Phase 1 — `tack-protocol`

Goal: the runner-v1 contract is one crate; the runner stops compiling SQLite; three server clocks
become one; the `*Schema` mirrors go because the real types derive `ToSchema`; `ExecutionState`
gains `as_str`/`FromStr` so phase 2 can convert at the store's edge.

What moves, nothing rewritten:

| From | To |
|---|---|
| `crates/tack-orch/src/execution/` (whole directory) | `crates/tack-protocol/src/execution/` |
| `crates/tack-orch/src/usage_provenance.rs` + `usage_provenance/` | `crates/tack-protocol/src/` |
| `crates/tack-core/src/mrp.rs` | `crates/tack-protocol/src/mrp.rs` |
| `crates/tack-core/src/prepare.rs` + `prepare/` | `crates/tack-protocol/src/` |
| `crates/tack-orch/tests/runner_contract.rs` + `runner_contract/` | `crates/tack-protocol/tests/` |
| `crates/tack-core/tests/mrp_contract.rs` | `crates/tack-protocol/tests/` |

Script (task 1.0):

```bash
set -euo pipefail; export PATH=$HOME/.cargo/bin:$PATH; P=crates/tack-protocol
mkdir -p $P/src $P/tests
cat > $P/Cargo.toml <<'EOF'
[package]
name = "tack-protocol"
description = "The runner-v1 contract: what the board and a runner say to each other"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[features]
openapi = ["dep:utoipa"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
utoipa = { workspace = true, optional = true }

[lints]
workspace = true
EOF
# root Cargo.toml: add "crates/tack-protocol" first in [workspace].members

git mv crates/tack-orch/src/execution $P/src/execution
git mv crates/tack-orch/src/usage_provenance.rs $P/src/usage_provenance.rs
git mv crates/tack-orch/src/usage_provenance $P/src/usage_provenance
git mv crates/tack-core/src/mrp.rs $P/src/mrp.rs
git mv crates/tack-core/src/prepare.rs $P/src/prepare.rs
git mv crates/tack-core/src/prepare $P/src/prepare
git mv crates/tack-orch/tests/runner_contract.rs $P/tests/runner_contract.rs
git mv crates/tack-orch/tests/runner_contract $P/tests/runner_contract
git mv crates/tack-core/tests/mrp_contract.rs $P/tests/mrp_contract.rs

cat > $P/src/lib.rs <<'EOF'
//! The runner-v1 contract: the types the board and a runner exchange, the attempt
//! lifecycle, capability negotiation, usage provenance, the merge-readiness pack and the
//! prepare step. No I/O, no database: the server and the runner both build on this and on
//! nothing else they share.

pub mod clock;
pub mod execution;
pub mod mrp;
pub mod prepare;
pub mod usage_provenance;

pub use clock::{Clock, SystemClock};
EOF
cat > $P/src/clock.rs <<'EOF'
use chrono::{DateTime, Utc};

/// Where "now" comes from, so a test can hold it still.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
EOF

# Paths outside the new crate.
files=$(git ls-files 'crates/**/*.rs' | grep -v '^crates/tack-protocol/')
sed -i -e 's/\btack_orch::execution\b/tack_protocol::execution/g' \
       -e 's/\btack_orch::usage_provenance\b/tack_protocol::usage_provenance/g' \
       -e 's/\btack_core::mrp\b/tack_protocol::mrp/g' \
       -e 's/\btack_core::prepare\b/tack_protocol::prepare/g' $files
sed -i -e 's/\bcrate::execution\b/tack_protocol::execution/g' \
       -e 's/\bcrate::usage_provenance\b/tack_protocol::usage_provenance/g' $(git ls-files 'crates/tack-orch/**/*.rs')
sed -i -e 's/\bcrate::mrp\b/tack_protocol::mrp/g' -e 's/\bcrate::prepare\b/tack_protocol::prepare/g' $(git ls-files 'crates/tack-core/**/*.rs')
# Paths inside the new crate: a crate never names itself.
sed -i -e 's/\btack_core::prepare\b/crate::prepare/g' -e 's/\btack_core::mrp\b/crate::mrp/g' \
       -e 's/\btack_orch::execution\b/crate::execution/g' $(git ls-files 'crates/tack-protocol/src/**/*.rs')
sed -i -e 's/\btack_core::mrp\b/tack_protocol::mrp/g' -e 's/\btack_orch::execution\b/tack_protocol::execution/g' \
  $(git ls-files 'crates/tack-protocol/tests/**/*.rs')

sed -i '/^pub mod execution;$/d;/^pub mod usage_provenance;$/d' crates/tack-orch/src/lib.rs
sed -i '/^pub mod mrp;$/d;/^pub mod prepare;$/d' crates/tack-core/src/lib.rs
for c in tack-core tack-db tack-orch tack-api tack-runner tack-cli tack-test-support; do
  grep -q 'tack-protocol' crates/$c/Cargo.toml || \
  sed -i '0,/^\[dependencies\]$/s//[dependencies]\ntack-protocol = { path = "..\/tack-protocol" }/' crates/$c/Cargo.toml
done
sed -i 's|^tack-protocol = { path = "../tack-protocol" }|tack-protocol = { path = "../tack-protocol", features = ["openapi"] }|' crates/tack-api/Cargo.toml
sed -i '/^tack-orch = /d' crates/tack-runner/Cargo.toml

# The three server clocks become tack_protocol::Clock.
sed -i -e 's/\bExecutionClock\b/Clock/g' -e 's/\bSystemExecutionClock\b/SystemClock/g' \
       -e 's/\bRetentionClock\b/Clock/g' -e 's/\bObservabilityClock\b/Clock/g' $files

cargo check --workspace --all-targets --message-format short 2>&1 | grep -E '^crates/.*: (error|warning)' | sort -u > /tmp/phase1-errors.txt
wc -l /tmp/phase1-errors.txt     # expected under ~40; more means a sed misfired: git checkout -- crates, retry one sed at a time
```

Model (task 1.1, input: `/tmp/phase1-errors.txt`):

1. Fix every line the list names: missing `use tack_protocol::...`, the three trait definitions
   and their `SystemClock` impls deleted by hand (`db/repo/execution.rs`,
   `orch/execution_retention.rs`, `orch/execution_observability.rs`), test clocks that implemented
   the old names. No new logic.
2. In `tack-protocol`, `#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` on every type
   that `crates/tack-api/src/handlers/executions.rs` mirrors as `*Schema`; delete each mirror and
   point its `components(schemas(...))` or `body = ...` at the real type.
3. `impl ExecutionState { pub const fn as_str(self) -> &'static str }` and `impl FromStr`, with
   the serde names, plus one test that round-trips every variant through both. (Also for
   `RecoveryDisposition` if it lives in the moved files; otherwise leave it.)
4. `./scripts/regen-generated.sh`; `git diff --stat docs/openapi.json` shows schema *names*
   changing only. A field appearing or vanishing means a mirror was not faithful: stop and report.

Done when: `cargo check --workspace --all-targets` is clean;
`cargo nextest run --workspace -E 'rdeps(tack-protocol)'` passes;
`cargo tree -p tack-runner -e normal --prefix none | grep -cE '^(sqlx|libsqlite3-sys|tack-db|tack-orch) '` → 0;
`grep -rn 'trait .*Clock' crates --include='*.rs' | grep -v target` → exactly `tack-protocol/src/clock.rs` and `tack-runner/src/clock.rs`;
`grep -rhoE 'pub (struct|enum) \w+Schema\b' crates/tack-api/src | wc -l` → 3 (the `openapi.rs` ones, D6);
`cargo nextest list --workspace | wc -l` equal to before.

Tighten: `tack-runner` → `"tack-core tack-protocol"`; every other row gains `tack-protocol`;
`tack-protocol` gets an empty row.

### Phase 2 — the SQL has one home

Goal: no `sqlx` outside `tack-db`; errors cross the boundary as `StoreError`; attempt state is
the enum everywhere; `repo/execution.rs` is split by lifecycle; `tack-api` never names a pool.

**The recipe** (every task of group 2 follows it, per site in its list):

| Kind of site | Where it goes | How |
|---|---|---|
| `SELECT` for a response, a list, a count or a tick's input | `crates/tack-db/src/queries/<table>.rs`, one `pub async fn` per query taking `&Repository`, returning a row struct (`#[derive(sqlx::FromRow)]`) defined beside it | SQL moved verbatim. The function's name is the question it answers (`attempts_of_request`), not the handler's name. The row's fields are the columns, with the column's names |
| `SELECT` that exists only to then write | the same store method as the write, in one `BEGIN IMMEDIATE` transaction | the handler's two calls become one `repo.<verb>_<aggregate>(...)` |
| `INSERT` / `UPDATE` / `DELETE` | a method in `impl Repository` in `crates/tack-db/src/repo/<aggregate>.rs` (or `repo/execution/<stage>.rs`), returning `Result<T, StoreError>` | one transaction per command; a verb in the aggregate's language (`revoke_runner`, `record_decision`) |
| `sqlx::Error` in a signature outside `tack-db` | `StoreError` | `?` converts; `NotFound` and `Conflict` are raised by the store, never guessed by the caller |
| a state literal | `ExecutionState` / `RecoveryDisposition` | `tack-db` converts once per row with `as_str()` and `from_str` in `repo/execution/state.rs`; no literal outside that file |
| `sqlx::` in a test under `tack-api/tests` | a **seed** → `tack_test_support::seed::<table>(...)`; an **assert over rows** → the new `queries::` function | no test keeps `sqlx::` |

`StoreError` (`crates/tack-db/src/error.rs`, task C1):

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}
```

`ApiError` gains `Store(#[from] StoreError)` in C1 (`NotFound` → 404, `Conflict` → 409, `Sql` →
500). `ApiError::Database(sqlx::Error)` **stays until task E2 deletes it**, so each task merges
on its own; the gate of the phase is the grep, not the compiler.

`Repository::pool()` becomes `pub(crate)` in E2. Until then a handler may still call it; after,
nothing outside `tack-db` can.

Script per task (the list is the whole input):

```bash
f=crates/tack-api/src/handlers/<file>.rs          # the task's file(s)
grep -nE 'sqlx::(query|query_as|query_scalar)|\.fetch_(one|all|optional)|sqlx::Error' $f > /tmp/<task>-sites.txt
grep -lE 'sqlx::' $(git ls-files 'crates/tack-api/tests/**/*.rs') | xargs grep -lE '<table1>|<table2>' > /tmp/<task>-tests.txt
```

Filterset while working: `-E 'binary(<the test binaries in /tmp/<task>-tests.txt>)'`.

Verify after E2:

```bash
grep -rn 'sqlx' crates/tack-api/src crates/tack-api/tests crates/tack-orch/src crates/tack-cli/src crates/tack-api/Cargo.toml crates/tack-orch/Cargo.toml | wc -l   # 0
grep -rnE '"(leased|preparing|waiting_decision|needs_operator)"' crates/tack-db/src crates/tack-api/src --include='*.rs' | grep -v tests | grep -v 'repo/execution/state.rs' | wc -l   # 0
grep -rn '\.pool()' crates/tack-api crates/tack-orch crates/tack-cli --include='*.rs' | wc -l   # 0
wc -l crates/tack-db/src/repo/execution/*.rs | sort -rn | head -1     # under 1 200
make deny && scripts/check-crate-graph.sh && cargo nextest run --workspace && .githooks/pre-push
```

### Phase 3 — `tack-app`: the use cases and the ticks

Goal: the rules of a run live in functions that take the store and a clock, not in axum
handlers; every background loop is an `async fn tick()` that `tack serve` schedules; the
embedded runner has one owner; `tack-orch` is pure.

Script (C2 runs the first block; each §5 task that moves a file runs its own lines of the
rest, after that file has no `sqlx`):

```bash
A=crates/tack-app; mkdir -p $A/src/runs $A/tests
# Cargo.toml: deps tack-protocol, tack-core, tack-orch, tack-db, tokio, tracing, chrono, uuid,
# serde, serde_json, thiserror, reqwest; dev-dep tack-test-support. NO axum, NO sqlx.
# Add to workspace members.
for m in auto_start autonomy merge execution_runtime github_sync rate_card; do
  git mv crates/tack-api/src/$m.rs $A/src/$m.rs; [ -d crates/tack-api/src/$m ] && git mv crates/tack-api/src/$m $A/src/$m
  sed -i "s/\bcrate::$m\b/tack_app::$m/g" $(git ls-files 'crates/tack-api/**/*.rs')
done
for m in execution_retention execution_observability; do
  git mv crates/tack-orch/src/$m.rs $A/src/${m#execution_}.rs; git mv crates/tack-orch/src/$m $A/src/${m#execution_}
  sed -i "/^pub mod $m;$/d" crates/tack-orch/src/lib.rs
  sed -i "s/\btack_orch::$m\b/tack_app::${m#execution_}/g" $(git ls-files 'crates/**/*.rs')
done
git mv crates/tack-orch/src/scheduler/wiring.rs $A/src/scheduler_wiring.rs
git mv crates/tack-orch/src/model_policy/wiring.rs $A/src/model_policy_wiring.rs
sed -i '/^pub mod wiring;$/d;/^pub use wiring::/d' crates/tack-orch/src/scheduler/mod.rs crates/tack-orch/src/model_policy/mod.rs
git mv crates/tack-orch/tests/ingestion.rs $A/tests/ingestion.rs; git mv crates/tack-orch/tests/ingestion $A/tests/ingestion
sed -i '/^tack-db = /d' crates/tack-orch/Cargo.toml
cargo check --workspace --all-targets --message-format short 2>&1 | grep -E '^crates/.*: error' | sort -u
# expected: every error is one of three kinds: a moved file names crate::AppState or an axum
# type (it takes &Repository, &dyn Clock and plain values instead), a path the sed missed, or a
# scheduler wiring re-export. The task that moved the file fixes them.
grep -n 'tokio::time::interval\|run_github_poll_task\|run_auto_start_task' crates/tack-api/src/server.rs   # the loops that become ticks
```

Shape (C2 writes the error type, one use case and `serve.rs`; the rest copy):

- `crates/tack-app/src/error.rs`: `AppError { NotFound, Conflict(String), InvalidTransition {
  from, to }, StaleLease { .. }, Store(#[from] StoreError), Core(#[from] CoreError) }`.
  `ApiError` gains `From<AppError>`; the mapping to the fifteen frozen runner-v1 codes moves
  into that one `impl`, out of the handlers.
- A use case: `pub async fn <verb>(repo: &Repository, clock: &dyn Clock, <args>) ->
  Result<T, AppError>` in `crates/tack-app/src/runs/<verb>.rs`, holding what the handler held:
  idempotency keys, fencing checks, `validate_transition`. The handler becomes extract, call,
  `map_err`, respond.
- A tick: each moved loop file exposes `pub async fn tick(repo, clock, <settings>) ->
  Result<Outcome, AppError>` that does one pass, and `pub async fn run(repo, clock, settings,
  interval)` that loops over it. The tests call `tick()` with a `FakeClock`; no
  `tokio::time::sleep` in a test.
- `crates/tack-cli/src/serve.rs` (new, ~60 lines): builds the `Repository`, the `AppState`,
  spawns every tick, calls `tack_api::serve`. `tack_api::serve` no longer spawns a loop.
  Websocket send/recv tasks and fire-and-forget spawns in handlers stay where they are: they are
  not loops.

Tests: a handler test under `tack-api/tests` that asserts a **rule** (a transition denied, an
idempotent replay, a limit reached) moves to `crates/tack-app/tests/<verb>.rs` with its HTTP
scaffolding removed, still on `setup_test_db()`. One that asserts a status code, a header or a
JSON shape stays. A test is never in both places. `tack-cli/tests/e6_scheduler_e2e_test.rs` and
`embedded_runner_state_scoping.rs` keep running the real server: they prove `serve.rs` composes
the same thing.

Verify after E2 (the same gate as Phase 2's):

```bash
grep -rn 'tokio::time::interval' crates/tack-api/src | grep -v tests | wc -l      # 0
grep -rln 'axum' crates/tack-app crates/tack-db crates/tack-orch | wc -l           # 0
grep -rn 'AppState' crates/tack-app | wc -l                                        # 0
grep -rln 'tack_db\|sqlx' crates/tack-orch/src | wc -l                             # 0
wc -l crates/tack-api/src/handlers/executions.rs crates/tack-api/src/handlers/runner_protocol.rs   # each under 1 000
scripts/check-crate-graph.sh && make deny && cargo nextest run --workspace && .githooks/pre-push
```

Tighten: the table becomes §3's; `gen-deny-toml.sh` adds `{ crate = "axum", wrappers = ["tack-api", "tack-cli"] }`.

### Phase 4 — shape and dead weight

Goal: unused code and dependencies are gone; the frontend's `shared/` only points inward; the
two files nobody can read in one sitting are split. Only 4.1 is a script; 4.2 to 4.4 are small
model tasks.

```bash
# 4.1 script
export PATH=$HOME/.cargo/bin:$PATH; cargo install cargo-machete
sed -i '/#\[allow(dead_code)\]/d;/#!\[allow(dead_code)\]/d;/#\[allow(unused/d' $(git ls-files 'crates/**/*.rs')
cargo clippy --workspace --all-targets --message-format short 2>&1 | grep -E 'never (used|read|constructed)' | sort -u > /tmp/dead.txt
cargo machete > /tmp/machete.txt || true
# /tmp/dead.txt and /tmp/machete.txt are task 4.2's input.
```

- 4.2 delete every item `/tmp/dead.txt` names (an item used only under `cfg(test)` gets
  `#[cfg(test)]`, not an allowance) and every dependency `/tmp/machete.txt` names (a
  `cfg(feature)` false positive goes in `[package.metadata.cargo-machete] ignored`).
  Done when `cargo clippy --workspace --all-targets -- -D warnings` is clean, `cargo machete`
  prints nothing, and `grep -rc 'allow(dead_code)\|allow(unused' $(git ls-files 'crates/**/*.rs') | grep -v ':0' | wc -l` → 0.
- 4.3 frontend (done: the three files only named `features/` in comments, so nothing moved; the
  work was rewording them and the two test cases below): `shared/` files stop importing `features/`. `shared/runWithAgent`, `shared/execution`
  and `shared/agents` **stay in `shared/`**: seven features import them and the architecture
  test forbids a feature importing another. `src/architecture.test.ts` grows two cases: no
  `shared/*` file imports from `features/*`; imports through the `@/` alias count. Done when
  `npm test` passes and `grep -rl "features/" frontend/src/shared --include='*.ts*' | wc -l` → 0.
- 4.4a `cli/src/main.rs`: one module per `Commands` group, cut along the `match` arms:
  `cmd/board.rs` (init, export, import, projects, project, add, list, move, board, branch, start,
  open, search, config, completions), `cmd/entities.rs` (sprint, template, role, comment, field),
  `cmd/backup.rs`, `cmd/runs.rs` (execution, fleet, runner, service, agent-profile). `main.rs`
  keeps `Commands`, `main` and the match. Done when `wc -l main.rs` < 600 and
  `cargo nextest run --workspace -E 'package(tack-cli)'` passes.
- 4.4b `runner/src/engine.rs`: split by `impl` block into `engine/decisions.rs`,
  `engine/outcome.rs`, `engine/resume.rs`; `engine.rs` keeps the struct, the trait and the run
  loop; `engine/tests.rs` stays one file. Done when `wc -l engine.rs` < 1 200 and
  `-E 'package(tack-runner)'` passes.

Verify: the four "done when" lines, then `cargo nextest run --workspace && (cd frontend && npm test && npx playwright test) && .githooks/pre-push`.

### Phase 5 — the documents say what the tree says

Script: `scripts/arch-facts.sh` prints the crate count, route count, migration count, test count
and `check-crate-graph.sh --print`. Model (task 5.1): rewrite the "Project structure" and
"Crate Boundaries & Responsibilities" sections of `docs/ARCHITECTURE.md` from §3 and the
script's output, replacing every literal count with the script's name; rewrite
`docs/book/src/developer/crate-tour.md`'s crate list; update its per-file walkthrough only for
files `git log --diff-filter=R --name-status develop..` lists; delete the sentence saying
`tack-orch` "depends inward on tack-core/tack-db"; add the `[Unreleased]` changelog entry for
the phase as a whole.

Verify: `scripts/check-comments.sh`, `mdbook build docs/book`,
`grep -n '92 migrations\|96 documented paths\|100% complete' docs/ARCHITECTURE.md` → 0.

## 5. The task list

One row is one agent run, or one script run. **Scope** is every source file the agent may read
or edit, with its line count: what the agent holds in context before editing. **Done when** is
the command. Rows sharing a **group** letter run in parallel in separate worktrees, at most three
at a time; groups run in the order shown. A row's commit names the row (`Phase 79, task D4: ...`).

**A file is visited once.** Phase 2 (the SQL) and Phase 3 (the use cases and the ticks) are two
recipes for one fan-out: the agent that takes a handler file out of `sqlx` also extracts its use
cases, or gives its loop a `tick()`, in the same run, because it has already read the file. The
two conventions tasks come first so every later task copies a pattern instead of inventing one.

An agent's prompt is: its row, the recipe sections of §4 its row names, and the agent rules of
§2. Nothing else. No agent reads §1, §6, §7, the ADR or the guides.

### Structure first

| Task | Kind | Scope (lines) | Done when | Group |
|---|---|---|---|---|
| 0.0 | script | `.gitignore`, `scripts/check-crate-graph.sh`, `.githooks/pre-push` | §4 Phase 0 verify | a |
| 1.0 | script | the Phase 1 script | `/tmp/phase1-errors.txt` under ~40 lines | b |
| 1.1 | model | the files `/tmp/phase1-errors.txt` names; `tack-protocol/src/execution/types.rs`; the `*Schema` mirrors of `tack-api/src/handlers/executions.rs` (~300 of its 3 023 lines, by line range) | Phase 1 done-when; full suite once; `pre-push` | c |

### Conventions

| Task | Kind | Scope (lines) | Done when | Group |
|---|---|---|---|---|
| C1 | model | **store conventions:** `tack-db/src/{error.rs, lib.rs}`, `tack-db/src/queries/mod.rs`, `tack-db/src/repo/execution/state.rs` (the enum conversion), `tack-api/src/error.rs` (`ApiError::Store`), `tack-test-support/src/seed.rs`; one worked example of every row of the Phase 2 recipe, taken from `handlers/attempt_lists.rs` (421, 5 sites) and its tests | `attempt_lists.rs` has no `sqlx`; each new type and the conversion has one test; `-E 'binary(handlers) + binary(repository)'` passes | d |
| C2 | model | **app conventions:** `crates/tack-app` created (`Cargo.toml`: protocol, core, orch, db, tokio, tracing, chrono, uuid, serde, thiserror, reqwest; dev-dep test-support; never axum or sqlx), `src/{lib.rs, error.rs}` (`AppError`, `From<AppError> for ApiError`), one use case `runs/mark_decision_viewed.rs` (25 lines, from `handlers/decisions.rs`), the tick shape as a doc comment in `lib.rs`; `tack-cli/src/serve.rs` that builds the `Repository` and the `AppState` and calls `tack_api::serve` (it spawns nothing yet: each loop moves with the task that owns its file) | `cargo check` clean; `-E 'binary(runner_protocol) + package(tack-cli)'` passes; `scripts/check-crate-graph.sh` with `tack-app` added | e (after C1) |
| C3 | model | `tack-db/src/repo/execution.rs` (4 087) → `repo/execution/{enqueue,claim,events,decisions,artifacts,recovery,completion}.rs`, cut along the existing methods by the stage each test file names; no SQL changes | `cargo check` clean; no file over 1 200 lines; `-E 'binary(repository)'` passes with the same count | e |

### The fan-out — one visit per file

SQL per the Phase 2 recipe; use cases and ticks per the Phase 3 shape. Each row's site list is
the per-task script of §4 Phase 2; "+ rules" means the row also extracts use cases, "+ tick"
means the file moves to `tack-app` and its loop leaves `server.rs` for `serve.rs`.

| Task | Scope (lines) | Sites | Also | Tests it touches | Done when | Group |
|---|---|---|---|---|---|---|
| D1 | `handlers/runner_admin.rs` (1 394), `handlers/runner_protocol/runner_auth.rs` | 20 | — | `tests/handlers/executions_runner_admin.rs` (70 sqlx), `tests/runner_protocol/enrollment.rs` | its sites → 0; its test files have no `sqlx`; `-E 'binary(handlers) + binary(runner_protocol)'` passes | f |
| D2 | `handlers/runner_protocol.rs`, handlers `enroll`, `claim`, `heartbeat`, `renew`, `observe_recovery` (~1 300 of 2 910, by line range) | ~17 | + rules → `runs/{enroll,claim,heartbeat,observe_recovery}.rs` | `tests/runner_protocol/{lifecycle,enrollment}.rs`, `tests/security/chaos_*` | its sites → 0; its handlers are extract/call/map; rule tests moved to `tack-app/tests`; `-E 'binary(runner_protocol) + binary(security)'` passes | f |
| D3 | `handlers/runner_protocol.rs`, handlers `submit_events`, `submit_artifacts`, `put_artifact_content`, `create_decision`, `complete` (~1 400) | ~18 | + rules → `runs/{report_events,submit_artifacts,create_decision,complete}.rs` | `tests/runner_protocol/{artifact_events,decisions}.rs`, `tests/wiring/artifact.rs` | same | f (may run beside D2: the only shared lines are the `use` block; the second to merge rebases) |
| D4 | `handlers/executions.rs`, handlers `create_execution`, `create_as`, `cancel`, `requeue_needs_operator` (~1 000 of 3 023) | ~12 | + rules → `runs/{start_run,cancel_run,requeue}.rs` | `tests/handlers/executions*.rs` naming those routes | same, `binary(handlers)` | f |
| D5 | `handlers/executions.rs`, `list_execution_attempts`, `load_profile`, `get_item_agent_context`, `run_limits_of` and the rest (~1 500) | ~18 | — (read models → `queries/`) | the rest of `tests/handlers/executions*.rs`, `tests/runner_vertical_slice/repository_crash.rs` | its sites → 0 | f (beside D4, same note) |
| D6 | `handlers/decisions.rs` (736), `handlers/mrp.rs` (269) | 8 | + rules → `runs/{resolve_decision,expire_decisions}.rs` | `tests/runner_protocol/decisions.rs`, `tests/handlers/mrp.rs` | same | g |
| D7 | `auto_start.rs` (351), `merge.rs` (382), `autonomy.rs` (307), `rate_card.rs` (57: its one site only; the file is routes and stays) | 21 | + tick: the three move to `tack-app`, each with `tick()`/`run()`; `serve.rs` spawns `auto_start::run`; the auto-start loop leaves `server.rs` | `tests/handlers/{autonomy,merge_button,rework}.rs`, the moved files' `tests.rs` | sites → 0; `grep -n auto_start crates/tack-api/src/server.rs` → 0; `-E 'binary(handlers) + package(tack-app)'` passes | g |
| D8 | `github_sync.rs` (1 032), `handlers/pull_requests.rs` (151), `lib.rs`'s `run_github_poll_task` | 9 | + tick: `github_sync` moves to `tack-app` with `tick()`/`run()`; `serve.rs` spawns it; the poll task leaves `lib.rs`/`server.rs` | `tests/handlers/pull_requests.rs` | sites → 0; `grep -n github_poll crates/tack-api/src` → 0 | g |
| D9 | `remote_backup.rs` (858) + `remote_backup/tests.rs`, `handlers/backup.rs` (496), `server.rs`'s `run_scheduled_backup` | 33 | + tick: the backup loop becomes `remote_backup::{tick,run}`; the module moves to `tack-app`; `serve.rs` spawns it when an interval is set | `remote_backup/tests.rs`, `tests/handlers/backup*` | sites → 0; `grep -n backup_interval crates/tack-api/src/server.rs` → 0 | g |
| D10 | `prill_store.rs` (284), `debug.rs` (135), `server.rs`'s 3 sites and `check_due_soon` | 12 | + tick: `check_due_soon` becomes `tack-app/src/due_soon.rs` with `tick()`/`run()`; the hourly loop leaves `server.rs` | `tests/handlers/production_router.rs`, `tests/wiring/execution_sweep.rs` | sites → 0; `grep -rn 'tokio::time::interval' crates/tack-api/src` → 0 once D7–D10 are merged | g |
| D11 | `handlers/local_runner.rs` (600), `tack-cli/src/local_runner.rs` (552), `tack-cli/src/local_enrollment.rs` (152) | 7 | + rules: the logic and `LocalRunnerControl` go to `tack-app/src/local_runner.rs`; the three keep routes and clap glue | `tack-cli/tests/embedded_runner_*.rs`, `e6_scheduler_e2e_test.rs` | sites → 0; the three old files sum under 400 lines; those binaries pass | g |
| D12 | `handlers/{templates,settings,roles,metrics,items,export}.rs`, sites only, by line | 13 | — | the handler tests naming those routes | sites → 0 | g |
| D13 | `tack-orch/src/scheduler/wiring.rs` (641), `model_policy/wiring.rs` (29), `execution_retention.rs` (238) + its `tests.rs`, `execution_observability.rs` (258) + its `tests.rs`, `tack-orch/tests/ingestion/` | 4 + the `SELECT`s | + tick: the `SELECT`s go to `tack-db/src/queries/`, signatures return `StoreError`; the four files move to `tack-app` (`scheduler_wiring.rs`, `model_policy_wiring.rs`, `retention.rs`, `observability.rs`), the two ticks expose `tick()`/`run()` and `serve.rs` spawns them; `tack-db` leaves `tack-orch/Cargo.toml` | `tack-orch/tests/scheduling/*`, the moved `ingestion/` | `grep -rln 'sqlx\|tack_db' crates/tack-orch/src` → 0; `-E 'binary(scheduling) + binary(ingestion)'` passes | g |

Group f is the four biggest files, three agents abreast; group g is nine disjoint scopes, three
abreast, in any order. D2 and D4 should land before D3 and D5 when run sequentially, so the
second agent on each file starts from a smaller file.

### Close

| Task | Kind | Scope | Done when | Group |
|---|---|---|---|---|
| E1 | model | every file under `tack-api/tests` still naming `sqlx::` after group g (`grep -rln 'sqlx::' crates/tack-api/tests`): each site becomes a `seed::` or a `queries::` call | that grep → 0; `-E 'package(tack-api)'` passes | h |
| E2 | script + small fixes | delete `ApiError::Database`; `Repository::pool()` → `pub(crate)` (the compiler lists the callers left; each becomes a method or a query); `sqlx` out of `tack-api/Cargo.toml` and `tack-orch/Cargo.toml`; `gen-deny-toml.sh` bans `sqlx` outside `tack-db`/`tack-test-support` and `axum` outside `tack-api`/`tack-cli`; the graph table becomes §3's | the Phase 2 and Phase 3 verify blocks; full suite once; `pre-push` | i |

### Shape and documents

| Task | Kind | Scope | Done when | Group |
|---|---|---|---|---|
| 4.1 | script | allowances removed; `/tmp/dead.txt`, `/tmp/machete.txt` | the two files exist | j |
| 4.2 | model | the items and dependencies the two files name | §4 Phase 4 bullet 4.2 | k |
| 4.3 | model | `frontend/src/shared/{state/itemEvents.ts, state/executionContext.tsx, agents/FirstRunBanner.tsx}`, `src/architecture.test.ts` | bullet 4.3 | k |
| 4.4a | model | `tack-cli/src/main.rs` (3 054) → `cmd/*.rs` | bullet 4.4a | k |
| 4.4b | model | `tack-runner/src/engine.rs` (2 611) → `engine/*.rs` | bullet 4.4b | k |
| 5.0 | script | `scripts/arch-facts.sh` | prints five lines | l |
| 5.1 | model | `docs/ARCHITECTURE.md` (two sections), `docs/book/src/developer/crate-tour.md`, `CHANGELOG.md` | Phase 5 verify; full suite once; `pre-push` | m |

**Totals:** 4 scripts, 24 model runs, three full-suite gates (after 1.1, E2, 5.1). The largest
scope is ~1 500 lines (D5). Every handler file is read by one agent per visit and the four
biggest are visited once, not once for the SQL and again for the rules: about 7 000 fewer lines
read than the two-pass version, and about half the agent runs on those files. Sequential tasks
reuse one worktree and one target directory (§2); a run that needs a fresh target directory
costs a full build of the workspace before its first edit.

## 6. What can go wrong, and what to do

| Symptom | Meaning | Do |
|---|---|---|
| 1.0 leaves more than ~40 errors | a sed hit something it should not have | `git checkout -- crates && git clean -fd crates/tack-protocol`, re-run one sed at a time with `cargo check` between |
| A moved module needs `tack_core::models` | it is domain, not protocol | leave it in `tack-core`; `tack-protocol` never depends on `tack-core` |
| `docs/openapi.json` shows a field appearing or vanishing in 1.1 | a mirror was not faithful | stop; report the field; the maintainer decides which side was right |
| A fan-out agent needs a store method a sibling task is writing | two tasks own one aggregate | the later task waits; the earlier commits first; never two agents in one `repo/<aggregate>` file |
| Two agents on one handler file (D2/D3, D4/D5) conflict | both edited the `use` block or a shared helper | the second to merge rebases; a helper both need moves to `tack-app` in the first to land |
| `cargo nextest list` count drops across a task by more than its deleted tests | a test got lost in a move | `git diff --stat -M` must show `R`, not `D` + `A` |
| A use case needs `axum::http::StatusCode` | the HTTP mapping leaked into the rule | the use case returns `AppError`; the status is `ApiError`'s business |
| A use case wants a method only a trait could fake | the temptation of the first draft | it takes `&Repository`; the test seeds SQLite |
| `cargo machete` names a dependency that `cfg(feature)` uses | false positive | `[package.metadata.cargo-machete] ignored` with the feature's name |
| An agent has read more than its scope's line count | its list was wrong, or it explored | it stops and reports; the row is split before it is re-run |

## 7. What the 2026-10-25 audit changed, and why

- **No ports, no fakes, no `store_contract!`.** The draft promised rule tests "in milliseconds
  against a fake"; every test already runs on in-memory SQLite and the slowest binary takes ~6 s.
  A fake over a `BTreeMap` would also hide what the store is for here: `BEGIN IMMEDIATE`, fencing,
  races. Two implementations plus a suite proving they agree is the same behaviour tested twice,
  which ADR 0010 decision 5 forbids. Use cases take `&Repository`.
- **`tack-api` → `tack-db` is an allowed edge.** The draft's table forbade it while its own
  handlers called `state.repo` and its `ApiError` wrapped `StoreError`.
- **`tack-orch` is made pure, not left "unchanged".** Its two `wiring.rs` named `sqlx` and
  `tack_db`; the draft's verify expected zero `sqlx` there with no step to make it so. The
  wiring and both of its ticks go to `tack-app`; the five ticks have one home.
- **The spawn criterion.** Only 4 of the 13 `tokio::spawn` in `tack-api` are loops; websocket
  tasks are not. The gate is "no `interval` loop in `tack-api`".
- **The graph script counts normal edges only.** The draft's table omitted the six `dev` edges
  its own script reported.
- **`repo/execution.rs` is split** (4 087 lines; the draft added ~90 sites to it).
- **The frontend folders stay in `shared/`.** Seven features import them; moving them into a
  feature would fail the architecture test that exists. The three inverted imports are the fix.
- **Phase 0's `.local/` move is gone**: everything it moved was already ignored.
- **Three script bugs fixed**: the self-referencing `tack_protocol::` path inside the new
  crate; `cargo check` output without `file:line`; the 2a `sed` that rewrote
  `state.repo.get_github_link(x)` into invalid code (the wrapper and the module function have
  different names). The 27 wrappers stay: they are the methods handlers call.
- **Tasks are sliced by file, after two conventions tasks, and each file is visited once.** The
  draft's "six agents" put ~7 800 lines in front of one agent for PR 2b and ~7 000 for phase 3's
  agent B, and read the four biggest files twice (once for the SQL, once for the rules). The
  2026-10-25 re-read merged the two passes into one fan-out and made sequential tasks reuse one
  worktree and target directory, since a fresh target directory is a full build.
- **`rate_card.rs` stays in `tack-api`**: the draft moved it, but it is routes.
- **`tack-app` may name `reqwest`**: the draft forbade it while moving `github_sync` and
  `merge`, which use it.
- **Evidence corrected**: ~8 mirrors, not 18; some of the 20 state literals are
  `RecoveryDisposition`; `ExecutionState` has no `as_str`/`FromStr` yet.

## 8. What the execution found (2026-10-26)

The tasks ran as written except where a measurement or the compiler disagreed. Each item is a
fact a reviewer should know, not a defect.

- **`Host`, one trait, not zero.** Moving `auto_start`, `merge`, `autonomy` and `github_sync`
  into `tack-app` needed ten calls back into code that still lives in `tack-api` (the GitHub
  token and API, prill's attestation, the artifact store, the websocket broadcast, the secret
  store). They sit behind `tack_app::host::Host`, implemented once in `tack-api/src/host.rs`.
  It is not a store trait: the store is still the concrete `Repository`. D7 and D8 each made a
  trait of their own; the merge folded them into this one. `ci_state` and `merge_pull_request`
  only forward to `tack_app::github_sync` and can go once `tack-app` is given the API base URL
  another way. `due_soon` has its own one-method sink for the webhook, which is an outgoing
  event, not a call back.
- **177 `Repository` signatures changed to `StoreError`.** Deleting `ApiError::Database` made the
  compiler list every public method that still returned `sqlx::Error`; the plan counted only the
  handler sites. The change is mechanical and the suite did not move.
- **`Repository::pool()` is behind a cargo feature, `raw-pool`,** enabled only by
  `tack-test-support`, because `pub(crate)` cannot serve a second crate. Cargo unifies features,
  so the gate is `check-crate-graph.sh` (fails on `.pool()` or `sqlx` outside `tack-db` and
  `tack-test-support`) and the `deny.toml` bans, not the visibility.
- **`axum` is allowed through `tack-api` and `tack-runner`** (the latter as a dev-dependency, for
  the fake docket), not `tack-cli`, which has no axum dependency.
- **Two edges the §3 table did not have were removed:** `tack-cli -> tack-orch` was unused, and
  `tack-api -> tack-orch` served one rule, now `tack_app::scheduler_wiring::runner_is_online`.
- **Task 4.3 had nothing to move.** The three `shared/` files only named `features/` in
  comments; the two new architecture-test cases are the whole change.
- **Three `*Schema` mirrors were not 8 but 5 plus 3:** five in `handlers/executions.rs` went in
  1.1; of the three in `openapi.rs`, two went with D6 and `DecisionAnswerSchema` stays, because
  the handler echoes the operator's JSON as sent (extra keys, `"text": null` versus absent).
  The generated spec lost the nullability of `Measurement<f64>.value`: utoipa 5 ignores field
  attributes on a generic parameter. The wire is unchanged and the frontend has its own type.
- **D11's three files sum to 661 lines, not under 400.** What is left needs `tack-runner` types
  and the API's `utoipa` attributes; moving it needs a `tack-app -> tack-runner` edge that would
  make `tack-api` link the runner. The 400 was an unmeasured estimate.
- **The four `dead_code` allowances on test-fixture modules are gone.** They were written when every file under `tests/` was its own binary and each module was compiled into several of them; with one binary per crate (ADR 0029) a helper no test uses is dead for real, and removing the allowances raised no warning.
- **The two retention and health loops lost their stop signal.** `ExecutionRuntime::stop` used to
  join them; they now follow the `run` shape and die with the process. A purge batch is one
  transaction, so a drop rolls it back.
- **Small behaviour differences, all on failure paths:** a few 500 messages now name the use case
  rather than the internal step; corrupt stored state text is a decode error (500) where some
  paths answered 409 or fell through to `Conflict`; the websocket `ItemUpdated` for a status
  move is sent after the rework enqueue instead of before; creating a profile and placing it in
  a fleet are one transaction instead of two statements.
- **`cargo deny check advisories` fails on advisories published since the last run** (an
  unsound `AsyncFd::new`, certificate-trust issues); `bans`, `licenses` and `sources` pass. Not
  caused by this phase and not touched.
- **Conflicts were the cost of the fan-out, not of the design.** Every parallel branch appended
  to the same registries (`queries/mod.rs`, `seed.rs`, `runs/mod.rs`, `serve.rs`), and three
  branches created the same helper twice. Three merge runs resolved them by hand. A rule for a
  next time: one task per registry owner, or append helpers only after a `grep` (the later
  prompts said so and the conflicts shrank).
- **The `git stash` is shared by every worktree**; an agent's merge ran a post-merge hook that
  built while another agent's stash sat on top. Agents do not use it.
