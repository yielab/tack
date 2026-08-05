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
