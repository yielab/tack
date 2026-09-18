# Claude Code — vendor findings

What `crates/tack-runner/src/harness/claude_code.rs` (`ClaudeCodeAdapter`) is built on.
Every claim below was checked once, by hand, against a real `claude` binary; the
fixtures in `2.1.223/`, `2.1.261/` and `2.1.273/` are the transcripts `claude_code/tests.rs`
parses to prove `parse_run_output`'s and `ClaudeCodeGrammar::signal`'s classification, kept
here instead of as inline string literals so a vendor-shape change is a diff to one file,
not a hunt through test bodies.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

## Fixture provenance

Each `<version>/*.jsonl` fixture has a sibling `*.jsonl.provenance` text file. Its first
line is `captured` (this is byte-for-byte, or trimmed only to the fields the parser
reads, from a real installed binary at the named version) or `constructed` (built by
hand because no real invocation produces this shape — a truncated stream, a
gateway-branch case not yet exercised live); everything after is why. A `captured`
fixture's directory name is the `claude_code_version` the transcript's own `init` line
reports; a `constructed` fixture is filed under whichever version's shape it imitates.

**Observed on:** `claude` version `2.1.223`, one machine, one point in time. A finding here
is "this is what that one installed copy actually did", never "this is how Claude Code
behaves on every machine or every version" — where a claim rests on something not
independently invoked (the Bedrock/Vertex/Foundry provider families, confirmed only by
`strings` against the binary, never an actual provider switch), that limit is called out
below rather than left implicit.

## Findings

- `claude --version` prints `"<version> (Claude Code)"` to stdout, exit 0, empty stderr, and
  needs neither `HOME` nor `PATH` — a fast, side-effect-free probe (`local_process::LocalProcessHarness::probe`).
- `claude -p` reads the prompt from **stdin** when no positional argument is given. The
  adapter always uses stdin, never argv, keeping the prompt out of `/proc/<pid>/cmdline`
  (matches `../process.rs`'s own documented preference).
- `--output-format json` (non-streaming) has **no reliable single "model used" field** —
  only an aggregate `modelUsage` map that, even for a single trivial prompt, included a
  second, unrequested internal model (`claude-haiku-4-5-...`) alongside the one actually
  requested. The adapter uses `--output-format stream-json --verbose` instead and reads the
  authoritative model from the `{"type":"system","subtype":"init"}` event's `model` field,
  cross-checked against `assistant` messages.
- `is_error` (boolean) is the only reliable success/failure signal. `subtype` is **not**: an
  invalid-model 404 was observed with `"subtype":"success"` alongside `"is_error":true`. The
  adapter keys exclusively off `is_error`.
- A persisted per-user settings file (`~/.claude/settings.json`, `"effortLevel"`) silently
  changed default behavior in a way that broke an otherwise-valid invocation (`effort
  'xhigh' is not supported when thinking is disabled`) even with the process environment
  fully cleared. The adapter always passes an explicit `--effort high` (verified compatible
  across every model exercised) and `--setting-sources ""` to reduce ambient configuration
  influence over a supposedly deterministic run.
- Claude Code's own Bash tool runs its command in a **new session** (distinct `pgid`/`sid`
  from the top-level `claude` process, confirmed twice via `ps`), unlike the shared fake
  harness's `spawn_child` mode. A graceful `SIGTERM` appeared to let Claude Code clean up
  that detached session itself, but a `SIGKILL` escalation cannot give it that chance, and
  `kill(-pgid, SIGKILL)` does not reach a different session's group — reflected as
  `cancel: Advisory`, never `Supported`, in `ClaudeCodeGrammar::capabilities`.

### Pausing to ask (measured against `claude` 2.1.273)

Checked with `claude` pointed at a loopback stand-in for the Messages API (a fake key, a
fresh `HOME`, zero spend), the fixtures in `2.1.273/` are that session's own
`control_request`/`control_response`/`result` lines:

- The flags that make it ask are `-p --input-format stream-json --output-format stream-json
  --verbose --permission-mode default --permission-prompt-tool stdio`, with the prompt as one
  stream-json user message on stdin and stdin left open. `--permission-prompts host` alone,
  without `--permission-prompt-tool`, does **not** ask — every question is denied on the spot,
  an easy flag to reach for that silently produces `bypassPermissions`-like behaviour.
- A question is one `control_request` stdout line
  (`ask-tool-permission-request.jsonl`); the answer is one `control_response` stdin line —
  `{"behavior":"allow"}` or `{"behavior":"deny","message":…}`
  (`ask-allow-response.jsonl`/`ask-deny-response.jsonl`). After *allow* the tool ran and the
  file existed; after *deny* it did not, the run went on, and its `result` line listed the
  refusal under `permission_denials`.
- With stdin open the CLI does **not** exit after its `result` line
  (`ask-result.jsonl`): it waits for another stdin message, and exits 0 only once stdin
  closes. `ClaudeCodeGrammar::signal` reads that line as `Finished` for exactly this reason —
  it is the core's own signal to close stdin, not the CLI's.

## What the adapter does not attempt

- Resolving `secret_reference`-only environment entries: no secret-store client exists in
  this crate yet. Such entries are skipped with a `tracing::warn!` (name only).
- Enumerating installed/available models ahead of a real invocation: the CLI has no
  `list-models`-style command, so the probe reports zero `model_combinations` rather than an
  unverified static alias list.
- Actually exercising the Bedrock/Vertex/Foundry provider paths: doing so needs real cloud
  credentials this adapter does not fabricate or request.
