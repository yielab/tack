# Codex — vendor findings

What `crates/tack-runner/src/harness/codex.rs` is built on, and what in it is still a
documented guess. No captured transcripts live here yet: `exec --json` output has never been
captured, which is why `CodexGrammar::report` reads the exit status and nothing else. The
first captured `<version>/*.jsonl` transcript, with its provenance line, is what lets that
change. The grammar's tests are pure (command line and exit reading); the live tests in
`tests/live/codex.rs` resolve a real `codex` from `PATH`, skip cleanly without it, and are
`#[ignore]`d.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

**Observed on:** `codex-cli 0.149.1`, one machine, one point in time.

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

## Unverified — documented guesses, not facts

1. The installed binary is literally named `codex`, found via a `PATH` search, never a
   hardcoded path (`local_process::BinaryLocator`).
2. Because non-interactive stdout/stderr shape is unverified, the adapter never attempts to
   parse it: `terminal_state` is derived solely from the child's exit code (`0` succeeded,
   nonzero/signalled failed, killed-by-timeout failed). It deliberately does not
   special-case the shared fixture's `malformed` mode into a different outcome
   (`CodexGrammar::report`) — special-casing it would itself be inventing a contract this file has
   not verified.
3. Whether/how Codex reports which model it actually used is unverified. Rather than
   fabricate an "observed" model, `ActualExecution`'s `model_provider`/`model_id` echo the
   **requested** selection with `model_observation_source = "requested_not_confirmed"` — a
   value not present in any frozen fixture, which only exemplifies `"harness_reported"`.
4. Session resume, a decision/approval protocol, and parseable usage (token/cost) output are
   all unverified. Each is reported honestly (`unsupported`/`advisory` with a reason) rather
   than assumed (`CodexGrammar::capabilities`).
5. Codex's real model-discovery mechanism (if any) is unverified, so
   `HarnessCapability::model_combinations` is always empty — never a hardcoded list.
6. How a request's `permission_policy` (tool list, network flag) and `budgets` map onto
   Codex's sandbox and approval flags is unmeasured, so none of them is passed. The
   capability table declares `permission_policy: unsupported` for this harness
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
