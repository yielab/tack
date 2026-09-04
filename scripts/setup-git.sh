#!/usr/bin/env bash
# One-time local git setup for this clone: the hook path and the merge driver
# for generated files. Both are per-clone git config, which cannot be committed,
# which is why this script exists rather than a checked-in config file.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

git config core.hooksPath .githooks
git config merge.tack-generated.name "regenerate rather than hand-merge (see .gitattributes)"
git config merge.tack-generated.driver "./scripts/git-merge-generated.sh %A %P"

echo "✓ hooks:        .githooks (pre-push gate, post-merge regeneration)"
echo "✓ merge driver: tack-generated for Cargo.lock, package-lock.json, openapi.json, schema.gen.ts"
