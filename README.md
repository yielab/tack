# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.94%2B-orange.svg)](https://www.rust-lang.org/)
[![Beta](https://img.shields.io/badge/status-beta-yellow.svg)](CHANGELOG.md)

**A project board that can hand its items to a coding agent — Claude Code, Codex,
docket or opencode — and keep the run on the item's record.**
Self-hosted. One binary, one SQLite file, no account.

<p align="center">
  <img src="docs/screenshots/board.png" width="98%" alt="The Tack board: Kanban columns with WIP limits, a run button on every card, and a banner offering to let this board run its items with an agent" />
</p>

## Why Tack

Coding agents already do the work well. What they lack is a plan to work from and a
record of what they did. Tack is both: a project board worth using on its own, that
can hand any item to the agent you already use and keep the result on the item.

- **Your plan and your agent in one place.** Press ▶ on a card, pick an agent, and
  it runs. What happened — events, questions, files, cost — stays on the card.
- **A run survives a crash.** Kill a run halfway through and Tack neither loses it
  nor starts it again behind your back. It stops and asks you.
- **Your keys never leave your machine.** The board never holds a model credential.
  The runner that does lives where your code already is.
- **Numbers are measured, never guessed.** Cost and tokens show what the agent
  reported, or say *Not measured*. Never an estimate, never a silent `0`.
- **Nothing else to run.** An 18.5 MiB binary using about 18 MiB of memory at rest
  ([Benchmarks](docs/BENCHMARKS.md)). No database server, no Docker, no cloud.

## Run an item with an agent

1. **Plan it on the board.** Create the item and pick who runs it.
2. **A runner picks it up.** It checks out a clean workspace and starts the agent
   with its own login or a key you gave it.
3. **Watch it on the item.** Progress arrives live. The finished run stays in the
   item's history.

<p align="center">
  <img src="docs/screenshots/hero.gif" width="98%" alt="A board item assigned to Claude Code through Run with agent, tracked live from Leased to Succeeded, with its Execution tab showing the matched model and measured cost" />
</p>

**Four harnesses, one flow.** Claude Code, Codex, docket and opencode, each driven
through its own adapter. The item looks the same whichever one runs.
[Choosing a harness](docs/book/src/user-guide/agent-runners.md#choosing-a-harness) says
what each needs and what each can't do.

**You choose how it's paid for.** Use the agent's own subscription (Claude Pro/Max, a
ChatGPT plan) and Tack never sees the credential. Or give the runner an API key for an
endpoint — Anthropic's API directly, or a gateway such as Vercel AI Gateway. The model
list comes live from that endpoint, not from a hand-kept file.

**It can ask first.** With approvals set to *Ask me*, Claude Code, Codex and opencode
pause before acting and wait in the item's decision inbox.

**The Agents page gets you started.** From installed binary to a finished test run on
one page: turn execution on, see which agents this machine has, check their login, pick
a default model, try it.

### A run survives a crash

A runner killed mid-run. The attempt turns `needs_operator` instead of running twice;
you decide, and the retry succeeds. Recorded from a real release binary.

<p align="center">
  <img src="docs/screenshots/recovery-demo.gif" width="98%" alt="An attempt is killed mid-run; the board shows needs_operator with no blind duplicate; an operator reconciles it with an explicit decision; the retry succeeds — recorded from a real GitHub Release binary running in Docker, not a development build" />
</p>

[`scripts/record-recovery-demo.sh`](scripts/record-recovery-demo.sh) re-records it, and
[`scripts/smoke.sh`](scripts/smoke.sh) checks the same sequence on every run.

## A full project manager, with or without agents

Everything here works the moment Tack starts, with no agent set up.

<p align="center">
  <img src="docs/screenshots/timeline.png" width="49%" alt="Timeline — a dependency-aware Gantt view; the same dependency graph decides which items an agent may pick up" />
  <img src="docs/screenshots/list.png" width="49%" alt="List — every item as a row with its type, priority and workflow status, sortable and editable in place" />
</p>

- **Six views:** board, list, table, calendar, timeline and overview.
- **Your workflow:** Scrum, Kanban or phases, with WIP limits.
- **Your words.** Rename *Task*, *Sprint* or *Backlog* per project. The UI, CLI and
  API all use your terms.
- **Dependencies that matter.** Cycles are rejected, and an item waiting on unfinished
  work isn't handed to an agent.
- **The rest you'd expect:** sub-items, custom fields, comments, attachments, full-text
  search, templates, bulk edits, live updates across tabs.
- **Your data stays portable.** Export to JSON, YAML or CSV. Import from GitHub Issues
  or Linear. Back up locally or to S3-compatible storage.

<details>
<summary>More screenshots: overview and vocabulary</summary>
<br>

![Overview — total items, completion rate, and status and priority distribution](docs/screenshots/dashboard.png)

![Vocabulary editor — rename any term (Task, Sprint, Epic, Backlog…) to match your domain; blank falls back to the default label](docs/screenshots/settings-vocabulary.png)

</details>

## Reach it from anywhere

- **CLI** with JSON output. `tack start <id>` moves an item to in progress and checks
  out its branch.
- **REST API** with a checked-in [OpenAPI spec](docs/openapi.json).
- **MCP server** (`tack mcp`): Claude Code, Codex and other MCP clients can read and
  update the board, within the same workflow rules.
- **GitHub sync:** a linked issue's state and comments flow both ways. Signed outbound
  webhooks for everything else.
- **Desktop app** with a tray icon, and `tack service install` to keep Tack running in
  the background. Closing the window doesn't stop the work.

## Get started

**Desktop app.** Download it from the
[releases page](https://github.com/yielab/tack/releases): `.AppImage` or `.deb` on
Linux, `.dmg` on macOS (Apple Silicon and Intel), `.msi` on Windows. Open it and the
board opens in its own window.

**Or the binary**, for servers or if you'd rather use a terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack serve --with-runner
```

Open **<http://localhost:3210>**. The installer checks every download against the
release's `SHA256SUMS`. Other ways to install — Cargo, a release archive, Windows —
are in the [Quick Start](docs/book/src/user-guide/quick-start.md).

**About `--with-runner`:** it starts a runner inside the same process, so agent runs
work right away. Without it, the board works fully and you can turn agents on later
from the Agents page, no restart needed. It only runs on `localhost`, because it starts
agent processes. For a shared server, attach a separate `tack-runner` instead:
[Enrolling a runner](docs/book/src/user-guide/agent-runners.md#enrolling-a-runner).

Your data is `tack.db` plus the `storage/` folder. Back up both.

> Binaries and the desktop app aren't code-signed yet. On macOS, right-click → **Open**
> the first time. On Windows, choose **More info → Run anyway**.

## How it works

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/two-components-dark.svg">
    <img src="docs/diagrams/two-components-light.svg" width="720" alt="Two components: the board (one) on the left holds workflows, timelines, leases, fencing, and history; runners (many) on the right each launch a harness — Claude Code, Codex, docket, or opencode — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
  </picture>
</p>

- **The board** plans, schedules and keeps the record. It never runs code and never
  holds a model key.
- **Runners** live where the code and keys are — your laptop, a CI box, a GPU machine.
  They pull work; the board never calls out to them.
- **One board, many runners.** A dead runner can't corrupt the board: its lease
  expires and its writes stop. A restarted board can't lose a run: the runner's journal
  knows what it started.

Rust (Axum, SQLite) and SolidJS, as a modular monolith with the runner as its own
binary. The [architecture overview](docs/book/src/developer/README.md) has the detail.

## Status

Public beta. The project manager and the agent runs are both complete, and
`v0.1.0-beta.9` is the first release with the desktop app. Claude Code and Codex have
recorded live end-to-end runs. `tack runner doctor` shows what your own machine can
run.

### Known limitations

- No user accounts: one optional shared token, no per-user permissions
- One SQLite writer. S3 backup copies snapshots; it isn't live sync
- No offline mode and no native mobile app
- Imported cost values may be estimates; Tack's own measurements always say where
  they came from
- Binaries aren't code-signed yet

The [roadmap](docs/book/src/roadmap.md) has what's next.

## Documentation

The full guide is at [yielab.github.io/tack](https://yielab.github.io/tack/)
(source in [`docs/book/`](docs/book/)).

| | |
| --- | --- |
| [Quick Start](docs/book/src/user-guide/quick-start.md) | Install and first run |
| [Agent Runners](docs/book/src/user-guide/agent-runners.md) | Harnesses, credentials, models, recovery |
| [CLI](docs/book/src/user-guide/cli.md) · [API](docs/book/src/developer/api-reference.md) · [MCP](docs/MCP.md) | Reaching the board from outside the browser |
| [Configuration](docs/book/src/user-guide/configuration.md) | Environment variables and `tack.toml` |
| [Architecture](docs/book/src/developer/README.md) | Crates, boundaries and decisions |
| [Benchmarks](docs/BENCHMARKS.md) · [Testing](docs/TESTING.md) | How the numbers and the tests are produced |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Building from source needs
[Rust 1.94+](https://rustup.rs/) and [Node.js 22+](https://nodejs.org/).

```bash
git clone https://github.com/yielab/tack.git && cd tack
git config core.hooksPath .githooks   # fmt + clippy before every push, like CI
make dev                              # API + Vite hot reload
make test && make e2e                 # unit/integration and browser tests
```

## License

MIT — see [LICENSE](LICENSE).
