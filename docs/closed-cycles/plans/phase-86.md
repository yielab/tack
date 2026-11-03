# Phase 86: a person sets a task's status, and the agents report its work

**Status: ACCEPTED 2026-11-02.** No ADR of its own: the decisions are below, and ADR 0033 decision 1
(a run that starts moves its item) carries a dated note that this phase reverses it. A removal and a
clarification, no feature added. Back end first (cards A, B), then the screen (card C). Keeps ADR 0032's
rule: nothing on the screen names a harness.

## The ask, in one screen

A task's **status** belongs to the person; its **work** belongs to the agents. Six places move the
status because a run did something, each with its own setting; one refused move leaves the board and the
run disagreeing with no word on screen. Remove them all, and make the two readings two things on screen.

## Decisions

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **Only a person changes a task's status**, by hand, by applying a change set (ADR 0034), or by importing a file. No run, review, completion, auto-start or finished child writes it | The column always means what a person last decided; it never moves by itself |
| 2 | **A task's work is read, never written.** It is derived from the task's newest run and shown wherever a task is: the card's chip, the item, the list, the strip's counts, Orchestration | A task in Backlog with a run Running says both, and neither is wrong |
| 3 | **Two fixed names on screen: "Status" and "Work".** A work word never reuses a status name, and a status control never carries a work word | "Ready for review" is the work; "In Review" is the column |
| 4 | **The moves are removed, with their settings.** "When a run starts" and "When a run finishes" go from Settings → Automation; `on_start_status`, `on_finish_status` and the run's `status_map_policy_id` go from the database, the API, the CLI, the MCP tools and the project file; a parent is no longer marked done when its last child is | One setting page less; a file or call that still names them is told they were ignored |
| 5 | **Auto-start no longer moves the task and no longer reads a column's limit.** It starts a task that is in the project's start status, says who does it, is not blocked and has never run, once. The limits that bound it are runs open and the token limits | A task started by itself stays where it was; running it again is a person's choice |
| 6 | **The decision above is the whole change.** Nothing replaces the moves: no suggestion, no rule | The Now panel shows the status with its control beside the work, and a person moves the card in one click |

**Why now.** The live suite of 2026-11-01 ended with seven refused moves (`In Progress` full at 5 of 5):
tasks stayed in Backlog while their runs worked, and tasks whose runs were over stayed in In
Progress and kept the column full for the next one. Two design audits of the screenshots found the
strip, the sidebar and the drawer giving three different counts for the same run.

**If you do nothing.** Each new move (the next harness's, the next policy's) adds a seventh place,
and a refused move stays a line in a log.

## Considered and not adopted now

**Status rules.** One list of rules per project, each "when this happens, and this holds, propose
these changes": a run starting moves its task to In Progress, a run with changes suggests In Review,
every child done marks the parent done. A rule would build a change set (ADR 0034) with the rule as
its actor, applied at once or left waiting for a person, so a refusal (a full column) is a visible
reason and not a log line; it would read the same events as a trigger (card R of Phase 85) and be written
in the project file. It is the way back to the moves of decision 4, chosen
by the person and not hard-coded. It is **future and optional**: no phase is opened, and
`docs/book/src/roadmap.md` lists it under "Future, not planned". If it is opened, it starts from this
phase's two nouns and writes only the status.

**A suggestion with no rule** ("move to In Review?" in the Now panel). It is the smallest of the
rules and also needs the choice of which status, which is a rule. Not adopted.

**Keeping the moves behind a setting that defaults to off.** It keeps the code, the six places and
the refusals, and only changes which people meet them.

## How every card runs

Phase 85's rules ("How every card runs" in `docs/closed-cycles/plans/phase-85.md`): read only the files the card
names; stop and report at about 150k tokens; run only the tests of what the card touched, the full suites
once per round on the merged tree; a card that changes the HTTP API runs `./scripts/regen-generated.sh`;
at most two cards build at once, each in its own worktree cut from `develop`; no card opens a window;
each card updates the guide paragraph its change makes true and adds its line to `CHANGELOG.md`'s
`[Unreleased]` (under Removed where it removes); one commit per card, `Type: what is now true`, no
attribution trailer. The Administrator and User guides carry no source paths, test names or phase numbers.

## Rounds

| Round | Cards | Gate |
|---|---|---|
| 1 | **A** | full suites once on the merged tree |
| 2 | **B ∥ C** | full suites once; the live suite (`onboarding-lab/e2e/`, git-ignored) |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | The moves are removed | Sonnet | `tack-app` `runs/move_on_start.rs` (deleted), `runs/complete/status_map.rs` (deleted) and its calls in `complete.rs` and `rework.rs`, `mrp.rs`, `runs/start_run.rs`; `tack-api` `runner_protocol.rs` (the start call), `handlers/decisions.rs`, `handlers/executions.rs`, `handlers/items.rs` (`propagate_parent_completion`), `handlers/outline.rs`; `tack-core` `workflow.rs` (`StatusMapPolicy`, `StatusMapEvent`), `outline.rs`, `models.rs`; `tack-db` a migration that drops `projects.on_start_status` and `on_finish_status`, and the queries that read them; `tack-protocol` `execution/types.rs`; `tack-cli` `cmd/runs.rs`, `execution.rs`, `mcp/runs.rs`, `mcp/tools_list.json`; regenerated API files; `docs/MCP.md`, the Administrator guide's runners page | — |
| B | Auto-start starts a task once and moves nothing | Sonnet | `tack-app` `auto_start.rs` (`start` writes no status), `tack-core` `auto_start.rs` (`Skip::Wip` and the column's limit leave `pick`; a candidate has never run), `tack-db` `queries/items.rs` (`waiting_with_someone_to_do_it` excludes a task that has a run), the explain route and its wording, the User guide's automation page | A |
| C | Status and Work on screen | Sonnet | `features/settings/panels/AutomationPanel.tsx` (the two folds go), `features/item-detail/` (the Now panel and the header: "Status" with its control, "Work" with its state), `features/board/` (the strip is titled "Work"; a card's chip is its work), `features/list/` (a Work column beside Status), `shared/ui/RunState.tsx` and `shared/runWithAgent/cardRun.ts` (no work word equals a status name), `shared/execution/api.ts` and `shared/runWithAgent/shared.ts` (the removed fields), the User guide's views and automation pages | A |

---

## Cards

### A. The moves are removed

**Why.** decisions 1 and 4. The six places and their settings are the thing to delete.

**Read first:** `runs/move_on_start.rs`, `runs/complete/status_map.rs`, `mrp.rs`,
`handlers/items.rs` `propagate_parent_completion`, `auto_start.rs` `start`; migration 098
(`on_start_status`); the places `grep -rn "status_map_policy_id\|on_start_status\|on_finish_status"
crates frontend/src docs` finds.

**Does.**
1. Delete the move on start, the move on completion (policy or `on_finish_status`), the move when a
   pack is accepted, and the parent's automatic completion. `after_update` keeps starting the merge,
   the recurrence and the GitHub sync when a person moves a task to Done.
2. Remove `on_start_status`, `on_finish_status` and the request's `status_map_policy_id` from the
   database (a migration, with the next free number), the stored request snapshot's reader (an old
   snapshot that names it is read and ignored), the HTTP API, the runner protocol, the CLI flag, the
   MCP tools' arguments and `tools_list.json`, and the project file. A project file or a call that
   still names them is accepted, ignored, and its answer says "ignored: the status is set by a person".
3. `StatusMapPolicy` and `StatusMapEvent` leave `tack-core`. The existing tests of the moves are
   deleted with them; every test of what a person's status change causes stays.
4. `docs/MCP.md`, the Administrator guide and the Developer guide stop describing the moves.

**Done when:** no run, completion, review, auto-start or child writes `items.status`; a person's move
still starts the merge, the recurrence and the sync; the old fields are ignored with that answer; the
full suites pass.

### B. Auto-start starts a task once and moves nothing

**Why.** decision 5: auto-start leaves its place by moving the task, which is what keeps it
from starting the same task again.

**Does.**
1. A task is a candidate when it is in the project's start status, says who does it, is not blocked
   and has no run (`waiting_with_someone_to_do_it` excludes a task with a request). `start` enqueues
   the run and writes no status.
2. `Skip::Wip` and the first in-progress column's limit leave `pick`; the explain route
   (`GET /api/projects/{id}/auto-start`) and its sentences lose the `wip` reason. The reasons that stay:
   a run is open, blocked, no one chosen, no computer, the token limit, the most runs open.
3. The User guide's automation page says a task starts once, where it is, and that a second run is the
   person's.

**Done when:** a task started by itself is in the status it was in; it is not started again when its
run ends; a full column does not stop it; the tick's other tests pass.

### C. Status and Work on screen

**Why.** decisions 2 and 3.

**Does.**
1. Settings → Automation: "When a run starts" and "When a run finishes" go. One line under Start says
   the status is a person's and a run never changes it.
2. The item: the header names **Status** with its control, and the Now panel names **Work** with its
   state and the one action. The two are never in one pill.
3. The board: the strip is titled "Work"; a card's chip is its work; the column is its status. The
   List gets a Work column beside Status, sortable and filterable by work. Orchestration, the inbox
   and the sidebar count work, and say "Work" where they name a count.
4. Wording: no work word equals a status name; the check is a test over `RunState`'s labels and the
   workflow templates' status names (the templates' own names stay).

**Done when:** a task in Backlog with a run Running shows "Status Backlog" and "Work Running" and
nothing contradicts; the strip, the sidebar and the list give one count for the same run; the settings
page has no move; the two guide pages say it in plain words.

---

## Left out, and why

Status rules, a suggestion in the Now panel and a switch that keeps the moves are under "Considered and
not adopted now" above; status rules are in the roadmap under "Future, not planned".

## Evidence

- `crates/tack-app/src/runs/move_on_start.rs` (a run starts, project `on_start_status`);
  `runs/complete/status_map.rs` (a run ends, the request's policy or `on_finish_status` when it left
  a patch); `mrp.rs` (a pack accepted, `done_on_mrp_accepted`); `auto_start.rs` `start` (the first
  in-progress status by order); `handlers/items.rs` `propagate_parent_completion` (the last child done).
  A sixth, `after_update`, writes no status: a status change starts the merge, the recurrence and
  the GitHub sync, and stays.
- The refusals: 7 `on start: transition refused, item untouched ... WipLimitExceeded` lines in one run of
  the live suite; the board of that run shows 3 Done items still in In Progress.
