# opencode — vendor findings

What `crates/tack-runner/src/harness/opencode.rs` (`OpencodeAdapter`) is built on. Every claim
below was checked by hand against the real installed binary, run with `HOME`,
`OPENCODE_CONFIG_DIR`, `AI_GATEWAY_API_KEY` and every `OPENCODE_DISABLE_*` variable set to a
fresh scratch value, pointed at a throwaway loopback server standing in for the model
endpoint — never a real model endpoint, never the real `~/.config/opencode`.

**Observed on:** `opencode 1.18.30`, one machine, one point in time.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

## Fixture provenance

Each `1.18.30/*.ndjson` fixture has a sibling `*.ndjson.provenance` text file: its first line is
`captured` (from a real invocation, with the scratch directory it ran in rewritten to
`/capture`) or `constructed`. All three fixtures here are `captured` — `completed`, `tool_denied`
and `cut_short` were all reproduced against the real binary, so none needed constructing.

A second measurement pass (served model, asking, shared install) added nine more, and a
re-check by the session that launched it added `adapter_shape_attempt.txt`; all
`captured`, all with a `.provenance` sibling in the same style: `served_model.ndjson` and
`served_model_export.json` (served-vs-requested model, see "Measured" below), `asking_run.ndjson`
+ `asking_run.stderr.txt` and `asking_acp.txt` (whether a permission ask pauses over each
interface), and `shared_install.du.txt` plus `shared_install_run1.strace.txt` /
`_run2.strace.txt` / `_run3.strace.txt` (three attempts, shared `npm_config_cache` across two of
them).

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
- **No served model anywhere.** With the fake model server answering every chunk's `model`
  field as `served-model-xyz` while the request names `tack/requested-model`, neither the
  `--format json` event stream (`served_model.ndjson`) nor `opencode export <sessionID>`
  (`served_model_export.json`) ever mentions `served-model-xyz` — both only ever name the
  *requested* model (`info.model`/`modelID`/`providerID` in the export, nothing at all in the
  event stream). `DESCRIPTOR.observes_served_model: false` has nothing left to read even in a
  surface the adapter doesn't currently parse.
- **`opencode run` never pauses for an ask; `opencode acp` does.** With `permission.bash` set
  to `"ask"` and no interactive terminal: `opencode run` auto-rejects in under a second with
  no question ever appearing on stdout (`asking_run.ndjson` — only the already-rejected
  `tool_use` event) — the one line that names the ask (`permission requested: bash (...);
  auto-rejecting`) is on stderr, not stdout (`asking_run.stderr.txt`), and nothing on stdin
  gets a chance to matter. `opencode acp --pure`, driven as a JSON-RPC client over its own
  stdio, sends a real `session/request_permission` *request* (carrying an `id` and the tool
  call's `optionId` choices) and leaves it genuinely pending — a parallel run left unanswered
  for 15s never proceeded — until a matching `{"id":...,"result":{"outcome":...}}` line is
  written to stdin, at which point the tool call actually runs and the turn completes
  (`asking_acp.txt`). The two interfaces disagree on `decisions` capability, not just on
  formatting.
- **A per-attempt network round-trip that a shared `npm_config_cache` does not avoid.** Three
  separate fresh-scratch-HOME `opencode run` attempts (none reusing another's `HOME`,
  `XDG_*`, or `BUN_INSTALL`) each made the identical `strace -f -e trace=connect` pattern:
  24 zero-port IPv4/IPv6 address-preference probes then 2 real, `EINPROGRESS` port-443
  connects to the same Cloudflare-anycast range this file's own "Does a fresh config
  directory reach the network?" measurement attributes to `registry.npmjs.org`
  (`shared_install_run1/2/3.strace.txt`) — including the two attempts whose `npm_config_cache`
  pointed at the first attempt's (already-used, still-warm) cache directory. Inspecting what
  landed there (two 240KB temp files, never promoted to a committed cache entry) showed a real
  npm-registry metadata response for `@opencode-ai/plugin`. None of the three attempts wrote a
  `node_modules` directory anywhere, and each stayed under 1MB (`shared_install.du.txt`) — a
  sharp contrast with an earlier same-session attempt (fresh scratch HOME, otherwise identical
  command) that needed a real ~220MB install first, matching this file's original "about 220MB
  from the registry" finding above. This run could not pin down why: the most likely
  explanation found along the way is a pre-existing, real (non-scratch) global package cache
  at `~/.bun/install/cache` on the measuring machine — already populated with
  `@ai-sdk/openai-compatible` from months before this session, unrelated to any of this
  session's own scratch directories — that the `opencode`/`bun` runtime appears to consult
  regardless of `HOME` or `BUN_INSTALL`. That candidate explanation is itself unverified: this
  measurement only confirms that whatever mechanism is responsible, sharing `npm_config_cache`
  alone did not make an attempt network-free, and an attempt-cost estimate of "~220MB, once,
  from a cold machine" should not be read as "~220MB every time" without re-checking on a
  machine known not to have this warm.
- **Re-checked the same day in the adapter's exact shape** (`adapter_shape_attempt.txt`): an
  untraced fresh-HOME attempt completed with no package install and under 1 MB on disk;
  three attempts under `strace` each installed `@opencode-ai/plugin` with **npm** (a
  `package-lock.json`, 63 MB of `node_modules` in each of the two config directories, 95 MB
  in `$HOME/.npm`, about 6 s) and then waited for nothing until their timeout without ever
  reaching the model. `BUN_INSTALL_CACHE_DIR` is not consulted. The install-then-hang is not
  explained; what holds for the adapter is that a completing attempt downloads nothing a
  cache could save, and the registry round trip happens regardless.

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
