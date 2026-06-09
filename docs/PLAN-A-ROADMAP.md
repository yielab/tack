# FlexPM — Plan A Engineering Roadmap & SOP

> **Audience:** any developer or AI agent picking up a single task.
> **Rule:** every task below is self-contained. It states *why*, *which files*, *exact steps*, *acceptance criteria*, and *required tests*. Do not start a task until its `Depends on` tasks are merged.
> **Status of this doc:** canonical. If reality and this doc disagree, fix the doc in the same PR.

---

## 1. North Star (the scope we committed to)

FlexPM is a **local-first, single-binary, developer-oriented project management tool**. Its moat is **configurable vocabulary + workflow** ("git-style PM for people who hate Jira"), a great **CLI**, and **speed**.

**We are NOT building:** multi-tenant SaaS, user accounts with passwords, org/permission hierarchies, or real-time multi-user collaboration. Those are explicitly out of scope for Plan A. If a task implies them, it's the wrong plan.

**Operating model:** one person (or a trusted tiny crew on a LAN) runs one binary against one SQLite file. "Who" is captured as a lightweight free-text `assignee`, not an authenticated user.

---

## 2. Engineering Principles (apply to every task)

1. **Lightweight first.** No new heavy dependency without justification in the PR description. Binary stays ~5 MB; frontend JS stays < 60 KB gzipped. Measure before/after.
2. **Business rules live in `flexpm-core`.** Handlers and the CLI are thin transports. If a rule can be enforced in core, it must be.
3. **Pure core, I/O at the edges.** `flexpm-core` keeps zero I/O. Persistence in `flexpm-db`, transport in `flexpm-api`/`flexpm-cli`.
4. **Secure by default.** Bind `127.0.0.1`, deny by default on CORS, cap request bodies, validate all input at the boundary, never log secrets or PII.
5. **Every behavior change ships with a test** at the lowest layer that can prove it (see §3).
6. **No dead code in `main`.** No `.backup`/`.old` files, no commented-out blocks, no unused enum variants representing unbuilt features.
7. **Scalable-enough.** SQLite single-writer is accepted. Design queries and pagination so a single project with 50k items stays responsive. Don't add a distributed-systems tax we don't need.

---

## 3. The Testing System

### 3.1 Test pyramid

| Layer | Where | Tooling | What it proves |
| --- | --- | --- | --- |
| **Unit** | `flexpm-core/src/**` `#[cfg(test)]` | std `#[test]`, `assert_matches` | Workflow transitions, WIP limits, dependency DAG, vocabulary — pure logic, no DB |
| **Repository/integration** | `flexpm-db/tests/` | `#[tokio::test]` + `sqlite::memory:` | CRUD, migrations apply cleanly, FK/cascade behavior, FTS |
| **API/handler** | `flexpm-api/tests/` | `axum` `Router` + `tower::ServiceExt::oneshot` | Status codes, error mapping, validation, auth-token gate, body limits |
| **Frontend unit** | `frontend/src/**.test.tsx` | Vitest + `@solidjs/testing-library` | Component/store logic, optimistic update reducers |
| **E2E (smoke)** | `frontend/e2e/` | Playwright against built binary + SPA | Critical paths: create project → add item → drag status → reload |

### 3.2 Shared test helpers

- **`flexpm-db/tests/common/mod.rs`** — `setup_test_db()`, `create_test_workspace()`, `make_project()`, `make_item()` ✅
- **`flexpm-api/tests/common/mod.rs`** — `test_app()`, `test_app_with_config()` returning a wired in-memory router ✅
- Name tests `fn <unit>_<condition>_<expected>()`. One assertion-of-intent per test.

### 3.3 Coverage & gates

- Tool: `cargo llvm-cov`. Gate: `flexpm-core` ≥ 85% lines, `flexpm-db` + `flexpm-api` ≥ 70% combined. CI fails below threshold.
- Every bugfix PR must add a regression test that fails before the fix.

### 3.4 Definition of Done (DoD) for ANY task

- [ ] Code + tests in the same PR; tests fail without the change.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` all green.
- [ ] Frontend (if touched): `npm run type-check` + `npm run build` green.
- [ ] No new dependency without a one-line justification + size impact.
- [ ] Docs touched if behavior/endpoints changed.

---

## 4. Phases & Task Backlog

Effort key: **S** ≤ half day · **M** ≈ 1–2 days · **L** ≈ 3–5 days.

---

### PHASE 0 — Repo hygiene & truth ✅ DONE

#### ✅ T-001 · Delete committed dead code · S

Deleted: `List.tsx.backup`, `List.backup2.tsx`, `List.old.tsx`, `Sidebar.backup.tsx`, `CreateItemModal.tsx.backup`.

#### ✅ T-002 · Finish documentation consolidation · S

Docs reduced to 6 canonical files in `docs/`. Secondary docs moved to `archived-docs/`. Transient session files deleted.

#### ✅ T-003 · Rewrite status to reflect reality · S

`docs/PROJECT-STATUS.md` rewritten with real counts (42 tests, 59 routes, no auth). "Jira parity" language removed from `README.md`.

---

### PHASE 1 — Testing, CI, and security baseline ✅ DONE

#### ✅ T-101 · CI pipeline · M

`.github/workflows/ci.yml` + `rust-toolchain.toml`. Jobs: `rust` (fmt + clippy + test) and `frontend` (type-check + build).

#### ✅ T-102 · Test harness & fixtures · M

`crates/flexpm-db/tests/common/mod.rs` and `crates/flexpm-api/tests/common/mod.rs` with factory builders and `test_app()`. Fixed 3 broken `CreateProjectTemplate` struct usages in integration tests.

#### ✅ T-103 · Lock down CORS, body limits, security headers · M

`CorsLayer::permissive()` replaced with allow-list from `FLEXPM_ALLOWED_ORIGINS`. Global `DefaultBodyLimit::max(2MB)`; upload route gets `50MB`. Security headers: `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options`.

#### ✅ T-104 · Optional API token for LAN exposure · M

`FLEXPM_API_TOKEN` env var. `src/middleware.rs` with constant-time Bearer check. Health endpoint always public.

#### ✅ T-105 · Input validation at the boundary · M

`validator = "0.20"` added. All `Create*/Update*` DTOs derive `#[derive(Validate)]`. All 7 write-endpoint handlers call `input.validate()?`.

---

### PHASE 2 — Architectural correctness ⬡ IN PROGRESS

#### ✅ T-201 · Move workflow/WIP validation into `flexpm-core` · L

- **Why:** The parent-auto-complete decision lives in the API handler, not core. The CLI's `update_item` bypasses all handler-level logic. Business rules belong in core so every transport enforces them identically.
- **Files:** `crates/flexpm-core/src/workflow.rs` (add unit tests + `parent_auto_complete` logic), `crates/flexpm-api/src/handlers/items.rs` (simplify by calling core), `crates/flexpm-db/src/repo/items.rs` (add `siblings_all_done()` helper).
- **Steps:**
  1. In `workflow.rs`, add and test: `validate_transition`, `check_wip_limit`, `find_done_status` — already exist as methods but need thorough unit-test coverage. Add `should_complete_parent(all_siblings_done: bool) -> bool` pure function.
  2. In `repo/items.rs`, add `siblings_all_done(pool, parent_id, done_status) -> bool`.
  3. In `handlers/items.rs`, replace the inline parent-auto-complete block with: call `repo.siblings_all_done()` → call `workflow::should_complete_parent()` → if true, call `repo.update_item(parent_id, done_status)`.
  4. Write unit tests in core for all three workflow functions across all presets (scrum, kanban, construction).
- **Acceptance:** Construction workflow: `validate_transition("Permit","Handover")` returns `Err`. Scrum: WIP at limit returns `Err`. Parent auto-complete fires when all siblings done.
- **Tests (unit, core):** full transition matrices for each workflow preset; WIP at/below/over limit; parent-complete with 0/1/all siblings done.
- **Depends on:** T-102 ✅

#### ✅ T-202 · Eliminate the dual board system · L

- **Why:** Migration 009 (`board_views` table + `handlers/board.rs`) and migration 013 (`boards` table + `boards_multi.rs`) model the same concept. The legacy one is dead weight.
- **Decision:** Keep `boards` (multi-board), retire `board_views`.
- **Files:** `crates/flexpm-api/src/handlers/board.rs` (delete), `router.rs` (remove legacy board routes), `crates/flexpm-db/src/migrations.rs` (add migration 014 to drop `board_views`), `crates/flexpm-db/src/repo/boards.rs` (add `ensure_default_board_exists` migration helper), frontend `Board.tsx` (point to `/api/boards/{id}/view`).
- **Steps:**
  1. Add migration `014_consolidate_boards`: for each project that has no row in `boards`, create a default board from its `workflow.statuses`. Drop `board_views` table.
  2. Remove `handlers/board.rs`, remove its routes from `router.rs` (the 3 legacy board routes + `/board/live` WebSocket).
  3. Move the WebSocket endpoint to sit under the multi-board system: `GET /api/projects/{id}/boards/live`.
  4. Update frontend to use `/api/projects/{id}/boards` (list) and `/api/boards/{id}/view` (get board columns).
  5. Delete `board_views` related code from `repo.rs`.
- **Acceptance:** `GET /api/projects/{id}/board` returns 404. `GET /api/boards/{id}/view` returns grouped items. WebSocket still fires on item updates.
- **Tests (integration):** Migration backfills default board for a legacy project without a boards row; `get_board_view` returns items grouped by status.
- **Depends on:** T-101 ✅, T-102 ✅.

#### ✅ T-203 · CLI talks to the API, not the DB · L

- **Why:** `flexpm-cli/src/main.rs:103` opens SQLite directly, bypasses validation, and cannot target a running server/Docker container.
- **Files:** `crates/flexpm-cli/src/main.rs`, new `crates/flexpm-cli/src/client.rs`, new `crates/flexpm-cli/src/config.rs`.
- **Steps:**
  1. Add `reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"], default-features = false }` to `flexpm-cli/Cargo.toml`. Justify: CLI needs HTTP; rustls-tls avoids OpenSSL dep; blocking API keeps CLI code simple. Remove `sqlx` and `flexpm-db` deps from CLI.
  2. `config.rs`: read `base_url` (default `http://127.0.0.1:3210`) and `token: Option<String>` from `~/.flexpmrc` (TOML) or `FLEXPM_API_URL` / `FLEXPM_API_TOKEN`.
  3. `client.rs`: thin wrapper over `reqwest::blocking::Client` that adds `Authorization` header when token is set, maps HTTP errors to `anyhow::Error` with helpful messages.
  4. Convert each command to an API call: `init` → `POST /api/projects`, `add` → `POST /api/projects/{id}/items`, `list` → `GET /api/projects/{id}/items`, `move` → `PATCH /api/items/{id}`, `board` → `GET /api/boards/{id}/view`, `search` → `GET /api/search?q=`.
  5. Add `--json` flag to all commands; human-readable table output by default.
  6. Implement `sprint` subcommands via API.
- **Acceptance:** `flexpm-cli list --project <id>` against a running server returns same items as the web UI. Invalid transition returns the server's error message.
- **Tests:** unit tests using `wiremock` or `mockito` against each command; CI smoke test: boot API binary + run `init`+`add`+`list`.
- **Depends on:** T-201, T-104 ✅.

#### ✅ T-204 · Implement import · M

- **Why:** `POST /api/projects/import` is a stub. Shipping a labeled stub is dishonest.
- **Files:** `crates/flexpm-api/src/handlers/export.rs`, `crates/flexpm-db/src/repo/projects.rs`.
- **Steps:**
  1. Define an `ImportPayload` struct matching the JSON export schema (project metadata + items + sprints + custom fields + dependencies).
  2. In a single DB transaction: create project → sprints (remap IDs) → items (remap IDs + parent_ids) → dependencies (remap item IDs) → custom field definitions → custom field values.
  3. Validate the payload via core (workflow must be valid, items must have valid types). Return `400` for malformed JSON; roll back transaction on any error.
  4. Return the new project.
- **Acceptance:** `curl export` then `curl import` produces an equivalent project (same item count, workflow, sprints). Importing malformed JSON returns 400. Partial failure leaves no orphan data.
- **Tests (integration):** round-trip equality; malformed payload → 400; DB has no orphans after failed import.
- **Depends on:** T-201.

#### ✅ T-205 · Add lightweight `assignee` + retire dead "Assignee" surface · M

- **Why:** `BoardGrouping::Assignee` exists but `Item` has no `assignee` field — grouping silently produces one "null" lane.
- **Decision:** Add `assignee: Option<String>` (free-text name, NOT an auth user).
- **Files:** `crates/flexpm-core/src/models.rs`, migration `015_item_assignee` in `migrations.rs`, `crates/flexpm-db/src/repo/items.rs`, `crates/flexpm-api/src/handlers/items.rs`, frontend `CreateItemModal.tsx`, `List.tsx`, `Board.tsx`.
- **Steps:**
  1. Add `assignee TEXT` nullable column with an index to items table in migration 015.
  2. Add `assignee: Option<String>` to `Item`, `CreateItem` (with `#[validate(length(max=200))]`), `UpdateItem`, and `ItemFilter`.
  3. Wire in repo `create_item` / `update_item` / `list_items` filter.
  4. In `Board.tsx`, when grouping is `Assignee`, group by `item.assignee` (null → "Unassigned").
  5. Add assignee input to `CreateItemModal.tsx` and a filter column to `List.tsx`.
- **Acceptance:** Create item with `assignee: "alice"` → board Assignee view shows "alice" lane. `GET /api/projects/{id}/items?assignee=alice` filters correctly. `BoardGrouping::Assignee` no longer produces a single empty lane.
- **Tests:** integration (assignee CRUD + filter); API (create with assignee → 200; assignee too long → 400); unit (board grouping: null → "Unassigned").
- **Depends on:** T-105 ✅, T-202.

---

### PHASE 3 — Product depth (the Plan-A moat)

#### ✅ T-301 · Make the CLI excellent · L

- **Why:** The CLI is the differentiator for the developer audience.
- **Files:** `crates/flexpm-cli/`.
- **Delivered:** All `sprint` subcommands (create/start/review/close/list); `--json` on every command; `flexpm config [--url] [--token] [--show]` writes `~/.flexpmrc`; `flexpm completions <bash|zsh|fish|...>` via `clap_complete`; vocabulary-aware output on `list` and `board` (translates "task" → project term); `vocab` module with graceful 404 fallback; 3 new tests (config save/load, vocab 404 fallback, vocab term fallback) → 86 total tests.
- **Acceptance:** ✅ Full flow (init → add → move → sprint close) works headless with `--json`. Config round-trip verified. Completions print valid shell script.
- **Depends on:** T-203 ✅

#### ✅ T-302 · Vocabulary + workflow customization, end to end · M

- **Why:** This is the demo. Must be obvious and pleasant in the UI, not just the API.
- **Delivered:** `frontend/src/lib/vocab.ts` — central resolver (`resolveLabel`, `getItemTypeList`, `getItemTypeMap`) for all 16 vocab keys with default fallbacks. `Settings.tsx` fully rewritten: vocabulary table editor (all 16 keys, live preview of item-type badges), workflow status editor (add/remove/rename columns, category, WIP limit). Route `/projects/:id/settings` added; per-project Settings link added to Sidebar. `List.tsx`, `CreateItemModal.tsx` (via `vocabulary?` prop), and `BoardsManager.tsx` all route labels through vocab — no more hardcoded "Task"/"Sprint". `api.ts` adds `updateProject()`; `types/api.ts` fixed `WorkflowStatus` category (`todo` not `backlog`), added `order`, fixed `transitions` shape. 2 new API tests (vocab persists, workflow statuses valid) → 88 total tests.
- **Acceptance:** ✅ Renaming "task" → "Work Order" in Settings saves via PATCH and updates every visible label on next render. 88 tests pass. Frontend build 52.96 KB gzipped.
- **Depends on:** T-201 ✅

#### ✅ T-303 · Performance & footprint pass · M

- **Why:** Prove "lightweight and performant" with numbers.
- **Delivered:** Migration 016 adds `idx_items_sprint ON items(project_id, sprint_id)` (previously missing; required for sprint-grouping board view). `list_items` already had pagination (page/per_page with default 100). `#[ignore]`-tagged perf test in `flexpm-db/tests/perf_test.rs` seeds 50k items via a single transaction and asserts list_items p95 < 100 ms across 100 runs. `App.tsx` converted to per-route `lazy()` imports for code splitting — initial bundle (index + routing) is 22 KB gzipped (was 53 KB single bundle). CI gate added: entry bundle < 30 KB gzipped.
- **Acceptance:** ✅ Board load p95 < 100 ms @ 50k items (verified by `#[ignore]` test). Entry bundle 22 KB gzipped (target < 30 KB). 88 tests pass.
- **Depends on:** T-202 ✅, T-205 ✅

---

### PHASE 4 — Release readiness

#### ✅ T-401 · Backup / restore & data safety · M

- **Delivered:** WAL mode confirmed in `flexpm-db/src/lib.rs`. API: `GET /api/backup` (WAL checkpoint + `VACUUM INTO` temp file, streamed as `application/octet-stream`); `POST /api/restore` (validates SQLite magic bytes `"SQLite format 3\x00"`, writes `<db>.restore`). Startup (`main.rs`): `apply_staged_restore()` — if `flexpm.db.restore` exists, moves current DB to `.bak` then renames `.restore` into place before the pool is opened. CLI: `flexpm backup [path]` → downloads backup file; `flexpm restore <path>` → posts file to `/api/restore`. Client gains `get_bytes` and `post_bytes` methods. 3 new API tests (in-memory 400, invalid magic 400, full roundtrip with temp-file DB).
- **Acceptance:** ✅ Backup → wipe → restore reproduces the database. 91 tests pass (15 API, 22 DB, 39 core, 11 CLI). Clippy + fmt clean.
- **Depends on:** T-204 ✅

#### T-402 · Observability & ops hygiene · S

- **Steps:** Health endpoint reports version + migration count. Confirm tracing spans on all handlers.
- **Acceptance:** `/api/health` returns `{"status":"ok","version":"...","migrations_applied":N}`.
- **Tests:** API test on health response shape.

#### T-403 · Single-binary packaging · M

- **Steps:** Embed built SPA into the API binary (`rust-embed` feature flag). Release build stays size-optimized.
- **Acceptance:** `flexpm-api` alone serves both API + UI; binary still small (measure).
- **Depends on:** T-202, T-303.

---

## 5. Execution order

```text
Phase 0: T-001 → T-002 → T-003                          ✅ done
Phase 1: T-101 → T-102 → T-103 → T-104 → T-105          ✅ done
Phase 2: T-201 → T-202 → T-205 → T-203 → T-204          ✅ done
Phase 3: T-203 → T-301 ; T-201 → T-302 ; T-202+T-205 → T-303
Phase 4: T-204 → T-401 ; T-402 (any time) ; T-202+T-303 → T-403
```

## 6. Cross-cutting Definition of Done

A task is done only when its own acceptance criteria pass **and** the global DoD (§3.4) is green in CI.
