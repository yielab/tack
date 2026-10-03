# ADR 0070: docket contract 1.1 is adopted — cancel becomes per-harness evidence, a recipe is one flag, the task goes by file

**Status: ACCEPTED 2026-10-03, with the two amendments at the end.** Written for `docs/plans/phase-66.md` (A1–A3). It
waits on docket Phase 35 (`../rack-cli/docs/adr/0017-docket-in-a-harness-agnostic-factory.md`,
cards P35-2, P35-3, P35-5, P35-6, P35-9), which on 2026-09-29 is opened and unbuilt
(docket `b17f73d`; `ls ../rack-cli/docs/contracts/` → `config-v1 harness-v1 operator-v1`).

**Decide:** approve that the docket grammar speaks contract 1.1 when the installed docket
offers it — task from a file, questions on stdout answered on stdin, a token file, the
caller's token bound and policy, a recipe chosen by the agent profile, the result's file list
— and that its capabilities change from `cancel: Advisory`, `decisions: Unsupported`,
`artifacts: Advisory` to `Supported` on each, one captured fixture per claim. Approve two
changes to the core that this needs and that touch settled decisions: the crate-wide cancel
ceiling becomes a per-harness fact, and "no docket pods" is read as "Tack never names,
provisions or polls a pod", which a `--recipe` flag does not.

**Why now:** Phase 65's U8 did not start because docket shipped no `harness-v1.1` (M3,
2026-09-20). docket has since decided to ship exactly what M3 asked (ADR 0017 §2) and is
building it. Tack's adapter still reads one line and passes neither the policy nor the budgets
(`crates/tack-runner/src/harness/docket.rs:113,189-224`), so docket enters Tack as the poorest
of four harnesses when it is the only governed one.

**If you do nothing:** docket stays `Unsupported` for decisions, its cancellation stays a hope,
its files are never listed, and the one harness that can run a multi-role pipeline runs one
agent for Tack.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | `--contract 1.1` is passed whenever the installed docket reports it; the fixtures under `crates/tack-runner/src/harness/fixtures/docket/<version>/` are **captured** from that binary with provenance files, never copied from `../rack-cli`. | The "no copied docket source or fixtures" row stands. docket's own fixtures are its oracle; Tack's are Tack's. |
| 2 | **`cancel: Supported` is a per-harness fact, not a crate constant.** `PROCESS_GROUP_CANCEL_CEILING` (`harness/mod.rs:156`, `Advisory`) is replaced by `HarnessDescriptor::reports_process_groups: bool`; the registry guard (`mod.rs:290-306`) accepts `Supported` only from a descriptor that says `true`, and the crash matrix proves it for that harness. codex, claude-code and opencode keep `false` and `Advisory`. | ADR 0066 already said it: "if — and only if — that mechanism exists and the crash matrix demonstrates termination, this adapter may declare `Supported`; the registry guard makes an unproven claim fail". The constant made that impossible for every harness at once. |
| 3 | **`--recipe` is one flag on one subprocess.** Tack passes `tool_policy.docket.recipe` from the agent profile when set. It never names a pod, provisions one, reads `DOCKET_HOME` after the run, or polls anything; it reads the same last result line, which may now carry a `task` block, kept as evidence in `terminal_reason`. | The refused item was Tack managing docket pods through the retired bridge (ADR 0060/0068). docket's ephemeral in-place pod (ADR 0017 §3) is docket's implementation detail behind its contract. |
| 4 | **The task goes by file.** `--task-file <scratch>/task.md` replaces `/dev/stdin`, and `prompt()` writes nothing, because docket refuses a stdin task together with `--answers stdin` (P35-5). `LocalProcessHarness::prepare` creates the scratch directory before `invocation` so a grammar may write there. | stdin becomes the answer channel, the one Tack decided for every harness (ADR 0068 amendment 2026-09-18). |
| 5 | **Asking is `--answers stdin`**, enabled only when the request's `permission_policy.approvals` is `ask`; `refuse` stays otherwise. The `approval_requested` event becomes a `Question` whose options are `accept` and `decline`; the answer is one `AnswerLine` in docket's shape (the MCP elicitation result). The run token travels in the question's metadata from the event envelope. | Conforms to the Decided row on stdio; no HTTP channel. |
| 6 | **The caller's limits are passed:** `budgets.tokens` → `--max-tokens`; `permission_policy.network == false` → a `--policy` file that denies `fetch`; the tool list is passed only once docket publishes the `kind: policy` document shape (plan question Q3). Everything not passed is declared in the capability reason, never silent. | ADR 0068 amendment §5: "a policy one harness ignores is declared, not silent". |
| 7 | **`artifacts: Supported` from the result's `files` list**, cross-checked by the runner's own git capture (ADR 0071, B1). `.tack-runner/` entries are dropped. | ADR 0017's verdict table: "Supported: `files` in the result (a diff is still the caller's to capture)". |
| 8 | Process events `process_started`/`process_exited` are mapped to two new `StreamSignal` variants; the core tracks live groups and kills them after the main group on cancel; evidence reports `tracked / killed / survived`. | The mechanism decision 2 requires. |

## Evidence

| Fact | Locator (read at `7718420`, docket at `b17f73d`) |
|---|---|
| The adapter passes `--task-file /dev/stdin` and reads only the last line | `crates/tack-runner/src/harness/docket.rs:148-149`, `:164-187` |
| Capabilities today | `docket.rs:189-224` |
| Policy and budgets not applied, by design note | `docket.rs:113-115` |
| `PROCESS_GROUP_CANCEL_CEILING = Advisory`, guard rejects any `Supported` | `crates/tack-runner/src/harness/mod.rs:156`, `:290-306`; `bootstrap.rs:160-198` |
| The child runs in its own process group; cancel signals that group only | `crates/tack-runner/src/harness/process.rs:211`, `:433-449` |
| The core's ask loop, proven once with the fake harness | `local_process.rs:942-1070`; `local_process/tests.rs:462` |
| The `Question`/`StreamSignal` types | `crates/tack-runner/src/engine.rs:40-59` |
| The agent profile's `tool_policy` is opaque JSON edited raw | `crates/tack-db/src/migrations.rs:1398-1406`; `frontend/src/features/agents/runnerFleet/AgentProfilesPanel.tsx:46` |
| `budgets.tokens` is already in the frozen fixture | `docs/contracts/runner-v1/claim.response.json:24` |
| docket's side: contract 1.1, `--answers stdin` refuses a stdin task, `files`, `--token-file`, `--max-tokens`, `--policy`, `--recipe` | `../rack-cli/docs/adr/0017-…md` §1–3; `../rack-cli/TODO.md` P35-2, P35-3, P35-5, P35-6, P35-9 |
| M3's four items and U8's gate | `docs/plans/phase-65.md`, M3 and U8 |

## What this reverses or amends

| Earlier decision | Now |
|---|---|
| "No docket pods" (harness plan, Refused by name) | **Kept, clarified** (decision 3). |
| "No copied docket source or fixtures" | **Kept** (decision 1). |
| `PROCESS_GROUP_CANCEL_CEILING` as a crate-wide `Advisory` (Part IX, `mod.rs:146-156`) | **Amended** (decision 2): the ceiling is a descriptor fact proven per harness. |
| ADR 0066 measured invocation: `--task-file /dev/stdin` | **Amended** (decision 4) for contract 1.1 only; a 1.0 docket keeps the old shape until it is uninstalled — the adapter branches on the probed version. |
| ADR 0066 amendment 2026-09-18: "docket declares `decisions: unsupported` until its harness mode prints a question and reads the answer from stdin" | **Fulfilled** (decision 5). |

## Cut and deferred

| Item | Status | Why |
|---|---|---|
| `resume: Supported` | Cut | docket defers reattachment (ADR 0017 "Cut and deferred"); Tack declares `Unsupported` everywhere. |
| An HTTP decision channel | Cut | Decided row; docket cut it too. |
| Reading the `task` block's hops into Tack's timeline as events | Deferred | Trigger: the evidence-v1 docket ships in Phase 36; until then the block stays in `terminal_reason`. |
| Passing the tool list as a docket policy | Deferred to Q3 | The document shape is not in the contract. |

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-10-03 — accepted by the maintainer, with two amendments.**

1. **docket is negotiated, never pinned.** docket is in continuous development and Tack must
   work with its next release (`v0.2.0-beta.4`, named from docket's tags and `[Unreleased]`
   changelog) and the ones after it. Decision 1's "whenever the installed docket reports it"
   becomes a probe at runner boot: the highest harness contract Tack knows that the binary
   accepts, and one probe per optional flag. Every capability line is derived from the probe.
   Fixtures are keyed by contract version (`fixtures/docket/contract-1.1/`) with a provenance
   file naming the docket commit; they are captured from a scratch install of docket's
   `develop`, never from the operator's install and never copied. No task waits for a docket
   release — only for the docket card that ships its flag.
2. **No interim.** Decision 6's "`network == false` → a `--policy` file that denies `fetch`;
   the tool list is passed only once docket publishes the shape" is withdrawn. docket already
   publishes the shape (`docs/contracts/config-v1/policy.schema.json`); A4 writes a full
   `kind: policy` document when the docket under test accepts `--policy`, and passes nothing
   before that. The capability is `Supported` or `Unsupported` with its reason; there is no
   `Advisory` half. Plan question 3 is closed.
