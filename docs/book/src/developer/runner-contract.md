# The Runner Contract

This page is for people who write or change a runner, add an agent adapter, or integrate with the runner protocol. It names the concepts the protocol keeps apart, the capability snapshot a runner reports, the recovery state that needs a person, the version rule, and what the test suite proves today. After reading it you can tell which statements are guaranteed by the contract and which are only the current behaviour of the code.

The contract itself is the set of language-neutral fixtures in `docs/contracts/runner-v1/`. They outrank every Rust and TypeScript type: change a fixture first, then the type that reads it, then the pin that hashes it. Operators who install a runner want [Installing and Operating Runners](../admin-guide/runners.md) instead.

## Concepts

| Term | What it is |
|---|---|
| **Execution request** | A durable, idempotent record: "run this item through this harness, on this runner or fleet, with this agent profile." Created via `POST /api/executions`. |
| **Execution attempt** | One numbered try at a request. Owns a fencing token, a lease, and an isolated workspace. A request can have more than one attempt over its life (retries, recovery). |
| **Runner** | A `tack-runner` process, identified by a durable `runner_id`, enrolled once and then polling for work. It may hold several attempts at once. |
| **Runner fleet** | A named group of runners sharing an optional concurrency limit and default policy. An execution request targets either one exact runner or a fleet. |
| **Agent profile** | Reusable instructions and tool policy and limits, snapshotted into the request at creation time so later edits to the profile never change history. A profile may name its agent (`tool_policy.agent`), its docket team (`tool_policy.docket.team`) and a fleet. |
| **Harness** | The coding-agent adapter a runner can launch: `codex`, `claude-code`, `docket` or `opencode`. The wire value is an opaque string, so a runner may report a kind this build has no bundled adapter for. |
| **Team** | docket's long-lived group of members with their models, pipeline and gates. A docket attempt is one task posted to a team. See [Runs, Teams and Automation](runs-and-teams.md#the-docket-adapter). |

`Item` ≠ `ExecutionRequest` ≠ `ExecutionAttempt`: these stay distinct on the wire and in the database on purpose; see `docs/contracts/runner-v1/protocol.json`.

### What the harness receives

Whoever creates the request, the server writes the instructions the harness is given: the agent profile's own text, then the item's title and description, then the item's brief (its definition of done, acceptance criteria and constraints) when it has one. The title and description are labelled with where they came from and marked as data to work from, not as instructions that change the agent's rules, because an imported item's text was written by someone else. The brief is also saved with the request, and the attempt keeps a copy of it as `brief.json` beside its `evidence.json`. An item without a brief still reaches the harness with its title and description.

## The protocol surface

The runner protocol is mounted at `/api/runner/v1` as a sibling of `/api`, outside the operator token check. `docs/openapi.json` lists 14 operations there, all authenticated with the runner's own hashed bearer credential:

| Group | Operations |
|---|---|
| Identity | `enroll`, `refresh` |
| Work | `claim`, `heartbeat` |
| One attempt | `accept`, `start`, `events`, `decisions`, `decisions/poll`, `artifacts`, `artifacts/{artifact_id}/content` (PUT, the bytes), `completion`, `cancellation-observation`, `recovery-observation` |

Thirteen of them carry a JSON body with `protocol_version`; the content upload carries the artifact's bytes. The frozen fixtures under `docs/contracts/runner-v1/` are 48 files, including 15 error envelopes under `errors/`. They are pinned by FNV-1a hash in `crates/tack-protocol/tests/runner_contract/fixtures.rs`, which asserts the count.

### Several attempts at once

A runner reports `concurrency { total, available }` in its capability snapshot and sends one heartbeat for all its leases (`heartbeat.request.two-active.json` shows two). The server accepts up to 64 active attempts per heartbeat (`active_attempts_per_heartbeat_max` in `limits.json`) and answers a conflict to a heartbeat whose `available_capacity` is not the runner's total capacity minus the attempts the server itself holds as active for it. A cancellation asked for an attempt arrives in the `lease_results` of that heartbeat, and the runner flips that attempt's cancel signal. A folder worked in place stays one attempt at a time, by the runner's own lock on the folder.

Every runner message (enrollment, refresh, claim, heartbeat, accept and start, events, decisions, the artifact manifest, completion and the cancellation and recovery observations) is a struct in `tack-protocol` (`execution/messages.rs`, plus the recovery observation in `types.rs`): the server's handlers and the runner's transport both serialize and deserialize them, and the contract test round-trips each matching fixture through them, so a field added to a fixture without a field on the struct fails there. They do not deny unknown fields, because `protocol.json` has an unknown optional field ignored. A request that does not fit its struct answers `400 invalid_request` with the field it got wrong in `details.field` (`options[0].label`, `fencing_token`).

Every request carries a correlation id in the `x-request-id` header, which the runner makes anew for each HTTP request, retries included. The board uses it as the `request_id` of any error body it answers with, repeats it in the response's `x-request-id` header and logs it; a request without one, or with one longer than 64 characters or made of anything but letters, digits and `_-.:`, gets an id the board makes.

### Decisions

A decision's `kind` is a free string. Every harness that can pause sends `tool_permission`. docket sends `approval` for a gated tool call and `consult` for a question a step has parked on. The inbox renders any kind, and the answer returns on the same decision route, so a new kind needs no protocol change.

## Capability matrix

Every runner reports a **capability snapshot** at enrollment and refresh time: protocol version, concurrency, installed harness versions, and per-feature support for `cancel`, `resume`, `decisions`, `artifacts`, and `usage`, each as `unsupported | advisory | supported` with an optional reason (`docs/contracts/runner-v1/capabilities.json`). The scheduler and UI are required to read this snapshot rather than assume a feature works.

A harness entry may also carry `teams` (a docket report of each team, its profile and the violations that keep it from running lights-out) and `autonomy_ceiling` (`L3` when any team is capped, else `L5`). Both are optional, and an unknown field lands in `additional`. `capabilities.with-teams.json` is the example. The board reads them when it decides whether an automatic merge is allowed; see [the ceiling](runs-and-teams.md#the-ceiling).

**The one enforced rule:** an adapter may claim `cancel: supported` only if its harness announces the process groups its shell tool starts, or stops its work over its own API. `AdapterRegistry::register_probe` rejects any other probe that claims it, at registration time, before any attempt can reference it: `crates/tack-runner/src/harness/mod.rs`, proved by `harness::tests::registering_a_probe_overclaiming_cancel_support_is_rejected`. The reason is structural: a harness's own shell tool spawns its subprocess in a new session outside the runner's process group, so without being told the group the runner cannot promise it stopped (confirmed with `ps` against each real harness installation; see each harness's own `fixtures/<kind>/README.md`). docket's cancellation is a call to its own cancel route, which stops the running step and its process group, so `cancel` is `supported` for docket and `advisory` for the other three harnesses. On a cancellation of a harness that announces process groups the runner stops the main group, then sends SIGTERM and, after the grace period, SIGKILL to each announced group still live. The cancellation evidence carries `details.groups` as `{tracked, killed, survived}`. A harness that announces nothing always shows `tracked: 0`, which means nothing was reported, not that nothing was left.

| Feature | Ceiling in this build | Why |
|---|---|---|
| `cancel` | `supported` for docket (its own cancel route), `advisory` for everything else | Only a harness that announces its process groups, or stops them over its own API, can be held to stopping them. `AdapterRegistry::register_probe` rejects a stronger claim from any other |
| `resume` | `supported` for docket; `unsupported` for the others | Only an agent that lives outside the runner can be taken back: a docket task stays in docket's daemon, and `HarnessAdapter::resume` rebuilds its handle from the journal's claim and `<team>/<task id>`. Every harness that runs as a child of the runner keeps the trait's default, `Unsupported`, because its process died with the runner |
| `decisions` | adapter-reported | Runner-driven bounded decisions (`POST .../decisions`) work for claude-code, codex, opencode and docket, and only when the request asks for it |
| `artifacts` | `supported` for docket, whose task branch is read from the repository, `advisory` otherwise | The other harnesses cannot guarantee artifact discovery |
| `usage` | `supported` for docket (the task record carries tokens in total and by model); `advisory` otherwise | Token totals may be absent from the output of the other harnesses |

## `needs_operator` and recovery

`needs_operator` is an explicit, non-automatically-retryable state for an attempt whose ownership after a crash or network split cannot be proven safe. Tack never blindly launches a second process against the same workspace. Full mechanics, including exactly which recovery observations lead where, are in the [Recovery Runbook](../admin-guide/recovery-runbook.md); the short version:

1. A restarted runner reads its own journal and reports what it actually observed: `ProcessStopped`, `ProcessRunning`, or `Ambiguous` (`POST /api/runner/v1/attempts/{id}/recovery-observation`). For docket, the adapter's `reconcile` reads the task named by the journal's `<team>/<task id>` and reports it stopped when the task is in a terminal state.

   When `reconcile` answers `ProcessRunning`, the journal is `process_observed_running` and it holds the claim (`AttemptJournal.claimed_work`, which carries references to secrets and no literal environment value), the engine calls `resume`, then sends one heartbeat for the lease with the journal's fencing token. If the heartbeat is accepted, `recover` hands the prepared run back as `RunCycle::Resumed` instead of following it, and `serve` follows it in a run slot of its own (`follow_resumed`), so the ticker heartbeats its lease with every other and the journals behind it are recovered without waiting. It takes the same path a fresh run takes after it starts (`follow_to_end`: the wait, evidence, verifier, publish, terminal report), and no recovery observation is sent. The attempt's deadline starts over at the resume. Decisions the board already holds for the attempt are read once and are not created again when the agent asks the same question; the existing decision is waited for. If `resume` fails or answers `Unsupported`, or the heartbeat is refused, the runner reports `ProcessRunning` as before. A journal written before this existed has no claim and recovers as it always did.
2. The server computes a disposition: `SafePreSpawnRequeue` (nothing had started yet, automatically safe), `NeedsOperator` (anything else), or `AlreadyTerminal`.
3. An operator resolves a `needs_operator` request explicitly, with an audited reason:

```sh
tack execution reconcile <request-id> \
  --recovery-key <key> \
  --reason "confirmed process was killed via ps on the runner host"
```

This calls `POST /api/executions/{id}/requeue`, which only succeeds for a request the server itself authoritatively recovered. An unresolved or already-queued request returns `409 invalid_transition` with the current state named in `details`, never a silent no-op. Verified by hand against a live server (an unresolvable request returns `{"code":"invalid_transition","details":{"from":"unknown","to":"queued"}}`) and pinned by `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`.

## Version compatibility

Every runner-v1 request body that carries JSON holds `"protocol_version": 1`. The server rejects anything else outright. `check_protocol_version` in `crates/tack-api/src/handlers/runner_protocol.rs` returns `unsupported_protocol` (the fixture is `docs/contracts/runner-v1/errors/unsupported-protocol.json`) for any other number, and `invalid_request` when the field is missing or not a number, on every one of the 13 operations that carry a JSON body. There is no negotiated fallback: a v2 protocol, if it ever exists, is a new contract revision, not a version range this server tries to interpolate.

Separately, each runner reports its own `runner_version` (a free-form string in the capability snapshot) and each harness reports its `installed_version`. Both are informational, logged and stored, and the server never uses them to gate behaviour. docket is the one place a version is enforced, and by the runner's probe, not by the protocol: the adapter reads `apiVersion` from docket's `GET /status.json` and accepts `"5"` or any later number.

## docket

docket is reached as any other agent: as a harness (`--harness docket`) on a runner-v1 execution request. Its adapter does not spawn a process per run. It speaks docket's operator HTTP API, version 6, which needs docket 0.2.0-beta.6.

- An `apiVersion` below 6, or one that is not a number, is reported as "needs docket 0.2.0-beta.6 (API 6); this docket answers API N" and no task is posted.
- `docket exec`, the contract probe, the NDJSON output parser and the stdin answers of the earlier adapter are gone. Nothing in the runner starts a throwaway docket per run.
- One attempt is one task posted to a team, with the attempt id as the idempotency key. The adapter follows the team's trace, carries approvals and parked questions to the inbox as decisions, cancels over HTTP, and reads the task's branch from the repository; the runner makes no checkout for it.
- The daemon is supervised by the runner only when `[docket].binary` is set in its configuration.

Every call, the task body and the team rules are in [Runs, Teams and Automation](runs-and-teams.md#the-docket-adapter). The separate Docket control-plane bridge, with its own dispatch, approval and budget routes, was retired earlier; docket has no scheduling path or API surface of its own in Tack.

## Known gaps

These are documented rather than papered over, per this project's "unsupported is typed, unknown is explicit" rule:

- **`execution_requests` has no real `priority` column.** A `metadata`-convention stopgap exists, documented as non-binding.
- **Only docket reports a run's steps while it works.** codex, claude-code and opencode show their steps in the log only; their timelines fill once their tool lines are read from captured output of the real binaries.
- **docket is proved against a fake.** No test runs a real docket 0.2.0-beta.6; see below.

## What actually runs today

Read this section before treating any earlier claim on this page as a promise about a live, end-to-end run. This is the one page where test names belong.

**Proven end-to-end against the real production router** (`build_router`, no card-local test scaffolding): creating an execution request, scheduling it to an exact runner or a fleet, the full runner-v1 protocol surface (enroll, claim, heartbeat, accept, start, events, decisions, artifacts, completion), cancellation and recovery-observation handling, the decision-resolve and artifact-download routes, and the retention and expiry sweeps. See `crates/tack-api/tests/handlers/production_router.rs`, `crates/tack-protocol/tests/runner_contract.rs` (byte-pins all 48 frozen fixtures), and the integrator test files referenced throughout this page.

**The `tack-runner` binary's own network transport is wired and proven.** `tack_runner::bootstrap::build_runtime` is the one composition root that both the standalone `tack-runner` binary and the embedded `tack serve --with-runner` and `tack runner start` paths call. It wires the real `HttpPullProtocol`, not `UnavailableProtocolClient`. `UnavailableProtocolClient` still exists in the crate as an explicit no-client fallback, reachable only if something constructs a runtime without attaching a protocol client, and stays pinned by `runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` so that path fails as a typed `RunnerError::ProtocolUnavailable` rather than a silent success.

Live, end-to-end proof of the real client: `crates/tack-runner/tests/bootstrap_entrypoint.rs` proves the composition root is reachable and shutdown-controllable from outside the crate against a real (mocked) HTTP enrollment exchange. Every run of `scripts/smoke.sh` exercises the real client against a real server: step 7 drives a real attempt through claim, checkout, harness and completion with a separate `tack-runner` process; step 9 kills the runner mid-attempt and proves restart recovery; step 10 does the claim-to-completion path inside a single `tack serve --with-runner` process. **A fresh-machine operator today can enroll a runner and point a real `tack-runner` process (standalone or embedded) at a real server and watch it enroll, claim, and complete real work.**

**Several attempts at once** is proved in `crates/tack-runner/src/harness/local_process/tests.rs`: with two slots, two claims run together, every heartbeat carries both leases, and cancelling one attempt through the heartbeat answer leaves the other running. `crates/tack-runner/tests/crash_matrix.rs` recovers two unresolved journals after a crash.

**docket is proved against a fake, not a real docket.** `crates/tack-runner/tests/docket_v5.rs` drives `DocketAdapter` against `crates/tack-runner/tests/fake_docket/mod.rs`, an in-process docket written from the published API specification. It covers the version probe and the "needs docket 0.2.0-beta.6" refusal, the teams and the ceiling, the task body key by key, the default team named for the folder, the trace cursor surviving a restart, approvals and parked questions as decisions, and the task's branch coming back into the worktree. The daemon's supervision is proved in `crates/tack-runner/src/harness/docket/daemon/tests.rs`. Until a real docket 0.2.0-beta.6 exists, nothing proves the fake matches it.

The whole workflow through the browser is proven by hand with a local suite that is not versioned (`onboarding-lab/e2e/docket-prill-e2e.sh` in the maintainer's checkout). It builds Tack from the checkout, runs docket and prill from their own source trees against a local model, and drives a project, its runner, its settings, a bug, a feature, a task and a run that asks through the UI, each verified by prill and reviewed, with a screenshot per step. It makes real model calls, so CI never runs it.
