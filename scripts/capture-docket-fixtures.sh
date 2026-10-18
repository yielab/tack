#!/usr/bin/env bash
# Records what a real docket answers to every request Tack's docket client sends, into
# crates/tack-runner/src/harness/fixtures/docket/v5/, one `<name>.json` per answer with a
# `<name>.json.provenance` beside it (and `<name>.request.json` when the request has a body).
# `tests/fake_docket/` answers from these files, and `tests/docket_v5.rs` fails when a route
# the client sends has no capture.
#
# docket runs in a throwaway HOME and DOCKET_HOME (a mktemp -d under /var/tmp) with no model,
# no provider key and an unreachable proxy, so nothing is spent. It starts with `--dispatch`
# the way Tack starts it. The only task it queues is refused at claim by an unknown pipeline,
# before any agent turn. It refuses to run when DOCKET_HOME is unset or is the operator's
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
[ "$API" = "5" ] || die "this docket answers apiVersion \"${API:-nothing}\", not \"5\": Tack's docket harness needs docket 0.2.0-beta.6 or later"

TOKEN=$(tr -d '\n' <"$TOKEN_FILE")
umask 077
printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" >"$CURL_AUTH"
umask 022

VERSION=$("$DOCKET_BIN" --version 2>/dev/null | tail -1 | tr -d '\r')
DATE=$(date -u +%F)
SRC=${DOCKET_SRC:-$HOME/Sites/rack-cli}
SOURCE=
[ -d "$SRC/.git" ] && SOURCE=$(git -C "$SRC" log -1 --format=%h 2>/dev/null || true)

# Drop the previous captures (never the constructed files) so a removed route cannot linger.
mkdir -p "$OUT"
for provenance in "$OUT"/*.provenance; do
  [ -e "$provenance" ] || continue
  if [ "$(head -1 "$provenance")" = "captured" ]; then
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
    -e 's#("claimPid": ?)[0-9]+#\10#g'
}
REDACTED="the scratch folder (as /scratch), timestamps (as 2026-01-01T00:00:00Z), 40-hex commit ids (as zeros), task ids (as an all-zero uuid), claimPid"

LAST_STATUS=
LAST_BODY=
# rec <name> <METHOD> <path> <tack-sends yes|no> <note> [--body JSON | --raw TEXT] [--no-auth]
# OPTIONAL_KEYS="a b" in front of a call names request keys Tack does not always send; the
# fake requires every other key of a request Tack sends.
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
    /tasks/*/answer) route=/tasks/'<id>'/answer ;;
    /tasks/*/*) route=/tasks/'<pod>'/'<id>' ;;
    /tasks/*) route=/tasks/'<pod>' ;;
    /traces/*) route=/traces/'<pod>' ;;
    /approvals/*) route=/approvals/'<token>' ;;
  esac
  {
    echo captured
    echo "route: $method $route"
    echo "status: $LAST_STATUS"
    [ -n "$request" ] && echo "request: $request"
    echo "tack-sends: $sends"
    [ -n "${OPTIONAL_KEYS:-}" ] && echo "optional-keys: $OPTIONAL_KEYS"
    echo "note: $note"
    echo "command: curl -X $method $BASE$path$([ "$auth" = 1 ] && echo ' (bearer token from docket'"'"'s --token-file)')${request:+ with the body in $request}" | redact
    echo "docket: $VERSION (API $API)${SOURCE:+, source checkout $SOURCE}"
    echo "env: HOME and DOCKET_HOME (a throwaway folder), PATH, HTTP_PROXY/HTTPS_PROXY/ALL_PROXY (an unreachable loopback port); no model, no provider key"
    echo "date: $DATE"
    echo "redacted: $REDACTED"
  } >"$OUT/$name.json.provenance"
  say "$name: $method $path -> $LAST_STATUS"
}

expect() { [ "$LAST_STATUS" = "$1" ] || die "$2 answered $LAST_STATUS, expected $1: $LAST_BODY"; }

# ── Before any team ────────────────────────────────────────────────────────────
rec health GET /health yes "the daemon supervision asks only for the status" --no-auth; expect 200 health
rec status-empty GET /status.json yes "no team yet, so no agents" --no-auth; expect 200 status
rec unauthorized GET /pods no "a request without the bearer token" --no-auth; expect 401 unauthorized
rec pods-empty GET /pods yes "no team yet"; expect 200 pods
rec inbox-empty GET /inbox yes "nothing needs a person"; expect 200 inbox
rec traces-empty GET /traces/fab-dev no "a team that does not exist yet has no events, and no error"; expect 200 traces

# ── Teams, over throwaway repositories ────────────────────────────────────────
for repo in repo repo2; do
  mkdir -p "$SCRATCH/$repo"
  git -C "$SCRATCH/$repo" init -q -b main
  printf '# %s\n' "$repo" >"$SCRATCH/$repo/README.md"
  git -C "$SCRATCH/$repo" add -A
  git -C "$SCRATCH/$repo" -c user.name=capture -c user.email=capture@localhost commit -q -m seed
done
DEV_BODY=$(jq -cn --arg location "$SCRATCH/repo" '{name: "fab-dev", location: $location, blueprint: "software", ensure: true}')
rec post-pods-old-fields POST /pods no "the fields before docket read name and location: refused" \
  --body "$(jq -cn --arg path "$SCRATCH/repo" '{project: "fab-dev", path: $path, blueprint: "software", ensure: true}')"; expect 400 old-fields
rec post-pods-created POST /pods yes "the body Tack sends; the answer still names the team project" --body "$DEV_BODY"; expect 201 post-pods
rec post-pods-existing POST /pods yes "the same body again: ensure answers 200, created false" --body "$DEV_BODY"; expect 200 post-pods-again
rec post-pods-conflict POST /pods no "the same body without ensure (Tack always sends ensure)" \
  --body "$(jq -c 'del(.ensure)' <<<"$DEV_BODY")"; expect 409 post-pods-conflict
LIGHTS_BODY=$(jq -cn --arg location "$SCRATCH/repo2" '{name: "fab-lights", location: $location, blueprint: "software", ensure: true}')
rec post-pods-created-second POST /pods no "a second team, set to lights-out below so its profile carries violations" --body "$LIGHTS_BODY"; expect 201 second-pod
guard
(cd "$SCRATCH/repo2" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod set profile lights-out --pod fab-lights </dev/null >"$SCRATCH/pod-set.log" 2>&1) \
  || die "docket pod set profile failed; see $SCRATCH/pod-set.log"

rec pods GET /pods yes "two teams, one attended and one lights-out"; expect 200 pods
rec pod-attended GET /pods/fab-dev yes "a team with no violation"; expect 200 pod-attended
rec pod-lights-out GET /pods/fab-lights yes "a lights-out team that lacks isolation, a network setting and an external gate"; expect 200 pod-lights-out
rec pod-unknown GET /pods/no-such-team yes "a team docket does not know"; expect 404 pod-unknown
rec status GET /status.json yes "with teams, the agents are their members" --no-auth; expect 200 status-teams
rec traces-since-empty GET '/traces/fab-dev?since=2026-01-01T00:00:00Z:0' yes "the compound cursor echoed when there is nothing newer"; expect 200 traces-since

# ── What docket refuses at POST /tasks/<pod> ──────────────────────────────────
refuse() { # <name> <status> <note> <path> <body>
  rec "$1" POST "$4" no "$3" --body "$5"; expect "$2" "$1"
}
refuse post-tasks-no-description 400 "a body without description" /tasks/fab-dev '{"externalRef":"x"}'
rec post-tasks-invalid-json POST /tasks/fab-dev no "a body that is not JSON" --raw 'not json'; expect 400 invalid-json
refuse post-tasks-max-tokens-zero 400 "maxTokens must be above 0" /tasks/fab-dev '{"description":"x","maxTokens":0}'
refuse post-tasks-empty-external-ref 400 "externalRef must have a character" /tasks/fab-dev '{"description":"x","externalRef":""}'
refuse post-tasks-gate-without-command 400 "gate needs a command" /tasks/fab-dev '{"description":"x","gate":{}}'
refuse post-tasks-brief-without-objective 422 "a brief with an empty objective" /tasks/fab-dev '{"description":"x","brief":{"objective":""}}'
refuse post-tasks-denied-tool-unknown 422 "deniedTools names a tool docket does not have" /tasks/fab-dev '{"description":"x","restrictions":{"deniedTools":["no-such-tool"]}}'
refuse post-tasks-network-open 422 "restrictions.network may only be none" /tasks/fab-dev '{"description":"x","restrictions":{"network":"open"}}'
refuse post-tasks-unknown-pod 404 "a team docket does not know" /tasks/no-such-team '{"description":"x"}'

# ── The one task: refused at claim by an unknown pipeline, before any agent turn ─
TASK_BODY=$(jq -cn '{description: "Capture only: this task fails at claim, no agent runs.",
  externalRef: "capture-item/attempt-1", idempotencyKey: "capture-attempt-1", baseRef: "main",
  pipeline: "no-such-pipeline", maxTokens: 1000, priority: "normal", trusted: false,
  restrictions: {network: "none", deniedTools: ["bash"]},
  gate: {command: "true", timeoutS: 600},
  brief: {objective: "Capture only", acceptance: ["nothing"], constraints: ["nothing"]}}')
rec post-task-created POST /tasks/fab-dev yes "a task as Tack posts it, with every key it can send; the pipeline does not exist" --body "$TASK_BODY"; expect 201 post-task
TASK_ID=$(jq -r '.task' <<<"$LAST_BODY")
rec post-task-existing POST /tasks/fab-dev yes "the same idempotencyKey again: 200, created false" --body "$TASK_BODY"; expect 200 post-task-again
MINIMAL_BODY=$(jq -cn '{description: "Capture only: a task with the four keys every task carries.",
  externalRef: "capture-item/attempt-2", idempotencyKey: "capture-attempt-2", baseRef: "main",
  pipeline: "no-such-pipeline"}')
OPTIONAL_KEYS=pipeline rec post-task-minimal POST /tasks/fab-dev yes "the four keys every task carries (plus the unknown pipeline that keeps it from running)" --body "$MINIMAL_BODY"; expect 201 post-task-minimal

state=
for _ in $(seq 1 60); do
  state=$(curl -s -m 5 -K "$CURL_AUTH" "$BASE/tasks/fab-dev/$TASK_ID" | jq -r '.status // empty')
  case "$state" in failed|done|blocked|cancelled) break ;; esac
  sleep 0.5
done
[ "$state" = failed ] || die "the task did not fail at claim (status ${state:-none}); see $SCRATCH/daemon.log"
rec task-failed-pipeline GET "/tasks/fab-dev/$TASK_ID" yes "a task refused at claim: the sentence is in reason and the kind in failureKind; there is no error key"; expect 200 task
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = pipeline ] || die "the task failed, but not on its pipeline: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"
rec task-diff-failed GET "/tasks/fab-dev/$TASK_ID/diff" yes "the diff of a task that never started"; expect 200 task-diff
rec task-cancel-finished POST "/tasks/fab-dev/$TASK_ID/cancel" yes "cancel of a task that has ended" --body '{"reason":"the capture"}'; expect 409 cancel
rec traces-after-failure GET /traces/fab-dev yes "the events of a task refused at claim"; expect 200 traces
rec inbox-failed GET /inbox yes "the failed task is in the inbox's failed list"; expect 200 inbox-failed

# ── Routes for tasks and approvals docket does not know ───────────────────────
rec task-unknown GET /tasks/fab-dev/task-unknown yes "a task docket does not know"; expect 404 task-unknown
rec task-cancel-unknown POST /tasks/fab-dev/task-unknown/cancel yes "cancel of a task docket does not know" --body '{"reason":"the capture"}'; expect 404 cancel-unknown
rec task-diff-unknown GET /tasks/fab-dev/task-unknown/diff yes "the diff of a task docket does not know"; expect 404 diff-unknown
OPTIONAL_KEYS=option rec approval-unknown POST /approvals/no-such-token yes "an answer to an approval docket does not know" \
  --body '{"action":"grant","channel":"tack","option":"approve_once","reason":""}'; expect 404 approval-unknown
rec approval-bad-action POST /approvals/no-such-token no "an action that is neither grant nor deny" --body '{"action":"maybe"}'; expect 400 approval-bad-action
OPTIONAL_KEYS=content rec answer-unknown POST /tasks/task-unknown/answer yes "an answer to a task docket does not know" \
  --body '{"pod":"fab-dev","action":"accept","actor":"tack","content":{"answer":"yes"}}'; expect 404 answer-unknown
rec answer-without-pod POST /tasks/task-unknown/answer no "an answer that names no team" --body '{"action":"accept"}'; expect 400 answer-without-pod
rec answer-bad-action POST /tasks/task-unknown/answer no "an action docket does not have" --body '{"pod":"fab-dev","action":"maybe"}'; expect 422 answer-bad-action

# No capture may hold the token.
if grep -rqF -- "$TOKEN" "$OUT"; then die "the bearer token is in a capture; nothing is kept"; fi
say "done: $(ls "$OUT"/*.provenance | wc -l) provenance files in $OUT"
