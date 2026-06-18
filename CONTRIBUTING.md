# Contributing to FlexPM

## Quick Start

```bash
git clone https://github.com/santiagoyie/FlexPM.git
cd FlexPM

# Activate the pre-push hook — runs fmt + clippy before every push
git config core.hooksPath .githooks

# Verify the build and tests pass
cargo build
cargo test --workspace
```

The hook in `.githooks/pre-push` runs `cargo fmt --all --check` and `cargo clippy --workspace -- -D warnings` automatically before every `git push`. This mirrors CI exactly, so formatting and lint failures are caught locally before they ever reach GitHub.

---

## Requirements

| Tool | Version | Install |
| --- | --- | --- |
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | [nodejs.org](https://nodejs.org/) |
| Git | 2.x | system package manager |
| curl | any | pre-installed on most systems |
| jq | any | `apt install jq` / `brew install jq` (optional, for pretty JSON) |

No external database, Docker, or services needed. SQLite is embedded.

---

## Project Structure

```text
FlexPM/
├── Cargo.toml                  # Workspace root (shared dependencies)
├── Cargo.lock                  # Pinned dependency versions
├── Makefile                    # Common dev commands
├── .githooks/
│   └── pre-push                # fmt + clippy gate (activate with git config core.hooksPath .githooks)
├── crates/
│   ├── flexpm-core/            # Pure domain logic (no I/O, no DB)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs       # All data structures, DTOs, and custom-field validation
│   │       ├── workflow.rs     # Workflow engine (transitions, WIP, parent-auto-complete)
│   │       ├── vocabulary.rs   # Term customization system
│   │       ├── dependency.rs   # Dependency graph (DAG with cycle detection)
│   │       └── error.rs        # Domain error types
│   ├── flexpm-db/              # Database layer
│   │   ├── src/
│   │   │   ├── lib.rs          # Pool initialization, WAL mode
│   │   │   ├── migrations.rs   # 16 schema migrations (auto-run on startup)
│   │   │   ├── repo.rs         # Repository struct
│   │   │   └── repo/           # One file per entity
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
│   ├── flexpm-api/             # Axum HTTP server + WebSocket
│   │   ├── src/
│   │   │   ├── main.rs         # Server entry point + staged restore
│   │   │   ├── lib.rs
│   │   │   ├── router.rs       # All routes, AppState, middleware
│   │   │   ├── config.rs       # TOML/env config
│   │   │   ├── error.rs        # ApiError → HTTP status mapping
│   │   │   ├── debug.rs        # /api/health, /api/debug/*
│   │   │   ├── middleware.rs   # Bearer token auth
│   │   │   ├── webhook.rs      # Outbound webhook delivery
│   │   │   └── handlers/
│   │   │       ├── alexa.rs            # POST /api/alexa voice skill
│   │   │       ├── attachments.rs
│   │   │       ├── backup.rs           # GET /api/backup, POST /api/restore
│   │   │       ├── boards_multi.rs     # Multiple boards per project
│   │   │       ├── comments.rs
│   │   │       ├── custom_fields.rs
│   │   │       ├── dependencies.rs
│   │   │       ├── export.rs           # JSON/CSV export + import
│   │   │       ├── import_github.rs    # GitHub Issues import
│   │   │       ├── items.rs
│   │   │       ├── projects.rs
│   │   │       ├── roles.rs
│   │   │       ├── spa.rs              # SPA fallback (--features embed-spa)
│   │   │       ├── sprints.rs
│   │   │       ├── templates.rs
│   │   │       └── websocket.rs
│   │   └── tests/
│   │       ├── common/mod.rs       # test_app(), test_app_with_config()
│   │       ├── api_test.rs         # 36 handler integration tests
│   │       └── alexa_test.rs       # 17 Alexa endpoint tests
│   └── flexpm-cli/             # clap CLI (talks to API over HTTP)
│       └── src/
│           ├── main.rs         # All commands
│           ├── client.rs       # HTTP client wrapper (reqwest)
│           ├── config.rs       # ~/.flexpmrc reader
│           └── vocab.rs        # Vocabulary-aware output
├── frontend/
│   ├── src/
│   │   ├── components/         # Reusable UI components
│   │   ├── pages/              # Board, List, Dashboard, Sprints, Calendar, Timeline, Settings, Templates
│   │   ├── lib/                # api.ts, vocab.ts, websocket, optimistic UI
│   │   └── types/              # TypeScript types
│   └── dist/                   # Built SPA (gitignored; embedded via --features embed-spa)
└── docs/                       # Documentation
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

---

## Development Workflow

### Common Commands

```bash
# ─── Building ────────────────────────────────────
cargo build                    # Debug build (fast compile)
cargo build --release          # Release build (optimized, ~5 MB binary)
cargo build -p flexpm-core     # Build only one crate

# ─── Testing ─────────────────────────────────────
cargo test --workspace         # Run all 164 tests
cargo test -p flexpm-core      # Test core (67 unit tests)
cargo test -p flexpm-db        # Test DB layer (22 integration tests)
cargo test -p flexpm-api       # Test API (64 tests)
cargo test -p flexpm-cli       # Test CLI (11 tests)
cargo test test_workflow        # Run tests matching a name
cargo test -- --nocapture      # Show println! output during tests

# Frontend tests
cd frontend && npm test         # 144 Vitest unit tests

# ─── Running ─────────────────────────────────────
cargo run --bin flexpm-api              # Start the API server
cargo run --bin flexpm-cli -- --help    # CLI help

# ─── Code Quality ────────────────────────────────
cargo fmt --all                         # Format all code
cargo fmt --all --check                 # Check formatting (same as CI)
cargo clippy --workspace -- -D warnings # Lint (same as CI)
cargo check                             # Type-check without building

# ─── Debugging ───────────────────────────────────
RUST_LOG=debug cargo run --bin flexpm-api           # Debug logging
RUST_LOG=flexpm_db=trace cargo run --bin flexpm-api # Trace SQL queries
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

**API handler tests** go in `crates/flexpm-api/tests/api_test.rs` using `axum::Router::oneshot()`:

```rust
#[tokio::test]
async fn my_handler_returns_200() {
    let (app, _) = common::test_app().await;
    let res = app.oneshot(
        Request::builder()
            .method(Method::GET)
            .uri("/api/health")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

**DB integration tests** go in `crates/flexpm-db/tests/integration_test.rs`:

```rust
#[tokio::test]
async fn test_my_db_feature() {
    let repo = setup_test_db().await;   // In-memory SQLite
    let ws_id = create_test_workspace(&repo).await;
    let project = repo.create_project(ws_id, CreateProject {
        name: "Test".into(),
        ..Default::default()
    }).await.unwrap();
    assert_eq!(project.name, "Test");
}
```

Each test gets its own isolated in-memory database — no cleanup needed.

---

## Good First Contributions

If you're looking for a focused starting point, these areas are self-contained and well-defined:

| Area | What to do | Files to touch |
| --- | --- | --- |
| New project-type preset | Add a workflow + vocabulary pair for a new domain (e.g. `education`, `events`, `research`) | `workflow.rs`, `vocabulary.rs`, `models.rs` |
| New custom field type | Add a new type variant with validation logic | `flexpm-core/src/models.rs` (CustomFieldType + validate_value) |
| Vocabulary translation | Add a non-English vocabulary pack for an existing project type | `flexpm-core/src/vocabulary.rs` |
| CLI output polish | Improve table formatting or add a `--format table\|csv\|json` flag to a command | `flexpm-cli/src/main.rs` |
| Frontend view polish | Fix a visual edge case, improve empty-state UX, or add keyboard shortcuts | `frontend/src/pages/` or `frontend/src/components/` |
| Test coverage | Add handler tests for an endpoint that only has a smoke test | `crates/flexpm-api/tests/api_test.rs` |

The crate layering rule is the main constraint: keep `flexpm-core` free of I/O and `flexpm-cli` free of direct DB access (all data goes through the HTTP API). See the Dependency Flow section above.

---

## How To Add a New Feature

### Adding a New Entity (e.g., "TimeEntry")

1. **Define the model** in `crates/flexpm-core/src/models.rs`
2. **Add a migration** in `crates/flexpm-db/src/migrations.rs` and add it to `run_all()`
3. **Add a repository module** at `crates/flexpm-db/src/repo/time_entries.rs`; add `pub mod time_entries;` to `repo.rs`
4. **Add a handler module** at `crates/flexpm-api/src/handlers/time_entries.rs`; add it to the `use crate::handlers::{...}` import in `router.rs`
5. **Add routes** in `crates/flexpm-api/src/router.rs`
6. **Write tests** in `crates/flexpm-api/tests/api_test.rs`

### Adding a New Workflow Preset

Edit `crates/flexpm-core/src/workflow.rs`, add a new `pub fn my_workflow() -> WorkflowConfig` function and wire it into `workflow_for_type()`.

### Adding a New Vocabulary Pack

Edit `crates/flexpm-core/src/vocabulary.rs`, add a new match arm in `vocabulary_for_type()`.

---

## Database

### Schema

SQLite with WAL mode enabled. Tables:

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
rm flexpm.db flexpm.db-shm flexpm.db-wal
cargo run --bin flexpm-api   # migrations re-run automatically
```

### Inspecting the Database

```bash
sqlite3 flexpm.db
.tables
SELECT * FROM _migrations;
PRAGMA journal_mode;   -- should show "wal"
```

---

## Error Handling

- **`flexpm-core`** — `CoreError` (thiserror)
- **`flexpm-db`** — `sqlx::Error` + `DependencyError`
- **`flexpm-api`** — `ApiError` maps all errors to HTTP status codes:

| Domain Error | HTTP Status |
| --- | --- |
| `ItemNotFound`, `ProjectNotFound` | 404 |
| `InvalidTransition`, `WipLimitExceeded` | 400 |
| `DependencyCycle` | 400 |
| Validation error (validator crate) | 422 |
| `sqlx::Error` (internal) | 500 |

Internal errors log the full cause but return only "Internal server error" to the client.

---

## Logging

All logging uses the `tracing` crate with structured spans.

- **Handlers** — `#[instrument(skip(state))]` auto-creates spans
- **Repository methods** — same instrumentation, logs at `debug` level
- **HTTP middleware** — `TraceLayer` logs every request with method, URI, and duration

```bash
RUST_LOG=error cargo run --bin flexpm-api                # errors only
RUST_LOG=flexpm_db=debug cargo run --bin flexpm-api      # debug the DB layer
RUST_LOG=trace cargo run --bin flexpm-api                # everything (very verbose)
```

---

## Code Style

- Run `cargo fmt --all` before committing (the pre-push hook will catch it anyway)
- Fix all `cargo clippy --workspace -- -D warnings` before pushing
- Keep `flexpm-core` free of I/O dependencies
- Use `#[instrument]` on public async functions for tracing
- Prefer returning `Result` over panicking
- Write tests for any new business logic
- No AI attribution lines in commit messages
