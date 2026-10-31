# Plan: Phase 73 — every run goes to a team, and automation is one ladder

**Status: opened 2026-10-14.** Serves ADR 0021
(`docs/adr/0021-every-run-goes-to-a-team-and-automation-is-one-ladder.md`). Written against
`develop` at `6565415f` and docket at `e880bbfd` (ADR 0027 accepted, Phase 43 planned, API
version 5 specified and not yet implemented). Re-run `rg -n` before trusting a line number.

What this phase builds, in one paragraph. The runner can run several attempts at once. A
project role points at an agent profile, and a profile names its docket team and the machines
it runs on; a run that names no profile gets the role's. A project can let a task start by
itself when it reaches a status. Tack's automatic merge reads one more fact, the ceiling the
team sets, and the settings page shows what blocks the project and why. Usage is kept per model
and a task can repeat. The docket harness is rewritten against docket's HTTP API version 5:
it supervises `docket start --dispatch`, posts each attempt as a task in the team's pod, follows
the trace, answers approvals and questions through Tack's inbox, and brings the task's branch
into the runner's worktree so evidence, verification and push stay as they are. Until docket
0.2.0-beta.6 exists, every docket test runs against a fake docket on loopback that implements
the spec key by key. Settings → Automation becomes a ladder of four rungs on one screen.

## Who owns what

| Responsibility | Owner | What the others do |
|---|---|---|
| What is done, in which order, with what priority and budget | **Tack** | docket takes the task; prill judges it |
| Which profile, and through it which team, takes a task | **Tack** (the role, then the project default) | docket keeps the team's members, models per role, pipeline, gates, policies |
| Starting a run (by a person, by a rule, or as rework) | **Tack** | docket never enqueues on its own |
| Running the agent and its team; the hop; the jail; the per-task limits | **docket** | Tack passes `maxTokens`, `gate`, `restrictions`; never a model, never a hop |
| Approvals and consultations inside a run | **docket** raises them; **Tack** shows them and sends the answer | the inbox is Tack's; the answer route is docket's |
| The verdict, the stages, the signature | **prill** | Tack reads the pack; docket's reviewer verdicts are timeline lines, not Tack's verdict |
| Push, pull request, merge, level, domains, ceiling, money | **Tack** | docket records the branch; prill signs |
| Parallelism | **Tack's runner** (N attempts) and **docket's pods** (one task per pod) | a pod never runs two tasks; more parallelism is more pods |

## Decided — not re-opened by any task

Every row of ADR 0021's table and its "Questions for the maintainer, with the default built",
plus:

| Question | Decision |
|---|---|
| Migrations | One per wave that needs one, numbered now so parallel waves cannot collide: `088_role_profile_fleet` (T), `089_item_recurrence` (Q), `090_auto_start` (A). A task never edits another wave's migration; a missing shape is a finding. |
| The docket harness's shape | A `DocketAdapter` that implements `HarnessAdapter + HarnessProbe` directly (engine.rs:351, harness/mod.rs:151) and is registered with `AdapterRegistry::register`. Not a `HarnessGrammar`: there is no stdout to parse and no stdin to write. |
| The docket workspace | The runner provisions its worktree as for every agent (`local_branch`), records `base_commit`, and sends it as `baseRef`. When the task ends, `git reset --hard <task.worktree.branch>` in that worktree; then evidence, prill and push run unchanged. `in_place` and `clone` are refused at `validate` with a sentence. |
| The trace cursor | A file `<state_dir>/docket/<attempt_id>.cursor` the adapter owns, written after each page is delivered. The journal does not change. |
| Decision kinds | `approval` for docket's `approval_required` (answer → `POST /approvals/<token>` with `channel: "tack"`), `consult` for a parked question (answer → `POST /tasks/<id>/answer`). Both ride the existing `decision_channels`; the inbox already renders any kind. |
| Docket's sandbox word | `sandbox: "harness"`, accepted by the server beside `bwrap` and `none`, rendered "Sandboxed by docket in its own jail". |
| The ceiling at merge time | `autonomy::gather` reads the attempt's runner capability snapshot and the request's resolved profile team; `Facts.ceiling: Option<Level>`; `Hold::HarnessCeiling { ceiling, wanted }` checked right after `LevelBelowL4`. Nothing is added to the request snapshot. |
| The resolver's selector | Profile `fleet_id` → `Fleet`; else, when exactly one active runner exists, `ExactRunner`; else no selector: a manual run asks "Select where this runs" as today, auto-start skips the item and says so. |
| Auto-start's loop | Its own `tokio::spawn` beside the GitHub poll (`server.rs:123`), `TACK_AUTO_START_SECONDS` (default 30; 0 off), because the GitHub loop does not run without a token. The tick enqueues through `enqueue_resolved` with principal `auto_start` (the path `post_test_run` already uses), never through the HTTP router. |
| The ladder UI | One task (U) builds the four rungs, the badges, the activity lines, the drawer row and the pre-filled dialog, after T, C and A have landed, because they all touch `AutomationPanel.tsx`. Each backend wave ships its own API and its tests; U ships the screen. |
| Usage per model | `Usage.additional["by_model"]`: a JSON object `{ "<model>": {"input": n, "output": n, "cache_read": n?, "cache_write": n?} }`. One SQL in `tack_db::repo::rate_card` serves `token_spend` and `dollars_today`: `json_each(by_model)` when present, else `RUN_MODEL_SQL` with `tokens_in`/`tokens_out`. |
| Recurrence | Columns on `items`; the next task is created in `update_item` when a task with `recurrence` enters Done from a status that was not Done, and no open task has `recurrence_of = this id`. `next_due` is the end of the next period (month end, leap years) from the task's `due_date`, or from today when it has none. |
| The e2e script | `scripts/docket-prill-e2e.sh` checks `apiVersion == "5"` and dies naming `docket 0.2.0-beta.6`. Until beta.6 exists it is not run; the fake docket is the proof. |
| Generated files | Each task regenerates and stages them (`./scripts/regen-generated.sh`); on a conflict the integrator regenerates. |
| The project file (wave 7) | Every row of ADR 0022's table and its questions. The file is `tack_core::outline::Outline` with six optional sections; the check is pure (`outline::bundle::check`) and the export is pure (`outline::bundle::to_bundle`); the routes gather rows and write them. Items are never updated by a file. A file never raises a level or switches auto-start on. Profiles match by name across the board. Migration `091_template_bundle` adds `roles`, `profiles`, `automation` to `project_templates`. `tack template create-from` keeps its meaning; `tack template save` stores one. |

## Rounds

At most two agents building cargo at once (`feedback_agent_load_caps`); a frontend-only task
runs beside them. Worktrees under `/var/tmp/tack-worktrees/p73-<id>` from `develop`,
`CARGO_TARGET_DIR=$HOME/Sites/.cargo-targets/tack-agent/<id>`. (`/` filled up with the old `/var/tmp` targets; `/home` has the room.)

| Round | Tasks | Needs |
|---|---|---|
| 0 | ADR, this plan, the roadmap section (integrator) | approval (given 2026-10-14) |
| 1 | R (runner, N at a time), T (role → profile → fleet) | — |
| 2 | C (ceiling), M (usage per model) | T merged (C reads the profile's team) |
| 3 | A (auto-start), Q (recurrence) | T (A) |
| 4 | D1 (fake docket + adapter); U (the ladder) beside | T, C, R (D1); T, C, A (U) |
| 5 | D2 (daemon, config, doctor, deletions, e2e) | D1 |
| 6 | Z (integrator) | everything |
| 7 | B1 (the bundle in core and the database) — wave 7, ADR 0022 | the owner's approval of the file (given 2026-10-17) |
| 8 | B2 (the routes read and write the bundle) | B1 |
| 9 | B3 (CLI, MCP, the Data panel, the book); then Z2 | B2 |

Ceilings: Haiku 60 calls; Sonnet one layer 100, two layers 150. One that reaches it hands off
and is replaced fresh with a shorter brief.

## How a task is handed to an agent

As `docs/closed-cycles/plans/phase-67.md` § "How a task is handed to an agent": one agent, one task, one
branch from `develop` named `phase73/<id>`; the brief is the task's section of this file plus
these lines, and nothing else:

```
Task <id> from docs/plans/phase-73.md. Read that section and the files it lists, nothing wider.
The ADR is docs/adr/0021-every-run-goes-to-a-team-and-automation-is-one-ladder.md; its table and this plan's "Decided" table are the decision, do not re-open them.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Add only the tests the task names. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag or helper the task does not name.
Never call a real model endpoint, never run the real docket against a real model: every run goes to a loopback fake, zero spend.
Never run docket, or any CLI, against the operator's own HOME or ~/.docket; use a scratch HOME and an isolated DOCKET_HOME.
Every contract change is a fixture first (docs/contracts/<name>/), then the type that reads it, then the pin.
Tack's words on every screen: agent, profile, team, this computer, folder, branch; never pod, harness, hop, runner, dispatch.
Build and test with CARGO_TARGET_DIR=<given>, --build-jobs 4, --test-threads 4; only the binary you change while working: cargo nextest run --workspace -E 'binary(<name>)'.
Finish by running PATH=$HOME/.cargo/bin:$PATH .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do; your tool-call count.
```

## The tasks

### R — the runner runs N attempts at a time (Sonnet, two layers)

**Files:** `crates/tack-runner/src/config.rs`, `main.rs`, `bootstrap.rs`, `transport.rs`,
`engine.rs` (the heartbeat arms only), a new `crates/tack-runner/src/active.rs`,
`crates/tack-runner/tests/crash_matrix.rs`, `crates/tack-runner/src/harness/local_process/tests.rs`
(one table row), `docs/contracts/runner-v1/heartbeat.request.json` and a new
`heartbeat.request.two-active.json`, `crates/tack-orch/tests/runner_contract/fixtures.rs`,
`crates/tack-cli/src/local_runner.rs` (`get_local_runner` only), `frontend/src/features/agents/ExecutionToggle.tsx`,
`frontend/src/features/agents/api.ts`, `docs/CONFIG.md` (the runner table).

**Do:**

- `RunnerConfig.max_concurrent_attempts: u32` (default 1, refused below 1 with the field named),
  from the TOML top level, `TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS` and `--max-concurrent-attempts`,
  through `ConfigOverrides` (the same precedence as `runner_id`).
- `bootstrap.rs`: `Concurrency { total: N, available: N }`.
- `active.rs`: `ActiveAttempts`, a shared registry of the leases in flight (attempt id,
  fencing token, journal state, last checkpoint, a `watch::Sender<bool>` for cancellation).
  `transport.rs::serve`: a `JoinSet` with N slots; recovery before the first claim as today;
  a claim is sent only while a slot is free, with `available_capacity = free slots`; pacing
  (`idle_pause`) only when every slot is idle; the idle heartbeat sends `total` and no leases.
- One ticker per runner (`LEASE_RENEWAL_INTERVAL`, 20 s) sends one `heartbeat` with every
  active lease (the server takes up to 64, `limits.json`) and `available_capacity = free slots`;
  for each `lease_results` row with `cancellation_requested` it flips that attempt's watch.
  The three `renewal.tick()` arms in `engine.rs` (`wait_with_lease_renewal`, the verifier loop,
  the publish loop) are removed; the attempt reads its watch where it read the first
  heartbeat's answer (`engine.rs:682-703`) and inside `wait_with_lease_renewal`, so a cancel
  asked mid-run cancels the harness.
- `GET /api/local-runner` gains `max_concurrent_attempts` and `active_attempts` (a count);
  the Agents page line reads "Up to N runs at a time on this computer · M running".
- `docs/CONFIG.md`: the field, its default, and the sentence "a folder worked in place runs one
  attempt at a time whatever this says; parallel attempts need a branch or a clone".

**Tests (RED first):**

- `local_process/tests.rs`: with `fake_harness.sh` and N = 2, two claims run at once; every
  heartbeat carries both leases and `available_capacity = 0`; cancelling one through the
  heartbeat answer cancels only that one. One table with the three claims.
- `crash_matrix.rs`: one new row, two journals unresolved after a crash, both recovered.
- `heartbeat.request.two-active.json` under `docs/contracts/runner-v1/` with two
  `active_attempts`; pinned in `fixtures.rs` (FNV-1a 64 of the bytes) and the file count
  46 → 47.
- `config.rs` tests: default 1; `0` refused; env and flag precedence, one table.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + binary(runner_contract)'`
and the gate are green.

### T — a role chooses the profile and the fleet (Sonnet, two layers)

**Files:** `crates/tack-db/src/migrations.rs`, `crates/tack-db/tests/migrations/` (one file,
beside `autonomy.rs`), `crates/tack-core/src/models.rs` (`Role`, new `UpdateRole`),
a new `crates/tack-core/src/run_defaults.rs` (+ `lib.rs`), `crates/tack-db/src/repo/roles.rs`,
`crates/tack-db/src/repo/execution.rs` (the profile row and `NewExecutionRequest` only),
`crates/tack-api/src/handlers/roles.rs`, `runner_admin.rs`, `executions.rs`
(`create_execution`, `enqueue_resolved`, `get_item_agent_context`), `router.rs`, `openapi.rs`,
`crates/tack-cli/src/main.rs` (`RoleAction`, `AgentProfileAction`, `ExecutionCreateArgs`),
`crates/tack-cli/src/mcp.rs` (`create_execution`), `crates/tack-api/tests/handlers/` (the roles
file and the executions file), `crates/tack-cli/tests/` (the existing execution/MCP files).

**Do:**

- Migration `088_role_profile_fleet`, exactly:

  ```sql
  ALTER TABLE roles ADD COLUMN agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL;
  ALTER TABLE agent_profiles ADD COLUMN fleet_id TEXT REFERENCES agent_fleets(id) ON DELETE SET NULL;
  ```

- `Role.agent_profile_id: Option<String>`; `UpdateRole { name?, color?, icon?,
  agent_profile_id?: Option<Option<String>> }`; `Repository::update_role`,
  `profiles_for_item(item_id) -> Vec<(role_name, Option<profile_id>)>`;
  `PATCH /api/roles/{id}` (404 on no role, 400 on a profile id that does not exist).
- `CreateProfile.fleet_id` and `UpdateProfile.fleet_id` (`Option<Option<String>>`), returned on
  every profile read; a `fleet_id` naming no fleet is 400. `tool_policy.docket.team`, when
  present, must be a string matching `^[a-z0-9][a-z0-9-]{0,63}$` (docket's project id), else
  400 naming the key.
- `run_defaults.rs`, pure:

  ```rust
  pub struct Candidate { pub profile_id: String, pub fleet_id: Option<String> }
  pub enum Source { Explicit, RunSettings, Role(String), ProjectDefault, Implementer, Ambiguous }
  pub struct Resolved { pub profile_id: String, pub fleet_id: Option<String>, pub source: Source }
  pub fn resolve(explicit: Option<&str>, run_settings: Option<&str>,
                 roles: &[(String, Option<String>)], project_default: Option<&str>,
                 implementer: Option<&str>, profiles: &[Candidate]) -> Option<Resolved>
  ```

  Order: explicit → run settings → the one role profile (two roles with different profiles →
  the project default with `Source::Ambiguous`) → project default → implementer → `None`.
  `fleet_id` is the resolved profile's. **Owner addition (2026-10-14):** `Candidate.harness`
  and `Resolved.harness`, from the profile's optional `tool_policy.agent` (a known harness kind,
  validated in `runner_admin.rs`); `CreateExecution.harness` becomes optional and falls back to
  the resolved profile's agent, then the project's `default_harness`, else 400
  `select_an_agent`. My SaaS needs it: Security and the Planner run on Claude Code, the rest on
  docket teams.
- `CreateExecution.agent_profile_id: Option<String>`, `selector_kind`/`selector_id` optional.
  When the profile is absent, `create_execution` resolves it with `resolve` and writes
  `metadata.profile_source` (`"role:Content"`, `"project_default"`, `"run_settings"`,
  `"implementer"`, `"ambiguous"`). When the selector is absent: the profile's fleet, else the
  only active runner, else 400 `select_where_this_runs`. `get_item_agent_context` uses the same
  resolver. MCP `create_execution` and `tack execution create --agent-profile` become optional
  the same way; `tack role create --profile <id>`, `tack role set-profile <role> <id|none>`,
  `tack agent-profile create --fleet <id>`.
- `./scripts/regen-generated.sh`, stage the result.

**Tests (RED first):**

- `run_defaults.rs`: one table, one row per source and the ambiguous case.
- Migration test: a database at 087 with a role and a profile migrates; both columns read `NULL`.
- Handler tests: `PATCH /api/roles/{id}` sets and clears the profile; a run created without a
  profile on an item with one role resolves to the role's profile and records
  `metadata.profile_source`; without a role, the project default; with two roles and two
  profiles, the project default and `ambiguous`; no selector with one active runner → that
  runner; no selector with two → 400.
- CLI/MCP: the existing create tests gain a row without `--agent-profile`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + binary(handlers) + package(tack-cli)'`
and the gate are green.

### C — the automatic merge respects the team's ceiling (Sonnet, two layers)

**Files:** `crates/tack-orch/src/execution/capabilities.rs`, `crates/tack-core/src/autonomy.rs`
(+ its tests), `crates/tack-api/src/autonomy.rs` (`gather`), `crates/tack-api/src/handlers/autonomy.rs`,
`crates/tack-api/src/handlers/runner_protocol.rs` (the `sandbox` check at 1135-1138 only),
`crates/tack-api/tests/handlers/` (the autonomy file), `docs/contracts/runner-v1/` (the
capability fixture that carries a harness, plus a new `capabilities.with-teams.json`),
`crates/tack-orch/tests/runner_contract/fixtures.rs`, `docs/book/src/user-guide/merging.md`.

**Do:**

- `HarnessCapability.autonomy_ceiling: Option<String>` (`"L3"`) and
  `teams: Option<Vec<TeamReport>>`, `TeamReport { name: String, profile: String, violations:
  Vec<TeamViolation> }`, `TeamViolation { requirement: String, fix: String }`; all
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, so an old runner's snapshot
  deserializes.
- `Facts.ceiling: Option<Level>`; `Hold::HarnessCeiling { ceiling: Level, wanted: Level }`,
  displayed "The team that ran this stays at {ceiling}: it needs a person during a run"; checked
  in `evaluate` after `LevelBelowL4`, for `Trigger::Auto` only.
- `gather`: read the attempt's runner (`execution_attempts.runner_id` →
  `agent_runners.capability_snapshot`), the harness entry of the attempt's kind, and the
  request's `resolved_agent_profile.tool_policy.docket.team`; the ceiling is the named team's
  (`attended` or any violation → L3) when teams are reported, else the harness's
  `autonomy_ceiling`, else `None`.
- `get_autonomy`: for every profile a role of this project points at, whose team is reported
  `attended` or with violations by any active runner, one sentence in
  `automatic_merge_blocked_by`: "The team {name} runs attended: its tasks stay at L3. Fix: {fix}"
  (the first violation's fix, or "run it lights-out" when docket gives none). Add
  `ceiling: String` (`"L5"`, `"L4"` or `"L3"`) and `teams: Vec<TeamLine { name, profile, caps_at,
  fix }>` to `AutonomySettings`, `#[schema(read_only)]`, for the Ceiling rung.
- `put_autonomy` answers with what `get_autonomy` would (today it answers an empty
  `automatic_merge_blocked_by` and zero counts, so the radios enable until a reload).
- The server accepts `sandbox: "harness"` beside `bwrap` and `none`.
- `merging.md`: under "Automatic merge → What it needs", one paragraph: the team's ceiling.
- `./scripts/regen-generated.sh`, stage the result.

**Tests (RED first):**

- `autonomy.rs` tests: one table: ceiling L3 and level L4 → `HarnessCeiling`; no ceiling →
  as today; ceiling L3 at `Trigger::Button` → no hold.
- `capabilities.rs`: a capability JSON without the two fields deserializes; one with them
  round-trips.
- `runner_contract`: `capabilities.with-teams.json` pinned; count +1.
- Handler: a project whose role points at a profile with an attended team lists the sentence
  and `ceiling: "L3"`; `PUT` returns the same `automatic_merge_blocked_by` as `GET`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-orch) + binary(handlers) + binary(runner_contract)'`
and the gate are green.

### M — usage per model, priced through one SQL (Sonnet, two layers)

**Files:** `crates/tack-runner/src/harness/local_process.rs` (`UsageDetail`),
`crates/tack-runner/src/harness/claude_code.rs` (`result_usage`),
`crates/tack-db/src/repo/rate_card.rs`, `crates/tack-api/src/handlers/metrics.rs`,
`crates/tack-orch/src/scheduler/wiring.rs` (`dollars_today`), `crates/tack-db/tests/repository/`
(the rate-card or metrics file), `frontend/src/features/dashboard/FactoryMetrics.tsx`,
`frontend/src/features/dashboard/FactoryMetrics.test.tsx`.

**Do:**

- `UsageDetail.by_model: BTreeMap<String, ModelUsage>`, `ModelUsage { input: u64, output: u64,
  cache_read: Option<u64>, cache_write: Option<u64> }`; `into_additional` writes it under
  `by_model`. `claude_code.rs` keeps each `modelUsage` entry instead of summing; `models` stays
  as the ordered name list.
- `rate_card.rs`: `pub async fn spend_by_model(pool, project, since: Option<&str>) ->
  Vec<SpendRow { model: Option<String>, repo: Option<String>, tokens_in: i64, tokens_out: i64,
  attempts: i64 }>`: one query that unions the attempts with `by_model` (`json_each`) and the
  rest (`RUN_MODEL_SQL`), grouped by model and `p.repo`. `token_spend` and `dollars_today` call
  it and keep no SQL of their own.
- `FactoryMetrics.cost_by_model: Vec<{ model, tokens_in, tokens_out, attempts, usd? }>`
  and `cost_by_team: Vec<{ team, tokens_in, tokens_out, attempts, usd? }>` (team from
  `request_snapshot.resolved_agent_profile.tool_policy.docket.team`, absent rows under
  "No team"). The page shows one table "Cost by model" and, when any team exists, "By team".

**Tests (RED first):**

- `claude_code.rs`: the existing `modelUsage` test asserts `by_model` keeps two models apart.
- Repository: `spend_by_model` over three attempts (one with `by_model` of two models, two
  without) gives three rows with the right sums; `dollars_today` prices them with a card.
- Frontend: the table renders rows and "Not measured" for an unpriced model.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + package(tack-db) + package(tack-orch) + binary(handlers)'`,
`npx vitest run FactoryMetrics` and the gate are green.

### Q — a task can repeat (Sonnet, two layers)

**Files:** `crates/tack-db/src/migrations.rs`, `crates/tack-db/tests/migrations/`, a new
`crates/tack-core/src/recurrence.rs` (+ `lib.rs`), `crates/tack-core/src/models.rs` (`Item`,
`CreateItem`, `UpdateItem`), `crates/tack-core/src/outline.rs`, `outline/csv.rs`,
`crates/tack-db/src/repo/items.rs` (the row and the create/update binds),
`crates/tack-api/src/handlers/items.rs` (`update_item`), `crates/tack-api/src/handlers/export.rs`,
`crates/tack-api/tests/handlers/` (items), `crates/tack-cli/src/main.rs` (`tack add`),
`frontend/src/features/item-detail/ItemHeader.tsx`, `frontend/src/features/board/Board.tsx`
(`ItemCard`), `frontend/src/features/list/List.tsx` (`ItemRow`), `frontend/src/shared/types.ts`.

**Do:**

- Migration `089_item_recurrence`, exactly:

  ```sql
  ALTER TABLE items ADD COLUMN recurrence TEXT CHECK (recurrence IN ('weekly','monthly','quarterly'));
  ALTER TABLE items ADD COLUMN recurrence_of TEXT REFERENCES items(id) ON DELETE SET NULL;
  ```

- `Recurrence` enum (`Weekly`, `Monthly`, `Quarterly`) on `Item`, `CreateItem`, `UpdateItem`,
  `OutlineItem` (`recurrence`), the CSV column `recurrence` on import and export.
- `recurrence.rs`, pure: `next_due(from: NaiveDate, r: Recurrence) -> NaiveDate` (weekly: +7
  days; monthly: last day of the next month; quarterly: last day of the quarter after the one
  `from` is in) and `clone_for_next(item: &Item, first_todo: &str, due: NaiveDate) -> CreateItem`
  (title, description, type, priority, tags, parent, estimate and unit; status = `first_todo`;
  `recurrence` kept; `recurrence_of = item.id`; no sprint, no assignee).
- `update_item`: after `propagate_parent_completion`, when the old status was not Done, the new
  one is, the item has `recurrence`, and no item with `recurrence_of = id` is unfinished:
  create the next one (roles copied with `assign_role_to_item`), and a system comment on the
  closed one: "Created the next one, due {date}".
- `tack add --recurrence weekly|monthly|quarterly`.
- UI: a "Repeats" select (Never, Weekly, Monthly, Quarterly) in `ItemHeader`'s grid; a badge
  "Repeats" on the card and the row when set.

**Tests (RED first):**

- `recurrence.rs`: one table for `next_due` (a month end, February in a leap year, a quarter
  end); `clone_for_next` copies what the plan lists and nothing else.
- Handler: closing a recurring task creates one successor; closing it again (reopen, close)
  while the successor is open creates none; the CSV export → import round-trips `recurrence`.
- Frontend: the select patches `recurrence`; the badge renders.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + binary(handlers) + package(tack-cli)'`,
`npx vitest run ItemHeader Board List` and the gate are green.

### A — a task starts by itself (Sonnet, two layers)

**Files:** `crates/tack-db/src/migrations.rs`, `crates/tack-db/tests/migrations/`,
`crates/tack-core/src/models.rs` (`Project`, `UpdateProject`), a new
`crates/tack-core/src/auto_start.rs` (+ `lib.rs`), `crates/tack-core/src/dependency.rs`
(`unfinished_blockers`), a new `crates/tack-api/src/auto_start.rs` (+ `lib.rs`, `server.rs`),
`crates/tack-api/src/config.rs` (`auto_start_seconds`), `crates/tack-api/src/handlers/executions.rs`
(a `pub(crate)` entry that takes a principal string instead of headers, used by
`enqueue_resolved` and by the tick), `crates/tack-api/src/autonomy.rs` (`demote`),
`crates/tack-api/src/handlers/metrics.rs`, `crates/tack-db/src/repo/metrics.rs`,
`crates/tack-api/tests/handlers/` (projects, executions, metrics), `docs/CONFIG.md`.

**Do:**

- Migration `090_auto_start`, exactly:

  ```sql
  ALTER TABLE projects ADD COLUMN auto_start_enabled INTEGER NOT NULL DEFAULT 0 CHECK (auto_start_enabled IN (0,1));
  ALTER TABLE projects ADD COLUMN auto_start_status TEXT;
  ALTER TABLE projects ADD COLUMN auto_start_max_open INTEGER NOT NULL DEFAULT 3 CHECK (auto_start_max_open >= 1);
  ```

  On `Project` and `UpdateProject` (`PATCH /projects/{id}`); `auto_start_status` must be a
  status of the project's workflow or 400.
- `dependency.rs`: `pub fn unfinished_blockers(&self, item: Uuid, status_of: &dyn Fn(Uuid) ->
  Option<String>, workflow: &WorkflowConfig) -> Vec<Uuid>`: the `blockers_of` whose status is
  not Done.
- `auto_start.rs` (core), pure:

  ```rust
  pub struct Candidate { pub item_id: Uuid, pub priority: Priority, pub sort_order: i32,
      pub version: i64, pub has_open_request: bool, pub unfinished_blockers: usize,
      pub profile: Option<Resolved> /* run_defaults */ }
  pub struct Limits { pub max_open: usize, pub open_now: usize, pub budget_reached: bool,
      pub wip_room: Option<usize> /* None = no limit on the in-progress column */ }
  pub enum Skip { OpenRequest, Blocked(usize), NoProfile, NoMachine, Wip, Budget, MaxOpen }
  pub fn pick(candidates: &[Candidate], limits: &Limits) -> (Vec<Uuid>, Vec<(Uuid, Skip)>)
  ```

  Ordered by priority (critical first), then `sort_order`; stops at `max_open - open_now`.
- `auto_start.rs` (api): `run_auto_start_task(state)`, an interval of
  `TACK_AUTO_START_SECONDS` (default 30, 0 returns at once), spawned in `server.rs` beside the
  GitHub poll. One `tick`: for each project with `auto_start_enabled`, the items in
  `auto_start_status` with at least one role; the candidates from `profiles_for_item`,
  `run_defaults::resolve`, the open requests (`queued|leased|preparing|running|waiting_decision`),
  `unfinished_blockers`, `budget_reached` (made `pub` in `tack_orch::scheduler::wiring`), the
  WIP room of the first in-progress status. For each pick: enqueue through the resolved entry
  with principal `auto_start`, `idempotency_key = "auto-start:{item_id}:{version}"`, no profile,
  no selector, `metadata.started_by = "auto_start"`, `permission_policy` = the project's
  defaults as the dialog sends them; on `Created`, move the item to the first in-progress
  status with a system comment "Started by itself: it reached {status} with the role {role}".
  A skip is logged at `debug`, never a comment.
- `demote`: when the demotion leaves `projects.autonomy_level` at L3 and `auto_start_enabled`
  was 1, set it to 0 and add a system comment on the project's activity: "Auto-start is off:
  the project dropped to L3". The GET of the project carries `auto_start_disabled_reason`
  (read from that comment's existence today; a column is not needed).
- Metrics: `FactoryMetrics.runs_by_start { person, auto, rework }` (`metadata.started_by`,
  `metadata.rework_of`), `merges_by_trigger { button, done, auto }` (`tack_merges.trigger`),
  `decisions_answered`, and `lights_off_rate`: items whose newest merged attempt belongs to a
  request with `started_by = auto_start` and whose requests have zero resolved decisions, over
  items merged in the window. All `GROUP BY` over existing columns.
- `docs/CONFIG.md`: `TACK_AUTO_START_SECONDS`.

**Tests (RED first):**

- `auto_start.rs` (core): one table, one row per `Skip` variant and the ordering.
- `dependency.rs`: `unfinished_blockers` ignores a Done blocker.
- Handler: two ticks over one item create one request (the key dedupes) with
  `profile_source` and `started_by`; the item moved once; an item without a role stays; a
  project at its cap starts nothing; a demotion to L3 switches the project off.
- Metrics: a request with `started_by = auto_start` counts under `auto`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + binary(handlers) + package(tack-orch)'`
and the gate are green.

### D1 — the docket harness over HTTP, against a fake docket (Sonnet, two layers)

**Files:** `crates/tack-runner/src/harness/docket.rs` (rewritten), `docket/probe.rs`
(rewritten), `docket/readiness.rs` (deleted), a new `docket/client.rs` (the HTTP calls, one
function per route), a new `docket/task.rs` (the task body from a request), `harness/mod.rs`
(`discover`, `check_declared_cancel`, `DESCRIPTORS`), `crates/tack-runner/src/config.rs`
(`[docket]`), `crates/tack-runner/Cargo.toml` (dev-dependency `axum` at the workspace version),
a new `crates/tack-runner/tests/fake_docket/` (`mod.rs` + the test file `docket_v5.rs`),
`crates/tack-api/src/handlers/runner_protocol.rs` (`enqueue_rework`: `metadata.rework_base`),
`crates/tack-runner/src/harness/fixtures/docket/` (deleted: `0.2.0b1/`, `0.2.0b5/`,
`contract-1.1/`, `README.md`; a new `v5/README.md` saying the fixtures are the fake's, to be
replaced by captures when beta.6 exists).

**Do:**

- `[docket]` in `RunnerConfig`: `home: Option<PathBuf>` (the daemon's `DOCKET_HOME`; the
  operator's own by default, never in tests), `port: u16` (default 7399), `token_file:
  Option<PathBuf>`, `binary: Option<PathBuf>`, `base_url: Option<String>` (set by tests to the
  fake; when set, nothing is spawned). D2 adds the supervision; D1 reads `base_url` or
  `http://127.0.0.1:{port}` and the token.
- `DocketAdapter` implements `HarnessProbe` (`probe`: `GET /status.json` → `apiVersion`; only
  `"5"` is valid; otherwise `probe_error = "needs docket 0.2.0-beta.6 (API 5); this docket
  answers API {n}"`; then `GET /pods` and `GET /pods/<p>` fill `teams` and `autonomy_ceiling`
  as C defines; `sandbox: "harness"`; `sign_in` from `/status.json`'s model block when present)
  and `HarnessAdapter`:
  - `validate`: `workspace_mode != local_branch` → `Rejected("the docket team works on a
    branch of your folder; choose that under Settings → Automation")`; a `tool_policy.docket.team`
    that is not reported by `GET /pods` → `Rejected` naming it.
  - `start`: ensure the pod (`POST /pods {name, location: <project repository path>,
    blueprint: "software", ensure: true}`; 200/201 both fine, once per pod per process); then
    `POST /tasks/<pod>` with the body of `task.rs`: `description` (title + description + "For
    the agent"), `brief` (objective/acceptance/context/constraints from Tack's brief; manual
    criteria as acceptance prose), `externalRef = "<item_id>/attempt-<n>"`, `idempotencyKey =
    attempt_id`, `baseRef` (`metadata.rework_base` when present, else the attempt's
    `base_commit`), `pipeline` (`tool_policy.docket.pipeline_by_type[item_type]` when present),
    `gate` (`{command, timeoutS}` from the brief's first `command` or `test` criterion;
    `file`/`absent`/`metric` criteria become an `sh -c` test; `manual` stays in Tack; the
    project's definition of done is prose and goes into `description`, never into `gate`),
    `maxTokens`
    (`budgets.tokens` when > 0), `restrictions` (`deniedTools` from the profile's denied list for
    docket; `network: "none"` when web tools are off), `priority` (critical/high → high,
    medium → normal, else low), `trusted` as today. `201`/`200` → `LocalRunHandle { process_id:
    task id }`. `400`/`422` → the attempt fails before spawn with the body's `error` on the
    timeline, never retried.
  - `wait`: every 2 s `GET /traces/<pod>?since=<cursor>`; the cursor file is written after each
    page; `hop_started` → `Progress("agent.hop_started", "Implementer (local/local-model)
    started")`; `hop_finished` → `Progress("agent.hop_finished", "Reviewer (anthropic/…):
    REQUEST-CHANGES, cycle 1")` with usage; `approval_required` (from the inbox) → a
    `Question { kind: "approval" }` whose answer goes to `POST /approvals/<token> {action,
    channel: "tack", option, reason}`; a parked question → `Question { kind: "consult" }`
    answered with `POST /tasks/<id>/answer`; then `GET /tasks/<pod>/<id>` until terminal.
    `done` → `git reset --hard <worktree.branch>` in the runner's worktree, `GET .../diff`'s
    `untracked` listed on the timeline as "N files docket did not commit are not in the
    evidence"; usage from `usage.byModel` into `by_model` (M), `actual_execution.model` = the
    last implementer hop's model, `additional.team = <pod>`. `failed` with `failureKind`
    `base_ref|pipeline|budget` → the attempt fails with that word and docket's `error`;
    `blocked` → the same with docket's text; `cancelled` → cancelled.
  - `cancel`: `POST /tasks/<pod>/<id>/cancel {reason}`.
  - `reconcile`: `GET /tasks/<pod>/<id>`: running → `ProcessRunning`; terminal →
    `ProcessStopped`; 404 → `Ambiguous`.
- `enqueue_rework` writes `metadata.rework_base = <the previous attempt's
  actual_execution.git.branch>` beside `rework_of`.
- `harness/mod.rs`: `discover` registers `DocketAdapter` with `register` when the binary or
  `base_url` is configured; `check_declared_cancel` accepts a probe whose descriptor says
  `cancels_over_api: true` (a new descriptor field, `true` only for docket, `reports_process_groups`
  becomes `false` for it).
- The fake docket (`tests/fake_docket/mod.rs`): an `axum` server on a bind-then-drop loopback
  port implementing, key by key from the spec: `GET /status.json` (configurable `apiVersion`),
  `GET /health`, `GET /pods`, `GET /pods/<p>`, `POST /pods` (ensure), `POST /tasks/<pod>`
  (idempotency, 400, 422 on a widening restriction), `GET /tasks/<pod>/<id>`, `POST .../cancel`,
  `GET .../diff`, `GET /traces/<pod>?since=`, `GET /inbox`, `POST /approvals/<token>`,
  `POST /tasks/<id>/answer`. A scripted task: the test tells the fake which trace events and
  terminal state to produce; the fake records every request body for assertion. It commits a
  file on a branch in the test's scratch repository so `reset --hard` has something to bring.

**Tests (RED first), in `tests/docket_v5.rs`:** probe on `apiVersion: "4"` → `probe_error`
naming beta.6 and no run; ensure is posted once for two attempts; the task body, asserted key
by key against the spec's example; a `422` fails the attempt with the body's text and no second
`POST /tasks`; the cursor survives a second adapter over the same state dir (the second poll
starts at the saved cursor); an approval round trip (`approval_required` → a `Question` of kind
`approval` → `POST /approvals/<token>` with `channel: "tack"`); a consult round trip; cancel
posts `cancel`; a `done` task brings the branch into the worktree and the evidence lists the
committed file; a rework request posts `baseRef = rework_base`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner)'` and the gate are
green, with zero processes named `docket` spawned (the fake is in-process).

### U — the ladder, the badges, the sentences (Sonnet, frontend)

**Files:** `frontend/src/features/settings/panels/AutomationPanel.tsx` (split: the ladder in
a new `panels/automation/Ladder.tsx` with one file per rung: `Teams.tsx`, `Start.tsx`,
`Merge.tsx` (today's `AutonomySection`, moved), `Ceiling.tsx`), `frontend/src/shared/help/texts.ts`,
`frontend/src/shared/runWithAgent/RunFlow.tsx` and `RunWithAgentModal.tsx` (the summary line
and the source), `frontend/src/features/board/Board.tsx`, `features/list/List.tsx`,
`features/table/` (the review column), a new `frontend/src/shared/ui/AutomationBadge.tsx`,
`frontend/src/features/item-detail/ItemDetailDrawer.tsx` (the Automation row),
`frontend/src/features/settings/panels/panels.test.tsx`, `frontend/e2e/run-with-agent.spec.ts`
(one step), `docs/book/src/user-guide/merging.md` (a short "The ladder" section at the top, or
a new `automation.md` page linked from `SUMMARY.md` if `merging.md` is the wrong home).

**Do:** the screen of ADR 0021 decision 8, with these rules:

- The ladder sits under the three existing sections, which collapse into `<details>` with a
  one-line summary each.
- **Teams**: one row per project role (`GET /projects/{id}/roles`): a select of profiles
  (`PATCH /roles/{id}`), then the profile's team and machine read-only ("saas-content ·
  lights-out · this computer", from the runner capability's `teams` and the profile's
  `fleet_id`), a last row "No role" bound to the project's default profile. "Manage profiles →"
  to `/agents#profiles`. The button "Apply the repository's teams" is shown disabled with
  "This project's folder has no .docket/teams" until D2 reports one; it stays disabled in this
  task.
- **Start**: the switch (`PATCH /projects/{id}` `auto_start_enabled`), the status select
  (workflow statuses, default the first to-do), the number (`auto_start_max_open`). Disabled
  with "Choose a profile for at least one role in Teams" while no role has a profile. Under
  it, when `auto_start_disabled_reason` is set, the sentence.
- **Merge**: today's section, unchanged in content, moved here; L4/L5 disabled with the
  sentences of `automatic_merge_blocked_by` (which now include the teams).
- **Ceiling**: "This project can reach {ceiling}." then one line per `teams[]` entry (✓ or ✗
  with `fix`) and one per remaining blocker.
- `RunFlow`'s summary line gains "Chosen by the role {name}" / "Project default" / "Saved on
  this task" from `metadata.profile_source` of the dry resolution (`GET /items/{id}/agent-context`
  already resolves; expose `profile_source` on its response in T's shape, or read the
  resolver's answer from the modal's own pre-fill logic replaced by the server's: the modal
  no longer picks the profile itself; it asks the server and shows the source).
- Badges: `AutomationBadge` renders "Started by itself" (`metadata.started_by`), "Team: {name}"
  (the request's team), "Merged by itself" (`merge_note` with trigger `auto`), beside
  `MergeNoteBadge` on the card, the row and the table's review column.
- The drawer's Automation row: "Starts by itself when it reaches {status} · Team {name} ·
  Merge {level}" or "By hand: the role {name} has no profile", from the project's settings and
  the item's roles.
- Every new text through `HELP` with a `(?)`; Tack's words only.

**Tests (RED first):** `panels.test.tsx`: the four rungs render in order; the Start switch is
disabled with its reason when no role has a profile; a team line with a fix renders in
Ceiling; `RunFlow` shows "Chosen by the role Content". E2E: the run dialog opens with the
role's profile on an item with a role.

**Done when:** `npm run type-check && npm test && npm run lint:tokens && npm run build` and
the gate are green.

### D2 — the daemon, the doctor, the deletions, the e2e (Sonnet, two layers)

**Files:** a new `crates/tack-runner/src/harness/docket/daemon.rs`, `harness/docket.rs`
(the start-up hook), `crates/tack-runner/src/bootstrap.rs` (start and stop the daemon with the
runner), `crates/tack-cli/src/doctor.rs` (a `teams:` block for a capability that carries
teams), `crates/tack-runner/tests/agent_boundaries.rs` (one rule: the daemon's environment),
`scripts/docket-prill-e2e.sh`, `frontend/e2e/docket-prill.spec.ts`, `docs/CONFIG.md`,
`docs/book/src/user-guide/agent-runners.md` (the docket paragraph),
`docs/book/src/user-guide/what-the-agent-can-reach.md` (one paragraph: docket's jail is
docket's), `crates/tack-api/src/handlers/runner_admin.rs` and `router.rs` (one route:
`POST /api/agent-profiles/{id}/apply-team {directory?, dryRun, accept?}` that forwards to the
profile's runner; refused with the runner's answer; the 409 body with `reviewCode` passed
through), `frontend/src/features/settings/panels/automation/Teams.tsx` (the Apply button is
enabled when the capability reports `repoConfigDir`, posts, and shows the review code to paste).

**Do:**

- `daemon.rs`: `ensure_running(config) -> DaemonHandle`: `GET /health`; when nothing answers,
  spawn `docket start --dispatch --bind 127.0.0.1 --port <n> --token-file <f>` with
  `DOCKET_HOME = config.home` (the operator's own when unset), `PATH` only, stdout/stderr to
  the runner's log directory; wait for `/health` with a bound; on exit, restart with a backoff
  and a `warn`; on runner shutdown, one `SIGTERM` (docket finishes the running task and claims
  no other, ADR 0027 §9). Never spawned when `base_url` is set.
- `tack runner doctor` prints, under docket: `api: 5`, `daemon: running on 127.0.0.1:7399`
  (or why not), and one line per team: `team saas-content: lights-out` / `team saas-research:
  attended (fix: docket pod set network none)`.
- The apply route, as ADR 0021 decision 2 and the spec's `POST /pods/<p>/apply`: the runner
  exposes it through its protocol as a runner-admin action (`/api/runner/v1` gains nothing: the
  board asks the runner over the existing admin channel, or, if none exists, the task reports
  it as a finding and the button stays disabled with "Apply from the terminal: docket init
  --name <pod>").
- Deletions: `docket/readiness.rs`; every `0.2.0b*`, `contract-1.1` fixture and its
  `.provenance`; the `DOCKET_SETUP_HOME` sharing (`SETUP_FILES`); every doc sentence that says
  `docket exec`, `contract 1.2`, `docket setup harness`, `0.2.0-beta.5` (`rg -n 'docket exec|contract 1\.[12]|setup harness|beta\.5' docs README.md crates frontend scripts`).
- `scripts/docket-prill-e2e.sh`: starts `docket start --dispatch` itself in the scratch HOME
  with an isolated `DOCKET_HOME`, two pods (`saas-code`, `saas-content`) over the fixture
  repository, dies before building when `GET /status.json` does not answer `apiVersion: "5"`
  ("needs docket 0.2.0-beta.6"); the spec adds two items at once (one code, one content), a
  rejection with a rework, and a screenshot of the ladder.

**Tests (RED first):** `daemon.rs` tests against a fake `docket` script on `PATH` that serves
`/health` after a delay: spawned once, restarted once after it exits, sent `SIGTERM` on
shutdown; never spawned when `base_url` is set. `agent_boundaries.rs`: the daemon's
environment carries only `PATH`, `DOCKET_HOME` and `HOME`. Doctor: a capability with teams
prints the lines.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + package(tack-cli) + binary(agent_boundaries)'`
and the gate are green; `scripts/docket-prill-e2e.sh` exits early naming beta.6 on this machine.

### Z — docs and the integrated gate (integrator)

- `CHANGELOG.md`: an `[Unreleased]` section written by hand (git-cliff skips `Add:`/`Fix:`
  commits), one line per wave.
- `docs/CONFIG.md`: `max_concurrent_attempts`, `[docket]`, `TACK_AUTO_START_SECONDS`.
- `docs/book/src/roadmap.md`: the Phase 73 section's status.
- `CLAUDE.md`'s "What's next": one sentence.
- On the merged `develop`: `PATH=$HOME/.cargo/bin:$PATH .githooks/pre-push`, `cargo nextest
  run --workspace`, `cd frontend && npm run type-check && npm test`, and the counts quoted.

### B1 — the bundle in core and the database (Sonnet, two layers)

Wave 7, ADR 0022 (`docs/adr/0022-a-whole-project-goes-in-and-out-through-one-file.md`); the
file is `crates/tack-api/tests/fixtures/bundles/my-saas.yaml`, every rule commented. Read
both before anything else.

**Files:** `crates/tack-core/src/outline.rs`, a new `crates/tack-core/src/outline/bundle.rs`
(+ `bundle/tests.rs`), `crates/tack-core/src/outline/tests.rs`, `crates/tack-core/src/workflow.rs`
(one function), `crates/tack-core/src/models.rs` (`ProjectTemplate`, `CreateProjectTemplate`),
`crates/tack-db/src/migrations.rs`, `crates/tack-db/tests/migrations.rs` and a new
`crates/tack-db/tests/migrations/template_bundle.rs`, `crates/tack-db/src/repo/templates.rs`
(+ `templates/tests.rs`), `crates/tack-db/src/repo/roles.rs`, `crates/tack-db/src/repo/custom_fields.rs`,
`crates/tack-db/src/repo/execution.rs`, `crates/tack-db/src/repo.rs`,
`crates/tack-db/tests/repository/integration.rs`, and the struct literals that stop
compiling when `ProjectTemplate`/`CreateProjectTemplate` grow: `crates/tack-api/src/handlers/templates.rs`
(`save_project_as_template`, fill the new fields with empty values; B2 fills them for real) and
whatever `rg -n "CreateProjectTemplate \{|ProjectTemplate \{" crates/ frontend/src` finds
(a frontend literal only gets the new keys).

**Do:**

- `Outline` gains, each `#[serde(default, skip_serializing_if = ...)]` so a plain outline
  serializes as today: `fields: Vec<BundleField>`, `profiles: Vec<BundleProfile>`,
  `roles: Vec<BundleRole>`, `sprints: Vec<BundleSprint>`, `automation: Option<BundleAutomation>`.
  `OutlineProject` gains `vocabulary: Option<VocabularyMap>` and `workflow: Option<BundleWorkflow>`.
  `OutlineItem` gains `roles: Vec<String>` (read with `text_list`), `sprint: Option<String>`,
  `fields: BTreeMap<String, serde_json::Value>`, `estimate_unit: Option<String>` (story_points,
  hours, days, any other word as custom). `PlannedItem` carries the same three names through
  `resolve`, which does not check them (no board in its signature). Every struct is
  `deny_unknown_fields`, `Serialize + Deserialize`, `ToSchema` under the feature, documented
  on each field in one line. Shapes, exactly:

  ```rust
  pub struct BundleWorkflow { pub preset: Option<String>, pub statuses: Vec<BundleStatus>, pub transitions: Option<Vec<Transition>> } // preset XOR statuses
  pub struct BundleStatus { pub name: String, pub category: String /* todo|in_progress|done */, pub wip_limit: Option<usize> }
  pub struct BundleField { pub name: String, #[serde(rename = "type")] pub field_type: String, pub description: Option<String>, pub required: bool, pub default: Option<Value>, pub options: Vec<String>, pub validation: Option<Value> }
  pub struct BundleProfile { pub name: String, pub kind: Option<String>, pub instructions: String, pub tool_policy: Value /* default {} */, pub limits: Value /* default {} */, pub fleet: Option<String>, pub summary: Option<String> }
  pub struct BundleRole { pub name: String, pub color: Option<String>, pub icon: Option<String>, pub profile: Option<String> }
  pub struct BundleSprint { pub name: String, pub goal: Option<String>, pub start: Option<String>, pub end: Option<String>, pub status: Option<String> /* planning|closed */ }
  pub struct BundleAutomation { pub code_origin: Option<String>, pub repository: Option<String>, pub default_branch: Option<String>, pub workspace_mode: Option<String>, pub push_after_run: Option<bool>, pub default_harness: Option<String>, pub default_profile: Option<String>, pub default_model: Option<String>, pub on_finish_status: Option<String>, pub definition_of_done: Option<String>, pub autonomy: Option<BundleAutonomy>, pub auto_start: Option<BundleAutoStart> }
  pub struct BundleAutonomy { pub level: Option<String>, pub domains: Vec<BundleDomain>, pub verify_require: Option<Vec<String>>, pub verify_loop_max_attempts: Option<i64>, pub budgets: Option<BundleBudgets>, pub merge_on_done: Option<bool> }
  pub struct BundleDomain { pub name: String, pub paths: Vec<String>, pub item_types: Vec<String>, pub level: String }
  pub struct BundleBudgets { pub tokens_per_day: Option<i64>, pub usd_per_day: Option<f64>, pub auto_merges_per_day: Option<i64>, pub queue_items_max: Option<i64> }
  pub struct BundleAutoStart { pub enabled: bool, pub status: Option<String>, pub max_open: Option<i64> }
  ```

- `workflow.rs`: `pub fn preset_workflow(name: &str) -> Option<WorkflowConfig>` for scrum,
  kanban, simple, construction, legal, research, event (any case).
- `outline/bundle.rs`, pure, no I/O:

  ```rust
  pub struct ProfileRow { pub id: String, pub name: String, pub kind: String, pub instructions: String, pub tool_policy: Value, pub limits: Value, pub fleet_id: Option<String>, pub builtin: bool }
  pub struct Board<'a> { pub workflow: &'a WorkflowConfig, pub fields: &'a [CustomFieldDefinition], pub profiles: &'a [ProfileRow], pub roles: &'a [(Uuid, String)], pub sprints: &'a [(Uuid, String)], pub fleets: &'a [(String, String)] /* (id, name) */, pub harness_kinds: &'a [&'a str], pub level: Level /* the project's today; L3 for a new one */, pub auto_start: bool }
  /// The workflow a new project gets: the file's preset or statuses (checked with `WorkflowConfig::validate`), else the type's.
  pub fn workflow_of(project: Option<&OutlineProject>, project_type: &ProjectType) -> Result<WorkflowConfig, OutlineError>;
  pub fn check(outline: &Outline, board: &Board<'_>) -> Result<Plan, OutlineError>;
  pub enum Upsert<T> { Create(T), Update { id: String, value: T } }
  pub struct Plan { pub vocabulary: Option<VocabularyMap>, pub fields: Vec<Upsert<BundleField>>, pub profiles: Vec<Upsert<BundleProfile>>, pub roles: Vec<Upsert<BundleRole>>, pub sprints: Vec<Upsert<BundleSprint>>, pub automation: Option<BundleAutomation> /* levels and auto_start already lowered */, pub items: ResolvedOutline, pub sections: Vec<SectionSummary>, pub warnings: Vec<String> }
  #[derive(Serialize, ToSchema)] pub struct SectionSummary { pub section: String, pub created: usize, pub updated: usize, pub not_mentioned: usize, pub warnings: Vec<String> }
  pub struct Parts<'a> { pub project: &'a Project, pub fields: &'a [CustomFieldDefinition], pub profiles: &'a [ProfileRow], pub roles: &'a [Role], pub sprints: &'a [Sprint], pub autonomy: Option<BundleAutonomy>, pub auto_start: BundleAutoStart, pub items: &'a [Item], pub item_roles: &'a [(Uuid, Uuid)] /* (item, role) */, pub item_fields: &'a [CustomFieldValue], pub dependencies: &'a [Dependency], pub fleets: &'a [(String, String)] }
  /// The export: the same file back, deterministic.
  pub fn to_bundle(parts: &Parts<'_>) -> Outline;
  ```

  `check` rules, in this order, every problem collected before returning (ADR 0022 §1–§5):
  a vocabulary key not in `vocabulary::validate` → problem; `preset` and `statuses` both or
  neither-when-`workflow`-given → problem; unknown preset → problem naming the seven; a status
  category not todo/in_progress/done → problem; a field type not one of the nine → problem; a
  select/multi_select field without options → problem; a name repeated in one section → problem;
  a profile `kind` not implementer/reviewer/researcher/planner/custom → problem; `tool_policy.agent`
  not in `harness_kinds`, or `tool_policy.docket.team` not matching `^[a-z0-9][a-z0-9-]{0,63}$`
  → problem naming the key; a `fleet` the board lacks → warning, saved without a fleet; a
  file's profile whose name is a built-in's → warning, not changed; a role's `profile` naming
  nothing in the file nor on the board → problem; a sprint status not planning/closed, or a
  date that is not a date → problem; `automation`: `code_origin`/`workspace_mode` not their
  words → problem, `repository` an absolute path for folder origins and a URL for url (the
  same rule as `handlers/projects.rs::repository_problem`, copy the two lines) → problem,
  `default_harness` not in `harness_kinds` → problem, `default_profile` naming nothing → problem,
  `on_finish_status` and `auto_start.status` not in the workflow → problem, `level` and each
  domain `level` not L3/L4/L5 → problem, a level above `board.level` → lowered to `board.level`
  with a warning "autonomy.level asked: L4; applied: L3. Raise it from Settings → Automation"
  (same shape per domain), `auto_start.enabled: true` when `board.auto_start` is false → applied
  false, warning "auto_start asked: true; applied: false. Turn it on from Settings → Automation",
  `max_open` below 1 → problem; items: `resolve` as today, then each `roles` name must be in
  the file's roles or the board's → problem, `sprint` the same → problem, each `fields` key a
  field of the file or the board → problem, its value checked with
  `CustomFieldDefinition::validate_value` (build a definition from a `BundleField`) → problem
  with the item's label and the field's name, a required field the item lacks → warning,
  `estimate_unit` any word → ok. Matching by name is exact after trim. `sections` holds one
  summary per section in file order, with `not_mentioned` = the board's names the file does not
  carry (0 for a new project). `Upsert::Update.id` is the board's id.
- `to_bundle`: `project` with `type`, `description`, `vocabulary` = only the keys that differ
  from `vocabulary_for_type`, `workflow` as `statuses` + `transitions` (never `preset`);
  fields sorted by name; roles in the board's order; sprints in the board's order; profiles =
  those the roles and `default_profile` point at, in first-reference order, with `fleet` as a
  name; `automation` from the project's columns and the given autonomy; items parents first
  (Kahn over `parent_id`, ties in the slice's order) with `ref` `"1"`…`"n"`, `parent` and
  `depends_on` as refs, `depends_on` only from `blocks` dependencies, `roles` sorted by role
  name, `fields` keyed by field name, `estimate_unit` and `recurrence` as their words, every
  optional key `skip_serializing_if` when absent. `to_bundle` then `check` on an empty board
  must produce zero problems.
- Migration `091_template_bundle`, exactly:

  ```sql
  ALTER TABLE project_templates ADD COLUMN roles TEXT NOT NULL DEFAULT '[]';
  ALTER TABLE project_templates ADD COLUMN profiles TEXT NOT NULL DEFAULT '[]';
  ALTER TABLE project_templates ADD COLUMN automation TEXT;
  ```

- `ProjectTemplate { roles: Vec<BundleRole>, profiles: Vec<BundleProfile>, automation: Option<BundleAutomation> }`
  and the same on `CreateProjectTemplate` (`#[serde(default)]`), read and written by
  `repo/templates.rs`. `BuiltinSpec` gains the three; three new built-ins beside the existing
  ones, exactly these names, at L3, with no repository, English words, the type's vocabulary:
  **"Web product with content"** (web; fields Channel select [app, newsletter, social],
  Audience select [new, returning]; roles Content, Development, Growth, Security, Legal, Ops;
  profiles "Content writer" (custom, `tool_policy.agent: docket`), "Product implementer"
  (implementer, docket), "Growth" (custom, docket), and three read-only reviewers "Security
  review", "Legal review", "Ops review" (reviewer kind, `tools: {claude-code: [Read, Grep, Glob]}`,
  instructions that say they only read)); **"Library or CLI"** (software; fields Platform
  multi_select [linux, macos, windows], Breaking boolean; roles Development, Docs, Security,
  Ops; profiles "Library implementer" (implementer), "Docs writer" (custom), "Security review",
  "Ops review"); **"Static content site"** (web; kanban preset; fields Section select [home,
  blog, docs], Published date; roles Content, Design, Legal, Ops; profiles "Content writer",
  "Design review" (reviewer), "Legal review", "Ops review"). A profile shared by two built-ins
  is declared in each (the import matches by name). `automation` on each: `autonomy.level: L3`,
  `merge_on_done: false`, nothing else.
- Repository reads, one SQL each, on `Repository`: `item_roles_for_project(project_id) -> Vec<(Uuid, Uuid)>`,
  `field_values_for_project(project_id) -> Vec<CustomFieldValue>`,
  `list_agent_profile_rows() -> Vec<ProfileRow>` (ORDER BY name), `fleet_names() -> Vec<(String, String)>`;
  one write: `write_agent_profile(row: &ProfileRow, clock) -> Result<(), sqlx::Error>` as
  `INSERT ... ON CONFLICT(id) DO UPDATE` over name, kind, instructions, tool_policy, limits,
  fleet_id, updated_at (never `builtin`).
- `./scripts/regen-generated.sh`, stage the result.

**Tests (RED first):**

- `bundle/tests.rs`: one table over `check` with a row per problem and per warning above,
  each asserting the message names the section, the item label or the field; one test that
  the My SaaS fixture (`include_str!` of the api fixture path) checks clean on an empty board
  with exactly the autonomy warnings (level, the content domain, auto-start); one test that
  `to_bundle` of a synthetic `Parts` with two items, a role, a field value and a blocks
  dependency gives refs `"1"`, `"2"`, `parent: "1"`, `depends_on: ["1"]`, and serializes to
  YAML without any `id` or `_at` key; one test that `workflow_of` with `preset: kanban` equals
  `kanban_workflow` and with statuses gives `WorkflowType::Custom`.
- `outline/tests.rs`: one row that an item's `roles`, `sprint`, `fields`, `estimate_unit`
  survive `resolve`.
- Migration test: a database at 090 with a template migrates; the three columns read their
  defaults.
- `templates/tests.rs`: the three built-ins are listed once after two seeds, each with its
  roles and profiles and `automation.autonomy.level == Some("L3")`.
- `repository/integration.rs`: the four reads return what two inserts put in;
  `write_agent_profile` twice on one id leaves one row with the second name.

**Done when:** `cargo nextest run --workspace -E 'package(tack-core) + package(tack-db) + binary(handlers)'`
and the gate are green.

### B2 — the routes: import, export and templates read the bundle (Sonnet, one layer)

Needs B1 merged. Read ADR 0022, `bundle.rs` and the fixture first.

**Files:** `crates/tack-api/src/handlers/outline.rs`, `crates/tack-api/src/handlers/export.rs`,
`crates/tack-api/src/handlers/templates.rs`, `crates/tack-api/src/handlers/autonomy.rs` (only
to make its settings conversions `pub(crate)`), `crates/tack-api/src/handlers/runner_admin.rs`
(only `HARNESS_KINDS` to `pub(crate)`), `crates/tack-api/src/openapi.rs` (the new schemas),
`crates/tack-api/tests/handlers.rs` (one `mod` line), a new `crates/tack-api/tests/handlers/bundle.rs`,
`crates/tack-api/tests/handlers/outline.rs`, `crates/tack-api/tests/handlers/templates.rs`,
`crates/tack-api/tests/openapi_contract.rs` only if it pins route counts.

**Do:**

- `OutlineResult.sections: Vec<SectionSummary>` (always present; empty for a CSV).
- `into_new_project`: the workflow from `bundle::workflow_of`; build `Board` (no
  fields/roles/sprints yet; the board's profiles and fleets; `harness_kinds` =
  `runner_admin::HARNESS_KINDS`; `level: L3`, `auto_start: false`); `bundle::check`; on a
  dry run answer the preview with `sections` without creating anything; else create the
  project, apply the file's vocabulary (type default overlaid with the file's keys) and
  workflow with `update_project`, then apply in order: fields (`create_field`), profiles
  (`write_agent_profile`, the fleet name resolved, id `ap_<uuid>` for a create), roles
  (`create_role` then `update_role` for the profile id, resolved file-first), sprints
  (`create_sprint`, then `update_sprint_status(Closed)` when asked), automation
  (`update_project` with only the keys given, `default_profile` resolved to an id;
  `autonomy::store::put_settings` from the current settings overlaid with the plan's; the
  `auto_start_*` columns), items: set each planned item's `sprint_id` from the sprint name,
  write with `create_outline_items` as today, then `assign_role_to_item` and
  `set_field_value` per item. Any error on a new project deletes the project; on an existing
  one the error names the section ("roles: <db error>").
- `into_existing_project`: `Board` from the project's rows (`list_fields_for_project`,
  `list_roles`, `list_sprints`, the project's level from `autonomy::store::get_settings`, its
  `auto_start_enabled`); the same apply; a `project` block is read for nothing but a warning
  "the project's name, workflow and vocabulary are kept; change them under Settings".
- `export_project`: `scope` query, `snapshot` (default) or `bundle`; `bundle` gathers
  `Parts` (`list_items` with the default filter, `item_roles_for_project`,
  `field_values_for_project`, `list_dependencies_for_project`, `list_agent_profile_rows`,
  `fleet_names`, `get_settings`) and answers `bundle::to_bundle` as YAML or JSON, filename
  `<name>-bundle.<ext>`; `csv` with `bundle` → 400.
- `build_project_from_template`: build an `Outline` from the template (project name, type,
  description, vocabulary, workflow as statuses, fields, profiles, roles, automation, no items)
  and run the same code path as `into_new_project` (extract it as
  `pub(crate) async fn create_from_bundle(state, outline, source) -> Result<(Project, OutlineResult), ApiError>`),
  then the boards as today. `save_project_as_template`: `to_bundle` of the project's `Parts`,
  store `fields` as today and `roles`, `profiles`, `automation` from the bundle.
- `./scripts/regen-generated.sh`, stage the result.

**Tests (RED first), in `handlers/bundle.rs` unless named:**

- The fixture, posted as YAML to `/api/projects/outline`: 200; the project has the five
  statuses and the vocabulary; 4 fields, 7 roles each with the profile id of the profile its
  name says (Planner = the built-in's id), 6 profiles created and `sections` says
  `profiles: 6 created`, 2 sprints `planning`, `default_profile_id`, `on_finish_status`,
  `push_after_run`, `repository`; autonomy `level == "L3"` and `domains[0].level == "L3"`, with
  the warnings in the automation section; `auto_start_enabled == false` with its warning; 20
  items, "El zorro que no podía dormir" with role Contenido, sprint Semana 42, fields
  Dialecto/Edad/Canal, `estimate_unit == "hours"`; "Newsletter semanal" with two roles and
  `recurrence == "weekly"`; dependencies 5.
- The same with `?dry_run=true`: 200, project count and profile count unchanged, `sections`
  present.
- Posted again to `/api/projects/{id}/outline`: roles `updated: 7`, fields `updated: 4`, items
  40; then a role added by hand and the file posted once more: `not_mentioned: 1` and the role
  still there.
- A copy with `Edad: "12"` → 400 whose message names the item and the field; with
  `roles: [Nadie]` → 400; with `workflow: {preset: agile}` → 400 naming the seven.
- An existing project set to L5 by `PUT /autonomy`, then the fixture (L4) posted to it:
  `level == "L4"`, no level warning.
- Round trip: fixture → export `?scope=bundle&format=yaml` → post to `/api/projects/outline`
  → export again → the two texts are equal; the JSON export has no key named `id`,
  `project_id`, `created_at` or `updated_at` anywhere.
- `handlers/templates.rs`: `POST /api/projects/from-template/{id}` of "Web product with
  content" creates six roles with profile ids and the reviewer profiles exist by name;
  `save-as-template` of the fixture's project stores 7 roles and 6 profiles and `automation`.
- `handlers/outline.rs`: the existing tests still pass; one row that a CSV answers
  `sections: []`.

**Done when:** `cargo nextest run --workspace -E 'binary(handlers) + binary(openapi_contract) + binary(security)'`
and the gate are green.

### B3 — the doors: the CLI, the MCP tool, the Data panel, the book (Sonnet)

Needs B2 merged (the generated types). Read ADR 0022 and `docs/book/src/user-guide/import-export.md` first.

**Files:** `crates/tack-cli/src/main.rs` (a new `Export` command, `Init --template`,
`TemplateAction::Save`, `cmd_import`'s output), `crates/tack-cli/src/mcp.rs` (`create_outline`),
`crates/tack-cli/src/mcp/tests.rs`, `crates/tack-cli/tests/cli_test.rs`,
`frontend/src/shared/api/data.ts`, `frontend/src/features/settings/panels/DataPanel.tsx`,
`frontend/src/features/settings/panels/panels.test.tsx`, `frontend/src/shared/api/resources.test.ts`,
`docs/book/src/user-guide/import-export.md`, `docs/book/src/user-guide/cli.md`, `docs/MCP.md`.

**Do:**

- `tack export <project> [--format json|yaml|csv] [--bundle] [-o FILE]`: `GET
  /projects/{id}/export?format=…&scope=…` through `client.get_bytes`; `--bundle` sets
  `scope=bundle` and the default format to yaml; `-o` writes the file, else stdout.
- `tack init <name> --template <id or name>`: a value that is not a UUID is matched against
  `GET /templates` by name, case-insensitive, `-` read as a space ("web-product-with-content");
  then `POST /projects/from-template/{id}`; no match → an error listing the names.
- `tack template save <project> --name <name> [--description]` → `POST /projects/{id}/save-as-template`.
- `cmd_import` prints one line per entry of `sections` ("Roles: 2 new, 5 updated, 3 not
  mentioned") before the items, and each section's warnings under it.
- MCP `create_outline`: the body is every argument except `project_id` and `dry_run`; the tool
  description says the file may also carry `fields`, `profiles`, `roles`, `sprints` and
  `automation` "as in Tack's project file (Settings → Data → File format)"; the schema lists
  the five as `array`/`object` with a one-line description each and no nested detail.
- `data.ts`: `exportProject(projectId, format, scope?: 'snapshot' | 'bundle')`; the panel's
  Export card gains "Export project file (YAML)" → `scope: 'bundle'`, with one sentence:
  "The whole project — fields, roles, profiles, sprints, automation and items — as one file
  you can edit and import again." The import preview lists `sections` when non-empty, one
  line each ("Roles: 2 new, 5 updated, 3 not mentioned") with that section's warnings under
  it, before the items; the items line reads "20 items will be added".
- The book: a new "The project file" section in `import-export.md` after "The CSV": the
  seven sections in a short YAML, a table of what each section takes, the rules (names are
  references, in-file first; nothing deleted; what is refused and what is a warning; the
  autonomy and auto-start rule in one sentence; built-in profiles are never changed), the
  export (`scope=bundle`, `tack export --bundle`, the YAML button), templates (`tack init
  --template`, `tack template save`, the three built-ins by name). `cli.md`: `tack export`,
  `tack init --template`, `tack template save`. `docs/MCP.md`: the `create_outline` row.
  Tack's words everywhere: project file, profile, team, agent.

**Tests (RED first):**

- `cli_test.rs`: `export --bundle` sends `GET /api/projects/{id}/export` with `scope=bundle`
  and `format=yaml`; `init --template web-product-with-content` lists templates then posts to
  `from-template/{id}`; `template save` posts to `save-as-template`.
- `mcp/tests.rs`: `create_outline` with `roles` and `profiles` forwards both.
- `panels.test.tsx`: the preview shows "Roles: 2 new, 5 updated, 3 not mentioned" and the
  section warning; the Export card calls the bundle URL. `resources.test.ts`: one row for
  `exportProject` with `scope`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-cli)'`, `cd frontend && npm run
type-check && npm test && npm run lint:tokens && npm run build`, and the gate are green.

### Z2 — docs and the integrated gate for wave 7 (integrator)

- `CHANGELOG.md` `[Unreleased]`: one "Added" entry for the project file and the templates.
- `docs/book/src/roadmap.md`: the Phase 73 section's status and a row for wave 7.
- `CLAUDE.md`'s "What's next": half a sentence.
- On the merged `develop`: the gate, `cargo nextest run --workspace`, the frontend checks,
  `scripts/check-comments.sh`, and the counts quoted.

## Open questions

Blocking: none.

Not blocking, decided by the defaults in ADR 0021's last section. One more found while
writing: the apply-team route (D2) needs the board to reach the runner; if no admin channel
exists, D2 reports it and the button stays disabled with the terminal command, which is
still a control shown with its reason.

## Status

- **Round 0** closed 2026-10-14: ADR 0021, this plan, the roadmap section.
- **Round 1**, 2026-10-14, first pass. R and T each stopped at files the task list missed and
  were finished by a second agent on the same worktree (R2, T2), not resumed.
  - T's calls, kept: `tack role create --profile` is a POST then a PATCH (`CreateRole` carries no
    profile); a profile's `fleet_id` is written by an UPDATE in the handler, since
    `NewAgentProfile` is shared with unlisted tests; `set_or_clear` is a private deserializer
    because tack-api has no `serde_with`. T2 adds the CLI/MCP optional keys (the file is
    `crates/tack-cli/src/execution.rs`, missed by the list) and the owner addition
    (`tool_policy.agent`).
  - R's calls, kept: after a `NoWork` answer the claim pause is a deadline on new claims even
    while other slots are busy, otherwise an immediately answering server spins the loop; the
    engine's renewal test was deleted with the arm it tested. R2 adds the `/api/local-runner`
    fields and the Agents line (the handler is in `crates/tack-api/src/handlers/local_runner.rs`,
    not tack-cli, as the list said), a cancel that reaches a process during `wait`
    (`local_process.rs` took it out of `running`), the named-field config error, and a test of
    the slot loop. The in-place rule needs no code: `workspace.rs:219` already locks the folder.
  - A message to a running agent was refused by the session's permission classifier, so an
    addition mid-task goes into the plan and to a fresh agent, never to the running one.
  - **T closed** 2026-10-14 in `9e3001d8` (T + T2 squashed). Gate green on develop; nextest over
    core, db, api and cli: 822 passed, 1 skipped. T2's calls, kept: the request field keeps its
    name `requested_harness_kind` (optional now); `HARNESS_KINDS` is a constant of
    `runner_admin.rs`, since tack-api does not depend on tack-runner, so a new harness is added
    there too; `enqueue_resolved` guards `select_an_agent` a second time.
  - **R closed** 2026-10-14 (R + R2 squashed). Gate green on develop; nextest over the runner,
    the contract, the handlers and the CLI: 702 passed, 2 skipped. R2's calls, kept: a
    `ProcessCanceller` in `process.rs` lets a cancel reach a process already taken by `wait`,
    which then returns `Cancelled`; `ConfigError::InvalidField { field, reason }`; the
    `ActiveAttempts` handle is passed into the embedded runner's spawn rather than returned
    from it; `GET /api/local-runner` stays an untyped JSON body, so nothing regenerated.
- **Round 2** opened 2026-10-14: C stopped at the `HarnessCapability` literals in five crates
  and `merge.rs`'s `Facts`; C2 finishes it with those files authorised. M cut from develop
  after R.
  - **C closed** 2026-10-14 (C + C2 squashed). Gate green; nextest over core, orch, db, handlers,
    contract, cli and runner: 1063 passed, 3 skipped after one integrator fix: R and C each added
    a runner-v1 fixture and each raised the count to 47; the merged tree has 48. C2's calls,
    kept: `TeamReport` and `TeamViolation` are re-exported from `tack_orch::execution`; the
    settings' `ceiling` is L3 when any role's team is attended, else L5; the ledger's `policy`
    JSON does not record the ceiling. Integrator fix: C2 read "attended" as "has a violation";
    `is_attended` now also holds when the team's `profile` is not `lights-out`, as the plan says.
  - **M closed** 2026-10-14. Gate green; nextest over runner, db, orch and handlers: 817 passed,
    3 skipped; Vitest on the Factory page 15/15. M's calls, kept: `SpendRow.team` so one query
    serves the by-team table too; a `by_model` row counts its attempt once per model, so the
    `attempts` column summed across models can exceed the attempt count; a team's `usd` is
    omitted when any of its rows is unpriced; a row without a model reads "No model named";
    `token_spend` is gone. The LEFT JOIN on `pull_requests` cannot double count: its primary key
    is `attempt_id`.
- **Round 3** opened 2026-10-14: A and Q cut from develop at `64854f00`.
  - **Q closed** 2026-10-14. Gate green; nextest over core, db, handlers, cli and orch: 724
    passed, 1 skipped; Vitest 74/74. Integrator fixes: `clone_for_next` lost the unused status
    parameter the plan's signature carried (`CreateItem` has no status; the handler passes the
    first to-do status to the repository); the squash brought Q's `schema.gen.ts`, cut before M,
    over M's fields, so the generated files are regenerated on develop after every squash from
    now on. Q's calls, kept: an unknown recurrence word in a CSV is a warning; the successor
    search is one SQL on `recurrence_of`, not the paged list; `recurrence_of` is not kept across
    a JSON import (no id map in pass 1); no List row test, since `ItemRow` is not exported.
  - **A** finished by its agent the same day; integrator fix before the squash: the tick sent no
    `approvals`, which the wire reads as `auto` even for an agent the machine does not sandbox.
    It now follows the Run dialog's rule: `auto` when every eligible machine reports the agent
    as `bwrap` or `harness`, `ask` otherwise; the tick test pins it. A's calls, kept:
    `Candidate.has_machine` (the plan's struct could not produce `Skip::NoMachine`);
    `Project.auto_start_disabled_reason` computed in the SELECT from the demotion comment; the
    demotion note is a system comment on the demoted item, since comments are per item; a
    blocker outside the project counts as unfinished; `timeout_seconds` 3600 and `network:
    false`, the dialog's defaults; `runs_by_start` counts rework over auto.
  - **A closed** 2026-10-14 in `0f54c611`. Gate green after one conflict in `migrations.rs`
    (Q's 089 and A's 090 on the same lines, both kept); nextest over core, db, handlers, orch and
    cli: 729 passed, 1 skipped; type-check clean after regenerating.
- **Round 4** opened 2026-10-14: D1 cut from develop at `560923bd`, U at `0f54c611`.
  - **U** finished by its agent the same day with two backend gaps it could not fill: nothing on
    the item says who merged it (`merge_note` is the held reason, null once merged), and the
    autonomy endpoint's `teams` lists only the attended ones, so the Ceiling rung never shows a
    ✓. U2 adds `Item.merged_by` (from `tack_merges.trigger`, computed like `merge_note`) and
    lists every team a role points at with `caps_at` only on the attended ones. U's calls,
    kept: `AgentContextResponse` answers `profile_id` and `profile_source`, and the dialog asks
    the server instead of picking the profile itself; `Item.started_by` and `Item.team` are
    computed from the newest request; the drawer's Automation row takes its team from the
    roles' profiles, since `item.team` exists only after a run; four sections collapse into
    `<details>`, not three (the plan miscounted).
  - **D1** finished by its agent the same day: 18 tests against the in-process fake, gate green.
    D1's calls, kept: `process_id` is `<pod>/<task id>` so `reconcile` knows the pod; a 400/422
    on `POST /tasks` makes `wait` end the attempt Failed with the body's text, since the engine
    fails cleanly only on a `validate` rejection; `description` is the composed instructions the
    server already built; `validate` checks API 5 and the team before anything is posted;
    `autonomy_ceiling` is L3 when any team is not lights-out; the gate is the first `command`
    criterion with exit 0, else the first `test`, else `file`/`absent` as `sh -c 'test -e'`,
    `metric` is not a gate; `deniedTools` come from `tool_policy.docket.denied_tools` only. Left
    for D2: a profile without a team must fall to the project's default pod (D1 rejects it);
    the server writes nothing about the item's type, priority or trust into the request, so
    `pipeline`, `priority` and `trusted` are omitted; the `rework_base` assertion; the
    `DocketSetupReader` boundary test went with the subprocess grammar and the daemon's
    environment rule replaces it; docs that still describe a real-binary docket test.
  - **D1 closed** 2026-10-14 in `b3a689ae`: gate green, the whole Rust suite 1284 passed,
    3 skipped.
  - **U closed** 2026-10-14 in `4557834d` (U + U2 squashed): gate green; core, db and handlers
    506 passed; type-check, Vitest 643/643, token lint and the build green. U2's call, kept: a
    team no runner reports shows as a ✗ "unknown" line and neither caps the ceiling nor adds a
    blocker sentence.
- **Round 5** opened 2026-10-14: D2 cut from develop at `b3a689ae`. The `[Unreleased]` changelog
  section and `CLAUDE.md`'s "What's next" sentence are written and wait for Z's commit.
  - **D2** finished by its agent the same day; gate green, runner, cli, boundaries and handlers
    696 passed. D2's calls, kept: the daemon is sent one `kill -TERM` and waited for at most
    five seconds, docket finishing its task on its own time; the daemon's log is
    `<state_dir>/logs/docket-daemon.log`; `scripts/smoke.sh` drops its docket shim, since the
    fake is the proof; the doctor reads the runner config file so `[docket]` is real. Decided
    by the integrator: the daemon starts only when `[docket]` names `binary` (never because a
    `docket` is on `PATH`), because an unasked `docket start --dispatch` would sweep the
    operator's own pods under their own `~/.docket`; the Agents page and the doctor say "not
    configured: add a [docket] table". Not built, with the reason shown: the apply-team button
    (no board-to-runner channel exists; the runner protocol is runner-initiated and the only
    board→runner seam is the embedded runner's control), so the Teams rung says "In the
    project's folder, run: docket init --name <team>". Not written: the e2e scenario with two
    pods and a rework, which needs a live docket beta.6 and the ladder; the script is ready for
    it. Known: `metadata.item` is part of the request fingerprint, so an idempotent replay after
    the item's type or priority changed answers 409.
- **Round 7** opened 2026-10-17: the owner answered "continua" to the file schema shown
  2026-10-14; ADR 0022 written, the fixture `crates/tack-api/tests/fixtures/bundles/my-saas.yaml`
  is the specification, B1 cut from develop at `cff502aa`.
  - **B1 closed** 2026-10-17 in `f5999a88`: gate green in the worktree; core and db 283 passed,
    1 skipped; handlers 232 passed. B1's calls, kept: `serde-saphyr` as a dev-dependency of
    `tack-core` (the fixture and the YAML export tests need it); `OutlineProject.project_type`
    serializes as `type` with `project_type` still read; `skip_serializing_if` on every optional
    outline key and sorted vocabulary keys, so an export has no `null` and is deterministic;
    `sections` lists only the sections the file carries, automation reports `updated: 1`; a
    file profile named like a built-in is dropped with a warning and `not_mentioned` counts
    only custom profiles; status names in `automation` match case-insensitively; a `transitions`
    list beside a `preset` overrides the preset's; the export omits a medium priority, a
    story-points unit and a built-in profile, writes `closed` only for a closed sprint, and a
    midnight due date as a date; "Library implementer" and "Docs writer" carry an empty
    `tool_policy` (the task named no agent for them). B2 cut from `f5999a88`.
  - **B2 closed** 2026-10-17 in `04f1142e`: gate green in the worktree; handlers, the
    OpenAPI contract and security 271 passed. B2's calls, kept: the `Settings` ↔ `BundleAutonomy`
    mapping is inline in the three handlers (the task named no helper); a CSV answers
    `sections: []`; on an existing project the `project` block is removed before the check, so
    a bad workflow in an ignored block cannot refuse the file; the export sorts sprints by
    start date then name (the list route answers newest first, which reversed the round trip);
    an update keeps the board's value for every key the file leaves empty; the template path
    re-applies the template's own workflow after the bundle so `workflow_type` survives (a
    file's statuses can only say `custom`); the response's project is re-read after the
    writes. Findings: the fixture has 6 dependencies, not the 5 the task said; "the error
    names the section" holds in the server log only, the client sees a generic 500
    (`ApiError::Internal` hides its text; no new variant); `test_app()` seeds neither
    built-in profiles nor templates, so the bundle tests build their own app. B3 cut from
    `04f1142e`.
  - **B3 closed** 2026-10-17 in `98d413d5`: gate green in the worktree; tack-cli 122 passed;
    type-check, Vitest 644, token lint and the build green. B3's calls, kept: the `Template`
    dispatch moved into `cmd_template` (clippy's line limit on `main`); a UUID is recognised
    by hand (tack-cli has no `uuid` dependency); the CLI and the panel print the "items"
    section too and capitalise the section names themselves; the panel's new-project line
    reads "A new project. N items will be added with M dependencies."; the CLI tests drive
    the real binary against wiremock with an empty `HOME`, since the `cmd_*` functions are
    private; the book's section is an H3 under the import H2, beside "The CSV".
  - **Z2 closed** 2026-10-17 on the merged `develop`: generated files current; the gate green;
    `cargo nextest run --workspace` 1310 passed, 3 skipped; type-check, Vitest 644 in 68 files,
    token lint and the build green; `scripts/check-comments.sh` green. Wave 7 is in `develop`,
    unpushed, as B1 `f5999a88`, B2 `04f1142e`, B3 `98d413d5` and this docs commit.
  - **Two follow-ups** asked by the owner the same day and done by the integrator: a section
    that fails to write answers `ApiError::SectionFailed` ("The roles section could not be
    written. What the file wrote before it stays, so importing it again finishes the job."; on a
    new project, "so nothing was created"); the Templates page shows a template's roles,
    profiles and merge level as chips, lists "role → profile" on hover, and both the create and
    the save-as-template dialogs say that roles, profiles and automation travel with it.
  - **The book is three guides** (2026-10-17, `f5b7b579`): a User, an Administrator and a
    Developer Guide, with a page for automating a project, reviewing a run, installing runners,
    troubleshooting the server, runs and teams, the project file format and the decisions; every
    old address redirects. Writing them found three faults, fixed in the same commit: turning on
    Start did not save the status the list showed, a docket run showed no sandbox line, and
    three statements in the guides were wrong.
  - **Pushed** with the rest of `develop` on 2026-10-17. CI went red on two things the local gate
    does not check, both fixed in `e4c0f8fc`: RustSec marked `zvariant`, `zvariant_derive` and
    `zbus_names` unmaintained the same week (reviewed and ignored with a date, since
    `secret-service` 5.2.0, the newest, still needs zbus 5), and the learning path's overview was
    a `README.md`, which mdBook writes as `index.html`, so the link to it broke.
- **Phase 73 is built.** What stays open is the live proof on docket `0.2.0-beta.6` and the
  end-to-end scenario with two teams and a rework; the roadmap's "Open" section lists them with
  the gaps shown in the product.
