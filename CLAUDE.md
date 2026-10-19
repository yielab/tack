# CLAUDE.md

A pointer, not a manual. Read `CONTRIBUTING.md` and `docs/TESTING.md` — the same docs a
human contributor reads — for everything else.

## What Tack is

**Tack** is two components shipped as one `tack` binary: the board (project manager — Rust
backend via Axum + SQLite, SolidJS frontend) and the runner (a worker that executes near
your code and credentials, embedded via `tack serve --with-runner`). Multiple workflows,
per-project vocabulary, an MCP server (`tack mcp`), a Tauri desktop app.

## What's next

`docs/book/src/roadmap.md` is the one place for what is delivered and what is open; read its
"Open" section first. In short, as of 2026-10-18:

- **Released:** `v0.1.0-beta.9` (2026-09-21). `v0.1.0-beta.10` (Phases 66–72) is tagged
  2026-10-14 but its GitHub release is not published, so the README's install commands 404
  until the maintainer publishes it (`docs/LAUNCH-CHECKLIST.md`).
- **In `develop`, unreleased:** Phase 73 (`docs/plans/phase-73.md`, ADRs 0080 and 0081) — every
  docket run goes to a long-lived team over docket's HTTP API version 5, a role chooses the
  profile, a task can start by itself and repeat, the automatic merge stops at the team's
  ceiling, usage is kept per model, Settings → Automation is a ladder of four rungs, and a
  whole project goes in and out through one file (a template is that file without items).
- **Opened 2026-10-17:** Phase 74 (`docs/plans/phase-74.md`, ADR 0082, accepted) — the twenty
  Tack defects the Fabulada onboarding found, fixed where each fact is owned, ending with
  Fabulada's first runs through its teams as Phase 73's live proof. Tasks F, I, W, C, L and X are in
  `develop` (2026-10-18); Fabulada's runs on paid models wait on docket's context and token count.
- **Opened 2026-10-18:** Phase 75 (`docs/plans/phase-75.md`, ADR 0083, accepted) — limits in
  tokens with no default and no dollars per day, cache reads never counted, every limit naming who
  applies it, and nothing sent to an agent twice. every round (A, G, B, D, E, V, L, CT, Z's screens) is in `develop`; left is
  Z's run of Fabulada, end to end through docket and prill.
- **Proposed 2026-10-18:** Phase 76 (`docs/plans/phase-76.md`, ADR 0084, proposed) — the project
  file kept in sync like Drupal's configuration: a key on every role, field, sprint and profile, an
  import that is a three-way comparison against the last revision (removes what the file removed,
  keeps what the board changed, stops on a conflict, shows every removal's cost), revisions to go
  back to, and every import in one transaction. Waits on the maintainer's "continúa".
- **Accepted 2026-10-19, from the Fabulada rerun:** ADR 0085 (built: the runner's default settings
  file, and a card with no check says so) and ADR 0086 (whoever makes the agent's copy installs its
  dependencies from the lockfile; Tack makes no copy of its own for a docket run; decision 5, a
  limit per profile learnt from its runs). Phase 77 (`docs/plans/phase-77.md`) built tasks S, L, N
  and P in `develop` (2026-10-19); K's first part too (docket API 6); its `check_changed` waits on docket.
- **The live proof passes (2026-10-18):** `scripts/docket-prill-e2e.sh`, 9 of 9 against docket
  develop and prill on the local model, inside docket's sandbox with a read-only toolchain grant.
  Every other docket test runs against the in-process fake in `crates/tack-runner/tests/fake_docket/`.
- **The maintainer's:** the release, and the Wave 0 items of `docs/plans/phase-65.md` and
  `docs/plans/phase-69.md`. Closed plans live in `docs/closed-cycles/plans/`.

Rules that history taught: never add a `Co-Authored-By` trailer (the history was rewritten on
2026-10-12 to remove 159 of them); `CHANGELOG.md`'s `[Unreleased]` is written by hand in the
change that makes it true; the book is three guides (User, Administrator, Developer) and the
first two carry no source paths, test names or phase numbers. Decisions go in an ADR, intent in
the roadmap, history in commits.

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
