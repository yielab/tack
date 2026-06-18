# Roadmap

**Current version:** 2.0.0  
**Status:** All thirteen engineering phases complete. The product is feature-complete for the
solo-dev / small-team use case. Future work is additive.

---

## Completed

### Phase 0 — Repo Hygiene
Dead code removed. Docs consolidated. Status and architecture accurately documented.

### Phase 1 — CI & Security Baseline
- GitHub Actions: fmt + clippy + test + frontend typecheck + build + bundle size gate
- CORS allow-list (`FLEXPM_ALLOWED_ORIGINS`)
- Global body-size limit + 50 MB upload cap
- Optional Bearer token auth (`FLEXPM_API_TOKEN`)
- Input validation via `validator` on all Create/Update DTOs

### Phase 2 — Architecture Correctness
- Workflow validation moved into `flexpm-core` (was in DB layer)
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
- Single-binary: `--features embed-spa` embeds SPA into the release binary (~5 MB)

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
- `CustomFieldDefinition::validate_value()` in `flexpm-core`: enforces type (string, number, boolean, date, URL, select option membership, multi-select array), plus JSON-configured rules (`pattern`, `min_length`, `max_length`, `min`, `max`, `max_items`)
- `set_field_value` handler returns `422 Unprocessable Entity` on validation failure instead of silently storing bad data; 28 new unit tests in `flexpm-core`
- `POST /api/alexa` custom-skill endpoint: maps voice intents onto existing item/workflow logic
  - **AddTaskIntent** — creates item at the initial workflow status
  - **ListTasksIntent** — speaks open-item count and first few titles
  - **CompleteTaskIntent** — moves matching open item to first Done status; enforces transition rules and WIP limits; propagates parent auto-completion
- Alexa endpoint authentication: constant-time skill-ID comparison + ±150 s timestamp replay guard
- Vocabulary-aware spoken responses (construction projects say "Work Order", not "task")
- Endpoint is exempt from the Bearer-token gate; disabled (404) when `FLEXPM_ALEXA_SKILL_ID` is unset
- Board view applies per-board item filters on fetch (replaces TODO no-op)
- 13 new Alexa handler tests covering verification, all intents, and edge cases

### Phase 9 — Full Integration Test Coverage

- API integration tests expanded from 16 → 36 (sprints, roles, comments, dependencies, search, JSON/CSV export, item update/delete)
- Frontend utility tests: vocab resolution, lens persistence, keyboard manager, optimistic-update rollback — total 144 Vitest tests across 21 files
- Vitest config cleaned up for vite 8 + OXC pipeline (removed stale esbuild shim)

### Phase 10 — Webhook Notifications

- `FLEXPM_WEBHOOK_URL` — when set, POSTs JSON events for every item create/update/delete, sprint status transition, and items due within the next hour
- `FLEXPM_WEBHOOK_SECRET` — optional HMAC-SHA256 signing; adds `X-FlexPM-Signature: sha256=<hex>` so receivers can verify authenticity
- Background task fires `item.due_soon` once per hour for incomplete items whose `due_date` falls in the next 60 minutes
- Delivery is fire-and-forget (tokio::spawn); errors are logged but never fail the originating request
- All event payloads include `event`, `timestamp`, and `project_id` fields

### Phase 11 — GitHub Issues Import

- `POST /api/projects/{id}/import-github` — fetches issues from any public (or token-accessible private) GitHub repository and creates FlexPM items
- Request body: `repo` (owner/repo or full URL), `token` (optional PAT), `import_closed` (default false), `label_filter` (optional label allow-list)
- Pull requests are silently skipped; closed issues map to the first Done-category status
- Pagination handled automatically (100 issues/request until the last page)
- Response includes `created`, `skipped`, and `rate_limit_remaining` counts
- 7 unit tests for URL parsing covering all input forms

### Phase 12 — Linear Import

- `POST /api/projects/{id}/import-linear` — fetches issues from Linear's GraphQL API and creates FlexPM items
- Request body: `api_key` (Linear personal API key), `team_id` (optional slug or ID), `project_id` (optional; overrides `team_id`), `import_completed` (default false), `label_filter` (optional label allow-list)
- Priority mapping: Urgent→Critical, High→High, Medium→Medium, Low→Low; No priority→unset
- Cursor-based pagination (50 issues per request)
- Response includes `created` and `skipped` counts
- 6 unit tests covering filter generation, cursor inclusion, project-vs-team precedence, priority mapping, and cursor injection sanitisation

### Phase 13 — Remote Cloud Backup (S3-Compatible)

**Goal:** Let a user back up their entire FlexPM instance to any S3-compatible object
store (Cloudflare R2 / Backblaze B2 free tiers, AWS S3, or a self-hosted MinIO) and
restore it on a different machine — so the same data is reusable across local
installations. This is **snapshot replication**, not a live shared database: semantics
are "one active writer at a time, last upload wins." It reuses the existing
`VACUUM INTO` backup flow ([backup.rs](../../../crates/flexpm-api/src/handlers/backup.rs))
and the existing staged-restore-on-startup mechanism
([main.rs](../../../crates/flexpm-api/src/main.rs)).

**Design decisions (locked):**

- Provider-agnostic via the [`object_store`](https://docs.rs/object_store) crate — a
  free provider and a custom server are the same thing (an endpoint + access keys),
  so there is exactly one code path. No per-provider SDKs.
- The backup bundle **includes uploaded attachments**, not just the database, because
  attachments live on disk in `FLEXPM_STORAGE_DIR` (not in SQLite) and would otherwise
  be lost on cross-machine restore.
- Keep it simple: no incremental/delta backups, no encryption in v1 (documented as a
  follow-up), no multi-writer conflict resolution.

#### Task 1 — Dependencies

Add to the workspace `Cargo.toml` `[workspace.dependencies]` and wire into
`crates/flexpm-api/Cargo.toml`:

- `object_store = { version = "0.11", features = ["aws"] }` — the `aws` feature speaks
  the S3 API and supports custom endpoints (R2/B2/MinIO).
- `tar = "0.4"` and `zstd = "0.13"` — to bundle DB + attachments into one compressed archive.
- `bytes` is already transitively available via axum; add it explicitly if needed.

#### Task 2 — Bundle format

A single artifact `flexpm-backup-<UTC-RFC3339>.tar.zst` containing:

- `database.db` — the `VACUUM INTO` snapshot (reuse the existing helper in
  `handlers/backup.rs`; factor the snapshot-to-bytes logic into a reusable function).
- `attachments/…` — a recursive copy of `FLEXPM_STORAGE_DIR` (skip if the dir is empty
  or unset).
- `manifest.json` — `{ "format_version": 1, "created_at": <rfc3339>, "migration_version": <u32>, "db_sha256": <hex>, "install_id": <uuid>, "item_count": <u64> }`.
  `migration_version` = the count of applied rows in the `_migrations` table.
  `install_id` = a UUID persisted once in a new `app_meta` row (or a sidecar file next
  to the DB); generate on first run if absent.

#### Task 3 — New module `crates/flexpm-api/src/remote_backup.rs`

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

#### Task 4 — Config ([config.rs](../../../crates/flexpm-api/src/config.rs))

Add fields to `AppConfig`, loaded from `flexpm.toml` and `FLEXPM_BACKUP_*` env vars
(same precedence pattern as existing config):

| Env var | TOML key | Type | Default | Notes |
| --- | --- | --- | --- | --- |
| `FLEXPM_BACKUP_ENDPOINT` | `backup.endpoint` | `Option<String>` | none | e.g. `https://<acct>.r2.cloudflarestorage.com` |
| `FLEXPM_BACKUP_BUCKET` | `backup.bucket` | `Option<String>` | none | required to enable |
| `FLEXPM_BACKUP_REGION` | `backup.region` | `String` | `auto` | R2 uses `auto` |
| `FLEXPM_BACKUP_ACCESS_KEY` | `backup.access_key` | `Option<String>` | none | |
| `FLEXPM_BACKUP_SECRET_KEY` | `backup.secret_key` | `Option<String>` | none | never log |
| `FLEXPM_BACKUP_PREFIX` | `backup.prefix` | `String` | `flexpm` | object key prefix |
| `FLEXPM_BACKUP_INTERVAL_SECS` | `backup.interval_secs` | `Option<u64>` | none | omit = manual only |
| `FLEXPM_BACKUP_RETENTION` | `backup.retention` | `usize` | `10` | keep newest N |

Add a helper `AppConfig::remote_backup_enabled() -> bool` (true when bucket + access
key + secret key are all set). Document every key in `CLAUDE.md` and the deployment guide.

#### Task 5 — Endpoints ([router.rs](../../../crates/flexpm-api/src/router.rs) + `handlers/backup.rs`)

All gated behind `remote_backup_enabled()` (return `409 Conflict` with a clear message
when disabled). All subject to the existing Bearer-token middleware.

- `POST /api/backup/remote` → `create_bundle` + `upload` + `prune`; returns the new
  `BackupManifest` as JSON.
- `GET  /api/backup/remote` → `list`; returns `[BackupManifest]` newest-first.
- `POST /api/backup/remote/restore` → body `{ "key": "<optional; defaults to latest>" }`;
  download, **validate `migration_version` ≤ the running binary's migration count**
  (reject newer snapshots with `409` to prevent corruption), then stage (Task 6).
  Returns `{ "staged": true, "restart_required": true }`.

#### Task 6 — Atomic staged restore (extend [main.rs](../../../crates/flexpm-api/src/main.rs))

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

#### Task 8 — CLI ([flexpm-cli/src/main.rs](../../../crates/flexpm-cli/src/main.rs))

Thin wrappers over the new endpoints (CLI talks HTTP, never the DB directly):

- `flexpm backup --remote` → `POST /api/backup/remote`, print manifest.
- `flexpm backups` → `GET /api/backup/remote`, print a table (date, size, item count, key).
- `flexpm restore --remote [--key <key>]` → `POST /api/backup/remote/restore`; print the
  "restart the server to apply" notice. Support `--json` like every other command.

#### Task 9 — Tests

- `remote_backup.rs` unit tests: bundle round-trips (create → unpack → identical DB bytes +
  attachment tree), manifest serialization, prune keeps newest N, version-guard logic.
- Handler tests: `409` when disabled; restore rejects a manifest whose `migration_version`
  exceeds the local count. Use a mock/in-memory `ObjectStore` (object_store ships an
  in-memory backend) so no network is required.
- CLI test: arg parsing for the new `--remote`/`--key` flags.

#### Acceptance criteria

- With env vars pointing at any S3-compatible bucket, `flexpm backup --remote` uploads a
  `.tar.zst` bundle + sidecar manifest; `flexpm backups` lists it.
- On a second, empty install pointed at the same bucket, `flexpm restore --remote`
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

No phases are currently planned. The product is feature-complete for the solo-dev /
small-team use case.

### Future / Optional

#### Multi-User / Auth
The current design is explicitly local-only and single-user (one shared token, no per-user accounts or identities). The API token (`FLEXPM_API_TOKEN`)
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
