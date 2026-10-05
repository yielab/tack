# docket — vendor findings

What `crates/tack-runner/src/harness/docket.rs` (`DocketAdapter`) is built on. Every claim
below was checked by hand against the real installed binary, run under `env -i` with an
isolated `DOCKET_HOME` and a throwaway loopback server standing in for the model endpoint —
never `../rack-cli` and never a real model endpoint.

**Observed on:** `docket 0.2.0b1`, one machine, one point in time.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

## Fixture provenance

Each `<version>/*.ndjson` fixture has a sibling `*.ndjson.provenance` text file: its first
line is `captured` (from a real invocation, with the scratch directory it ran in rewritten to
`/capture` and nothing else changed) or `constructed` (built by hand
because no real invocation produces this shape); everything after is why and how. All four
fixtures here are `captured` — every status this adapter's tests exercise (`ok`, `refused`,
`blocked`, `cancelled`) was reproduced against the real binary, so none needed constructing.

## Measured

- `docket harness run --workspace <dir> --task-file /dev/stdin --model <provider>/<id>
  --agent-id <id> --timeout <seconds>` streams one NDJSON progress event per line on stdout,
  finishing with exactly one more line: a versioned result object carrying `token`, `status`,
  `stop_reason`, `error`, `blocked`, `model.requested`/`model.served`,
  `usage.input_tokens`/`usage.output_tokens` and `cost_usd`. The adapter reads only that last
  non-empty line (`docket::last_non_empty_line`) — the preceding progress events are real
  evidence of the run but are not part of the adapter's contract.
- `--model P/ID` is split on the first `/`; only `ID` is sent to the configured endpoint's
  `model` field (`gw/anthropic/claude-x` on argv produced `"model":"anthropic/claude-x"` in
  the captured request body). The adapter passes `<requested_model_provider>/<requested_model_id>`
  verbatim and never inspects what docket does with the prefix.
- The endpoint is called at `<DOCKET_LLM_BASE_URL>/chat/completions` with
  `Authorization: Bearer <DOCKET_LLM_API_KEY>`. `DOCKET_LLM_BASE_URL` carries the `/v1`
  segment itself (mirrors the Vercel AI Gateway's own catalog host); `DOCKET_LLM_API_KEY` is
  docket's own variable name, never a provider's — `HarnessDescriptor::credential_env`
  injects the resolved endpoint credential under it.
- `model.served` in the result line is populated straight from the chat-completions
  response's own `model` field — a real observation of what answered, even though the
  request went through a gateway that could in principle have substituted a model
  (`HarnessDescriptor::observes_served_model`). It is empty (`""`, recorded as `None`) when
  no model response was ever received (`refused`, and `cancelled` before any reply arrived).
- Leaving `DOCKET_HOME` unset refuses the run before any network call: exit code 2, one
  result line with `status: "refused"` and no `token`.
- A tool call outside docket's own curated allowlist (`bash` running `curl`, which is not
  allowlisted) is denied immediately — harness mode's approval posture is fixed to
  non-interactive refusal, never a pause. The run's own terminal `status` becomes `"blocked"`,
  and the result's `blocked` object names the tool, the call id and the denial reason. Exit
  code 1.
- `SIGTERM` mid-run (sent while the fake server was deliberately not answering) is persisted
  as a cancellation request; the process exits with one result line, `status: "cancelled"`,
  `stop_reason: "run_cancelled"`. Exit code 1 — cancellation is not distinguished from
  `failed`/`blocked` by exit code, only by the result line's `status`, which is why this
  adapter never reads the exit code to decide the verdict.
- `docket --version` prints `docket 0.2.0b1` — a program-name token followed by a version
  that itself mixes digits and a trailing letter+digit suffix (`0.2.0b1`, not a plain
  `X.Y.Z`), which `local_process::parse_version` now recognizes as a later token.
- `usage.input_tokens`/`usage.output_tokens` are real per-run token counts; `cost_usd` was
  `null` in every captured result. The adapter reports tokens as measured and cost as always
  unmeasured.

## Unverified — documented guesses, not facts

1. `cancel` is `Supported` only on contract 1.1, where docket announces each tool's process
   group (`process_started`/`process_exited`, `contract-1.1/cancelled-process.ndjson`) and the
   runner stops every announced group that is still live; on 1.0 it stays `Advisory`. Not
   covered: a run that hits its timeout rather than a cancel is stopped by its main group
   only, because the non-interactive wait does not read the announcements as they arrive.
2. `resume`/`decisions: Unsupported`: no reattachment interface and no ask-the-operator event
   were observed; harness mode's own `--help` text (`docket harness run --help`) documents
   the fixed non-interactive-refusal posture directly, so this is read from the vendor's own
   words rather than inferred.
3. `artifacts: Advisory`: only the staged stdout/stderr log is claimed; nothing in a result
   line names a file docket's own tools wrote, so no per-file artifact discovery is
   implemented.
4. Whether a network-denying `permission_policy` or a budget could be mapped onto any of
   docket's own flags is unmeasured; none is passed, and `capabilities()` declares
   `permission_policy: unsupported` for that reason.

## Contract 1.1 and the boot probe

`DocketFeatures::probe` (`docket/probe.rs`) runs the located binary at runner boot. Every
probe is a `harness run` with `DOCKET_HOME` unset and `PATH` the only variable, so docket
refuses before any network call: zero spend, nothing written. Measured on a scratch install
of `../rack-cli` develop `25fe5252` (`docket --version` prints `docket 0.2.0b3`, contains
P35-2 `7b42f860`), on a second scratch install of `7b42f860^` (contract 1.0 only; also
prints `0.2.0b3`), and on the operator's `docket 0.2.0b1`. Command shape, `W` any existing
directory:

    env -i PATH=/usr/bin:/bin <docket> harness run --workspace W --task x --model a/b <extra>

- **Contract.** `<extra>` = `--contract 1.1`. Exit 2 and one `refused` result line either way;
  the line's `"v"` is `1.1.0` on a docket that speaks 1.1 and `1.0.0` on one that does not.
  Measured: 1.1 on the scratch develop install and on the operator's, 1.0 on the `7b42f860^`
  install. `--contract 9.9` on a 1.1 docket is also a refusal (`v` `1.0.0`, error `--contract
  must be one of ['1.0', '1.1']`), so a refusal alone proves nothing; `v` does.
- **Flags.** An unknown option is NOT a usage error: docket parses `harness run` by hand and
  ignores what it does not know (the `7b42f860^` install ignores `--contract 1.1` and all five
  flags below and reaches the `DOCKET_HOME` refusal). The reliable probe is the opposite one:
  `<extra>` = `--contract 1.0 <flag> <value>`. A docket that knows the flag refuses it for
  needing 1.1 and names it in `error` (`--answers and --answer-timeout need --contract 1.1`,
  `--token-file, --max-tokens and --policy need --contract 1.1`, `--recipe needs --contract
  1.1`); one that does not knows nothing of it and refuses for `DOCKET_HOME is not set`. The
  adapter reads a flag as present when the refusal's `error` contains the flag's name.
  Measured with `--answers stdin`, `--token-file /x`, `--max-tokens 1`, `--policy /x`,
  `--recipe x`: all five present on the develop installs, all absent on `7b42f860^`. Flags
  are only probed once the contract probe says 1.1.
- **The operator's `0.2.0b1` speaks 1.1.** `~/.local/bin/docket` execs a venv whose package is
  an editable install of the `../rack-cli` working tree, so its `--version` metadata is stale
  while its code is current. The version string says nothing about what the binary does,
  which is why no version is ever compared.
- **1.1 result line.** `contract-1.1/ok.ndjson`: same fields as 1.0 plus `files`, `run_state`,
  `limits`, `approvals`, `question`, `usage.cached_tokens`/`turns`. `--task-file` accepts a real
  file (the adapter writes `<scratch>/task.md`); `/dev/stdin` stays the 1.0 shape.
