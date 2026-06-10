# Roadmap

**Current version:** 2.0.0  
**Status:** All four engineering phases complete. The product is feature-complete for the
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

---

## Planned

### Frontend Tests (Phase 5)
Vitest unit tests for signal/store logic and component interaction. Playwright end-to-end
tests for the golden path (create project → add items → move through board). Currently deferred —
the type-checker and Rust handler tests cover the critical paths.

### Multi-User / Auth (Future, Optional)
The current design is explicitly local-first and single-user. Adding multi-user would require:
- A proper auth layer (session or JWT)
- Per-user access control on projects
- Audit log for item changes

This is not planned for the near term. The API token (`FLEXPM_API_TOKEN`) covers the "shared
on a LAN" use case without full auth.

### Notifications / Reminders
- Due-date notifications (OS native, email, or webhook)
- Recurring items

### CLI Completeness
The CLI covers projects, items, sprints, and backup/restore. Not yet covered:
- Roles
- Comments
- Templates
- Custom fields

### Import Formats
- CSV import (items only)
- GitHub Issues import
- Linear export import

### Mobile / Offline
No current plans. The SPA is responsive and works on mobile browsers, but there is no native
app and no offline-first sync.

---

## Known Gaps

| Area | Gap |
|---|---|
| Frontend tests | None (Vitest + Playwright deferred) |
| CLI | Roles, comments, templates, custom fields not covered |
| Import | CSV import not implemented |
| Auth | No multi-user auth (by design for v1) |
| WebSocket | No reconnect backoff (plain reconnect loop in frontend) |

---

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
