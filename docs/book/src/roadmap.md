# Roadmap

**Current version:** 0.1.0-beta.6 (unreleased work pending → `v0.1.0-beta.7`)  
**Status:** All thirteen engineering phases complete, plus competitive/growth phases
20 (MCP server), 22 (dev-native CLI), 23 (Table view), 24 (positioning & presets),
and 25 (local-first). A full-repo audit (July 2026) produced the **audit-driven
Phases 26–32**, which are now **implemented and verified green** (244 Rust tests,
169 Vitest, clippy clean, frontend builds) — see the status board below. The work
is staged for release as `v0.1.0-beta.7`.

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
