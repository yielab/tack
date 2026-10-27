#!/usr/bin/env bash
# Records what a real docket answers to every request Tack's docket client sends, into
# crates/tack-runner/src/harness/fixtures/docket/v5/, one `<name>.json` per answer with a
# `<name>.json.provenance` beside it (and `<name>.request.json` when the request has a body).
# `tests/fake_docket/` answers from these files, and `tests/docket_v5.rs` fails when a route
# the client sends has no capture.
#
# docket runs in a throwaway HOME and DOCKET_HOME (a mktemp -d under /var/tmp) with no model,
# no provider key and an unreachable proxy, so nothing is spent. It declares one read-only
# sandbox grant on a folder it makes under its scratch folder (`docket setup sandbox read add`)
# and sets the lights-out team's `readOnly` to none, so one capture shows a team that keeps the
# machine's grant and one that narrowed it away. It starts with `--dispatch`
# the way Tack starts it. The tasks it queues are refused at claim by an unknown pipeline, before
# any agent turn, with two exceptions that still run no model: one enqueue a team policy of the
# throwaway home holds for approval (a pending escalation, then answered), and one task at L5 on
# the lights-out team whose pipeline has an approval step, which docket fails at once with
# `failureKind: needs_human`. It refuses to run when DOCKET_HOME is unset or is the operator's
# ~/.docket, and it never reads that folder. The bearer token docket writes is read from its
# file and is never printed or recorded; the script fails if it shows up in a capture.
#
# Usage: scripts/capture-docket-fixtures.sh [--out DIR]
# Environment:
#   DOCKET_BIN                 the docket to run (default: `docket` on PATH)
#   DOCKET_SRC                 a docket checkout, only to name its commit in the provenance
#                              (default ~/Sites/rack-cli when it exists)
#   DOCKET_CAPTURE_PORT        the loopback port (default: a free one; never 7399 or 7331)
#   DOCKET_CAPTURE_KEEP=1      keep the scratch folder
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT=$ROOT/crates/tack-runner/src/harness/fixtures/docket/v5
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
    *) printf 'usage: %s [--out DIR]\n' "$0" >&2; exit 2 ;;
  esac
done

die() { printf 'capture-docket-fixtures: %s\n' "$*" >&2; exit 1; }
say() { printf '==> %s\n' "$*"; }

for tool in jq curl git python3; do
  command -v "$tool" >/dev/null 2>&1 || die "$tool is needed"
done
DOCKET_BIN=${DOCKET_BIN:-$(command -v docket || true)}
[ -x "$DOCKET_BIN" ] || die "no docket on PATH; set DOCKET_BIN"

REAL_HOME=$HOME
SCRATCH=$(mktemp -d /var/tmp/docket-capture-XXXXXX)
FAKE_HOME=$SCRATCH/home
export DOCKET_HOME=$FAKE_HOME/.docket
mkdir -p "$DOCKET_HOME"

# The one guard: nothing below runs against anything but a folder made here.
guard() {
  [ -n "${DOCKET_HOME:-}" ] || die "DOCKET_HOME is unset"
  local target operator
  target=$(realpath -m "$DOCKET_HOME")
  operator=$(realpath -m "$REAL_HOME/.docket")
  [ "$target" != "$operator" ] || die "DOCKET_HOME is the operator's ~/.docket"
  case "$target" in
    /var/tmp/docket-capture-*/home/.docket) ;;
    *) die "DOCKET_HOME is not a folder this script made: $target" ;;
  esac
}
guard

DAEMON_PID=
cleanup() {
  [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null || true
  if [ "${DOCKET_CAPTURE_KEEP:-0}" != "1" ]; then
    case "$SCRATCH" in /var/tmp/docket-capture-*) rm -rf "$SCRATCH" ;; esac
  fi
}
trap cleanup EXIT

SYSTEM_PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
DOCKET_ENV=(env -i HOME="$FAKE_HOME" DOCKET_HOME="$DOCKET_HOME"
  PATH="$(dirname "$DOCKET_BIN"):$SYSTEM_PATH"
  HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9)

PORT=${DOCKET_CAPTURE_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')}
case "$PORT" in 7399|7331) die "port $PORT is the operator's docket" ;; esac
BASE=http://127.0.0.1:$PORT
TOKEN_FILE=$SCRATCH/token
CURL_AUTH=$SCRATCH/curl-auth

say "starting docket start --dispatch on :$PORT (scratch $SCRATCH)"
guard
"${DOCKET_ENV[@]}" "$DOCKET_BIN" start --dispatch --bind 127.0.0.1 --port "$PORT" --token-file "$TOKEN_FILE" \
  </dev/null >"$SCRATCH/daemon.log" 2>&1 &
DAEMON_PID=$!

STATUS=
for _ in $(seq 1 60); do
  STATUS=$(curl -fsS -m 3 "$BASE/status.json" 2>/dev/null || true)
  [ -n "$STATUS" ] && [ -s "$TOKEN_FILE" ] && break
  sleep 0.5
done
[ -s "$TOKEN_FILE" ] || die "docket wrote no token; see $SCRATCH/daemon.log (DOCKET_CAPTURE_KEEP=1 keeps it)"
API=$(jq -r '.apiVersion // empty' <<<"$STATUS" 2>/dev/null || true)
[ "$API" = "6" ] || die "this docket answers apiVersion \"${API:-nothing}\", not \"6\": Tack's docket harness needs docket 0.2.0-beta.6 or later"

TOKEN=$(tr -d '\n' <"$TOKEN_FILE")
umask 077
printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" >"$CURL_AUTH"
umask 022

VERSION=$("$DOCKET_BIN" --version 2>/dev/null | tail -1 | tr -d '\r')
DATE=$(date -u +%F)
SRC=${DOCKET_SRC:-$HOME/Sites/rack-cli}
SOURCE=
[ -d "$SRC/.git" ] && SOURCE=$(git -C "$SRC" log -1 --format=%h 2>/dev/null || true)

# Drop this script's previous captures so a removed route cannot linger: never a constructed
# file, and never one captured by hand from a real run (only this script writes `tack-sends:`).
mkdir -p "$OUT"
for provenance in "$OUT"/*.provenance; do
  [ -e "$provenance" ] || continue
  if [ "$(head -1 "$provenance")" = "captured" ] && grep -q '^tack-sends: ' "$provenance"; then
    base=${provenance%.provenance}
    rm -f "$provenance" "$base" "${base%.json}.request.json" "${base%.json}.request.txt"
  fi
done

# What would make a capture unstable is rewritten; the provenance says so.
redact() {
  sed -E \
    -e "s#$SCRATCH#/scratch#g" \
    -e 's#[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+(Z|\+00:00)#2026-01-01T00:00:00Z#g' \
    -e 's#("lastActivity": ?")[0-9-]+"#\12026-01-01"#g' \
    -e 's#\b[0-9a-f]{40}\b#0000000000000000000000000000000000000000#g' \
    -e 's#task-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}#task-00000000-0000-0000-0000-000000000000#g' \
    -e 's#esc-[0-9a-f]{12}#esc-000000000000#g' \
    -e 's#(apr|run)-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}#\1-00000000-0000-0000-0000-000000000000#g' \
    -e 's#("claimPid": ?)[0-9]+#\10#g'
}
REDACTED="the scratch folder (as /scratch), timestamps (as 2026-01-01T00:00:00Z), 40-hex commit ids (as zeros), task ids (as an all-zero uuid), escalation ids (as esc-000000000000), approval and sweep-run ids (as an all-zero uuid), claimPid"

LAST_STATUS=
LAST_BODY=
# rec <name> <METHOD> <path> <tack-sends yes|no> <note> [--body JSON | --raw TEXT] [--no-auth]
# OPTIONAL_KEYS="a b" in front of a call names request keys Tack does not always send; the
# fake requires every other key of a request Tack sends. HIDE_CURSOR=1 keeps a `since` cursor,
# whose event count varies between runs, out of the provenance's command line.
rec() {
  local name=$1 method=$2 path=$3 sends=$4 note=$5
  shift 5
  local body= raw= auth=1
  while [ $# -gt 0 ]; do
    case "$1" in
      --body) body=$2; shift 2 ;;
      --raw) raw=$2; shift 2 ;;
      --no-auth) auth=0; shift ;;
      *) die "rec: $1" ;;
    esac
  done
  guard
  local args=(-s -m 30 -X "$method" -o "$SCRATCH/answer" -w '%{http_code}')
  [ "$auth" = 1 ] && args+=(-K "$CURL_AUTH")
  local request=
  if [ -n "$body" ]; then
    printf '%s' "$body" >"$SCRATCH/request"
    args+=(-H 'Content-Type: application/json' --data-binary @"$SCRATCH/request")
    request="$name.request.json"
  elif [ -n "$raw" ]; then
    printf '%s' "$raw" >"$SCRATCH/request"
    args+=(-H 'Content-Type: application/json' --data-binary @"$SCRATCH/request")
    request="$name.request.txt"
  fi
  LAST_STATUS=$(curl "${args[@]}" "$BASE$path")
  LAST_BODY=$(cat "$SCRATCH/answer")
  if jq . <"$SCRATCH/answer" >/dev/null 2>&1; then
    jq . <"$SCRATCH/answer" | redact >"$OUT/$name.json"
  else
    redact <"$SCRATCH/answer" >"$OUT/$name.json"
  fi
  if [ -n "$body" ]; then
    jq . <"$SCRATCH/request" | redact >"$OUT/$request"
  elif [ -n "$raw" ]; then
    redact <"$SCRATCH/request" >"$OUT/$request"
  fi
  # The route as the client names it: the ids in the path become placeholders.
  local route=${path%%\?*}
  case "$route" in
    /pods/*) route=/pods/'<pod>' ;;
    /tasks/*/*/cancel) route=/tasks/'<pod>'/'<id>'/cancel ;;
    /tasks/*/*/diff) route=/tasks/'<pod>'/'<id>'/diff ;;
    /tasks/*/*) route=/tasks/'<pod>'/'<id>' ;;
    /tasks/*) route=/tasks/'<pod>' ;;
    /traces/*) route=/traces/'<pod>' ;;
    /escalations/*) route=/escalations/'<id>' ;;
  esac
  {
    echo captured
    echo "route: $method $route"
    echo "status: $LAST_STATUS"
    [ -n "$request" ] && echo "request: $request"
    echo "tack-sends: $sends"
    [ -n "${OPTIONAL_KEYS:-}" ] && echo "optional-keys: $OPTIONAL_KEYS"
    echo "note: $note"
    echo "command: curl -X $method http://127.0.0.1:<port>$path$([ "$auth" = 1 ] && echo ' (bearer token from docket'"'"'s --token-file)')${request:+ with the body in $request}" | redact |
      { if [ -n "${HIDE_CURSOR:-}" ]; then sed -E 's#since=[^ ]*#since=<the cursor the previous read ended on>#'; else cat; fi; }
    echo "docket: $VERSION (API $API)${SOURCE:+, source checkout $SOURCE}"
    echo "env: HOME and DOCKET_HOME (a throwaway folder), PATH, HTTP_PROXY/HTTPS_PROXY/ALL_PROXY (an unreachable loopback port); no model, no provider key"
    echo "date: $DATE"
    echo "redacted: $REDACTED"
  } >"$OUT/$name.json.provenance"
  say "$name: $method $path -> $LAST_STATUS"
}

expect() { [ "$LAST_STATUS" = "$1" ] || die "$2 answered $LAST_STATUS, expected $1: $LAST_BODY"; }

# wait_for <pod> <task> <status>: the task reaches that status, or the capture stops.
wait_for() {
  local state=
  for _ in $(seq 1 60); do
    state=$(curl -s -m 5 -K "$CURL_AUTH" "$BASE/tasks/$1/$2" | jq -r '.status // empty')
    [ "$state" = "$3" ] && return 0
    sleep 0.5
  done
  die "task $2 is ${state:-unknown}, not $3; see $SCRATCH/daemon.log"
}

# ── Before any team ────────────────────────────────────────────────────────────
rec health GET /health yes "the daemon supervision asks only for the status" --no-auth; expect 200 health
rec status-empty GET /status.json yes "no team yet, so no agents" --no-auth; expect 200 status
rec unauthorized GET /pods no "a request without the bearer token" --no-auth; expect 401 unauthorized
rec pods-empty GET /pods yes "no team yet"; expect 200 pods
rec traces-empty GET /traces/saas-dev no "a team that does not exist yet has no events, and no error"; expect 200 traces
rec escalations-empty GET '/escalations?since=0' yes "the probe: nothing is waiting, and next equals since"; expect 200 escalations-empty

# ── Teams, over throwaway repositories ────────────────────────────────────────
for repo in repo repo2; do
  mkdir -p "$SCRATCH/$repo"
  git -C "$SCRATCH/$repo" init -q -b main
  printf '# %s\n' "$repo" >"$SCRATCH/$repo/README.md"
  git -C "$SCRATCH/$repo" add -A
  git -C "$SCRATCH/$repo" -c user.name=capture -c user.email=capture@localhost commit -q -m seed
done
DEV_BODY=$(jq -cn --arg location "$SCRATCH/repo" '{name: "saas-dev", location: $location, blueprint: "software", ensure: true}')
rec post-pods-old-fields POST /pods no "the fields before docket read name and location: refused" \
  --body "$(jq -cn --arg path "$SCRATCH/repo" '{project: "saas-dev", path: $path, blueprint: "software", ensure: true}')"; expect 400 old-fields
rec post-pods-created POST /pods yes "the body Tack sends; the answer still names the team project" --body "$DEV_BODY"; expect 201 post-pods
rec post-pods-existing POST /pods yes "the same body again: ensure answers 200, created false" --body "$DEV_BODY"; expect 200 post-pods-again
rec post-pods-conflict POST /pods no "the same body without ensure (Tack always sends ensure)" \
  --body "$(jq -c 'del(.ensure)' <<<"$DEV_BODY")"; expect 409 post-pods-conflict
LIGHTS_BODY=$(jq -cn --arg location "$SCRATCH/repo2" '{name: "saas-lights", location: $location, blueprint: "software", ensure: true}')
rec post-pods-created-second POST /pods no "a second team, set to lights-out below so its profile carries violations" --body "$LIGHTS_BODY"; expect 201 second-pod
guard
(cd "$SCRATCH/repo2" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod set profile lights-out --pod saas-lights </dev/null >"$SCRATCH/pod-set.log" 2>&1) \
  || die "docket pod set profile failed; see $SCRATCH/pod-set.log"

mkdir -p "$SCRATCH/toolchain/node"
guard
"${DOCKET_ENV[@]}" "$DOCKET_BIN" setup sandbox read add "$SCRATCH/toolchain/node" </dev/null >"$SCRATCH/read-add.log" 2>&1 \
  || die "docket setup sandbox read add failed; see $SCRATCH/read-add.log"
(cd "$SCRATCH/repo2" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod set readOnly none --pod saas-lights </dev/null >"$SCRATCH/pod-set-read.log" 2>&1) \
  || die "docket pod set readOnly failed; see $SCRATCH/pod-set-read.log"

rec pods GET /pods yes "two teams, one attended and one lights-out"; expect 200 pods
rec pod-attended GET /pods/saas-dev yes "a team with no violation, keeping the machine's one read-only grant in readOnly"; expect 200 pod-attended
rec pod-lights-out GET /pods/saas-lights yes "a lights-out team that lacks isolation, a network setting and an external gate; its readOnly setting is none, so readOnly is empty"; expect 200 pod-lights-out
rec pod-unknown GET /pods/no-such-team yes "a team docket does not know"; expect 404 pod-unknown
rec status GET /status.json yes "with teams, the agents are their members" --no-auth; expect 200 status-teams
rec traces-since-empty GET '/traces/saas-dev?since=2026-01-01T00:00:00Z:0' yes "the compound cursor echoed when there is nothing newer"; expect 200 traces-since

# ── What docket refuses at POST /tasks/<pod> ──────────────────────────────────
refuse() { # <name> <status> <note> <path> <body>
  rec "$1" POST "$4" no "$3" --body "$5"; expect "$2" "$1"
}
refuse post-tasks-no-description 400 "a body without description" /tasks/saas-dev '{"externalRef":"x"}'
rec post-tasks-invalid-json POST /tasks/saas-dev no "a body that is not JSON" --raw 'not json'; expect 400 invalid-json
refuse post-tasks-max-tokens-zero 400 "maxTokens must be above 0" /tasks/saas-dev '{"description":"x","maxTokens":0}'
refuse post-tasks-empty-external-ref 400 "externalRef must have a character" /tasks/saas-dev '{"description":"x","externalRef":""}'
refuse post-tasks-gate-without-command 400 "gate needs a command" /tasks/saas-dev '{"description":"x","gate":{}}'
refuse post-tasks-brief-without-objective 422 "a brief with an empty objective" /tasks/saas-dev '{"description":"x","brief":{"objective":""}}'
refuse post-tasks-denied-tool-unknown 422 "deniedTools names a tool docket does not have" /tasks/saas-dev '{"description":"x","restrictions":{"deniedTools":["no-such-tool"]}}'
refuse post-tasks-network-open 422 "restrictions.network may only be none" /tasks/saas-dev '{"description":"x","restrictions":{"network":"open"}}'
refuse post-tasks-autonomy-above-ceiling 422 "autonomy L5 on an attended team, whose ceiling is L3" /tasks/saas-dev '{"description":"x","autonomy":"L5"}'
refuse post-tasks-autonomy-invalid 400 "autonomy that is not L3, L4 or L5" /tasks/saas-dev '{"description":"x","autonomy":"L9"}'
refuse post-tasks-unknown-pod 404 "a team docket does not know" /tasks/no-such-team '{"description":"x"}'

# ── The one task: refused at claim by an unknown pipeline, before any agent turn ─
TASK_BODY=$(jq -cn '{description: "Capture only: this task fails at claim, no agent runs.",
  externalRef: "capture-item/attempt-1", idempotencyKey: "capture-attempt-1", baseRef: "main",
  pipeline: "no-such-pipeline", maxTokens: 1000, priority: "normal", trusted: false, autonomy: "L3",
  restrictions: {network: "none", deniedTools: ["bash"]},
  gate: {command: "true", timeoutS: 600},
  brief: {objective: "Capture only", acceptance: ["nothing"], constraints: ["nothing"]}}')
rec post-task-created POST /tasks/saas-dev yes "a task as Tack posts it, with every key it can send; the pipeline does not exist" --body "$TASK_BODY"; expect 201 post-task
TASK_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-dev "$TASK_ID" failed
rec post-task-existing POST /tasks/saas-dev yes "the same idempotencyKey again: 200, created false" --body "$TASK_BODY"; expect 200 post-task-again
MINIMAL_BODY=$(jq -cn '{description: "Capture only: a task with the four keys every task carries.",
  externalRef: "capture-item/attempt-2", idempotencyKey: "capture-attempt-2", baseRef: "main",
  pipeline: "no-such-pipeline"}')
OPTIONAL_KEYS=pipeline rec post-task-minimal POST /tasks/saas-dev yes "the four keys every task carries (plus the unknown pipeline that keeps it from running)" --body "$MINIMAL_BODY"; expect 201 post-task-minimal
MINIMAL_ID=$(jq -r '.task' <<<"$LAST_BODY")

wait_for saas-dev "$MINIMAL_ID" failed
rec task-failed-pipeline GET "/tasks/saas-dev/$TASK_ID" yes "a task refused at claim: the sentence is in reason and the kind in failureKind; there is no error key"; expect 200 task
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = pipeline ] || die "the task failed, but not on its pipeline: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"
rec task-diff-failed GET "/tasks/saas-dev/$TASK_ID/diff" yes "the diff of a task that never started"; expect 200 task-diff
rec task-cancel-finished POST "/tasks/saas-dev/$TASK_ID/cancel" yes "cancel of a task that has ended" --body '{"reason":"the capture"}'; expect 409 cancel
sleep 2
rec traces-after-failure GET /traces/saas-dev yes "the events of the tasks refused at claim, a second after the last (docket holds back the newest second)"; expect 200 traces

# ── Routes for tasks docket does not know ─────────────────────────────────────
rec task-unknown GET /tasks/saas-dev/task-unknown yes "a task docket does not know"; expect 404 task-unknown
rec task-cancel-unknown POST /tasks/saas-dev/task-unknown/cancel yes "cancel of a task docket does not know" --body '{"reason":"the capture"}'; expect 404 cancel-unknown
rec task-diff-unknown GET /tasks/saas-dev/task-unknown/diff yes "the diff of a task docket does not know"; expect 404 diff-unknown

# trace_cursor: docket holds back the events of the last second, so wait it out, then print the
# cursor a read of the team's events would end on; a later read from it holds only what came after.
trace_cursor() {
  sleep 2
  curl -s -m 5 -K "$CURL_AUTH" "$BASE/traces/$1" | jq -r '.next'
}

# ── An enqueue a team policy holds for a person: a pending escalation, no model ─
# A `pre_input` policy that asks holds the enqueue itself; docket opens the escalation and
# answers 201 with its id. The policy is a file of the throwaway home, on the saas-dev team only.
guard
mkdir -p "$DOCKET_HOME/workspaces/pods/saas-dev/config/policies"
cat >"$DOCKET_HOME/workspaces/pods/saas-dev/config/policies/capture-needs-approval.yaml" <<'POLICY'
kind: policy
name: capture-needs-approval
description: Capture only; holds an enqueue that names it until a person answers
appliesTo: ["*"]
on: input
when:
  matches: 'capture-needs-approval'
then: ask
message: Capture only; a person answers this enqueue.
POLICY
TRACE_FROM=$(trace_cursor saas-dev)
GATED_BODY=$(jq -cn '{description: "Capture only: capture-needs-approval holds this enqueue for a person.",
  externalRef: "capture-item/attempt-3", idempotencyKey: "capture-attempt-3", baseRef: "main",
  pipeline: "no-such-pipeline"}')
OPTIONAL_KEYS=pipeline rec post-task-gated POST /tasks/saas-dev yes "a task a policy holds for approval: 201, status waiting_approval and the id of its escalation" --body "$GATED_BODY"; expect 201 post-task-gated
[ "$(jq -r '.status' <<<"$LAST_BODY")" = waiting_approval ] || die "the policy did not hold the enqueue: $LAST_BODY"
GATED_ID=$(jq -r '.task' <<<"$LAST_BODY")
ESC_ID=$(jq -r '.escalation' <<<"$LAST_BODY")
case "$ESC_ID" in esc-*) ;; *) die "no escalation id in the answer: $LAST_BODY" ;; esac

rec escalations-pending GET '/escalations?since=0' yes "one pending escalation of kind approval, with its options, affordances and expiry"; expect 200 escalations-pending
[ "$(jq -r '.escalations[0].id' <<<"$LAST_BODY")" = "$ESC_ID" ] || die "the list does not hold $ESC_ID: $LAST_BODY"
NEXT=$(jq -r '.next' <<<"$LAST_BODY")
rec escalations-task-since GET "/escalations?since=$NEXT&task=$GATED_ID" yes "the cursor and the task filter together: nothing newer, next equal to since"; expect 200 escalations-since
[ "$(jq '.escalations | length' <<<"$LAST_BODY")" = 0 ] || die "the cursor returned a record: $LAST_BODY"
rec task-waiting-escalation GET "/tasks/saas-dev/$GATED_ID" yes "a task waiting on an escalation: escalations lists the ids opened for it, and the keys of the retired shapes are gone"; expect 200 task-waiting

OPTIONAL_KEYS=content rec escalation-unknown POST /escalations/esc-000000000000 yes "an answer to an escalation docket does not know" \
  --body '{"optionId":"approve_once","actor":"tack"}'; expect 404 escalation-unknown
rec escalation-unknown-option POST "/escalations/$ESC_ID" no "an option the escalation does not offer" --body '{"optionId":"no-such-option","actor":"tack"}'; expect 422 escalation-unknown-option
rec escalation-bad-option-type POST "/escalations/$ESC_ID" no "an optionId that is not a string" --body '{"optionId":3}'; expect 400 escalation-bad-option-type
OPTIONAL_KEYS=content rec escalation-accepted POST "/escalations/$ESC_ID" yes "approve_once: the resolved view, state resolved, no options left" \
  --body '{"optionId":"approve_once","actor":"tack"}'; expect 200 escalation-accepted
OPTIONAL_KEYS=content rec escalation-settled POST "/escalations/$ESC_ID" yes "the same answer again: the escalation is already settled" \
  --body '{"optionId":"approve_once","actor":"tack"}'; expect 409 escalation-settled
wait_for saas-dev "$GATED_ID" failed
rec task-failed-after-escalation GET "/tasks/saas-dev/$GATED_ID" yes "the approved task resumed, and the unknown pipeline refused it at claim: escalations still lists the settled record"; expect 200 task-after-escalation
sleep 2
HIDE_CURSOR=1 rec traces-escalation GET "/traces/saas-dev?since=$TRACE_FROM" yes "the events after the held enqueue: escalation_asked, escalation_resolved, and the grant"; expect 200 traces-escalation

# ── Cancel a task that waits on a person ───────────────────────────────────────
PARKED_BODY=$(jq -cn '{description: "Capture only: capture-needs-approval holds this one too, and it is cancelled.",
  externalRef: "capture-item/attempt-5", idempotencyKey: "capture-attempt-5", baseRef: "main",
  pipeline: "no-such-pipeline"}')
PARKED=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' -d "$PARKED_BODY" "$BASE/tasks/saas-dev")
[ "$(jq -r '.status' <<<"$PARKED")" = waiting_approval ] || die "the policy did not hold the second enqueue: $PARKED"
PARKED_ID=$(jq -r '.task' <<<"$PARKED")
rec cancel-accepted POST "/tasks/saas-dev/$PARKED_ID/cancel" yes "cancel of a task that waits on a person: status cancelled, and its escalation is closed" --body '{"reason":"the capture"}'; expect 200 cancel

# ── A task at L5 whose pipeline asks a person: needs_human, no model ───────────
# The lights-out team's ceiling is L5, and docket runs a lights-out team only once its profile is
# met: the sandbox on, its network cut, and an external gate on the implementer step. Those are
# settings of the throwaway home, made here after the captures that show the unmet profile. An
# approval step at L5 asks nobody: docket records a decided escalation and fails the task before
# any hop, with failureKind needs_human.
guard
"${DOCKET_ENV[@]}" "$DOCKET_BIN" setup sandbox on </dev/null >"$SCRATCH/sandbox-on.log" 2>&1 \
  || die "docket setup sandbox on failed; see $SCRATCH/sandbox-on.log"
"${DOCKET_ENV[@]}" "$DOCKET_BIN" setup sandbox network none </dev/null >"$SCRATCH/sandbox-network.log" 2>&1 \
  || die "docket setup sandbox network none failed; see $SCRATCH/sandbox-network.log"
mkdir -p "$DOCKET_HOME/workspaces/pods/saas-lights/config/pipelines"
cat >"$DOCKET_HOME/workspaces/pods/saas-lights/config/pipelines/capture-approval.yaml" <<'PIPELINE'
name: capture-approval
steps:
  - ask: lead
    approval: "Capture only: a step that asks a person"
  - implement: implementer
    gate: {external: "true"}
PIPELINE
L5_BODY=$(jq -cn '{description: "Capture only: an approval step at L5 asks nobody, so this task fails needs_human.",
  externalRef: "capture-item/attempt-4", idempotencyKey: "capture-attempt-4", baseRef: "main",
  pipeline: "capture-approval", autonomy: "L5"}')
OPTIONAL_KEYS="pipeline autonomy" rec post-task-autonomy-l5 POST /tasks/saas-lights yes "a task at L5 on the lights-out team: accepted, and the level is shown on the task" --body "$L5_BODY"; expect 201 post-task-l5
L5_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-lights "$L5_ID" failed
rec task-failed-needs-human GET "/tasks/saas-lights/$L5_ID" yes "a task at L5 whose pipeline asks a person: failed, failureKind needs_human, no hop ran"; expect 200 task-needs-human
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = needs_human ] || die "the task failed, but not needs_human: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"
rec escalations-settled GET '/escalations?since=0' yes "every record whatever its state: one resolved by a person, one closed by a cancel, one decided by the level"; expect 200 escalations-settled
sleep 2
rec traces-escalation-decided GET /traces/saas-lights yes "the events of the L5 task: escalation_decided"; expect 200 traces-decided

# No capture may hold the token.
if grep -rqF -- "$TOKEN" "$OUT"; then die "the bearer token is in a capture; nothing is kept"; fi
say "done: $(ls "$OUT"/*.provenance | wc -l) provenance files in $OUT"
