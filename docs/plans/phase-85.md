# Phase 85: a manager works through the board

**Status: ACCEPTED 2026-11-01; ADR 0034 holds the decisions,** both of its questions answered yes (the
Create rung exists, off by default; a credential may name every project). Starts after Phase 84's cards S (a
project's runs with their step), I (the accept panel as a tree) and J (one inbox) are in `develop`.
docket's six asks (ADR 0034 "What this asks of docket") block no card: each is read when present and
said on screen when absent. Tack edits nothing in docket's repository.

**One rule for every card (ADR 0034).** No type, table, route, tool or screen is called "manager"
except the preset of card R. Every card builds a general part — principal, change set, activity,
digest, thread, trigger — that any profile can be given. Everything stays harness-neutral (ADR 0032):
a capability says what an agent can do, and a control it cannot serve is shown disabled with its
reason.

## The shape

```
            operator ─────────── thread (T, TU) ───────────┐
                │                                          │ turn = a run of the profile
                ▼                                          ▼
   ┌──────────────────── Tack (plan of record) ────────────────────┐
   │ principal + scopes (P)    change sets + rung (C, U)           │
   │ domain events (E) ──► triggers (R)                            │
   │ team activity (A) ──► digest (D)                              │
   │ MCP: tool table (M) ──► agent tools + writes as change sets (W)│
   └──────────▲──────────────────────────────┬─────────────────────┘
              │ activity, cursor             │ who does it → auto-start → run
        runner (adapter: features.activity, features.converse)
              │
        any harness: docket team · Claude Code · Codex · opencode
```

The manager is one profile among the rest at the bottom. It reads and writes only through the MCP
row, under its own principal; the teams it delegates to start through auto-start like any other task.

## How every card runs

Phase 82's rules, unchanged (`docs/closed-cycles/plans/phase-82.md`, "How every card runs"): read only
the files the card names, stop and report at about 150k tokens; run only the tests of what the card
touched; the full suites once per round on the merged tree; a card that changes the HTTP API runs
`./scripts/regen-generated.sh`; at most two cards build at once, each in its own worktree cut from
`develop`; no card opens a window; each card updates the guide paragraph its change makes true and
adds its line to `CHANGELOG.md`'s `[Unreleased]`; one commit per card, `Type: what is now true`, no
attribution trailer. CI never needs docket: the fake docket in `crates/tack-runner/tests/fake_docket/`
answers every new shape, and G measures the real one.

## Rounds

| Round | Cards | Gate |
|---|---|---|
| 0 | **P ∥ M**, then **E** | full suites once on the merged tree |
| 1 | **C ∥ A** | full suites once |
| 2 | **D ∥ U** | full suites once |
| 3 | **W** | full suites once |
| 4 | **T**, then **TU ∥ R** | full suites once |
| 5 | **G** | full suites once; the live suite of the three |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| P | Every caller is a principal; an agent has a credential | Sonnet | `tack-core` a new `principal.rs`; `tack-api` `middleware.rs` (the resolver that replaces `operator_principal_value`), the credential routes; `tack-db` migration and repo `agent_credentials`; `tack-app` issue, rotate and revoke (beside `runs/enroll.rs`); the writes of items, comments and dependencies (principal recorded); `features/settings/` a Credentials panel; ADR 0004's dated note; the administrator guide's access page | — |
| M | The MCP server is a table of tools | Sonnet | `tack-cli/src/mcp.rs` split into `mcp/` (`tool.rs` with `Tool {name, spec, kind, handler}`, one module per domain, `projection.rs`), the card-text fence taken from `tack-protocol` (the second copy deleted); no tool changes; `docs/MCP.md` structure note | — |
| E | Domain events, one port | Sonnet | `tack-app/src/ports.rs` (`BoardEvents` widened into `DomainEvents` with item, run, decision events), their emitters in `tack-app` and `tack-api`, the WebSocket mapping in `handlers/websocket.rs` unchanged for the browser | P |
| C | Change sets and the rung | Sonnet | `tack-core` a new `change_set.rs` (operations, validation, the rung's decision, pure); `tack-db` migration and repo `change_sets`; `tack-app` apply in one transaction with an idempotency key; `tack-api` routes (list, get, apply, reject, edit an operation); `create_subtasks_from_plan` becomes "a change set of creates, applied"; the project's `agents_on_board` rung and caps; regenerated API files | P |
| A | A team's activity | Sonnet | `tack-protocol` `TeamActivity`, `FeatureCapabilities.activity`; `tack-runner` `harness/docket/` (an activity reader from the trace cursor and the task list; `current` and `updatedSince` when present), the runner-protocol route that sends it; `tack-db` the current state and a 14-day window; `tack-api` `GET /api/teams/{id}/activity`; the fake's task-list routes; the developer guide's runs-and-teams page | — |
| D | The digest | Sonnet | `tack-core` a new `digest.rs` (pure, bounded); `tack-db` the queries behind it; `tack-api` `GET /api/digest`; regenerated API files | A, C |
| U | One panel for change sets, one rung on screen | Sonnet | `shared/runWithAgent/PlanPanel.tsx` (after Phase 84 I) generalized to every operation; a new `features/changesets/` waiting list; `features/settings/panels/automation/` the "Agents on the board" rung; the user guide's "When a run proposes items", renamed "When an agent proposes changes" | C |
| W | The MCP tools a manager needs | Sonnet | `tack-cli/src/mcp/`: `digest`, `team_activity`, `list_teams`, `list_roles`, `get_change_set`, `propose` (a change set as such); `create_item`, `create_outline`, `update_item`, `move_item`, `add_comment` become change sets under an agent credential; `create_execution` refused under one; `docs/MCP.md` | M, C, A, D |
| T | Threads and turns | Sonnet | `tack-core` a new `thread.rs` (the window of messages a turn reads, the caps, pure); `tack-db` migrations and repos `threads`, `thread_messages`; `tack-app` a turn starts a run of the thread's profile with principal `thread:<id>` and posts its reply; `tack-protocol` `FeatureCapabilities.converse` (`continues`, `replays`); every adapter reports `replays`, docket `continues` when its ask 4 is present; `tack-api` thread routes; MCP `read_thread`, `post_message` | P, E, W |
| TU | The thread on screen | Sonnet | a new `features/threads/` (panel, message, the change sets and decisions inline using U's and Phase 84 J's components), the sidebar entry, the user guide's new "Talking with a manager" page | T, U |
| R | Triggers and the preset | Sonnet | `tack-core` a new `trigger.rs` (match, quiet period, daily cap, the rule that a principal's own events never start its runs, pure); `tack-db` migration and repo `triggers`; `tack-app` a `triggers` tick through `Loops::spawn_tick`; `features/settings/panels/automation/` Triggers; `features/agents/` "Add a manager" | E, T |
| G | The live suite proves it | Sonnet | `onboarding-lab/e2e/` (git-ignored): a manager profile on My SaaS reads the digest, proposes a change set in its thread, the operator applies it, two teams start; records which docket asks it found and whether a lights-out docket manager reaches `tack mcp` (ask 6); `TESTING.md`'s live-suite section | all |

---

## Cards

### P. Every caller is a principal; an agent has a credential

**Why.** Every request is the operator today, so an agent with Tack's MCP is the operator, and its
changes look like a person's. ADR 0034 decision 2.

**Read first:** ADR 0004; ADR 0034; `tack-api` `middleware.rs` (`require_token`,
`operator_principal_value`, `inject_operator_principal`, `FromMcp`), `router.rs` (the layers);
`tack-app` `runs/enroll.rs` and `handlers/runner_protocol/runner_auth.rs` (the pattern to copy);
`handlers/decisions.rs` (`require_decision_token`, unchanged).

**Does.**
1. `Principal {kind, id, scopes}` in `tack-core`; `Scope` is a project list (or every project) times
   `read`, `propose`, `act`, `converse`. Pure checks: `may(principal, project, capability)`.
2. One resolver in the middleware: the operator's token gives an operator with every scope; an agent
   credential (`tack_agent_<random>`, stored hashed) gives an agent with its scopes; it goes into the
   request's extensions on every route, not only the execution ones.
3. `agent_credentials`: name, hash, scopes, created, expires (90 days), last used, revoked. Issue
   shows the secret once; rotate; revoke.
4. Writes to items, comments and dependencies record the principal; an agent's comment shows the
   credential's name and ignores a supplied `author`.
5. A route an agent's scopes do not cover answers 403 naming the scope it lacks.

**Done when:** an agent credential reads only its projects, cannot resolve a decision, and its writes
show its name; the operator's token behaves as before; ADR 0004 carries a dated note.

### M. The MCP server is a table of tools

**Why.** Card W adds about ten tools; today a tool is a branch of one `match` and a separate entry in
a spec list that can drift from it.

**Does.** `Tool {name, spec, kind: Read | Write, handler}` in a table; modules `items`, `projects`,
`runs`, `fleet`; `projection.rs` for the compact rows; the fence from `tack-protocol`, applied to every
piece of card text, `get_execution` included. No tool's name, arguments or answer changes; the existing
MCP tests pass untouched.

**Done when:** `tools/list` lists the same tools in the same order with the same schemas; one place
declares each tool.

### E. Domain events, one port

**Why.** Triggers (R) and turns (T) wake on run and decision events that today never reach the bus.

**Does.** `DomainEvents` replaces `BoardEvents` in `tack-app/src/ports.rs`: item created, updated,
moved, deleted; run started, finished, failed, waiting on a person; decision opened, resolved; each
with its project and its principal. The WebSocket keeps sending the browser what it sends today.

**Done when:** every event above is emitted once with its principal, tested at the port.

### C. Change sets and the rung

**Why.** ADR 0034 decisions 3 and 4: the only way an agent changes the board.

**Read first:** ADR 0034; `handlers/items.rs` (`create_subtasks_from_plan`, `check_plan_links`,
`update_item`), `handlers/dependencies.rs`, `handlers/roles.rs`; `docs/contracts/plan-v1/`.

**Does.**
1. `ChangeSet {id, project, principal, origin (run, thread, mcp), reason, idempotency_key, status:
   waiting | applied | rejected | partly, ops}`; operations `create` (a plan-v1 entry; refs between
   new entries and to existing ids), `who_does_it`, `priority`, `move`, `depend`, `comment`, `due`.
2. Validation in `tack-core`, using the rules the person's routes use (workflow, WIP, cycles in
   dependencies).
3. Apply in one transaction; a repeated key returns the first answer.
4. The rung `agents_on_board` (`read`, `propose`, `act`, `create` with two caps) decides, per
   operation, apply now or wait; the stricter of the rung and the credential wins.
5. `create_subtasks_from_plan` builds a change set of creates and applies it: one write path.

**Done when:** a change set with a cycle, a status the workflow lacks or a full column is refused
whole with every reason; an applied one is one transaction; at Propose every change set waits; at Act
creates wait and the rest apply.

### A. A team's activity

**Why.** ADR 0034 decision 5. Phase 84 card S gives a project's runs and their step; the manager also
needs the work a team holds that the board did not start, and the steps it already finished.

**Read first:** ADR 0034; `tack-protocol` `capabilities.rs`; `tack-runner` `harness/docket.rs`
(`relay_trace`, the cursor), `harness/docket/client.rs`, `probe.rs`; docket's `GET /tasks/<pod>`,
`GET /traces/<pod>` (read only); Phase 84 card S's route.

**Does.**
1. `TeamActivity {team, tasks: [{ref, on_board (item id or none), state, current_step {name, role,
   model, since, cycle}, waits_on, finished_steps [{name, verdict, summary, assumptions, questions,
   tokens}]}]}` in neutral words; `features.activity`.
2. docket's adapter reads the task list and the trace with a cursor per team; it reads `current` and
   `updatedSince` (asks 1 and 2) when present and derives them otherwise.
3. The runner sends activity to Tack on the runner protocol; Tack keeps the current state and 14 days.
4. `GET /api/teams/{id}/activity`; other harnesses report `activity` unsupported with their reason.

**Done when:** a task started in docket by its CLI shows as "outside the board" with its step; a
finished step shows its verdict and summary; the fake answers with and without the asks.

### D. The digest

**Does.** `digest(projects, since)` in `tack-core`, pure and bounded (a fixed number of rows per
section, "and N more"): items per status; what waits on a person and since when; blocked chains;
tasks no team will pick up and why; stale items; runs failed since; tokens against the limits; each
team's current work and queue (from A); change sets waiting (from C). `GET /api/digest?projects=&since=`
filtered by the caller's scopes.

**Done when:** the digest of the fixtures' board is stable byte for byte and under its size bound.

### U. One panel for change sets, one rung on screen

**Does.** The accept panel (after Phase 84 card I) shows any change set: a tree of creates and a list
of the other operations, each editable and tickable, with its reason and origin. A waiting list in
the sidebar's inbox (Phase 84 card J). The rung "Agents on the board" in Settings → Automation, in
the ladder's form.

**Done when:** a change set of every operation kind can be applied in part or rejected from the panel.

### W. The MCP tools a manager needs

**Does.** New tools: `digest`, `team_activity`, `list_teams`, `list_roles`, `get_change_set`,
`propose`. Under an agent credential, `create_item`, `create_outline`, `update_item`, `move_item` and
`add_comment` build a change set and answer "applied" or "waiting for a person (id)"; `update_item`
gains `who_does_it` and `depends_on`; `create_execution` answers that an agent starts work by saying
who does it. Under the operator's token nothing changes.

**Done when:** an agent credential cannot change the board except through a change set, and every new
tool returns the compact projection.

### T. Threads and turns

**Read first:** ADR 0034 decision 7; `tack-app` `runs/start_run.rs` (`start_run`, the principal);
`tack-runner` `engine.rs` (`HarnessAdapter`, `decision_channels`); docket's `consult` and
`escalations` (read only).

**Does.**
1. `Thread {id, scope (project or credential), profile, status}` and `Message {thread, principal,
   body, refs}`.
2. A message from the operator starts a turn: a run of the profile with principal `thread:<id>`,
   input the window `tack-core` computes (the last messages whole, older ones as their first line) and
   the digest. The run's final message is posted as the reply; change sets with origin `thread:<id>`
   are referenced in it.
3. `features.converse`: every adapter reports `replays`; docket reports `continues` when ask 4 is
   present and then continues the same task.
4. The caps: turns per thread, the profile's token limit per turn, closing after 7 idle days.
5. MCP `read_thread` and `post_message` (scope `converse`).

**Done when:** three turns of a thread reach the profile with the earlier turns, through replay with
the fake and through continue when the fake offers it.

### TU. The thread on screen

**Does.** The thread panel: messages, a turn in progress, the change sets inline with U's panel, a
question as Phase 84 J's decision; the sidebar entry; the user guide's "Talking with a manager".

### R. Triggers and the preset

**Does.**
1. `Trigger {event, filter, profile, quiet_seconds, daily_cap, enabled}`, plus a schedule
   (weekdays and a time). One run per firing takes every event since its last run.
2. The rule in `tack-core`: an event caused by a principal never fires a trigger that runs that
   principal.
3. Settings → Automation → Triggers.
4. Agents → "Add a manager": the profile, a credential (`read`, `propose`, `converse`), the MCP
   configuration to paste or write, the thread, three triggers, the rung left at Propose.

**Done when:** a burst of ten failed runs fires one manager run with ten events; a change set the
manager applied does not wake it; the preset produces only general parts.

### G. The live suite proves it

**Does.** In the git-ignored `onboarding-lab/e2e/`: a manager profile on My SaaS through docket;
`digest` first; a change set proposed in its thread; applied by the operator; two teams start through
auto-start; the suite reports which docket asks it found, and whether a lights-out docket manager
reaches `tack mcp` (ask 6). `TESTING.md` names the new tests.

## Left out, and why

Each is in ADR 0034 "Considered and not adopted": a manager inside docket; a manager calling a team
runtime; push from docket; a digest written by a model; user accounts; an agent credential that starts
runs; a hierarchy of managers; threads as comments. There is no docs card and no measurement card.
