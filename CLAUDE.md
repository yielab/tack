# CLAUDE.md

Guidance for Claude Code in this repository. This file is a **map, not a manual** —
deep reference lives in the docs listed at the bottom and is read on demand.

## Project Overview

**Tack** is two components built to be one product — **the board** (project manager: Rust
backend via Axum + SQLite, SolidJS frontend) and **the runner** (a small worker that executes
near your code and credentials) — shipped as a single `tack` binary where `tack serve
--with-runner` embeds a runner in the board's own process. Multiple workflows
(Scrum/Kanban/phase) with per-project vocabulary; 10 project-type presets; MCP server
(`tack mcp`). Core is complete (backend, frontend, CLI).

**Phase 64 — a codebase for human maintainers — is the live plan, and it comes before the
release tag and the publish list in `docs/LAUNCH-CHECKLIST.md`.** Its decision record is
ADR 0068 (accepted 2026-09-18, read its last amendment first); its stages are in
`docs/book/src/roadmap.md`; `docs/plans/phase-64.md` cuts them into tasks, with their order,
their files and the limits an agent works under; the harness stage's tasks are in
`docs/plans/harnesses.md`.
The goal is the smallest codebase that does everything a user can do today: what serves AI
agents as product stays (runner, harnesses, runner-v1, MCP, desktop app); what only existed
because agents built the tree goes.

**Work is no longer tracked as cards and waves.** Every board in `TODO.md` is closed
(Parts I–IX); nothing new is added to it and no new per-card handoff is written. A decision
goes in an ADR, intent goes in the roadmap, a plan for one stage goes in `docs/plans/`, and
the history of a change goes in its commit message. `TODO.md`, `docs/agent-handoffs/` and
`docs/closed-cycles/` are history: read one only when something names it. A new line of work
branches from `develop`; `README.md` and `docs/screenshots/**` are shared files.

**Never read `TODO.md` whole** (`wc -c TODO.md` ÷ 4 for today's token cost). It holds closed
boards only — Parts IX down to IV; Parts I–III are under `docs/closed-cycles/boards/` — and
Phase 64's Stage 2 archives the rest. Costs and extraction recipes for every big file:
**`.claude/context-budget.md`**. Before designing anything, read
**`.claude/scope-discipline.md`** — this tree's recurring defect is well-built mechanisms
with no caller (`model_profiles`, the `decisions` path, the superseded docket control plane).

**Skills:** `/feature` (feature work), `/gate` (scoped verification), `/status` (where am I /
what is next), `/tokens` (usage measurement vs `.claude/token-baseline.md`). `/card` and
`/integrate` drive the closed card-and-wave process and have nothing to act on; they leave
with `.claude/` in Phase 64's Stage 2. Where a skill's text tells you to write a handoff or
edit a board, this file wins: don't.

## Local Domain

Served at **https://tack.test** via the workspace-central Caddy + dnsmasq setup.
`Caddyfile.local` here is auto-imported by `~/Sites/Caddyfile`. No global `{}` block, no
separate Caddy instance, no custom ports; reload with `sudo systemctl reload caddy`.
See `~/Sites/LOCAL-DOMAINS.md`.

## Commands

```bash
cargo build                                # whole workspace compiles
cargo run -p tack-cli -- serve             # server + web UI at http://127.0.0.1:3210
cargo run -p tack-runner -- --help         # the runner is a SEPARATE binary from `tack`

cargo nextest run --workspace              # the test runner (.config/nextest.toml): ~15 s, prints failures + one summary line
cargo nextest run --workspace -E 'binary(wave2_gate)'        # select with a filterset — never `-p`: it builds a second copy of every crate
cargo nextest run --workspace -E 'binary(runner_contract)'   # byte-pins every runner-v1 fixture
cargo nextest run --workspace -E 'binary(openapi_contract)'  # spec drift gate

cd frontend && npm run dev                 # http://localhost:5173, proxies /api (start API first)
cd frontend && npm run type-check && npx vitest run
make e2e                                   # Playwright (make e2e-install once)
make audit                                 # cargo audit + npm audit

python3 scripts/maintainability.py check --changed   # size budgets for tests and comments, ratcheted against a baseline
.githooks/pre-push                         # THE definition of done — run it before saying a change is finished
```

**`.githooks/pre-push` is the gate that decides whether work can leave the machine**, and
running it directly is the only check that cannot drift from it. It covers more than the
test suite: `scripts/check-comments.sh`, `scripts/check-test-hygiene.sh`, **`cargo fmt --all
--check` for the workspace *and* for `crates/tack-desktop` separately**, `cargo clippy
--workspace --all-targets -- -D warnings`, and a freshness check on the lockfiles and
`schema.gen.ts`. A green test suite is not a finished change. Formatting in particular is
invisible until a push is attempted, so unformatted work accumulates silently across merges
and then blocks the whole branch at once — which is exactly how it has failed here before.

Live-harness runner tests are `#[ignore]` (Claude Code's is billed — run deliberately with
`--run-ignored ignored-only`). Never `cargo test --workspace`: it prints ~84k tokens for a
green run where nextest prints ~150.
Release profile optimizes for size (`lto`, `opt-level="z"`) and compiles slowly — use
`--release` sparingly. Full testing guide: `docs/TESTING.md`.

## Configuration

All `TACK_*` variables (server, runner, backup, orchestration, execution domain) are
documented in **`docs/CONFIG.md`** — the single authority for those tables. Posture
rules that bite:

- Anything that **deletes data or reaches the network** is off-by-default behind a
  `TACK_*_ENABLE` gate (`TACK_ORCH_ENABLE`, `TACK_EXECUTION_RETENTION_ENABLE`);
  read/log-only watchers may default on.
- Privileged actions carry their **own token**, distinct from `TACK_API_TOKEN`,
  fail-closed when unset (`TACK_ORCH_APPROVAL_TOKEN`, `TACK_EXECUTION_DECISION_TOKEN`).
- Secrets are write-only over the API, never logged, and every new secret column is
  added to `remote_backup.rs::scrub_snapshot_secrets` in the same commit.

## Architecture (map)

```
crates/
├── tack-core/     Pure business logic, zero I/O (models, workflow, vocabulary, DAG)
├── tack-db/       SQLite via sqlx; 62 migrations; FTS5; repository pattern in repo/
├── tack-orch/     ControlPlane trait + reconciler + neutral runner-v1 execution domain.
│                  Depends on core+db only — must NEVER depend on tack-api
├── tack-api/      Axum server (library; tack_api::serve). 97 documented paths + WebSocket
├── tack-runner/   Pull-based execution runner — separate binary; owns credentials,
│                  workspace, journal, and the harness subprocess. harness/local_process.rs
│                  is the one lifecycle; a harness is a descriptor + a 4-method grammar
└── tack-cli/      The single `tack` binary: serve + CLI client (HTTP only, never opens the DB)

frontend/          SolidJS + Tailwind v4; two-axis design tokens (mode × palette);
                   types generated from the OpenAPI spec

crates/tack-desktop/   Tauri shell that supervises `tack` as a bundled sidecar. NOT a
                   member of the workspace above — its own, via `exclude` in the root
                   Cargo.toml, because Tauri drags GTK/WebKit/glib into whatever workspace
                   holds it and the server must keep building where none of that exists.
                   `make desktop` builds it; `cargo --workspace` never sees it. Its own
                   Cargo.lock, CI job and Dependabot entry.
```

Boundary rules: **two auth surfaces, structurally separated** — operator routes under
`/api` behind `require_token`; runner routes (`/api/runner/v1`) are a sibling of `/api`
with per-handler hashed-credential auth; never route one through the other. Every
attempt-scoped mutation validates runner id + attempt id + fencing token; a stale fence
returns `stale_lease` and writes nothing (16 call sites in
`handlers/runner_protocol.rs`). **Known inconsistency (III-G2 audit, 2026-08-19,
finding F1, non-blocking):** when an attempt was superseded by a pre-spawn *recovery*,
the retried old fence returns `409 conflict` instead of `stale_lease` on heartbeat,
decisions, artifacts and the observation routes — an earlier guard fires first. Nothing
is written either way; the code is inconsistent with itself, not unsafe. Full crate detail, design patterns, and implementation
notes (workflow validation, auto-status propagation, WebSocket events, attachments…):
**`docs/ARCHITECTURE.md`**.

## Rules that bite (learned here, enforced everywhere)

- **`BEGIN IMMEDIATE` is mandatory for read-then-write transactions** — deferred ones
  deadlock under concurrency; prove concurrency tests against a file-backed DB, not the
  shared in-memory harness.
- **One `ALTER` per migration name.** The migration runner executes statements
  individually with no wrapping transaction; a multi-`ALTER` migration failing midway
  bricks the install.
- **`docs/contracts/runner-v1/` fixtures outrank any Rust/TS type.** Fixture edits update
  the pin table in `crates/tack-orch/tests/runner_contract.rs` in the same change.
- **`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are generated** —
  never hand-edit; regenerate via `UPDATE_OPENAPI=1 cargo nextest run --workspace -E
  'binary(openapi_contract)'` then `cd frontend && npm run gen:api`, or `./scripts/regen-generated.sh`
  for those plus the lockfiles. Never hand-**merge** them either: `.gitattributes` routes
  them through the `tack-generated` driver (`./scripts/setup-git.sh` registers it) and
  `post-merge` regenerates. After merging several branches, regenerate once at the end
  rather than trusting any branch's copy.
- **Unsupported is typed, unknown is explicit, unmeasured is nullable** — no
  `unimplemented!()`, no zero standing in for "unknown" (never render `$0.00` for
  unmeasured money; the literal is `Not measured`). Capability claims are load-bearing.
- **Logs carry ids only** — never credentials, prompt bodies, query strings or env
  values; tests assert the redaction.
- **A status-code assertion alone proves little.** For "writes nothing / rejects before
  X" claims, assert the absence directly (row counts, untouched checkpoint) and prove
  the test load-bearing by reverting the fix once.
- **A load-bearing number carries the command that produces it, and gets re-measured before
  it is quoted.** A claim like "234 doc comments cite this" decides things — it justified
  freezing 9k lines in place, and it was 13× wrong within two weeks of being written while
  four documents kept repeating it. If you are about to rely on a count someone else wrote,
  run its command first; if it has no command, that is the finding.
- **A test's temporary paths come from a `tempfile` guard, never from
  `env::temp_dir().join(...)`.** A hand-built path is removed by a statement at the end of
  the test, which a failing assertion skips, and the removal has to name each file, so
  whatever the code under test writes *beside* it — SQLite's `-wal`/`-shm`, the migration
  runner's pre-upgrade snapshot — is never named. Hold the guard as long as anything reads
  the path: a helper returning only the path deletes the directory as it returns, and the
  compiler will not tell you. `scripts/check-test-hygiene.sh` enforces this (~0.7s, in
  `pre-push` and CI); production code may still use the temp directory and is not scanned.
- **A behaviour is tested once, at the lowest layer that can express it** (ADR 0068
  decisions 5 and 6): pure unit tests in `tack-core`, repository tests against SQLite in
  `tack-db`, one HTTP test per route outcome in `tack-api`, contract tests for runner-v1 and
  the OpenAPI spec, fake-harness tests in the runner, Playwright for critical journeys. An
  invariant is pinned at its lowest layer plus at most one test through the HTTP API,
  besides the contract fixtures. A test that proves how a change was built rather than what
  the product does — a wave gate, a narrative test asserting unrelated claims, a test of a
  private helper, a near-copy at another layer — is deleted once its real claim has a home.
  For a security or "writes nothing" claim, revert the fix once and watch the test fail; a
  test that still passes is proving something else.
- **Harnesses: the lifecycle is tested once.** `harness/local_process/tests.rs` proves
  spawn, environment, secrets, redaction, probe, cancel and reconcile through a grammar
  that adds nothing, against `fixtures/fake_harness.sh`. A harness's own tests are pure — a
  request in, a command line out; a captured transcript in, a report out — and never spawn
  a process to re-prove the core. The one exception, per open-wire harness, is a single
  test that runs the real binary against a fake model server on loopback: it proves the
  vendor's contract, is not billed, and returns early when the binary is absent. Vendor
  output is a file under `fixtures/<kind>/<version>/`, its provenance (captured or
  constructed) stated in that directory's README. A rule that binds requests goes in
  the core so it binds every harness; a policy a harness cannot enforce is declared in its
  `permission_policy` capability, never silently ignored. Shape, rules and the order of
  work for docket and opencode: `docs/plans/harnesses.md`.
- **Test sizes are still gated, until Phase 64's Stage 2 replaces the gate with clippy
  lints.** `scripts/maintainability.py check --changed` runs in `pre-push`: a trailing
  `#[cfg(test)] mod tests` under 150 lines or moved to `<module>/tests.rs`; a test body
  ≤ 40 lines, its name ≤ 60 characters and stating the claim; a test file's preamble ≤ 10
  lines and the file ≤ 1 000; variants as rows of one table-driven test; no fixed waits; a
  live or billed test under `tests/live/` and `#[ignore]`d; helpers in `tests/common`,
  `tack-test-support` or a crate's own `test_support` module, never per file. Its
  15-tests / 600-lines ceiling per change is a tripwire, not a target: a change that
  restructures tests and trips it is reviewed on what it removed, not re-cut to fit.
  Throwaway proof goes in `crates/*/tests/scratch_*.rs`, which is gitignored.
- Changing an API response shape updates the matching frontend unit/E2E mocks in the
  same change. Frontend colors come from `--color-*` tokens only, never raw hex.

## Code style

`tracing` macros for logging; `#[instrument(skip(pool))]` on async fns; `thiserror` in
core/db, `anyhow` in CLI; `chrono::DateTime<Utc>`; UUIDv4 stored as TEXT;
`assert_matches!` in tests.

**Comments explain the code, never the project's history.** Write what the code does when
the name doesn't say it, why a non-obvious choice was made, what breaks if you change it,
and what isn't true yet (an unwired column, a mechanism with no caller). Never write card,
wave, phase or `TODO.md §` references, narratives of how the code got here, instructions
aimed at a finished cycle, dates, attributions, or commented-out code — `git log` and the
handoffs already hold all of that, and a reader with the code but not the board can't use
it. Full rule, with the examples it was derived from: `.claude/scope-discipline.md`.
**`scripts/check-comments.sh` enforces this** (~0.2s, runs in `pre-push` and CI). It exists
because this rule decayed silently once already: card citations returned to doc comments and
reached operator log lines and API error responses before anyone noticed. When it fires,
rewriting is almost always right and deleting is almost always wrong — the comment usually
wraps something real in board scaffolding. Keep the knowledge, drop the pointer.
**And how much:** a module preamble ≤ 30 lines (test file: ≤ 10), a `///` block ≤ 15
lines, a production file ≤ 30 % comment. Vendor behaviour at a version goes in the fixture
README, design rationale in an ADR, history nowhere. `scripts/maintainability.py check`
measures it, `comment-worklist` lists what is over.

## Where everything else lives

| Topic | Doc |
|---|---|
| All configuration tables + debugging | `docs/CONFIG.md` |
| Crate detail, patterns, implementation notes, troubleshooting | `docs/ARCHITECTURE.md` |
| Endpoint reference / examples | `docs/API-REFERENCE.md` |
| Testing guide (unit → E2E → load) | `docs/TESTING.md` |
| The live plan, its stages and its tasks | `docs/book/src/roadmap.md` (Phase 64), `docs/plans/phase-64.md`, `docs/adr/0068-a-codebase-for-human-maintainers.md` |
| Harness architecture, rules, docket and opencode | `docs/plans/harnesses.md`, ADRs 0066 and 0067 |
| Decisions | `docs/adr/` — lead with the ask; evidence in its own section |
| History: closed boards, per-card handoffs | `TODO.md`, `docs/agent-handoffs/`, `docs/closed-cycles/` — only when named |
| Size budgets still gated in `pre-push` | `scripts/maintainability.py --help` (the plan behind them, Phase 63, is closed: `docs/plans/human-maintainability.md`) |
| Wire contract of record | `docs/contracts/runner-v1/` |
| MCP, GitHub sync, deployment | `docs/MCP.md`, `docs/GITHUB-SYNC.md`, `docs/DEPLOYMENT-GUIDE.md` |
| User/developer book (mdBook) | `docs/book/src/` |
