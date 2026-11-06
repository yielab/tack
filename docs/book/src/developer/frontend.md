# Frontend & Design System

The web UI is a [SolidJS](https://www.solidjs.com/) single-page app in `frontend/`,
built with Vite and Tailwind v4. It talks to the API over `fetch` and a WebSocket,
and is embedded into the `tack` binary at release time via the `embed-spa` feature.

This page covers how the frontend is organized and, in particular, the **design
token system** every component relies on.

## Layout

```
frontend/src/
├── app/          App shell — Router, Layout (sidebar + top bar), routes
├── features/     One folder per surface: board, list, calendar, timeline,
│                 orchestration, sprints, item-detail, dashboard (Overview and
│                 Factory metrics), projects, settings, templates, agents,
│                 changesets, threads
├── shared/
│   ├── ui/       The component kit (Button, Badge, Modal, Drawer, Tabs, Card,
│   │             Switch, ConfirmDialog, Fold, CommandPalette, SearchBar,
│   │             Sidebar, ToastContainer …) plus primitives such as
│   │             Avatar/AvatarStack, TypeTile, ProgressBar, PriorityDot, WipChip, KbdHint,
│   │             RunState, LoadError, RouteBoundary and the icon set
│   ├── tree.ts   ancestorsOf / itemsUnder: the one place the web derives from the tree
│   ├── state/    Context stores + signals (project, items, theme, palette,
│   │             display, commandPalette, optimistic updates, toasts)
│   ├── api/      Typed fetch client, one module per resource, and the
│   │             generated schema.gen.ts
│   ├── realtime/ Reconnecting board WebSocket
│   ├── runWithAgent/ The run dialog (RunFlow), attempt list with its pull-request
│   │             badge, decision inbox, merge-readiness panel, event timeline
│   ├── execution/, agents/ Execution types and API calls; the Agents page's pieces
│   ├── vocab/    Per-project terminology resolution (useVocab)
│   ├── keyboard/, help/ Shortcuts; the links from screen text to the guides
│   └── types/    DTOs mirroring the backend
└── index.css     The design tokens (see below)
```

### Module boundary

Features are isolated: **a `features/*` file may import from `shared/*`, never from
another feature.** This is enforced by `frontend/src/architecture.test.ts` — a
failing import shows up as a unit-test failure. Anything two features need goes in
`shared/`.

## Design tokens

All colour, surface, and shadow values live as CSS custom properties in
[`frontend/src/index.css`](https://github.com/yielab/tack/blob/develop/frontend/src/index.css).
Components consume **only** these `--color-*` tokens (via inline `style`), never raw
hex or Tailwind colour literals. That single indirection is what lets one attribute
flip restyle the entire app.

The system has **two axes**:

- **Mode** — a `.dark` class on `<html>` (managed by `shared/state/theme.ts`).
  `:root` holds the light values; `.dark` overrides only what differs.
- **Palette** — a `data-palette="harbor|clay|graphite"` attribute on `<html>`
  (managed by `shared/state/palette.ts`). **Harbor** is the default the app
  applies; no attribute = the **Teal** base values in `:root`.

So the cascade is `:root` → `.dark` → `:root[data-palette="…"]` →
`.dark[data-palette="…"]`. Each block redefines only the **primitive** values
(backgrounds, text tiers, the accent, semantic solids); everything derived (the
primary ramp, hover/active surfaces, inverse text, focus ring) is expressed once as
`var()` aliases that re-resolve against whichever palette is active. Adding another
palette means adding one primitives block — nothing else changes.

A Tailwind `@theme inline` block re-exposes the runtime tokens under utility names
(`bg-surface`, `text-content`, `border-line`, `bg-brand-*`, …) for the places that
use classes instead of inline styles. A `@theme static` block holds the radius and
text-size tokens (`rounded-card`, `rounded-item`, `text-micro`, `text-caption`,
`text-small`, `text-lead`); `npm run lint:tokens` counts bracketed values such as
`rounded-[28px]` against a ratchet, so use a token when one exists.

### Accessibility constraint

Token values are tuned to **WCAG 2.1 AA** (4.5:1 text contrast) and verified by an
axe scan in the E2E suite (`frontend/e2e/a11y.spec.ts`). When changing a colour,
keep white-on-accent and faint-text-on-surface above 4.5:1 — the CI a11y job will
fail otherwise.

### Typography

Self-hosted fonts (via `@fontsource`, so they work offline):

- **Bricolage Grotesque** — the display face: headings, step numbers, buttons
  (`--font-heading`, utility `font-heading`). Variable in weight and optical
  size; its weight is `--font-heading-weight`, which the `font-heading` utility
  and `h1`–`h4` apply.
- **Figtree** — the UI body (`--font-body`, also `--font-sans`).
- **JetBrains Mono** — ids, estimates, and keycaps (`--font-mono`).
- **Atkinson Hyperlegible Next** — replaces body and display when the user turns
  on the high-legibility font (`data-font="legible"` on `<html>`).

Size text in **rem, never px**. The user's text size (Settings → Display &
accessibility, `shared/state/display.ts`) sets the root font size as a
percentage of the browser's default, and only rem follows it: write
`'font-size': '0.8125rem'` or `text-[0.8125rem]` (px ÷ 16). `npm run
lint:tokens` fails on a new px font size.

### Shape

The shape language is over-rounded. Controls — buttons, inputs, tags, tab
pills, banners — are pills (`--radius-pill`). Containers such as board columns
and settings cards take `--radius-card` (28px) on the `--color-bg-panel` fill,
and the items inside them take `--radius-item` (20px) back on the page ground.
A panel is a `<Card>`, which carries that radius and fill; an on/off setting is a
`<Switch>`; a question before something is removed is `createConfirm()` with its
`<ConfirmHost />`, not `window.confirm`.

## Showing a group and its work

Whether an item holds work (`node`), how far its work is (`progress`) and which types exist
(`Project.item_types`, read with `useItemTypes()`) come from the server; a screen never works the
tree out. The one thing the web derives is an item's ancestors and what lies under a group, in
`shared/tree.ts` (`ancestorsOf(item, byId)`, `itemsUnder(group, items)`), because every view already
holds the project's items. Every view draws a group the same way, with these pieces:

- `TypeTile` (`shared/ui/TypeTile.tsx`): a type's name in its catalog colour, stacked for a type that
  holds work and flat for work. It replaces the old type badge everywhere.
- `ProgressBar` (`shared/ui/ProgressBar.tsx`): the server's `Progress` as a bar and "3 of 7 done" or
  "12 of 20 points", plus "n need you"; `progressText` and `needsYouText` give the same words without
  the bar.
- `GroupHeader` (`shared/ui/GroupHeader.tsx`): the only way a group is drawn above its work. It holds
  the `ItemPath`, the `TypeTile`, the title as a link, a status chip that opens the status menu, the
  `ProgressBar`, the teams, a fold (remembered per group, like `Fold`) and **Add item**, **Plan with
  agent** and **Move under…**. It has two densities, `lane` (the Board) and `heading` (above a group's
  items in a list), and takes the work it heads as `children`.
- `ItemPath` (`shared/ui/ItemPath.tsx`): the ancestors as links, from `ancestorsOf`. The app's
  navigation is `Breadcrumb`, a different thing.
- `ParentPicker` (`shared/ui/ParentPicker.tsx`): a combobox over `GET /api/projects/{id}/parents`
  (`api.items.parents`). Every drag that changes a parent has this keyboard twin, so the web offers
  exactly the parents the server admits.

`GroupHeader`, `ItemPath` and `ParentPicker` are imported from their own files, not from
`shared/ui/index.ts`: the header opens the run dialog, which imports the kit. A control a group
cannot use stays on screen, disabled, with its reason.

## Adding a UI component

1. Build it in `shared/ui/` as a small Solid component. Style it with inline token
   styles — `style={{ 'background-color': 'var(--color-bg-base)' }}` — so it
   re-themes for free. Reuse existing primitives (`Avatar`, `TypeTile`,
   `PriorityDot`, `WipChip`, `KbdHint`) rather than re-inlining markup.
2. Export it from `shared/ui/index.ts`.
3. If it has pure logic (a colour map, a formatter), put that in a co-located
   `*.ts` and unit-test it (`shared/ui/primitives.test.ts` is the pattern).
4. Consume it from a feature — never reach into another feature.

## Failures and disabled controls

A screen never turns a failed load into an empty state. Do not end a request with
`.catch(() => [])` or `.catch(() => null)`: let the resource fail, and where it is read
check `resource.error` before `resource()` (reading a failed resource throws), then show
`<LoadError what="the roles" error={resource.error} onRetry={refetch} />` in place of
the empty answer. Anything left to throw is caught by `RouteBoundary`: the app has one at its root and
`app/routes.tsx` wraps every route in its own, so a screen that fails leaves the shell standing and offers
"Try again". A control that cannot be used is shown disabled, never hidden; give `Button` a `reason`
and the text appears beside it while it is disabled and is tied to it with `aria-describedby`. Screen text
names no environment variable or command: it links to the Administrator Guide (`shared/help/routes.ts`).

## Running it

```sh
cd frontend
npm install
npm run dev          # http://localhost:5173, proxies /api to :3210
npm run type-check
npm test             # Vitest unit tests
npm run build
```

Start the API (`cargo run -p tack-cli -- serve`) before the dev server. See
[Testing](testing.md) for the Playwright E2E setup.
