# What the Agent Can Reach

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
- **The sandbox.** Where `bwrap` is installed, each harness runs inside it. The agent
  sees the workspace and a scratch home, the folders its harness keeps under your home
  (`~/.claude`, `~/.codex`), and the system directories read-only. It does not see the
  runner's state, `~/.tackrc`, `~/.ssh` or the token file. The network is not
  restricted: the harness has to reach its model. The runner decides, for each harness,
  by starting it once inside `bwrap`. One that cannot start there (a program installed
  in a virtualenv or under Homebrew, for example) runs without a sandbox, and
  `tack runner doctor` and the run say so. Without `bwrap` the agent has your user's
  access to the machine, and the start report says `sandbox: none`. An agent without a
  sandbox must be able to ask before it acts, so the run dialog asks by default; one that
  cannot ask (a docket too old to accept `--answers`, for example) does not run on that
  machine, and the Agents page, the run dialog and the project's Automation settings say
  why.
- **The workspace.** The agent works in a clone or a branch of the project's own
  repository, or in your folder if you chose that. The server takes the repository from
  the project; a request that names a different one is refused. See
  [Where the agent works](agent-runners.md#where-the-agent-works).
- **The card's text.** What a card says reaches the agent between markers that tell it
  the text is data, not an instruction. Both the server's prompt and the
  [MCP server](https://github.com/yielab/tack/blob/develop/docs/MCP.md) do this.

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

## MCP

A run started through `tack mcp` always asks you before each action, whatever the
calling agent sends, and its tool arguments are checked before they become part of a
request. An agent driving Tack cannot choose the environment, the permission policy or
the repository of a run.

## What this does not cover

Tack stores your database unencrypted. Anything that can run as your user and read your
home can read it. A harness you run without a sandbox can do whatever you can. Turn on
the push only for agents you would trust with your git credentials. The deployment
checklist is in [Administration & Security](administration.md).
