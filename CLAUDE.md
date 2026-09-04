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

**Active cycles — three, in parallel:** **Part VI** (Phase 60, agent onboarding + provider UX:
one screen that owns the path from an installed binary to a completed attempt, project-level
model choice, Vercel AI Gateway as a runner-side provider — unstarted) and **Part V** (Phase
59, adoption + first public release — Wave 13 in flight). Part IV (Phase 58, `tack serve
--with-runner`) is done. **Part VII** (Phase 61, desktop app + background service — ADR 0062 accepted 2026-09-03;
Wave 18 ready; board at the top of `TODO.md`, dispatch plan in
`docs/agent-handoffs/part-vii/README.md`) is the third. Both active Parts branch from `develop` and share `README.md` and
`docs/screenshots/**` under the conflict rules in `TODO.md` §VI.3 and §V.3 — read them before
branching a card in either. The boards are the authority for what shipped;
`docs/book/src/roadmap.md` records only intent.

**Never read `TODO.md` whole — it costs ~199k tokens.** Active boards sit in its first ~2400
lines (Part VII, then VI, then V, then IV — extract one, never all); the archive (Parts I–III) sits below them.
It was kept in-file because hundreds of doc comments cited its section numbers; **no Rust file cites it any
more**, so extracting it is now an open option rather than a blocked one — see `TODO.md`'s own header. Costs and extraction recipes for every big file:
**`.claude/context-budget.md`**. Before designing anything, read
**`.claude/scope-discipline.md`** — this tree's recurring defect is well-built mechanisms
with no caller (`model_profiles`, the `decisions` path, the superseded docket control plane).

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
├── tack-db/       SQLite via sqlx; 62 migrations; FTS5; repository pattern in repo/
├── tack-orch/     ControlPlane trait + reconciler + neutral runner-v1 execution domain.
│                  Depends on core+db only — must NEVER depend on tack-api
├── tack-api/      Axum server (library; tack_api::serve). ~90 documented paths + WebSocket
├── tack-runner/   Pull-based execution runner — separate binary; owns credentials,
│                  workspace, journal, and the harness subprocess (codex/claude_code)
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
  openapi_contract` then `cd frontend && npm run gen:api`, or `./scripts/regen-generated.sh`
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
- **Each board card writes one handoff** in `docs/agent-handoffs/`; corrections are
  appended as amendments, never rewritten.
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
| MCP, GitHub sync, deployment | `docs/MCP.md`, `docs/GITHUB-SYNC.md`, `docs/DEPLOYMENT-GUIDE.md` |
| User/developer book (mdBook) | `docs/book/src/` |
