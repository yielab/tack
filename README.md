# FlexPM

[![CI](https://github.com/santiagoyie/FlexPM/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/santiagoyie/FlexPM/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

> Local-first, single-binary project management for solo developers and small teams.
> Built with Rust (backend) + SolidJS (frontend).

Supports any workflow — Scrum, Kanban, phase-based construction, personal tasks — through fully configurable vocabulary and status columns. The same binary handles software sprints, construction phases, and personal task lists with equal depth. No accounts, no cloud, no subscriptions.

---

## Why

Existing PM tools either lock you into one workflow (Jira), require accounts and cloud infrastructure (Linear, Asana, ClickUp), or hit a customization ceiling that does not extend to non-software use cases (Trello). I wanted something I could run locally as a single binary, fully customizable per project — the same tool for software sprints, a kitchen renovation, or thesis chapters.

FlexPM is also a deliberate exploration of stacks outside my paid client work, which has mostly been PHP/Symfony/Drupal for US agencies. Rust (Axum, sqlx), modular crate architecture with strict layering (no I/O in the domain crate), and SolidJS are all areas I wanted to work with in a project of meaningful scope.

---

## Quick Start (development)

**Prerequisites:** [Rust toolchain](https://rustup.rs/) · [Node.js 20+](https://nodejs.org/)

Two processes run side by side: the Rust API and the Vite frontend dev server. Start the API first, then the frontend.

```bash
git clone https://github.com/santiagoyie/flexpm.git
cd flexpm

# Terminal 1 — API server (http://127.0.0.1:3210)
cargo run --bin flexpm-api

# Terminal 2 — Frontend dev server (proxies /api → 127.0.0.1:3210)
cd frontend && npm install && npm run dev
```

**To check the UI, open the frontend dev server URL** — Vite prints it on startup:

```text
➜  Local:   http://localhost:5173/
```

> If port 5173 is taken, Vite automatically uses the next free port (5174, …) and prints
> that URL instead. Always open the URL Vite actually prints. The dev server gives you
> hot-reload and proxies all `/api` calls to the API server, so you must keep Terminal 1 running.

The API server auto-creates `flexpm.db` and runs all 16 migrations on first start.

```bash
# Verify the API is up
curl http://localhost:3210/api/health
# {"status":"ok","version":"0.1.0","migrations_applied":16}
```

The fresh database is empty — open the UI and create a project (or press **Ctrl+K** for the
command palette) to populate the Board, List, and Dashboard views.

### Viewing the UI on `http://127.0.0.1:3210` (or a reverse-proxy domain)

`cargo run --bin flexpm-api` serves the **API only** — opening `http://127.0.0.1:3210` in a
browser will not show the UI. To serve the SPA from the API binary itself (e.g. behind a
reverse proxy such as Caddy), build with the `embed-spa` feature — see
[Single-binary distribution](#single-binary-distribution) below. For day-to-day development,
use the Vite dev server URL above.

---

## Single-binary distribution

Build one binary that serves both the API and the embedded SPA:

```bash
# 1. Build the frontend
cd frontend && npm ci && npm run build && cd ..

# 2. Build the API with the embedded SPA feature
cargo build --release --features embed-spa -p flexpm-api

# Resulting binary: target/release/flexpm-api (~5 MB)
# Serves API at /api/* and the SPA at all other paths.
./target/release/flexpm-api
# open http://127.0.0.1:3210
```

Without `--features embed-spa` the binary is API-only; use the dev server or any static file host for the frontend.

---

## CLI

The `flexpm` CLI talks to a running API server.

```bash
# Point at the server (default: http://127.0.0.1:3210)
export FLEXPM_API_URL=http://127.0.0.1:3210

# Projects
flexpm init "Kitchen Reno" --type construction
flexpm list --project <id>

# Items
flexpm add "Design login page" --project <id> --type task --priority high
flexpm move <item-id> "In Progress"

# Sprints
flexpm sprint create --project <id> --name "Sprint 1"
flexpm sprint start <sprint-id>
flexpm sprint close <sprint-id>

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
| `GET /api/backup` | Download a clean SQLite backup |
| `POST /api/restore` | Stage a backup file for next-startup restore |
| `GET /projects/{id}/search?q=term` | FTS5 full-text search |
| `GET /search?q=term` | Global search across all projects |

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
| `FLEXPM_ALLOWED_ORIGINS` | `localhost:8080,127.0.0.1:8080,...` | CORS allow-list |
| `FLEXPM_MAX_BODY_SIZE` | `2097152` | Global body limit (bytes); uploads always 50 MB |

Example `flexpm.toml`:

```toml
host = "127.0.0.1"
port = 3210
database_url = "sqlite:flexpm.db?mode=rwc"
log_level = "info"
storage_dir = "./storage"
# api_token = "change-me"
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

All vocabulary and workflow columns are editable after creation — via the **Settings panel** in the UI (`/projects/:id/settings`) or via `PATCH /api/projects/{id}`.

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

## Debugging

```bash
# Health + version + migration count
curl http://localhost:3210/api/health

# System info, DB size, config
curl http://localhost:3210/api/debug/info

# Row counts per table
curl http://localhost:3210/api/debug/db-stats

# Verbose logging
FLEXPM_LOG_LEVEL=debug cargo run --bin flexpm-api

# Trace SQL queries
RUST_LOG=flexpm_db=trace,flexpm_api=debug cargo run --bin flexpm-api
```

---

## Running tests

```bash
# All tests (92 tests, 1 ignored perf test)
cargo test --workspace

# With embed-spa feature (95 tests, requires frontend/dist/ to exist)
cargo test -p flexpm-api --features embed-spa

# Single crate
cargo test -p flexpm-core   # 39 unit tests
cargo test -p flexpm-db     # 22 integration tests
cargo test -p flexpm-api    # 20 tests (4 unit + 16 handler)
cargo test -p flexpm-cli    # 11 CLI tests

# Run the ignored 50k-item performance test
cargo test -p flexpm-db list_items_p95 -- --ignored
```

Integration tests use in-memory SQLite — no external services needed.

---

## Architecture

```text
crates/
├── flexpm-core/     Pure domain logic — no I/O
│   ├── models.rs    All entities and DTOs
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
│   ├── config.rs    TOML + env config
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

## Frontend features

- **7 view modes:** Board (Kanban drag-and-drop), List, Tree (hierarchy), Calendar, Timeline, Dashboard, Sprints
- **Settings panel** — live vocabulary and workflow editor per project
- **Real-time updates** via WebSocket
- **Optimistic UI** — instant feedback, rollback on error
- **Global search** (Ctrl+/) powered by FTS5
- **Command palette** (Ctrl+K)
- **Project templates** — create and reuse project blueprints
- **Custom fields** — 9 types: Text, LongText, Number, Date, Boolean, Select, MultiSelect, URL, Email
- **Multiple boards per project** — 6 grouping modes: Status, Priority, Item Type, Sprint, Assignee, Custom Field
- Dark mode, skeleton loading, toast notifications
- Entry bundle: 22 KB gzipped (lazy-loaded routes)

---

## Documentation

- [API Reference](docs/API-REFERENCE.md) — complete endpoint reference
- [Testing Guide](docs/TESTING.md) — test pyramid, commands, coverage
- [Deployment Guide](docs/DEPLOYMENT-GUIDE.md) — bare binary and reverse proxy setup
- [Project Status](docs/PROJECT-STATUS.md) — current state and known gaps
- [CONTRIBUTING.md](CONTRIBUTING.md) — code style, PR process, how to add features
- [CHANGELOG.md](CHANGELOG.md) — version history

---

## License

MIT

---
_Personal R&D project — actively developed. Built as a learning exercise in stacks outside my day-to-day paid work._
