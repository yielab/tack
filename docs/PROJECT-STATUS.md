# FlexPM Project Status

**Last updated:** 2026-06-08
**Version:** 1.2.0
**Positioning:** Local-first, single-binary project management for solo devs and tiny crews.

---

## What is production-ready

| Component | Status | Notes |
| --- | --- | --- |
| Backend API | ✅ Working | REST endpoints, Axum + SQLite, 15 migrations |
| Domain logic | ✅ Working | Workflow engine, vocabulary, dependency DAG |
| Frontend SPA | ✅ Working | Board, List, Sprint, Dashboard, Calendar, Timeline views |
| WebSocket | ✅ Working | Live board updates via `/api/projects/{id}/boards/live` |
| File attachments | ✅ Working | Multipart upload, up to 50 MB |
| FTS5 search | ✅ Working | Per-project + global |
| Export (JSON/CSV) | ✅ Working | Full project snapshot |
| Import (JSON) | ✅ Working | Round-trip import with ID remapping (T-204) |
| Project templates | ✅ Working | CRUD + create-from-template |
| Custom fields | ✅ Working | 9 field types |
| Multiple boards | ✅ Working | Multi-board per project, 6 grouping modes |
| Assignee field | ✅ Working | `assignee: Option<String>` on Item, filter + board grouping (T-205) |
| CLI | ✅ Working | HTTP client; `--json` everywhere; `config` command; shell completions; vocab-aware output (T-301) |
| CI pipeline | ✅ Done | GitHub Actions: fmt + clippy + test + frontend build |
| CORS restrictions | ✅ Done | Allow-list via `FLEXPM_ALLOWED_ORIGINS` |
| Body size limits | ✅ Done | 2 MB global, 50 MB upload route |
| Security headers | ✅ Done | `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options` |
| API token auth | ✅ Done | Optional Bearer token via `FLEXPM_API_TOKEN` |
| Input validation | ✅ Done | `validator` on all `Create*/Update*` DTOs |
| Workflow correctness | ✅ Done | Parent-auto-complete + WIP decisions live in `flexpm-core` (T-201) |
| Board consolidation | ✅ Done | Single `boards` system; `board_views` table dropped (T-202) |

---

## What is a stub or incomplete

| Feature | Status | Detail |
| --- | --- | --- |
| **Authentication** | ❌ None by design | Plan A is local-only. Use `FLEXPM_API_TOKEN` if you expose the port on a LAN. |
| **Frontend vocabulary labels** | ✅ Complete | All UI labels route through `vocab.ts`; Settings panel edits vocab + workflow (T-302). |
| **CLI `--json` output** | ✅ Complete | All commands support `--json` (T-301). |

---

## Test coverage (actual)

- **Total test functions:** 91 (`cargo test --workspace`) + 1 `#[ignore]` perf test
- **Breakdown:** flexpm-core 39 unit, flexpm-db 22 integration + 1 ignored perf test (in-memory SQLite), flexpm-api 15 handler tests (4 middleware + 11 integration + 3 backup/restore), flexpm-cli 11 (wiremock + unit)
- **CI:** GitHub Actions runs `cargo test --workspace` on every push ✅; entry bundle gate (< 30 KB gzipped) ✅
- **Frontend tests:** none (Vitest + Playwright deferred to Phase 4)

---

## Known issues

None currently. Phase 3 complete.

---

## Next actions

See [PLAN-A-ROADMAP.md](PLAN-A-ROADMAP.md) for the full phased SOP.

**Phase 3 complete:**

1. **T-301** ✅ — CLI excellent: `--json` everywhere, shell completions, `config` command, vocabulary-aware output.
2. **T-302** ✅ — Vocabulary + workflow UI: `vocab.ts` resolver, Settings panel, all labels routed through vocab map.
3. **T-303** ✅ — Performance pass: migration 016 adds sprint index; `#[ignore]` perf test seeds 50k items; lazy route loading drops entry bundle to 22 KB gzipped; CI gate < 30 KB.

**Phase 4 in progress:**

1. **T-401** ✅ — Backup / restore: `GET /api/backup` (VACUUM INTO), `POST /api/restore` (staged), `flexpm backup/restore` CLI commands, startup auto-apply.
2. **T-402** — Observability: `/api/health` returns version + migration count
3. **T-403** — Single-binary: embed SPA into API binary via `rust-embed`
