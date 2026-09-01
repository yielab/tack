# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![Beta](https://img.shields.io/badge/status-beta-yellow.svg)](CHANGELOG.md)

**A self-hosted project manager that can run the work it tracks.** Assign a board item
to Claude Code, Codex, or OpenCode and it executes as a durable, auditable attempt —
right inside the same project history, in one binary.

![Board, Timeline, and vocabulary editor](docs/screenshots/hero.gif)

**Jump to:** [Features](#features) · [Screenshots](#screenshots) ·
[Requirements](#requirements) · [Run it](#run-it) · [Status](#status) ·
[Architecture](#architecture) · [Documentation](#documentation) ·
[Contributing](#contributing)

---

## Why Tack

Most open-source project managers — Plane, Huly, Focalboard, Leantime, Vikunja —
track work but execute none of it. The open-source agent orchestrators that used to
sit alongside them have mostly shut down or gone closed-source. Tack does both at
once: a real project manager (boards, timelines, dashboards, per-project vocabulary)
and a real, multi-harness agent execution fleet, in one self-hosted binary with no
external services.

## Features

### Project management

- Configurable workflows — Scrum, Kanban, or phase-based — with per-project vocabulary
  so the UI, CLI, and API all speak the language of your domain
- Board, list, table, calendar, timeline, and dashboard views
- Hierarchical items, dependency DAGs with cycle detection, custom fields, comments,
  attachments, full-text search, templates, and bulk operations
- Realtime updates over WebSocket with optimistic UI
- JSON/YAML/CSV export, GitHub Issues and Linear import, hot and S3-compatible backup

### Agent execution

- Assign a board item to `claude-code`, `codex`, or `opencode` and it runs as a
  tracked, durable attempt — not a fire-and-forget shell command
- Pull-based runner protocol: a lightweight `tack-runner` claims eligible work and
  executes near its own repo and credentials; Tack never calls into your machine
- Fenced leases (one active attempt per request), decisions for human-in-the-loop
  approval, and structured artifacts on every run
- Measured usage only — cost and token counts are shown as measured or explicitly
  **not measured**, never estimated or silently shown as zero

### Automation surfaces

- A REST API described by a checked-in [OpenAPI spec](docs/openapi.json), plus a CLI
  with JSON output and shell completions
- `tack mcp` — lets Claude Code, Codex, and other MCP clients read and update the
  board through normal workflow validation
- Outbound signed webhooks and optional GitHub push sync

## Screenshots

<p align="center">
  <img src="docs/screenshots/board.png" width="49%" alt="Board — Kanban with WIP limits and drag-and-drop" />
  <img src="docs/screenshots/timeline.png" width="49%" alt="Timeline — Gantt view with draggable bars" />
</p>
<p align="center">
  <img src="docs/screenshots/dashboard.png" width="49%" alt="Dashboard — status distribution and sprint throughput" />
  <img src="docs/screenshots/list.png" width="49%" alt="List — sortable rows with inline editing" />
</p>

<details>
<summary>Vocabulary editor — rename any term to match your domain</summary>
<br>

![Vocabulary editor](docs/screenshots/settings-vocabulary.png)

</details>

## Requirements

Running a release binary needs nothing else: SQLite and the web UI are embedded in
the single `tack` binary — no Docker, no database server, no separate frontend build.

| | |
| --- | --- |
| **Platform** | Linux, macOS (Intel + Apple Silicon), Windows |
| **Browser** | Any current Chrome, Firefox, Safari, or Edge |
| **Footprint** | 10.3 MiB binary (UI embedded), ~11.7 MiB idle memory — measured in [Benchmarks](docs/BENCHMARKS.md) |

Building from source instead needs [Rust 1.85+](https://rustup.rs/) and
[Node.js 20+](https://nodejs.org/).

## Run it

**One line** (Linux / macOS):

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack serve --with-runner
```

**With Cargo** — installs from the `develop` branch, the single binary with the UI
embedded:

```bash
cargo install --git https://github.com/yielab/tack tack-cli --features embed-spa
tack serve --with-runner
```

**Or download** the archive for your system from the
[releases page](https://github.com/yielab/tack/releases):

```bash
tar xzf tack-*.tar.gz && cd tack-*/
./tack serve --with-runner
```

**Windows:** extract the zip and run `tack.exe serve --with-runner`.

Open **`http://localhost:3210`**. Project data lives in `tack.db`; attachments live in
`storage/`. Back up both.

`--with-runner` self-provisions an agent runner inside the same process — no second
binary, no token to copy anywhere — so a board item assigned to `claude-code`,
`opencode`, or `codex` (whichever of those you have installed and logged in) actually
executes. It's off by default and loopback-only; see
[Agent Runners](docs/book/src/user-guide/agent-runners.md#standalone-mode-tack-serve---with-runner)
and [`docs/CONFIG.md`](docs/CONFIG.md#embedded-runner-tack-serve---with-runner). Plain
`tack` (no flag) starts just the server and board UI, with no runner.

> The binary is not code-signed yet. On macOS, right-click **Open** the first time
> (or run `xattr -d com.apple.quarantine tack`). On Windows, use
> **More info → Run anyway** if SmartScreen appears.

## Status

Tack is in public beta. The core project-management product — workflows, views,
DAGs, search, backup — is complete. The native, harness-agnostic agent-execution
fleet described above is implemented and gated behind `--with-runner`, but has not
shipped in a tagged release yet; install it today from the `develop` branch with the
Cargo command above.

**Harness proof** — every row below is checked against the actual installed binaries
on a real machine, not mocked out; see `docs/agent-handoffs/part-v/V-A2.md` for the
full evidence, including reverted-fix proofs and live run logs.

| Harness | Status |
| --- | --- |
| `claude-code` | Completes real, live end-to-end attempts today. |
| `opencode` | Completes real, live end-to-end attempts today. |
| `codex` | Runs the complete pipeline — claim, checkout, spawn, real network call, structured result — but has not completed a live attempt: this machine's connected account doesn't have access to the requested model. That's an account-tier limitation, not a code defect. |

**Known limitations:**

- One optional shared Bearer token; no per-user identities or permissions
- One active SQLite writer; S3 backup is snapshot replication, not live multi-writer sync
- The browser UI requires its Tack server to be running — no offline mode
- The existing Docket integration is a legacy, disabled-by-default bridge, not proof
  of harness-agnostic execution
- Imported usage/cost values may be estimates; native telemetry always labels its
  measurement source
- Responsive web UI only — no native mobile application
- Release binaries are not code-signed yet

Full phase-by-phase history lives in the [roadmap](docs/book/src/roadmap.md); the
active board is [`TODO.md`](TODO.md).

## Architecture

Tack owns scheduling, policy, leases, and execution history. A lightweight
`tack-runner` runs near the source repository and credentials, claims eligible work,
and invokes a local harness through an adapter:

```text
PM item
   │ create execution request
   ▼
Tack scheduler ── policy + capability matching ──► eligible fleet
   │                                                   │
   │ lease with fencing token                          │ runner claims work
   ▼                                                   ▼
durable attempt ◄── events / decisions / artifacts ─ tack-runner
                                                        │
                                   HarnessAdapter ──────┼──────┐
                                                        │      │
                                                   Codex CLI  Claude Code
                                                        │      │
                                                    OpenCode  future harnesses
```

This separates concepts that must not be conflated:

| Concept | Responsibility |
| --- | --- |
| **PM item** | Human-facing unit of planned work. |
| **Execution request** | Durable request to perform an item, with policy and eligibility constraints. |
| **Fleet** | Schedulable pool of runners with declared capabilities. |
| **Runner** | Worker process that leases work and executes it near its repo and credentials. |
| **Harness** | Agent runtime such as Codex CLI, Claude Code, or OpenCode. |
| **Attempt** | Immutable execution history for one lease and run. |
| **Decision** | Structured request for human input or authorization. |

Rules that hold everywhere: runners pull work, Tack never calls back into developer
machines; compatibility is capability-driven, not assumed; a request has at most one
valid active lease, enforced by a fencing token; ambiguous outcomes stop for operator
review instead of being retried blindly.

The application itself is a modular monolith, with the runner fleet as a separate
binary rather than a new layer inside it:

```text
tack-core   Domain models, workflow rules, vocabulary, and dependency graph (no I/O)
    ↑
tack-db     SQLite persistence through sqlx, FTS5, and repositories
    ↑
tack-orch   Optional Docket client and reconciliation boundary
    ↑
tack-api    Axum HTTP/WebSocket server, configuration, and integrations
    ↑
tack-cli    Server binary, CLI client, embedded SolidJS app, and MCP server

tack-runner Separate binary — pull-based protocol, credentials, harness adapters
```

See the [developer architecture overview](docs/book/src/developer/README.md) for the
current code and the [roadmap](docs/book/src/roadmap.md) for where it's headed.

## Documentation

Full documentation is in [`docs/book/`](docs/book/), built with
[mdBook](https://rust-lang.github.io/mdBook/) and published to
[yielab.github.io/tack](https://yielab.github.io/tack/) on every push to `develop`.

| Guide | Description |
| --- | --- |
| [Quick Start](docs/book/src/user-guide/quick-start.md) | First-run walkthrough. |
| [API Reference](docs/book/src/developer/api-reference.md) | Auth and API examples; OpenAPI is the machine-readable source of truth. |
| [CLI Reference](docs/book/src/user-guide/cli.md) | `tack` subcommands. |
| [MCP Server](docs/MCP.md) | Connect Tack to an interactive MCP-capable agent. |
| [Configuration](docs/book/src/user-guide/configuration.md) | Environment and `tack.toml` reference. |
| [Architecture](docs/book/src/developer/README.md) | Current crate boundaries and design decisions. |
| [Benchmarks](docs/BENCHMARKS.md) | Reproducible footprint and latency measurements. |
| [Testing](docs/TESTING.md) | Unit, integration, E2E, load, and security tests. |
| [Roadmap](docs/book/src/roadmap.md) | Phase-by-phase status, past and active. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to report bugs, propose features, and
submit pull requests. Quick start for local development:

```bash
git clone https://github.com/yielab/tack.git
cd tack
git config core.hooksPath .githooks   # runs fmt + clippy before every push, like CI
make build                            # frontend + release binary
make dev                              # API + Vite hot reload
make test && make e2e && make audit   # unit/integration, browser, dependency checks
```

## License

MIT — see [LICENSE](LICENSE).
