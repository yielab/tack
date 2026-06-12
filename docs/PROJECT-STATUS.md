# FlexPM Project Status

**Last updated:** 2026-06-11
**Version:** 2.0.0
**Positioning:** Local-first, single-binary project management for solo devs and tiny crews.

---

## What is production-ready

| Component | Status | Notes |
| --- | --- | --- |
| Backend API | ✅ Working | REST endpoints, Axum + SQLite, 16 migrations |
| Domain logic | ✅ Working | Workflow engine, vocabulary, dependency DAG |
| Frontend SPA | ✅ Working | Board, List, Sprint, Calendar, Timeline (all interactive) |
| WebSocket | ✅ Working | Live board updates via `/api/projects/{id}/boards/live` |
| File attachments | ✅ Working | Multipart upload, up to 50 MB |
| FTS5 search | ✅ Working | Per-project + global |
| Export (JSON/CSV) | ✅ Working | Full project snapshot |
| Import (JSON) | ✅ Working | Round-trip import with ID remapping |
| Import (CSV) | ✅ Working | Items into existing project via Settings → Data |
| Project templates | ✅ Working | CRUD + create-from-template + save-as-template; built-in templates seeded on first run |
| Custom fields | ✅ Working | 9 field types; value validation (type + options + rules); designer in template creator |
| Board view | ✅ Working | Columns derived from workflow; no board object needed |
| Sprint view | ✅ Working | Two-pane backlog ↔ sprint planning; capacity + burndown per lane |
| Calendar/Timeline | ✅ Working | Fully interactive — drag to reschedule items |
| Assignee field | ✅ Working | `assignee: Option<String>` on Item, filter support |
| CLI | ✅ Working | HTTP client; projects, items, sprints, templates, roles, comments, custom fields, backup/restore; `--json` everywhere; shell completions |
| CI pipeline | ✅ Done | GitHub Actions: fmt + clippy + test + frontend build |
| CORS restrictions | ✅ Done | Allow-list via `FLEXPM_ALLOWED_ORIGINS` |
| Body size limits | ✅ Done | 2 MB global, 50 MB upload route |
| Security headers | ✅ Done | `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options` |
| API token auth | ✅ Done | Optional Bearer token via `FLEXPM_API_TOKEN` |
| Alexa voice integration | ✅ Working | `POST /api/alexa`; AddTask / ListTasks / CompleteTask intents; skill-ID + timestamp auth; vocabulary-aware responses; exempt from Bearer-token gate |
| Webhook notifications | ✅ Working | `FLEXPM_WEBHOOK_URL` — item.created/updated/deleted, sprint.started/completed, item.due_soon; optional HMAC-SHA256 signing via `FLEXPM_WEBHOOK_SECRET` |
| GitHub Issues import | ✅ Working | `POST /api/projects/{id}/import-github`; accepts owner/repo or full URL, optional PAT, label filter, closed-issue toggle |
| Input validation | ✅ Done | `validator` on all `Create*/Update*` DTOs |
| Workflow correctness | ✅ Done | Parent-auto-complete + WIP decisions live in `flexpm-core` |
| Single binary | ✅ Done | `--features embed-spa` embeds SPA; ~5 MB release binary |

---

## What is not implemented (by design)

| Feature | Status | Detail |
| --- | --- | --- |
| **Authentication** | ❌ None by design | Local-only. Use `FLEXPM_API_TOKEN` if you expose the port on a LAN. |
| **Playwright E2E tests** | ⏸ Deferred | Phase 6 delivered 106 Vitest unit tests; Playwright end-to-end tests remain deferred. Golden-path coverage provided by Rust handler tests. |

---

## Test coverage (actual)

- **Total test functions:** 164 (`cargo test --workspace`) + 1 `#[ignore]` perf test
- **Breakdown:** flexpm-core 67 unit (incl. 28 custom-field validation), flexpm-db 22 integration + 1 ignored perf test (in-memory SQLite), flexpm-api 64 handler tests (11 unit incl. 7 GitHub URL-parsing + 17 Alexa + 36 integration), flexpm-cli 11
- **CI:** GitHub Actions runs `cargo test --workspace` on every push ✅; entry bundle gate (< 30 KB gzipped) ✅
- **Frontend tests:** 144 Vitest unit tests across 21 test files; Playwright E2E deferred

---

## Known issues

None. All eleven phases of the engineering roadmap are complete.

---

## Completed phases

**Phase 0 — Repo hygiene:** dead code removed, docs consolidated, status rewritten.

**Phase 1 — CI & security baseline:** GitHub Actions pipeline, CORS/body-limit hardening, API token auth, input validation.

**Phase 2 — Architecture:** workflow validation in `flexpm-core`, dual-board system removed, CLI rewritten to use HTTP API, import implemented, assignee field added.

**Phase 3 — Product depth:** CLI with sprint commands / completions / vocab-aware output; vocabulary + workflow Settings UI; performance pass (sprint index, lazy routes, 22 KB entry bundle).

**Phase 4 — Release readiness:** backup/restore CLI + API; `/api/health` observability; `--features embed-spa` single-binary build.

**Phase 5 — Frontend view consolidation:** "Group By" removed; Board derives columns from workflow; Tree deleted (Hierarchy toggle added to List); Calendar and Timeline made interactive (drag to reschedule); Sprint rebuilt as two-pane planning surface and promoted to a work tab; all views share one `ProjectItemsContext`.

**Phase 6 — Frontend tests:** 106 Vitest unit tests across 17 test files; covers API client contracts, `deriveBoard` pure function, context providers, settings panels, CSV import UI; Playwright E2E deferred.

**Phase 7 — Template management depth:** Template creator authoring UI (vocabulary, workflow, custom fields, boards — "Coming Soon" removed); save-project-as-template endpoint + dialog; built-in templates seeded per project type; gallery enriched with per-card metadata; CLI template commands; template payload validation.

**Phase 8 — Custom field validation + Alexa voice integration:** `CustomFieldDefinition::validate_value()` moved into `flexpm-core` (type, options, pattern/min/max rules); `set_field_value` handler returns 422 on invalid value; `POST /api/alexa` Alexa custom-skill endpoint (AddTask, ListTasks, CompleteTask intents) with skill-ID + timestamp replay protection, vocabulary-aware spoken responses, full workflow guard enforcement (transitions, WIP limits, parent auto-completion), and WebSocket event broadcast; board view applies per-board item filters on fetch.

**Phase 9 — Full integration test coverage:** API integration tests expanded from 16 to 36 (added sprints, roles, comments, dependencies, search, JSON/CSV export, item update/delete). Frontend utility tests added: vocab resolution, lens persistence, keyboard manager, optimistic-update rollback — total 144 Vitest tests across 21 files. Vitest config cleaned up for vite 8 + OXC pipeline.

**Phase 10 — Webhook notifications:** Outbound webhook delivery via `FLEXPM_WEBHOOK_URL`. Fires `item.created`, `item.updated`, `item.deleted`, `sprint.started`, `sprint.completed`, and `item.due_soon` (background hourly check). Optional HMAC-SHA256 payload signing via `FLEXPM_WEBHOOK_SECRET` (`X-FlexPM-Signature` header). Delivery is fire-and-forget — errors are logged, never surfaced to callers.

**Phase 11 — GitHub Issues import:** `POST /api/projects/{id}/import-github` fetches issues from any public or token-accessible GitHub repository. Accepts `owner/repo`, full GitHub URLs, optional PAT, label filter, and a closed-issue toggle. Pull requests are skipped automatically. Closed issues land in the first Done-category status; open issues land in the first workflow status. Handles pagination. 7 unit tests for URL parsing.
