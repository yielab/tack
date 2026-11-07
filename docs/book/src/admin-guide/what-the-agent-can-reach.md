# What the Agent Can Reach

This page is for the person who decides what an agent may touch. After reading it you know what
protects the board, what a run receives, what the sandbox does and does not cover, and when a run
asks before it acts.

An agent reads text it did not write: a card, a repository, a web page. Any of that text
can try to give it orders. This page says what Tack does so that, even then, the agent
reaches only what you gave it, and where the limits are. The short version is in
[`SECURITY.md`](https://github.com/yielab/tack/blob/develop/SECURITY.md).

## Who can talk to the board

Tack has one operator and one secret. There are no user accounts.

- **The token.** Every `/api` request except `/api/health` and `/api/openapi.json` must carry the operator token, as
  `Authorization: Bearer <token>` or as the `tack_session` cookie the page sets for your browser (the
  page's scripts cannot read it). On a loopback bind the server makes the token itself if you set none,
  and a server that cannot read or create its file does not start. Where it lives and how the `tack`
  command finds it is in [Administration & Security](administration.md#authentication).
- **The Host rule.** A request whose `Host` is not `localhost`, `127.0.0.1`, `::1`, the
  address the server is bound to, or the host of an origin you listed in
  `TACK_ALLOWED_ORIGINS` is answered `421`. This is what stops a web page from reaching
  your board through a name it controls.
- **Other sites.** A request that changes something (anything but a read) is refused with
  `403` when the browser says it came from another site, or when its `Origin` is not one
  of the server's own. A body that does not say what it is (JSON, YAML, or the backup
  file's own type) is refused with `415`.

- **An agent that uses Tack itself.** A manager, or any agent that reads and changes the board through
  `tack mcp`, should not hold the operator token. It gets an [agent credential](administration.md#agent-credentials):
  it sees only the projects you named, it changes the board only through change sets, and it can never answer
  a run's question, start a run or change a setting.

Binding beyond loopback still needs a token; see [Network exposure](administration.md#network-exposure-and-tls).

## What a run receives

- **Environment.** A run gets the variables the runner's own `[environment]` table
  names, plus what the request asks for, within limits. A request may not set `HOME`,
  `PATH`, `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV`, anything starting
  `GIT_`, `SSH_`, `LD_`, `DYLD_` or `PYTHON`, or anything ending `_BASE_URL`: each of
  those changes what runs or where requests go. A request cannot read a variable from
  the runner's own environment (`env:`). It can name a `store:` secret only if you
  listed that name under `[secrets] request_may_reference` in the runner's config; the
  default list is empty. The attempt records the names (never the values) the agent got.
- **The agent's own permissions.** Each agent enforces what it may do itself:
  Codex through its sandbox modes, Claude Code through its permission mode and allowed
  tools, opencode through its permission settings, docket through its policy. Tack passes
  the run's tool list and approvals to each one. How far each honours them is reported by
  `tack runner doctor`, on the "permission policy" line of each agent.
- **Who contains the agent.** Every agent runs contained: by this computer (bubblewrap), by its own
  sandbox, or by docket. Claude Code and opencode bring no sandbox, so for them it is bubblewrap
  only; Codex has its own and is also run inside bubblewrap. Where nothing contains an agent it
  does not run, unless the administrator turned the requirement off, and then it asks before each
  action. The run dialog's Sandbox line and the Agents page say which of these holds for each
  agent.
- **Tack's sandbox on Linux.** Where `bwrap` is installed, the runner starts each agent inside it.
  The agent
  sees the workspace and a scratch home, the system directories read-only, and of the
  folders it keeps under your home (`~/.claude`, `~/.codex`) only its settings and
  instructions, read-only, and its login file, which it may refresh. It cannot rewrite
  its own settings, and the MCP servers in your Codex configuration do not load. What it
  writes there while it works (sessions, caches) goes to the scratch home and is
  discarded with it. It does not see the
  runner's state, `~/.tackrc`, `~/.ssh` or the token file, does not share IPC or the hostname with
  your machine, and holds no Linux capabilities. The network is not
  closed by the sandbox: the agent has to reach its model, and where it may go is
  decided by its profile's [web access](#web-access). The runner decides, for each agent,
  by starting it once inside `bwrap`. One that cannot start there (a program installed
  in a virtualenv or under Homebrew, for example) counts as not contained, and
  `tack runner doctor` and the run say so.
- **Turning the requirement off.** The runner's settings file requires a sandbox by default. An
  administrator who cannot or will not install bubblewrap writes `required = false` under
  `[sandbox]`. Without `bwrap`, an agent then runs with your user's access to the machine and
  asks before each action; the start report says `sandbox: none`, the run's timeline says why it
  asks, the run dialog and the Agents page say "Not contained: the administrator turned the
  requirement off", and `tack runner doctor` shows the setting. An agent that cannot ask does not
  run even then.
- **docket's jail is docket's.** Tack does not wrap docket in `bwrap`: docket is a daemon with its
  own home, and it confines its agents itself, in a jail it sets up. Tack asks docket whether it
  does: when docket says so, the run reports `sandbox: harness`, shown on screen as "Contained by
  docket"; when it says it does not, or does not say, Tack takes the strictest reading and the run
  asks before each action. Tack never refuses to run docket over its answer. Tack reaches
  docket only over its loopback API with a bearer token, passes it the task, the denied tools
  and the network flag as restrictions, and holds no model key for it.
- **The workspace.** The agent works in a clone or a branch of the project's own
  repository, or in your folder if you chose that. The server takes the repository from
  the project; a request that names a different one is refused. See
  [Where the agent works](../user-guide/automation.md#where-the-agent-works).
- **The card's text.** What a card says reaches the agent between markers that tell it
  the text is data, not an instruction. Both the server's prompt and the
  [MCP server](https://github.com/yielab/tack/blob/develop/docs/MCP.md) do this.

## When a run asks before each action

A run that asks pauses before each action and waits for a person to answer in **Waiting for you**. Whether a
run asks depends on the sandbox, and it is decided in two places:

- **The Run with agent dialog.** "Ask before each action" starts switched on when any machine the run
  could land on reports the chosen agent as not contained (`sandbox: none`), and off otherwise. The
  person can switch it either way. For an agent that cannot pause to ask, the switch is disabled and the
  dialog says "This agent can't pause to ask — it always decides on its own."
- **A task a project starts by itself.** Nobody is at the dialog, so Tack decides. The run asks unless
  **every** machine it could land on reports the agent as contained, by `bwrap` or by the agent's own
  jail. A machine that reports no sandbox, or does not say, makes the run ask. The machines are the
  fleet of the profile, or the only active runner. See
  [Starting tasks by itself](runners.md#starting-tasks-by-itself).

Whatever the dialog or the project says, an agent that nothing contains does not run on that machine
at all, unless the requirement is off and the agent can ask. The runner refuses the attempt before
starting it, with "not contained on this computer … so this agent does not run here; install
bubblewrap". The Agents page, the run dialog and the project's Automation settings show the same
reason. A run started through `tack mcp` always asks.

## What the runner's git refuses

The runner runs git in a workspace the agent has written to, so it treats that
repository's configuration as untrusted. Every git command runs with hooks off, the
file-system monitor off, plain `ssh` as the ssh command, and the `ext::` transport
forbidden; the system git configuration is ignored; and diffs run no external diff or
text-conversion program.

Before a branch is pushed the runner checks two things. The repository's git
configuration must be the same as when the work began, or the push is refused (*The
agent changed the repository's git configuration; nothing was pushed*). And the commit
at `HEAD` must be the one that was verified, or it is refused too (*The tree changed
after it was verified; nothing was pushed*). The push goes to the URL recorded when the
workspace was made, and sends exactly that commit.

## What the evidence is computed from

The verifier and the review pack are computed from the commit the work started from,
which the runner pins when it makes the workspace, not from a branch name the agent
could move. [prill](https://github.com/yielab/prill) reads its `prill.yaml` from that
commit, so an agent that edits `prill.yaml` does not change how its own work is checked.

The pack also lists the files that decide how work is checked, under *The agent changed
how it is checked*: everything under `.github/`, any file or folder with `test` in its
name, and `package.json`, `Cargo.toml`, `Makefile`, `justfile` and `prill.yaml`. An
empty list is left out.

A run ends when its process ends. The first result the agent prints is the one that
counts, and a non-zero exit status is a failure even if the agent's output said it
succeeded.

## When the pull request opens

Never at completion. A pull request opens when you accept the attempt in its
review, and only if the runner pushed the branch
(`push_branches = true`) and the card is linked to a GitHub issue. Until you accept,
nothing has been proposed to anyone.

## Web access

A profile chooses how far its agent may reach on the web: **closed** (only the model's own address), **listed**
(the model and the hosts in the profile's `tool_policy.allowed_hosts`, each `example.com` or `*.example.com`) or
**open** (any public site). A new profile starts with the mode of its kind: Implementer and custom profiles listed,
Reviewer and Planner closed, Researcher open. The Agents page edits it as **Web access**; the project file and the
profile API call it `egress_mode`.

The runner enforces the choice when its configuration says so, and not before:

```toml
[egress]
enforce = true
model_hosts = ["openrouter.ai"]   # hosts every mode lets through, besides the agent's own model address
```

With `enforce = true` each run of Claude Code, Codex or opencode goes through a proxy the runner starts for that run
on loopback and tells the agent about through `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` (a request cannot set them).
In every mode the proxy refuses this computer's own address, private networks, link-local addresses and the cloud
metadata addresses. It judges the address a name resolves to, not the name, and connects to that address and no other,
so a name that answers differently a second time reaches nothing inside. A refused destination is an event on the run
and each one reached is logged with its bytes. Over plain HTTP only `GET`, `HEAD` and `OPTIONS` to a URL of at most
2048 bytes pass; an HTTPS tunnel hides its URL, so **open** limits where the agent connects and not what it sends,
and a public site it allows can still receive data. `tack runner doctor` prints, per agent, whether this runner
enforces it.

Know before you turn it on:

- Claude Code and Codex name their model's address; opencode takes whichever provider its own configuration names,
  so list those hosts under `model_hosts`. A model served on this computer, or on a private network, is refused like
  any other internal address.
- A profile with `listed` and no `allowed_hosts` reaches its model and nothing else, as `closed` does. Check the
  profiles that browse before you enforce, and give them `open` or their hosts.
- This is a request to the agent through the proxy variables, not a wall. A program that ignores them, or a shell
  command that opens its own socket, is not held, whatever the sandbox does. The sandbox does not yet remove the other
  ways out.
- docket confines its own traffic; Tack asks it and shows the answer, and does not enforce.

## What "Web tools" does and does not do

The **Web tools** switch decides whether the agent may use its own web-fetch and
web-search tools. Off, claude-code is started without them. It does not cut the
network: a shell command the agent runs can still reach any address the machine can,
inside the sandbox or not. If a run must not reach the network, run it on a machine
or in a container that cannot.

A profile can say its role needs the network: the box **This role needs the network (web search
and reading sources)** on the profile form, or `tool_policy.network: true` in the project file.
The switch then starts on for that profile's runs, and a run a task starts by itself has it on
without anyone ticking it. A profile whose tools for the agent include the web tools, or every tool,
starts on too, for any agent but docket. A profile that says nothing else is off, and a run that sets
the switch itself keeps its own choice. A run with the switch off drops the web tools from the tools
the profile names and keeps the rest; it is refused only when the web tools were the only ones named.
opencode reaches the network on every run, so its switch is always on.
A docket team's web is the team's own, not the profile's: the switch asks docket for the network but does
not give it. A run with the switch off also takes docket's `fetch` tool away from the team, because the
network restriction only cuts docket's jail and `fetch` runs outside it. What a team reads is on its sheet
in Agents → Teams (**Web**), in docket's own words.

## MCP

A run started through `tack mcp` always asks you before each action, whatever the
calling agent sends, and its tool arguments are checked before they become part of a
request. An agent driving Tack cannot choose the environment, the permission policy or
the repository of a run.

## What this does not cover

Tack stores your database unencrypted. Anything that can run as your user and read your
home can read it. An agent you run without a sandbox can do whatever you can. Turn on
the push only for agents you would trust with your git credentials. The deployment
checklist is in [Administration & Security](administration.md).
