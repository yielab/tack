# Plan: Phase 67 — a run a normal person can start

**Status: proposed 2026-10-07, waiting on the maintainer's go.** Serves ADR 0074 (which amends
0069, 0071 and 0072). Written against `develop` at `c859729`; every locator below was read on
that commit. Re-run `rg -n` before trusting a line number. Nothing is dispatched.

What this phase builds, in one paragraph. Today a run is started from a dialog that asks, in
the runner's own words, for everything `POST /api/executions` needs — a remote URL, a base
revision, a harness, an agent profile, a provider and a model, a tool list — resets all of it
on every open, offers an "Auto" model that never schedules, accepts a provider the agent then
rejects, and shows a failed run with no reason and a succeeded run whose file was deleted
with its workspace. After this phase a project is created knowing where its code is; the
agent works in a branch of that folder (or in the folder, or in a clone for another computer);
no model or provider is required; four profiles exist with their tools; a task is written as
description plus acceptance criteria, with what only the agent needs collapsed; the dialog is
a pre-flight that shows what the agent will read and two switches; a finished run flags the
task **Needs your review** and hands the person the diff, the folder, the branch and the
result; and every word on screen is Tack's, with a `(?)` beside it.

## What was verified before planning (2026-10-07, `c859729`)

| Claim | Read at | Holds? |
|---|---|---|
| The scheduler rejects every `ModelSelector::AutoSelect` request, on every harness | `crates/tack-orch/src/scheduler/select.rs:151-160`; `scheduler/types.rs:148-153` | yes |
| The claude-code adapter declares it needs no model and names its native provider; `local_process.rs` already enforces `Explicit` and ignores `Optional` | `crates/tack-runner/src/harness/claude_code.rs:24-29`; `local_process.rs:60-65`, `:406-409` | yes. The attestation never reaches the capability snapshot or the scheduler |
| The adapter rejects an unknown provider before spawn; the UI validates nothing | `claude_code.rs:54-60`, `:196-205`; user's `att_4ad4867d` `terminal_reason.code = harness_rejected` | yes |
| Evidence capture diffs against `workspace.base_revision` verbatim; the checkout fetched into `FETCH_HEAD` and `refs/remotes/origin/<name>` only; reproduced: `git diff --cached develop` → `unknown revision`; against the sha → `A test.md` | `crates/tack-runner/src/git.rs:452-466` vs `resolve_revision` `:313-345`; scratch repro | yes. Every real-git test passes a commit id (`source.first_commit`), the book's example a 40-char sha |
| The workspace is deleted after the terminal report whether or not evidence was captured; push is off by default | `crates/tack-runner/src/engine.rs:696-720`, `:677`, `:782`; `config.rs:149`; `~/.local/share/tack/runner/workspaces/` empty after the user's run | yes |
| `terminal_reason` is on the attempt DTO, untyped, and no frontend file reads it | `crates/tack-api/src/handlers/executions.rs:441`; `docs/openapi.json:6533` (`{}`); `rg terminal_reason frontend/src` → tests only | yes |
| `RepositorySnapshot` carries `#[serde(flatten)] additional`, so a new key travels without touching the frozen fixtures | `crates/tack-orch/src/execution/types.rs:296-303`; `docs/contracts/runner-v1/claim.response.json:31-32` | yes. The runner's `RepositorySpec` reads only `remote` + `base_revision` (`client.rs:195-198`) |
| The project has `default_model` and `github_token_ref`, nothing about a folder; items have `parent_id`, no settings column; agent profiles are name/instructions/tool_policy/limits | `crates/tack-core/src/models.rs:27-46`, `:106`; `crates/tack-db/src/migrations.rs:1402-1410`; last migration `081_pull_requests` | yes |
| The desktop app already depends on `tauri-plugin-dialog` | `crates/tack-desktop/Cargo.toml:28` | yes; unused for folders today |
| `compose_instructions` builds what the harness reads: profile text, title, description, rendered brief | `executions.rs:562-583`; `crates/tack-core/src/brief.rs:39` | yes. Not exposed on any route |
| The form resets on every open by design; the remote is a hard error; `Kind` is an editable text field | `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:73-75`, `:180`; `RunFlow.tsx:191` | yes |
| Native `<select>` has no `color-scheme`; `Select` and `Field` share tokens | `frontend/src/index.css:611` (media query only); `shared/ui/Select.tsx:29-34`, `Field.tsx:10-15` | yes |
| The Agents page's "Verified" is a per-tab signal; the test run requires remote, provider and model and creates an item | `features/agents/steps/ProviderStep.tsx:11-17`; `TestRunStep.tsx:73,79` | yes |
| Per-harness tool lists were measured in phase 66 | `docs/plans/measurements/tools-{claude-code,codex,docket,opencode}.md` | yes; profiles seed from them |

## Decided — not re-opened by any task

Phase 66's table stands except the one row this phase reverses. Decisions the maintainer took
on 2026-10-07, each one line to reverse:

- **Three layers.** Machine = `/agents`. Project = Settings → **Automation** (replaces the
  Agents tab). Task = the dialog. A value is owned by the highest layer that can; lower layers
  show it read-only with "Change".
- **The project's code origin is asked at creation** — existing folder / new folder Tack creates
  (`git init` ticked) / none — and may be changed in Automation. It sets the mode: git folder →
  `local_branch`; folder without git → `in_place`; URL → `clone`. The person sees "on a new
  branch" / "in the folder", pre-selected.
- **`local_branch` reverses phase 66's "nothing survives as a local branch"**: the branch
  `tack/<item short id>-a<n>` stays in the user's repo. `in_place` never deletes or `git add`s
  anything of the user's. `clone` stays, fixed, for remote runners.
- **Push off by default**, per project (`push_after_run`), honoured only when the runner has
  `[git] push_branches`; otherwise `attempt.push_skipped` with the reason, never silent.
- **The agent's default model is the default**: the harness reports `model_selection`; the
  scheduler accepts Auto on `optional`; `AutoSelectNotVerified` stays for a harness that reports
  `required` or nothing. The provider leaves the UI: `native_provider` from the harness, or the
  configured gateway.
- **Four built-in profiles, seeded**: Implementer (default), Reviewer, Researcher, Planner.
  Pills with a tooltip (what it may do, one line). No "cautious" profile; "Ask before each
  action" is a switch on the run.
- **The Planner proposes, a person creates.** `tack-plan.json` at the workspace root is staged
  as an artifact of kind `plan` (contract `plan-v1`) and excluded from the evidence diff.
  Subtasks are created from a panel, selected and editable, never by the agent.
- **A task's form is description + acceptance criteria** (the brief's `manual` criteria, as a
  checklist of sentences) with "For the agent" collapsed (the other criterion kinds,
  constraints, risk). The definition of done is the project's (`projects.definition_of_done`),
  appended by `compose_instructions`. The Brief tab goes; the entity and the wire do not.
- **Needs your review is a flag, not a column.** Derived: the latest terminal attempt captured
  changes and has no verdict. Cleared by `POST …/attempts/{n}/review` (accept/reject, note),
  distinct from the MRP review. The project may also name a column to move to on finish
  (`on_finish_status`, default none) through the existing `update_item_atomically` path.
- **Succeeded is green only with captured changes.** Amber: finished with no changes, or
  changes not captured (workspace kept). Red: failed, reason first.
- **Nothing resets.** `items.run_settings` holds only what the person changed for that task;
  the idempotency key alone is fresh per open.
- **Vocabulary** per ADR 0074 decision 9. The wire, the CLI flags and the contracts keep their
  names; only screens and the book change.
- **The migrations of this phase land together, first (S0)**, exactly as spelled there. A
  consumer that finds a column wrong adds `083_…` and reports; it never edits S0's.

## The waves

Two agents at a time. A wave's tasks share no file, and no task changes a type another task
in the same wave calls (phase 66's lesson: disjoint files are not enough).

| Wave | Tasks | Needs | Closes findings |
|---|---|---|---|
| **0 — stop losing data** | Z1, Z2, Z3, Z4, Z5, Z6a, Z6b | nothing | 2, 8, 11, 12, 13, 14 |
| **1 — the agent's default model** | M0 (launcher), M1, M2, M3 | Z6a (snapshot shape) | 1, 8 |
| **2 — the project knows where its code is** | S0, S1, S2, S3, S4, S5, S6, S7 | M3 (dialog's model radio) | 3, 5, 6, 7 |
| **3 — the agent works where the code lives** | R1, R2, R3 | S1 (project fields), Z2 | 5, 6, 13 |
| **4 — the task and the dialog** | T1, T2a, T2b, T3, T4a, T4b, T5, P1, P2, P3 | S0–S7, R3 | 4, 9, 10, 7, persistence |
| **5 — words and help** | V1, V2, V3 | everything above | 4, 9 |

Order inside a wave is free except where "needs" says otherwise. Waves 0 and 1 may run
interleaved. Wave 5 is last because it touches every string once.

## How a task is handed to an agent

One agent, one task, one branch from `develop` named after the task (`z1-evidence-base`).
The brief is the task's section of this file plus these lines, and nothing else — the agent
reads `CLAUDE.md` on its own:

```
Task <id> from docs/plans/phase-67.md. Read that section and the files it lists, nothing wider.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Add only the tests the task names. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag or helper the task does not name.
Never call a real model endpoint: every run goes to a loopback fake, zero spend.
Never run a CLI against the operator's own HOME or config; use a scratch HOME.
Every contract change is a fixture first (docs/contracts/<name>/), then the type that reads it, then the pin.
Finish by running .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do; your tool-call count.
```

The launcher cuts the section with
`awk '/^### <id> /{p=1;next} /^### |^## /{p=0} p' docs/plans/phase-67.md` and pastes it after
the lines. The brief names the ADR and says "do not open it": the section carries the decision.
Base: `git rev-parse --short develop` at dispatch, and the agent runs
`git merge-base --is-ancestor <sha> HEAD` first.

Limits, imposed from outside the prompt (phase 65/66, unchanged, re-measured 2026-09-18):

- **Haiku when the shape is written here; Sonnet when the agent has to choose it.** Haiku:
  every type, column, flag and test name spelled; at most five files in one layer; no hot
  handler, no `engine.rs`, no migration it has to design; RED test named. Ceilings: Haiku 60
  calls, Sonnet one layer 100, Sonnet two layers 150. One that reaches it hands off and is
  replaced fresh with a shorter brief, never resumed with a large context.
- **One gate, once.** `.githooks/pre-push` at the end. While working, only the binary being
  changed: `cargo nextest run --workspace -E 'binary(<name>)'`; `npx vitest run <file>`.
  The gate's "generated files are current" step is `git diff --quiet` on the generated
  files, so a regenerated `schema.gen.ts` that is not yet staged always reads as stale (Z3,
  2026-10-07): a task that regenerates stages the result (`git add`) before the gate.
- **At most two agents building at once**, `--build-jobs 4`, `--test-threads 4`;
  `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<task>`, removed when the branch merges.
  `renice` from the launcher's loop, never the prompt. No agent opens a GUI (no Tauri, no
  headed Playwright) while the user is at the machine.
- **Secrets:** any process that opens the secret store runs with
  `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`.
- **The pinned toolchain, not Homebrew's.** On this workstation Homebrew's `cargo` shadows
  rustup's and ignores `rust-toolchain.toml`, so clippy fails on lints CI never sees (seen on
  Z1, 2026-10-07). Every cargo command and the gate run with `PATH=$HOME/.cargo/bin:$PATH`;
  the brief says so and the agent confirms `rustc --version` prints the pinned version.
- **Split at the layer boundary; the regenerated schema is the hand-off.** A task that finds
  itself editing the other layer stops: that is a finding.
- **Docs are pasted, not merged.** One writer per wave for `agent-runners.md` and
  `quick-start.md` (V3); every other task's paragraph travels in its report.
- **Mechanical edits are the integrator's** (renames by script, pasted paragraphs, version
  bumps). V1 is the one exception: the table is the brief.
- **Review before merge** by the launching session against the task's *done when*, reading the
  diff. `git merge --no-ff` into `develop`; the gate once more on the merge; then push.

## The tasks

### Z1 — evidence diffs against the commit the checkout resolved

**Files:** `crates/tack-runner/src/git.rs` (`capture_evidence`: resolve `workspace.base_revision`
through the existing `resolve_revision` before the three `diff --cached … <base>` calls, and
use the sha; `base_commit` in the returned `GitEvidence` gains the resolved sha — add the field);
`crates/tack-runner/src/evidence.rs` (`base_commit` = the resolved sha when git evidence
exists, the requested name otherwise, and a new `base_revision_requested` field with the name);
`crates/tack-runner/src/git/tests.rs`; `docs/contracts/evidence-v1/schema.json` and
`attempt-evidence.example.json` (the one new optional field); `crates/tack-runner/tests/evidence_contract.rs`
(re-pin).

**Done when:** a real temp repo fetched by branch name into a `git init` checkout, with one
file written by the fake harness, yields `captured: true`, one `added` entry and a non-empty
patch; the same with a 40-char sha still passes; `base_commit` is 40 hex chars in both; the
example round-trips and its pin holds; `.githooks/pre-push` passes.

**RED:** `git/tests.rs::evidence_is_captured_when_the_base_is_a_branch_name` fails at the base
with `captured: false`.

**Model:** Sonnet.

### Z2 — a workspace whose evidence failed is kept, and the attempt says so

**Files:** `crates/tack-runner/src/engine.rs` (`run_claimed`, after `capture_attempt_evidence`
at `:699-720`: when the manifest says `captured: false`, move the workspace under the state
dir's `quarantine/<attempt>` through `WorkspaceManager` instead of `cleanup`, add
`workspace_kept_at` to `terminal_reason`, and emit one `attempt.evidence_failed` event with
the manifest's `reason`); `crates/tack-runner/src/workspace.rs` (one method
`WorkspaceManager::keep(&self, workspace) -> Result<PathBuf, WorkspaceError>` that renames the
directory into `quarantine/`, reusing the path validation `cleanup` has); `engine/tests.rs`.

**Done when:** in `engine/tests.rs` a fake provisioner whose `capture_evidence` returns
`Err(WorkspaceError::Git)` leaves the workspace directory present under `quarantine/`, the
terminal reason carries its path, one `attempt.evidence_failed` event reaches the fake protocol,
and the attempt's state is unchanged; a provisioner that captures still deletes the workspace;
`.githooks/pre-push` passes.

**RED:** `engine/tests.rs::a_failed_evidence_capture_keeps_the_workspace`.

**Model:** Sonnet.

### Z3 — `terminal_reason` is typed on the wire

**Files:** `crates/tack-api/src/handlers/executions.rs` (`:441`: `Option<TerminalReason>` where
`TerminalReason { code: Option<String>, message: Option<String>, artifact: Option<Value>,
artifacts: Option<Vec<Value>>, workspace_kept_at: Option<String>, #[serde(flatten)] additional:
BTreeMap<String, Value> }` with `ToSchema`, `additionalProperties: true`); run
`./scripts/regen-generated.sh` (`docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`);
`frontend/src/shared/execution/types.ts` (`TerminalReason` re-exported from the generated
schema; `ExecutionAttemptSummary.terminal_reason` typed).

**Done when:** `docs/openapi.json` has a `TerminalReason` schema with the five named fields;
the existing attempt-list handler test still passes with the user's own failed-attempt JSON
(`{"code":"harness_rejected","message":"…"}`) added as one more row; `npx tsc --noEmit` passes;
`.githooks/pre-push` passes.

**RED:** `crates/tack-api/src/handlers/attempt_lists.rs` test
`terminal_reason_round_trips_typed` (a new row in its existing table test).

**Model:** Haiku.

### Z4 — the attempt card leads with the outcome

**Files:** `frontend/src/shared/runWithAgent/attemptFormat.ts` and `attemptFormat.test.ts`;
`AttemptList.tsx` and `AttemptList.test.tsx`; `EventTimeline.tsx` and `EventTimeline.test.tsx`.

Rules, exactly:

1. A terminal attempt whose state is `failed`, `cancelled` or `lost` renders first a line
   **Why it stopped:** `terminal_reason.message` (fallback `code`, fallback "No reason was
   reported"). A `code` of `harness_rejected` is prefixed "Did not start — ".
2. `describeModelProvenance`'s `matched` label is produced only when
   `model_observation_source` is an observation (`observed`, `confirmed_from_output` — read the
   enum in `types.ts`); `requested_not_confirmed` renders "Requested <provider> / <model> (not
   confirmed by the run)", tone neutral.
3. `formatWallClock(null)` on a terminal attempt renders "Did not start"; on a live one the
   current text.
4. `succeeded` badge: green when `terminal_reason.artifacts` contains kind `patch` with
   `size_bytes > 0`; amber "Finished — no changes recorded" when the patch is 0 bytes; amber
   "Finished — changes could not be read" when `workspace_kept_at` is set, with the path.
5. `EventTimeline`: for `attempt.terminal`, render `payload.result` (string) as the body and
   the artifact names as a list; every other key behind a "Raw" toggle. Other kinds unchanged.

**Done when:** one table-test row per rule in `attemptFormat.test.ts`; `AttemptList.test.tsx`
renders the user's failed attempt (fixture in the test) with "Did not start — requested model
provider …" visible and no "Matched request"; `EventTimeline.test.tsx` shows the result text
and hides `modelUsage` until "Raw"; `.githooks/pre-push` passes.

**RED:** `attemptFormat.test.ts::a_rejected_attempt_leads_with_why_it_stopped`.

**Model:** Sonnet.

### Z5 — native controls follow the palette

**Files:** `frontend/src/index.css` only.

Each palette block that defines `--color-bg-base` (the six at `:94`, `:230`, `:276`, `:317`,
`:360`, `:396`, `:436`, `:483`) declares `color-scheme: light` or `dark` to match its
background; the `@media (prefers-color-scheme: dark)` block at `:611` declares `dark`.

**Done when:** `rg -c 'color-scheme:' frontend/src/index.css` ≥ 9; `npx vitest run
frontend/src/shared/ui/kit.test.tsx` passes; a screenshot of the Run dialog in the default dark
palette (launcher takes it, headless Playwright) shows the Agent select on a dark field.

**RED:** none (a stylesheet); the screenshot is the check.

**Model:** Haiku.

### Z6a — the harness reports its native provider and which providers reach it

**Files:** `crates/tack-runner/src/harness/local_process.rs` (`probe` at `:896`: add
`native_provider: String` and `providers: Vec<String>` — the descriptor's `native_provider`
plus every configured provider whose wire matches, read from `crate::provider::registry()` —
to the `HarnessCapability` it returns); `crates/tack-orch/src/execution/capabilities.rs`
(`HarnessCapability` gains both as `#[serde(default)]` fields);
`docs/contracts/runner-v1/capabilities.json` (one harness entry gains both keys) and the pin in
`crates/tack-orch/tests/runner_contract.rs`; `local_process/tests.rs`.

**Done when:** `tack runner doctor --json` (run by the launcher against a scratch HOME) prints
`native_provider: "anthropic"` under claude-code and `"openai"` under codex; the fixture pins;
`.githooks/pre-push` passes.

**RED:** `local_process/tests.rs::probe_reports_the_native_provider`.

**Model:** Sonnet.

### Z6b — the provider is derived, never typed

**Files:** `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` (`modelPart('provider')`:
when `modelMode === 'choose'`, the provider is the selected harness's `native_provider`, or the
gateway's wire name when `form.viaGateway` is set and the harness lists it in `providers`;
`customProvider` removed from the form); `RunFlow.tsx` (the Provider `Field` at `:148` removed;
one `RadioRow`-less line under the model select: "Through: Anthropic" / "Through: Vercel AI
Gateway" when both are listed, as a `Select`); `shared.ts` (`RunWithAgentFormValues` unchanged);
`RunWithAgentModal.test.tsx`; `shared/execution/types.ts` (the two Z6a fields on
`HarnessCapability`, from the regenerated schema).

**Done when:** the modal test's capability fixture gains `native_provider`; a run submitted with
"Other (type a model id)" sends `requested_model_provider: "anthropic"` without any provider
input existing in the DOM; with a gateway listed the select appears and switching it changes
the sent provider; `.githooks/pre-push` passes.

**RED:** `RunWithAgentModal.test.tsx::the_provider_comes_from_the_harness`.

**Model:** Sonnet.

### M0 — measure which model ids each harness accepts (launching session)

Not dispatched. The launching session runs, per installed harness, the cheapest real call the
CLI offers with a deliberately wrong model id and with each candidate id, and records in
`docs/plans/measurements/models-<harness>.md`: the command, the ids that ran, the ids
rejected, the version. Spend is bounded (one short prompt per id); the file carries the
commands. M3 reads it.

### M1 — the harness reports whether it needs a model

**Files:** `crates/tack-runner/src/harness/local_process.rs` (`probe`: `model_selection:
"optional" | "required"` from the descriptor's `ModelSelection`);
`crates/tack-orch/src/execution/capabilities.rs` (`HarnessCapability.model_selection:
Option<ModelSelectionReport>`, `#[serde(default)]`, enum `Optional | Required`);
`docs/contracts/runner-v1/capabilities.json` and its pin; `local_process/tests.rs`.

**Done when:** doctor prints `model_selection: optional` for claude-code and codex (read each
descriptor; report what it says, do not change it); the fixture pins; `.githooks/pre-push`
passes.

**RED:** `local_process/tests.rs::probe_reports_model_selection`.

**Model:** Sonnet.

### M2 — the scheduler accepts the agent's own model when the agent says so

**Files:** `crates/tack-orch/src/scheduler/select.rs` (`evaluate_candidate` `:151-160`:
`AutoSelect` is eligible when the matched harness reports `model_selection: Some(Optional)`;
otherwise `AutoSelectNotVerified` as today); `scheduler/types.rs` (the doc comment on
`AutoSelectNotVerified` says when it still applies); `scheduler/select/tests.rs` (or the
module's existing test block).

**Done when:** two rows in the existing eligibility table test: Auto on a harness reporting
`optional` schedules; Auto on one reporting nothing is `AutoSelectNotVerified`;
`crates/tack-cli/tests/e6_scheduler_e2e_test.rs` still passes; `.githooks/pre-push` passes.

**RED:** `select` tests: `auto_select_schedules_on_a_harness_that_declares_optional`.

**Model:** Sonnet.

### M3 — "The agent's default" is the model radio

**Files:** `frontend/src/shared/runWithAgent/shared.ts` (`gateHarnessModelSelection`: Auto is
allowed, not advisory, when the target harness reports `model_selection === 'optional'`, reason
"Uses the agent's own model"; the `pinned_auto`/`unresolved` branches apply only when it does
not); `RunFlow.tsx` (the three radios become two: "The agent's default (recommended)" and
"Specific model"; "Project default — …" becomes a line under the first when the project has
one; the model `Select` lists `docs/plans/measurements/models-<harness>.md`'s accepted ids from
a new `frontend/src/shared/runWithAgent/models.ts` table keyed by harness kind, then "Other…"
which reveals the model-id `Field`); `RunWithAgentModal.tsx` (`initialForm.modelMode` defaults
to `auto`; `project`/`choose` mapping); `shared.test.ts`; `RunWithAgentModal.test.tsx`.

**Done when:** with a capability fixture reporting `optional`, the dialog opens with Run enabled
and no model chosen, and submits `null/null`; with `required` it opens on "Specific model" with
the list; "Other…" shows the id field; `.githooks/pre-push` passes.

**RED:** `shared.test.ts::auto_is_allowed_when_the_harness_needs_no_model`.

**Model:** Sonnet.

### S0 — the migrations of this phase, together

**Files:** `crates/tack-db/src/migrations.rs` (one `MIGRATION_082` constant and its entry in
the list, after `081_pull_requests`); its existing migration test.

`082_automation`, exactly:

```sql
ALTER TABLE projects ADD COLUMN code_origin TEXT NOT NULL DEFAULT 'none';          -- 'folder' | 'new_folder' | 'url' | 'none'
ALTER TABLE projects ADD COLUMN repository TEXT;                                    -- absolute path, or URL when code_origin = 'url'
ALTER TABLE projects ADD COLUMN default_branch TEXT;                                -- e.g. 'main'
ALTER TABLE projects ADD COLUMN workspace_mode TEXT NOT NULL DEFAULT 'local_branch'; -- 'local_branch' | 'in_place' | 'clone'
ALTER TABLE projects ADD COLUMN push_after_run INTEGER NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN default_harness TEXT;                               -- 'claude-code' | 'codex' | …
ALTER TABLE projects ADD COLUMN default_profile_id TEXT REFERENCES agent_profiles(id);
ALTER TABLE projects ADD COLUMN on_finish_status TEXT;                              -- a workflow status id, or NULL = do not move
ALTER TABLE projects ADD COLUMN definition_of_done TEXT;
ALTER TABLE items ADD COLUMN run_settings TEXT;                                     -- JSON object of per-task overrides, or NULL
ALTER TABLE agent_profiles ADD COLUMN kind TEXT NOT NULL DEFAULT 'custom';          -- 'implementer' | 'reviewer' | 'researcher' | 'planner' | 'custom'
ALTER TABLE agent_profiles ADD COLUMN builtin INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_profiles ADD COLUMN summary TEXT;                                 -- the tooltip line
CREATE TABLE attempt_reviews (
  attempt_id TEXT PRIMARY KEY NOT NULL REFERENCES execution_attempts(id),
  verdict TEXT NOT NULL,                                                            -- 'accepted' | 'rejected'
  note TEXT,
  reviewed_at TEXT NOT NULL
);
```

**Done when:** the migration test applies 001→082 on a fresh database and on a copy seeded at
081; `PRAGMA table_info` shows every column above with its default; `.githooks/pre-push` passes.

**RED:** the migrations test's "latest is 082" assertion.

**Model:** Haiku.

### S1 — the project carries its automation settings

**Files:** `crates/tack-core/src/models.rs` (`Project` gains the nine S0 fields as typed
Options/enums: `CodeOrigin`, `WorkspaceMode` with `serde(rename_all = "snake_case")`;
`UpdateProject` gains them as `Option<Option<…>>`-style "set / clear / leave" fields, using the
crate's existing pattern if one exists, else `#[serde(default, with = "double_option")]`);
`crates/tack-db/src/` (the project repository's read/update of those columns);
`crates/tack-api/src/handlers/projects.rs` (PATCH accepts them; `code_origin = 'url'` requires
`repository` to parse as a URL, `'folder'`/`'new_folder'` require an absolute path; otherwise
400 naming the field); `./scripts/regen-generated.sh`; the handler's existing test file.

**Done when:** `PATCH /api/projects/{id}` with `{"repository": "/tmp/x", "code_origin":
"folder", "workspace_mode": "in_place"}` reads back on `GET`; `{"repository": null}` clears it;
a relative path is 400 with `details.field = "repository"`; `docs/openapi.json` carries the
fields; `.githooks/pre-push` passes.

**RED:** `projects.rs` handler test `automation_settings_round_trip_and_clear`.

**Model:** Sonnet.

### S2 — the local runner checks a folder, makes one, and says who is signed in

**Files:** `crates/tack-api/src/handlers/local_runner.rs` (three routes, mounted beside the
existing `/api/local-runner` ones in `crates/tack-api/src/router.rs`):

- `POST /api/local-runner/check-folder {path}` → `{exists, is_dir, is_git, branch,
  remote_url, dirty_files}`; runs `git` in the server process (it is the same machine as the
  embedded runner; refuse with 409 `local_runner_unavailable` when the embedded runner is not
  configured); the path must be absolute; nothing is written.
- `POST /api/local-runner/init-folder {path}` → creates the directory and runs `git init`;
  refuses an existing non-empty directory with 409.
- `GET /api/local-runner/harness-verification` → per harness kind, the `ended_at` of the latest
  `succeeded` attempt whose `actual_execution.harness_kind` matches, or `null`.

Tests in the handler file against a temp directory and a seeded attempt.

**Done when:** the three routes answer as specified in the handler tests; `check-folder` on a
file (not a dir) returns `is_dir: false`; `init-folder` twice is 409; `harness-verification`
returns the seeded attempt's time; `.githooks/pre-push` passes.

**RED:** `local_runner.rs` test `check_folder_reports_git_facts`.

**Model:** Sonnet.

### S3 — project creation asks where the code is; Settings gets Automation

**Files:** `frontend/src/features/projects/CreateProjectModal.tsx` (one step "Does this project
have code?" with the three radios of ADR 0074 decision 2; "Browse…" calls
`@tauri-apps/plugin-dialog`'s `open({directory: true})` when `window.__TAURI_INTERNALS__`
exists, else the path is typed; the chosen path is checked through S2 and the result shown
under the field as a sentence); `frontend/src/features/settings/ProjectSettings.tsx` (the
`agents` tab becomes `automation`, label "Automation");
`frontend/src/features/settings/panels/AutomationPanel.tsx` (new; replaces `AgentsPanel.tsx`,
which is deleted) with four sections — *Where the code is* (origin, folder/URL, branch, check
result), *How the agent works on it* (mode radios worded as in the ADR, push switch, disabled
with its reason when the mode is `in_place` or the runner reports no `push_configured`), *When
a run finishes* (on_finish_status select from the workflow's statuses, "Do not move" first),
*Agent* (harness pills from the live `GET /runners` snapshot, model radio as M3, default
profile pills from `GET /agent-profiles`); `panels.test.tsx`; `package.json`
(`@tauri-apps/plugin-dialog`).

**Done when:** creating a project with "existing folder" and a temp git path stores
`code_origin: folder`, `workspace_mode: local_branch`, `default_branch` from the check; "new
folder" calls `init-folder` then stores `new_folder`; "no code" stores `none` and the Automation
tab says so with a link to change it; every field saves on change (no Save button) with a
toast on failure; `.githooks/pre-push` passes.

**RED:** `panels.test.tsx::automation_panel_saves_each_field_on_change`.

**Model:** Sonnet.

### S4 — the Agents page is about this computer only

**Files:** `frontend/src/features/agents/AgentsPage.tsx` (three steps: `ExecutionToggle`,
`HarnessStep`, `ProviderStep`; the project picker, `ModelDefaultStep` and the old `TestRunStep`
removed — `ModelDefaultStep.tsx` deleted); `steps/HarnessStep.tsx` (each row's badge: "Installed
v… · Signed in <date>" from S2's `harness-verification`, "Installed · Not signed in — run
`claude` once in a terminal" otherwise, "Not installed" with the npm line);
`steps/ProviderStep.tsx` (one paragraph as ADR 0074 decision 1; the gateway key panel beneath,
unchanged); `steps/TestRunStep.tsx` (one button, no fields; calls S5's route; shows the
`ExecutionTimeline` for the returned request); `AgentsPage.test.tsx`; `agentsProjectPreference.ts`
deleted.

**Done when:** the page renders no input except the gateway key; the test button exists
without a project selected; the signed-in date comes from the S2 route in the test's mock;
`.githooks/pre-push` passes.

**RED:** `AgentsPage.test.tsx::the_test_run_needs_no_fields`.

**Model:** Sonnet.

### S5 — a test run with nothing typed

**Files:** `crates/tack-api/src/handlers/local_runner.rs` (`POST /api/local-runner/test-run
{harness_kind}` → creates, on demand, one archived project named `Agent tests` in the default
workspace, one item "Agent test <timestamp>" in it, and an execution request targeting this
machine's runner with the harness, `requested_model_provider/id: null`, the Implementer
profile's id, `repository_snapshot: {kind: "scratch", remote: "", base_revision: "",
subdirectory: null}`, `permission_policy: {tools: [], network: false}`, `timeout_seconds: 300`;
returns the request id); `crates/tack-runner/src/engine.rs` (`kind == "scratch"`: provision an
empty temp directory with `git init` in place of the fetch, through a
`WorktreeProvisioner::provision_scratch` default method implemented in `git.rs`); handler and
engine tests.

**Done when:** the route returns a request id, the project is archived (hidden from
`GET /projects`'s default listing), the fake runner completes the attempt from a scratch
directory with no remote, and a second call reuses the project; `.githooks/pre-push` passes.

**RED:** `local_runner.rs` test `test_run_creates_a_hidden_project_and_a_request`.

**Model:** Sonnet.

### S6 — four built-in profiles, seeded

**Files:** `crates/tack-db/src/` (a `seed_builtin_profiles` run after migrations, idempotent by
`kind`, that inserts the four rows below when absent and never overwrites a row the user
edited — `builtin = 1` rows are updated only in `summary`); `crates/tack-api/src/handlers/`
(the agent-profiles handler: `PATCH /api/agent-profiles/{id}` with `name`, `instructions`,
`tool_policy`, `limits`, `summary`; `DELETE` refuses `builtin = 1` with 409); the handler's
test file; `./scripts/regen-generated.sh`.

The rows (`tool_policy.tools` uses the names in `docs/plans/measurements/tools-claude-code.md`
and `tools-codex.md`; for a harness with no list the entry is `"*"`):

| kind | name | summary | tools (claude-code) | instructions |
|---|---|---|---|---|
| `implementer` | Implementer | Reads, edits and runs commands to make the change | Read, Edit, Write, Bash, Agent, Grep, Glob | "Complete the requested change. Keep the diff focused. Run the project's tests when they exist. Summarize what changed and what you could not do." |
| `reviewer` | Reviewer | Reads and reports; changes nothing | Read, Grep, Glob, WebFetch | "Review the task against its acceptance criteria. Do not modify files. Report findings, each with file and line, most severe first." |
| `researcher` | Researcher | Reads and searches; writes a report | Read, Grep, Glob, WebFetch, WebSearch | "Investigate the question. Do not modify project files. Write your findings as the result, with sources." |
| `planner` | Planner | Plans and designs; proposes subtasks you can accept | Read, Grep, Glob, WebFetch | "Plan the work. Do not modify project files. Write `tack-plan.json` at the workspace root: `{\"v\":\"1\",\"subtasks\":[{\"title\",\"description\",\"acceptance\":[\"…\"],\"depends_on\":[index…],\"estimate\":\"S|M|L\"}]}`. Explain the plan as the result." |

**Done when:** a fresh database lists the four with `builtin = 1`; a second start inserts
nothing; `PATCH` on a built-in changes `instructions` and keeps `builtin`; `DELETE` is 409;
`.githooks/pre-push` passes.

**RED:** the profiles handler test `builtin_profiles_exist_on_a_fresh_database`.

**Model:** Sonnet.

### S7 — profiles are edited in a form, from Automation

**Files:** `frontend/src/features/agents/runnerFleet/AgentProfilesPanel.tsx` (the JSON textareas
become: name, summary, instructions, a tool checklist per harness from the M3-style static
table plus free text, approvals default; built-ins show a "Built-in" chip and no Delete);
`AgentProfilesPanel.test.tsx`; `frontend/src/features/settings/panels/AutomationPanel.tsx`
(its *Agent* section links "Manage profiles" to the panel, now mounted at
`/agents#profiles` inside the Advanced section as today — no new route).

**Done when:** creating a profile through the form sends `tool_policy: {"tools": [...]}` and no
raw JSON is typed anywhere; a built-in shows the chip and no Delete; `.githooks/pre-push` passes.

**RED:** `AgentProfilesPanel.test.tsx::the_form_builds_the_tool_policy`.

**Model:** Sonnet.

### R1 — `local_branch`: a worktree of the user's repo, the branch stays

**Files:** `crates/tack-runner/src/client.rs` (`RepositorySpec` gains `workspace_mode:
WorkspaceMode` and `repository_path: Option<PathBuf>`, read from `repository_snapshot.additional`
— `clone` when absent); `crates/tack-runner/src/git.rs` (`provision`: for `local_branch`, `git
-C <repository_path> worktree add --detach <workspace.path> <resolved base>` then `checkout -b
tack/<item short id>-a<n>`; `capture_evidence` unchanged (it runs in the worktree);
`publish_branch`: commit leftover changes, run `git worktree remove` on cleanup, keep the
branch; push only when the request asks and `[git] push_branches` is on, else event
`attempt.push_skipped {reason}`); `crates/tack-runner/src/engine.rs` (reads the mode; the
"Decided" row on push); `git/tests.rs`; `engine/tests.rs`.

**Done when:** against a temp repo with a commit, a `local_branch` attempt whose fake harness
writes one file ends with the branch `tack/…-a1` present in the temp repo containing that file
as a commit, the worktree directory gone, the temp repo's own checkout untouched (its
`git status --porcelain` empty), `evidence.json` `captured: true` with one file, and `branch`
in the manifest; with `push_after_run` asked and push not configured, one
`attempt.push_skipped` event; `.githooks/pre-push` passes.

**RED:** `git/tests.rs::local_branch_leaves_the_branch_in_the_users_repo`.

**Model:** Sonnet.

### R2 — `in_place`: the user's folder, read-only evidence, nothing deleted

**Files:** `crates/tack-runner/src/git.rs` (for `in_place`: `provision` only validates the path
and takes a lock file at `<state dir>/locks/<sha256(path)>` (409-like `WorkspaceError::Busy`,
new variant, when held); `capture_evidence` uses a temporary index —
`GIT_INDEX_FILE=<scratch>/index git read-tree HEAD && git add -A` then `diff --cached` against
the resolved base — and never touches the user's index; a folder without `.git` yields a
snapshot diff: `files.json` from a listing `{path, size, mtime, sha256}` taken before spawn and
after exit, `patch` absent, `captured: true`); `crates/tack-runner/src/workspace.rs`
(`cleanup` is a no-op for `in_place` — it releases the lock only); `engine.rs` (the snapshot
before spawn); tests.

**Done when:** an `in_place` attempt on a temp git repo with a dirty index leaves the index
byte-identical (`git diff --cached` before == after), the working tree holds the harness's file,
evidence lists it, and nothing was deleted; on a temp folder without git, `files.json` lists the
added file and `patch` is absent; a second attempt while the first holds the lock fails before
spawn with `Busy`; `.githooks/pre-push` passes.

**RED:** `git/tests.rs::in_place_never_touches_the_users_index`.

**Model:** Sonnet.

### R3 — the server fills the repository from the project

**Files:** `crates/tack-api/src/handlers/executions.rs` (`create_execution`: when
`repository_snapshot` is absent or `{}`, build it from the item's project — `kind: "git"`,
`remote`: the project's `repository`, `base_revision`: `default_branch`, `additional:
{workspace_mode, repository_path}`; a project with `code_origin = 'none'` is 400
`project_has_no_code`; `CreateExecution.repository_snapshot` becomes `Option<Value>`); the
handler tests; `./scripts/regen-generated.sh`; `docs/MCP.md` and the CLI keep sending an
explicit snapshot (unchanged; one line in each saying it is now optional — pasted by the
integrator).

**Done when:** a request without `repository_snapshot` against a project with a folder is
created with the filled snapshot visible on `GET /executions/{id}`; against a `none` project
it is 400; an explicit snapshot still wins; `.githooks/pre-push` passes.

**RED:** `executions.rs` test `the_repository_comes_from_the_project_when_omitted`.

**Model:** Sonnet.

### T1 — the task's form: description, acceptance criteria, and "For the agent"

**Files:** `frontend/src/features/item-detail/tabs/DetailsTab.tsx` (under Description: an
*Acceptance criteria* checklist — one line per `manual` criterion of the brief, add/remove/
reorder, saved through `api.briefs.put` on blur; then a collapsed *For the agent* block with
the non-`manual` criteria editor, constraints and risk — the editors moved from `BriefTab.tsx`);
`BriefTab.tsx` deleted; `ItemDetailDrawer.tsx` (the `brief` tab removed from `BASE_TABS`;
`?tab=brief` maps to `details`); `DetailsTab.test.tsx`; `BriefTab.test.tsx` cases moved.

**Done when:** typing two sentences in the checklist and blurring produces a `PUT /briefs`
with two `manual` criteria whose `text` is the sentence and `title` its first 60 chars; the
"For the agent" block is collapsed by default and shows a count ("2 checks · 1 limit") when
non-empty; every former BriefTab test passes in its new home; `.githooks/pre-push` passes.

**RED:** `DetailsTab.test.tsx::acceptance_sentences_save_as_manual_criteria`.

**Model:** Sonnet.

### T2a — what the agent will read, on a route

**Files:** `crates/tack-api/src/handlers/executions.rs` (`GET /api/items/{id}/agent-context`
→ `{text}`: `compose_instructions` with the project's default profile's instructions (or the
Implementer's) and, new, the project's `definition_of_done` appended as `## Definition of
done` when set — the same function `create_execution` uses); router entry; handler test;
`./scripts/regen-generated.sh`.

**Done when:** the route returns the composed text for a seeded item with a description and
two criteria, containing the DoD section when the project has one and not otherwise;
`.githooks/pre-push` passes.

**RED:** `executions.rs` test `agent_context_includes_the_projects_definition_of_done`.

**Model:** Haiku.

### T2b — the dialog is a pre-flight

**Files:** `frontend/src/shared/runWithAgent/RunFlow.tsx` (rewritten to ADR 0074's shape: one
summary line — agent · model · "on a new branch of ~/…" / "in ~/…" — each word a link to
Automation; a collapsed *What the agent will read* fetching T2a; profile pills with the
summary as `title`; two switches, "Ask before each action" (disabled with reason when
`decisions` is not attested) and "Allow network access"; *Advanced for this run* collapsed:
model, branch, tools, timeout, verify, push, PR rows exactly as today; the blocking state is
one sentence with one link in place of Run); `RunWithAgentModal.tsx` (reads the project's
settings for the summary; the repository fields are gone from `initialForm` — R3 fills them —
unless Advanced overrides branch); `Prerequisite.tsx` (unchanged); tests.

**Done when:** with a project that has a folder, a default profile and an `optional` harness,
the dialog opens with Run enabled and nothing to fill; the summary names the folder and mode;
"What the agent will read" shows T2a's text; a project with `code_origin: none` shows "Choose
where this project's code is →" and no Run; `.githooks/pre-push` passes.

**RED:** `RunWithAgentModal.test.tsx::a_configured_project_opens_ready_to_run`.

**Model:** Sonnet.

### T3 — per-task changes are remembered

**Files:** `crates/tack-core/src/models.rs` and `crates/tack-db/src/` (`items.run_settings`
as `Option<Value>` on `Item`/`UpdateItem`); `crates/tack-api/src/handlers/items.rs` (PATCH
accepts it; `null` clears); `./scripts/regen-generated.sh`; `frontend/src/shared/runWithAgent/
RunWithAgentModal.tsx` (Advanced values are read from `item.run_settings` on open and written
on Run — only the keys that differ from the project's; the idempotency key is never stored);
tests on both sides.

**Done when:** running with a changed timeout and re-opening the dialog shows the timeout; the
stored JSON contains only `timeout_seconds`; a project-level change is reflected where the task
did not override; `.githooks/pre-push` passes.

**RED:** `items.rs` test `run_settings_round_trip_and_clear`; `RunWithAgentModal.test.tsx::
advanced_values_are_remembered_on_the_task`.

**Model:** Sonnet (two layers, split if the first exceeds 100 calls: `T3a` api, `T3b` frontend).

### T4a — the verdict route and the "needs review" flag

**Files:** `crates/tack-api/src/handlers/attempt_lists.rs` (`POST /api/executions/{id}/
attempts/{n}/review {verdict, note}` → writes `attempt_reviews`, 409 on a second; the attempt
DTO gains `review: Option<{verdict, note, reviewed_at}>`); `crates/tack-api/src/handlers/items.rs`
(`Item` DTO gains `needs_review: bool` — the latest terminal attempt of the latest request has
`terminal_reason.artifacts` with a `patch` of `size_bytes > 0` or `workspace_kept_at`, and no
review; the list endpoint computes it in one query); `crates/tack-api/src/handlers/executions.rs`
(on a terminal completion with such evidence and a project `on_finish_status`, move the item
through `update_item_atomically`, as `status_map_policy` does); tests; regen.

**Done when:** a seeded succeeded attempt with a patch makes `needs_review: true` on the item;
posting `accepted` makes it false; a second post is 409; with `on_finish_status` set the item's
status changes on completion; `.githooks/pre-push` passes.

**RED:** `attempt_lists.rs` test `a_reviewed_attempt_clears_needs_review`.

**Model:** Sonnet.

### T4b — the flag on the board and the verdict on the card

**Files:** `frontend/src/features/board/Board.tsx` (a chip "Needs your review" on a card whose
item has `needs_review`, and the existing run-state chip reworded: Running · Asking you ·
Finished — review · Failed); `frontend/src/features/list/` and `table/` (the same chip, one
column); `frontend/src/shared/runWithAgent/AttemptList.tsx` (Accept / Reject with a note,
calling T4a; the card's heading "Finished — needs your review" until a verdict exists); tests.

**Done when:** a board fixture with `needs_review: true` shows the chip; Accept posts the
verdict and the chip disappears on refetch; `.githooks/pre-push` passes.

**RED:** `Board.test.tsx::a_card_shows_needs_your_review`.

**Model:** Sonnet.

### T5 — the tools to see what it did

**Files:** `frontend/src/shared/runWithAgent/AttemptList.tsx` (a row of actions on a terminal
attempt: **Open diff** — a modal rendering the `patch` artifact's content as a unified diff
with per-file headings, fetched from the existing artifact content route; **Open folder** —
`@tauri-apps/plugin-shell` `open(path)` on the desktop, "Copy path" on the web; **Copy branch**
when `evidence.branch` is set; **Result** — the T2-style result text; **Log** — the existing
log artifact); `DiffView.tsx` (new, no library: split on `diff --git`, colour `+`/`-` lines
with tokens); tests.

**Done when:** a fixture attempt with a patch artifact shows four actions; Open diff renders two
files with added lines coloured; without a patch only Result and Log show; `.githooks/pre-push`
passes.

**RED:** `AttemptList.test.tsx::a_terminal_attempt_offers_the_diff_and_the_folder`.

**Model:** Sonnet.

### P1 — `plan-v1`: the Planner's proposal is an artifact

**Files:** `docs/contracts/plan-v1/schema.json`, `README.md`, `plan.example.json` (the S6
shape); `crates/tack-runner/src/evidence.rs` (after capture: if `<workspace>/tack-plan.json`
exists and parses against the type, stage it as kind `plan`, media type
`application/vnd.tack.plan+json`; it is added to the evidence exclude list so it never enters
the patch); `crates/tack-runner/tests/plan_contract.rs` (pin); `evidence/tests.rs`.

**Done when:** a fake harness that writes a valid `tack-plan.json` yields a `plan` artifact and a
patch that does not contain it; an invalid file yields no artifact and one
`attempt.plan_invalid` event with the parse error; the example pins; `.githooks/pre-push` passes.

**RED:** `evidence/tests.rs::a_plan_file_is_staged_and_excluded_from_the_patch`.

**Model:** Sonnet.

### P2 — subtasks from a plan, on a route

**Files:** `crates/tack-api/src/handlers/items.rs` (`POST /api/items/{id}/subtasks-from-plan
{artifact_id, subtasks: [{title, description, acceptance: [..]}]}` → creates each as a child
(`parent_id`) with a brief of `manual` criteria, in the given order, returning the ids; the
body is what the person edited, the artifact id only records provenance in the item's
`source`); test; regen.

**Done when:** posting two subtasks creates two children with briefs; the parent's children
list shows them in order; `.githooks/pre-push` passes.

**RED:** `items.rs` test `subtasks_from_plan_create_children_with_criteria`.

**Model:** Haiku.

### P3 — the proposal panel

**Files:** `frontend/src/shared/runWithAgent/PlanPanel.tsx` (new; on an attempt with a `plan`
artifact: the list of proposed subtasks, each with a checkbox, editable title/description/
criteria, "Create selected" calling P2, then links to the created items); `AttemptList.tsx`
(mounts it); tests.

**Done when:** a fixture attempt with a plan artifact shows three proposals; unticking one and
creating posts two; the panel then shows "Created 2 subtasks" with links; `.githooks/pre-push`
passes.

**RED:** `PlanPanel.test.tsx::only_selected_subtasks_are_created`.

**Model:** Sonnet.

### V1 — Tack's words

**Files:** every `frontend/src/**/*.tsx` string the table names; `frontend/src/shared/
runWithAgent/shared.ts` (`HARNESS_KINDS` labels unchanged; a `AGENT_WORD = 'Agent'` is not
needed — literal strings). The table is the whole brief:

| Was (user-visible) | Is |
|---|---|
| Harness | Agent |
| Agent profile | Profile |
| Runner / a runner is connected | This computer / Agents are on |
| Fleet, enroll, capacity | (Advanced only; unchanged) |
| Remote, Base revision, Repository remote | Folder (or URL under Advanced), Branch |
| Approvals: Automatic / Ask me | Ask before each action (switch) |
| Decision inbox / Decisions | Questions from the agent |
| Terminal reason | Why it stopped |
| Execution request / attempt | Run / Attempt #n |
| Brief | (gone; "Acceptance criteria", "For the agent") |
| Model/token cost, Runner time | Cost, Time |
| Run-state chip labels (`STATE_LABEL` in `shared.ts`, shown by `RunWithAgentButton.tsx`): Queued / Running / Waiting for decision / Succeeded / Failed | Queued · Running · Asking you · Finished · Failed (moved here from T4b, 2026-10-08) |

**Done when:** `rg -n 'Harness|harness|Agent profile|Base revision|Remote|Approvals|Decision inbox|Terminal reason' frontend/src --glob '!*.test.*' --glob '!*.gen.ts'`
matches only wire-name usages (`harness_kind`, `harnessKind`, code comments) and no JSX text;
every test updated; `.githooks/pre-push` passes.

**RED:** none; the `rg` is the check.

**Model:** Haiku.

### V2 — `(?)` on every field, and a help button

**Files:** `frontend/src/shared/ui/Field.tsx` (`FieldShell` gains `help?: {text: string; href?:
string}` rendering a `(?)` with a native `title` and, on click, a small popover with the text
and "More" linking to the book); `frontend/src/shared/ui/HelpButton.tsx` (new; in the top bar,
opens the book page mapped from the current route in `shared/help/routes.ts`: `/agents` →
`user-guide/agent-runners.html#agents-on-this-computer`, settings automation → `…#automation`,
the dialog → `…#the-run-with-agent-dialog`, item details → `user-guide/items.html`);
`shared/help/texts.ts` (one sentence per field of the Automation panel, the dialog, the
task form and the Agents page — the texts are in this task's section: see below); the
components that pass `help`; tests.

Texts (verbatim, the brief carries them):

- Folder — "The folder on this computer where the code lives. The agent works here or on a branch of it."
- How the agent works — "On a new branch: a separate copy, your folder stays as it is. In the folder: directly on your files."
- Push the branch — "After a run, push the branch to your remote with your own git credentials. Off keeps everything local."
- Model — "The agent's default is what it uses when you run it yourself. Pick a specific one only if you need to."
- Profile — "What the agent is allowed to do and how it is told to work. Implementer changes code; Reviewer and Researcher only read; Planner proposes subtasks."
- Ask before each action — "The agent pauses before every action and waits for your answer under Questions from the agent."
- Allow network access — "Lets the agent fetch pages and search. Off is safer; some tasks need it."
- Acceptance criteria — "One sentence per thing that must be true when this is done. The agent reads them; a person checks them."
- For the agent — "Checks a machine can run, limits on what it may touch, and the risk you see. Optional."

**Done when:** every listed field renders a `(?)` whose `title` is the text; the help button
opens the mapped URL in a new tab (the book's base URL from the existing docs link in the
footer); `.githooks/pre-push` passes.

**RED:** `kit.test.tsx::a_field_with_help_renders_the_hint`.

**Model:** Sonnet.

### V3 — the book describes the new flow (one writer)

**Files:** `docs/book/src/user-guide/quick-start.md` (the UI walkthrough: create a project
with a folder, run, review); `docs/book/src/user-guide/agent-runners.md` (sections "The Run
with agent dialog", "Running an item with an agent" intro, a new "Where the agent works" with
the three modes and the four profiles, "Choosing a model and a provider" rewritten around the
agent's default; the CLI example keeps its sha and gains the `workspace_mode` key); the
anchors V2 links to exist; screenshots re-taken by the launcher after the wave merges
(`docs/screenshots/run-with-agent.png`, `agents-page.png`, new `automation.png`).

**Done when:** `mdbook build` passes the link check CI already runs (`813f28c`); every anchor in
`shared/help/routes.ts` resolves; no sentence states a number not measured in this phase;
`.githooks/pre-push` passes.

**RED:** none; the link check is the gate.

**Model:** Sonnet.

### E1 — the e2e specs follow the new screens (added 2026-10-08)

**Files:** `frontend/e2e/run-with-agent.spec.ts`, `a11y.spec.ts`, `scheduler-e2e.spec.ts`,
`tutorial-assets.spec.ts`, `execution-attempt-detail.spec.ts`, `screenshots.spec.ts` and any
other spec under `frontend/e2e/` that still drives the old dialog (a `combobox` named "Agent
profile", a "Harness" select, "Repository remote"/"Base revision" fields, the "Choose…" radio,
a "Succeeded" chip, a "Decisions" heading, the Brief tab); `frontend/e2e/helpers.ts` only for
a helper those specs share. No `src/` file: a behaviour the spec cannot reach any more is a
finding, not a UI change.

**Done when:** `npx playwright test --project=chromium --workers=1` passes headless on the
default suite (the config's excluded specs stay excluded; `agent-assets.spec.ts`,
`recovery-demo.spec.ts` and `screenshots.spec.ts` are not run); every spec drives the screens as
V1 names them; `.githooks/pre-push` passes.

**RED:** the suite as it is at `aceb0b6`; the report names which specs failed before.

**Model:** Sonnet.

## Parked — not scheduled, and why

| Item | Why |
|---|---|
| Remote runners choosing a folder on their own machine | `clone` covers them; a per-runner folder map is a fleet feature, no user asked |
| A pull request per run from the dialog | ADR 0071: opened automatically for a pushed branch with a linked issue; unchanged |
| A live model catalog from a gateway | M0's measured lists suffice for four harnesses; the gateway catalog count stays informational |
| Profiles per project | ADR 0072 deferred it to a project-type system; `default_profile_id` is the project's choice among global profiles |
| Snapshot evidence for a non-git folder with a patch | files only; a patch needs a baseline copy, which doubles the folder |
| docket and opencode in the dialog's model lists | M0 measures them too; the lists ship when measured, same table |

## Questions for the maintainer

None open on 2026-10-07. The two that may come up while building:

1. R2's lock lives in the runner state dir, not in the user's folder. Writing nothing into
   the user's folder is the rule; say so if a visible marker is wanted instead.
2. S5's hidden project is archived, not deleted; a "Clear agent tests" button is one line if
   the archive list grows.

## Status

Proposed 2026-10-07; the maintainer said "start the phase" the same day.

- **Wave 0** (Z1–Z6b) merged 2026-10-07, gate green. **Wave 1** (M0 measured, M1–M3) merged
  2026-10-08. **Wave 2**: S0, S1, S2, S3, S4 merged 2026-10-08; S5, S6, S7 in progress.
- M1's finding: codex, opencode and docket declare `ModelSelection::Explicit`, so they report
  `required`; only claude-code reports `optional`. The fixture's codex entry says `required`.
- M0's finding: codex 0.149.1 on a ChatGPT account refused every model id, including its own
  configured default; it ships with no list ("Other…" only). opencode's list depends on the
  configured providers and is not pinned.

Follow-ups found while executing, not yet scheduled (small, one task each when a wave has room):

| Found by | Follow-up |
|---|---|
| S3 | The Automation panel's "Specific model" radio is shown disabled ("not available yet"); deleting `AgentsPanel` removed the only editor of `default_model`. A task writes `default_model` from that radio with M3's list. |
| S3 | `ApiError` carries no `details.field`, so a 400 on an invalid folder or URL is a toast, not a sentence under the field (decision 8 wants the field). Extend the client's error type. |
| S3 | `schema.gen.ts` types the check-folder response as `unknown` and `AgentProfileSummary` has no `summary`: the S2/S6 handlers need `ToSchema` response types. |
| Z2 | When `WorkspaceManager::keep` itself fails, the workspace is still deleted by the later cleanup; log it at warn. |
| S4 | The agents-page e2e asserts only that "Run test" is enabled; the click flow is asserted once S5's route exists. |
| T1 | The brief's `definition_of_done` lost its only editor when `BriefTab` went; ADR 0074 moves it to the project. The Automation panel needs a "Definition of done" textarea writing `projects.definition_of_done` (T2a appends it to what the agent reads). |
| S5 | "This machine's runner" is found as the newest active `local-%` runner; a runner-id accessor on `LocalRunnerControl` would make it exact. |
| T3a/T3b | `run_settings` is typed `Object` in OpenAPI, which generates `Record<string, never>`; the dialog casts. A `schema(value_type = Value)` or a named schema fixes the generated type. |
| T3b | `agent_profile_id` is remembered whenever the auto-selected profile differs from the project's default, so a project without a default stores it needlessly; store it only when the person picked a pill. |
| T5 | "Open folder" shows only when `workspace_kept_at` is set; a `local_branch` or `in_place` run whose work is in the project's own folder gets no folder action. The card should fall back to the project's `repository` (the item's project is one fetch away). |
| T4b | The kit has one attention tone, so "Needs your review" and "Asking you" share a colour and differ by text only. |
| P2 | `ItemSource` is a closed enum (Manual, Github, Linear, …) so subtasks created from a plan are `Manual` and the artifact id is not recorded; a `Plan { artifact_id }` variant (model + column + migration) would keep the provenance. |
| V2 | `Select` has no `help` prop, so the Model `(?)` in the dialog is positioned by a fixed offset; the popover closes only by clicking `(?)` again (no Escape, no outside click); the help button may sit under the modal overlay while the dialog is open. Three small kit changes. |
| V2 | The app has no documentation link of its own; the book base URL is a constant in `shared/help/routes.ts` (`https://yielab.github.io/tack/`, matching the README). |
