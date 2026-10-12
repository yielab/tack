# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Beta](https://img.shields.io/badge/status-beta-yellow.svg)](CHANGELOG.md)

**A project board that hands tasks to your coding agent, and shows you what it did before
you merge.**

Write the task and what "done" means, press ▶, and Claude Code, Codex, opencode or docket
works on it on a branch of your repository. Every run, question and change stays on the task,
and you review the evidence before anything is merged. Tack runs on your computer, with the
agent logins you already have.

**[Download the desktop app](https://github.com/yielab/tack/releases/latest)** ·
[Website](https://yielab.com/tack) · [Documentation](https://yielab.github.io/tack/)

<p align="center">
  <img src="docs/screenshots/board.png" width="98%" alt="The Tack board: Kanban columns with WIP limits, a run button on every card, and two cards carrying an agent run's live state, Starting and Finished" />
</p>

> [!NOTE]
> Tack is in public beta. It works end to end, but it hasn't been run in production yet:
> expect rough edges and breaking changes between releases, and run `tack backup` before
> you upgrade. [Status](#status) says what has been proven so far.

## Get started

1. **Install the desktop app** from the
   [releases page](https://github.com/yielab/tack/releases/latest): `.AppImage` or `.deb`
   on Linux, `.dmg` on macOS (Apple Silicon and Intel), `.msi` on Windows.
2. **Open the Agents page.** It shows which agents this computer has and whether they are
   signed in, and runs a test with nothing typed.
3. **Create a project** pointed at the folder with your code, or import one from an outline
   or a CSV.
4. **Run a task.** Add its acceptance criteria and press ▶ **Run with agent**. When the task
   says *Needs your review*, read the diff and accept or reject it.

The [step-by-step tutorial](https://yielab.github.io/tack/user-guide/tutorial.html) walks this whole path with a
screenshot at every step.

> The desktop app and the binaries aren't code-signed yet. On macOS, right-click → **Open**
> the first time. On Windows, choose **More info → Run anyway**.

### Other ways to install

The same binary runs as a server, for a machine with no desktop, a host other people reach,
or the terminal. The data and the features are the same.

```bash
TAG=v0.1.0-beta.10
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/yielab/tack/releases/download/$TAG/install.sh" | TACK_VERSION="$TAG" sh
tack serve --with-runner        # then open http://localhost:3210
```

- **Windows (PowerShell):**
  `$tag = 'v0.1.0-beta.10'; $env:TACK_VERSION = $tag; irm "https://github.com/yielab/tack/releases/download/$tag/install.ps1" | iex`
- **Homebrew:** `brew install yielab/tap/tack`
- **Scoop, Docker, a release archive or from source:** see the
  [Quick Start](https://yielab.github.io/tack/user-guide/quick-start.html).

The installers check every download against the release's `SHA256SUMS`. `--with-runner`
runs agents on this machine; for a shared server, attach a runner where the code is
([Enrolling a runner](https://yielab.github.io/tack/user-guide/agent-runners.html#enrolling-a-runner)). Your data is
`tack.db` plus the `storage/` folder: see [Backup and restore](https://yielab.github.io/tack/user-guide/backup-restore.html).

## What Tack does

### Agents work on the task

- **Four agents, one flow.** Claude Code, Codex, opencode and docket. The task looks the
  same whichever one runs.
- **Your login or your key.** An agent runs with its own subscription (Claude Pro/Max, a
  ChatGPT plan) or an API key you give the runner, including a model server on your own
  machine. The board never holds a model credential.
- **It asks before it acts.** With *Ask me*, the agent waits on the task with its options,
  their risks and its recommendation. An agent with no sandbox must be able to ask.
- **A run survives a crash.** Kill a run halfway and Tack neither loses it nor starts it
  again behind your back. It stops and asks you.

<p align="center">
  <img src="docs/screenshots/workflow.gif" width="98%" alt="The whole workflow, recorded: a new project pointed at a folder on this computer, a task added with two acceptance criteria, Run with agent with Claude Code, the card's chip and the Execution tab following the run to 'Finished — needs your review', Accept, what the agent did, and the files from the run" />
</p>

### Review before you merge

- **Evidence on every run:** the patch, the commits and the files changed, kept after the
  workspace is gone.
- **Verifiable by something other than the agent.** A runner can hand that evidence to an
  independent verifier, a program of your own or an external checker such as prill. It
  writes a merge-readiness pack: a recommendation, the risk, each acceptance criterion with
  its evidence, and what it did not check.
- **Branch and pull request.** The runner can push the work as a branch, and for a task
  linked to a GitHub issue Tack opens the pull request.

### A project manager on its own

Six views (board, list, table, calendar, timeline, overview), Scrum, Kanban or phases,
dependencies, custom fields, your own words for *Task* or *Sprint*, and import and export
in JSON, YAML, CSV, GitHub Issues and Linear. All of it works with no agent set up.

### Reach it from anywhere

A CLI with JSON output, a REST API with an [OpenAPI spec](docs/openapi.json), an MCP server
(`tack mcp`) for Claude Code, Codex and other MCP clients, and two-way GitHub issue sync.

Every feature and option is in the [documentation](https://yielab.github.io/tack/).

## How it works

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/two-components-dark.svg">
    <img src="docs/diagrams/two-components-light.svg" width="720" alt="Two components: the board (one) on the left holds workflows, timelines, leases, fencing, and history; runners (many) on the right each launch a harness — Claude Code, Codex, docket, or opencode — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
  </picture>
</p>

The **board** plans, schedules and keeps the record; it never runs code. **Runners** live
where the code and the keys are, pull work from the board, and start the agents. The desktop
app runs both in one background process. Rust (Axum, SQLite) and SolidJS; the
[architecture overview](https://yielab.github.io/tack/developer/index.html) has the detail, and
[What the agent can reach](docs/book/src/user-guide/what-the-agent-can-reach.md) says what an
agent is given and what it is not.

## Status

Public beta, still in development. The project manager and the agent runs are both
complete, and the desktop app ships for Linux, macOS and Windows. Claude Code and Codex have
recorded live end-to-end runs, and the whole workflow has run through the UI with docket as
the agent and an independent verifier checking each run, against a local model.

What it still needs is wider testing: no team has run it in production yet, so the
limitations below are the ones known so far, not all there are.

- No user accounts: one shared API token, no per-user permissions
- One SQLite writer; S3 backup copies snapshots, it isn't live sync
- No offline mode and no native mobile app
- Only docket shows a run's steps on its timeline while it works; the other agents show
  them in the run's log
- Binaries aren't code-signed yet

The [roadmap](https://yielab.github.io/tack/roadmap.html) has what's next.

## Documentation

The full guide is at **[yielab.github.io/tack](https://yielab.github.io/tack/)**, built from
[`docs/book/`](docs/book/).

| | |
| --- | --- |
| [Quick Start](https://yielab.github.io/tack/user-guide/quick-start.html) · [Tutorial](https://yielab.github.io/tack/user-guide/tutorial.html) | Install, and a first run from start to finish |
| [Agent Runners](https://yielab.github.io/tack/user-guide/agent-runners.html) | Agents, credentials, models, verification, recovery |
| [Configuration](https://yielab.github.io/tack/user-guide/configuration.html) | Environment variables and `tack.toml` |
| [CLI](https://yielab.github.io/tack/user-guide/cli.html) · [API](https://yielab.github.io/tack/developer/api-reference.html) · [MCP](docs/MCP.md) | Reaching the board from outside the browser |
| [Architecture](https://yielab.github.io/tack/developer/index.html) · [Benchmarks](docs/BENCHMARKS.md) · [Testing](docs/TESTING.md) | How it is built and checked |

## Built alongside

- **[docket](https://github.com/yielab/docket)** gives a repository a team of coding agents
  and governs what they may do: roles, policies, approvals and an audit trail, in YAML next to
  your code. Tack runs it as one of its four agents.
- **prill** is an independent verifier of agents' changes. It re-runs the change on clean
  code in a sandbox with no network and writes a merge-readiness pack. It is one of the
  verifiers a Tack runner can use. It isn't published yet.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Building from source needs
[Rust 1.94+](https://rustup.rs/) and [Node.js 22+](https://nodejs.org/).

```bash
git clone https://github.com/yielab/tack.git && cd tack
git config core.hooksPath .githooks   # the same checks as CI, before every push
make dev                              # API + Vite hot reload
make test && make e2e                 # unit/integration and browser tests
```

## License

MIT — see [LICENSE](LICENSE).
