# opencode — vendor findings

What `crates/tack-runner/src/harness/opencode.rs` (`OpencodeAdapter`) is built on. Every claim
below was checked by hand against the real installed binary, run with `HOME`,
`OPENCODE_CONFIG_DIR`, `AI_GATEWAY_API_KEY` and every `OPENCODE_DISABLE_*` variable set to a
fresh scratch value, pointed at a throwaway loopback server standing in for the model
endpoint — never a real model endpoint, never the real `~/.config/opencode`.

**Observed on:** `opencode 1.18.30`, one machine, one point in time.

## Fixture provenance

Each `1.18.30/*.ndjson` fixture has a sibling `*.ndjson.provenance` text file: its first line is
`captured` (from a real invocation, with the scratch directory it ran in rewritten to
`/capture`) or `constructed`. All three fixtures here are `captured` — `completed`, `tool_denied`
and `cut_short` were all reproduced against the real binary, so none needed constructing.

## Two measurements the adapter's design rests on

### 1. Does a fresh config directory reach the network?

**Yes — a non-loopback connection is attempted on every run, not only a first-run install.**

Command:
```
strace -f -e trace=network -o strace-net.log -- \
  opencode run --pure --format json --title tack -m tack/fake-model "write HELLO.txt saying hi"
```
run with `HOME`/`OPENCODE_CONFIG_DIR` under a fresh scratch directory, `OPENCODE_CONFIG_CONTENT`
naming only a loopback provider, and `OPENCODE_DISABLE_PROJECT_CONFIG`,
`OPENCODE_DISABLE_AUTOUPDATE`, `OPENCODE_DISABLE_MODELS_FETCH`, `OPENCODE_DISABLE_SHARE` all
`=1`. `grep 'connect(' strace-net.log | grep -v 127.0.0.1` shows a real, non-blocking TCP
`connect()` (not merely a source-address preference probe) to port 443 of a Cloudflare-anycast
address (`104.16.x.34`, and the IPv6 equivalent) on every invocation observed, whether or not the
run wrote a file or called a tool. Nothing in `--pure` or the four disable variables stops it,
and it is not particular to an empty config directory: a second run against an already-populated
one did the same thing.

The host is the npm registry. The DNS queries in the same trace are for
`registry.npmjs.org`, whose addresses those are (`getent ahostsv4 registry.npmjs.org`), and
after the run both `$HOME/.config/opencode` and `$OPENCODE_CONFIG_DIR` hold a `package.json`
depending on `@opencode-ai/plugin` at the CLI's own version, with its `node_modules` beside
it: 158 MB and 63 MB (`du -sh`). So a fresh home costs an install of about 220 MB from the
registry, and a populated one still asks the registry. The adapter gives every attempt a
fresh home, so every attempt pays it.

Because this adapter cannot make opencode keep a promise the CLI itself breaks,
`OpencodeGrammar::invocation` rejects any request whose `permission_policy.network` is `false`,
typed, before anything spawns. Pre-populating the config directory at probe time would not remove the connection: it was
observed on a populated directory too.

### 2. Is `{env:VAR}` in the injected config substituted?

**Yes.** With `OPENCODE_CONFIG_CONTENT` naming
`"apiKey": "{env:AI_GATEWAY_API_KEY}"` and the process environment carrying
`AI_GATEWAY_API_KEY=sk-fake-canary-measure-2`, the fake server's captured request carried
`Authorization: Bearer sk-fake-canary-measure-2` — the literal value, never the placeholder
string. The adapter references the resolved endpoint's own `credential_env_var` by name in the
config rather than a fixed variable, so it stays correct if a future provider uses a different
one. The value is still registered for redaction by the shared core regardless (every
`resolve_endpoint` credential is), since a vendor's own substitution behaviour is not something
this tree trusts blindly.

## Measured

- With no positional message, `opencode run` reads the prompt from stdin and completes once
  stdin closes; `LocalProcessHarness::prepare` always closes it after writing the prompt, so
  this needs nothing from the grammar.
- `opencode` populates `HOME` and `OPENCODE_CONFIG_DIR` when they do not exist (database, log,
  npm cache); nothing in the adapter creates them itself.
- The chat-completions request opencode sends has `"stream":true` — a non-streaming JSON
  response is not read as a completion at all: it triggers an immediate resend, hundreds of
  times a second, of the exact same request. The fake model server must answer
  `text/event-stream` chunks ending `data: [DONE]`, never a single JSON body.
- Every stdout line is one JSON event object (`step_start`, `text`, `step_finish`, `tool_use`,
  and presumably `error`, never observed here). Only `step_finish` (`part.reason`,
  `part.tokens.input`/`output`) and any `error` event carry what `report()` reads; every other
  line is real evidence of the run but not part of the adapter's contract, the same posture
  `docket.rs` takes toward its own progress events.
- A tool the injected `permission` block denies is not offered to the model at all; a model that
  tries to call it anyway (as the fake server was told to) gets back a
  `{"type":"tool_use","part":{"tool":"invalid",...,"error":"Model tried to call unavailable tool
  'write'. Available tools: ..."}}` event, the run's own exit code is 0, and — if the model's next
  turn answers normally — the final `step_finish.reason` is still `"stop"`. The denial is
  evidence in the transcript, not a run failure; `report()` never treats it as one on its own,
  matching ADR 0067 decision 5 (the exit code is never read to decide the verdict; here neither
  is a denial).
- `SIGTERM` mid-run, sent while the fake server was deliberately not answering: opencode exits
  143 and produces **zero bytes of stdout** — not even the `step_start` a normal turn opens
  with. `report()` reads that combination (`Exited(143)`, no terminal `step_finish`) as
  `cancelled`; any other exit with no terminal event is `malformed_output`.
- `opencode --version` prints a plain `1.18.30` — the existing `local_process::parse_version`
  reads it unchanged.

## Unverified — documented guesses, not facts

1. `cancel: Advisory`, never `Supported`: the bash tool's own child was observed (via `ps`,
   ADR 0067) to survive `SIGTERM` in its own process/session group; whether every tool a run
   starts behaves the same way is not separately confirmed here.
2. `resume`/`decisions: Unsupported`: no reattachment interface and no ask-the-operator event
   were observed; `--continue`/`--session` were read from `--help` text, not exercised against a
   real paused run.
3. `artifacts: Advisory`: only the staged stdout/stderr log is claimed; nothing in the event
   stream names a file opencode's own tools wrote, so no per-file artifact discovery is
   implemented.
4. Whether an `error` event type actually appears in a real transcript was not reproduced here;
   `report()` treats one as failing the run if it ever does, but the claim rests on ADR 0067's
   own risk section and the shape of `step_finish`/`tool_use`, not a captured `error` line.
5. `permission_policy: advisory`: `task`'s "grants sub-agents" gate is read from
   `permission_policy.tools` naming `task` by the same tool-list-membership convention
   `edit`/`bash` use — `PermissionPolicy` carries no dedicated sub-agent field, and no request
   naming `task` was exercised against a real opencode sub-agent run.
