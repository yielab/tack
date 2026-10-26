# Phase 81: what the 2026-10-26 audit found

**Status: proposed 2026-10-26.** No ADR: nothing here changes a decision. The one rule below is
already true today and is written down so no card breaks it.

An audit of the tree with Phases 79 and 80 built (tests, structure, code design, screens, and the
seams with docket and prill) found the defects and gaps this plan orders. Every card is sized for
one Sonnet or Haiku agent, says what it owns, what to read first, what to run and when it is done,
so the agent needs no other briefing.

## The rule: CI never needs docket or prill

Tack does not depend on docket or prill: each is a neighbour Tack talks to when a person installs
it. So:

- **CI tests only Tack's own code**, with doubles that live in this repository: the in-process fake
  docket (`crates/tack-runner/tests/fake_docket/`), the fake prill script, and committed fixtures
  captured from the real programs. Today `.github/` names neither program, and `scripts/smoke.sh`
  in fake mode needs neither; no card may change that.
- **Everything that talks to a real docket or prill is local**, in one command,
  `scripts/docket-prill-e2e.sh`, plus its `--check` mode (card A). It is run by hand, at the end
  of a round, never in CI.
- A Rust test that needs a real program is `#[ignore = "<reason>"]` and run by that command, never
  a test that silently returns early when the program is missing (today
  `crates/tack-runner/tests/briefing_schema.rs` does that; card A fixes it).

## How every card runs

- **Model.** Each card names Sonnet or Haiku. Haiku takes the cards that follow a pattern already
  in the tree; Sonnet takes the ones that change a design.
- **Budget.** Read only the files the card names before starting (about 25k tokens). Stop and
  report at about 150k tokens with what is done and what is left, rather than pressing on.
- **Tests while working: only what the card touched.** Each crate has one integration-test binary,
  `it`, and each file under its `tests/` is a module of it (ADR 0029), so a file is selected by its
  module: `cargo nextest run --workspace -E 'package(<crate>) & test(/^<module>::/)'`; unit tests by
  name, `-E 'package(<crate>) & test(/<name>/)'`. Always `--workspace` with a filterset, never `-p`
  (it builds every crate a second time), and always `--build-jobs 4 --test-threads 4` (two
  different limits; in nextest `-j` is `--test-threads`). `npx vitest run <paths>` for the
  frontend. `cargo check --workspace` between edits. Every build runs with `CARGO_INCREMENTAL=0`:
  a card compiles a handful of times, and the incremental cache would cost more disk than it saves
  time. Never the unfiltered workspace suite, never the whole Vitest or Playwright suite, never
  `.githooks/pre-push` inside a card. Fix the tests the change breaks; add a test for a behaviour
  that changes, never for a restyle or a move.
- **The full suites run once per round**, on the merged tree, by whoever integrates: `.githooks/pre-push`,
  `cargo nextest run --workspace`, `npx vitest run`, and the Playwright CI config on Chromium. A
  card that changes the HTTP API runs `./scripts/regen-generated.sh` before it finishes.
- **The local suite of the three** (`scripts/docket-prill-e2e.sh` and `--check`) runs after Round 1
  merges and once at the end; only card A runs it inside a card, once, at its end.
- **Load.** At most two cards build at the same time, each in its own worktree with its own build
  folder on the partition with room; the dispatcher renices builds from outside. No card opens a
  window: no desktop app, no headed Playwright.
- **Docs go with the change.** Each card updates the guide paragraph, `ARCHITECTURE.md` line or
  `TESTING.md` section its change makes true, and adds its line to `CHANGELOG.md`'s `[Unreleased]`.
  There is no docs card. Never edit docket's or prill's repositories.

## Rounds

| Round | Cards, in pairs (at most two at once) | Gate |
|---|---|---|
| 1 | **B + C**, then **D + S** (D after B), then **A + H** (A after C), then **R** (after S) | full suites once; then the local suite of the three once; re-measure the coverage floor (D's number) |
| 2 | **E**, then **F**, in one worktree | full suites once; local suite once |
| 3, only if wanted | **I** (after S and R) beside **J** (after F); **K** after J | full suites once |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | The local suite of the three can pass and says what it proved | Sonnet | the e2e script, the capture script, `docket-prill.spec.ts`, `briefing_schema.rs` | C |
| B | Background work has one owner and stops | Sonnet | `tack-cli` serve, `tack-api` execution runtime and server start, `tack-app` loops, `complete` | — |
| C | The docket and prill adapters read typed answers and say things plainly | Sonnet | the docket harness and its fake, the prill verifier and its fixtures, event kinds in `tack-protocol` | — |
| D | CI and the gate check what the docs say they check | Sonnet | `ci.yml`, `pre-push`, `nextest.toml`, the bound-address line, three tests' waits | B |
| S | Screens show errors, plain words and disabled controls | Sonnet | `frontend/src` outside the run surface, two server sentences | — |
| R | The run surface | Sonnet | `shared/runWithAgent`, `shared/execution`, the level check in `start_run` | S |
| H | The routes, tools and upgrade nobody tests | Haiku | test files, one database fixture | — |
| E | The execution store is typed | Sonnet | `tack-db` execution repo and queries, `tack-app/src/runs`, the executions handlers | Round 1 |
| F | One wire format and one error model | Sonnet | `tack-protocol`, runner transport, runner-protocol handlers, errors in db, app and api | E |
| I | One kit for cards, switches, confirms, radii and type | Haiku | `shared/ui`, `index.css`, the files the codemod rewrites | S, R |
| J | `Host` split and the environment read once | Sonnet | `tack-app` host and GitHub client, `tack-api` host and config, `tack-cli` main | F |
| K | The item and its read model are separate | Sonnet | core `Item`, `tack-db` items repo and queries, items handlers | J |

## Left out, and why

- Folding `tack-orch` into `tack-core`, a facade over the runner's modules, one home per aggregate
  in `tack-db` as a task of its own, and renaming `fixtures/docket/v5/` to `v6/`: each costs more
  reading than it returns now. E and K settle the aggregates they touch.
- Splitting a long function as a task: a card splits the one it is already changing
  (`enqueue_run` in E, the docket `follow` in C), never others.
- A contract test inside docket's CI: that is docket's work. A note to docket's maintainers is
  enough: it kept `apiVersion` "6" while removing routes, so the number cannot tell its shapes
  apart, and Tack probes the route instead.
- Runs on a paid model, and Fabulada's code card through a team: they stay the maintainer's
  (roadmap, "Waiting on a partner").

**Done when:** the local suite of the three passes and names the docket and prill commits it
proved; no CI job, hook or default test needs docket or prill; each card's "done when" holds; and
the two round gates passed on the merged tree.

---

## Cards

### A. The local suite of the three can pass and says what it proved

**Model:** Sonnet. **Waits on:** C (A asserts the texts C writes).

**Why.** The only place Tack, docket and prill meet is `scripts/docket-prill-e2e.sh` with
`frontend/e2e/docket-prill.spec.ts`. Its "9 of 9" (2026-10-18) predates the escalation record, and
today it cannot pass:
- test 08 expects the sentence `Allow bash… docket's policy:`, which Tack no longer writes (the
  adapter passes docket's `prompt` through), and when the model asks nothing it only adds an
  annotation and passes without asserting the end state;
- the script's only check of docket is `apiVersion >= 5`, which both docket shapes answer, so a
  docket without the escalation record fails at test 02, after the builds;
- in tests 05–07 approvals are answered through the API (`waitAllowingApprovals`), not the screen;
  the two teams the script creates are never used;
- `briefing_schema.rs` returns early when `prill` is not on `PATH`, so it passes in CI without
  checking anything.

**Read first:** the script, the spec, `scripts/capture-docket-fixtures.sh` (its zero-model way of
making docket pause), `docs/TESTING.md`'s live-suite section, docket's
`specs/api/operator-api.spec.md` "Escalations" (read only).

**Does.**
1. Before any build, the script asks `GET /escalations?since=0` and stops with one sentence unless
   it answers 200, and writes docket's and prill's commit and version at the top of its summary.
2. Test 08 makes docket pause without a model, the way the capture script does (a policy that asks
   before the step), answers in the decision inbox on screen, and asserts the end state on every
   path. No pause is a failure, not an annotation. It asserts docket's own prompt, not a Tack sentence.
3. Tests 05–07 add no run: their profiles name the team the script already creates, the test
   asserts the run went to that team, that usage arrives per model with cache reads left out of
   the limit, and approvals are answered on screen.
4. New cases, each without a model where the capture script's technique allows; a case that needs
   a model rides one of 05–07 instead of adding a run:
   - a lights-out team at L5 meets an approval: the run ends needing a person, "Run again at L3"
     asks, the answer completes it; a level above the team's ceiling shows docket's 422 reason;
   - cancel while parked; restart the runner while parked, then docket: the run goes on, and docket
     received one task for the attempt (same idempotency key);
   - a signed pack: the verifier signs, and attest verify, record and metrics answer;
   - one verifier reject, shown as rejected on the card.
5. `--check` mode, no build and no run: recapture docket's answers into a temporary folder and diff
   their keys against `crates/tack-runner/src/harness/fixtures/docket/`; diff Tack's prill fixtures
   (`docs/contracts/mrp-v1/fixtures/prill/`, `crates/tack-runner/src/verify/fixtures/prill/`)
   against the prill checkout's `contracts/` (`PRILL_SRC`, like `DOCKET_SRC`); run the ignored
   prill tests. Exit non-zero on any drift.
6. `briefing_schema.rs`: `#[ignore = "needs prill: run by docket-prill-e2e.sh --check"]`, no early
   return.
7. `TESTING.md`'s live-suite section says what the suite proves, what `--check` compares, and that
   CI never runs either; it drops the scenario it describes that does not exist (a rejection with
   rework). The roadmap's and `CLAUDE.md`'s "9 of 9" sentence gives this run's result and date.

**Tests.** `bash -n` on both scripts; `npx playwright test -c playwright.docket-prill.config.ts --list`;
`cargo nextest run --workspace --run-ignored all -E 'package(tack-runner) & test(/^briefing_schema::/)'`
with prill on `PATH`;
then the whole local suite once, at the end, on the local model.

**Done when:** `docket-prill-e2e.sh` and `--check` pass against docket's and prill's current
checkouts, and fail with one sentence against a docket without the escalation record.

---

### B. Background work has one owner and stops

**Model:** Sonnet. **Waits on:** nothing.

**Why.**
- Two loops purge `execution_events` when `TACK_EXECUTION_RETENTION_ENABLE` is on:
  `spawn_artifact_and_decision_sweep` in `tack-api/src/execution_runtime.rs` (joined on stop) and
  `tack_app::retention::run` (spawned by `tack-cli/src/serve.rs`, its handle dropped).
- The six `tack-app` loops (`auto_start`, `github_sync`, `observability`, `due_soon`, `retention`,
  `remote_backup`) are near copies, spawned fire-and-forget: a panic is lost, and retention and
  observability lost their stop signal in Phase 79.
- Handlers spawn use cases nobody waits for: `merge::on_done` from the items handler (a GitHub merge
  cut off at shutdown), `record_mrp` in the runner-protocol handlers, and the spawns in the items and
  comments handlers.
- `runs::complete` commits, then runs `status_map::apply` and `rework::enqueue_rework` each in its
  own transaction; a crash between them loses the board move and the rework.
- Phase 79 deleted two tests with no replacement: the stale-lease alert logs once on the transition
  (`observability::log_transitions`), and a failing purge is retried next cycle (`retention::run`).

**Read first:** `tack-cli/src/serve.rs`, `tack-api/src/execution_runtime.rs`, the `run`/`tick` pair
of each loop, `tack-app/src/runs/complete.rs`, `ARCHITECTURE.md`'s background-work paragraph.

**Does.**
1. One helper in `tack-app` spawns a named tick on an interval into a `JoinSet`, stops on a shared
   `CancellationToken`, and logs a panic with the loop's name. Every `run` becomes a call to it;
   `serve.rs` owns the token and awaits the set on shutdown.
2. One owner per table: the event purge runs once, in `tack-app`. The artifact sweep and decision
   expiry move to `tack-app` ticks if `ArtifactStorage` can be handed in without HTTP types;
   otherwise only the artifact sweep stays in the runtime and its event sweep is deleted.
3. Handler spawns go on one tracked task set (tokio-util `TaskTracker`) that shutdown waits for,
   bounded.
4. `complete`: the status move and the rework enqueue run inside the completion transaction. Only a
   step that needs the network would justify an outbox; none is expected.
5. The two lost tests come back in `tack-app/tests/`.

**Tests.** `cargo nextest run --workspace -E 'package(tack-app) | (package(tack-api) & test(/execution_runtime|server/)) | (package(tack-cli) & test(/serve/))'`.

**Done when:** shutdown stops every loop and waits for every spawned use case; one loop deletes
from `execution_events`; a crash after the completion commit cannot lose the board move; and
`ARCHITECTURE.md` describes this.

---

### C. The docket and prill adapters read typed answers and say things plainly

**Model:** Sonnet. **Waits on:** nothing.

**Why.**
- `DocketAdapter::start` holds the `ensured` lock across the `ensure_pod` HTTP call, so one slow
  docket queues every docket run behind it.
- About ninety `value["key"]` reads fall back silently: an unknown escalation `kind` becomes
  `"approval"`; the API version is a `String` re-parsed on each check.
- Event kinds are bare strings; `"escalation.decided"` is written by the runner and read by the
  metrics SQL in `tack-db` with no shared constant.
- The timeline's detail text is internal: "approval answered: option opt_x", "decided alone:
  option x (policy foo, level L4)".
- The fake answers `apiVersion` "5" by default, but no docket with the escalation record answers 5,
  so some tests in `docket_v5.rs` exercise a combination that cannot exist; two of its tests sleep a
  fixed 700 ms.
- Tack's prill fixtures in `docs/contracts/mrp-v1/fixtures/prill/` say "copied verbatim" but lack
  the `coverage` stage, and prill's `m6-fields.json` was never copied, although Tack reads
  `nextSteps`; the `coverage` section is not translated.

**Read first:** `harness/docket.rs` (`start`, `follow`, the escalation reader, `hop_progress`),
`harness/docket/{client,probe,task}.rs`, the fake's `mod.rs` head, `verify/prill.rs`, the fixtures'
`README.md`.

**Does.**
1. `start`: check the set, drop the guard, call `ensure_pod`, insert on success; a per-team
   `OnceCell` is fine too.
2. Serde structs for every docket answer Tack reads (status, team, task view, escalation, trace
   event), with `#[serde(default)]` only where docket's spec says a field may be absent. An unknown
   kind is a timeline line saying so, never a guessed one. The probe keeps the version as a number.
3. Event-kind constants in `tack-protocol`, used by the runner and the metrics query.
4. Timeline text in plain words: what was asked, the label of the option a person picked, "docket
   decided this on its own". No option ids, policy names or level codes.
5. The fake answers "6" by default; tests of an impossible combination are deleted or retargeted;
   the two 700 ms sleeps use paused time.
6. Prill fixtures re-copied from prill's `contracts/` with prill's commit in their `README.md`;
   `coverage` is translated, or dropped by name with a test saying so; tests for a reject (exit 1),
   a refusal (exit 2) and a timeout with the fake prill, if any is missing.
7. `TESTING.md` and `runner-contract.md` stop saying "API 5"; `ARCHITECTURE.md`'s docket paragraph
   describes the escalation record.

**Tests.** `cargo nextest run --workspace -E 'package(tack-protocol) | (package(tack-runner) & test(/^docket_v5::|prill/)) | (package(tack-db) & test(/metrics/))'`.

**Done when:** two docket runs on different teams start without waiting on each other; every docket
answer is read through a struct; the timeline text has no ids; and the fixtures match prill's
contracts at the recorded commit.

---

### D. CI and the gate check what the docs say they check

**Model:** Sonnet. **Waits on:** B (both touch server start).

**Why.**
- Two tests start `tack serve` on a port picked by binding and dropping (`free_port`): one is in
  `embedded_runner_id.rs`, the other is the serve-warning test in `cli_test.rs`. Neither is in
  `.config/nextest.toml`'s serial list, so they can collide with the three that are.
- The server already learns its real address after `bind`, but logs only the requested one.
- Fixed waits: `embedded_runner_id.rs` sleeps 2 s; `RunWithAgentModal.test.tsx` waits 450 ms four
  times. `TESTING.md` claims none exist.
- `scripts/smoke.sh` in fake mode (free, deterministic, the whole claim-to-requeue pipeline) never
  runs in CI. `pre-push` runs `tsc` but no Vitest.
- A manually dispatched workflow runs only Chromium, while `TESTING.md` says all three browsers.
- The coverage floor (78.93) is explained differently in `ci.yml` and `TESTING.md`, and neither
  explanation adds up.
- `e2e/api.spec.ts` skips itself when no project exists, so it can pass without checking anything.

**Read first:** `.github/workflows/ci.yml`, `.githooks/pre-push`, `.config/nextest.toml`,
`tack-api/src/server.rs` `serve`, the two tests, `TESTING.md`'s CI sections.

**Does.**
1. After `bind`, the server logs one line with the bound address. The two tests start on port 0 and
   read that line; `free_port` goes, and so does every serial entry no longer needed.
2. The 2 s sleep becomes `tack_test_support`'s poll; the modal test uses `vi.useFakeTimers`.
3. The merge-tier e2e job runs `scripts/smoke.sh` in fake mode against the binary it built.
4. `pre-push` runs `vitest run --changed` against what the push sends.
5. A dispatched workflow runs every browser, as `TESTING.md` says.
6. `ci.yml` and `TESTING.md` give one formula for the floor; the number is re-measured at the
   Round 1 gate and written into both.
7. `api.spec.ts` creates the project it needs.

**Tests.** `cargo nextest run --workspace -E 'package(tack-cli) & test(/^(embedded_runner_id|cli_test)::/)'`; `npx vitest run`
on the modal test; `bash -n .githooks/pre-push`; `actionlint` if installed.

**Done when:** no test picks a port by bind-and-drop; `TESTING.md`'s "no fixed waits" is true;
smoke runs in CI; and the docs describe what CI runs.

---

### S. Screens show errors, plain words and disabled controls

**Model:** Sonnet. **Waits on:** nothing. **Owns** everything in `frontend/src` except
`shared/runWithAgent/` and `shared/execution/` (card R), plus two server sentences.

**Why.**
- There is no error boundary. Twenty-two `.catch(() => [] / null)` turn a failed load into a false
  empty state: the Automation ladder says "Autonomy settings are not available yet." when the load
  failed.
- Screens name prill (Merge rung, factory metrics), environment variables
  (`TACK_EXECUTION_DECISION_TOKEN` in the decision inbox, enrolment) and CLI commands (`docket init`
  on the Teams rung). The server's "prill not installed" sentence names `TACK_PRILL_PROGRAM`.
- Controls are hidden instead of disabled: "Browse…" on the web, cloud backup while unconfigured.
- `Button` has no way to show why it is disabled.
- Automation's folded sections lost their disclosure marker (a `flex` `<summary>`), and rung
  numbers 1–4 sit next to levels L3–L5.
- Accessibility: `Field` does not link its error or hint (`aria-describedby`), `Tabs` has no
  `aria-controls`, and `a11y.spec.ts` skips project settings, the item page, factory metrics, the
  create dialogs and the command palette.

**Read first:** `app/` (routes, layout), `shared/ui/Button.tsx`, `Field.tsx`, `Tabs.tsx`,
`features/settings/automation/`, `e2e/a11y.spec.ts`; `rg -n "catch\(\(\) =>" src` for the list.

**Does.**
1. An error boundary at the root and one per route, each with "Try again". Each silent catch
   becomes an error state saying what could not load, with a retry; Automation first.
2. `Button` takes a `reason`: visible text under or beside it, tied with `aria-describedby`. The
   hidden controls are shown disabled with theirs ("Available in the desktop app"; "Set up cloud
   backup first").
3. Screen text says "the verifier", never prill, and has no variable names or commands; a link to
   the administrator guide replaces them. The two server sentences
   (`tack-api/src/prill.rs`'s "not installed", `tack-app/src/autonomy.rs`'s "Only a docket run…")
   follow the same rule.
4. Automation: the marker is back; rung headings lose their numbers.
5. `Field` links error and hint; `Tabs` gets `aria-controls`/`aria-labelledby`; `a11y.spec.ts`
   covers the pages listed above.

**Tests.** `npx vitest run` on the folders touched; the a11y spec runs at the gate.

**Done when:** a failed load is never shown as empty; no screen names prill, a variable or a command;
no deferred control is hidden; and `a11y.spec.ts` covers the listed pages.

---

### R. The run surface

**Model:** Sonnet. **Waits on:** S (uses `Button`'s `reason`).

**Why.**
- "Run again at L3" is enabled only for a docket run, but the dialog it opens can pick another
  agent, and `start_run` keeps the level while `level_to_send` drops it for anything but docket: the
  run starts without the level and nobody is told. After it starts, the new run is not selected (no
  `onCreated`, unlike "Run again").
- The new events (`escalation.asked`, `.resolved`, `.expired`, `.decided`, `agent.answer_failed`)
  have no label in `EVENT_LABELS` and show their raw kind; a failed answer is drawn blue.
- A run that needs a person shows a warning badge with a red reason, may show the raw `needs_human`,
  and links neither to the question nor to the decision inbox.
- L3, L4 and L5 are shown on a button and in the dialog's note without saying what they mean.
- A run has no link of its own, and state changes are not announced (`aria-live`).

**Read first:** `shared/runWithAgent/` (`EventTimeline.tsx`, `RerunAtL3.tsx`, `RunWithAgentModal.tsx`,
`LatestRunSummary.tsx`), `shared/execution/ExecutionTimeline.tsx`, `attemptFormat.ts`,
`tack-app/src/runs/start_run.rs` around the `autonomy` field.

**Does.**
1. `start_run` refuses a level for an agent other than docket (400, "A level applies only to a
   docket run"). The dialog opened by "Run again at L3" keeps docket and shows the other agents
   disabled with that reason. The new run is selected.
2. Labels for every new event; a failed answer in the failure colour.
3. "Needs a person" is one state in one colour, with a link to the question; never the raw code.
4. Wherever a level appears, one plain sentence says what it allows (L3: you approve each step that
   needs it; L4 and L5 as the user guide words them).
5. `?tab=execution&run=<id>` opens a given run; run state changes are announced politely.

**Tests.** `npx vitest run src/shared/runWithAgent src/shared/execution`;
`cargo nextest run --workspace -E 'package(tack-app) & test(/start_run/)'`.

**Done when:** a level can only reach docket and is never dropped in silence; every timeline event
reads in words; and a run can be linked to.

---

### H. The routes, tools and upgrade nobody tests

**Model:** Haiku. **Waits on:** nothing. Writes tests only, following the files named as patterns.

**Why.** Six HTTP routes are named by no test: `/backup/remote/restore` (it overwrites the database),
`/backup/remote/verify`, `/debug/*`, `…/mrp/viewed`, the dependency `DELETE`, `/projects/{id}/items/tree`.
Three MCP tools are listed but never called in tests: `create_item`, `search_items`, `ping`. No
test upgrades a database from a published release through every migration. Calendar, list, sprints
and timeline have no unit test. `create_project_and_item` is defined four times in `tack-api`'s
tests, and a `flush` helper is copied into 37 Vitest files.

**Patterns to copy:** `tack-api/tests/handlers/crud.rs` (oneshot route tests),
`tack-cli/src/mcp/tests.rs` (wiremock), `tack-db/tests/migrations/`, `frontend/src/features/table/`
(a view's test).

**Does.**
1. One test per route above: its happy path and its refusal; for the remote backup, a local
   object store, and that restore refuses without a verified archive.
2. One wiremock test per MCP tool above.
3. A beta.9 database: build `tack` once at the `v0.1.0-beta.9` tag in a throwaway worktree, create a
   project with items, comments, a template and one run through its API, and commit the database
   file under `tack-db/tests/fixtures/`. The test copies it, migrates to now, and checks row counts
   and that the schema matches a fresh database's.
4. One render test per view (calendar, list, sprints, timeline): it shows the items it is given and
   its empty state.
5. `create_project_and_item` moves to `tack-api/tests/common/`; `flush` to `frontend/src/test/`.

**Tests.** Only the new or moved test binaries and files, by name.

**Done when:** every route in the router and every MCP tool is named by a test, and the beta.9
database migrates.

---

### E. The execution store is typed

**Model:** Sonnet. **Waits on:** Round 1 (B changed `complete`, R changed `start_run`).

**Why.** `tack-protocol` has `AttemptId`, `RunnerId`, `FencingToken` and `ExecutionState` with a
transition table tested against a frozen fixture, but `tack-db` and `tack-app` use them nowhere:
store inputs are `&str` and `i64`, JSON travels as text, and the state machine lives in 55 SQL
literals (`state IN ('leased','preparing',…)`); `validate_transition` has two callers.
`executions.rs` handlers still compare `row.state == "queued"`. `RunRequest` carries the selector,
level, permission policy and environment as `String`/`Value` though the protocol has their types.
56 `Utc::now()` calls in db, app and api bypass the clock, which is why `FixedClock` is defined
twice in production code. `canonical_json` exists twice with different key order.
`list_execution_attempts` builds its read model in the handler with one query per attempt and
swallows errors.

**Read first:** ADR 0028 decision 3, `tack-protocol/src/execution/lifecycle.rs`,
`tack-db/src/repo/execution/`, `tack-db/src/queries/execution_*`, `tack-app/src/runs/`,
`tack-api/src/handlers/executions.rs`.

**Does.**
1. Execution repo methods take and return the protocol's ids, token and state; JSON is encoded
   inside `tack-db`.
2. `lifecycle` gives the states a transition may start from; the store builds each guard from it,
   and one test walks every guarded `UPDATE` against the table. No state literal is left outside
   `lifecycle`.
3. Read-model rows carry `ExecutionState`; the handler string comparisons go.
4. `RunRequest` uses the protocol's selector, level and permission policy; `enqueue_run` is split
   while it is open.
5. The store takes `now` as a value where it took a clock; one `FixedClock`, in test support; one
   `canonical_json`.
6. Attempts are listed by one query in `tack-db/src/queries/`, errors returned.
7. `./scripts/regen-generated.sh` if any response type changed.

**Tests.** `cargo nextest run --workspace -E 'package(tack-db) | package(tack-app) | package(tack-protocol) | (package(tack-api) & test(/execution|^runner_protocol::/))'`.

**Done when:** outside tests, no SQL string in `crates/` spells an execution state; the guards come
from `lifecycle`; and ADR 0028 decision 3 holds.

---

### F. One wire format and one error model

**Model:** Sonnet. **Waits on:** E, in the same worktree.

**Why.**
- `tack-protocol` defines no claim, heartbeat, event-batch or completion structs: both sides build
  `json!{}` and read fields one by one, and only fixtures hold the contract. Every runner request
  carries the request id `"req_runner"`.
- Errors: `tack-app` returns `AppError`, `Refusal` (an HTTP status as `u16`), `String` and three more
  types. Converting `AppError` to `ApiError` drops its `StableErrorCode`. Every core rule (WIP limit,
  transition, cycle) answers the same 400. 94 `map_err(|_| …)` drop the cause.
- In the store, `StoreError::malformed` dresses a decode failure as an SQL error, `NotFound` names
  no entity, and 78 `unwrap`s in row mapping panic on a corrupt row.

**Read first:** `tack-protocol/src/` (errors, execution types), `tack-runner/src/transport.rs`,
`tack-api/src/handlers/runner_protocol.rs`, `tack-db/src/error.rs`, `tack-app/src/error.rs`,
`tack-api/src/error.rs`.

**Does.**
1. Claim, heartbeat, events and completion structs in `tack-protocol`, used with serde on both
   sides and checked against the existing fixtures; the field helpers go. The request id is a real
   one.
2. `StableErrorCode` knows its HTTP status; `AppError` carries a code; `Refusal` and `String`
   errors fold into it; every `ApiError` body has `code`; each core rule has its own code.
3. `StoreError` gains `Corrupt { table, column }` and `NotFound { entity }`; row mapping returns
   them instead of `unwrap`; `map_err(|_|` keeps its source.
4. `./scripts/regen-generated.sh`.

**Tests.** `cargo nextest run --workspace -E 'package(tack-protocol) | package(tack-db) | package(tack-app) | package(tack-runner) | package(tack-api)'`:
this card touches every layer, so it runs those crates whole, once, at its end.

**Done when:** no runner message is built with `json!`; every error body has a code a client can
match on; and no row read can panic.

---

### I. One kit for cards, switches, confirms, radii and type

**Model:** Haiku. **Waits on:** S and R. Optional (Round 3).

**Why.** 457 arbitrary Tailwind values; `rounded-[28px]` written 57 times though `--radius-card`
exists; 28 font sizes; the card style rebuilt 33 times and drifted; eight native `confirm()`; 16 raw
`<select>` beside the kit's `Select`; `CreateItemModal` sits in `shared/ui`. The command palette uses
other labels and icons than the sidebar and lacks Agents.

**Does.** Radius and type tokens in `@theme`; `Card`, `Switch` and `ConfirmDialog` in `shared/ui`;
a codemod replaces the repeated values and elements; `scripts/check-tokens.sh` ratchets arbitrary
values to the new count; `CreateItemModal` moves to its feature; the palette is built from the
sidebar's list. No visual change beyond the drift it removes.

**Tests.** `npx vitest run` on changed folders.

---

### J. `Host` split and the environment read once

**Model:** Sonnet. **Waits on:** F. Optional (Round 3).

**Why.** `Host` has ten methods across GitHub, secrets, verifier, artifacts and websocket, with
`String` errors; one of them only calls back into `tack-app`. `AppConfig` lives in `tack-api` but
configures `tack-app`'s loops. 22 environment reads sit outside the config modules, and the CLI
passes the embedded runner's switch to `tack-app` through `unsafe set_var`.

**Does.** `Host` becomes small ports (GitHub, secrets, verifier, artifacts, board events) with
typed errors; the GitHub client and its base URL live in `tack-app`; config is read once and
passed as values; the `set_var` blocks go.

**Tests.** `cargo nextest run --workspace -E 'package(tack-app) | package(tack-cli) | (package(tack-api) & test(/host|^handlers::/))'`.

---

### K. The item and its read model are separate

**Model:** Sonnet. **Waits on:** J. Optional (Round 3).

**Why.** `Item` mixes the aggregate with read-model fields built by subqueries (`needs_review`,
`merge_note`, `team`, …); its column list is pasted nine times; adding a field touches about fifteen
files; `update_item` is dead outside tests; workflow and WIP rules run inside the store; row reads
drop bad values with `.ok()`.

**Does.** An `ItemCard` query for the projections; one column list; `UpdateItem::is_empty` replaces
the boolean chain; `update_item` goes; workflow and WIP checks move to `tack-core`, called by the
use case; a bad row is an error. `./scripts/regen-generated.sh`.

**Tests.** `cargo nextest run --workspace -E 'package(tack-core) | package(tack-db) | (package(tack-api) & test(/^handlers::items/))'`.
