#!/usr/bin/env bash
# Fails when Rust source carries the project's own history instead of an
# explanation of the code. The rule is in CLAUDE.md, "Code style"; this
# script is what stops it decaying between the times somebody remembers it.
#
# Prose in a rules file lasted two weeks before 234 board citations crept
# back into doc comments, and some reached operator logs and API error
# responses. A grep that runs on every push does not forget.
#
# Scope: comments, doc comments and human-readable strings under crates/.
# Usage: scripts/check-comments.sh [path ...]   (default: crates/)
set -uo pipefail

ROOTS=("${@:-crates/}")
status=0

# Paths whose names or fixtures legitimately contain a flagged word. Each
# entry needs a reason; an allowlist nobody can justify is a disabled check.
readonly ALLOW='
crates/tack-api/tests/wave2_gate.rs
'

# Names that look like a file reference but are prose. Each needs a reason.
readonly ALLOW_CITED='
foo.rs
'

report() {
  local title=$1 explanation=$2 hits=$3
  [ -z "$hits" ] && return 0
  status=1
  printf '\n\033[1m%s\033[0m\n' "$title"
  printf '  %s\n\n' "$explanation"
  printf '%s\n' "$hits" | sed 's/^/  /'
}

# Only lines that are comments or string literals; skips code that happens to
# contain a word (a `wave` variable, a `card` struct field).
commentish() {
  grep -rnE "$1" --include='*.rs' "${ROOTS[@]}" 2>/dev/null \
    | grep -E ':[[:space:]]*(//|///|//!)|"' \
    | grep -vFf <(printf '%s\n' "$ALLOW" | grep -v '^$') || true
}

report "Board citations" \
  "A reader with the code but not the board cannot use these. State the rule, bar or hazard itself." \
  "$(commentish 'TODO\.md')"

report "Card ids" \
  "Card names mean nothing outside the board. Say what the code does instead." \
  "$(commentish '\b(I{1,3}|IV|V|VI{1,3})-[A-H][0-9]\b')"

report "Board vocabulary in prose" \
  "'this card', 'the acceptance bar', 'the handoff' are scaffolding from whoever wrote the code. Keep the knowledge, drop the pointer." \
  "$(commentish '\b([Tt]his card|[Tt]he card'"'"'s|acceptance bar|the handoff|the integrator)\b')"

report "Dates" \
  "A date in a comment is history; git log already has it. Exception: a date that is itself test data." \
  "$(grep -rnE '^[[:space:]]*(//|///|//!).*[0-9]{4}-[0-9]{2}-[0-9]{2}' --include='*.rs' "${ROOTS[@]}" 2>/dev/null \
      | grep -viE 'fixture|since = |sample|example|"20' || true)"

# Matches crediting a model for the code, not naming one. "Claude Code" and
# "Codex" are harnesses this crate drives, so they appear legitimately on
# almost every line of the runner — a pattern that flags those is a pattern
# somebody switches off.
report "AI attribution" \
  "This project does not credit a model for its code. Naming a harness (Claude Code, Codex) is fine; claiming one wrote this is not." \
  "$(commentish '(Co-Authored-By|Generated with|Co-authored-by).*(Claude|Copilot|GPT)|🤖|\b(written|authored|generated) by (an? )?(AI|model|agent|assistant|Claude|GPT|Copilot)\b')"

# A comment that points at a file must point at a file that exists. Renaming or
# merging a module silently turns every citation of its old name into a dead end,
# and a reader who follows two dead pointers stops trusting the third.
#
# Matches on basename, so it catches a removed or renamed file rather than a
# wrong directory. A filename broken across a line wrap hides from this check —
# which is a reason not to wrap one.
dead_pointers() {
  local known cited
  known=$(git ls-files '*.rs' 2>/dev/null | sed 's#.*/##' | sort -u)
  [ -z "$known" ] && return 0
  cited=$(grep -rhE '^[[:space:]]*(//|///|//!)' --include='*.rs' "${ROOTS[@]}" 2>/dev/null \
    | grep -oE '[A-Za-z0-9_.-]+\.rs' | sort -u \
    | grep -vxFf <(printf '%s\n' "$ALLOW_CITED" | grep -v '^$') || true)
  [ -z "$cited" ] && return 0
  local dead
  dead=$(comm -23 <(printf '%s\n' "$cited") <(printf '%s\n' "$known"))
  [ -z "$dead" ] && return 0
  grep -rnE "$(printf '%s\n' "$dead" | sed 's/\./\\./g' | paste -sd'|')" \
    --include='*.rs' "${ROOTS[@]}" 2>/dev/null \
    | grep -E ':[[:space:]]*(//|///|//!)' || true
}

report "Pointers to files that do not exist" \
  "The named file was renamed or merged away. Repoint it to the new path, or state the fact the name stood in for — the second survives the next reorganisation." \
  "$(dead_pointers)"

if [ "$status" -ne 0 ]; then
  cat <<'EOF'

──────────────────────────────────────────────────────────────────────────
The rule, from CLAUDE.md:

  Comments explain the code, never the project's history. Write what the
  code does when the name doesn't say it, why a non-obvious choice was
  made, what breaks if you change it, and what isn't true yet.

Deleting the comment is usually the wrong fix — most of these wrap
something real in scaffolding. Keep the knowledge, drop the pointer.

A genuine exception goes in this script's ALLOW list with its reason, not
in a silenced line.
EOF
else
  echo "✓ no board archaeology in ${ROOTS[*]}"
fi

exit "$status"
