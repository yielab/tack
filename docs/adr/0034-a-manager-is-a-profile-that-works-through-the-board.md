# ADR 0034: a manager is a profile that works through the board — it reads every team, changes the board through change sets, and talks with you in a thread

**Status: ACCEPTED 2026-11-01** (proposed and accepted the same day, with both open questions answered
yes). Executed by Phase 85 (`docs/plans/phase-85.md`). Amends ADR 0004 (an agent becomes a principal;
Tack stays single-operator) and ADR 0032 decision 4 (decision 4's Create rung). Builds on ADR 0033: the Orchestration view is what a person sees of the teams;
this ADR gives an agent the same knowledge and a way to act on it.

**The ask.** A team that manages — reads what every other team is doing, decides what comes next,
gives each task to whoever should do it, and talks it over with you — built from parts Tack already
has, without a new kind of agent and without a door around the board.

**The rule behind every decision.** There is no "manager" in the code. Every part below is general:
a credential with scopes, a change set, a team's activity, a digest, a thread, a trigger. A profile
becomes a manager by being given them; the word "manager" exists only as a preset on screen. Every
part is harness-neutral (ADR 0032): an adapter reports what its agent can do, and the screen shows the
rest disabled with its reason.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **The board is the manager's only door.** A manager reaches Tack through Tack's MCP server and nothing else. It delegates by saying who does a task; the task starts through auto-start, so the ladder, the WIP limits, the token limits and the team's merge ceiling apply to what a manager sets in motion exactly as to anything else. A manager is never given a team runtime's control plane (for docket: `resolve`, `task_pregrant`, `delegate`, `dispatch`), and an agent credential cannot start a run directly | A manager's work is on the board, where you can see it, and nothing it does skips a rule a person's work obeys |
| 2 | **Every caller is a principal** (amends ADR 0004). A request carries `Principal {kind: operator, agent, runner or system; id; scopes}`. An **agent credential** is issued in Settings → Agents, stored hashed, expires after 90 days, rotates and is revoked like a runner's credential. Its scopes are a list of projects times `read`, `propose`, `act`, `converse`. Every write to items, comments, dependencies and change sets records its principal; a comment an agent writes shows the credential's name. The operator's token keeps every scope. Resolving a decision still needs the decision token, which no agent credential is or holds. Tack stays single-operator: a credential is a key you hand to an agent, not an account | Settings → Agents → **Credentials**: name, projects, what it may do, last used, Rotate, Revoke. The activity says "Manager (credential) moved X to Ready" |
| 3 | **An agent changes the board only through a change set.** A change set is a list of operations: **create** (a plan-v1 entry: title, type, role, team, parent, depends on), **who does it**, **priority**, **move**, **depend**, **comment**, **due**, each checked as the person's own route checks it. It has a reason, the run or thread it came from and an idempotency key, and is applied in one transaction or not at all. Under an agent credential, the MCP write tools (`create_item`, `create_outline`, `update_item`, `move_item`, `add_comment`) become change sets and answer "applied" or "waiting for a person". A run's proposal (plan file, records, uploaded file; ADR 0032 decision 3) becomes a change set of creates. One panel shows every change set waiting, as the tree of ADR 0033 decision 3 | One place says "the manager wants to move 3 tasks, give 2 to the code team and create 4"; you tick, edit, apply or reject |
| 4 | **A project says how far an agent may go.** One more rung in Settings → Automation, **Agents on the board**: **Read** (reads and talks, changes nothing), **Propose** (the default: every change set waits for a person), **Act** (who does it, priority, move, depend, comment and due apply at once; creates wait), **Create** (creates apply too, at most N per change set and M per day). The stricter of the project's rung and the credential's scopes wins. **Create** amends ADR 0032 decision 4: it is off until a project chooses it, and its two caps have no default above zero | The same ladder you already read for runs, one rung for what agents may do to the board |
| 5 | **Tack knows what every team is doing, in its own words.** A team's **activity** is the tasks it holds — including those the board did not start, marked "outside the board" — each with its state, the step it is in (name, role, model, since, cycle), what it waits on, and its finished steps with summary, verdict, assumptions, questions and tokens. A harness reports `features.activity` (supported, or not with its reason). The runner reads it with a cursor and sends it to Tack, which keeps the current state and a 14-day window. ADR 0033's Flow shows it to a person; the MCP tool `team_activity` gives it to an agent | Flow also shows a team's work that did not start on the board; the manager knows what every team is on without opening a run |
| 6 | **A digest before detail.** `GET /api/digest?projects=` returns, for the projects a caller may read: items per status, what waits on a person and since when, blocked chains, tasks no team will pick up, stale items, runs failed since a time, tokens used against the limits, each team's current work and queue, and the change sets waiting. It is computed, never written by a model, and bounded in size. The MCP tool `digest` is the first call a manager makes; the detail tools follow | A manager's round costs a few thousand tokens, not a read of the whole board |
| 7 | **You talk with a manager in a thread.** A **thread** belongs to a project or to a credential's projects, has one profile and is open or closed; a **message** has a principal, a body and references to items, change sets and decisions. You start a thread or write in one; the manager may write too (a report, an alert) with the `converse` scope. Each message of yours starts one **turn**: a run of the thread's profile whose input is the thread's recent messages and the digest. A harness reports `features.converse`: **continues** (it keeps the context of the previous turns) or **replays** (a new run with the transcript; every agent can). The reply is posted to the thread, the change sets the turn made are shown inside it with their buttons, and a question the turn asks is the decision it already is, shown inline. A thread has a cap of turns, every turn the token limit of its profile, and a thread with no message for 7 days closes | A **Manager** panel: you write "what is blocking the launch?", it answers with the digest's facts and offers a change set; you apply it in the thread |
| 8 | **A manager wakes on events.** A **trigger** is an event (an item created or moved, a run finished, failed or waiting on a person, a decision opened, a change set decided, a thread message, a team's activity, or a schedule), a filter, the profile to run, a quiet period and a daily cap. One run takes every event since the last one as its input. An event a principal caused never starts a run of that same principal | Settings → Automation → **Triggers**; the preset gives a manager three: a message in its thread, a run that failed or waited on you over an hour, a weekday stand-up at 09:00 |

**The preset.** Agents → **Add a manager** chooses a profile (any agent: a docket team, Claude Code,
Codex, opencode), issues its credential with `read`, `propose` and `converse` on the chosen projects,
writes its MCP configuration, opens its thread, adds the three triggers, and leaves each project's
rung at Propose. Everything it creates is the general parts above and can be changed one by one.

**Why now.** The analysis of 2026-11-01 found that Tack already holds every fact a manager needs and
every lever it would pull (who does it, auto-start, the ladder, proposals, decisions), but that an
agent with Tack's token is the operator: unscoped, unrecorded, able to create and change items
without anyone choosing it (`create_item`, `create_outline`, `update_item`), while a run's proposal
cannot. It cannot see the teams (`GET /api/teams` is not an MCP tool), cannot assign (no role,
profile or dependency in the MCP tools), cannot be talked to (decisions only answer the question a
run asked), and wakes only by a person or a recurring task. docket, for its part, keeps pods isolated
by design, deleted its own org manager because nothing ran it, and puts supervision in the plan of
record.

**If you do nothing.** A person remains the only manager. An agent given Tack's MCP to help is either
read-only by convention or the operator by token, and its changes look like yours.

## Decided by the maintainer (2026-11-01)

1. **The Create rung exists.** It amends ADR 0032 decision 4 ("nothing proposed becomes an item
   without a person"): off by default, with a cap per change set and a cap per day.
2. **A credential may name "every project".** The screen shows the projects it reaches today and
   says that it will reach every project created after.

## What this asks of docket

None blocks a card; each is read when present, and the screen says so when absent. All are pulls, so
they stay inside docket's own ADR 0027 (no push, the consumer polls a cursor) and ADR 0016 (every
conversation is bound to a task and bounded).

1. **`current` on a running task** in `GET /tasks/<pod>/<id>`: step id, role, member, model,
   `startedAt`, cycle. Today it is derived from the trace (the newest `hop_started` with no
   `hop_finished`).
2. **`updatedSince` on `GET /tasks/<pod>`**, a cursor over every task a pod holds with its `channel`,
   so the runner sees the work a routine, Telegram or the CLI started, not only Tack's.
3. **`GET /pods/<p>/memory`**: the approved entries, read only. Today the CLI alone reads them.
4. **A task that continues.** A task created with `conversation: {maxTurns}` ends each turn
   `waiting_input` with its answer as output, and `POST /tasks/<pod>/<id>/messages {content, actor}`
   starts the next turn on the same step history (already kept per member, pod, task and step). Until
   it exists, a docket team converses by replaying.
5. **A read-only control plane:** `docket start --mcp --read-only` and a read-scoped serve token. Tack
   does not use them (decision 1); they keep anyone from wiring `resolve` into a pod by accident.
6. **A pod's MCP server outside the jail**, with a listed perimeter as a service has, so a lights-out
   pod (network `none`, isolation on) can reach `tack mcp`. Only a docket manager at lights-out needs
   it; card G measures whether it does.

## Evidence (read 2026-11-01 on `develop` at `fb936d5a`, docket at `00e2b25d`)

| Fact | Where |
|---|---|
| One operator token; the principal is `operator:token:<sha8>` or `operator:local`, injected only on the execution, fleet and decision routes | `tack-api` `middleware.rs` `operator_principal_value`, `inject_operator_principal`; ADR 0004 |
| An MCP call is marked `FromMcp`, read only by the merge on Done | `middleware.rs`; `handlers/items.rs` `update_item`; `tack-core` `autonomy.rs` `Hold::FromMcp` |
| A comment's `author` is free text, up to 200 characters | `tack-core` `models.rs`; comments table |
| The only scoped credential is the decision token; runner credentials are hashed, expire after 90 days and rotate | `handlers/decisions.rs` `require_decision_token`; `tack-app` `runs/enroll.rs`; `runner_auth.rs` |
| A proposal is only an artifact; the accept endpoint creates items in several writes and only creates | `handlers/items.rs` `create_subtasks_from_plan`; `PlanPanel.tsx` |
| The MCP server is one `match` and a separate spec list; card text is fenced in `get_item` and `search_items` only, by a second copy of the fence | `tack-cli` `mcp.rs` `dispatch_tool`, `tool_specs`, `fence_card_text`; `tack-protocol` `execution/types.rs` |
| No MCP tool sets a role, a profile, a dependency, or reads teams, decisions or an item's agent context | `mcp.rs`; routes in `tack-api` `router.rs` |
| Board events are item, sprint and config changes; run state is not on the bus; recurrence fires on Done | `handlers/websocket.rs` `BoardEvent`; `tack-app` `ports.rs`; `handlers/items.rs` `create_next_recurrence` |
| A harness reports optional features (`proposals` is the precedent) and adapters expose optional channels | `tack-protocol` `capabilities.rs` `FeatureCapabilities`; `tack-runner` `engine.rs` `HarnessAdapter` |
| docket: pods isolated, no org-wide queue; cross-pod dispatch refused | docket `docs/AGENT-TEAMS.md`; `runner.py` |
| docket: the org manager and the portfolio manager were deleted because nothing ran them | docket ADR 0022 |
| docket: supervision lives in the plan of record, which polls; no push, no aggregate statistics | docket ADR 0027 §4, §10 |
| docket: no free chat; every conversation is bound to a task and bounded; the plan of record owns the task graph | docket ADR 0016 |
| docket: one serve token, no caller identity; MCP `resolve` checks no caller; `delegate` caps text at 500 characters | docket `interfaces/http/auth.py`; `tasks/application/escalations.py`; `interfaces/cli/commands/mcp.py` |
| docket: the step a running task is in is visible only in the trace | docket `routes/traces.py`; `hops/application/compose.py` `hop_started` |
| docket: an answered question re-runs the same step on its own history | docket `tasks/application/escalations.py`; `kernel/ids.py` |

## Considered and not adopted

- **A manager inside docket** (an org pod that reads the other pods). docket deleted exactly that; its
  pods are isolated by design; and the board would not see the work the manager set in motion.
- **A manager that calls a team runtime directly.** Tasks it created would be invisible to the board,
  and docket's `resolve` would let it approve another team's questions.
- **Push from docket.** Its ADR 0027 refuses it; the runner's cursor is enough at this scale.
- **A digest written by a model inside Tack.** Tack calls no model (ADR 0078); the digest is facts,
  and the manager writes the prose.
- **User accounts.** ADR 0004 stands: a credential is a key for an agent.
- **An agent credential that starts runs.** Teams start through auto-start, so every rule that bounds a
  run bounds the manager's.
- **A hierarchy of managers.** Scopes compose; nothing is built for it until it is asked for.
- **Threads as comments.** A comment belongs to one item and has no turns; a thread spans the
  projects its profile reaches and each message is a turn.
