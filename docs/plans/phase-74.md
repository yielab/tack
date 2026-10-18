# Plan: Phase 74 — what the first real project found

**Status: opened 2026-10-17.** Serves ADR 0082
(`docs/adr/0082-what-the-first-real-project-found.md`). Written against `develop` at `d99503dd`.
Every path and line below was read that day; re-run `rg -n` before trusting a line number.

What this phase does, in one paragraph. The Fabulada onboarding put a real project into Tack and
docket and found twenty defects in Tack. This phase closes them at their roots, one owner per
fact: a run that cannot start ends with its reason, and one nobody holds asks a person; the
runner's name comes from the board and its liveness from one rule; what a runner reports about
its teams stays current and the ceiling is per team; everything Tack runs is logged; the doctor
reads the runner's own file; the project file round-trips the auto-start switch; the docket
contract is proven by captures; four screens use the project's words. It ends by running the
Fabulada calibration (R0) again on the real board, which is also Phase 73's live proof.

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

Every row of ADR 0082's table and its three defaults, plus:

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
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<id>`.

| Round | Tasks | Needs | Defects |
|---|---|---|---|
| 0 | ADR 0082, this plan, the roadmap section (integrator) | the maintainer's approval | — |
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
The ADR is docs/adr/0082-what-the-first-real-project-found.md; its table and this plan's "Decided" table are the decision, do not re-open them.
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
`frontend/src/features/settings/panels/ImportFromFile.tsx`,
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
  it does not report is held with "Team fab-dev is not reported yet by this computer"; a run of an
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
- If docket fixes K7 as ADR 0082 defaults, the client does not change; if it decides the other
  way, the client, the e2e body and the capture change in this task, with no alias.

**Tests:** the provenance test above; every existing `docket_v5.rs` test still passes against the
captures.

**Done when:** `cargo nextest run --workspace -E 'package(tack-runner)'` and the gate are green,
and the capture script ran once against docket with K7 fixed.

### Z — docs, the gate, and R0 on the real board (integrator)

- `CHANGELOG.md` `[Unreleased]`, by hand, one line per decision of ADR 0082 that landed.
- The book: the troubleshooting page gains "A run says the computer stopped answering" and
  "The board's log", in the Administrator guide's words (no paths, no ids).
- `CLAUDE.md` "What's next" and the roadmap's Phase 74 status.
- The integrated gate on `develop`.
- R0 on the maintainer's board with docket K7 fixed: the three Fabulada calibration tasks (a bug
  with a test, a piece of content, a research note) through their teams, each run read to its end
  in the UI, the numbers written into Fabulada's `BLUEPRINT.md` by pull request. This is also
  Phase 73's live proof; `scripts/docket-prill-e2e.sh` runs once in the same round.
- The defects list beside the test is marked with the commit that closed each row.

## Open questions

Blocking: none. Not blocking: ADR 0082's three defaults. X waits on docket's answer to K7, which
is docket's decision (`docket-triage.md` recommends the same default).

## Status

- **Round 0** opened 2026-10-17: ADR 0082 (proposed), this plan, the roadmap section.
