# CLAUDE.md

Guidance for Claude Code in this repository. This file is a **map, not a manual** —
deep reference lives in the docs listed at the bottom and is read on demand.

## Project Overview

**Tack** — a lightweight project-management tool: Rust backend (Axum + SQLite),
SolidJS frontend, single `tack` binary with the SPA embedded. Multiple workflows
(Scrum/Kanban/phase) with per-project vocabulary; 10 project-type presets; MCP server
(`tack mcp`); Alexa integration. Core is complete (backend, frontend, CLI).

**Active cycle:** `TODO.md`'s header states which Part is active and its status board
records what shipped, at which SHA — that board is the authority, `docs/book/src/roadmap.md`
records only intent. Don't read `TODO.md` whole (~10k lines, mostly archive) — use the
`/card` skill's extraction recipe.

**Skills:** `/card` (work a board card), `/feature` (feature work off the board),
`/gate` (scoped verification), `/tokens` (usage measurement vs `.claude/token-baseline.md`), `/status` (where am I / what is next), `/integrate` (merge finished card branches and review them as one change). Prefer them over improvising the workflow.

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

cargo test --workspace                     # everything; scope with -p <crate> first
cargo test -p tack-api --test wave2_gate       # runner lifecycle on the real router
cargo test -p tack-orch --test runner_contract # byte-pins every runner-v1 fixture
cargo test -p tack-api --test openapi_contract # spec drift gate

cd frontend && npm run dev                 # http://localhost:5173, proxies /api (start API first)
cd frontend && npm run type-check && npx vitest run
make e2e                                   # Playwright (make e2e-install once)
make audit                                 # cargo audit + npm audit
```

Live-harness runner tests are `--ignored` (Claude Code's is billed — run deliberately).
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
├── tack-db/       SQLite via sqlx; 61 migrations; FTS5; repository pattern in repo/
├── tack-orch/     ControlPlane trait + reconciler + neutral runner-v1 execution domain.
│                  Depends on core+db only — must NEVER depend on tack-api
├── tack-api/      Axum server (library; tack_api::serve). ~90 documented paths + WebSocket
├── tack-runner/   Pull-based execution runner — separate binary; owns credentials,
│                  workspace, journal, and the harness subprocess (codex/claude_code/opencode)
└── tack-cli/      The single `tack` binary: serve + CLI client (HTTP only, never opens the DB)

frontend/          SolidJS + Tailwind v4; two-axis design tokens (mode × palette);
                   types generated from the OpenAPI spec
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
is written either way; the code is inconsistent with itself, not unsafe. III-G5 routes it. Full crate detail, design patterns, and implementation
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
  never hand-edit; regenerate via `UPDATE_OPENAPI=1 cargo test -p tack-api --test
  openapi_contract` then `cd frontend && npm run gen:api`.
- **Unsupported is typed, unknown is explicit, unmeasured is nullable** — no
  `unimplemented!()`, no zero standing in for "unknown" (never render `$0.00` for
  unmeasured money; the literal is `Not measured`). Capability claims are load-bearing.
- **Logs carry ids only** — never credentials, prompt bodies, query strings or env
  values; tests assert the redaction.
- **A status-code assertion alone proves little.** For "writes nothing / rejects before
  X" claims, assert the absence directly (row counts, untouched checkpoint) and prove
  the test load-bearing by reverting the fix once.
- **Each board card writes one handoff** in `docs/agent-handoffs/`; corrections are
  appended as amendments, never rewritten.
- Changing an API response shape updates the matching frontend unit/E2E mocks in the
  same change. Frontend colors come from `--color-*` tokens only, never raw hex.

## Code style

`tracing` macros for logging; `#[instrument(skip(pool))]` on async fns; `thiserror` in
core/db, `anyhow` in CLI; `chrono::DateTime<Utc>`; UUIDv4 stored as TEXT;
`assert_matches!` in tests.

## Where everything else lives

| Topic | Doc |
|---|---|
| All configuration tables + debugging | `docs/CONFIG.md` |
| Crate detail, patterns, implementation notes, troubleshooting | `docs/ARCHITECTURE.md` |
| Endpoint reference / examples | `docs/API-REFERENCE.md` |
| Testing guide (unit → E2E → load) | `docs/TESTING.md` |
| Active board, wave rules, card ownership | `TODO.md` (extract, don't read whole) |
| Per-card decision history | `docs/agent-handoffs/` |
| Wire contract of record | `docs/contracts/runner-v1/` |
| MCP, Alexa, GitHub sync, deployment | `docs/MCP.md`, `docs/ALEXA.md`, `docs/GITHUB-SYNC.md`, `docs/DEPLOYMENT-GUIDE.md` |
| User/developer book (mdBook) | `docs/book/src/` |
