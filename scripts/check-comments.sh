#!/usr/bin/env bash
# Fails when a comment cites a file that does not exist — a renamed or merged
# module turning every citation of its old name into a dead end. The rule is
# in CONTRIBUTING.md, "Code Style"; this script is what keeps it mechanical.
#
# Scope: comments, doc comments and human-readable strings under crates/
# (*.rs), frontend/src (*.ts, *.tsx) and frontend/e2e (*.ts).
# Usage: scripts/check-comments.sh [path ...]   (default: crates/ frontend/src frontend/e2e)
set -uo pipefail

if [ "$#" -eq 0 ]; then
  ROOTS=(crates/ frontend/src frontend/e2e)
else
  ROOTS=("$@")
fi
INCLUDES=(--include='*.rs' --include='*.ts' --include='*.tsx')

# Names that look like a file reference but are prose. Each needs a reason.
readonly ALLOW_CITED='
foo.rs
'

# A comment that points at a file must point at a file that exists. Renaming or
# merging a module silently turns every citation of its old name into a dead end,
# and a reader who follows two dead pointers stops trusting the third.
#
# Matches on basename, so it catches a removed or renamed file rather than a
# wrong directory (checked against every tracked .rs/.ts/.tsx file in the repo,
# not just the scanned roots, since a TS comment routinely cites a Rust handler
# and vice versa) — never a path relative to the comment's own file, so nothing
# here depends on which root is scanned.
#
# A citation broken across a line wrap (a filename ending one line and
# continuing on the next, e.g. `provider-key-panel` / `.spec.ts`) yields a
# fragment such as "spec.ts": not a real name, so "dead" by the comm below —
# and then, once that fragment becomes part of a search pattern, a substring
# match on every genuine "*.spec.ts" citation in the scanned roots, reporting
# real, checked-in files as missing. Anchoring the search cannot fix this:
# every real filename also ends in ".ts"/".tsx"/".rs", so no anchor tells a
# fragment's tail from a whole name's tail. The fix is catching it earlier —
# a "dead" candidate that is itself the tail of some real tracked filename is
# exactly what a split citation produces, and is dropped before it is ever
# turned into a pattern.
dead_pointers() {
  local known cited
  known=$(git ls-files '*.rs' '*.ts' '*.tsx' 2>/dev/null | sed 's#.*/##' | sort -u)
  [ -z "$known" ] && return 0
  cited=$(grep -rhE '^[[:space:]]*\{?[[:space:]]*(//|///|//!|/\*|\*)' "${INCLUDES[@]}" "${ROOTS[@]}" 2>/dev/null \
    | grep -oE '[A-Za-z0-9_.-]+\.(rs|tsx?)' | sort -u \
    | grep -vxFf <(printf '%s\n' "$ALLOW_CITED" | grep -v '^$') || true)
  [ -z "$cited" ] && return 0
  local dead candidate real_dead=""
  dead=$(comm -23 <(printf '%s\n' "$cited") <(printf '%s\n' "$known"))
  [ -z "$dead" ] && return 0
  while IFS= read -r candidate; do
    [ -z "$candidate" ] && continue
    if printf '%s\n' "$known" | grep -qE -- "$(printf '%s' "$candidate" | sed 's/[.[\*^$]/\\&/g')\$"; then
      continue  # tail of a real filename: a line-wrapped citation, not a stale one
    fi
    real_dead="${real_dead}${candidate}
"
  done <<< "$dead"
  [ -z "$real_dead" ] && return 0
  grep -rnE "$(printf '%s\n' "$real_dead" | grep -v '^$' | sed 's/\./\\./g' | paste -sd'|')" \
    "${INCLUDES[@]}" "${ROOTS[@]}" 2>/dev/null \
    | grep -E ':[[:space:]]*\{?[[:space:]]*(//|///|//!|/\*|\*)' || true
}

hits="$(dead_pointers)"

if [ -n "$hits" ]; then
  printf '\n\033[1m%s\033[0m\n' "Pointers to files that do not exist"
  printf '  %s\n\n' "The named file was renamed or merged away. Repoint it to the new path, or state the fact the name stood in for — the second survives the next reorganisation."
  printf '%s\n' "$hits" | sed 's/^/  /'
  echo
  echo "A genuine exception goes in this script's ALLOW_CITED list with its reason, not in a silenced line."
  exit 1
fi

echo "✓ no dead file pointers in ${ROOTS[*]}"
