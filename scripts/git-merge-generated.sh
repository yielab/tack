#!/usr/bin/env bash
# Git merge driver for generated files (registered by scripts/setup-git.sh,
# selected by `merge=tack-generated` in .gitattributes).
#
# Git calls this with temporary files, not the real paths, so regenerating here
# is impossible — the tools write to the real path, which git has not finished
# updating. Instead this resolves to our side and exits clean; the post-merge
# hook regenerates from the merged sources immediately afterwards. Taking a side
# is safe precisely because these files hold no information of their own.
#
#   $1 = %A  our version, and the file the result must be left in
#   $2 = %P  the pathname being merged
set -euo pipefail

ours="$1"
path="${2:-a generated file}"

# %A already holds our version, so resolving to ours means leaving it alone.
echo "  merge: resolved '$path' to ours; post-merge will regenerate it" >&2
[ -f "$ours" ]
