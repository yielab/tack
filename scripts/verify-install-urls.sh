#!/usr/bin/env bash
# Resolves every URL the docs advertise as a way to get tack, and fails if any
# of them doesn't come back with a 2xx. This is the only thing in the repo
# that would have caught the install one-liner pointing at a branch that
# didn't exist — nothing else touches these files together.
#
# Run from the repository root: scripts/verify-install-urls.sh
#
# If a new page starts advertising an install command, add its path to FILES.
set -euo pipefail

FILES=(
  README.md
  docs/DEPLOYMENT-GUIDE.md
  docs/book/src/user-guide/quick-start.md
  docs/book/src/roadmap.md
  install.sh
)

# The one-line installer's raw-content URL (whichever branch it names), and
# the releases page linked as the alternative "download a release" method.
pattern='https://(raw\.githubusercontent\.com/yielab/tack/[^[:space:]")]+|github\.com/yielab/tack/releases)'

mapfile -t urls < <(grep -hoE "$pattern" "${FILES[@]}" | sort -u)

if [ "${#urls[@]}" -eq 0 ]; then
  echo "verify-install-urls: found no install URLs in: ${FILES[*]}" >&2
  echo "verify-install-urls: the file list above is probably stale" >&2
  exit 1
fi

echo "Checking ${#urls[@]} install URL(s):"
printf '  %s\n' "${urls[@]}"
echo

fail=0
for url in "${urls[@]}"; do
  code="$(curl -s -o /dev/null -w '%{http_code}' -L --max-time 15 "$url" || echo 000)"
  if [ "$code" -ge 200 ] && [ "$code" -lt 300 ]; then
    printf 'OK   [%s] %s\n' "$code" "$url"
  else
    printf 'FAIL [%s] %s\n' "$code" "$url"
    fail=1
  fi
done

exit "$fail"
