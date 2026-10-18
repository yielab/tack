# Runs, Teams and Automation

This page is for contributors who change how a run starts, which agent profile and team take it, or when Tack merges the result. It follows one automatic run from the moment a task waits in the trigger status to the merge, and names the function that does each step. After reading it you can find the code behind any rung of the Automation ladder and extend it.

People use the same features through [Automating a Project](../user-guide/automation.md); operators set up the machine side in [Installing and Operating Runners](../admin-guide/runners.md). The decisions are ADR 0080 (teams, ladder) and ADR 0079 (merge authority); see [Architecture Decisions](decisions.md).

## Who owns what

| Responsibility | Owner |
|---|---|
| What is done, in which order, with what priority and budget | Tack |
| Which profile, and through it which team, takes a task | Tack: the role, then the project default |
| Starting a run (by a person, by a rule, as rework) | Tack. docket never enqueues on its own |
| Running the agent and its team, the jail, the per-task limits | docket |
| Approvals and questions inside a run | docket raises them; Tack shows them and sends the answer |
| The verdict, the stages, the signature | The verifier (prill is one) |
| Push, pull request, merge, level, ceiling, money | Tack |
| Parallelism | Tack's runner (several attempts) and docket's teams (one task per team at a time) |

## The life of an automatic run

The numbered steps are what happens when a project has auto-start on and a task with a role reaches the trigger status. A run started by hand skips steps 1 to 3 and joins at step 4 through `POST /api/executions`.

1. **The tick.** `tack_api::auto_start::run_auto_start_task` is spawned beside the GitHub poll in `server.rs`. It calls `tick` every `AppConfig.auto_start_seconds` (`TACK_AUTO_START_SECONDS`, default 30, `0` returns at once and starts nothing). The loop is its own task because the GitHub loop does not run without a token. `tick` lists the projects with `auto_start_enabled = 1`, not archived, with an `auto_start_status`, and calls `project_tick` for each.
2. **The candidates.** `project_tick` reads the tasks in that status that have at least one role (`item_roles`). It finds the project's first in-progress status (lowest `order`) and the room under its WIP limit, the requests still open (`queued`, `leased`, `preparing`, `running`, `waiting_decision`), whether the daily budget is spent (`tack_orch::scheduler::wiring::budget_reached`), and each task's unfinished blockers (`DependencyGraph::unfinished_blockers`). For each task it runs `resolve_profile` and `default_selector` (step 4) to learn whether a profile and a machine exist.
3. **The choice.** `tack_core::auto_start::pick` is pure. It sorts by priority, then `sort_order`, and walks the list once. A task is skipped for the first reason that applies, in this order: `OpenRequest`, `Blocked(n)`, `NoProfile`, `NoMachine`, `Budget`, `MaxOpen`, `Wip`. `max_open` is the project's cap minus the runs already open; the WIP room is shared by the tasks picked in this pass. Skips are logged at debug level only; nothing is written to the task.
4. **The request.** `start` builds a `CreateExecution` with `idempotency_key = "auto-start:<item>:<version>"` and `metadata.started_by = "auto_start"`. It names no profile, agent or selector. It sets `permission_policy` to the profile's tools for the agent and `network` from the profile (`tool_policy.network`, `tack_core::run_defaults::profile_network`; absent is `false`), and adds `approvals: "ask"` unless every machine the run may land on reports the agent's `sandbox` as `bwrap` or `harness` (`sandboxed_everywhere`). That is the Run dialog's rule. `create_as` then resolves the rest as for any other run, with the principal `auto_start`:
    - `resolve_profile` calls `tack_core::run_defaults::resolve` (next section) and records the source in `metadata.profile_source`.
    - The agent is the profile's `tool_policy.agent`, else the project's `default_harness`; with neither, the request is refused with `select_an_agent`.
    - `default_selector` returns the profile's fleet, else the only active runner, else an error.
    - `load_profile` snapshots the profile, fills `permission_policy.network` from the profile's `tool_policy.network` when the request does not carry the key (a request that says `true` or `false` keeps it), and resolves the run's limits; `enqueue_as` writes the request. A replay answers the same request and the tick does nothing more.
5. **The move.** Only after the request exists, `start` moves the task to the first in-progress status with `update_item_atomically`, broadcasts `BoardEvent::ItemUpdated`, and writes the system comment "Started by itself: it reached X with the role Y" through `merge::tell`.
6. **The claim.** The scheduler (`tack-orch`) matches the request to a runner. A request no runner can take waits with a reason and fails with `no_runner_can_claim` after `SchedulingPolicy.unclaimable_fail_after`, 30 minutes by default. The runner claims it when one of its attempt slots is free.
7. **The slot.** On a claim the engine calls `ActiveAttempts::register`, which records the lease and returns a `watch::Receiver<bool>` that turns `true` when the server asks for a cancellation. One ticker sends a single heartbeat for every active lease (`transport.rs`, `heartbeat_all`) and passes each `lease_results` row to `ActiveAttempts::apply`. A folder worked in place stays one attempt at a time because `workspace.rs` locks the folder.
8. **The agent.** For docket the engine drives `DocketAdapter` (`validate`, `start`, `wait`, `cancel`, `reconcile`), described below. For every other agent it drives the shared `local_process` lifecycle.
9. **After the attempt.** Before it deletes the workspace the engine captures the evidence, runs the verifier and pushes the branch (`evidence.rs`, `verify.rs`, `git.rs`). The board opens the pull request.
10. **The merge.** `tack_api::merge::attempt` collects the facts and calls `tack_core::autonomy::evaluate`. At L4 and L5 the autonomy `tick` (`tack_api::autonomy::tick`) does this by itself. At L3 only the Merge button does, or the project's merge-on-Done option (`merge::on_done`). The ceiling is one of the holds (below).

## Which profile a run gets

`tack_core::run_defaults::resolve(explicit, run_settings, roles, project_default, implementer, profiles)` returns a `Resolved { profile_id, fleet_id, harness, source }`. It takes the first of these that exists:

| Order | Input | `Source` | `metadata.profile_source` |
|---|---|---|---|
| 1 | The caller named a profile | `Explicit` | absent |
| 2 | The task's saved run settings | `RunSettings` | `run_settings` |
| 3 | The profile of the task's roles, when every role that names one names the same | `Role(name)` | `role:<name>` |
| 4 | The project's default profile | `ProjectDefault` | `project_default` |
| 5 | The built-in Implementer (`kind = 'implementer'`) | `Implementer` | `implementer` |

Two roles on one task that name different profiles are `Ambiguous`: `resolve` returns the project default with that source (`ambiguous`), or falls to the Implementer when the project has none. `fleet_id` and `harness` come from the chosen profile's `fleet_id` column and its `tool_policy.agent`. The server resolves; the browser does not. `GET /api/items/{id}/agent-context` answers `profile_id` and `profile_source` so that the Run dialog can show "Chosen by the role X".

## The docket adapter

`crates/tack-runner/src/harness/docket.rs` holds `DocketAdapter`, which implements `HarnessAdapter` and `HarnessProbe` directly. docket is a daemon the runner talks to, so there is no stdout to parse and no `HarnessGrammar`. The submodules are `client.rs` (one method per route), `probe.rs` (version and teams), `task.rs` (the task body) and `daemon.rs` (supervision).

The adapter is registered in `harness::discover` only when `[docket]` names a `binary` or a `base_url` (`DocketConfig::configured`). The daemon is started only when `binary` is set and `base_url` is not (`bootstrap::start_docket`). A `docket` on `PATH` is never enough: an unasked `docket start --dispatch` would take over the operator's own teams.

### Routes used

| Step | Call | Function |
|---|---|---|
| Version | `GET /status.json`; `apiVersion` must be `"5"` | `probe::status` |
| Teams and their profiles | `GET /pods`, then `GET /pods/<pod>` for each | `probe::teams` |
| Make the team once per process | `POST /pods` with `ensure: true`, `blueprint: "software"` | `Client::ensure_pod` |
| Post the attempt | `POST /tasks/<pod>` | `Client::enqueue_task` |
| Read the task | `GET /tasks/<pod>/<id>` | `Client::task` |
| Follow the hops | `GET /traces/<pod>?since=<cursor>` | `follow` |
| Look for approvals and questions | `GET /inbox` | `next_question` |
| Answer an approval | `POST /approvals/<token>` with `channel: "tack"` | `deliver` |
| Answer a parked question | `POST /tasks/<id>/answer` | `deliver` |
| Cancel | `POST /tasks/<pod>/<id>/cancel` | `cancel` |
| Untracked files | `GET /tasks/<pod>/<id>/diff` | `finish` |

Any other `apiVersion` is a probe error: "needs docket 0.2.0-beta.6 (API 5); this docket answers API N". `validate` runs the probe before any task is posted, so no task reaches a docket of another version.

### The task

`task::body` builds the `POST /tasks/<pod>` body from the request snapshot:

- `idempotencyKey` is the attempt id, so a replayed post answers the same task.
- `externalRef` is `<item id>/attempt-<n>`.
- `description` is the instructions the server already composed. `brief` carries the objective, acceptance criteria and constraints. `gate` is the first acceptance criterion docket can run as a command (a `command` criterion, else a `test` or file check); a `metric` is never a gate.
- `baseRef` is the commit the runner's worktree stands on, or the `rework_base` branch for a rework.
- `maxTokens` comes from the request's token budget. `restrictions` carries `deniedTools` from `tool_policy.docket.denied_tools` and `network: "none"` when the run has the network off. A restriction can only narrow what the team already has (ADR 0077).
- `pipeline` is read from `tool_policy.docket.pipeline_by_type` by item type. `priority` and `trusted` are sent when the request's metadata carries them.

A `400` or `422` from docket makes the attempt fail with docket's text and the task is never posted again. The process id is `<pod>/<task id>`, which is how `reconcile` finds the pod after a restart.

### Which team

`DocketAdapter::team` returns the profile's `tool_policy.docket.team`. Without one it returns `default_pod`, the repository folder's basename lowercased to `[a-z0-9-]`, 64 characters at most, `default` when nothing is left. A profile with no team therefore falls to the project's default team, created on first use in the project's folder. `validate` checks that a named team exists on the daemon and rejects the attempt before anything is posted when it does not. Only the `local_branch` workspace mode is accepted; the others are refused with a sentence.

### Following the task

`follow` polls every two seconds (`POLL_INTERVAL`):

1. It reads the trace page since the cursor and turns `hop_started` and `hop_finished` records for this task into timeline lines. The cursor lives in `<state_dir>/docket/<attempt id>.cursor` and is written by rename after each page is delivered, so a second adapter over the same directory resumes where the first stopped.
2. It reads the inbox. An item with a `token` for this task becomes a decision of kind `approval` (options `approve_once`, `approve_task`, `deny`). A task in `waiting_input` becomes a decision of kind `consult`. Both go through the existing decision channels, so the inbox needs no new route.
3. It reads the task. In `done`, `failed`, `blocked` or `cancelled` it stops. When the attempt's own timeout passes it cancels the task and ends the attempt with `timeout`.
4. A person's answer arrives on the answer channel and is posted at once; a `409` from docket means it had already resolved and is not an error.

### The branch comes back

On `done`, `bring_in` runs `git reset --hard refs/heads/<task branch>` in the runner's worktree. Evidence, verification and push then run unchanged, as for every other agent. Files docket reports as untracked produce one timeline line ("N files docket did not commit are not in the evidence"). On a reset failure the attempt fails with `branch_not_applied`.

### Capabilities

`capabilities()` declares `cancel`, `decisions`, `artifacts`, `usage` and `policy` as supported, `resume` as unsupported. `sandbox()` returns `"harness"`, which the server accepts beside `bwrap` and `none` and the UI renders as "Sandboxed by docket in its own jail". `probe::probe` adds the teams to the capability snapshot and sets `autonomy_ceiling` to `L3` when any team is capped, else `L5`.

## The ceiling

`probe::capped` marks a team as one that needs a person during a run: its profile is not `lights-out`, or it reports a violation. The runner reports the teams in `HarnessCapability.teams` (`TeamReport { name, profile, violations, read_only }`; `read_only` is `GET /pods/<team>`'s `readOnly`, the folders the team's sandbox may read, and stays absent for a docket that sends no such key).

The ceiling is read at merge time, never frozen in the request. `tack_db::repo::autonomy::attempt_ceiling_inputs` joins the attempt to its runner's capability snapshot and to `resolved_agent_profile.tool_policy.docket.team` in the request. `tack_api::autonomy::ceiling_of` then returns `L3` when the profile names a team the runner reports and that team is attended. When the profile names no team, or the runner does not report the one it names, it falls back to the agent's own `autonomy_ceiling`: `L3` when any team the runner reports is capped, else `L5`. An agent that reports neither leaves the ceiling unknown (`None`), and an unknown ceiling is no hold.

`tack_core::autonomy::Facts.ceiling` carries it. In `evaluate`, `Hold::HarnessCeiling { ceiling, wanted }` is checked in the `Trigger::Auto` branch only, right after `LevelBelowL4`. The Merge button and the merge on Done are not held by the ceiling. The reason is that the policy must say what will actually happen next time, and a team's profile can change between the run and the merge.

`GET /api/projects/{id}/autonomy` reports `ceiling` and a `teams` line for each team a role uses (`TeamLine { name, profile, caps_at, fix }`). The Ceiling rung of the UI reads it.

When a demotion takes the whole project to L3, `tack_api::autonomy::demote` also sets `auto_start_enabled = 0` and writes the system comment `DROPPED_TO_L3`. `Project.auto_start_disabled_reason` is computed in the SELECT from that comment while auto-start is off and the level is L3.

## Usage per model

A harness that can tell which models it used writes `Usage.additional["by_model"]`, an object `{ "<model>": { "input": n, "output": n, "cache_read": n?, "cache_write": n? } }`. Claude Code writes it from `claude_code.rs`; docket from `usage.byModel` (`DocketAdapter::outcome`, which keeps `input` and `output` only).

One SQL prices everything. `tack_db::repo::rate_card::spend_by_model(pool, project, since)` returns one `SpendRow { model, repo, team, tokens_in, tokens_out, attempts }` per model, repository and team. An attempt with a `by_model` object counts once per model, with cache reads and writes added to `tokens_in`. Any other attempt counts under `RUN_MODEL_SQL`, the model the agent reported, else the one the run asked for. A run on the agent's default model that reported none has no model and is not priced.

The Factory page (`handlers/metrics.rs`) and the daily dollar budget (`scheduler/wiring.rs`, `dollars_today`) both call it, so they cannot disagree. A team's dollar figure is omitted when any of its rows is unpriced.

## A task that repeats

`items.recurrence` is `weekly`, `monthly` or `quarterly` (migration 089). When a task with a recurrence enters a done status from one that was not done, `create_next_recurrence` in `handlers/items.rs` runs. It does nothing when an earlier successor (`recurrence_of = this id`) is still unfinished. Otherwise `tack_core::recurrence::next_due` computes the due date from the task's due date, or from today when it has none: a week later, the last day of the next month, or the last day of the quarter after the current one. `clone_for_next` copies the words and shape, drops the sprint and assignee, and keeps the roles through `assign_role_to_item`. The new task starts in the first to-do status and the closed one gets the system comment "Created the next one, due …".

## The migrations

| Migration | Adds |
|---|---|
| `088_role_profile_fleet` | `roles.agent_profile_id` and `agent_profiles.fleet_id`, both `ON DELETE SET NULL` |
| `089_item_recurrence` | `items.recurrence` (`weekly`, `monthly`, `quarterly`) and `items.recurrence_of` |
| `090_auto_start` | `projects.auto_start_enabled` (default 0), `auto_start_status`, `auto_start_max_open` (default 3, at least 1) |
| `091_template_bundle` | `project_templates.roles`, `.profiles` and `.automation`; see [The Project File Format](project-file.md) |

## The fake docket

There is no real docket in any test. `crates/tack-runner/tests/fake_docket/mod.rs` is an in-process docket on a loopback port, written key by key from docket's published operator-API spec (version 5.0.1). Its README, `crates/tack-runner/src/harness/fixtures/docket/v5/README.md`, lists the routes it implements and the keys it leaves out. The adapter is tested against it in `crates/tack-runner/tests/docket_v5.rs`.

To extend it:

1. Add the route to the fake's router and make it record the request, so a test can assert on `FakeDocket::requests_to(method, prefix)`.
2. Script behaviour with a `Script`: trace events before and after the gate, a `Gate` (`None`, `Approval`, `Consult` or `Hold`), the final status, a file to commit on the task's branch, untracked files and the usage.
3. Add the route to the "What the fake implements" list in the README, and the keys you skip to "What it leaves out".
4. When a real docket 0.2.0-beta.6 exists, replace what the fake says with captures from it and say so in the README.

The daemon's supervision is tested separately with a shell script on a scratch path that serves `/health`; see `crates/tack-runner/src/harness/docket/daemon/tests.rs`.
