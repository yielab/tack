# Frontend & Design System

The web UI is a [SolidJS](https://www.solidjs.com/) single-page app in `frontend/`,
built with Vite and Tailwind v4. It talks to the API over `fetch` and a WebSocket,
and is embedded into the `tack` binary at release time via the `embed-spa` feature.

This page is for contributors who change the web UI. After reading it you know where code
goes, which module boundary you must not cross, and how the **design token system** every
component relies on works.

## Layout

```
frontend/src/
├── app/          App shell — Router, Layout (sidebar + top bar), routes
├── features/     One folder per surface: board, list, calendar, timeline,
│                 orchestration, sprints, item-detail, dashboard (Overview and
│                 Factory metrics), projects, settings, templates, agents,
│                 changesets, threads, notifications (the inbox)
├── shared/
│   ├── ui/       The component kit (Button, Badge, Modal, Drawer, Tabs, Card,
│   │             Switch, ConfirmDialog, Fold, CommandPalette, SearchBar,
│   │             Sidebar, ToastContainer …) plus primitives such as
│   │             Avatar/AvatarStack, TypeTile, ProgressBar, PriorityDot, WipChip, KbdHint,
│   │             RunState, LoadError, RouteBoundary and the icon set
│   ├── tree.ts   ancestorsOf / itemsUnder: the one place the web derives from the tree
│   ├── state/    Context stores + signals (project, items, theme, palette,
│   │             display, commandPalette, optimistic updates, notifications,
│   │             the auto-start explanation)
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
text-size tokens (`rounded-sm`, `rounded-md`, `rounded-lg`, `rounded-pill`, `text-micro`,
`text-caption`, `text-small`, `text-lead`, `text-body`, `text-reading`, `text-section`,
`text-step`, `text-title`); `npm run lint:tokens` counts bracketed values such as
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

There are four corners. `rounded-sm` (4px) is a tag or a chip, `rounded-md` (6px) a button, a
field or a small control, `rounded-lg` (8px) a panel, a card, a dialog or a popover, and
`rounded-pill` only what is round: an avatar, a dot, a switch, a progress track. A status, a
button, a field and a tag each have their own shape, and a panel is never nested in a panel.

Content is white on a tinted page: `bg-app` is the page, `bg-surface` the panel, with a 1px
`border-line` around it. A panel is a `<Card>`, which carries that radius, fill and border (the
caller sets its padding, 20 to 24px); an on/off setting is a `<Switch>`; a question before
something is removed is `createConfirm()` with its `<ConfirmHost />`, not `window.confirm`.

A `<Button>` is `primary` (solid, the one action of a place), `secondary` (outlined), `ghost`
(text) or `destructive` (red text, no fill), at `sm` (32px), `md` (36px) or `lg` (44px). Rare
actions go in an `<ItemMenu>`.

### Headings

The type scale is `text-title` (34px, the page title, bold), `text-step` (21px, the key step or a
dialog title), `text-section` (19px), `text-lead` (15px, a subsection), `text-reading` (17px) and
`text-body` (16px) for running text, and `text-small`, `text-caption` and `text-micro` for meta.
An `h1`–`h4` with `text-step`, `text-section` or `text-lead` is semibold; with `text-title`, bold.
A section opens with `<SectionHeading>`: an icon in a 28px tinted square, the `h2`, an optional
count and an optional slot for actions. Its `tone` says what the section is for: `done` (green),
`agent` (teal), `people` (blue), `needs` (amber) or `neutral`.

## Showing a group and its work

Whether an item holds work (`node`), how far its work is (`progress`) and which types exist
(`Project.item_types`, read with `useItemTypes()`) come from the server; a screen never works the
tree out. The one thing the web derives is an item's ancestors and what lies under a group, in
`shared/tree.ts` (`ancestorsOf(item, byId)`, `itemsUnder(group, items)`), because every view already
holds the project's items. Every view draws a group the same way, with these pieces:

- `TypeTile` (`shared/ui/TypeTile.tsx`): a type's name in its catalog colour, stacked for a type that
  holds work and flat for work.
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
