#!/usr/bin/env bash
# Stands up everything `frontend/e2e/docket-prill.spec.ts` drives, then runs it:
#
#   - `tack` built from this checkout with the SPA embedded (`--features embed-spa`),
#     serving a fresh database on its own port, never the dev pair or `tack.toml`;
#   - `docket` from its source tree (DOCKET_SRC, default ~/Sites/rack-cli) and
#     `prill` from its source tree (PRILL_SRC, default ~/Sites/prill), each behind a
#     one-line launcher placed ahead of any installed release on PATH;
#   - a runner configuration with prill as the verifier (`[verify]`, through
#     `scripts/prill-tack-verify.py`) and docket pointed at a local OpenAI-compatible
#     model endpoint through the gateway provider's loopback override — the same
#     test-only knob `scripts/smoke.sh` uses, because docket always needs a
#     Tack-configured endpoint and nothing else on this machine stands in for one.
#
# The spec enrols that runner through the Agents page and starts it itself, so
# the whole workflow (project, settings, items, runs, packs, review) is one UI
# session. Every run is a real model call against DOCKET_PRILL_MODEL_BASE_URL.
#
# Usage: scripts/docket-prill-e2e.sh [extra playwright args…]
# Knobs (environment):
#   DOCKET_PRILL_MODEL_BASE_URL  OpenAI-compatible endpoint, without /v1 (default http://127.0.0.1:8081)
#   PRILL_IMAGE                  prill's sandbox image, present locally (default prill-verify-py313:local)
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
curl -fsS -m 5 "$MODEL_BASE_URL/v1/models" >/dev/null || die "no OpenAI-compatible endpoint answers at $MODEL_BASE_URL/v1/models"
command -v ffmpeg >/dev/null 2>&1 || echo "note: ffmpeg not found; the spec records no GIF without it"
if ss -ltn 2>/dev/null | awk '{print $4}' | grep -q ":$PORT\$"; then
  die "port $PORT is already taken — set DOCKET_PRILL_PORT"
fi

mkdir -p "$WORK/bin" "$WORK/server" "$WORK/runner-state" "$SHOTS"
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
{
  echo "tack: $(git -C "$ROOT" rev-parse --short HEAD) ($(git -C "$ROOT" branch --show-current))$([ -n "$(git -C "$ROOT" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket: $("$WORK/bin/docket" --version 2>/dev/null | tail -1) from $DOCKET_SRC @ $(git -C "$DOCKET_SRC" rev-parse --short HEAD) ($(git -C "$DOCKET_SRC" branch --show-current))$([ -n "$(git -C "$DOCKET_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "docket installed release (not used): $(command -v docket || echo none) $(docket --version 2>/dev/null | tail -1)"
  echo "prill: $("$WORK/bin/prill" --version 2>/dev/null | tail -1) from $PRILL_SRC @ $(git -C "$PRILL_SRC" rev-parse --short HEAD) ($(git -C "$PRILL_SRC" branch --show-current))$([ -n "$(git -C "$PRILL_SRC" status --porcelain)" ] && echo ' +uncommitted')"
  echo "prill image: $PRILL_IMAGE $(docker image inspect "$PRILL_IMAGE" --format '{{.Id}}')"
  echo "model endpoint: $MODEL_BASE_URL/v1 -> $(curl -fsS -m 5 "$MODEL_BASE_URL/v1/models" | python3 -c 'import json,sys; print(", ".join(m["id"] for m in json.load(sys.stdin).get("data", [])))')"
} | tee "$WORK/versions.txt"

# ── Build ─────────────────────────────────────────────────────────────────────
if [ "${DOCKET_PRILL_SKIP_BUILD:-0}" != "1" ]; then
  say "building the SPA and the tack binary with it embedded"
  (cd "$ROOT/frontend" && npm run build >"$WORK/build-frontend.log" 2>&1) || die "frontend build failed — see $WORK/build-frontend.log"
  (cd "$ROOT" && cargo build -p tack-cli --features embed-spa >"$WORK/build-tack.log" 2>&1) || die "cargo build failed — see $WORK/build-tack.log"
fi
BIN="$ROOT/target/debug/tack"
[ -x "$BIN" ] || die "$BIN is missing"

# ── The server ────────────────────────────────────────────────────────────────
DECISION_TOKEN=docket-prill-decision
NO_KEYCHAIN=unix:path=/nonexistent/tack-docket-prill-no-keychain
say "starting tack serve on :$PORT (work dir $WORK)"
(
  cd "$WORK/server"
  env PATH="$WORK/bin:$PATH" \
    TACK_PORT="$PORT" \
    TACK_DATABASE_URL="sqlite:$WORK/tack.db?mode=rwc" \
    TACK_STORAGE_DIR="$WORK/storage" \
    TACK_LOG_LEVEL=info \
    TACK_EXECUTION_DECISION_TOKEN="$DECISION_TOKEN" \
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

# ── The runner's configuration: prill as verifier, docket through the loopback endpoint ──
RUNNER_PATH="$WORK/bin:$SYSTEM_PATH"
TACK_RUNNER_SECRET_VALUE=local-endpoint-needs-no-key \
  DBUS_SESSION_BUS_ADDRESS="$NO_KEYCHAIN" \
  "$BIN" runner secret set vercel-ai-gateway/default --state-dir "$WORK/runner-state" >/dev/null
cat > "$WORK/runner.toml" <<EOF
# Written by scripts/docket-prill-e2e.sh for one run of the docket + prill e2e.
[provider.vercel_ai_gateway]
enabled = true
secret = "vercel-ai-gateway/default"

[verify]
enabled = true
program = "$ROOT/scripts/prill-tack-verify.py"
args = ["--image", "$PRILL_IMAGE"]
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
  DOCKET_PRILL_MODEL_BASE_URL="$MODEL_BASE_URL" \
  DOCKET_PRILL_DECISION_TOKEN="$DECISION_TOKEN" \
  DOCKET_PRILL_NO_KEYCHAIN="$NO_KEYCHAIN" \
  npx playwright test e2e/docket-prill.spec.ts --config playwright.docket-prill.config.ts --project=chromium --workers=1 "$@"
STATUS=$?
set -e
say "screenshots: $SHOTS"
ls -1 "$SHOTS" 2>/dev/null | sed 's/^/   /'
exit $STATUS
