# FlexPM Testing Guide

**Total tests:** 92 passing + 1 `#[ignore]` perf test (`cargo test --workspace`)
**With embed-spa feature:** 95 tests (`cargo test -p flexpm-api --features embed-spa`)

## Quick start

```bash
# Run everything
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
```

Integration tests use in-memory SQLite — no external services needed.

---

## Test breakdown

| Crate | Count | Type |
| --- | --- | --- |
| `flexpm-core` | 39 | Unit tests (`#[cfg(test)]` in source files) |
| `flexpm-db` | 22 | Integration tests in `tests/integration_test.rs` |
| `flexpm-db` | 1 | Performance test (`#[ignore]`, seeds 50k items) |
| `flexpm-api` | 16 | Handler tests in `tests/api_test.rs` (19 with `--features embed-spa`) |
| `flexpm-api` | 4 | Middleware unit tests in `src/middleware.rs` |
| `flexpm-cli` | 11 | CLI tests (wiremock + unit) |

---

## Core unit tests (`flexpm-core`)

Business logic lives in `flexpm-core` with no I/O. Tests go in the same file inside `#[cfg(test)]`.

```bash
cargo test -p flexpm-core
```

Key test areas:

- Workflow transition validation across all presets (Scrum, Kanban, construction)
- WIP limit enforcement (at/below/over limit)
- Parent auto-complete (all siblings done → parent auto-completes)
- Dependency cycle detection
- Vocabulary term resolution

Example:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_workflow_rejects_skip() {
        let wf = construction_workflow();
        assert!(wf.validate_transition("Permit", "Handover").is_err());
    }
}
```

---

## DB integration tests (`flexpm-db`)

Located in `crates/flexpm-db/tests/integration_test.rs`. Each test gets a fresh
in-memory SQLite database with all migrations applied via `setup_test_db()`.

```bash
cargo test -p flexpm-db
```

Covers: project CRUD, item hierarchy, sprint lifecycle, custom fields, boards,
templates, FTS5 search, dependency graph, assignee filtering, role assignment.

### Performance test (ignored by default)

Seeds 50k items in a single transaction and asserts list_items p95 < 100 ms:

```bash
cargo test -p flexpm-db list_items_p95 -- --ignored
```

### Test helpers (`tests/common/mod.rs`)

```rust
setup_test_db()             // fresh in-memory pool, all migrations applied
create_test_workspace(&repo) // inserts a workspace, returns its UUID
make_project(&repo, ws_id)  // creates a software project
make_item(&repo, project_id) // creates an item in that project
```

---

## API handler tests (`flexpm-api`)

Located in `crates/flexpm-api/tests/api_test.rs`. Uses `axum::Router::oneshot()`
to fire requests without binding a real port.

```bash
cargo test -p flexpm-api
```

### API test helpers (`tests/common/mod.rs`)

```rust
test_app()                        // in-memory DB, default config, wired router
test_app_with_config(config)      // same with custom AppConfig
test_app_with_file_db(db_url)     // file-based DB (needed for backup/restore tests)
```

### What's covered

- Health endpoint shape (`status`, `version`, `migrations_applied`)
- API token auth: no token, correct token, wrong token, missing token
- Health bypasses token check
- Body size limit (413 for oversized requests)
- Input validation (empty name/title → 400)
- Vocabulary persistence via `PATCH /api/projects/{id}`
- Workflow status validation
- Backup: in-memory DB returns 400, invalid magic bytes return 400, full roundtrip
- Embedded SPA (with `--features embed-spa`): root → HTML, unknown route → index.html, API takes priority

### Running with embed-spa

```bash
# Requires frontend/dist/ to exist
cd frontend && npm run build && cd ..
cargo test -p flexpm-api --features embed-spa
```

---

## CLI tests (`flexpm-cli`)

Located in `crates/flexpm-cli/tests/cli_test.rs`. Uses `wiremock` to stub the API.

```bash
cargo test -p flexpm-cli
```

Covers: `init`, `add`, `list`, `move`, sprint commands, global search, config save/load,
vocab fetch (including graceful 404 fallback), Bearer token forwarding.

---

## Continuous integration

The CI pipeline (`.github/workflows/ci.yml`) runs three jobs on every push:

### `rust` job

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
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
# Reports binary size
```

---

## Coverage

Install `cargo-llvm-cov` (requires LLVM toolchain):

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --html --output-dir coverage/
open coverage/index.html
```

Targets from the roadmap: `flexpm-core` ≥ 85% lines, `flexpm-db` + `flexpm-api` ≥ 70% combined.

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

# Backup
curl -s $BASE/backup -o smoke-backup.db
file smoke-backup.db   # should say "SQLite 3.x database"

# Cleanup
curl -s -X DELETE $BASE/projects/$PID
```
