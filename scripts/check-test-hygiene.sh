#!/usr/bin/env bash
# Fails when test code builds its own path under the OS temporary directory
# instead of taking a `tempfile` guard.
#
# A hand-built path leaks two ways. It is removed by a statement at the end of
# the test, so a failing assertion skips the removal; and the removal has to
# name each file, so anything the code under test writes *beside* the path --
# SQLite's `-wal` and `-shm`, the migration runner's pre-upgrade snapshot --
# is not named and stays. Measured before this rule existed: one green run of
# the suite left 83 entries behind, and 4,499 had accumulated.
#
# `tempfile::tempdir()` has neither failure mode: the directory and everything
# under it go when the guard drops, panic or not.
#
# Scope: whole files under crates/*/tests/, plus the `#[cfg(test)] mod tests`
# tail of any crate source file. Production code may use the temp directory --
# it is what `TMPDIR` is for -- and is not scanned.
#
# Usage: scripts/check-test-hygiene.sh
set -uo pipefail

status=0

# Line where a file's trailing test module starts, or nothing when it has
# none. Anchored on the module, not on any `#[cfg(test)]`: a `#[cfg(test)]`
# constructor can sit anywhere in the production half of a file.
test_module_line() {
  awk '/^#\[cfg\(test\)\]$/ { pending = NR; next }
       /^mod tests/ { if (pending == NR - 1) { print NR; exit } }
       { pending = 0 }' "$1"
}

hits=$(
  for file in $(git ls-files 'crates/*.rs'); do
    case "$file" in
      */tests/*) start=0 ;;
      *) start=$(test_module_line "$file"); [ -z "$start" ] && continue ;;
    esac
    awk -v start="$start" -v file="$file" \
      'NR > start && /env::temp_dir\(\)/ { printf "%s:%d: %s\n", file, NR, $0 }' "$file"
  done
)

if [ -n "$hits" ]; then
  status=1
  printf '\n\033[1m%s\033[0m\n' "Test code building its own temporary path"
  printf '  %s\n\n' "Take a guard instead: \`let dir = tempfile::tempdir().expect(\"temporary directory\");\`, then \`dir.path()\`. Hold the guard for as long as anything reads the path — a helper that returns only the path deletes the directory as it returns."
  printf '%s\n' "$hits" | sed 's/^/  /'
fi

[ "$status" -eq 0 ] && echo "✓ tests take their temporary paths from a guard"
exit "$status"
