# docket — vendor findings

What `crates/tack-runner/src/harness/docket.rs` (`DocketAdapter`) is built on. Every claim
below was checked by hand against the real installed binary, run under `env -i` with an
isolated `DOCKET_HOME` and a throwaway loopback server standing in for the model endpoint —
never `../rack-cli` and never a real model endpoint.

**Observed on:** `docket 0.2.0b1`, one machine, one point in time; the boot probe below was
re-measured on `docket exec`.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

## Fixture provenance

Each `<version>/*.ndjson` fixture has a sibling `*.ndjson.provenance` text file: its first
line is `captured` (from a real invocation, with the scratch directory it ran in rewritten to
`/capture` and nothing else changed) or `constructed` (built by hand
because no real invocation produces this shape); everything after is why and how. Every
fixture here is `captured`, the 1.0 ones (`ok`, `refused`, `blocked`, `cancelled`) and the
`contract-1.1/` ones alike, so none needed constructing.

## Measured

- `docket exec --workspace <dir> --task-file /dev/stdin --model <provider>/<id>
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
2. `resume: Unsupported`: no reattachment interface was observed. `decisions` is
   `Supported` only when the boot probe found `--answers` on contract 1.1: with `--answers
   stdin` docket prints an `approval_requested` event for each call its policy gates and
   waits for one answer line (`contract-1.1/asked-answered.ndjson`). The approval's token is
   the event's `payload.token`, not `payload.approvalToken` as docket's own hand-written
   sample shows; the event line's top-level `token` is the run's. With answers on stdin the
   task must come from a real `--task-file`, never `/dev/stdin`. Without `--answers`,
   harness mode keeps its non-interactive refusal and `decisions` stays `Unsupported`.
3. `artifacts` is `Supported` only when the probe found `--token-file`, the flag that came
   with the 1.1 result's `files` list (`contract-1.1/ok-files.ndjson`); the paths go into the
   terminal reason with `.tack-runner/` entries dropped. `files` lists only paths the run
   changed, not ones already dirty before it. A 1.1 docket without that flag stays
   `Unsupported`: only the staged stdout/stderr log.
4. `permission_policy` is `Supported` only when the probe found `--policy`. The request's tool
   list and network flag go to docket as one `kind: policy` document that blocks every docket
   tool they do not allow (an empty list blocks all of them). Only docket's own tool names
   can be expressed (`read`, `write`, `edit`, `glob`, `grep`, `bash`, `fetch`, `skill`,
   `consult`); a request naming any other tool, or asking for `fetch` with network off, is
   refused before spawn with the field named, never narrowed. docket's published
   `policy.schema.json` marks `appliesTo` optional, but its validator refuses a document
   without it, so the document always carries `appliesTo: ["*"]`.

## Contract 1.1 and the boot probe

`DocketFeatures::probe` (`docket/probe.rs`) runs the located binary at runner boot. Every
probe is a `docket exec` with `DOCKET_HOME` unset and `PATH` the only variable, so docket
refuses before any network call: zero spend, nothing written. Measured on a scratch install
of `../rack-cli` `924d32cf` (`docket --version` prints `docket 0.2.0b4`). Command shape, `W`
any existing directory:

    env -i PATH=/usr/bin:/bin <docket> exec --workspace W --task x --model a/b <extra>

- **Contract.** `<extra>` = `--contract 1.1`. Exit 2 and one `refused` result line either way;
  the line's `"v"` is `1.1.0` on a docket that speaks 1.1 and `1.0.0` on one that does not.
  `--contract 9.9` is also a refusal (`v` `1.0.0`, error `--contract must be one of ['1.0',
  '1.1']`), so a refusal alone proves nothing; `v` does.
- **Flags.** `<extra>` = `--contract 1.0 <flag> <value>`. A docket that knows the flag refuses
  it for needing 1.1 and names it in `error` (`--answers and --answer-timeout need --contract
  1.1`, `--token-file, --max-tokens and --policy need --contract 1.1`, `--recipe needs
  --contract 1.1`). An unknown option is a usage error: exit 2 and nothing on stdout, so no
  result line and the flag reads absent. The adapter reads a flag as present when the
  refusal's `error` contains the flag's name. Measured with `--answers stdin`, `--token-file
  /x`, `--max-tokens 1`, `--policy /x`, `--recipe x`: all five present. Flags are only probed
  once the contract probe says 1.1.
- **The version string says nothing.** The `924d32cf` scratch install and a packaged
  `docket 0.2.0b4` print the same version; only the first has `exec` (the packaged one answers
  `No such command 'exec'`, no result line, so every probe reads absent). Which is why no
  version is ever compared.
- **1.1 result line.** `contract-1.1/ok.ndjson`: same fields as 1.0 plus `files` (read when
  the docket supports it, item 3 above), `run_state`, `limits` (`maxTokens` is echoed into the
  terminal reason), `approvals`, `question`, `usage.cached_tokens`/`turns`. `--task-file` accepts a real
  file (the adapter writes `<scratch>/task.md`); `/dev/stdin` stays the 1.0 shape.
