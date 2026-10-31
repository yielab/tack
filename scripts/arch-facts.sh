#!/usr/bin/env bash
# Prints the counts the architecture documents point to instead of quoting, then the
# allowed crate edges. Run from anywhere; it works from the repo root.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.."
echo "crates: $(cargo metadata --format-version 1 --no-deps | jq '.workspace_members | length')"
echo "documented paths: $(jq '.paths | length' docs/openapi.json)"
echo "migrations: $(grep -oE '"[0-9]{3}_[a-zA-Z0-9_]+"' crates/tack-db/src/migrations.rs | sort -u | wc -l)"
echo "tests listed: $(cargo nextest list --workspace 2>/dev/null | wc -l)"
scripts/check-crate-graph.sh --print
