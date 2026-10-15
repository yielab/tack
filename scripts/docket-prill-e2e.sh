#!/usr/bin/env bash
# Stands up everything `frontend/e2e/docket-prill.spec.ts` drives, then runs it:
#
#   - `tack` built from this checkout with the SPA embedded (`--features embed-spa`),
#     serving a fresh database on its own port, never the dev pair or `tack.toml`;
#   - `docket` from its source tree (DOCKET_SRC, default ~/Sites/rack-cli; it must answer
#     operator API version 5, docket 0.2.0-beta.6 or later) and `prill` from its source tree
#     (PRILL_SRC, default ~/Sites/prill), each behind a one-line launcher placed ahead of any
#     installed release on PATH;
#   - a scratch HOME for the server and the runner, and an isolated DOCKET_HOME inside it in
#     which docket's own setup is made with docket's own command
#     (`docket setup provider add local <endpoint>`), so docket reaches the local model the
#     way a person's docket does and Tack holds no endpoint;
#   - `docket start --dispatch` run by this script on loopback, with two teams (pods),
#     `fab-code` and `fab-content`, over the fixture repository; the runner's `[docket]`
#     table points at it with `base_url`, so the runner starts nothing;
#   - a runner configuration with prill as the verifier (`[verify]` with
#     `kind = "prill"`, the runner's own seam).
#
# The spec enrols that runner through the Agents page and starts it itself, so
# the whole workflow (project, settings, items, runs, packs, review) is one UI
# session. Every run is a real model call against DOCKET_PRILL_MODEL_BASE_URL, on the
# model docket's team members run on.
#
# Usage: scripts/docket-prill-e2e.sh [extra playwright args…]
# Knobs (environment):
#   DOCKET_PRILL_MODEL_BASE_URL  OpenAI-compatible endpoint, without /v1 (default http://127.0.0.1:8081)
#   PRILL_IMAGE                  prill's sandbox image, present locally (default prill-verify-py313:local);
#                                passed to prill by its image id
#   DOCKET_PRILL_WORK            working directory (default: a fresh mktemp dir, kept afterwards)
#   DOCKET_PRILL_PORT            the server's port (default 3377)
#   DOCKET_PRILL_DOCKET_PORT     the docket daemon's port (default 7399)
#   DOCKET_PRILL_SHOTS           where the screenshots go (default frontend/test-results/docket-prill)
#   DOCKET_PRILL_SKIP_BUILD=1    reuse target/debug/tack and frontend/dist as they are
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DOCKET_SRC=${DOCKET_SRC:-$HOME/Sites/rack-cli}
PRILL_SRC=${PRILL_SRC:-$HOME/Sites/prill}
MODEL_BASE_URL=${DOCKET_PRILL_MODEL_BASE_URL:-http://127.0.0.1:8081}
PRILL_IMAGE=${PRILL_IMAGE:-prill-verify-py313:local}
PORT=${DOCKET_PRILL_PORT:-3377}
DOCKET_PORT=${DOCKET_PRILL_DOCKET_PORT:-7399}
WORK=${DOCKET_PRILL_WORK:-$(mktemp -d -t docket-prill-XXXXXX)}
SHOTS=${DOCKET_PRILL_SHOTS:-$ROOT/frontend/test-results/docket-prill}
SYSTEM_PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

if command -v rustup >/dev/null 2>&1; then
  PATH="$(dirname "$(command -v rustup)"):$PATH"
  export PATH
fi

say() { printf '\n==> %s\n' "$*"; }
die() { printf 'docket-prill-e2e: %s\n' "$*" >&2; exit 1; }

# ── Preconditions ─────────────────────────────────────────────────────────────
[ -d "$DOCKET_SRC/.venv" ] || die "no $DOCKET_SRC/.venv — run 'uv sync' in the docket checkout first"
[ -x "$PRILL_SRC/.venv/bin/prill" ] || die "no $PRILL_SRC/.venv/bin/prill — run 'uv sync' in the prill checkout first"
docker image inspect "$PRILL_IMAGE" >/dev/null 2>&1 || die "the prill sandbox image $PRILL_IMAGE is not present locally (prill never pulls one)"
# prill counts only a digest or a bare `sha256:` id as pinned; a local tag would
# put `image_not_pinned` on every pack, so the runner is handed the id.
PRILL_IMAGE_ID=$(docker image inspect "$PRILL_IMAGE" --format '{{.Id}}')
curl -fsS -m 5 "$MODEL_BASE_URL/v1/models" >/dev/null || die "no OpenAI-compatible endpoint answers at $MODEL_BASE_URL/v1/models"
command -v ffmpeg >/dev/null 2>&1 || echo "note: ffmpeg not found; the spec records no GIF without it"
if ss -ltn 2>/dev/null | awk '{print $4}' | grep -q ":$PORT\$"; then
  die "port $PORT is already taken — set DOCKET_PRILL_PORT"
fi

AGENT_HOME="$WORK/home"
mkdir -p "$WORK/bin" "$WORK/server" "$WORK/runner-state" "$AGENT_HOME" "$SHOTS"
chmod 700 "$WORK/runner-state"

# ── The source-backed launchers ───────────────────────────────────────────────
cat > "$WORK/bin/docket" <<EOF
#!/usr/bin/env bash
exec "$DOCKET_SRC/.venv/bin/python" -m docket "\$@"
EOF
cat > "$WORK/bin/prill" <<EOF
#!/usr/bin/env bash
exec "$PRILL_SRC/.venv/bin/prill" "\$@"
EOF
chmod +x "$WORK/bin/docket" "$WORK/bin/prill"
# The API version first, and before anything is built: a docket that does not answer
# apiVersion "5" cannot run this, however much else is in place.
DOCKET_HOME_DIR="$AGENT_HOME/.docket"
mkdir -p "$DOCKET_HOME_DIR"
DOCKET_ENV=(env -i HOME="$AGENT_HOME" DOCKET_HOME="$DOCKET_HOME_DIR" PATH="$WORK/bin:$SYSTEM_PATH")
DOCKET_TOKEN_FILE="$WORK/docket-token"
umask 077; head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n' >"$DOCKET_TOKEN_FILE"; umask 022
"${DOCKET_ENV[@]}" docket setup provider add local "$MODEL_BASE_URL/v1" --model local-model --ctx 32768 --max-tokens 4096 \
  </dev/null >"$WORK/docket-setup.log" 2>&1 || die "docket setup provider add failed — see $WORK/docket-setup.log"
say "starting docket start --dispatch on :$DOCKET_PORT"
if ss -ltn 2>/dev/null | awk '{print $4}' | grep -q ":$DOCKET_PORT\$"; then
  die "port $DOCKET_PORT is already taken — set DOCKET_PRILL_DOCKET_PORT"
fi
"${DOCKET_ENV[@]}" docket start --dispatch --bind 127.0.0.1 --port "$DOCKET_PORT" --token-file "$DOCKET_TOKEN_FILE" \
  </dev/null >"$WORK/docket-daemon.log" 2>&1 &
echo $! >"$WORK/docket.pid"
DOCKET_URL="http://127.0.0.1:$DOCKET_PORT"
STATUS_JSON=""
for _ in $(seq 1 60); do
  STATUS_JSON=$(curl -fsS -m 3 -H "Authorization: Bearer $(cat "$DOCKET_TOKEN_FILE")" "$DOCKET_URL/status.json" 2>/dev/null || true)
  [ -n "$STATUS_JSON" ] && break
  sleep 0.5
done
[ "$(jq -r '.apiVersion // empty' <<<"$STATUS_JSON" 2>/dev/null)" = "5" ] \
  || die "docket at $DOCKET_SRC does not answer apiVersion \"5\" on GET /status.json (needs docket 0.2.0-beta.6) — point DOCKET_SRC at a checkout that does; see $WORK/docket-daemon.log"
{
  echo "tack: $(git -C "$ROOT" rev-parse --short HEAD) ($(git -C "$ROOT" branch --show-current))$([ -n "$(git -C "$ROOT" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket: $("$WORK/bin/docket" --version 2>/dev/null | tail -1) from $DOCKET_SRC @ $(git -C "$DOCKET_SRC" rev-parse --short HEAD) ($(git -C "$DOCKET_SRC" branch --show-current))$([ -n "$(git -C "$DOCKET_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket installed release (not used): $(command -v docket || echo none) $(docket --version 2>/dev/null | tail -1)"
  echo "prill: $("$WORK/bin/prill" --version 2>/dev/null | tail -1) from $PRILL_SRC @ $(git -C "$PRILL_SRC" rev-parse --short HEAD) ($(git -C "$PRILL_SRC" branch --show-current))$([ -n "$(git -C "$PRILL_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "prill image: $PRILL_IMAGE $PRILL_IMAGE_ID"
  echo "docket api: $(jq -r '.apiVersion' <<<"$STATUS_JSON") on $DOCKET_URL, DOCKET_HOME $DOCKET_HOME_DIR"
  echo "model endpoint: $MODEL_BASE_URL/v1 -> $(curl -fsS -m 5 "$MODEL_BASE_URL/v1/models" | python3 -c 'import json,sys; print(", ".join(m["id"] for m in json.load(sys.stdin).get("data", [])))')"
} | tee "$WORK/versions.txt"

# ── The two teams, over the fixture repository ────────────────────────────────
# The fixture repository is made here, the spec adopts it as its folder.
FIXTURE="$WORK/calc"
if [ ! -d "$FIXTURE/.git" ]; then
  mkdir -p "$FIXTURE"
  git -C "$FIXTURE" init -q -b main
  printf '# calc\n' >"$FIXTURE/README.md"
  git -C "$FIXTURE" add -A
  git -C "$FIXTURE" -c user.name=e2e -c user.email=e2e@localhost commit -q -m "seed"
fi
for POD in fab-code fab-content; do
  CODE=$(curl -fsS -m 30 -o "$WORK/pod-$POD.json" -w '%{http_code}' -X POST "$DOCKET_URL/pods" \
    -H "Authorization: Bearer $(cat "$DOCKET_TOKEN_FILE")" -H 'Content-Type: application/json' \
    -d "{\"name\": \"$POD\", \"location\": \"$FIXTURE\", \"blueprint\": \"software\", \"ensure\": true}" || true)
  case "$CODE" in 200|201) ;; *) die "docket would not make the team $POD (HTTP $CODE) — see $WORK/pod-$POD.json" ;; esac
done

# ── Build ─────────────────────────────────────────────────────────────────────
if [ "${DOCKET_PRILL_SKIP_BUILD:-0}" != "1" ]; then
  say "building the SPA and the tack binary with it embedded"
  (cd "$ROOT/frontend" && npm run build >"$WORK/build-frontend.log" 2>&1) || die "frontend build failed — see $WORK/build-frontend.log"
  (cd "$ROOT" && cargo build -p tack-cli --features embed-spa >"$WORK/build-tack.log" 2>&1) || die "cargo build failed — see $WORK/build-tack.log"
fi
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/debug/tack"
[ -x "$BIN" ] || die "$BIN is missing"

# ── The server ────────────────────────────────────────────────────────────────
DECISION_TOKEN=docket-prill-decision
API_TOKEN=docket-prill-api-token
NO_KEYCHAIN=unix:path=/nonexistent/tack-docket-prill-no-keychain
say "starting tack serve on :$PORT (work dir $WORK)"
(
  cd "$WORK/server"
  env PATH="$WORK/bin:$PATH" \
    HOME="$AGENT_HOME" \
    TACK_PORT="$PORT" \
    TACK_DATABASE_URL="sqlite:$WORK/tack.db?mode=rwc" \
    TACK_STORAGE_DIR="$WORK/storage" \
    TACK_LOG_LEVEL=info \
    TACK_EXECUTION_DECISION_TOKEN="$DECISION_TOKEN" \
    TACK_API_TOKEN="$API_TOKEN" \
    DBUS_SESSION_BUS_ADDRESS="$NO_KEYCHAIN" \
    "$BIN" serve >"$WORK/server.log" 2>&1 &
  echo $! >"$WORK/server.pid"
)
cleanup() {
  say "stopping"
  [ -f "$WORK/runner.pid" ] && kill "$(cat "$WORK/runner.pid")" 2>/dev/null || true
  [ -f "$WORK/docket.pid" ] && kill -TERM "$(cat "$WORK/docket.pid")" 2>/dev/null || true
  [ -f "$WORK/server.pid" ] && kill "$(cat "$WORK/server.pid")" 2>/dev/null || true
  echo "logs and state kept under $WORK"
}
trap cleanup EXIT
for _ in $(seq 1 120); do
  curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null || die "the server never answered — see $WORK/server.log"

# ── The runner's configuration: prill as verifier; docket is the daemon above ──
RUNNER_PATH="$WORK/bin:$SYSTEM_PATH"
cat > "$WORK/runner.toml" <<EOF
# Written by scripts/docket-prill-e2e.sh for one run of the docket + prill e2e.
[verify]
enabled = true
kind = "prill"
args = ["--image", "$PRILL_IMAGE_ID"]
timeout_seconds = 900

[git]
push_branches = false

[docket]
base_url = "$DOCKET_URL"
token_file = "$DOCKET_TOKEN_FILE"
EOF

# ── The spec: it enrols the runner on the Agents page and starts it with these ──
say "running the spec"
cd "$ROOT/frontend"
set +e
env E2E_API_ORIGIN="http://127.0.0.1:$PORT" \
  DOCKET_PRILL_SHOTS="$SHOTS" \
  DOCKET_PRILL_WORK="$WORK" \
  DOCKET_PRILL_TACK_BIN="$BIN" \
  DOCKET_PRILL_RUNNER_CONFIG="$WORK/runner.toml" \
  DOCKET_PRILL_RUNNER_STATE="$WORK/runner-state" \
  DOCKET_PRILL_RUNNER_PATH="$RUNNER_PATH" \
  DOCKET_PRILL_HOME="$AGENT_HOME" \
  DOCKET_PRILL_DECISION_TOKEN="$DECISION_TOKEN" \
  DOCKET_PRILL_API_TOKEN="$API_TOKEN" \
  DOCKET_PRILL_NO_KEYCHAIN="$NO_KEYCHAIN" \
  npx playwright test e2e/docket-prill.spec.ts --config playwright.docket-prill.config.ts --project=chromium --workers=1 "$@"
STATUS=$?
set -e
say "screenshots: $SHOTS"
ls -1 "$SHOTS" 2>/dev/null | sed 's/^/   /'
exit $STATUS
