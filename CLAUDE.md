# CLAUDE.md

A pointer, not a manual. Read `CONTRIBUTING.md` and `docs/TESTING.md` — the same docs a
human contributor reads — for everything else.

## What Tack is

**Tack** is two components shipped as one `tack` binary: the board (project manager — Rust
backend via Axum + SQLite, SolidJS frontend) and the runner (a worker that executes near
your code and credentials, embedded via `tack serve --with-runner`). Multiple workflows,
per-project vocabulary, an MCP server (`tack mcp`), a Tauri desktop app.

## What's next

Phase 64 shipped 2026-09-19; the runner, harnesses, MCP and desktop app are the product
surface. Phase 65 is live, and `docs/plans/phase-65.md` is the only list of pending work:
the release tag, inbound GitHub sync, `tack start` and the harness upgrades, in four waves.
Decisions go in an ADR, intent in `docs/book/src/roadmap.md`, history in commits.

## Commands

```bash
cargo build
cargo run -p tack-cli -- serve             # server + web UI at http://127.0.0.1:3210
cargo nextest run --workspace              # the test runner — see docs/TESTING.md
cd frontend && npm run dev                 # proxies /api; start the API first
.githooks/pre-push                         # the gate that decides whether a change can leave the machine
```

## Where things live

| Topic | Doc |
|---|---|
| Setup, structure, PR process, database, migrations, logging | `CONTRIBUTING.md` |
| Testing, test layering, runner-v1 contract tests, CI | `docs/TESTING.md` |
| Configuration, secrets posture | `docs/CONFIG.md` |
| Crate detail, architecture, patterns | `docs/ARCHITECTURE.md` |

`docs/openapi.json`/`schema.gen.ts` are generated — never hand-edit; `./scripts/regen-
generated.sh` regenerates both plus the lockfiles. Never `git commit` unless asked.
