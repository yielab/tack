# FlexPM — Design System & Brand Roadmap

_Status: proposal · Author: design audit · Date: 2026-06-13_

This document audits FlexPM's current UI/UX and styling, benchmarks it against
2025–2026 best practices, defines a proper brand, and lays out a phased roadmap
to get there. It follows the project's existing "Phase N" convention.

---

## 1. Audit — Where We Are Today

FlexPM already has a **better-than-average foundation**: a single-source token
file ([frontend/src/index.css](../frontend/src/index.css)) with light/dark
themes, semantic color scales (success/warning/danger/info), shadow + radius
scales, and a real component kit under
[frontend/src/shared/ui/](../frontend/src/shared/ui/). The architecture intent —
_"the shared UI kit consumes ONLY these tokens, never raw hex/Tailwind color
literals"_ — is exactly right.

The problem is **the intent is not enforced**, and the system is incomplete in
the areas that make a product feel branded and polished.

### 1.1 Strengths

- **Token-driven theming.** Light/dark via a single `.dark` class flip; OS
  preference honored pre-toggle. Clean, modern approach.
- **Semantic color scales** (50/100 surfaces, 500–700 solids) — the right shape
  for badges, alerts, and status chips.
- **Componentized kit** — `Button`, `Badge`, `Modal`, `Drawer`, `Menu`,
  `Tabs`, `EmptyState`, `Skeleton`, command palette, toasts. Good coverage.
- **UX affordances already present** — Ctrl+K command palette, global search,
  optimistic updates, skeleton loaders, keyboard shortcuts, real-time board.
  These are the hard things, and they exist.

### 1.2 Problems (evidence-based)

| # | Issue | Evidence | Impact |
|---|-------|----------|--------|
| P1 | **Token rule is violated at scale.** Raw Tailwind color literals (`text-gray-900 dark:text-white`, `bg-purple-100`, `hover:border-purple-500`) appear **288 times across 12 files**, including kit files (`Sidebar`, `CommandPalette`, `SearchBar`, `ToastContainer`, `SkeletonScreen`). Meanwhile 42 files use tokens correctly. | `grep` count; [Projects.tsx](../frontend/src/features/projects/Projects.tsx) uses `dark:bg-gray-800` while [Dashboard.tsx](../frontend/src/features/dashboard/Dashboard.tsx) uses `var(--color-bg-elevated)` | Dark mode is **inconsistent** (two different grays for the same surface); rebrand requires touching 12+ files instead of 1. |
| P2 | **Brand font is declared but never loaded.** `font-family: 'Inter'…` is set in CSS, but there is no `@font-face`, no Google Fonts `<link>`, no self-hosted file. Users see `system-ui` (Segoe/Roboto/SF), not Inter. | [index.css](../frontend/src/index.css), [index.html](../frontend/index.html) | The product looks different on every OS; no typographic identity. |
| P3 | **No type scale, no spacing scale, no motion tokens.** Font sizes/weights are ad-hoc Tailwind utilities (`text-2xl`, `text-3xl`) chosen per-file. Radii/shadows are tokenized; type, spacing, and animation are not. | `text-3xl` in Projects vs `text-2xl` in Dashboard for the same "page title" role | Visual rhythm drifts page to page; no single lever for density. |
| P4 | **Iconography is mixed.** Feather icons (`solid-icons/fi`) in the sidebar, but **emoji** (📊 ✅ 📈 ➕ ⬛) as functional icons in Dashboard stat cards and the command palette. | [Dashboard.tsx](../frontend/src/features/dashboard/Dashboard.tsx), [Layout.tsx](../frontend/src/app/Layout.tsx) | Emoji render differently per OS, break visual cohesion, and read as unpolished. |
| P5 | **Brand mark is inconsistent.** The favicon is a polished P3-purple "bolt/arrow" mark with cyan accents (`#863bff` + `#47bfff`); the in-app logo is just the letter **"F"** in a `violet-500→purple-600` gradient box. The favicon's brand purple (`#863bff`) doesn't match the token primary (`#7c3aed`). | [favicon.svg](../frontend/public/favicon.svg) vs [Sidebar.tsx](../frontend/src/shared/ui/Sidebar.tsx) | No single recognizable brand mark; three different purples in play. |
| P6 | **Inline JS style mutation for interaction states.** Hover is implemented with `onMouseEnter/onMouseLeave` handlers setting `element.style` directly. | [Sidebar.tsx](../frontend/src/shared/ui/Sidebar.tsx) | Not keyboard/focus reachable, no transitions, bypasses the token/CSS layer, hard to maintain. |
| P7 | **Tailwind theme isn't wired to tokens.** `tailwind.config.js` has an empty `extend: {}`, so `bg-primary-600` etc. don't exist as utilities — which is _why_ developers reach for `bg-purple-600`. The token system and the utility system are disconnected. | [tailwind.config.js](../frontend/tailwind.config.js) | The path of least resistance leads _away_ from tokens. Root cause of P1. |
| P8 | **No accessibility baseline.** Focus-visible rings exist on `Button` but not consistently; no documented contrast targets; emoji icons lack labels; no reduced-motion handling. | kit scan | WCAG 2.2 AA not verifiable; risk for keyboard/AT users. |
| P9 | **No brand definition exists.** No name story, no logo system, no color rationale, no voice/tone, no defined personality. The purple is a default, not a decision. | repo-wide | The app is _competent_ but _generic_ — it doesn't look like anything. |
| P10 | **`dark:` variant is keyed to the OS, not the toggle.** The theme toggle sets a `.dark` class on `<html>`, but Tailwind v4's `dark:` variant defaults to `prefers-color-scheme`. So the 288 raw `dark:` literals follow the **OS setting**, while token components follow the **class**. A user on a light OS who toggles dark in-app gets a half-dark UI. | [theme.ts](../frontend/src/shared/state/theme.ts) applies a class; no `@custom-variant dark` configured | Manual theme toggle is partially broken today. Resolved by Phase 15 (token utilities don't use `dark:` at all). |

### 1.3 Root cause

P7 (Tailwind disconnected from tokens) **causes** P1 (raw literals everywhere).
Fixing the plumbing so `bg-primary-600` / `text-content-primary` are real,
ergonomic utilities removes the incentive to type `bg-purple-600`. **Fix the
plumbing first, then the brand, then the polish.**

---

## 2. Best Practices We're Targeting (2025–2026)

- **Single source of truth, three tiers:** _primitive_ tokens (raw palette) →
  _semantic_ tokens (`surface`, `content`, `border`, `brand`) → _component_
  tokens. Theme by swapping the semantic layer. (Aligns with the W3C Design
  Tokens format and how Radix/shadcn/Material 3 structure themes.)
- **OKLCH color** for perceptually-even scales and reliable contrast, with hex
  fallbacks. Modern, and well-supported in current browsers.
- **Type scale by role, not size** (`display`, `title`, `body`, `caption`,
  `code`) on a modular scale, with a self-hosted variable font (no layout-shift,
  no privacy/CDN dependency).
- **One icon system**, optical-size aware, with accessible labels. No emoji as UI.
- **Motion tokens** (duration + easing) and `prefers-reduced-motion` respect.
- **WCAG 2.2 AA** as a hard gate: ≥4.5:1 text contrast, visible focus, 24px+
  targets, AT labels.
- **A living component gallery** (a `/_kit` route or Histoire/Storybook) so the
  system is documented and visually regression-checked.
- **Density & spacing tokens** for a comfortable/compact toggle — table-stakes
  for PM tools (Linear, Jira, Height all ship this).

---

## 3. Brand Definition (proposed)

This is the missing decision layer. Proposed direction — **refine, don't
replace** (the bolt mark and purple already have equity):

- **Name & idea:** _FlexPM_ — "work tracking that bends to your words." The
  brand personality: **precise, calm, fast, adaptable.** Not playful, not
  enterprise-grey. Think Linear's restraint with a warmer, more human edge.
- **Logo system:** promote the **favicon bolt/arrow mark** to the primary logo.
  Define: app icon (mark only), horizontal lockup (mark + "FlexPM" wordmark),
  and monochrome variant. Retire the placeholder "F" box.
- **Brand color:** standardize on **one** brand purple across favicon, tokens,
  and lockup (recommend `#7C3AED` / OKLCH equivalent as `brand`, with the bolt's
  `#863BFF` as the gradient highlight and `#47BFFF` cyan as the **single**
  accent for data-viz/links). Three purples → one purple + one accent.
- **Typography:** **Inter** (UI) self-hosted as a variable font + a monospace
  (e.g. **JetBrains Mono**) for IDs/keys/code. Defined roles, not raw sizes.
- **Voice & tone:** short, active, jargon-free; vocabulary-aware (the product
  already renames "task"→"work order" — the copy should too).
- **Deliverable:** a one-page `docs/BRAND.md` + a Figma/excalidraw board.

> **Decision (2026-06-13): explore a FRESH palette & mark.** The roadmap below
> keeps the foundational/plumbing phases palette-agnostic (Phases 14–16 work
> regardless of final colors); the new palette and logo are defined and applied
> in **Phase 17**, where a short exploration round precedes implementation. The
> bolt mark and purple are treated as _prior art to react to_, not constraints.

---

## 4. Target Token Architecture

```
primitive   --purple-600: oklch(...)        // raw palette, never used directly in components
   │
semantic    --brand: var(--purple-600)       // light + dark remaps live here only
            --surface / --surface-elevated / --surface-sunken
            --content / --content-muted / --content-subtle
            --border / --border-strong
            --accent (cyan) · --success/warning/danger/info
   │
component   --btn-bg, --card-bg, --chip-bg   // optional, for complex parts
```

Plus **non-color** scales as tokens: `--font-size-*` (role-based),
`--space-*`, `--duration-*`, `--ease-*`, `--density-*`. All exposed to Tailwind
via `tailwind.config.js` so `bg-surface`, `text-content`, `text-title` are
first-class utilities.

---

## 5. Roadmap (phased)

Each phase is independently shippable and ordered by leverage. Estimates assume
one developer.

### Phase 14 — Token Plumbing & Lint Gate ✅ _done (foundation)_
**Goal:** make tokens the path of least resistance; stop the bleeding.
1. Restructure [index.css](../frontend/src/index.css) into primitive → semantic
   tiers (rename `--color-bg-*`→`--surface-*`, `--color-text-*`→`--content-*`;
   keep back-compat aliases so nothing breaks mid-migration).
2. Wire tokens into [tailwind.config.js](../frontend/tailwind.config.js) so
   `bg-surface`, `text-content`, `border-default`, `bg-brand`, `text-title`
   become real utilities (fixes **P7**).
3. Add an ESLint rule / CI grep that **fails the build** on raw color literals
   (`*-(gray|purple|violet|…)-[0-9]`) in `src/` (locks in the rule from the
   CSS file's own header comment).
4. _Exit:_ new utilities exist; CI blocks regressions. No visual change yet.

### Phase 15 — Migrate Raw Literals → Tokens ✅ _done (cleanup)_
**Goal:** kill the 284 raw-literal occurrences (**P1, P6**).
1. Convert the 12 offending files to token utilities, **starting with kit
   files** (`Sidebar`, `CommandPalette`, `SearchBar`, `ToastContainer`,
   `SkeletonScreen`) then features (`Board`, `List`, `Projects`, `Templates`,
   `FieldsPanel`, `CreateItemModal`, `RichTextEditor`).
2. Replace inline `onMouseEnter/Leave` style mutation with CSS `:hover` +
   transition utilities (**P6**).
3. _Exit:_ CI grep passes; dark mode renders one consistent surface everywhere.

### Phase 16 — Typography & Iconography _(identity; ~2 days)_
**Goal:** real type system + one icon language (**P2, P3, P4**).
1. Self-host **Inter variable** + **JetBrains Mono**; add `@font-face`, preload,
   `font-display: swap`.
2. Define role-based type tokens (`display/title/heading/body/caption/code`) on
   a modular scale; create a tiny `<Text as=… variant=…>` or utility classes;
   migrate page titles to one consistent role.
3. Standardize on **one icon set** (keep Feather/`solid-icons`); replace every
   emoji-as-icon with a labeled `<Icon>`; add `aria-label`s.
4. Add `--space-*`, `--duration-*`, `--ease-*` tokens + `prefers-reduced-motion`.
5. _Exit:_ consistent type rhythm; zero functional emoji; fonts load with no CLS.

### Phase 17 — Brand System _(differentiation; ~2–3 days)_
**Goal:** make FlexPM look like _something_ (**P5, P9**).
1. Write [docs/BRAND.md](BRAND.md): name story, personality, voice, color
   rationale, logo usage, do/don't.
2. Logo system: promote the bolt mark; build mark / horizontal lockup /
   monochrome SVGs; replace the "F" box in the sidebar and the mobile top bar.
3. Standardize on one brand purple + one cyan accent across favicon, tokens,
   lockup, and OG/social image.
4. Optional: migrate semantic palette to **OKLCH** with hex fallback.
5. _Exit:_ one mark, one purple, documented brand.

### Phase 18 — Component Polish & Density _(refinement; ~3 days)_
**Goal:** raise the ceiling on the views users live in.
1. Redesign Dashboard stat cards, Board cards, List rows against the new system
   (real icons, consistent elevation, tighter rhythm).
2. Add **density toggle** (comfortable/compact) via `--density-*` tokens.
3. Empty states, loading, and error states audited for consistency.
4. Micro-interactions: card hover lift, drag affordance, toast/drawer easing
   using motion tokens.
5. _Exit:_ flagship views feel cohesive and premium.

### Phase 19 — Accessibility & Living Docs _(durability; ~2 days)_
**Goal:** lock quality in so it doesn't regress (**P8**).
1. WCAG 2.2 AA pass: contrast audit (tokens make this a finite list), focus-
   visible everywhere, target sizes, AT labels, keyboard traps.
2. Ship a `/_kit` gallery route (or Histoire) rendering every component in light
   + dark — doubles as visual-regression surface.
3. Add an a11y smoke test (axe) to the Vitest suite.
4. _Exit:_ documented, contrast-verified, regression-guarded system.

### Sequencing summary

| Phase | Theme | Leverage | Risk | Status |
|-------|-------|----------|------|--------|
| 14 | Token plumbing + lint gate | ★★★★★ | low | ✅ done |
| 15 | Migrate raw literals | ★★★★☆ | low | ✅ done (288→0; P10 resolved) |
| 16 | Typography + icons | ★★★★☆ | low | next |
| 17 | Brand system | ★★★★☆ | med (design decisions) | — |
| 18 | Component polish + density | ★★★☆☆ | med | — |
| 19 | A11y + living docs | ★★★☆☆ | low | — |

---

## 6. Decisions

1. **Brand direction:** ✅ _Decided 2026-06-13_ — **fresh palette & mark**
   (explored + applied in Phase 17).
2. **Color model:** adopt OKLCH now (Phase 17) or stay hex for one more cycle?
   _(open)_
3. **Density:** is a comfortable/compact toggle in scope, or comfortable-only?
   _(open)_
4. **Docs tooling:** lightweight in-app `/_kit` route, or full Histoire/Storybook?
   _(open)_

> Implementation note: this project uses **Tailwind v4**, which is configured in
> CSS via the `@theme` directive — _not_ `tailwind.config.js` (now vestigial).
> Phase 14 therefore wires tokens through an `@theme inline` block in
> [index.css](../frontend/src/index.css), mapping Tailwind's color namespace to
> the runtime CSS vars so utilities switch with the `.dark` class automatically.

---

## 7. Quick Wins (can ship this week, independent of the roadmap)

- Load Inter (one `<link>` or self-host) — instant, visible identity upgrade.
- Replace the 5 Dashboard emoji stat-icons with Feather icons.
- Swap the sidebar "F" box for the real favicon bolt mark.
- Unify the three purples to `#7C3AED` in `index.css` + favicon.
- Add the CI grep against raw color literals (prevents the problem growing).
