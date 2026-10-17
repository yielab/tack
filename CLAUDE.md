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
push and pull requests, factory metrics, docket contract 1.1) closed 2026-10-05. Phase 67 (ADR 0074: a run a normal person can start — project
Automation settings, the agent works on a branch of your repo or in your folder, the agent's
default model, built-in profiles, acceptance criteria, Needs your review, Tack's words and
help) executed 2026-10-07/08. Phase 68 (`docs/plans/phase-68.md`) finished the fourteen
follow-ups phase 67's agents found and closed 2026-10-08; its Status section tables what it
found in turn, the weightiest being a request no runner can claim staying Queued with no
reason shown. Phases 66–68 and the work below ship together as `v0.1.0-beta.10`, not yet
tagged. Phase-65's
Wave 0 publish list is the user's.
ADR 0074's follow-ups found by the first live docket + prill run (2026-10-10) are in ADR 0075 and
landed the same day: prill is the runner's native verifier, a run that asks reads `waiting_decision`, the request follows its
attempt, the embedded runner takes `--runner-config`. The second run (2026-10-11, ADR 0075
decisions 10–14) put a running agent's steps on its timeline (docket only so far), gave every runner
event its own sequence, and made docket's question name the command. `scripts/docket-prill-e2e.sh`
is the live proof. What ships is in `CHANGELOG.md`'s `[0.1.0-beta.10]` section, written by hand
because the `Add:`/`Fix:` commits since beta.9 are not conventional and git-cliff skips them.
A `v0.1.0-beta.10` tag was pushed on 2026-10-12 and withdrawn the same day: a security audit found
the board answers anyone without a token, a web page can post to it, an agent runs with permissions
off, and the evidence base is a name the agent can move. ADR 0077 and Phase 69
(`docs/plans/phase-69.md`, four rounds for Haiku/Sonnet agents, two boundary suites as required
checks) closed that on 2026-10-12, in `develop` and not yet pushed; the release is re-cut as
beta.10 by the maintainer, after the GitHub settings in the plan's Wave 0. The same day the history
was rewritten to drop the `Co-Authored-By: Claude` trailers 159 commits carried — never add one.
Phase 70 (`docs/plans/phase-70.md`, closed 2026-10-12) finished the follow-ups earlier phases wrote
down: a queued run says why it waits and fails after 30 minutes no runner can take it, the doctor
prints per-harness lines, the real-`bwrap` test is required in CI.
Phase 71 (`docs/plans/phase-71.md`, ADR 0078) removed the provider layer: Tack holds no model key, endpoint
or catalog; each agent signs in with its own tools and a run names its model as one string.
Phase 72 (`docs/plans/phase-72.md`, ADR 0079, opened 2026-10-13) lets Tack merge: a Merge button,
merge on Done, and an automatic merge at L4/L5 under the verifier's signed verdict. One owner per
responsibility: prill owns the verdict, required stages and every quality figure; Tack the loop,
the merge (and whether its evidence is enough), GitHub and money; docket runs the agent. K2 closed
2026-10-14 without waiting for prill: Tack reads the required stages and what to fix off the pack.
Phase 73 (`docs/plans/phase-73.md`, ADR 0080, opened 2026-10-14) sends every docket run to a
long-lived team over docket's HTTP API version 5 (tested against an in-process fake until docket
0.2.0-beta.6 exists), lets a role choose the profile and a task start by itself, caps the automatic
merge at the team's ceiling, keeps usage per model, lets a task repeat, and puts a project's
automation on one screen: Settings → Automation as a ladder of four rungs. Its seventh wave
(ADR 0081, approved 2026-10-17) lets a whole project go in and out through one file: the outline
gains fields, profiles, roles, sprints and automation, the export writes the same file back, and
a template is that file without items.
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
