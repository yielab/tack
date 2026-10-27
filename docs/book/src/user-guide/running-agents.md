# Running Agents on Tasks

This page is for anyone who wants an agent to work on a task. After reading it you can turn agents on
for your computer, pick an agent and a model, start a run from a task or from the command line, and answer
the questions an agent asks while it works. Installing runners for a team, or running Tack on a server,
is the Administrator Guide's job: see [Installing and Operating Runners](../admin-guide/runners.md).

## Do you need an agent?

Only if you want a task's **Run with agent** button to do something. Tack is a project manager that
always runs and an agent executor that stays off until you turn it on. Everything else works without one.

| Works without an agent | Needs an agent on this computer |
|---|---|
| Board, list, table, calendar, timeline and dashboard views | The **Run with agent** button on a task |
| Creating, moving, commenting on and searching tasks | An agent such as Claude Code or Codex working on your code |
| The CLI and `tack mcp` for all of the above | A task that starts by itself ([Automating a Project](automation.md)) |
| Webhooks, GitHub sync and backups | |

With no agent turned on, **Run with agent** says "Agent execution is off" instead of showing a form. Tack
never queues a request that nothing could ever take.

Turning agents on for one computer is a switch on the Agents page. A shared server needs a separate
runner instead, which is an administrator's task: see
[Installing and Operating Runners](../admin-guide/runners.md).

---

## Agents on this computer

The **Agents** page (`/agents`) is about this computer only. It holds nothing that belongs to a project.

<img src="../screenshots/agents.png" width="60%" alt="The Agents page: a switch for agent execution on this computer, which agents are installed and when each last ran here, a Sign in step with one row per installed agent, and a Run test button.">

1. **Turn agents on.** One switch for this computer. It takes effect at once, with no restart.
2. **See which agents are installed**, and when each last ran here. Tack looks where a terminal would
   look; if an agent is installed somewhere unusual, see
   [Where Tack looks for an agent](../admin-guide/runners.md#where-tack-looks-for-a-harness-binary).
3. **Sign in.** One row per installed agent, asked of the agent itself: "Signed in", or "Not signed in.
   Run `<command>` in a terminal." with a link to that agent's own documentation. Tack holds no model key.
   Each agent signs in and reaches its model with its own tools.
4. **Run test.** Checks that an installed agent starts and answers. It names no model, so it uses an agent
   that runs with its own default (Claude Code today). When every installed agent needs a model named,
   the button is disabled and says so.

The default agent, the model, where the agent works and what happens after a run are project settings,
described in [Automating a Project](automation.md#the-projects-other-settings).

---

## Choosing an agent

Tack drives a coding-agent program; it never runs a model itself. Four are supported: Claude Code, Codex,
opencode and docket. Pick one by what it needs installed, how you sign in, and whether it can pause to ask
you before acting.

| Agent | Install | Sign in | Can pause to ask you |
|---|---|---|---|
| Claude Code | Its own install, see [Claude Code](https://code.claude.com/docs/en/authentication) | `claude`, with a Claude subscription or its own API key | Yes, before every tool call |
| Codex | Its own install, see [Codex](https://developers.openai.com/codex/auth) | `codex login` | Yes, but only for a command that needs more than its sandbox allows |
| opencode | Its own install, see [opencode](https://opencode.ai/docs/providers/) | `opencode auth login` | Yes, before every tool your list allows |
| docket | Its own install, see the [docket project](https://github.com/yielab/docket) | docket's own sign-in | Yes, before each call its policy gates |

`tack runner doctor` shows, per agent on a machine, whether it is found and signed in and which models it
offers; see [CLI](cli.md#runner).

### Details per agent

- **Claude Code.** Reports the tokens it used, how many model calls it made and every model it touched. It
  also reports its own cost estimate at list price, which is an estimate and not your bill. It honours the
  tools you allow; when the run names none, the profile's list for Claude Code is used. Every tool means
  its file, shell and web tools, not its sub-agents or schedules, and it is not asked to sign its commits as
  Claude's. Tack stops it when the run's counted tokens reach the token limit. A run that turns web tools off blocks only Claude Code's own web fetch
  and search tools by name. Its shell can still reach the network if the allowed tools include it, so
  narrow the tool list too for a sensitive task.
- **Codex.** Reports tokens, not cost. Allowing a tool that writes files runs Codex in its
  workspace-write sandbox; allowing none, or only read tools, runs it read-only. With **Ask before each
  action** off, a write the sandbox forbids simply fails inside the run. With it on, Codex pauses only for a
  command that needs to escape the sandbox; a command inside the sandbox runs without asking. Codex has no
  switch for web tools or a token limit, so neither reaches it.
- **opencode.** Reports tokens, summed over the run, with what its cache served counted apart. Tack stops it when the tokens it has used reach the run's token limit. A model that refuses the run, for example because its key has expired, fails the attempt with the model's own sentence. It reaches the npm registry and installs packages before
  it does anything else, on every run, so a request that turns the network off is refused before opencode starts. A run gets
  its own copy of your opencode sign-in and model settings; its other settings (extra tool servers,
  plugins, permissions) do not reach the run.
- **docket.** A run goes to a team. Each docket team is a long-lived group with its own members, models,
  checks and way of working, kept by docket itself. Tack sends the task to the team, follows what the team
  does, shows its steps on the task's timeline, relays its questions to you, and cancels it if you do.
  A profile names its team; a profile with no team uses a team named for the project's folder, which Tack
  creates when it does not exist. docket works on a new branch of your folder: choose that under
  [Where the agent works](automation.md#where-the-agent-works). docket keeps its own sign-in and
  confines its agents itself, so Tack passes it no model key. The task carries your denied tools and web
  setting, your token limit, its type and priority, and the acceptance criteria that name a command or a
  test, which become the team's gate. A team that needs a person during a run limits how far Tack may merge
  its work by itself: see [The ceiling](automation.md#ceiling). The administrator sets up docket on
  the runner and needs docket 0.2.0-beta.6 or later; see
  [Installing and Operating Runners](../admin-guide/runners.md).

---

## Choosing a model

**You usually choose nothing.** The default is **Agent's default**: the model the agent itself uses when
you run it in a terminal.

A run's model is one optional string in the agent's own spelling: `sonnet` for Claude Code,
`gpt-5.6-terra` for Codex, `anthropic/claude-sonnet-4-5` or `local/qwen` for opencode. Tack passes it to the
agent and never interprets it. The **Model** field in the run dialog and in a project's Automation settings
shows:

- **Agent's default**, when the agent does not need a model named;
- the models the agent listed, or the ones its documentation suggests;
- **Another model…**, a text field for any name.

A line under the field says where the list came from ("From Codex on this computer") or, when the agent
cannot list its models, how to name one, with a link "How `<agent>` names models". Tack never checks a typed
name; the agent checks it when the run starts.

| Agent | Models it offers here | Its documentation |
|---|---|---|
| Claude Code | Suggested: `sonnet`, `opus`, `haiku` | [Model configuration](https://code.claude.com/docs/en/model-config) |
| Codex | The entries `codex debug models` lists | [Codex models](https://developers.openai.com/codex/models) |
| opencode | `opencode models`, from the providers you signed in to; with no sign-in, only opencode's free models | [Models](https://opencode.ai/docs/models/) |
| docket | None listed; type a name | [docket](https://github.com/yielab/docket) |

### Which model a run gets

The first of these that has a value wins:

1. **The run's own model**: the dialog's model field, or `--model <name>` on the command line.
2. **The project's default model**, when the run uses the project's default agent. Set it in
   **Settings → Automation**. Changing the project's default agent clears it.
3. **The agent's own default.**

The attempt card shows which models answered. When the agent named the model that answered, the card says
so. When it only echoed the request, the card shows the model you asked for and does not claim more.

### When no model is named

A run with no model is fine for an agent that does not need one: Claude Code and Codex use their own default,
and so does opencode when its own settings name one. docket needs a model named, and so does opencode when
its settings have none. A run for such an agent with no model waits, and the task says why ("Waiting: docket
needs a model named"). If a runner looks at it and finds it cannot take it, and nothing changes for 30 minutes, it fails with that reason.

A run waiting for a computer that has stopped answering says so with the time it last did: "The computer this run asked for has not answered since" followed by that time. It waits until the computer answers again, or you run it on another computer.

---

## Running an item with an agent

There are four ways to start a run, and they all make the same request.

| Where | Use it for |
|---|---|
| The **Run with agent** dialog on a task | Everyday use |
| `tack execution create` | Scripts; see [Running from the command line](#running-from-the-command-line) |
| The REST API | Integrations; see the [API reference](../developer/api-reference.md) |
| The MCP tool `create_execution` | An AI assistant driving Tack; see [MCP Server](../developer/mcp.md) |

For the dialog the path is short. Turn agents on for this computer ([Agents on this
computer](#agents-on-this-computer)). Tell the project where its code is, either when you create the
project ("Does this project have code?") or later under **Settings → Automation**
([Automating a Project](automation.md)). Then press **Run with agent** on a task.
[Quick Start](quick-start.md#run-an-item-with-an-agent) walks through it with screenshots.

### The Run with agent dialog

The dialog is a pre-flight, not a form. What belongs to the computer or the project is decided and shown
read-only, and each part links to the page that owns it, so nothing is asked twice.

<img src="../screenshots/run-with-agent.png" width="60%" alt="The Run with agent dialog: one summary line saying which agent runs the task, in which profile, on which model and where it works; the section What the agent will read; two switches, Ask before each action and Web tools; and a collapsed Advanced section.">

- **The summary line** says which profile runs the task, on which model, and where the agent works ("on a
  new branch of your repo", "in the folder, directly"). Each part links to where you change it.
- **Where the profile came from** appears under the summary: "Chosen by the role X", "Project default",
  "Saved on this task", "The built-in Implementer", or "More than one role has a profile, so the project
  default is used". To use another profile for this run, press another name in the **Profile** row.
  [Teams](automation.md#teams) explains how a role chooses.
- **The profile row follows the profile.** The profiles you made come first, each with the line saying
  what it delivers, and the built-in ones sit apart below. Pressing a profile also picks the agent it is
  written for (a profile with a docket team means docket; otherwise the agent it lists tools for), when
  this computer offers that agent, and a line under the row says so when it replaces the agent the task
  had. A profile that runs no commands says "No install: this profile runs no commands" where the dialog
  tells what is installed before the agent starts.
- **Machine or group.** Lists this computer and the other machines by name, then the groups. A machine that
  has stopped answering is shown disabled, with the time it was last heard. With one machine answering and
  no group there is nothing to choose, so the field is one line, "Runs on this computer".
- **What the agent will read** shows the real text it will be given: the profile's instructions, the task's
  title and description, its acceptance criteria, the project's **Definition of done**, and anything in the
  task's collapsed **For the agent** block. See [Working with Items](items.md#acceptance-criteria-and-for-the-agent).
- **Ask before each action.** On when the agent is not sandboxed, off otherwise. When on, the agent pauses
  and waits for your answer: see [Asking before acting](#asking-before-acting). For an agent that cannot
  pause, the switch is disabled and says why.
- **Web tools.** Off by default, unless the profile says its role needs the network (the profile's box on
  the Agents page), in which case the switch starts on and you can turn it off for this run. A choice the
  task remembered wins over the profile. Turning it on lets the agent fetch pages and search the web. Off
  turns those tools off, but the agent's shell can still reach the network. A line under the switches says
  whether the agent is sandboxed or has your user's access to this machine; see
  [What the Agent Can Reach](../admin-guide/what-the-agent-can-reach.md).
- **Advanced.** The computer, the agent, the model, the allowed tools, the **Token limit** and the
  timeout (both filled from the profile; clear the token limit to run without one), and the steps after a
  run (verify, push). A step that is not available is shown disabled with the reason beside it, and
  **Verify the result** and **Push the branch** say which part of the runner's setup turns them on; that is
  the administrator's to set ([After a succeeded attempt](../admin-guide/runners.md#after-a-succeeded-attempt)).
  A pull request is not chosen per run: Tack opens one when a pushed branch's task is linked to a GitHub issue.

A setting you change here is saved on the task, and one you change in the project stays on the project, so
running the same task again does not make you retype it. A field that cannot work says why in a sentence
with a link where you typed it (the folder does not exist, the branch is not in the repository, the model
is not on the agent's list), rather than leaving **Run** disabled with no explanation.

### Token limits

A limit is in tokens, and a run always has one while the provisional limit is on. You can set one in four places:

| Where | What it limits |
|---|---|
| A profile, on the Agents page: **Token limit per run** and **Time limit** | Every run of that profile |
| The dialog's **Advanced** section | This run only; it replaces the profile's and the provisional limit, and an empty box runs it with the profile's, else the provisional one |
| Settings → Automation → Merge, **Provisional tokens per run** | Every run whose profile sets no limit; 2 000 000 tokens until you change it, and empty means no limit |
| Settings → Automation → Merge, **Tokens per day** | The project's day; see [Budgets](merging.md#budgets) |

A token counted is a new one: the input the agent's cache did not serve, what it wrote to the cache, and
its output. What the cache served is shown beside the count and never counted, because the model did not
read it again at full price. A run's summary reads, for example, "48 210 tokens (+ 412 000 read from cache,
not counted)", or says "cache not reported" for an agent that does not separate it.

Every run says its limit and who applies it, in one line:

- "Provisional limit: 2 000 000 tokens (no measured runs yet) · set in Settings → Automation · …": the
  profile sets none, so the run takes the provisional limit, which is the cap the SWE-Effi benchmark gives
  one software task. Failed runs cost about four times a successful one, so it is there to stop a run that
  has run away, not to trim good runs.

- "300 000 tokens · from the profile Implementer · docket stops the task there"
- "No limit set in Tack · docket stops each step at its own limit (docket's setting)": a docket team has
  a limit of its own for each step, which you change in docket, not in Tack.
- "No limit of its own · gets what is left of today's limit (400 000 tokens left) · …": the run names no
  limit and its project has a daily limit, so the run takes what is left of the day when Tack starts it. The
  sentence gives the tokens left at the time you look.
- "300 000 tokens · set for this run · Tack stops Claude Code's run there": Claude Code and opencode
  report each model call's tokens as it finishes, and Tack stops the run when the tokens counted reach the
  limit. The attempt fails with "the run reached its token limit (300 000)". A call's output is complete
  only at the end, so a run can end a little past its limit.
- "measured, not enforced": Codex reports its tokens, but Tack cannot stop it at a limit yet. The limit is
  still shown, and still counts against the project's day. A run given what was left of the day says it can
  go past today's limit.

The dialog prints the same line under **Token limit** before you start, and it changes as you edit the
limit, clear it or choose another agent.

#### A limit learnt from a profile's own runs

When you edit an agent profile, a line under **Token limit per run** tells what its last successful runs
used. A run counts as successful when its attempt succeeded and nobody, neither a person nor the
verifier, refused its result; the last 20 count.

- With fewer than 5: "3 of 5 successful runs measured before a limit is proposed".
- With 5 or more: "The last 7 runs used 180 000 to 520 000 tokens. Proposed limit: 790 000", with an
  **Apply** button. The proposal is the 90th percentile of those runs times 1.5, rounded up to two
  significant figures. Tack never applies it by itself: **Apply** writes it as the profile's limit, and you
  can type another number instead.

#### A run that reached its limit

While a run works, the line "Used 1 650 000 of 2 000 000 tokens" shows its count and turns amber at 80% of
the limit. A run stopped by its limit says "Stopped at its limit: 2 000 000 of 2 000 000" and offers
**Run again with more**, which opens the Run dialog for the same task with the token limit already typed as
twice the old one. Change it or press **Run**.

A docket run that ended because the agent needed a person shows "Needs a person" and the agent's own
sentence, and offers **Run again at L3**: the same task, run attended from the start, started only when you
press it. The button is disabled, with its reason, when the run did not use docket or already ran at L3.
A sentence beside it says what L3 means: you approve each step that needs it, and you merge every change
yourself. The same sentence is in the dialog, where the other agents are disabled with "A level applies only
to a docket run", because only docket takes a level. Where Tack merged a change by itself, the run says what
its level, L4 or L5, allows: L4 merges by itself inside the domains you named, L5 anywhere in the repository once
its checks pass. Whether the team may run attended or alone is docket's own decision, not the project's merge level; if docket
refuses the level, the run fails with docket's sentence.

From the moment a docket run stops to ask, and after one ended asking, the run shows the same state, **Needs a
person**, in the same colour, and **Open the question** on the card and on the attempt goes to the question. A
screen reader is told when a run changes state.

Dollars are never a limit. Where Tack shows one, it is an estimate: "≈ $0.42 (estimated)".

### While a run works

The run's **Timeline** (under **What the agent did** on the task's Execution tab) fills as the agent
works: each call to the model with its tokens, each tool it ran (a shell command is shown as the command),
and each call it was refused, and each question docket opened, who answered it, whether it expired and whether
docket's own policy settled it. It refreshes every couple of seconds until the run ends. Only docket reports
its steps while it works. With Claude Code, Codex and opencode the timeline holds the run's events (a
question, a failure, the end) and the run's log has the rest once it ends.

When the run ends, see [Reviewing a Run](reviewing-runs.md).

---

## Asking before acting

By default an agent decides everything on its own: every tool call, every file it touches. Turning on
**Ask before each action** in the dialog (or `"approvals":"ask"` in a command-line permission policy) makes it
pause and wait for a person.

Each agent pauses at a different point, as the table in [Choosing an agent](#choosing-an-agent) says. For an
agent that cannot pause, the dialog disables the switch and says why, and a request built by hand that asks
anyway is never given to a runner that would ignore it.

With docket, a question names the call and docket's reason for stopping it. For example: *Allow bash to run
"sh -c 'echo done > DONE.txt'"? docket's policy: 'sh' is not on the curated allowlist.* The agent's own
reason follows when it gave one. **Allow** lets the call run. **Deny** refuses it, tells the agent it was
refused, and the agent carries on.

A question waits on the task's Execution tab, in the attempt's **Questions from the agent**. Each question
shows its kind, so a tool permission reads differently from a choice the agent wants you to make. An option
can carry a short description, its risks and a rough cost in tokens. When the agent has a preferred answer,
that option is preselected and marked **Recommended**, with the reason and the files it looked at beneath it.
Nothing is applied until you press **Resolve**, and you can pick any option.

<img src="../screenshots/decision-inbox.png" width="50%" alt="A pending question on the Execution tab: the agent asks which chart library to add, offers uPlot, Apache ECharts and hand-written SVG, each with a description, a risk in red and an estimate in tokens, and marks uPlot as Recommended with its reasoning and evidence files; a Details field and a Resolve button sit below.">

Answering needs the **decision token**, a secret your administrator sets that is separate from the ordinary
sign-in. Paste it into the field **Your decision token** above the question; the browser keeps it for the
session. How to set it is in
[Installing and Operating Runners](../admin-guide/runners.md#non-loopback-and-security-posture). A question
nobody answers in time expires, and an expired question is never treated as an Allow.

---

## Running from the command line

This is one complete run from the command line. It was executed against a real `tack serve --with-runner`
with a stand-in `claude` program in place of a signed-in install, so it cost nothing and never left the
machine. Every id and line of output below is copied from that run. Swap in a real, signed-in agent and the
same commands reach the same place.

**Before this:** a project and a task exist (`tack init`, `tack add`), and an embedded runner is running
(`tack serve --with-runner`) or an administrator has enrolled one. `tack execution create` requires
`--permission-policy` and `--timeout-seconds`; the CLI fills in an idempotency key for you. The server
loads the profile by id and builds the repository from the project.

Create a profile. Its instructions and limits are copied into every request made with it:

```sh
tack agent-profile create "demo-profile" \
  --instructions "Print the single word DONE and exit. Do not modify any files."
```

```text
Created agent profile: demo-profile (ap_91b4e)
  id: ap_91b4ea76-9f1a-4725-8a58-21a57d92572c
```

Now start the run. `--runner` is optional: without it the request goes to the profile's fleet, or else to
the only active runner, which is the embedded one here.

```sh
tack execution create <ITEM_ID> \
  --agent-profile ap_91b4ea76-9f1a-4725-8a58-21a57d92572c \
  --harness claude-code \
  --model sonnet \
  --permission-policy '{"tools":[],"network":false}' \
  --timeout-seconds 120
```

```text
Created execution request: exec_0fe
  state: queued
  id:    exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

Poll for the end. A fresh embedded runner finishes against a stand-in agent in well under a second:

```sh
tack execution get exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```text
Execution request exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
  item:    e1d5e03d-4610-45c2-b5f1-835e69f148a7
  state:   succeeded (done)
  created: 2026-09-03T19:00:16.697646760+00:00
```

That is a completed run, reached with no browser. `tack execution list` shows every request, newest first,
and `tack execution cancel <ID>` asks the runner to stop one. For the detail of an attempt, such as which
model answered and how the run ended, open the task's Execution tab or read the attempts over the API: see
[The Runner Contract](../developer/runner-contract.md#what-actually-runs-today). Every `tack execution`
option is in the [CLI reference](cli.md#runs).
