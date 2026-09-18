# Plan: executing Phase 64

How the stages of Phase 64 (`docs/book/src/roadmap.md`, ADR 0068) are cut into tasks an
agent can carry out alone, in what order, and under which limits. Stage 3's tasks are
detailed in `docs/plans/harnesses.md`; every other stage is detailed here.

**This is a plan, not a board.** There is no card file, no per-task handoff, no gate test
written to prove a task was done. A task's record is its commits; its status is one word in
the table at the bottom. The file is archived to `docs/closed-cycles/plans/` when the phase
lands.

Every number carries its command. Re-run it before quoting it.

## Before anything

The harness core redesign is in `develop` (`git log --oneline --grep harness-seam-redesign
develop`). Every task below branches from a `develop` that has it.

## The batches

A batch is a set of tasks with no file in common, so they can run at the same time. A task
starts when the tasks it names have been merged into `develop`.

| Batch | Tasks, in parallel | Starts after |
|---|---|---|
| **A** | **M0** measure · **C1** CI and coverage · **H1** harness core fixes | the branch above is in `develop` |
| **B** | **H2** docket · **S2** agent scaffolding out · **P1** `model_profiles` out | H1 · C1 · M0 |
| **C** | **H3** opencode · **R1a** → **R1b** bridge: frontend (serial) | H2 · P1 |
| **D** | **D1** decisions: the runner half · **R2** bridge: API and CLI | H3, for D1 · R1b, for R2 |
| **E** | **R3** bridge: `tack-orch` → **R4** bridge: schema (serial) · **H4** harness docs | R2 · D1, for H4 |
| **F** | **D2** decisions: the board half, then **T1** API tests by layer · **T2** board feature tests · **T3** E2E and CI tiers · **T4** mutation report | R4 |

```
M0 ──────────────► P1 ─────────┐
C1 ──► S2                      ├─► R1a ► R1b ► R2 ► R3 ► R4 ──► D2 ► T1 T2 T3 T4 ──► release tag
H1 ► H2 ► H3 ► H3b ► D1 ► H4   │
        └──────────────────────┘
```

Why this order: the three tracks of batch A touch disjoint trees (docs, `.github/`,
`crates/tack-runner/src/harness/`). The bridge is not touched until docket is reachable as a
harness (H2). H1, H2, H3 and D1 all change `crates/tack-runner/src/harness/`, and D1 changes
types H3 builds, so they are serial. P1, R1–R4 and D2 all regenerate `docs/openapi.json` and
`schema.gen.ts`, so they are serial with each other. The test rebuild comes last so it does
not rewrite tests for code that is about to be deleted.

## How a task is handed to an agent

One agent, one task, one branch from `develop` named after the task (`h2-docket`). The brief
is the task's section of this file or of `harnesses.md`, plus these lines, and nothing else
— the agent reads `CLAUDE.md` on its own:

```
Task <id> from docs/plans/<file>.md. Read that section and the files it lists, nothing wider.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Add only the tests the task names. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag or helper the task does not name.
Finish by running .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do.
```

Limits, imposed from outside the prompt, because a prompt does not enforce them:

- **Model:** Sonnet for every task except M0 and the reviews, which need judgement about
  what a number means.
- **One gate, once.** The agent runs `.githooks/pre-push` at the end. While working it runs
  only the test binary it is changing (`cargo nextest run --workspace -E 'binary(<name>)'`).
- **150 tool calls per agent.** An agent that reaches it reports where it stopped instead
  of pushing on, and is replaced by a fresh agent with a short brief — never resumed with a
  large context.
- **At most two agents building at once** (a third may run when its work is the frontend or docs), each building with `--build-jobs 4` and testing with
  `--test-threads 4`. Sized to the workstation on 2026-09-18: 16 cores (`nproc`), 20 GB of
  memory available and swap full (`free -g`), 100 GB free on `/` (`df -h /`). Re-measure
  before raising it.
- **`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<task>`**, on the `/` partition, removed
  when the task's branch merges. `renice` is applied by a loop the launcher runs, never by
  the prompt. No agent opens a GUI.
- **Review before merge** is done by the launching session, against the task's *done when*,
  reading the diff — not by a second agent asked to agree.

## What keeps a task small

These are review criteria. A task that breaks one is sent back, not merged and fixed later.

1. **Nothing is added without its caller in the same change.** A descriptor field, an enum
   variant, a helper: the task names what calls it, or it is not written.
2. **Tests are budgeted per task.** Each task states its test functions by name or by count.
   More behaviour to prove means more rows in a table, not more functions.
3. **A deletion task adds no test** except the one that proves an upgrade path (R4). Tests
   of deleted code are deleted with it, not rewritten.
4. **Nothing is moved that can be deleted, and nothing is renamed for taste.**
5. **A security or "writes nothing" claim is proved once by reverting the fix** and watching
   the test fail; the report says so.
6. **Docs change in the same task as the behaviour**, in the file a user reads — never in a
   new file.

## The tasks

### M0 — measure (Stage 0)

Read-only on code. **Files:** ADR 0068 (a dated amendment, never a rewrite) and this file.

1. Re-run every table in ADR 0068's *Full reasoning* with its command; record the new
   numbers and date in an amendment.
2. Produce the exact file lists R1–R4 and T1 need, and write them into those sections here:
   every file of the bridge by layer; which files under `crates/tack-api/tests/orchestration/`
   exercise runner-v1 fleet templates and therefore stay; every test file that asserts the
   stale-lease invariant (`git grep -l "stale_lease\|StaleLease" -- 'crates/*/tests/**'
   'crates/**/tests.rs'`).
3. List every caller of DAG-ordered sprint dispatch outside the bridge (`git grep -n
   "sprint_dispatch"` for callers that are not `orch_*`), so R2 removes them with it. It is
   dropped either way; it comes back on runner-v1 scheduling only if use of the released
   product asks for it.

**Done when:** R1–R4 and T1 below carry file lists with a date.

### C1 — CI and coverage (Stage 1)

**Files:** `.github/workflows/ci.yml`, `deny.toml` and `scripts/gen-deny-toml.sh` if the
advisory needs it, `Cargo.lock`, `docs/TESTING.md` (the CI section only).

- One job runs `cargo llvm-cov nextest --workspace --lcov --output-path lcov.info`; it
  replaces both the plain test job and the five per-crate coverage builds. Its floor is the
  measured workspace line total minus one point, written next to the command that measures
  it.
- Patch coverage: `diff-cover lcov.info --compare-branch origin/<the pull request's base>
  --fail-under 80`. The tool, not a script of our own.
- Three tiers by trigger in the same workflow file: pull request (fmt, clippy, the run
  above, frontend type-check and Vitest, OpenAPI drift, deny and audit); push to `develop`
  or `main` (adds the SPA build, the desktop build, Chromium E2E); `schedule` (cross-browser
  E2E, MSRV). No new workflow file.
- RUSTSEC-2026-0285 (rustls): `cargo update -p rustls` first; an ignore entry with its
  reason only if no fixed release exists.
- The work happens on `develop`. The `main` ruleset still requires the ten old job names;
  it is changed by the user (`gh api`) when `develop` is next released to `main`, from the
  list this task reports. The agent does not change repository settings.

**Tests:** none. **Done when:** the task's own pull request into `develop` is green; the
pull-request tier's wall-clock, from `gh run view`, is under 15 minutes.

### S2 — agent scaffolding out (Stage 2)

**Files:** `.claude/` (removed, except `settings.local.json`, which is untracked),
`TODO.md` → `docs/closed-cycles/boards/`, `docs/agent-handoffs/` →
`docs/closed-cycles/handoffs/`, `scripts/maintainability.py`,
`scripts/maintainability-baseline.json`, `scripts/list-fixed-waits.py`,
`docs/adr/0064-fixed-waits.txt`, `scripts/check-test-hygiene.sh`, `scripts/check-comments.sh`,
`.githooks/pre-push`, `.github/workflows/ci.yml` (the steps that call those scripts), a new
`clippy.toml`, `CLAUDE.md`, `CONTRIBUTING.md`, `docs/TESTING.md`.

- `clippy.toml`: `disallowed-methods` with `std::env::temp_dir` and its reason;
  `too-many-lines-threshold` at the value that passes today, measured, not chosen. Production
  code that legitimately uses the temp directory gets an `#[allow]` with a reason.
- `check-comments.sh` keeps one rule — a cited file must exist — or is removed if that rule
  has no failing case in the last year of history.
- `pre-push` runs fmt (workspace and desktop), clippy and the generated-files check.
- The two rules worth keeping from `.claude/` — a decision document leads with the ask; no
  mechanism without a caller — move into `CONTRIBUTING.md` as two short paragraphs. The test
  and harness rules in `CLAUDE.md` move to `docs/TESTING.md`. `CLAUDE.md` becomes ≤ 40 lines
  pointing at both.
- `git mv` only; no history rewrite. No check may read `docs/closed-cycles/`.

**Tests:** none. **Done when:** `wc -l CLAUDE.md` ≤ 40; `ls .claude` shows nothing tracked;
`pre-push` runs three things and passes.

### P1 — `model_profiles` out (Stage 4, unconditional half)

**Files** — the 23 that `git grep -il model_profile -- ':!docs/closed-cycles'
':!docs/agent-handoffs' ':!docs/plans' ':!TODO.md'` lists: the routes in `crates/tack-api/src/handlers/runner_admin.rs` and their `openapi.rs`
entries; the CLI commands in `crates/tack-cli/src/execution.rs` and `main.rs`; the MCP tool
in `crates/tack-cli/src/mcp.rs`; their tests; `ModelProfilesPanel.tsx` and its test;
`frontend/src/shared/execution/api.ts`; `frontend/e2e/helpers.ts`; `docs/MCP.md`; the user
guide. Plus one new migration.

- One migration, one statement: `DROP TABLE model_profiles`. If an index or trigger must go
  first, each is its own migration name. The repository functions go with their tests in
  `crates/tack-db/tests/repository/execution_enqueue.rs`.
- Regenerate, never hand-edit: `./scripts/regen-generated.sh`.
- Release notes gain one line naming the removed routes, the MCP tool and the panel.

**Tests:** none added. **Done when:** the grep above finds `model_profiles` only in
migrations and ADRs.

### D1 and D2 — `decisions` gets its first caller (Stage 4, second half)

The board half of `decisions` is built: the runner-v1 routes, the operator's resolve route
behind its own token, the inbox in the UI, the runner's `create_decision` and
`poll_decisions`. What never existed is a harness that asks. It stays, and these two tasks
give it a caller. **D1**, the runner half, is detailed in `docs/plans/harnesses.md`: one seam
in the core that any harness can use, and claude-code as the first to use it. **D2**, the
board half, is here.

**D2 — whoever dispatches chooses.** **Files:** the dispatch handler in
`crates/tack-api/src/handlers/executions.rs` and its `openapi.rs` entry; the *Run with
agent* dialog under `frontend/src/features/` and its test; `frontend/src/shared/execution/`
(`capabilities.ts`, `api.ts`); `docs/book/src/user-guide/agent-runners.md`.

- The dispatch request accepts `permission_policy.approvals`: `auto` or `ask`. Absent means
  `auto`, which is what every run does today.
- `ask` on a harness whose `decisions` capability is not `supported` is rejected with a
  typed validation error before anything is enqueued. The dialog offers the choice only
  where the capability is `supported`, and says why where it is not.
- Regenerate, never hand-edit: `./scripts/regen-generated.sh`.

**Tests:** one table-driven HTTP test (accepted with `auto`, accepted with `ask`, rejected
with `ask` on a harness that cannot, with a row count proving nothing was enqueued); one
Vitest case for the dialog. **Done when:** a run dispatched with `ask` from the dialog
reaches `waiting_decision`, shows in the inbox, and continues when answered — walked once by
hand against the real `claude` and a loopback model server, and written into the report.

### R1–R4 — retire the Docket control plane (Stage 5)

Serial, callers before callees, so the tree compiles after each. File lists are M0's; the
layers, from ADR 0068:

| Task | Removes | Notes |
|---|---|---|
| **R1a** frontend: the pages | `features/{fleet, approvals, economics, provisioning}`, `features/settings/{orchestration, orchestrationSettings}`, their entries in `src/app/routes.tsx`, nav entries, mocks and E2E specs | `features/agents/runnerFleet` stays — it is runner-v1 |
| **R1b** frontend: what the board views share with the bridge | `shared/dispatch/`, `shared/orch/`, `shared/agentActivity/`, `features/sprints/DispatchSprintModal.tsx` and its test, and every use of them in the files measured below | `shared/runWithAgent/` and `shared/execution/` are runner-v1 and stay; where they import a type from a removed module, the type moves into `shared/execution/types.ts` |
| **R2** API and CLI | `handlers/{orch, provisioning, economics}.rs`, `dispatcher.rs`, `orch_store.rs`, `orch_runtime.rs`, `sprint_dispatch.rs`, `tack orch`, `TACK_ORCH_*` from `docs/CONFIG.md`, `tests/orchestration/**` except the files M0 marked runner-v1 | regenerate the OpenAPI spec and `schema.gen.ts` once, at the end |
| **R3** `tack-orch` | the `ControlPlane` trait, `reconciler`, `adapters/`, the `docket_*` tests, their fixtures and goldens | the execution domain, scheduler, model policy, retention and `runner_contract` stay |
| **R4** schema | one migration per `DROP TABLE` for `control_planes` and the `orch_*` tables, children before parents. No export step: the migration runner already writes a whole-database `VACUUM INTO` snapshot before any upgrade (`create_pre_upgrade_backup_if_needed`), so the rows survive there; `tack-db`'s `repo/orch.rs`, `repo/economics.rs` and their tests go with the tables | the secret columns of those tables leave `remote_backup.rs::scrub_snapshot_secrets` in the same commit |

**Measured 2026-09-18 on `develop` at `707f71a`** (`cat <files> | wc -l`; importers with
`git grep -l`):

- The ADR's table holds: `tack-orch` production 3 432 lines, `tack-api` production 6 156,
  the six frontend feature directories 7 323, `tests/orchestration/**` 7 957.
- The frontend bridge is larger than that table: `shared/dispatch/` 1 123 lines,
  `shared/agentActivity/` 750, `shared/orch/` 393, `DispatchSprintModal` 559. They are used
  from `features/board/Board.tsx`, `features/list/List.tsx`, `features/table/Table.tsx`,
  `features/sprints/Sprints.tsx`, `features/item-detail/ItemDetailDrawer.tsx`,
  `features/item-detail/tabs/AgentActivityTab.tsx` and its test, `shared/ui/AgentStateChip.tsx`,
  `shared/api/client.ts`, `shared/execution/{api,capabilities,realtime,types}.ts`,
  `shared/runWithAgent/shared.ts`, and the E2E files `a11y.spec.ts`, `helpers.ts`,
  `run-with-agent.spec.ts`, `execution-attempt-detail.spec.ts`. Hence R1b.
- DAG-ordered sprint dispatch is called only by `handlers/orch.rs`, the router, the OpenAPI
  table and the sprint view's *Dispatch sprint* dialog (`git grep -n sprint_dispatch`, `git
  grep -l SprintDispatch -- frontend`). The dialog goes in R1b, the rest in R2.
- In `tests/orchestration/`, two files are runner-v1 and stay:
  `fleet_templates/fleet_membership.rs` (`/api/runner-fleets`) and
  `fleet_templates/templates.rs` (`/api/templates`). R2 moves them, with `git mv`, under the
  test binary that already covers their handlers. `dispatch/dual_scheduling.rs` tests the
  guard between a docket task and a runner-v1 request; the guard and the file go together.
- `tack-orch`: `adapters/` (six files), `reconciler.rs` 1 442, `reconciler/tests.rs` 1 920,
  `tests/docket_*.rs` with `docket_tick_contract_test/support.rs` (3 071 together),
  `tests/golden/{wire,tick}/`.
- `tack-db`: `src/repo/orch.rs` 2 101, `src/repo/economics.rs`,
  `tests/repository/orch_repo.rs` 947, `tests/migrations/orch_migrations.rs` 1 189,
  `tests/migrations/orch_metrics.rs` 619.
- Tables still created by the migrations: `control_planes`, `orch_approvals`, `orch_events`,
  `orch_events_daily`, `orch_links`, `orch_metrics`, `orch_metrics_daily`, `orch_runs`,
  `orch_tasks`, `orch_trace_cursors` (`grep -o "CREATE TABLE.*orch_[a-z_]*"
  crates/tack-db/src/migrations.rs`; the two `_new` names are rebuild scaffolding). R4 checks
  each against `sqlite_master` on a fresh install before writing its `DROP`.
- `TACK_ORCH` is read outside the bridge in `tack-api`'s `config.rs`, `error.rs`,
  `server.rs`, `router.rs`, `handlers/{items,settings}.rs`, in `tack-core/src/models.rs`,
  `tack-orch/src/execution_retention.rs` and `tack-cli/src/main.rs`; and documented in
  `docs/{CONFIG,API-REFERENCE,ARCHITECTURE,MIGRATION-GUIDE}.md` and five book pages (`git
  grep -l TACK_ORCH`). R2 and R3 each take the ones in their crates; the docs go with R2.
- The stale-lease invariant is asserted in 12 test files (`git grep -c
  "stale_lease\|StaleLease" -- 'crates/*/tests/**' 'crates/**/tests.rs'`). T1 keeps
  `tack-db/tests/repository/execution_claim_lease_heartbeat.rs` and
  `tack-api/tests/runner_protocol/lifecycle.rs`; the runner's own two (`engine/tests.rs`,
  `transport/tests.rs`) assert what the runner does on receiving it and stay; the other
  eight lose the assertion.

Found while R2 was reviewed, and added to the tasks that follow:

- **R3b — the `orchestration` block of a project template.** `TemplateOrchestration` in
  `tack-core`, its validation in `tack-api`'s `handlers/templates.rs` and the tests in
  `tests/handlers/templates.rs` configure a bridge that no longer exists. It goes after R3,
  with the OpenAPI spec regenerated and a line in the release notes; a template that still
  carries the block must load, with the block ignored.
- **R4 also removes `Repository::update_item_status_checked`** and
  `tests/repository/status_update_checked.rs`: the dispatcher was its only caller.

**Tests:** R1–R3b add none. R4 adds exactly one: a file-backed database populated at the
last pre-removal migration upgrades, the pre-upgrade snapshot still holds the rows, and no
`orch_*` table remains. **Done when:** a fresh install has no `orch_*` table; `git grep -n TACK_ORCH`
finds nothing outside ADRs and history; release notes name what a bridge user loses — the
approvals inbox, one-click pod provisioning, per-product cost from docket's events, and
DAG-ordered sprint dispatch.

### T1–T4 — the test suite by layer (Stage 6)

| Task | Does | Budget |
|---|---|---|
| **T1** API tests | `crates/tack-api/tests/wave2_gate.rs` deleted after each real claim it holds has a home at its lowest layer; the stale-lease invariant kept in the repository test and one HTTP test, removed from the other files M0 listed | net test lines go down; no new file |
| **T2** board features | HTTP tests for `attachments`, `custom_fields` and `boards_multi`: success, auth, validation and each documented error, one table per route | three test files, one per handler |
| **T3** E2E and tiers | Playwright reduced to the critical journeys, listed in `docs/TESTING.md`; Chromium on merge, cross-browser nightly; the flaky-test quarantine rule written down | no new spec |
| **T4** mutation report | a weekly `cargo-mutants` job on `tack-core` and `tack-db`'s repository layer, report-only, uploaded as an artifact | one workflow job; no gate |

**Done when:** workspace line coverage is at or above C1's floor with fewer tests
(`cargo nextest list --workspace | wc -l` before and after); every public route has a
success, an auth and an error test.

## Decisions taken

All on 2026-09-18, by the user. Nothing in this plan waits on a decision.

| Question | Answer |
|---|---|
| Is the `decisions` path removed? | No. Every harness can pause and ask; Tack implements it once, in the core, and each harness uses it when its CLI offers a way (D1, D2) |
| Is DAG-ordered sprint dispatch rebuilt on runner-v1? | No. It goes with the bridge and is named in the release notes; it comes back only on evidence of use |
| The `main` ruleset's required checks | Left alone. Work targets `develop`; the ruleset changes when `develop` is next released |
| The six Dependabot branches | Merged after C1, so they run on the new CI |
| How many agents, and how long each | Two at once, 150 tool calls each — see the limits above |

## Status

| Task | State |
|---|---|
| Harness core redesign | done |
| M0 | done |
| H1 · C1 · P1 · H2 · S2 · R1a | done, in `develop` |
| H3 · H3b · R1b | done, in `develop` — opencode installs its plugin package from the npm registry on every attempt, so it refuses a request that denies network |
| R2 | done, in `develop` — 24 518 lines out, 97 documented paths become 78 |
| R3 | done, in `develop` — `tack-orch` goes from 19 056 lines to 7 854 |
| D1 | done, in `develop` — claude-code asks through `--permission-prompt-tool stdio`; the walk through the real binary and the operator route is D2's |
| R4 · R3b | running |
| H4 | not started |
| D2 · T1 · T2 · T3 · T4 | not started |
