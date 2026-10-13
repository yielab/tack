# Plan: Phase 71 — the agent brings its own model

**Status: opened 2026-10-13. ADR 0078 (`docs/adr/0078-the-agent-brings-its-own-model.md`)
carries the decisions; this file carries the work.**
Written against `develop` at `5db03ff2`. Re-run `rg -n` before trusting a line number.

What this phase builds, in one paragraph. Tack stops holding model keys, endpoints and catalogs.
Each agent uses its own login and its own configuration; Tack asks it which models it offers and
whether it is signed in. A run names its model as one string in the agent's own spelling, or
names none and gets the agent's default. The model field is the same control in the run dialog
and the project's Automation panel: the agent's default, the models the agent listed, or a typed
name with a line that says how to find one and a link to the agent's own documentation. The Agents
page's third step stops being a gateway key and becomes **Sign in**, read from each agent. docket
runs from its contract 1.2, which reads the person's own `docket setup`. Nothing is kept for
compatibility with the provider layer.

## Decided — not re-opened by any task

Every row of ADR 0078's table, plus:

| Question | Decision |
|---|---|
| The HTTP secrets routes (`/api/local-runner/secrets`) | Removed with the key panel: their only caller was the panel (ADR 0061 decision 2). `tack runner secret set\|list\|remove` stays and reaches the store directly. |
| `ModelSelectionReport` / `model_selection` | Replaced in C by `requires_model: bool` and a reason; B keeps it as is so the wire changes once per field. |
| `model_passthrough` | Removed in B: every model string is passed through, so the field says nothing. |
| An agent's listing or sign-in command | Runs at probe time, as the runner's user, with only `HOME` and `PATH`, a 20-second timeout each, in parallel, never inside the sandbox. A failure reports the reason and no list; it never fails the probe. |
| The intermediate state | After A, opencode and docket do not run until D and E. `develop` is not pushed during the phase. |
| docket before 0.2.0-beta.5 | Does not run (ADR 0078 decision 7). E waits for that docket; the prompt that builds it was handed to the maintainer on 2026-10-13. |

## Rounds

At most two agents building cargo at once; a frontend-only task runs beside them.

| Round | Tasks | Needs |
|---|---|---|
| 0 | fixtures (integrator) | nothing |
| 1 | A, F1 | nothing |
| 2 | B, then B2 on B's branch; merged together | round 1 |
| 3 | C, D | round 2; round 0's fixtures |
| 4 | F2 | C |
| 5 | E | docket 0.2.0-beta.5 and its report; C |
| 6 | G (integrator) | everything else |

## How a task is handed to an agent

As `docs/plans/phase-67.md` § "How a task is handed to an agent", with `phase-71.md` in the first
line of the brief, the three lines Phase 69 added, and these:

```
The ADR is docs/adr/0078-the-agent-brings-its-own-model.md; its table is the decision, do not re-open it.
Delete, do not deprecate: no shim, no alias, no "legacy" branch, no serde rename kept for old input.
Every harness output in a test comes from a fixture under crates/tack-runner/src/harness/fixtures/, printed by a fake binary.
```

Ceilings: the phase-67 limits, except where a task names its own (A, B and C are wide but
mechanical; reaching the ceiling hands off with a written state, never a resumed context).

## The tasks

### 0 — fixtures from the real agents (integrator)

**Do:** capture, with `.provenance` files beside them, under
`crates/tack-runner/src/harness/fixtures/<kind>/<version>/`:
- Claude Code: `claude auth status` signed in, and with a scratch `HOME` (signed out).
- Codex: `codex login status` signed in, and with an empty `CODEX_HOME` (signed out);
  `codex debug models`, and the same command with `--bundled`.
- opencode: `opencode models`; `opencode auth list`; `opencode debug config` and
  `opencode debug paths`, each with the person's config and with a scratch `HOME`.

Redact emails, org ids and every key value. No agent runs these commands.

### A — the runner holds no model credential

**Files:**
- Delete:
  - `crates/tack-runner/src/provider/**`
  - `crates/tack-cli/tests/embedded_runner_live_secret.rs`
- Runner:
  - `crates/tack-runner/src/{lib.rs, config.rs, config/tests.rs, error.rs, bootstrap.rs}`
  - `crates/tack-runner/src/harness/{mod.rs, tests.rs, test_support.rs, local_process.rs, local_process/tests.rs, claude_code.rs, claude_code/tests.rs, codex.rs, codex/tests.rs, opencode.rs, opencode/tests.rs, docket.rs, docket/tests.rs}`
  - `crates/tack-runner/tests/{agent_boundaries.rs, crash_matrix.rs, live/claude_code.rs, live/codex.rs}`
- CLI:
  - `crates/tack-cli/src/{doctor.rs, doctor/tests.rs, local_runner.rs, local_runner/tests.rs, main.rs}` (in `main.rs`, doc strings only)
- API:
  - `crates/tack-api/src/{handlers/local_runner.rs, router.rs, lib.rs, server/tests.rs}`
  - `crates/tack-api/tests/handlers/local_runner.rs`
- Generated: the regenerated `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts`.
- Other: `.config/nextest.toml`; `scripts/smoke.sh`.

**Do:**
1. Delete the provider module and everything that reaches it:
   - `[provider.*]`, `ProviderConfig`, `ProviderOverride`, `TACK_RUNNER_PROVIDER_*`, `ConfigError::ProviderBaseUrlMissing`.
   - The catalog in bootstrap and discovery.
   - `HarnessDescriptor.wire` and `.credential_env`, `RunContext.endpoint`, `RunRecord.endpoint`.
   - The `providers` field and `with_providers`, `resolve_endpoint`, the credential injection in `prepare`.
   - The probe's `providers` list.
2. In each grammar, delete the endpoint branch:
   - claude-code: `ANTHROPIC_BASE_URL`/`ANTHROPIC_API_KEY`, and the registry lookup in `is_known_provider`.
   - codex: the `-c model_provider*` overrides, `PROVIDER_KEY`, `toml_quoted`, and `modelProvider` in `thread/start`.
   - opencode: the injected `tack` provider. Without one it refuses with "opencode can't run yet: it needs its own login, which this runner doesn't pass".
   - docket: the injected base URL and `credential_env`. It refuses as today when no `DOCKET_LLM_BASE_URL` reaches it.
3. Leave `native_provider` and `observes_served_model` for B.
4. In `tack runner doctor`, remove `render_provider` and its footer, and rewrite each `credential_note` as "<Agent> signs in with its own login; Tack holds no model credential."
5. In `local_runner.rs`, remove the catalog, the Vercel auto-enable, `provider_resolves_secret` and the provider restart (`restart_for_provider_change_locked` and `stop_and_join_locked` too, if nothing else calls them).
6. In the API:
   - remove `CatalogSnapshot`, `catalog()` and the `catalog` key of `GET /api/local-runner`;
   - remove the `/api/local-runner/secrets` routes and their tests.
7. In `smoke.sh`, delete steps 13 and 14 and the `GATEWAY_PID` cleanup, and renumber the header.
8. In `nextest.toml`, drop `binary(embedded_runner_live_secret)`.
9. Regenerate, and stage the generated files.

**Test (RED first):** in `agent_boundaries.rs`:
- `the_runner_passes_no_model_credential_of_its_own`: for each of the four descriptors, a spawn whose request names no environment carries none of `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, `AI_GATEWAY_API_KEY`, `OPENAI_API_KEY`, `DOCKET_LLM_API_KEY`, `DOCKET_LLM_BASE_URL`, even with all of them set in the runner's own environment.
- In `config/tests.rs`, a runner file with `[provider.vercel_ai_gateway]` is refused, and the error names `provider`.

**Done when:**
- `rg -n "provider::|ProviderConfig|vercel|Vercel|AI_GATEWAY|openai_compatible|anthropic-direct" crates scripts .config` prints nothing outside history files;
- `cargo nextest run --workspace` is green;
- the gate is green.

**Hand-off:** Sonnet, runner + cli + a thin API slice, ceiling 200.

### F1 — the app asks for no key

**Files:**
- Delete:
  - `frontend/src/features/agents/{ProviderKeyPanel.tsx, ProviderKeyPanel.test.tsx, steps/ProviderStep.tsx}`
  - `frontend/e2e/provider-key-panel.spec.ts`
- Agents page: `frontend/src/features/agents/{AgentsPage.tsx, AgentsPage.test.tsx, api.ts, constants.ts}`
- Run dialog: `frontend/src/shared/runWithAgent/{RunWithAgentModal.tsx, RunFlow.tsx, RunWithAgentModal.test.tsx}`
- Automation panel: `frontend/src/features/settings/panels/{AutomationPanel.tsx, panels.test.tsx}`
- Playwright: `frontend/e2e/{agents-page.spec.ts, execution-toggle.spec.ts}` (their references to the deleted spec only)

**Do:**
1. On the Agents page, remove step 3 and its marker. The page has two numbered steps until F2 adds **Sign in**.
2. In `api.ts`, remove `VERCEL_AI_GATEWAY_SECRET_NAME`, `CatalogSnapshot`, `LocalRunnerStatus.catalog`, `SecretMeta` and the secrets calls.
3. In the run dialog and the Automation panel, remove "Through" and everything feeding it:
   - `viaGateway`, `gatewayProvider`, `throughOptions`, `PROVIDER_LABELS`/`GATEWAY_LABELS`;
   - the "add a gateway key on the Agents page" note.

   The provider a run sends is the agent's native one until B2.
4. In `constants.ts`, the `HarnessStep` labels that say "uses a provider endpoint (step 3)" say "Can't run yet on this computer" until F2 replaces them.

**Test (RED first):**
- `AgentsPage.test.tsx`: the page renders no key field and no "Provider" heading.
- The dialog and the panel render no "Through" control.

**Done when:**
- `rg -n "gateway|Gateway|viaGateway|ProviderKey|catalog" frontend/src frontend/e2e --glob '!*.gen.ts'` prints nothing;
- vitest is green, and typecheck is green against the schema A regenerated;
- the gate is green.

**Hand-off:** Sonnet, frontend, ceiling 120.

### B — one model string (Rust)

**Files:**
- Contract: `docs/contracts/runner-v1/{capabilities.json, enrollment.request.json, claim.response.json, completion.request.json, README.md}`
- Orchestration:
  - `crates/tack-orch/src/execution/{types.rs, capabilities.rs, mod.rs}` and their tests
  - `crates/tack-orch/src/model_policy/**`
  - `crates/tack-orch/src/scheduler/{types.rs, select.rs, wiring.rs}` and their tests
  - `crates/tack-orch/src/usage_provenance.rs` and its tests
  - `crates/tack-orch/tests/**`
- Database: a new migration in `crates/tack-db` (086), `crates/tack-db/src/repo/{execution.rs, projects.rs}`, `crates/tack-db/tests/**`
- Core: `crates/tack-core/src/models.rs`
- API: `crates/tack-api/src/{handlers/executions.rs, handlers/projects*.rs, handlers/templates.rs, export.rs, openapi.rs}`, `crates/tack-api/tests/**`
- CLI: `crates/tack-cli/src/{main.rs, execution.rs, execution/tests.rs, mcp.rs}`, `crates/tack-cli/tests/{e6_scheduler_e2e_test.rs, cli_test.rs}`
- Runner:
  - `crates/tack-runner/src/{engine.rs, verify/prill.rs}`
  - `crates/tack-runner/src/harness/{mod.rs, local_process.rs}` and each grammar, with their tests
  - `crates/tack-runner/tests/**`
- Generated: the regenerated `docs/openapi.json` and `schema.gen.ts`, staged.

**Do:**
1. **The wire.**
   - `requested_model: string | null` replaces `requested_model_provider` and `requested_model_id`.
   - `actual_execution.model: string | null` and `actual_execution.model_source: "reported" | "requested" | "agent_default"` replace `model_provider`, `model_id` and `model_observation_source`.
   - `HarnessCapability` loses `native_provider`, `providers`, `model_combinations` and `model_passthrough`. `ModelCombination`, `ModelMetadata` and `ModelProvider` are deleted. Fixture first.
2. **The migration.**
   - `execution_requests` gains `requested_model`, filled from `requested_model_id`, and drops both old columns. Recreate any index that named them.
   - Each attempt's `actual_execution` JSON is rewritten to the new keys:
     - `model` comes from `model_id`;
     - `model_source` is `reported` where the old source said the harness reported it, `requested` otherwise, and `agent_default` when there was no model.
   - `projects.default_model` becomes a bare model string: `explicit` keeps its `model_id`, `auto` and null become NULL.
   - Each item's `run_settings` `model_provider`/`model_id` become `model`.
3. **The policy.** A run's model is:
   - the request's;
   - otherwise the project's default model, when the run's harness is the project's default agent;
   - otherwise none.

   Delete `ModelSelector`, the profile and fleet `default_model` tiers (`parse_model_default_convention` and its fetches) and `ProjectModelDefault`. Updating a project's default agent clears its default model.
4. **The scheduler.** Delete the model-combination match, `ModelCombinationNotDeclared` and `PartialModelSelector`. A request with no model, for a harness whose `model_selection` is explicit, is ineligible with the reason "<agent> needs a model named".
5. **Provenance:** `{requested, served, source}`, nothing else.
6. **CLI and MCP.** The CLI takes `tack execution create --model <name>`; `--model-provider` and `--model-id` are gone. MCP `create_execution` takes `model`.
7. **The runner.**
   - `check_request` asks only for a model when the descriptor says explicit.
   - Each grammar passes the string verbatim, through its own flag and only when present: claude-code `--model`; codex `--model`, plus `model` in `thread/start`; opencode `-m`; docket `--model`.
   - `outcome` records `reported` when the agent's own output named the model:
     - docket's `model.served`;
     - claude-code's init line;
     - codex's `thread/start` result.

     It records `requested` when only the request named it, and `agent_default` when neither did.
   - Delete `native_provider`, `observes_served_model`, `requires_unconfirmed_model_recording` and `RequestedNotConfirmed`.
   - `verify/prill.rs` sends `generator.model` and no `family`.
8. **The frontend is B2's.** Run the Rust half of the gate (fmt, clippy, nextest, the generated-files check) and report the frontend typecheck errors the new schema causes, without fixing them.

**Test (RED first):**
- A migration test seeds each old shape and reads back the new one:
  - an old request;
  - an attempt with `harness_reported`, one with `requested_not_confirmed`, and one with no model;
  - a project default of `explicit` and of `auto`;
  - an item's `run_settings`.
- `model_policy`: the three tiers as one table test, including a run on a harness other than the project's default agent that gets no project model.
- The scheduler: the "needs a model named" sentence.
- Each grammar: no model flag when none is named.

**Done when:**
- `rg -n "requested_model_provider|requested_model_id|model_combinations|ModelSelector|native_provider|model_passthrough" crates docs/contracts` prints nothing;
- `cargo nextest run --workspace` is green; clippy is clean;
- the generated files are staged.

**Hand-off:** Sonnet, wide and mechanical, ceiling 300.

### B2 — the frontend follows the string (on B's branch)

**Files:**
- Execution types: `frontend/src/shared/execution/{types.ts, capabilities.ts, index.ts, api.ts, attempts.ts}` and their tests
- Run dialog: `frontend/src/shared/runWithAgent/{shared.ts, RunWithAgentModal.tsx, RunFlow.tsx, attemptFormat.ts}` and their tests, plus `modelPolicyContract.test.ts`
- Automation panel: `frontend/src/features/settings/panels/{AutomationPanel.tsx, panels.test.tsx}`
- Agents page: `frontend/src/features/agents/{runnerObservations.ts, runnerObservations.test.ts, steps/TestRunStep.tsx}`
- Shared types: `frontend/src/shared/types/index.ts`
- Playwright: `frontend/e2e/{helpers.ts, run-with-agent.spec.ts, agent-assets.spec.ts, screenshots.spec.ts, execution-attempt-detail.spec.ts, recovery-demo.spec.ts, docket-prill.spec.ts}`

**Do:**
1. Follow B's schema: one `model` string everywhere.
2. Delete:
   - the combination aggregation and `isCombinationSupported`;
   - `ModelDefaultConvention`, `parseModelDefaultConvention`, `describeProjectModelDefault`'s pair and `isModelPassthroughAttested`;
   - the dead helpers in `runnerObservations.ts`;
   - `describeModelProvenance`, or make it read `model_source`, whichever has a caller.
3. Until F2, the model control keeps today's shape (default, or a list plus "Other…"), carrying a string.
4. The project's default model saves a string; changing the default agent clears it.

**Test (RED first):** the existing dialog and panel tests, rewritten to the string. Add one test: changing the panel's default agent clears its default model.

**Done when:**
- `rg -n "model_provider|modelProvider|model_combinations|modelCombos" frontend/src frontend/e2e --glob '!*.gen.ts'` prints nothing;
- vitest and typecheck are green, and the gate is green on the B + B2 branch.

**Hand-off:** Sonnet, frontend, ceiling 150.

### C — each agent says what it offers and whether it is signed in

**Files:**
- Contract: `docs/contracts/runner-v1/{capabilities.json, README.md}`
- Orchestration: `crates/tack-orch/src/execution/capabilities.rs` and its tests
- Runner:
  - `crates/tack-runner/src/harness/local_process.rs` (`probe` only)
  - new `crates/tack-runner/src/harness/{claude_code,codex,opencode}/readiness.rs`
  - in each grammar file, its descriptor's hook line only
  - `crates/tack-runner/src/harness/tests.rs`
- CLI: `crates/tack-cli/src/{doctor.rs, doctor/tests.rs}`
- Generated: the regenerated `docs/openapi.json` and `schema.gen.ts`, staged.

**Do:**
1. `HarnessCapability` gains these fields, fixture first:
   - `requires_model: bool` and `requires_model_reason`, replacing `model_selection`;
   - `models: { source: "listed" | "suggested" | "none", ids: [string], reason: string | null }`;
   - `default_model: string | null`, only when the agent names one;
   - `sign_in: { state: "signed_in" | "signed_out" | "unknown", detail: string, command: string | null }`;
   - `docs: { models: url, sign_in: url }`.
2. Per agent, from round 0's fixtures:

   | Agent | Sign-in (`sign_in`) | Models (`models`, `default_model`) | `requires_model` |
   |---|---|---|---|
   | claude-code | `claude auth status` (`loggedIn`; detail `authMethod`; command `claude`) | suggested `sonnet`, `opus`, `haiku`; no default | false |
   | codex | `codex login status`; command `codex login` | `codex debug models` entries with `visibility: list`, falling back to `--bundled`; no default | false |
   | opencode | signed in when `opencode auth list` names a credential or `opencode debug config` names a provider; otherwise signed out with the detail "No login: only opencode's free models are available" (with no login it still lists its free `opencode/*` models, `models-scratch-home.txt`); command `opencode auth login` | `opencode models`, listed; `default_model` is the config's `model` | true when the config has no `model` |
   | docket | unknown, "Tack reads docket's sign-in from docket 0.2.0-beta.5" | none | true |

3. Probe rules:
   - Commands run at probe, as in "Decided".
   - Parsing lives in each `readiness.rs`.
   - The descriptor carries one function pointer.
   - Docs URLs are constants in the descriptor (the ADR's table, plus the sign-in pages the integrator gives in the brief).
4. `tack runner doctor` prints a `signed in:` and a `models:` line per agent.

**Test (RED first):**
- One table test per `readiness.rs` over the fixtures: signed in, signed out, garbage output, timeout.
- A probe test: a listing that fails leaves `models.source = none` with the reason, and the probe still succeeds.

**Done when:**
- `cargo nextest run --workspace` is green;
- the gate is green, with the frontend's `model_selection` reads switched to `requires_model` in the same branch. That is the only frontend edit allowed; anything more is a finding.

**Hand-off:** Sonnet, two layers, ceiling 200.

### D — opencode uses its own login, and nothing else of its setup

**Files:**
- `crates/tack-runner/src/harness/{opencode.rs, opencode/tests.rs, sandbox.rs}`
- `crates/tack-runner/tests/agent_boundaries.rs`
- the fixtures from round 0

**Do:**
1. At spawn, run `opencode debug config` and `opencode debug paths` against the person's real `HOME`, the way C runs its probes.
2. Copy only `provider`, `model` and `small_model` into the config Tack injects, beside Tack's own permissions and agent. Tack's injected config never carries `mcp`, `plugin`, `agent` from the person, `instructions`, `command` or `skills`.
3. Keep the scratch `HOME`, and pass `--pure`.
4. Make the person's `auth.json`, from `debug paths`' `data` directory, reachable at the scratch data path, read-write, because opencode refreshes OAuth tokens there:
   - under `bwrap`, a file bind;
   - unsandboxed, a symlink.

   No other file under the person's opencode data or config directory is reachable.
5. Register every string under a copied `apiKey`, `api_key` or `headers` key as secret material for redaction.
6. `-m <model>` is passed verbatim. With no model and no copied `model`, refuse with "opencode has no default model: name one, or set `model` in opencode's own config".
7. Replace A's refusal.

**Test (RED first):** in `agent_boundaries.rs`, `opencode_gets_its_login_and_providers_and_nothing_else`, using the fixture config with MCP servers, plugins and permission allows. Assert:
- the injected config has the fixture's `provider`/`model` and Tack's permissions, and none of the person's `mcp`, `plugin`, `permission` or `agent`;
- `--pure` is in the arguments;
- `auth.json` is reachable and a sibling file is not;
- a seeded `apiKey` value never reaches the run log.

**Done when:** those pass; `cargo nextest run --workspace -E 'package(tack-runner)'` is green; the gate is green.

**Hand-off:** Sonnet, one layer, ceiling 150.

### F2 — one model control, and Sign in on the Agents page

**Files:**
- Run dialog:
  - new `frontend/src/shared/runWithAgent/{ModelField.tsx, ModelField.test.tsx}`
  - `frontend/src/shared/runWithAgent/{RunFlow.tsx, RunWithAgentModal.tsx, models.ts}` and their tests
- Automation panel: `frontend/src/features/settings/panels/{AutomationPanel.tsx, panels.test.tsx}`
- Agents page: `frontend/src/features/agents/{AgentsPage.tsx, AgentsPage.test.tsx, steps/HarnessStep.tsx, steps/TestRunStep.tsx, constants.ts, api.ts}` and new `steps/SignInStep.tsx`
- Help: `frontend/src/shared/help/texts.ts`
- API: `crates/tack-api/src/{handlers/local_runner.rs, router.rs}`, `crates/tack-api/tests/handlers/local_runner.rs` (removing `harness-verification`)
- Generated: the regenerated `docs/openapi.json` and `schema.gen.ts`
- Playwright: `frontend/e2e/{agents-page.spec.ts, run-with-agent.spec.ts, screenshots.spec.ts}` and the screenshots they write

**Do:** one `ModelField`, used by the dialog and the panel.

1. **Its options:**
   - **Agent's default** (shown with the name when `default_model` is set), only when `requires_model` is false;
   - each listed or suggested id;
   - **Another model…**, which opens a text field.
2. **The line under it, spelled:**
   - listed: "From <Agent> on this computer."
   - suggested: "<Agent> can't list its models here. These are the names its documentation gives; you can type another."
   - none: "<Agent> can't list its models here. Type the model's name as <Agent> spells it."

   Each is followed by a link, "How <Agent> names models", to `docs.models`.
3. A typed name is never validated by Tack.
4. With `requires_model` and no model, the Run button is replaced by the existing prerequisite row, saying `requires_model_reason`.
5. **Agents page, step 3 "Sign in":** one row per installed agent, from `sign_in`.
   - signed in: "Signed in" plus the detail;
   - signed out: "Not signed in. Run `<command>` in a terminal." plus a link to `docs.sign_in`;
   - unknown: the detail, plus the link.

   The step is done when every installed agent that can run here reads signed in.
6. Delete `ACCEPTED_MODELS`, the `harness-verification` route, its frontend call and `CANNOT_OBSERVE_VENDOR_LOGIN`.
7. Regenerate the screenshots the Playwright screenshot spec writes (headless only).

**Test (RED first):**
- `ModelField.test.tsx`: one table over the three `models.source` values crossed with `requires_model`, asserting the options, the line and the link.
- `AgentsPage.test.tsx`: the three sign-in states.
- The dialog: a typed model reaches the request verbatim.

**Done when:**
- vitest, typecheck and the Playwright suite (one worker) are green;
- the gate is green.

**Hand-off:** Sonnet, frontend plus a thin API slice, ceiling 200.

### E — docket from contract 1.2 (waits on docket 0.2.0-beta.5)

**Files:**
- Runner: `crates/tack-runner/src/harness/{docket.rs, docket/probe.rs, docket/tests.rs, sandbox.rs}`, new `crates/tack-runner/src/harness/docket/readiness.rs`
- Fixtures: `fixtures/docket/<version>/`
- Live proof: `scripts/docket-prill-e2e.sh`, `frontend/e2e/docket-prill.spec.ts`

**Do:** docket's report (the probe, the readiness JSON, the setup files it reads) is pasted into the brief.

1. Probe `--contract 1.2` the way 1.1 is probed. A docket without it carries `probe_error` "needs docket 0.2.0-beta.5 or later".
2. Set `DOCKET_SETUP_HOME` to the person's docket home, bound read-only to exactly the files the report names.
3. Pass `--model` only when one is named.
4. Fill `sign_in`, `models`, `default_model` and `requires_model` from the readiness JSON.
5. Delete every `DOCKET_LLM_BASE_URL` path.
6. The e2e script configures docket's own setup, in a scratch setup home with a `local` provider, instead of a runner provider.

**Test (RED first):**
- A probe table over the 1.1 and 1.2 refusals.
- `docket_reads_its_setup_and_nothing_else` in `agent_boundaries.rs`: the setup files are reachable, and a pod or trace file under the same home is not.

**Done when:** those pass; the gate is green; the integrator runs `scripts/docket-prill-e2e.sh` live against the local model.

**Hand-off:** Sonnet, one layer, ceiling 150.

### G — the integrator's

**Do:**
1. ADRs:
   - 0061, 0063, 0067, 0074 and 0075 each gain one dated line naming what ADR 0078 superseded.
   - 0078 records the phase's merges.
2. Configuration and the book:
   - `docs/CONFIG.md` loses the provider sections.
   - `agent-runners.md`, `cli.md`, `tutorial.md` and `quick-start.md` follow the new model field and the Sign in step. One writer, pasted.
3. Other docs:
   - `docs/TESTING.md`, `docs/API-REFERENCE.md`, `docs/ARCHITECTURE.md`;
   - the `roadmap.md` lines;
   - `CHANGELOG.md`'s `[0.1.0-beta.10]` section, as a removal entry;
   - `CLAUDE.md`'s pointer.
4. The README's Requirements: one line, "an agent installed and signed in with its own tools; Tack holds no model key".
5. The landing's run notes (newPortaflio), in English and Spanish, to the same sentence.
6. Re-check every docs URL the descriptors carry: each answers 200.

## Status

Opened 2026-10-13.

- **Round 0** (`742c0f28`): fixtures captured from claude 2.1.291, codex 0.149.1 and opencode 1.18.30.
- **Round 1:**
  - F1 merged in `bee9b2cc`, A in `3b40816e`. On the merge: the gate is green, nextest is 1244/1244, vitest is 664/664.
  - Neither agent wrote its tests RED first.
  - A's integrator fix: an old `[provider]` table now fails with `ConfigError::ProviderTable`. The error names the key and never the parser text, because that text can echo an enrolment credential. `wiremock` left `tack-runner`'s dev-dependencies.
  - A's finding: docket cannot get a base URL through the runner at all now. `env_policy` refuses `*_BASE_URL` in a request, and the grammar reads only the request's environment. So A deleted the two docket real-binary tests that drove a fake model server; E brings them back on contract 1.2.
  - Strings left for B/B2/G: `scripts/docket-prill-e2e.sh` (E), `crates/tack-api/tests/handlers/attempt_lists.rs:572`, `crates/tack-orch/src/execution/capabilities/tests.rs:69`, `frontend/e2e/helpers.ts:615-618`, `frontend/e2e/docket-prill.spec.ts:531,552`, `frontend/src/shared/execution/types.ts:172`, `frontend/src/shared/runWithAgent/AttemptList.test.tsx:116`.
- **Round 2:**
  - B and B2 merged in `e7852db7`. On the branch: the gate is green, nextest is 1217/1217, vitest is 592/592, typecheck is clean, and Playwright is 54/54 with one worker. `a11y` agents-page is the known color-contrast flake: it failed once in the full run, then passed 3/3 run alone.
  - B also rewrote each stored `request_snapshot`, so a request queued across the upgrade keeps its model. The wire's `model_selection` is `optional | required`. The scheduler's reason is `model_not_named`, "<agent> needs a model named".
  - The integrator deleted the e2e test of an undeclared model, since there is no such rule now, and made two specs type the model through "Other…".
  - The verifier's evidence never carried the agent's version or model: `Usage` has neither, and prill read them from it. `HarnessOutcome::evidence_usage` now adds both, and a unit test covers it.
  - B2's finding for F2: the Automation panel has no "Other…", so it cannot set a default model for an agent that lists none.
- **Round 3:**
  - D merged in `029124f8`, C in `c650d33d`. On the merges: the gate is green, nextest is 1225/1225, vitest is 592/592.
  - D, integrator fix: opencode's invocation located `opencode` on `PATH` again instead of using the binary the harness's locator resolved. In a test whose locator points at a fake, that ran the real opencode against the real `HOME`. `RunContext` now carries `program`, and the spawn path passes the located binary.
  - D's own additions: `Invocation.secrets` (registered for redaction) and `Invocation.shared_files` (a single-file bind under `bwrap`, a symlink unsandboxed).
  - C added `requires_model` (omitted when false), `requires_model_reason`, `models {source, ids, reason}`, `default_model`, `sign_in {state, detail, command}` and `docs {models, sign_in}`. The capability is opaque in OpenAPI, so the frontend types are hand-written.
  - C, integrator fix: codex's descriptor became `Optional`, as decision 2 says. The old reason, that no model could be recorded, no longer holds: `model_source` records `agent_default`.
  - Neither agent wrote its tests RED first.
- **Round 4 and G:**
  - G merged in `81849047`, F2 in `3ce424c5`. On F2's branch: the gate is green, vitest is 602/602, Playwright is 54/54 with one worker, and screenshot capture is 11/11.
  - The integrator added opencode and docket shims to Playwright's `PATH`. Since C, the probe runs each agent's sign-in and model commands, and the e2e server found the real opencode and docket installed on the machine and read the operator's own login. The two specs that still typed into the old "Specific model" control now use "Another model…".
  - The integrator also fixed `scripts/smoke.sh`. It sent the old model pair, and its shims counted the probe's checks as runs. Its step 9 had been blind since Phase 69's sandbox: the shim's run markers and release file sat in the sandbox's private `/tmp`, so it counted 0 runs and the requeued attempt hung. The runners now get a HOME whose `~/.codex` holds both. Fake mode: SMOKE PASSED.
  - Left for the maintainer, because these are live, billed recordings against a release build: the tutorial screenshots `07-agents-off`/`08-agents-on` and the book's `agents.png`, which still show the old "Provider" step.
  - Only E was open. It waited on docket 0.2.0-beta.5.
- **Round 5 (E):**
  - Done by the integrator on 2026-10-13 against docket 0.2.0-beta.5 (docket's branch `harness-contract-1.2`, `67665c58`, not yet released). Fixtures under `fixtures/docket/0.2.0b5/`: the probe on b5 and on b4, three `docket setup harness --json` documents, and a run on the setup default against a loopback fake model.
  - The grammar speaks only contract 1.2: a docket that does not carries `probe_error` "needs docket 0.2.0-beta.5 or later…" (a new `HarnessGrammar::unsupported` hook, also refused before spawn). Every 1.1 flag is part of 1.2, so the flag probes went; the answer line stamps `1.2.0`.
  - `DOCKET_SETUP_HOME` is a scratch directory with `docket-providers.json`, `docket-models.json` and `secrets.json` of `~/.docket` bound read-only (`Invocation.read_only_files`, `sandbox::SharedFiles`); every value in `secrets.json` is registered for redaction. `docket_reads_its_setup_and_nothing_else` checks it under real bwrap, and fails when the bind is removed.
  - Recipes are refused before spawn: docket takes `--recipe` only on 1.1, which needs `DOCKET_LLM_BASE_URL`.
  - The two real-binary tests are back (a run, and a run that asks), against a wiremock endpoint named in a scratch docket setup; they skip on a docket older than b5.
  - `scripts/docket-prill-e2e.sh` had been broken since Phase 69: its direct API calls carried no token (401). It now sets `TACK_API_TOKEN`, and makes docket's setup with `docket setup provider add` in a scratch `HOME` shared by the server and the runner. Live run: 9/9, prill's pack on every run, one real question answered.
  - `smoke.sh`'s docket shim answers the 1.2 probe, and step 8 covers docket again on its own default: SMOKE PASSED. The gate is green, nextest is 1229/1229, Playwright is 54/54.
