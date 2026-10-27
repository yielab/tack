#!/usr/bin/env bash
#
# Design-token lint gate (Phase 14).
#
# Counts raw Tailwind color literals (e.g. bg-gray-800, text-purple-600) in the
# frontend source. These bypass the design-token system documented in
# src/index.css and docs/DESIGN-ROADMAP.md, and break the .dark-class theme
# toggle (see finding P10). Use token utilities instead: bg-surface,
# text-content, border-line, bg-brand, bg-success-100, …
#
# This is a RATCHET: the count may never exceed BASELINE. As files migrate
# (Phase 15) the real count drops below BASELINE — when it does, lower BASELINE
# to the new number to lock the progress in. Target is 0.
set -euo pipefail

BASELINE=0

cd "$(dirname "$0")/.."

PATTERN='(text|bg|border|ring|from|to|via|fill|stroke|divide|placeholder|outline|decoration|shadow|accent|caret)-(gray|slate|zinc|neutral|stone|purple|violet|indigo|fuchsia|red|rose|green|emerald|teal|blue|sky|cyan|yellow|amber|orange|pink|lime)-([0-9]{2,3})(/[0-9]+)?'

# `|| true`: grep exits 1 when there are zero matches, which would abort the
# script under `set -o pipefail`. Zero is the success case here.
COUNT=$(grep -rhoE "$PATTERN" src --include='*.tsx' --include='*.ts' | wc -l | tr -d ' ' || true)

echo "Raw color literals: $COUNT (baseline $BASELINE, target 0)"

if [ "$COUNT" -gt "$BASELINE" ]; then
  echo ""
  echo "ERROR: raw color-literal count increased ($COUNT > $BASELINE)."
  echo "Use design tokens instead of raw Tailwind colors:"
  echo "  bg-white/dark:bg-gray-800  → bg-surface / bg-elevated"
  echo "  text-gray-900/dark:text-white → text-content"
  echo "  border-gray-200            → border-line"
  echo "  bg-purple-600              → bg-brand"
  echo "See docs/DESIGN-ROADMAP.md (Phase 15)."
  exit 1
fi

if [ "$COUNT" -lt "$BASELINE" ]; then
  echo "Progress: count dropped below baseline."
  echo "→ Lower BASELINE in scripts/check-tokens.sh to $COUNT to lock it in."
fi

echo "✓ token gate passed"

# ── Gate 2: raw hex color literals in inline `style` props ────────────────────
#
# Inline styles like `color: '#ef4444'` or `border: '2px solid #ef4444'` bypass
# the two-axis token system: they ignore .dark mode and the Clay/Graphite
# palettes. Use tokens (var(--color-*)) or the shared priorityColor() helper.
#
# Same RATCHET rule as above. Test fixtures carry arbitrary color *data* (not
# styling), so they're excluded.
STYLE_BASELINE=0

# A hex literal used as a CSS value: right after a property colon, a border/
# outline shorthand keyword, or the else-branch of a style ternary.
STYLE_PATTERN="(:|solid|dashed|dotted)[[:space:]]*['\"]?#[0-9a-fA-F]{3,8}"

STYLE_COUNT=$(grep -rhoE "$STYLE_PATTERN" src --include='*.tsx' --include='*.ts' \
  | wc -l | tr -d ' ' || true)
# Subtract test-fixture matches (color data, not styling).
STYLE_TEST=$(grep -rhoE "$STYLE_PATTERN" src --include='*.test.tsx' --include='*.test.ts' \
  | wc -l | tr -d ' ' || true)
STYLE_COUNT=$((STYLE_COUNT - STYLE_TEST))

echo "Inline-style hex literals: $STYLE_COUNT (baseline $STYLE_BASELINE, target 0)"

if [ "$STYLE_COUNT" -gt "$STYLE_BASELINE" ]; then
  echo ""
  echo "ERROR: inline-style hex-literal count increased ($STYLE_COUNT > $STYLE_BASELINE)."
  echo "Use design tokens or the priorityColor() helper instead of raw hex:"
  echo "  color: '#ef4444'          → color: 'var(--color-danger-600)'"
  echo "  '2px solid #ef4444'       → '2px solid var(--color-danger-600)'"
  echo "  priority colors           → priorityColor(item.priority) (src/shared/ui/PriorityDot.tsx)"
  exit 1
fi

if [ "$STYLE_COUNT" -lt "$STYLE_BASELINE" ]; then
  echo "Progress: inline-style hex count dropped below baseline."
  echo "→ Lower STYLE_BASELINE in scripts/check-tokens.sh to $STYLE_COUNT to lock it in."
fi

echo "✓ inline-style hex gate passed"

# ── Gate 3: font sizes in px ──────────────────────────────────────────────────
#
# Text follows the user's text size (Settings → Display & accessibility) only
# when it is sized in rem: the setting moves the root font size, on top of the
# browser's own default. A px size stays put and breaks that. Write
# `'font-size': '0.8125rem'` or `text-[0.8125rem]`, not 13px.
#
# The baseline counts the one deliberate exception: the sidebar's identity
# mark, a letter inside a circle of fixed px size that would overflow it if it
# grew. (Avatar.tsx's initials are the same case, sized through a map.)
FONT_PX_BASELINE=1

FONT_PX_COUNT=$(grep -rhoE "'font-size': ?'[0-9.]+px'|text-\[[0-9.]+px\]" src --include='*.tsx' --include='*.ts' \
  --exclude='*.test.tsx' --exclude='*.test.ts' | wc -l | tr -d ' ' || true)

echo "Font sizes in px: $FONT_PX_COUNT (baseline $FONT_PX_BASELINE)"

if [ "$FONT_PX_COUNT" -gt "$FONT_PX_BASELINE" ]; then
  echo ""
  echo "ERROR: font sizes in px increased ($FONT_PX_COUNT > $FONT_PX_BASELINE)."
  echo "Size text in rem so it follows the user's text size:"
  echo "  'font-size': '13px'  → 'font-size': '0.8125rem'"
  echo "  text-[13px]          → text-[0.8125rem]   (px ÷ 16)"
  exit 1
fi

echo "✓ font-size gate passed"

# ── Gate 4: arbitrary Tailwind values ─────────────────────────────────────────
#
# A value written in brackets (`rounded-[28px]`, `text-[0.8125rem]`) does not come
# from the theme, so it drifts from the rest. Use the token when there is one
# (rounded-card, rounded-item, text-micro, text-caption, text-small, text-lead:
# the @theme static block in src/index.css) or a Tailwind scale class.
#
# Same RATCHET rule. Test files are excluded.
ARBITRARY_BASELINE=235

ARBITRARY_COUNT=$(grep -rhoE '[a-z0-9]-\[[^]]+\]' src --include='*.tsx' --include='*.ts' \
  --exclude='*.test.tsx' --exclude='*.test.ts' | wc -l | tr -d ' ' || true)

echo "Arbitrary Tailwind values: $ARBITRARY_COUNT (baseline $ARBITRARY_BASELINE, target 0)"

if [ "$ARBITRARY_COUNT" -gt "$ARBITRARY_BASELINE" ]; then
  echo ""
  echo "ERROR: arbitrary Tailwind value count increased ($ARBITRARY_COUNT > $ARBITRARY_BASELINE)."
  echo "Use a theme token or a scale class instead of a bracketed value:"
  echo "  rounded-[28px]        → rounded-card"
  echo "  rounded-[20px]        → rounded-item"
  echo "  text-[0.8125rem]      → text-small"
  echo "See the @theme static block in src/index.css."
  exit 1
fi

if [ "$ARBITRARY_COUNT" -lt "$ARBITRARY_BASELINE" ]; then
  echo "Progress: arbitrary value count dropped below baseline."
  echo "→ Lower ARBITRARY_BASELINE in scripts/check-tokens.sh to $ARBITRARY_COUNT to lock it in."
fi

echo "✓ arbitrary-value gate passed"
