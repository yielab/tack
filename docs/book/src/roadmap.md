# Roadmap

**Current version:** 2.0.0  
**Status:** All eight engineering phases complete. The product is feature-complete for the
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

---

## Planned

### Future / Optional

#### Multi-User / Auth
The current design is explicitly local-first and single-user. The API token (`FLEXPM_API_TOKEN`)
covers the "shared on a LAN" use case. Full multi-user would require a proper auth layer (session
or JWT), per-user access control, and an audit log.

#### Notifications / Reminders
- Due-date notifications (OS native, email, or webhook)
- Recurring items

#### Import Formats
- GitHub Issues import
- Linear export import

#### Mobile / Offline
No current plans. The SPA is responsive on mobile browsers; no native app and no offline-first sync.

---

## Known Gaps

| Area | Gap |
|---|---|
| Frontend tests | 106 Vitest unit tests; Playwright E2E deferred |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported |
| Auth | No multi-user auth (by design for v1) |

---

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
