#!/usr/bin/env bash
# Regenerates every file .gitattributes marks `merge=tack-generated`, from the
# sources they are derived from. The single place that knows how, so the
# post-merge hook, the pre-push gate and a human all do the same thing.
#
#   --fast   skip anything that needs a Rust build (used by the pre-push gate,
#            where CI's `openapi_contract` test already covers the slow half)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

fast=false
[ "${1:-}" = "--fast" ] && fast=true

# Cargo.lock — resolving alone brings it back in line with the merged manifests.
cargo metadata --format-version 1 >/dev/null

if [ "$fast" = false ]; then
  # docs/openapi.json — written by the contract test itself under UPDATE_OPENAPI.
  UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract >/dev/null
fi

# frontend/src/shared/api/schema.gen.ts — derived from docs/openapi.json.
if [ -d frontend/node_modules ]; then
  npm --prefix frontend run gen:api >/dev/null
fi
