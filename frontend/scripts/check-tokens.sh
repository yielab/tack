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
