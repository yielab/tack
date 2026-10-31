#!/usr/bin/env bash
# Fails when a workspace crate has a normal dependency on one this table does not allow.
# dev-dependencies are exempt: every crate may test with tack-test-support.
# A second check keeps the SQL driver and the raw pool inside tack-db and tack-test-support.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.."
declare -A ALLOW=(
  [tack-protocol]=""
  [tack-core]="tack-protocol"
  [tack-orch]="tack-protocol tack-core"
  [tack-db]="tack-protocol tack-core"
  [tack-app]="tack-protocol tack-core tack-orch tack-db"
  [tack-api]="tack-protocol tack-core tack-db tack-app"
  [tack-runner]="tack-protocol tack-core"
  [tack-cli]="tack-api tack-app tack-runner tack-db tack-protocol tack-core"
  [tack-test-support]="tack-protocol tack-core tack-db"
)
if [ "${1:-}" = "--print" ]; then for k in "${!ALLOW[@]}"; do echo "$k -> ${ALLOW[$k]:-(none)}"; done | sort; exit 0; fi
fail=0
while IFS=$'\t' read -r from to; do
  case " ${ALLOW[$from]-} " in *" $to "*) ;; *) echo "forbidden edge: $from -> $to"; fail=1;; esac
done < <(cargo metadata --format-version 1 --no-deps \
  | jq -r '.packages[] | .name as $n | .dependencies[] | select(.path != null and .kind == null) | [$n, .name] | @tsv')

# The SQL driver and `Repository::pool()` have two homes. Cargo unifies features, so the
# `raw-pool` feature cannot keep other crates out; this check does.
while IFS= read -r hit; do
  echo "outside tack-db and tack-test-support: $hit"; fail=1
done < <(git ls-files 'crates/*.rs' 'crates/*/Cargo.toml' \
  | grep -vE '^crates/(tack-db|tack-test-support)/' \
  | xargs -r grep -nE '\.pool\(\)|sqlx' /dev/null)
exit $fail
