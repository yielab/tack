# Appearance

Tack ships a two-axis theme system: a **mode** (light or dark) and a **palette**
(the accent + surface colour family). Both are controlled from the bottom of the
sidebar and take effect instantly across the whole app.

The controls live in the **sidebar footer**: a sun/moon button toggles the mode,
and three coloured dots switch the palette.

---

## Mode — light / dark

Click the **sun/moon button** in the sidebar footer to flip between light and dark.

Until you pick one explicitly, Tack follows your operating system's
`prefers-color-scheme` setting. The first toggle pins an explicit choice that
overrides the OS preference from then on.

## Palette

Three palettes ship, each available in light and dark:

| Palette | Accent | Feel |
|---------|--------|------|
| **Teal** (default) | teal | calm, the default brand |
| **Clay** | warm terracotta | warm, earthy |
| **Graphite** | lime on neutral grey | high-contrast, understated |

Click a swatch in the sidebar footer to switch. Every surface, accent, badge, and
chart re-colours immediately — there is no reload and no per-view setting.

---

## How it's stored

Both choices are saved in the browser's `localStorage`:

- `tack_theme` → `light` \| `dark` \| `system`
- `tack_palette` → `teal` \| `clay` \| `graphite`

Because Tack is local-first and single-user, appearance is **per browser** — it is
not stored in the database and not synced between machines. Clearing site data
resets both to their defaults (system mode, Teal palette).

## Accessibility

All palette/mode combinations are tuned to meet **WCAG 2.1 AA** contrast (4.5:1 for
text), and the choice is verified automatically by an axe accessibility scan in
CI. If you fork Tack and change the colour tokens, keep that bar in mind — see
[Frontend & Design System](../developer/frontend.md) for where the tokens live.
