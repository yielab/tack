# Roadmap

**Current version:** 2.0.0  
**Status:** All thirteen engineering phases complete, plus competitive/growth phases
20 (MCP server), 22 (dev-native CLI), 23 (Table view), and 24 (positioning & presets).
The product is feature-complete for the solo-dev / small-team use case; remaining work
(Phase 21 GitHub sync, Phase 25 local-first) is additive and gated — see **Planned** below.

---

## Completed

### Phase 0 — Repo Hygiene
Dead code removed. Docs consolidated. Status and architecture accurately documented.

### Phase 1 — CI & Security Baseline
- GitHub Actions: fmt + clippy + test + frontend typecheck + build + bundle size gate
- CORS allow-list (`TACK_ALLOWED_ORIGINS`)
- Global body-size limit + 50 MB upload cap
- Optional Bearer token auth (`TACK_API_TOKEN`)
- Input validation via `validator` on all Create/Update DTOs

### Phase 2 — Architecture Correctness
- Workflow validation moved into `tack-core` (was in DB layer)
- Dual-board system removed — one `boards` table
- CLI rewritten to use the HTTP API (no direct DB access)
- Import implemented (JSON round-trip with ID remapping)
- `assignee` field added to Item model

### Phase 3 — Product Depth
- CLI: sprint commands, shell completions, vocabulary-aware output, `--json` on all commands
- Settings UI: live vocabulary editor, workflow column editor
- Performance: sprint index, lazy-loaded routes, 22 KB entry bundle

### Phase 4 — Release Readiness
- Backup/restore: `GET /api/backup` (VACUUM INTO), `POST /api/restore` (staged), CLI commands
- Observability: `/api/health` with migration count, all handlers instrumented with `#[instrument]`
- Single-binary: `--features embed-spa` embeds SPA into the release binary (~10 MB)

### Phase 5 — Frontend View Consolidation
- "Group By" removed; Board always shows columns by workflow status — no setting to configure
- Board derives columns from the project workflow directly; no saved board object required
- Settings → Boards CRUD panel and `BoardSelector` removed
- Board drag = status change with WIP limit enforcement on drop
- Tree view deleted; List gains a Hierarchy toggle (indent by `parent_id`, expand/collapse all)
- Calendar and Timeline made interactive: drag to reschedule items / resize to set date range
- Sprint rebuilt as a two-pane planning surface: Backlog ↔ sprint lanes with capacity/burndown
- Sprint promoted to a work tab alongside Board / List / Timeline / Calendar
- All five views share one data source via `ProjectItemsContext` — no redundant fetches

### Phase 6 — Frontend Tests
- 106 Vitest unit tests across 17 test files — all passing
- Coverage: API client contracts, `deriveBoard` pure function, context providers, settings panels, CSV import UI
- Playwright end-to-end tests: deferred (golden-path covered by Rust handler tests)

### Phase 7 — Template Management Depth
- Template creator: vocabulary, workflow, custom-field, and board sections — "Coming Soon" placeholder gone
- `POST /api/projects/{id}/save-as-template` snapshots vocabulary, workflow, custom-field definitions, and boards
- "Save as template" dialog in project Settings routes to the Templates gallery on completion
- Built-in templates seeded for every `ProjectType` on first run (`is_builtin = true`, not deletable)
- Templates gallery groups built-in vs user templates; each card shows status count, vocab overrides, field count, board count
- CLI: `template list`, `template show <id>`, `template create-from <id>` with `--json`
- Template payload validation: at least one status per category, no duplicate names, Select/MultiSelect require options

### Phase 8 — Custom Field Validation + Alexa Voice Integration
- `CustomFieldDefinition::validate_value()` in `tack-core`: enforces type (string, number, boolean, date, URL, select option membership, multi-select array), plus JSON-configured rules (`pattern`, `min_length`, `max_length`, `min`, `max`, `max_items`)
- `set_field_value` handler returns `422 Unprocessable Entity` on validation failure instead of silently storing bad data; 28 new unit tests in `tack-core`
- `POST /api/alexa` custom-skill endpoint: maps voice intents onto existing item/workflow logic
  - **AddTaskIntent** — creates item at the initial workflow status
  - **ListTasksIntent** — speaks open-item count and first few titles
  - **CompleteTaskIntent** — moves matching open item to first Done status; enforces transition rules and WIP limits; propagates parent auto-completion
- Alexa endpoint authentication: constant-time skill-ID comparison + ±150 s timestamp replay guard
- Vocabulary-aware spoken responses (construction projects say "Work Order", not "task")
- Endpoint is exempt from the Bearer-token gate; disabled (404) when `TACK_ALEXA_SKILL_ID` is unset
- Board view applies per-board item filters on fetch (replaces TODO no-op)
- 13 new Alexa handler tests covering verification, all intents, and edge cases

### Phase 9 — Full Integration Test Coverage

- API integration tests expanded from 16 → 36 (sprints, roles, comments, dependencies, search, JSON/CSV export, item update/delete)
- Frontend utility tests: vocab resolution, lens persistence, keyboard manager, optimistic-update rollback — total 144 Vitest tests across 21 files
- Vitest config cleaned up for vite 8 + OXC pipeline (removed stale esbuild shim)

### Phase 10 — Webhook Notifications

- `TACK_WEBHOOK_URL` — when set, POSTs JSON events for every item create/update/delete, sprint status transition, and items due within the next hour
- `TACK_WEBHOOK_SECRET` — optional HMAC-SHA256 signing; adds `X-Tack-Signature: sha256=<hex>` so receivers can verify authenticity
- Background task fires `item.due_soon` once per hour for incomplete items whose `due_date` falls in the next 60 minutes
- Delivery is fire-and-forget (tokio::spawn); errors are logged but never fail the originating request
- All event payloads include `event`, `timestamp`, and `project_id` fields

### Phase 11 — GitHub Issues Import

- `POST /api/projects/{id}/import-github` — fetches issues from any public (or token-accessible private) GitHub repository and creates Tack items
- Request body: `repo` (owner/repo or full URL), `token` (optional PAT), `import_closed` (default false), `label_filter` (optional label allow-list)
- Pull requests are silently skipped; closed issues map to the first Done-category status
- Pagination handled automatically (100 issues/request until the last page)
- Response includes `created`, `skipped`, and `rate_limit_remaining` counts
- 7 unit tests for URL parsing covering all input forms

### Phase 12 — Linear Import

- `POST /api/projects/{id}/import-linear` — fetches issues from Linear's GraphQL API and creates Tack items
- Request body: `api_key` (Linear personal API key), `team_id` (optional slug or ID), `project_id` (optional; overrides `team_id`), `import_completed` (default false), `label_filter` (optional label allow-list)
- Priority mapping: Urgent→Critical, High→High, Medium→Medium, Low→Low; No priority→unset
- Cursor-based pagination (50 issues per request)
- Response includes `created` and `skipped` counts
- 6 unit tests covering filter generation, cursor inclusion, project-vs-team precedence, priority mapping, and cursor injection sanitisation

### Phase 13 — Remote Cloud Backup (S3-Compatible)

**Goal:** Let a user back up their entire Tack instance to any S3-compatible object
store (Cloudflare R2 / Backblaze B2 free tiers, AWS S3, or a self-hosted MinIO) and
restore it on a different machine — so the same data is reusable across local
installations. This is **snapshot replication**, not a live shared database: semantics
are "one active writer at a time, last upload wins." It reuses the existing
`VACUUM INTO` backup flow ([backup.rs](../../../crates/tack-api/src/handlers/backup.rs))
and the existing staged-restore-on-startup mechanism
([main.rs](../../../crates/tack-api/src/main.rs)).

**Design decisions (locked):**

- Provider-agnostic via the [`object_store`](https://docs.rs/object_store) crate — a
  free provider and a custom server are the same thing (an endpoint + access keys),
  so there is exactly one code path. No per-provider SDKs.
- The backup bundle **includes uploaded attachments**, not just the database, because
  attachments live on disk in `TACK_STORAGE_DIR` (not in SQLite) and would otherwise
  be lost on cross-machine restore.
- Keep it simple: no incremental/delta backups, no encryption in v1 (documented as a
  follow-up), no multi-writer conflict resolution.

#### Task 1 — Dependencies

Add to the workspace `Cargo.toml` `[workspace.dependencies]` and wire into
`crates/tack-api/Cargo.toml`:

- `object_store = { version = "0.11", features = ["aws"] }` — the `aws` feature speaks
  the S3 API and supports custom endpoints (R2/B2/MinIO).
- `tar = "0.4"` and `zstd = "0.13"` — to bundle DB + attachments into one compressed archive.
- `bytes` is already transitively available via axum; add it explicitly if needed.

#### Task 2 — Bundle format

A single artifact `tack-backup-<UTC-RFC3339>.tar.zst` containing:

- `database.db` — the `VACUUM INTO` snapshot (reuse the existing helper in
  `handlers/backup.rs`; factor the snapshot-to-bytes logic into a reusable function).
- `attachments/…` — a recursive copy of `TACK_STORAGE_DIR` (skip if the dir is empty
  or unset).
- `manifest.json` — `{ "format_version": 1, "created_at": <rfc3339>, "migration_version": <u32>, "db_sha256": <hex>, "install_id": <uuid>, "item_count": <u64> }`.
  `migration_version` = the count of applied rows in the `_migrations` table.
  `install_id` = a UUID persisted once in a new `app_meta` row (or a sidecar file next
  to the DB); generate on first run if absent.

#### Task 3 — New module `crates/tack-api/src/remote_backup.rs`

Pure-ish module, unit-testable. Public surface:

```rust
pub struct RemoteBackupConfig { /* from AppConfig, Task 4 */ }
pub fn store_from_config(cfg: &AppConfig) -> Result<Arc<dyn ObjectStore>, BackupError>;
pub async fn create_bundle(pool: &SqlitePool, storage_dir: &str) -> Result<Vec<u8>, BackupError>;
pub async fn upload(store: &dyn ObjectStore, prefix: &str, bytes: Vec<u8>) -> Result<BackupManifest, BackupError>;
pub async fn list(store: &dyn ObjectStore, prefix: &str) -> Result<Vec<BackupManifest>, BackupError>;
pub async fn download(store: &dyn ObjectStore, key: &str) -> Result<Vec<u8>, BackupError>;
pub async fn prune(store: &dyn ObjectStore, prefix: &str, keep: usize) -> Result<usize, BackupError>;
```

- Build the `AmazonS3` store with `AmazonS3Builder` — set `endpoint`, `region`, bucket,
  access key, secret key from config; call `.with_allow_http(true)` only when the
  endpoint is plain `http://` (local MinIO).
- `list` reads each object's key + the small `manifest.json` it stores **alongside** the
  archive (`<archive>.manifest.json`) so listing never downloads the full bundle.
- Define a `BackupError` (`thiserror`) mapped to HTTP 5xx, plus a 400 for "remote backup
  not configured."

#### Task 4 — Config ([config.rs](../../../crates/tack-api/src/config.rs))

Add fields to `AppConfig`, loaded from `tack.toml` and `TACK_BACKUP_*` env vars
(same precedence pattern as existing config):

| Env var | TOML key | Type | Default | Notes |
| --- | --- | --- | --- | --- |
| `TACK_BACKUP_ENDPOINT` | `backup.endpoint` | `Option<String>` | none | e.g. `https://<acct>.r2.cloudflarestorage.com` |
| `TACK_BACKUP_BUCKET` | `backup.bucket` | `Option<String>` | none | required to enable |
| `TACK_BACKUP_REGION` | `backup.region` | `String` | `auto` | R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | `backup.access_key` | `Option<String>` | none | |
| `TACK_BACKUP_SECRET_KEY` | `backup.secret_key` | `Option<String>` | none | never log |
| `TACK_BACKUP_PREFIX` | `backup.prefix` | `String` | `tack` | object key prefix |
| `TACK_BACKUP_INTERVAL_SECS` | `backup.interval_secs` | `Option<u64>` | none | omit = manual only |
| `TACK_BACKUP_RETENTION` | `backup.retention` | `usize` | `10` | keep newest N |

Add a helper `AppConfig::remote_backup_enabled() -> bool` (true when bucket + access
key + secret key are all set). Document every key in `CLAUDE.md` and the deployment guide.

#### Task 5 — Endpoints ([router.rs](../../../crates/tack-api/src/router.rs) + `handlers/backup.rs`)

All gated behind `remote_backup_enabled()` (return `409 Conflict` with a clear message
when disabled). All subject to the existing Bearer-token middleware.

- `POST /api/backup/remote` → `create_bundle` + `upload` + `prune`; returns the new
  `BackupManifest` as JSON.
- `GET  /api/backup/remote` → `list`; returns `[BackupManifest]` newest-first.
- `POST /api/backup/remote/restore` → body `{ "key": "<optional; defaults to latest>" }`;
  download, **validate `migration_version` ≤ the running binary's migration count**
  (reject newer snapshots with `409` to prevent corruption), then stage (Task 6).
  Returns `{ "staged": true, "restart_required": true }`.

#### Task 6 — Atomic staged restore (extend [main.rs](../../../crates/tack-api/src/main.rs))

The current code stages a `.restore` DB file applied on next startup. Extend it so the
**DB and attachments swap together**:

- On restore request: unpack the bundle to `<db>.restore` (database) and
  `<storage_dir>.restore/` (attachments).
- Extend `apply_staged_restore()` (startup) to, when `.restore` artifacts exist:
  move current DB → `<db>.bak`, current storage dir → `<storage_dir>.bak`, then promote
  the `.restore` artifacts atomically. Log every move at `warn` for auditability.
- Because migrations auto-run forward on startup, restoring an **older** snapshot is
  safe; the Task 5 version guard blocks the unsafe **newer** direction.

#### Task 7 — Scheduler (in `main.rs`, mirror the webhook due-soon loop)

When `interval_secs` is set, `tokio::spawn` a loop that sleeps the interval, calls the
backup path, prunes to `retention`, and logs success/failure (never panics the server).
Model it on the existing hourly `item.due_soon` background task.

#### Task 8 — CLI ([tack-cli/src/main.rs](../../../crates/tack-cli/src/main.rs))

Thin wrappers over the new endpoints (CLI talks HTTP, never the DB directly):

- `tack backup --remote` → `POST /api/backup/remote`, print manifest.
- `tack backups` → `GET /api/backup/remote`, print a table (date, size, item count, key).
- `tack restore --remote [--key <key>]` → `POST /api/backup/remote/restore`; print the
  "restart the server to apply" notice. Support `--json` like every other command.

#### Task 9 — Tests

- `remote_backup.rs` unit tests: bundle round-trips (create → unpack → identical DB bytes +
  attachment tree), manifest serialization, prune keeps newest N, version-guard logic.
- Handler tests: `409` when disabled; restore rejects a manifest whose `migration_version`
  exceeds the local count. Use a mock/in-memory `ObjectStore` (object_store ships an
  in-memory backend) so no network is required.
- CLI test: arg parsing for the new `--remote`/`--key` flags.

#### Acceptance criteria

- With env vars pointing at any S3-compatible bucket, `tack backup --remote` uploads a
  `.tar.zst` bundle + sidecar manifest; `tack backups` lists it.
- On a second, empty install pointed at the same bucket, `tack restore --remote`
  followed by a server restart reproduces the **items, sprints, and attachments** of the
  source install.
- Restoring a snapshot created by a newer schema is rejected, not silently applied.
- `cargo test --workspace` and `cargo clippy` pass; no secrets are logged.

#### Out of scope (explicit follow-ups, do not implement here)

- Client-side encryption of bundles (`age`/AES) before upload.
- Incremental/delta backups and deduplication.
- Live multi-writer sync (would require Turso/libSQL — a separate, larger effort).

---

## Planned

The product is feature-complete for the solo-dev / small-team use case. Phases 20–25
are **competitive / growth** work, driven by a deep competitive analysis of Tack
against the Rust and self-hosted PM ecosystem.

**Why these phases (the research, in brief):** No mature Rust-native, web-based,
self-hostable PM tool exists among the leading Jira alternatives — Tack is alone in
its niche. Competitors run other stacks (Plane/Django, Vikunja/Go, Huly/TypeScript,
OpenProject/Rails); the only _actually Rust_ rivals are hobby-scale terminal tools
(taskwarrior-tui, rust_kanban, kanbanban, fulsomenko/kanban). The verified gaps Tack
should close are: a **Table view** (Vikunja), **bi-directional GitHub sync** (Huly),
**AI-agent / MCP** support (Plane, Vibe Kanban), and **git-from-card** dev
conveniences (fulsomenko/kanban). Tack's differentiators to strengthen: the **single
~10 MB binary** (no Postgres/Docker-compose), **per-project vocabulary + domain
presets**, the **web + API + CLI triad**, and the **MIT license** (vs the AGPL/EPL
field). The Rust/self-host community values keyboard-first, offline/local-first,
plaintext, single-binary, low-memory tools — and criticizes heavy Docker-compose
deployments and bloated SPAs. The dominant 2025–26 trend is **AI agents as
first-class PM actors**, which makes Phase 20 the highest-leverage work.

Each phase below carries file paths and acceptance criteria so it can be picked up
cold. Three product decisions gate the dependent tasks — call them before dispatching:

1. **MCP transport** — stdio sidecar (`tack mcp`) vs an HTTP/SSE endpoint in `tack serve`? _(blocks Phase 20)_ — recommend **stdio sidecar** for v1.
2. **GitHub sync scope** — mirror status + comments + close-state, or status only for v1? _(scopes Phase 21)_
3. **Local-first** — is offline/CRDT sync in scope this cycle, or parked? _(gates Phase 25)_

> **Suggested parallelization:** Phase 22 (T1), Phase 24 (T1, T2), and the Phase 20
> decision have no dependencies and can start immediately. Phases 20 and 21 are each
> internally sequential; Phase 23 is a self-contained frontend track.

### Phase 20 — MCP Server (AI-agent-controllable board) ✅ _done_

**Goal:** Expose Tack's board to AI coding agents (Claude Code, Codex, etc.) via the
Model Context Protocol so an agent can read, search, and update items. No self-hosted
PM competitor ships a first-class MCP server, and the architecture fit is strong: it's
a thin protocol layer over the existing REST handlers. Writes go **through the REST
API**, never the DB directly (per the CLI contract), so workflow validation, WIP
limits, and parent-auto-complete all still apply.

> Shipped: `tack mcp` ([mcp.rs](../../../crates/tack-cli/src/mcp.rs)) — a hand-rolled
> JSON-RPC 2.0 stdio server (stdio sidecar transport, decision recorded in
> [docs/MCP.md](../../MCP.md)). Eight tools: `list_projects`, `list_items`,
> `get_item`, `search_items`, `create_item`, `update_item`, `move_item`,
> `add_comment`. 10 unit tests + a live end-to-end session (handshake → tools/list →
> read → create/move/comment, with invalid transitions correctly rejected as tool
> errors). Documented in [docs/MCP.md](../../MCP.md), the README, and the CLI reference.

#### Task 1 — Decide & document transport

Resolve decision #1. Write a `docs/MCP.md` stub comparing a stdio sidecar subcommand
(`tack mcp`, spawned per-agent, talks to `tack serve` over HTTP) vs an HTTP/SSE MCP
endpoint mounted in `tack serve`. Recommend **stdio sidecar** for v1 (simplest for
Claude Code's MCP config; no new auth surface). Get sign-off. **Blocks all other Phase 20 tasks.**

#### Task 2 — Scaffold the `tack mcp` subcommand

Add an `Mcp` subcommand in [tack-cli/src/main.rs](../../../crates/tack-cli/src/main.rs)
and a new `crates/tack-cli/src/mcp.rs`. Prefer the official `rmcp` Rust SDK in
`Cargo.toml`; fall back to a minimal JSON-RPC-over-stdio loop with `serde_json`.
Implement the MCP `initialize` + `tools/list` handshake over stdio. Reuse
[tack-cli/src/client.rs](../../../crates/tack-cli/src/client.rs) to reach `tack serve`,
reading server URL + optional `TACK_API_TOKEN` from
[tack-cli/src/config.rs](../../../crates/tack-cli/src/config.rs).

#### Task 3 — Read tools

`list_projects`, `get_board`, `search_items`, `get_item`. Map each to an existing REST
call (mirror DTOs from `handlers/{projects,boards_multi,items}.rs`; `search_items` uses
the global `/api/search` and per-project search). Return compact JSON (id, title, type,
status, assignee) to keep agent context small.

#### Task 4 — Write tools

`create_item`, `update_item`, `move_item` (status transition), `add_comment`. Route all
writes through the REST API in
[handlers/items.rs](../../../crates/tack-api/src/handlers/items.rs); surface validation
errors back as MCP tool errors. WebSocket broadcast happens server-side automatically.

#### Task 5 — Document & ship config

Expand `docs/MCP.md` with a copy-paste Claude Code `.mcp.json` snippet, a tool reference
table, and a security note (the MCP server inherits the API token — document scoping).
Add a README feature bullet and a `CHANGELOG.md` entry.

#### Acceptance criteria

- `tack mcp` completes an MCP handshake and lists tools (test with an MCP inspector or
  scripted stdin/stdout).
- An agent can enumerate projects, read a board, full-text search, fetch an item, create
  an item, move it through a **valid** transition, get a clear error on an **invalid**
  transition / WIP breach, and add a comment — with the live board updating over the
  existing WebSocket.
- Unit test the JSON-RPC tool dispatch; integration-test writes against an in-memory server.
- A new user can wire Tack into Claude Code in under 5 minutes from `docs/MCP.md`.

### Phase 21 — Bi-Directional GitHub Sync

**Goal:** Upgrade the existing one-way GitHub _import_ into two-way _sync_ so item
status/comments/close-state mirror back to GitHub — closing the biggest "real work" gap
vs Huly. Scope per decision #2.

#### Task 1 — Spec the sync model

Write `docs/GITHUB-SYNC.md`: link storage (item ↔ GitHub issue number + repo), conflict
policy (last-write-wins vs GitHub-authoritative), v1 scope (recommend: status +
close-state + comments outbound; inbound via webhook or poll), and push trigger (reuse
the webhook dispatch path in [webhook.rs](../../../crates/tack-api/src/webhook.rs) vs a
periodic reconciler). **Blocks the rest of Phase 21.**

#### Task 2 — Persist the issue↔item link

Add migration #18 in
[tack-db/src/migrations.rs](../../../crates/tack-db/src/migrations.rs) — an
`external_links` table (or `gh_repo` / `gh_issue_number` / `gh_synced_at` columns on
items) — plus a repo module under `crates/tack-db/src/repo/`, following the "Adding a
New Entity" pattern. Backfill links for previously imported items.

#### Task 3 — Outbound push

On a linked item's update, PATCH the GitHub issue (state, optionally labels) and POST
mirrored comments via the GitHub REST API using the stored PAT. Hook into the item
update path in `handlers/items.rs`; new logic in `handlers/import_github.rs` (or a new
`github_sync.rs`). Rate-limit aware; best-effort with logged failures.

#### Task 4 — Inbound sync

Implement either `POST /api/projects/{id}/github/webhook` (verify
`X-Hub-Signature-256`) or a poll-on-interval reconciler, per Task 1. Apply remote
changes through `tack-core` so workflow rules hold. Register the route in
[router.rs](../../../crates/tack-api/src/router.rs).

#### Acceptance criteria

- Moving a linked item to a Done status closes the GitHub issue; adding a comment mirrors
  it (test with a mocked GitHub endpoint).
- Closing an issue on GitHub moves the linked Tack item to Done (test with a captured
  webhook payload fixture).
- Migration runs clean on startup; `cargo test --workspace` and `cargo clippy` pass.

### Phase 22 — Dev-Native CLI (git-from-card)

**Goal:** Make the CLI the surface Rust devs love — cheap, high-signal, and a praised
feature in the Rust kanban field (mirrors fulsomenko/kanban).

#### Task 1 — `tack branch <item-id>` ✅ _done_

Fetch the item via `client.rs`; slugify `type/id-short-title` (e.g.
`feat/a1b2-add-table-view`). Print `git checkout -b <branch>` by default; run it with
`--checkout`; `--json` outputs the name. New `crates/tack-cli/src/git.rs` +
subcommand in `main.rs`. Respect a configurable prefix template. Unit-test the slug
function.

> Shipped: [git.rs](../../../crates/tack-cli/src/git.rs) (`slugify`, `type_prefix`,
> `branch_name`, 8 unit tests) + the `Branch` subcommand and `cmd_branch` in
> `main.rs`. `--prefix` overrides the type-derived prefix (feature→feat, bug→fix,
> …). Documented in the [CLI reference](user-guide/cli.md).

#### Task 2 — `tack open` / smart start

`tack open <id>` opens the item's web URL in `$BROWSER`. Optional `tack start <id>` =
move to the first in-progress status + `tack branch --checkout` in one step. Cover with
CLI help + a smoke test.

#### Acceptance criteria

- `tack branch <id>` prints a sane branch name; `--checkout` creates+switches; `--json`
  works. `cargo test` green.

### Phase 23 — Table View (editable grid) ✅ _Task 1 done_

**Goal:** Add a sortable, column-configurable, inline-editable grid — Vikunja's 4th view
and a common expectation. Parity, not differentiation.

#### Task 1 — Table view component ✅ _done_

New `frontend/src/features/table/` mirroring the structure of
`frontend/src/features/list/`; register the route and nav entry. Reuse the List data
layer and API client. Columns: title, type, status, priority, assignee, custom fields,
due date — headers respect per-project vocabulary. Inline edit → existing item-update
endpoint with optimistic UI. Sort + filter + column show/hide.

> Shipped: [Table.tsx](../../../frontend/src/features/table/Table.tsx) — sixth work
> lens (tab + `/projects/:id/table` route + command palette). Columns title, type,
> status, priority, assignee, due; inline edit of all but type (read-only, since the
> frontend `UpdateItem` doesn't carry `item_type`); sort (priority-rank aware),
> filter, and `localStorage`-persisted column show/hide. Threaded `assignee` through
> the TS `Item`/`UpdateItem` types. 11 Vitest tests for the pure `sortItems`/
> `filterItems`/`typeKey` helpers + 2 Playwright journeys (list/filter, inline-edit
> persistence). **Custom-field columns deferred** — they need per-item value fetches
> (a separate, larger change); the rest of the column set shipped.

#### Task 2 — Density toggle ✅ _done_

Wire the table to the `--density-*` tokens from the design roadmap's Phase 18 if they've
landed; otherwise ship comfortable-only.

> Shipped: a self-contained comfortable/compact toggle on the Table (row padding),
> persisted to `localStorage` — independent of the design roadmap's Phase 18 tokens,
> which can later supersede the local padding values.

#### Acceptance criteria

- Grid renders project items; inline edits persist and broadcast over WebSocket;
  sorting/filtering work. Vitest unit tests + a Playwright journey in `frontend/e2e/`.

### Phase 24 — Positioning & Proof ✅ _done_

**Goal:** Convert real strengths into evidence and messaging. Mostly docs/CI — low cost,
high marketing leverage.

> Shipped: [docs/BENCHMARKS.md](../../BENCHMARKS.md) (measured 10.3 MiB binary,
> ~113 ms cold start, ~12 MiB idle RSS, sub-3 ms p99 reads, with repro steps); a
> [one-line installer](../../../install.sh) verified end-to-end against the real
> v0.1.0-beta.6 release (resolves the platform asset via the GitHub API); README
> install options (`curl | sh`, `cargo install`) + a "Why Tack" comparison table vs
> Plane/Vikunja/Huly + an AI-agent lede; and three new vertical presets (Task 3,
> shipped earlier). Docs cross-linked in the README index.

#### Task 1 — Footprint benchmark ✅ _done_

Extend the existing k6 baseline in `tests/load/`; publish `docs/BENCHMARKS.md` with
cold-start, idle RSS, p50/p99 latency, binary size (embed-spa vs not), and a
deploy-step-count comparison vs a named competitor. Reproducible methodology — replaces
the inferential footprint claims with numbers.

#### Task 2 — One-line install + README positioning

Add a prebuilt-binary install path (`install.sh` curl-pipe-sh and/or `cargo binstall`)
with GitHub Release artifacts in `.github/workflows/`. Rewrite the README lede around
"one binary, one SQLite file, MIT-licensed, AI-agent-ready," with a comparison table
(Tack vs Plane/Vikunja/Huly: deploy complexity, deps, license, CLI, MCP).

#### Task 3 — Vertical workflow presets ✅ _done_

Add ≥2 new `ProjectType` presets (e.g. legal/case, research/lab, editorial/content,
event planning) across
[tack-core/src/workflow.rs](../../../crates/tack-core/src/workflow.rs), `vocabulary.rs`,
and `models.rs`, wiring `workflow_for_type()`, per the "Adding a New Project Type"
guide. Unit-test in `workflow.rs`.

> Shipped: three new types — **`legal`** (Intake → Discovery → Drafting → Review →
> Closed), **`research`** (Hypothesis → Design → Experiment → Analysis → Published),
> **`event`** (Ideas → Booked → In Progress → Confirmed → Done) — each with a
> workflow + 16-key vocabulary preset, a seeded built-in template, and CLI/UI
> selectors. The three duplicated `ProjectType`→string matches in
> `tack-db/repo/templates.rs` were collapsed to `.to_string()` (Display) while
> here. New unit tests in `workflow.rs` and `vocabulary.rs`; verified live (a
> `legal` project resolves task→Filing, board→Docket, assignee→Counsel).

#### Acceptance criteria

- `docs/BENCHMARKS.md` published with reproducible numbers; a user can install Tack with
  one command; README leads with the differentiators; new project types are selectable.

### Phase 25 — Plaintext / Local-First ✅ _done_

**Goal:** Court the plaintext + local-first crowd.

#### Task 1 — YAML/TOML project round-trip ✅ _done_

Extend [handlers/export.rs](../../../crates/tack-api/src/handlers/export.rs) with a
`format=yaml` export and a matching import; stable key ordering for clean git diffs;
reuse the existing round-trip ID remapping. Golden-file test for lossless round-trip.

> Shipped: `export?format=yaml` plus content-negotiated import (YAML when
> `Content-Type` mentions YAML, else JSON — both decode to the same intermediate
> `Value`, so a YAML export round-trips unchanged). serde_json's sorted keys keep
> diffs stable. Exposed in Settings → Data (Export YAML; import accepts `.yaml`/
> `.yml`). Integration test `export_yaml_round_trips_through_import`. Chose YAML over
> TOML because items' many null/Option fields can't serialize to TOML.

#### Task 2 — Offline-capable PWA (spike) ✅ _done_

Time-boxed spike: evaluate a service-worker + IndexedDB cache over the SolidJS SPA, and
whether CRDT sync is worth it vs the WebSocket model. Output `docs/LOCAL-FIRST-SPIKE.md`
with a go/no-go recommendation — not an implementation.

> Shipped: [docs/LOCAL-FIRST-SPIKE.md](../../LOCAL-FIRST-SPIKE.md). Recommendation:
> **NO-GO on CRDT sync** (misaligned with the single-writer/small-team model);
> **conditional-go, low-priority on a read-only offline PWA**; the YAML round-trip
> (Task 1) already captures the high-value, low-cost part of the trend.

#### Acceptance criteria

- YAML export→edit→import round-trips losslessly; `docs/LOCAL-FIRST-SPIKE.md` carries a
  clear go/no-go.

### Future / Optional

#### Multi-User / Auth
The current design is explicitly local-only and single-user (one shared token, no per-user accounts or identities). The API token (`TACK_API_TOKEN`)
covers the "shared on a LAN" use case. Full multi-user would require a proper auth layer (session
or JWT), per-user access control, and an audit log.

#### Notifications / Reminders
- Due-date notifications (OS native, email, or webhook)
- Recurring items

#### Import Formats
- ~~GitHub Issues import~~ — shipped in Phase 11
- ~~Linear export import~~ — shipped in Phase 12

#### Mobile / Offline
No current plans. The SPA is responsive on mobile browsers; no native app and no offline-first sync.

---

## Known Gaps

| Area | Gap |
|---|---|
| Frontend tests | 144 Vitest unit tests; Playwright E2E deferred |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported |
| Auth | No multi-user auth (by design for v1) |

---

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
