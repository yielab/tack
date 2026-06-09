# Contributing to FlexPM

## Development Setup

### Requirements

| Tool | Version | Install |
| --- | --- | --- |
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Git | 2.x | System package manager |
| curl | any | Pre-installed on most systems |
| jq | any | `apt install jq` / `brew install jq` (optional, for pretty JSON) |

### First-Time Setup

```bash
git clone <repo-url>
cd flexpm

# Build the workspace (downloads dependencies on first run)
cargo build

# Run all tests to verify everything works
cargo test

# Start the dev server
cargo run --bin flexpm-api
```

No external database, Docker, or services needed. SQLite is embedded.

## Project Structure

```text
flexpm/
├── Cargo.toml                 # Workspace root (shared dependencies)
├── Cargo.lock                 # Pinned dependency versions
├── config/
│   └── flexpm.example.toml    # Example configuration file
├── crates/
│   ├── flexpm-core/           # Pure domain logic (no I/O, no DB)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs      # All data structures and DTOs
│   │       ├── workflow.rs    # Workflow engine (transitions, WIP)
│   │       ├── vocabulary.rs  # Term customization system
│   │       ├── dependency.rs  # Dependency graph (DAG)
│   │       └── error.rs       # Domain error types
│   ├── flexpm-db/             # Database layer
│   │   ├── src/
│   │   │   ├── lib.rs         # Pool initialization, WAL mode
│   │   │   ├── migrations.rs  # 16 schema migrations (auto-run on startup)
│   │   │   ├── repo.rs        # Repository struct
│   │   │   └── repo/          # One file per entity
│   │   │       ├── projects.rs
│   │   │       ├── items.rs
│   │   │       ├── sprints.rs
│   │   │       ├── roles.rs
│   │   │       ├── comments.rs
│   │   │       ├── dependencies.rs
│   │   │       ├── attachments.rs
│   │   │       ├── boards.rs
│   │   │       ├── custom_fields.rs
│   │   │       └── templates.rs
│   │   └── tests/
│   │       ├── integration_test.rs  # DB integration tests
│   │       └── perf_test.rs         # 50k-item perf test (#[ignore])
│   ├── flexpm-api/            # Axum HTTP server + WebSocket
│   │   └── src/
│   │       ├── main.rs        # Server entry point + staged restore
│   │       ├── lib.rs
│   │       ├── router.rs      # All routes, AppState, middleware wiring
│   │       ├── config.rs      # TOML/env config, db_file_path()
│   │       ├── error.rs       # ApiError → HTTP status mapping
│   │       ├── debug.rs       # /api/health, /api/debug/*
│   │       ├── middleware.rs  # Bearer token auth
│   │       └── handlers/
│   │           ├── attachments.rs
│   │           ├── backup.rs       # GET /api/backup, POST /api/restore
│   │           ├── boards_multi.rs
│   │           ├── comments.rs
│   │           ├── custom_fields.rs
│   │           ├── dependencies.rs
│   │           ├── export.rs       # JSON/CSV export + import
│   │           ├── items.rs
│   │           ├── projects.rs
│   │           ├── roles.rs
│   │           ├── spa.rs          # SPA fallback (--features embed-spa)
│   │           ├── sprints.rs
│   │           ├── templates.rs
│   │           └── websocket.rs
│   └── flexpm-cli/            # clap CLI (talks to API over HTTP)
│       └── src/
│           ├── main.rs        # All commands
│           ├── client.rs      # HTTP client wrapper (reqwest)
│           ├── config.rs      # ~/.flexpmrc reader
│           └── vocab.rs       # Vocabulary-aware output
├── frontend/
│   ├── src/
│   │   ├── components/        # Reusable UI components
│   │   ├── pages/             # Board, List, Dashboard, Sprints, …
│   │   ├── lib/               # api.ts, vocab.ts, websocket, optimistic UI
│   │   └── types/             # TypeScript types
│   └── dist/                  # Built SPA (gitignored; embedded via embed-spa)
└── docs/                      # Documentation
```

## Dependency Flow

```text
flexpm-core  (pure logic, no I/O)
     ^
     |
flexpm-db    (depends on core, adds SQLite)
     ^
     |
flexpm-api   (depends on core + db, adds HTTP)

flexpm-cli   (depends on core only — talks to flexpm-api over HTTP, no DB)
```

**Rule:** `flexpm-core` must never import `flexpm-db` or any I/O crate.
Keep business logic testable without a database. `flexpm-cli` must never import
`flexpm-db` — all data access goes through the HTTP API.

## Development Workflow

### Common Commands

```bash
# ─── Building ────────────────────────────────────
cargo build                    # Debug build (fast compile)
cargo build --release          # Release build (optimized)
cargo build -p flexpm-core     # Build only one crate

# ─── Testing ─────────────────────────────────────
cargo test                     # Run all tests
cargo test -p flexpm-core      # Test one crate
cargo test -p flexpm-db        # Test DB layer (integration tests)
cargo test test_workflow        # Run tests matching a name
cargo test -- --nocapture      # Show println! output during tests

# ─── Running ─────────────────────────────────────
cargo run --bin flexpm-api     # Start the API server
cargo run --bin flexpm-cli -- --help   # CLI help

# ─── Code Quality ────────────────────────────────
cargo clippy --workspace       # Lint all crates
cargo fmt --all                # Format all code
cargo fmt --all -- --check     # Check formatting (CI)
cargo check                    # Type-check without building

# ─── Debugging ───────────────────────────────────
RUST_LOG=debug cargo run --bin flexpm-api          # Debug logging
RUST_LOG=flexpm_db=trace cargo run --bin flexpm-api # Trace DB queries
FLEXPM_LOG_JSON=true cargo run --bin flexpm-api     # JSON log output
```

### Manual API Testing

Once the server is running (`cargo run --bin flexpm-api`):

```bash
# 1. Check health
curl -s localhost:3210/api/health | jq

# 2. Create a project
curl -s -X POST localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Project","project_type":"software"}' | jq

# 3. Copy the project ID, then create an item
curl -s -X POST localhost:3210/api/projects/<PROJECT_ID>/items \
  -H "Content-Type: application/json" \
  -d '{"title":"My first task","item_type":"task","priority":"high"}' | jq

# 4. List all items
curl -s localhost:3210/api/projects/<PROJECT_ID>/items | jq

# 5. Check DB stats
curl -s localhost:3210/api/debug/db-stats | jq
```

### Writing Tests

**Unit tests** go in the same file as the code, inside a `#[cfg(test)]` module:

```rust
// In crates/flexpm-core/src/workflow.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "In Progress").is_ok());
    }
}
```

**Integration tests** (requiring a database) go in `crates/flexpm-db/tests/`:

```rust
// In crates/flexpm-db/tests/integration_test.rs

#[tokio::test]
async fn test_my_db_feature() {
    let repo = setup_test_db().await;   // In-memory SQLite
    let ws_id = create_test_workspace(&repo).await;

    let project = repo.create_project(ws_id, CreateProject {
        name: "Test".into(),
        description: None,
        project_type: ProjectType::Software,
        template: None,
    }).await.unwrap();

    assert_eq!(project.name, "Test");
}
```

The `setup_test_db()` helper creates a fresh in-memory SQLite database with
all migrations applied. Each test gets its own isolated database.

## How To Add a New Feature

### Adding a New Entity (e.g., "TimeEntry")

1. **Define the model** in `crates/flexpm-core/src/models.rs`:

   ```rust
   pub struct TimeEntry {
       pub id: Uuid,
       pub item_id: Uuid,
       pub duration_minutes: i32,
       pub started_at: DateTime<Utc>,
       pub notes: Option<String>,
   }

   pub struct CreateTimeEntry {
       pub item_id: Uuid,
       pub duration_minutes: i32,
       pub notes: Option<String>,
   }
   ```

2. **Add a migration** in `crates/flexpm-db/src/migrations.rs`:

   ```rust
   const MIGRATION_011: [&str; 1] = [
       "CREATE TABLE IF NOT EXISTS time_entries ( ... )",
   ];
   ```

   Add it to the `migrations` vec in `run_all()`.

3. **Add a repository module** at `crates/flexpm-db/src/repo/time_entries.rs`:
   - Implement `create_time_entry`, `list_time_entries`, etc. on `Repository`
   - Add `pub mod time_entries;` to `crates/flexpm-db/src/repo.rs`

4. **Add a handler module** at `crates/flexpm-api/src/handlers/time_entries.rs`:
   - Add `pub mod time_entries;` to `crates/flexpm-api/src/handlers.rs`

5. **Add routes** in `crates/flexpm-api/src/router.rs`:

   ```rust
   .route("/items/{item_id}/time", post(time_entries::create))
   .route("/items/{item_id}/time", get(time_entries::list))
   ```

6. **Write tests** in `crates/flexpm-db/tests/integration_test.rs`

7. **Run tests**: `cargo test`

### Adding a New Workflow Preset

Edit `crates/flexpm-core/src/workflow.rs`:

```rust
pub fn my_custom_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Custom,
        statuses: vec![
            StatusDef { name: "New".into(), category: StatusCategory::Todo, wip_limit: None, order: 0 },
            StatusDef { name: "Active".into(), category: StatusCategory::InProgress, wip_limit: Some(3), order: 1 },
            StatusDef { name: "Complete".into(), category: StatusCategory::Done, wip_limit: None, order: 2 },
        ],
        transitions: None,
    }
}
```

### Adding a New Vocabulary Pack

Edit `crates/flexpm-core/src/vocabulary.rs`, add a new match arm in `vocabulary_for_type()`.

## Database

### Schema

The database is SQLite with WAL mode enabled for concurrent reads. Tables:

| Table | Purpose |
| --- | --- |
| `workspaces` | Top-level container (one per installation) |
| `projects` | Projects with vocabulary + workflow JSON |
| `sprints` | Sprints/iterations with status lifecycle |
| `items` | Universal work items (tasks, epics, etc.) with hierarchy |
| `dependencies` | Item-to-item relationships (blocks, relates_to, etc.) |
| `roles` | Role/specialty definitions per project |
| `item_roles` | Many-to-many junction between items and roles |
| `comments` | Comments and activity log per item |
| `attachments` | File metadata per item |
| `boards` | Multiple boards per project (grouping, filters) |
| `project_templates` | Reusable project blueprints |
| `custom_field_definitions` | User-defined field types per project |
| `custom_field_values` | Field values per item |
| `items_fts` | FTS5 virtual table — full-text search (auto-synced via triggers) |

### Resetting the Database

```bash
# Delete the database file and restart (migrations re-run automatically)
rm flexpm.db flexpm.db-shm flexpm.db-wal
cargo run --bin flexpm-api
```

### Inspecting the Database

```bash
# SQLite CLI (if installed)
sqlite3 flexpm.db

# Useful queries
.tables                                    -- List all tables
.schema items                              -- Show table schema
SELECT COUNT(*) FROM items;                -- Count items
SELECT * FROM _migrations;                 -- See applied migrations
PRAGMA journal_mode;                       -- Should show "wal"
```

## Error Handling

- **`flexpm-core`** uses `CoreError` (thiserror) for domain errors
- **`flexpm-db`** uses `sqlx::Error` and `DependencyError`
- **`flexpm-api`** uses `ApiError` which maps all errors to HTTP status codes:

| Domain Error | HTTP Status |
| --- | --- |
| `ItemNotFound`, `ProjectNotFound` | 404 |
| `InvalidTransition`, `WipLimitExceeded` | 400 |
| `DependencyCycle` | 400 |
| `sqlx::Error` (internal) | 500 |

Internal errors (500) log the full error but return only "Internal server error" to the client.

## Logging

All logging uses the `tracing` crate with structured spans.

- **Handlers**: `#[instrument(skip(state))]` auto-creates spans with function args
- **Repository methods**: Same instrumentation, logs at `debug` level
- **HTTP middleware**: `TraceLayer` logs every request with method, URI, and duration
- **Migrations**: Log each migration name on apply

Set log levels via `RUST_LOG` environment variable:

```bash
# Only errors
RUST_LOG=error cargo run --bin flexpm-api

# Debug the database layer specifically
RUST_LOG=flexpm_db=debug cargo run --bin flexpm-api

# Trace everything (very verbose)
RUST_LOG=trace cargo run --bin flexpm-api
```

## Code Style

- Run `cargo fmt --all` before committing
- Run `cargo clippy --workspace` and fix all warnings
- Keep `flexpm-core` free of I/O dependencies
- Use `#[instrument]` on public functions for tracing
- Prefer returning `Result` over panicking
- Write tests for any new business logic
