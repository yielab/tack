# CLAUDE.md

A pointer, not a manual. Read `CONTRIBUTING.md` and `docs/TESTING.md` — the same docs a
human contributor reads — for everything else.

## What Tack is

**Tack** is two components shipped as one `tack` binary: the board (project manager — Rust
backend via Axum + SQLite, SolidJS frontend) and the runner (a worker that executes near
your code and credentials, embedded via `tack serve --with-runner`). Multiple workflows,
per-project vocabulary, an MCP server (`tack mcp`), a Tauri desktop app.

## What's next

`docs/book/src/roadmap.md` is the one place for what is open and what was delivered; read its
"Open" section first. Decisions are in `docs/adr/`, history in `CHANGELOG.md` and the commits, and
every closed plan, board, handoff and audit in `docs/closed-cycles/` (frozen). As of 2026-11-15:

- **Delivered through Phase 95**, all in `develop`. The roadmap's "Delivered" section has one
  paragraph per phase (73 to 95) with its ADR and plan.
- **Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. `v0.2.0-beta.1`
  (Phases 66–95) is prepared: the version is bumped and `CHANGELOG.md`'s section is written, and it
  is not tagged. `v0.1.0-beta.10` (tagged 2026-10-14) never had a release and is not re-cut, so the
  README's install commands install `v0.1.0-beta.9` until the maintainer publishes
  `v0.2.0-beta.1` (`docs/LAUNCH-CHECKLIST.md`). Tagging is the maintainer's step, not a card.
- **Open cards:** D1 to D4 from the recorded demo (a commit names its item; a failed docket gate
  says why; the jail and the verifier agree about localhost; a manager's turn reaches its team).
  Codex's eval baseline and `agents-flow.gif` are postponed by the maintainer.
- **Future, not planned:** Phase 76 (`docs/plans/phase-76.md`, ADR 0025, proposed; re-read it
  against the tree before opening it), status rules, and what Phase 94 left out. Until Phase 76
  is opened the project file only creates and updates, never deletes.
- **The live suite** lives in the git-ignored `onboarding-lab/e2e/` (`docket-prill-e2e.sh`, plus
  `--check` for drift against the fixtures), beside the `onboard-project` skill it exercises. It
  passed 35/35 against docket `3caccedd` (2026-11-08). CI never needs docket or prill: every other
  docket test runs against the in-process fake in `crates/tack-runner/tests/fake_docket/`.
- **Evals:** the development crate `tack-evals` (`evals/`, baselines in `evals/baselines/`) runs
  regression, safety and verifier suites with real models at integration only, never in CI.
- **How work is cut:** cards for Sonnet and Haiku agents; a card runs only the tests of what it
  touched, and the full suites run once per round through `.githooks/pre-push`.

Rules that history taught: never add a `Co-Authored-By` trailer (the history was rewritten on
2026-10-12 to remove 159 of them); never name a real project in a versioned file or a commit message (the history
was rewritten on 2026-11-10 to remove one, so the fictional project is My SaaS); `CHANGELOG.md`'s `[Unreleased]` is written by hand in the
change that makes it true; the book is three guides (User, Administrator, Developer) and the
first two carry no source paths, test names or phase numbers. The docs describe `develop` with no version attached:
never "since vX", "new in", "used to"; a version is named only where a reader needs it to act (the oldest
docket Tack works with, upgrading), the next release only in the roadmap, history only in `CHANGELOG.md`. Decisions go in an ADR, intent in
the roadmap, history in commits. A change to what the agent reads or receives names its suite, its threshold and its baseline, and writes the task first when none captures it. A phase closes on `regression` with no new reds plus the live suite.

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
