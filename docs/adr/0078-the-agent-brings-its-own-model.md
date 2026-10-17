# ADR 0078: the agent brings its own model

**Status: ACCEPTED 2026-10-13 (the maintainer: "apply that change, clean up all the old code
with no need to keep compatibility; choosing a model must stay possible, besides using the
default"); executed by Phase 71 (`docs/closed-cycles/plans/phase-71.md`).**

**Decide:** that Tack stops holding model credentials, endpoints and catalogs. Each agent
(Claude Code, Codex, opencode, docket) signs in and reaches its model with its own tools, the way
the person already set it up. A run names its model as one string in that agent's own spelling,
or names none and gets the agent's default. Tack asks each agent which models it offers and
whether it is signed in; where an agent cannot say, the model field takes a typed name and links
to that agent's own documentation.

**Why now:** Tack's provider layer (ADR 0061, ADR 0063) made a person learn two credential
systems before a first run: the agent's own login, and a Tack-held gateway key with its own
catalog and a "Through" choice in every run. It covers the agents unevenly: Codex takes a key only
through the gateway, local models only reach opencode and docket, and only the gateway key can be
set from the app; the rest needs a TOML file. opencode and docket cannot use their own setup at
all, because Tack hides it from them. The catalog's prices and context windows reach no screen.
Every agent already has to be installed and signed in with its own tools in a terminal, so the
"no terminal" promise the layer was built for is not kept anyway. `v0.1.0-beta.10` is not yet
re-cut, and Tack has no users: there is no cheaper moment.

**If you do nothing:** the README and the landing need a table to say which agent takes which
key; a person who pastes a gateway key sees hundreds of models offered to every agent, most of
them never measured; and Tack keeps holding a credential it does not need.

## Decisions

| # | Decision |
|---|---|
| 1 | **Tack holds no model credential, endpoint or catalog.** The runner's provider module, `[provider.*]` tables, `TACK_RUNNER_PROVIDER_*` variables, the gateway key on the Agents page, the "Through" choice in the run dialog and the Automation panel, the catalog in `GET /api/local-runner` and in `tack runner doctor` are removed. Nothing is kept for compatibility: an old `[provider.*]` table is an unknown field and the runner refuses the file, naming the table. The runner's secret store stays: `[environment]` and `secret_reference` use it. |
| 2 | **A run's model is one optional string, in the agent's own spelling.** `requested_model_provider` and `requested_model_id` become `requested_model` (for example `sonnet`, `gpt-5.6-terra`, `anthropic/claude-sonnet-4-5`, `local/qwen`). None means the agent's default. The runner passes the string through its agent's own flag and never parses it. The recorded result is `actual_execution.model` with `model_source`: `reported` when the agent named the model that answered, `requested` when it only echoed the request, `agent_default` when none was named and the agent did not say. A migration rewrites stored rows; nothing reads the old fields after it. |
| 3 | **(feel) The model is chosen in one control, the same everywhere.** The run dialog and the project's Automation panel show: **Agent's default** (when the agent has one), the models the agent listed, and **Another model…** with a text field. Under the field one line says where the list came from ("From Codex on this computer") or, when the agent cannot list its models, how to name one ("Claude Code can't list its models here. Type a name such as `sonnet` or `opus`.") with a link to that agent's model documentation. A typed name is never rejected by Tack; the agent checks it when the run starts. |
| 4 | **The agent lists its models; Tack only asks.** At probe time the runner runs each agent's own offline listing: `codex debug models` (entries with `visibility: list`), `opencode models`, docket's setup report (decision 7). Claude Code has no listing command, so its probe reports the aliases its documentation names (`sonnet`, `opus`, `haiku`) as **suggested**, not listed. A listing that fails or times out reports no list and why; it never fails the probe. |
| 5 | **(feel) The Agents page says whether each agent is signed in, from the agent itself.** Step 3 becomes **Sign in**, one line per installed agent, from the agent's own offline check: `claude auth status` (`loggedIn`), `codex login status`, `opencode auth list` plus the providers in its resolved config, docket's setup report. When an agent is not signed in, the line names its own command and links to its documentation; Tack gives no install or setup instructions for another program. The route that called an agent "Signed in" because a past run of that kind succeeded (`/api/local-runner/harness-verification`) is removed. |
| 6 | **The agent's own setup reaches the run; nothing else of it does (ADR 0077 holds).** Claude Code and Codex keep their sandbox binds (`~/.claude`, `~/.codex`) and read their own login and settings. opencode keeps its scratch `HOME`, because a measured merge showed that its injected config cannot remove the person's MCP servers or permission allows; it gets its own login file (`auth.json`) bound in, and the `provider`, `model` and `small_model` keys copied from `opencode debug config` into the config Tack injects, with Tack's permissions and no external plugins (`--pure`). docket reads its setup from `DOCKET_SETUP_HOME`, bound read-only to the files docket names. An agent whose key lives only in a shell variable does not get it: the runner forwards no new variables, and the docs say to use the agent's own login or config file. |
| 7 | **docket runs from contract 1.2 (docket 0.2.0-beta.5).** Contract 1.2 lets `docket exec` read the endpoint, credential and default model from the person's own `docket setup`, makes `--model` optional, and adds an offline setup report (ready, default model, models, providers). Tack probes for it by name, as it probes 1.1; a docket without it reads **Doesn't run here: needs docket 0.2.0-beta.5 or later**. There is no interim path through `DOCKET_LLM_BASE_URL`. |
| 8 | **The default model is the project's, for its default agent, or the agent's own.** A run's model is the one it names; otherwise the project's default model when the run uses the project's default agent; otherwise none (the agent's default). Changing the project's default agent clears its default model. The agent-profile and fleet `default_model` tiers are removed: a model string belongs to one agent, and neither a profile nor a fleet names one. |
| 9 | **The scheduler matches the agent, not the model.** `ModelSelector`, `model_combinations` and the "model combination not declared" reason are removed. A request that names no model goes to an agent whose probe says it needs one (`requires_model`) only to fail eligibility with "<agent> needs a model named". |
| 10 | **prill gets the model, not a family.** The verifier's evidence carries `generator.model` and no `generator.family`: the provider Tack used as a stand-in no longer exists. |

If you accept this table, you have accepted the ADR. Everything below is evidence.

---

## What this supersedes

- **ADR 0061**, decisions 1–5 (a provider key held by the runner, the route that hands it one,
  the runner fetching the catalog, the gateway's own name, the console/UI table as applied to
  keys). Decision 6 (the UI switch that starts the embedded runner) stands.
- **ADR 0063**, decisions 1–7 (two credential modes, the provider trait, the catalog and its
  metadata). Its "subscription" mode is now the only mode, and it is the agent's business.
- **ADR 0067**, decisions 2, 4 and 10 (opencode's injected provider, its config replacing the
  person's, its own logins out of scope).
- **ADR 0074** decision 1's "optional gateway key"; **ADR 0075** decisions 3 and 7
  (`[provider.openai_compatible]`, the Automation panel's `{provider, model_id}`).
- **ADR 0050**'s "requested and actual harness/provider/model use distinct fields" becomes one
  model string; its "no Tack model gateway" is now literally true.

## Evidence (measured 2026-10-13 on the maintainer's machine)

| Agent | Version | Lists its models | Says whether it is signed in |
|---|---|---|---|
| Claude Code | 2.1.291 | No command. | `claude auth status` prints JSON offline: `loggedIn`, `authMethod`, `apiProvider`, `subscriptionType`. |
| Codex | 0.149.1 | `codex debug models` prints `{"models":[{slug, display_name, visibility, …}]}`; 5 entries, 2 with `visibility: list`. `--bundled` skips the refresh. | `codex login status` prints "Logged in using ChatGPT". |
| opencode | 1.18.30 | `opencode models` prints one `provider/model` per line; 86 lines from the person's own providers. | `opencode auth list` names each stored credential and its kind (`oauth`, `api`); `opencode debug config` prints the resolved config with its `provider` and `model` keys. |
| docket | 0.2.0b4 | `docket setup provider list` prints the catalog (built-in and global). No machine-readable model list yet. | No offline check yet; `docket exec` refuses without `DOCKET_LLM_BASE_URL` ("harness mode never falls back to a stored provider"). |

**opencode's config merge.** With `OPENCODE_CONFIG_CONTENT='{"mcp":{},"plugin":[],"permission":{…},"model":…}'`
and the person's real config present, `opencode debug config` shows `model` and `plugin`
replaced, but all five of the person's MCP servers still present and the person's permission
allows (`external_directory: allow`, …) still in force beside the injected ones. Pointing opencode
at the person's own config would therefore hand the agent what ADR 0077 took away; copying only
`provider`, `model` and `small_model` into a scratch `HOME` does not.

**What the provider layer's catalog reached.** `model_metadata` (price, context window,
modality) and `requested_not_confirmed` are rendered by no component under `frontend/src`.
The gateway catalog was attached to every agent whose wire the gateway serves, unfiltered.

**What is removed.** About 4,500–5,500 lines outright (the provider module, its config and
tests, the gateway step of `scripts/smoke.sh`, `embedded_runner_live_secret.rs`, the key panel),
plus the mechanical change from a `(provider, model_id)` pair to one string across the wire, the
database, the scheduler, the API, the CLI, MCP and the frontend. The inventory is in
`docs/closed-cycles/plans/phase-71.md`.

## Documentation links the model field uses

Each was fetched (HTTP 200) on 2026-10-13; the plan re-checks them before the UI ships.

| Agent | Link |
|---|---|
| Claude Code | https://code.claude.com/docs/en/model-config |
| Codex | https://developers.openai.com/codex/models |
| opencode | https://opencode.ai/docs/models/ |
| docket | https://github.com/yielab/docket/blob/develop/docs/CONFIGURATION.md#31-choose-which-model-each-agent-runs-on (models), https://github.com/yielab/docket/blob/develop/docs/MODEL-GATEWAYS.md (sign-in), fetched 2026-10-13 |
