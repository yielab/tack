# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Local Domain (Caddy Reverse Proxy)

This project is served at **https://tack.test** via a centralized Caddy + dnsmasq setup.

- The `Caddyfile.local` in this directory is auto-imported by the global `/home/ox/Sites/Caddyfile`
- **Do NOT add a global `{}` block** to `Caddyfile.local` — it conflicts with the main config
- **Do NOT run a separate Caddy instance** — use `sudo systemctl reload caddy` after changes
- **Do NOT use custom ports** — Caddy runs on standard 80/443 via systemd
- See `/home/ox/Sites/LOCAL-DOMAINS.md` for full documentation

## Project Overview

Tack is a lightweight, versatile project management tool built in Rust (backend) and SolidJS (frontend). It supports multiple workflows (Scrum, Kanban, phase-based) with fully customizable terminology and statuses. Designed for solo developers and small teams across diverse domains: software development, construction, personal tasks, etc.

**Core Philosophy:** Universal work tracking with domain-specific vocabulary. The same underlying system adapts to different project types through configurable workflows and terminology.

**Current Status:** Phase 8 complete

- Backend: complete (REST endpoints + WebSocket, 17 migrations, custom field value validation, Alexa voice integration)
- Frontend: complete (Board, List, Timeline, Sprints, Settings views; 144 Vitest unit tests + Playwright E2E)
- CLI: complete (init, add, list, move, board, search, sprint, template, role, comment, field, backup, restore)

## Development Commands

### Building and Running

```bash
# Check the whole workspace compiles
cargo build

# Run the server + web UI (http://127.0.0.1:3210) — bare `tack` defaults to serve
cargo run -p tack-cli -- serve

# Run the CLI tool (same binary)
cargo run --bin tack -- --help

# Release binary (slow compile — use sparingly)
cargo build --release
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Show test output
cargo test --workspace -- --nocapture

# Single test by name
cargo test test_workflow_transition_validation

# One crate only
cargo test -p tack-core
cargo test -p tack-db
cargo test -p tack-api

# End-to-end (Playwright: cross-browser smoke + journeys + a11y + API contract)
make e2e-install   # one-time: download browser engines
make e2e

# Dependency CVE scan (cargo audit + npm audit) and k6 load baseline
make audit
make load
```

Integration tests use in-memory SQLite — no external services needed. The E2E
suite (`frontend/e2e/`, `frontend/playwright.config.ts`) starts an isolated API
(dedicated port + throwaway `e2e.db`) and the SPA itself — see `docs/TESTING.md`.

### Frontend Development

```bash
cd frontend
npm install          # once
npm run dev          # http://localhost:5173 — proxies /api to 127.0.0.1:3210
npm run type-check
npm run build
```

Start the API server before the frontend dev server.

### Configuration

The API server loads configuration from `tack.toml` (if present) or environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_HOST` | `127.0.0.1` | Server bind address |
| `TACK_PORT` | `3210` | Server port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite database path |
| `TACK_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `TACK_LOG_JSON` | `false` | Structured JSON logging |
| `TACK_LOG_FILE` | _(none)_ | Optional log file path |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(none)_ | Optional Bearer token — requires `Authorization: Bearer <token>` on all API requests |
| `TACK_ALLOWED_ORIGINS` | `localhost:8080,127.0.0.1:8080` | Comma-separated CORS allow-list |
| `TACK_MAX_BODY_SIZE` | `2097152` | Global request body limit in bytes (default 2 MB; upload endpoint is always 50 MB) |
| `TACK_ALEXA_SKILL_ID` | _(none)_ | Amazon Alexa skill ID — enables `POST /api/alexa` (see `docs/ALEXA.md`); endpoint returns 404 when unset |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL — when set, POSTs JSON events on item create/update/delete, sprint status changes, and due-soon alerts |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>` to each delivery |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |

The `TACK_BACKUP_*` values are **defaults**. Cloud-backup settings (endpoint, bucket, region, access/secret key, prefix, retention) can also be edited at runtime from the UI (**Settings → Cloud Backup**) and are stored in the `app_meta` table; UI values override the env defaults. `TACK_BACKUP_INTERVAL_SECS` (automatic scheduling) remains env-only and takes effect at startup. The secret key is write-only over the API — never returned to clients.

### Debugging

```bash
# Debug logging
TACK_LOG_LEVEL=debug cargo run -p tack-cli -- serve

# Trace SQL queries
RUST_LOG=tack_db=trace,tack_api=debug cargo run -p tack-cli -- serve

# JSON logs (for log aggregators)
TACK_LOG_JSON=true cargo run -p tack-cli -- serve
```

## Architecture

**Project structure:**
```
crates/
├── tack-core/     Pure business logic (no I/O)
├── tack-db/       SQLite persistence layer
├── tack-api/      Axum HTTP server + WebSocket (library; pub fn serve)
└── tack-cli/      The single `tack` binary — runs the server (tack serve) and the CLI client

frontend/
├── src/
│   ├── components/  Reusable UI components
│   ├── pages/       Route pages (Board, Projects)
│   ├── lib/         Utilities (API client, WebSocket, optimistic UI)
│   └── types/       TypeScript type definitions
├── public/          Static assets
└── package.json     Frontend dependencies (SolidJS, Vite, Tailwind v4)

docs/                Documentation
├── API-REFERENCE.md Complete API documentation
├── API-EXAMPLES.md  Example workflows with curl
├── TESTING.md       Testing guide
└── *.md            Various guides
```

### Crate Boundaries & Responsibilities

**tack-core** (pure, zero I/O):
- Domain models: `Project`, `Item`, `Sprint`, `Role`, `Comment`, `Dependency`, etc.
- Workflow engine: validates transitions, enforces WIP limits, provides presets
- Vocabulary system: customizable term mapping per project
- Dependency graph: DAG with cycle detection (DFS-based)
- Error types: typed domain errors (`CoreError`)

**tack-db**:
- SQLite via `sqlx` (async)
- 17 migrations with FTS5 full-text search on items
- Repository pattern: CRUD for all entities in `repo/` submodules
- Auto-runs migrations on startup
- Database is created automatically if missing

**tack-api** (library — does not build its own binary):
- Axum HTTP server with 34 REST endpoints (100% complete)
- Server entry point exposed as `tack_api::serve()` (in `server.rs`)
- WebSocket support for real-time board updates
- Request handlers in `handlers/` (per entity)
- Config loading: TOML + env vars
- Error mapping: `CoreError` → HTTP status codes
- Debug endpoints: `/api/health`, `/api/debug/info`, `/api/debug/db-stats`
- File upload support: multipart/form-data (max 50MB)
- Export functionality: JSON and CSV formats

**tack-cli** (the single `tack` binary):
- `tack` with no subcommand (or `tack serve`) starts the server + web UI via `tack_api::serve()` — the primary, UI-first entry point
- CLI client using `clap`: `init`, `add`, `list`, `move`, `board`, `search`, `sprint`, `template`, `role`, `comment`, `field`, `backup`, `restore` (complete)
- Client commands talk to the server over HTTP (blocking `reqwest`); never open the DB directly

**frontend** (SolidJS + TypeScript):
- Responsive SPA with dark mode support
- **Board view** with HTML5 drag-and-drop (visual Kanban-style)
- **List view** with sortable table, filtering, and bulk operations
- WebSocket integration for real-time updates
- Optimistic UI updates (instant feedback)
- Keyboard shortcuts + command palette (Ctrl+K)
- Global search (Ctrl+/)
- Toast notifications
- Skeleton loading screens
- Status: 100% complete (all core features working)

### Key Design Patterns

**Universal Item Model**: All work units (epics, features, tasks, bugs, etc.) share the same `Item` struct. The `item_type` field and project vocabulary determine how they're labeled and displayed.

**Workflow Engine**: Each project has a `WorkflowConfig` defining:
- Status columns (name, category, WIP limit, order)
- Optional explicit transitions (e.g., construction workflow enforces linear progression)
- Presets: `scrum_workflow()`, `kanban_workflow()`, `simple_workflow()`, `construction_workflow()`

**Vocabulary Mapping**: Projects store a `VocabularyMap` to rename terms:
```rust
{
  "task": "Work Order",
  "sprint": "Phase",
  "epic": "Building"
}
```

**Dependency Graph**: DAG-based system prevents cycles. Uses adjacency lists for both forward (`edges`) and reverse (`reverse_edges`) lookups. Validates new edges before insertion.

**Project Types**: Each `ProjectType` (software, construction, personal, etc.) auto-selects default workflow and vocabulary. Users can fully customize after creation.

### Database Schema Highlights

- **17 migrations** tracked in `_migrations` table
- **FTS5 virtual table** (`items_fts`) for full-text search across titles, descriptions, tags
- **Triggers** maintain FTS index on INSERT/UPDATE/DELETE
- **Foreign keys** enforce referential integrity (e.g., items → projects, items → sprints)
- **Indexes** on common queries: project_id, status, priority, parent_id
- **Attachments table** with file metadata (filename, mime_type, storage_path, size)

### API Endpoint Structure

All routes follow RESTful conventions:

- `/api/projects` — CRUD for projects (5 endpoints)
- `/api/projects/{id}/boards` — Multiple boards per project (CRUD + view)
  - `GET /api/projects/{id}/boards/live` — **WebSocket** for real-time updates
- `/api/projects/{id}/export` — Export to JSON/CSV (1 endpoint)
- `/api/projects/import` — Import from JSON (1 endpoint)
- `/api/projects/{id}/items` — Items scoped to project (3 endpoints)
- `/api/items/{id}` — Individual item operations (3 endpoints with WebSocket broadcasting)
- `/api/items/{id}/dependencies` — Dependency management (3 endpoints)
- `/api/items/{id}/attachments` — File attachments (2 endpoints)
- `/api/attachments/{id}` — Download/delete attachments (2 endpoints)
- `/api/projects/{id}/sprints` — Sprint management (4 endpoints)
- `/api/projects/{id}/roles` — Role/specialty management (5 endpoints)
- `/api/items/{id}/comments` — Comments on items (2 endpoints)
- `/api/projects/{id}/search` — Full-text search within project (1 endpoint)
- `/api/search` — **Global search** across all projects (1 endpoint)
- `/api/alexa` — **Alexa skill webhook** (1 endpoint; disabled unless `TACK_ALEXA_SKILL_ID` is set; authenticates via skill-ID + timestamp checks and is exempt from the Bearer-token gate)
- `/api/projects/{id}/import-github` — GitHub Issues import (1 endpoint; `owner/repo` or full URL, optional PAT, label filter, PR-skipping, cursor pagination)
- `/api/projects/{id}/import-linear` — Linear import (1 endpoint; Linear API key, optional team/project filter, label filter, priority mapping, cursor pagination)
- `/api/backup`, `/api/restore` — Local DB backup download / staged restore (2 endpoints)
- `/api/backup/remote` (POST/GET), `/api/backup/remote/restore` — Cloud (S3-compatible) backup, list, and staged restore (3 endpoints)
- `/api/settings/backup` (GET/PUT) — Read/update the UI-editable cloud-backup config; secret key is write-only (returned as a `secret_key_set` boolean)

Query parameters support filtering, pagination, and search.

## Important Implementation Details

### Workflow Validation

When updating an item's status:
1. Check if both statuses exist in project workflow
2. If explicit transitions defined, validate the move is allowed
3. Check WIP limit for target column (before adding)
4. Update `started_at` when moving to in-progress category
5. Update `completed_at` when moving to done category
6. **Auto-propagate parent status** if item has a parent and all siblings are complete

**Example:** Construction projects have strict transitions (Permit → Procurement → Build → Inspect → Handover). Jumping from Permit to Handover is rejected.

### Auto-Status Propagation

When a child item is moved to a "done" status, the system automatically checks if all siblings are also complete. If so, the parent item is automatically updated to "done" as well. This cascades up the hierarchy.

**Implementation:**
- Repository method: `check_and_update_parent_status(parent_id, completed_status)`
- Triggered in `update_item` handler after status change
- Only activates when moving to a status with `StatusCategory::Done`
- Errors are silently ignored (best-effort feature)

**Example workflow:**
1. Epic "User Auth" has 3 child tasks
2. Complete task 1 → no parent update (still 2 incomplete)
3. Complete task 2 → no parent update (still 1 incomplete)
4. Complete task 3 → **parent epic auto-completes** ✓

### Dependency Cycle Detection

Before creating a dependency:
1. Check for self-reference (`source == target`)
2. Build adjacency graph from existing dependencies
3. Run DFS from target to see if it can reach source
4. Reject if cycle detected

The graph is reconstructed on each validation. For large dependency sets, consider caching.

### Sprint Status Lifecycle

Sprints have four states:
- `Planning` → `Active` → `Review` → `Closed`

Items can only be assigned to active or planning sprints (enforced in handlers).

### Tags & Search

- Tags are stored as JSON arrays in SQLite
- FTS5 index includes tags, titles, descriptions
- Search via `/api/projects/{id}/search?q=term` uses FTS5 `MATCH`

### File Attachments

- Upload via multipart/form-data to `/api/items/{id}/attachments`
- Max file size: 50MB (configurable in handler)
- Files stored in `TACK_STORAGE_DIR` organized by item ID
- Unique filenames prevent collisions (UUID-based)
- Download includes proper Content-Type and Content-Disposition headers
- Metadata stored in database: filename, mime_type, storage_path, size_bytes

### Export/Import

- **JSON Export**: Complete project snapshot with items, sprints, metadata
  - `GET /api/projects/{id}/export?format=json`
  - Returns downloadable JSON file with all project data
- **CSV Export**: Simplified item list for spreadsheet import
  - `GET /api/projects/{id}/export?format=csv`
  - Includes: id, title, type, status, priority, parent_id, created_at
- **Import**: Placeholder endpoint for future implementation
  - `POST /api/projects/import` (basic structure in place)

### WebSocket Real-Time Updates

- **Endpoint**: `GET /api/projects/{id}/boards/live` (WebSocket upgrade)
- **Purpose**: Real-time board state updates for live collaboration
- **Implementation**:
  - Uses Tokio broadcast channel (100 message capacity)
  - AppState contains `broadcast_tx: broadcast::Sender<BoardEvent>`
  - Each WebSocket connection subscribes to the broadcast channel
  - Events are filtered by project_id before sending to client
- **Event Types**:
  - `ItemCreated` - New item added to board
  - `ItemUpdated` - Item status/details changed
  - `ItemDeleted` - Item removed
  - `BoardConfigUpdated` - WIP limits or columns changed
  - `SprintUpdated` - Sprint status changed
  - `Ping` - Keepalive (sent to all)
- **Usage**: Frontend connects via WebSocket, receives JSON events, updates UI reactively
- **Broadcasting**: Other handlers call `websocket::broadcast_event()` to notify all subscribers

### Testing Strategy

- **Unit tests** in `tack-core` for business logic (workflow, dependencies, vocabulary)
- **Integration tests** in `tack-db` for repository operations (require SQLite)
- **Handler tests** in `tack-api` use in-memory databases
- **Frontend unit tests** in `frontend/src/**/*.test.tsx` (Vitest + jsdom)
- **End-to-end tests** in `frontend/e2e/` (Playwright): cross-browser smoke, user
  journeys, axe accessibility scans, and API wire-contract checks. Test setup
  talks to the API directly; the browser exercises the SPA via the proxy.
- **Security**: `cargo audit` + `npm audit` in CI; justified advisory exceptions
  in `.cargo/audit.toml`. **Performance**: k6 baseline in `tests/load/`.

Use `assert_matches!` macro for enum matching in tests. When changing an API
response shape, update both the Rust handler and the matching mock in the
frontend unit/E2E tests (e.g. the `GET /items/{id}` detail envelope).

## Common Patterns When Adding Features

### Adding a New Entity

1. Define model in `tack-core/src/models.rs`
2. Add migration in `tack-db/src/migrations.rs`
3. Create repository module in `tack-db/src/repo/`
4. Add handler in `tack-api/src/handlers/`
5. Register routes in `tack-api/src/router.rs`
6. Add DTOs for create/update operations

### Extending Workflow Logic

1. Add logic to `tack-core/src/workflow.rs`
2. Write unit tests in the same file
3. Update handlers to call new validation/logic
4. No database changes needed (workflow is JSON in DB)

### Adding a New Project Type

1. Add variant to `ProjectType` enum in `models.rs`
2. Create workflow preset in `workflow.rs` (e.g., `education_workflow()`)
3. Create vocabulary preset in `vocabulary.rs`
4. Update `workflow_for_type()` match statement

## Troubleshooting

**Migration errors on startup:**
- Check `_migrations` table: `SELECT * FROM _migrations`
- Manually delete failed migration record if needed
- Restart server to retry

**Database locked errors:**
- SQLite only supports one writer at a time
- Check if another process has the DB open
- Use `?mode=rwc` in connection string (already default)

**FTS5 not found:**
- SQLite must be compiled with FTS5 support
- Check with: `sqlite3 tack.db "PRAGMA compile_options;"`

## Code Style Notes

- Use `tracing` macros for logging (`info!`, `debug!`, `warn!`, `error!`)
- Instrument async functions with `#[instrument(skip(pool))]`
- Prefer `thiserror` for error types in core/db, `anyhow` for CLI
- Use `chrono::DateTime<Utc>` for all timestamps
- Use `uuid::Uuid` (v4) for all IDs
- Store UUIDs as TEXT in SQLite (for compatibility)
- JSON fields in SQLite use `serde_json::Value` or typed serialization

## Release Configuration

The workspace uses aggressive size optimization in release mode:
```toml
[profile.release]
lto = true              # Link-time optimization
strip = true            # Strip symbols
codegen-units = 1       # Better optimization, slower compile
opt-level = "z"         # Optimize for size
```

This produces the single `tack` binary — ~10 MB with the SPA embedded (`--features embed-spa`), or ~3.5 MB without. For faster compile times during development, use `--release` sparingly.
