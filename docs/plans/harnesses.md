# Plan: four harnesses on one core

Implements ADR 0066 (docket as a third harness) and ADR 0067 (opencode readmitted as a
fourth), both accepted, under the rules of ADR 0068. It is Stage 3 of Phase 64; the order of
every stage, and how a task is handed to an agent, is in `docs/plans/phase-64.md`. The audit
that first proposed the shape is archived at
`docs/closed-cycles/plans/harness-maintainability-audit.md`.

Every number below carries its command. Re-run it before quoting it.

## Where things stand (measured 2026-09-18)

**The core is in place and both shipped harnesses are on it.** A harness is a
`HarnessDescriptor` (data) plus a four-method `HarnessGrammar`; everything else is
`crates/tack-runner/src/harness/local_process.rs`, once.

`wc -l crates/tack-runner/src/harness/{local_process,codex,claude_code,mod}.rs`, and the
same files at `develop` before this change:

| File | Before | Now |
|---|---|---|
| `local_process.rs` (the shared lifecycle) | 531 | 895 |
| `codex.rs` | 886 | 189 |
| `claude_code.rs` | 1 141 | 305 |
| `mod.rs` (registry, descriptors, discovery) | 329 | 417 |
| Harness unit tests, all files | 3 240 | 1 554 |

What a harness no longer writes: locating its binary, the `--version` probe and its
parser, the kind and model-selection checks, environment and `secret_reference`
resolution, provider-endpoint resolution and credential injection, prompt delivery,
timeouts and capture caps, the handle format, cancellation, reconciliation with a pid
identity check, log staging, and the assembly of `ActualExecution` and `Usage`.

**All four CLIs are installed on this machine** (`command -v docket opencode codex claude`),
so every proof below runs here with zero spend.

### docket, measured against the installed binary

`docket --version` prints `docket 0.2.0b1`; harness mode shipped in the repository as
`0.2.0-beta.2` (commits `01d6c67`, `0d7f4cc` in `../rack-cli`). Each row was seen by running
`docket harness run` under `env -i` against a throwaway `/v1/chat/completions` server on
loopback, with `DOCKET_HOME` in a scratch directory:

| Fact | What it means for Tack |
|---|---|
| `--task-file /dev/stdin` reads the prompt from stdin | the core's stdin delivery works unchanged; no prompt file, nothing on argv |
| docket creates a `DOCKET_HOME` that does not exist | the grammar only names the path; the core creates nothing |
| It runs with an empty environment | `inherited_env` is a choice, not a need; `PATH` is inherited so its `bash` tool finds the user's toolchain |
| `--model P/ID` splits on the first `/` and sends `ID` upstream (`gw/anthropic/claude-x` → `anthropic/claude-x`) | Tack passes `<requested provider>/<requested model id>` |
| It calls `<DOCKET_LLM_BASE_URL>/chat/completions` with `Authorization: Bearer <DOCKET_LLM_API_KEY>` | the base URL carries `/v1`; the key variable is docket's, not the provider's |
| The result line carries `model.served` from the endpoint's response body, and real token counts | the served model is an observation, even behind a gateway |
| `docket` is a bash launcher that `exec`s a Python interpreter, so `argv[0]` is Python | the core's pid identity check, which compares `argv[0]`, would call a live docket dead; the attempt id on argv (`--agent-id`) is what identifies it |
| `--timeout` defaults to 300 s | Tack always passes the request's timeout |
| `docket harness status TOKEN` answers `finished` / `unknown`, but the token is only on stdout | not used: the core cannot journal a token it has not read yet, and the argv check above is enough |
| `docket --version` is not a plain `X.Y.Z` | the core's version parser reports it as unrecognized today; fixed in H2 |

What ADR 0066 described and did not ship — a stdin flag, `--attempt`, `docket harness
capabilities`, child process-group events — is not needed for any of the above.

### opencode, measured against `opencode 1.18.30`

ADR 0067's table stands. Two rows were added today, same method:

| Fact | What it means for Tack |
|---|---|
| With no positional message, `opencode run` takes the prompt from stdin and completes once stdin closes | the core's stdin delivery works unchanged. ADR 0067's "open stdin hangs" is the same fact seen from the other side: Tack always closes it |
| It creates `HOME` and `OPENCODE_CONFIG_DIR` when they do not exist, and fills them with 38 MB including an `.npm` cache (`du -sh`) | the grammar only names the paths; whether that first run reaches the network is still unmeasured and is H3's first step |

## The shape

```
HarnessDescriptor   kind · program · wire · model selection · native provider ·
  (data)            inherited env · capture floor · pass-through reason · probe notes ·
                    credential note

HarnessGrammar      descriptor()    the data above
  (four methods)    capabilities()  what this CLI honestly supports, permission_policy included
                    invocation()    request + resolved endpoint -> args and extra env, or a typed rejection
                    report()        finished process -> verdict, evidence, observed model, tokens, cost

LocalProcessHarness everything else, identical for every harness
```

Adding a harness is one module, one line in `harness::DESCRIPTORS` and one in
`harness::discover`. `provider::wire_for_harness`, the model catalog's eligibility and
`tack runner doctor` all read the descriptor; none of them is edited.

## Rules every harness is held to

1. **The lifecycle is tested once**, in `local_process/tests.rs`, through a grammar that
   adds nothing, against `fixtures/fake_harness.sh`. A harness's own tests are pure: a
   request in, a command line out; a captured transcript in, a report out.
2. **One test per open-wire harness runs the real binary**, against a fake model server on
   loopback: it proves the vendor's contract, which no fake can. It is not billed, runs in
   the ordinary suite, and returns early with a message when the binary is absent.
3. **Vendor output is a file**, under `fixtures/<kind>/<version>/`, and the directory's
   README says per file whether it was captured or constructed. Findings about a vendor go
   in that README, not in a module preamble.
4. **A rule that binds requests lives in the core**, so it binds every harness at once. A
   grammar only adds what is genuinely its CLI's: the flags, and the requests its CLI
   cannot honour.
5. **The permission policy is declared per harness.** `capabilities()` carries a
   `permission_policy` entry saying how far the tool list, the network flag and the
   budgets are enforced. A request a harness cannot honour at all is rejected from
   `invocation()` before anything spawns. Today: claude-code `advisory`, codex `unsupported`.
6. **Capabilities are claimed from proof for this adapter**, never from what the CLI can do
   in general. Each upgrade to `Supported` is its own change with its own test.
7. **A variant is added with its first caller.** `RunContext`, `Invocation` and the
   descriptor grow a field in the change that needs it, not before.
8. **Anything that runs a billed binary lives under `tests/live/` and is `#[ignore]`d.**

## The tasks

Four tasks, in order. H1 is independent of everything; H2 needs H1; H3 needs H2 (both edit
`DESCRIPTORS` and share the wire); H4 needs H3. Each is one branch from `develop`, and each
lists every file it may touch — a change outside that list is a finding to report, not to
make.

### H1 — two fixes in the core that every stream harness needs

**Files:** `harness/process.rs` and its tests; `harness/local_process.rs` and
`harness/local_process/tests.rs`.

1. **Capture keeps the head and the tail of a stream.** Today `capture_bounded` keeps the
   first `cap` bytes, so a transcript over the cap loses its last line — the one that
   carries the result for claude-code, docket and opencode. Keep the first `cap / 2` bytes
   and the last `cap - cap / 2` (a `VecDeque<u8>` that drops from the front). A stream at or
   under the cap is returned whole and byte-identical to today. Over the cap,
   `finalize_capture` joins head and tail with one line, `[… <n> bytes dropped …]`;
   `truncated`, `bytes_dropped` and `total_bytes_seen` keep their meaning. Check that
   `claude_code::parse_run_output` skips a line that is not JSON, since the cut can land
   mid-line; if it does not, make it, with one fixture row.
   *Test:* the existing truncation test becomes one table — under the cap, exactly at it,
   over it — asserting the text starts with the head, ends with the stream's last line, and
   the three counters.
   *Not in this task:* removing `min_capture_bytes`. It becomes unnecessary only once a
   captured long transcript proves it.
2. **A live pid is this attempt's process if `argv[0]` is the program *or* any argument is
   the attempt id.** `process_is_this_harness` takes the journal's `attempt_id`. A CLI that
   is a launcher script (docket) never has the program as `argv[0]`.
   *Test:* one more row in `reconcile_tells_this_harness_from_a_reused_pid`: a child whose
   `argv[0]` is a different program but whose arguments carry the attempt id reconciles as
   `ProcessRunning`. Trap: `sh -c 'sleep 30'` `exec`s `sleep` and loses its arguments; keep
   the shell alive (`sh -c 'sleep 30; :' <attempt id>`). Revert the change once and watch the
   row fail.

**Done when:** `.githooks/pre-push` passes; no new test function was added, only rows.

### H2 — docket

**Files:** `provider/mod.rs`, `provider/vercel_ai_gateway.rs`, `provider/anthropic.rs`,
`provider/tests.rs`; `harness/local_process.rs` and its tests; `harness/codex.rs` and
`harness/claude_code.rs` (one descriptor line each); new `harness/docket.rs`,
`harness/docket/tests.rs`, `harness/fixtures/docket/<version>/`,
`harness/fixtures/docket/README.md`; `harness/mod.rs`; `crates/tack-runner/Cargo.toml`
(`wiremock = { workspace = true }` under dev-dependencies).

**Core changes, each with docket as its first caller:**

| Change | Where | Why |
|---|---|---|
| `Wire::OpenAiChatCompletions`. Vercel answers `https://ai-gateway.vercel.sh/v1` with `AI_GATEWAY_API_KEY`; its loopback test override gains the `/v1` arm; Anthropic answers `None` until measured | the three provider files | the wire docket and opencode speak |
| `HarnessDescriptor::credential_env: Option<&'static str>` — the variable this CLI reads its key from; `None` keeps the provider's own name. One line in `prepare` | `local_process.rs` | docket reads only `DOCKET_LLM_API_KEY` |
| `HarnessDescriptor::observes_served_model: bool` — the model this CLI reports comes from the endpoint's response, not from its own configuration. When true, `outcome()` records `harness_reported` even behind a gateway | `local_process.rs` | docket's `model.served` is a real observation; the gateway downgrade exists for CLIs that echo their request |
| `parse_version` accepts a later token that starts `X.Y` and continues with letters, digits, `.`, `-` or `+` | `local_process.rs` | `docket 0.2.0b1` |

**`harness/docket.rs`** — target 200 lines, in the range of `codex.rs`:

- *Descriptor:* kind and program `docket`; `Wire::OpenAiChatCompletions`;
  `ModelSelection::Explicit` (docket refuses to run without `--model`); `inherited_env:
  &["PATH"]`; `min_capture_bytes: (0, 0)`; `credential_env: Some("DOCKET_LLM_API_KEY")`;
  `observes_served_model: true`.
- *`invocation()`:* rejects, typed, a request with no resolved endpoint and no
  `DOCKET_LLM_BASE_URL` in its own `environment` — docket would refuse it anyway, after a
  spawn. Args: `harness run --workspace <workspace root> --task-file /dev/stdin --model
  <provider>/<model id> --agent-id <attempt id> --timeout <request timeout>`. Env:
  `DOCKET_HOME=<workspace root>/.tack-runner/docket-home`, and `DOCKET_LLM_BASE_URL` from
  the endpoint when there is one. Never `--task`: it would put the prompt on argv.
- *`report()`:* the last non-empty stdout line, through one `serde` struct (`status`,
  `stop_reason`, `error`, `blocked`, `model.served`, `usage.input_tokens`,
  `usage.output_tokens`, `token`). `ok` succeeds; `failed`, `blocked`, `cancelled` and
  `refused` fail. `terminal_reason` keeps `status` as the code, `error` as the message, and
  `stop_reason`, `blocked` and `token` whole. An empty `served` is `None`. Tokens are
  measured; `cost_usd` is always `None`. No parseable result line is a failed run with code
  `malformed_output` and the exit status as evidence. The exit code never decides.
- *`capabilities()`:* `cancel` advisory (docket stops cooperatively on `SIGTERM`, but does
  not report the process groups its tools start); `resume` and `decisions` unsupported
  (harness mode refuses approvals rather than pausing); `artifacts` advisory (the log
  only); `usage` as claude-code words a partial measurement — tokens yes, cost never;
  `permission_policy` unsupported (docket applies its own per-tool policy engine; the
  request's tool list, network flag and budgets are not passed to it).

**Fixtures.** First capture, with a throwaway script outside the tree: a loopback server
that answers one `write` tool call and then a final message, and logs the `tools` array of
the request so the tool's argument names are known rather than guessed. Keep `ok.ndjson`
(captured) and `refused.ndjson` (captured, by leaving `DOCKET_HOME` unset); write
`blocked.ndjson` and `cancelled.ndjson` by hand from `../rack-cli/docs/contracts/harness-v1/
schema.json`, marked *constructed* in the README. Nothing is copied from `../rack-cli`, and
nothing there is edited.

**Tests — five functions, no more:**

1. `a_request_becomes_a_harness_run_command` — a table: with an endpoint; with the base URL
   in the request's own environment.
2. `a_request_with_no_model_endpoint_is_refused`.
3. `a_result_line_becomes_a_report` — a table over the four fixtures: verdict, served
   model, tokens, cost unmeasured, `blocked` kept.
4. `output_without_a_result_line_is_a_failed_run`.
5. `the_real_docket_edits_a_file_against_a_fake_model_server` — `wiremock` on loopback, the
   Vercel provider pointed at it through its existing loopback override, a stored gateway
   key. Asserts the file the tool wrote, `harness_reported` with the served id the server
   answered, measured tokens, the `Authorization` header the server received, and that the
   key is absent from the staged log. Returns early when `docket` is not on `PATH`.

The core's existing table tests gain rows, not functions: the version table
(`docket 0.2.0b1`), the model-source table (`observes_served_model`), the provider table (the
third wire), and one assertion for `credential_env` in the test that already checks what
reaches the child.

**Done when:** `tack runner doctor` lists `docket` with the gateway's catalog under it;
`wc -l harness/docket.rs` is under 250; `.githooks/pre-push` passes.

### H3 — opencode

**Files:** new `harness/opencode.rs`, `harness/opencode/tests.rs`,
`harness/fixtures/opencode/1.18.30/`, `harness/fixtures/opencode/README.md`;
`harness/mod.rs`. No core change is expected; one that turns out to be needed follows rule 7
and is reported.

**First, two measurements**, recorded in the fixture README with their commands:

1. Whether the first run on a fresh config directory reaches the network: `strace -f -e
   trace=network` on a run with a fresh `HOME`, looking for a `connect` to anything but
   loopback. If it does, `invocation()` rejects a `network: false` request, typed, and the
   README says why. Pre-populating at probe time is refused until someone needs it.
2. Whether `"apiKey": "{env:AI_GATEWAY_API_KEY}"` in the injected config is substituted: the
   fake server's log shows the `Authorization` header. If it is not, the key goes into the
   config value itself — it is registered for redaction by the core either way.

**`harness/opencode.rs`** — target 250 lines:

- *Descriptor:* kind and program `opencode`; `Wire::OpenAiChatCompletions`;
  `ModelSelection::Explicit`; `inherited_env: &["PATH"]`; `credential_env: None`;
  `observes_served_model: false`; a probe note naming `1.18.30` as the tested version (ADR
  0067 decision 9 — outside it the note says untested, never a refusal).
- *`invocation()`:* rejects a request with no resolved endpoint. Args: `run --pure --format
  json --title tack -m tack/<model id>`, no positional message — the prompt arrives on
  stdin. Env: `HOME=<workspace root>/.tack-runner/opencode-home`, `OPENCODE_CONFIG_DIR`
  under it, `OPENCODE_DISABLE_PROJECT_CONFIG`, `_AUTOUPDATE`, `_MODELS_FETCH` and `_SHARE`
  set to `1`, and `OPENCODE_CONFIG_CONTENT` built with `serde_json::json!`: one provider
  `tack` on `@ai-sdk/openai-compatible` with the endpoint's base URL, the key reference and
  the one requested model, plus the `permission` block — `webfetch` and `task` denied unless
  the policy grants network or sub-agents, `edit` and `bash` from the tool list (ADR 0067
  decision 6).
- *`report()`:* from the events, never the exit code (a denied tool exits 0). Success is a
  final `step_finish` with `reason: "stop"` and no `error` event; tokens are summed from
  `step_finish.tokens`; `cost` is always `None` (the stream says `0` for an unknown model);
  `observed_model` is `None`, so every attempt is `requested_not_confirmed`. Exit 143 with
  no terminal event is a failed run whose reason says `cancelled`.
- *`capabilities()`:* as ADR 0067 decisions 3, 7 and 8; `permission_policy` advisory, with
  the mapping above as its reason.

**Tests — five functions:** the command line and environment for a request; the
`permission` block as a table over policies; the refusals as a table; events to a report as
a table over captured fixtures (completed, tool denied with exit 0, cut short); and the real
binary against `wiremock` answering a streamed (`text/event-stream`) tool call and a final
message, asserting the written file, measured tokens, `requested_not_confirmed`, and that
the real `HOME` was not touched.

**Done when:** as H2, for `opencode`.

### H4 — what a user reads

**Files:** `README.md` (shared — touch only the harness table), `docs/book/src/user-guide/
agent-runners.md`, the two fixture READMEs' cross-references. One table of four harnesses:
what each needs installed, which provider wires reach it, what it measures and what it
cannot. One install method per harness. `docs/CONFIG.md` gains nothing — no new variable.

## Asked of docket, none of it blocking

A `harness-v1.1` that would let Tack claim more: an event per child process group it starts,
which is what `cancel: Supported` needs; the token on stderr's first line or a
`--token-file`, which is what would make `harness status` usable for recovery; a README
beside `schema.json` stating the argv/env/stdout/exit-code contract in prose. Nothing in
`../rack-cli` is edited or committed from this repository.

## Upgrades, each its own change with its own proof

| Upgrade | Proof required |
|---|---|
| codex reads `exec --json` (usage, served model) | a captured transcript under `fixtures/codex/<version>/` |
| codex applies the permission policy | its sandbox and approval flags measured against a live run |
| `min_capture_bytes` removed from the descriptor | a captured transcript over the cap, read correctly from head and tail |
| docket `cancel: Supported` | docket emits child group ids; the crash matrix shows nothing survives a `SIGKILL` of the harness |
| docket `artifacts: Supported` | the result lists the paths its own tools wrote |
| docket `decisions: Supported` | **its own ADR** — it turns a subprocess into a session that survives a pause. It is also the only caller the runner-v1 `decisions` path would ever have; see ADR 0068's 2026-09-18 amendment |
| opencode served model confirmed | an event or export field carrying the served id |

## Refused, by name

- A plugin seam, descriptors read from a file, or a DSL for grammars.
- A grammar hook for recovery, a prompt-file delivery mode, or a core step that creates a
  harness's home directory: today's measurements show none has a caller.
- A remote docket, docket's multi-agent pods, or any import from `tack_orch::adapters`.
- Copying docket source or fixtures into this tree; Tack's fixtures are its own captures.
- A new `TACK_*` variable: endpoint and key already flow through provider injection.
- A UI card: the harness picker renders whatever kinds the runner reports.

## Done when

1. On a machine with all four CLIs and a Vercel key, `tack runner doctor` lists four
   harnesses, with the gateway's catalog under the two open-wire ones.
2. An item runs to completion with `--harness docket` and `--harness opencode`, producing a
   real diff, measured tokens, `Not measured` cost, a served model for docket and
   `requested_not_confirmed` for opencode.
3. Both real-binary tests pass in CI with zero spend, or return early where the binary is
   not installed.
4. `wc -l` of each new harness module is in the range of the two existing ones.
