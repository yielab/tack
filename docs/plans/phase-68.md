# Plan: Phase 68 — what Phase 67 left on the table

**Status: opened 2026-10-08 by the maintainer ("continue the roadmap"); every task merged and
the phase closed the same day — see Status at the end.**
Serves ADR 0074 (accepted, executed): every task here finishes a decision that ADR already
takes. No new ADR: nothing below is a decision a reader could disagree with, only work the
phase-67 agents found and did not have room for. Written against `develop` at `7470450`; every
locator was read on that commit. Re-run `rg -n` before trusting a line number.

What this phase builds, in one paragraph. Phase 67 ended with fourteen follow-ups in its
Status table, each found by the agent that built the task next to it. Three are honesty on
the wire (a response typed `unknown`, a settings object typed as "no keys", a runner found by
a `LIKE`); one is a workspace that can still be deleted after the runner decided to keep it;
one is provenance (a subtask created from a plan forgets the plan); four are the Automation
panel and the dialog finishing what ADR 0074 decisions 4, 6 and 8 asked for (a specific model,
the definition of done, a sentence under the field, remembering only what the person picked);
one is the attempt card finding the folder for a run that worked in the user's own repo; three
are the kit (`Select` has no `(?)`, the popover does not close, one attention tone); one is an
e2e assertion that waited for a route that now exists. The fourteenth (the app's only docs
link is a constant) is not a task: the constant *is* the decision, and this plan says so.

## What was verified before planning (2026-10-08, `7470450`)

| Claim | Read at | Holds? |
|---|---|---|
| `WorkspaceManager::keep` can fail (`UnsafePath`, `UnsafeRoot`, `Io`, destination exists) and the engine only reads `Ok` | `crates/tack-runner/src/workspace.rs:328-343`; `engine.rs:757-769` | yes; on `Err` nothing is logged and the later cleanup deletes the folder |
| `items.run_settings` is `Option<serde_json::Value>` with `schema(value_type = Option<Object>)`, which generates `Record<string, never>`; the dialog casts both ways | `crates/tack-core/src/models.rs:187-190`, `:605-608`; `frontend/src/shared/api/schema.gen.ts:3713`, `:4278`; `RunWithAgentModal.tsx:165`, `:311` | yes |
| `AgentProfileSummary.summary` is already typed | `schema.gen.ts:2953-2962` | **already done** — not a task |
| The check-folder client declares its own `FolderCheck` type; the route is not in the generated paths | `frontend/src/shared/api/projects.ts:31-35`; `rg 'check-folder' schema.gen.ts` → nothing | yes; the handler's response needs `ToSchema` and the route needs to be in `openapi.rs` |
| "This machine's runner" is the newest active `local-%` runner | `crates/tack-api/src/handlers/local_runner.rs:574-581` | yes; `EmbeddedRunnerControl` (`crates/tack-cli/src/local_runner.rs:315`) implements `LocalRunnerControl` and knows its own enrolment |
| Subtasks from a plan are created `ItemSource::Manual`; the request carries `artifact_id` and the handler drops it | `crates/tack-api/src/handlers/items.rs:549-553`, `:628` | yes |
| `ItemSource` is a closed enum stored as text; `is_trusted` is true only for `Manual` | `crates/tack-core/src/models.rs:209-290` | yes |
| The Automation panel renders "Specific model" disabled with "not available yet"; `UpdateProject` already accepts `default_model: Option<ProjectModelDefault>` and `definition_of_done` | `AutomationPanel.tsx:232-240`; `models.rs:521`, `:546` | yes; nothing in the panel writes either |
| The server sends `details.field` on the three validation errors the panel can hit; the client drops `details` | `handlers/projects.rs:123`; `local_runner.rs:384`, `:487`; `client.ts:95-105`, `:130-150` | yes |
| The dialog remembers `agent_profile_id` whenever the form's profile differs from the project's default, including the auto-selected one | `RunWithAgentModal.tsx:117-123`, `:291` | yes |
| "Open folder" needs both a patch and `workspace_kept_at`; a `local_branch`/`in_place` run sets neither kept path | `AttemptList.tsx:71`, `:96-102`; `engine.rs:748-756` | yes |
| Every attention badge is `tone="warning"`: Needs your review, Pending (a question), Unverified, Cancellation pending | `Badge.tsx:4-10`; `Board.tsx:96`; `DecisionInbox.tsx:124`; `RunFlow.tsx:359` | yes |
| `Select` has no `help` prop although `FieldShell` takes one; the dialog positions the Model `(?)` absolutely; `HelpHint` closes only on its own click; `HelpButton` is in the top bar under the modal overlay | `Select.tsx:16-21`; `Field.tsx:32-66`, `:71`; `RunFlow.tsx:233`; `HelpButton.tsx` | yes |
| The agents-page spec stops at "Run test is enabled"; `POST /api/local-runner/test-run` exists | `frontend/e2e/agents-page.spec.ts:54-55`; `router.rs:97` | yes |
| The book base URL is one constant and the only docs link | `frontend/src/shared/help/routes.ts:1-2` | yes; decided below |

## Decided — not re-opened by any task

Phase 67's table stands in full. Added here, each one line to reverse:

- **A workspace the runner could not keep is left where it is**, logged at `warn` with the
  attempt id and the error, and `terminal_reason.workspace_kept_at` is its original path. The
  cleanup that follows skips it. Nothing of a run whose evidence failed is deleted.
- **`run_settings` is a named shape, `RunSettings`**, every field optional, unknown keys
  kept (`#[serde(flatten)] extra`), stored as JSON exactly as today. The dialog reads typed
  fields and stops casting.
- **A subtask created from a plan is `ItemSource::Plan`**, untrusted like every non-manual
  source (the text is the agent's, edited or not), and `items.source_artifact_id` records
  the plan artifact (`083_plan_source`). Nothing in the UI reads it yet; that is fine.
- **The embedded runner tells the API its own runner id**; the `LIKE 'local-%'` query goes.
- **The Automation panel's "Specific model" writes `default_model`** as
  `{kind: "explicit", provider, model_id}` from the same measured list the dialog uses, with
  the provider derived the way the dialog derives it (native, or the gateway); "The agent's
  default" writes `{kind: "auto"}`. "Other…" is a text field under Advanced, as in the dialog.
- **A validation error with `details.field` is a sentence under that field**, not a toast;
  `ApiError` carries `field`. A 400 without a field stays a toast.
- **The dialog remembers the profile only when the person pressed a pill.**
- **"Open folder" falls back to the project's folder** for a run whose project is
  `local_branch` or `in_place` and whose attempt kept nothing.
- **A question from the agent is `info`; needing a review is `warning`.** No new token.
- **`Select` takes `help`; the popover closes on Escape and on a click outside; the help
  button sits above the modal overlay.**
- **The book URL stays a constant** in `shared/help/routes.ts`. The app has no setting for
  it and will not: the book is published with the app, at one address.

## The waves

Two agents at a time. A round's two tasks share no file.

| Round | Tasks | Needs |
|---|---|---|
| 1 | K1, K2 | nothing |
| 2 | K3, D1 | K2 merged (K3 regenerates the schema after it) |
| 3 | K4, D2 | K2 merged (D2 reads the typed `RunSettings`) |
| 4 | D3, D4 | nothing; last because D3 touches the kit every screen imports |

## How a task is handed to an agent

Exactly as `docs/plans/phase-67.md` § "How a task is handed to an agent", with `phase-68.md`
in the first line of the brief. Two changes since: the gate enforces the pinned toolchain
itself (commit `7470450`), so `PATH=$HOME/.cargo/bin:$PATH` is still set on every cargo
command the agent runs but the gate no longer depends on it; and the Playwright suite runs
as `npx playwright test --project=chromium --workers=1 --reporter=line` from the worktree's
`frontend/` (API on 3399, Vite on 5199, first compile takes minutes).

## The tasks

### K1 — a workspace that cannot be kept is not deleted

**Decision served:** ADR 0074 decision 7 (a run that could not capture its changes keeps them).
**Files:** `crates/tack-runner/src/engine.rs` (the `keep` call and the cleanup after the
terminal report), `crates/tack-runner/tests/crash_matrix.rs` or `h3_checkout.rs` (whichever
already asserts `workspace_kept_at`; extend that one).
**Do:** when `self.workspaces.keep(&spec.workspace)` returns `Err(e)`, log
`tracing::warn!(attempt_id = %…, error = ?e, "workspace could not be moved to quarantine; left in place")`,
set `terminal_reason.workspace_kept_at` to `spec.workspace.path` as today's `Ok` branch does
for the moved path, and make the cleanup that runs after the report skip this workspace
(the same way a `worktree_kept` workspace is skipped today — read how that flag reaches the
cleanup and reuse it; do not add a second flag if one boolean covers both).
**Test (RED first):** a run whose evidence fails and whose `keep` fails (make the quarantine
destination exist beforehand: `keep` returns `Io` when `destination.exists()`) ends with the
workspace directory still present and `workspace_kept_at` equal to its original path.
**Done when:** that test passes; the existing `workspace_kept_at` test still passes;
`cargo nextest run -p tack-runner` green; the gate green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### K2 — the API names its shapes: `RunSettings`, and the folder check

**Decision served:** ADR 0074 decision 8 (nothing resets; every field validates where it is
typed) needs the wire to say what a run setting is.
**Files:** `crates/tack-core/src/models.rs` (`Item.run_settings`, `UpdateItem.run_settings`,
new `RunSettings`), `crates/tack-api/src/handlers/items.rs` (the "object or null" check
becomes the typed deserialisation), `crates/tack-api/src/handlers/local_runner.rs` (the
check-folder response struct derives `ToSchema`), `crates/tack-api/src/openapi.rs` (register
the route and both schemas), `crates/tack-db/src/repo/items.rs` only if the JSON column read
needs the new type, the tests beside each, `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts` (regenerated with `./scripts/regen-generated.sh`,
staged before the gate).
**Do:** `RunSettings { harness, agent_profile_id, branch, model_provider, model_id, push,
timeout_seconds, tools, network, approvals, verify }`, each `Option<_>` with
`skip_serializing_if = "Option::is_none"`, plus `#[serde(flatten)] extra: serde_json::Map`
so a key this version does not know survives a round trip; `approvals` is a `String`, not an
enum (the dialog validates). The column stays JSON text. The check-folder response gets a
named struct with `ToSchema` and the route goes into the OpenAPI paths. Do not touch the
frontend beyond the regenerated file: the dialog's cast removal is D2's.
**Test (RED first):** a `PATCH /api/items/{id}` with `{"run_settings": {"push": true, "later_key": 1}}`
reads back both keys; `{"run_settings": "x"}` is still a 400; the `openapi_contract` test
sees `RunSettings` and the folder-check response named.
**Done when:** `schema.gen.ts` types `run_settings` as the named shape and the check-folder
response by name; `cargo nextest run -p tack-api -p tack-core` green; the gate green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### K3 — a subtask created from a plan remembers the plan

**Decision served:** ADR 0074 decision 5 (the Planner proposes; a person creates — and the
record says so).
**Files:** `crates/tack-core/src/models.rs` (`ItemSource::Plan`, `Item.source_artifact_id`,
`CreateItem.source_artifact_id`), `crates/tack-db/src/migrations.rs` (`083_plan_source`:
`ALTER TABLE items ADD COLUMN source_artifact_id TEXT`), `crates/tack-db/src/repo/items.rs`
(write and read the column; every `SELECT` that lists `source` lists it too),
`crates/tack-db/tests/migrations/` (the table test for 083, in the file that tests 082),
`crates/tack-api/src/handlers/items.rs:581-660` (`ItemSource::Plan` and
`source_artifact_id: Some(input.artifact_id.clone())`), `crates/tack-api/src/handlers/export.rs`
only if the export round-trips `source` by hand, `docs/openapi.json` and `schema.gen.ts`
(regenerated, staged).
**Do:** `Plan` is a unit variant, `"plan"` on the wire and in the column, `is_trusted` false.
The migration is a new `ordinary("083_plan_source", …)` after 082; S0's migration is not
edited. `Item` exposes `source_artifact_id: Option<String>`.
**Test (RED first):** `POST /api/items/{id}/subtasks-from-plan` creates items whose `source`
is `"plan"` and whose `source_artifact_id` is the request's; the migration test asserts the
column; `ItemSource::from_str("plan")` round-trips.
**Done when:** those pass; `cargo nextest run -p tack-core -p tack-db -p tack-api` green;
the gate green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### K4 — the embedded runner says which runner it is

**Decision served:** ADR 0074 decision 1 (the machine layer is exact, not inferred).
**Files:** `crates/tack-api/src/handlers/local_runner.rs` (the `LocalRunnerControl` trait
and `post_test_run`), `crates/tack-cli/src/local_runner.rs` (`EmbeddedRunnerControl`),
`crates/tack-api/src/server/tests.rs` (the fake control), the handler test that covers
`post_test_run`.
**Do:** add `async fn runner_id(&self) -> Option<String>` to the trait: the enrolled runner's
id once the runner has registered, `None` before. `EmbeddedRunnerControl` returns the id it
enrolled with (read how it enrols; the id is in its state, do not query the table). The
fake in `server/tests.rs` returns a fixed id. `post_test_run` uses it and the `LIKE
'local-%'` query is deleted. The error when it is `None` stays "The embedded runner is not
enrolled".
**Test:** the existing `post_test_run` test passes with the fake's id; one row for `None` →
503 with that message.
**Done when:** `rg "local-%" crates` finds nothing; `cargo nextest run -p tack-api -p
tack-cli` green; the gate green.
**Hand-off:** Haiku, ceiling 60.

### D1 — Automation finishes: a specific model, the definition of done, a sentence under the field

**Decision served:** ADR 0074 decisions 4, 6 and 8.
**Files:** `frontend/src/features/settings/panels/AutomationPanel.tsx` and its test,
`frontend/src/shared/api/client.ts` (`ApiError.field` from `error.details.field`),
`frontend/src/shared/runWithAgent/models.ts` and `shared.ts` only to *import* the measured
list and the provider derivation (do not edit them; if the derivation is not exported, export
it — that one line is allowed).
**Do:** (1) the Model field: "The agent's default (recommended)" writes
`default_model: {kind: 'auto'}`; "Specific model" enables a `Select` of the measured ids for
the project's `default_harness` (the dialog's list) and writes `{kind: 'explicit', provider,
model_id}` on change; with no `default_harness`, the radio is disabled with the sentence
"Choose an agent first". The "not available yet" note goes. (2) a "Definition of done"
textarea under "When a run finishes", saving `definition_of_done` on blur, with `(?)` text
"Appended to what the agent reads on every run of this project" and a link to the book's
Automation section. (3) `ApiError` gains `readonly field?: string`; `toApiError` reads
`inner.details?.field` when it is a string. The panel's `save` shows an error whose `field` is
`repository` under the folder field (the `Field` `error` prop) and leaves the toast for the
rest.
**Test:** vitest on the panel: the explicit model PATCH body; the DoD PATCH body; a rejected
folder renders the message under the field and no toast. `client.test.ts`: `field` parsed.
**Done when:** `npx vitest run src/features/settings src/shared/api` green; the gate green.
**Hand-off:** Sonnet, two layers, ceiling 150.

### D2 — the dialog remembers only what was picked; the card finds the folder

**Decision served:** ADR 0074 decisions 7 and 8.
**Files:** `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` and its test,
`frontend/src/shared/runWithAgent/AttemptList.tsx` and its test. Needs K2 merged: read
`item().run_settings` through the generated `RunSettings` type and delete both `as` casts.
**Do:** (1) a `profilePicked` signal set only by the pill's `onClick` (RunFlow passes the
click through; add a `onProfilePicked` prop or set the signal in the existing `setForm`
wrapper — choose the smaller); `settingsDiff` includes `agent_profile_id` when
`profilePicked()` or when the saved settings already had one. (2) `AttemptTools` falls back:
when `patch()` exists and `folder()` is null, read the item's project (`props.itemId` →
item → project; one `createResource`) and, if `workspace_mode` is `local_branch` or
`in_place` and `repository` is set, use it as the folder. The button keeps its two labels.
**Test:** vitest: opening the dialog on a project with no default profile and running
without touching a pill stores no `agent_profile_id`; pressing a pill stores it; the card on
a `local_branch` project with a patch and no `workspace_kept_at` shows "Copy path" with the
project's folder.
**Done when:** `npx vitest run src/shared/runWithAgent` green; the gate green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### D3 — three kit changes and a second tone

**Decision served:** ADR 0074 decisions 7 and 9.
**Files:** `frontend/src/shared/ui/Select.tsx`, `Field.tsx` (`HelpHint`), `HelpButton.tsx`,
`kit.test.tsx`, `frontend/src/shared/runWithAgent/RunFlow.tsx:230-236` (the Model `(?)`),
`DecisionInbox.tsx:124` (`Pending` → `info`), `frontend/src/index.css` only for a z-index
token if none exists.
**Do:** `Select` accepts `help?: FieldHelp` and passes it to `FieldShell`; the dialog's Model
field uses it and the absolute `<span>` goes. `HelpHint` closes on `Escape` and on a
`pointerdown` outside its root (one document listener while open, removed on close). The
help button renders above the modal overlay (read `Modal.tsx`'s z-index and go one above, or
portal the button; choose the smaller). `Pending` on a question becomes `tone="info"`; every
"Needs your review" stays `warning`.
**Test:** `kit.test.tsx`: Select renders `(?)` with `help`; HelpHint closes on Escape and on
outside click.
**Done when:** `npx vitest run src/shared/ui src/shared/runWithAgent` green; the gate green.
**Hand-off:** Sonnet, one layer, ceiling 100.

### D4 — the agents page's test run, asserted

**Files:** `frontend/e2e/agents-page.spec.ts` only.
**Do:** replace the "asserted after S5 lands" line: click "Run test", then assert the
embedded timeline appears for the run and reaches a terminal line (the fake shims answer in
seconds; read `agent-assets.spec.ts:315-340` for the selectors that already work against the
same screen) and that the agent's line no longer reads "Not signed in" or reads the fake's
failure, whichever the fake shim produces — assert what happens, do not fake a success.
**Done when:** `npx playwright test e2e/agents-page.spec.ts --project=chromium --workers=1`
green twice in a row; the gate green.
**Hand-off:** Haiku, ceiling 60.

## Parked — not scheduled, and why

| Item | Why |
|---|---|
| A "Proposed by the Planner" marker on an item | K3 records the provenance; nothing on screen asked for it yet |
| A docs URL setting in the app | decided above: the constant |
| A "Clear agent tests" button on the hidden project | phase-67's open question 2; the archive list has not grown |

## Status

Opened 2026-10-08. Rounds are dispatched in order; each merge is a `--no-ff` into `develop`
with the gate on the merge. Pushing and the release are the maintainer's.

- **Round 1 merged 2026-10-08**: K1 (`066817c`), K2 (`ad27a9d`); D1 merged early (`26853aa`)
  because it depended on nothing. Gate green on each merge; `tack-api`/`tack-core`/`tack-db`
  605 passed after K2.
- K1's finding: the `workspace_kept_at` assertions live in `engine/tests.rs`, not in the two
  integration files the plan named; the test went there.
- K2's finding: a body that is not an `UpdateItem` was axum's 422; it is now a 400 for every
  malformed item PATCH, with axum's message and no `details.field`.
- D1's findings, tabled: the panel derives the provider from the runner's `native_provider`
  only; the dialog's gateway choice is not offered at project level. codex and opencode have
  no measured list, so the panel offers no specific model for them (the dialog's "Other…"
  stays per run). The disabled list says why.

- **Rounds 2–4 merged 2026-10-08**: K3 (`48c50bc`), D2 (`62b5289`), K4 (`e080152`), D3
  (`3849d2c`), D4 (`3115cf5`). Every task is in `develop`; the phase is **closed** the same
  day it opened. `cargo nextest run --workspace` 1157 passed, `npx vitest run` 617 passed (624
  after D4's unit test), `tsc` clean, gate green on every merge, Playwright default suite 55
  passed — after one spec that predated this phase was brought up to date: the board dialog
  spec still expected "Choose a profile." although `2908ded` (after E1) preselects the
  Implementer, so it had been red since phase 67's last commit.
- D3's finding, corrected at review: the top bar is a `sticky z-40` stacking context, so a
  z-index on the help button inside it can never rise above the modal's `z-50` overlay. The
  Run dialog carries its own **Help** link in its title row instead (`Modal.help`).
- **D4 found two product bugs**, both fixed in `develop` before the spec could pass:
  1. The test run picked the first installed agent (codex), which reports it *requires* a
     model, while the test run names none; the scheduler refused it and the request sat
     **Queued forever with nothing to say why**. The test run now picks an agent that reports
     `model_selection: optional`, and when none does it is disabled with the reason
     (`f90b519`).
  2. K4 as merged returned `runner_config.runner_id`, the runner's configured name, not the
     `runr_` id assigned at enrolment; `exact_runner` refused it as "unavailable or
     revoked" and every test run failed to start. It now reads the persisted session's id
     (`e54b3aa`, `4362a98`). K4's unit test passed because its fake returned a seeded id: a
     fake that cannot be wrong proves nothing about the real implementation.
  With both fixed, the fake shim's run finishes amber, "Finished — no changes recorded",
  with its runner named, which is exactly what the card should say.

Follow-ups found while executing, not yet scheduled:

| Found by | Follow-up |
|---|---|
| D1 | A "Through the gateway" choice beside the native provider in Automation, when a gateway key is set. |
| K2 | A stored `run_settings` that does not fit `RunSettings` reads back as `None` (the row conversion's `.ok()`); log it at warn so a typed-wrong value is not silent. |
| D4 | **A request no runner can ever claim stays Queued forever, silently.** The scheduler's ineligibility reason (`AutoSelectNotVerified`, no capable runner, …) is computed on every claim and thrown away. Record the last reason on the request and show it on the card ("Waiting: codex needs a model named"), and let a request that has been ineligible for N minutes fail with that reason. This is the same class as ADR 0074's finding 11 and deserves a task of its own. |
| D4 | `EmbeddedRunnerControl::runner_id` has no test against the real implementation; a test that enrols the embedded runner against a loopback API and reads the id back would have caught K4. |
