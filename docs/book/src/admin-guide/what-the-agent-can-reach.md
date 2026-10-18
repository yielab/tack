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

- **The token.** On a loopback bind the server always has a token. If you set
  `TACK_API_TOKEN` it uses that; if not, it reads `api-token` from the directory of its
  database file, or creates it there at the first start (mode `0600`, one line). Every
  `/api` request except `/api/health` must carry it, as `Authorization: Bearer <token>`
  or as the cookie below. A server that cannot read or create the file does not start,
  and says which path failed.
- **The cookie.** The page the server answers at `/` sets the token as a `tack_session`
  cookie (`HttpOnly`, `SameSite=Strict`, sent only to `/api`). Your browser holds it; the
  page's scripts cannot read it. The cookie is only set for a host name the server
  answers to. The `tack` command finds the token by itself on the same machine, after
  `--token`, `TACK_API_TOKEN` and `~/.tackrc`. If the server's database is somewhere
  else, set `TACK_API_TOKEN` on both ends.
- **The Host rule.** A request whose `Host` is not `localhost`, `127.0.0.1`, `::1`, the
  address the server is bound to, or the host of an origin you listed in
  `TACK_ALLOWED_ORIGINS` is answered `421`. This is what stops a web page from reaching
  your board through a name it controls.
- **Other sites.** A request that changes something (anything but a read) is refused with
  `403` when the browser says it came from another site, or when its `Origin` is not one
  of the server's own. A body that does not say what it is (JSON, YAML, or the backup
  file's own type) is refused with `415`.

Binding beyond loopback still needs a token, and the server will not start without one
unless you set `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1`. Do that only on a network
you already trust. See [Administration & Security](administration.md).

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
- **Tack's sandbox on Linux.** On top of that, where `bwrap` is installed, the runner
  starts each agent inside it. The agent
  sees the workspace and a scratch home, the folders it keeps under your home
  (`~/.claude`, `~/.codex`), and the system directories read-only. It does not see the
  runner's state, `~/.tackrc`, `~/.ssh` or the token file. The network is not
  restricted: the agent has to reach its model. The runner decides, for each agent,
  by starting it once inside `bwrap`. One that cannot start there (a program installed
  in a virtualenv or under Homebrew, for example) runs without a sandbox, and
  `tack runner doctor` and the run say so. Without `bwrap` the agent has your user's
  access to the machine, and the start report says `sandbox: none`. An agent without a
  sandbox must be able to ask before it acts, so the run dialog asks by default; one that
  cannot ask does not run on that
  machine, and the Agents page, the run dialog and the project's Automation settings say
  why.
- **docket's jail is docket's.** Tack does not wrap docket in `bwrap`: docket is a daemon with its
  own home, and it confines its agents itself, in a jail it sets up. The run reports
  `sandbox: harness`, shown on screen as "Sandboxed by docket in its own jail". Tack reaches
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

A run that asks pauses before each action and waits for a person to answer in Tack's inbox. Whether a
run asks depends on the sandbox, and it is decided in two places:

- **The Run with agent dialog.** "Ask before each action" starts switched on when any machine the run
  could land on reports the chosen agent as not sandboxed (`sandbox: none`), and off otherwise. The
  person can switch it either way. For an agent that cannot pause to ask, the switch is disabled and the
  dialog says "This agent can't pause to ask — it always decides on its own."
- **A task a project starts by itself.** Nobody is at the dialog, so Tack decides. The run asks unless
  **every** machine it could land on reports the agent as sandboxed, by `bwrap` or by the agent's own
  jail. A machine that reports no sandbox, or does not say, makes the run ask. The machines are the
  fleet of the profile, or the only active runner. See
  [Starting tasks by itself](runners.md#starting-tasks-by-itself).

Whatever the dialog or the project says, an agent that is not sandboxed **and** cannot ask does not run
on that machine at all. The runner refuses the attempt before starting it, with "not sandboxed on this
machine … and this agent cannot ask before it acts, so it does not run here; install bubblewrap, or
update the agent to a version that can ask". The Agents page, the run dialog and the project's
Automation settings show the same reason. A run started through `tack mcp` always asks.

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

Never at completion. A pull request opens when you accept the attempt, in the attempt
review or in the merge-readiness pack review, and only if the runner pushed the branch
(`push_branches = true`) and the card is linked to a GitHub issue. Until you accept,
nothing has been proposed to anyone.

## What "Web tools" does and does not do

The **Web tools** switch decides whether the agent may use its own web-fetch and
web-search tools. Off, claude-code is started without them. It does not cut the
network: a shell command the agent runs can still reach any address the machine can,
inside the sandbox or not. If a run must not reach the network, run it on a machine
or in a container that cannot.

A profile can say its role needs the network: the box **This role needs the network (web search
and reading sources)** on the profile form, or `tool_policy.network: true` in the project file.
The switch then starts on for that profile's runs, and a run a task starts by itself has it on
without anyone ticking it. A profile that says nothing is off, and a run that sets the switch
itself keeps its own choice.

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
