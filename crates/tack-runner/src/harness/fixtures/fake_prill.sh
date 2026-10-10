#!/bin/sh
# Fake prill for the runner's `[verify]` tests with `kind = "prill"`. POSIX `sh`.
#
# Called as `<program> verify --repo DIR --evidence FILE [--brief FILE]
# --out DIR --json <configured args...>`. The child environment is `PATH`
# only, so a test sets the knobs through a one-line launcher it writes:
# `exec env TACK_FAKE_PRILL_FIXTURE=... sh <this file> "$@"`.
#
#   TACK_FAKE_PRILL_ERROR    when set, prints an `error/v1` document with it
#                            as the code on stdout and exits 2, as prill does
#                            when it can emit no pack.
#   TACK_FAKE_PRILL_FIXTURE  otherwise, copies
#                            docs/contracts/mrp-v1/fixtures/prill/<name>.json
#                            to `<out>/mrp.json`, prints it on stdout and
#                            exits 1 when its decision is `reject`, else 0
#                            (default name: m2-sections).
#
# Whatever it answers, it records its whole argument list, one per line, in
# `<out>/argv` so a test can see the command line it was given.

out=""
previous=""
for arg in "$@"; do
    if [ "$previous" = "--out" ]; then
        out="$arg"
    fi
    previous="$arg"
done
[ -n "$out" ] || { echo "fake prill: no --out" >&2; exit 2; }
mkdir -p "$out"
printf '%s\n' "$@" > "$out/argv"

if [ -n "${TACK_FAKE_PRILL_ERROR:-}" ]; then
    printf '{"contract": "error/v1", "code": "%s", "message": "fake prill refusing on request"}\n' "$TACK_FAKE_PRILL_ERROR"
    echo "prill: $TACK_FAKE_PRILL_ERROR: fake prill refusing on request" >&2
    exit 2
fi

here=$(cd "$(dirname "$0")" && pwd)
fixture="$here/../../../../../docs/contracts/mrp-v1/fixtures/prill/${TACK_FAKE_PRILL_FIXTURE:-m2-sections}.json"
cp "$fixture" "$out/mrp.json"
echo "# fake pack" > "$out/mrp.md"
cat "$fixture"
if grep -q '"decision": "reject"' "$fixture"; then
    exit 1
fi
exit 0
