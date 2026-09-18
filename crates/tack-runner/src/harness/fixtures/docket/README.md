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

1. `cancel: Advisory`, never `Supported`: docket's own result line for a cancelled run gives
   no evidence about what happened to a tool subprocess's own process group, so whether a
   `SIGTERM` reaches everything the run started is unconfirmed.
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
