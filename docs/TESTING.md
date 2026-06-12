# FlexPM Testing Guide

**Rust tests:** 164 passing + 1 `#[ignore]` perf test (`cargo test --workspace`)
**Frontend tests:** 144 Vitest unit tests (`cd frontend && npm test`)
**With embed-spa feature:** ~167 tests (`cargo test -p flexpm-api --features embed-spa`)

## Quick start

```bash
# Run all Rust tests
cargo test --workspace

# Show println! output
cargo test --workspace -- --nocapture

# Single test by name
cargo test test_workflow_transition_validation

# One crate
cargo test -p flexpm-core
cargo test -p flexpm-db
cargo test -p flexpm-api
cargo test -p flexpm-cli

# Frontend
cd frontend && npm test
```

Integration tests use in-memory SQLite — no external services needed.

---

## Test breakdown

| Crate | Count | Type |
| --- | --- | --- |
| `flexpm-core` | 67 | Unit tests (`#[cfg(test)]` in source files) |
| `flexpm-db` | 22 | Integration tests in `tests/integration_test.rs` |
| `flexpm-db` | 1 | Performance test (`#[ignore]`, seeds 50k items) |
| `flexpm-api` | 36 | Handler integration tests in `tests/api_test.rs` |
| `flexpm-api` | 17 | Alexa endpoint tests in `tests/alexa_test.rs` |
| `flexpm-api` | 11 | Unit tests (middleware, GitHub URL parsing) |
| `flexpm-cli` | 11 | CLI tests (wiremock + unit) |
| **Rust total** | **164** | |
| Frontend | 144 | Vitest unit tests across 21 test files |

---

## Core unit tests (`flexpm-core`)

Business logic lives in `flexpm-core` with no I/O. Tests go in the same file inside `#[cfg(test)]`.

```bash
cargo test -p flexpm-core   # 67 tests
```

Key test areas:

- Workflow transition validation across all presets (Scrum, Kanban, construction)
- WIP limit enforcement (at/below/over limit)
- Parent auto-complete (all siblings done → parent auto-completes)
- Dependency cycle detection
- Vocabulary term resolution
- Custom field value validation — 28 tests covering all 9 field types, option lists, pattern/min/max rules

Example:

```rust
#[test]
fn construction_workflow_rejects_skip() {
    let wf = construction_workflow();
    assert!(wf.validate_transition("Permit", "Handover").is_err());
}
```

---

## DB integration tests (`flexpm-db`)

Located in `crates/flexpm-db/tests/integration_test.rs`. Each test gets a fresh
in-memory SQLite database with all migrations applied via `setup_test_db()`.

```bash
cargo test -p flexpm-db   # 22 tests + 1 ignored
```

Covers: project CRUD, item hierarchy, sprint lifecycle, custom fields, boards,
templates, FTS5 search, dependency graph, assignee filtering, role assignment.

### Performance test (ignored by default)

Seeds 50k items in a single transaction and asserts `list_items` p95 < 100 ms:

```bash
cargo test -p flexpm-db list_items_p95 -- --ignored
```

### Test helpers (`tests/common/mod.rs`)

```rust
setup_test_db()              // fresh in-memory pool, all migrations applied
create_test_workspace(&repo) // inserts a workspace, returns its UUID
make_project(&repo, ws_id)   // creates a software project
make_item(&repo, project_id) // creates an item in that project
```

---

## API handler tests (`flexpm-api`)

All tests in `crates/flexpm-api/tests/`. Uses `axum::Router::oneshot()` to fire
requests without binding a real port.

```bash
cargo test -p flexpm-api   # 64 tests
```

### API test helpers (`tests/common/mod.rs`)

```rust
test_app()                        // in-memory DB, default config, wired router
test_app_with_config(config)      // same with custom AppConfig
test_app_with_file_db(db_url)     // file-based DB (needed for backup/restore tests)
```

### Handler integration tests (`tests/api_test.rs`) — 36 tests

- Health endpoint shape (`status`, `version`, `migrations_applied`)
- API token auth: no token, correct token, wrong token
- Body size limit (413 for oversized requests)
- Input validation (empty name/title → 422)
- Project CRUD and vocabulary persistence
- Item create/update/delete
- Sprint lifecycle (create → start → close)
- Role assignment and removal
- Comment create and list
- Dependency create/list/delete and cycle rejection
- JSON and CSV export
- Custom field create, set, type validation (422 on wrong type)
- Board create/update/view with item-type filter
- FTS5 search (per-project and global)
- Backup: in-memory DB returns 400, invalid magic bytes return 400, full roundtrip
- Embedded SPA (with `--features embed-spa`): root → HTML, API takes priority

### Alexa endpoint tests (`tests/alexa_test.rs`) — 17 tests

- 404 when `FLEXPM_ALEXA_SKILL_ID` not configured
- Wrong skill ID rejected (403)
- Timestamp tolerance enforcement
- LaunchRequest welcome message (EN and ES)
- AddTaskIntent: missing slot prompt, successful add
- ListTasksIntent: empty project and populated project
- CompleteTaskIntent: not-found, WIP-limit rejection, successful completion
- Locale detection: `es-MX` → Spanish, `en-US` → English

### Unit tests — 11 tests

- Bearer token middleware (no token / correct / wrong / health bypass)
- GitHub URL parsing: `owner/repo`, full HTTPS URL, SSH URL, trailing `.git`, invalid inputs

### Running with embed-spa

```bash
cd frontend && npm run build && cd ..
cargo test -p flexpm-api --features embed-spa
```

---

## CLI tests (`flexpm-cli`)

Located in `crates/flexpm-cli/tests/cli_test.rs`. Uses `wiremock` to stub the API.

```bash
cargo test -p flexpm-cli   # 11 tests
```

Covers: `init`, `add`, `list`, `move`, sprint commands, global search, config save/load,
vocab fetch (including graceful 404 fallback), Bearer token forwarding.

---

## Frontend tests

```bash
cd frontend && npm test   # 144 tests across 21 files
```

Covers:

- API client contracts (`api.ts`) — request shaping, error propagation
- `deriveBoard` pure function — column derivation, WIP limit display, item ordering
- `ProjectItemsContext` — fetch lifecycle, optimistic updates, rollback on error
- Vocabulary resolver (`vocab.ts`) — term resolution, per-project overrides, defaults
- Settings panels — vocabulary editor, workflow editor
- CSV import UI — file selection, column mapping, error handling
- Keyboard manager — shortcut registration, conflict detection
- Lens/view persistence — last-view state across navigation
- Optimistic update rollback — revert on server error

---

## Continuous integration

The CI pipeline (`.github/workflows/ci.yml`) runs three jobs on every push to `develop`:

### `rust` job

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

### `frontend` job

```bash
npm ci
npm run type-check
npm run build
# Gate: entry bundle (index + routing chunks) < 30 KB gzipped
```

### `embed-spa` job

Runs after `frontend` finishes. Downloads the built dist artifact, then:

```bash
cargo clippy -p flexpm-api --features embed-spa -- -D warnings
cargo test -p flexpm-api --features embed-spa
cargo build -p flexpm-api --release --features embed-spa
# Reports binary size (~5 MB)
```

### Pre-push hook (local)

To catch CI failures before they reach GitHub, activate the pre-push hook:

```bash
git config core.hooksPath .githooks
```

This runs `cargo fmt --all --check` and `cargo clippy --workspace -- -D warnings`
before every `git push`, blocking the push if either check fails.

---

## Coverage

Install `cargo-llvm-cov` (requires LLVM toolchain):

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --html --output-dir coverage/
open coverage/index.html
```

Targets: `flexpm-core` ≥ 85% lines, `flexpm-db` + `flexpm-api` ≥ 70% combined.

---

## Manual smoke test

With the server running (`cargo run --bin flexpm-api`):

```bash
BASE=http://localhost:3210/api

# Health
curl -s $BASE/health | jq

# Create → add → move
PID=$(curl -s -X POST $BASE/projects \
  -H "Content-Type: application/json" \
  -d '{"name":"Smoke","project_type":"software"}' | jq -r '.id')

IID=$(curl -s -X POST $BASE/projects/$PID/items \
  -H "Content-Type: application/json" \
  -d '{"title":"Test task","item_type":"task"}' | jq -r '.id')

curl -s -X PATCH $BASE/items/$IID \
  -H "Content-Type: application/json" \
  -d '{"status":"In Progress"}' | jq .status

# WebSocket (requires websocat)
websocat "ws://localhost:3210/api/projects/$PID/boards/live"

# Search
curl -s "$BASE/projects/$PID/search?q=test" | jq

# GitHub import (requires a valid token for private repos)
curl -s -X POST $BASE/projects/$PID/import-github \
  -H "Content-Type: application/json" \
  -d '{"repo":"owner/repo","label_filter":["bug"]}' | jq

# Backup
curl -s $BASE/backup -o smoke-backup.db
file smoke-backup.db   # should say "SQLite 3.x database"

# Cleanup
curl -s -X DELETE $BASE/projects/$PID
```
