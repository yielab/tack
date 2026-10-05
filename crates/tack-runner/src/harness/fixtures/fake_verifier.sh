#!/bin/sh
# Fake verifier for the runner's `[verify]` tests. POSIX `sh`.
#
# Called as `<program> <args...> --evidence DIR --workspace DIR --output FILE`.
# The child environment is `PATH` only, so a test sets the knobs through
# `env`: program `env`, args `TACK_FAKE_VERIFIER_FIXTURE=ready sh <this file>`.
#
#   TACK_FAKE_VERIFIER_EXIT_CODE   when set, exits with it and writes nothing.
#   TACK_FAKE_VERIFIER_FIXTURE     otherwise, copies
#                                  docs/contracts/mrp-v1/fixtures/<name>.json
#                                  to `--output` (default name: ready).

output=""
while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output" ]; then
        output="$2"
        shift
    fi
    shift
done

if [ -n "${TACK_FAKE_VERIFIER_EXIT_CODE:-}" ]; then
    echo "fake verifier failing on request" >&2
    exit "$TACK_FAKE_VERIFIER_EXIT_CODE"
fi

here=$(cd "$(dirname "$0")" && pwd)
fixture="$here/../../../../../docs/contracts/mrp-v1/fixtures/${TACK_FAKE_VERIFIER_FIXTURE:-ready}.json"
cp "$fixture" "$output"
