# Roadmap

**Current version:** 0.1.0-beta.6 (unreleased work pending → `v0.1.0-beta.7`)  
**Status:** All thirteen engineering phases complete, plus competitive/growth phases
20 (MCP server), 22 (dev-native CLI), 23 (Table view), 24 (positioning & presets),
and 25 (local-first). A full-repo audit (July 2026) produced the **audit-driven
Phases 26–32**, which are now **implemented and verified green** (244 Rust tests,
169 Vitest, clippy clean, frontend builds) — see the status board below. The work
is staged for release as `v0.1.0-beta.7`.

**Next cycle (added August 2026): Phases 33–38 — the Agent-Factory Control Center.**
Tack becomes the control panel for a factory of products built by
[docket](https://github.com/yielab/docket) agent fleets: a new `tack-orch` crate with
a `ControlPlane` trait and a pull-based reconciler, six new tables, dispatch from the
board, a fleet-wide approvals inbox, one-click product+pod provisioning, and
per-product unit economics. Executable task cards for parallel agents are in
[TODO.md](../../../TODO.md); the reciprocal docket-side work is Phase 22 of that
project's `ROADMAP.md`.

## Audit-driven cycle (Phases 26–32) — status board

| Phase | Title | Status | Deferred within phase |
|---|---|---|---|
| 26 | Correctness Hotfix | ✅ Done | — (release cut/tag itself pending) |
| 27 | Security Hardening | ✅ Done | 27.1 shipped a shared-secret Alexa gate instead of full X.509 cert-chain validation (size-budget trade-off, documented) |
| 28 | Backup → Sync v2 | ✅ Done (28.1–28.5) | 28.6 client-side bundle encryption — deliberately deferred (crypto-dep size cost) |
| 29 | Contract-First API | ✅ Done (29.1, 29.3–29.6) | 29.2 partial: pagination + error-envelope shipped; full handler response-typing + 201/204 normalization still pending (~20 endpoints still return ad-hoc JSON, modeled as `Object` in the spec) |
| 30 | Vocabulary & Nav | ✅ Done | — |
| 31 | Construction Verticals | ✅ Done | — |
| 32 | Enterprise OSS Standards | ✅ Done (32.1–32.5) | 32.6 binary-size guard (tokio `full` trim, feature-gate `object_store`, CI size gate) not started; retroactive git tags for beta.1–5 pending |

**Still open after this cycle:** Phase 21 inbound GitHub sync (webhook/poll + comment mirroring); the four deferred sub-tasks above (28.6, 29.2 typing, 32.6, historical tags). Everything else in 26–32 is code-complete and tested.

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

### Phase 21 — Bi-Directional GitHub Sync ⏳ _v1 push-only shipped; inbound + comments pending_

**Goal:** Upgrade the existing one-way GitHub _import_ into two-way _sync_ so item
status/comments/close-state mirror back to GitHub — closing the biggest "real work" gap
vs Huly. Scope per decision #2.

> **v1 shipped (push-only, status):** imported items are linked in a `github_links`
> table; completing a linked item closes its GitHub issue (reopening on the way back
> out) when `TACK_GITHUB_TOKEN` is set. Best-effort/fire-and-forget. Pieces:
> migration 018, `repo/github_links.rs`, `github_sync.rs` (`push_issue_state` +
> `state_change`), the `maybe_sync_github` hook in `handlers/items.rs`, and import
> linking. `TACK_GITHUB_API_BASE` makes import+push testable/Enterprise-ready. Tests:
> wiremock push (3), link round-trip, and a full import→complete→close integration
> test — all against a **mocked** GitHub (no real repo touched). Decision recorded in
> [docs/GITHUB-SYNC.md](../../GITHUB-SYNC.md).
>
> **Still pending (future slices):** Task 1 comments-mirroring, Task 4 inbound sync
> (webhook/poll), per-project tokens + manual item linking, and a real-repo live test.
> The task notes below describe the _full_ bi-directional vision.

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
whether CRDT sync is worth it vs the WebSocket model. Produce a go/no-go recommendation —
not an implementation.

> Spike complete. Recommendation:
> **NO-GO on CRDT sync** (misaligned with the single-writer/small-team model);
> **conditional-go, low-priority on a read-only offline PWA**; the YAML round-trip
> (Task 1) already captures the high-value, low-cost part of the trend.

#### Acceptance criteria

- YAML export→edit→import round-trips losslessly; the offline-PWA spike reached a
  clear go/no-go.

## Next — Audit-Driven Phases 26–32 (July 2026)

A four-track parallel audit (backend/security, frontend/UX, docs/OSS standards,
backup-sync/API contracts) of the full repo produced these phases. They are ordered
by risk: **Phase 26 is a release blocker** (two shipped UI features silently lose
data); 27 closes security holes; 28–32 are quality/growth. Every task carries file
paths and acceptance criteria so it can be dispatched cold to an agent.

**Headline audit findings (evidence, not opinion):**

| # | Finding | Where | Severity |
|---|---------|-------|----------|
| 1 | `update_item` has **no UPDATE branch for `sprint_id`, `due_date`, `estimate_unit`** — drag-to-sprint (`Sprints.tsx:191`) and due-date edits (`ItemHeader.tsx:133`) are silent no-ops that vanish on refresh | `crates/tack-db/src/repo/items.rs:179-262` | Critical |
| 2 | `started_at`/`completed_at` are **never set on ordinary status moves** (only in parent auto-propagation) — due-soon webhooks keep firing for completed items; cycle-time data is never populated; contradicts CLAUDE.md | `crates/tack-db/src/repo/items.rs:202-208` vs `:401` | Critical |
| 3 | Alexa endpoint authenticates by **skill-ID + timestamp only — no Amazon cert-chain/signature validation**; skill IDs are not secret, so the endpoint is forgeable when enabled | `crates/tack-api/src/handlers/alexa.rs:368-384` | High |
| 4 | No warning when binding non-loopback with no `TACK_API_TOKEN` — unauthenticated read/write API + DB download exposed | `crates/tack-api/src/server.rs:104-108`, `middleware.rs:30-33` | High |
| 5 | `PRAGMA foreign_keys=ON` runs on **one pooled connection**; the other 4 enforce nothing — advertised referential integrity is best-effort | `crates/tack-db/src/lib.rs:24` | High |
| 6 | Backup restore: **tar path traversal** (`attachments/../../…` escapes staging), `db_sha256` **never verified**, stale `-wal`/`-shm` not removed (corruption risk at recovery time), swap can strand an empty DB | `crates/tack-api/src/remote_backup.rs:461-471`, `server.rs:183-218` | High |
| 7 | **S3 secret key + install_id ride inside every backup bundle** (they live in `app_meta`, which `VACUUM INTO` snapshots) — exfiltratable via `GET /api/backup`; restore clones another install's identity | `crates/tack-api/src/handlers/settings.rs` + backup flow | High |
| 8 | Backup **scheduler reads env config only** — UI-saved settings (Settings → Cloud Backup) are ignored by scheduled runs; split-brain destinations | `crates/tack-api/src/server.rs:229-239` vs `effective_backup_config()` | High |
| 9 | Linear import interpolates `team_id`/`project_id` **unescaped into GraphQL** (cursor is sanitized; these aren't) | `crates/tack-api/src/handlers/import_linear.rs:303-312` | Medium |
| 10 | Vocabulary promise half-kept: global "+ New" modal ignores project vocabulary; Sprints view, WorkTabs, command palette, first-run guide hardcode "sprint"/"Story Points" | `frontend/src/app/Layout.tsx:181`, `features/sprints/Sprints.tsx`, `shared/ui/WorkTabs.tsx:11` | High (product) |
| 11 | Breadcrumb broken for Table + Sprints views (stale `tree` key, `sprints` vs `/sprint` mismatch); Table view missing from the sidebar entirely | `frontend/src/shared/ui/Breadcrumb.tsx:5-10`, `Sidebar.tsx:165-170` | Medium |
| 12 | `docs/API-REFERENCE.md` ("canonical") drifts from `router.rs` in 8+ places; `docs/DEPLOYMENT-GUIDE.md` documents a **Docker/compose setup that does not exist in the repo** | docs | High (trust) |
| 13 | No API contract artifact (no OpenAPI/schema); DTOs triple-maintained by hand (Rust ⇄ TS types ⇄ test mocks); two incompatible error JSON shapes; item lists silently truncate at 100 in the UI | `crates/tack-api/src/handlers/*`, `frontend/src/types/` | High (SDD) |
| 14 | Releases ship unsigned (no checksums/SBOM/provenance); SECURITY.md routes disclosure through **public** issues; no CoC; MSRV claim (1.75+) not enforced anywhere | `.github/workflows/release.yml`, `SECURITY.md` | Medium |

**What the audit confirmed is genuinely strong (keep, don't churn):** pure
`tack-core` reused identically by REST/CLI/MCP/Alexa; centralized `CoreError→HTTP`
mapping; disciplined parameterized SQL; constant-time token compare + write-only
secrets; ~82 KB gzip total frontend JS with per-route lazy loading; aggressive
release profile (10.3 MiB binary); 6-job CI with bundle-size, design-token, and
axe gates. The architecture is **not overengineered** — the two size wins available
are trimming `tokio = "full"` features and feature-gating `object_store`.

### Phase 26 — Correctness Hotfix ✅ _done_ (release blocker → cut `v0.1.0-beta.7`)

**Goal:** No shipped surface silently loses user data. Everything here is small,
sequential-free, and must land before any other phase ships.

#### Task 26.1 — Persist the missing `update_item` fields

In [repo/items.rs](../../../crates/tack-db/src/repo/items.rs) add UPDATE branches for
`sprint_id`, `due_date`, `estimate_unit`. Support **null-clears** (the frontend
already sends `null` to clear): switch the Rust `UpdateItem` fields in
[models.rs](../../../crates/tack-core/src/models.rs) to double-`Option`
(`#[serde(default, with = "::serde_with::rust::double_option")]`) so "absent" ≠
"set to null", and thread through the handler. Regression tests: PATCH sprint
assignment persists across a re-fetch; PATCH `due_date: null` clears it.

#### Task 26.2 — Set `started_at` / `completed_at` on status transitions

In the item-update path, when the target status category becomes in-progress and
`started_at` is NULL → set it; when it becomes done → set `completed_at`; when it
leaves done → clear `completed_at`. This fixes `list_items_due_soon`
([items.rs:157](../../../crates/tack-db/src/repo/items.rs)) firing webhooks for
completed items. Assert both timestamps in tests (none exist today — that's why
this shipped broken).

#### Task 26.3 — Enforce foreign keys on every pooled connection

Replace the one-off `PRAGMA foreign_keys=ON` ([lib.rs:24](../../../crates/tack-db/src/lib.rs))
with `SqliteConnectOptions::foreign_keys(true)` (or `after_connect`). Add a test that
an orphaning insert actually rejects.

#### Task 26.4 — Fix broken frontend navigation

[Breadcrumb.tsx](../../../frontend/src/shared/ui/Breadcrumb.tsx): drop stale `tree`,
add `table` + `sprint` keys, fix the `sprints`/`/sprint` mismatch. Add the Table
lens to [Sidebar.tsx](../../../frontend/src/shared/ui/Sidebar.tsx). Pick one label
("Sprints") across WorkTabs / Sidebar / Breadcrumb / document title.

#### Task 26.5 — Alexa create path must validate

`add_task` in [alexa.rs](../../../crates/tack-api/src/handlers/alexa.rs) builds
`CreateItem` and skips `.validate()` — the only mutation path that does. Add it.

#### Task 26.6 — Cut the release

Add the pending UI-redesign + WCAG entries to `CHANGELOG.md [Unreleased]`
(commits `536d959`, `bef2771` are undocumented), fold in 26.1–26.5, tag
`v0.1.0-beta.7`. The Unreleased section currently holds ~10 shipped features —
that backlog is itself a cadence bug.

**Acceptance:** drag an item into a sprint, refresh — it's still there. Set and
clear a due date — persists. Complete an item — `completed_at` set, no due-soon
webhook. `cargo test --workspace` includes new regressions for every fix.

### Phase 27 — Security Hardening ✅ _done_

**Goal:** Safe-by-default for a tool that invites `TACK_HOST=0.0.0.0` + a public
S3 bucket. Independent tasks; parallelize freely.

> Shipped. Note: 27.1 landed a **mandatory shared-secret gate**
> (`TACK_ALEXA_SHARED_SECRET`, constant-time compared, passed via the skill
> endpoint URL) rather than full Amazon `SignatureCertChainUrl` X.509/RSA
> verification — the pure-Rust cert stack was judged too heavy for the ~10 MB
> binary budget. Documented in [docs/ALEXA.md](../../ALEXA.md). All other tasks
> (exposed-bind warning, tar-slip rejection, sha256 + format_version restore
> verification, secret-scrubbed bundles, Linear GraphQL escaping, validation
> stragglers) shipped as specified.

#### Task 27.1 — Alexa request signature validation

Implement Amazon's required `SignatureCertChainUrl` + `Signature` validation
(cert-chain URL allow-list `https://s3.amazonaws.com/echo.api/…`, chain verification,
SAN check for `echo.api`, body-hash compare) in
[alexa.rs](../../../crates/tack-api/src/handlers/alexa.rs), keeping the existing
skill-ID + timestamp checks. If the cert dependency is deemed too heavy for the
binary budget, the fallback is a mandatory shared-secret query param documented in
`docs/ALEXA.md` — but say so explicitly; today's check is forgeable by anyone who
knows the (non-secret) skill ID.

#### Task 27.2 — Refuse/warn on exposed unauthenticated bind

In [server.rs](../../../crates/tack-api/src/server.rs): if the bind host is
non-loopback and `api_token` is `None`, log a prominent warning at startup (and
consider requiring `TACK_INSECURE_NO_AUTH=1` to proceed). Also stop returning
`database_url` from `/api/debug/info` ([debug.rs:40](../../../crates/tack-api/src/handlers/debug.rs)).

#### Task 27.3 — Sanitize tar extraction

In `parse_bundle` ([remote_backup.rs:461-471](../../../crates/tack-api/src/remote_backup.rs)):
reject any entry whose path contains `..`/absolute components (mirror
`tar::Entry::unpack_in` semantics). Test with a malicious bundle fixture.

#### Task 27.4 — Verify backup integrity on restore

`stage_restore` must verify `db_sha256` and `format_version` from the manifest
before staging (both are computed at backup time and ignored at restore time).
Reject mismatches with a clear 409. ~15 lines + tests.

#### Task 27.5 — Escape Linear GraphQL filter fields

Apply the cursor-style sanitization to `team_id`/`project_id` in
[import_linear.rs:303-312](../../../crates/tack-api/src/handlers/import_linear.rs);
add the injection test that exists for cursors but not for these.

#### Task 27.6 — Keep secrets out of backup bundles

Strip (or re-key) `app_meta.backup_config` and `install_id` from the `VACUUM INTO`
snapshot — the S3 secret currently ships inside every local download and remote
bundle, and restoring clones the source install's identity. Simplest: after
snapshotting to the temp file, open it and `DELETE FROM app_meta WHERE key IN (…)`;
on restore, regenerate `install_id`.

#### Task 27.7 — Validation stragglers

Add `validator` derives + `.validate()` calls to `GitHubImportRequest`,
`LinearImportRequest`, board create/update, `UpdateSprintStatus`,
`CreateProjectFromTemplate`, and `UpdateBackupSettings` (retention ≥ 1,
interval ≥ 60 — today `retention: 0` deletes the backup you just made and
`interval_secs: 0` panics the scheduler task).

**Acceptance:** a forged Alexa POST with a known skill ID is rejected; a bundle
with `attachments/../../x` fails restore; a tampered bundle fails the sha check;
`retention=0` returns 422; startup on `0.0.0.0` without a token screams.

### Phase 28 — Backup → Sync v2 (safe personal multi-device) ✅ _done (28.1–28.5); 28.6 deferred_

**Goal:** Upgrade snapshot replication into a **trustworthy** "one active writer,
last upload wins" sync across a user's machines — without building CRDTs (Phase 25
spike already ruled that out). Depends on 27.3/27.4/27.6.

> Shipped: generation-counter conflict detection (persisted in `app_meta`, no
> migration), scheduler honoring runtime UI settings, fail-safe restore swap
> (stale `-wal`/`-shm` cleanup, rollback-on-failure, backup-before-restore),
> sidecar-first + `put_multipart` uploads with orphan reconciliation, a
> `POST /api/backup/remote/verify` preview endpoint wired into the Settings panel,
> and migration-version parity on the local restore path. **28.6 (client-side
> bundle encryption) is deferred** — it would add a crypto dependency against the
> binary-size budget; a `// TODO(phase-28.6)` marks the hook and
> [DEPLOYMENT-GUIDE.md](../../DEPLOYMENT-GUIDE.md) documents the interim guidance
> (private bucket + provider-side encryption-at-rest).

#### Task 28.1 — Scheduler honors runtime (UI) settings

`run_scheduled_backup` ([server.rs:229-239](../../../crates/tack-api/src/server.rs))
must call `effective_backup_config()` each tick instead of captured env config —
today UI-configured backups never schedule, and UI-overridden buckets are silently
ignored by the scheduler. Set `MissedTickBehavior::Delay` while there.

#### Task 28.2 — Generation counter + conflict detection

Add a monotonically increasing `generation` in `app_meta`, bumped on every write
transaction batch (or per-backup), recorded in the manifest. On **upload**: if the
remote head manifest has `generation` ≥ local and a different `install_id`, return
a 409 with both manifests ("another device has uploaded newer work — restore first
or force"). On **restore**: if local `generation` > snapshot's, require
`{"force": true}`. This is the single biggest missing piece for sync semantics.

#### Task 28.3 — Fail-safe restore swap

Make `apply_staged_restore` transactional-ish: delete stale `-wal`/`-shm` files,
roll back to `.bak` if any rename in the sequence fails (today a half-failed swap
boots a brand-new empty DB), keep one timestamped `.bak` generation, and clear
pre-existing `.restore` staging dirs before unpacking (they currently merge across
attempts). Auto-snapshot to the remote (if configured) before staging — "backup
before restore".

#### Task 28.4 — Upload robustness

Upload the sidecar manifest **before** the bundle (or reconcile orphans in
`prune`) — today a failed sidecar PUT leaks an invisible, unprunable bundle
forever. Use `object_store::put_multipart` for bundles over ~32 MB instead of
building everything in RAM (bundle creation currently holds DB + all attachments +
tar + zstd output simultaneously).

#### Task 28.5 — Restore preview + local-path parity

`POST /api/backup/remote/verify` (download + sha + version check, no staging) and
surface it in Settings → Cloud Backup as "Verify". Give the **local** `POST
/api/restore` the same migration-version guard the remote path has (today a local
restore of a newer-schema DB bricks startup).

#### Task 28.6 — Optional bundle encryption (v2 follow-up, keep scoped)

`age`-style symmetric encryption of bundles with a user passphrase stored only in
memory/env. Explicitly out of scope if it threatens the size budget; document
either way in `docs/DEPLOYMENT-GUIDE.md`.

**Acceptance:** two installs pointed at one bucket cannot silently overwrite each
other's newer snapshot; a mid-restore failure leaves the original DB bootable; a
UI-only cloud config schedules backups; 500 MB of attachments backs up without
holding it all in RAM.

### Phase 29 — Contract-First API (the SDD workflow) ✅ _done (29.1, 29.3–29.6); 29.2 partial_

**Goal:** One machine-readable contract, generated from the code, gating CI, and
feeding the frontend types and the docs — making the API-REFERENCE drift class of
bug structurally impossible. Sequence: 29.1 → 29.2 → 29.3 → (29.4 ∥ 29.5 ∥ 29.6).

> Shipped: the error envelope is unified across all handlers and parsed by the
> frontend; item lists return a `{data,total,page,per_page}` envelope (fixing the
> silent truncation at 100) with the CLI, MCP, and SPA consumers updated;
> **`utoipa` generates an OpenAPI 3.1 spec** (68 operations / 43 paths) served at
> `GET /api/openapi.json` and committed to [docs/openapi.json](../../openapi.json)
> — no bundled Swagger UI, and `ToSchema` is feature-gated so `tack-core` stays
> pure. Two CI drift gates lock the chain **handlers → `docs/openapi.json` →
> `frontend/src/shared/api/schema.gen.ts`** (Rust `git diff` gate + frontend
> `gen:api` gate), so neither the spec nor the generated TS client can silently
> diverge. Both API-reference docs now point at the spec as the source of truth
> instead of listing endpoints by hand. **29.2 is partial:** the pagination and
> error-envelope fixes shipped, but the broader refactor of ~59 `Json<Value>`
> handlers to typed response DTOs and the create→201 / delete→204 normalization
> are **not done** — ~20 endpoints are modeled as free-form `Object` in the spec
> until their handlers return named types. That typed-response pass + a `/api/v1`
> prefix is the batched pre-1.0 breaking change.

#### Task 29.1 — Unify the error contract

Migrate `handlers/backup.rs` and `handlers/settings.rs` ad-hoc `{"error": "…"}`
tuples to the structured `ApiError` envelope
([error.rs:85-92](../../../crates/tack-api/src/error.rs)); make the frontend's
`toApiError` ([client.ts:62-70](../../../frontend/src/shared/api/client.ts)) parse
it (users currently see raw JSON in error toasts).

#### Task 29.2 — Type the responses

Replace `Json<serde_json::Value>` returns with typed DTOs across
`handlers/` (~59 REST handlers; the DTOs mostly already exist in `tack-core`).
Standardize: creates → `201` + entity, deletes → `204` (they're currently split
between `{"deleted": true}`/200 and 204 by module). Add a pagination envelope
`{data, total, page, per_page}` to item lists — the UI silently truncates at 100
items today because no total is returned — and thread paging through
[api/items.ts](../../../frontend/src/shared/api/items.ts).

#### Task 29.3 — Generate `openapi.json` with `utoipa`

`utoipa` + `utoipa-axum` derives on DTOs/handlers (behind a feature flag in
`tack-core` so the pure crate stays dependency-light); serve at
`/api/openapi.json`; CI drift gate: regenerate and `git diff --exit-code` a
committed `docs/openapi.json`. Known manual spots: multipart upload, WS endpoint,
`ItemType::Custom` untagged enum.

#### Task 29.4 — Generate the frontend types

`openapi-typescript` in `frontend/scripts/`, replacing the hand-written
`frontend/src/types/` mirrors (the audit found `CreateItem.status` sent but
ignored, `assignee` supported but omitted). Type-check gate stays; hand-written
types remain only for frontend-internal shapes.

#### Task 29.5 — Regenerate the API docs from the spec

Replace the endpoint tables in `docs/API-REFERENCE.md` and
`docs/book/src/developer/api-reference.md` with spec-generated sections (keep the
prose). The audit found 8+ documented endpoints that don't exist (old board routes,
`PATCH/DELETE /sprints/{id}`, wrong role-assignment verbs, `GET /items/{id}/history`)
and shipped endpoints that are undocumented (import-github/linear/csv, YAML export,
save-as-template, backup/restore).

#### Task 29.6 — Contract tests

Point `schemathesis` (or a Rust equivalent) at the spec against the in-memory test
app in CI; retire the hand-rolled envelope checks in `frontend/e2e/api.spec.ts`
once redundant. `/api/v1` prefixing is **deliberately deferred** until the first
post-1.0 breaking change; batch the 201/204 normalization (29.2) as the last
pre-contract break.

**Acceptance:** `git diff --exit-code docs/openapi.json` gates CI; frontend types
are generated, not hand-copied; both API reference docs match `router.rs` because
they're derived from it; error toasts show human messages.

### Phase 30 — Vocabulary Integrity & Navigation Polish ✅ _done_

**Goal:** The core differentiator — per-project vocabulary — holds on **every**
surface. A construction project must never say "sprint". All frontend; parallelize
by file.

> Shipped: the global "+ New" modal, Sprints view, WorkTabs, command palette,
> document titles, and the first-run guide all resolve per-project vocabulary;
> breadcrumb + sidebar navigation fixed (Table lens now reachable); ~28 raw
> priority-hex literals replaced with tokens (dark-mode/palette-correct); dead
> deps removed; axe scans extended to five more pages; `check-tokens.sh` now also
> catches hex in inline styles.

#### Task 30.1 — Fix the global "+ New" modal

[Layout.tsx:181-187](../../../frontend/src/app/Layout.tsx) renders
`CreateItemModal` without the `vocabulary` prop (Board passes it; the global
entry doesn't) — the type picker shows Epic/Feature/Task regardless of project.
Resolve vocabulary from the active project context.

#### Task 30.2 — Vocab-ify the Sprints view

[Sprints.tsx](../../../frontend/src/features/sprints/Sprints.tsx): "Sprint
Planning" h1, "Backlog" pane (vocab key exists: "Pending Work"), drag hints,
toasts, "Sprint 1" placeholder — `t()` is already in scope; this is mechanical.
Same pass: `CreateItemModal.tsx:332` "Story Points" → `t('story_points')`,
`Dashboard.tsx:234`, `ItemHeader.tsx:138`.

#### Task 30.3 — Vocab-ify the chrome

WorkTabs "Sprint" tab, Sidebar "Sprints", command palette entries
([Layout.tsx:85-103](../../../frontend/src/app/Layout.tsx)), document titles, and
[EmptyProjectGuide.tsx:88-96](../../../frontend/src/shared/ui/EmptyProjectGuide.tsx)
step 3 ("Plan a sprint") — the **first-run screen** a construction user sees.
Point the guide's vocabulary link at the vocabulary tab, not `/settings`.

#### Task 30.4 — Token + hygiene sweep

Replace the 28 raw hex literals in inline styles (priority colors duplicated in
`Sprints.tsx:33-38`, `Timeline.tsx:260-265,457-460`, `Calendar.tsx:49-52`) with the
existing tokenized `priorityColor()` from
[PriorityDot.tsx](../../../frontend/src/shared/ui/PriorityDot.tsx) — they currently
ignore dark mode and the Clay/Graphite palettes. Extend `scripts/check-tokens.sh`
to catch hex in `style` props. Remove dead deps `@kobalte/core` and
`@solid-primitives/keyboard` (zero imports). Extend axe E2E scans beyond the
current 3 pages to Table, Timeline, Calendar, Sprints, and the item drawer.

**Acceptance:** `grep -ri "sprint\|story points" frontend/src --include='*.tsx'`
returns only vocab-resolved or teaching-copy hits; a `construction` project reads
Phase/Work Order/Effort Hours on every surface including first-run; all palettes
render priority colors correctly in dark mode.

### Phase 31 — Construction Verticals & Preset Onboarding ✅ _done_

**Goal:** Make "works for building projects" concretely true for wood-frame,
steel-frame, and SIP-panel builds — and make presets discoverable at project
creation. The template system already carries vocabulary + workflow +
custom-fields + boards as JSON; **no schema change, no new enum variants.**

> Shipped: three seeded `BuiltinSpec` construction verticals (Wood Frame, Steel
> Frame, SIP Panel), each with a tailored linear-plus-rework workflow and
> build-system custom fields (stud spacing, steel grade, panel count, …); the
> built-in seed dedup key moved from project-type to template-name so multiple
> Construction templates coexist (backward-compatible). The New Project modal is
> now template-first (selectable cards with a workflow + vocabulary preview and a
> "start blank" fallback), and the Templates page type filter covers all 11 types.

#### Task 31.1 — Three construction sub-preset templates

Add `BuiltinSpec` entries (all `ProjectType::Construction`) in
[templates.rs:201-260](../../../crates/tack-db/src/repo/templates.rs):

- **Wood Frame Build** — Permit → Foundation → Framing → Rough-In (MEP) →
  Insulation & Drywall → Finish → Inspect → Handover; custom fields: stud spacing
  (select: 16"/24" o.c.), lumber grade, sheathing type, shear-wall schedule ref.
- **Steel Frame Build** — Permit → Engineering → Fabrication → Erection →
  Decking/MEP → Fireproofing → Inspect → Handover; custom fields: steel grade,
  bolt spec, weld inspection class, torque log ref.
- **SIP Panel Build** — Design/Shop Drawings → Panel Fabrication → Delivery →
  Foundation → Panel Set → Seal & Penetrations → MEP Chases → Inspect → Handover;
  custom fields: panel count, panel thickness, spline type, sealant spec.

Each keeps the construction vocabulary base (Work Order/Phase/Inspection Point)
with per-template workflow transitions (linear + rework loops back from Inspect).
Unit-test seeding and workflow transition rules.

#### Task 31.2 — Templates in the New Project modal

[CreateProjectModal.tsx](../../../frontend/src/shared/ui/CreateProjectModal.tsx)
currently offers a bare 11-way `<select>`; the richer Templates gallery is a
disconnected page. Replace the type select with template cards (the
`templateSummaryChips` preview component already exists) grouped by domain, with
"start blank" fallback. Fix the Templates page type filter which is missing
legal/research/event ([Templates.tsx:38-47](../../../frontend/src/pages/Templates.tsx)).

#### Task 31.3 — Preset preview

On template-card focus, show the workflow columns and 3 sample vocabulary mappings
("task → Work Order") before the user commits — today the only hint is one line of
help text.

**Acceptance:** a user can create a "SIP Panel Build" project from the New Project
dialog in two clicks, and its board shows SIP phases with SIP custom fields;
`tack init --template` can instantiate the same from the CLI.

### Phase 32 — Enterprise OSS Standards ✅ _done (32.1–32.5); 32.6 not started_

**Goal:** Close the governance/integrity gaps between "excellent solo hygiene" and
"enterprise-grade open source". All independent; ideal for parallel dispatch.

> Shipped: truthful single-binary deployment docs **plus** a real `Dockerfile` +
> `docker-compose.yml` (the old guide documented a compose setup that didn't
> exist); release integrity in `release.yml` (SHA256SUMS, build provenance
> attestation, CycloneDX SBOMs, `cargo auditable`); private disclosure via GitHub
> Security Advisories; `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`, PR-process +
> branching sections in CONTRIBUTING; CI gates for MSRV (**Rust 1.85**, the
> edition-2024 floor — the old "1.75+" claim was impossible), `cargo-llvm-cov` +
> Vitest coverage thresholds, `cargo-deny`, and a weekly scheduled audit; and the
> enumerated doc-staleness fixes. **32.6 (binary-size guard — trim `tokio` `full`
> features, feature-gate `object_store`, add a CI binary-size gate) is not
> started.** Retroactive git tags for beta.1–5 remain a manual follow-up.

#### Task 32.1 — Truthful deployment docs

`docs/DEPLOYMENT-GUIDE.md` leads with Docker Compose instructions **for a
Dockerfile and compose file that do not exist in the repo**, describing the retired
two-service architecture. Either ship a minimal `Dockerfile` (scratch/distroless +
the static musl binary — trivially small) + compose file, or rewrite the guide
around the single-binary + systemd + Caddy reality. Decide and do one.

#### Task 32.2 — Release integrity

In `.github/workflows/release.yml`: emit `SHA256SUMS`, add
`actions/attest-build-provenance`, publish an SBOM (`cargo auditable` +
CycloneDX; npm SBOM for the SPA). Tag every future CHANGELOG release (beta.1–5
have entries but no tags; only beta.6 is tagged).

#### Task 32.3 — Security policy & community files

SECURITY.md currently tells reporters to open a **public** issue — switch to GitHub
Security Advisories (keep email fallback `info@yielab.com`). Add
CODE_OF_CONDUCT.md (Contributor Covenant), a PR-process/branching section in
CONTRIBUTING.md (README promises it's there; it isn't), `.github/ISSUE_TEMPLATE/config.yml`
with a security-contact link, and a one-paragraph GOVERNANCE.md (BDFL statement).

#### Task 32.4 — MSRV + coverage gates

README claims "Rust 1.75+" but nothing enforces it: set `rust-version` in the
workspace `Cargo.toml`, add an MSRV CI job, state the bump policy. Add
`cargo-llvm-cov` + Vitest coverage thresholds to CI (TESTING.md documents targets
core ≥85% / db+api ≥70% with zero enforcement). Add `cargo-deny`
(license + duplicate-dep policy) and a weekly scheduled audit run.

#### Task 32.5 — Doc staleness sweep

Fix the enumerated drift: introduction.md's false "`mdbook test` in CI" claim,
TESTING.md's stale a11y-debt note + "three jobs" (there are six),
developer/README.md "67 unit tests" → 73, API-REFERENCE CORS section
(`TACK_CORS_ORIGIN` doesn't exist → `TACK_ALLOWED_ORIGINS`), health-response
example shape. (The endpoint tables themselves are superseded by Phase 29.5.)

#### Task 32.6 — Binary size guard

Trim `tokio = { features = ["full"] }` to the used set
(`rt-multi-thread, net, macros, signal, time, fs`); feature-gate `object_store`
(`remote-backup` feature, on by default in releases) so minimal builds drop the
S3 machinery; verify `regex` is actually needed at the workspace level. Add a
binary-size gate to CI next to the existing 30 KB frontend bundle gate.

**Acceptance:** release assets ship with checksums + provenance + SBOM; a security
report can be filed privately; CI enforces MSRV + coverage + licenses; every doc
claim spot-checked in the audit is now true.

### Multi-Agent Dispatch Plan (Phases 26–32)

Designed for parallel agent execution. Rules of engagement: every agent writes
regression tests for its own fixes, runs `cargo test --workspace`, `cargo clippy`,
and `npm run type-check` before finishing, and never touches another track's files.

**Wave 1 — no interdependencies, dispatch simultaneously:**

| Agent | Scope | Tasks | Primary files |
|-------|-------|-------|---------------|
| A1 (Rust, DB) | Item persistence hotfix | 26.1, 26.2, 26.3 | `tack-db/src/repo/items.rs`, `tack-core/src/models.rs`, `tack-db/src/lib.rs` |
| A2 (Rust, security) | Restore integrity | 27.3, 27.4, 28.3 | `tack-api/src/remote_backup.rs`, `tack-api/src/server.rs` |
| A3 (Rust, security) | Endpoint hardening | 27.1, 27.2, 27.5, 26.5 | `handlers/alexa.rs`, `server.rs`, `handlers/import_linear.rs` |
| A4 (Rust, backup) | Sync correctness | 28.1, 28.4, 27.6, 27.7 | `server.rs`, `remote_backup.rs`, `handlers/settings.rs` |
| A5 (frontend) | Nav + vocab integrity | 26.4, 30.1, 30.2, 30.3 | `Breadcrumb.tsx`, `Sidebar.tsx`, `Layout.tsx`, `Sprints.tsx` |
| A6 (frontend) | Hygiene sweep | 30.4 | inline-style hex, dead deps, axe coverage |
| A7 (docs/CI) | OSS standards | 32.1–32.5 | `SECURITY.md`, `CONTRIBUTING.md`, `.github/`, `docs/` |

**Wave 2 — after Wave 1 merges:**

| Agent | Scope | Tasks | Depends on |
|-------|-------|-------|-----------|
| B1 (Rust) | Error + response typing | 29.1, 29.2 | A1 (touches `handlers/`) |
| B2 (Rust+data) | Generation-counter sync | 28.2, 28.5 | A2, A4 |
| B3 (Rust, templates) | Construction presets | 31.1 | none (waits only to avoid `templates.rs` churn) |
| B4 (frontend) | Template-first onboarding | 31.2, 31.3 | B3 (template data) |
| B5 (build) | Size guard | 32.6 | A4 (feature-gating `object_store`) |
| B6 (release) | Cut `v0.1.0-beta.7` | 26.6 | A1–A5 merged |

**Wave 3 — the SDD backbone (sequential inside the track):**

| Agent | Scope | Tasks | Depends on |
|-------|-------|-------|-----------|
| C1 (Rust) | utoipa spec + CI drift gate | 29.3 | B1 |
| C2 (frontend) | Generated TS types | 29.4 | C1 |
| C3 (docs) | Spec-generated API reference | 29.5 | C1 |
| C4 (QA) | Contract tests in CI | 29.6 | C1 |

**Definition of done for the whole cycle:** `v0.1.0-beta.7` tagged with all Wave 1
fixes; a SIP-panel construction project creatable in two clicks with correct
vocabulary end-to-end; two devices sharing a bucket cannot clobber each other;
`openapi.json` gates CI and generates the frontend types and the API docs; release
assets are signed and SBOM'd.

---

## Next — Agent-Factory Control Center (Phases 33–38, August 2026)

**Thesis:** Tack becomes the **control center for a factory of products** built by
governed agent fleets. [docket](https://github.com/yielab/docket) (folder
`~/Sites/rack-cli`) already runs the fleets — pods of Lead/Implementer/Reviewer/Tester
agents, per-project isolation, a policy chokepoint on every tool call, budgets, and a
hash-chained audit log. What it does not have is a plan of record, a roadmap, a board,
or a place to see cost-per-outcome. Tack is exactly that, and already carries the
primitives: a universal item model, a DAG, per-project workflows and vocabulary,
real-time `BoardEvent` push, and an external-link precedent (`github_links` +
`maybe_sync_github`) that this cycle copies wholesale.

**The architecture in one line:** _Tack holds desired state, docket executes, and a
reconciler in a new `tack-orch` crate closes the loop._ Intent flows **push** (Tack →
docket, synchronous, returns a run id); progress flows **pull** (a jittered poll loop,
Kubernetes-style). Pull is not a compromise — docket has no outbound webhook, and a
reconciler survives docket restarts, missed deliveries, and Tack downtime with no
queue and no replay logic.

```text
┌──────────────────────── Tack (control center) ───────────────────────┐
│  Fleet · Approvals inbox · Board/Timeline · Item agent-activity      │
│  tack-api    POST /api/items/{id}/dispatch                           │
│              POST /api/sprints/{id}/dispatch      (DAG-ordered)      │
│              POST /api/approvals/{token}                             │
│  tack-orch   ControlPlane trait ──► DocketAdapter                    │
│    dispatcher   item → task → run       (intent, synchronous)        │
│    reconciler   poll: /health /status.json /runs /approvals /metrics │
│                 → orch_* tables → workflow engine → BoardEvent       │
└────────────────────────────────┬─────────────────────────────────────┘
                                 │ HTTP + Bearer
┌────────────────────────────────▼─────────────────────────────────────┐
│  docket serve --dispatch    Lead → Implementer → Reviewer → Tester   │
│  gated turn loop · approval store · budgets · audit chain            │
└──────────────────────────────────────────────────────────────────────┘
```

### What docket exposes today (verified against `~/Sites/rack-cli/src/docket/serve.py`)

| Surface | Auth | Payload |
|---|---|---|
| `GET /status.json` | none | `{apiVersion, gateway, channels, agents[{id,name,kind,scope,model,lastActivity,costUsd,budgetUsd}], totalCostUsd}` |
| `GET /metrics` | none | Prometheus: `docket_agent_turns_total`, `docket_tool_calls_total`, `docket_approvals_total`, `docket_policy_hits_total`, `docket_turn_duration_seconds`, `docket_gateway_up` |
| `GET /health` | none | liveness |
| `GET /runs?project=`, `GET /runs/{id}` | Bearer | run registry — `source` (cli/webhook/schedule/sweep/mcp), `state` (queued/running/succeeded/failed/cancelled), hops, pids |
| `GET /approvals` | Bearer | pending records **including `context: {taskId, pipelineIndex}`** — this is what correlates an approval back to a Tack item |
| `POST /dispatch/{project}` | Bearer | binds pipeline `variables` from the JSON body, creates the run record **before** work starts, returns `{ok, run, project}` |
| `POST /approvals/{token}` | Bearer | `{action: grant\|deny}`, audit-logged with `channel="http"` |

### Two blocking gaps found in docket (fix upstream first)

1. **There is no HTTP endpoint to enqueue a task.** `POST /dispatch/{project}` dispatches
   an _existing_ queue (`effective_pipeline` → `resolve_variables` → `dispatch_pod`).
   Task creation lives only in `core/dispatch.py::enqueue_task`, reachable via the CLI
   or the `delegate` MCP tool. **Phase 35 cannot start until docket ships
   `POST /tasks/{project}`.** Likewise `docket add` (pod provisioning) is CLI-only,
   which blocks Phase 37.
2. **docket's turn loop builds its tool registry from built-ins only** — its agents
   _cannot_ call Tack's MCP server from inside a turn. So agents cannot self-report
   progress, and this cycle must not depend on it: reporting comes from the dispatch
   lifecycle (runs + traces), not from the agents. The `orch_tasks` link table makes
   agent self-reporting a drop-in addition the day docket closes that wire.

Both are tracked as a docket-side work package in `~/Sites/rack-cli/ROADMAP.md`
(Phase 20 — "Tack control-plane integration").

### Non-negotiable design rules for this cycle

- **Status transitions go through the workflow engine, never raw SQL.** The reconciler
  calls the same update path as a user drag, so WIP limits, explicit transitions
  (construction is linear!), `started_at`/`completed_at`, and parent auto-propagation
  all still fire. A rejected transition records an `orch_events` row of type
  `status_map_rejected` and **leaves the item alone** — it never forces the move.
- **Never hold a SQLite write transaction across an HTTP call to docket.** Tack is
  single-writer; "database locked" is already in the troubleshooting guide. Fetch →
  parse → short write txn.
- **Preserve docket's cost honesty.** docket reports _measured tokens_ and a
  _clearly labelled estimate_, and refuses to call an estimate spend. Tokens are the
  primary stored metric; the money column is named `cost_usd_estimated`, carries the
  pricing-snapshot date, and renders as "estimated" in the UI. Silently relabelling it
  as spend would be this cycle's worst failure mode.
- **A control-plane failure never fails a user request.** The reconciler is a background
  task with its own backoff, exactly like the existing due-soon and backup schedulers
  in [server.rs](../../../crates/tack-api/src/server.rs).

### Status of this cycle as of 2026-08-05

Built by a multi-agent run tracked in [`TODO.md`](../../../TODO.md); every card's
handoff note is in that file's §6.

| Phase | Status | Notes |
|---|---|---|
| 33 — Control-Plane Link (read-only) | ✅ **done** | Reconciler polls a live docket; Fleet view ships |
| 34 — Run Mirroring & Telemetry | ✅ **done** | Runs, approvals, traces, metrics, retention, realtime, Agent Activity UI |
| 35 — Dispatch | ✅ **done** | Item + DAG-ordered sprint dispatch, trust boundary, dispatch UI |
| 36 — Governance Surface | ✅ **done** | Approvals inbox + budget/policy panels. **No pause control** — docket exposes no pause/resume over HTTP, in either direction |
| 37 — Factory Provisioning | ✅ **done** | `POST /pods` had already shipped; the block was wrong. Provisioning wizard, rollback before the pod exists, never after |
| 38 — Unit Economics | ✅ **done** | Tokens, estimated cost, lead time, rework rate — with the comparisons the data cannot support deliberately not computed |

**Every card in this cycle is built.** Nothing is blocked.

**Corrections made during the run, worth carrying forward:**

- Phases 34 and 35 were marked blocked on docket endpoints that **had already
  shipped**. docket's own `ROADMAP.md` still lists them as `TODO` and is stale against
  its source; `src/docket/serve.py` is the authority. Re-verify before trusting any
  "blocked" marker.
- The `POST /tasks/{project}` response shape documented in `TODO.md` §1.4 was wrong;
  corrected after live verification.
- Migrations run to **029**, not 024 — see the corrected table below.

### Schema added this cycle (migrations 019–029)

| Migration | Table | Purpose |
|---|---|---|
| 019 | `control_planes` | `id, name, kind, base_url, token` (write-only over the API), `api_version, health, last_seen_at, consecutive_failures` |
| 020 | `orch_links` | Tack project ↔ docket pod: `project_id, control_plane_id, remote_project, pipeline_file, blueprint, auto_dispatch, budget_usd, status_map` (JSON) |
| 021 | `orch_tasks` | item ↔ docket task, **PK `(item_id, remote_task_id)`** — 1:N, an item can be redispatched. `remote_run_id, remote_status, attempt, tokens_in, tokens_out, cost_usd_estimated, dispatched_at, trusted` |
| 022 | `orch_runs` | mirror of `/runs` — `run_id` PK, `control_plane_id, remote_project, source, state, started_at, ended_at, error, item_id` |
| 023 | `orch_events` | append-only telemetry (hops, verdicts, rework, tool calls, `status_map_rejected`); index `(item_id, occurred_at)` |
| 024 | `orch_approvals` | mirror of `/approvals`, correlated to items via `context.taskId`; `token` PK |
| 025 | `orch_metrics` | mirror of docket's Prometheus `/metrics` scrape |
| 026 | `orch_events_daily` | per-day aggregate of purged `orch_events`. Keyed `(day, control_plane_id, event_type)` — **drops `item_id`**, so per-item history truncation is not recoverable from the aggregate |
| 027 | `orch_metrics_daily` | per-day aggregate of purged `orch_metrics`; non-finite samples counted but excluded from sum/min/max |
| 028 | `orch_trace_cursors` | resumption cursor per `(control_plane_id, remote_project)`, stored opaque |
| 029 | `items.source` | sticky provenance for the prompt-injection trust boundary; defaults to `unknown`, which resolves to untrusted |

Agent state is **not** denormalized onto `items` — the board query LEFT JOINs the latest
`orch_tasks` row. Revisit only if profiling says so.

### Phase 33 — Control-Plane Link (read-only) ✅ _done 2026-08-05_

**Goal:** See the whole agent fleet from inside Tack, with **zero write path** to
docket. Independently shippable and safe to run against a live fleet.

#### Task 33.1 — `crates/tack-orch` crate skeleton

New workspace member. Defines the `ControlPlane` **trait** (`health`, `status`,
`metrics`, `list_runs`, `get_run`, `list_approvals`, `list_tasks`) plus `OrchError`
(`thiserror`) and the shared DTOs every other task consumes. The trait is what makes
this a factory control center rather than a docket-specific dashboard — a future
GitHub-Actions or Temporal adapter drops in beside `DocketAdapter`. Register in the
workspace `Cargo.toml`; `tack-orch` may depend on `tack-core` and `tack-db`, never on
`tack-api`.

#### Task 33.2 — Migrations 019–024

Land all six tables in [migrations.rs](../../../crates/tack-db/src/migrations.rs) in one
change (single owner — this file is the cycle's biggest merge chokepoint). Follow the
`018_github_links` precedent. Every FK `ON DELETE CASCADE` from `items`/`projects`.

#### Task 33.3 — `DocketAdapter`

`crates/tack-orch/src/adapters/docket.rs`: async `reqwest` client implementing
`ControlPlane` against the table above. Bearer token on the authenticated routes only.
Per-request timeout (default 5s), typed deserialization, and a **Prometheus text
parser** for `/metrics` (no new dependency — the format is trivial; parse
`name{labels} value`). Tests run against a `wiremock` fixture server built from real
captured payloads.

#### Task 33.4 — `orch` repository module

`crates/tack-db/src/repo/orch.rs`: CRUD for `control_planes` and `orch_links`, plus
upsert helpers for the mirror tables (used from Phase 34). Register in
[repo.rs](../../../crates/tack-db/src/repo.rs).

#### Task 33.5 — Config + control-plane CRUD API

`TACK_ORCH_ENABLE` (default **false** — nothing in this cycle runs until it is on),
`TACK_ORCH_POLL_SECS` (default 10), `TACK_ORCH_EVENT_RETENTION_DAYS` (default 90) in
[config.rs](../../../crates/tack-api/src/config.rs) and `tack.toml`. New handler
`handlers/orch.rs` + routes `GET/POST /api/control-planes`, `GET/PATCH/DELETE
/api/control-planes/{id}`, `GET/PUT /api/projects/{id}/orch-link`. The docket token is
**write-only** over the API — returned as a `token_set: bool`, exactly like the S3
secret key in [settings.rs](../../../crates/tack-api/src/handlers/settings.rs).

#### Task 33.6 — Reconciler skeleton

`crates/tack-orch/src/reconciler.rs`: a `tokio` task per registered control plane,
jittered interval, exponential backoff, and a health state machine
(`healthy → degraded` after 3 consecutive failures `→ unreachable` after 10). Spawned
from [server.rs](../../../crates/tack-api/src/server.rs) alongside the existing
schedulers, and **only** when `TACK_ORCH_ENABLE=true`. This task polls `/health` and
`/status.json` only; Phase 34 adds the rest.

#### Task 33.7 — Fleet view (frontend, read-only)

`frontend/src/features/fleet/`: one row per product — Tack project, pod health, roster
(roles + models), last activity, burn vs budget, gateway state. Sourced from
`GET /api/control-planes` + `GET /api/fleet`. Empty state explains how to register a
control plane. Tokens only, WCAG AA, axe-clean — the design-system rules in
[frontend.md](developer/frontend.md) apply unchanged.

#### Task 33.8 — docket-side read endpoints

Upstream in `~/Sites/rack-cli`: `GET /tasks/{project}` (the pod's `TASK_LIST.json` as
JSON) and `GET /traces/{project}?since=` (trace events). See that repo's ROADMAP
Phase 20. Tack can ship 33.1–33.7 without these; 34.4 needs them.

#### Task 33.9 — Docs + OpenAPI

New `docs/book/src/user-guide/orchestration.md` and
`docs/book/src/developer/orchestration.md`; register both in `SUMMARY.md`. All new
endpoints annotated with `utoipa` so the Phase 29.3 CI drift gate covers them.

**Acceptance:** with `TACK_ORCH_ENABLE=true` and a live `docket serve`, the Fleet view
lists every pod with correct roles, models, and budget state; killing `docket serve`
flips the control plane to `degraded` then `unreachable` without a single failed Tack
request or a `database is locked` error; with the flag off, no reconciler task is
spawned and no new route accepts traffic.

### Phase 34 — Run Mirroring & Telemetry ✅ _done 2026-08-05_

**Goal:** Every run, hop, verdict, approval, and token is visible in Tack, live, and
attributable to an item.

#### Task 34.1 — `/runs` ingestion

Extend the reconciler: for each linked project, `GET /runs?project=` → upsert
`orch_runs`. State transitions emit an `orch_events` row and a `BoardEvent`. Runs are
correlated to items through `orch_tasks.remote_run_id` (populated in Phase 35; until
then runs mirror unattributed, which is fine and must not error).

#### Task 34.2 — `/approvals` ingestion

`GET /approvals` → upsert `orch_approvals`. Correlate to an item by reading
`record.context.taskId` and joining `orch_tasks.remote_task_id`. Records that don't
correlate are stored with `item_id = NULL` and still shown in the fleet-level inbox.

#### Task 34.3 — `/metrics` ingestion + rollup

Parse the Prometheus text into `orch_metrics` (one row per scrape per metric per label
set). Reuse the parser from 33.3 — do not write a second one.

#### Task 34.4 — Trace ingestion

`GET /traces/{project}?since=<cursor>` → `orch_events`. Map docket's event types
(`tool_call`, `approval_*`, `cost_charged`, `budget_exceeded`, `verification_failed`,
`tester_verdict_failed`, `rework_started`, `review_rejected`, `session_end`, …) to a
Tack-side taxonomy; store unknown types verbatim rather than dropping them, so a docket
upgrade that adds an event type degrades to "shown as-is".

#### Task 34.5 — Broadcast agent state

Add `BoardEvent::AgentRunUpdated { project_id, item_id, run_id, state }` and
`BoardEvent::ApprovalPending { … }` in
[websocket.rs](../../../crates/tack-api/src/handlers/websocket.rs). Frontend
`shared/realtime` handles both. This is what makes the board move by itself while a
fleet works.

#### Task 34.6 — Retention + daily rollup

`orch_events` and `orch_metrics` grow unbounded (docket has the identical open gap).
A daily task rolls raw rows into per-day aggregates **before** deleting anything older
than `TACK_ORCH_EVENT_RETENTION_DAYS`, so the Phase 38 unit-economics history outlives
the raw events. Retention is not optional and not a follow-up.

#### Task 34.7 — Tack's own `GET /api/metrics`

Prometheus text merging Tack's work-tracking metrics (items by status, cycle time,
throughput) with the mirrored docket ones, so one Grafana scrape covers the factory.
Unauthenticated like docket's, but bound by the same CORS/bind warnings as Phase 27.2.

#### Task 34.8 — Item "Agent Activity" tab

In `frontend/src/features/item-detail/`: a timeline of hops, tool calls, verdicts,
rework cycles, approvals, and tokens/estimated cost for the item's `orch_tasks`. This
is where "running / finished / any progress" actually lives for a single work item.

#### Task 34.9 — Agent badges on Board, List, Table

A compact state chip (queued / running / waiting-approval / failed) driven by the
`orch_tasks` LEFT JOIN. One shared component; no per-view reimplementation.

**Acceptance:** dispatching a pod from the docket CLI (no Tack involvement) surfaces
the run in Tack within one poll interval; the item timeline shows every hop with token
counts; every money figure in the UI reads "estimated"; a 90-day-old event is gone but
its day's aggregate survives.

### Phase 35 — Dispatch: Tack drives the factory ✅ _done 2026-08-05 — the `POST /tasks` block was incorrect; docket had already shipped it_

**Goal:** Moving a card dispatches a governed agent pipeline.

#### Task 35.1 — docket-side `POST /tasks/{project}`

Upstream. Body `{description, priority, trusted}` → `dispatch.enqueue_task` → returns
`{taskId}`. Same Bearer auth, same audit logging, and it must honour the `pre_input`
policy gate (a `block` verdict returns 4xx with the policy id).

#### Task 35.2 — `status_map` schema + validation

`orch_links.status_map` shape:

```json
{ "dispatch_from": ["Ready"], "on_running": "In Progress",
  "on_waiting_approval": "Blocked", "on_succeeded": "Done",
  "on_failed": "Blocked", "on_cancelled": "Ready" }
```

Validated **at save time** against the project's `WorkflowConfig` — every named status
must exist. Hardcoding "running → In Progress" would break the construction, personal,
and homework presets, which is the whole point of Tack's vocabulary system.

#### Task 35.3 — `POST /api/items/{id}/dispatch`

Resolve the link → enqueue the task on docket → `POST /dispatch/{project}` with the
item's fields bound as pipeline `variables` → persist `orch_tasks` (task id + run id)
→ apply `on_running`. Idempotent per `(item_id, attempt)`.

#### Task 35.4 — `POST /api/sprints/{id}/dispatch` (DAG-ordered)

**The highest-value primitive in this cycle** — this is what "run a product line" means,
and it has no precedent in either codebase, so it gets the most design attention.
Topologically sort the sprint's items using
[dependency.rs](../../../crates/tack-core/src/dependency.rs), enqueue in order, and hold
an item until its dependencies reach a `done`-category status. Respect the pod's WIP by
capping in-flight dispatches per project. A cycle in the graph is already impossible
(the DAG validator prevents it) — assert it anyway and fail loudly.

#### Task 35.5 — Auto-dispatch hook

In [items.rs](../../../crates/tack-api/src/handlers/items.rs), beside the existing
`maybe_sync_github` call: when an item enters a `dispatch_from` status and
`orch_links.auto_dispatch` is true, fire the dispatcher. Best-effort like the GitHub
hook — a dispatch failure logs and records an event, never fails the user's PATCH.

#### Task 35.6 — Reconciler applies `status_map`

Terminal run states drive the item's status **through the workflow engine**. On
rejection, record `status_map_rejected` with the workflow's reason and surface it in
the UI. See the non-negotiables above.

#### Task 35.7 — Untrusted-source flag

GitHub/Linear-imported item titles and descriptions are **attacker-authored text that
becomes agent input**. Mark imported items `source: imported` at import time and pass
`trusted: false` on enqueue so docket's `pre_input` policy hook treats them as
untrusted. This is a real prompt-injection boundary, not a formality.

#### Task 35.8 — Dispatch UI

"Dispatch to agents" on the item detail and the board card menu; "Run sprint" on the
Sprints view with a dependency-order preview and an in-flight cap control.

#### Task 35.9 — Security gating + backup exclusion

Every dispatch route is gated behind `TACK_ORCH_ENABLE` **and** a configured control
plane token; off by default. The docket token must be **excluded from backup bundles**
— the S3 secret key already leaked this way once (finding #7 of the July audit). Ship
the exclusion and its regression test in the same commit.

**Acceptance:** dragging a card to Ready enqueues a docket task, the pipeline runs
Lead → Implementer → Reviewer → Tester, and the card lands in Done by itself; a
construction project with a linear workflow refuses an illegal auto-move and shows why;
`GET /api/backup` contains no control-plane token; with `TACK_ORCH_ENABLE` unset, every
dispatch route 404s.

### Phase 36 — Governance Surface ✅ _done 2026-08-05 — no pause control: docket exposes none over HTTP_

**Goal:** Approve, pause, and audit the fleet from the board.

#### Task 36.1 — Approvals proxy

`POST /api/approvals/{token}` `{action}` → docket's `POST /approvals/{token}`. Requires
a **separate** `TACK_ORCH_APPROVAL_TOKEN`; with only `TACK_API_TOKEN` set, the
approvals surface is read-only. Rationale: the Tack token is a single shared secret,
and granting a docket approval is a genuinely higher-privilege action than editing a
card.

#### Task 36.2 — Approvals inbox

A fleet-wide page: every pending approval across every pod, with the requesting agent,
the action text, the correlated item, and grant/deny. Ordered oldest-first — docket's
approvals **fail closed on timeout**, so latency here has a real cost.

#### Task 36.3 — Budget & pause visibility

Per-pod burn vs cap from `/status.json`, a warning band, and a clear "this pod is
budget-paused; its queue is refusing tasks" state with the `docket profile <id>
--resume` remedy spelled out. Tack does not clear the pause itself in this phase.

#### Task 36.4 — Policy & audit panel

Denial rate, policy hits by id, approvals by channel, tool-call volume — all from
`/metrics`. Link out to `docket audit verify` rather than reimplementing chain
verification in Rust.

#### Task 36.5 — docket-side `channel="tack"`

Upstream: accept and record a `tack` approval channel so the audit chain's provenance
stays honest instead of every board decision masquerading as `http`.

**Acceptance:** a gated `git push` from an Implementer appears in the Tack inbox within
one poll interval, granting it from Tack resumes the pipeline, and
`docket audit verify` shows the entry tagged `channel="tack"`.

### Phase 37 — Factory Provisioning ✅ _done 2026-08-05 — the block was incorrect; `POST /pods` had shipped_

**Goal:** One click creates a product: a Tack project **and** its governed pod,
pipeline, verify command, and budget. This is what makes it a factory rather than a
very good dashboard.

#### Task 37.1 — `orchestration` block on templates

Extend the template payload
([templates.rs](../../../crates/tack-db/src/repo/templates.rs)) with an optional
`orchestration` object: docket blueprint (`software` / `research` / `content` / `ops` /
`agentic-product`), pipeline YAML reference, `verifyCmd`, budget cap, default
`status_map`, pod shape (`--pod full` / `--with`). Backwards compatible — templates
without it behave exactly as today.

#### Task 37.2 — Provisioning flow

`POST /api/projects/from-template/{id}` gains `provision_pod: true` → create the Tack
project, then the docket pod, pipeline, budget, and `orch_link`, in that order, with
**rollback on partial failure** (a Tack project with a half-created pod is worse than
no project). Requires the upstream provisioning endpoint from 37.5.

#### Task 37.3 — Pipeline library

Store validated docket pipeline YAML in Tack (`pipelines` table or a template field),
validated by round-tripping through docket's own `pipeline validate` rather than
reimplementing the schema in Rust — a second validator would drift.

#### Task 37.4 — New-product wizard

Product type → template → pod shape → budget → verify command → create. Reuses the
Phase 31 template-first onboarding surface.

#### Task 37.5 — docket-side pod provisioning over HTTP

Upstream: `docket add` is CLI-only. Needs `POST /pods` `{project, path, blueprint,
pod, budget, verifyCmd}` returning the pod roster.

**Acceptance:** from an empty install, one wizard pass yields a board with the right
vocabulary **and** a `docket list` entry with the right roles, models, budget, and
verify command; a forced failure mid-provision leaves neither a stray project nor a
stray pod.

### Phase 38 — Unit Economics & Optimization ✅ _done 2026-08-05_

**Goal:** Answer "which product lines are economical to build with agents?" — the
question a factory operator actually has, and one nobody can currently answer.

#### Task 38.1 — Core metrics

Per completed item: tokens in/out, estimated cost, **agent lead time**
(`dispatched_at → completed_at`) versus Tack's existing human `started_at →
completed_at`, and **rework rate** (`rework_started` + `verification_failed` +
`tester_verdict_failed` events per task). Rework rate is the agent-fleet equivalent of
a defect escape rate and is the earliest regression signal available.

#### Task 38.2 — Product-line comparison

Slice every metric by `project_type` and `item_type`. Cost-per-completed-item by
product line is the headline number of the whole cycle.

#### Task 38.3 — Model right-sizing feedback

Export per-role outcome quality (rework rate, verdict pass rate) against the model
docket used, in a shape docket's role→model policy can consume. This closes the
optimization loop: Tack observes outcomes, docket adjusts models. Export only in this
phase — Tack does not mutate docket's model policy.

#### Task 38.4 — Analytics export

CSV/JSON export of the factory analytics, reusing the existing export machinery in
[export.rs](../../../crates/tack-api/src/handlers/export.rs).

**Acceptance:** a dashboard answers "what did each product line cost, in tokens and
estimated dollars, per shipped item, and how often did agents need rework?" — with
every dollar figure labelled an estimate and carrying its pricing-snapshot date.

### Multi-Agent Dispatch Plan (Phases 33–38)

The full wave plan, per-agent task cards, file-ownership map, and rules of engagement
live in **[TODO.md](../../../TODO.md)** at the repo root — written to be picked up cold
by parallel Sonnet agents.

**Sequencing at a glance:** Task 33.1 + 33.2 are a blocking Wave 0 (they define the
crate, the trait, and every table the rest of the cycle writes to). Phase 33's
remaining tasks fan out in Wave 1. Phase 34 is the widest parallel wave. Phase 35 is
blocked on the upstream docket endpoint and is the only phase with real cross-task
coupling. Phases 36–38 are largely independent tracks.

**Definition of done for the whole cycle:** from a single Tack install, an operator can
provision a new product with its governed agent pod in one action, watch its roadmap
execute itself on the board in real time, approve gated actions from the approvals
inbox, and answer what the product line cost per shipped item — with every cost figure
honestly labelled an estimate.

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

| Area | Gap | Tracked in |
|---|---|---|
| Item updates | `sprint_id` / `due_date` / `estimate_unit` not persisted by `update_item`; `started_at`/`completed_at` never set on ordinary status moves | Phase 26 (blocker) |
| Security | Alexa endpoint lacks Amazon cert-chain validation; no warning on unauthenticated non-loopback bind; tar-slip + unverified sha256 in backup restore; S3 secret embedded in backup bundles | Phase 27 |
| Backup as sync | No conflict detection between installs; scheduler ignores UI-saved settings; non-atomic restore swap | Phase 28 |
| API contract | No OpenAPI spec; hand-maintained TS types and API docs have drifted from the router; two error JSON shapes; item lists truncate at 100 without a total | Phase 29 |
| Vocabulary | Global "+ New" modal, Sprints view, tabs/palette/first-run guide hardcode "sprint"/"Story Points" | Phase 30 |
| Coverage reporting | 168 Vitest unit tests and a Playwright E2E suite ship; automated coverage thresholds in CI are not yet enforced | Phase 32 |
| Release integrity | No checksums/SBOM/provenance on release assets; SECURITY.md routes disclosure through public issues | Phase 32 |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported | Future |
| Auth | No multi-user auth (by design for v1) | Future |

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
