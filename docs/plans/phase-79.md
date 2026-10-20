# Phase 79: the crates say what depends on what, and the SQL lives in one place

**Executes:** [ADR 0087](../adr/0087-the-protocol-is-a-crate-and-the-sql-has-one-home.md)
(proposed 2026-10-20). Waits on the maintainer's "continúa".

**Done when:** `tack-runner` compiles without `sqlx` or `libsqlite3-sys` in its dependency tree;
no file under `crates/tack-api/src` contains `sqlx::`; the state of an attempt is one Rust enum
used by every crate; `scripts/check-crate-graph.sh` passes and runs from `.githooks/pre-push`;
the full suite passes once, on the merged tree, with no test duplicated across old and new
locations.

**Status (2026-10-20):** proposed. Nothing built.

A companion guide in Spanish, [`phase-79-guia.md`](phase-79-guia.md), explains the concepts
behind each step for someone learning architecture, Rust and AI engineering, and
[`phase-79-desde-cero.md`](phase-79-desde-cero.md) builds the target architecture from scratch,
layer by layer; this file is the operational plan.

This plan is written for whoever executes it, person or agent, one phase at a time. Every phase
says what a script does, what needs a model, which tests move or die, which filterset to run
while working, and what the tree looks like when it is done.

## 1. What was measured (2026-10-20, `develop` at `9c046fba`)

Every number below came from a command; re-run it before trusting it.

| Fact | Number | Command |
|---|---|---|
| `sqlx` query sites in `tack-api/src` | ~190 (78 `SELECT`, 12 `UPDATE`, 9 `INSERT`, 2 `DELETE`, the rest multi-line or built) | `grep -rcE 'sqlx::query\|query_as\|query_scalar\|\.fetch_(one\|all\|optional)' crates/tack-api/src --include='*.rs'` |
| `sqlx` query sites in `tack-db/src` | 503 | same, on `crates/tack-db/src` |
| `sqlx::` in `tack-api/tests` | 258 sites in 31 files | `grep -rl 'sqlx::' crates/tack-api/tests \| wc -l` |
| Attempt-state string literals in `tack-db` + `tack-api` | 20 | `grep -rnE '"(leased\|preparing\|waiting_decision\|needs_operator)"' crates/tack-db/src crates/tack-api/src --include='*.rs' \| grep -v tests \| wc -l` |
| `tack-runner` pulls `tack-db`, `sqlx`, `libsqlite3-sys` | yes, via `tack-orch` | `cargo tree -p tack-runner -e normal --prefix none \| grep -E '^(sqlx\|libsqlite3-sys\|tack-db) '` |
| `tack-orch/src/execution/*` and `usage_provenance.rs` import anything with I/O | no | `grep -n '^use' crates/tack-orch/src/execution/*.rs crates/tack-orch/src/usage_provenance.rs` |
| Equivalent `Clock` traits (`fn now(&self) -> DateTime<Utc>`) | 3 (`ExecutionClock` in db, `RetentionClock` and `ObservabilityClock` in orch); the runner's uses `SystemTime` | `grep -rn 'trait .*Clock' crates --include='*.rs'` |
| Hand-kept "mirror" types between api/orch/db | 18 structs, ~47 comment mentions | `grep -rhoE 'pub (struct\|enum) \w+(Schema\|Row\|Outcome\|Snapshot)\b' crates/tack-api/src crates/tack-orch/src \| sort -u \| wc -l` |
| `Repository` forwarding wrappers in `repo.rs` | 27 | `grep -c 'pub async fn' crates/tack-db/src/repo.rs` |
| Tests, by crate (`#[test]` + `#[tokio::test]`) | core 158, db 153, orch 112, api 476, runner 426, cli 128 | `grep -rE '#\[(tokio::)?test' crates/<c> --include='*.rs' \| wc -l` |
| Test files placed by `#[path = "..."]` (the `foo.rs` + `foo/tests.rs` convention) | 122 | `grep -rn '#\[path' crates --include='*.rs' \| wc -l` |
| `allow(dead_code)` / `allow(unused)` sites | 50 (17 in `tack-api/tests/common/mod.rs`, 12 in `tack-db/tests/common/execution_fixture.rs`, 10 in `handlers/runner_protocol.rs`) | `grep -rc 'allow(dead_code)\|allow(unused' crates --include='*.rs' \| grep -v ':0'` |
| `TACK_ORCH_ENABLE` in code | 0 (only in `docs/closed-cycles`) | `grep -rn TACK_ORCH_ENABLE crates frontend/src` |
| `cli/src/main.rs` | 3 054 lines, 65 functions | `wc -l`, `grep -cE '^(async )?fn '` |
| `runner/src/engine.rs` | 2 611 lines, 71 functions | same |
| `frontend/src/shared/runWithAgent` + `shared/execution` + `shared/agents` | 5 590 + 2 311 + 80 lines; 3 `shared/` files import from `features/` | `grep -rl "features/" frontend/src/shared --include='*.ts*'` |
| Pre-push gate runs | fmt, clippy `-D warnings`, nextest of 4 binaries, `gen:api` | `.githooks/pre-push` |
| `decision-token` at the repo root | untracked **and not ignored** (43 bytes) | `git check-ignore decision-token` (exits 1) |

## 2. Rules every phase follows

### Script or model

A step goes to a **script** when `cargo check` (or `npm test`) can tell whether it is right: moving
files, renaming paths, deleting what the compiler calls unused, listing sites. A step goes to a
**model** only when someone has to choose a name, a grouping or a transaction boundary. Every model
step gets, as its input, the output of a script (a list of `file:line`), a recipe with no choices
left open that the recipe does not name, and a "done when" that is a command. One agent per model
step; it never explores the tree, never runs the whole suite, never touches a file outside its list.

### Tests: nothing coexists

1. A test moves with its code, in the same commit: `git mv foo.rs` and `git mv foo/tests.rs`
   together. The `#[path = "foo/tests.rs"]` line follows the file.
2. There are no transition tests. When a new suite covers a behaviour, the old test that covered it
   is deleted in the same PR, and the PR description names both.
3. While working, run only what the change can reach:
   `cargo nextest run --workspace -E 'rdeps(<crate>)'`. The full suite runs once, on the merged
   tree of the phase, as the gate.
4. A store contract suite is written once as a macro (`store_contract!`), instantiated over SQLite
   and over the in-memory fake. It replaces the per-method tests in `crates/tack-db/tests` that it
   covers; those are deleted when the contract instance passes, in the same PR.
5. Never run `cargo test`; `cargo nextest run --workspace` only, and never `-p` (see
   `.config/nextest.toml`).

### The graph is checked, not described

From phase 0 on, `scripts/check-crate-graph.sh` holds a table of allowed `path` dependencies and
fails on any other edge. Each phase tightens the table in the same PR that makes the tightening
true. `scripts/gen-deny-toml.sh` gains `bans.deny` entries with `wrappers` for `sqlx` and `axum`
when phase 2 and 3 make them true.

### Agents

- Worktree from `develop`, never from `main`; `PATH=/home/ox/.cargo/bin:$PATH`;
  `CARGO_TARGET_DIR` under `/home/ox/tack-agent-targets/<id>`; `--build-jobs 4 --test-threads 4`;
  `renice` applied from outside.
- No GUI, no headed browser; never the live board, `~/.docket`, `~/.config/tack/runner.toml` or a
  `tack.db`.
- No `Co-Authored-By`; the change writes its own `CHANGELOG.md` line under `[Unreleased]`; code
  comments carry no phase, task or ADR numbers (`scripts/check-comments.sh`); the User and
  Administrator guides carry no source paths.

## 3. Where the tree ends up

Seven crates today, nine after. Two are new; none is renamed, so no path outside the moved files
changes.

| Crate | Layer | Holds | May depend on | Why it is a crate |
|---|---|---|---|---|
| `tack-protocol` **(new)** | contract | the runner-v1 types (`execution/`), `lifecycle::validate_transition`, capabilities, `usage_provenance`, `mrp`, `prepare`, one `Clock`, `Measurement`; `utoipa` behind an `openapi` feature | nothing in the workspace | the server and the runner share it and nothing else; it is the trust boundary |
| `tack-core` | domain | planning (`models`, `workflow`, `vocabulary`, `dependency`, `outline`, `recurrence`, `brief`, `rich_text`) and runs policy (`autonomy`, `run_defaults`, `auto_start`, `rate_card`), pure | `tack-protocol` | pure logic with one dependency set |
| `tack-orch` | domain + jobs | scheduler, model policy (pure); retention and observability ticks (I/O) | protocol, core, db | unchanged in this phase except what leaves for `protocol` |
| `tack-db` | adapter | every `sqlx` line in the server; per-aggregate modules; `queries/` for read models; `StoreError`; migrations | protocol, core | the only crate allowed to name `sqlx` |
| `tack-app` **(new)** | application | the runs use cases (`StartRun`, `ClaimAttempt`, `ReportEvents`, `Decide`, `Complete`), the ticks (`merge`, `auto_start`, `github_sync`) as `async fn tick()`, the ports `RunStore` and `RunnerStore`, the embedded runner's one module | protocol, core, orch, db (for the SQLite impls of its ports) | the only place that composes planning and runs |
| `tack-api` | adapter | axum routes, auth, websocket, OpenAPI, DTOs, `ApiError` | app, protocol, core | the only crate allowed to name `axum` |
| `tack-runner` | process | unchanged | protocol, core | never `tack-db` again |
| `tack-cli` | binary | `tack serve` composition (spawns the ticks), subcommands, MCP | api, app, runner, orch | the composition root |
| `tack-test-support` | test | SQLite pool + seeds (feature `sqlite`), the in-memory fakes of the ports, `store_contract!` | protocol, core, db (optional) | shared by every crate's tests |

Allowed edges after the phase (this is the table `check-crate-graph.sh` ends with):

```
tack-protocol  -> (none)
tack-core      -> tack-protocol
tack-db        -> tack-protocol tack-core
tack-orch      -> tack-protocol tack-core tack-db
tack-app       -> tack-protocol tack-core tack-orch tack-db
tack-api       -> tack-protocol tack-core tack-app tack-orch
tack-runner    -> tack-protocol tack-core
tack-cli       -> tack-api tack-app tack-runner tack-orch tack-protocol tack-core
tack-test-support -> tack-protocol tack-core tack-db(optional)
```

Not in this phase, on purpose: a `tack-client` crate (the CLI's `reqwest` client works), an
`integrations` crate (GitHub, Linear, S3 stay as modules in `tack-app`'s tick files or `tack-api`),
a split of the runner into engine and adapters (it already has its ports), migrations as `.sql`
files (cosmetic, with checksum risk), and dropping the mirrored request state in
`execution_requests` (a schema change; phase 2's typed enum will show whether it ever diverges,
and that decides it).

## 4. The phases

Each phase is one PR into `develop` and one run of `.githooks/pre-push` on its merged tree.
Phase 2 is three PRs. Do them in order; each one's "expected tree" is the next one's starting
point.

---

### Phase 0 — hygiene and the tools

**Goal:** nothing structural changes; the repo stops being able to leak a token, and the graph
check exists so every later phase has something to tighten.

**Script, all of it:**

```bash
# 1. The token file is never committed.
printf '/decision-token\n' >> .gitignore
git check-ignore decision-token            # expected: prints "decision-token", exit 0

# 2. Local state leaves the root (nothing tracked moves; these are all ignored already).
mkdir -p .local && git mv -k tack.toml .local/ 2>/dev/null || true
mv tack.db tack.db-shm tack.db-wal tack.db.before-063_drop_model_profiles.sqlite logs storage .local/ 2>/dev/null || true
#    Then point whatever reads them (docs/CONFIG.md, Makefile, scripts/smoke.sh) at .local/:
grep -rn 'tack\.db\b\|/logs/\|/storage/' Makefile scripts docs/CONFIG.md CONTRIBUTING.md | grep -v target
#    expected after the edit: every hit either names .local/ or is an env-var default that
#    the operator overrides (TACK_DATABASE_URL, TACK_STORAGE_DIR).

# 3. Tools (one-time, per machine).
PATH=$HOME/.cargo/bin:$PATH cargo install ast-grep cargo-machete
ast-grep --version && cargo machete --version

# 4. The graph check.
cat > scripts/check-crate-graph.sh <<'EOF'
#!/usr/bin/env bash
# Fails when a workspace crate depends on one this table does not allow.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.."
declare -A ALLOW=(
  [tack-core]=""
  [tack-db]="tack-core tack-test-support"
  [tack-orch]="tack-core tack-db tack-test-support"
  [tack-api]="tack-core tack-db tack-orch tack-test-support"
  [tack-runner]="tack-core tack-orch"
  [tack-cli]="tack-api tack-runner tack-orch tack-test-support"
  [tack-test-support]="tack-core tack-db"
)
fail=0
while IFS=$'\t' read -r from to; do
  case " ${ALLOW[$from]-} " in *" $to "*) ;; *) echo "forbidden edge: $from -> $to"; fail=1;; esac
done < <(cargo metadata --format-version 1 --no-deps \
  | jq -r '.packages[] | .name as $n | .dependencies[] | select(.path != null) | [$n, .name] | @tsv')
exit $fail
EOF
chmod +x scripts/check-crate-graph.sh
scripts/check-crate-graph.sh                 # expected: no output, exit 0 (the table IS today's graph)

# 5. Wire it into the gate, next to the clippy line.
#    Edit .githooks/pre-push: add `scripts/check-crate-graph.sh` after `cargo clippy ...`.
```

The `ALLOW` table above is today's graph including dev-dependencies as `cargo metadata` reports
them; if the script prints an edge on first run, the edge exists and the table is wrong, not the
tree — add it, and note it in the PR.

**Model:** none.

**Tests:** none move. The graph check is a script, not a test.

**Verify:**

```bash
git check-ignore decision-token && scripts/check-crate-graph.sh && .githooks/pre-push
```

Expected: all three exit 0; `git status --porcelain` shows `.gitignore`, `scripts/`,
`.githooks/pre-push`, and the doc edits of step 2 only.

**Expected tree:** identical crates; `.local/` holds the state; `scripts/check-crate-graph.sh`
exists and runs in the hook.

---

### Phase 1 — `tack-protocol`

**Goal:** the runner-v1 contract is one crate that the server and the runner both depend on;
the runner stops compiling SQLite; the three server clocks become one; the eighteen hand-kept
mirror types go away because the real types can derive `ToSchema`.

**What moves (nothing is rewritten):**

| From | To |
|---|---|
| `crates/tack-orch/src/execution/` (whole directory: `mod.rs`, `types.rs`, `types/tests.rs`, `capabilities.rs`, `capabilities/tests.rs`, `lifecycle.rs`) | `crates/tack-protocol/src/execution/` |
| `crates/tack-orch/src/usage_provenance.rs` + `usage_provenance/tests.rs` | `crates/tack-protocol/src/` |
| `crates/tack-core/src/mrp.rs` | `crates/tack-protocol/src/mrp.rs` |
| `crates/tack-core/src/prepare.rs` + `prepare/tests.rs` | `crates/tack-protocol/src/` |
| `crates/tack-orch/tests/runner_contract.rs` + `runner_contract/` | `crates/tack-protocol/tests/` |
| `crates/tack-core/tests/mrp_contract.rs` | `crates/tack-protocol/tests/` |

What stays in `tack-orch`: `scheduler/`, `model_policy/`, `execution_retention.rs`,
`execution_observability.rs`, `tests/scheduling/`, `tests/ingestion/`.

**Script:**

```bash
set -euo pipefail
export PATH=$HOME/.cargo/bin:$PATH
P=crates/tack-protocol

# 1. The crate.
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
#    Add "crates/tack-protocol" to [workspace].members in the root Cargo.toml (first in the list).

# 2. The moves (tests travel with their files).
git mv crates/tack-orch/src/execution $P/src/execution
git mv crates/tack-orch/src/usage_provenance.rs $P/src/usage_provenance.rs
git mv crates/tack-orch/src/usage_provenance $P/src/usage_provenance
git mv crates/tack-core/src/mrp.rs $P/src/mrp.rs
git mv crates/tack-core/src/prepare.rs $P/src/prepare.rs
git mv crates/tack-core/src/prepare $P/src/prepare
git mv crates/tack-orch/tests/runner_contract.rs $P/tests/runner_contract.rs
git mv crates/tack-orch/tests/runner_contract $P/tests/runner_contract
git mv crates/tack-core/tests/mrp_contract.rs $P/tests/mrp_contract.rs

# 3. lib.rs of the new crate.
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

# 4. Paths. Every rename is an identifier no other crate uses, so sed is safe here.
#    (ast-grep is the tool if any rename below turns out to be ambiguous.)
files=$(git ls-files 'crates/**/*.rs')
sed -i \
  -e 's/\btack_orch::execution\b/tack_protocol::execution/g' \
  -e 's/\btack_orch::usage_provenance\b/tack_protocol::usage_provenance/g' \
  -e 's/\btack_core::mrp\b/tack_protocol::mrp/g' \
  -e 's/\btack_core::prepare\b/tack_protocol::prepare/g' \
  $files
#    Inside the crates that lost the modules, `crate::` paths become the new crate's:
sed -i -e 's/\bcrate::execution\b/tack_protocol::execution/g' \
       -e 's/\bcrate::usage_provenance\b/tack_protocol::usage_provenance/g' \
  $(git ls-files 'crates/tack-orch/**/*.rs')
sed -i -e 's/\bcrate::mrp\b/tack_protocol::mrp/g' -e 's/\bcrate::prepare\b/tack_protocol::prepare/g' \
  $(git ls-files 'crates/tack-core/**/*.rs')
#    Inside the moved files, `super::`/`crate::` that pointed at tack-orch/tack-core siblings:
grep -rn 'crate::\|super::' $P/src $P/tests | grep -v 'crate::execution\|crate::usage_provenance\|crate::mrp\|crate::prepare\|crate::clock\|super::types\|super::capabilities\|super::lifecycle' || true
#    expected: empty. Anything printed is a dependency the measurement missed — fix by hand, and
#    if it points at tack-core/tack-db, the module does not belong in protocol; stop and report.

# 5. Module declarations: remove the moved `pub mod` lines, add the dependency.
sed -i '/^pub mod execution;$/d;/^pub mod usage_provenance;$/d' crates/tack-orch/src/lib.rs
sed -i '/^pub mod mrp;$/d;/^pub mod prepare;$/d' crates/tack-core/src/lib.rs
for c in tack-core tack-db tack-orch tack-api tack-runner tack-cli tack-test-support; do
  grep -q 'tack-protocol' crates/$c/Cargo.toml || \
  sed -i '0,/^\[dependencies\]$/s//[dependencies]\ntack-protocol = { path = "..\/tack-protocol" }/' crates/$c/Cargo.toml
done
#    tack-api needs the feature: tack-protocol = { path = "../tack-protocol", features = ["openapi"] }
#    tack-runner drops tack-orch entirely:
sed -i '/^tack-orch = /d' crates/tack-runner/Cargo.toml

# 6. The three clocks become tack_protocol::Clock.
sed -i -e 's/\bExecutionClock\b/Clock/g' -e 's/\bSystemExecutionClock\b/SystemClock/g' $files
sed -i -e 's/\bRetentionClock\b/Clock/g' -e 's/\bObservabilityClock\b/Clock/g' $files
#    then delete the three trait definitions and their SystemClock impls by hand (they are 6–8
#    lines each, in db/repo/execution.rs, orch/execution_retention.rs, orch/execution_observability.rs)
#    and `use tack_protocol::{Clock, SystemClock};` where the compiler asks.
#    The runner's `Clock` (SystemTime) is a different type and stays; it is not in this list.

# 7. Let the compiler list what is left.
cargo check --workspace --all-targets 2>&1 | grep -E '^(error|warning: unused)' | sort | uniq -c
```

Step 7's output is the model step's input. Expected: a short list — missing `use` lines, a
`FakeClock`/`FixedExecutionClock` in a test that implemented the old trait name, and the
`ToSchema` references. If it lists more than ~30 errors, something in steps 4–6 misfired: `git
stash`, re-read the sed output, retry.

**Model (one agent, scope closed by step 7's list):**

1. Fix every `cargo check` error the list names, adding `use` lines and renaming the test clocks.
   No new logic.
2. In `tack-protocol`, add `#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` to every
   type that `tack-api` currently mirrors "for documentation only". The list of mirrors is in
   `crates/tack-api/src/handlers/executions.rs` (`RunnerV1ErrorEnvelope`, `RunnerV1Error`,
   `MeasurementSourceSchema`, `UsdMeasurementSchema`, `RunnerTimeCostSchema`,
   `UsageEconomicsSchema`, `ModelProvenanceSchema`) and whatever
   `grep -rn 'doc-only mirror\|hand-verified mirror\|field-for-field' crates/tack-api/src` adds.
3. Delete each mirror and point its `#[utoipa::path(... body = ...)]` or `components(schemas(...))`
   reference at the real type. `cargo check -p tack-api --features` is not needed: `tack-api`
   enables the feature in its `Cargo.toml`.
4. `crates/tack-orch/src/execution_observability.rs` and `execution_retention.rs` each keep one
   `From<tack_db::...Row>` mirror (`ExecutionFleetSnapshot`, `PurgeOutcome`). These stay: they
   are the seam between the store and the tick, and phase 3 moves them. Do not touch them.
5. Regenerate: `./scripts/regen-generated.sh`. `git diff --stat docs/openapi.json` should show
   only schema *names* changing (the mirrors' names become the real types' names) — if a field
   appears or disappears, the mirror was not faithful; report it, do not paper over it.

Done when: `cargo check --workspace --all-targets` is clean and
`cargo nextest run --workspace -E 'rdeps(tack-protocol)'` passes.

**Tests:** moved with their files (table above). Deleted: none. Changed by sed only: the 11
test files that named `tack_orch::execution` paths (`grep -rl 'tack_orch::' crates/*/tests` lists
them before the sed; after it the list is the scheduling and ingestion tests only).

**Tighten:** in `scripts/check-crate-graph.sh`, `tack-runner` becomes `"tack-core tack-protocol"`;
every other row gains `tack-protocol`; `tack-protocol` gets an empty row.

**Verify:**

```bash
cargo tree -p tack-runner -e normal --prefix none | grep -cE '^(sqlx|libsqlite3-sys|tack-db|tack-orch) '
#   expected: 0
grep -rn 'trait .*Clock' crates --include='*.rs' | grep -v target
#   expected: exactly 2 lines — crates/tack-protocol/src/clock.rs and crates/tack-runner/src/clock.rs
grep -rn 'doc-only mirror\|hand-verified mirror' crates/tack-api/src | wc -l
#   expected: 0
scripts/check-crate-graph.sh && cargo nextest run --workspace -E 'rdeps(tack-protocol)'
#   expected: exit 0; the test count equals the sum of the moved files' tests plus the dependents'
#   (compare `cargo nextest list --workspace | wc -l` before and after: equal)
.githooks/pre-push
```

**Expected tree:** eight crates: `tack-protocol` exists; `tack-app` does not yet. `crates/tack-orch/src/` has `lib.rs`, `scheduler/`, `model_policy/`,
`execution_retention*`, `execution_observability*` and nothing else. `tack-runner/Cargo.toml`
names `tack-protocol` and `tack-core`, not `tack-orch`.

---

### Phase 2 — the SQL has one home (three PRs)

**Goal:** no `sqlx` outside `tack-db`; `tack-db` is organised by aggregate, with read models in
`queries/`; errors cross the boundary as `StoreError`, not `sqlx::Error`; attempt state is the
enum from `tack-protocol` everywhere.

Split by table group so each PR has one owner and one filterset. Every PR follows the same
recipe; only the list differs.

| PR | Tables | Sites to move (from the script, approx.) | Who loses them |
|---|---|---|---|
| **2a** runners | `agent_runners`, `agent_profiles`, `runner_fleets`, `fleet_members`, `enrollment_tokens` | ~27 | `runner_admin.rs` 18, `local_runner.rs` 7, `runner_auth.rs` 2 |
| **2b** runs | `execution_requests`, `execution_attempts`, `execution_events`, `execution_decisions`, `execution_artifacts`, `mrp_reviews` | ~94 | `runner_protocol.rs` 35, `executions.rs` 30, `auto_start.rs` 12, `decisions.rs` 6, `attempt_lists.rs` 5, `merge.rs` 4, `mrp.rs` 2 |
| **2c** the rest | backups, prill store, pull requests, settings, templates, github links, metrics, items, export, debug | ~69 | `remote_backup.rs` 18, `prill_store.rs` 11, `backup.rs` 10, `pull_requests.rs` 6, `debug.rs` 6, `server.rs` 3, `templates.rs` 3, `settings.rs` 3, `github_sync.rs` 3, `roles.rs` 2, `metrics.rs` 2, `items.rs` 2, `export.rs` 1, `autonomy.rs` 1 |

**Script (run once per PR; its output is the whole input of the model step):**

```bash
# The list: every sqlx site in tack-api, with verb and first table, grouped by file.
perl -0777 -ne '
  while (/\n(\d*)?.*?sqlx::(query|query_as|query_scalar)[^;]*?"\s*(SELECT|INSERT|UPDATE|DELETE|WITH)\b.*?\b(FROM|INTO|UPDATE)?\s+([a-z_]+)/gs) {
    my $pos = pos(); my $line = () = substr($_,0,$pos) =~ /\n/g;
    print "$ARGV:$line:$3:$5\n";
  }' $(git ls-files 'crates/tack-api/src/**/*.rs' | grep -v tests) \
  | sort > /tmp/sql-sites.txt
wc -l /tmp/sql-sites.txt          # expected: within ±10 of the 190 measured
grep -c ':SELECT:' /tmp/sql-sites.txt
# Filter to this PR's tables:
grep -E ':(agent_runners|agent_profiles|runner_fleets|fleet_members|enrollment_tokens)$' /tmp/sql-sites.txt > /tmp/pr-2a.txt

# For 2b only — the literal states that become the enum:
grep -rnE '"(queued|leased|preparing|running|waiting_decision|succeeded|failed|cancelled|lost|needs_operator)"' \
  crates/tack-db/src crates/tack-api/src --include='*.rs' | grep -v tests > /tmp/state-literals.txt
wc -l /tmp/state-literals.txt     # expected: ~20 + the request-state mirrors

# For 2a only — the Repository wrappers go (callers use the module functions):
grep -n 'pub async fn' crates/tack-db/src/repo.rs | sed -E 's/.*fn (\w+)\(.*/\1/' > /tmp/wrappers.txt
for w in $(cat /tmp/wrappers.txt); do
  mod=$(grep -A4 "pub async fn $w(" crates/tack-db/src/repo.rs | grep -oE '^\s+[a-z_]+::' | tr -d ' :' | head -1)
  [ -n "$mod" ] && sed -i -E "s/\brepo\.$w\(/tack_db::repo::$mod::$w(repo.pool(), /g; s/\.repo\.$w\(/.repo.pool(), /g" \
    $(git ls-files 'crates/**/*.rs')
done
# Then delete the wrapper bodies from repo.rs (keep `Repository`, `new`, `pool`) and let
# `cargo check` name any call the sed missed (a wrapper with zero args produces `pool(), )`:
# fix those by hand — they are few).
```

**The recipe the model follows, per site in its list:**

| Kind | Where it goes | How |
|---|---|---|
| `SELECT` that returns rows for a response, a list, a count or a tick's input | `crates/tack-db/src/queries/<table>.rs`, one `pub async fn` per query, returning a plain row struct (`#[derive(sqlx::FromRow)]`) that lives next to it | Move the SQL string verbatim. The function's name is the question it answers (`attempts_of_request`, `runners_with_capacity`), not the handler's name. The row struct's fields are the columns, with the column's name |
| `SELECT` that exists only to then write (read-then-write) | into the same store method as the write, in one `BEGIN IMMEDIATE` transaction | The handler's two calls become one `store.<verb>_<aggregate>(...)` |
| `INSERT` / `UPDATE` / `DELETE` | `crates/tack-db/src/repo/<aggregate>.rs`, a `pub async fn` taking `&SqlitePool` or `&mut SqliteConnection`, returning `Result<T, StoreError>` | One transaction per command; `BEGIN IMMEDIATE` when it reads first. A command's name is a verb in the aggregate's language (`revoke_runner`, `record_decision`) |
| a state string literal (2b) | `tack_protocol::execution::ExecutionState` | The store converts at its edge: `ExecutionState::from_str` on read, `.as_str()` on write, once per row, in `repo/execution.rs`. No handler sees the string |

`StoreError` (new, `crates/tack-db/src/error.rs`, written in 2a):

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

`ApiError::Database(#[from] sqlx::Error)` becomes `ApiError::Store(#[from] StoreError)` in 2a;
`NotFound` → 404, `Conflict` → 409, `Sql` → 500. The 2a PR adds `impl From<sqlx::Error> for
ApiError` **removed**, so 2b and 2c cannot compile a handler that still holds `sqlx` — the
compiler enforces the recipe.

**Traits (ports) — only two, only in 2a and 2b:** `RunnerStore` (2a) and `RunStore` (2b), as
`#[async_trait]`-free traits with `impl Future` returns, each implemented by `tack_db::Repository`
and, in `tack-test-support`, by `FakeRunnerStore` / `FakeRunStore` over a `Mutex<BTreeMap>`. A
method joins the trait only when a `tack-api` or `tack-app` test needs the fake for it; the rest
stay inherent methods on `Repository`. 2c adds no trait: planning CRUD is tested against SQLite
and that is fine.

**Tests, per PR:**

- The `tack-api/tests` files that use `sqlx::` directly (31 files, 258 sites): the script lists
  this PR's share with `grep -ln 'sqlx::' crates/tack-api/tests -r | xargs grep -l '<table>'`.
  Each site is either a **seed** (goes to a helper in `tack-test-support`, e.g.
  `seed::runner(&pool, ...)`) or an **assert over rows** (becomes a call to the new `queries::`
  function). No test keeps `sqlx::`; the PR fails `grep -rn 'sqlx::' crates/tack-api/tests` → 0.
- `store_contract!` is born in 2a, in `crates/tack-test-support/src/contract.rs`: a macro that
  takes an expression producing an `impl RunnerStore` and expands to the suite (register, revoke,
  rotate, list, token redemption, fenced claim in 2b). Instantiated twice: `crates/tack-db/tests/
  store_contract.rs` (SQLite) and `crates/tack-test-support/tests/fake_contract.rs` (the fake).
- When an instance passes, the per-method tests in `crates/tack-db/tests/repository/` that cover
  the same methods are deleted in the same PR. The PR lists them by name. The count must not
  drop: `cargo nextest list --workspace | wc -l` before minus deleted plus (2 × contract cases) ≥
  before.

**Filterset while working:** `cargo nextest run --workspace -E 'rdeps(tack-db)'`.

**Tighten (2c):** `scripts/gen-deny-toml.sh` gains, in `common_policy`:

```toml
[[bans.deny]]
crate = "sqlx"
wrappers = ["tack-db", "tack-test-support"]
```

**Verify (after 2c):**

```bash
grep -rn 'sqlx::' crates/tack-api/src crates/tack-api/tests crates/tack-orch/src crates/tack-cli/src | wc -l
#   expected: 0
grep -rnE '"(leased|preparing|waiting_decision|needs_operator)"' crates/tack-db/src crates/tack-api/src --include='*.rs' | grep -v tests | wc -l
#   expected: 0 (the only place the strings exist is ExecutionState's FromStr/as_str in tack-protocol)
grep -c 'pub async fn' crates/tack-db/src/repo.rs
#   expected: 0 (Repository keeps new() and pool() and the trait impls)
ls crates/tack-db/src/queries/      # expected: one file per table that had a read model
make deny                           # expected: exit 0 with the sqlx ban in place
cargo nextest run --workspace       # once, on the merged tree
.githooks/pre-push
```

**Expected tree:** `crates/tack-db/src/{error.rs, queries/, repo/<aggregate>.rs, migrations.rs}`;
`crates/tack-api/src/handlers/*.rs` call `state.repo.<method>` or `queries::<table>::<fn>` and
nothing else for data; `tack-test-support` has `fake/`, `seed/`, `contract.rs` and a `sqlite`
feature on by default.

---

### Phase 3 — `tack-app`: the use cases and the ticks

**Goal:** the rules of a run live in functions that take ports, not in axum handlers; the
background work is `async fn tick()` that the binary schedules; the embedded runner has one
owner.

**Script:**

```bash
A=crates/tack-app
mkdir -p $A/src $A/tests
# Cargo.toml: deps tack-protocol, tack-core, tack-orch, tack-db, tokio, tracing, chrono, uuid,
# serde, serde_json, thiserror; dev-dep tack-test-support. NO axum, NO sqlx, NO reqwest.
# Add to workspace members.

# The files that are already services, not handlers — they move as they are:
git mv crates/tack-api/src/auto_start.rs        $A/src/auto_start.rs
git mv crates/tack-api/src/autonomy.rs          $A/src/autonomy.rs
git mv crates/tack-api/src/merge.rs             $A/src/merge.rs
git mv crates/tack-api/src/execution_runtime.rs $A/src/execution_runtime.rs
git mv crates/tack-api/src/github_sync.rs       $A/src/github_sync.rs
git mv crates/tack-api/src/rate_card.rs         $A/src/rate_card.rs
#   (and each one's tests.rs sibling, if it has one: ls crates/tack-api/src/<name>/)
sed -i -e 's/\bcrate::auto_start\b/tack_app::auto_start/g' -e 's/\bcrate::autonomy\b/tack_app::autonomy/g' \
       -e 's/\bcrate::merge\b/tack_app::merge/g' -e 's/\bcrate::execution_runtime\b/tack_app::execution_runtime/g' \
       -e 's/\bcrate::github_sync\b/tack_app::github_sync/g' -e 's/\bcrate::rate_card\b/tack_app::rate_card/g' \
  $(git ls-files 'crates/tack-api/**/*.rs')
sed -i -e 's/\btack_api::auto_start\b/tack_app::auto_start/g' -e 's/\btack_api::merge\b/tack_app::merge/g' \
       -e 's/\btack_api::github_sync\b/tack_app::github_sync/g' $(git ls-files 'crates/tack-cli/**/*.rs' 'crates/tack-api/**/*.rs')
cargo check --workspace --all-targets 2>&1 | grep -E '^error' | sort | uniq -c
#   expected: every error is one of two kinds — the moved file uses `crate::AppState` (it must
#   stop: it takes its ports instead), or it uses an axum type (same). That list is the model's input.

# The spawns leave tack-api:
grep -n 'tokio::spawn' crates/tack-api/src/server.rs crates/tack-api/src/*.rs | grep -v tests
#   expected before: 13 sites; the model moves each to crates/tack-cli/src/serve.rs (new) as
#   `tokio::spawn(tack_app::<tick>::run(ports.clone(), interval))`.
```

**Model (two agents, each with its list):**

*Agent A — the protocol's use cases* (`runner_protocol.rs`, `decisions.rs`):

For each handler that does more than parse → call → map (the list is every handler in those
files longer than 25 lines; `awk` it), write one `pub async fn` in
`crates/tack-app/src/runs/<verb>.rs` that takes `&impl RunStore`, `&impl RunnerStore`,
`&impl Clock` and plain arguments, returns `Result<T, AppError>`, and holds the logic that the
handler held. The handler becomes: extract, call, `map_err`, respond. Idempotency keys, fencing
checks and `validate_transition` calls move into the use case; the HTTP status mapping stays in
`tack-api`. `AppError` (`crates/tack-app/src/error.rs`): `NotFound`, `Conflict(String)`,
`InvalidTransition { from, to }`, `StaleLease { .. }`, `Store(#[from] StoreError)`,
`Core(#[from] CoreError)`. `ApiError` gains `From<AppError>`; the mapping to the fifteen frozen
runner-v1 codes moves from the handlers into that one `impl`.

*Agent B — the operator's use cases and the ticks* (`executions.rs`, `local_runner.rs` +
`cli/local_runner.rs` + `cli/local_enrollment.rs`, the six moved tick files):

Same recipe for `create`, `create_as`, `cancel`, `requeue` in `executions.rs`. The six moved files
lose `AppState`: each `pub async fn run(...)` takes its ports and an interval and loops; each
exposes `pub async fn tick(...)` that does one pass, which is what its tests call. The embedded
runner: `crates/tack-app/src/local_runner.rs` is the one module; `tack-api`'s
`handlers/local_runner.rs` keeps only the routes, `tack-cli`'s two files keep only the clap glue
and call it. `LocalRunnerControl` (the trait) moves to `tack-app`.

Both: `crates/tack-cli/src/serve.rs` (new, ~60 lines) is the composition root: builds the
`Repository`, the ports, the `AppState`, spawns every tick, calls `tack_api::serve`.
`tack_api::serve()` no longer spawns anything; its signature gains nothing — it already takes
state.

Done when: `grep -rn 'tokio::spawn' crates/tack-api/src | grep -v tests | wc -l` → 0;
`grep -rn 'AppState' crates/tack-app | wc -l` → 0; `cargo nextest run --workspace -E
'rdeps(tack-app)'` passes.

**Tests:**

- A handler test in `tack-api/tests/handlers/` that asserts a rule (a transition denied, an
  idempotent replay, a limit reached) moves to `crates/tack-app/tests/<verb>.rs` against the
  fakes, with its HTTP scaffolding removed. One that asserts a status code, a header or a JSON
  shape stays. Each agent lists the moved tests in the PR; a test is never in both places.
- The ticks' tests (`auto_start`, `merge`, `autonomy`) move with their files and call `tick()`
  directly, with a `FixedClock`. The `tokio::time::sleep`-based waits in them, if any, go.
- `tack-cli/tests/e6_scheduler_e2e_test.rs` and `embedded_runner_state_scoping.rs` keep running
  the real server; they are the proof that `serve.rs` composes the same thing.

**Filterset:** `cargo nextest run --workspace -E 'rdeps(tack-app)'`.

**Tighten:** the graph table becomes the one in §3; `gen-deny-toml.sh` adds
`{ crate = "axum", wrappers = ["tack-api", "tack-cli"] }`.

**Verify:**

```bash
grep -rn 'tokio::spawn' crates/tack-api/src | grep -v tests | wc -l     # 0
grep -rln 'axum' crates/tack-app crates/tack-db crates/tack-orch | wc -l  # 0
wc -l crates/tack-api/src/handlers/executions.rs crates/tack-api/src/handlers/runner_protocol.rs
#   expected: each under 1 000 lines (today 3 023 and 2 910)
wc -l crates/tack-api/src/handlers/local_runner.rs crates/tack-cli/src/local_runner.rs crates/tack-cli/src/local_enrollment.rs crates/tack-app/src/local_runner.rs
#   expected: the three old files sum under 400; the logic is in the fourth
scripts/check-crate-graph.sh && make deny && cargo nextest run --workspace && .githooks/pre-push
```

**Expected tree:** nine crates. `crates/tack-app/src/{lib.rs, error.rs, ports.rs, runs/,
auto_start.rs, autonomy.rs, merge.rs, github_sync.rs, execution_runtime.rs, rate_card.rs,
local_runner.rs}`. `crates/tack-cli/src/serve.rs` exists. `tack-api/src/` has
`{config, error, middleware, openapi, router, server, webhook, debug, remote_backup, prill*,
handlers/}` and nothing that loops.

---

### Phase 4 — shape and dead weight

**Goal:** the files a person cannot read in one sitting are split along lines the code already
has; what the compiler calls unused is gone; the frontend's `shared/` only points inward.

**Script, all of it (no model):**

```bash
export PATH=$HOME/.cargo/bin:$PATH

# 1. cli/main.rs: one module per `Commands` variant group. The groups already exist as the
#    `match` arms; the script cuts at them.
grep -n '^\s*Commands::[A-Z][a-zA-Z]*' crates/tack-cli/src/main.rs | head -40
#    Read the arm names; for each group {init,export,import,projects,project,add,list,move,board,
#    branch,start,open,search,config,completions} → cmd/board.rs; {sprint,template,role,comment,
#    field} → cmd/entities.rs; {backup,backups,restore} → cmd/backup.rs; {execution,fleet,runner,
#    service,agent-profile} → cmd/runs.rs. Cut the `fn` bodies each arm calls (they are
#    free functions in main.rs) into those files with `ast-grep`:
#      ast-grep run -p 'fn $NAME($$$ARGS) -> $RET { $$$BODY }' crates/tack-cli/src/main.rs --json \
#        | jq -r '.[] | .metaVariables.single.NAME.text' > /tmp/cli-fns.txt
#    then move by name with a loop over sed ranges (`/^(async )?fn NAME\(/,/^}/`). main.rs keeps
#    `Commands`, `main`, and the match.
wc -l crates/tack-cli/src/main.rs        # expected after: under 600

# 2. runner/engine.rs: split by `impl` block (the file has distinct impls for RunnerEngine,
#    DecisionState, HarnessOutcome). Same technique; engine.rs keeps the struct, the trait and
#    `impl RunnerEngine`'s run loop; engine/decisions.rs, engine/outcome.rs, engine/resume.rs
#    take the rest. `#[path]`-style tests.rs stays one file.
wc -l crates/tack-runner/src/engine.rs   # expected after: under 1 200

# 3. Dead code: remove every allowance, let the compiler name what it hid, delete that.
sed -i '/#\[allow(dead_code)\]/d;/#!\[allow(dead_code)\]/d;/#\[allow(unused/d' $(git ls-files 'crates/**/*.rs')
cargo clippy --workspace --all-targets 2>&1 | grep -E 'warning: (function|struct|field|method|constant|enum|variant|associated).*never (used|read|constructed)' | sort -u > /tmp/dead.txt
wc -l /tmp/dead.txt
#    Delete each item named (ast-grep by name, or by hand — they are in test helpers mostly).
#    An item that is used only under cfg(test) gets `#[cfg(test)]`, not an allow.
cargo clippy --workspace --all-targets -- -D warnings      # expected: clean
grep -rc 'allow(dead_code)\|allow(unused' crates --include='*.rs' | grep -v ':0' | wc -l   # expected: 0

# 4. Unused dependencies.
cargo machete                              # delete every line it names from the Cargo.tomls
cargo machete                              # expected: "no unused dependencies"

# 5. Frontend: shared/ stops holding features.
cd frontend
git mv src/shared/runWithAgent src/features/runs
git mv src/shared/execution   src/features/runs/execution
git mv src/shared/agents      src/features/runs/agents
#    imports: every "../shared/runWithAgent" / "shared/execution" / "shared/agents" path:
grep -rl "shared/runWithAgent\|shared/execution\|shared/agents" src e2e | xargs sed -i \
  -e 's#shared/runWithAgent#features/runs#g' -e 's#shared/execution#features/runs/execution#g' -e 's#shared/agents#features/runs/agents#g'
#    relative imports from inside the moved folders break; `npx tsc --noEmit` lists them:
npx tsc --noEmit 2>&1 | grep -c 'error TS2307'      # fix each; expected after: 0
#    The three shared files that imported features (itemEvents.ts, executionContext.tsx,
#    FirstRunBanner.tsx): FirstRunBanner moved with agents/; the other two now import from
#    features/runs — move them into features/runs too, or invert the import by passing the value
#    in. `npx tsc --noEmit` is the arbiter.

# 6. The architecture test grows two cases (edit src/architecture.test.ts):
#    - no shared/* file imports from features/*  (same walk, inverted)
#    - imports via the '@/' alias count (resolve the alias before featureOf())
npm test                                            # expected: both new cases pass
```

**Model:** none. Anything in steps 1–2 that the sed-range cut cannot move cleanly (a function
whose body contains a bare `}` at column 0 — rare, rustfmt forbids it) is left where it is and
noted.

**Tests:** none move except the frontend files, which move with their folders. Deleted: whatever
step 3 names (an unused test helper is dead code).

**Verify:**

```bash
grep -rc 'allow(dead_code)\|allow(unused' crates --include='*.rs' | grep -v ':0' | wc -l   # 0
cargo machete                                                                              # nothing
wc -l crates/tack-cli/src/main.rs crates/tack-runner/src/engine.rs                        # < 600, < 1200
grep -rl "features/" frontend/src/shared --include='*.ts*' | wc -l                        # 0
du -sh frontend/src/shared/runWithAgent 2>&1 | grep -c 'No such'                           # 1
cargo nextest run --workspace && (cd frontend && npm test && npx playwright test) && .githooks/pre-push
```

---

### Phase 5 — the documents say what the tree says

**Goal:** `ARCHITECTURE.md` and the Developer Guide describe nine crates and the edges the script
enforces, with no hand-kept number.

**Script:**

```bash
# A generator for the numbers ARCHITECTURE.md quotes today by hand.
cat > scripts/arch-facts.sh <<'EOF'
#!/usr/bin/env bash
set -euo pipefail; cd "$(dirname "$0")/.."
echo "crates: $(cargo metadata --format-version 1 --no-deps | jq '.packages | length')"
echo "routes: $(python3 -c "import json; print(len(json.load(open('docs/openapi.json'))['paths']))")"
echo "migrations: $(grep -oE '"[0-9]{3}_[a-zA-Z0-9_]+"' crates/tack-db/src/migrations.rs | sort -u | wc -l)"
echo "tests: $(cargo nextest list --workspace 2>/dev/null | grep -c '^\s')"
echo "edges:"; scripts/check-crate-graph.sh --print 2>/dev/null || true
EOF
chmod +x scripts/arch-facts.sh && scripts/arch-facts.sh
```

**Model (one agent, one pass):** rewrite the "Project structure" and "Crate Boundaries &
Responsibilities" sections of `docs/ARCHITECTURE.md` from §3 of this plan and the script's
output; replace every literal count with "see `scripts/arch-facts.sh`"; rewrite
`docs/book/src/developer/crate-tour.md`'s crate list to match (the per-file walkthrough it
carries is updated only for files that moved; `git log --diff-filter=R --name-status` since the
phase's start lists them). Delete the sentence that says `tack-orch` "depends inward on
`tack-core`/`tack-db`". Add the `[Unreleased]` changelog entry for the phase as a whole.

**Verify:** `scripts/check-comments.sh`, the book builds (`mdbook build docs/book`), and
`grep -n '92 migrations\|96 documented paths\|100% complete' docs/ARCHITECTURE.md` → 0 hits.

---

## 5. The run sheet

| Phase | PRs | Script share | Model agents | Working filterset | Gate |
|---|---|---|---|---|---|
| 0 | 1 | 100 % | 0 | `package(tack-core)` | `pre-push` |
| 1 | 1 | ~90 % | 1 (compiler's list + mirrors) | `rdeps(tack-protocol)` | `pre-push` |
| 2 | 3 | ~40 % | 3 (one per table group, one list each) | `rdeps(tack-db)` | full suite once after 2c, then `pre-push` |
| 3 | 1 | ~30 % | 2 (protocol use cases; operator use cases + ticks) | `rdeps(tack-app)` | full suite, `pre-push` |
| 4 | 1 | 100 % | 0 | `rdeps(tack-cli)` + `npm test` | full suite, playwright, `pre-push` |
| 5 | 1 | ~50 % | 1 | — | `check-comments`, `mdbook build` |

Six model agents in all, each with a list it did not make and a command that says when it is
done. Everything else is `git mv`, `sed`, `ast-grep`, `cargo check` and the hook.

## 6. What can go wrong, and what to do

| Symptom | Meaning | Do |
|---|---|---|
| Phase 1 step 7 prints more than ~30 errors | a sed hit something it should not have | `git checkout -- crates && git clean -fd crates/tack-protocol`, re-run steps 1–6 one at a time, `cargo check` after each |
| A moved module needs `tack_core::models` | it is not protocol; it is domain | leave it in `tack-core`; `tack-protocol` must never depend on `tack-core` |
| `docs/openapi.json` diff shows a field appearing or vanishing in phase 1 | a hand-kept mirror was not faithful to the real type | stop; report which field; the maintainer decides which side was right |
| A `tack-db/tests` file cannot be deleted because the contract does not cover one of its cases | the contract is incomplete | add the case to the macro (both instances get it), then delete |
| `cargo nextest list` count drops across a PR by more than the deleted tests it names | a test got lost in a move | `git diff --stat -M` finds the file; it must show as a rename (`R`), not `D` + `A` |
| A phase-3 use case needs `axum::http::StatusCode` | the HTTP mapping leaked into the rule | the use case returns `AppError`; the status is `ApiError`'s business |
| `cargo machete` names a dependency that `cfg(feature)` uses | false positive | add it to `[package.metadata.cargo-machete] ignored` with the feature's name |
