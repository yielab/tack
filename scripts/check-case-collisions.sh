#!/usr/bin/env bash
# Two tracked files in one folder whose names differ only by case, or whose stems do
# (`Flow.tsx` beside `flow.ts`), are one file on macOS and Windows: an import of either
# resolves to whichever the filesystem finds first, and the release build there breaks.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
git ls-files | python3 -c '
import collections, os, sys
seen = collections.defaultdict(set)
for path in sys.stdin:
    folder, name = os.path.split(path.strip())
    seen[(folder.lower(), name.split(".")[0].lower())].add(os.path.join(folder, name.split(".")[0]))
clashes = sorted(sorted(group) for group in seen.values() if len(group) > 1)
for group in clashes:
    print("  differ only by case: " + "  ".join(group), file=sys.stderr)
sys.exit(1 if clashes else 0)
'
