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

- **Released:** `v0.1.0-beta.9` (2026-09-21). The next release is `v0.2.0-beta.1` (Phases 66–82):
  the version is bumped and the changelog section written in the Phases 79–82 merge (PR #66), and it
  is not tagged. `v0.1.0-beta.10` (tagged 2026-10-14) never had a release and is not re-cut, so the
  README's install commands install `v0.1.0-beta.9` until the maintainer publishes
  `v0.2.0-beta.1` (`docs/LAUNCH-CHECKLIST.md`).
- **In `develop`, unreleased:** Phase 73 (`docs/closed-cycles/plans/phase-73.md`, ADRs 0021 and 0022) — every
  docket run goes to a long-lived team over docket's HTTP API version 6, a role chooses the
  profile, a task can start by itself and repeat, the automatic merge stops at the team's
  ceiling, usage is kept per model, Settings → Automation is a ladder of four rungs, and a
  whole project goes in and out through one file (a template is that file without items).
- **Opened 2026-10-17:** Phase 74 (`docs/closed-cycles/plans/phase-74.md`, ADR 0023, accepted) — the twenty
  Tack defects the My SaaS onboarding found, fixed where each fact is owned, ending with
  My SaaS's first runs through its teams as Phase 73's live proof. Tasks F, I, W, C, L and X are in
  `develop` (2026-10-18); My SaaS's runs on paid models wait on docket's context and token count.
- **Opened 2026-10-18:** Phase 75 (`docs/closed-cycles/plans/phase-75.md`, ADR 0024, accepted) — limits in
  tokens with no default and no dollars per day, cache reads never counted, every limit naming who
  applies it, and nothing sent to an agent twice. every round (A, G, B, D, E, V, L, CT, Z's screens) is in `develop`; left is
  Z's run of My SaaS, end to end through docket and prill.
- **Future, not planned (2026-10-27):** Phase 76 (`docs/plans/phase-76.md`, ADR 0025, proposed) — the project
  file kept in sync like Drupal's configuration. Postponed with no date; the plan predates Phases 79 to 81 and
  needs a re-read before it is opened. Until then the project file only creates and updates, never deletes.
- **Accepted 2026-10-19, from the My SaaS rerun:** ADR 0026 (built: the runner's default settings
  file, and a card with no check says so) and ADR 0027 (whoever makes the agent's copy installs its
  dependencies from the lockfile; Tack makes no copy of its own for a docket run; decision 5, a
  limit per profile learnt from its runs). Phase 77 (`docs/closed-cycles/plans/phase-77.md`) built tasks S, L, N
  and P in `develop` (2026-10-19); and K (docket API 6, `check_changed`): Phase 77 is built, its proof is My SaaS's code card through a team.
- **Built 2026-10-19:** Phase 78 (the roadmap's own section, no plan file, no ADR) — every other
  open finding in one pass: the desktop app's first runs, the board audit, a page of its own for each
  item with a run card and an Execution tab, and "Done when" ticks. Rounds 1 to 4 are in `develop`,
  the gate run once; the billed recordings were retaken 2026-10-20 (tutorial shots, `workflow.gif`,
  `hero.gif`, `attempt.png`); left are `agents.png` and `agents-flow.gif` (the test run starts Codex, whose
  configured model its login rejects), the desktop shots, `recovery-demo.gif` (a released build) and three
  small items listed in the roadmap.
- **The live suite (2026-10-27):** 14 of 15 against docket `52c22f89` (API 6) and prill `6b95423` on the local
  model; the fifteenth needs the model's lead step, which repeats itself (with Claude Haiku 5.5, 2026-10-31, all 20 tests of the suite as it is now pass in one run), and `--check` passes. It found five Tack
  defects, fixed in Phase 81's round 1. Since 2026-10-26 the suite lives in the
  git-ignored `onboarding-lab/e2e/` (`docket-prill-e2e.sh`), beside the `onboard-project` skill it exercises.
  Every other docket test runs against the in-process fake in `crates/tack-runner/tests/fake_docket/`.
- **Built 2026-10-27, in `develop` since 2026-10-31 (PR #66):** Phase 81 (`docs/closed-cycles/plans/phase-81.md`, no ADR) — what an audit of the tree
  with Phases 79 and 80 found, as cards for Sonnet and Haiku agents in three rounds. CI never needs
  docket or prill; the three meet only in the local, unversioned `onboarding-lab/`.
  A card runs only the tests of what it touched; the full suites run once per round.
- **Built 2026-10-30, in `develop` (PR #66); ADRs 0030 and 0031 accepted 2026-10-31:** Phase 82 (`docs/closed-cycles/plans/phase-82.md`, ADR 0030, accepted) — docket closed its
  Phases 52–54 the same day (`task` fields on `POST /tasks/<pod>`, a `promptManifest` and `handoff`
  `assumptions`/`questions` on every hop, API still 6, all additive). Cards: A sends the item as
  `task` fields and the previous answers on a rerun; B reads each hop's prompts into one `steps` value and
  the verifier's `family`/`trajectory`; C shows it on the Execution tab; D recaptures the fixtures and reruns
  the live suite. Tack writes no prompt file for a team (ADR 0030, decision 5). Widened 2026-10-30 (ADR 0031,
  accepted) after docket closed Phase 55 and neared the end of 56: E shows a `wait` as a wait and the whole
  call of an `outward` approval while pending, erased after; F runs a folder team (no repository) to
  Succeeded, keeps the rows a task wrote, drops `blueprint` from `ensure`. Rounds A∥B, E∥F, C∥D.
- **Built 2026-10-31, in `develop`:** Phase 83 (`docs/closed-cycles/plans/phase-83.md`, ADR 0032, accepted) — harness-neutral:
  one `TeamReport` any agent fills (docket first) shown on Agents → Teams with what each member can do; a task
  says who does it (a profile, grouped by team) and auto-starts without a role (and says why when it cannot);
  one proposal format (plan-v1 with type, role, team, parent) reaching the same accept panel from a run's file
  (local agents), a team's `tack_proposals` records (docket, read by task) or an uploaded file. A person always
  accepts. Planner and Researcher can write the plan file and a run that changes anything else fails. The
  live suite (20 tests, git-ignored `onboarding-lab/e2e/`) runs against docket `cdeabac3`: 15 fails on the
  local model's looping lead step, the rest pass; with Claude Haiku 5.5 all 20 pass in one run.
- **Built 2026-11-01, in `develop`:** Phase 84 (`docs/closed-cycles/plans/phase-84.md`, ADR 0033) — from an audit of
  the live suite's screenshots: one run-state language, every screen ordered by what needs a person first, a run
  that starts can move its item, the Table view removed (the List sorts, filters and edits in place), and a new
  work view, Orchestration: Flow (a lane per team with its own steps) and Map (roles → profiles → teams →
  members). Front end except N (`on_start_status`) and S (`GET /api/executions?project_id=` with `latest_hop`).
  The live suite passes its 20 tests against docket `9cdd6253` on the local model; two Opus audits of its screenshots
  drove a polish pass and a review pass; the second audit's list of fixes is what is left.
- **Built 2026-11-02, in `develop`:** Phase 85 (`docs/closed-cycles/plans/phase-85.md`, ADR 0034) — a manager is a profile
  that works through the board, with no "manager" type in the code: every caller a principal (agent credentials
  with scopes, amends ADR 0004), change sets as an agent's only write path with an "Agents on the board" rung,
  team activity (incl. work the board did not start), a computed digest, threads where each message is a turn
  (`features.converse`: continues or replays), triggers on domain events. The Create rung exists (off by
  default, capped; amends ADR 0032 d4) and a credential may name every project. Cards P, M, E, C, A, D, U, W, T,
  TU, R and G are built. Through docket a manager runs at L3/L4: a docket `send` tool always asks a person and at L5
  an `ask` is denied at once, so a `propose` that is a `send` cannot run at L5; whether it stays a `send` is the
  maintainer's call. The live suite (27 tests) passes in one run with Claude Haiku 5.5 against docket `a268257b` (2026-11-02;
  18 tasks, about 130 000 counted tokens), and `--check` reports no drift.
- **Built 2026-11-02, in `develop`:** Phase 86 (`docs/closed-cycles/plans/phase-86.md`, no ADR; amends ADR 0033 d1) — a person sets a
  task's status and the agents report its work: the six automatic status moves are removed with their settings,
  auto-start starts a task once and moves nothing, and the screen shows "Status" and "Work" apart (cards A, B, C).
  Status rules (a rule builds a change set) are a future, optional idea in the roadmap.
- **Built 2026-11-02, in `develop`:** docket's Phase 58 (API still 6, all additive and negotiated by `features`):
  `settledCursor` (a question answered at docket's own CLI closes in Tack as "Answered in docket"; the runner's
  `answered-elsewhere` operation), `failureKinds` (a spent team budget reads as such), `podPause` (a paused team
  says so; docket has no route to resume it) and `hopTimes` (a step says how long it took). Plus the third audit
  of the live suite's screenshots: a refused run reads "Not started" and a reject-review says so, the board sorts
  needs-you first, the item drawer's status is one control that opens the menu, drawer sections fold
  (`shared/ui/Fold.tsx`), and dependencies were updated (the desktop app's Tauri crates and Vitest 5 are not taken).
- **Built 2026-11-03, in `develop`:** Phase 87 (`docs/closed-cycles/plans/phase-87.md`, no ADR) — docket's Phase 59 (its ADR
  0038): a step that runs out of rounds, tool calls or time ends `step_budget` (tokens stay `budget`), says
  where to raise the limit, keeps its patch and report, and a person chooses Continue (a new request that
  `POST .../retry`s the same docket task) or Start over (`from_run` → `rework_base`). All six cards (A, B, C,
  D, S, L) landed the same day; the live proof against a real docket (P59-6/7/8) and a real Tack found and
  fixed two bugs (`continue`'s team lookup, a tokens-bound stop's missing branch) that the fake alone never
  would have caught. A genuine `time`-bound stop stays unproved live — docket's own per-request deadline
  races a round's own landing, so this needs a slower or steadier model to exercise for real.
- **Built 2026-11-04, in `develop`:** Phase 88 (`docs/plans/phase-88.md`, ADR 0035, accepted) — a person is told what needs them:
  one `notifications` table fed by the event bus (the rule is `tack-core/src/notification.rs`, the write path `tack-app`'s
  `Inbox`, a reconcile from the store at start and on `Lagged`), `GET /api/notifications` + `/live` socket, sidebar bell,
  drawer, title count, desktop notifications and tray count, and a `Channel` trait whose first implementation is the webhook with
  retries. Phase 10's webhook is gone.
- **Built 2026-11-04, in `develop` (live proof against docket pending):** Phase 89 (`docs/plans/phase-89.md`, ADR 0036) — the board orchestrates, a team
  declares what it accepts: one owner per concept; `TeamReport` gains a contract (`delivers`, `methods[]` with
  missing roles, `needs[]`) and the fit check grows from two facts to five; one router (`GET
  /api/projects/{id}/routing`) for every screen; a parent shows its children's progress and moves nothing;
  each limit shown with its owner and **Resume team**; a business structure is a template (`teams:` descriptive,
  "Digital business") and the repository's teams are applied through the runner. Any agent first: a profile with
  no team is a team of one filled from its runner's report; docket adds members, methods, step and team limits
  and folder teams, reported and probed, never required. Four additive asks to docket
  (`podShape`, `podContract`, `pipelineChecked`, `podResume`), each probed. Cards A ∥ C; D; E; F; G.
- **Opened 2026-11-04:** Phase 91 (the roadmap's own section, no plan file, no ADR; amends ADR 0034's `conversation`
  note and ADR 0035 d1) — the docket features Tack did not use: `conversation` (a docket thread continues),
  `memoryStatus` (pending memory needs you), `pipelineChecked` read as a refusal, `readScope` not used (why), a
  two-part consult proved in the fake, and Tack names docket 0.3.0-beta.1. Cards D; A ∥ B ∥ C.
- **Cancelled 2026-10-27:** everything open in the roadmap that was not a card (Wave 0 of Phases 65 and 69,
  the live proof with a real install, directives from corrections, the screenshot retakes, polling). The release
  is the maintainer's own step (`docs/LAUNCH-CHECKLIST.md`). Closed plans live in `docs/closed-cycles/plans/`.

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
