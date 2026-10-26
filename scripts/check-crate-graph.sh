#!/usr/bin/env bash
# Fails when a workspace crate has a normal dependency on one this table does not allow.
# dev-dependencies are exempt: every crate may test with tack-test-support.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.."
declare -A ALLOW=(
  [tack-protocol]=""
  [tack-core]="tack-protocol"
  [tack-db]="tack-core tack-protocol"
  [tack-orch]="tack-core tack-db tack-protocol"
  [tack-app]="tack-core tack-db tack-orch tack-protocol"
  [tack-api]="tack-app tack-core tack-db tack-orch tack-protocol"
  [tack-runner]="tack-core tack-protocol"
  [tack-cli]="tack-api tack-db tack-runner tack-orch tack-protocol"
  [tack-test-support]="tack-core tack-db tack-protocol"
)
if [ "${1:-}" = "--print" ]; then for k in "${!ALLOW[@]}"; do echo "$k -> ${ALLOW[$k]:-(none)}"; done | sort; exit 0; fi
fail=0
while IFS=$'\t' read -r from to; do
  case " ${ALLOW[$from]-} " in *" $to "*) ;; *) echo "forbidden edge: $from -> $to"; fail=1;; esac
done < <(cargo metadata --format-version 1 --no-deps \
  | jq -r '.packages[] | .name as $n | .dependencies[] | select(.path != null and .kind == null) | [$n, .name] | @tsv')
exit $fail
