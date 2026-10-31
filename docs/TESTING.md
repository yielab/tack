# Tack Testing Guide

Every Rust test runs with one command and needs no external service:

```bash
cargo nextest run --workspace
```

The summary line says how many ran — `N tests run: N passed, M skipped`. The skipped ones
are `#[ignore]`d on purpose: the perf test and the live-harness runner tests, which bill a
real agent account. `cargo nextest` is not part of cargo; install it once with
`cargo install cargo-nextest --locked`, or take the prebuilt from <https://get.nexte.st>.
`tack-desktop` is a workspace of its own: `cd crates/tack-desktop && cargo test` (what CI runs).

Frontend: `cd frontend && npm test` (Vitest; shared test helpers — `flush`, `mountProjectView` for a project view over stubbed items — are in `frontend/src/test/`). Browser E2E: `make e2e`.

A count appears in this guide only next to the command that produces it, and — like any
load-bearing number quoted anywhere in this repository — gets re-measured before it is
repeated rather than copied from the last time someone ran it. A number that drops between
two runs on the same branch is a finding — a suite silently stopped running — even when
everything passes.

## Quick start

```bash
cargo nextest run --workspace                                   # everything
cargo nextest run --workspace -E 'package(tack-db)'             # one crate
cargo nextest run --workspace -E 'test(/^runner_contract::/)'    # one file of a crate's tests
cargo nextest run --workspace -E 'test(/fencing/)'              # tests whose name matches a regex
cargo nextest run --workspace -E 'rdeps(tack-core)'             # everything a core change can reach
cargo nextest run --workspace --no-capture -E 'test(<name>)'    # see println!/tracing output (runs serially)
cargo nextest run --workspace --run-ignored ignored-only -E 'package(tack-db)'   # the perf test (50k items, p95 < 100 ms)
cargo nextest list --workspace                                  # what would run, without running it
```

Filtersets: `cargo nextest run --help` and <https://nexte.st/docs/filtersets/>.
`make test`, `make test-core` and `make test-db` wrap the first two.

## Rules

**Always `--workspace`; select with `-E`, never with `-p`.** `cargo test -p tack-api` and
`cargo test --workspace` resolve dependency features differently, so `target/` keeps two
copies of every tack crate and each source change is compiled once per form you use. The `-E`
filter selects what *runs*; the build is the workspace's either way, and that is the point.
To see it: `cargo test --workspace --no-run && cargo test -p tack-api --no-run`, and count the
`Compiling` lines of the second.

**Never read a green run's output.** `.config/nextest.toml` prints failures and one summary
line, so a green run is a few lines. `cargo test --workspace` prints one line per passing test.
Compare `cargo test --workspace 2>&1 | wc -c` with `cargo nextest run --workspace 2>&1 | wc -c`.
`cargo test` still works, but it is not what CI or `make test` call, and nothing in this
repository should tell anyone to run it.

**A test's temporary paths come from a guard.** Ask `tempfile` for the directory and hold
the guard; never build a path with `env::temp_dir().join(...)`:

```rust
let dir = tempfile::tempdir().expect("temporary directory");
let db_path = dir.path().join("subject.db");   // -wal, -shm and the migration
                                               // runner's snapshot land beside it
```

The guard removes the directory when it drops, so a panicking test cleans up too, and it
removes *everything* inside, so nothing has to be named: the migration runner writes a
`.before-<migration>.sqlite` snapshot next to a file-backed database, and a hand-named
cleanup misses it. To check that a run leaves nothing behind:
`touch /tmp/mark && cargo nextest run --workspace && find /tmp -maxdepth 1 -newer /tmp/mark | wc -l`.

The one thing the compiler will not catch: a helper that builds the directory and returns
only a path deletes it as it returns. Return the guard alongside — `(Repository, TempDir)` —
or take the directory as a parameter. `clippy.toml`'s `disallowed-methods` rejects a bare
`std::env::temp_dir()` anywhere in the workspace, test or production; production code that
legitimately needs the OS temp directory carries its own `#[allow(clippy::disallowed_methods)]`
with a one-line reason instead of being exempted by scope.

## Where the tests live

A behaviour is tested once, at the lowest layer that can express it (ADR 0010): pure unit
tests in `tack-core`, repository tests against SQLite in `tack-db`, one HTTP test per route
outcome in `tack-api`, contract tests for runner-v1 and the OpenAPI spec, fake-harness tests
in the runner, Playwright for critical journeys. An invariant is pinned at its lowest layer
plus at most one test through the HTTP API, besides the contract fixtures. A test that proves
how a change was built rather than what the product does — a narrative test asserting several
unrelated claims, a test of a private helper, a near-copy of another layer's test — is deleted
once its real claim has a home elsewhere.

| Crate | Where | Harness | What belongs here |
|---|---|---|---|
| `tack-core` | `#[cfg(test)]` next to the code, or `<module>/tests.rs` past 150 lines | plain `#[test]`; the crate has no I/O | business rules — a rule that can be tested without a database is tested here, not above (merge policy, the profile a run gets, which tasks start, recurrence, the project file's `check` and `to_bundle`), as table tests |
| `tack-db` | `crates/tack-db/tests/` | `common::setup_test_db()`: a fresh `sqlite::memory:` pool with every migration applied, per test | repository round-trips, migrations, FTS, cascades. **Locking claims need a file-backed DB** — the in-memory harness masks races. `tests/fixtures/tack-0.1.0-beta.9.sqlite` is a database written by that release (provenance in the README beside it): `migrations/upgrade_from_beta9.rs` migrates it to now and compares its schema with a fresh one |
| `tack-protocol` | `#[cfg(test)]` (or `<module>/tests.rs`) and `crates/tack-protocol/tests/` | `runner_contract` byte-pins `docs/contracts/runner-v1/`; `mrp_contract` pins the merge-readiness pack | the runner-v1 contract: capability negotiation, request/attempt lifecycle, usage provenance, the pack. **`docs/contracts/runner-v1/` fixtures outrank any Rust/TS type** — a fixture edit updates the pin table in `tests/runner_contract/fixtures.rs` in the same change |
| `tack-orch` | `#[cfg(test)]` (or `<module>/tests.rs`) | plain `#[test]`; the crate has no database and no clock | the scheduler and the model policy, as tables of cases over plain values |
| `tack-app` | `crates/tack-app/tests/` (one file per use case; `scheduling/`, `ingestion/`) | `tack_test_support::setup_test_db()` and a `ControllableClock`; a fake `Host` for the ticks | a rule of a run: call the use case or a `tick()` directly on a real store and read the result with `tack_db::queries::` — no router, no sleeping. Plant rows with `tack_test_support::seed` |
| `tack-api` | `crates/tack-api/tests/` | `common::test_app()`, `test_app_with_config()`, `test_app_with_file_db()`: a wired router over an in-memory DB, driven with `tower::ServiceExt::oneshot` — no port | status codes, response shapes, auth surfaces, wiring that proves a handler is reachable. The project file's routes are `tests/handlers/bundle.rs` (it builds its own app, since `test_app()` seeds no built-in profiles or templates) |
| `tack-runner` | mostly `#[cfg(test)]` (or `<module>/tests.rs`); `crates/tack-runner/tests/` | `src/harness/fixtures/fake_harness.sh`, captured vendor transcripts under `fixtures/<kind>/<version>/`, the crash matrix | credential handling, journal, subprocess boundary — see below |
| `tack-cli` | `crates/tack-cli/tests/` | `wiremock` stubs the API; the scheduler and embedded-runner tests spawn a real `tack serve` on port 0 and read the port it logs (`common::spawn_serve`), so they share nothing and run alongside everything else | request shaping, error surfacing, the end-to-end scheduler path |
| `tack-desktop` | `crates/tack-desktop/tests/` | `cargo test`, in its own workspace | `dependency_boundary.rs` proves `tack-cli` never links a webview |

**The runner's harnesses.** The harness lifecycle is proved once, in
`harness/local_process/tests.rs`, through a grammar that adds nothing: spawn, environment,
secrets, redaction, probe, cancel and reconcile against `fixtures/fake_harness.sh`. A harness's
own tests are pure — a request in, a command line out; a captured transcript in, a report out —
and never spawn a process to re-prove the core. A rule that binds requests goes in the core so
it binds every harness; a policy a harness cannot enforce is declared in its
`permission_policy` capability, never silently ignored. Vendor output is a file under
`fixtures/<kind>/<version>/`, its provenance (captured or constructed) stated beside it.
Live-harness tests are `#[ignore]` and billed.

docket is a daemon, not a process. Its adapter is proved against `tests/fake_docket/`, an
in-process fake of docket's API that answers each route from a file under `fixtures/docket/v5/`
(a capture from a real docket, or an answer marked `constructed` because a model would have to
run for docket to send it) and answers `400` to a request key no captured request has.
`scripts/capture-docket-fixtures.sh` records the captures from a docket started in a throwaway
home with no model (rerun it when docket changes, and commit the diff). `tests/docket_v5.rs`
fails when a route the client sends has no capture, and the daemon's supervision is proved
against a shell script on a scratch path that serves `/health`. No test runs a real docket or a
model.

Each crate has **one integration-test binary**, `it` (ADR 0029): `tests/it.rs` declares every
file under `tests/` as a module, so the dependency graph is linked once per crate, not once per
file. A file is selected by its module, `-E 'test(/^<module>::/)'`. **Add a test to the existing
module whose subject fits; a new subject is a new `mod` line in `tests/it.rs`.** Helpers shared by
several modules (`common`, and in some crates `support` or `fake_docket`) are declared once in
`it.rs` and reached as `crate::common`.

```
crates/<crate>/
  src/<module>.rs              production; a trailing `mod tests` of ≤ 150 lines may stay
  src/<module>/tests.rs        that module's unit tests once they outgrow 150 lines
                               (`#[cfg(test)] mod tests;` — `use super::*` still works)
  tests/common/mod.rs          the crate's shared fixtures — the only place a helper lives
  tests/it.rs                  the crate's one test binary: a `mod` line per subject
  tests/<subject>.rs + dir     one module per subject
  tests/live/                  real binaries or billed runs, #[ignore]d
  tests/scratch_*.rs           gitignored: your local proof, never tracked
crates/tack-test-support/      fixtures for the layers below the API
```

An integration test imports the crate (`use tack_api::handlers::decisions;`). It never pulls
a source file in with `#[path]`: that compiles a second copy of the module into the test
binary, the tests pass against the copy, and coverage counts nothing for the real one.

## Conventions

Size is held by standard lints, not a custom ratchet (ADR 0010): `cargo clippy --workspace
--all-targets -- -D warnings` catches an oversized function against `clippy.toml`'s
`too-many-lines-threshold` and a disallowed hand-built temp path; rustfmt carries formatting.
Review carries the rest.

- One claim per test; variants are rows of a table-driven test. The name states the claim, with
  no articles or narrative.
- An invariant is pinned at the repository and at one router-level test, not a third time.
- A trailing `#[cfg(test)] mod tests` in a production file moves to `<module>/tests.rs` once it
  crowds the file.
- No fixed waits. Poll a condition with a bound, or pause time; never `sleep`.
- A test that early-returns on an env var belongs under `tests/live/`, `#[ignore]`d.
- `assert!(matches!(..))` for enum variants; `#[tokio::test]` for async.
- A test of "writes nothing" or "rejects before X" asserts the absence directly (row counts, an
  untouched checkpoint) and proves itself load-bearing by reverting the fix once.
- A flaky test is recorded, never retried into green (`retries = 0` in the nextest config).

A handler test with the shared harness:

```rust
#[tokio::test]
async fn health_returns_ok() {
    let (app, _workspace) = common::test_app().await;
    let res = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

## Contract and regeneration gates

Two tests guard artifacts that are committed rather than computed. They run inside the
full suite; the first also *rewrites* the artifact when asked, and CI fails when that
rewrite differs from what is committed:

```bash
UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary_id(tack-api::it) & test(/^openapi_contract::/)' && git diff --exit-code docs/openapi.json
cargo nextest run --workspace -E 'binary_id(tack-protocol::it) & test(/^runner_contract::/)'   # never regenerated: the fixtures are the authority
```

Three more contracts under `docs/contracts/` are held the same way, each by tests that read
its committed example: `brief-v1` (`crates/tack-core/src/brief/tests.rs`), `mrp-v1`
(`crates/tack-protocol/tests/mrp_contract.rs`, with its fixtures) and `evidence-v1`
(`crates/tack-runner/tests/evidence_contract.rs`, which also pins the example's bytes). Change
the example first, then the type.

`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are generated;
`./scripts/regen-generated.sh` does both plus the lockfiles. Never hand-edit or hand-merge
them.

With the embedded SPA:

```bash
cd frontend && npm run build && cd ..
cargo nextest run -p tack-api --features embed-spa    # -p on purpose: a feature build is its own resolution anyway
```

## Continuous integration

`.github/workflows/ci.yml` runs in three tiers, by trigger, in one file:

| Tier | Trigger | Jobs |
|---|---|---|
| Pull request | every push (`main`, `develop`, `claude/**`) and every pull request | `rust`, `coverage`, `frontend`, `docs`, `deny`, `security`, `security-boundaries` |
| Merge | push to `develop` or `main` | adds `embed-spa`, `desktop`, `e2e` (`scripts/smoke.sh` in fake mode, then Chromium only) |
| Schedule | weekly cron, or by hand (`workflow_dispatch` runs every job in every tier) | `msrv`, `mutants`, `e2e` (smoke, then all three browsers) |

| Job | What it runs |
|---|---|
| `rust` | `scripts/check-comments.sh` → `cargo fmt --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"` → the OpenAPI regenerate-and-diff gate |
| `coverage` | `cargo llvm-cov nextest --workspace --lcov --output-path lcov.info --fail-under-lines 78.93` — the one run of the whole suite, instrumented. On a pull request, `diff-cover lcov.info --compare-branch=origin/<base> --fail-under=80` additionally requires 80% coverage of the lines the pull request itself changes; `lcov.info` is uploaded as an artifact either way |
| `frontend` | schema drift, type-check, Vitest with coverage thresholds (70% lines/functions/statements, 60% branches), token lint, build, entry-bundle budget (30 KB gzipped) |
| `docs` | `mdbook build` + offline link check |
| `deny` | `cargo deny check` for the root workspace and for `tack-desktop` |
| `security` | `cargo audit` for both workspaces + `npm audit` |
| `security-boundaries` | the boundary suites; see below |
| `embed-spa` | clippy and the `tack-api` tests with the SPA embedded, release build, binary-size budgets (25 MB for `tack`, 8 MB for `tack-runner`) |
| `desktop` | fmt, clippy, `cargo test` in the `tack-desktop` workspace |
| `e2e` | `scripts/smoke.sh` in fake mode against the binaries the job built, then Playwright, the a11y scan and the API contract — Chromium only on a merge, all three browsers on the schedule run and on a manual dispatch |
| `msrv` | `cargo build --workspace --locked` on the pinned minimum Rust version |
| `mutants` | `cargo-mutants` over `tack-core` and `tack-db`'s repository layer. Report-only: surviving mutants are listed in the job summary and the `mutants-*` artifacts, and never fail the build. Read it when deciding where the next test goes — a survivor is a line a test runs but would not miss |

The suite runs exactly once per applicable trigger, in the `coverage` job. The `rust` job's
last step deliberately runs one test a second time: the OpenAPI contract test is re-invoked
with `UPDATE_OPENAPI=1`, which regenerates `docs/openapi.json` from the current code, and the
step diffs the result against what is committed. `CARGO_INCREMENTAL=0` throughout: CI never
reuses incremental state.

### Pre-push hook

`scripts/setup-git.sh` (or `git config core.hooksPath .githooks`) activates it. It checks that
`cargo` is the one `rust-toolchain.toml` pins, then runs, in order:

1. `cargo fmt --all --check`, and separately for `crates/tack-desktop` (its own workspace;
   nothing else local sees that crate);
2. `cargo clippy --workspace --all-targets -- -D warnings`;
3. `scripts/check-crate-graph.sh`, which fails when a crate depends on one its table does not allow;
4. the four boundary suites below (seconds, once built);
5. the generated files: both lockfiles are current, and the book's `developer/api-reference.md`
   matches what `scripts/gen-api-reference.py` writes;
6. when `frontend/node_modules` exists: `schema.gen.ts` is current, `tsc -b` passes, and
   `vitest run --changed` runs against what the push sends (each pushed ref against the commit
   the remote has, or against where a new branch left `develop`).

It does **not** run the rest of the test suite, on purpose: a hook that takes a minute is a hook
people bypass, and the suite is CI's job. A checkout that has never run `npm install` gets no
local protection against schema drift; CI's `frontend` job still catches it. Run
`cargo nextest run --workspace` yourself before pushing anything you claim is green.

## The boundary suites are required

Four test modules are required checks. The CI job `security-boundaries` runs them on every
push and pull request, and the pre-push gate runs the same filterset after clippy:

```bash
cargo nextest run --workspace -E '(binary_id(tack-api::it) & test(/^security::/)) | (binary_id(tack-runner::it) & test(/^(agent_boundaries|h3_checkout|crash_matrix)::/))'
```

- `security` (`crates/tack-api/tests/security.rs`, module `trust_boundary`) is the server's trust
  boundary: who may call the API, from which origin, and what the browser is allowed to run.
- `agent_boundaries` (`crates/tack-runner/tests/agent_boundaries.rs`) is the runner's boundary:
  the sandbox, the environment policy, which secrets a request may name, and the workspace. CI
  sets `TACK_REQUIRE_BWRAP=1` so the real-sandbox test fails, rather than skips, when `bwrap` is
  missing.
- `h3_checkout` and `crash_matrix` (`crates/tack-runner/tests/`) hold the runner's checkout and
  crash-recovery rules.

**A rule without a test in one of these suites is not a rule.** A new server rule gets its test
in `trust_boundary.rs`; a new runner rule gets its test in `agent_boundaries.rs`, named after
the rule.

## Coverage

```bash
make coverage   # CI's coverage floors locally: one instrumented workspace run + Vitest thresholds
```

Floors `make coverage` and CI enforce: workspace line coverage ≥ 78.93 %; frontend Vitest ≥ 70 %
lines/functions/statements and ≥ 60 % branches. On a pull request CI also requires 80 % of
the changed lines to be covered (`diff-cover`).

The workspace floor is the line total the `coverage` job measured on `develop` (summed
`LH`/`LF` over its `coverage-lcov` artifact), minus one point. It is re-measured from that
artifact, never on a workstation, and moved only in `ci.yml`, the `Makefile` and here together.

For an HTML report instead of the pass/fail gate:

```bash
cargo install cargo-llvm-cov
cargo llvm-cov nextest --workspace --html --output-dir coverage/
```

## End-to-end smoke (`scripts/smoke.sh`)

A real `tack serve` and a real `tack-runner` with harness binaries, driven from claim to
requeue. Fake mode uses shim harness binaries and is free and deterministic; `--live` runs real
harness binaries and spends a real model run.

```bash
./scripts/smoke.sh          # fake mode: what CI runs
./scripts/smoke.sh --live   # real harness binaries
```

It covers the whole claim-to-completion pipeline through production routes, restart recovery
(kill the runner mid-attempt, then an explicit operator requeue), `tack serve --with-runner`,
and the refusal of `--with-runner` on a non-loopback bind. It binds ports 3399–3402. A step that
cannot pass because the product cannot do the thing is a FAIL, never a SKIP; a missing harness
binary is reported as ABSENT and never counted as a pass.

## End-to-end, accessibility and API-contract tests (Playwright)

Browser-level tests that drive the real app — the `tack-api` server plus the Vite-served SPA — in
Chromium, Firefox and WebKit. Playwright owns both server lifecycles, so one command is enough;
the API runs against a throwaway `e2e.db`, so your working database is never touched.

```bash
make e2e-install     # one-time: download the browser engines
make e2e             # run the whole suite (chromium + firefox + webkit)
make e2e-ui          # interactive runner for debugging
```

**The database is reset on every run.** The API `webServer` entry in
`frontend/playwright.config.ts` deletes `e2e.db*` and `storage-e2e/` (in that order: a runner
credential in the storage directory is tied to a runner id in the database) before it runs
`cargo run -p tack-cli -- serve`, so every invocation starts from an empty database and storage
directory. A flake rate measured against an accumulated database means nothing unless the
database's starting state is stated with it. To inspect what a run left behind, copy `e2e.db` and
`storage-e2e` aside before the next run; there is no flag to skip the reset.

**Only one instance at a time.** The API binds a fixed port (3399) and the SPA a fixed port
(5199), the same for every checkout. Locally (never in CI), `reuseExistingServer` means a second
invocation that finds something answering on that port reuses it instead of starting its own, and
the reset above only runs when a fresh server starts. Reuse is silent: nothing reports that the
server answering belongs to another checkout with a different database. If a run reports
failures that do not reproduce solo and match nothing you changed, check for another instance:

```bash
pgrep -af "playwright|tack serve"   # any other run or leftover server
ss -ltnp | grep -E '3399|5199'      # who holds this suite's ports
```

A `tack serve` on another port (3210 is the plain default) is unrelated.

E2E is for a journey that crosses surfaces — a click in the browser, the production router, the
database, and back to the screen without a reload. A rule one layer can state (which runner may
claim a request, what a route returns for bad input) is pinned in that layer and not re-proved
through a browser. The suite, in `frontend/e2e/`:

| Spec | Journey |
| --- | --- |
| `smoke.spec.ts` | Every primary surface renders without a blank screen or a page error |
| `journey.spec.ts`, `table.spec.ts` | An item is created, appears on the board and in the list, opens with its title, and an inline edit persists; the drawer's Expand opens the item's page with its breadcrumb, properties, tabs and Copy link |
| `brief.spec.ts` | A checklist line and a machine check on an item survive a reload |
| `run-with-agent.spec.ts` | "Run with agent" from the board, the item and the sprint: required fields block submit, a run is created and shows up without navigation, the project's model default is sent unchanged |
| `scheduler-e2e.spec.ts` | A request made in the UI is claimed by a runner and the UI shows it live |
| `execution-attempt-detail.spec.ts` | An attempt's events, decisions and artifacts: a pending decision resolves through the UI, an artifact downloads, a poll tick never discards typed input |
| `agents-page.spec.ts`, `execution-toggle.spec.ts` | Turning agent execution on and off, the installed agents it reveals, a test run reaching the timeline |
| `board-websocket-subprotocol.spec.ts` | A real browser WebSocket offering `tack.v1` stays connected and receives a board event — only a browser enforces that handshake rule |
| `a11y.spec.ts` | WCAG 2.0/2.1 A & AA scans via axe-core — new violations fail CI — and the board's own checks: a column of thirty cards scrolls to its last card, the horizontal bar is inside the screen, every scrollable region takes the keyboard |
| `api.spec.ts` | Wire-contract checks: health shape, hardening headers, response envelopes, 404s |
| `shared-project-identity.spec.ts` | Guards the suite itself: the shared project keeps one identity while specs run concurrently |

`helpers.ts` is the single source of truth for API response shapes and setup calls. Four more
files in the directory are recorders, not tests — `screenshots.spec.ts`, `recovery-demo.spec.ts`,
`agent-assets.spec.ts` and `tutorial-assets.spec.ts` produce the README's and the book's images
and GIFs, each through its own config. The default config ignores them and CI never runs them.

**Live docket and prill suite.** `docket-prill.spec.ts` is not versioned: it lives in the
git-ignored `onboarding-lab/e2e/` with `docket-prill-e2e.sh`, which builds `tack` with the SPA
embedded, runs docket and prill from their own source trees (`DOCKET_SRC`, `PRILL_SRC`) in a
scratch `HOME`, and drives a project through the UI the way a person would. CI never runs it
and never needs docket or prill; the three meet only here. A test that needs a model can fail on
the local model alone, so read the log before blaming Tack. The script lists its knobs at the top;
`DOCKET_PRILL_SKIP_BUILD=1` reuses the last build.

`docket-prill-e2e.sh --check` needs no model and no container. It runs
`scripts/capture-docket-fixtures.sh` against a real docket, compares the keys of docket's answers
with the fixtures under `crates/tack-runner/src/harness/fixtures/docket/`, compares
`docs/contracts/mrp-v1/fixtures/prill/` with prill's own contract fixtures, runs the
`briefing_schema` test (`#[ignore]`d in CI because it needs `prill` on `PATH`), and runs the fake
docket's tests against the captures, so a key docket sends that the fake does not, or the reverse,
fails by name. It exits non-zero when anything drifted. What the captures cannot reach stays
`constructed`: it needs a model's reply or an agent's call that leaves the machine, and each says
so in its provenance.

**A flaky E2E test.** CI retries a failed test once, only to record a trace and a video; a
test that passes on the retry still fails the run (`failOnFlakyTests`). The same day, mark it
`test.fixme()` with a one-line comment that says what was seen and links the issue, so the
rest of the suite keeps gating. Within two weeks it is fixed or deleted. Never raise
`retries` or a timeout to get to green, and never wait a fixed time — poll a condition
with a bound, as `expect(...).toBeVisible({ timeout })` does.

**Accessibility debt.** `KNOWN_ISSUES` in `a11y.spec.ts` is empty, so the axe scan gates on a
clean baseline. If a justified, hard-to-fix violation must be suppressed, add its axe rule id to
`KNOWN_ISSUES` with a tracking note rather than deleting the assertion, so the suite keeps
blocking new classes of regression.

Cross-browser coverage is the `projects` list in `frontend/playwright.config.ts`;
engine-independent specs (`a11y`, `api`) self-skip to chromium only.

## Dependency vulnerability scanning

```bash
make audit           # cargo audit (both workspaces) + npm audit --audit-level=high (frontend)
```

Runs in CI as the `security` job. [Dependabot](https://github.com/yielab/tack/blob/develop/.github/dependabot.yml)
opens grouped monthly update PRs for cargo, npm and GitHub Actions, and security updates as
advisories appear.

`npm audit` exits 1 both for a finding and when npm's advisory endpoint is down. CI tries
three times; a finding fails the job, an outage that outlasts the tries is a warning on the
run, because no commit can fix it.

Known, justified Rust advisory exceptions live in
[`.cargo/audit.toml`](https://github.com/yielab/tack/blob/develop/.cargo/audit.toml) with a documented reason each — the
gate still fails on any new advisory. Re-review that list on every dependency bump.

## Load testing (k6)

An HTTP-level load test for the performance baseline. It is not part of CI (it needs a running
server and takes time); run it on demand.

```bash
# terminal 1: a server with a throwaway DB
TACK_DATABASE_URL='sqlite:load.db?mode=rwc' cargo run -p tack-cli --release -- serve
# terminal 2: the server's token is in api-token, beside load.db
TOKEN=$(cat api-token) make load
```

It ramps to 50 virtual users on the read hot path plus a write path, asserting p95 latency and
error-rate thresholds. The write p95 threshold is where SQLite's single-writer model shows up
first. See [`tests/load/README.md`](https://github.com/yielab/tack/blob/develop/tests/load/README.md).
