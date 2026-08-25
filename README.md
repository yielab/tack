# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

**Local-first project management with a harness-agnostic control plane for agent work.**

Tack is a self-hosted project manager delivered as one Rust binary with an embedded
web UI, REST API, CLI, MCP server, and SQLite database. It is being extended so a
project item can become a durable execution request assigned to a fleet of agents,
without making the project depend on one agent CLI, model provider, or model.

The intended product is one place where a person can:

1. plan work using configurable projects, workflows, and vocabulary;
2. decide which work may be executed by agents;
3. choose an eligible fleet and, when policy allows it, a harness, provider, and model;
4. review attempts, decisions, artifacts, failures, and measured usage on the item; and
5. retain an auditable history even when a runner disconnects or a harness is replaced.

> **Development status:** Tack's project-management foundation is available today.
> The native runner fleet described below — **Phases 50–56** of the roadmap — is
> implemented on the development branch: durable execution requests, `tack-runner`,
> real harness adapters, fleet scheduling, decisions, artifacts, and model profiles all
> exist and are tested. It is **not yet a released feature**: no tagged release ships
> it, and **Phase 57** (release hardening and the optional legacy Docket bridge) is
> in progress — the runner's HTTP transport, per-attempt checkouts, event/decision/
> artifact submission, and fleet-membership write routes are all built and gated
> green; the release tag is blocked only on installing the `codex` CLI on the build
> machine so the three-harness live smoke can run end to end. The existing Docket
> integration is retained as a legacy optional bridge, not as the architecture of the
> new control plane.

Built with Rust (Axum + sqlx), SolidJS, and SQLite.

![Board, Timeline, command palette, and vocabulary editor](docs/screenshots/hero.gif)

<details>
<summary>More screenshots</summary>

![Board — Kanban with WIP limits and drag-and-drop](docs/screenshots/board.png)
![Timeline — Gantt view with draggable bars](docs/screenshots/timeline.png)
![Dashboard — status distribution and sprint throughput](docs/screenshots/dashboard.png)
![List — sortable rows with inline editing and bulk operations](docs/screenshots/list.png)
![Vocabulary editor — rename any term to match your domain](docs/screenshots/settings-vocabulary.png)

</details>

---

## Product direction

Tack remains the plan of record and owns scheduling, policy, leases, and execution
history. A lightweight `tack-runner` runs near the source repository and credentials,
claims eligible work, and invokes a local harness through an adapter.

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
| **Provider / model** | Model endpoint and model selected only when the harness supports that choice. |
| **Attempt** | Immutable execution history for one lease and run. |
| **Decision** | Structured request for human input or authorization. |

The core rules are deliberately conservative:

- Runners pull work; Tack does not send arbitrary callbacks into developer machines.
- Compatibility is capability-driven. Tack does not pretend every harness supports
  every provider, model, resume mode, usage metric, or approval mechanism.
- A request has at most one valid active lease in v1, enforced by a fencing token.
- Delivery is not advertised as exactly-once. Ambiguous outcomes stop for operator
  review instead of being retried blindly.
- Missing usage is shown as **not measured**, never as zero and never as observed cost.
- Docket can survive as an optional adapter, but native execution does not depend on it.
- Multi-agent fan-out inside one request is deferred until single-runner recovery and
  audit semantics are proven.

The complete design and acceptance gates are in the
[roadmap](docs/book/src/roadmap.md#next--harness-agnostic-runner-fleet-phases-5057).
Implementation cards intended for independent Terra agents are in
[TODO Part III](TODO.md#part-iii--harness-agnostic-runner-fleet-phases-5057).

## Delivery status

Tack is in beta. Completed work is preserved in the roadmap; superseded phases remain
there as design history instead of being rewritten as completed work.

| Phases | Status | What that means |
| --- | --- | --- |
| **0–32** | **Complete** | Core PM product, integrations, hardening, and audit-driven work are documented as done. |
| **33–38** | **Complete** | The optional Docket-based factory control-center cycle is done. It is now a legacy integration path. |
| **39–40** | **Implemented, unreleased** | Regression oracle plus capability/adapter foundations exist in the current working tree. |
| **41** | **Acceptance closed, unreleased** | Atomic-write and browser-ETag acceptance, previously reopened, are now closed: item `PATCH` runs as one transaction proven with failure-injection tests, and the browser sends `If-Match` and handles `412` with a refresh-and-retry flow. |
| **42** | **Acceptance closed, unreleased** | Provider-scoped identity acceptance, previously reopened, is now closed: migrations 037/038 rebuild `orch_runs`/`orch_approvals` as a transactional copy/verify/swap with checksum and pre-upgrade snapshot safety, replacing a boot-loop risk with recovery. |
| **43–49** | **Superseded or frozen** | Do not implement this old control-plane sequence; its useful outcomes were re-scoped into Phases 50–57. |
| **50–57** | **Phases 50–56 delivered, unreleased; 57 in progress** | Native harness-agnostic runner fleet — execution domain, runner protocol, real harness adapters, fleet scheduling, decisions/artifacts, and model profiles. Not shipped in any tagged release; the tag is blocked only on installing `codex` locally to complete the three-harness live smoke. |

The active cycle is:

| Phase | Outcome | Status |
| --- | --- | --- |
| **50** | Boundary, safety, migration, and contract freeze. | Delivered, unreleased |
| **51** | Durable execution domain and schema. | Delivered, unreleased |
| **52** | Pull-based runner protocol and `tack-runner` skeleton. | Delivered, unreleased |
| **53** | Real Codex, Claude Code, and OpenCode harness proofs. | Delivered, unreleased |
| **54** | Fleet scheduler and item-assignment UX. | Delivered, unreleased |
| **55** | Decisions, artifacts, and realtime execution activity. | Delivered, unreleased |
| **56** | Model profiles, policy enforcement, and honest measured usage. | Delivered, unreleased |
| **57** | Optional Docket bridge, recovery testing, and release hardening. | In progress — runner HTTP transport, per-attempt checkouts, event/decision/artifact submission, and fleet-membership write routes delivered and gated; blocked on installing `codex` for the live smoke |

## What works today

### Project management

- Configurable workflows, columns, transition rules, and WIP limits.
- Per-project vocabulary so the UI, CLI, and API use the terms of the domain.
- Board, list, table, calendar, timeline, dashboard, and sprint-planning views.
- Hierarchical items, dependency DAGs with cycle detection, custom fields, comments,
  attachments, full-text search, templates, and bulk operations.
- Realtime WebSocket updates and optimistic UI behavior.
- JSON, YAML, and CSV export; GitHub Issues and Linear import; hot and S3-compatible
  backup/restore.

### Automation surfaces

- A REST API described by the checked-in [OpenAPI specification](docs/openapi.json).
- A CLI with JSON output and shell completions.
- `tack mcp`, which lets Claude Code, Codex, and other MCP clients read and update the
  project board through normal workflow validation.
- Outbound signed webhooks and optional GitHub push sync.
- An optional, feature-gated Docket integration from the completed Phases 33–38.

`tack mcp` and the runner fleet solve different problems: MCP gives an interactive
agent tools for managing the board; the runner fleet — implemented on the
development branch through Phase 57's release-hardening work, not yet released —
lets Tack durably schedule, lease, observe, and recover agent execution.

## Quick start

**One line (Linux / macOS):**

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack            # starts the server + web UI at http://localhost:3210
```

**With Cargo** (builds the single binary with the UI embedded):

```bash
cargo install --git https://github.com/yielab/tack tack-cli --features embed-spa
```

**Or download** the archive for your system from the
[releases page](https://github.com/yielab/tack/releases):

```bash
# Linux / macOS
tar xzf tack-*.tar.gz
cd tack-*/
./tack
```

**Windows:** extract the zip and run `tack.exe`.

Open **`http://localhost:3210`**. Project data lives in `tack.db`; attachments live in
`storage/`. Back up both.

> The binary is not code-signed yet. On macOS, right-click **Open** the first time
> (or run `xattr -d com.apple.quarantine tack`). On Windows, use
> **More info → Run anyway** if SmartScreen appears.

## Current limitations

| Area | Current state |
| --- | --- |
| Authentication | One optional shared Bearer token; no per-user identities or permissions. |
| Multi-device data | One active SQLite writer. S3 backup is snapshot replication, not live multi-writer sync. |
| Offline UI | The browser UI requires its Tack server to be running. |
| Native runner fleet | Implemented through Phase 56 on the development branch (execution domain, `tack-runner`, harness adapters, fleet scheduling, decisions, artifacts, model profiles); not shipped in any tagged release. Phase 57 (release hardening, optional legacy bridge) is in progress and gated green; the release tag is blocked only on installing `codex` locally for the three-harness live smoke, and some execution-domain behavior (for example, decision resolution) requires its own separate configuration. |
| Existing orchestration | Docket-specific and disabled by default. It is a legacy bridge, not proof of harness-agnostic execution. |
| Usage and cost | Existing imported values may be estimates or absent. Native telemetry must label measurement source; absent usage is not zero. |
| Mobile | Responsive web UI only; no native mobile application. |
| Binary signing | Release binaries are not code-signed yet. |

## Build from source

**Prerequisites:** [Rust 1.85+](https://rustup.rs/) (2024 edition) and
[Node.js 20+](https://nodejs.org/).

```bash
git clone https://github.com/yielab/tack.git
cd tack
make build   # builds the frontend and embeds it into the release binary
make run     # starts Tack at http://127.0.0.1:3210
```

For development with hot reload:

```bash
make dev     # API + Vite dev server at http://localhost:5173
```

Useful verification commands:

```bash
make test
make e2e
make audit
make lint
make fmt
make help
```

## Configuration

Configuration is loaded from `tack.toml` in the working directory or from environment
variables.

| Variable | Default | Description |
| --- | --- | --- |
| `TACK_HOST` | `127.0.0.1` | Bind address. |
| `TACK_PORT` | `3210` | HTTP port. |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite connection URL. |
| `TACK_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, or `error`. |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory. |
| `TACK_API_TOKEN` | _(none)_ | Bearer token required for `/api/*` when configured. |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook destination. |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 webhook signing key. |
| `TACK_BACKUP_BUCKET` | _(none)_ | S3 bucket used for cloud backup. |
| `TACK_ORCH_ENABLE` | `false` | Enables the current legacy Docket orchestration routes and reconciler. |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Separate secret for current Docket approval actions. |

See the [configuration guide](docs/book/src/user-guide/configuration.md) and
[administration guide](docs/book/src/user-guide/administration.md) for the full
reference. `tack-runner` configuration (API URL, enrollment token, state directory)
shipped with Phases 51–52 on the development branch; a full operator reference is
part of Phase 57's remaining release-hardening work.

## Architecture

The current application is a modular monolith:

```text
tack-core   Domain models, workflow rules, vocabulary, and dependency graph (no I/O)
    ↑
tack-db     SQLite persistence through sqlx, FTS5, and repositories
    ↑
tack-orch   Current optional Docket client and reconciliation boundary
    ↑
tack-api    Axum HTTP/WebSocket server, configuration, and integrations
    ↑
tack-cli    Server binary, CLI client, embedded SolidJS application, and MCP server
```

Phases 50–56 added a pull runner protocol and harness adapters, running as a separate
`tack-runner` binary rather than moving PM domain logic into this stack; Phase 57
(release hardening, optional Docket bridge) is in progress, blocked on installing
`codex` locally for the three-harness live smoke. See the
[developer architecture overview](docs/book/src/developer/README.md) for the current
code and the [roadmap](docs/book/src/roadmap.md) for the target boundary.

## Documentation

Full documentation is in [`docs/book/`](docs/book/). Build it locally with
[mdBook](https://rust-lang.github.io/mdBook/):

```bash
cargo install mdbook
mdbook serve docs/book
```

| Guide | Description |
| --- | --- |
| [Quick Start](docs/book/src/user-guide/quick-start.md) | First-run walkthrough. |
| [API Reference](docs/book/src/developer/api-reference.md) | Auth and API examples; OpenAPI is the machine-readable source of truth. |
| [CLI Reference](docs/book/src/user-guide/cli.md) | `tack` subcommands. |
| [MCP Server](docs/MCP.md) | Connect Tack to an interactive MCP-capable agent. |
| [Current Docket integration](docs/book/src/user-guide/orchestration.md) | Existing optional orchestration behavior and limitations. |
| [Configuration](docs/book/src/user-guide/configuration.md) | Environment and `tack.toml` reference. |
| [Architecture](docs/book/src/developer/README.md) | Current crate boundaries and design decisions. |
| [Benchmarks](docs/BENCHMARKS.md) | Reproducible footprint and latency measurements. |
| [Testing](docs/TESTING.md) | Unit, integration, E2E, load, and security tests. |
| [Roadmap](docs/book/src/roadmap.md) | Historical phase status and active Phases 50–57. |
| [Execution TODO](TODO.md#part-iii--harness-agnostic-runner-fleet-phases-5057) | Parallel-agent implementation cards for the active cycle. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to report bugs, propose features, and
submit pull requests.

Before pushing, activate the pre-push hook that runs formatting and Clippy:

```bash
git config core.hooksPath .githooks
```

## License

MIT — see [LICENSE](LICENSE).
