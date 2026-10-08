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
surface. Phase 65 shipped as `v0.1.0-beta.9` (2026-09-21), the first release with the
desktop bundles. Phase 66 (briefs, evidence, the verifier's merge-readiness pack, branch
push and pull requests, factory metrics, docket contract 1.1) closed 2026-10-05 and ships
as `v0.1.0-beta.10`. Phase 67 (ADR 0074: a run a normal person can start — project
Automation settings, the agent works on a branch of your repo or in your folder, the agent's
default model, built-in profiles, acceptance criteria, Needs your review, Tack's words and
help) executed 2026-10-07/08 and sits in `develop` unpushed. Phase 68 (`docs/plans/phase-68.md`,
opened 2026-10-08) finishes the fourteen follow-ups phase 67's agents found: named wire shapes,
a workspace never deleted after a failed keep, the plan's provenance, the Automation panel's
model and definition of done, the dialog and card, the kit. Phase-65's Wave 0 publish list is
the user's.
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
generated.sh` regenerates both plus the lockfiles.
