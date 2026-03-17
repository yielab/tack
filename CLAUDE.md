# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FlexPM is a lightweight, versatile project management tool built in Rust (backend) and SolidJS (frontend). It supports multiple workflows (Scrum, Kanban, phase-based) with fully customizable terminology and statuses. Designed for solo developers and small teams across diverse domains: software development, construction, personal tasks, etc.

**Core Philosophy:** Universal work tracking with domain-specific vocabulary. The same underlying system adapts to different project types through configurable workflows and terminology.

**Current Status:** ✅ **Production-Ready** (Phase 4 Complete - 100%)
- Backend: 100% complete (34 REST endpoints + WebSocket)
- Frontend: 100% complete (Board view, List view, optimistic UI, real-time updates)
- CLI: 20% complete (structure exists, needs implementation)

## Development Commands

### Building and Running

```bash
# Build everything (workspace)
cargo build

# Build release binary
cargo build --release

# Run API server (http://127.0.0.1:3210)
cargo run --bin flexpm-api

# Run CLI tool
cargo run --bin flexpm-cli -- --help
```

### Testing

```bash
# Run all tests (unit + integration)
cargo test

# Show test output
cargo test -- --nocapture

# Run specific test
cargo test test_workflow_transition_validation

# Run tests for a specific crate
cargo test -p flexpm-core
cargo test -p flexpm-db
```

### Docker (Recommended)

```bash
# Start all services (backend + frontend + Caddy)
docker compose up -d

# View logs
docker compose logs -f

# Rebuild after code changes
docker compose up -d --build

# Stop and remove all data
docker compose down -v

# Use CLI inside container
docker compose exec flexpm flexpm-cli --help
```

**Services:**
- `flexpm` - Backend API (http://localhost:3210)
- `frontend` - Frontend SPA (http://localhost:8080)
- `caddy` - Reverse proxy (https://flexpm.local - requires hosts file entry)

### Frontend Development

```bash
cd frontend

# Install dependencies
npm install

# Development server with hot reload
npm run dev

# Build for production
npm run build

# Type checking
npm run type-check
```

### Configuration

The API server loads configuration from `flexpm.toml` (if present) or environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `FLEXPM_HOST` | `127.0.0.1` | Server bind address |
| `FLEXPM_PORT` | `3210` | Server port |
| `FLEXPM_DATABASE_URL` | `sqlite:flexpm.db?mode=rwc` | SQLite database path |
| `FLEXPM_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `FLEXPM_LOG_JSON` | `false` | Structured JSON logging |
| `FLEXPM_LOG_FILE` | _(none)_ | Optional log file path |
| `FLEXPM_STORAGE_DIR` | `./storage` | Attachment storage directory |

### Debugging

```bash
# Debug logging
FLEXPM_LOG_LEVEL=debug cargo run --bin flexpm-api

# Trace SQL queries
RUST_LOG=flexpm_db=trace,flexpm_api=debug cargo run --bin flexpm-api

# JSON logs (for log aggregators)
FLEXPM_LOG_JSON=true cargo run --bin flexpm-api
```

## Architecture

**Project structure:**
```
crates/
├── flexpm-core/     Pure business logic (no I/O)
├── flexpm-db/       SQLite persistence layer
├── flexpm-api/      Axum HTTP server + WebSocket
└── flexpm-cli/      CLI tool (clap)

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

**flexpm-core** (pure, zero I/O):
- Domain models: `Project`, `Item`, `Sprint`, `Role`, `Comment`, `Dependency`, etc.
- Workflow engine: validates transitions, enforces WIP limits, provides presets
- Vocabulary system: customizable term mapping per project
- Dependency graph: DAG with cycle detection (DFS-based)
- Error types: typed domain errors (`CoreError`)

**flexpm-db**:
- SQLite via `sqlx` (async)
- 10 migrations with FTS5 full-text search on items
- Repository pattern: CRUD for all entities in `repo/` submodules
- Auto-runs migrations on startup
- Database is created automatically if missing

**flexpm-api**:
- Axum HTTP server with 34 REST endpoints (100% complete)
- WebSocket support for real-time board updates
- Request handlers in `handlers/` (per entity)
- Config loading: TOML + env vars
- Error mapping: `CoreError` → HTTP status codes
- Debug endpoints: `/api/health`, `/api/debug/info`, `/api/debug/db-stats`
- File upload support: multipart/form-data (max 50MB)
- Export functionality: JSON and CSV formats

**flexpm-cli**:
- Terminal interface using `clap`
- Commands: `init`, `add`, `list`, `move`, etc.
- Status: 20% complete (basic structure exists)

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

- **10 migrations** tracked in `_migrations` table
- **FTS5 virtual table** (`items_fts`) for full-text search across titles, descriptions, tags
- **Triggers** maintain FTS index on INSERT/UPDATE/DELETE
- **Foreign keys** enforce referential integrity (e.g., items → projects, items → sprints)
- **Indexes** on common queries: project_id, status, priority, parent_id
- **Attachments table** with file metadata (filename, mime_type, storage_path, size)

### API Endpoint Structure (34 endpoints - 100% complete)

All routes follow RESTful conventions:
- `/api/projects` - CRUD for projects (5 endpoints)
- `/api/projects/{id}/board` - Board view with WIP limits (3 endpoints)
  - GET `/api/projects/{id}/board` - Get board state
  - PATCH `/api/projects/{id}/board` - Update board config (broadcasts WebSocket event)
  - GET `/api/projects/{id}/board/live` - **WebSocket** for real-time updates
- `/api/projects/{id}/export` - Export to JSON/CSV (1 endpoint)
- `/api/projects/import` - Import from JSON (1 endpoint)
- `/api/projects/{id}/items` - Items scoped to project (3 endpoints)
- `/api/items/{id}` - Individual item operations (3 endpoints with WebSocket broadcasting)
- `/api/items/{id}/dependencies` - Dependency management (3 endpoints)
- `/api/items/{id}/attachments` - File attachments (2 endpoints)
- `/api/attachments/{id}` - Download/delete attachments (2 endpoints)
- `/api/projects/{id}/sprints` - Sprint management (4 endpoints)
- `/api/projects/{id}/roles` - Role/specialty management (5 endpoints)
- `/api/items/{id}/comments` - Comments on items (2 endpoints)
- `/api/projects/{id}/search` - Full-text search within project (1 endpoint)
- `/api/search` - **Global search** across all projects (1 endpoint)

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
- Files stored in `FLEXPM_STORAGE_DIR` organized by item ID
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

- **Endpoint**: `GET /api/projects/{id}/board/live` (WebSocket upgrade)
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

- **Unit tests** in `flexpm-core` for business logic (workflow, dependencies, vocabulary)
- **Integration tests** in `flexpm-db` for repository operations (require SQLite)
- **Handler tests** in `flexpm-api` use in-memory databases

Use `assert_matches!` macro for enum matching in tests.

## Common Patterns When Adding Features

### Adding a New Entity

1. Define model in `flexpm-core/src/models.rs`
2. Add migration in `flexpm-db/src/migrations.rs`
3. Create repository module in `flexpm-db/src/repo/`
4. Add handler in `flexpm-api/src/handlers/`
5. Register routes in `flexpm-api/src/router.rs`
6. Add DTOs for create/update operations

### Extending Workflow Logic

1. Add logic to `flexpm-core/src/workflow.rs`
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
- Check with: `sqlite3 flexpm.db "PRAGMA compile_options;"`

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

This produces ~5MB binaries. For faster compile times during development, use `--release` sparingly.
