# Tack — plan the work, hand it to your coding agent, check what it did

<p align="center">
  <img src="docs/screenshots/board.png" width="98%" alt="The Tack board: Kanban columns with WIP limits, a run button on every card, and two cards carrying an agent run's live state, Starting and Finished" />
</p>

<p align="center">
  <a href="https://github.com/yielab/tack/actions/workflows/ci.yml"><img src="https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="CHANGELOG.md"><img src="https://img.shields.io/badge/status-beta-yellow.svg" alt="Status: beta"></a>
</p>

Tack is an open-source project board, with columns, tasks and sprints, that can hand a task
to an AI coding agent such as Claude Code or Codex. The agent works on your code, on your
computer, and everything it did comes back to the task: what it changed, what it asked you
and how many tokens it used. You read the result and decide whether it goes in.

Coding agents write code well, but their work lives in a terminal session that is gone when
it ends. Tack gives that work a place: the plan it came from, a record that stays, and a
review before anything reaches your code.

**Yours, on your machine.** Tack is a desktop app, or a single program you run as a server,
with all its data in one SQLite file. There is no account and no cloud service. The agent
signs in with its own login or a key you choose, and Tack never sees that credential.

[Website](https://yielab.com/tack) · [Documentation](https://yielab.github.io/tack/) · [Tutorial](https://yielab.github.io/tack/user-guide/tutorial.html) · [Releases](https://github.com/yielab/tack/releases/latest) · [Changelog](CHANGELOG.md)

> [!NOTE]
> Tack is in public beta. It works end to end, but it hasn't been run in production yet:
> expect rough edges and breaking changes between releases, and run `tack backup` before
> you upgrade. [Status](#status) says what has been proven so far.

## Install

**The desktop app** is the easiest way in. Download it from the [releases page](https://github.com/yielab/tack/releases/latest):

| System | File |
| --- | --- |
| Linux (x86_64) | `.AppImage` or `.deb` |
| macOS (Apple Silicon and Intel) | `.dmg` |
| Windows | `.msi` |

The app isn't code-signed yet. On macOS, right-click it and choose **Open** the first time;
on Windows, choose **More info → Run anyway**.

**From the command line**, for a server or if you prefer the terminal. It installs the same
program, which serves the same app in your browser:

```bash
# Linux / macOS
TAG=v0.1.0-beta.10
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/yielab/tack/releases/download/$TAG/install.sh" | TACK_VERSION="$TAG" sh
```

```powershell
# Windows PowerShell
$tag = 'v0.1.0-beta.10'; $env:TACK_VERSION = $tag; irm "https://github.com/yielab/tack/releases/download/$tag/install.ps1" | iex
```

Or `brew install yielab/tap/tack`. The installers check every download against the
release's checksums. Scoop, Docker, a release archive and building from source are in the
[Quick Start](https://yielab.github.io/tack/user-guide/quick-start.html).

## Quick start

**Before you start: install a coding agent.** Tack doesn't include one. It uses the agent
already on your computer, signed in the same way you would sign in to use it by hand. For
example:

```bash
npm install -g @anthropic-ai/claude-code && claude    # Claude Code: sign in when it asks
npm install -g @openai/codex && codex login           # Codex
```

The [list of supported agents](https://yielab.github.io/tack/user-guide/agent-runners.html#choosing-a-harness) says
what each one needs. Without an agent, Tack still works as a complete project board.

1. **Open Tack.** Start the desktop app, or run `tack serve --with-runner` and open
   <http://localhost:3210>.
2. **Check that Tack finds your agent.** Open **Agents** in the sidebar. It lists the agents
   installed on this computer; **Run test** runs one once to prove it works.
3. **Create a project.** Click **New Project**, give it a name, and under *Does this project
   have code?* choose the folder with your code. You decide whether the agent works on a new
   branch, leaving your folder as it is, or directly in the folder.
4. **Add a task and say what "done" means.** Click **+** in a column and give the task a
   title. Open it and add its **acceptance criteria**, one line for each thing that must be
   true when it's finished. The agent reads them along with the description.
5. **Run it.** Click ▶ **Run with agent**, then **Run**. The card shows the run's progress
   live; open it to follow along or to answer the agent if it asks something.
6. **Review the result.** When the agent has changed files, the task is marked **Needs your
   review**. Read the changes, then **Accept** or **Reject** them.

The [tutorial](https://yielab.github.io/tack/user-guide/tutorial.html) follows this same path with a screenshot at
every step. The whole loop, recorded:

<p align="center">
  <img src="docs/screenshots/workflow.gif" width="98%" alt="The whole workflow, recorded: a new project pointed at a folder on this computer, a task added with two acceptance criteria, Run with agent with Claude Code, the card's chip and the Execution tab following the run to 'Finished — needs your review', Accept, what the agent did, and the files from the run" />
</p>

## What you get

- **A complete project board.** Board, list, table, calendar and timeline views; Scrum,
  Kanban or phases; dependencies between tasks; and your own words for *Task* or *Sprint*.
  It's useful even with no agent at all.
- **A record of every run.** Progress while it works, the agent's questions, the changes,
  the files it produced and the tokens it used, kept on the task after the run ends.
- **A review before anything is merged.** A run that changed files waits for you. An
  independent verifier, a program of yours or a tool such as prill, can check the work
  against the acceptance criteria first, and the result can go out as a branch and a pull
  request.
- **An agent that asks first.** Turn on *Ask before each action* and the agent stops before
  each step, showing its options, their risks and what it recommends.
- **Runs that survive a crash.** If Tack or the agent stops halfway, the run is neither lost
  nor started again behind your back. Tack waits for you to decide.
- **Access from your own tools.** A command-line client, a REST API, an MCP server for AI
  assistants, and two-way sync with GitHub issues.

## How it fits together

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/two-components-dark.svg">
    <img src="docs/diagrams/two-components-light.svg" width="720" alt="Two components: the board (one) on the left holds workflows, timelines, leases, fencing, and history; runners (many) on the right each launch a harness — Claude Code, Codex, docket, or opencode — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
  </picture>
</p>

- The **board** holds your projects and tasks and keeps the record of every run. It never
  runs code and never holds a model credential.
- A **runner** lives where your code is. It takes tasks from the board, prepares a copy of
  the code and starts the agent.
- The **agent** does the work: Claude Code, Codex, opencode or docket.
- The **desktop app** runs the board and a runner together on your computer. As a server,
  one board can serve a team, with runners on other machines
  ([Enrolling a runner](https://yielab.github.io/tack/user-guide/agent-runners.html#enrolling-a-runner)).

## Security

An agent can do what its environment allows. On Linux with `bubblewrap` installed, Tack
runs each agent in a sandbox where it can change only its working copy and can't see your
SSH keys or Tack's own data. An agent without a sandbox must be able to ask you before it
acts, or it doesn't run. The board's API needs a token, and the board never holds a model
credential.

Read [What the agent can reach](docs/book/src/user-guide/what-the-agent-can-reach.md) and
[SECURITY.md](SECURITY.md) before you let other people or machines reach your Tack.

## Status

Tack is in public beta and still in development. The project board and the agent runs are
complete, and the desktop app ships for Linux, macOS and Windows. Claude Code and Codex have
completed real end-to-end runs, and the whole workflow has run through the app with docket
as the agent and an independent verifier checking each run.

It still needs wider testing: no team has run it in production yet, so these are the
limitations known so far, not all there are.

- No user accounts: one shared API token, no per-user permissions
- One database writer; backups to S3 are snapshots, not live sync
- No offline mode and no mobile app
- Only docket shows an agent's steps on the task while it works; the other agents show them
  in the run's log
- The app and the binaries aren't code-signed yet

The [roadmap](https://yielab.github.io/tack/roadmap.html) has what's next.

## Documentation

| Goal | Start here |
| --- | --- |
| Install and run a first task | [Quick Start](https://yielab.github.io/tack/user-guide/quick-start.html) · [Tutorial](https://yielab.github.io/tack/user-guide/tutorial.html) |
| Set up agents, logins and models | [Agent runners](https://yielab.github.io/tack/user-guide/agent-runners.html) |
| Organize work on the board | [Items](https://yielab.github.io/tack/user-guide/items.html) · [Views](https://yielab.github.io/tack/user-guide/views.html) · [Workflows](https://yielab.github.io/tack/user-guide/workflows.html) |
| Move data in and out | [Import and export](https://yielab.github.io/tack/user-guide/import-export.html) · [Backup and restore](https://yielab.github.io/tack/user-guide/backup-restore.html) |
| Use Tack from the terminal or other tools | [CLI](https://yielab.github.io/tack/user-guide/cli.html) · [API](https://yielab.github.io/tack/developer/api-reference.html) · [MCP](docs/MCP.md) |
| Configure or run a server | [Configuration](https://yielab.github.io/tack/user-guide/configuration.html) · [Architecture](https://yielab.github.io/tack/developer/index.html) |

## Built alongside

- **[docket](https://github.com/yielab/docket)** gives a repository a team of coding agents
  and governs what they may do: roles, policies, approvals and an audit trail, in YAML next to
  your code. Tack can run it as the agent on a task.
- **prill** is an independent verifier of agents' changes. It re-runs the change on clean
  code in a sandbox with no network and writes a merge-readiness report. It is one of the
  verifiers a Tack runner can use. It isn't published yet.

## Development

Building from source needs [Rust 1.94+](https://rustup.rs/) and
[Node.js 22+](https://nodejs.org/).

```bash
git clone https://github.com/yielab/tack.git && cd tack
git config core.hooksPath .githooks   # the same checks as CI, before every push
make dev                              # API + Vite hot reload
make test && make e2e                 # unit/integration and browser tests
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how changes are made and reviewed.

## License

MIT — see [LICENSE](LICENSE).
