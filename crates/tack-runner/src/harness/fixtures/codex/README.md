# Codex — vendor findings

What `crates/tack-runner/src/harness/codex.rs` is built on, and what in it is still a
documented guess. `exec --json` output, `exec --help`'s own flag names, and an `app-server`
approval round trip are now captured under `0.149.1/` (see "Fixture provenance" below);
`CodexGrammar::report` still reads only the exit status, because those captures are new
vendor evidence for a future change to the grammar, not a change to
`codex.rs` itself. The grammar's tests are pure
(command line and exit reading); the live tests in `tests/live/codex.rs` resolve a real
`codex` from `PATH`, skip cleanly without it, and are `#[ignore]`d.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

**Observed on:** `codex-cli 0.149.1`, one machine, one point in time.

## Fixture provenance

Each `0.149.1/*` fixture has a sibling `*.provenance` text file: first line `captured`, then
the exact command, the env var names it ran with (never values), and the capture date. Every
capture ran against the installed `codex-cli 0.149.1` (resolved from `PATH`), a
scratch `HOME`/`XDG_CONFIG_HOME`/`CODEX_HOME` (never `~/.codex`), and a loopback Python fake
standing in for the OpenAI Responses wire — `python3 fake_responses_server.py <port>`, kept
under `/var/tmp/tack-measure/wave1/m1/` (a scratch path, not part of this repo), speaking
just enough of the SSE Responses shape (`response.created`, `response.output_item.added`,
`response.function_call_arguments.{delta,done}`, `response.output_item.done`,
`response.completed` with a `usage` object and a `model` field) for `codex` to make one real
tool call — its `name`/`parameters` read straight off whatever `tools` array the real request
itself carried — and finish. No real model endpoint was ever contacted; `OPENAI_API_KEY`/the
provider's `env_key` were set to an obviously fake value.

- `exec-tool-call.jsonl` — `codex exec --json`'s full stdout for a prompt that makes one tool
  call (`--sandbox danger-full-access`, so the call runs with nothing to deny).
- `exec-help.txt` — `codex exec --help`, verbatim, against the real binary.
- `exec-sandbox-read-only.jsonl`, `exec-approve-for-me.jsonl`,
  `exec-bypass-approvals-and-sandbox.jsonl` — the same one-tool-call prompt, one run per
  sandbox/approval flag this installed version actually has (`--sandbox <mode>`,
  `--approve-for-me`, `--dangerously-bypass-approvals-and-sandbox` — there is no
  `--ask-for-approval` or `--full-auto` flag in 0.149.1; see `exec-help.txt`).
- `app-server-approval.txt` — a trimmed `codex app-server` transcript over stdio: the one
  stdout line carrying an approval request, and the one stdin line answering it.

## Measured

- Non-interactive execution is `codex exec --json --model <requested model id>` with the
  agent profile's instructions piped over **stdin**, never argv, for the same
  `ps`/`/proc` exposure reason `../process.rs` documents — confirmed against the real
  binary, including with a provider pointed at a different endpoint via per-invocation `-c`
  overrides (`model_provider=...`, `model_providers.<key>.*`): the request genuinely reached
  that endpoint rather than Codex's built-in OpenAI provider, and no `~/.codex/config.toml`
  was written (`CodexGrammar::invocation`).
- Version detection invokes `codex --version`, exit code 0; the real CLI prefixes the
  version with a program-name token rather than printing it bare, so detection scans for a
  strict `X.Y[.Z]` numeric token anywhere in the output rather than requiring the whole line
  to be one (`local_process::parse_version`, shared by every harness).
- On the installed binary (0.149.1), `model_providers.<key>.wire_api="responses"` is
  already the effective default for a per-invocation `-c` provider override — the adapter
  sets it explicitly anyway, defensively, matching the vendor's own documented shape
  (`CodexGrammar::invocation`).
- **Usage is in the output.** `exec --json`'s terminal `turn.completed` line carries a real
  `usage` object — `input_tokens`, `cached_input_tokens`, `cache_write_input_tokens`,
  `output_tokens`, `reasoning_output_tokens` — every one of these captures (`exec-tool-call.jsonl`
  and the per-flag fixtures beside it). No `cost_usd` field appears anywhere. The adapter does
  not parse this yet (`CodexGrammar::report` still reads only the exit status); this is
  evidence for that future change, not the change itself.
- **The served model is not in the output.** Nothing in `exec --json` names which model
  answered as opposed to which was requested; the only model-shaped line is a `type:"error"`
  warning ("Model metadata for `<model>` not found...") that just echoes the `--model` flag
  back. This strengthens finding 3 below rather than resolving it.
- **Flag mapping for `codex exec` (0.149.1 has no `--ask-for-approval` or `--full-auto`; see
  `exec-help.txt`):** no sandbox flag at all, `--sandbox danger-full-access`, and
  `--dangerously-bypass-approvals-and-sandbox` all let a tool call proceed with no prompt
  (`exec-tool-call.jsonl`, `exec-bypass-approvals-and-sandbox.jsonl`). `--sandbox read-only`
  and `--approve-for-me` also proceed with **no prompt**, but the OS-level sandbox silently
  denies any write the call attempts (`Read-only file system`, exit 1 inside the tool's own
  output) — and a denied call does not appear as any `item.*` line in `--json` at all; only a
  successful one does (`exec-sandbox-read-only.jsonl`, `exec-approve-for-me.jsonl`). None of
  these flags ever produced an interactive ask in `exec --json`.
- **`codex app-server` does ask over stdio.** Default transport is `stdio://`, framed as one
  JSON object per line (no Content-Length headers). Under `approvalPolicy:"on-request"`, a
  plain command still auto-runs with no prompt — asking is tied to the tool call's own
  `sandbox_permissions:"require_escalated"` (a field the real `exec_command` tool schema
  exposes), not to the approval policy alone. When a call does need escalation, exactly one
  `item/commandExecution/requestApproval` request appears on stdout, and answering it on
  stdin with `{"decision":"accept"}` releases it — the command then runs and the item
  completes with `exitCode:0` (`app-server-approval.txt`). Answering with `{"decision":
  "approved"}` instead is rejected by codex's own deserializer (logged to stderr) and the
  item completes `"failed"`; the accepted decision values are `accept`, `acceptForSession`,
  `acceptWithExecpolicyAmendment`, `applyNetworkPolicyAmendment`, `decline`, `cancel`.

## Unverified — documented guesses, not facts

1. The installed binary is literally named `codex`, found via a `PATH` search, never a
   hardcoded path (`local_process::BinaryLocator`).
2. Because non-interactive stdout/stderr shape is unverified, the adapter never attempts to
   parse it: `terminal_state` is derived solely from the child's exit code (`0` succeeded,
   nonzero/signalled failed, killed-by-timeout failed). It deliberately does not
   special-case the shared fixture's `malformed` mode into a different outcome
   (`CodexGrammar::report`) — special-casing it would itself be inventing a contract this file has
   not verified.
3. Whether/how Codex reports which model it actually used is still unverified — now measured
   as *absent*, not merely unmeasured: none of `exec --json`'s own line types ever name a
   served model (see "Measured" above). Rather than fabricate an "observed" model,
   `ActualExecution`'s `model_provider`/`model_id` echo the **requested** selection with
   `model_observation_source = "requested_not_confirmed"` — a value not present in any frozen
   fixture, which only exemplifies `"harness_reported"`.
4. Session resume is still unverified. `exec --json`'s usage output and `exec`'s own
   sandbox/approval flags are no longer unverified (see "Measured" above and the
   `0.149.1/exec-*` fixtures); a decision/approval protocol is now observed too, but only on
   `codex app-server` (`app-server-approval.txt`), a transport `CodexGrammar` does not drive —
   `codex exec`, the one this adapter actually runs, never asks. Each capability is still
   reported honestly (`unsupported`/`advisory` with a reason) rather than assumed
   (`CodexGrammar::capabilities`); these captures are vendor evidence for a later change, not a
   capability-table change itself.
5. Codex's real model-discovery mechanism (if any) is unverified, so
   `HarnessCapability::model_combinations` is always empty — never a hardcoded list.
6. How a request's `permission_policy` (tool list, network flag) and `budgets` map onto
   Codex's sandbox and approval flags is still unmeasured in the sense that matters here: the
   adapter passes none of them. What the flags themselves do in isolation is now measured
   (see "Measured" above and `0.149.1/exec-sandbox-read-only.jsonl`,
   `exec-approve-for-me.jsonl`, `exec-bypass-approvals-and-sandbox.jsonl`) — deciding which
   flag a given `permission_policy`/`budgets` request should map to is separate follow-up
   work. The capability table declares `permission_policy: unsupported` for this harness
   (`CodexGrammar::capabilities`) rather than leaving the difference silent.
7. Codex runs with an environment that carries neither `HOME` nor `PATH`
   (`codex::DESCRIPTOR.inherited_env` is empty). Whether its tool commands find a user's
   toolchain that way has not been measured with a live run.

## Why `ActualExecution.model_provider`/`model_id` are non-nullable but this adapter cannot
   always fill them honestly

`ExecutionRequestSnapshot.requested_model_provider`/`requested_model_id` are `Option<...>` —
nullable when auto-selection is allowed. `ActualExecution.model_provider`/`model_id` are
**not** `Option`. Because this adapter has no verified way to observe which model an
auto-selected Codex run actually used (finding 3 above), it cannot honestly fill a
non-nullable field for that case without fabricating a value — exactly the kind of hidden
fake success this codebase forbids. Rather than guess, `validate` rejects a spec with no
explicit `requested_model_provider`/`requested_model_id` pre-spawn
(`ModelSelection::Explicit` in `codex::DESCRIPTOR`). This is a real, falsifying observation about the frozen
contract — non-nullable fields this adapter cannot always honestly fill — not something this
adapter resolves unilaterally.
