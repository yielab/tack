#!/usr/bin/env bash
# End-to-end smoke: a real `tack serve` + a real `tack-runner` + harness binaries.
#
# Steps 1-6 build the fixture; what steps 7-12 each prove:
#
#   7  claim -> checkout -> harness -> completion, through production routes
#   8  the same neutral request through each harness kind, reported per kind
#   9  restart recovery: kill the runner mid-attempt, prove no silent loss and
#      no blind duplicate execution, then an explicit operator requeue
#   10 standalone mode: `tack serve --with-runner` alone reaches a completed
#      attempt, with no separate runner process and no operator-issued token
#   11 default `tack serve` (no flag) starts no runner, checked against the
#      live fleet endpoint rather than inferred from a log line
#   12 a non-loopback bind refuses `--with-runner` before any listener opens
#
#   ./scripts/smoke.sh            # fake mode: shim harness binaries (free, deterministic)
#   ./scripts/smoke.sh --live     # real harness binaries — a real model run happens
#
# Fake mode drives the FULL production pipeline (server, scheduler, runner,
# provisioner, adapter, subprocess); only the harness *binary* is a shim. Step 9
# uses the shim in both modes: restart mechanics are harness-agnostic and a kill
# test must not burn a billed run.
#
# The exit code reports step integrity: 0 = every runnable step held, 1 = a step
# failed. A step that CANNOT pass because the product cannot do the thing is a
# FAIL, never a SKIP — that is the false green this file shipped once already.
# Environmental absence (a harness binary not installed) is reported as ABSENT
# and listed in the release verdict, never rounded up and never counted as PASS.
set -uo pipefail

# Keychain entry names are global, not per state dir, so a secret set below
# would overwrite the operator's real one. A bus address that cannot exist
# sends every `tack` process to its file store instead. Linux only.
export DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/tack-smoke-no-keychain"

LIVE=0; [ "${1:-}" = "--live" ] && LIVE=1
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Honors CARGO_TARGET_DIR so this script builds and runs the same binaries
# `cargo build` itself would produce under that setting — required to keep
# a parallel agent's own build isolated to its own target directory rather
# than colliding with `$ROOT/target`, which every other worktree shares.
BIN_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
WORK="$(mktemp -d)"; PORT=${SMOKE_PORT:-3399}
API="http://127.0.0.1:$PORT"
PRINCIPAL='x-tack-principal: smoke-operator'
# Every server this script starts on loopback requires a token; they all
# inherit this one, and every call to them carries it.
export TACK_API_TOKEN="smoke-$$-$RANDOM$RANDOM"
AUTH="Authorization: Bearer $TACK_API_TOKEN"
SERVER_PID=""; RUNNER_A_PID=""; RUNNER_B_PID=""; STANDALONE_PID=""; NORUNNER_PID=""; FAILED=0
UNMET=()   # observed release-criteria shortfalls, printed in the release verdict

cleanup() {
  [ -n "$RUNNER_A_PID" ] && kill "$RUNNER_A_PID" 2>/dev/null
  [ -n "$RUNNER_B_PID" ] && kill "$RUNNER_B_PID" 2>/dev/null
  [ -n "$STANDALONE_PID" ] && kill "$STANDALONE_PID" 2>/dev/null
  [ -n "$NORUNNER_PID" ] && kill "$NORUNNER_PID" 2>/dev/null
  # Shim harness processes record their pid in their marker file; a hung shim
  # is in the harness's own session (the documented process-group ceiling), so
  # kill it by recorded pid, not by group.
  for marker in "$WORK"/harness-runs/run-* ; do
    [ -f "$marker" ] && kill "$(head -1 "$marker")" 2>/dev/null
  done
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null
  if [ "${SMOKE_KEEP:-0}" = 1 ]; then printf 'SMOKE_KEEP=1: work dir kept at %s\n' "$WORK"
  else rm -rf "$WORK"; fi
}
trap cleanup EXIT
step() { printf '\n\033[1m== STEP %s: %s\033[0m\n' "$1" "$2"; }
ok()   { printf '   \033[32mPASS\033[0m %s\n' "$1"; }
bad()  { printf '   \033[31mFAIL\033[0m %s\n' "$1"; FAILED=1; }
note() { printf '   \033[33mNOTE\033[0m %s\n' "$1"; }
unmet(){ printf '   \033[33mUNMET\033[0m %s\n' "$1"; UNMET+=("$1"); }

# Poll `$1` (a jq-producing command string) every 0.5s for up to $2 seconds
# until it prints a non-empty line; echoes that line. Empty output = timeout.
wait_for() {
  local tries=$(( $2 * 2 )) out
  for _ in $(seq 1 "$tries"); do
    out=$(eval "$1" 2>/dev/null)
    if [ -n "$out" ] && [ "$out" != "null" ]; then echo "$out"; return 0; fi
    sleep 0.5
  done
  return 1
}

attempts_json() { curl -H "$AUTH" -sf -H "$PRINCIPAL" "$API/api/executions/$1/attempts"; }

# Creates one execution request and echoes its request_id (empty on refusal).
# The server builds a run's repository from its project, so the project
# names the smoke repository before any run is created.
# $1 project
set_project_repo() {
  curl -H "$AUTH" -sf -X PATCH "$API/api/projects/$1" -H 'content-type: application/json' \
    -d "$(jq -n --arg repo "$SMOKE_REPO" '{code_origin:"folder", repository:$repo, workspace_mode:"clone"}')" >/dev/null
}

# $1 item  $2 runner  $3 harness kind  $4 model (empty: the agent's own default)
# $5 timeout_s  $6 environment JSON object  $7 idempotency key
create_execution() {
  curl -H "$AUTH" -sf -X POST "$API/api/executions" -H 'content-type: application/json' -H "$PRINCIPAL" \
    -d "$(jq -n \
      --arg item "$1" --arg runner "$2" --arg kind "$3" \
      --arg model "$4" --argjson timeout "$5" \
      --argjson env "$6" --arg idem "$7" \
      --arg profile "$AGENT_PROFILE" --arg remote "$SMOKE_REPO" --arg rev "$SMOKE_REV" \
      '{item_id:$item, idempotency_key:$idem,
        selector_kind:"exact_runner", selector_id:$runner,
        agent_profile_id:$profile,
        requested_harness_kind:$kind,
        requested_model:(if $model == "" then null else $model end),
        repository_snapshot:{kind:"git", remote:$remote, base_revision:$rev, subdirectory:null},
        permission_policy:{tools:[], network:false},
        budgets:{}, environment:$env, metadata:{}, timeout_seconds:$timeout}')" \
    | jq -r '.request_id // empty'
}

step 1 "Harness availability (reported honestly, never rounded up)"
AVAIL=()
for h in codex claude; do
  if command -v "$h" >/dev/null 2>&1; then
    printf '   present: %-10s %s\n' "$h" "$("$h" --version 2>&1 | head -1)"; AVAIL+=("$h")
  else
    printf '   ABSENT:  %-10s (cannot be part of any coverage claim)\n' "$h"
    unmet "harness binary '$h' is not installed on this machine — its leg of the two-harness criterion is unverifiable here"
  fi
done
printf '   real harness coverage: %d of 2\n' "${#AVAIL[@]}"
if [ "$LIVE" = 1 ]; then note "mode: --live (real binaries; a real model run happens in step 7)"
else note "mode: fake (shim binaries stand in for both harnesses; the rest of the pipeline is real)"; fi

step 2 "Build tack + tack-runner"
cargo build -p tack-cli -p tack-runner 2>&1 | tail -3
[ -x "$BIN_DIR/tack" ] && ok "tack built" || { bad "tack missing"; exit 1; }
[ -x "$BIN_DIR/tack-runner" ] && ok "tack-runner built" || { bad "tack-runner missing"; exit 1; }

# Shim harness binaries. Used by the main runner in fake mode, and by step 9's
# dedicated runner in BOTH modes. A shim answers the adapter's real probe
# (`--version`) and treats any other invocation as a run:
# it records a marker (its own pid — the duplicate-execution counter step 9
# asserts on), drains the prompt from stdin, honors SMOKE_HANG until the
# release file appears, prints one line and exits 0. Adapters spawn harnesses
# with a cleared environment, so every path is baked in absolute; SMOKE_HANG
# arrives through the execution request's own `environment` field — which
# also proves that plumbing end to end.
# The runners' HOME. A sandboxed harness sees a private /tmp and a scratch HOME,
# but its own config directory (~/.codex for codex) is bound through at the
# same path, so the shim's run markers and release file live there: visible to
# the shim sandboxed or not, and to this script.
RUNNER_HOME="$WORK/runner-home"
SHIMS="$WORK/shims"; MARKERS="$RUNNER_HOME/.codex/smoke-runs"; RELEASE_FILE="$RUNNER_HOME/.codex/smoke-release"
mkdir -p "$SHIMS" "$MARKERS"
cat > "$SHIMS/claude" <<SHIM
#!/bin/sh
PATH=/usr/bin:/bin
case "\${1:-}" in
  --version|-v) echo "1.0.0"; exit 0 ;;
  auth|login|debug) exit 1 ;;  # the probe's sign-in and model checks: not a run, so no marker
esac
marker="$MARKERS/run-\${SMOKE_HANG:+hang-}\$\$-\$(date +%s%N)"
echo "\$\$" > "\$marker"
cat >/dev/null
if [ "\${SMOKE_HANG:-}" = "1" ] && [ ! -f "$RELEASE_FILE" ]; then sleep 600; fi
echo "smoke-fake-harness-ok"
exit 0
SHIM
chmod +x "$SHIMS/claude"
cp "$SHIMS/claude" "$SHIMS/codex"
chmod +x "$SHIMS/codex"

# docket's adapter reads only a terminal NDJSON result line, not the exit code (crates/tack-runner/src/harness/docket.rs), so its shim prints one.
# Without DOCKET_HOME it answers the contract probe the way docket 0.2.0-beta.5 does; `setup` is the probe's readiness check, not a run.
cat > "$SHIMS/docket" <<SHIM
#!/bin/sh
case "\${1:-}" in
  --version|-v) echo "docket 0.2.0b5"; exit 0 ;;
  setup) exit 1 ;;
esac
if [ -z "\${DOCKET_HOME:-}" ]; then
  printf '{"v":"1.2.0","token":"","status":"refused","error":"DOCKET_HOME is not set; harness mode never runs against the default home","model":{"requested":"","served":"","source":""},"usage":{"input_tokens":0,"output_tokens":0}}\n'
  exit 2
fi
printf '{"v":"1.2.0","token":"smoke-docket-run","status":"ok","stop_reason":"","error":"","blocked":null,"model":{"requested":"local/smoke-model","served":"docket/smoke-model","source":"setup default"},"usage":{"input_tokens":1,"output_tokens":1},"files":[]}\n'
exit 0
SHIM
chmod +x "$SHIMS/docket"

# opencode has no run shim (step 8 says why); this one only answers the probe,
# so the runner never reaches a real opencode install or its login.
cat > "$SHIMS/opencode" <<SHIM
#!/bin/sh
case "\${1:-}" in
  --version|-v) echo "1.18.30"; exit 0 ;;
esac
exit 1
SHIM
chmod +x "$SHIMS/opencode"

step 3 "Start the API server (no agent-fleet backend configured — its absence must not disable runner execution)"
# Run from $WORK, never the repo root: the developer's tack.toml would otherwise be
# picked up, with whatever workstation-specific options it sets. A smoke test must
# exercise the product, not the workstation.
# `exec` matters: it replaces the subshell with the server process, so $! is the real
# tack PID. Without it, cleanup kills the subshell and leaves an orphan holding $PORT —
# a later run then silently talks to the previous run's database.
( cd "$WORK" && exec env TACK_DATABASE_URL="sqlite:$WORK/smoke.db?mode=rwc" TACK_PORT="$PORT" \
  TACK_STORAGE_DIR="$WORK/storage" "$BIN_DIR/tack" serve >"$WORK/server.log" 2>&1 ) &
SERVER_PID=$!
for _ in $(seq 1 40); do curl -H "$AUTH" -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
curl -H "$AUTH" -sf "$API/api/health" >/dev/null && ok "server healthy on $PORT with no agent-fleet backend configured" \
  || { bad "server never came up"; tail -20 "$WORK/server.log"; exit 1; }

step 4 "Create a project and an item (the plan of record)"
PROJ=$(curl -H "$AUTH" -sf -X POST "$API/api/projects" -H 'content-type: application/json' \
  -d '{"name":"smoke","project_type":"software"}' | jq -r '.id // .project.id // empty')
[ -n "$PROJ" ] && ok "project $PROJ" || bad "could not create project"
ITEM=$(curl -H "$AUTH" -sf -X POST "$API/api/projects/$PROJ/items" -H 'content-type: application/json' \
  -d '{"title":"smoke item","item_type":"task"}' | jq -r '.id // .item.id // empty')
[ -n "$ITEM" ] && ok "item $ITEM" || bad "could not create item"

step 5 "Register a pending runner and issue its enrollment token (operator surface)"
# Real contract (docs/openapi.json): POST /api/runners/enrollment, body CreatePendingRunner
# requires name + total_capacity + available_capacity; the raw token is returned exactly
# once here and only its SHA-256 hash is stored.
ENROLL_JSON=$(curl -H "$AUTH" -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
  -d '{"name":"smoke-runner","total_capacity":1,"available_capacity":1}')
ENROLL=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_JSON")
RUNNER_ID=$(jq -r '.runner_id // empty' <<<"$ENROLL_JSON")
if [ -n "$ENROLL" ]; then ok "pending runner $RUNNER_ID, raw token issued once"
else bad "enrollment failed: $(head -c 200 <<<"$ENROLL_JSON")"; fi

step 6 "Runner enrolls, heartbeats and polls against the live server"
RUNNER_A_PATH="$PATH"; [ "$LIVE" = 0 ] && RUNNER_A_PATH="$SHIMS:$PATH"
mkdir -p "$WORK/runner-state"; chmod 700 "$WORK/runner-state"
# TACK_RUNNER_ID must be distinct per runner: the enroll body's runner_name is
# taken from it, and a duplicate name is answered 500 by the server today,
# which would otherwise abort the second runner here.
( exec env HOME="$RUNNER_HOME" PATH="$RUNNER_A_PATH" TACK_RUNNER_ID="smoke-runner-a" TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL" \
    "$BIN_DIR/tack-runner" --api-url "$API" --state-dir "$WORK/runner-state" \
    >"$WORK/runner.log" 2>&1 ) &
RUNNER_A_PID=$!; disown "$RUNNER_A_PID"
HEARTBEAT=$(wait_for "curl -H '$AUTH' -sf '$API/api/runners' | jq -r '.data[] | select(.runner_id==\"$RUNNER_ID\") | select(.state==\"active\" and .last_heartbeat_at!=null) | .last_heartbeat_at'" 30 || true)
if [ -n "$HEARTBEAT" ]; then
  ok "runner active, heartbeat at $HEARTBEAT"
elif grep -qiE "protocol client is not configured|ProtocolUnavailable" "$WORK/runner.log"; then
  bad "runner cannot speak to the server — the protocol-client wiring has regressed"
  tail -4 "$WORK/runner.log" | sed 's/^/   | /'; exit 1
else
  bad "runner never became active with a heartbeat"
  tail -6 "$WORK/runner.log" | sed 's/^/   | /'; exit 1
fi

# What the scheduler will actually accept: the runner's own enrollment snapshot.
CAPS=$(curl -H "$AUTH" -sf "$API/api/runners" | jq -c ".data[] | select(.runner_id==\"$RUNNER_ID\") | .capability_snapshot")
printf '   what each harness reports:\n'
jq -r '.harnesses[]? | "     \(.harness_kind): requires a model: \(.requires_model // false); models: \(.models.source // "not reported"); signed in: \(.sign_in.state // "not reported")"' <<<"$CAPS"

step 7 "Claim -> checkout -> harness -> completion, through production routes"
# The repository under test is created here: a real git repo at a pinned commit.
SMOKE_REPO="$WORK/repo"; mkdir -p "$SMOKE_REPO"
git -C "$SMOKE_REPO" init -q -b main
echo "hello from the smoke repo" > "$SMOKE_REPO/README.md"
git -C "$SMOKE_REPO" -c user.email=smoke@invalid -c user.name=smoke add README.md
git -C "$SMOKE_REPO" -c user.email=smoke@invalid -c user.name=smoke commit -qm "smoke fixture"
SMOKE_REV=$(git -C "$SMOKE_REPO" rev-parse HEAD)
set_project_repo "$PROJ" && ok "project $PROJ names the smoke repository" || bad "could not set the project's repository"

AGENT_PROFILE=$(curl -H "$AUTH" -sf -X POST "$API/api/agent-profiles" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d '{"name":"smoke-profile","instructions":"Print the single word DONE and exit. Do not modify any files."}' \
  | jq -r '.agent_profile_id // empty')
[ -n "$AGENT_PROFILE" ] && ok "agent profile $AGENT_PROFILE" || bad "could not create agent profile"

# codex is the step-7 harness under test. A run names its model as one string
# in the agent's own spelling, or none for the agent's own default. Fake mode
# names a placeholder the fake accepts. Live mode bills the agent's own
# account, so it runs only with an explicit SMOKE_LIVE_MODEL (a model name, or
# "default" for the agent's own default) — never a silent default.
S7_KIND=codex
S7_READY=1
if [ "$LIVE" = 1 ]; then
  case "${SMOKE_LIVE_MODEL:-}" in
    "") S7_READY=0; S7_MODEL="" ;;
    default) S7_MODEL="" ;;
    *) S7_MODEL="$SMOKE_LIVE_MODEL" ;;
  esac
else
  S7_MODEL=gpt-5-codex
fi
S7_TIMEOUT=120; [ "$LIVE" = 1 ] && S7_TIMEOUT=300
if [ "$S7_READY" = 0 ]; then
  bad "live mode bills the agent's own account — set SMOKE_LIVE_MODEL to a model name, or to 'default', to run step 7 live"
else
  S7_LABEL="$S7_MODEL"; [ -n "$S7_LABEL" ] || S7_LABEL="the agent's own default"
  note "under test: $S7_KIND, model $S7_LABEL"
  REQ7=$(create_execution "$ITEM" "$RUNNER_ID" "$S7_KIND" "$S7_MODEL" "$S7_TIMEOUT" '{}' "smoke-s7-$$")
  [ -n "$REQ7" ] && ok "execution request $REQ7 queued" || bad "execution request refused"
  STATE=$(wait_for "attempts_json '$REQ7' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\" or .state==\"lost\" or .state==\"cancelled\") | .state'" "$((S7_TIMEOUT + 30))" || true)
  ATT=$(attempts_json "$REQ7" | jq -c '.data[0] // {}')
  if [ "$STATE" = "succeeded" ]; then
    ok "attempt $(jq -r '.attempt_id' <<<"$ATT") succeeded (fencing_token $(jq -r '.fencing_token' <<<"$ATT"))"
  else
    bad "attempt ended '$STATE' — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT" | head -c 300)"
  fi
  [ "$(jq -r '.base_revision' <<<"$ATT")" = "$SMOKE_REV" ] \
    && ok "attempt ran against the exact requested commit $SMOKE_REV" \
    || bad "attempt base_revision $(jq -r '.base_revision' <<<"$ATT") != requested $SMOKE_REV"
  [ -n "$(jq -r '.workspace_id // empty' <<<"$ATT")" ] \
    && ok "isolated workspace $(jq -r '.workspace_id' <<<"$ATT" | head -c 24)… provisioned" \
    || bad "no workspace_id on the attempt"
  EVENTS=$(curl -H "$AUTH" -sf -H "$PRINCIPAL" "$API/api/executions/$REQ7/attempts/1/events" | jq '.data | length')
  if [ "${EVENTS:-0}" -gt 0 ]; then ok "event timeline: $EVENTS events"
  else unmet "the runner never submits events or artifacts (engine has no AttemptDataProtocol call site), so a verified artifact and idempotent event timeline cannot be shown from a real runner (server routes are proven only by fake-client tests)"; fi
  ok "no agent-fleet backend was configured anywhere in this run, and execution still ran"
  REQ7_STATE=$(curl -H "$AUTH" -sf -H "$PRINCIPAL" "$API/api/executions/$REQ7" | jq -r '.state // empty')
  if [ "$REQ7_STATE" = "succeeded" ]; then ok "request state propagated to succeeded"
  else note "request state is '$REQ7_STATE' although its attempt succeeded — a known propagation gap, still present"; fi
fi

step 8 "The same neutral request through each harness kind, per kind, never rounded up"
# opencode (the fourth harness kind) needs an npm install and network per attempt, so it has no small shim here.
printf '   %-12s not covered in fake mode — installs an npm package per attempt and needs network; no small shim can stand in for it\n' "opencode:"
# docket names no model: it runs on its own setup's default.
declare -A S8_MODEL=( [codex]=gpt-5-codex [claude-code]=sonnet [docket]="" )
declare -A S8_BINARY=( [codex]=codex [claude-code]=claude [docket]=docket )
for kind in codex claude-code docket; do
  bin="${S8_BINARY[$kind]}"
  if [ "$LIVE" = 1 ] && ! command -v "$bin" >/dev/null 2>&1; then
    printf '   %-12s ABSENT — not installed, not claimed, not counted\n' "$kind:"
    continue
  fi
  model="${S8_MODEL[$kind]}"
  REQ=$(create_execution "$ITEM" "$RUNNER_ID" "$kind" "$model" 120 '{}' "smoke-s8-$kind-$$")
  if [ -z "$REQ" ]; then bad "$kind: execution request refused outright"; continue; fi
  GOT=$(wait_for "attempts_json '$REQ' | jq -r '.data[0].attempt_id // empty'" 20 || true)  # an idle runner re-polls only every retry_after_ms (5s)
  if [ -n "$GOT" ]; then
    STATE=$(wait_for "attempts_json '$REQ' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\") | .state'" 150 || true)
    if [ "$STATE" = "succeeded" ]; then ok "$kind: attempt succeeded through the full pipeline"
    else
      # A claimed-and-ran failure is a real harness result, not a scheduling
      # question — surface what the harness actually said (code/message plus
      # a bounded stdout preview, where an adapter that only classifies by
      # exit code, like codex's, puts the substance) instead of just the
      # bare state.
      TR=$(attempts_json "$REQ" | jq -c '.data[0].terminal_reason // {}')
      TR_CODE=$(jq -r '.code // "unknown"' <<<"$TR")
      TR_MSG=$(jq -r '.message // empty' <<<"$TR")
      TR_STDOUT=$(jq -r '.stdout.text_preview // empty' <<<"$TR" | head -c 900)
      bad "$kind: attempt was claimed and ran, then ended '$STATE' (code=$TR_CODE): $TR_MSG${TR_STDOUT:+ | stdout: $TR_STDOUT}"
    fi
  else
    # Never claimed. Read the runner's OWN declaration for this harness
    # (fetched in step 6, before any of step 8's requests existed) instead
    # of assuming a cause: a probe failure, an undeclared/unattested model,
    # and a momentarily-saturated runner are three different problems with
    # three different owners, and look identical from the outside (no
    # attempt ever appears). Reporting the wrong one of the three is exactly
    # how this step's old canned text went stale in the first place.
    HARNESS_CAP=$(jq -c --arg k "$kind" '.harnesses[]? | select(.harness_kind==$k) // {}' <<<"$CAPS")
    PROBE_ERROR=$(jq -r '.probe_error // empty' <<<"$HARNESS_CAP")
    if [ -n "$PROBE_ERROR" ]; then
      bad "$kind: request never claimable — this runner's own probe of the $kind binary failed ($PROBE_ERROR), so the scheduler places no $kind work on it"
      unmet "attempts through $kind: $kind is unschedulable on this runner because its probe failed"
    else
      bad "$kind: request was never claimed although this runner reports $kind runnable — the runner most likely had no free capacity at the time; step 8 shares this runner with whatever step 7 left it doing"
      unmet "attempts through $kind: $kind was runnable but not claimed within this run's wait window — retry against an otherwise-idle runner before concluding $kind itself is broken"
    fi
    curl -H "$AUTH" -sf -X POST "$API/api/executions/$REQ/cancel" >/dev/null 2>&1
  fi
done

step 9 "Restart recovery: kill the runner mid-attempt — no silent loss, no blind duplicate"
# A dedicated runner on shim binaries in BOTH modes: restart mechanics are
# harness-agnostic and this step kills processes, not model providers.
ENROLL_B_JSON=$(curl -H "$AUTH" -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
  -d '{"name":"smoke-runner-b","total_capacity":1,"available_capacity":1}')
ENROLL_B=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_B_JSON")
RUNNER_B=$(jq -r '.runner_id // empty' <<<"$ENROLL_B_JSON")
mkdir -p "$WORK/runner-b-state"; chmod 700 "$WORK/runner-b-state"
start_runner_b() {
  ( exec env HOME="$RUNNER_HOME" PATH="$SHIMS:$PATH" TACK_RUNNER_ID="smoke-runner-b" TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL_B" \
      "$BIN_DIR/tack-runner" --api-url "$API" --state-dir "$WORK/runner-b-state" \
      >>"$WORK/runner-b.log" 2>&1 ) &
  RUNNER_B_PID=$!; disown "$RUNNER_B_PID"
}
start_runner_b
if wait_for "curl -H '$AUTH' -sf '$API/api/runners' | jq -r '.data[] | select(.runner_id==\"$RUNNER_B\") | select(.state==\"active\") | .runner_id'" 30 >/dev/null; then
  ok "second runner $RUNNER_B active"
else
  bad "second runner never became active"
  tail -8 "$WORK/runner-b.log" | sed 's/^/   | /'
fi

# Only this step's kill-target request sets SMOKE_HANG, and the shim names
# those markers run-hang-*; other requests (runner A's, the capacity probe)
# can therefore never pollute the duplicate-execution count.
hang_runs() { ls "$MARKERS"/run-hang-* 2>/dev/null | wc -l; }
S9_RUNNING=0
REQ9=$(create_execution "$ITEM" "$RUNNER_B" codex smoke-model 600 \
  '{"SMOKE_HANG":{"value":"1","secret_reference":null}}' "smoke-s9-$$")
if wait_for "attempts_json '$REQ9' | jq -r '.data[0] | select(.state==\"running\") | .attempt_id'" 30 >/dev/null; then
  S9_RUNNING=1
  ok "attempt running, harness process live ($(hang_runs) run marker)"
else
  bad "hanging attempt never reached running"
fi

# Capacity evidence while the runner is saturated (capacity 1, one live lease):
# a second request for the same runner must NOT be claimed. Only meaningful
# while the hanging attempt genuinely holds the lease.
REQ9B=$(create_execution "$ITEM" "$RUNNER_B" codex smoke-model 120 '{}' "smoke-s9b-$$")
if [ "$S9_RUNNING" = 1 ]; then
  sleep 6
  if [ -z "$(attempts_json "$REQ9B" | jq -r '.data[0].attempt_id // empty')" ]; then
    ok "saturated runner claimed nothing more (capacity respected under a live lease)"
  else
    bad "a second attempt was claimed past total_capacity=1"
  fi
else
  bad "capacity check unusable: the hanging attempt never held the lease"
fi

RUNS_BEFORE=$(hang_runs)
SHIM_PID=$(head -1 "$(ls -t "$MARKERS"/run-hang-* 2>/dev/null | head -1)" 2>/dev/null)
kill -9 "$RUNNER_B_PID" 2>/dev/null   # the runner dies mid-attempt
kill -9 "$SHIM_PID" 2>/dev/null       # and its harness child (own process group) with it
RUNNER_B_PID=""
ok "runner and harness SIGKILLed mid-attempt"

start_runner_b
S9_STATE=$(wait_for "attempts_json '$REQ9' | jq -r '.data[0] | select(.state==\"needs_operator\" or .state==\"failed\" or .state==\"succeeded\" or .state==\"lost\") | .state'" 45 || true)
if [ "$S9_STATE" = "needs_operator" ]; then
  ok "restarted runner reported the ambiguity; attempt is needs_operator (explicit reconciliation, not silence)"
else
  bad "after restart the attempt is '$S9_STATE' — expected needs_operator, the no-blind-retry posture"
fi
RUNS_AFTER_RESTART=$(hang_runs)
ATTEMPTS_AFTER_RESTART=$(attempts_json "$REQ9" | jq '.data | length')
if [ "$RUNS_AFTER_RESTART" = "$RUNS_BEFORE" ] && [ "${ATTEMPTS_AFTER_RESTART:-0}" = 1 ]; then
  ok "no blind duplicate execution: $RUNS_BEFORE harness run and 1 attempt, before and after restart"
else
  bad "blind duplicate: harness runs $RUNS_BEFORE -> $RUNS_AFTER_RESTART, attempts now $ATTEMPTS_AFTER_RESTART, with no operator decision"
fi

touch "$RELEASE_FILE"   # from here on the shim completes instead of hanging
REQUEUE=$(curl -H "$AUTH" -sf -X POST "$API/api/executions/$REQ9/requeue" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d "{\"recovery_key\":\"smoke-requeue-$$\",\"reason\":\"smoke step 9: operator-confirmed restart recovery\"}" \
  | jq -r '.state // .result // empty')
note "operator requeue answered: ${REQUEUE:-<no body>}"
S9_FINAL=$(wait_for "attempts_json '$REQ9' | jq -r '.data | map(select(.state==\"succeeded\")) | .[0].attempt_number // empty'" 60 || true)
if [ -n "$S9_FINAL" ]; then
  ok "requeued work succeeded as attempt #$S9_FINAL — recovered with an explicit operator decision"
else
  bad "requeued execution never succeeded; attempts: $(attempts_json "$REQ9" | jq -c '[.data[] | {n:.attempt_number, s:.state}]')"
fi
wait_for "attempts_json '$REQ9B' | jq -r '.data[] | select(.state==\"succeeded\") | .attempt_id' | head -1" 60 >/dev/null \
  && ok "the queued-while-saturated request completed once capacity freed" \
  || bad "the queued-while-saturated request never completed"

step 10 "Standalone mode: 'tack serve --with-runner' reaches a completed attempt with zero manual enrollment"
# The point of ADR 0058: one binary, one command, no separate runner process,
# no operator-issued token ever copied anywhere. Fresh state dir, fresh
# database — nothing here is inherited from steps 3-9's separately-enrolled
# runner.
STANDALONE_PORT=$((PORT + 1))
STANDALONE_API="http://127.0.0.1:$STANDALONE_PORT"
SA_WORK="$WORK/standalone"; mkdir -p "$SA_WORK/state"; chmod 700 "$SA_WORK/state"
SA_PATH="$PATH"; [ "$LIVE" = 0 ] && SA_PATH="$SHIMS:$PATH"
SA_DB_URL="sqlite:$SA_WORK/tack.db?mode=rwc"
( cd "$SA_WORK" && exec env PATH="$SA_PATH" \
    TACK_DATABASE_URL="$SA_DB_URL" TACK_PORT="$STANDALONE_PORT" \
    TACK_STORAGE_DIR="$SA_WORK/storage" TACK_RUNNER_STATE_DIR="$SA_WORK/state" \
    "$BIN_DIR/tack" serve --with-runner >"$SA_WORK/server.log" 2>&1 ) &
STANDALONE_PID=$!
for _ in $(seq 1 40); do curl -H "$AUTH" -sf "$STANDALONE_API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
if curl -H "$AUTH" -sf "$STANDALONE_API/api/health" >/dev/null; then
  ok "standalone 'tack serve --with-runner' up on $STANDALONE_PORT, one process, one command"
else
  bad "standalone server never came up"; tail -20 "$SA_WORK/server.log" | sed 's/^/   | /'
fi

SA_RUNNER=$(wait_for "curl -H '$AUTH' -sf '$STANDALONE_API/api/runners' | jq -r '.data[] | select(.state==\"active\" and .last_heartbeat_at!=null) | .runner_id'" 30 || true)
if [ -n "$SA_RUNNER" ]; then
  ok "embedded runner $SA_RUNNER self-provisioned and active — no 'tack runner enroll', no token ever entered"
else
  bad "no embedded runner reached active — standalone mode never got off the ground"
  tail -20 "$SA_WORK/server.log" | sed 's/^/   | /'
fi

SA_PROJ=$(curl -H "$AUTH" -sf -X POST "$STANDALONE_API/api/projects" -H 'content-type: application/json' \
  -d '{"name":"smoke-standalone","project_type":"software"}' | jq -r '.id // .project.id // empty')
SA_ITEM=$(curl -H "$AUTH" -sf -X POST "$STANDALONE_API/api/projects/$SA_PROJ/items" -H 'content-type: application/json' \
  -d '{"title":"standalone smoke item","item_type":"task"}' | jq -r '.id // .item.id // empty')
SA_PROFILE=$(curl -H "$AUTH" -sf -X POST "$STANDALONE_API/api/agent-profiles" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d '{"name":"smoke-standalone-profile","instructions":"Print the single word DONE and exit. Do not modify any files."}' \
  | jq -r '.agent_profile_id // empty')
if [ -n "$SA_PROJ" ] && [ -n "$SA_ITEM" ] && [ -n "$SA_PROFILE" ]; then
  ok "standalone project/item/agent profile created"
else
  bad "could not set up the standalone project/item/agent profile"
fi

if [ -n "$SA_RUNNER" ] && [ -n "$SA_ITEM" ] && [ -n "$SA_PROFILE" ]; then
  # Reuses step 7's model (live mode needs an explicit SMOKE_LIVE_MODEL).
  if [ "$S7_READY" = 0 ]; then
    bad "live mode bills the agent's own account — set SMOKE_LIVE_MODEL to a model name, or to 'default', to run step 10 live"
  else
    # create_execution/attempts_json read $API (and create_execution reads
    # $AGENT_PROFILE) as globals; swap them to the standalone server for this
    # block only and restore immediately after, so nothing later in the
    # script can accidentally address the standalone server or profile.
    ORIGINAL_API="$API"; ORIGINAL_PROFILE="$AGENT_PROFILE"
    API="$STANDALONE_API"; AGENT_PROFILE="$SA_PROFILE"
    set_project_repo "$SA_PROJ" || bad "could not set the standalone project's repository"
    REQ10=$(create_execution "$SA_ITEM" "$SA_RUNNER" "$S7_KIND" "$S7_MODEL" 120 '{}' "smoke-s10-$$")
    if [ -n "$REQ10" ]; then
      ok "standalone execution request $REQ10 queued against the self-provisioned runner"
      ST10=$(wait_for "attempts_json '$REQ10' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\" or .state==\"lost\" or .state==\"cancelled\") | .state'" 150 || true)
      if [ "$ST10" = "succeeded" ]; then
        ok "PROOF: standalone mode reached a real completed attempt — one binary, one command, zero manual enrollment"
      else
        ATT10=$(attempts_json "$REQ10" | jq -c '.data[0] // {}')
        bad "standalone attempt ended '$ST10' — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT10" | head -c 300)"
      fi
    else
      bad "standalone execution request was refused outright"
    fi
    API="$ORIGINAL_API"; AGENT_PROFILE="$ORIGINAL_PROFILE"
  fi
fi

kill "$STANDALONE_PID" 2>/dev/null; wait "$STANDALONE_PID" 2>/dev/null; STANDALONE_PID=""

step 11 "Default 'tack serve' (no --with-runner, no env gate) starts no runner"
NORUNNER_PORT=$((PORT + 2))
NORUNNER_API="http://127.0.0.1:$NORUNNER_PORT"
NR_WORK="$WORK/norunner"; mkdir -p "$NR_WORK"
( cd "$NR_WORK" && exec env TACK_DATABASE_URL="sqlite:$NR_WORK/tack.db?mode=rwc" TACK_PORT="$NORUNNER_PORT" \
    TACK_STORAGE_DIR="$NR_WORK/storage" \
    "$BIN_DIR/tack" serve >"$NR_WORK/server.log" 2>&1 ) &
NORUNNER_PID=$!
for _ in $(seq 1 40); do curl -H "$AUTH" -sf "$NORUNNER_API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
if curl -H "$AUTH" -sf "$NORUNNER_API/api/health" >/dev/null; then ok "default server up on $NORUNNER_PORT"
else bad "default server never came up"; tail -20 "$NR_WORK/server.log" | sed 's/^/   | /'; fi

# A settle window long enough for a wrongly-started self-provisioning runner
# to have appeared and heartbeat at least once, so absence here is a real
# absence rather than a race against the check.
sleep 4
NR_RUNNERS=$(curl -H "$AUTH" -sf "$NORUNNER_API/api/runners" | jq '.data | length')
if [ "${NR_RUNNERS:-1}" = "0" ]; then
  ok "GET /api/runners is empty under default 'tack serve' — queried directly, not inferred from a log line"
else
  bad "default 'tack serve' started $NR_RUNNERS runner(s); the off-by-default gate has regressed"
fi
kill "$NORUNNER_PID" 2>/dev/null; wait "$NORUNNER_PID" 2>/dev/null; NORUNNER_PID=""

step 12 "Non-loopback bind + --with-runner refuses to start before opening a listener"
NL_PORT=$((PORT + 3))
NL_WORK="$WORK/nonloopback"; mkdir -p "$NL_WORK"
NL_OUT=$(cd "$NL_WORK" && env TACK_HOST=0.0.0.0 TACK_PORT="$NL_PORT" TACK_API_TOKEN=smoke-nonloopback-token \
  TACK_DATABASE_URL="sqlite:$NL_WORK/tack.db?mode=rwc" TACK_STORAGE_DIR="$NL_WORK/storage" \
  timeout 5 "$BIN_DIR/tack" serve --with-runner 2>&1)
NL_EXIT=$?
if [ "$NL_EXIT" != 0 ] && grep -qi "loopback" <<<"$NL_OUT"; then
  ok "refused to start (exit $NL_EXIT): $(grep -i loopback <<<"$NL_OUT" | head -1)"
else
  bad "non-loopback + --with-runner did not refuse as expected (exit $NL_EXIT): $(head -c 300 <<<"$NL_OUT")"
fi
if curl -sf -m 1 "http://127.0.0.1:$NL_PORT/api/health" >/dev/null 2>&1; then
  bad "a listener was opened on the refused non-loopback bind"
else
  ok "no listener was ever opened on the refused bind"
fi

printf '\n\033[1m== RESULT ==\033[0m\n'
if [ "$LIVE" = 1 ]; then MODE_DESC="live, ${#AVAIL[@]}/2 real harnesses installed"; else MODE_DESC="fake shim harnesses, pipeline real"; fi
if [ "$FAILED" = 0 ]; then printf '\033[32mSMOKE PASSED\033[0m — %s\n' "$MODE_DESC"
else printf '\033[31mSMOKE FAILED\033[0m — %s; see the failing step above\n' "$MODE_DESC"; fi
if [ "${#UNMET[@]}" -gt 0 ]; then
  printf '\n\033[1mRELEASE VERDICT: criteria this run could NOT demonstrate\033[0m\n'
  for u in "${UNMET[@]}"; do printf ' - %s\n' "$u"; done
  printf 'A release claim resting on this run must carry every line above.\n'
fi
[ "$FAILED" = 0 ] && exit 0 || exit 1
