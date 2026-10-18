#!/usr/bin/env bash
# Measures what Claude Code and opencode report about their tokens while they run, and what
# Claude Code sends the model, into crates/tack-runner/src/harness/fixtures/{claude_code,opencode}/.
# Each fixture has a `.provenance` beside it saying what it shows.
#
# Without --paid nothing is spent: both programs talk to a loopback stand-in for their model API,
# with a fake key, in a fresh HOME under /var/tmp, never the operator's ~/.claude or opencode
# config. It writes claude_code/<version>/request-shape.json (which CLAUDE.md files, memory and
# reminders reach the request, and what each tool's definition weighs) and
# opencode/<version>/cache_read.ndjson (where opencode puts cache reads).
#
# --paid also runs four short billed calls on the operator's own Claude Code login (Sonnet,
# read-only tools, permission mode default, in a fresh folder): live-usage.jsonl and the three
# tools-*-result.jsonl. It needs the maintainer's word; Tack never runs it.
#
# Usage: scripts/capture-agent-usage-fixtures.sh [--paid] [--keep]
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
FX=$ROOT/crates/tack-runner/src/harness/fixtures
PAID=0; KEEP=0
for arg in "$@"; do case "$arg" in --paid) PAID=1 ;; --keep) KEEP=1 ;; *) echo "usage: $0 [--paid] [--keep]" >&2; exit 2 ;; esac; done
die() { printf 'capture-agent-usage-fixtures: %s\n' "$*" >&2; exit 1; }
for tool in claude opencode python3 git; do command -v "$tool" >/dev/null || die "$tool is needed"; done

SCRATCH=$(mktemp -d /var/tmp/agent-usage-capture-XXXXXX)
PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
  [ "$KEEP" = 1 ] || case "$SCRATCH" in /var/tmp/agent-usage-capture-*) rm -rf "$SCRATCH" ;; esac; }
trap cleanup EXIT
SYSTEM_PATH=/usr/bin:/bin
CLAUDE_DIR=$(dirname "$(command -v claude)"); OPENCODE_DIR=$(dirname "$(command -v opencode)")
CC_VERSION=$(claude --version | cut -d' ' -f1); OC_VERSION=$(opencode --version)
free_port() { python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
new_repo() { mkdir -p "$1"; (cd "$1" && git init -q && for f in a b c; do
  python3 -c "print('\n'.join(f'$f line {i}: the quick brown fox jumps over the lazy dog' for i in range(1, 41)))" > $f.txt; done
  git add . && git -c user.email=capture@invalid -c user.name=capture commit -qm init); }
# Every line of stdout, without the scratch folder's name.
scrub() { sed "s#$SCRATCH#/capture#g"; }

cat > "$SCRATCH/fake_messages.py" <<'PY'
"""Loopback stand-in for the Messages API: records every request body, answers one short text turn."""
import json, sys, os, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
out = sys.argv[2]; os.makedirs(out, exist_ok=True)
lock = threading.Lock(); n = [0]
def sse(ev, data): return f"event: {ev}\ndata: {json.dumps(data)}\n\n".encode()
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        with open(os.path.join(out, "paths.log"), "a") as f: f.write(f"GET {self.path}\n")
        self.send_response(404); self.end_headers()
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        with lock:
            n[0] += 1; i = n[0]
        with open(os.path.join(out, "paths.log"), "a") as f: f.write(f"POST {self.path} -> {i}\n")
        try: req = json.loads(body)
        except Exception: req = {"raw": body.decode(errors="replace")}
        with open(os.path.join(out, f"req-{i:02d}.json"), "w") as f: json.dump(req, f, indent=1)
        if "count_tokens" in self.path:
            data = json.dumps({"input_tokens": 1}).encode()
            self.send_response(200); self.send_header("content-type","application/json"); self.end_headers(); self.wfile.write(data); return
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        m = req.get("model", "fake")
        self.wfile.write(sse("message_start", {"type":"message_start","message":{"id":f"msg_{i}","type":"message","role":"assistant","model":m,"content":[],"stop_reason":None,"usage":{"input_tokens":10,"output_tokens":1}}}))
        self.wfile.write(sse("content_block_start", {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}))
        self.wfile.write(sse("content_block_delta", {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}))
        self.wfile.write(sse("content_block_stop", {"type":"content_block_stop","index":0}))
        self.wfile.write(sse("message_delta", {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}))
        self.wfile.write(sse("message_stop", {"type":"message_stop"}))
ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
cat > "$SCRATCH/fake_openai.py" <<'PY'
"""Loopback chat-completions stand-in: first tool turn calls `read`, then a final answer; usage reports cache reads."""
import json, sys, os, time, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
out = sys.argv[2]; os.makedirs(out, exist_ok=True); lock = threading.Lock(); n = [0]; turn = [0]
def chunk(d): return f"data: {json.dumps(d)}\n\n".encode()
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self): self.send_response(404); self.end_headers()
    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
        with lock: n[0] += 1; i = n[0]
        json.dump(req, open(os.path.join(out, f"req-{i:02d}.json"), "w"), indent=1)
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        base = {"id": f"c{i}", "object": "chat.completion.chunk", "created": 0, "model": "fake"}
        if req.get("tools"):
            with lock: turn[0] += 1; t = turn[0]
            time.sleep(1.5)
            if t == 1:
                delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": "read", "arguments": json.dumps({"filePath": "a.txt"})}}]}
                fin = "tool_calls"
            else:
                delta = {"role": "assistant", "content": "a.txt has 40 lines."}; fin = "stop"
        else:
            delta = {"role": "assistant", "content": "Title"}; fin = "stop"
        self.wfile.write(chunk({**base, "choices": [{"index": 0, "delta": delta, "finish_reason": None}]}))
        self.wfile.write(chunk({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": fin}]}))
        self.wfile.write(chunk({**base, "choices": [], "usage": {"prompt_tokens": 1000 + i, "completion_tokens": 7, "total_tokens": 1007 + i, "prompt_tokens_details": {"cached_tokens": 800}}}))
        self.wfile.write(b"data: [DONE]\n\n")
ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY

# --- Claude Code: what reaches the request (free) ---------------------------------------------
CH=$SCRATCH/cc/home; CP=$SCRATCH/cc/project; new_repo "$CP"; mkdir -p "$CH/.claude"
echo "Always mention CANARY_USER_CLAUDE_MD." > "$CH/.claude/CLAUDE.md"
echo "Always mention CANARY_PROJECT_CLAUDE_MD." > "$CP/CLAUDE.md"
echo "Always mention CANARY_LOCAL_CLAUDE_MD." > "$CP/CLAUDE.local.md"
MEM=$CH/.claude/projects/$(echo "$CP" | sed 's#[/.]#-#g')/memory; mkdir -p "$MEM"
echo "- [Canary](canary.md) — CANARY_MEMORY_INDEX" > "$MEM/MEMORY.md"
printf -- '---\nname: canary\n---\nCANARY_MEMORY_FILE\n' > "$MEM/canary.md"
PORT=$(free_port); python3 -I "$SCRATCH/fake_messages.py" "$PORT" "$SCRATCH/cc/reqs" & PIDS+=($!); sleep 0.5
CLAUDE_FLAGS=(-p --output-format stream-json --verbose --no-session-persistence --permission-mode default --strict-mcp-config --effort high)
fake_claude() { local label=$1; shift; local extra=(); while [ "$1" != -- ]; do extra+=("$1"); shift; done; shift
  local before; before=$(find "$SCRATCH/cc/reqs" -name 'req-*' 2>/dev/null | wc -l)
  (cd "$CP" && echo "Reply with the single word ok." | timeout 90 env -i HOME="$CH" PATH="$CLAUDE_DIR:$SYSTEM_PATH" \
    ANTHROPIC_BASE_URL="http://127.0.0.1:$PORT" ANTHROPIC_API_KEY=sk-fake-capture ENABLE_CLAUDEAI_MCP_SERVERS=false "${extra[@]}" \
    claude "${CLAUDE_FLAGS[@]}" "$@" > /dev/null)
  cp "$SCRATCH/cc/reqs/req-$(printf %02d $((before + 1))).json" "$SCRATCH/cc/$label.json"; }
fake_claude A -- --setting-sources "" --tools default
fake_claude B CLAUDE_CODE_DISABLE_AUTO_MEMORY=1 -- --setting-sources "" --tools default
fake_claude C -- --setting-sources "" --settings '{"autoMemoryEnabled":false,"includeCoAuthoredBy":false,"includeGitInstructions":false}' --tools default
fake_claude D -- --setting-sources "" --tools "Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch"
fake_claude E -- --setting-sources "" --tools ""
mkdir -p "$FX/claude_code/$CC_VERSION"
python3 -I - "$SCRATCH/cc" "$FX/claude_code/$CC_VERSION/request-shape.json" <<'PY'
import json, sys, os
src = sys.argv[1]
flags = {"A": "--setting-sources '' --tools default (Tack today)", "B": "A + CLAUDE_CODE_DISABLE_AUTO_MEMORY=1",
         "C": "A + --settings '{\"autoMemoryEnabled\":false,\"includeCoAuthoredBy\":false,\"includeGitInstructions\":false}'",
         "D": "--setting-sources '' --tools Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch", "E": "--setting-sources '' --tools ''"}
canaries = ["CANARY_USER_CLAUDE_MD", "CANARY_PROJECT_CLAUDE_MD", "CANARY_LOCAL_CLAUDE_MD", "CANARY_MEMORY_INDEX", "CANARY_MEMORY_FILE"]
out = {}
for key in flags:
    r = json.load(open(os.path.join(src, key + ".json"))); whole = json.dumps(r)
    out[key] = {"flags": flags[key], "request_bytes": len(whole), "system_bytes": len(json.dumps(r.get("system"))),
                "tools": {t["name"]: len(json.dumps(t)) for t in r.get("tools", [])},
                "found": {c: c in whole for c in canaries},
                "attribution_reminder": "Co-Authored-By" in whole, "git_status_reminder": "gitStatus" in whole}
json.dump(out, open(sys.argv[2], "w"), indent=1)
PY

# --- opencode: where cache reads land (free) --------------------------------------------------
OP=$SCRATCH/oc/project; new_repo "$OP"
PORT=$(free_port); python3 -I "$SCRATCH/fake_openai.py" "$PORT" "$SCRATCH/oc/reqs" & PIDS+=($!); sleep 0.5
OC_CONFIG=$(python3 -c "import json; print(json.dumps({'provider': {'tack': {'npm': '@ai-sdk/openai-compatible', 'name': 'tack', 'options': {'baseURL': 'http://127.0.0.1:$PORT/v1', 'apiKey': 'sk-fake'}, 'models': {'fake-model': {'name': 'fake'}}}}, 'small_model': 'tack/fake-model', 'permission': {'webfetch': 'allow', 'task': 'deny', 'edit': 'deny', 'bash': 'deny'}}))")
mkdir -p "$FX/opencode/$OC_VERSION"
(cd "$OP" && echo "Read a.txt and say how many lines it has." | timeout 120 env -i PATH="$OPENCODE_DIR:$SYSTEM_PATH" HOME="$SCRATCH/oc/home" \
  OPENCODE_CONFIG_DIR="$SCRATCH/oc/config" OPENCODE_DISABLE_PROJECT_CONFIG=1 OPENCODE_DISABLE_AUTOUPDATE=1 OPENCODE_DISABLE_MODELS_FETCH=1 \
  OPENCODE_DISABLE_SHARE=1 OPENCODE_CONFIG_CONTENT="$OC_CONFIG" opencode run --pure --format json --title tack -m tack/fake-model) \
  | scrub > "$FX/opencode/$OC_VERSION/cache_read.ndjson"

# --- Claude Code: live usage and each tool list's weight (billed) -----------------------------
if [ "$PAID" = 1 ]; then
  RP=$SCRATCH/paid/project; new_repo "$RP"
  paid_claude() { (cd "$RP" && echo "$1" | timeout 300 env -i HOME="$HOME" PATH="$CLAUDE_DIR:$SYSTEM_PATH" ENABLE_CLAUDEAI_MCP_SERVERS=false \
    CLAUDE_CODE_DISABLE_AUTO_MEMORY=1 claude "${CLAUDE_FLAGS[@]}" --setting-sources "" --settings '{"includeCoAuthoredBy":false}' \
    --tools "$2" --model sonnet) | scrub; }
  OUT=$FX/claude_code/$CC_VERSION
  paid_claude "Use the Read tool on a.txt, then on b.txt, then on c.txt, one call at a time and in that order. Then reply with one sentence saying how many lines each file has." \
    "Read,Glob,Grep" > "$OUT/live-usage.jsonl"
  paid_claude "Reply with the single word ok." default | tail -n 1 > "$OUT/tools-default-result.jsonl"
  paid_claude "Reply with the single word ok." "Bash,Edit,Glob,Grep,Read,WebFetch,WebSearch,Write" | tail -n 1 > "$OUT/tools-eight-result.jsonl"
  paid_claude "Reply with the single word ok." "" | tail -n 1 > "$OUT/tools-none-result.jsonl"
fi
echo "==> claude $CC_VERSION, opencode $OC_VERSION; fixtures in $FX"
