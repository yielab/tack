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
# `failureKind: needs_human`. Three more need no model either: a pipeline whose only step is a wait
# (its escalation), a task on the team's own pipeline whose first hop writes its prompt manifest and
# then finds no model to ask, and a routine team in a plain folder with the records recipe applied
# (a team with no codebase, its record sets and one row). That folder team also takes a task as Tack
# sends it there and refuses, at claim, one with a `baseRef` (`failureKind` base_ref) and one with a
# `prepare` (`prepare`). Last, it declares `tack_proposals` with `scope: task` on three teams, writes rows through
# docket's own records port and reads them back by task (rows, the two 400s, `limit`, what each member reaches).
# A team's memory (accepted, pending, refused and a stale entry) is recorded by docket's own function in the throwaway home,
# since no model writes the `## Memory` section of a reply; the three status queries are then read over HTTP.
# An approval that expires is not reachable (docket's deadline is hours): that
# fixture stays constructed. It refuses to run when DOCKET_HOME is unset or is the operator's
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
[ "$API" = "6" ] || die "this docket answers apiVersion \"${API:-nothing}\", not \"6\": Tack's docket harness needs docket 0.3.0-beta.1 or later"

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
    -e 's#("claimPid": ?)[0-9]+#\10#g' \
    -e 's#("(reviewCode|accept)": ?")[0-9a-f]{16}"#\10000000000000000"#g'
}
REDACTED="the scratch folder (as /scratch), timestamps (as 2026-01-01T00:00:00Z), 40-hex commit ids (as zeros), task ids (as an all-zero uuid), escalation ids (as esc-000000000000), approval and sweep-run ids (as an all-zero uuid), claimPid, review codes (as zeros)"

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
    /pods/*/records/*) route=/pods/'<pod>'/records/'<set>' ;;
    /pods/*/records) route=/pods/'<pod>'/records ;;
    /pods/*/apply) route=/pods/'<pod>'/apply ;;
    /pods/*/resume) route=/pods/'<pod>'/resume ;;
    /pods/*/memory/*) route=/pods/'<pod>'/memory/'<id>' ;;
    /pods/*/memory) route=/pods/'<pod>'/memory ;;
    /pods/*) route=/pods/'<pod>' ;;
    /tasks/*/*/cancel) route=/tasks/'<pod>'/'<id>'/cancel ;;
    /tasks/*/*/retry) route=/tasks/'<pod>'/'<id>'/retry ;;
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
DEV_BODY=$(jq -cn --arg location "$SCRATCH/repo" '{name: "saas-dev", location: $location, ensure: true}')
rec post-pods-old-fields POST /pods no "the fields before docket read name and location: refused" \
  --body "$(jq -cn --arg path "$SCRATCH/repo" '{project: "saas-dev", path: $path, ensure: true}')"; expect 400 old-fields
rec post-pods-created POST /pods yes "the body Tack sends (no shape is named: docket defaults it); the answer still names the team project and its blueprint" --body "$DEV_BODY"; expect 201 post-pods
rec post-pods-existing POST /pods yes "the same body again: ensure answers 200, created false" --body "$DEV_BODY"; expect 200 post-pods-again
rec post-pods-conflict POST /pods no "the same body without ensure (Tack always sends ensure)" \
  --body "$(jq -c 'del(.ensure)' <<<"$DEV_BODY")"; expect 409 post-pods-conflict
LIGHTS_BODY=$(jq -cn --arg location "$SCRATCH/repo2" '{name: "saas-lights", location: $location, ensure: true}')
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
refuse post-tasks-task-unknown-key 400 "task names a field docket does not have" /tasks/saas-dev '{"description":"x","task":{"objective":"x","colour":"red"}}'
refuse post-tasks-task-bad-type 400 "a task field of the wrong type" /tasks/saas-dev '{"description":"x","task":{"paths":"src/"}}'
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
  task: {objective: "Capture only", doneWhen: "nothing is asked of it", acceptance: ["nothing"],
    constraints: ["nothing"], paths: ["README.md"], risk: "low", kind: "Feature",
    answers: [{question: "Which client?", answer: "The HTTP one"}]}}')
rec post-task-created POST /tasks/saas-dev yes "a task as Tack posts it, with every key it can send, the card as `task` fields and the answers of a previous attempt; the pipeline does not exist" --body "$TASK_BODY"; expect 201 post-task
TASK_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-dev "$TASK_ID" failed
rec post-task-existing POST /tasks/saas-dev yes "the same idempotencyKey again: 200, created false" --body "$TASK_BODY"; expect 200 post-task-again
MINIMAL_BODY=$(jq -cn '{description: "Capture only: a task with the four keys every task carries.",
  externalRef: "capture-item/attempt-2", idempotencyKey: "capture-attempt-2", baseRef: "main",
  pipeline: "no-such-pipeline"}')
OPTIONAL_KEYS=pipeline rec post-task-minimal POST /tasks/saas-dev yes "the four keys every task carries (plus the unknown pipeline that keeps it from running)" --body "$MINIMAL_BODY"; expect 201 post-task-minimal
MINIMAL_ID=$(jq -r '.task' <<<"$LAST_BODY")

wait_for saas-dev "$MINIMAL_ID" failed
# A thread's first turn sent as a conversation to a team whose method has several steps: docket
# enqueues nothing and answers 400 with a code.
CONVERSATION_BODY=$(jq -cn '{description: "Capture only: a thread continued as one docket task.",
  externalRef: "capture-thread/attempt-1", idempotencyKey: "capture-thread-1", baseRef: "main",
  conversation: {maxTurns: 5}}')
OPTIONAL_KEYS=conversation rec post-task-conversation-needs-one-step POST /tasks/saas-dev yes "a thread's first turn sent with conversation to a team whose pipeline has several steps: 400 with code conversation_needs_one_step, and nothing is enqueued" --body "$CONVERSATION_BODY"; expect 400 conversation-needs-one-step
[ "$(jq -r '.code' <<<"$LAST_BODY")" = conversation_needs_one_step ] || die "the refusal carries no conversation_needs_one_step code: $LAST_BODY"

rec task-failed-pipeline GET "/tasks/saas-dev/$TASK_ID" yes "a task refused at claim: the sentence is in reason and the kind in failureKind; there is no error key"; expect 200 task
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = pipeline ] || die "the task failed, but not on its pipeline: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"
rec task-diff-failed GET "/tasks/saas-dev/$TASK_ID/diff" yes "the diff of a task that never started"; expect 200 task-diff
OPTIONAL_KEYS=actor rec task-cancel-finished POST "/tasks/saas-dev/$TASK_ID/cancel" yes "cancel of a task that has ended" --body '{"reason":"the capture","actor":"tack"}'; expect 409 cancel
sleep 2
rec traces-after-failure GET /traces/saas-dev yes "the events of the tasks refused at claim, a second after the last (docket holds back the newest second)"; expect 200 traces

# ── Routes for tasks docket does not know ─────────────────────────────────────
rec task-unknown GET /tasks/saas-dev/task-unknown yes "a task docket does not know"; expect 404 task-unknown
OPTIONAL_KEYS=actor rec task-cancel-unknown POST /tasks/saas-dev/task-unknown/cancel yes "cancel of a task docket does not know" --body '{"reason":"the capture","actor":"tack"}'; expect 404 cancel-unknown
rec task-diff-unknown GET /tasks/saas-dev/task-unknown/diff yes "the diff of a task docket does not know"; expect 404 diff-unknown
rec post-task-retry-unknown POST /tasks/saas-dev/task-unknown/retry yes "retry of a task docket does not know" ; expect 404 retry-unknown

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
mkdir -p "$DOCKET_HOME/config/pods/saas-dev/policies"
cat >"$DOCKET_HOME/config/pods/saas-dev/policies/capture-needs-approval.yaml" <<'POLICY'
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
rec escalation-pending GET "/escalations/$ESC_ID" yes "the one record of a pending approval, the view the list holds (a call that leaves is shown whole here only for an outward approval)"; expect 200 escalation-pending
rec escalations-task-since GET "/escalations?since=$NEXT&task=$GATED_ID" yes "the cursor and the task filter together: nothing newer, next equal to since"; expect 200 escalations-since
[ "$(jq '.escalations | length' <<<"$LAST_BODY")" = 0 ] || die "the cursor returned a record: $LAST_BODY"
rec task-waiting-escalation GET "/tasks/saas-dev/$GATED_ID" yes "a task waiting on an escalation: escalations lists the ids opened for it, and the keys of the retired shapes are gone"; expect 200 task-waiting
rec post-task-retry-refused POST "/tasks/saas-dev/$GATED_ID/retry" yes "retry of a task that is not blocked or failed (here one waiting on a person; a pending, running or done task answers the same): 409, the sentence in error; Tack takes it as 'the retry was already sent' and follows the task" ; expect 409 retry-refused

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
OPTIONAL_KEYS=actor rec cancel-accepted POST "/tasks/saas-dev/$PARKED_ID/cancel" yes "cancel of a task that waits on a person: status cancelled, and its escalation is closed" --body '{"reason":"the capture","actor":"tack"}'; expect 200 cancel

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
mkdir -p "$DOCKET_HOME/config/pods/saas-lights/pipelines"
cat >"$DOCKET_HOME/config/pods/saas-lights/pipelines/capture-approval.yaml" <<'PIPELINE'
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

# ── A wait step: a pause that asks nobody, no model ────────────────────────────
# A pipeline whose only step is a timer parks the task in `waiting_input` with an escalation of
# kind `wait`, one option (`resume`) and the due time as its expiry. No agent turn runs.
guard
mkdir -p "$DOCKET_HOME/config/pods/saas-dev/pipelines"
cat >"$DOCKET_HOME/config/pods/saas-dev/pipelines/capture-wait.yaml" <<'PIPELINE'
name: capture-wait
steps:
  - hold: {wait: "1h"}
PIPELINE
WAIT_BODY=$(jq -cn '{description: "Capture only: a timer, nothing else.",
  externalRef: "capture-item/attempt-7", idempotencyKey: "capture-attempt-7", baseRef: "main",
  pipeline: "capture-wait"}')
WAITING=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' -d "$WAIT_BODY" "$BASE/tasks/saas-dev")
WAIT_ID=$(jq -r '.task' <<<"$WAITING")
case "$WAIT_ID" in task-*) ;; *) die "docket did not accept the task of the wait: $WAITING" ;; esac
wait_for saas-dev "$WAIT_ID" waiting_input
WAIT_ESC=$(curl -s -m 5 -K "$CURL_AUTH" "$BASE/tasks/saas-dev/$WAIT_ID" | jq -r '.escalations[0]')
rec escalation-wait GET "/escalations/$WAIT_ESC" yes "the escalation of a wait step: kind wait, policy machine, the one option resume, expiresAt the due time"; expect 200 escalation-wait
[ "$(jq -r '.kind' <<<"$LAST_BODY")" = wait ] || die "the escalation is not a wait: $LAST_BODY"

# ── A hop that starts and cannot reach a model: its prompt manifest, no model ──
# A task with no pipeline runs the team's own, and its first hop composes its prompt before it asks
# a model. The throwaway home has no provider, so the hop fails there; docket has written the
# manifest of what it was made of, and the task view carries it. Nothing is sent anywhere.
HOP_BODY=$(jq -cn '{description: "Capture only: the hop starts and cannot reach a model.",
  task: {objective: "Capture only", acceptance: ["nothing"]},
  externalRef: "capture-item/attempt-6", idempotencyKey: "capture-attempt-6", baseRef: "main"}')
HOP=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' -d "$HOP_BODY" "$BASE/tasks/saas-dev")
HOP_ID=$(jq -r '.task' <<<"$HOP")
case "$HOP_ID" in task-*) ;; *) die "docket did not accept the task of the hop: $HOP" ;; esac
wait_for saas-dev "$HOP_ID" failed
rec task-failed-hop GET "/tasks/saas-dev/$HOP_ID" yes "a task whose first hop could not reach a model: the hop carries promptManifest (layers, model, family, class, profile) and an artifact without assumptions or questions, which only a reply writes"; expect 200 task-hop
[ "$(jq '.hops[0].promptManifest.layers | length' <<<"$LAST_BODY")" -gt 0 ] || die "the hop carries no prompt manifest: $LAST_BODY"

# ── A team with no codebase and the records it declares: no model ───────────────
# A routine team works in a plain folder, so it has no codebase. The records recipe declares one
# set, which `pod update` makes. A row is written only by a hop's tool, so one is inserted into the
# set's own table, with the columns docket fills itself, to show a row as the read route answers it.
mkdir -p "$SCRATCH/notes"
guard
rec_note=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' \
  -d "$(jq -cn --arg location "$SCRATCH/notes" '{name: "saas-notes", location: $location, blueprint: "routine", ensure: true}')" "$BASE/pods")
[ "$(jq -r '.created' <<<"$rec_note")" = true ] || die "docket would not make the folder team: $rec_note"
(cd "$SCRATCH/notes" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod update records --pod saas-notes </dev/null >"$SCRATCH/pod-update-records.log" 2>&1) \
  || die "docket pod update records failed; see $SCRATCH/pod-update-records.log"
rec pod-folder GET /pods/saas-notes yes "a team with no codebase: repoConfigDir is empty, and the report carries no codebase key (the list of teams does)"; expect 200 pod-folder
rec records GET /pods/saas-notes/records yes "the sets the team declares, with their columns and row counts"; expect 200 records
SETS_DB=$(find "$DOCKET_HOME/state/workspaces/pods/saas-notes" -name records.sqlite | head -1)
[ -n "$SETS_DB" ] || die "the team has no records.sqlite"
python3 -I - "$SETS_DB" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("INSERT INTO entries (_at, _task, _session, _step, _updated_at, ref, title, amount) VALUES "
           "('2026-01-01T00:00:00+00:00', 'task-00000000-0000-0000-0000-000000000000', 'session-0', 'capture', "
           "'2026-01-01T00:00:00+00:00', 'story-1', 'A story', 12.5)")
db.commit()
PY
rec records-rows GET '/pods/saas-notes/records/entries?limit=50&sql=SELECT%20*%20FROM%20entries' yes "one set's rows with the system columns docket fills; cut says how many rows the limit left out"; expect 200 records-rows

# ── A task with a stated install command: `prepare`, no model ────────────────────────────────────
# A team that has a worktree gets the install command Tack states as `prepare` (Tack sends it only
# then, and an empty one to switch docket's detection off). It comes last among the tasks of saas-dev
# so the events captured above stay the same; the unknown pipeline keeps the task from running a model.
PREPARE_BODY=$(jq -cn '{description: "Capture only: a task with a stated install command.",
  externalRef: "capture-item/attempt-8", idempotencyKey: "capture-attempt-8", baseRef: "main",
  pipeline: "no-such-pipeline", prepare: "true"}')
OPTIONAL_KEYS="pipeline prepare" rec post-task-prepare POST /tasks/saas-dev yes "a task for a team with a codebase that states its install command as prepare" --body "$PREPARE_BODY"; expect 201 post-task-prepare
wait_for saas-dev "$(jq -r '.task' <<<"$LAST_BODY")" failed

# ── Tasks to a team that works in a folder: what it takes and what it refuses, no model ─────────
# A folder team has no repository to cut a worktree from. A task as Tack sends it there carries
# neither `baseRef` nor `prepare`; the unknown pipeline keeps it from running a model. A task posted
# with either is claimed and settles `failed` at once, before any hop, with `failureKind` `base_ref`
# or `prepare`. Tack never sends those two, so the refusals are not requests its client makes.
FOLDER_BODY=$(jq -cn '{description: "Capture only: a task for a team in a folder, with no baseRef and no prepare.",
  externalRef: "capture-item/attempt-9", idempotencyKey: "capture-attempt-9",
  pipeline: "no-such-pipeline"}')
OPTIONAL_KEYS="baseRef prepare pipeline" rec post-task-folder POST /tasks/saas-notes yes "a task for a team that works in a folder, as Tack sends it: no baseRef and no prepare, since docket refuses both there" --body "$FOLDER_BODY"; expect 201 post-task-folder
FOLDER_BASE_BODY=$(jq -cn '{description: "Capture only: a baseRef sent to a team in a folder is refused at claim.",
  externalRef: "capture-item/attempt-10", idempotencyKey: "capture-attempt-10", baseRef: "main"}')
rec post-task-folder-base-ref POST /tasks/saas-notes no "a task with a baseRef for a team that works in a folder: accepted, then refused at claim" --body "$FOLDER_BASE_BODY"; expect 201 post-task-folder-base-ref
BASE_REF_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-notes "$BASE_REF_ID" failed
rec task-failed-base-ref-folder GET "/tasks/saas-notes/$BASE_REF_ID" yes "a task with a baseRef for a team in a folder, settled failed before any hop: failureKind base_ref, the sentence in reason, no worktree"; expect 200 task-base-ref-folder
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = base_ref ] || die "the folder task failed, but not on its baseRef: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"
FOLDER_PREPARE_BODY=$(jq -cn '{description: "Capture only: a prepare sent to a team in a folder is refused at claim.",
  externalRef: "capture-item/attempt-11", idempotencyKey: "capture-attempt-11", prepare: "true"}')
rec post-task-folder-prepare POST /tasks/saas-notes no "a task with a non-empty prepare for a team that works in a folder: accepted, then refused at claim" --body "$FOLDER_PREPARE_BODY"; expect 201 post-task-folder-prepare
PREPARE_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-notes "$PREPARE_ID" failed
rec task-failed-prepare-folder GET "/tasks/saas-notes/$PREPARE_ID" yes "a task with a prepare for a team in a folder, settled failed before any hop: failureKind prepare"; expect 200 task-prepare-folder
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = prepare ] || die "the folder task failed, but not on its prepare: $LAST_BODY"
[ "$(jq '.hops | length' <<<"$LAST_BODY")" = 0 ] || die "a hop ran; this capture must spend nothing: $LAST_BODY"

# ── A retry of a failed task: no body, back to pending, nothing spent ───────────
# The retry clears failureKind and reason and queues the task again; the folder team's claim refuses
# the same `prepare` once more, so the task is failed again before the next capture reads it.
rec post-task-retry POST "/tasks/saas-notes/$PREPARE_ID/retry" yes "a failed task retried: 200, status pending, the task id; the next run starts at the step whose hop failed and continues its session (retryContinues), here a task refused at claim that is refused again" ; expect 200 retry
[ "$(jq -r '.status' <<<"$LAST_BODY")" = pending ] || die "the retry did not queue the task: $LAST_BODY"
wait_for saas-notes "$PREPARE_ID" failed

# ── Record sets that belong to a task, and what each member of a team reaches: no model ────────
# A team that plans declares `tack_proposals` with `scope: task` (onboarding-lab/skill/references/
# proposals.md): its `ref` is then unique within one task, and `?task=<id>` reads the rows of one
# task. The set is declared on the folder team (`pod update` from a directory) and on the team with
# a codebase, so one capture shows the members of both blueprints with what each reaches. A row is
# written by an agent's tool, which no model is here to call, so the rows go in through docket's own
# records port (the one its tools use), with a task and a role as their origin. What `add` answers for
# a ref that is already there (`created: false`) is printed by that script and checked, not captured:
# it is the answer of an agent's tool, not of a route.
mkdir -p "$SCRATCH/plan-team/records"
cat >"$SCRATCH/plan-team/records/tack_proposals.yaml" <<'RECORDS'
kind: records
name: tack_proposals
description: Items that the team proposes to Tack, one row each.
key: ref
scope: task
columns:
  - {name: ref, type: string, required: true, description: "A short unique name of this row; other rows name it in parent and depends_on"}
  - {name: title, type: string, required: true, description: "What the item is, in a few words"}
  - {name: description, type: string, description: "What to do and why, as a full card"}
  - {name: type, type: string, description: "The item type, when the project has it"}
  - {name: role, type: string, description: "The project role that does it, by name"}
  - {name: team, type: string, description: "The team that does it, by name"}
  - {name: parent, type: string, description: "The ref of an earlier row this item is a sub-item of"}
  - {name: depends_on, type: string, description: "The refs of the items that must finish first, separated by commas"}
  - {name: acceptance, type: string, description: "What is expected, one sentence a line"}
RECORDS
for pair in "saas-notes:$SCRATCH/notes" "saas-dev:$SCRATCH/repo" "saas-lights:$SCRATCH/repo2"; do
  guard
  (cd "${pair#*:}" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod update "$SCRATCH/plan-team" --pod "${pair%%:*}" </dev/null >"$SCRATCH/pod-update-plan.log" 2>&1) \
    || die "docket pod update failed for ${pair%%:*}; see $SCRATCH/pod-update-plan.log"
done
# A third team declares the set and then denies the `record` kind pod-wide: nobody there can propose.
guard
(cd "$SCRATCH/repo2" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod set deniedTools record --pod saas-lights </dev/null >"$SCRATCH/pod-set-denied.log" 2>&1) \
  || die "docket pod set deniedTools failed; see $SCRATCH/pod-set-denied.log"
cat >"$SCRATCH/write-rows.py" <<'PY'
import json, sys
from docket.bootstrap import bind_adapters
from docket.kernel.ports import records
from docket.kernel.records_port import RecordOrigin

pod, set_name, task, role, rows = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], json.loads(sys.argv[5])
bind_adapters()
store = records.open(pod)
print(json.dumps(separators=(",", ":"), obj=[store.add(set_name, row, RecordOrigin(task=task, session="session-0", step="capture", role=role)).created for row in rows]))
PY
# The python of the docket that runs: the checkout's own environment when there is one.
DOCKET_PY=${DOCKET_PYTHON:-}
[ -n "$DOCKET_PY" ] || { [ -x "$SRC/.venv/bin/python" ] && DOCKET_PY=$SRC/.venv/bin/python; }
[ -n "$DOCKET_PY" ] || DOCKET_PY=$(sed -n 's#^exec "\(.*\)" -m docket.*#\1#p' "$DOCKET_BIN" | head -1)
[ -x "$DOCKET_PY" ] || die "no python for docket's records port; set DOCKET_PYTHON"
add_rows() { # <pod> <set> <task> <role> <json rows>: prints the `created` of each
  guard
  "${DOCKET_ENV[@]}" "$DOCKET_PY" -I "$SCRATCH/write-rows.py" "$@" 2>"$SCRATCH/write-rows.log" || die "writing rows failed: $(cat "$SCRATCH/write-rows.log")"
}
TASK_A=task-capture-a
TASK_B=task-capture-b
PLAN_ROWS=$(jq -cn '[
  {ref: "research", title: "Research the sign-up flow", description: "Read the flow and list what blocks it.", type: "epic", role: "analyst", acceptance: "The blockers are listed.\nEach has a cause."},
  {ref: "copy", title: "Fix the sign-up copy", parent: "research", role: "writer"},
  {ref: "form", title: "Fix the sign-up form", parent: "research", depends_on: "copy"}
]')
[ "$(add_rows saas-notes tack_proposals "$TASK_A" lead "$PLAN_ROWS")" = "[true,true,true]" ] || die "the task-scoped rows were not all created"
# The same ref again, in the same task: the row stays, nothing new is written.
[ "$(add_rows saas-notes tack_proposals "$TASK_A" lead '[{"ref":"research","title":"Changed"}]')" = "[false]" ] || die "a repeated ref in a task-scoped set was written again"
# The same ref from another task: a task-scoped set keeps its own row for each task.
[ "$(add_rows saas-notes tack_proposals "$TASK_B" analyst '[{"ref":"research","title":"Another task, same ref"}]')" = "[true]" ] || die "a task-scoped set refused the same ref from another task"
# A pod-scoped set (`entries`, from the recipe) holds one row per ref across every task.
[ "$(add_rows saas-notes entries "$TASK_A" lead '[{"ref":"story-2","title":"First"}]')" = "[true]" ] || die "the pod-scoped row was not created"
[ "$(add_rows saas-notes entries "$TASK_B" lead '[{"ref":"story-2","title":"Same ref, other task"}]')" = "[false]" ] || die "a pod-scoped set took the same ref from another task"

rec records-scoped GET /pods/saas-notes/records yes "the sets of a team that declares tack_proposals with scope task; every set carries scope, pod or task, and a client that sees it knows ?task= is supported"; expect 200 records-scoped
rec records-rows-task GET "/pods/saas-notes/records/tack_proposals?task=$TASK_A&limit=50" yes "the rows one task wrote, in write order; _role is the role of the hop that wrote the row, and a column the row left empty is null"; expect 200 records-rows-task
rec records-rows-task-other GET "/pods/saas-notes/records/tack_proposals?task=$TASK_B&limit=50" no "another task wrote a row with the same ref and has its own: a task-scoped set does not share"; expect 200 records-rows-task-other
rec records-rows-task-limit GET "/pods/saas-notes/records/tack_proposals?task=$TASK_A&limit=2" no "limit with task: the first rows in write order are kept and cut counts the rest"; expect 200 records-rows-task-limit
rec records-rows-pod-shared GET "/pods/saas-notes/records/entries?task=$TASK_B&limit=50" no "a pod-scoped set read by task: the task sees only the rows it wrote, and a ref taken by another task is not its row"; expect 200 records-rows-pod-shared
rec records-task-unknown GET "/pods/saas-notes/records/tack_proposals?task=task-never-wrote&limit=50" no "a task that wrote nothing: no rows, not an error"; expect 200 records-task-unknown
rec records-task-empty GET "/pods/saas-notes/records/tack_proposals?task=" no "task given empty: refused"; expect 400 records-task-empty
rec records-task-and-sql GET "/pods/saas-notes/records/tack_proposals?task=$TASK_A&sql=SELECT%20*%20FROM%20tack_proposals" no "task together with sql: refused"; expect 400 records-task-and-sql
rec records-rows-sql-columns GET '/pods/saas-notes/records/tack_proposals?limit=50&sql=SELECT%20*%20FROM%20tack_proposals' no "the same set read by statement: where _role sits among the columns"; expect 200 records-rows-sql-columns
rec pod-capabilities GET /pods/saas-dev yes "a team with a codebase that declares tack_proposals: every member lists kinds, tools and records {write, query}; the lead writes no project file and proposes, the implementer writes files and has no record tool"; expect 200 pod-capabilities
rec pod-capabilities-denied GET /pods/saas-lights yes "a team that declares tack_proposals and denies the record kind: no member lists a record tool and records.write is empty, though the set exists"; expect 200 pod-capabilities-denied
rec pod-capabilities-folder GET /pods/saas-notes yes "a team in a folder that declares tack_proposals: the lead, reviewer, analyst and writer, each with what it reaches"; expect 200 pod-capabilities-folder

# ── What a team says of its contract, its budget, its shape: no model ───────────────────────────
# A team in a folder with the web-research recipe applied: its pipeline is bound (`default` on one
# method), a method's file that does not load carries `error`, and the researchers' three needs are
# unmet. Applying a recipe is `docket pod update`, which runs no model.
guard
rec_contract=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' \
  -d '{"name": "saas-contract", "blueprint": "respond", "ensure": true}' "$BASE/pods")
[ "$(jq -r '.created' <<<"$rec_contract")" = true ] || die "docket would not make the contract team: $rec_contract"
mkdir -p "$SCRATCH/contract"
(cd "$SCRATCH/contract" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod update web-research --pod saas-contract </dev/null >"$SCRATCH/pod-update-web.log" 2>&1) \
  || die "docket pod update web-research failed; see $SCRATCH/pod-update-web.log"
mkdir -p "$DOCKET_HOME/config/pods/saas-contract/pipelines"
printf 'not: [valid\n' >"$DOCKET_HOME/config/pods/saas-contract/pipelines/capture-broken.yaml"
rec pod-contract GET /pods/saas-contract yes "a team in a folder with a bound pipeline: delivers is a list (report), methods lists every pipeline a task may name (default on the bound one, missingRoles for a step the roster cannot fill, error for a file that does not load) and needs holds the three unmet needs the web-research recipe records"; expect 200 pod-contract

# A team over its budget: a day window, a cap of 1000 and 2500 tokens spent today. docket counts a
# pod's tokens from its sessions' per-day usage, which only a model writes, so one session record is
# written the way docket's own tests do. The first task's first hop meets the gate and pauses the
# team, which is the state a spent budget leaves; no model is asked.
guard
rec_budget=$(curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' \
  -d "$(jq -cn --arg location "$SCRATCH/repo" '{name: "saas-budget", location: $location, ensure: true}')" "$BASE/pods")
[ "$(jq -r '.created' <<<"$rec_budget")" = true ] || die "docket would not make the budget team: $rec_budget"
for setting in "budgetTokens 1000" "budgetWindow day"; do
  (cd "$SCRATCH/repo" && "${DOCKET_ENV[@]}" "$DOCKET_BIN" pod set $setting --pod saas-budget </dev/null >"$SCRATCH/pod-set-budget.log" 2>&1) \
    || die "docket pod set $setting failed; see $SCRATCH/pod-set-budget.log"
done
cat >"$SCRATCH/spend.py" <<'PY'
import json, sys
from datetime import datetime, timezone
from urllib.parse import quote
from docket.kernel.home import current_home

agent, tokens = sys.argv[1], int(sys.argv[2])
day = datetime.now(timezone.utc).strftime("%Y-%m-%d")
key = f"agent:{agent}:default"
folder = current_home().sessions_dir / quote(key, safe="")
folder.mkdir(parents=True, exist_ok=True)
usage = {"inputTokens": 0, "outputTokens": tokens, "cachedTokens": 0, "turns": 1}
(folder / "session.json").write_text(json.dumps({
    "sessionKey": key, "created": f"{day}T00:00:00Z", "updated": f"{day}T00:00:00Z",
    "messages": [], "usage": usage, "usageByDay": {day: usage},
}))
PY
guard
"${DOCKET_ENV[@]}" "$DOCKET_PY" -I "$SCRATCH/spend.py" saas-budget-implementer 2500 || die "could not record the tokens spent"
rec pod-budget-spent GET /pods/saas-budget yes "a team whose day budget is spent but that has not met the gate yet: budget.used is above budget.tokens and paused is false" ; expect 200 pod-budget-spent
curl -s -m 30 -K "$CURL_AUTH" -H 'Content-Type: application/json' -d '{"description":"meets the budget gate"}' "$BASE/tasks/saas-budget" >/dev/null
for _ in $(seq 1 90); do
  [ "$(curl -s -m 5 -K "$CURL_AUTH" "$BASE/pods/saas-budget" | jq -r '.paused')" = true ] && break
  sleep 0.5
done
rec pod-budget GET /pods/saas-budget yes "a team paused on its budget: paused true and pausedReason budget beside budget {tokens, used, window day, windowStart}, used above tokens"; expect 200 pod-budget
[ "$(jq -r '.paused' <<<"$LAST_BODY")" = true ] || die "the budget team was not paused; see $SCRATCH/daemon.log"
rec pods-paused GET /pods yes "a row of a paused team carries paused, pausedReason and budget, as the team's report does"; expect 200 pods-paused
rec pod-resumed POST /pods/saas-budget/resume yes "a paused team resumed: the whole report of the team plus resumed true; the budget is still spent, so docket pauses it again at the next hop"; expect 200 pod-resumed
rec pod-resume-not-paused POST /pods/saas-budget/resume yes "a team that is not paused: 200 with the same report and resumed false, nothing is written" ; expect 200 pod-resume-not-paused
rec pod-resume-unknown POST /pods/no-such-team/resume yes "a team docket does not know"; expect 404 pod-resume-unknown

# A team made from a shape, and a folder of the repository applied to it. A folder shape takes no
# location from Tack: docket gives the team its own working folder, so the team never works inside
# the checkout. The folder of the repository is `.docket/teams/<team>/`; its pod.yaml names its
# shape and asks for a fetch host, which docket reviews before it runs anything on this machine.
rec post-pods-shape POST /pods yes "the body Tack sends to make a team in a folder from a shape: no location, since docket gives the team its own folder" \
  --body '{"name": "saas-team", "blueprint": "respond", "ensure": true}'; expect 201 post-pods-shape
TEAM_DIR=$SCRATCH/team-repo/.docket/teams/saas-team
mkdir -p "$TEAM_DIR"
printf 'shape: respond\nsettings:\n  fetchDomains:\n    - docs.python.org\n' >"$TEAM_DIR/pod.yaml"
APPLY_BODY=$(jq -cn --arg directory "$TEAM_DIR" '{directory: $directory}')
OPTIONAL_KEYS=accept rec pod-apply-needs-code POST /pods/saas-team/apply yes "a folder that would run something on this machine and no accept: 409 with the reviewCode and what runs" --body "$APPLY_BODY"; expect 409 pod-apply-needs-code
CODE=$(jq -r '.reviewCode' <<<"$LAST_BODY")
[ -n "$CODE" ] && [ "$CODE" != null ] || die "docket gave no review code: $LAST_BODY"
OPTIONAL_KEYS=accept rec pod-apply-code-changed POST /pods/saas-team/apply yes "an accept that is not the code of what the folder runs: 409, no reviewCode, runs lists what changed" \
  --body "$(jq -c '. + {accept: "0123456789abcdef"}' <<<"$APPLY_BODY")"; expect 409 pod-apply-code-changed
OPTIONAL_KEYS=accept rec pod-apply-applied POST /pods/saas-team/apply yes "the same call with the reviewCode as accept: 200, one item per thing the folder declares" \
  --body "$(jq -c --arg code "$CODE" '. + {accept: $code}' <<<"$APPLY_BODY")"; expect 200 pod-apply-applied
OPTIONAL_KEYS=accept rec pod-apply-again POST /pods/saas-team/apply yes "the same folder again with no accept: nothing runs that was not reviewed, every item is skipped" --body "$APPLY_BODY"; expect 200 pod-apply-again
rec pod-apply-no-directory POST /pods/saas-team/apply no "a team in a folder with no directory: refused, since it has no repository to read .docket/ from" --body '{}'; expect 400 pod-apply-no-directory
rec pod-apply-unknown POST /pods/no-such-team/apply yes "a team docket does not know" --body "$APPLY_BODY"; expect 404 pod-apply-unknown
OTHER_DIR=$SCRATCH/team-repo/.docket/teams/other
mkdir -p "$OTHER_DIR"
printf 'shape: build\n' >"$OTHER_DIR/pod.yaml"
OPTIONAL_KEYS=accept rec pod-apply-shape-mismatch POST /pods/saas-team/apply yes "a folder whose pod.yaml names a shape the team was not made from: 400 naming both" \
  --body "$(jq -cn --arg directory "$OTHER_DIR" '{directory: $directory}')"; expect 400 pod-apply-shape-mismatch

# A task that names a method the team cannot staff: accepted, then refused at claim with
# failureKind pipeline and the roles in the sentence, before any hop.
ROLES_BODY=$(jq -cn '{description: "a method the team cannot staff", pipeline: "cross-family-review"}')
OPTIONAL_KEYS="pipeline" rec post-task-pipeline-roles POST /tasks/saas-dev no "a task naming a method whose roles the team lacks: POST /tasks does not refuse it" --body "$ROLES_BODY"; expect 201 post-task-pipeline-roles
ROLES_ID=$(jq -r '.task' <<<"$LAST_BODY")
wait_for saas-dev "$ROLES_ID" failed
rec task-failed-pipeline-roles GET "/tasks/saas-dev/$ROLES_ID" yes "the same task refused at claim: failed, failureKind pipeline, the pipeline, its source and each missing role with its step in reason, no hop"; expect 200 task-pipeline-roles
[ "$(jq -r '.failureKind' <<<"$LAST_BODY")" = pipeline ] || die "the task was not refused for its pipeline: $LAST_BODY"

# ── What a team has learnt and what waits to be approved: no model ──────────────────────────────
# A team's memory is written when a task ends, from the `## Memory` section of a hop's reply, and
# docket decides every entry itself: a path present at the task's commit is accepted, a path that
# is not is refused, and a pitfall or an instruction waits for the operator. No model writes a reply
# here, so docket's own function (`record_task_memory`) is called in the throwaway home with one
# hop whose reply carries the section and whose commit is the seed of the throwaway repository.
cat >"$SCRATCH/write-memory.py" <<'PY'
import json, os, subprocess, sys
from types import SimpleNamespace
from docket.bootstrap import bind_adapters
from docket.observability import trace
from docket.teams import memory

pod, root, task = sys.argv[1], sys.argv[2], sys.argv[3]
bind_adapters()

def git(*args):
    return subprocess.run(["git", "-C", root, "-c", "user.name=capture", "-c", "user.email=capture@localhost", *args],
                          capture_output=True, text=True, check=True).stdout.strip()

def finish(task_id, reply):
    """What a finished task leaves for docket to record: one ok hop at the repository's commit."""
    session = f"agent:{pod}:{task_id}"
    trace.trace_event(pod, session, "implementer", "session_start", json.dumps({"source": "capture"}))
    hop = SimpleNamespace(role="implementer", ok=True, output=reply, evidence={"commit": git("rev-parse", "HEAD")}, verify=None,
                          trace={"project": pod, "session": session, "firstTs": "0", "lastTs": "9"})
    return memory.record_task_memory(pod, task_id, SimpleNamespace(hops=[hop]), root=root)

os.makedirs(os.path.join(root, "notes"), exist_ok=True)
with open(os.path.join(root, "notes", "old.md"), "w") as handle:
    handle.write("to be removed\n")
git("add", "-A")
git("commit", "-q", "-m", "notes")
entries = finish(task, "\n".join([
    "Done.",
    "",
    "## Memory",
    "- generated: README.md -- The repository's readme, written by hand.",
    "- generated: notes/old.md -- A note that a later commit removes.",
    "- generated: dist/gone.js -- A build output that is not in the repository.",
    "- pitfall: README.md -- Edits to the readme need the docs rebuilt.",
    "- convention: docs/ -- Every page there opens with its audience.",
]))
# A later task removes the note: docket marks the accepted entry stale when it next looks.
git("rm", "-q", "notes/old.md")
git("commit", "-q", "-m", "remove the note")
finish(task + "-later", "Done.")
print(json.dumps(sorted((e.status, e.kind, e.subject) for e in entries)))
PY
guard
MEMORY_WRITTEN=$("${DOCKET_ENV[@]}" "$DOCKET_PY" -I "$SCRATCH/write-memory.py" saas-dev "$SCRATCH/repo" task-capture-memory 2>"$SCRATCH/write-memory.log") \
  || die "writing the team's memory failed: $(cat "$SCRATCH/write-memory.log")"
[ "$(jq -c '[.[][0]] | unique' <<<"$MEMORY_WRITTEN")" = '["accepted","pending","refused"]' ] || die "docket did not decide the entries as the capture needs: $MEMORY_WRITTEN"
rec pod-memory-accepted GET '/pods/saas-dev/memory?status=accepted' yes "a team's accepted entries, oldest first: docket checked each path against the task's commit; evidence is empty for a path, provenance names the task, the date and the commit; a later task removed one of the files, so its entry is stale"; expect 200 pod-memory-accepted
[ "$(jq -c '[.entries[].stale]' <<<"$LAST_BODY")" = '[false,true]' ] || die "the team does not hold one fresh and one stale accepted entry: $LAST_BODY"
rec pod-memory-pending GET '/pods/saas-dev/memory?status=pending' yes "the entries that wait for the operator: a pitfall and a convention are held, reason says why, and docket's inbox gives them docket pod memory approve and refuse"; expect 200 pod-memory-pending
[ "$(jq '.entries | length' <<<"$LAST_BODY")" = 2 ] || die "the team does not hold the two pending entries: $LAST_BODY"
rec pod-memory-refused GET '/pods/saas-dev/memory?status=refused' no "the entries docket refused itself, with its reason; Tack does not read them"; expect 200 pod-memory-refused
rec pod-memory-default GET /pods/saas-dev/memory no "no status: accepted, as docket documents"; expect 200 pod-memory-default
rec pod-memory-none GET '/pods/saas-lights/memory?status=pending' no "a team that has learnt nothing: an empty list, not an error"; expect 200 pod-memory-none
rec pod-memory-bad-status GET '/pods/saas-dev/memory?status=waiting' no "a status docket does not have: 400 naming the three"; expect 400 pod-memory-bad-status
rec pod-memory-unknown GET '/pods/no-such-team/memory?status=pending' no "a team docket does not know: 404"; expect 404 pod-memory-unknown

# ── Deciding what a team waits to learn, and entries with every kind of evidence: no model ─────
# `memoryDecide`: POST /pods/<pod>/memory/<id> approves or refuses a pending entry. The two
# entries `saas-dev` holds pending are decided here, after every read of them above.
PENDING_IDS=$(curl -s -m 30 -K "$CURL_AUTH" "$BASE/pods/saas-dev/memory?status=pending" | jq -r '.entries[].id')
APPROVE_ID=$(sed -n 1p <<<"$PENDING_IDS")
REFUSE_ID=$(sed -n 2p <<<"$PENDING_IDS")
[ -n "$APPROVE_ID" ] && [ -n "$REFUSE_ID" ] || die "the team does not hold two pending entries: $PENDING_IDS"
OPTIONAL_KEYS=reason rec pod-memory-approved POST "/pods/saas-dev/memory/$APPROVE_ID" yes "a pending entry approved: 200 and the one entry as the list gives it, not wrapped in entries, now accepted" \
  --body '{"decision":"approve"}'; expect 200 pod-memory-approved
OPTIONAL_KEYS=reason rec pod-memory-decide-settled POST "/pods/saas-dev/memory/$APPROVE_ID" yes "the same entry again: it is not pending any more, 409, and it is left as it was" \
  --body '{"decision":"approve"}'; expect 409 pod-memory-decide-settled
OPTIONAL_KEYS=reason rec pod-memory-refused-by-http POST "/pods/saas-dev/memory/$REFUSE_ID" yes "a pending entry refused with a reason: 200 and the entry, now refused, its reason the one sent" \
  --body '{"decision":"refuse","reason":"The readme is rebuilt by CI."}'; expect 200 pod-memory-refused-by-http
rec pod-memory-decide-unknown-entry POST /pods/saas-dev/memory/mem-nothing no "an entry the team does not have: 404" --body '{"decision":"approve"}'; expect 404 pod-memory-decide-unknown-entry
rec pod-memory-decide-unknown-pod POST /pods/no-such-team/memory/mem-nothing no "a team docket does not dispatch: 404" --body '{"decision":"approve"}'; expect 404 pod-memory-decide-unknown-pod
rec pod-memory-decide-no-reason POST "/pods/saas-dev/memory/$REFUSE_ID" no "a refusal with no reason: 400" --body '{"decision":"refuse"}'; expect 400 pod-memory-decide-no-reason
rec pod-memory-decide-blank-reason POST "/pods/saas-dev/memory/$REFUSE_ID" no "a refusal whose reason is blank: 400" --body '{"decision":"refuse","reason":"  "}'; expect 400 pod-memory-decide-blank-reason
rec pod-memory-decide-bad-decision POST "/pods/saas-dev/memory/$REFUSE_ID" no "a decision that is neither approve nor refuse: 400" --body '{"decision":"keep"}'; expect 400 pod-memory-decide-bad-decision
rec pod-memory-decide-bad-reason POST "/pods/saas-dev/memory/$REFUSE_ID" no "a reason that is not a string: 400" --body '{"decision":"refuse","reason":3}'; expect 400 pod-memory-decide-bad-reason
rec pod-memory-decide-not-object POST "/pods/saas-dev/memory/$REFUSE_ID" no "a body that is not a JSON object: 400" --body '["approve"]'; expect 400 pod-memory-decide-not-object
rec pod-memory-decide-invalid-json POST "/pods/saas-dev/memory/$REFUSE_ID" no "a body that is not JSON: 400" --raw 'not json'; expect 400 pod-memory-decide-invalid-json
rec pod-memory-decide-unauthorized POST "/pods/saas-dev/memory/$REFUSE_ID" no "no bearer token: 401" --body '{"decision":"approve"}' --no-auth; expect 401 pod-memory-decide-unauthorized

# Entries with every kind of evidence and every status. No task of the capture runs a command or a
# tool, so the evidence of an entry about one cannot come from a task: this builds the team's memory
# file with docket's own types, the way docket's own HTTP tests do (tests/integration/interfaces/
# test_serve_pod_memory.py). That is a test-side recipe, not a product API: the entries are real in
# shape, and docket reads and lists them as it does any other.
cat >"$SCRATCH/build-memory.py" <<'PY'
import sys
from docket.kernel.home import current_home
from docket.platform import store
from docket.teams.memory import MemoryEntry, entry_id

pod = sys.argv[1]

def entry(kind, subject, note, status, reason, evidence=None, stale=False):
    return MemoryEntry(id=entry_id(kind, subject, note), kind=kind, subject=subject, note=note, status=status,
                       reason=reason, provenance={"task": "task-capture-built", "date": "2026-01-01", "commit": "0" * 40},
                       stale=stale, evidence=evidence or {})

entries = [
    entry("tool", "bash make test", "The suite runs with make test.", "accepted", "exit status found in the task's trace", {"exit": 1}),
    entry("tool", "grep", "Search the tree with grep.", "accepted", "tool call found in the task's trace", {"ok": True}),
    entry("generated", "dist/bundle.js", "Built output, not source.", "accepted", "path present at the task commit"),
    entry("convention", "docs/", "Every page opens with its audience.", "pending", "awaits operator approval"),
    entry("tool", "make lint", "Lint with make lint.", "refused", "no call or exit status for this in the task's trace"),
]
path = current_home().pod_memory_path(pod)
path.parent.mkdir(parents=True, exist_ok=True)
store.write_json(path, {"entries": [e.to_dict() for e in entries]})
PY
guard
"${DOCKET_ENV[@]}" "$DOCKET_PY" -I "$SCRATCH/build-memory.py" saas-notes || die "could not build the memory entries"
rec pod-memory-evidence-accepted GET '/pods/saas-notes/memory?status=accepted' no "constructed in the throwaway home with docket's own MemoryEntry (see the capture script): a command's exit status, a tool call's ok and a path with no evidence; docket lists them as it lists any entry"; expect 200 pod-memory-evidence-accepted
rec pod-memory-evidence-pending GET '/pods/saas-notes/memory?status=pending' no "built as above: one pending entry"; expect 200 pod-memory-evidence-pending
rec pod-memory-evidence-refused GET '/pods/saas-notes/memory?status=refused' no "built as above: one refused entry"; expect 200 pod-memory-evidence-refused

# No capture may hold the token.
if grep -rqF -- "$TOKEN" "$OUT"; then die "the bearer token is in a capture; nothing is kept"; fi
say "done: $(ls "$OUT"/*.provenance | wc -l) provenance files in $OUT"
