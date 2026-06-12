# FlexPM

[![CI](https://github.com/santiagoyie/FlexPM/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/santiagoyie/FlexPM/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

> Local-first, single-binary project management for solo developers and small teams.
> Built with Rust (backend) + SolidJS (frontend).

Supports any workflow — Scrum, Kanban, phase-based construction, personal tasks — through fully configurable vocabulary and status columns. No accounts, no cloud, no subscriptions. One binary, one SQLite file.

---

## Features

### Views

| View | Description |
| --- | --- |
| **Board** | Kanban-style drag-and-drop with WIP limits per column |
| **List** | Sortable table with inline editing and bulk operations |
| **Calendar** | Items by due date — drag to reschedule |
| **Timeline** | Gantt-style dependency overlay — drag to reschedule |
| **Dashboard** | Throughput charts and sprint progress |
| **Sprints** | Two-pane sprint planning (Backlog ↔ Sprint), capacity and burndown |

### Workflow engine

- **7 project types** with pre-built workflows: `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`
- **Custom workflows** — define any columns, categories (todo / in-progress / done), WIP limits, and explicit transition rules
- **Per-project vocabulary** — rename 16 terms to match your domain: Task → Work Order, Sprint → Phase, Epic → Building
- **Dependency graph** — DAG with cycle detection; blocks / relates-to / depends-on
- **Auto-complete** — parent item closes automatically when all children reach done

### Data

- **Custom fields** — 9 types: Text, LongText, Number, Date, Boolean, Select, MultiSelect, URL, Email; with pattern/min/max validation rules
- **File attachments** — up to 50 MB per file, stored locally
- **Full-text search** — SQLite FTS5, per-project and global (Ctrl+/)
- **Export** — JSON (full snapshot) and CSV per project
- **Import** — JSON round-trip with ID remapping; CSV into existing project
- **GitHub Issues import** — fetch issues from any public or private repo; supports label filter, PAT, and closed-issue toggle
- **Backup / restore** — hot backup via `VACUUM INTO`; staged restore on next startup
- **Project templates** — built-in templates per project type; save any project as a template

### Interface

- **Command palette** (Ctrl+K) available on every page
- **Real-time updates** via WebSocket — open the same board in two tabs
- **Optimistic UI** — changes apply instantly, roll back on error
- **Dark mode**, skeleton loading screens, toast notifications
- **22 KB entry bundle** (lazy-loaded routes)

### API & CLI

- **63 REST endpoints** — full CRUD for all entities plus debug and diagnostic routes
- **CLI** (`flexpm`) with `--json` output and shell completions (bash/zsh/fish)
- **Optional Bearer token** auth (`FLEXPM_API_TOKEN`)
- **Outbound webhooks** — POST events to any URL on item changes, sprint starts, and due-soon alerts; HMAC-SHA256 signing available
- Single binary with embedded SPA (`--features embed-spa`, ~5 MB)

### Voice (Amazon Alexa)

- **Alexa skill webhook** (`POST /api/alexa`) — add tasks, list open work, and complete items by voice
- **Bilingual** — understands and answers in English and Spanish, following the request locale
- **Same rules as the UI** — workflow transitions, WIP limits, auto-parent-completion, and per-project vocabulary all apply; boards update live via WebSocket
- **Off by default** — enable by setting `FLEXPM_ALEXA_SKILL_ID`; see [docs/ALEXA.md](docs/ALEXA.md) for skill setup

---

## Why

Existing PM tools either lock you into one workflow (Jira), require accounts and cloud infrastructure (Linear, Asana, ClickUp), or hit a customization ceiling that does not extend to non-software domains (Trello). I wanted something I could run locally as a single binary, fully customizable per project — the same tool for software sprints, a kitchen renovation, or thesis chapters.

FlexPM is also a deliberate exploration of stacks. Rust (Axum, sqlx), modular crate architecture with strict layering (no I/O in the domain crate), and SolidJS are all areas I wanted to work with in a project of meaningful scope.

### Why Rust?

The honest answer is split between technical fit and deliberate learning.

**Technical fit:** A local-first tool that ships as a single binary with no runtime dependencies and a SQLite file as its only data store is a genuinely good fit for Rust. The binary is ~5 MB statically linked, starts in milliseconds, and uses negligible memory at rest.

**Learning purpose:** My day-to-day work runs on more standard stacks. I picked Rust specifically because it forces explicit thinking about things those languages abstract away — memory layout, async runtimes, error propagation without exceptions. FlexPM was scoped large enough to encounter those problems in real form: async handlers, a multi-crate workspace with strict layering, compile-time SQL, a broadcast channel for WebSockets. A todo-list tutorial would not have surfaced any of that.

---

## Getting Started

### Run it (single binary)

**Prerequisites:** [Rust toolchain](https://rustup.rs/) · [Node.js 20+](https://nodejs.org/)

```bash
git clone https://github.com/santiagoyie/FlexPM.git
cd FlexPM
make build   # compile: builds frontend then embeds it into the release binary (~30s)
make run     # start: launches the pre-built binary, nothing is recompiled
```

Open **`http://127.0.0.1:3210`** — one process serves everything. The database and storage directory are created automatically on first start. Re-run `make build` only when code changes; `make run` just starts what was already compiled.

### Develop it

```bash
make dev
```

Starts the API and the Vite dev server together. Ctrl-C stops both. Open **`http://localhost:5173`** — Vite proxies all `/api` requests to the API and hot-reloads the frontend on every save. Frontend dependencies are installed automatically on first run.

### Other useful commands

```bash
make test       # run all 164 Rust tests
make lint       # clippy --workspace -- -D warnings
make fmt        # rustfmt --all
make debug      # API only with verbose logging
make reset-db   # wipe the database and start fresh
make help       # full command list
```

---

## CLI

The `flexpm` CLI talks to a running API server.

```bash
# Point at the server (default: http://127.0.0.1:3210)
export FLEXPM_API_URL=http://127.0.0.1:3210

# Projects
flexpm init "Kitchen Reno" --type construction
flexpm projects

# Items
flexpm add "Design login page" --project <id> --type task --priority high
flexpm list --project <id>
flexpm move <item-id> "In Progress"

# Sprints
flexpm sprint create --project <id> --name "Sprint 1"
flexpm sprint start <sprint-id>
flexpm sprint close <sprint-id>

# Templates
flexpm template list
flexpm template show <template-id>
flexpm template create-from <template-id> "My New Project"

# Roles, comments, custom fields
flexpm role create --project <id> --name "Designer"
flexpm comment create <item-id> --content "Looks good"
flexpm field create --project <id> --name "Story Points" --type number
flexpm field set <item-id> <field-id> 5

# Backup and restore
flexpm backup                        # saves flexpm-backup.db in the current directory
flexpm backup --path /safe/place.db  # custom path
flexpm restore /safe/place.db        # stages the restore; restart the server to apply

# Config
flexpm config --url http://myserver:3210 --token mytoken
flexpm config --show

# Shell completions
flexpm completions bash >> ~/.bashrc
flexpm completions zsh  >> ~/.zshrc
```

All commands support `--json` for machine-readable output.

---

## API Overview

Base URL: `http://127.0.0.1:3210/api`

### Core resources

| Endpoint | Description |
| --- | --- |
| `POST /projects` | Create project |
| `GET /projects` | List all projects |
| `PATCH /projects/{id}` | Update project (including vocabulary + workflow) |
| `POST /projects/{id}/items` | Create item |
| `GET /projects/{id}/items` | List items (with filters, pagination) |
| `PATCH /items/{id}` | Update item (status change validated by workflow) |
| `GET /projects/{id}/boards` | List boards |
| `GET /boards/{id}/view` | Board view with items grouped by column |
| `GET /projects/{id}/boards/live` | WebSocket — real-time board updates |

### Operations

| Endpoint | Description |
| --- | --- |
| `GET /projects/{id}/export?format=json\|csv` | Export project |
| `POST /projects/import` | Import project (with ID remapping) |
| `POST /projects/{id}/import-csv` | Import items from CSV into existing project |
| `POST /projects/{id}/import-github` | Import issues from a GitHub repository |
| `GET /backup` | Download a clean SQLite backup |
| `POST /restore` | Stage a backup file for next-startup restore |
| `GET /projects/{id}/search?q=term` | FTS5 full-text search |
| `GET /search?q=term` | Global search across all projects |
| `POST /alexa` | Alexa skill webhook (requires `FLEXPM_ALEXA_SKILL_ID`) |

See [API Reference](docs/API-REFERENCE.md) for the full endpoint list.

### Full curl workflow

```bash
BASE=http://localhost:3210/api

# Create a project
PROJECT=$(curl -s -X POST $BASE/projects \
  -H "Content-Type: application/json" \
  -d '{"name":"My App","project_type":"software"}')
PID=$(echo $PROJECT | jq -r '.id')

# Add an item
ITEM=$(curl -s -X POST $BASE/projects/$PID/items \
  -H "Content-Type: application/json" \
  -d '{"title":"Login page","item_type":"task","priority":"high"}')
IID=$(echo $ITEM | jq -r '.id')

# Move it forward
curl -s -X PATCH $BASE/items/$IID \
  -H "Content-Type: application/json" \
  -d '{"status":"In Progress"}'

# Export
curl -s "$BASE/projects/$PID/export?format=json" -o backup.json

# Import GitHub issues
curl -s -X POST $BASE/projects/$PID/import-github \
  -H "Content-Type: application/json" \
  -d '{"repo":"owner/my-repo","token":"ghp_...","label_filter":["bug"]}'
```

---

## Configuration

`flexpm.toml` in the working directory, or environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `FLEXPM_HOST` | `127.0.0.1` | Bind address |
| `FLEXPM_PORT` | `3210` | Port |
| `FLEXPM_DATABASE_URL` | `sqlite:flexpm.db?mode=rwc` | SQLite path |
| `FLEXPM_LOG_LEVEL` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `FLEXPM_LOG_JSON` | `false` | Structured JSON logs |
| `FLEXPM_LOG_FILE` | _(none)_ | Optional log file |
| `FLEXPM_STORAGE_DIR` | `./storage` | Attachment storage |
| `FLEXPM_API_TOKEN` | _(none)_ | Bearer token — required on all `/api/*` requests when set |
| `FLEXPM_ALLOWED_ORIGINS` | `localhost:5173,127.0.0.1:5173,...` | CORS allow-list |
| `FLEXPM_MAX_BODY_SIZE` | `2097152` | Global body limit (bytes); upload routes always 50 MB |
| `FLEXPM_ALEXA_SKILL_ID` | _(none)_ | Enables `POST /api/alexa`; endpoint returns 404 when unset |
| `FLEXPM_WEBHOOK_URL` | _(none)_ | Outbound webhook destination; off when unset |
| `FLEXPM_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing key for webhook payloads |

Example `flexpm.toml`:

```toml
host = "127.0.0.1"
port = 3210
database_url = "sqlite:flexpm.db?mode=rwc"
log_level = "info"
storage_dir = "./storage"
# api_token = "change-me"
# webhook_url = "https://hooks.example.com/flexpm"
# webhook_secret = "change-me"
# alexa_skill_id = "amzn1.ask.skill.xxxxxxxx"
```

---

## Project types & default workflows

| Type | Workflow | Vocabulary highlights |
| --- | --- | --- |
| `software` | Scrum (Backlog → In Progress → Done) | Epic, Feature, Task, Sprint |
| `web` / `mobile` | Scrum | Same as software |
| `construction` | Phase-based (Permit → Procurement → Build → Inspect → Handover) | Building, Work Order, Phase |
| `personal` | Simple (To Do → Doing → Done) | Goal, Action, Step |
| `homework` | Simple | Course, Assignment, Module, Week |
| `maintenance` | Kanban | System, Ticket, Job |
| `custom` | Simple | Default agile terms |

All vocabulary and workflow columns are editable after creation — via the **Settings panel** in the UI or via `PATCH /api/projects/{id}`.

---

## Customizing vocabulary

```bash
curl -X PATCH http://localhost:3210/api/projects/$PID \
  -H "Content-Type: application/json" \
  -d '{"vocabulary":{"task":"Work Order","sprint":"Phase","epic":"Building"}}'
```

The UI Settings panel provides a live editor for all 16 vocabulary keys.

---

## Customizing workflow

```bash
curl -X PATCH http://localhost:3210/api/projects/$PID \
  -H "Content-Type: application/json" \
  -d '{
    "workflow": {
      "workflow_type": "custom",
      "statuses": [
        {"name":"Ideas",    "category":"todo",        "wip_limit":null, "order":0},
        {"name":"Building", "category":"in_progress", "wip_limit":3,    "order":1},
        {"name":"Shipped",  "category":"done",        "wip_limit":null, "order":2}
      ],
      "transitions": null
    }
  }'
```

---

## Webhooks

Set `FLEXPM_WEBHOOK_URL` to receive POST events on every significant state change:

| Event | Trigger |
| --- | --- |
| `item.created` | New item added |
| `item.updated` | Item status, title, priority, or assignee changed |
| `item.deleted` | Item removed |
| `sprint.started` | Sprint moved to Active |
| `sprint.completed` | Sprint moved to Closed |
| `item.due_soon` | Item due within the next hour (checked hourly) |

Payloads are signed when `FLEXPM_WEBHOOK_SECRET` is set — verify with the `X-FlexPM-Signature: sha256=<hex>` header. Delivery is fire-and-forget; errors are logged and never surfaced to API callers.

---

## Backup and restore

```bash
# Download a clean offline copy of the live database
curl http://localhost:3210/api/backup -o backup.db

# Stage a restore (the running server is not interrupted)
curl -X POST http://localhost:3210/api/restore \
  -H "Content-Type: application/octet-stream" \
  --data-binary @backup.db
# → {"status":"staged","message":"Restore staged. Stop the server and restart to apply."}

# On next server start, the staged file is applied automatically.
# The previous database is moved to flexpm.db.bak.
```

---

## Running tests

```bash
# All Rust tests (164 tests, 1 ignored perf test)
cargo test --workspace

# With embed-spa feature (requires frontend/dist/ to exist)
cargo test -p flexpm-api --features embed-spa

# Single crate
cargo test -p flexpm-core   # 67 unit tests
cargo test -p flexpm-db     # 22 integration tests
cargo test -p flexpm-api    # 64 tests (11 unit + 17 Alexa + 36 handler)
cargo test -p flexpm-cli    # 11 CLI tests

# Run the ignored 50k-item performance test
cargo test -p flexpm-db list_items_p95 -- --ignored

# Frontend tests (144 Vitest unit tests)
cd frontend && npm test
```

Integration tests use in-memory SQLite — no external services needed.

---

## Architecture

```text
crates/
├── flexpm-core/     Pure domain logic — no I/O
│   ├── models.rs    All entities, DTOs, and custom field validation
│   ├── workflow.rs  Transition validation, WIP limits, presets
│   ├── vocabulary.rs Customizable term mapping
│   ├── dependency.rs DAG with cycle detection
│   └── error.rs     Typed domain errors
├── flexpm-db/       SQLite persistence (sqlx, async)
│   ├── migrations.rs 16 migrations, FTS5, WAL mode
│   └── repo/        Repository pattern per entity
├── flexpm-api/      Axum HTTP server + WebSocket
│   ├── router.rs    All routes, middleware, AppState
│   ├── handlers/    One module per entity
│   │   ├── alexa.rs         Voice skill endpoint
│   │   ├── attachments.rs
│   │   ├── backup.rs        Backup / restore
│   │   ├── boards_multi.rs  Multiple boards per project
│   │   ├── comments.rs
│   │   ├── custom_fields.rs
│   │   ├── dependencies.rs
│   │   ├── export.rs        JSON/CSV export + import
│   │   ├── import_github.rs GitHub Issues import
│   │   ├── items.rs
│   │   ├── projects.rs
│   │   ├── roles.rs
│   │   ├── spa.rs           SPA fallback (--features embed-spa)
│   │   ├── sprints.rs
│   │   ├── templates.rs
│   │   └── websocket.rs
│   ├── config.rs    TOML + env config
│   ├── webhook.rs   Outbound webhook delivery
│   └── debug.rs     Health and diagnostics
└── flexpm-cli/      clap CLI — talks to the API over HTTP

frontend/
├── src/
│   ├── components/  UI components (modals, board, list, etc.)
│   ├── pages/       Board, List, Dashboard, Sprints, Calendar,
│   │                Timeline, Settings, Templates views
│   ├── lib/         API client, WebSocket, optimistic UI, vocab resolver
│   └── types/       TypeScript types
└── dist/            Built SPA (embedded when using --features embed-spa)
```

### Crate dependency rules

```text
flexpm-core  ← no deps on other crates (pure logic)
     ↑
flexpm-db    ← adds SQLite I/O
     ↑
flexpm-api   ← adds HTTP transport
flexpm-cli   ← talks to flexpm-api over HTTP (no DB access)
```

---

## Developer Setup

### Requirements

| Tool | Version | Install |
| --- | --- | --- |
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | [nodejs.org](https://nodejs.org/) |
| Git | 2.x | system package manager |

### First clone

```bash
git clone https://github.com/santiagoyie/FlexPM.git
cd FlexPM

# Activate the pre-push hook (runs fmt + clippy before every push)
git config core.hooksPath .githooks

# Verify everything builds and tests pass
cargo build
cargo test --workspace
```

The `.githooks/pre-push` script runs `cargo fmt --all --check` and `cargo clippy --workspace -- -D warnings` before each push. This is what the CI checks — running it locally means CI never sees a formatting or lint failure.

### Code quality commands

```bash
cargo fmt --all                    # auto-format
cargo fmt --all --check            # check only (same as CI)
cargo clippy --workspace -- -D warnings  # lint (same as CI)
cargo check --workspace            # fast type-check
```

---

## Documentation

The full documentation is in [`docs/book/`](docs/book/) — build it with
[mdBook](https://rust-lang.github.io/mdBook/):

```sh
cargo install mdbook
mdbook serve docs/book   # opens http://localhost:3000
```

Sections: **User Guide** · **Developer Guide** (with Learning Path for Rust/SolidJS newcomers) · **API Reference** · **Roadmap**.

Quick links to the source:

- [User Guide](docs/book/src/user-guide/quick-start.md)
- [Architecture Overview](docs/book/src/developer/README.md)
- [Learning Path](docs/book/src/developer/learning/README.md)
- [API Reference](docs/book/src/developer/api-reference.md)
- [Roadmap](docs/book/src/roadmap.md)
- [Deployment](docs/book/src/developer/deployment.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

---

## License

MIT

---

_Personal R&D project — actively developed. Built as a learning exercise in stacks outside my day-to-day paid work._
