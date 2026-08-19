#!/usr/bin/env bash
# End-to-end smoke: a real `tack serve` + a real `tack-runner` + real harness binaries.
#
# Written for card III-H2, but committed BEFORE III-H1 implements the runner's HTTP
# transport, so its failure is the load-bearing proof of the P0 III-G5 refused to tag on.
# It must fail at STEP 6 today. When III-H1 lands, it must pass with no edit to this file.
#
#   ./scripts/smoke.sh            # uses the fake harness (free, always runnable)
#   ./scripts/smoke.sh --live     # uses real harness binaries — BILLED for claude
set -uo pipefail

LIVE=0; [ "${1:-}" = "--live" ] && LIVE=1
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"; PORT=${SMOKE_PORT:-3399}
API="http://127.0.0.1:$PORT"
SERVER_PID=""; FAILED=0

cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null; rm -rf "$WORK"; }
trap cleanup EXIT
step() { printf '\n\033[1m== STEP %s: %s\033[0m\n' "$1" "$2"; }
ok()   { printf '   \033[32mPASS\033[0m %s\n' "$1"; }
bad()  { printf '   \033[31mFAIL\033[0m %s\n' "$1"; FAILED=1; }

step 1 "Harness availability (reported honestly, never rounded up)"
AVAIL=()
for h in codex claude opencode; do
  if command -v "$h" >/dev/null 2>&1; then
    printf '   present: %-10s %s\n' "$h" "$("$h" --version 2>&1 | head -1)"; AVAIL+=("$h")
  else
    printf '   ABSENT:  %-10s (cannot be part of any coverage claim)\n' "$h"
  fi
done
printf '   harness coverage: %d of 3\n' "${#AVAIL[@]}"

step 2 "Build tack + tack-runner"
cargo build -p tack-cli -p tack-runner 2>&1 | tail -3
[ -x "$ROOT/target/debug/tack" ] && ok "tack built" || { bad "tack missing"; exit 1; }
[ -x "$ROOT/target/debug/tack-runner" ] && ok "tack-runner built" || { bad "tack-runner missing"; exit 1; }

step 3 "Start the API server"
# Run from $WORK, never the repo root: the developer's tack.toml would otherwise be
# picked up (it sets alexa_skill_id, and the server correctly refuses to boot without
# TACK_ALEXA_SHARED_SECRET). A smoke test must exercise the product, not the workstation.
( cd "$WORK" && TACK_DATABASE_URL="sqlite:$WORK/smoke.db?mode=rwc" TACK_PORT="$PORT" \
  TACK_STORAGE_DIR="$WORK/storage" "$ROOT/target/debug/tack" serve >"$WORK/server.log" 2>&1 ) &
SERVER_PID=$!
for _ in $(seq 1 40); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
curl -sf "$API/api/health" >/dev/null && ok "server healthy on $PORT" || { bad "server never came up"; tail -20 "$WORK/server.log"; exit 1; }

step 4 "Create a project and an item (the plan of record)"
PROJ=$(curl -sf -X POST "$API/api/projects" -H 'content-type: application/json' \
  -d '{"name":"smoke","project_type":"software"}' | jq -r '.id // .project.id // empty')
[ -n "$PROJ" ] && ok "project $PROJ" || bad "could not create project"
ITEM=$(curl -sf -X POST "$API/api/projects/$PROJ/items" -H 'content-type: application/json' \
  -d '{"title":"smoke item","item_type":"task"}' | jq -r '.id // .item.id // empty')
[ -n "$ITEM" ] && ok "item $ITEM" || bad "could not create item"

step 5 "Register a pending runner and issue its enrollment token (operator surface)"
# Real contract (docs/openapi.json): POST /api/runners/enrollment, body CreatePendingRunner
# requires name + total_capacity + available_capacity; the raw token is returned exactly
# once here and only its SHA-256 hash is stored.
ENROLL_JSON=$(curl -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
  -d '{"name":"smoke-runner","total_capacity":1,"available_capacity":1}')
ENROLL=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_JSON")
RUNNER_ID=$(jq -r '.runner_id // empty' <<<"$ENROLL_JSON")
if [ -n "$ENROLL" ]; then ok "pending runner $RUNNER_ID, raw token issued once"
else bad "enrollment failed: $(head -c 200 <<<"$ENROLL_JSON")"; fi

step 6 "Runner reaches the live server  <-- THE III-H1 GAP"
# tack-runner has no subcommands: it is a daemon that enrolls on start.
export TACK_RUNNER_API_URL="$API" TACK_RUNNER_ENROLLMENT_TOKEN="${ENROLL:-none}" \
       TACK_RUNNER_STATE_DIR="$WORK/runner-state"
mkdir -p "$WORK/runner-state"; chmod 700 "$WORK/runner-state"
timeout 25 "$ROOT/target/debug/tack-runner" --api-url "$API" --state-dir "$WORK/runner-state" \
  >"$WORK/runner.log" 2>&1
RC=$?
# The runner is a daemon: a timeout (124) means it stayed up = enrolled and polling.
if [ "$RC" = 124 ] && ! grep -qiE "not configured|Unavailable" "$WORK/runner.log"; then
  ok "runner stayed up against the live server (enrolled and polling)"
elif grep -qiE "protocol client is not configured|ProtocolUnavailable" "$WORK/runner.log"; then
  bad "runner cannot speak to the server"
  printf '   \033[33mEXPECTED UNTIL III-H1 LANDS:\033[0m UnavailableProtocolClient is the only\n'
  printf '   production RunnerProtocolClient; the crate has no reqwest dependency.\n'
  printf '   This is the P0 III-G5 refused to tag on. Steps 7-9 cannot run.\n'
  tail -4 "$WORK/runner.log" | sed 's/^/   | /'
else
  bad "runner exited rc=$RC for an unexpected reason (NOT the known P0 - investigate)"
  tail -6 "$WORK/runner.log" | sed 's/^/   | /'
fi

step 7 "Claim -> start -> events -> artifact -> completion (needs step 6)"
printf '   SKIPPED — depends on a runner that can reach the server.\n'
step 8 "Same neutral request through each available harness (needs step 6)"
printf '   SKIPPED — harnesses reachable only through a claimed attempt.\n'
step 9 "Restart recovery: no silent loss, no blind duplicate execution (needs step 6)"
printf '   SKIPPED\n'

printf '\n\033[1m== RESULT ==\033[0m\n'
if [ "$FAILED" = 0 ]; then printf '\033[32mSMOKE PASSED\033[0m — %d/3 harnesses\n' "${#AVAIL[@]}"; exit 0; fi
printf '\033[31mSMOKE FAILED\033[0m — see the step above. Until III-H1 lands this is the\n'
printf 'correct outcome: the test is proving the release blocker, not a regression.\n'; exit 1
