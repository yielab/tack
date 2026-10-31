# Plan: Phase 74 — what the first real project found

**Status: opened 2026-10-17.** Serves ADR 0023
(`docs/adr/0023-what-the-first-real-project-found.md`). Written against `develop` at `d99503dd`.
Every path and line below was read that day; re-run `rg -n` before trusting a line number.

What this phase does, in one paragraph. The My SaaS onboarding put a real project into Tack and
docket and found twenty defects in Tack. This phase closes them at their roots, one owner per
fact: a run that cannot start ends with its reason, and one nobody holds asks a person; the
runner's name comes from the board and its liveness from one rule; what a runner reports about
its teams stays current and the ceiling is per team; everything Tack runs is logged; the doctor
reads the runner's own file; the project file round-trips the auto-start switch; the docket
contract is proven by captures; four screens use the project's words. It ends by running the
My SaaS calibration (R0) again on the real board, which is also Phase 73's live proof.

## The rule for every task: fix the seam, not the caller

Each defect was a fact computed in two places, or an error that took a second road. The fix
always goes into the one place that already owns the fact, and the other callers read it.

| Fact | Its one owner after this phase | Callers that stop computing it |
|---|---|---|
| A pre-start failure | `RunnerEngine::fail_before_spawn` (`engine.rs`) | the provision step, `validate`'s other errors, `start` |
| An expired, unrenewed lease | the claim poll (`tack-orch/src/scheduler/wiring.rs`, beside the unclaimable rule) | nobody closed it before |
| A runner's name | the `agent_runners` row, written at provision or enrollment | `refresh` (both UPDATEs) |
| A runner is online | `SchedulingPolicy::is_online(runner, now)` (`tack-orch/src/scheduler/select.rs`) | `default_selector`, `no_runner_online`, the scheduler's filter, `runnerObservations.ts` |
| A team caps the merge | `is_attended` (`tack-api/src/autonomy.rs`), per team | `ceiling_of`'s machine-wide fallback, `Teams.tsx`'s `lightsOut()` |
| The settings ceiling | `current()` in `handlers/autonomy.rs`: the lowest of what it lists | `Ceiling.tsx` prints it, never decides it |
| What the log keeps | `tracing_subscriber_for` (`tack-api/src/server.rs`) | — |
| The runner's config file | `local_runner::load_runner_config` | the doctor |
| The per-user data folder (CLI) | `service::data_root` (`tack-cli`) | `config::read_server_token_file`, the new start-up warning |
| The project file's auto-start section | one constructor beside `BundleAutoStart` | `export.rs`, `templates.rs` |
| docket's wire | the captured fixtures under `fixtures/docket/v5/` | the hand-written fake |
| A page's name | one table of top-level pages (`frontend/src/app/`) | `Layout.tsx`, `Breadcrumb.tsx` |
| An item type's key | `typeKey` in `shared/ui/TypeBadge.tsx` | `Table.tsx`, `computeStats.ts` copies |
| Importing a project file | one `ImportFromFile` component | `DataPanel.tsx` inline flow, `CreateProjectModal.tsx` |

No task adds a layer, a trait, a loop, a table or a setting the task does not name. A new number
(a grace, an interval) is a named constant beside the one it derives from, never a config key.

## Decided — not re-opened by any task

Every row of ADR 0023's table and its three defaults, plus:

| Question | Decision |
|---|---|
| Migrations | None. Every fix reads or writes columns that exist. A task that needs one stops and reports it. |
| The expired-lease rule | In the claim poll, for attempts in `preparing`, `running` or `waiting_decision` whose `lease_expires_at` is older than `now - 2 × lease_duration`: the attempt and its request move to `needs_operator` through the same repository function the recovery path uses for `RecoveryDisposition::NeedsOperator`, with the reason `lease_expired` and the sentence "The computer that took this run stopped answering." Idempotent: a second poll finds nothing. |
| Cancel without a holder | `request_execution_cancellation` ends the attempt and the request `cancelled` at once when the attempt is `needs_operator` or its lease has expired; otherwise it stamps `cancellation_requested_at` as today. |
| Refresh errors | `establish_session` enrolls again only on `credential_refused` / `revoked` (the codes the refresh route returns for a bad credential); any other error is returned and the runner's retry loop handles it. The refresh route maps `is_unique_violation` to a 409 naming the column, never a bare 500. |
| `online` on the wire | `GET /api/runners` gains `online: bool`, computed with `is_online` at read time. Nothing stored. |
| Capability refresh | `CAPABILITY_REPROBE_INTERVAL = 5 min` in `transport.rs`; after each attempt and on that interval while idle, the runner calls `capability_report()` again and sends `refresh` only when it differs from the last one sent. |
| The docket version on the Agents page | The probe writes docket's own version when `GET /status.json` carries one, else nothing; "API 5" moves to its own `api_version` field, which the page shows as "API 5". No "v" before a string that is not a version. |
| The log filter | `"tack={level},tower_http={tower}"`: a target directive matches by prefix, so `tack` covers every Tack crate. `RUST_LOG` still wins. |
| The data-folder warning | Lives in `tack-cli` (`local_runner::serve`), which has `dirs`; `tack-api` gains no dependency. Shown only when `TACK_DATABASE_URL` is unset, no `tack.toml` names a database, and `<data root>/tack.db` exists. |
| The docket captures | `scripts/capture-docket-fixtures.sh` starts `docket start --dispatch` in a throwaway `HOME` and `DOCKET_HOME`, with no provider configured, and records the read routes, `POST /pods {ensure}`, and the 400/422 answers of `POST /tasks/<pod>` (no task runs, zero spend). Each answer is `v5/<route>.json` with a `.provenance` sidecar (`captured`, the command, the docket version, the date). A task's full lifecycle stays constructed, marked `constructed`. |
| Generated files | A task that changes an API type regenerates and stages them (`./scripts/regen-generated.sh`). |

## Rounds

At most two agents building cargo at once (`feedback_agent_load_caps`); the frontend task runs
beside them. Worktrees under `/var/tmp/tack-worktrees/p74-<id>` cut from `develop`,
`CARGO_TARGET_DIR=$HOME/Sites/.cargo-targets/tack-agent/<id>`.

| Round | Tasks | Needs | Defects |
|---|---|---|---|
| 0 | ADR 0023, this plan, the roadmap section (integrator) | the maintainer's approval | — |
| 1 | F (a run that cannot start ends), I (one name, one liveness rule); W (screens) beside | — | T19; T14, T17, T3, T4; T5, T9, T11, T12 |
| 2 | C (current capabilities, per-team ceiling), L (logs, messages, the file's switch) | I (C touches `transport.rs` after it) | T13, T6, T10, T15, T16; T18, T7, T2, T1, T8 |
| 3 | X (the docket contract from captures) | docket's answer on K7 | T20 |
| 4 | Z (integrator): docs, changelog, the gate; then R0 on the real board | everything, and docket with K7 fixed | the live proof |

Ceilings: Sonnet one layer 100 calls, two layers 150. One that reaches it hands off and is
replaced fresh with a shorter brief.

## How a task is handed to an agent

As Phase 73: one agent, one task, one branch from `develop` named `phase74/<id>`; the brief is
the task's section of this file plus these lines, and nothing else:

```
Task <id> from docs/plans/phase-74.md. Read that section, its "The rule for every task" table, and the files it lists, nothing wider.
The ADR is docs/adr/0023-what-the-first-real-project-found.md; its table and this plan's "Decided" table are the decision, do not re-open them.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Fix the fact in its owner and make the other callers read it; never add a second copy of a rule.
Add only the tests the task names, RED first. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag, loop or helper the task does not name.
Never call a real model endpoint; never run docket or any CLI against the operator's own HOME or ~/.docket.
Tack's words on every screen: agent, profile, team, this computer, folder, branch; never pod, harness, hop, runner, dispatch.
Build and test with CARGO_TARGET_DIR=<given>, --build-jobs 4, --test-threads 4; only the binaries you change while working.
Finish by running PATH=$HOME/.cargo/bin:$PATH .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do; your tool-call count.
```

## The tasks

### F — a run that cannot start ends, and one nobody holds asks a person (Sonnet, two layers)

**Files:** `crates/tack-runner/src/engine.rs` (`run_claimed`, `fail_before_spawn`),
`crates/tack-runner/src/engine/tests.rs`, `crates/tack-orch/src/scheduler/wiring.rs`,
`crates/tack-orch/src/scheduler/select.rs` (one constant), `crates/tack-db/src/repo/execution.rs`
(`request_execution_cancellation`, one query for expired leases),
`crates/tack-db/tests/repository/execution_recovery_requeue.rs`,
`crates/tack-api/src/handlers/executions.rs` (the cancel answer's text only),
`crates/tack-runner/tests/docket_v5.rs` (one test).

**Do:**

- `run_claimed`: build the `ExecutionSpec` before the workspace is provisioned, so every step
  from provisioning to the first `ProcessObservedRunning` journal write can end through
  `fail_before_spawn`. One `match` routes `Err(HarnessError::Rejected { reason })` from both
  `validate` and `start`, a provisioning error, and any other `HarnessError` (its `Display`, which
  never carries a secret) to `fail_before_spawn`. An error talking to the board stays an
  `EngineError`.
- `fail_before_spawn`'s capability reason reads "the agent refused this run before it started";
  its doc comment lists the four entries.
- The expired-lease rule and the cancel rule exactly as "Decided" says. The grace is
  `EXPIRED_LEASE_GRACE = 2 × lease duration`, a constant beside `max_heartbeat_age` with its
  derivation in the comment.
- The cancel handler's text for a run that is now terminal reads "Cancelled."; the CLI prints the
  server's text, so `tack-cli` does not change.

**Tests (RED first):**

- `engine/tests.rs`: the twin of `a_rejection_at_validate_is_reported_failed_and_never_spawns`
  for a rejection at `start`, and a row for a provisioning error. One table with the three.
- `docket_v5.rs`: the fake answers `POST /pods` with 400; the attempt ends `failed` with that
  body's text, and no task is posted.
- `execution_recovery_requeue.rs`: an attempt in `preparing` with a lease expired past the grace
  becomes `needs_operator` on one poll and nothing changes on the second; one inside the grace is
  untouched; cancel on `needs_operator` ends it `cancelled`.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + package(tack-orch) +
package(tack-db)'` and the gate are green.

### I — one name, one liveness rule (Sonnet, two layers)

**Files:** `crates/tack-api/src/handlers/runner_protocol.rs` (the two refresh UPDATEs, their
error mapping), `crates/tack-runner/src/transport.rs` (`establish_session` only),
`crates/tack-orch/src/scheduler/select.rs`, `crates/tack-api/src/handlers/executions.rs`
(`default_selector`, `no_runner_online`), `crates/tack-api/src/handlers/runner_admin.rs` (the runner
list's response type), `crates/tack-runner/src/harness/docket/probe.rs` (the version fields),
`crates/tack-orch/src/execution/capabilities.rs` (`api_version` on the harness capability, serde
default), `crates/tack-cli/src/main.rs` (`Doctor`), `crates/tack-cli/src/doctor.rs`,
`frontend/src/features/agents/runnerObservations.ts`,
`frontend/src/features/agents/steps/HarnessStep.tsx`, the tests named below, the generated files.

**Do:**

- Refresh never writes `name`; drop the column and its bind from both UPDATEs, with the comment
  that points at `redeem_enrollment_token`. `is_unique_violation` maps to 409 naming the column.
- `establish_session`: enroll again only on the refused-credential codes; otherwise return the
  error.
- `SchedulingPolicy::is_online(&runner, now) -> bool` from the predicate in `no_runner_online`;
  the scheduler's own filter, `no_runner_online` and `default_selector` call it.
- `GET /api/runners` returns `online` per runner. `findThisMachineRunner` takes the online
  `local-*` runner with the newest heartbeat, else the newest; "Up to N runs" counts online ones.
- The docket probe: `installed_version` from `/status.json` when docket sends a version, else
  absent; `api_version: "5"`. `HarnessStep.tsx` prints "API 5" without "v".
- `tack runner doctor --config <file>`: `load_runner_config(cli, path.or(runner_config_path()))`,
  the error printed and the exit code non-zero on a broken file; the state directory is the one
  the same file names.

**Tests (RED first):**

- `crates/tack-api/tests/runner_protocol/enrollment.rs`: a runner provisioned as `local-<uuid>`
  refreshes with `runner_id = "local-runner"` while another row is named `local-runner`: 200, the
  name unchanged.
- `crates/tack-cli/tests/embedded_runner_id.rs`: the embedded runner restarts twice on one
  database that already holds a `local-runner` row; it is `running` both times.
- `crates/tack-api/tests/handlers/executions_runner_admin.rs`: two active runners, one with a
  heartbeat two hours old; a run without a machine resolves to the live one.
- `select.rs` tests: `is_online`, one table (no heartbeat, stale, fresh, revoked).
- `runnerObservations.test.ts`: two `local-*` rows, the newer online one wins.
- `crates/tack-cli/src/doctor/tests.rs`: `--config` with a `[docket]` table is read; a broken file
  is an error.

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-cli) +
package(tack-orch)'`, `cd frontend && npm test -- runnerObservations`, and the gate are green.

### W — the screens use the project's words and one import (Sonnet, frontend)

**Files:** a new `frontend/src/app/pages.ts` (the table of top-level pages: path, label),
`frontend/src/app/Layout.tsx`, `frontend/src/shared/ui/Breadcrumb.tsx`,
`frontend/src/features/dashboard/Dashboard.tsx`, `frontend/src/features/dashboard/computeStats.ts`,
`frontend/src/features/table/Table.tsx` (its `typeKey` copy only),
`frontend/src/features/settings/ProjectSettings.tsx`, a new
`frontend/src/shared/ui/ImportFromFile.tsx` (shared, because a feature may not import another),
`frontend/src/features/settings/panels/DataPanel.tsx`,
`frontend/src/features/projects/CreateProjectModal.tsx`, the tests named below.

**Do:**

- `pages.ts` lists Projects, Templates, Agents and Settings with their paths (`routes.tsx`'s top level); the title and
  the breadcrumb both read it. A path not in it falls back to Projects, as today.
- `Dashboard.tsx` labels each type with `t(typeKey(type.name))`; the two other `typeKey` copies go
  and import the one in `TypeBadge.tsx`.
- `ProjectSettings.tsx` derives the active tab from `searchParams.tab` reactively; an unknown id
  shows General.
- `ImportFromFile` takes a `target` (`{ kind: 'new' }` or `{ kind: 'project', id }`) and holds the
  preview and confirm that `DataPanel.tsx` has inline today (`PendingImport`, `send`,
  `previewFile`, `confirmImport`, the dialog). `DataPanel` uses it for both targets;
  `CreateProjectModal` gains "From a project file" as a third way, using it with `kind: 'new'`.

**Tests (RED first):**

- `frontend/src/features/agents/AgentsPage.test.tsx`: the title reads "Agents · Tack" and the
  breadcrumb "Agents".
- `computeStats.test.ts`: a project whose vocabulary renames Epic shows the project's word.
- `panels.test.tsx`: `?tab=automation` opens Automation; changing it to `roles` follows.
- `CreateProjectModal.test.tsx`: "From a project file" previews a bundle and creates the project
  with one confirm; the Data panel's existing import tests still pass unchanged.

**Done when:** `cd frontend && npm run type-check && npm test`, `npm run lint:tokens` and `npm run build`
are green.

### C — what a runner reports stays current, and the ceiling is per team (Sonnet, two layers)

**Files:** `crates/tack-runner/src/transport.rs` (the re-probe in `serve`'s idle and after-attempt
paths), `crates/tack-runner/src/bootstrap.rs` (`report_capabilities` called again, no change in
shape), `crates/tack-api/src/autonomy.rs` (`ceiling_of`, `is_attended`),
`crates/tack-api/src/handlers/autonomy.rs` (`current()`),
`frontend/src/features/settings/panels/automation/Teams.tsx`,
`frontend/src/features/settings/panels/automation/Ceiling.tsx`, the tests named below.

**Do:**

- Re-probe as "Decided" says. The comparison is on the serialized report, so nothing new is
  stored.
- `ceiling_of`: a run whose team the snapshot reports takes that team's ceiling; a run whose team
  it does not report is held with "Team saas-dev is not reported yet by this computer"; a run of an
  agent with no teams keeps today's `autonomy_ceiling`. The machine-wide fallback goes for docket.
- `current()`: the level is the lowest `caps_at` over every listed team, and L3 when
  `automatic_merge_blocked_by` is not empty.
- `Teams.tsx` shows each team's state from the server's `settings.teams` (`caps_at`), as
  `Ceiling.tsx` does; `lightsOut()` goes. The apply button stays disabled; its reason says what is
  missing: "Tack cannot run this on the team's computer from the board yet. In the project's
  folder, run:" and the command.
- `Ceiling.tsx` prints the server's level and never computes one.

**Tests (RED first):**

- `crates/tack-runner/tests/docket_v5.rs`: a pod added to the fake after the runner started is in
  the next refresh; an unchanged report sends no refresh.
- `crates/tack-api/tests/handlers/autonomy.rs`: two teams on one runner, one attended, one
  lights-out; a run of the lights-out team is not capped (T6); a team the snapshot lacks holds
  with the sentence; the settings level is L3 when a blocker is listed (T10).
- `panels.test.tsx` "The ladder": an attended team with no violations shows "needs a person"
  (T15); the apply button's reason.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + package(tack-api)'`,
`cd frontend && npm test -- panels`, and the gate are green.

### L — logs, messages and the project file's switch (Sonnet, one layer)

**Files:** `crates/tack-api/src/server.rs` (`tracing_subscriber_for`, the start-up lines),
`crates/tack-api/src/server/tests.rs`, `crates/tack-runner/src/config.rs`,
`crates/tack-runner/src/error.rs`, `crates/tack-runner/src/config/tests.rs`,
`crates/tack-cli/src/local_runner.rs` (`serve`, the warning only),
`crates/tack-cli/src/service.rs` (`data_root` made `pub(crate)`), `crates/tack-cli/src/config.rs`
(`read_server_token_file` uses it), `crates/tack-core/src/outline.rs` (`BundleAutoStart`'s
constructor), `crates/tack-api/src/handlers/export.rs`, `crates/tack-api/src/handlers/templates.rs`,
`crates/tack-api/tests/handlers/bundle.rs`, `docs/CONFIG.md` (the database row and the log row).

**Do:**

- The filter as "Decided" says.
- The start-up line names the absolute database path; a build without the UI adds "This build
  serves the API only; the web UI is in the release build or `npm run dev`."
- The data-folder warning as "Decided" says, reusing `data_root`.
- `ConfigError::UnknownKey { key }`: the closure that already re-parses as `toml::Table` compares
  its top-level keys with `FileConfig`'s and names the first unknown one; a table that is not a
  key of `FileConfig` reads "unknown table [runner]: the runner's settings go at the top of the
  file". Never a value.
- `BundleAutoStart::from_project(enabled, status, max_open)` writes `enabled: Some(enabled)`;
  `export.rs` and `templates.rs` call it.

**Tests (RED first):**

- `server/tests.rs`: an event from target `tack_runner::transport` at `info` is kept by the
  default filter.
- `config/tests.rs`: `[runner]` names the table; an unknown top-level key names the key; a value
  never appears in the message (one table).
- `bundle.rs` `the_export_is_the_import`: a project with auto-start off exports `enabled: false`,
  and importing that file into a project with it on turns it off.
- `crates/tack-cli/tests/cli_test.rs`: one row for the warning (a temp data root with a
  `tack.db`, the server started in another folder).

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-runner) +
package(tack-cli) + package(tack-core)'` and the gate are green.

### X — the docket contract from captures (Sonnet, two layers; waits on docket's K7 answer)

**Files:** a new `scripts/capture-docket-fixtures.sh`,
`crates/tack-runner/src/harness/fixtures/docket/v5/` (the captures, their `.provenance` files, the
README rewritten), `crates/tack-runner/tests/fake_docket/mod.rs`,
`crates/tack-runner/tests/docket_v5.rs`, `crates/tack-runner/src/harness/docket/client.rs` (only
if a capture shows a field Tack sends wrong), `scripts/docket-prill-e2e.sh` (the `POST /pods` body
only, from the same capture), `docs/TESTING.md` (the docket row).

**Do:**

- The capture script as "Decided" says. It refuses to run when `DOCKET_HOME` is unset or is the
  operator's `~/.docket`, and stops naming `docket 0.2.0-beta.6` when `apiVersion` is not `"5"`.
- The fake answers each captured route from its file and checks each request body against the
  captured request's keys: an unknown or missing key is a 400 naming it, so the fake is never
  more lenient than docket.
- `docket_v5.rs`: one test reads every `.provenance` and fails if a route the client sends has
  no `captured` file.
- If docket fixes K7 as ADR 0023 defaults, the client does not change; if it decides the other
  way, the client, the e2e body and the capture change in this task, with no alias.

**Tests:** the provenance test above; every existing `docket_v5.rs` test still passes against the
captures.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner)'` and the gate are green,
and the capture script ran once against docket with K7 fixed.

### Z — docs, the gate, and R0 on the real board (integrator)

- `CHANGELOG.md` `[Unreleased]`, by hand, one line per decision of ADR 0023 that landed.
- The book: the troubleshooting page gains "A run says the computer stopped answering" and
  "The board's log", in the Administrator guide's words (no paths, no ids).
- `CLAUDE.md` "What's next" and the roadmap's Phase 74 status.
- The integrated gate on `develop`.
- R0 on the maintainer's board with docket K7 fixed: the three My SaaS calibration tasks (a bug
  with a test, a piece of content, a research note) through their teams, each run read to its end
  in the UI, the numbers written into My SaaS's `BLUEPRINT.md` by pull request. This is also
  Phase 73's live proof; `scripts/docket-prill-e2e.sh` runs once in the same round.
- The defects list beside the test is marked with the commit that closed each row.

### R — a docket task that outlives the runner is followed again (Sonnet, two layers)

ADR 0023's amendment, decision 1. Found in R0 on 2026-10-18: a restart of `tack serve` left two
docket tasks that were waiting on an approval in `needs_operator`, and an operator had to requeue
them by hand.

**Files:** `crates/tack-runner/src/journal.rs` (the claim kept in the record),
`crates/tack-runner/src/engine.rs` (`recover`, `run_claimed` split at the point the agent is seen
running), `crates/tack-runner/src/harness/mod.rs` (`resume` on the adapter trait, routed by kind
like `reconcile`; the default answers `Unsupported`), `crates/tack-runner/src/harness/docket.rs`
(`resume`: rebuild `RunInfo` from the journal's claim and its `pod/task` handle, then `follow`,
which already reads its persisted cursor), `crates/tack-runner/src/engine/tests.rs`,
`crates/tack-runner/tests/docket_v5.rs`, `crates/tack-runner/tests/fake_docket/mod.rs` (only if
the test needs a route the fake lacks), `docs/book/src/admin-guide/recovery-runbook.md` (the
`process_running` row), `docs/book/src/developer/runner-contract.md` (the recovery section).

**Do:**

- `AttemptJournal.claimed_work: Option<ClaimedWork>` with `#[serde(default)]`, written in the
  `persist_before_spawn` record. An old journal has none and recovers as today. The claim carries
  references to secrets, never their values. If a field turns out to carry a value, stop and
  report it.
- In `recover`, before `report_recovery_and_apply_disposition`: when `reconcile` answers
  `ProcessRunning`, the record has a claim and `adapter.resume(&spec, &record)` returns a handle,
  send one heartbeat for that lease with the journal's fencing token. If it is accepted, run the
  same post-start path as `run_claimed` (follow, evidence, verifier, publish, terminal report).
  There is one copy of that path, called from both places. If the heartbeat is refused, or
  `resume` fails or answers `Unsupported`, go on to the recovery report exactly as today.
- The attempt's deadline starts over at the resume. Say so in the code's comment and in the runbook.
- A question docket still holds is not raised twice: `follow` starts with the attempt's decisions
  already on the board in its `asked` set (read once from the server). If that read needs a route
  the runner does not have, stop and report it.
- Every local-process harness keeps answering `Unsupported`: its child died with the runner.

**Tests (RED first):**

- `engine/tests.rs`: a journal with a claim and a running handle whose lease heartbeat is accepted
  is followed to `succeeded` with no recovery report; the same with the heartbeat refused reports
  `process_running` and ends `needs_operator`; a journal without a claim behaves as today (one
  table).
- `docket_v5.rs`: a task parked on an approval survives a new adapter (a restart). The approval
  is not raised again, answering it lets the task end, and the attempt ends with evidence.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner) + package(tack-orch)'` and
the gate are green.

### T — the board makes its decision token, and the desktop app hands it over (Sonnet, two layers)

ADR 0023's amendment, decision 2. Found in R0 on 2026-10-18: a new `tack serve` had no
`TACK_EXECUTION_DECISION_TOKEN`, so every answer from the Inbox was a 403 and the run waited.

**Files:** `crates/tack-api/src/config.rs` (`decision_token_file` and `ensure_decision_token`
beside `api_token_file` and `ensure_api_token`, with the same rules for where the file lives and
for loopback), the place `ensure_api_token` is called at start, `crates/tack-api/src/config/tests.rs`
(or the module's existing tests), `crates/tack-desktop/src/` (where the app signs its window in:
the decision token goes through the same door, into the key `decisionTokenStore` reads),
`frontend/src/shared/execution/decisions.ts` and the screen that asks for the token (the
sentence only), their unit tests, `docs/CONFIG.md` (the decision-token row),
`docs/book/src/admin-guide/` (the page that lists the board's files).

**Do:**

- When the variable is unset or empty, read `decision-token` beside `api-token`, or create it
  with 0600 and 32 random bytes, the way `api-token` is made. The variable still wins. The value
  is never logged.
- The desktop app reads the file and gives it to its window through the door it already uses for
  its sign-in. If there is no such door, stop and report how the window signs in.
- In a browser, the prompt for the token names the file: "the decision-token file beside the
  board's database". No path, since the screen does not know it.

**Tests (RED first):** config: missing → created 0600 and reused on the next start; the variable
wins; an empty file is refused the way an empty `api-token` is (one table). Frontend: the prompt
names the file. Desktop: the token reaches the window's store (a unit test if the door can be
tested; otherwise the finding says why not).

**Done when:** `cargo nextest run --workspace -E 'package(tack-api) + package(tack-desktop)'`,
`cd frontend && npm test`, and the gate are green.

### U — a run that ended says why, on every screen (Sonnet, two layers)

Found in R0 on 2026-10-18: My SaaS's CI1 waited an hour on an approval nobody gave and ended
`failed` with `terminal_reason` `{"code": "timeout"}` and no message, and the item's Last run
panel showed "Failed" alone. That panel prints what the agent said, never why a run stopped, so a
`no_change` or a refused start reads the same way.

**Files:** `crates/tack-runner/src/harness/docket.rs` (the deadline branch of the follow loop),
the other adapters' own deadline branches if they end without a `message` (read them; change only
those that do), their tests (`crates/tack-runner/tests/docket_v5.rs` and the adapters' existing
tables), `frontend/src/shared/runWithAgent/LatestRunSummary.tsx` and its unit test,
`frontend/src/shared/runWithAgent/attemptFormat.ts` only if `describeAttemptOutcome` needs a row.

**Do:**

- A run whose time runs out ends with `message`: "The run's time ran out after N minutes", and,
  when a question was open, "while it waited for an answer". `code` stays `timeout`. N is the
  request's `timeout_seconds`, rounded to whole minutes (seconds below one minute).
- The Last run panel of a failed, cancelled or lost attempt prints `describeAttemptOutcome`'s
  `whyStopped`, the line the Execution tab already prints, in the place the agent's words go. The
  agent's words still show for a run that succeeded.

**Tests (RED first):** `docket_v5.rs`: a task parked on an approval past the deadline ends
`failed` with the message naming the wait; one without a question ends with the plain sentence
(one table if the file has one). Frontend: the panel of a failed attempt shows its reason, and of
a `timeout` with no message still shows a sentence, never the bare code.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner)'`, `cd frontend && npm
test`, and the gate are green.

### V — a question ends with its run, and the board finds the prill its runner uses (Sonnet, two layers)

Found in R0 on 2026-10-18, two defects of the board:

1. CI1's approval stayed `pending` in the Inbox after the run ended. The runner answers the agent
   with the question's deny option when the time runs out, but never tells the board, and the
   sweep that expires an overdue decision rides `TACK_EXECUTION_RETENTION_ENABLE`, off by default.
   Four more pending rows on the real board belong to attempts that ended `needs_operator`.
2. Factory metrics said "prill is not installed on the board" while the embedded runner verified
   two packs with prill: the runner names its program in `[verify] program`, the board looks for
   `prill` on its own `PATH`.

**Files:** `crates/tack-db/src/repo/execution.rs` (the function or functions through which an
attempt reaches `succeeded`, `failed`, `cancelled`, `lost` or `needs_operator`),
`crates/tack-api/src/handlers/decisions.rs` (the `expired` shape it already writes),
`crates/tack-api/src/server.rs` (start), `crates/tack-api/src/prill.rs`,
`crates/tack-api/src/prill_store.rs`, `crates/tack-api/src/config.rs` (`prill_program`), the
`LocalRunnerControl` trait and `crates/tack-cli/src/local_runner.rs`'s `EmbeddedRunnerControl`,
their existing tests, `crates/tack-api/tests/handlers/metrics.rs`, `docs/CONFIG.md`
(`TACK_PRILL_PROGRAM`'s row).

**Do:**

- When an attempt reaches one of those states, every one of its `pending` decisions becomes
  `expired` in the same transaction, `resolved_by` `{"kind": "system", "subject_id":
  "attempt_ended"}`. Owned by the repository's transition, not by each handler. If the
  transitions do not share one function, stop and report the list before adding a call to each.
- At start, the board closes the same way the pending decisions of attempts already in those
  states (the rows a board from before this change left). One idempotent statement, beside the
  transition's rule. The retention gate is not touched.
- The board's prill: `TACK_PRILL_PROGRAM` wins; else, with an embedded runner whose `[verify]` is
  prill, its `program`; else `prill` on `PATH`. The embedded runner says it through one method on
  `LocalRunnerControl` (default `None`).
- The sentence names the fix once, in `prill.rs`, and `prill_store.rs` reads it: "prill is not
  installed on the board (set TACK_PRILL_PROGRAM to its path)".

**Tests (RED first):** repository: an attempt that ends in each of the five states leaves no
pending decision, and a resolved one keeps its answer (one table); the start statement closes a
pending decision of an ended attempt and leaves a running attempt's alone. API: the board with an
embedded runner that verifies with a program elsewhere reports prill's version from it.

**Done when:** `cargo nextest run --workspace -E 'package(tack-db) + package(tack-api) +
package(tack-cli)'` and the gate are green.

### S — the tutorial's screenshots 10 and 11 (integrator)

`9d4ce61e` gave the tutorial's announcement writer read tools only; screenshots 10 (the profile
form) and 11 (the Run dialog) still show the old tools. Captured by `tutorial-assets.spec.ts`
against a fresh release build, headless. Test 11 drives a real, billed run after its screenshot,
so the spec stops before Run when `TUTORIAL_STOP_BEFORE_RUN=1`; only files 10 and 11 change.

## Open questions

Blocking: none. Not blocking: ADR 0023's three defaults. X waits on docket's answer to K7, which
is docket's decision (`docket-triage.md` recommends the same default).

## Status

- **Round 0** done 2026-10-17: ADR 0023 accepted with its three defaults, this plan, the roadmap section.
- **Round 1** done 2026-10-17: F, I and W.
  - F merged (`92dc79c8`). As built: one private `apply_recovery` holds the transition after
    the attempt row is read; `recover_attempt` (the runner's own report, with its runner check)
    and `expire_unheld_leases` (the claim poll, any holder state) both call it, and the sentence is
    written only when the attempt moved. The crash tests crash for real (a provisioner that never
    returns, aborted), since a failing provisioner is now a clean Failed.
  - I merged (`9899036c`). As built: the refused-credential codes are `unauthorized` (401) and
    `runner_revoked` (403); `api_version` rides in the capability's flattened `additional` map, read
    and written through two accessors; `is_online` takes `(active, last_heartbeat_at, now)` because the
    API and the scheduler hold different runner types; with no runner online, `default_selector`
    keeps every active one, so a single sleeping runner still gets the run and it waits with its
    reason. Added at integration: `isInstalled`, `isReady` and `versionText` in
    `runnerObservations.ts` are the one rule for the five agents screens, since docket publishes no
    version of its own.
  - W merged (`90fad17d`). Known, not in scope: Escape in a dialog opened from another closes both, because each `Modal` listens on `document`.
- **Round 2** done 2026-10-17: C and L.
  - C merged (`20247edf`). As built: the report is compared with `reported_at` and each
    `probed_at` neutralized; the re-probe runs on its interval whether or not a run is in flight
    (a claim occupies a slot, so "idle" cannot be told apart, and a probe is cheap). Added at
    integration: a team the computer does not report is a ceiling fact, `Facts::ceiling:
    Result<Option<Level>, String>` with its own `Hold::TeamNotReported`, not a borrowed `require`
    failure; the Teams rung reads the ladder's settings instead of fetching them again.
  - L merged (`9eb252f1`). As built: the per-user data folder is owned by `config::data_root` in
    the tack-cli lib (`service` is private to the binary and delegates to it); an unknown key is
    named only when serde's refusal names a key that is really in the file's top-level table.
- **Round 4, the part that does not need docket**, done 2026-10-18: the troubleshooting page's two
  entries, the roadmap's and `CLAUDE.md`'s status. On 2026-10-18 docket's `POST /pods` still read
  `project`/`path` (`develop` at `4ace6bd1`, the installed `0.2.0b6`); K7 went to docket's session
  with its evidence. Round 3 (X) and the runs on the real board start when it lands.
- **Round 3** done 2026-10-18: X merged (`0232f898`). docket fixed K7 the same day (`a9f6f81c`, name and
  location). As built: the captures come from the installed 0.2.0b6 in a throwaway home; they showed the
  adapter read a task's `error`, which only a hop has, so every failed docket run reached the board
  with an empty message (now `reason`), and six shapes the fake had wrong. No-model task shapes
  stay `constructed`.
- **R0, first pass** 2026-10-18: the three runs reached their teams. They first failed on "unknown pipeline",
  because the onboarding skill copied the pipelines to a folder docket does not read (fixed in the
  skill). After that, each ended on docket's default per-step limit of 100 000 tokens. Found on the
  way and fixed in `2ea23786`: the request trace filled nine lines in ten of an `info` log, and the
  doctor printed an empty version for docket. Paid runs pause on docket's K8, since its dollar budget
  counts an unpriced model as free. Open, not in this phase: a heartbeat whose capacity disagrees
  is answered as an idempotency conflict, once per runner start.
- **Round 5** opened 2026-10-18, from R0's runs on the real board (ADR 0023's amendment): R (a
  docket task that outlives the runner is followed again) and T (the board makes its decision
  token), side by side. Both merged the same day.
  - T merged (`fa3617f0`). As built: `ensure_decision_token` reuses `ensure_api_token`'s rules and
    file helper; the desktop app had no door to its window (it signs in by the server's HttpOnly
    cookie), so it gained an initialization script that sets the key the Inbox reads, for the
    board's origin only, from the file beside the database it starts the server with.
  - R merged (`dc276387`, test race fixed in `88053f77`). As built: the journal keeps the claim with
    literal environment values cleared. At integration, recovery was found to follow a resumed
    task inline, before the heartbeat ticker existed, so its lease would expire mid-follow and no
    new work would be claimed; `recover` now hands it back as `RunCycle::Resumed` and `serve`
    follows it in a slot. The question already on the board is waited on, never created again.
- **Round 6** opened 2026-10-18, from R0's report: U (a run that ended says why) and V (a question
  ends with its run; the board finds its runner's prill) side by side, then S (the tutorial's
  screenshots 10 and 11). All three merged the same day; the workspace passes 1381 of 1381.
  - U merged (`2542fb8c`). As built: the sentence is `harness::time_ran_out`, read by docket's
    deadline branch and by `local_process::outcome`, the one place every local agent's outcome is
    built, so a process stopped by its deadline is a timeout whatever its partial output reads as
    (the agent's first patch put it in claude-code's grammar alone, which missed a timeout after
    partial output and every other agent).
  - V merged (`a9b621bb`). As built: the four roads that end an attempt did not share a function,
    so the agent stopped and reported them; each now calls one private `end_attempt_decisions` in
    its own transaction. A `TACK_PRILL_PROGRAM` set to exactly `prill` counts as unset.
  - S: the spec had not followed `9d4ce61e` (it still gave the profile all tools, so a full
    re-record would have ended `no_change`), nor Phase 69 (its API calls sent no token, refused on a
    loopback board since), and it changed the agent before the Advanced section's `toggle` event
    landed, which closed the section. `TUTORIAL_STOP_BEFORE_RUN=1` records 01–11 at no cost.
