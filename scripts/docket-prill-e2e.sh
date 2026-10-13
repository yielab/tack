#!/usr/bin/env bash
# Stands up everything `frontend/e2e/docket-prill.spec.ts` drives, then runs it:
#
#   - `tack` built from this checkout with the SPA embedded (`--features embed-spa`),
#     serving a fresh database on its own port, never the dev pair or `tack.toml`;
#   - `docket` from its source tree (DOCKET_SRC, default ~/Sites/rack-cli; it must speak
#     harness contract 1.2, docket 0.2.0-beta.5 or later) and `prill` from its source tree
#     (PRILL_SRC, default ~/Sites/prill), each behind a one-line launcher placed ahead of any
#     installed release on PATH;
#   - a scratch HOME for the server and the runner, in which docket's own setup is made
#     with docket's own command (`docket setup provider add local <endpoint>`), so docket
#     reaches the local model the way a person's docket does and Tack holds no endpoint;
#   - a runner configuration with prill as the verifier (`[verify]` with
#     `kind = "prill"`, the runner's own seam).
#
# The spec enrols that runner through the Agents page and starts it itself, so
# the whole workflow (project, settings, items, runs, packs, review) is one UI
# session. Every run is a real model call against DOCKET_PRILL_MODEL_BASE_URL, on the
# model docket's setup names as its default.
#
# Usage: scripts/docket-prill-e2e.sh [extra playwright args…]
# Knobs (environment):
#   DOCKET_PRILL_MODEL_BASE_URL  OpenAI-compatible endpoint, without /v1 (default http://127.0.0.1:8081)
#   PRILL_IMAGE                  prill's sandbox image, present locally (default prill-verify-py313:local);
#                                passed to prill by its image id
#   DOCKET_PRILL_WORK            working directory (default: a fresh mktemp dir, kept afterwards)
#   DOCKET_PRILL_PORT            the server's port (default 3377)
#   DOCKET_PRILL_SHOTS           where the screenshots go (default frontend/test-results/docket-prill)
#   DOCKET_PRILL_SKIP_BUILD=1    reuse target/debug/tack and frontend/dist as they are
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DOCKET_SRC=${DOCKET_SRC:-$HOME/Sites/rack-cli}
PRILL_SRC=${PRILL_SRC:-$HOME/Sites/prill}
MODEL_BASE_URL=${DOCKET_PRILL_MODEL_BASE_URL:-http://127.0.0.1:8081}
PRILL_IMAGE=${PRILL_IMAGE:-prill-verify-py313:local}
PORT=${DOCKET_PRILL_PORT:-3377}
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
PROBE=$(cd "$WORK" && env -i PATH="$SYSTEM_PATH" "$WORK/bin/docket" exec --workspace . --task probe --contract 1.2 2>/dev/null | tail -1 || true)
[ "$(jq -r '.v // empty' <<<"$PROBE" 2>/dev/null)" = "1.2.0" ] \
  || die "docket at $DOCKET_SRC does not speak harness contract 1.2 (docket 0.2.0-beta.5 or later) — point DOCKET_SRC at a checkout that does"

# ── docket's own setup, in the scratch HOME: the local endpoint as its provider ──
env -i HOME="$AGENT_HOME" PATH="$WORK/bin:$SYSTEM_PATH" \
  docket setup provider add local "$MODEL_BASE_URL/v1" --model local-model --ctx 32768 --max-tokens 4096 \
  </dev/null >"$WORK/docket-setup.log" 2>&1 || die "docket setup provider add failed — see $WORK/docket-setup.log"
READY=$(env -i HOME="$AGENT_HOME" PATH="$WORK/bin:$SYSTEM_PATH" docket setup harness --json || true)
[ "$(jq -r '.ready' <<<"$READY")" = "true" ] || die "docket's setup is not ready: $(jq -r '.reason' <<<"$READY")"
{
  echo "tack: $(git -C "$ROOT" rev-parse --short HEAD) ($(git -C "$ROOT" branch --show-current))$([ -n "$(git -C "$ROOT" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket: $("$WORK/bin/docket" --version 2>/dev/null | tail -1) from $DOCKET_SRC @ $(git -C "$DOCKET_SRC" rev-parse --short HEAD) ($(git -C "$DOCKET_SRC" branch --show-current))$([ -n "$(git -C "$DOCKET_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket installed release (not used): $(command -v docket || echo none) $(docket --version 2>/dev/null | tail -1)"
  echo "prill: $("$WORK/bin/prill" --version 2>/dev/null | tail -1) from $PRILL_SRC @ $(git -C "$PRILL_SRC" rev-parse --short HEAD) ($(git -C "$PRILL_SRC" branch --show-current))$([ -n "$(git -C "$PRILL_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "prill image: $PRILL_IMAGE $PRILL_IMAGE_ID"
  echo "docket setup: $(jq -r '.default_model' <<<"$READY") in $AGENT_HOME/.docket"
  echo "model endpoint: $MODEL_BASE_URL/v1 -> $(curl -fsS -m 5 "$MODEL_BASE_URL/v1/models" | python3 -c 'import json,sys; print(", ".join(m["id"] for m in json.load(sys.stdin).get("data", [])))')"
} | tee "$WORK/versions.txt"

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
  [ -f "$WORK/server.pid" ] && kill "$(cat "$WORK/server.pid")" 2>/dev/null || true
  echo "logs and state kept under $WORK"
}
trap cleanup EXIT
for _ in $(seq 1 120); do
  curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null || die "the server never answered — see $WORK/server.log"

# ── The runner's configuration: prill as verifier; docket brings its own model ──
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
