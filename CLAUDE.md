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

**Current Status:** Core complete; the active cycle is the **Harness-Agnostic Runner Fleet**
(Part III, Phases 50–57). Phases 50–53 are delivered; Phase 54 is next. `TODO.md`'s Part III
board is the authority for wave status and accepted integration SHAs — `docs/book/src/roadmap.md`
records the architectural intent, the board records what shipped.

- Backend: complete (REST endpoints + WebSocket, 61 migrations, custom field value validation, Alexa voice integration)
- Frontend: complete (Board, List, Table, Timeline, Sprints, Calendar, Settings views; teal multi-palette design system — Teal/Clay/Graphite × light/dark; 482 Vitest unit tests + Playwright E2E)
- CLI: complete (init, add, list, move, board, branch, search, sprint, template, role, comment, field, backup, restore, mcp)
- AI agents: `tack mcp` — Model Context Protocol server over stdio (8 tools); see `docs/MCP.md`
- Project types: 10 presets (software, web, mobile, construction, personal, homework, maintenance, legal, research, event, custom)

## Development Commands

### Building and Running

```bash
# Check the whole workspace compiles
cargo build

# Run the server + web UI (http://127.0.0.1:3210) — bare `tack` defaults to serve
cargo run -p tack-cli -- serve

# Run the CLI tool (same binary)
cargo run --bin tack -- --help

# Run the execution runner (a SEPARATE binary from `tack`)
cargo run -p tack-runner -- --help

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
cargo test -p tack-orch
cargo test -p tack-runner

# Wave gates — these prove things no single card's tests do
cargo test -p tack-api --test wave2_gate        # full runner lifecycle through the real router
cargo test -p tack-orch --test runner_contract  # byte-pins every runner-v1 fixture

# Opt-in live harness tests (ignored by default, skipped when the binary is absent).
# Claude Code's is billed — run it deliberately.
cargo test -p tack-runner -- --ignored

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
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope). When set, item status changes are pushed back to linked GitHub issues (Phase 21, push-only: item done ⇄ issue closed). Never logged. See `docs/GITHUB-SYNC.md` |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root — override for GitHub Enterprise or to point tests at a mock. Used by both import and push-back |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |
| `TACK_ORCH_ENABLE` | `false` | Enables the orchestration reconciler and the `/api/control-planes`, `/api/projects/{id}/orch-link`, `/api/fleet` routes (and their later-wave successors). Unset ⇒ no reconciler task spawned, every orch route 404s |
| `TACK_ORCH_POLL_SECS` | `10` | Reconciler base poll interval in seconds (before per-plane backoff + jitter) |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `90` | Days of `orch_events` (and, once ingested, `orch_metrics`) history kept before the retention sweep rolls old rows into per-day aggregates and deletes them |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Separate shared secret required to grant/deny a docket approval via `POST /api/approvals/{token}` (Wave 4). Deliberately distinct from `TACK_API_TOKEN` — granting an approval is higher-privilege than editing a card. Never logged |
| `TACK_EXECUTION_RETENTION_ENABLE` | `false` | Enables the execution-domain retention sweep (replay/idempotency bookkeeping + terminal `execution_events` purge, Wave 5 card III-F5). **Off by default** (Wave 5 integrator III-F6 amendment — F5 originally shipped this `true`; see `crates/tack-api/src/config.rs#default_execution_retention_enable`) — this sweep deletes rows, so data deletion must be an explicit operator opt-in, matching `TACK_ORCH_ENABLE`'s own off-by-default posture |
| `TACK_EXECUTION_RETENTION_DAYS` | `90` | Days of replay/idempotency bookkeeping and terminal `execution_events` history kept before the sweep purges them |
| `TACK_EXECUTION_RETENTION_INTERVAL_SECS` | `3600` | Interval, in seconds, between execution-retention sweeps |
| `TACK_EXECUTION_HEALTH_ENABLE` | `true` | Enables the execution-domain health watch (runner/queue/lease/event counts; logs a `warn!` on stale-lease/`needs_operator` onset, Wave 5 card III-F5). On by default, unlike retention above — this reads and logs only, deletes nothing |
| `TACK_EXECUTION_HEALTH_INTERVAL_SECS` | `60` | Interval, in seconds, between execution health-watch checks |
| `TACK_EXECUTION_DECISION_TOKEN` | _(none)_ | Separate shared secret required to resolve a scoped execution decision via `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve` (Wave 5 card III-F1, wired by integrator III-F6). Mirrors `TACK_ORCH_APPROVAL_TOKEN` exactly: distinct from `TACK_API_TOKEN`, **fail-closed when unset** (the route rejects rather than falling back to the operator token). Never logged |

The `tack-runner` binary is configured separately (defaults → `TOML` → environment → CLI flags,
in that order):

| Variable | Description |
|----------|-------------|
| `TACK_RUNNER_API_URL` | Tack API base URL the runner polls |
| `TACK_RUNNER_ENROLLMENT_TOKEN` | One-time operator-issued token; exchanged for a durable credential and never persisted |
| `TACK_RUNNER_ID` | Runner identity once enrolled |
| `TACK_RUNNER_STATE_DIR` | Owner-only directory for the journal and credential |

Runner credentials are redacted in every log, `Debug` impl and error — the redaction is
structural (`RunnerCredential`'s `Debug`/`Display` are hardcoded to `[REDACTED]`), not
convention.

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
├── tack-orch/     Agent-fleet orchestration client (ControlPlane trait, reconciler)
│                  + the neutral runner-v1 execution domain (execution/)
├── tack-api/      Axum HTTP server + WebSocket (library; pub fn serve)
├── tack-runner/   Pull-based execution runner — its own binary; owns local
│                  credentials, workspace, journal and the harness subprocess
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
- 18 migrations with FTS5 full-text search on items
- Repository pattern: CRUD for all entities in `repo/` submodules
- Auto-runs migrations on startup
- Database is created automatically if missing

**tack-orch** (agent-fleet orchestration client — off by default, gated behind `TACK_ORCH_ENABLE`):
- Defines the `ControlPlane` trait (`health`, `status`, `metrics`, `list_runs`, `list_approvals`, `list_tasks`, `traces`, plus write methods gated behind Phase 35 dispatch) — the seam that makes Tack a factory control center rather than a docket-specific dashboard. `docket` (`adapters::docket::DocketAdapter`) is the only implementor today.
- Depends only on `tack-core` and `tack-db`; must never depend on `tack-api` — the dependency points inward, `tack-api` depends on this crate to spawn the reconciler and expose the orchestration routes
- `reconciler.rs`: one `tokio` task per registered control plane, polling `/health` + `/status.json` on a jittered interval and driving a `healthy` → `degraded` (3 consecutive failures) → `unreachable` (10) health state machine, persisted to `control_planes`
- Remote enums (`RunState`, `RunSource`, `TaskStatus`, `ApprovalState`) all carry an `Unknown(String)` fallback so a docket upgrade degrades gracefully instead of failing a poll
- `adapters::prometheus`: dependency-free `/metrics` text-exposition parser, reused by any future metrics ingestion
- Every dollar-valued field is named `*_usd_estimated` — token counts are the primary, trustworthy measure; docket reports no real spend, so any cost figure downstream is a derived estimate. See `docs/book/src/developer/orchestration.md`

**tack-api** (library — does not build its own binary):
- Axum HTTP server with 84 documented paths + 1 WebSocket (includes the 8 orchestration endpoints gated behind `TACK_ORCH_ENABLE`, the operator execution/fleet surface, and the 13 `/api/runner/v1` runner-protocol paths)
- **Two authentication surfaces, separated structurally.** Operator routes live under `/api` behind `require_token`. Runner routes are nested as a _sibling_ of `/api` on the outer router, so they never traverse the operator auth layer at all — deliberately not an exemption-list entry, which a later edit could quietly widen. Each runner handler authenticates its own hashed bearer credential via `handlers/runner_protocol/runner_auth.rs`.
- `x-tack-principal` is **overwritten from server config** by `middleware::inject_operator_principal` and never read from the request. Operator idempotency is scoped by principal, so a trusted header would let one caller collide with another's requests.
- Server entry point exposed as `tack_api::serve()` (in `server.rs`)
- WebSocket support for real-time board updates
- Request handlers in `handlers/` (per entity)
- Config loading: TOML + env vars
- Error mapping: `CoreError` → HTTP status codes
- Debug endpoints: `/api/health`, `/api/debug/info`, `/api/debug/db-stats`
- File upload support: multipart/form-data (max 50MB)
- Export functionality: JSON and CSV formats
- `orch_store.rs`: wires `tack-orch`'s `ControlPlaneStore` trait to the real `Repository` + a `kind`-dispatched adapter constructor; spawns the reconciler from `server.rs` behind `config.orch_enable`

**tack-runner** (the pull-based execution runner — its own binary, separate from `tack`):
- Polls the Tack API's `/api/runner/v1` surface: enroll, refresh capabilities, claim, heartbeat, accept, start, stream events, poll decisions, submit artifacts, complete, and report cancellation/recovery observations
- Owns everything the API must not: local vendor credentials, the isolated per-attempt workspace/worktree, the owner-only TOML journal written **before** spawn, and the harness subprocess itself
- `harness/` holds the adapter layer — `process.rs` (bounded output capture, timeouts, process-group cancellation), `event_sink.rs` (backpressure), `redact.rs`, `artifact.rs`, and one module per harness: `codex.rs`, `claude_code.rs`, `opencode.rs`
- Two traits: `client::engine::HarnessAdapter` (per-attempt lifecycle — `validate`/`start`/`cancel`/`wait`/`reconcile`) and `harness::HarnessProbe` (version/capability discovery, which needs no claimed attempt). `AdapterRegistry` implements `HarnessAdapter` by dispatching on the requested harness kind, so the engine takes exactly one adapter type
- **Capabilities are honest or the adapter is rejected.** `AdapterRegistry::register_probe` refuses any probe claiming `Supported` cancellation, because every harness's shell tool spawns its subprocess in a new session outside the runner's process group — verified with `ps` against real `claude` and real `opencode`. Cancellation is `Advisory` everywhere; the scheduler must read the capability snapshot, never assume
- Live harness tests are opt-in (`#[ignore]` + a PATH check) and never required in CI; the shared fake binary at `harness/fixtures/fake_harness.sh` is the always-runnable path, driven by `TACK_FAKE_HARNESS_MODE`
- See `docs/agent-handoffs/part-iii/III-D{1,2,3,4,5}.md` for each adapter's observed-vs-assumed CLI contract

**tack-cli** (the single `tack` binary):
- `tack` with no subcommand (or `tack serve`) starts the server + web UI via `tack_api::serve()` — the primary, UI-first entry point
- CLI client using `clap`: `init`, `add`, `list`, `move`, `board`, `branch`, `search`, `sprint`, `template`, `role`, `comment`, `field`, `backup`, `restore` (complete)
- `tack mcp` — Model Context Protocol server over stdio (hand-rolled JSON-RPC 2.0 in `mcp.rs`); proxies tool calls to a running server over HTTP so workflow rules apply. See `docs/MCP.md`
- `tack branch <item-id>` — derives/creates a git branch from an item (`git.rs`)
- Client commands talk to the server over HTTP (blocking `reqwest`); never open the DB directly

**frontend** (SolidJS + TypeScript):
- Responsive SPA on a **two-axis design-token system**: mode (`.dark` class) × palette (`data-palette` attr) → Teal/Clay/Graphite × light/dark, switched from the sidebar footer. All colors come from `--color-*` tokens in `index.css`; components never use raw hex. WCAG AA, axe-gated in CI. Fonts: Hanken Grotesk + JetBrains Mono (self-hosted via `@fontsource`). See `docs/book/src/developer/frontend.md`.
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

- **61 migrations** tracked in `_migrations` table (039–048 added the ten neutral execution tables; 049+ refine execution replay, recovery and attempt-start facts)
- Migrations are transactional with ordered-prefix and checksum enforcement; 037/038 do a copy/verify/swap rebuild guarded by a `VACUUM INTO` snapshot
- **`BEGIN IMMEDIATE` is mandatory for read-then-write transactions.** A deferred transaction that reads then writes deadlocks under concurrency — two callers both upgrade from reader to writer and SQLite returns `SQLITE_LOCKED`. Ten sites in `repo/execution.rs` hit this; each was stress-tested before and after the fix. Write-first methods are fine as-is and were deliberately left deferred. Note the shared in-memory test harness can _mask_ these races — prove any new concurrency test load-bearing against a file-backed DB by reverting the fix and watching it fail
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
- `/api/projects/{id}/import-github` — GitHub Issues import (1 endpoint; `owner/repo` or full URL, optional PAT, label filter, PR-skipping, cursor pagination). Imported items are linked in the `github_links` table so completing them pushes a close back to GitHub when `TACK_GITHUB_TOKEN` is set (Phase 21, push-only). See `docs/GITHUB-SYNC.md`
- `/api/projects/{id}/import-linear` — Linear import (1 endpoint; Linear API key, optional team/project filter, label filter, priority mapping, cursor pagination)
- `/api/backup`, `/api/restore` — Local DB backup download / staged restore (2 endpoints)
- `/api/backup/remote` (POST/GET), `/api/backup/remote/restore` — Cloud (S3-compatible) backup, list, and staged restore (3 endpoints)
- `/api/settings/backup` (GET/PUT) — Read/update the UI-editable cloud-backup config; secret key is write-only (returned as a `secret_key_set` boolean)
- `/api/control-planes` (GET/POST), `/api/control-planes/{id}` (GET/PATCH/DELETE), `/api/projects/{id}/orch-link` (GET/PUT), `/api/fleet` (GET) — Agent-fleet orchestration (8 endpoints; all gated behind `TACK_ORCH_ENABLE`, 404 when unset). Control-plane token is write-only (`token_set` boolean). See `docs/book/src/developer/orchestration.md` and `docs/book/src/user-guide/orchestration.md`
- `/api/executions`, `/api/runner-fleets`, `/api/runners/*`, `/api/agent-profiles`, `/api/model-profiles` — **Operator** execution surface (create/list/get/cancel/requeue, fleet and profile management, runner enrollment and revocation). Under operator auth. Raw enrollment tokens are returned exactly once at issue time and only their SHA-256 hash is stored
- `/api/runner/v1/*` — **Runner protocol**, 13 paths under a separate credential: `enroll`, `refresh`, `claim`, `heartbeat`, and per-attempt `accept`, `start`, `events`, `decisions`, `decisions/poll`, `artifacts`, `completion`, `cancellation-observation`, `recovery-observation`. Every attempt-scoped mutation validates runner identity + attempt id + current fencing token; a stale fence returns the stable `stale_lease` error and writes nothing

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
- **Wave gates** in `crates/tack-api/tests/wave2_gate.rs` — deliberately import no test
  infrastructure from any card and drive the real `build_router`, because a card's own
  green tests are not evidence that the integrated system works.

**A test that asserts a status code has usually not proved the claim.** The recurring failure
in this codebase has been tests that pass while proving something weaker than their name says:
a 413 asserted without checking the DB was unwritten; a concurrency test that caught `Err(_)`
and retried sequentially, so it silently stopped being concurrent; a capability reported
`Supported` with no implementation behind it. When adding a test for a "writes nothing" or
"rejects before X" claim, assert the absence directly — row counts, an untouched checkpoint,
empty bookkeeping — and prove the test is load-bearing by reverting the fix and watching it
fail.

Use `assert_matches!` macro for enum matching in tests. When changing an API
response shape, update both the Rust handler and the matching mock in the
frontend unit/E2E tests (e.g. the `GET /items/{id}` detail envelope).

## Working on Part III (the active runner-fleet cycle)

`TODO.md` carries the full board, the 14 working rules and per-card ownership. The ones that
bite hardest in practice:

- **`docs/contracts/runner-v1/` is the authority**, not any Rust/TypeScript type. 46 frozen
  JSON fixtures, byte-pinned by `crates/tack-orch/tests/runner_contract.rs`. Hand-written
  DTOs are never a second authority — if a fixture and the code disagree, say so rather than
  bending either side. Adding or editing a fixture requires updating that pin table in the
  same change.
- **Shared files have one owner per wave**: `migrations.rs`, `router.rs`, `openapi.rs`,
  the generated schema, root `Cargo.toml`, the CI workflow, and the contract directory. If a
  change needs one of those and it isn't yours, record the request rather than making it.
- **`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are generated.** Never
  hand-edit. Regenerate with `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`
  then `cd frontend && npm run gen:api`. CI gates on the diff.
- **Unsupported is typed, unknown is explicit, unmeasured is nullable.** No
  `unimplemented!()`, no structural zero standing in for "unknown", no capability reporting
  `false` where the truth is "not verified". A capability claim is load-bearing — the
  scheduler will act on it.
- **Logs carry ids only** — never credentials, prompt bodies, query strings or whole
  environment values, and tests assert the redaction.
- **Each card writes one handoff** in `docs/agent-handoffs/part-iii/`. Corrections are
  appended as amendments with the original claim left standing, never rewritten — the
  history of what was believed and later falsified is the point.

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
