#!/usr/bin/env bash
# Generates deny.toml — the single source of truth for `cargo deny`'s policy,
# used identically by `make deny` and the CI `deny` job so the two can never
# drift. Not committed itself (see CLAUDE.md: generated artifacts stay out of
# version control); regenerated every run instead.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cat > deny.toml <<'EOF'
[graph]
all-features = true

[bans]
# Duplicate (multiple-version) dependencies are reported as an
# advisory (warning), not a hard failure.
multiple-versions = "warn"
wildcards = "allow"

[licenses]
version = 2
confidence-threshold = 0.8
allow = [
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "MPL-2.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "Unlicense",
  "0BSD",
  "BSL-1.0",
  "OpenSSL",
  "CDLA-Permissive-2.0",
]
EOF
