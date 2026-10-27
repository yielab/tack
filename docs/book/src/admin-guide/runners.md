# Installing and Operating Runners

This page is for the person who sets up the machines that run agents. After reading it you can attach
a runner, sign its agents in, let it work on several attempts at once, set up docket teams, and know
what the runner does after each run.

A **runner** is the process that does the work Tack only schedules. It asks the board for an
attempt, prepares a workspace for it on its own machine, starts the agent in that workspace, reports
what happened, and then collects the evidence and, if you allow it, verifies and pushes the result.
The board never runs an agent, a verifier or `git push` itself. So the runner belongs on the machine
that has your code, your git credentials and your agents' sign-ins.

## The three ways to attach a runner

| Way | Command | Use it when |
|---|---|---|
| Inside the board | `tack serve --with-runner` | One person, one machine. No enrollment and no token to copy. Loopback only. |
| A runner of its own, from the `tack` binary | `tack runner start` | The runner is on another machine, or you want it apart from the board. Needs [enrollment](#enrolling-a-runner). |
| The separate runner executable | `tack-runner` | A machine that should run agents but not the board. Each GitHub release has a `tack-runner` archive of its own, with a systemd unit. It takes the same settings as `tack runner start` and one more flag, `--max-concurrent-attempts`. |

All three speak the same protocol to the board, so everything below applies to each unless it says
otherwise. A runner is configured by a TOML file, by environment variables and by flags, in that
order of increasing priority. The file may hold these top-level keys and these tables; any other key
makes the runner refuse to start.

| Key or table | Meaning |
|---|---|
| `api_base_url` | The board's runner API. Default `http://127.0.0.1:3210/api/runner/v1`. |
| `runner_id` | The name the runner sends to the board. Default `local-runner`. |
| `state_dir` | Where the runner keeps its state. Default `.tack-runner`. |
| `enrollment_credential` | The one-time token. Prefer the environment variable. |
| `max_concurrent_attempts` | How many attempts at once. See [below](#running-several-attempts-at-once). |
| `[verify]`, `[git]` | The verifier and the branch push. See [After a succeeded attempt](#after-a-succeeded-attempt). |
| `[secrets]`, `[environment]` | What a run may reference and the variables every run gets. See [What the Agent Can Reach](what-the-agent-can-reach.md). |
| `[docket]` | Where docket's API is. See [docket teams](#docket-teams). |

Every variable and key is in the [Configuration Reference](configuration-reference.md). Run
`tack runner doctor` on the runner's machine at any point: it reports, for each agent, whether it was
found, whether it is signed in and which models it offers, plus the secret store, the verifier and
docket.

## Enrolling a runner

Enrollment is two steps. You create a *pending* runner on the board and get a one-time token. Then
the runner redeems that token.

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

The token is redeemable for an hour unless you pass `--enrollment-lifetime-secs`. The board keeps only
a hash of it, so it is shown once and cannot be retrieved. If you lose it, revoke it with
`tack runner revoke-token <runner-id> <token-id>` and enroll again. Pass `--out <path>` to write the
response to an owner-only file instead of printing the token. That suits a provisioning script:

```sh
tack runner enroll ci-runner-1 --total-capacity 1 --available-capacity 1 \
  --out /secure/place/ci-runner-1.enrollment.json
```

A new runner shows as `pending_enrollment` in `GET /api/runners` until it redeems the token.
There is no `tack runner list`; use that route or the Agents page.

### Redeeming the token on the runner

The runner exchanges the token for a durable credential the first time it starts:

```sh
export TACK_RUNNER_API_URL=https://tack.example.com/api/runner/v1
export TACK_RUNNER_ID=runr_d91da686-...
export TACK_RUNNER_STATE_DIR=/var/lib/tack-runner
export TACK_RUNNER_ENROLLMENT_TOKEN=enr_57cd3592-...
tack runner start
```

Prefer the environment variable to `--enrollment-token`: the flag leaves the secret in your shell
history and in the process list.

On a later start the runner first tries the credential it stored in its state directory, and uses the
token only if the board refuses that credential. It still refuses to start when no token is
configured at all, before it touches the disk or the network. So keep the variable set in the
runner's service environment. A token that has expired or been revoked cannot enroll a replacement; if
the stored credential is ever refused, enroll the runner again.

### Running a runner as a service

The `tack-runner` archive of each release holds a systemd unit (`tack-runner.service`) and an
environment file (`tack-runner.env.example`). They install the runner under its own user, with its
state in `/var/lib/tack-runner`:

```sh
sudo cp tack-runner.service /etc/systemd/system/
sudo useradd --system --create-home --home-dir /var/lib/tack-runner tack-runner   # once
sudo mkdir -p /etc/tack-runner
sudo cp tack-runner.env.example /etc/tack-runner/tack-runner.env
sudo $EDITOR /etc/tack-runner/tack-runner.env   # set the API URL, the runner id and the token
sudo chmod 600 /etc/tack-runner/tack-runner.env
sudo chown tack-runner:tack-runner /etc/tack-runner/tack-runner.env
sudo systemctl daemon-reload
sudo systemctl enable --now tack-runner
```

Keep `TACK_RUNNER_ENROLLMENT_TOKEN` in the environment file, as the previous section explains. The
unit stops the runner with `SIGTERM` and gives it 30 seconds to finish. The agents run as the
`tack-runner` user, so sign each one in as that user. The unit also hides `/home` from the runner, so
a project folder the runner works on must live elsewhere.

### Revoking a runner

```sh
tack runner revoke runr_d91da686-...
```

Revocation takes effect on the runner's next request: its credential stops working. It does not stop
an attempt already running on that machine. Such an attempt ends up finished or in `needs_operator`
by the steps in the [Recovery Runbook](recovery-runbook.md).

## Standalone mode: `tack serve --with-runner`

The steps above assume two processes: the board, and a runner you enrolled by hand.
`tack serve --with-runner` (or `TACK_LOCAL_RUNNER_ENABLE=1`) puts both in one command. It is the
common case of one person running agents against their own board. It is off by default. You can also
turn it on and off from the **Agents** page, under "Agent execution on this machine", without
restarting the board. The page also shows "Up to N runs at a time on this computer" and how many are
running.

- **It is a real runner.** It runs inside the board's process but talks to the board over loopback
  HTTP like any remote runner, with the same protocol and the same checks.
- **No enrollment.** On a fresh state directory it creates its own pending runner and redeems its own
  token. No token is printed or configured. A later start with the same state directory reuses the
  stored credential instead of creating a second runner.
- **Its name** is "This computer (host name)", the name of the machine it runs on. A runner the
  board enrolled earlier under a generated `local-…` name takes it on its next start and keeps its
  identity and credential; a name you chose yourself is never changed. A runner on another machine
  has the name you gave it when you enrolled it.
- **Loopback only.** `TACK_HOST` must be a loopback address. A board bound anywhere else refuses to
  start with the embedded runner, before it opens a socket or the database, and never falls back to
  a board without a runner. The Agents page then says the embedded runner is not available. An
  embedded runner runs agents on the machine that serves the web page, so it is limited to the
  machine you sit at.
- **Off unless asked.** A plain `tack serve` starts no runner.
- **Its state directory** is `<TACK_STORAGE_DIR>/runner` unless `TACK_RUNNER_STATE_DIR` says
  otherwise. Tying it to the storage directory keeps two boards from sharing one runner's state. A
  state directory from an older default location is moved there once, never the other way.
- **Its settings file.** Put it at `~/.config/tack/runner.toml` and the embedded runner reads it on
  every start, including a service installed by `tack service install`. Wiping the board's data
  folder leaves it in place. To use another file, pass `tack serve --runner-config <path>` (or set
  `TACK_LOCAL_RUNNER_CONFIG`). It is the same file `tack runner start --config` reads. It is the only
  way to give the embedded runner a `[verify]`, `[git]`, `[secrets]`, `[environment]` or `[docket]`
  table, because those have no environment variable. With no file the embedded runner runs on
  defaults: no verifier, no branch push and no docket. The Agents page says which file it read, or
  that it read none, and so does `tack runner doctor`.
- **Its log lines** are in the board's log by default: enrollment, claims, refusals and
  completions. For every step, start the board with `RUST_LOG=tack=debug`.

Everything else on this page (agent lookup, credentials, workspace storage, what happens after a
run) applies to an embedded runner exactly as to a separate one.

## Where Tack looks for agents

<a id="where-tack-looks-for-a-harness-binary"></a>

The Agents page's "installed" check and the runner's own start-up probe find `codex`, `claude`,
`opencode` and `docket` the same way. They look on the runner process's own `PATH` first, as a shell would. If
that finds nothing, the runner checks a fixed list of per-user install folders. A terminal usually
has them on its `PATH`, but a desktop launcher or a systemd user service usually does not:

- `~/.local/bin`
- `~/.cargo/bin`
- `~/.bun/bin`
- `~/.npm-global/bin` and `~/.npm/bin`
- the `bin` folder of every Node version under `~/.nvm/versions/node/`
- `~/.linuxbrew/bin`
- `/opt/homebrew/bin` (Homebrew on macOS), `/home/linuxbrew/.linuxbrew/bin` (Homebrew on Linux) and
  `/usr/local/bin`
- on Windows, `%APPDATA%\npm` and `%LOCALAPPDATA%\nvm`

The list is fixed; no `TACK_*` variable changes it. A machine whose shell already sees the install is
unaffected, because `PATH` is searched first and the first match wins. If neither search finds the
program, the error names every folder it checked. Install the agent in any of them, or put it on the
`PATH` the runner inherits.

docket is still turned on only by the `[docket]` table, because it is a daemon the runner talks to.
Without one, the doctor reports "docket is not configured: add a [docket] table with its binary or
base_url". When the table's `binary` is a bare name such as `docket`, the runner looks for it in the
folders above; a path is used as written.

When the desktop app starts the board, it also stops it: quitting from the tray, or sending the app
`SIGTERM`, `SIGINT` or `SIGHUP`, stops the board and its embedded runner. A board the app found
already running is left running.

## Local credential handling

- The enrollment token is a secret. So is the durable credential the runner gets for it. The board
  stores only hashes of both, so it cannot hand either back.
- The runner never prints its credential. It appears as `[REDACTED]` in every log line and error.
- The runner's **state directory** (`TACK_RUNNER_STATE_DIR`, default `.tack-runner`) holds the attempt
  journal, the credential (`session.json`) and a **secret store** for the values that `[environment]`
  entries and a request's `secret_reference` name. The store is the platform keychain where one
  answers, and an owner-only file (`secrets.json`) otherwise. It is never in the board's database.
  Set a value with `tack runner secret set <name>`. It reads `TACK_RUNNER_SECRET_VALUE` or, failing
  that, standard input, and never a command-line argument, which other users could read in the
  process list. `tack runner doctor` says which store a given start picked.
- **Tack holds no model key.** Each agent signs in with its own command and reaches its model with
  its own tools. The runner forwards no extra environment variables, so a key that exists only in a
  shell variable does not reach an agent. The board never sees, stores or forwards a model
  credential.

  | Agent | Sign in with | Where the sign-in lives |
  |---|---|---|
  | Claude Code | `claude` | `~/.claude` |
  | Codex | `codex login` | `~/.codex` |
  | opencode | `opencode auth login` | opencode's own `auth.json` |
  | docket | docket's own sign-in | docket's own home (`DOCKET_HOME`) |

  opencode runs with a scratch home that receives only its `auth.json` and the `provider`, `model`
  and `small_model` keys of its configuration. Its MCP servers, plugins and permissions do not reach
  the run. A runner whose settings file has a `[provider.*]` table refuses to start and names the
  table.

## Running several attempts at once

A runner works on one attempt at a time unless you raise `max_concurrent_attempts`. Set it in any of
three ways. A later one overrides an earlier one:

| Source | Setting |
|---|---|
| The runner's settings file | `max_concurrent_attempts = 3` |
| The environment | `TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS=3` |
| A flag of the `tack-runner` executable | `--max-concurrent-attempts 3` |

`tack runner start` and `tack serve` read the file and the environment; they have no flag for it.
The default is `1`. A value below `1` is refused, and so is an environment value that is not a whole
number; the runner stops at start and names the setting.

The runner reports to the board, in every heartbeat, how many attempts it has room for. The Agents
page shows the limit for the embedded runner as "Up to N runs at a time".

Two limits to know:

- **A folder worked in place stays one at a time.** If a project lets the agent work in your own
  folder rather than on a branch, the runner takes a lock on that folder and a second attempt there is
  refused with "another attempt is working in this folder". The lock is a file under
  `<state dir>/locks/`. A lock left by a process that died is not detected; remove the file by hand.
  Parallel attempts need a branch or a clone.
- **Stopping an attempt reaches the agent.** Cancelling one attempt tells only that attempt's agent to
  stop, and the others carry on.

Each attempt uses the machine's CPU, memory and the agent's model quota. Raise the number as far as
the machine and the quota allow and no further.

## docket teams

docket runs a long-lived **team** of agents, each with its own model, steps and checks. When a profile
uses docket, the runner posts each attempt to the team as one task, follows what the team does,
carries its questions and approvals to the person through Tack's inbox, and cancels it over docket's
HTTP API. Setting this up has three parts: install docket, tell the runner where it is, and make each
team able to run without a person.

### Install docket and tell the runner

Install docket 0.2.0-beta.6 from the [docket repository](https://github.com/yielab/docket). The
runner speaks docket's HTTP API version 5, which that release provides, and any later version. It
checks the version before every task. A version below 5 is reported as "needs docket 0.2.0-beta.6
(API 5); this docket answers API N", and no task is sent to it. A docket on version 6 or later also
installs the project's dependencies in its own copy, using the project's install setting.

Tack's support for this API was written from docket's published specification and exercised against a
stand-in for docket. Run `tack runner doctor` before you rely on it.

Add a `[docket]` table to the runner's settings file:

```toml
[docket]
home = "/home/me/.docket"      # the daemon's DOCKET_HOME; docket's own default when unset
port = 7399                    # where the daemon listens, on 127.0.0.1
token_file = "/home/me/.docket/token"   # the daemon's bearer token
binary = "docket"              # starts `docket start --dispatch` with the runner
# base_url = "http://127.0.0.1:7399"   # a daemon someone else runs; the runner starts nothing
```

| Key | Default | Meaning |
|---|---|---|
| `home` | docket's own | The daemon's `DOCKET_HOME`, passed to it when the runner starts it. |
| `port` | `7399` | The port the daemon listens on, on `127.0.0.1`. |
| `token_file` | none | The file holding the daemon's bearer token. The runner reads it again on every call, so a daemon restarted with a new token needs no runner restart. |
| `binary` | none | The docket program. Naming one turns docket on for this runner and lets the runner start the daemon. |
| `base_url` | none | A daemon that is already running at this URL. The runner uses it as it is and never starts one. |

docket is on for a runner as soon as `binary` or `base_url` is set. It is never turned on because a
`docket` program happens to be on the `PATH`. The runner starts the daemon only when `binary` is set
and `base_url` is not, and only if nothing already answers `GET /health` on the port. This keeps the
runner from starting a second daemon next to one you run yourself.

### How the runner looks after the daemon

When the runner does start it:

- The command is `docket start --dispatch --bind 127.0.0.1 --port <port>`, with `--token-file` when
  you set one.
- The daemon's environment is `PATH`, `HOME` and, if you set `home`, `DOCKET_HOME`. Nothing else the
  runner holds reaches it.
- Its output goes to `<state dir>/logs/docket-daemon.log`.
- If it exits, the runner restarts it after 2 seconds, then 4, 8 and so on up to a minute. A daemon
  that stays up for more than 30 seconds starts the wait over.
- When the runner stops, it sends the daemon one `SIGTERM` and waits five seconds. docket finishes
  the task it is running and takes no new one, on its own time. A daemon the runner did not start is
  left alone.
- The daemon runs in its own process group, so a Ctrl-C meant for the runner does not stop it. After
  a hard crash of the runner a daemon may still be running; the next start finds it healthy and
  uses it.

### Which team a task goes to

A profile names its team in its tool policy, as in this line from a project file:

```yaml
tool_policy: { agent: docket, tools: { docket: ["*"] }, docket: { team: mer-dev } }
```

The runner checks that docket reports that team and refuses the attempt, saying so,
if it does not. A profile that names no team uses a **default team named for the project's folder**:
the folder's name in lowercase letters, digits and hyphens, at most 64 characters, or `default` if
nothing is left. The runner creates that team in the project's folder the first time it needs it, if
docket does not have it yet.

docket works on a new branch of the project's folder. A project set to work in the folder itself
fails its docket attempts with "the docket team works on a branch of your folder; choose that under
Settings → Automation". When docket finishes, the runner brings the task's branch into its own
workspace so the evidence, the verifier and the branch push read it as they read any other agent's
work.

### Making a team run without a person

`tack runner doctor` prints what docket reports, one line per team:

```text
docket
  status:      present
  ...
  api:         5
  daemon:      running on 127.0.0.1:7399
  team mer-content: lights-out · reads /home/me/.nvm/versions/node/v22.17.1
  team mer-research: attended (fix: docket pod set network none) · reads nothing outside its work
```

A team that can finish a task with nobody present is `lights-out`. Any other team is `attended`, and
the line ends with the fixes docket asks for, in docket's words (the example above is the doctor's
own test data). A team that is `attended`, or reports any fix, **keeps its tasks at autonomy level
L3**, however high the project's level is: Tack will not merge them by itself. The project's
Automation settings show the same teams and fixes under "Ceiling". Run the fix with docket's own
commands, then run the doctor again. The Merge button is not held by this ceiling. See
[Automating a Project](../user-guide/automation.md) for the person's side.

The end of each team's line says which folders its sandbox reads besides its own work: the folders
docket granted, `reads nothing outside its work` when there are none, or `reads: not reported by this
docket` for a docket that does not say. The Teams rung of the Automation settings shows the same fact
under each role's team.

When the daemon does not answer, the `daemon:` line says why, for example "not answering at
http://127.0.0.1:7399/health; the runner starts it with the runner" (the runner is configured to
start it but has not), or "no binary is set to start one".

The Teams rung of the Automation settings has a button to apply a repository's teams. It is shown
disabled, with its reason, because the board has no channel to the runner's machine. Apply a team
with docket's own commands on that machine.

### What a team's sandbox can read

A team's sandbox reads only the system folders and the team's own work. A toolchain installed under
your home folder, such as Node under `~/.nvm` or a Python virtual environment, is outside both, so
the team's tests cannot load it and every check fails. Grant it once for the whole machine, on the
computer that runs docket:

```sh
docket setup sandbox read add ~/.nvm/versions/node/v22.17.1
```

The sandbox may read and run that folder and never write to it. docket refuses a folder that would
expose its own home or yours. A team's own `readOnly` setting can only narrow the machine's grants:
unset, the team keeps all of them; `none` or a list of folders keeps fewer. It cannot add a folder
the machine has not granted. `tack runner doctor` shows, per team, what is left.

### Where docket's sandbox comes from

Tack does not wrap docket in `bwrap`. docket confines its own agents in a jail it sets up, and the
run reports `sandbox: harness`. See [What the Agent Can Reach](what-the-agent-can-reach.md).

## Workspace and artifact storage

Each attempt gets its own workspace, a git worktree that no other attempt or request shares. The
runner keeps them under `<state dir>/workspaces`. A project set to work in your folder is the one
exception; the folder is used as it is.

**On the runner,** before an agent is allowed to start, the runner writes an owner-only journal record
to `<state dir>/journal`. It names the workspace path and the revision the work began from. Writing it
first is what makes crash recovery possible; see the [Recovery Runbook](recovery-runbook.md). Logs,
staging for uploads and, for docket, the daemon log live in the same state directory.

**On the board,** artifacts a runner uploads (logs, diffs, generated files) are stored under
`<TACK_STORAGE_DIR>/execution-artifacts`, apart from item attachments so the retention sweep never
touches the wrong folder. The attempt's artifact panel lists them, and each is a download under the
operator token:
`GET /api/executions/{request_id}/attempts/{n}/artifacts/{artifact_id}/content`. Each upload is
checked against its checksum.

## Installing the project's dependencies

A project can say how its dependencies are installed before an agent works on its code. By default
the command comes from the lockfile at the root of the repository: `npm ci` for
`package-lock.json`, `pnpm install --frozen-lockfile` for `pnpm-lock.yaml`, `yarn install
--immutable` for `yarn.lock`, `uv sync --frozen` for `uv.lock`, `poetry install` for `poetry.lock`
and `cargo fetch` for `Cargo.lock`; with several lockfiles the first in that order wins, and with
none nothing is installed. In Settings → Automation, under **Where the code is**, a project can
replace the command with its own single line or turn the install off. The same screen and the Run
dialog show what will run. For a folder on the board's own computer the board reads the lockfile
itself; for any other project the line says it is detected on the runner when the run starts.

For Claude Code, Codex and opencode the runner makes the copy of the code the agent works in, so it
also runs the command there, before the agent starts. It runs with the runner's own environment,
network included, and within the run's time limit. What npm, pnpm and yarn install into
`node_modules` is kept once in the runner's state folder, under `prepare-cache`, per lockfile. The
next run with the same lockfile gets its own `node_modules` whose files are hard links to the kept
ones, so it prepares in seconds without running the command and without copying the packages; the
state folder and the workspaces must therefore be on the same filesystem, and when they are not the
command simply runs every time. A file named `.tack-prepared` in that folder marks it as the runner's, and
the folder is not counted as the agent's change; when the project's `.gitignore` already lists it,
git leaves it out by itself. `uv sync`, `poetry install`, `cargo fetch` and a
command a project replaced keep nothing and run every time. If the command exits with an error or
runs out of time, the run ends as failed before the agent starts and spends no tokens; its reason is
"Could not prepare the dependencies", the command and how it ended, and the last lines of its output.
A docket run is prepared by docket, not by the runner.

The runner installs only what the agent can use. When a run's tools are named and none of them runs
a command (only Read, Grep, Glob, WebFetch and WebSearch) and the card has no check that runs
something (a command, a test or a metric), nothing is installed, whatever the project's install
setting says, and the run's timeline says "No install: this profile runs no commands". A run with
every tool, with no tool list, or with a check installs as before. The agent's sandbox stays on for
every run.

## Non-loopback and security posture

The routes a runner uses (`/api/runner/v1/*`) are separate from the operator routes. They do not use
the operator token. Each one checks the runner's own credential. A change to the operator token rules
cannot loosen runner access by accident.

For everything else about exposing Tack beyond `127.0.0.1` (TLS behind a reverse proxy,
`TACK_ALLOWED_ORIGINS`, `TACK_API_TOKEN`, request limits) see
[Administration & Security](administration.md). It applies whether or not a runner is enrolled. One
setting is specific to runs: `TACK_EXECUTION_DECISION_TOKEN` is a **second, higher-privilege secret**
that guards answering a run's decisions through the API. It is separate from `TACK_API_TOKEN`, a
request without it is refused, and it is never logged.

`tack serve --with-runner` adds a stricter rule on top: an embedded runner runs arbitrary agent
processes on the machine that serves the web page, so it refuses to start unless `TACK_HOST` is a
loopback address, whatever token the board uses.

## After a succeeded attempt

What a runner does once an attempt has succeeded, and before its workspace is deleted. Each step is the
runner's, on its machine. None of them can change the attempt's outcome: if one fails, the attempt is
still `succeeded` and the failure is recorded as an event on its timeline.

**Evidence.** For every agent, and also for a cancelled attempt, the runner records what the attempt
changed against the revision it started from. It uploads three artifacts: `changes.patch` (the full
diff, cut at 8 MiB with a flag in the manifest when it is longer), `files.json` (each path and whether
it was added, modified, deleted or renamed) and `evidence.json` (the base and head commits, the
patch's checksum and size, the files, the terminal reason and the usage). If the request carried a
brief, `brief.json` is kept beside them. If git cannot read the workspace, only `evidence.json` is
written, and it says why.

**Verification.** A runner with an enabled `[verify]` section runs a program of yours over that
evidence and uploads the merge-readiness pack it writes. It is off by default. A verifier that fails,
is missing or times out records `attempt.verify_failed` and nothing more. The pack is what a person
reads in [Reviewing a merge-readiness pack](../user-guide/reviewing-runs.md#reviewing-a-merge-readiness-pack).
The settings are in the [Configuration Reference](configuration-reference.md);
`tack runner doctor` reports whether the verifier is on and whether its program is found.

**Branch push.** A runner with `push_branches = true` under `[git]` commits what the agent left
uncommitted onto a branch named `<prefix><first 8 characters of the item's id>-a<attempt number>` and
pushes it to the remote the repository came from, using your own git credentials. It is off by
default. A failed push records `attempt.push_failed`. Turn it on only for agents you would trust with
those credentials. The Configuration Reference says what the push does and does not guard against.

**Pull request.** When the attempt pushed a branch, its item is linked to a GitHub issue and a GitHub
token is configured, the board opens a pull request from that branch, with the merge-readiness pack as
its body. The attempt then shows a **PR #n** link and a badge: open, merged, closed or reverted. The
state follows GitHub on the sync poll; see
[GitHub sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md). An item with no link,
or a board with no token, gets no pull request and nothing else changes.

**Moving the item.** A request can carry a status policy. `done_on_success` moves the item to its
workflow's first Done status when the attempt succeeds, and `done_on_mrp_accepted` moves it when a
person accepts the attempt's merge-readiness pack. Without one, the item's status is never touched.
Set it with `tack execution create --status-map-policy`, or `status_map_policy_id` in the API and the
MCP server; the Run with agent dialog does not offer it. The move follows the workflow's ordinary
rules, and open boards are told like after any edit.

## Starting tasks by itself

A project can start a task's run without anyone pressing Run. The person switches this on under
Settings → Automation → Start (see [Automating a Project](../user-guide/automation.md)); this section
is about the part you run. It happens **in the board's process**, not in a runner.

- **The interval.** Every `TACK_AUTO_START_SECONDS` seconds the board looks at each project that has
  Start switched on and is not archived. The default is `30`. `0` turns the look off for every
  project. A value that is not a whole number counts as the default. The first look comes one interval
  after the board starts. It needs no GitHub token and no poll.
- **What it does.** For each project it takes the tasks waiting in the project's start status that
  have a role, orders them by priority and position, and starts as many as the project's conditions
  allow: no open run on the task, no unfinished blocker, room under the next column's work-in-progress
  limit, the daily budget not spent, and fewer open runs than the project's cap. It then moves each
  started task to the first in-progress status and writes "Started by itself" on its activity.
- **Which machine.** The run goes to the fleet its profile names. If the profile names none, there
  must be exactly one active runner, and that runner is used. With several runners and no fleet the
  task is not started, because Tack cannot tell where it should run. Name a fleet in the profile.
- **Whether it asks.** A started run asks before each action unless **every** machine it could land on
  reports the agent as sandboxed, by `bwrap` or by the agent's own jail (docket). A machine that
  reports no sandbox, or says nothing, makes it ask. The person answers in Tack's inbox.
- **With no active runner,** or with several and no fleet, nothing starts. Enroll a runner or name a
  fleet first.

The automatic **merge** that follows is a separate job. It runs on the GitHub poll, so it needs both
`TACK_GITHUB_TOKEN` and a non-zero `TACK_GITHUB_POLL_SECONDS`, and a `prill` the board can run
(`TACK_PRILL_PROGRAM`). The project's Automation settings say when one of these is missing.
