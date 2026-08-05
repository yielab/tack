# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

**Tack** is a self-hosted project management tool that runs entirely from a single binary. Drop it on any machine, run it, and you have a full-featured project manager — web UI, REST API, and database all in one file. No Docker, no database server, no cloud accounts, no subscriptions.

Most project management tools force you into their workflow. Tack works the other way: you define the vocabulary and status columns that match your domain, and every surface — the UI, the CLI, and the API — adapts to your terms. The same tool works for a software sprint, a construction phase plan, a thesis outline, or a home renovation, without switching apps or losing context between projects.

**Your data stays on your machine.** Everything is stored in a single SQLite file next to the binary. Back it up by copying one file. Migrate by moving two files. No vendor lock-in, no sync, no network required to use your own data.

**AI-agent-ready.** `tack mcp` exposes the board to Claude Code, Codex, and any [MCP](https://modelcontextprotocol.io) client, so an agent can read, search, and update work — with the same workflow rules your team relies on.

**Optional factory control center.** Link a project to a [docket](https://github.com/yielab/docket) agent pod and Tack can dispatch work, mirror runs and approvals back onto the board, and show what each item cost in tokens. This is a client integration — Tack never runs agents itself, and the whole thing is off until you set `TACK_ORCH_ENABLE`. See [Factory control center](#factory-control-center-optional) below.

Built with Rust (Axum + sqlx) and SolidJS.

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

## Why Tack

Most self-hosted PM tools are built on heavy multi-service stacks. Tack is one Rust
binary and one SQLite file — and it's **AI-agent-ready** out of the box.

| | **Tack** | Plane | Vikunja | Huly |
| --- | --- | --- | --- | --- |
| Built in | Rust | Python/Django | Go | TypeScript |
| Deploy | **single binary** | Docker Compose (multi-service) | single binary or Docker | Docker Compose (multi-service) |
| Runtime deps | **none** (embedded SQLite) | Postgres + Redis | SQLite / Postgres / MySQL | MongoDB + others |
| Idle footprint | **~12 MiB RSS** | hundreds of MiB | tens of MiB | hundreds of MiB |
| License | **MIT** | AGPL-3.0 | AGPL-3.0 | EPL-2.0 |
| First-class CLI | **yes** | no | partial | no |
| MCP server (AI agents) | **yes** (`tack mcp`) | yes | no | no |
| Per-project vocabulary | **yes** | no | no | no |
| Agent-fleet control center (optional) | **yes** (via [docket](https://github.com/yielab/docket)) | no | no | no |

_Competitor details are best-effort from public docs/repos as of 2026-06; stacks
change. Footprints are order-of-magnitude. See [docs/BENCHMARKS.md](docs/BENCHMARKS.md)
for Tack's measured, reproducible numbers._

---

## Features

### Deployment

- **Single binary** — the web UI, REST API, and SQLite engine are all embedded in one ~10 MB statically-linked file
- **Zero dependencies** — no runtime, no database server, no container required; starts in milliseconds
- **Local data** — everything stored in `tack.db` + `storage/` next to the binary; back up by copying two files

### Views & Interface

| View | Description |
| --- | --- |
| **Board** | Kanban-style drag-and-drop with configurable columns and WIP limits |
| **List** | Sortable, hierarchy-aware list with inline editing and bulk operations |
| **Table** | Spreadsheet-style grid: inline-edit title/status/priority/assignee/due, sort, filter, and show/hide columns |
| **Calendar** | Items by due date — drag to reschedule |
| **Timeline** | Gantt-style view with dependency overlay — drag to reschedule |
| **Dashboard** | Throughput charts, status distribution, and sprint burndown |
| **Sprints** | Two-pane sprint planning (Backlog ↔ Sprint), capacity tracking |

- **Command palette** (Ctrl+K) and **global search** (Ctrl+/) available everywhere
- **Themes & palettes** — light/dark plus three accent palettes (Teal, Clay, Graphite), switched from the sidebar; WCAG AA contrast, verified in CI
- **Real-time updates** via WebSocket — open the same board in multiple tabs
- **Optimistic UI** — changes apply instantly, roll back on error
- Dark mode, skeleton loading screens, toast notifications

### Workflow & Project Types

- **10 project types** with pre-built workflows and vocabulary out of the box:

| Type | Default workflow | Vocabulary highlights |
| --- | --- | --- |
| `software` / `web` / `mobile` | Scrum | Epic, Feature, Task, Sprint |
| `construction` | Phase-based (Permit → Procurement → Build → Inspect → Handover) | Building, Work Order, Phase |
| `personal` | Simple (To Do → Doing → Done) | Goal, Action, Step |
| `homework` | Simple | Course, Assignment, Module, Week |
| `maintenance` | Kanban | System, Ticket, Job |
| `legal` | Phase-based (Intake → Discovery → Drafting → Review → Closed) | Matter, Case, Filing, Counsel |
| `research` | Kanban (Hypothesis → Design → Experiment → Analysis → Published) | Study, Experiment, Protocol, Researcher |
| `event` | Phase-based (Ideas → Booked → In Progress → Confirmed → Done) | Event, Track, Run Sheet |

- **Custom vocabulary** — rename any of 16 terms per project; the UI, CLI, and API all follow your terms
- **Custom workflows** — define any columns, WIP limits per column, and explicit transition rules
- **Dependency graph** — DAG with cycle detection; parent item auto-closes when all children reach done

### Data & Fields

- **Custom fields** — 9 types: Text, LongText, Number, Date, Boolean, Select, MultiSelect, URL, Email; with pattern / min / max validation
- **File attachments** — up to 50 MB per file, stored locally
- **Full-text search** — SQLite FTS5, scoped per project or global
- **Project templates** — built-in templates per type; save any project as a reusable template

### API & Integrations

- **89 REST operations across 60 paths, + WebSocket** — full CRUD for all entities, search, export, diagnostics, and (optional) agent-fleet orchestration; see the [OpenAPI spec](docs/openapi.json)
- **CLI client** — the same `tack` binary with `tack add`, `tack list`, `tack move`, `tack branch` (git branch from an item), and more; `--json` output and shell completions (bash/zsh/fish)
- **MCP server for AI agents** — `tack mcp` exposes the board to Claude Code, Codex, and any MCP client (list/search/read items, create/update/move, comment) over stdio; writes still pass through workflow validation. See [docs/MCP.md](docs/MCP.md)
- **Import** — GitHub Issues (public or private repos, label filter, PAT), Linear (GraphQL API, team/project filter), JSON/YAML round-trip, CSV
- **GitHub push sync** — set `TACK_GITHUB_TOKEN` and completing an imported item closes its GitHub issue (push-only v1). See [docs/GITHUB-SYNC.md](docs/GITHUB-SYNC.md)
- **Export** — JSON snapshot, plaintext **YAML** (git-diffable), and CSV per project
- **Backup / restore** — hot backup via `VACUUM INTO`; cloud backup to any S3-compatible bucket (Cloudflare R2, Backblaze B2, AWS S3); configurable from **Settings → Cloud Backup**
- **Webhooks** — outbound POST events on item changes and sprint transitions; HMAC-SHA256 payload signing
- **Agent-fleet control center (optional)** — link a project to a [docket](https://github.com/yielab/docket) pod and dispatch, mirror, and govern agent work from the board; off by default. See [Factory control center](#factory-control-center-optional)

---

## Quick Start

**One line (Linux / macOS):**

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack            # starts the server + web UI at http://localhost:3210
```

**With Cargo** (builds the single binary with the UI embedded):

```bash
cargo install --git https://github.com/yielab/tack tack-cli --features embed-spa
```

**Or download** the archive for your system from the [releases page](https://github.com/yielab/tack/releases):

```bash
# Linux / macOS
tar xzf tack-*.tar.gz && cd tack-*/
./tack
```

**Windows:** extract the zip and double-click `tack.exe`.

Open **`http://localhost:3210`** in your browser. Your data lives in `tack.db` and a `storage/` folder next to the binary — back those two up and you've backed up everything.

> **First-run note:** the binary is not code-signed yet. On macOS, right-click → **Open** the first time (or `xattr -d com.apple.quarantine tack`). On Windows, click **More info → Run anyway** if SmartScreen appears.

---

## Status & Limitations

Tack is in **beta**. Core features are complete; a few constraints to know upfront:

| Area | Current state |
| --- | --- |
| Authentication | One shared optional Bearer token — no per-user accounts. Suited for solo use or a small trusted group on the same network. |
| Multi-user | No per-user identities or permissions. All API clients share the same access level. |
| Multi-device sync | Single server, single database. Cross-device use is **snapshot replication** via S3-compatible cloud backup — one active writer, last-upload-wins, with generation-counter conflict detection and integrity-checked restores. Not live/concurrent multi-writer sync. |
| Mobile | Responsive web UI works on mobile browsers; no native app. |
| Binary signing | Not code-signed yet. See first-run note above. Roadmap item. |
| Offline | Browser UI requires the local server to be running. |
| Agent-fleet control (optional) | Requires a reachable [docket](https://github.com/yielab/docket) instance — Tack doesn't run agents itself, only dispatches to and mirrors state from one. No pause control from Tack in either direction: docket exposes no pause/resume over HTTP. Cost figures are **estimates** from a snapshotted price table, never observed spend. |

---

## Build from Source

**Prerequisites:** [Rust 1.85+](https://rustup.rs/) (the workspace uses the 2024 edition) · [Node.js 20+](https://nodejs.org/)

```bash
git clone https://github.com/yielab/tack.git
cd tack
make build   # builds the frontend then embeds it into the release binary
make run     # starts the pre-built binary at http://127.0.0.1:3210
```

For development with hot reload:

```bash
make dev     # starts API + Vite dev server; open http://localhost:5173
```

Other commands:

```bash
make test         # all Rust tests (581 tests) — frontend has 406 more (npm run test, in frontend/)
make e2e          # Playwright end-to-end tests (auto-starts servers)
make e2e-install  # one-time: download browser engines
make audit        # CVE scan (cargo audit + npm audit)
make lint         # clippy --workspace -- -D warnings
make fmt          # rustfmt --all
make help         # full command list
```

---

## Configuration

Configuration is loaded from `tack.toml` in the working directory, or from environment variables.

| Variable | Default | Description |
| --- | --- | --- |
| `TACK_HOST` | `127.0.0.1` | Bind address |
| `TACK_PORT` | `3210` | Port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite path |
| `TACK_LOG_LEVEL` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(none)_ | Bearer token — required on all `/api/*` requests when set |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook destination |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing key for webhook payloads |
| `TACK_BACKUP_BUCKET` | _(none)_ | S3 bucket name — required to enable cloud backup |
| `TACK_ORCH_ENABLE` | `false` | Enables the agent-fleet control center (reconciler + `/api/control-planes`, `/api/fleet`, dispatch, approvals, etc.). Unset ⇒ no reconciler task spawned, every orchestration route 404s |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Separate shared secret required to grant/deny a docket approval — deliberately distinct from `TACK_API_TOKEN` |

See the full variable reference in the [Configuration guide](docs/book/src/user-guide/configuration.md), or [Administration & Security](docs/book/src/user-guide/administration.md) for tokens, CORS, webhooks, and cloud backup. Every `TACK_ORCH_*` variable (including `TACK_ORCH_POLL_SECS` and `TACK_ORCH_EVENT_RETENTION_DAYS`) is documented there and in [Factory control center](#factory-control-center-optional) below.

---

## Factory control center (optional)

Tack can act as the control center for a whole factory of products, each with its own
board and its own governed agent pod running under [docket](https://github.com/yielab/docket),
an agent-fleet orchestrator. Tack is a **client** of docket — it never runs agents
itself; docket does that. Tack dispatches work to a pod and mirrors what comes back
(runs, approvals, traces, token counts) onto the board.

**Off by default.** None of this runs, and none of these routes exist, unless you set
`TACK_ORCH_ENABLE=true`. A plain `tack` install never makes an outbound call to
anything but its own SQLite file.

Once enabled and a control plane is registered:

- **Register & link** — add a docket control plane (base URL + Bearer token, token
  write-only over the API) and link a Tack project to one of its pods.
- **Fleet view** — one row per product: pod health, roster (roles + models), last
  activity, burn vs. budget. Health is a real state machine per control plane —
  `healthy` → `degraded` after 3 consecutive poll failures → `unreachable` after 10 —
  not a flag flipped on the first timeout.
- **Agent activity per item** — a timeline on the item detail view: runs, hops,
  approvals, traces, and token counts for that item's dispatches.
- **Dispatch** — send a single item or a whole sprint to the pod. Sprint dispatch
  walks the dependency graph and enqueues items in topological order, with a
  dry-run preview before anything actually runs.
- **Approvals inbox** — a fleet-wide list of pending approvals with grant/deny,
  gated behind a **separate** `TACK_ORCH_APPROVAL_TOKEN` — deliberately distinct from
  the ordinary `TACK_API_TOKEN`, because granting a docket approval is a higher-
  privilege action than editing a card.
- **Budget & policy panels** — burn vs. cap per pod, policy-hit and denial rates,
  approval/tool-call volume, mirrored from docket's own metrics.
- **Unit economics** — tokens in/out, **estimated** cost, agent lead time, and rework
  rate, per item and per product line. Token counts are the primary figure; Tack
  never sees a real bill — any dollar figure is derived from a snapshotted price
  table and is always labelled "estimated," never spend.
- **Provisioning** — create a Tack project and its docket pod together from a
  template, with rollback if either half fails partway through.

**Known limits, stated plainly:** docket exposes no pause/resume over HTTP in either
direction, so Tack has no pause control — resuming a budget-paused pod is still
`docket profile <id> --resume` on the docket side. Orchestration requires a reachable
docket instance; if it goes away, the fleet view degrades gracefully and the rest of
Tack keeps working. See [docs/book/src/user-guide/orchestration.md](docs/book/src/user-guide/orchestration.md)
and [docs/book/src/developer/orchestration.md](docs/book/src/developer/orchestration.md)
for the full reference.

---

## Architecture

```text
tack-core   Pure domain logic — models, workflow engine, vocabulary, dependency graph (no I/O)
    ↑
tack-db     SQLite persistence via sqlx — 31 migrations, FTS5, repository pattern
    ↑
tack-orch   Agent-fleet control-plane client — ControlPlane trait, docket adapter, reconciler
            (off by default; gated behind TACK_ORCH_ENABLE)
    ↑
tack-api    Axum HTTP server + WebSocket — 89 REST operations across 60 paths, config, webhooks
            (library crate)
    ↑
tack-cli    The `tack` binary — embeds tack-api to run the server; also the CLI client
```

The frontend is a SolidJS SPA embedded into the release binary via `--features embed-spa`. See the [Architecture overview](docs/book/src/developer/README.md) for details.

---

## Documentation

Full documentation is in [`docs/book/`](docs/book/). Build it locally with [mdBook](https://rust-lang.github.io/mdBook/):

```bash
cargo install mdbook
mdbook serve docs/book   # opens http://localhost:3000
```

| Guide | Description |
| --- | --- |
| [Quick Start](docs/book/src/user-guide/quick-start.md) | First-run walkthrough |
| [API Reference](docs/book/src/developer/api-reference.md) | Auth model + examples; the machine-generated [OpenAPI spec](docs/openapi.json) (served live at `/api/openapi.json`) is the source of truth |
| [CLI Reference](docs/book/src/user-guide/cli.md) | Every `tack` subcommand |
| [MCP Server](docs/MCP.md) | Wire Tack into Claude Code / AI agents via `tack mcp` |
| [Orchestration (user guide)](docs/book/src/user-guide/orchestration.md) | Link a project to docket, dispatch, approvals, budgets, unit economics |
| [Configuration](docs/book/src/user-guide/configuration.md) | Full variable reference and `tack.toml` |
| [Architecture](docs/book/src/developer/README.md) | Crate boundaries, design decisions |
| [Benchmarks](docs/BENCHMARKS.md) | Measured footprint and latency, with repro steps |
| [Testing](docs/TESTING.md) | Unit, integration, E2E, load, and security tests |
| [Roadmap](docs/book/src/roadmap.md) | Planned features and known gaps |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to report bugs, propose features, and submit pull requests.

Before pushing, activate the pre-push hook that runs `fmt` and `clippy`:

```bash
git config core.hooksPath .githooks
```

---

## License

MIT — see [LICENSE](LICENSE).
