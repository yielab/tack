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

### Phase 5 — Frontend View Consolidation (Active)

**Goal:** A fixed set of standard project-management **view types** over the same items.
**The view type *is* the organization** — there is no "Group By" option to configure. You pick
the view that fits your workflow (Kanban for flow, Sprint for iterations, Timeline for
schedules, etc.) and each view organizes and renders items its own fixed, sensible way. Every
view works the instant a project exists, shows every item in its correct slot, and lets you drag
items in the one way that makes sense for that view.

**Why this phase exists (UX problems with the current design):**

1. **"Group By" must be deleted — it is the core confusion.** The board carries a `grouping`
   field (status / priority / item_type / sprint), surfaced as a Settings dropdown. It is the
   wrong concept: it makes you configure what a view *is*, every option renders the same Kanban
   (the frontend flattens all columns to `status` anyway), so every choice feels identical and
   pointless. **Remove the option entirely.** A Kanban is, by definition, columns by workflow
   status — that is not a setting.
2. **The Board shows nothing by default.** A new project has zero `boards` rows, so the Board
   renders an empty shell and you must go to *Settings → Boards → Create Board* first. A board is
   a database object you have to author before the core feature works — backwards. The Board
   should derive its columns from the project workflow and just work.
3. **Tree ≈ List.** Both render the same parent/child hierarchy from the same data. Tree adds
   only "expand/collapse all"; List adds drag-reparenting and more metadata. Delete Tree, fold
   its one trick into List.
4. **Calendar and Timeline are read-only.** They display items by date but can't move them. A
   calendar you can't drag on and a Gantt you can't reschedule on are just reports.
5. **Sprints is a disconnected page, and its "sprint board" is just another Kanban.** It lives
   outside the view tabs and duplicates Board semantics instead of offering real sprint planning
   (backlog ↔ sprint assignment, capacity, commitment).

**Target view types** — one fixed set in the work tabs, no grouping config. Each view has a
single, fixed organization and a single drag action that fits its workflow:

| View | Fixed organization | Drag action | Best for | Replaces |
|---|---|---|---|---|
| **Board** (Kanban) | columns = workflow statuses | move card = change status | continuous flow / Kanban | current Board + board-in-settings |
| **List** (Table) | flat rows, optional parent indent | reorder / reparent | triage, bulk edit, hierarchy | current List **and** Tree |
| **Timeline** (Gantt) | bars on a date axis | move/resize bar = set start/due dates | scheduled / phase-based work | current Timeline (now interactive) |
| **Calendar** | items on due-date days | drop on a day = set `due_date` | deadline-driven work | current Calendar (now interactive) |
| **Sprint** (Planning) | backlog vs sprint lanes | drop into a sprint = assign `sprint_id` | Scrum / iteration planning | current Sprints page |

#### A. Delete "Group By" and the board-as-settings-object
- [x] **T-501** — **Remove the grouping option entirely.** Delete the "Group By" select and the
      `grouping` field plumbing from the UI; the Board is always columns-by-status, full stop.
      *Files:* `features/settings/panels/BoardsPanel.tsx`, `shared/api/boards.ts`, board types.
- [x] **T-502** — **Board derives its columns from the project workflow** with no saved board
      required. Opening a brand-new project's Board immediately shows the workflow's status
      columns with an inviting "Add item" affordance. No Settings trip, ever.
      *Files:* `features/board/Board.tsx`, `shared/api/boards.ts` (`projectBoardState`).
- [x] **T-503** — **Retire the "create a board in Settings" flow.** Remove the *Settings → Boards*
      CRUD panel and the multi-board `BoardSelector` (one Board per project, defined by the
      workflow). Drop the now-unused board-object endpoints/types from the frontend.
      *Files:* `features/settings/panels/BoardsPanel.tsx`, `features/board/BoardSelector.tsx`,
      `features/settings/ProjectSettings.tsx`, `app/routes.tsx`.

#### B. One clear drag action per view
- [x] **T-504** — **Board drag = status change**, with workflow-transition validation and WIP
      limits enforced on drop (single `items.update`). No other drop behavior to reason about.
      *Files:* `features/board/Board.tsx`.

#### C. Merge Tree into List, then delete Tree
- [x] **T-505** — Add a **"Hierarchy" toggle** to List (indent by `parent_id` + expand/collapse
      all) so List fully covers Tree's only unique behavior.
      *Files:* `features/list/List.tsx`.
- [x] **T-506** — **Delete the Tree view:** remove `features/tree/TreeView.tsx`,
      `features/tree/buildTree.ts`, the `/projects/:id/tree` route, the Tree tab, and the
      `api.items.tree()` call. List becomes the single canonical table/hierarchy view.
      *Files:* `app/routes.tsx`, `shared/ui/WorkTabs.tsx`, `features/tree/*`, `shared/api/items`.

#### D. Make Calendar and Timeline interactive
- [x] **T-507** — **Calendar drag-to-reschedule:** drag an item from one day cell to another to
      set `due_date`; drag from the "no due date" tray onto a day to schedule it.
      *Files:* `features/calendar/Calendar.tsx`.
- [x] **T-508** — **Timeline (Gantt) interactivity:** drag a bar to shift its date range, drag
      either edge to set `started_at` / `due_date`. Snap to the active week/month/quarter grid.
      *Files:* `features/timeline/Timeline.tsx`.

#### E. A real Sprint view (not another Kanban)
- [x] **T-509** — Rebuild Sprints as a **two-pane Sprint Planning view**: Backlog (unassigned
      items) on the left, sprint lanes on the right. Drag from backlog into a sprint to assign
      `sprint_id`; drag between sprints to reassign; drag back to backlog to unassign. Honor the
      "items only join planning/active sprints" rule already enforced server-side.
      *Files:* `features/sprints/Sprints.tsx`.
- [x] **T-510** — Each sprint lane shows **capacity vs commitment** (story points), item count,
      and a lightweight progress/burndown summary (done/total items, done/total pts, % bar).
      *Files:* `features/sprints/Sprints.tsx`.
- [x] **T-511** — Promote **Sprint into the work tabs** (`WorkTabs`) beside Board / List /
      Timeline / Calendar. Redirect the old standalone `/projects/:id/sprints` route to `/sprint`.
      *Files:* `shared/ui/WorkTabs.tsx`, `app/routes.tsx`, `shared/state/lastView.ts`.

#### F. Unify the shell so the views feel like one feature
- [x] **T-512** — Finalized work tabs: **Board · List · Timeline · Calendar · Sprint** (Tree
      removed, Sprint added). Updated `Lens` type and active-lens detection.
      *Files:* `shared/ui/WorkTabs.tsx`, `shared/state/lastView.ts`.
- [x] **T-513** — **One data source:** all five views use `useProjectItems()` from a shared
      `ProjectItemsContext` provided by `WorkLayout`. Tab-switching never triggers a redundant
      fetch. Board derives columns reactively via `deriveBoard(project, items)`.
      *Files:* new `shared/state/projectItemsContext.tsx`, `app/WorkLayout.tsx`, all five views.
- [x] **T-514** — Every view renders a meaningful zero-items empty state: Board shows
      `EmptyProjectGuide`; List shows "No items yet" + create button; Calendar/Timeline show
      `EmptyState` with guidance; Sprint shows "No active sprints" + create button.
      *Files:* each view component (already present from prior phases).

**Acceptance criteria for Phase 5 (done when all are true):**
- There is **no "Group By" control anywhere** in the product.
- A fresh project's Board shows workflow columns immediately, with **zero** Settings interaction.
- There is exactly **one** hierarchy/table view (List); Tree no longer exists.
- Calendar and Timeline can both **reschedule items by dragging**.
- Sprint is a tab and is a backlog ↔ sprint planning surface — not a duplicate Kanban.
- Every created item appears in every applicable view in its correct slot.

### Frontend Tests (Phase 6)
Vitest unit tests for signal/store logic and component interaction. Playwright end-to-end
tests for the golden path (create project → add items → move through board). Currently deferred —
the type-checker and Rust handler tests cover the critical paths.

### Phase 7 — Template Management Depth

**Goal:** Make a template capture a project's *full blueprint* — vocabulary, workflow, custom
fields, and default boards — and let users author templates from scratch or capture an existing
project. The four "Advanced Configuration" bullets shown as *Coming Soon* in the template creator
become real, the placeholder is retired, and the Templates gallery ships populated.

**Why this phase exists (the backend is done; the frontend is the gap):**

The persistence and API layers *already* support rich templates end-to-end. What blocks the
feature is entirely client-side: the creator form only sends three fields and the rest fall back
to project-type defaults.

- **`project_templates` table** (migration 011) stores `vocabulary`, `workflow`, `custom_fields`,
  and `default_boards` as JSON — every "Advanced Configuration" bullet has a column already.
  *(`flexpm-db/src/migrations.rs`)*
- **Core models** `ProjectTemplate`, `CreateProjectTemplate`, `BoardTemplate`, and
  `CustomFieldDefinition` carry all four facets. *(`flexpm-core/src/models.rs`)*
- **API** exposes full CRUD — `POST/GET/DELETE /api/templates`, `GET /api/templates/{id}` — plus
  `POST /api/projects/from-template/{id}`, which instantiates the project *and* applies the
  template's vocabulary + workflow, creates its custom-field definitions, and creates its boards.
  *(`flexpm-api/src/handlers/templates.rs`, `router.rs`)*
- **The gap:** `frontend/src/features/templates/TemplateCreator.tsx` collects only `name`,
  `description`, and `project_type`; everything else is sent as defaults. Lines 113–129 render a
  *"Coming Soon: Advanced Configuration"* panel listing exactly the four facets the backend
  already stores. There is also **no reverse flow** — you cannot snapshot an existing project's
  configuration into a template.

| Facet | Backend storage | Frontend authoring |
|---|---|---|
| Custom workflow statuses & transitions | `workflow` JSON column | **missing** — defaults only |
| Project-specific vocabulary | `vocabulary` JSON column | **missing** — defaults only |
| Pre-defined custom fields | `custom_fields` JSON column | **missing** — none captured |
| Default board configurations | `default_boards` JSON column | **missing** — none captured |

#### A. Template authoring UI — make the four "Advanced Configuration" bullets real
- [x] **T-701** — **Vocabulary editor in the template creator.** Reuse the live vocabulary editor
      from Settings so a template can override any of the 16 terms. Send the map as
      `CreateProjectTemplate.vocabulary`.
      *Files:* `features/templates/TemplateCreator.tsx`, reuse `features/settings/panels/` vocab
      editor, `shared/api/templates.ts`.
- [x] **T-702** — **Workflow editor in the template creator.** Reuse the Settings workflow column
      editor (status name, category, WIP limit, order) and expose optional explicit transitions.
      Send as `CreateProjectTemplate.workflow`.
      *Files:* `features/templates/TemplateCreator.tsx`, reuse `features/settings/panels/`
      workflow editor.
- [x] **T-703** — **Custom-field designer.** Add/edit/remove `CustomFieldDefinition` entries —
      all 9 types (Text, LongText, Number, Date, Boolean, Select, MultiSelect, URL, Email), with
      `required`, `default_value`, `options` (Select/MultiSelect), and `validation`. Send as
      `CreateProjectTemplate.custom_fields`.
      *Files:* `features/templates/TemplateCreator.tsx`, new `CustomFieldDesigner` component,
      `shared/api/templates.ts`, `shared/types/index.ts`.
- [x] **T-704** — **Default board configuration editor.** Define one or more `BoardTemplate`s:
      columns mapped to workflow statuses, per-column WIP, and the default board. Send as
      `CreateProjectTemplate.default_boards`.
      *Files:* `features/templates/TemplateCreator.tsx`.
- [x] **T-705** — **Remove the "Coming Soon: Advanced Configuration" placeholder**
      (`TemplateCreator.tsx:113–129`) once A is complete; the form now submits a full
      `CreateProjectTemplate` instead of name/type/description only.
      *Files:* `features/templates/TemplateCreator.tsx`.

#### B. Capture an existing project as a template (reverse flow)
- [x] **T-706** — **"Save as template" endpoint.** `POST /api/projects/{id}/save-as-template`
      snapshots the project's vocabulary, workflow, custom-field *definitions*, and board configs
      into a new `project_templates` row. Item data is **not** copied — a template is a blueprint,
      not a backup.
      *Files:* `flexpm-api/src/handlers/templates.rs`, `router.rs`, `flexpm-db/src/repo/templates.rs`.
- [x] **T-707** — **"Save as template" dialog** in the project Settings / header. Prompts for
      name + description, calls T-706, then routes to the Templates gallery.
      *Files:* `features/settings/ProjectSettings.tsx` (or project header), `shared/api/templates.ts`.

#### C. Ship built-in templates and a richer gallery
- [x] **T-708** — **Seed built-in templates** (`is_builtin = true`) for each `ProjectType` from the
      existing workflow + vocabulary presets, so the gallery is populated on first run. Built-ins
      are not deletable (already enforced in `delete_template`).
      *Files:* `flexpm-db/src/repo/templates.rs` (`seed_builtin_templates`), called from `flexpm-api/src/main.rs`.
- [x] **T-709** — **Gallery shows what each template includes:** group built-in vs user templates
      and summarize each card — workflow status count, vocabulary overrides, # custom fields, #
      boards — so users can tell templates apart before instantiating.
      *Files:* `features/templates/Templates.tsx`.

#### D. CLI parity and validation
- [x] **T-710** — **CLI template commands** — `template list`, `template show <id>`,
      `template create-from <id>` (create a project from a template), `--json` on each. Closes the
      "Templates" line in **CLI Completeness** / **Known Gaps**.
      *Files:* `flexpm-cli/src/main.rs`.
- [x] **T-711** — **Validate template payloads** on create: workflow has at least one status in
      each category, no duplicate status names, Select/MultiSelect custom fields carry non-empty
      `options`. Reject with `UNPROCESSABLE_ENTITY` (422) / `CoreError::InvalidWorkflow` mapped to 400.
      *Files:* `flexpm-api/src/handlers/templates.rs`, `flexpm-core/src/workflow.rs`, `flexpm-core/src/error.rs`.

**Acceptance criteria for Phase 7 (done when all are true):**
- The template creator lets you set **vocabulary, workflow, custom fields, and default boards** —
  the *"Coming Soon: Advanced Configuration"* panel is gone.
- Creating a project from such a template yields a project whose vocabulary, workflow, custom-field
  definitions, and boards match the template exactly.
- You can **save any existing project as a template** (blueprint only — no items copied).
- The Templates gallery is **populated with built-in templates** for every project type and shows
  what each one contains.
- The CLI can list, show, and instantiate templates.

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
The CLI covers projects, items, sprints, templates, roles, comments, custom fields, and backup/restore.

### Import Formats
- [x] CSV import (items into existing project) — `POST /api/projects/:id/import-csv`, UI in Settings → Data
- GitHub Issues import
- Linear export import

### Mobile / Offline
No current plans. The SPA is responsive and works on mobile browsers, but there is no native
app and no offline-first sync.

---

## Known Gaps

| Area | Gap |
|---|---|
| Frontend tests | None (Vitest + Playwright deferred to Phase 6) |
| Auth | No multi-user auth (by design for v1) |

---

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
