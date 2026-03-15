# FlexPM - Flexible Project Management

A lightweight, versatile project management tool for solo developers and small teams.
Supports Scrum, Kanban, mixed workflows, and fully customizable terminology.

Handles everything from software sprints to construction builds to homework tracking.

## Quick Start

### Option A: Docker (recommended)

```bash
# Clone and start
git clone <repo-url> && cd flexpm
docker compose up -d

# Verify
curl http://localhost:3210/api/health
```

That's it. The database, migrations, and storage are handled automatically.
Data persists in a Docker volume (`flexpm-data`).

```bash
# View logs
docker compose logs -f

# Stop
docker compose down

# Stop and delete all data
docker compose down -v

# Rebuild after code changes
docker compose up -d --build

# Use the CLI inside the container
docker compose exec flexpm flexpm-cli --help
```

### Option B: Build from source

**Prerequisites:** [Rust 1.75+](https://rustup.rs/)

```bash
git clone <repo-url> && cd flexpm

# Build in release mode
cargo build --release

# Run the API server (starts on http://127.0.0.1:3210)
cargo run --bin flexpm-api

# Or run the CLI
cargo run --bin flexpm-cli -- --help
```

The server auto-creates a SQLite database (`flexpm.db`) and runs all migrations on first start.

### Verify It Works

```bash
curl http://localhost:3210/api/health
# {"status":"ok","service":"flexpm","version":"0.1.0"}
```

## Usage Guide

### Creating a Project

Every project has a **type** that determines its default vocabulary and workflow.
Available types: `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `custom`.

```bash
# Create a software project (Scrum workflow)
curl -X POST http://localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My App",
    "description": "Mobile app project",
    "project_type": "software"
  }'
```

**What happens automatically:**
- Vocabulary is set to agile terms (Epic, Task, Sprint, etc.)
- Workflow is set to Scrum (Backlog -> To Do -> In Progress -> In Review -> Done)
- WIP limits are applied (5 for In Progress, 3 for In Review)

```bash
# Create a construction project (Phase-based workflow)
curl -X POST http://localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Kitchen Renovation",
    "project_type": "construction"
  }'
```

This auto-configures:
- Vocabulary: Task -> "Work Order", Sprint -> "Phase", Epic -> "Building"
- Workflow: Permit -> Procurement -> Build -> Inspect -> Handover

### Adding Items (Tasks, Epics, etc.)

```bash
# Save the project ID from the create response
PROJECT_ID="<uuid-from-response>"

# Create an epic
curl -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
  -H "Content-Type: application/json" \
  -d '{
    "title": "User Authentication",
    "description": "Implement full auth flow",
    "item_type": "epic",
    "priority": "high",
    "tags": ["backend", "security"]
  }'

# Create a task under that epic
EPIC_ID="<uuid-from-response>"

curl -X POST http://localhost:3210/api/projects/$PROJECT_ID/items \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Design login page",
    "item_type": "task",
    "parent_id": "'$EPIC_ID'",
    "priority": "medium",
    "estimate": 5
  }'
```

### Moving Items Through the Workflow

```bash
ITEM_ID="<uuid>"

# Move from Backlog to In Progress
curl -X PATCH http://localhost:3210/api/items/$ITEM_ID \
  -H "Content-Type: application/json" \
  -d '{"status": "In Progress"}'

# Move to Done
curl -X PATCH http://localhost:3210/api/items/$ITEM_ID \
  -H "Content-Type: application/json" \
  -d '{"status": "Done"}'
```

The workflow engine validates transitions. For construction projects with strict transitions,
trying to skip from "Permit" to "Handover" returns a `400 Bad Request`.

### Sprints (Scrum Mode)

```bash
# Create a sprint
curl -X POST http://localhost:3210/api/projects/$PROJECT_ID/sprints \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Sprint 1",
    "goal": "Ship MVP login flow",
    "start_date": "2026-03-16T00:00:00Z",
    "end_date": "2026-03-30T00:00:00Z"
  }'

# Start the sprint
SPRINT_ID="<uuid>"
curl -X PATCH http://localhost:3210/api/sprints/$SPRINT_ID/status \
  -H "Content-Type: application/json" \
  -d '{"status": "active"}'

# Assign an item to the sprint
curl -X PATCH http://localhost:3210/api/items/$ITEM_ID \
  -H "Content-Type: application/json" \
  -d '{"sprint_id": "'$SPRINT_ID'"}'
```

### Roles & Specialties

Roles tag **what kind of work** an item needs, not who does it.
Perfect for solo devs wearing multiple hats, or small teams with specialties.

```bash
# Create roles for the project
curl -X POST http://localhost:3210/api/projects/$PROJECT_ID/roles \
  -H "Content-Type: application/json" \
  -d '{"name": "Frontend Dev", "color": "#3b82f6"}'

curl -X POST http://localhost:3210/api/projects/$PROJECT_ID/roles \
  -H "Content-Type: application/json" \
  -d '{"name": "Designer", "color": "#8b5cf6"}'

# Assign a role to an item
ROLE_ID="<uuid>"
curl -X PUT http://localhost:3210/api/items/$ITEM_ID/roles/$ROLE_ID
```

### Dependencies

```bash
# Item A blocks Item B
curl -X POST http://localhost:3210/api/items/$ITEM_A_ID/dependencies \
  -H "Content-Type: application/json" \
  -d '{
    "target_item_id": "'$ITEM_B_ID'",
    "dependency_type": "blocks"
  }'
```

The dependency engine validates the graph and rejects cycles.
Dependency types: `blocks`, `is_blocked_by`, `relates_to`, `duplicates`.

### Comments

```bash
curl -X POST http://localhost:3210/api/items/$ITEM_ID/comments \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Updated the design mockup, ready for review.",
    "author": "Alice"
  }'
```

### Searching

```bash
# Full-text search across item titles, descriptions, and tags
curl "http://localhost:3210/api/projects/$PROJECT_ID/search?q=login"
```

### Listing & Filtering

```bash
# All items in a project
curl "http://localhost:3210/api/projects/$PROJECT_ID/items"

# Filter by status
curl "http://localhost:3210/api/projects/$PROJECT_ID/items?status=In%20Progress"

# Filter by type and priority
curl "http://localhost:3210/api/projects/$PROJECT_ID/items?item_type=task&priority=high"

# Hierarchical tree view
curl "http://localhost:3210/api/projects/$PROJECT_ID/items/tree"

# Paginated results
curl "http://localhost:3210/api/projects/$PROJECT_ID/items?page=1&per_page=25"
```

### Customizing Vocabulary

Rename any term to fit your domain:

```bash
curl -X PATCH http://localhost:3210/api/projects/$PROJECT_ID \
  -H "Content-Type: application/json" \
  -d '{
    "vocabulary": {
      "task": "Work Item",
      "sprint": "Iteration",
      "epic": "Initiative",
      "backlog": "Idea Pool",
      "story_points": "Complexity"
    }
  }'
```

### Customizing Workflow

Add/remove/reorder status columns and set WIP limits:

```bash
curl -X PATCH http://localhost:3210/api/projects/$PROJECT_ID \
  -H "Content-Type: application/json" \
  -d '{
    "workflow": {
      "workflow_type": "custom",
      "statuses": [
        {"name": "Ideas", "category": "todo", "wip_limit": null, "order": 0},
        {"name": "Designing", "category": "in_progress", "wip_limit": 2, "order": 1},
        {"name": "Building", "category": "in_progress", "wip_limit": 3, "order": 2},
        {"name": "Testing", "category": "in_progress", "wip_limit": 2, "order": 3},
        {"name": "Shipped", "category": "done", "wip_limit": null, "order": 4}
      ],
      "transitions": null
    }
  }'
```

## Configuration

FlexPM can be configured via a `flexpm.toml` file in the working directory, or via environment variables.

```toml
# flexpm.toml (copy from config/flexpm.example.toml)
host = "127.0.0.1"
port = 3210
database_url = "sqlite:flexpm.db?mode=rwc"
log_level = "info"        # trace | debug | info | warn | error
log_json = false          # true for structured JSON logs
# log_file = "./logs/flexpm.log"
storage_dir = "./storage"
```

Environment variable equivalents (override the file):

| Variable | Default | Description |
|----------|---------|-------------|
| `FLEXPM_HOST` | `127.0.0.1` | Server bind address |
| `FLEXPM_PORT` | `3210` | Server port |
| `FLEXPM_DATABASE_URL` | `sqlite:flexpm.db?mode=rwc` | SQLite database path |
| `FLEXPM_LOG_LEVEL` | `info` | Log verbosity |
| `FLEXPM_LOG_JSON` | `false` | JSON structured logging |
| `FLEXPM_LOG_FILE` | _(none)_ | Optional log file path |
| `FLEXPM_STORAGE_DIR` | `./storage` | Attachment storage directory |

## Project Types & Defaults

| Type | Workflow | Example Vocabulary |
|------|----------|--------------------|
| `software` | Scrum (Backlog/To Do/In Progress/In Review/Done) | Epic, Feature, Task, Sprint |
| `web` | Scrum | Same as software |
| `mobile` | Scrum | Same as software |
| `construction` | Phase-based (Permit/Procurement/Build/Inspect/Handover) | Building, Work Order, Phase, Inspection |
| `personal` | Simple (To Do/Doing/Done) | Goal, Action, Step |
| `homework` | Simple (To Do/Doing/Done) | Course, Assignment, Module, Week |
| `maintenance` | Kanban (Queue/In Progress/Review/Done) | System, Ticket, Job, Schedule |
| `custom` | Simple (To Do/Doing/Done) | Default agile terms |

## Running Tests

```bash
# Run all tests (unit + integration)
cargo test

# Run with output visible
cargo test -- --nocapture

# Run a specific test
cargo test test_workflow_transition_validation

# Run only core unit tests
cargo test -p flexpm-core

# Run only database integration tests
cargo test -p flexpm-db
```

## Debug Endpoints

When the server is running:

```bash
# Health check
curl http://localhost:3210/api/health

# System info (version, build mode, database info)
curl http://localhost:3210/api/debug/info

# Database statistics (row counts per table)
curl http://localhost:3210/api/debug/db-stats
```

## Logging & Debugging

```bash
# Run with debug logging
FLEXPM_LOG_LEVEL=debug cargo run --bin flexpm-api

# Run with trace logging (very verbose, shows SQL)
RUST_LOG=flexpm_db=trace,flexpm_api=debug cargo run --bin flexpm-api

# Run with JSON structured logging
FLEXPM_LOG_JSON=true cargo run --bin flexpm-api
```

Every API request is traced with method, URI, and timing. Every database
operation logs at `debug` level with parameters and results.

## Docker

### Production Deployment

```bash
# Build and run
docker compose up -d

# Custom port
FLEXPM_PORT=8080 docker compose up -d

# With JSON logs (for log aggregators)
# Edit docker-compose.yml: set FLEXPM_LOG_JSON=true
```

### Docker Image Only (no compose)

```bash
# Build the image
docker build -t flexpm .

# Run with a named volume for persistent data
docker run -d \
  --name flexpm \
  -p 3210:3210 \
  -v flexpm-data:/data \
  flexpm

# Run with a host directory for data
docker run -d \
  --name flexpm \
  -p 3210:3210 \
  -v $(pwd)/data:/data \
  flexpm
```

### Container Details

| Property | Value |
|----------|-------|
| Base image | `debian:bookworm-slim` (~80MB) |
| Runs as | Non-root user `flexpm` |
| Data directory | `/data` (SQLite DB + attachments) |
| Default port | `3210` |
| Health check | `GET /api/health` every 30s |
| Includes | `sqlite3` CLI for DB inspection |

```bash
# Inspect the database inside the container
docker compose exec flexpm sqlite3 /data/flexpm.db ".tables"

# Backup the database
docker compose exec flexpm sqlite3 /data/flexpm.db ".backup /data/backup.db"
docker cp flexpm:/data/backup.db ./flexpm-backup.db
```

## Architecture

```
crates/
├── flexpm-core/     Core domain models & business logic (no I/O)
│   ├── models.rs    Entities: Project, Item, Sprint, Role, Comment, etc.
│   ├── workflow.rs  Workflow engine: transitions, WIP limits, presets
│   ├── vocabulary.rs Customizable term mapping per project
│   ├── dependency.rs DAG graph with cycle detection
│   └── error.rs     Typed domain error hierarchy
├── flexpm-db/       Database layer (SQLite via sqlx)
│   ├── migrations.rs 10 migrations with FTS5 full-text search
│   └── repo/        Repository pattern: CRUD for all entities
├── flexpm-api/      HTTP server (Axum)
│   ├── router.rs    30+ REST endpoints
│   ├── handlers/    Request handlers per entity
│   ├── error.rs     API error -> HTTP status mapping
│   ├── debug.rs     Health & diagnostics endpoints
│   └── config.rs    TOML/env config loading
└── flexpm-cli/      CLI tool (clap)
    └── main.rs      Terminal commands (init, add, list, move, etc.)
```

## License

MIT
