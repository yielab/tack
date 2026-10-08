# Agent Runners & Fleet Execution

Tack can hand a board item to a coding agent — codex, Claude Code, docket or
opencode — and track what it did as first-class project history: requested vs. actual
harness/model, a fenced execution attempt, an event timeline, decisions the agent needs
answered, and verified artifacts it produced. This page is the operator's guide to that
surface: which harness to pick, running the `tack-runner` binary, enrolling and revoking
runners, where credentials and workspaces live, what a runner can honestly promise (the
capability matrix), and how version compatibility and network exposure work.

If you only want to press **Run with agent** on a project that has a folder, you do not
need most of this page: see [Running an item with an agent](#running-an-item-with-an-agent)
and [Where the agent works](#where-the-agent-works). The question every new user asks next —
*which agent, which model, and where do I put the key* — has a direct answer below, not a
negation: see [Choosing a harness](#choosing-a-harness) and
[Choosing a model and a provider](#choosing-a-model-and-a-provider). The four ways to
turn a board item into a completed attempt are in
[Running an item with an agent](#running-an-item-with-an-agent), before the operational
detail (enrollment, credentials, recovery) that follows it.

Docket's own control-plane history — retired, and what replaced it — is in
[Docket compatibility](#docket-compatibility) below.

**Read this before you rely on it in production:** the last section,
[What actually runs today](#what-actually-runs-today), states plainly which parts of
this pipeline are proven end-to-end against a real router and which parts stop at the
enrollment step in the current build. Every claim on this page cites the test that
proves it.

---

## Do you need a runner?

Only if you want an item's **Run with agent** button to do something. Tack is two
things bundled into one binary: a project manager that always runs, and an agent
executor that is off until something turns it on. Nothing below requires a runner:

| Works with zero runners | |
|---|---|
| Board, timeline, dashboard, list, calendar views | ✅ |
| Creating, moving, commenting on, searching items | ✅ |
| The CLI, REST API, and `tack mcp` for everything above | ✅ |
| Webhooks, GitHub sync, backups | ✅ |

| Needs an active runner (embedded or remote) | |
|---|---|
| Clicking **Run with agent** on an item | ❌ shows "Agent execution is off" instead of a form |
| A harness (Claude Code, Codex, docket or opencode) actually running against your code | ❌ nothing to run it |

Nothing silently queues forever: the button tells you there's no runner to give the
request to, rather than accepting one that no runner will ever claim.

**The analogy, if you've used a CI runner (GitHub Actions, GitLab):** the board is the
pipeline definition and its history — it always exists, whether or not anything is
attached to run it. **The runner is the runner** — a separate worker that polls for
eligible work, executes it near your own code and credentials, and reports back. Zero
runners attached is a normal, fully working state: the board just has nothing to hand
work to yet.

Three ways to attach one, in order of effort:

| | How | When |
|---|---|---|
| **Embedded, from the first second** | `tack serve --with-runner` | One developer, one machine — the fastest path to a completed attempt. |
| **Embedded, turned on later, no restart** | Agents page (`/agents`) → **Turn on** | You started plain `tack serve` and changed your mind. Same runner as above, same process, flipped on live — a flag and this toggle are two doors to the identical on/off switch. |
| **A separate `tack-runner` process, enrolled against this board** | [Enrolling a runner](#enrolling-a-runner) below | **Required** for a shared or production deployment not bound to `127.0.0.1` — an embedded runner refuses to start there on purpose. Executing arbitrary agent processes on a machine reachable from outside the loopback interface is the one thing this product refuses to do silently; see [Non-loopback and security posture](#non-loopback-and-security-posture). |

**"Run with agent"** (the button on an item) and **`--with-runner`** (the boot flag) are
two different things that happen to sound alike: the button always exists; the flag —
or the Agents-page toggle, or a separate `tack-runner` — is what gives it something to
run against.

---

## Concepts

| Term | What it is |
|---|---|
| **Execution request** | A durable, idempotent record: "run this item through this harness, on this runner or fleet, with this agent profile." Created via `POST /api/executions`. |
| **Execution attempt** | One numbered try at a request. Owns a fencing token, a lease, and an isolated workspace. A request can have more than one attempt over its life (retries, recovery). |
| **Runner** | A `tack-runner` process, identified by a durable `runner_id`, enrolled once and then polling for work. |
| **Runner fleet** | A named group of runners sharing an optional concurrency limit and default policy. An execution request targets either one exact runner or a fleet. |
| **Agent profile** | Reusable instructions + tool policy + limits, snapshotted into the request at creation time so later edits to the profile never change history. |
| **Harness** | The coding-agent CLI a runner can launch: `codex`, `claude-code`, `docket` or `opencode` — see [Choosing a harness](#choosing-a-harness). The wire value is an opaque string, so a runner may report a kind this build has no bundled adapter for. |

`Harness` ≠ `ModelProvider` ≠ `ModelId`, and `Item` ≠ `ExecutionRequest` ≠
`ExecutionAttempt` — these stay distinct on the wire and in the database on purpose;
see `docs/contracts/runner-v1/protocol.json`.

### What the harness receives

Whoever creates the request, the server writes the instructions the harness is given: the
agent profile's own text, then the item's title and description, then the item's brief
(its definition of done, acceptance criteria and constraints) when it has one. The title
and description are labelled with where they came from and marked as data to work from, not
as instructions that change the agent's rules, because an imported item's text was written
by someone else. The brief is also saved with the request, and the attempt keeps a copy of
it as `brief.json` beside its `evidence.json`. An item without a brief still reaches the
harness with its title and description.

---

## Choosing a harness

Tack drives a coding-agent CLI — it never runs a model itself. Four are supported today:
`codex`, `claude-code`, `docket` and `opencode`. Pick one by what it needs installed, how
it reaches a model, and how far it lets Tack steer it — the rest of this page assumes
you've already picked one.

| Harness | Install | Reaches a model through | What it measures | Pauses to ask you | Honours the permission policy | Can't |
|---|---|---|---|---|---|---|
| `codex` | The official Codex CLI, e.g. `npm install -g @openai/codex` | Its own login (ChatGPT/API-key session) needs no Tack configuration. A Tack-configured Vercel AI Gateway key also reaches it, over the OpenAI Responses wire — Anthropic's own API doesn't serve that wire, so it's never an option here. | Tokens, read from the run's own terminal line (`exec --json`'s `turn.completed`). Cost is never measured: no such field exists in the output. The model that actually served the request is never confirmed — Tack records the one you requested, tagged `requested_not_confirmed`. | **Yes** — over `codex app-server`, and only when the request's permission policy sets `approvals` to `ask` and a command needs to escalate beyond the sandbox | Advisory — your tool list picks codex's sandbox mode (a write-capable tool grants `--sandbox workspace-write`, otherwise `--sandbox read-only`); your network flag and budget never reach it | Resume a session, report a cost, confirm which model served a request, or honour a network deny or a budget |
| `claude-code` | `npm install -g @anthropic-ai/claude-code` | Its own login (a Claude subscription or its own API key) needs no Tack configuration. A Tack-configured endpoint can be Anthropic's own API directly, or the Vercel AI Gateway — both over the Anthropic Messages wire. | Tokens and cost, from the run's own result line — advisory, because an auxiliary model's cost is folded into the total while token counts were only confirmed to cover the primary turn. The served model is confirmed from its own session-start line on a direct connection; routed through the gateway, it's recorded `requested_not_confirmed` instead, since a gateway can still substitute a model underneath it. | **Yes** — before every tool call, when the request's permission policy sets `approvals` to `ask` | Advisory — the tool list and a cost budget are enforced through its own flags; a network deny only blocks the WebFetch/WebSearch tools by name | Reattach to an already-running attempt (`--resume` starts a new process against stored history, not the in-flight one), or guarantee a network deny holds against everything it runs |
| `docket` | From the [docket project](https://github.com/yielab/docket), following its own install instructions | Always needs a Tack-configured endpoint — the Vercel AI Gateway, over the OpenAI Chat Completions wire; Anthropic's own API doesn't serve that wire either. It has no login of its own that this adapter uses. | Tokens, genuinely read from its result line, and a served model that's a real observation of the endpoint's own response — never downgraded to `requested_not_confirmed`, even behind a gateway. Cost is never measured: the installed version always reports it `null`. A recipe run has no single turn to read a served model from, so none is recorded for it. When the installed docket reports the files a run wrote, they appear in the attempt's result (paths under `.tack-runner/` are left out). | **Yes, when the installed docket accepts answers** — `tack runner doctor --json` shows it as `decisions: supported` under docket. Then, when the request's permission policy sets `approvals` to `ask`, docket pauses before each tool call its own policy gates and waits for your answer; a call its policy allows runs without a question. A docket that doesn't accept answers refuses a gated call immediately instead | Depends on the installed docket. When it accepts a policy, your tool list and network flag reach it as a policy blocking every docket tool you don't allow, and your token budget reaches it as its own limit — except on a recipe run, which takes no token limit. A docket without those flags is passed none of them and applies only its own policy engine | Resume, report a cost, take a token budget on a recipe run, or run a recipe with an Implementer step (Tack passes no verify command, so that step fails) |
| `opencode` | `brew install opencode` | Always needs a Tack-configured endpoint — its own vendor logins are out of scope for this adapter — the Vercel AI Gateway, over the same OpenAI Chat Completions wire as docket. | Tokens, summed across every step of the run. The served model is never confirmed — every run is recorded `requested_not_confirmed`, because nothing in its output names what actually answered. Cost is never measured for a model it doesn't recognize. | **Yes** — but only over `opencode acp`, and only when the request's permission policy sets `approvals` to `ask`: each granted tool then pauses on its own request until answered | Advisory — network access and per-tool access (edit/bash/task) are each gated through its own permission block; a budget is never passed | Confirm which model served a request, or run at all against a request that denies network — see below |

A few things above are easy to trip over:

- **opencode reaches the npm registry on every attempt**, not just a first run. Even a
  fresh, isolated config directory makes a real connection to `registry.npmjs.org` and
  installs about 220 MB before it does anything else. Because Tack can't make opencode
  keep a promise the CLI itself breaks, a request whose permission policy denies network
  is rejected before opencode ever spawns.
- **docket reads its key from one fixed variable.** Whatever provider you configure,
  Tack injects its resolved credential under `DOCKET_LLM_API_KEY` — docket never sees a
  provider-named variable, and it refuses to start without one.
- **docket is asked what it can do when the runner starts.** Tack asks the installed docket
  which harness contract it speaks (1.0 or 1.1) and which optional flags it accepts, using calls
  docket refuses before doing any work: no model is called and nothing is written. Tack never
  reads docket's version number to decide anything, because an install's version string can be
  stale while its code is current. What it found is what everything below follows from, and
  `tack runner doctor` prints docket's version and, with `--json`, its `decisions` entry, which
  names the contract it negotiated.
- **docket gets your limits, policy and a list of the files it wrote, when it can take them.**
  When the installed docket accepts them, the run's token budget goes to docket as its own limit,
  and the run's tool list and network setting go to it as a policy that blocks every docket tool
  the run does not allow; an empty list blocks all of them. Only docket's own tool names can be
  used there (`read`, `write`, `edit`, `glob`, `grep`, `bash`, `fetch`, `skill`, `consult`): a run
  that names any other tool, or allows `fetch` with network off, is refused before it starts,
  with the field named, never quietly narrowed. docket then reports the files the run changed,
  and they appear in the attempt's result (files under `.tack-runner/` are left out). A docket
  without these flags gets none of them and says so: its permission policy and artifacts are
  `unsupported`, never half-supported.
- **docket can run a named recipe instead of a single task.** Put the recipe's name in the agent
  profile's tool policy, as `{"docket": {"recipe": "research-review"}}`. When the installed docket
  supports recipes, Tack passes `--recipe` and docket runs that recipe in place on the workspace.
  docket refuses a recipe run that also carries an agent id or a token limit, so Tack leaves both
  off: a recipe run takes no token budget. A recipe with an Implementer step needs a verify
  command, which Tack does not pass, so that step fails with `verifyCmd required but not set`;
  recipes without one (for example `research-review`) run to the end. The result's `task` block,
  with each step's role and outcome, is kept in the attempt's result.
- **codex maps your tool list onto its sandbox mode, and says what it still can't.**
  A tool list granting `bash`, `shell`, `edit`, `write` or `apply_patch` runs codex under
  `--sandbox workspace-write`; an empty or read-only tool list runs it under
  `--sandbox read-only`. Under "Automatic" the sandbox never prompts on a denial — a write it
  disallows just fails inside the tool call. Under "Ask me" codex runs as `codex app-server`
  instead and pauses only for a command that needs to escalate beyond that sandbox; a command
  inside it still runs without asking. Your network flag and budget never reach it either way:
  codex has no flag for either.
- **claude-code's network deny is narrower than it sounds.** `network: false` only
  blocks the WebFetch and WebSearch tools by name; its Bash tool can still reach the
  network if your tool list grants it, so a network-sensitive item needs its tool list
  narrowed too, not just the network flag.

### Asking before acting

By default an agent decides everything on its own: every tool call, every file it
touches. Setting a request's `permission_policy.approvals` to `ask` changes that. The
harness pauses before each tool call and waits for a person to answer. In the **"Run with
agent"** dialog this is the "Approvals" choice, "Automatic" or "Ask me"; the CLI and the
API set it directly (`--permission-policy
'{"tools":[...],"network":...,"approvals":"ask"}'`). Unset, or `"auto"`, never pauses.

Claude Code, codex and opencode honour `ask`, and so does docket when the installed one accepts
answers. Each one pauses at a different point: claude-code before every tool call, opencode
before every call to a tool your list grants, codex only for a command that needs to escalate
beyond its sandbox, docket before each call its own policy gates (a call its policy allows runs
without a question). Which dockets accept answers is decided when the runner starts, by asking the
installed one — see the docket notes above. For a harness that does not honour `ask` the dialog
disables "Ask me" and says why, and a request built by hand that asks anyway is never scheduled
onto a runner that would ignore the choice and run as `auto`. A docket without answer support
still refuses a gated call outright.

With docket, choosing "Ask me" pauses the agent before each gated call and waits for your answer
in Tack. Allow lets the call run; Deny refuses it, and the agent is told it was refused and
carries on. This works the same whether docket is running a single task or a recipe.

A question waits on the item's Execution tab. Each option can carry a description, its risks and
an estimate in tokens; the option the agent recommends is marked, with its reasoning and the files
it looked at.

<img src="../screenshots/decision-inbox.png" width="50%" alt="A pending question on the Execution tab: the agent asks which chart library to add, offers uPlot, Apache ECharts and hand-written SVG, each with a description, a risk in red and an estimate in tokens, and marks uPlot as Recommended with its reasoning and evidence files; a Details field and a Resolve button sit below.">

The question appears in the attempt's decision inbox, in the same item view as the run's
timeline and artifacts. Answering it needs `TACK_EXECUTION_DECISION_TOKEN`, a secret
separate from the ordinary API token. A question nobody answers before the attempt's
deadline is answered with its own deny option. It is never left open and never defaults
to allow.

Each question shows its kind, so a tool permission reads differently from a choice the
agent wants you to make. An option can carry a short description, the risks of choosing it
and a rough token cost. When the agent has a preferred answer, that option is preselected
and marked **Recommended**, with the reason and any evidence beneath it; nothing is applied
until you press **Resolve**, and you can pick any option. The first time someone opens a
pending question, the board records that it was seen.

### Checking a machine

Run `tack runner doctor` on the machine that will actually run the harness before
trusting anything above about your own install — it probes every harness this build has
an adapter for, with no runner enrollment and nothing written to the board. For each
harness it prints whether the binary was found and its version, one line on how it
authenticates, and whether it accepts an operator-chosen model verbatim
(`model_passthrough`) or only a declared list (`model_combinations`). Condensed from a
real run on a machine with all four installed:

```text
codex
  status:      present
  version:     0.149.1
  credentials: Codex authenticates itself (its own CLI login, or an API key it reads
               from its own config)...
  model_combinations: (none reported)
  model_passthrough: supported — requested_model_id is forwarded verbatim via --model...

claude-code
  status:      present
  version:     2.1.273
  ...

docket
  status:      present
  version:     0.2.0b1
  ...

opencode
  status:      present
  version:     1.18.30
  ...

Runner-wide capabilities (apply identically to every harness above):
  cancel     advisory    — a process-group signal cannot reach a detached descendant unless the harness reports its process groups; see each harness's own cancel capability
  resume     unsupported — no resumable session contract
  decisions  supported   — at least one harness adapter opens a decision; see each harness's own decisions capability for which one
  artifacts  advisory    — uploaded when an adapter stages one; best-effort, not replayed on restart
  usage      advisory    — usage is reported only when a harness emits it

Secret store (resolves `secret_reference` environment entries):
  backend: keychain
  ...

Provider endpoint (vercel_ai_gateway):
  reaches: codex, claude-code, docket, opencode
  status:  not configured

Provider endpoint (anthropic):
  reaches: claude-code
  status:  not configured

verify: disabled
```

The last line reports the runner's verifier: `disabled`, or `enabled` with the program, whether
it was found on `PATH`, its arguments and its timeout. See
[After a succeeded attempt](#after-a-succeeded-attempt).

A harness absent from your machine prints `status: absent` and names every directory it
searched, instead of the fields above — see
[Where Tack looks for a harness binary](#where-tack-looks-for-a-harness-binary) below.

The **runner-wide capabilities** block is a single, deliberately conservative statement
that applies to every harness alike — it is not where claude-code's ability to pause and
ask shows up. `decisions: supported` here only means the protocol path exists on this
runner at all; *which* harness can actually use it is a per-harness fact, set by that
request's own `permission_policy.approvals` and recorded in the attempt's own capability
snapshot at claim time, not in this enrollment-time summary. Reading `decisions:
supported` here does not mean every harness can pause; see the table above for which
one actually does.

How a provider's endpoint and credential are configured — the `TACK_RUNNER_PROVIDER_*`
variables, which secret-store entry each one reads, and the embedded runner's own key
field — is the single authority in `docs/CONFIG.md`, not restated here.

---

## Running an item with an agent

There are four ways to turn a board item into an execution request. All four produce
the identical `POST /api/executions` record underneath — none is more "real" than
another. For a UI-only user the path is short and has three parts. Turn agents on for this
computer on the **Agents** page (`/agents`), see [Agents on this computer](#agents-on-this-computer).
Tell the project where its code is, either when you create it ("Does this project have
code?") or later under **Settings → Automation**, see [Automation](#automation). Then press
**Run with agent** on a task. The dialog reads the project's settings back and needs no
hand-typed identifiers. The worked example further down uses the CLI, because every field
it sends is visible on the command line; see
[Quick Start](quick-start.md#run-an-item-with-an-agent) for the UI-first walkthrough.

| Entry point | Where | Notes |
|---|---|---|
| The **"Run with agent"** dialog | Item detail drawer, web UI | One summary line and two switches (see below). It takes the computer, the profile, the agent's model and the place the agent works from the project's settings, and blocks with a named reason (and, where one exists, a link to fix it) instead of submitting a request that would queue forever. |
| `tack execution create` | CLI | Scriptable; every field the API accepts is a flag. Used for the worked example below. |
| `POST /api/executions` | Raw HTTP | Same JSON body the CLI sends. See [API Reference](https://github.com/yielab/tack/blob/develop/docs/API-REFERENCE.md#worked-examples) for a worked request/response pair. |
| MCP `create_execution` | `tack mcp`, for an agent driving Tack itself | Same required fields as the REST call. See the [MCP guide](https://github.com/yielab/tack/blob/develop/docs/MCP.md). |

### The Run with agent dialog

The dialog is a pre-flight, not a form. Everything that belongs to the computer or the
project is already decided and shown read-only, with a **Change** link to the page that owns
it, so nothing is asked twice:

<img src="../screenshots/run-with-agent.png" width="60%" alt="The Run with agent dialog: one summary line saying which agent runs the task, in which profile, on which model and where it works; the section What the agent will read; two switches, Ask before each action and Allow network access; and a collapsed Advanced section.">

- **The summary line** — which agent runs it, as which profile, on which model, and where it
  works ("on a new branch of your repo", "in the folder, directly"). Each part links to where
  you change it.
- **What the agent will read** — the real text it will be given: the profile's instructions,
  the task's title and description, its acceptance criteria, the project's **Definition of
  done**, and anything in the task's collapsed **For the agent** block.
- **Ask before each action** — off by default. When on, the agent pauses before each action and
  waits for your answer (see [Asking before acting](#asking-before-acting)). For an agent that
  cannot pause, the switch is disabled and says why.
- **Allow network access** — off by default.
- **Advanced** — the computer, the agent, the model, the allowed tools, the timeout and the
  after-run steps (verify, push). A step that is not available is never hidden: it is shown
  disabled with the reason beside it, and **Verify the result** and **Push the branch** say
  which section of the runner's TOML config turns them on (see
  [After a succeeded attempt](#after-a-succeeded-attempt)). Opening a pull request is not chosen
  per run: it is opened for you once a pushed branch's item is linked to a GitHub issue.

Settings you change here for one task are saved on the task, and settings you change on the
project stay on the project, so running the same task again does not make you retype them.
A field that cannot work says so in a sentence with a link where you typed it (the folder
does not exist, the branch is not in the repository, the model is not on the agent's list),
rather than leaving **Run** disabled with no explanation.

### Writing a task the agent can use

The task form has the fields a person would put in any tracker. The **Description** is the
story. **Acceptance criteria** is one sentence per line, and each line becomes a checklist
item the review can tick off. What only the agent needs — automatic checks, limits, risk —
sits in a collapsed **For the agent** block, which you can ignore. The project carries a
**Definition of done** once (under **Settings → Automation**), and every run reads it.
All of this is stored as the item's [brief](items.md#brief-tab); the dialog's **What the
agent will read** shows exactly what it will be given.

### What happens when a run finishes

A run that left changes marks the task **Needs your review**. This is a flag on the task,
separate from your board columns, and it clears when you press **Accept** or **Reject** on the
attempt. If the project chose a column for finished runs (under **Settings → Automation**), the
task also moves there.

The attempt card leads with the outcome and the tools to check it:

- **Open diff** — what changed.
- **Open folder** (or Copy path) — where the work is on this computer.
- **Copy branch** — the branch name, when the agent worked on a branch.
- **Result** — what the agent said it did, and why it stopped.
- **Log** — the full log.

**Succeeded** is green only when the changes were captured. It is amber when the agent changed
nothing, or when the changes could not be read. A failed run leads with why it failed.

#### A Planner run

A task run with the **Planner** profile changes no files. Its attempt shows **The Planner
proposes N subtasks**, each with a checkbox and its title, description and acceptance criteria
editable. Nothing is created until you press **Create selected**: no agent writes to your
board.

### The same run from the CLI

The rest of this section is one complete run through the CLI path, executed against a
real `tack serve --with-runner` with a stand-in `claude` binary standing in for a real,
authenticated install (so it costs nothing and never leaves the machine) — every id and
every line of output below is copied from that run, not constructed from the schema.
Swap the stand-in for a real, logged-in harness and the same commands reach the same
place against real work.

**Before this:** a project and an item exist (`tack init`, `tack add`), and either a
real enrolled runner is polling (see [Enrolling a runner](#enrolling-a-runner) below) or
an embedded one is running (`tack serve --with-runner` — see
[Standalone mode](#standalone-mode-tack-serve---with-runner) below). `POST
/api/executions` requires all thirteen fields shown below; the CLI fills in an
`idempotency_key` (a fresh UUID) and empty objects for `budgets`/`environment`/`metadata`
if you omit them, so five of the thirteen are effectively optional in practice.

Create an agent profile — its instructions and limits are snapshotted into every
request created against it:

```sh
tack agent-profile create "demo-profile" \
  --instructions "Print the single word DONE and exit. Do not modify any files."
```

```text
Created agent profile: demo-profile (ap_91b4e)
  id: ap_91b4ea76-9f1a-4725-8a58-21a57d92572c
```

Find the runner to target. An embedded runner self-provisions under a name starting
`local-`; there is no `tack runner list` CLI subcommand today (see
[Enrolling a runner](#enrolling-a-runner)), so read it back over the API:

```sh
curl -s http://127.0.0.1:3210/api/runners | jq -r '.data[].runner_id'
```

```text
runr_8fa0dfb9-638f-492d-b278-ee06a789ad04
```

Now the full request, all thirteen required fields:

```sh
tack execution create <ITEM_ID> \
  --idempotency-key "release-notes-001" \
  --runner runr_8fa0dfb9-638f-492d-b278-ee06a789ad04 \
  --agent-profile ap_91b4ea76-9f1a-4725-8a58-21a57d92572c \
  --harness claude-code \
  --model-provider anthropic \
  --model-id claude-sonnet-4-5 \
  --agent-profile-snapshot '{"name":"demo-profile","instructions":"Print the single word DONE and exit. Do not modify any files.","tool_policy":{},"timeout_seconds":120,"budgets":{}}' \
  --repository '{"kind":"git","remote":"/path/to/local/repo","base_revision":"ce38796ef4f70db35eeb8d6d8b8e86477e1a883c","subdirectory":null}' \
  --permission-policy '{"tools":[],"network":false}' \
  --timeout-seconds 120
```

```text
Created execution request: exec_0fe
  state: queued
  id:    exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

Poll for the terminal state — a fresh embedded runner claims and completes an attempt
against a stand-in harness in well under a second:

```sh
tack execution get exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```text
Execution request exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
  item:    e1d5e03d-4610-45c2-b5f1-835e69f148a7
  state:   succeeded (done)
  created: 2026-09-03T19:00:16.697646760+00:00
```

That is a completed attempt, reached with the commands above and no browser. For the
attempt-level detail `tack execution get` does not surface — fencing token, workspace
id, the staged artifact, the harness's own capability report at claim time — use
`GET /api/executions/{request_id}/attempts`:

```sh
curl -s http://127.0.0.1:3210/api/executions/exec_0fe7252989.../attempts | jq '.data[0]'
```

```json
{
  "attempt_id": "att_fea7528f-853b-4c3c-a2d6-d1cf1b904e48",
  "state": "succeeded",
  "fencing_token": 1,
  "workspace_id": "ws_6174745f66656137353238662d383533622d346333632d613264362d643163663162393034653438",
  "actual_execution": {
    "harness_kind": "claude-code",
    "model_provider": "anthropic",
    "model_id": "unknown",
    "model_observation_source": "not_observed"
  },
  "terminal_reason": {
    "exit": "Exited(0)",
    "reason": "no structured result envelope was produced; inferred success from exit code 0",
    "artifact": { "kind": "log", "name": "claude-code-run.log", "size_bytes": 52 }
  }
}
```

Two things in that real output are worth reading closely rather than past: `model_id`
came back `"unknown"` with source `not_observed` — the stand-in binary used for this
page never echoes a model id the way a real harness does, and Tack reports that
honestly rather than assuming the requested id was actually the one that ran (the same
"not measured, never a fabricated value" rule as
[usage economics](#usage-economics-and-not-measured), applied to model identity instead
of cost). And `terminal_reason.reason` came from an *inferred* exit code, not a
structured result — a real harness's own structured output, when it produces one, is
read instead; see [What actually runs today](#what-actually-runs-today).

## Agents on this computer

The **Agents** page (`/agents`) is about this computer only. It does not hold anything that
belongs to a project.

<img src="../screenshots/agents.png" width="60%" alt="The Agents page: a switch for agent execution on this computer, which agents are installed and when each last ran here, a paragraph about logins and the optional gateway key, and a Run test button.">

- **Agents are on or off.** One switch for this computer. Turning it on starts the embedded
  runner without a restart (see [Standalone mode](#standalone-mode-tack-serve---with-runner)).
- **Which agents are installed**, and when each last ran here. Tack looks where a terminal
  would (see [Where Tack looks for a harness binary](#where-tack-looks-for-a-harness-binary)).
- **Logins and the optional gateway key.** An agent that you are already signed in to in your
  terminal needs nothing from Tack. If you would rather not rely on that login, you can paste a
  Vercel AI Gateway key; it lives in this computer's own secret store and never in the board's
  database (see [Local credential handling](#local-credential-handling)).
- **Run test.** One button that checks that an installed agent starts and answers on this
  computer. It names no model, so it uses an agent that runs with its own default (Claude Code
  today); when every installed agent needs a model named, the button is disabled and says so.

The default profile, the model, where the agent works and what happens after a run are
project settings, in [Automation](#automation).

---

## Automation

**Settings → Automation** is the project's tab for everything about running agents on it.
A value lives at the highest layer that can own it: the computer (the Agents page), the
project (this tab) or the task (the dialog). The dialog shows project values read-only, with
**Change**.

<img src="../screenshots/automation.png" width="60%" alt="The Automation tab of a project's settings: where the code is, how the agent works on it, the push switch, the default profile, the column a finished run moves the task to, and the Definition of done.">

- **Where the code is** — the answer to **Does this project have code?** from when you
  created the project: an existing folder on this computer, a new folder Tack creates, or no
  code yet. Under **Advanced** there is also a URL option, for a repository the agent should
  clone.
- **How the agent works on it** — see [Where the agent works](#where-the-agent-works). The
  default is chosen from the folder.
- **Push** — a switch, off by default. See [Where the agent works](#where-the-agent-works).
- **Default profile** — which of the [profiles](#profiles) a new run uses. Implementer unless you
  change it.
- **When a run finishes** — optionally the column the task moves to, in addition to
  **Needs your review**.
- **Definition of done** — one text the agent reads on every run of this project.
- **Agent** — which installed agent a new run uses. Each pill is an agent a runner on this
  computer reports.
- **Model** — **The agent's default (recommended)**, or **Specific model**: an id from the
  short list measured for that agent, sent with the provider the agent speaks natively. An
  agent with no measured list yet (codex, opencode) keeps its default here; the dialog's
  **Other…** is where you type an id for one run.

## Where the agent works

Where the agent works is a project setting with three modes. Tack picks the first two for
you from your folder. You never type a repository address for code on this computer.

| Mode | What it does | When it is the default |
|---|---|---|
| **On a new branch of your repo** | Makes a worktree of your repository on a branch named `tack/<task>-a<n>`. The branch stays in your repository after the run. Your own checkout, your files and your index are untouched. | A folder that is a git repository. |
| **In the folder, directly** | The agent edits your folder itself, as if you had run it in a terminal. Nothing is ever deleted or moved, and your git index is untouched. | A folder without git. |
| **In a fresh clone per run** | Clones the repository into a new workspace for each run. | Never the default. It is under **Advanced**, for a URL or for a runner on another computer. |

For a git folder, the first mode is the default because it costs nothing (no network, shared
objects), keeps the result in your repository, and does not build on top of whatever is
half-done in your checkout. **In the folder, directly** is one click away. In a folder without
git it is the only mode.

**Push is off by default.** It is a switch per project. Turning it on pushes the run's branch
to the repository's remote, using your own git credentials, and it needs push configured on the
runner (see [After a succeeded attempt](#after-a-succeeded-attempt)). If the project has push on
and the runner does not, the run records "push skipped" instead of failing. The work is not lost
either way: in the first mode it is on the local branch.

### Profiles

A profile is what the agent is told to be and which tools it may use. Four are built in:

| Profile | What it does |
|---|---|
| **Implementer** (default) | Reads, edits and runs commands. |
| **Reviewer** | Reads and reports. Changes nothing. |
| **Researcher** | Reads and searches, and writes a report. |
| **Planner** | Plans and designs, and proposes subtasks you can accept. |

You pick one with the pills on the task; each pill has a tooltip saying what it may do. You edit
a profile in a form, never as JSON, and a profile you change is snapshotted into each request
when it is created, so editing it later never changes the history of an earlier run.

---

## Choosing a model and a provider

**You usually choose nothing.** The default is **the agent's default (recommended)**: the
model the agent itself uses when you run it in a terminal. Tack passes no model and no provider
and the agent decides. If you want a specific model, pick one from the short list the dialog
offers for that agent. For `claude-code` the list is the model ids that were measured to work.
`codex` and `opencode` offer **Other…** only, where you type the model id. You never type a
provider: it is the agent's own, or the gateway when you have set a gateway key.

The rest of this section is the precedence behind that, for anyone driving Tack from the CLI
or the API. Two rules sit next to each other, on purpose, because conflating them is what causes
confusion: **the board never holds a provider
credential** — no `TACK_*` variable configures a vendor API key, and the API server
itself never proxies a model call (ADR 0050, ADR 0058) — and **the runner can hold
one**, on its own machine, in its own secret store, and use it to reach a model
provider on the board's behalf. The two statements are about different halves of the
product: see [Local credential handling](#local-credential-handling) below and
`docs/CONFIG.md`'s "Embedded runner" section, "Vendor/provider credentials" bullet, for
exactly where a credential may and may not live (ADR 0061 draws that line and bounds it).

Separately, and independently of where a credential lives, **Tack routes the model
choice**: which `(provider, model_id)` pair reaches the harness for a given request is
resolved server-side, deterministically, before the request is ever offered to a
runner. Routing a choice and holding a secret are different things — this section's
four-tier precedence below is the routing rule; the runner-side gateway path (the
fastest UI-only route to a working key) is in
[Local credential handling](#local-credential-handling).

### The four-tier precedence

`crates/tack-orch/src/model_policy` resolves a request's model in this order, most
specific first, stopping at the first tier that has a value:

1. **Request override** — `requested_model_provider`/`requested_model_id` on the
   request itself (`--model-provider`/`--model-id` on the CLI, the `POST
   /api/executions` body fields of the same name). Set by an explicit CLI flag, a raw
   API caller, or the dialog's model picker when you pick a specific model instead of the
   agent's default.
2. **Agent-profile default** — a `{"default_model": {"provider": "...", "model_id":
   "..."}}` object inside the agent profile's own `limits` field (`POST
   /api/agent-profiles --limits '...'` or `tack agent-profile create --limits '...'`).
   Live-verified: creating a profile with this default and a request that omits
   `--model-provider`/`--model-id` entirely still resolves and completes:
   ```sh
   tack agent-profile create "sonnet-profile" \
     --instructions "..." \
     --limits '{"default_model":{"provider":"anthropic","model_id":"claude-sonnet-4-5"}}'
   # then tack execution create <item> --agent-profile ap_6e5649b8... --harness claude-code ... (no --model-provider/--model-id)
   ```
   the resulting attempt's `actual_execution.model_provider` came back `"anthropic"` —
   resolved server-side from the profile, never supplied on the request.
3. **Project default** — `projects.default_model` (migration 062), the same
   `{"default_model": {"provider": ..., "model_id": ...}}` (or literal `"auto"`)
   convention as the other tiers, set with `PATCH /api/projects/{id}`.
   `resolve_request_model_policy` reads it as this tier
   (`crates/tack-orch/src/model_policy/wiring.rs`).
4. **Fleet default** — the same `{"default_model": {...}}` convention, inside
   `agent_fleets.default_policy` (`tack fleet create --policy '...'`). Applies only to a
   request that targets the fleet itself (`selector_kind: "fleet"`) — an
   `exact_runner`-targeted request never consults any fleet's policy, since it never
   names one. Live-verified the same way as tier 2: a fleet created with
   `--policy '{"default_model":{"provider":"anthropic","model_id":"claude-opus-4-1"}}'`,
   a request targeting `--fleet <that fleet>` with an agent profile that has no default
   of its own, resolved to `actual_execution.model_provider: "anthropic"`.
5. **The agent's default** — what happens when every tier above is empty. This is not a
   fallback that quietly picks something: see the next section.

All four tiers are resolved once, server-side, at request-creation time
(`crates/tack-api/src/handlers/executions.rs`, immediately before the request is
stored) — only when the caller supplied neither `requested_model_provider` nor
`requested_model_id`; an explicit pair (even a deliberately wrong one) is never
second-guessed by a lower tier.

### When no model is named: the agent's default

A request created with no model in any tier above is valid for an agent that reports it needs
no model. The scheduler asks the agent's own adapter, and when the adapter says its model is
optional, the request goes to the runner without a model and the agent uses its own default.
Today that is true for `claude-code`.

`codex`, `opencode` and `docket` still need a model named. A request for one of them with no
model in any tier stays `queued` rather than being guessed at, so give those agents a model in
one of the tiers above (for `codex` and `opencode`, **Other…** in the dialog).

### What a runner will actually accept

An explicit `(provider, model_id)` pair is eligible to schedule when either the target
harness's capability snapshot **declares** that exact pairing in `model_combinations`,
or the harness attests `model_passthrough: supported` (the adapter forwards the
operator's opaque model id verbatim and the harness validates it at its own run time).
`model_passthrough: advisory` is treated as unverified and rejected exactly like
`unsupported` — a capability claim below `supported` is not load-bearing
(`crates/tack-orch/src/scheduler/select.rs`). Run `tack runner doctor` on the actual
runner host to see this for real rather than trusting a stale copy of this page — the
full command and its unabridged output are in the [CLI reference](cli.md#runner), and
what the rest of its output means is in
[Choosing a harness](#choosing-a-harness). Condensed from a real run on a machine with
all four harnesses installed:

| Harness | `model_combinations` | `model_passthrough` |
|---|---|---|
| `codex` | (none reported) | supported |
| `claude-code` | (none reported) | supported |
| `docket` | (none reported) | supported |
| `opencode` | (none reported) | supported |

None of the four has a `list-models` command to probe, so every one declares zero
combinations and relies entirely on passthrough — any operator-specified model is accepted
pre-spawn and only the harness itself validates it at run time. A harness that can
enumerate its own installed/configured models would declare them instead and refuse
passthrough — rejecting an undeclared model before any process spawns rather than after
— but no adapter in this tree does that today.

---

## Enrolling a runner

Enrollment is operator-initiated and two-step: the operator creates a *pending* runner
and gets a one-time token; the runner (or whoever configures it) redeems that token.

```sh
tack runner enroll my-first-runner \
  --total-capacity 2 \
  --available-capacity 2
```

```text
Enrolled runner: my-first-runner (runr_d91da686-...)
  token id:   ent_da7943b1-...
  expires at: 2026-08-19T16:17:51Z
  enrollment token: enr_57cd3592-...
  (shown once — copy it into the runner's TACK_RUNNER_ENROLLMENT_TOKEN now;
   it cannot be retrieved again)
```

The raw `enrollment_token` is generated by the server in this one response and is
**never stored anywhere retrievable again** — only its SHA-256 hash is kept
(`crates/tack-api/src/handlers/runner_admin.rs`). If you lose it, revoke the token
(`tack runner revoke-token <runner-id> <token-id>`) and enroll again. Pass `--out
<path>` to write the response straight to an owner-only file instead of printing the
token to the terminal — useful when handing enrollment off to a provisioning script:

```sh
tack runner enroll ci-runner-1 --total-capacity 1 --available-capacity 1 \
  --out /secure/place/ci-runner-1.enrollment.json
```

Verified: `tack runner enroll` against a live server returns a `runner_id`,
`token_id`, and one-time `enrollment_token`, and a subsequent
`GET /api/runners` shows the runner in `pending_enrollment` state (there is no
`tack runner list` CLI subcommand today — only `enroll`/`revoke`/`revoke-token`; use
`curl` or the Agents page to list runners) — reproduced by hand for this page and
pinned by `crates/tack-api/tests/handlers/production_router.rs` and the runner-lifecycle handler tests
in `crates/tack-api/src/handlers/runner_admin.rs`.

### Redeeming the token (the runner side)

The runner process exchanges the enrollment token for a durable credential the first
time it starts:

```sh
export TACK_RUNNER_API_URL=https://tack.example.com/api/runner/v1
export TACK_RUNNER_ID=runr_d91da686-...
export TACK_RUNNER_STATE_DIR=/var/lib/tack-runner
export TACK_RUNNER_ENROLLMENT_TOKEN=enr_57cd3592-...
tack-runner
```

Prefer the environment variable over `--enrollment-token`: the flag exists but the CLI
help deliberately notes the env var keeps the secret out of shell history and process
listings. `RunnerConfig::require_enrollment_credential()` fails closed with a typed
error before any filesystem or network side effect if no credential is configured —
`crates/tack-runner/src/config.rs`.

### Revoking a runner

```sh
tack runner revoke runr_d91da686-...
```

Revocation is immediate: the runner's hashed credential stops authenticating on the
next request. It does not retroactively invalidate an already-issued lease's fencing
token — a mid-flight attempt still needs the ordinary lease/lifecycle machinery (see
the [recovery runbook](recovery-runbook.md)) to reach a terminal or `needs_operator`
state. Revoke an unredeemed enrollment token without touching the runner record with
`tack runner revoke-token <runner-id> <token-id>`.

---

## Standalone mode: `tack serve --with-runner`

Everything above assumes two processes: an operator running `tack serve`, and a
separate `tack-runner` process enrolled against it by hand. `tack serve --with-runner`
(or `TACK_LOCAL_RUNNER_ENABLE=1`) collapses that to one binary and one command — the
common case of one developer running an agent against their own board. It is off by
default; the full configuration reference (gate, state directory, provider-credential
story per harness) is `docs/CONFIG.md`, section "Embedded runner". This section covers
what changes about the enrollment/credential story above when you use it.

- **Not a shortcut.** The embedded runner runs as a task inside the server's own
  process, but speaks runner-v1 over loopback HTTP exactly like a remote runner — the
  same client, the same server-side path, the same contract fixtures. See
  `docs/adr/0058-standalone-single-binary-runner.md`.
- **Zero-touch enrollment on a fresh state directory.** The first start with no stored
  session self-provisions: it creates its own pending-runner row and redeems its own
  one-time token in-process. No `tack runner enroll` call is needed, and no token is
  ever printed, copied, or configured by hand. A second start against the same state
  directory reuses the stored credential on disk instead of provisioning a second
  runner — no second pending-runner row, no second token.
- **Loopback-only, refused before anything opens.** `TACK_HOST` must be a loopback
  address for `--with-runner` to start at all; a non-loopback bind is refused as a
  startup error — before any socket or database is opened — never downgraded to a
  runner-less server. Verified by `crates/tack-cli/src/local_runner.rs`'s
  `embedded_runner_refuses_non_loopback_bind` test and, live, by `scripts/smoke.sh`
  step 12: a real attempt to bind non-loopback with `--with-runner` exits non-zero
  naming "loopback", and no listener is ever opened on the refused port.
- **Off by default.** Plain `tack serve` — no flag, no environment variable — starts no
  runner at all. Verified live by `scripts/smoke.sh` step 11, which queries
  `GET /api/runners` directly after a settle window and finds it empty, not merely the
  absence of a log line.
- **Proven end to end.** `scripts/smoke.sh` step 10 drives `tack serve --with-runner` on
  a fresh state directory with no token, through self-provisioning, an active runner,
  and a real completed attempt, using the same fake-harness shim pattern the rest of the
  smoke script uses.
- **Log visibility.** The embedded runner's own log lines are silent under default
  logging (a pre-existing tracing-filter gap, not specific to this mode) — see
  `docs/CONFIG.md`'s "Embedded runner" section for the exact `RUST_LOG` setting that
  surfaces them.

Everything else on this page — the capability matrix, workspace/artifact storage,
`needs_operator` recovery, version compatibility — applies identically whether the
runner is embedded or a separate process; the wire contract does not know the
difference.

---

## Where Tack looks for a harness binary

The Agents page's "harness detected" check, and the runner's own startup probe, resolve
`codex`, `claude`, `docket` and `opencode` the same way: the runner process's own `PATH`
first, exactly as a shell would find it. If that search comes up empty, the runner then
checks a fixed list of
per-user install locations that a shell-launched terminal usually has on `PATH` but a
desktop launcher (a `.desktop` entry, Finder, the Start menu) or `tack service` under
systemd's user manager usually does not, since both start from a minimal session
`PATH`:

- `~/.local/bin`
- `~/.cargo/bin`
- `~/.bun/bin`
- `~/.npm-global/bin` and `~/.npm/bin`
- every installed Node version's own bin directory under `~/.nvm/versions/node/`
- `/opt/homebrew/bin` and `/usr/local/bin` (Homebrew)
- on Windows, `%APPDATA%\npm` and `%LOCALAPPDATA%\nvm`

This list is fixed, not configurable — there is no `TACK_*` variable for it. A machine
whose shell can already see the install is unaffected: `PATH` is always searched first,
and the first match wins. If neither search finds the binary, the reported error names
every directory it actually checked, so "not installed" always comes with a way to
confirm it — install the harness anywhere on that list, or make sure it is on the
`PATH` the runner process actually inherits.

---

## Local credential handling

- The enrollment token is a bearer secret; the durable credential the runner receives
  after redeeming it is a second, longer-lived bearer secret. Both are **hashed at
  rest** — the server never stores either in a form it could hand back to you.
- `EnrollmentCredential`'s `Debug` and `Display` implementations are hardcoded to
  print `[REDACTED]` — this is structural, not a logging convention that a future
  `println!` could bypass by accident (`crates/tack-runner/src/config.rs`).
- The runner's on-disk state directory (`TACK_RUNNER_STATE_DIR`, default
  `.tack-runner`) holds the attempt journal (below) and, since ADR 0061, a
  **runner-local secret store** for a configured provider's key — the platform
  keychain where one answers, an owner-only file otherwise, never `app_meta` or any
  other server-visible table. Set a value with `tack runner secret set <name>`
  (reads `TACK_RUNNER_SECRET_VALUE` or, failing that, stdin — never a CLI argument,
  which `/proc` would make world-readable), or, when the runner is embedded in the
  same process as the board (`tack serve --with-runner`), from the **Agents** page's
  own key field. A harness's own vendor login (Claude Max, `codex login`, ...) needs
  none of this — it is still the runner operator's own local environment, unmanaged by
  Tack either way. In every case, **Tack's API and database never see, store, or
  forward a credential value** — only the runner's own store resolves one, at spawn
  time, into the one subprocess that needs it.
- Logs never carry the credential value. Redaction is tested with a positive control:
  the redaction tests capture real `tracing` output and assert the secret marker
  never appears **and** that an id does appear, ruling out "the capture rig just
  isn't observing anything."

---

## Workspace and artifact storage

Each execution *attempt* gets one isolated workspace — a dedicated worktree, never
shared across attempts or requests (`crates/tack-runner/src/workspace.rs`). The
`WorktreeProvisioner` trait is the only boundary allowed to create it; tests inject a
fake so unit tests never touch a real git checkout.

**On the runner side**, before any harness process is allowed to start, an owner-only
journal record is written to `TACK_RUNNER_STATE_DIR` describing the workspace path and
base revision (`crates/tack-runner/src/journal.rs`). This durability-before-spawn
ordering is what makes crash recovery possible — see the
[recovery runbook](recovery-runbook.md).

**On the server side**, verified artifacts a runner uploads (logs, diffs, generated
files — anything the harness produced worth keeping) are streamed to
`<TACK_STORAGE_DIR>/execution-artifacts`, a dedicated subtree kept apart from ordinary
item attachments (`<TACK_STORAGE_DIR>` itself) so the retention sweep documented below
can never touch the wrong directory
(`crates/tack-api/src/router.rs`, `with_artifact_storage_root`). Download is
`GET /api/executions/{request_id}/attempts/{n}/artifacts/{artifact_id}/content`, under
ordinary operator auth. `GET .../attempts/{n}/artifacts` and `GET .../attempts/{n}/decisions`
list what an attempt produced without a caller already knowing an id — the UI's
artifact panel and decision inbox both read these lists directly now.

Content integrity is checksum-verified end to end:
`execution-attempt-detail.spec.ts` uploads real content through a real runner
credential, computes its own sha256, downloads it back through the UI via a real
Playwright download event, and asserts the downloaded bytes equal the uploaded bytes
exactly.

---

## Capability matrix

Every runner reports a **capability snapshot** at enrollment and refresh time —
protocol version, concurrency, installed harness versions, and per-feature support for
`cancel`, `resume`, `decisions`, `artifacts`, and `usage`, each as
`unsupported | advisory | supported` with an optional reason
(`docs/contracts/runner-v1/capabilities.json`). The scheduler and UI are required to
read this snapshot rather than assume a feature works.

**The one enforced rule:** an adapter may claim `cancel: supported` only if its harness
announces the process groups its shell tool starts. `AdapterRegistry::register_probe` rejects
any other probe that does, at registration time, before any attempt can reference it —
`crates/tack-runner/src/harness/mod.rs`, proved by
`harness::tests::registering_a_probe_overclaiming_cancel_support_is_rejected`. The reason is
structural: a harness's own shell tool spawns its subprocess in a new session outside the
runner's process group, so without being told the group the runner cannot promise it stopped
(confirmed with `ps` against each real harness installation; see each harness's own
`fixtures/<kind>/README.md`). Only docket announces its bash calls' process groups
(`process_started` / `process_exited`), and only when its negotiated contract is 1.1, so
`cancel` is `supported` for docket on 1.1 and `advisory` otherwise and for the other three
harnesses. On a cancellation the runner stops the main group, then sends SIGTERM and, after the
grace period, SIGKILL to each announced group still live. The cancellation evidence carries
`details.groups` as `{tracked, killed, survived}`. A harness that announces nothing always shows
`tracked: 0`, which means nothing was reported, not that nothing was left.

| Feature | Ceiling in this build | Why |
|---|---|---|
| `cancel` | `supported` for docket on contract 1.1, `advisory` for everything else | Only a harness that announces its process groups can be held to stopping them — `AdapterRegistry::register_probe` rejects a stronger claim from any other, before any attempt can reference it |
| `resume` | adapter-reported | No harness in this build declares a resumable session contract |
| `decisions` | adapter-reported | Runner-driven bounded decisions (`POST .../decisions`) work when the harness supports them — claude-code, codex and opencode, and docket when the installed one accepts answers, and only when the request asks for it; see [Asking before acting](#asking-before-acting) |
| `artifacts` | `supported` for docket when it reports the files a run wrote, `advisory` otherwise | The other harnesses cannot guarantee artifact discovery; downgraded from an earlier `supported` claim |
| `usage` | `advisory` | Token totals may be absent from harness output; see [usage economics](#usage-economics-and-not-measured) |

---

## `needs_operator` and recovery

`needs_operator` is an explicit, non-automatically-retryable state for an attempt
whose ownership after a crash or network split cannot be proven safe. Tack never
blindly launches a second process against the same workspace. Full mechanics —
including exactly which recovery observations lead where — are in the
[Recovery Runbook](recovery-runbook.md); the short version:

1. A restarted runner reads its own journal and reports what it actually observed —
   `ProcessStopped`, `ProcessRunning`, or `Ambiguous`
   (`POST /api/runner/v1/attempts/{id}/recovery-observation`).
2. The server computes a disposition: `SafePreSpawnRequeue` (nothing had started yet —
   automatically safe), `NeedsOperator` (anything else), or `AlreadyTerminal`.
3. An operator resolves a `needs_operator` request explicitly, with an audited reason:

```sh
tack execution reconcile <request-id> \
  --recovery-key <key> \
  --reason "confirmed process was killed via ps on the runner host"
```

This calls `POST /api/executions/{id}/requeue`, which only succeeds for a request the
server itself authoritatively recovered — an unresolved or already-queued request
returns `409 invalid_transition` with the current state named in `details`, never a
silent no-op. Verified by hand against a live server (an unresolvable request returns
`{"code":"invalid_transition","details":{"from":"unknown","to":"queued"}}`) and pinned
by `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`.

---

## Version compatibility

Every runner-v1 request body carries `"protocol_version": 1`. The server rejects
anything else outright — `check_protocol_version` in
`crates/tack-api/src/handlers/runner_protocol.rs` returns `unsupported-protocol`
(`docs/contracts/runner-v1/errors/unsupported-protocol.json`) for any value other than
the literal integer `1`, on every one of the 13 runner-protocol operations. There is
no negotiated fallback: a v2 protocol, if it ever exists, is a new contract revision,
not a version range this server tries to interpolate.

Separately, each runner reports its own `runner_version` (a free-form string in the
capability snapshot) and each harness reports its `installed_version` — both are
informational, logged and stored, never used to gate behavior today.

---

## Docket compatibility

Docket is reached the same way as any other coding agent: as a harness
(`--harness docket`) on a runner-v1 execution request, through the API and
CLI surface described on this page — see [Choosing a harness](#choosing-a-harness) for
what it needs installed and what it can and can't do. The earlier, separate Docket
control-plane bridge — a standing background poller with its own dispatch,
approval and budget routes — has been retired; Docket carries no special
scheduling path or API surface of its own anymore.

---

## Non-loopback and security posture

Runner-protocol routes (`/api/runner/v1/*`) are mounted as a structural **sibling** of
the operator `/api` router, not nested inside it — they never traverse
`require_token` (the operator bearer-token gate) at all, and each handler
authenticates its own hashed runner credential independently
(`crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`). This is deliberate:
a future edit that widens the operator auth exemption list cannot accidentally loosen
runner auth, because runner auth was never implemented as an exemption in the first
place.

For everything else about exposing Tack beyond `127.0.0.1` — TLS termination behind a
reverse proxy, `TACK_ALLOWED_ORIGINS`, `TACK_API_TOKEN`, request body limits — see
[Administration and Security](administration.md), which applies identically whether or
not any runner is enrolled. The one addition specific to this domain:
`TACK_EXECUTION_DECISION_TOKEN` is a **separate, higher-privilege secret** layered on
top of `TACK_API_TOKEN`, fail-closed when unset (the route rejects rather than silently
falling back to the ordinary operator token) and never logged. See `docs/CONFIG.md` for every `TACK_*` variable in this
domain.

`tack serve --with-runner` (see "Standalone mode" above) adds a second, stricter
loopback requirement on top of all of this: an embedded runner executes arbitrary
coding-agent processes on the same host serving the UI, so `--with-runner` itself
refuses to start unless `TACK_HOST` is a loopback address — independent of, and checked
before, whichever `TACK_API_TOKEN`/`TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` posture
the server would otherwise apply.

---

## After a succeeded attempt

What a runner does once an attempt has succeeded, and before its workspace is deleted. Every
step below is the runner's, on its machine; the board never runs a program or pushes a branch.
None of them can change the attempt's outcome: if one fails, the attempt is still `succeeded` and
the failure is recorded as an event on its timeline.

**Evidence.** For every harness, and also for a cancelled attempt, the runner records what the
attempt changed against the revision it started from. Three artifacts are uploaded:
`changes.patch` (the full diff, cut at 8 MiB with a flag in the manifest when it is longer),
`files.json` (each path and whether it was added, modified, deleted or renamed) and
`evidence.json` (the manifest: the base and head commits, the patch's checksum and size, the
files, the terminal reason and the usage). When the request carried a brief, `brief.json` is
kept beside them. If git cannot read the workspace, only `evidence.json` is written and it says
why. The shape is `docs/contracts/evidence-v1/`.

**Verification.** A runner with a `[verify]` section enabled in its TOML config runs a program
of yours over that evidence and uploads the merge-readiness pack it writes as one more artifact.
It is off by default. A failing, missing or timed-out verifier records `attempt.verify_failed`
and nothing more. The pack is what you read under
[Reviewing a merge-readiness pack](#reviewing-a-merge-readiness-pack). The section is described in
`docs/CONFIG.md`; `tack runner doctor` reports whether it is on and whether the program is found.

**Branch push.** A runner with `push_branches = true` under `[git]` commits what the harness left
uncommitted onto a branch named `<prefix><item short id>-a<attempt number>` and pushes it to the
remote the repository was fetched from, using your own git credentials. It is off by default. A
failed push records `attempt.push_failed`. Turn it on only for agents you would trust with those
credentials; `docs/CONFIG.md` says what the push does and does not guard against.

**Pull request.** When the attempt pushed a branch, its item is linked to a GitHub issue and a
GitHub token is configured, the server opens a pull request from that branch, with the
merge-readiness pack as its body. The attempt then shows a **PR #n** link and a badge: open,
merged, closed or reverted. The state follows GitHub on the sync poll; see
[GitHub sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md). An item with no link, or no token, gets no pull request
and nothing else changes.

**Moving the item.** A request can carry a status policy: `done_on_success` moves the item to
its workflow's first Done status when the attempt succeeds, and `done_on_mrp_accepted` moves it
when you accept the attempt's merge-readiness pack. Without one the item's status is never
touched. Set it with `tack execution create --status-map-policy` or `status_map_policy_id` in the
API and MCP; the Run with agent dialog does not offer it. The move goes through the workflow's
ordinary rules and open boards are told, like any edit.

---

## Reviewing a merge-readiness pack

When a verifier checks an attempt, it leaves a merge-readiness pack. Open the attempt's
details on the Execution tab and the pack appears under "Merge-readiness pack".

<img src="../screenshots/merge-readiness.png" width="50%" alt="A merge-readiness pack on a succeeded attempt: Recommendation merge, Risk low, three reasons, a criteria table with every criterion passed and its evidence file, the verify command with exit code 0, mutation score, static analysis counts, a blind judge's verdicts, and Your review with a required reason before Accept or Reject.">

From top to bottom it shows the verifier's recommendation and the risk level with its
reasons, then one row per criterion from the brief (id, kind, status, evidence), the
verify command with its exit code and output, mutation results, static-analysis counts and
the judge's verdict on each criterion.

A section the verifier did not run says **Not run**. That is not a pass: nothing was
checked.

To record your verdict, write a reason, then choose **Accept** or **Reject**. Both stay
disabled until the reason has text. The first review is final; a second one is refused.
If the run was created with the `done_on_mrp_accepted` status policy, accepting also moves
the item to its workflow's first Done status.

---

## Usage economics and "Not measured"

`usage_economics.runner_time_cost.cost_usd_estimated` is **always**
`{value: null, source: "not_measured"}` in production — no runner infra cost-rate is
stored anywhere in this schema, so there is nothing honest to compute it from. The UI
renders the literal string `Not measured`, never `$0.00`: rendering zero would claim
the run was free, which is not known to be true.
`attemptFormat.test.ts#formatUsdMeasurement` asserts the exact string, that it does
**not** contain `$0`, and — as a positive control — that a real
`{value: 0, source: "measured"}` renders the genuinely different `"$0.00 (measured)"`,
proving the two cases are distinguished rather than both collapsing to one string.

Token usage, when the harness reports it, is `measured`. When it doesn't, it is
`not_measured` — never a structural zero standing in for "unknown."

---

## Factory metrics

The Factory metrics page (**Factory metrics** in the project's sidebar, at
`/projects/<id>/factory`; `GET /api/projects/{id}/metrics/factory` for the same numbers) shows one card per metric from an aggregated summary of all
attempts, decisions, and verification activity for a project. The page covers all time; the endpoint takes an optional `since`
(RFC 3339) to start the window later. Each card displays the
computed ratio or value (when it can be measured), or "Not measured" with its reason when
measurement is not possible.

| Metric | Definition | Source |
|---|---|---|
| Escalation rate | Decisions ÷ Attempts | `execution_decisions` ÷ `execution_attempts` in the window |
| Human minutes per decision | Median and p90 of the time a decision waits for an answer | From when someone first opened the decision (or from when it was raised, if nobody opened it first) to its resolution; `execution_decisions` in the window |
| Human minutes per pack review | Median and p90 of the time a merge-readiness pack waits for its verdict | From when someone first opened the pack (or from when it was uploaded, if nobody opened it before the verdict) to the review; `mrp_reviews` in the window |
| Verification tax (tokens) | (Verification + Rework) ÷ Implementation | Implementation = tokens from each request's first attempt; Rework = tokens from later attempts plus requests marked as rework; Verification = the tokens each verifier reported spending in its pack |
| Pack acceptance rate | Accepted ÷ Reviewed | Counts from `mrp_reviews`; also shows unreviewed and produced packs |
| Outcomes | Pull requests opened, merged, closed, reverted | Data from `pull_requests`; also shows PQC (pull request quality: merged and not reverted) |

When a metric has nothing to count, the card says "Not measured" and why, never 0. No
card shows money: token counts only. When timing data has no resolved samples in the window, it shows "Not
measured" rather than zero minutes. The source column for each metric shows which table
it reads and, for time-based metrics, whether resolution time started from `viewed_at`
(human first opened it) or `created_at` (never viewed before resolution).

---

## Known gaps

These are documented rather than papered over, per this project's
"unsupported is typed, unknown is explicit" rule:

- **`execution_requests` has no real `priority` column.** A `metadata`-convention
  stopgap exists, documented as non-binding.
- **A docket run that times out is not cancelled.** Cancelling stops every process group docket
  announced; a timeout stops only docket's main process group, so a command docket started in its
  own group can outlive the attempt.
- **`tack runner doctor` does not print each harness's `artifacts` and `permission_policy`
  support** in its readable output; for docket those are decided when the runner starts.
- **The branch push runs git in the attempt's workspace**, which the agent could have
  reconfigured while it worked. Hooks are disabled; see `docs/CONFIG.md`.

---

## What actually runs today

Read this section before treating any earlier claim on this page as a promise about a
live, end-to-end run.

**Proven end-to-end against the real production router** (`build_router`, no
card-local test scaffolding): creating an execution request, scheduling it to an exact
runner or a fleet, the full runner-v1 protocol surface (enroll → claim → heartbeat →
accept → start → events → decisions → artifacts → completion), cancellation and
recovery-observation handling, the decision-resolve and artifact-download routes, and
the retention/expiry sweeps — see `crates/tack-api/tests/handlers/production_router.rs`,
`crates/tack-orch/tests/runner_contract.rs` (byte-pins all 46 frozen fixtures), and the
integrator test files referenced throughout this page.

**The `tack-runner` binary's own network transport is wired and proven.**
`tack_runner::bootstrap::build_runtime` (the one composition root both the standalone
`tack-runner` binary and the embedded
`tack serve --with-runner`/`tack runner start` paths call — see "Standalone mode"
above) wires the real `HttpPullProtocol`, not `UnavailableProtocolClient`.
`UnavailableProtocolClient` still exists in the crate as an explicit no-client
fallback — reachable only if something constructs a runtime without attaching a
protocol client — and stays pinned by
`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` precisely so that
path fails as a typed `RunnerError::ProtocolUnavailable` rather than a silent success;
it is not what a real `tack-runner` startup wires today.

Live, end-to-end proof of the real client: `crates/tack-runner/tests/
bootstrap_entrypoint.rs` proves the composition root itself is reachable and
shutdown-controllable from outside the crate against a real (mocked) HTTP enrollment
exchange. Every run of `scripts/smoke.sh` exercises the real client against a real
server: steps 6-9 enroll a separate `tack-runner` process and drive a real attempt
through claim → checkout → harness → completion, restart recovery included; step 10
does the same inside a single `tack serve --with-runner` process. **A fresh-machine
operator today can enroll a runner and point a real `tack-runner` process (standalone
or embedded) at a real server and watch it enroll, claim, and complete real work.**
