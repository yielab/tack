# Tack Testing Guide

**Rust tests:** 164 passing + 1 `#[ignore]` perf test (`cargo test --workspace`)
**Frontend tests:** 144 Vitest unit tests (`cd frontend && npm test`)
**With embed-spa feature:** ~167 tests (`cargo test -p tack-api --features embed-spa`)

## Quick start

```bash
# Run all Rust tests
cargo test --workspace

# Show println! output
cargo test --workspace -- --nocapture

# Single test by name
cargo test test_workflow_transition_validation

# One crate
cargo test -p tack-core
cargo test -p tack-db
cargo test -p tack-api
cargo test -p tack-cli

# Frontend
cd frontend && npm test
```

Integration tests use in-memory SQLite — no external services needed.

---

## Test breakdown

| Crate | Count | Type |
| --- | --- | --- |
| `tack-core` | 67 | Unit tests (`#[cfg(test)]` in source files) |
| `tack-db` | 22 | Integration tests in `tests/integration_test.rs` |
| `tack-db` | 1 | Performance test (`#[ignore]`, seeds 50k items) |
| `tack-api` | 36 | Handler integration tests in `tests/api_test.rs` |
| `tack-api` | 17 | Alexa endpoint tests in `tests/alexa_test.rs` |
| `tack-api` | 11 | Unit tests (middleware, GitHub URL parsing) |
| `tack-cli` | 11 | CLI tests (wiremock + unit) |
| **Rust total** | **164** | |
| Frontend | 144 | Vitest unit tests across 21 test files |

---

## Core unit tests (`tack-core`)

Business logic lives in `tack-core` with no I/O. Tests go in the same file inside `#[cfg(test)]`.

```bash
cargo test -p tack-core   # 67 tests
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

## DB integration tests (`tack-db`)

Located in `crates/tack-db/tests/integration_test.rs`. Each test gets a fresh
in-memory SQLite database with all migrations applied via `setup_test_db()`.

```bash
cargo test -p tack-db   # 22 tests + 1 ignored
```

Covers: project CRUD, item hierarchy, sprint lifecycle, custom fields, boards,
templates, FTS5 search, dependency graph, assignee filtering, role assignment.

### Performance test (ignored by default)

Seeds 50k items in a single transaction and asserts `list_items` p95 < 100 ms:

```bash
cargo test -p tack-db list_items_p95 -- --ignored
```

### Test helpers (`tests/common/mod.rs`)

```rust
setup_test_db()              // fresh in-memory pool, all migrations applied
create_test_workspace(&repo) // inserts a workspace, returns its UUID
make_project(&repo, ws_id)   // creates a software project
make_item(&repo, project_id) // creates an item in that project
```

---

## API handler tests (`tack-api`)

All tests in `crates/tack-api/tests/`. Uses `axum::Router::oneshot()` to fire
requests without binding a real port.

```bash
cargo test -p tack-api   # 64 tests
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

- 404 when `TACK_ALEXA_SKILL_ID` not configured
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
cargo test -p tack-api --features embed-spa
```

---

## CLI tests (`tack-cli`)

Located in `crates/tack-cli/tests/cli_test.rs`. Uses `wiremock` to stub the API.

```bash
cargo test -p tack-cli   # 11 tests
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
cargo clippy -p tack-api --features embed-spa -- -D warnings
cargo test -p tack-api --features embed-spa
cargo build -p tack-api --release --features embed-spa
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

Targets: `tack-core` ≥ 85% lines, `tack-db` + `tack-api` ≥ 70% combined.

---

## Manual smoke test

With the server running (`cargo run --bin tack-api`):

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

---

## End-to-end, accessibility & API-contract tests (Playwright)

Browser-level tests that drive the **real** app — the `tack-api` server plus
the Vite-served SPA — in Chromium, Firefox and WebKit. Playwright owns both
server lifecycles, so a single command is all that's needed; the API runs
against a throwaway `e2e.db` so your working database is never touched.

```bash
make e2e-install     # one-time: download the browser engines
make e2e             # run the whole suite (chromium + firefox + webkit)
make e2e-ui          # interactive runner for debugging
```

Layout (`frontend/e2e/`):

| File | Covers |
| --- | --- |
| `smoke.spec.ts` | Every primary surface renders without a blank screen or page error — **all 3 browsers** |
| `journey.spec.ts` | A created item flows to the board and opens with the correct title (regression guard for the two QA bugs) |
| `a11y.spec.ts` | WCAG 2.0/2.1 A & AA scans via axe-core (chromium) — new violations fail CI |
| `api.spec.ts` | Wire-contract checks: health shape, hardening headers, response envelopes, 404s |
| `helpers.ts` | Single source of truth for API response shapes (`getOrCreateProject`, etc.) |

Config: `frontend/playwright.config.ts`. Cross-browser coverage is the `projects`
list; engine-independent specs (`a11y`, `api`) self-skip to chromium only.

**Triaging existing a11y debt:** add the axe rule id to `KNOWN_ISSUES` in
`a11y.spec.ts` with a tracking note instead of deleting the assertion, so the
gate keeps blocking *new* regressions.

---

## Dependency vulnerability scanning

```bash
make audit           # cargo audit (Rust) + npm audit --audit-level=high (frontend)
```

Runs in CI as the **security** job (`cargo-audit` via the RustSec advisory DB +
`npm audit`). [Dependabot](../.github/dependabot.yml) opens weekly grouped
update PRs for cargo, npm and GitHub Actions.

Known, justified Rust advisory exceptions live in
[`.cargo/audit.toml`](../.cargo/audit.toml) with a documented reason each — the
gate still fails on any **new** advisory. Re-review that list on every dep bump.

> **Known a11y debt:** `color-contrast` (palette-wide, needs a design-token
> contrast pass) and `select-name` (board project selector lacks an `aria-label`)
> are recorded in `e2e/a11y.spec.ts` `KNOWN_ISSUES`. They keep the suite green
> while still blocking *new* a11y regressions — fix and remove them when able.

---

## Load / performance testing (k6)

HTTP-level load test establishing the performance baseline. Not part of default
CI (needs a running server, time-consuming) — run on demand.

```bash
# terminal 1: a server with a throwaway DB
TACK_DATABASE_URL='sqlite:load.db?mode=rwc' cargo run -p tack-api --release
# terminal 2:
make load
```

Ramps to 50 VUs on the read hot path + a write path, asserting p95 latency and
error-rate thresholds. The write p95 threshold is where SQLite's single-writer
model shows up first. See [`tests/load/README.md`](../tests/load/README.md).

---

## CI gates (`.github/workflows/ci.yml`)

| Job | Gate |
| --- | --- |
| `rust` | fmt + clippy + `cargo test --workspace` |
| `frontend` | type-check + token lint + build + bundle-size budget |
| `docs` | mdBook build + broken-link check |
| `embed-spa` | single-binary packaging + release build |
| `security` | cargo audit + npm audit (high+) |
| `e2e` | Playwright across chromium/firefox/webkit + a11y + API contract |
