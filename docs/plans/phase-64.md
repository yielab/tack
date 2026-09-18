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
| **C** | **H3** opencode · **P2** `decisions`, only if the user decided to remove it | H2 · P1 |
| **D** | **H4** harness docs · **R1** bridge: frontend | H3, for H4 · H2, P1 and P2 if it runs, for R1 |
| **E** | **R2** bridge: API and CLI → **R3** bridge: `tack-orch` → **R4** bridge: schema (serial) | R1 |
| **F** | **T1** API tests by layer · **T2** board feature tests · **T3** E2E and CI tiers · **T4** mutation report | R4 |

```
M0 ──────────────► P1 ──► P2? ─┐
C1 ──► S2                      ├─► R1 ► R2 ► R3 ► R4 ──► T1 T2 T3 T4 ──► release tag
H1 ──► H2 ──► H3 ──► H4        │
        └──────────────────────┘
```

Why this order: the three tracks of batch A touch disjoint trees (docs, `.github/`,
`crates/tack-runner/src/harness/`). The bridge is not touched until docket is reachable as a
harness (H2). P1, P2 and R1–R4 all regenerate `docs/openapi.json` and `schema.gen.ts`, so
they are serial with each other. The test rebuild comes last so it does not rewrite tests
for code that is about to be deleted.

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
- **A turn cap per agent**, set by whoever launches it; an agent that reaches it reports
  where it stopped instead of pushing on.
- **`CARGO_TARGET_DIR` on the `/` partition**, one per parallel agent, removed when its
  branch merges. `renice` is applied by the launcher. No agent opens a GUI.
- **At most three agents at once.** The fourth waits.
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
3. Answer one question with evidence: does DAG-ordered sprint dispatch have a user outside
   the bridge (`git grep -n "sprint_dispatch"` for callers that are not `orch_*`)? Record the
   answer as a decision in the amendment: re-implemented on runner-v1 scheduling, or dropped
   and named in the release notes.

**Done when:** R1–R4 and T1 below carry file lists with a date, and the DAG question has a
recorded answer.

### C1 — CI and coverage (Stage 1)

**Files:** `.github/workflows/ci.yml`, `deny.toml` and `scripts/gen-deny-toml.sh` if the
advisory needs it, `Cargo.lock`, `docs/TESTING.md` (the CI section only).

- One job runs `cargo llvm-cov nextest --workspace --lcov --output-path lcov.info`; it
  replaces both the plain test job and the five per-crate coverage builds. Its floor is the
  measured workspace line total minus one point, written next to the command that measures
  it.
- Patch coverage: `diff-cover lcov.info --compare-branch origin/main --fail-under 80`. The
  tool, not a script of our own.
- Three tiers by trigger in the same workflow file: pull request (fmt, clippy, the run
  above, frontend type-check and Vitest, OpenAPI drift, deny and audit); push to `main`
  (adds the SPA build, the desktop build, Chromium E2E); `schedule` (cross-browser E2E,
  MSRV). No new workflow file.
- RUSTSEC-2026-0285 (rustls): `cargo update -p rustls` first; an ignore entry with its
  reason only if no fixed release exists.
- The `main` ruleset's required checks are changed by the user (`gh api`), from the list the
  task reports. The agent does not change repository settings.

**Tests:** none. **Done when:** pull request #56 is green; the pull-request tier's
wall-clock, from `gh run view`, is under 15 minutes.

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

### P2 — the `decisions` path (Stage 4, conditional half)

Blocked on a decision that is the user's: does docket get a `decisions` ADR? If yes, this
task is struck and `decisions` stays with docket as its first caller. If no: remove
`crates/tack-api/src/handlers/decisions.rs` and its routes, the runner's transport calls, the
UI, the four `decision.*.json` fixtures in `docs/contracts/runner-v1/`, and their rows in the
pin table of `crates/tack-orch/tests/runner_contract.rs` — all in one commit, because the
fixtures outrank the types. The `decisions` entry in every harness's capability table stays:
it is how a harness says it has none.

### R1–R4 — retire the Docket control plane (Stage 5)

Serial, callers before callees, so the tree compiles after each. File lists are M0's; the
layers, from ADR 0068:

| Task | Removes | Notes |
|---|---|---|
| **R1** frontend | `features/{fleet, approvals, economics, provisioning}`, `features/settings/{orchestration, orchestrationSettings}`, their routes, nav entries, mocks and E2E specs | `features/agents/runnerFleet` stays — it is runner-v1 |
| **R2** API and CLI | `handlers/{orch, provisioning, economics}.rs`, `dispatcher.rs`, `orch_store.rs`, `orch_runtime.rs`, `sprint_dispatch.rs` (per M0's DAG answer), `tack orch`, `TACK_ORCH_*` from `docs/CONFIG.md`, `tests/orchestration/**` except the files M0 marked runner-v1 | regenerate the OpenAPI spec and `schema.gen.ts` once, at the end |
| **R3** `tack-orch` | the `ControlPlane` trait, `reconciler`, `adapters/`, the `docket_*` tests, their fixtures and goldens | the execution domain, scheduler, model policy, retention and `runner_contract` stay |
| **R4** schema | one migration per `DROP TABLE` for `control_planes` and the `orch_*` tables, children before parents; before the first, the rows are exported as JSON into the pre-upgrade snapshot the migration runner already writes | the secret columns of those tables leave `remote_backup.rs::scrub_snapshot_secrets` in the same commit |

**Tests:** R1–R3 add none. R4 adds exactly one: a file-backed database populated at the
last pre-removal migration upgrades, the export file holds the rows, and no `orch_*` table
remains. **Done when:** a fresh install has no `orch_*` table; `git grep -n TACK_ORCH`
finds nothing outside ADRs and history; release notes name what a bridge user loses — the
approvals inbox, one-click pod provisioning, per-product cost from docket's events, and
DAG-ordered dispatch if M0 dropped it.

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

## Decisions that are the user's

| Decision | Blocks | Default if unanswered |
|---|---|---|
| Does docket get a `decisions` ADR? | P2 | `decisions` stays; P2 does not run |
| The `main` ruleset's required checks | C1 merging | — |
| Whether DAG-ordered dispatch is rebuilt, if M0 finds a user | R2 | dropped, named in the release notes |
| Merge the six Dependabot branches before or after C1 | nothing | after, so they run on the new CI |

## Status

| Task | State |
|---|---|
| Harness core redesign | done |
| M0 · C1 · H1 | not started |
| H2 · S2 · P1 | not started |
| H3 · P2 | not started · blocked on a decision |
| H4 · R1 | not started |
| R2 · R3 · R4 | not started |
| T1 · T2 · T3 · T4 | not started |
