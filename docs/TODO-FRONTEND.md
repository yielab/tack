# FlexPM — Frontend Re-architecture TODO (Phase 5)

> **Audience:** any developer or AI agent picking up a single task.
> **Rule:** every task is self-contained — *why*, *which files*, *exact steps*, *acceptance
> criteria*, *required tests*. Do not start a task until its `Depends on` tasks are merged.
> **Parent:** [`PLAN-A-ROADMAP.md`](./PLAN-A-ROADMAP.md) §4 PHASE 5. The global Definition of
> Done (§3.4 there) applies to every task: `npm run type-check` + `npm run build` green, no
> new heavy dep without justification, frontend entry bundle stays < 30 KB gzipped.
> **Decision baseline:** keep & re-architect SolidJS (no Rust/WASM rewrite); SPA is served
> same-origin from the `flexpm-api` binary via `embed-spa` (T-403, already done).
> **If reality and this doc disagree, fix the doc in the same PR.**

Effort key: **S** ≤ half day · **M** ≈ 1–2 days · **L** ≈ 3–5 days.

---

## Target architecture (reference for all tasks)

```
frontend/src/
├── app/            App.tsx (router only) · routes.tsx · providers.tsx · Layout.tsx
├── shared/
│   ├── api/        client.ts + one module per resource + index.ts (`export const api`)
│   ├── realtime/   boardSocket.ts (reconnecting WS)
│   ├── types/      mirror of backend DTOs (single source)
│   ├── state/      optimistic.ts · project-context · stores
│   ├── vocab/      useVocab()
│   ├── ui/         design-system kit (Button, Modal, Drawer, Tabs, Field, …)
│   └── keyboard/   shortcuts + command palette
└── features/       projects · board · list · tree · item-detail · sprints ·
                    timeline · calendar · dashboard · templates · settings
```

**Hard rule:** `features/*` may import from `shared/*`, never from another `features/*`.
No raw `fetch` outside `shared/api/`. No absolute API hosts anywhere.

### Backend route inventory (the parity checklist — source of truth: `crates/flexpm-api/src/router.rs`)

| Resource | Routes |
| --- | --- |
| Projects | `POST/GET /projects` · `GET/PATCH/DELETE /projects/{id}` |
| Items | `POST/GET /projects/{id}/items` · `GET /projects/{id}/items/tree` · `GET/PATCH/DELETE /items/{id}` |
| Search | `GET /search` · `GET /projects/{id}/search` |
| Boards | `POST/GET /projects/{id}/boards` · `GET/PATCH/DELETE /boards/{id}` · `GET /boards/{id}/view` · **`GET /projects/{id}/boards/live` (WS)** |
| Sprints | `POST/GET /projects/{id}/sprints` · `GET /sprints/{id}` · `PATCH /sprints/{id}/status` |
| Roles | `POST/GET /projects/{id}/roles` · `DELETE /roles/{id}` · `PUT/DELETE /items/{id}/roles/{roleId}` |
| Comments | `POST/GET /items/{id}/comments` |
| Dependencies | `POST/GET /items/{id}/dependencies` · `DELETE /items/{id}/dependencies/{depId}` |
| Attachments | `POST/GET /items/{id}/attachments` · `GET/DELETE /attachments/{id}` |
| Custom fields | `POST/GET /projects/{id}/custom-fields` · `GET/PATCH/DELETE /custom-fields/{id}` · `PUT/GET/DELETE /items/{id}/custom-fields/{fieldId}` · `GET /items/{id}/custom-fields` |
| Templates | `POST/GET /templates` · `GET/DELETE /templates/{id}` · `POST /projects/from-template/{id}` |
| Export/Import | `GET /projects/{id}/export` · `POST /projects/import` |
| Backup/Restore | `GET /backup` · `POST /restore` |

---

## T-501 · Unified API client foundation + kill hardcoded hosts · M

> **Status: ✅ Done.** `shared/api/client.ts` (`request`/`requestBlob`/`requestForm`,
> `ApiError`, `tokenStore`) + resource modules (`projects, items, boards, sprints,
> search, templates, customFields`) and `shared/api/index.ts` (`export const api`).
> All six hardcoded-host pages converted; `lib/api.ts` is now a flat-method shim over
> `shared/api` (removed in T-503). `grep localhost:3210 src` and `grep fetch( src/pages`
> are both clean; `type-check` + `build` green; entry bundle 9.74 KB gzipped.
> Vitest added (`npm test`).
> **Deviations from spec:** (1) tests are written at the resource-module contract layer
> (`shared/api/resources.test.ts` asserts URL+method+body for every converted page's
> `api.*` call) instead of per-page component renders — same coverage of the "calls
> `api.*`" guarantee, far less brittle. (2) `api.sprints.update` still targets the
> non-existent `PATCH /sprints/{id}` to preserve existing behavior; the drift is left
> for T-502. (3) embed-spa same-origin path relies on `.env.production` (`VITE_API_URL=/api`),
> already present — not runtime-verified in this pass. (4) the dev `.env` keeps the
> absolute host for `npm run dev`; it lives outside `src/`, so the grep rule still holds.

- **Why:** `Sprints.tsx`, `Templates.tsx`, `TemplateCreator.tsx`, `BoardsManager.tsx`,
  `CustomFieldsManager.tsx`, and `BoardSelector.tsx` hardcode `http://localhost:3210/api/...`.
  These break the instant the SPA is served same-origin from the binary (T-403) or from
  `flexpm.test`. Half the app bypasses the typed `lib/api.ts` client, so there is no single
  place for base-URL, auth header, or error handling.
- **Files:** new `frontend/src/shared/api/client.ts`; migrate `frontend/src/lib/api.ts`
  into `shared/api/*`; edit every file containing a `http://localhost:3210` literal.
- **Steps:**
  1. Create `shared/api/client.ts` exporting `request<T>(path, init?)`:
     - `const BASE = import.meta.env.VITE_API_URL ?? '/api';` (relative by default).
     - Default header `Content-Type: application/json`; merge an optional `Authorization:
       Bearer <token>` from a small `tokenStore` (reads `localStorage`/env; may be empty).
     - On `!res.ok`, throw a typed `ApiError` carrying `status` + parsed message text.
     - Return `undefined` for `204`; otherwise `res.json()`.
     - Provide `requestBlob(path)` and `requestForm(path, FormData)` helpers (no JSON
       content-type for the form variant) for later tasks.
  2. Re-express the existing `lib/api.ts` methods (projects, items, board, sprints, search)
     as `shared/api/{projects,items,boards,sprints,search}.ts` using `request`.
  3. Replace **every** raw `fetch('http://localhost:3210/api/...')` call with the matching
     `api.*` method. Where the method does not exist yet, add it to the right resource
     module (full parity is finished in T-502, but converted callers must not regress).
  4. Keep a temporary re-export shim `lib/api.ts → shared/api` so untouched imports compile
     during the transition; remove the shim at the end of T-503.
- **Acceptance:**
  - `grep -rn "localhost:3210" frontend/src` returns **nothing**.
  - `grep -rn "fetch(" frontend/src/features frontend/src/pages` returns **nothing**
    (all network access flows through `shared/api`).
  - App works both via `npm run dev` (Vite proxy) and when served by
    `cargo run -p flexpm-api --features embed-spa` (same origin, no CORS).
- **Tests (Vitest):** `request()` joins base+path, throws `ApiError` on non-2xx, returns
  `undefined` on 204; one mocked test per converted page confirming it calls `api.*`.
- **Depends on:** — (T-403 ✅ provides the same-origin target).

---

## T-502 · Complete the API client to full backend parity + fix drift · M

> **Status: ✅ Done.** Every route in the inventory table now has exactly one
> `api.*` method. New modules: `comments`, `dependencies`, `roles`, `attachments`,
> `data` (export/import/backup/restore); `customFields` extended with per-item value
> methods (`listValues`/`getValue`/`setValue`/`clearValue`); `boards.view` returns the
> real `BoardViewResponse`. New DTOs mirrored in `shared/types`. Reconnecting socket
> built at `shared/realtime/boardSocket.ts` (status `connecting|open|reconnecting|closed`,
> capped backoff, `project_id` filter, `ping` keepalive consumed). **Drift fixed:** the
> dead `GET /projects/{id}/board` call is gone (the `lib/api.ts` shim now composes
> `boards.list` → default board → `boards.view`); `lib/websocket.ts` URL corrected to
> `/boards/live`. `grep board/live src` is clean; the only remaining `/board` hits are
> SPA *router* paths (`/projects/:id/board`), not API calls. `type-check` + `build` green;
> 61 Vitest tests pass.
> **Deviations / carry-forward:** (1) `POST /restore` takes **raw SQLite bytes** as the
> body (not multipart) — `data.restore(Blob)` sends `application/octet-stream`; correct
> the "multipart" wording in T-512. (2) `api.sprints.update` still targets the
> nonexistent `PATCH /sprints/{id}` (extra method, not in the inventory) to preserve the
> Sprints edit form; needs a backend route or a UI change — left as a backend gap.
> (3) `lib/websocket.ts` event *parsing* is still the legacy `event_type`/PascalCase
> shape (the wire is `type`/snake_case); Board.tsx realtime refetch stays inert until
> T-513 swaps it for `boardSocket.ts`. The correct parsing already lives in
> `boardSocket.ts`. (4) The cross-tab WebSocket smoke test was not run in this pass (no
> live server); covered by `boardSocket.test.ts` unit tests instead.

- **Why:** `lib/api.ts` covers only 6 of ~12 backend areas. Comments, dependencies,
  attachments, roles, custom-fields, templates, export/import, and backup/restore have no
  client method. Two calls are also **wrong**: `getBoard()` hits `GET /projects/{id}/board`
  (does not exist) and `createBoardSocket()` connects to `/board/live` (route is
  `/boards/live`).
- **Files:** `shared/api/{boards,comments,dependencies,attachments,roles,customFields,
  templates,data}.ts`, `shared/api/index.ts`, `shared/realtime/boardSocket.ts`.
- **Steps:**
  1. For **every** route in the inventory table above, add exactly one typed method. Use the
     table as a literal checklist — a route with no method is a failing task.
  2. Fix drift: board columns come from `GET /boards/{id}/view`; the board list from
     `GET /projects/{id}/boards`. Delete the dead `GET /projects/{id}/board` call.
  3. Rebuild `boardSocket.ts`:
     - URL `${wsProto}//${location.host}/api/projects/${projectId}/boards/live`
       where `wsProto = location.protocol === 'https:' ? 'wss:' : 'ws:'`.
     - Auto-reconnect with capped exponential backoff; respond to/emit the existing `Ping`
       keepalive; filter incoming events by `project_id`; expose a `status` signal
       (`connecting | open | reconnecting | closed`) and an `onEvent(BoardEvent)` callback.
  4. `shared/api/index.ts`: `export const api = { projects, items, boards, sprints, search,
     comments, dependencies, attachments, roles, customFields, templates, data };`
  5. Mirror any missing DTOs (Comment, Dependency, Attachment, Role, CustomField,
     CustomFieldValue, Template) into `shared/types/`.
- **Acceptance:** every route in the inventory table has exactly one `api.*` method (verify
  against the checklist); a manual WebSocket connect receives an `ItemUpdated` event after a
  PATCH from another tab; no remaining reference to `/projects/{id}/board` (singular) or
  `/board/live`.
- **Tests (Vitest):** URL+method shape for each new module (mocked `request`); `boardSocket`
  reconnect/backoff and `project_id` filtering against a mock `WebSocket`.
- **Depends on:** T-501.

---

## T-503 · Feature-oriented folder restructure · M

> **Status: ✅ Done.** New tree: `app/` (`App.tsx` router-only, `routes.tsx`, `Layout.tsx`),
> `shared/` (`api`, `realtime`, `types`, `state/optimistic`, `vocab/vocab`,
> `keyboard/keyboard`, `ui/*` incl. `toast`, `Modal`, `SkeletonScreen`, `ToastContainer`,
> `CommandPalette`, `SearchBar`, `Sidebar`, `RichTextEditor`, `CreateItemModal`), and
> `features/*` (projects, board, list, dashboard, sprints, calendar, timeline, templates,
> settings). All pages moved into their feature; feature-specific components
> (`CreateProjectModal`→projects, `BoardSelector`→board) co-located; components shared by
> ≥2 features moved to `shared/ui`. The `lib/api.ts` shim is **deleted** — all callers now
> use nested `api.*` (the legacy `getBoard` composition became `api.boards.projectBoardState`).
> `App.tsx` is the route table only. `type-check` + `build` green (entry 9.48 KB gzipped);
> 62 Vitest tests pass.
> **Boundary rule:** enforced by `src/architecture.test.ts` — a zero-dependency filesystem
> scan that fails if any `features/*` file imports another `features/*` (chosen over
> dependency-cruiser / eslint-plugin-boundaries to avoid a new dev dependency; runs in CI
> via `npm test`).
> **Deviations:** (1) `src/types/api.ts` kept as the type leaf (`shared/types` re-exports it)
> rather than physically relocated — avoids churn, single logical source preserved.
> (2) `lib/websocket.ts` moved to `shared/realtime/websocket.ts` (still legacy; replaced by
> `boardSocket.ts` in T-513). (3) No `app/providers.tsx` yet — there are no app-wide
> providers until T-505's `ProjectProvider`; the file is added then.

- **Why:** flat `pages/ + components/ + lib/` gives no isolation; cross-imports tangle and
  every page re-implements patterns. Restructure once, mechanically, with no behavior change.
- **Files:** all of `frontend/src/**` (moves + import-path fixes); `App.tsx`.
- **Steps:**
  1. Create `app/`, `shared/`, `features/` per the target tree above.
  2. Move each page into its feature folder (`pages/Board.tsx` → `features/board/Board.tsx`,
     etc.); move shared primitives (`lib/optimistic.ts`, `lib/vocab.ts`, `lib/keyboard.ts`,
     `lib/toast.ts`, `lib/websocket.ts`) under `shared/`; move generic components
     (`Modal`, `SkeletonScreen`, `ToastContainer`, `CommandPalette`, `SearchBar`) into
     `shared/ui/`; feature-specific components into their feature folder.
  3. Reduce `App.tsx` to routing only; move `Layout` to `app/Layout.tsx`.
  4. Fix all import paths; remove the `lib/api.ts` shim from T-501.
  5. Add a dependency rule (dependency-cruiser config or an ESLint `no-restricted-imports`
     boundary) forbidding `features/*` → `features/*` imports. Cheap to add; document if a
     tool is introduced (with size/justification note).
- **Acceptance:** `npm run type-check` + `npm run build` green; no file under `features/`
  imports from another `features/` folder; `App.tsx` contains only the route table.
- **Tests:** build + type-check are the gate; if the boundary linter is added, it must pass
  in CI.
- **Depends on:** T-501.

---

## T-504 · Design tokens + shared UI kit · M

> **Status: ✅ Done (tokens + kit + app-wide button/field/modal sweep).**
> `index.css` now holds one token block: `:root` = light, `.dark` = dark overrides
> (toggled by T-512), plus a `prefers-color-scheme` fallback; added missing
> semantic scales (success/warning/danger/info 50–700) and `--color-focus-ring`.
> Built the kit in `shared/ui` — `Button`, `Badge`, `Skeleton`, `EmptyState`,
> `Field` (+`FieldShell`), `Select`, `Modal`, `Drawer`, `Tabs`, `Menu` (+`MenuItem`)
> — each token-only (no hardcoded colors), typed, with focus rings + ARIA; barrel at
> `shared/ui/index.ts`. `grep '#[0-9a-fA-F]{6}' src/shared/ui` → clean. Modal was
> rewritten in place (token-based + ESC + focus return), so its existing consumers are
> upgraded automatically. `CreateProjectModal` migrated to `Field`/`Select`/`Button` as
> the reference adoption. `type-check` + `build` green (entry 9.48 KB gzipped); 71 Vitest
> tests (render each kit component; Modal/Drawer ESC; Tabs `aria-selected`).
> **App-wide sweep: done.** Every standard button/modal/form-field across the feature
> pages now uses the kit: `Sprints`, `BoardsManager`, `CustomFieldsManager`,
> `TemplateCreator`, `Templates`, `Settings`, `CreateProjectModal`, `CreateItemModal`
> (forms → `Field`/`Select`/`FieldShell`, raw modals → kit `Modal`); `Projects`,
> `Dashboard`, `Calendar`, `Timeline`, `Board`, `List` (action/nav buttons → `Button`,
> status pills → `Badge`, empty states → `EmptyState`). Added a `success` Button variant
> for semantic green actions. **Intentionally left as raw elements** (specialized patterns,
> not standard Buttons/Fields, all token-based): toggle-chips (item type / priority /
> estimate / timeline view-mode / template type-filter), icon-only row actions (add-child,
> delete, expand, inline save/cancel), and the dropdown/search widgets (`BoardSelector`,
> `SearchBar`, `Sidebar` chrome). **Carry-forward:** a few legacy `shared/ui` widgets
> (`SearchBar`, `Sidebar`, `CommandPalette`, `RichTextEditor`) still use Tailwind color
> *utilities* internally (not hex, so the grep passes); fold onto tokens incrementally.

- **Why:** card/button/modal Tailwind clusters are duplicated ~30× across pages; a restyle is
  dozens of edits and the look is inconsistent. The `--color-*` CSS variables already exist —
  formalize them and build a kit on top.
- **Files:** `frontend/src/index.css` (token definitions, light/dark sets); new
  `shared/ui/{Button,Modal,Drawer,Tabs,Field,Select,Skeleton,EmptyState,Menu,Badge}.tsx`.
- **Steps:**
  1. Consolidate all theme colors into one token block with explicit light + dark values;
     remove ad-hoc hex/Tailwind color literals from shared components.
  2. Build kit components that consume **only** tokens (no hardcoded colors). Each is small,
     typed, and accessible (focus ring, ARIA where relevant).
  3. Replace at minimum every `Button`, `Modal`, and form `Field` usage across the app with
     the kit equivalents; leave a short note for incremental replacement of the rest.
- **Acceptance:** toggling dark/light restyles all kit components from tokens alone;
  `grep -rn "#[0-9a-fA-F]\{6\}" frontend/src/shared/ui` returns nothing; build green.
- **Tests (Vitest):** render each kit component; `Modal`/`Drawer` open/close + ESC; `Tabs`
  switches panels and sets `aria-selected`.
- **Depends on:** T-503.

---

## T-505 · Project-context provider + reactive vocabulary · M

> **Status: ✅ Done.** `shared/state/projectContext.tsx` — `ProjectProvider` mounted at
> the app root (`Layout`), keyed by the route `:id`, fetches the active project once and
> exposes `projectId()`, `project`, `workflow()`, `vocabulary()`, `refetch()` via
> `useProject()`. `shared/vocab/useVocab.ts` — `useVocab()` returns `t(key)` (all 16 keys,
> default fallback), `types()`, `typeMap()`, all reactive to `vocabulary()`. Per-page
> project fetches removed in favour of the context across **Board, List, Sprints, Settings,
> Calendar, Dashboard, Timeline, BoardsManager, CustomFieldsManager** → one project fetch
> per view. Domain nouns routed through `t()` (Sprints headings/buttons/labels; List item
> types via `useVocab().types()`; Board's create modal now receives the reactive
> `vocabulary()`). Settings `handleSave` calls the context `refetch()`, so a vocabulary
> edit updates every label app-wide with no reload. `type-check` + `build` green (entry
> 9.67 KB gzipped); 74 Vitest tests (incl. `useVocab` custom+fallback and reactive update
> through the context).
> **Notes:** `ProjectProvider` reads `useParams()` from the Router root — reactive to the
> active route, `undefined` off project routes (no fetch). `CreateItemModal` keeps its
> `vocabulary` prop (Board now passes the reactive value; List's edit modal still defaults)
> — folding it onto `useVocab()` directly is a small future cleanup.

- **Why:** pages each refetch the project; vocabulary (`task`→`Work Order`) is not applied
  reactively. One context should hold the active project, its workflow, and its vocabulary.
- **Files:** new `shared/state/projectContext.tsx`, `shared/vocab/useVocab.ts`; consumers in
  `features/board`, `features/list`, `features/sprints`, `features/settings`.
- **Steps:**
  1. `ProjectProvider` keyed by route `:id` exposes `project()`, `workflow()`, `vocabulary()`
     and a `refetch()`; fetch the project once and share it.
  2. `useVocab()` returns `t(term)` mapping all 16 vocab keys with default fallbacks
     (reuse logic from existing `lib/vocab.ts`); reactive to `vocabulary()` changes.
  3. Route every visible domain noun through `t()`; remove per-page project fetches in favor
     of the context.
- **Acceptance:** editing vocabulary in Settings updates every label app-wide without a page
  reload; a project view triggers exactly one project fetch (verify in network panel/test).
- **Tests (Vitest):** `useVocab` maps custom + falls back to default; changing the context
  vocabulary updates a consuming component's rendered label.
- **Depends on:** T-502, T-503.

---

## T-506 · Item Detail drawer shell + deep link · M

> **Status: ✅ Done.** `features/item-detail/ItemDetailDrawer.tsx` (kit `Drawer` — full
> sheet on mobile, ESC + focus return), opens whenever `?item=<id>` is present
> (deep-linkable; open-on-load, close clears the param). `ItemHeader.tsx` gives inline
> editing of title, status (workflow-aware via `useProject().workflow()`), priority,
> estimate (+unit label), due date, sprint, and tags — each commit PATCHes `/items/{id}`
> optimistically (resource `mutate` then reconcile/rollback on error). Tab bar
> `Details · Activity · Dependencies · Files · Fields`: **Details** ships (description via
> `RichTextEditor`, debounced PATCH); the rest are stubs (`tabs/stubs.tsx`) wired in
> T-507–T-510. `type-check` + `build` green (entry 11.40 KB gzipped); 76 Vitest tests
> (URL open/close + ESC clears the param; header edit fires the right PATCH).
> **Architecture:** the drawer is mounted **once in `app/Layout`** (inside `ProjectProvider`)
> rather than inside Board/List — this keeps the `features/* ↛ features/*` boundary intact
> (the boundary test caught the first attempt). Board/List open it via `setSearchParams`
> and refresh on a shared `ITEM_UPDATED_EVENT` (`shared/state/itemEvents.ts`) the drawer
> dispatches after a successful edit; the old edit path via `CreateItemModal` is retired.
> **Deviations:** the model has no `assignee` field and `estimate_unit` isn't in
> `UpdateItem`, so assignee editing is omitted and the unit is shown read-only (label
> only). Re-parenting is deferred to the tree view (T-514); the header notes when an item
> has a parent.

- **Why:** the single biggest UX unlock. There is no item detail surface today, which is why
  comments, dependencies, attachments, custom-field values, and roles are unreachable. Build
  the shell here; tabs are filled in by T-507–T-510.
- **Files:** new `features/item-detail/ItemDetailDrawer.tsx`, `.../ItemHeader.tsx`,
  `.../tabs/` (empty tab stubs), wiring from `features/board/Card` and `features/list` rows.
- **Steps:**
  1. Right-side drawer over any view (full-screen sheet on mobile via the `ui/Drawer`).
  2. Open from a card/row click; sync to the URL query `?item=:itemId` (open on load if
     present, close clears it) so detail views are shareable/deep-linkable.
  3. Header: inline-editable core fields — title, status (workflow-aware), priority,
     assignee, sprint, estimate (+unit), due date, tags, parent — each PATCHing
     `/items/{id}` optimistically through `shared/state/optimistic`.
  4. Tab bar: `Details · Activity · Dependencies · Files · Fields`. Ship `Details`
     (description via existing `RichTextEditor`); others are stubs wired in later tasks.
  5. A11y: focus-trap, ESC to close, return focus to the originating card.
- **Acceptance:** clicking a card opens the drawer populated from `GET /items/{id}`; editing
  a header field persists and updates the underlying board/list optimistically;
  `?item=ID` deep-links to an open drawer; ESC closes and clears the query param.
- **Tests (Vitest):** open/close + URL sync; header edit calls `api.items.update` with the
  right patch; focus trap active while open.
- **Depends on:** T-502, T-504.

---

## T-507 · Item Detail · Activity (comments) tab · S

> **Status: ✅ Done.** `features/item-detail/tabs/ActivityTab.tsx` lists comments
> newest-last with relative timestamps (`GET /items/{id}/comments`), composer posts via
> `api.comments.create` with an optimistic append (resource `mutate`) that rolls back and
> restores the draft to the composer on failure (toast on error); empty state via
> `EmptyState`. Wired into the drawer's Activity tab. 79 Vitest tests (renders fetched
> comments; optimistic append; rollback-on-failure).

- **Why:** comments exist server-side (`/items/{id}/comments`) with no UI.
- **Files:** `features/item-detail/tabs/ActivityTab.tsx`.
- **Steps:** list comments newest-last with relative timestamps; composer posts via
  `api.comments.create` and optimistically appends, rolling back on error; empty state via
  `ui/EmptyState`.
- **Acceptance:** posting a comment shows it immediately and persists across reload; a failed
  post rolls back and toasts the error.
- **Tests (Vitest):** renders fetched comments; optimistic append + rollback on rejected
  `create`.
- **Depends on:** T-506.

---

## T-508 · Item Detail · Dependencies tab · M

- **Why:** dependency endpoints exist with cycle detection server-side, but no UI.
- **Files:** `features/item-detail/tabs/DependenciesTab.tsx`.
- **Steps:** show "blocks" and "blocked by" lists from `GET /items/{id}/dependencies`; add a
  dependency via an item picker (searches project items) → `POST`; delete via `DELETE
  /items/{id}/dependencies/{depId}`. The server rejects cycles with an error — surface the
  400 message inline (do not crash). Each linked item is clickable to open its drawer.
- **Acceptance:** adding a valid dependency renders it in the correct direction; attempting a
  cycle shows the server's rejection message and adds nothing; deleting removes it.
- **Tests (Vitest):** renders both directions; cycle-rejection path surfaces the error;
  delete removes the row.
- **Depends on:** T-506.

---

## T-509 · Item Detail · Files (attachments) tab · M

- **Why:** upload/list/download/delete endpoints exist (50 MB limit), no UI.
- **Files:** `features/item-detail/tabs/FilesTab.tsx`; relies on `requestForm`/`requestBlob`
  from `shared/api/client.ts` (T-501).
- **Steps:** drag-and-drop + file-picker zone uploads via `multipart/form-data` to
  `POST /items/{id}/attachments` (no JSON content-type); list with filename, size, MIME, and
  a download link hitting `GET /attachments/{id}`; delete via `DELETE /attachments/{id}`.
  Reject > 50 MB client-side with a clear message before upload.
- **Acceptance:** dropping a file uploads and lists it; download returns the original file;
  delete removes it; a 60 MB file is rejected client-side with a readable message.
- **Tests (Vitest):** builds a `FormData` upload request; list renders metadata; oversize
  guard blocks upload.
- **Depends on:** T-506.

---

## T-510 · Item Detail · Fields (custom values) + Roles assignment · M

- **Why:** custom-field *definitions* have a manager page but values can't be edited per
  item; role assignment to items has no UI at all.
- **Files:** `features/item-detail/tabs/FieldsTab.tsx`.
- **Steps:**
  1. Load the project's field definitions (`GET /projects/{id}/custom-fields`) and this
     item's values (`GET /items/{id}/custom-fields`); render an input per definition by its
     type; `PUT /items/{id}/custom-fields/{fieldId}` on change; `DELETE` to clear.
  2. Roles section: load project roles (`GET /projects/{id}/roles`); assign/unassign to the
     item via `PUT/DELETE /items/{id}/roles/{roleId}`.
- **Acceptance:** setting a custom-field value persists across reload; clearing removes it;
  assigning a role reflects on the item and can be removed.
- **Tests (Vitest):** renders one input per definition; `PUT` on edit; role assign/unassign
  calls correct endpoints.
- **Depends on:** T-506, T-502.

---

## T-511 · Project Settings consolidation + workflow/vocab/roles/data editors · L

- **Why:** settings are scattered (`BoardsManager`, `CustomFieldsManager` as standalone
  pages) and key editors (workflow, vocabulary, roles, data) are missing or weak. Fold them
  into one tabbed Project Settings surface.
- **Files:** `features/settings/ProjectSettings.tsx` + tab components; remove standalone
  `BoardsManager`/`CustomFieldsManager` pages (move their logic into tabs); routes in
  `app/routes.tsx`.
- **Steps:** build `/projects/:id/settings` with tabs:
  - **General** — name, type, description, archive (`PATCH /projects/{id}`).
  - **Workflow** — add/rename/remove status columns, set category + WIP limit + order, edit
    transitions; save to the project's `workflow` (reuse the existing `Settings.tsx` editor
    from T-302 as the base).
  - **Vocabulary** — the 16-key term editor with live preview (from T-302/T-505).
  - **Boards** — multi-board CRUD (migrated from `BoardsManager`, now via `api.boards`).
  - **Fields** — custom-field definition CRUD (migrated from `CustomFieldsManager`).
  - **Roles** — role/specialty CRUD (`/projects/{id}/roles`, `DELETE /roles/{id}`).
  - **Data** — export (`GET /projects/{id}/export?format=json|csv`, download) + import
    (`POST /projects/import`, file upload).
- **Acceptance:** every former manager page is reachable as a settings tab and the standalone
  routes are gone; workflow/vocabulary/roles edits persist; export downloads a file; import
  creates a project. No raw `fetch` remains in these views.
- **Tests (Vitest):** each tab renders and calls the correct `api.*` method on save; export
  triggers a blob download; import posts the file.
- **Depends on:** T-502, T-503.

---

## T-512 · Global Settings · backup/restore + theme + system · M

- **Why:** backup/restore endpoints (T-401) and the health/debug endpoints have no UI; the
  theme toggle isn't centralized.
- **Files:** `features/settings/GlobalSettings.tsx`; route `/settings`.
- **Steps:** sections —
  - **Appearance** — light/dark/system theme, persisted to `localStorage`, applied via the
    token system (T-504).
  - **Data & Backup** — "Download backup" (`GET /backup` → blob) and "Restore from file"
    (`POST /restore`, multipart) behind an explicit confirm dialog warning it replaces the DB.
  - **System** (advanced/collapsed) — show `GET /health` (version, migrations applied) and
    `GET /debug/db-stats`.
- **Acceptance:** theme choice persists across reloads; backup downloads a non-empty file;
  restore shows a confirm and posts the file; System shows live health JSON.
- **Tests (Vitest):** theme persistence; backup/restore call the right endpoints; restore is
  gated by confirmation.
- **Depends on:** T-502.

---

## T-513 · Real-time reconnection + store reconciliation · M

- **Why:** make the board the source of eventual truth across clients and survive dropped
  sockets; today optimistic edits, list edits, and drag each reconcile differently.
- **Files:** `shared/state/itemStore.ts`, `features/board/Board.tsx`, `app/Layout.tsx`
  (status indicator), `shared/realtime/boardSocket.ts` (from T-502).
- **Steps:** route board items through one `createStore`-based item store; apply
  `boardSocket` events (`ItemCreated/Updated/Deleted/BoardConfigUpdated/SprintUpdated`) as
  store patches; optimistic local edits are confirmed or replaced by the broadcast echo;
  show a header "live / reconnecting" indicator driven by the socket `status` signal.
- **Acceptance:** an edit in tab A appears in tab B without reload; killing/restoring the
  socket flips the indicator and resyncs; a conflicting optimistic edit reconciles to the
  server value.
- **Tests (Vitest):** store reducer applies each event type; reconnect updates status; echo
  replaces an optimistic value.
- **Depends on:** T-502, T-505.

---

## T-514 · New & upgraded views · Tree, Timeline deps, Dashboard metrics · L

- **Why:** the hierarchy endpoint, dependency data, and real item data are unused by views.
- **Files:** new `features/tree/TreeView.tsx`; `features/timeline/Timeline.tsx`;
  `features/dashboard/Dashboard.tsx`; route `/projects/:id/tree`.
- **Steps:**
  1. **Tree** — render `GET /projects/{id}/items/tree` as an expand/collapse hierarchy
     (epic→feature→task→subtask); click opens the Item Detail drawer.
  2. **Timeline** — overlay dependency arrows from `/items/{id}/dependencies` onto the
     existing timeline; highlight blocked items.
  3. **Dashboard** — replace placeholder metrics with values computed from the real items
     resource: throughput (done over time), WIP per column, completion %, optional burndown
     for the active sprint. (No backend metrics endpoint exists; compute client-side.)
- **Acceptance:** Tree reflects real parent/child structure and opens detail on click;
  Timeline shows dependency links; Dashboard numbers match the project's actual items.
- **Tests (Vitest):** tree builds nested structure from a flat/tree payload; dashboard
  aggregations are correct for a fixture set.
- **Depends on:** T-502, T-505.

---

## T-515 · Frontend build → `embed-spa` pipeline wiring · S

- **Why:** T-403 added the backend `embed-spa` feature reading `frontend/dist/`; the
  frontend-side build/handoff and a one-command path should be documented and scripted.
- **Files:** `Makefile` (or `package.json` script), `docs/DEPLOYMENT-GUIDE.md`, CI job.
- **Steps:** add a `make build-spa` (or `npm run build:embed`) target that runs
  `npm --prefix frontend ci && npm --prefix frontend run build` then
  `cargo build -p flexpm-api --release --features embed-spa`; document the single-binary
  output; ensure CI's `embed-spa` job consumes the freshly built `dist`.
- **Acceptance:** one command from a clean checkout produces a single binary that serves both
  `/api/*` and the SPA same-origin; binary stays ~5 MB; entry bundle < 30 KB gzipped.
- **Tests:** CI `embed-spa` job builds and runs the feature-gated API tests green.
- **Depends on:** T-501.

---

## Not a frontend task — CLI / voice ("Alexa-type") intake

The CLI and any voice/automation layer (Alexa skill, phone shortcut, cron) call the **same
REST API** the GUI uses — e.g. `POST /projects/{id}/items`. Because the GUI subscribes to the
board WebSocket (T-513), an item added by voice or CLI appears live in an open GUI with no
refresh. No frontend special-casing is required: a complete, correct API layer (T-501/T-502)
makes GUI, CLI, and voice first-class citizens of one contract. Any voice work belongs in the
CLI/API backlog, not here.
