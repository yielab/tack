# Crate Tour

This chapter walks through each of the four Rust crates in depth: what it owns, what it deliberately does not own, the key files, and the patterns worth understanding.

> The SolidJS web UI in `frontend/` is covered separately in
> [Frontend & Design System](frontend.md) — structure, the design-token system,
> and the `shared/ui` component kit.

---

## `tack-core`

**Lives in:** `crates/tack-core/src/`

**Owns:** domain models, workflow engine, vocabulary system, dependency DAG, typed error enum.

**Does not own:** anything that performs I/O. No `sqlx`, no `reqwest`, no file operations, no `tokio`. This is enforced by the `Cargo.toml` — the crate has no async runtime dependency at all.

---

### `models.rs`

The single source of truth for every domain struct. Notable types:

- `Item` — the universal work unit. Fields include `item_type: ItemType`, `status: String`, `parent_id: Option<Uuid>`, `tags: Vec<String>`. The `status` field is a plain string rather than an enum because valid statuses are project-specific configuration, not compile-time constants.
- `Project` — carries the `workflow: WorkflowConfig` and `vocabulary: VocabularyMap` inline. Both are serialised to JSON when stored in SQLite.
- `ItemType` — an enum with `Epic`, `Feature`, `Task`, `Subtask`, `Bug`, `Requirement`, and `Custom(String)`. The `Custom` variant allows ad-hoc item types without a code change.
- `CreateItem`, `UpdateItem`, `CreateProject`, `UpdateProject`, etc. — all DTOs used for both API deserialization and repository function parameters. Keeping them in `tack-core` means the API and CLI reference the same validated shapes.

Validation constraints (length, range) are expressed via the `validator` crate's derive macros directly on the DTO structs. The API handlers call `.validate()?` before doing anything with the data.

---

### `workflow.rs`

Defines `WorkflowConfig`, which is what gets stored as JSON per project.

```rust
pub struct WorkflowConfig {
    pub workflow_type: WorkflowType,
    pub statuses: Vec<StatusDef>,
    pub transitions: Option<Vec<Transition>>,
}
```

Each `StatusDef` has a `name`, a `category` (`Todo`, `InProgress`, or `Done`), an optional `wip_limit`, and an `order` integer for display sorting.

**`validate_transition(from, to)`** is the central enforcement function. It:
1. Checks that both names exist in the status list.
2. If `transitions` is `Some(list)`, checks that the pair appears in the list.
3. Returns `Ok(())` or `Err(CoreError::InvalidTransition { from, to })`.

If `transitions` is `None`, any move between two known statuses is allowed. This is the default for Scrum and Kanban workflows.

**`check_wip_limit(status, current_count)`** looks up the `StatusDef` for the target column and returns `Err(CoreError::WipLimitExceeded { ... })` if `current_count >= limit`.

**Preset functions** produce ready-made configs for each domain:

| Function | Type | Statuses | Transitions |
|---|---|---|---|
| `scrum_workflow()` | `Scrum` | Backlog, To Do, In Progress (WIP 5), In Review (WIP 3), Done | None (open) |
| `kanban_workflow()` | `Kanban` | Queue, In Progress (WIP 3), Review (WIP 2), Done | None (open) |
| `simple_workflow()` | `Simple` | To Do, Doing, Done | None (open) |
| `construction_workflow()` | `Construction` | Permit, Procurement, Build, Inspect, Handover | Explicit linear list |

`workflow_for_type(project_type)` maps each `ProjectType` to the right preset. Adding a new project type means adding a variant to the `ProjectType` enum, a preset function, and a match arm here.

The test suite in this file (29 tests) covers initial status selection, transition validation for open and constrained workflows, WIP limit edge cases, done-status detection, and parent-completion logic — all without any database or async runtime.

---

### `vocabulary.rs`

`VocabularyMap` is a type alias for `HashMap<String, String>`. It maps canonical keys (like `"task"`, `"sprint"`, `"epic"`) to their display labels for a given project.

**`resolve(vocab, key)`** looks up a key in the project's vocabulary, falls back to the default vocabulary, and finally falls back to the key itself. This means partial vocabularies work fine — a construction project only needs to override the terms it cares about.

**`vocabulary_for_type(project_type)`** provides preset vocabularies. The construction preset, for example, maps:

- `"task"` → `"Work Order"`
- `"sprint"` → `"Phase"`
- `"epic"` → `"Building"`
- `"bug"` → `"Defect"`

**`validate(vocab)`** ensures that all keys in a user-supplied vocabulary are from the recognised list (`VOCABULARY_KEYS`). Unknown keys return `Err(CoreError::InvalidVocabularyKey(...))` — this prevents typos from silently creating orphaned entries.

---

### `dependency.rs`

`DependencyGraph` is an adjacency-list representation of item dependencies:

```rust
pub struct DependencyGraph {
    edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,   // item → items it blocks
    reverse_edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,  // item → items that block it
}
```

`DependencyGraph::from_edges(edges)` builds the graph from a slice of `DependencyEdge` values. The API handler loads all existing dependencies for the involved items, builds the graph, then calls `validate_new_edge(source, target)` before inserting.

**`would_create_cycle(source, target)`** runs a depth-first search starting from `target`, following the `edges` adjacency list. If it ever reaches `source`, adding `source → target` would close a cycle and the function returns `true`. The check is O(V + E) over the existing graph.

`validate_new_edge` wraps the check in a `Result`, also catching the self-reference case (`source == target`).

---

### `error.rs`

`CoreError` is a `thiserror`-derived enum that covers every domain-level failure:

- `ItemNotFound(Uuid)`, `ProjectNotFound(Uuid)`, `SprintNotFound(Uuid)`, `RoleNotFound(Uuid)` — map to HTTP 404.
- `InvalidTransition { from, to }`, `WipLimitExceeded { column, limit, current }`, `DependencyCycle(Uuid)`, `DuplicateDependency { ... }`, `InvalidVocabularyKey(String)`, `EmptyWorkflow`, `HasChildren(Uuid, usize)`, `Validation(String)` — map to HTTP 400.

The mapping from `CoreError` to HTTP status codes lives in `tack-api/src/error.rs`, keeping the core crate free of HTTP knowledge.

---

## `tack-db`

**Lives in:** `crates/tack-db/src/`

**Owns:** SQLite connection pool initialisation, migration runner, repository pattern over all entities.

**Does not own:** HTTP concerns, config loading, or business rule enforcement. The repository functions are thin: they translate between Rust structs and SQL rows.

---

### `lib.rs`

`init_pool(database_url)` creates a `SqlitePool` using `SqlitePoolOptions` with a max of 5 connections, then immediately runs two `PRAGMA` statements:

- `PRAGMA journal_mode=WAL` — enables Write-Ahead Logging for better concurrent read performance.
- `PRAGMA foreign_keys=ON` — SQLite does not enforce foreign keys by default; this enables them.

---

### `migrations.rs`

Contains 18 migrations as `const` arrays of SQL strings. Each entry is `(&str name, &[&str] statements)`. The runner:

1. Creates `_migrations` table if absent.
2. For each migration, checks if the name is already recorded.
3. Executes each SQL statement in order.
4. Records the migration name on success.

Migrations are idempotent — running them on an existing database is safe. Notable migrations:

- `004_items` — creates the `items` table with indexes on `project_id`, `status`, `priority`, `parent_id`, and `sprint_id`.
- `010_fts` — creates the FTS5 virtual table `items_fts` and three triggers (`after_item_insert`, `after_item_update`, `after_item_delete`) that keep the FTS index in sync with the `items` table.
- `012_custom_fields` — `custom_field_definitions` and `custom_field_values` tables.
- `016_perf_indexes` — additional composite indexes added after profiling.

---

### `repo.rs` and `repo/`

`repo.rs` declares the `Repository` struct, which holds a `SqlitePool`, and re-exports a method for each database operation by delegating to the appropriate submodule:

```rust
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub async fn create_item(&self, ...) -> Result<Item, sqlx::Error> {
        items::create_item(self.pool(), ...).await
    }
    // ...
}
```

This design gives callers a single `repo` value to pass around while keeping each entity's SQL in its own file.

**Per-entity submodules** (`items.rs`, `projects.rs`, `sprints.rs`, `roles.rs`, `comments.rs`, `dependencies.rs`, `attachments.rs`, `boards.rs`, `templates.rs`, `custom_fields.rs`):

- Functions take `&SqlitePool` (or `&self` for the struct-based submodules) and return `Result<T, sqlx::Error>` or `Result<T, DependencyError>`.
- Queries use `sqlx::query` / `sqlx::query_as` with positional `?` parameters.
- UUIDs are stored as `TEXT` — bound as `.bind(id.to_string())` and parsed back from the row.
- JSON fields (`workflow`, `vocabulary`, `tags`) are serialised to/from strings with `serde_json`.
- Timestamps are stored as RFC 3339 strings and parsed via `chrono::DateTime<Utc>`.

**Notable function: `check_and_update_parent_status`** (in `items.rs`). After an item is moved to a `Done`-category status, the handler calls this function with the item's `parent_id`. It queries whether all sibling items are also in a done status, and if so, updates the parent. The `WorkflowConfig::should_complete_parent(all_siblings_done)` call in `tack-core` provides the decision logic — the repository only handles the data queries.

---

## `tack-api`

**Lives in:** `crates/tack-api/src/`

**Owns:** HTTP server startup, route registration, request/response handling, configuration, WebSocket management, error mapping.

**Does not own:** SQL queries (those are in `tack-db`) or business rules (those are in `tack-core`). Handlers orchestrate calls to both.

This crate is a **library only** — it does not produce its own binary. The single
`tack` binary (in `tack-cli`) calls `tack_api::serve()` to start the server.

---

### `server.rs`

Exposes `pub async fn serve()`, the server entry point. It does these things in order:

1. Loads `AppConfig` (TOML file or environment variables).
2. Initialises the `tracing` subscriber (plain text or JSON depending on config).
3. Applies any staged database restore (rename `.restore` file into place).
4. Calls `init_pool()` and `migrations::run_all()`.
5. Ensures a default workspace row exists (creates one if the table is empty).
6. Builds `AppState`, calls `build_router(state)`, and starts `axum::serve` with graceful shutdown on `CTRL+C`.

`tack-cli` builds a Tokio runtime and calls `serve()` when you run `tack` with no
subcommand (or `tack serve`).

---

### `router.rs`

`AppState` is the shared state cloned into every handler:

```rust
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    pub broadcast_tx: broadcast::Sender<BoardEvent>,
}
```

`build_router(state)` assembles the Axum `Router`. Routes are grouped by entity and nested under `/api`. The file also wires up:

- **CORS** — reads `config.allowed_origins`, constructs a `tower_http::cors::CorsLayer`.
- **Body limit** — `DefaultBodyLimit::max(config.max_body_size_bytes)` globally; the attachment upload route overrides this to 50 MB.
- **Security headers** — `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, `X-Frame-Options: DENY` via `SetResponseHeaderLayer`.
- **Request tracing** — `TraceLayer` logs every request with method and URI.
- **Token gate** — `middleware::from_fn_with_state(state, require_token)` wraps all `/api` routes.
- **embed-spa feature** — when compiled with `--features embed-spa`, a fallback handler serves the bundled SPA.

---

### `handlers/`

One file per entity group. A typical handler follows this shape:

```rust
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateItem>,
) -> ApiResult<Json<Item>> {
    input.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    // load, validate, persist, broadcast
}
```

Handlers return `ApiResult<Json<T>>`, which is `Result<Json<T>, ApiError>`. The `ApiError` type implements `IntoResponse`, so Axum converts errors to JSON automatically.

**`websocket.rs`** is somewhat different from other handler files — see the Architecture Overview for a full walkthrough of the connection lifecycle. The key public API it exposes to other handlers is:

```rust
pub fn broadcast_event(state: &AppState, event: BoardEvent) { ... }
```

Any handler that mutates data calls this after a successful write.

---

### `config.rs`

`AppConfig::load()` tries to read `tack.toml` from the current directory. If that fails, it reads environment variables (`TACK_HOST`, `TACK_PORT`, `TACK_DATABASE_URL`, etc.) over a `Default::default()` base. There is no `figment` or other config framework — the logic is a straightforward chain of `if let Ok(v) = std::env::var(...)` assignments.

The API token is never logged. The only place it appears in logs is a boolean "token configured: true/false" in the startup message.

---

### `error.rs`

`ApiError` is the unified error type for all handlers. It implements `IntoResponse` with this mapping:

| `ApiError` variant | HTTP status |
|---|---|
| `NotFound` | 404 |
| `BadRequest` | 400 |
| `Conflict` | 409 |
| `Core(CoreError::*NotFound*)` | 404 |
| `Core(CoreError::InvalidTransition \| WipLimitExceeded \| …)` | 400 |
| `Database` | 500 |
| `Internal` | 500 |

The response body is always `{ "error": { "status": <code>, "message": "<text>" } }`.

---

## `tack-cli`

**Lives in:** `crates/tack-cli/src/`

**Owns:** the single `tack` binary — both starting the server and the command-line client (parsing, human-readable output, HTTP calls to the API).

**Does not own:** any `tack-core` or `tack-db` types directly. The client commands work entirely through the HTTP API — they serialise to JSON for requests and deserialise from `serde_json::Value` for responses (no strongly-typed response structs). This keeps the CLI decoupled from internal model changes that do not affect the API contract. (To *run* the server it depends on `tack-api` and calls `tack_api::serve()`.)

---

### `main.rs`

Uses `clap`'s derive API. The top-level `Cli` struct has two global flags (`--api-url`, `--token`) and an **optional** `Commands` enum. Commands include `serve`, `init`, `projects`, `add`, `list`, `move`, `board`, `search`, `sprint`, `config`, `completions`, `backup`, and `restore`.

Running `tack` with **no subcommand** — or `tack serve` — starts the server + web UI: `run_server()` builds a Tokio runtime and calls `tack_api::serve()`. This is the primary, UI-first entry point. Everything else is the CLI client.

Three cases (`serve`, `config`, and `completions`) are handled before the `TackClient` is constructed, since they do not need a live connection.

All other commands instantiate a `TackClient`, call the appropriate method, and then either print raw JSON (with `--json`) or format a human-readable table. The table formatter (`print_table_row`) pads and truncates columns to fixed widths, which keeps the output readable in standard terminals.

The `add` and `list` commands fetch the project's vocabulary via `vocab::fetch()` and translate `item_type` strings through it before printing — so a construction project shows `Work Order` rather than `task` in the output.

---

### `client.rs`

`TackClient` wraps `reqwest::blocking::Client`. All methods prepend `/api` to the supplied path and attach the `Authorization: Bearer <token>` header when a token is configured.

Public methods:

- `get(path)` → `serde_json::Value`
- `post(path, body)` → `serde_json::Value`
- `patch(path, body)` → `serde_json::Value`
- `delete(path)` → `()`
- `get_bytes(path)` → `Vec<u8>` — used for backup download
- `post_bytes(path, data)` → `serde_json::Value` — used for restore upload

Error handling: the `extract()` helper parses the response body regardless of status code, then returns the body on success or extracts the `error.message` field and surfaces it as an `anyhow::Error` on failure.

---

### `config.rs`

`Config::load(base_url_override, token_override)` applies a precedence chain:

1. CLI flag value (passed as `Option<String>`)
2. Environment variable (`TACK_API_URL`, `TACK_API_TOKEN`)
3. `~/.tackrc` — a TOML file with `base_url` and optional `token` fields
4. Default: `http://127.0.0.1:3210`

`config::save(base_url, token)` writes `~/.tackrc`. This is what `tack config --url <url>` does.
