# docket — vendor findings

What `crates/tack-runner/src/harness/docket.rs` (`DocketAdapter`) is built on. Every claim
below was checked by hand against the real binary, run under `env -i` with an isolated
`DOCKET_HOME`, a scratch setup home and a throwaway loopback server standing in for the model
endpoint — never a real model endpoint.

**Observed on:** `docket 0.2.0b1` (the result shapes under `0.2.0b1/`), `0.2.0b3`/`0.2.0b4`
(contract 1.1, `contract-1.1/`) and `0.2.0b5` (contract 1.2, the only one the adapter writes,
`0.2.0b5/`); one machine, one point in time.

What a user reads about this harness — install, capabilities, caveats — is
[Choosing a harness](../../../../../../docs/book/src/user-guide/agent-runners.md#choosing-a-harness)
in the user guide.

## Fixture provenance

Each fixture has a sibling `*.provenance` text file: its first line is `captured` (from a real
invocation, with the scratch directory it ran in rewritten to `/capture` and nothing else
changed) or `constructed` (built by hand because no real invocation produces this shape);
everything after is why and how. Every fixture here is `captured`, so none needed constructing.
The progress events and the result line keep their shape from 1.0 to 1.2 apart from `v` and the
fields each contract adds, so the 1.0 and 1.1 transcripts still prove how a line is read.

## Contract 1.2: the probe and the setup home

`DocketFeatures::probe` (`docket/probe.rs`) runs the located binary once, at runner boot:

    env -i PATH=/usr/bin:/bin <docket> exec --workspace . --task probe --contract 1.2

With no `DOCKET_HOME` docket refuses before any network call, exit 2, zero spend, nothing
written. A docket that speaks 1.2 refuses on a `v: "1.2.0"` line (`0.2.0b5/probe-1.2.ndjson`);
an older one on a `v: "1.0.0"` line whose error begins `--contract must be one of`
(`0.2.0b5/probe-1.2-on-0.2.0b4.ndjson`). Only `v` is read: a refusal alone proves nothing, and
the version string is reported, never compared. A docket that does not speak 1.2 carries a
`probe_error` and never runs.

Every option the adapter passes is part of 1.2: `--task-file`, `--answers stdin` (an `ask`
request), `--token-file`, `--max-tokens` (a positive `budgets.tokens`), `--policy`. `--recipe`
is not: docket refuses it under 1.2 (`--recipe needs --contract 1.1`), and 1.1 needs
`DOCKET_LLM_BASE_URL`, which Tack does not hold, so a profile naming a recipe is refused before
spawn.

- **The setup home.** With `DOCKET_LLM_BASE_URL` unset, a 1.2 run reads the endpoint, the
  credential and, when `--model` is omitted, the model from `DOCKET_SETUP_HOME` (default
  `~/.docket`): exactly `docket-providers.json`, `docket-models.json` and `secrets.json`,
  without a lock and without writing. The adapter sets `DOCKET_SETUP_HOME` to a directory in the
  attempt's scratch and shares each of those three files of the runner user's `~/.docket` that
  exists into it, read-only (`--ro-bind` under bwrap, a symlink without one). Pods, policies,
  traces and the audit log beside them are not shared. Every value in `secrets.json` (a map of
  credential name to value; a name kept in the keyring has an empty one) is stripped from what
  the run prints.
- **The keyring.** A credential `secrets.json` lists with an empty value is looked up with
  `secret-tool`, which needs a D-Bus session. A run gets none, sandboxed or not, so docket
  refuses it naming the provider and the credential; `docket setup harness` run by the probe,
  with only `HOME` and `PATH`, refuses the same way.
- **The model.** `--model` is passed only when the request names one; otherwise docket takes
  the setup registry's `roles.implementer`, then its `default`. The result's `model` gains
  `source` (`explicit` or `setup default`); `model.requested` is the model the run used and
  `model.served` what the endpoint's response named (`0.2.0b5/ok-setup-default.ndjson`). The
  adapter records `served` only: a run that stopped before any reply had no model answer it.
- **Readiness.** `docket setup harness --json` (exit 0 when ready, 1 when not, the document
  printed either way) is offline and prints no credential value. `ready` says a run with no
  model would resolve one; `reason` is then empty, otherwise exactly the refusal that run would
  print. `models` lists `provider/id` for every model whose provider needs no credential or has
  one that resolves. The adapter reads it as: signed in when any model is listed (the detail
  names their providers, and the reason when not ready), signed out with docket's reason when
  none is; the listed models; the default model only when `ready`; and a request must name a
  model when there is no default. `0.2.0b5/setup-harness-{ready,nodefault,empty}.json`.
- **The answer line.** An `AnswerLine` stamps the run's contract: `v: "1.2.0"`. docket
  validates it.

## Measured

- `docket exec` streams one NDJSON progress event per line on stdout, finishing with exactly
  one more line: a versioned result object carrying `token`, `status`, `stop_reason`, `error`,
  `blocked`, `model`, `usage.input_tokens`/`usage.output_tokens`, `cost_usd`, `files`,
  `run_state`, `limits`, `approvals` and `question`. The adapter reads the first line that
  parses as a result; the progress events are read only for the steps and process groups below.
- `model.served` is populated straight from the chat-completions response's own `model` field —
  a real observation of what answered (it is what `ActualExecution.model` records, with source
  `reported`). It is empty (`""`, recorded as `None`) when no model response was ever received
  (`refused`, and `cancelled` before any reply arrived).
- A tool call outside docket's own curated allowlist (`bash` running `curl`, which is not
  allowlisted) is denied immediately when the run does not ask. The run's own terminal `status`
  becomes `"blocked"`, and the result's `blocked` object names the tool, the call id and the
  denial reason. Exit code 1.
- `SIGTERM` mid-run is persisted as a cancellation request; the process exits with one result
  line, `status: "cancelled"`, `stop_reason: "run_cancelled"`. Exit code 1 — cancellation is
  not distinguished from `failed`/`blocked` by exit code, only by the result line's `status`,
  which is why this adapter never reads the exit code to decide the verdict.
- `docket --version` prints `docket 0.2.0b5` — a program-name token followed by a version that
  mixes digits and a trailing letter+digit suffix, which `local_process::parse_version`
  recognizes as a later token.
- `usage.input_tokens`/`usage.output_tokens` are real per-run token counts; `cost_usd` was
  `null` in every captured result. The adapter reports tokens as measured and cost as always
  unmeasured.
- `files` lists the paths the run changed, not ones already dirty before it
  (`contract-1.1/ok-files.ndjson`); the adapter keeps them in the terminal reason with
  `.tack-runner/` entries dropped. `limits.maxTokens` echoes `--max-tokens`.

## Unverified — documented guesses, not facts

1. `cancel` is `Supported`: docket announces each tool's process group
   (`process_started`/`process_exited`, `contract-1.1/cancelled-process.ndjson`) and the runner
   stops every announced group that is still live, on a cancel and on a timeout alike.
2. `resume: Unsupported`: no reattachment interface was observed. `decisions` is `Supported`:
   with `--answers stdin` docket prints an `approval_requested` event for each call its policy
   gates and waits for one answer line (`contract-1.1/asked-answered.ndjson`). The approval's
   token is the event's `payload.token`, not `payload.approvalToken` as docket's own
   hand-written sample shows; the event line's top-level `token` is the run's.
3. `permission_policy` is `Supported`. The request's tool list and network flag go to docket as
   one `kind: policy` document that blocks every docket tool they do not allow (an empty list
   blocks all of them). Only docket's own tool names can be expressed (`read`, `write`, `edit`,
   `glob`, `grep`, `bash`, `fetch`, `skill`, `consult`); a request naming any other tool, or
   asking for `fetch` with network off, is refused before spawn with the field named, never
   narrowed. docket's published `policy.schema.json` marks `appliesTo` optional, but its
   validator refuses a document without it, so the document always carries
   `appliesTo: ["*"]`. Under 1.2 docket validates `--policy` only once its setup resolves a
   model.
