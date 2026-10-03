# Plan: Phase 66 — close the Level 3 loop: evidence, briefs, escalation packs, merge-readiness

**Status: open 2026-10-03.** The maintainer decided Wave 0 that day (table below): ADR 0069,
0070 and 0071 accepted, ADR 0072 accepted with the interface as a priority, ADR 0073 withdrawn
as an ADR and kept as two tasks. Nothing is dispatched yet; Wave 1 has no gate left. Written against `develop` at
`7718420`; every locator below was read on that commit and re-checked on 2026-10-02 (same
HEAD). Re-run `rg -n` before trusting a line number.

**The re-cut of 2026-10-02**, in one paragraph. Same work, cut so that two agents at a time —
Sonnet, and Haiku wherever the shape is already written here — finish it with fewer resumes
and fewer tokens: the four migrations land together first (S0), so no task waits on another
only for a number in `migrations.rs`; the three tasks that crossed four crates and the
frontend in one agent (C1, H2, G1) are split at the layer boundary, the regenerated schema
being the hand-off; the docket chain follows docket's own delivery order (P35-2 and P35-3 are
merged in docket's `develop`, P35-5, P35-6 and P35-9 are not), so the contract flag and
`cancel: Supported` no longer wait for `--recipe`; the one file two Wave-3 tasks both edited
(`agent-runners.md`) has one writer per wave; and every brief is a section extracted from this
file, never the file. ADR 0072 and 0073 were read against the tree; what the maintainer then decided
about them is in Wave 0.

What this phase builds, in one paragraph. Today an attempt ends with a log: the runner deletes
the workspace and stages nothing else (`crates/tack-runner/src/engine.rs:627,698`;
`crates/tack-runner/src/harness/local_process.rs:758-790`), the item carries no acceptance
criteria (`crates/tack-core/src/models.rs:103-135`), a question from a harness carries options
but no recommendation (`crates/tack-db/src/migrations.rs:1523-1541`), nothing measures the
minutes a human spends deciding, and the result of the work — merged, closed, reverted — is
never seen (`crates/tack-api/src/github_sync.rs` reads issues only). After this phase every
attempt leaves a patch, both commits and a file list; an item can carry a typed brief that
travels to the harness and to an independent verifier; a question arrives as a pack with
options and a recommendation; a verifier's Merge-Readiness Pack (MRP) is rendered and
accepted or rejected with a reason; a branch is pushed and its pull request is followed to
merged, closed or reverted; and one page shows escalation rate, human minutes, the
verification tax in tokens and the MRP acceptance rate. docket's contract 1.1 is adopted when
docket ships it, which revives Phase 65's U8.

The business plan this serves is `../rack-cli/internal-docs/plan-fabrica-agentica-l3-l5-2026-09-29.es.md`
(§3 contracts, §4 the L3 column, §5 Phases 0–1). Its decision that verification belongs to a
separate tool (working name **Assay**, `../assay`, which does not exist on 2026-09-29) is taken
as given here; Tack builds the surfaces and the seam, never the judge.

## What was verified before planning (2026-09-29, `7718420`)

| Claim | Read at | Holds? |
|---|---|---|
| Tack reaches docket only by spawning `docket harness run --task-file /dev/stdin …` and reads the last stdout line | `crates/tack-runner/src/harness/docket.rs:148-149`, `:164-187` | yes |
| docket capabilities: `cancel: Advisory`, `resume: Unsupported`, `decisions: Unsupported`, `artifacts: Advisory`, `usage: Advisory`, policy/budgets `Unsupported` | `docket.rs:189-224` | yes |
| The workspace is deleted after the terminal report; only the run log is staged as an artifact; the artifact pipeline already accepts `text/x-diff` | `engine.rs:627`, `:698`, `:1417`; `local_process.rs:758-790`; `docs/contracts/runner-v1/artifact.request.json` | yes. `submit_terminal_evidence` (`engine.rs:942-957`) uploads exactly one `artifact` key |
| `execution_decisions` (kind, prompt, options, metadata, answer, expiry) and `DecisionInbox.tsx` exist and work for claude-code | `migrations.rs:1523-1541`; `frontend/src/shared/runWithAgent/DecisionInbox.tsx`; `claude_code.rs:347-411` | yes. `kind` already exists on the wire (`decision.create.request.json`); options carry `option_id` and `label` only |
| The item has no acceptance criteria, risk or definition of done; custom fields exist | `models.rs:103-135`, `:666-745` | yes |
| `status_map_policy_id` is threaded everywhere and read nowhere | `crates/tack-api/src/handlers/decisions.rs:16-22`; `crates/tack-orch/src/execution/types.rs:357` | yes |
| GitHub: issues by polling, no pull requests, no checks, no merge detection, no webhook receiver | `github_sync.rs:67,109,192,227,365`; `docs/GITHUB-SYNC.md:12-16` | yes. `crates/tack-api/src/webhook.rs` is an *outbound* client, not a receiver |
| Only claude-code applies `budgets` (`cost_usd`) | `claude_code.rs:259-261`; `codex.rs:528`; `opencode.rs:423`; `docket.rs:113` | yes. The frozen `claim.response.json:24` already carries `budgets.tokens`; the UI sends `budgets: {}` (`frontend/src/shared/runWithAgent/shared.ts:135`) |
| docket ships no `harness-v1.1`; Assay does not exist | `ls ../rack-cli/docs/contracts/` → `config-v1 harness-v1 operator-v1`; docket HEAD `b17f73d` opens Phase 35 with P35-1..10 all `TODO`; `ls ../assay` → absent | yes. Plan against the cards; dependency marked per task |
| `target/` is ~52 GB | `du -sh target` | yes; not touched |

Two facts the brief did not list and the plan depends on:

- **The item's own text never reaches the harness.** The prompt is the agent profile's
  `instructions`, verbatim (`local_process.rs:527`; `frontend/src/shared/runWithAgent/shared.ts:131`;
  `RunWithAgentModal.tsx:399`). Title and description are not in it. C3 fixes this together
  with the brief, server-side, so the CLI and MCP paths get it too.
- **No harness may declare `cancel: Supported` today**, whatever it does:
  `PROCESS_GROUP_CANCEL_CEILING` is a crate-wide constant (`crates/tack-runner/src/harness/mod.rs:156`,
  guard at `:290-306`). A3 makes the ceiling per-harness evidence, which ADR 0070 records.

Re-checked 2026-10-02, same `7718420`: the last migration is still `077`; `../assay` is still
absent; docket moved — its `develop` (`f394be2`, 2026-09-29) has `docs/contracts/harness-v1.1/schema.json`
and `tests/fixtures/harness-contract/v1.1/` (`answer-lines`, `asked-answered`,
`cancelled-process`, `ok-files`, `recipe-ok`), P35-1..4 merged and unpushed, P35-5/7/8 ready
to claim, P35-6 and P35-9 queued; the installed `docket` is `0.2.0b1` from a venv launcher
(`~/.local/bin/docket`) and `docket harness run --help` shows no `--contract`. Tack's
descriptor struct is `HarnessDescriptor` at `crates/tack-runner/src/harness/local_process.rs:69`,
a `&'static` table of facts, not in `mod.rs`.

## Decided — not re-opened by any task

Phase 65's table stands in full. Rows this phase touches, and how:

| Decided | Where | This phase |
|---|---|---|
| A run can pause and ask on every harness; each harness uses it when its CLI offers a stdio way; only stdio fits the seam | ADR 0068/0066 amendments | **Conforms.** A2 is docket's stdio way (`--answers stdin`). |
| No plugin seam, no descriptors from a file, no grammar DSL, no remote docket, **no docket pods**, no new `TACK_*` variable for a harness, **no copied docket source or fixtures** | harness plan, "Refused, by name" | **Kept, with two clarifications in ADR 0070:** `--recipe` is one flag on one subprocess and Tack never names, provisions or polls a pod; every docket fixture under `fixtures/docket/contract-1.1/` is *captured* from the installed binary with a provenance file, never copied from `../rack-cli`. Nothing here adds a `TACK_*` variable: the verifier and push settings are `[verify]` and `[git]` tables in the runner's TOML. |
| GitHub push is best-effort and fire-and-forget; Tack-initiated last write wins on the way out; inbound sync polls, no webhook receiver | `docs/GITHUB-SYNC.md`; phase-65 decision | **Kept and extended, ADR 0071:** opening a pull request is one more best-effort outbound call; its state arrives through the poll that already runs. |
| DAG-ordered sprint dispatch is gone; one operator; unsigned desktop bundles; `tack.toml` never reads the runner gate; providers are modules | ADR 0068, 0059, 0062, 0058, 0063 | untouched (see Parked) |

Decisions this plan takes because no task can start without them. Each is one line to reverse:

- **Nothing of an attempt survives as a local branch.** The workspace is a throwaway `git init`
  plus fetch (`crates/tack-runner/src/git.rs:3-8`), not a worktree of the operator's clone, so a
  branch there dies with it. The durable evidence is the patch, `base_commit`, `head_commit`
  and the file list (B1); the durable *branch* is the one H1 pushes.
- **The evidence excludes `.tack-runner/`**, the directory `stage_run_log` writes inside the
  workspace before evidence is captured (`local_process.rs:770-772`).
- **`budgets.tokens` is the one key a harness reads for a token bound.** It is already in the
  frozen fixture (`claim.response.json:24`); docket maps it to `--max-tokens` (A1). `cost_usd`
  stays claude-code's.
- **A docket recipe is chosen by the agent profile**, as `tool_policy.docket.recipe` — an
  opaque JSON the profile already carries (`migrations.rs:1398-1406`; the panel edits it raw,
  `AgentProfilesPanel.tsx:46`). No column, no migration.
- **Human minutes start when the operator sees the thing.** The UI reports `viewed_at` once per
  decision and per MRP; the metric falls back to `created_at` and says which it used. Never a
  side effect on a `GET`.
- **`status_map_policy_id` has three values:** `null`, `done_on_success`, `done_on_mrp_accepted`.
  Anything else is refused at create. A move goes through `update_item_atomically`, the way the
  GitHub poll moves items (`github_sync.rs:10-14`), and never calls the outbound push.
- **The verifier runs in the runner, after the attempt, behind `[verify] enabled = false`.**
  It receives the evidence directory and the still-present workspace; a clean re-checkout is
  its own job. The board never executes anything.
- **Media types:** the evidence manifest is `application/vnd.tack.evidence+json`, the MRP is
  `application/vnd.tack.mrp+json`, the patch is `text/x-diff` (kind `patch`, as the fixture).
- **Tack drafts `mrp-v1` and `evidence-v1`** as consumer/producer contracts with fixtures.
  Assay adopts them or supersedes them with a version bump; until it exists, the fixtures are
  the only oracle, and F1 is tested against a fake verifier that emits them.
- **The four migrations of this phase land together, first (S0), before any task reads them.**
  `078_item_briefs`, `079_execution_decisions_pack`, `080_mrp_reviews`, `081_pull_requests`,
  each exactly as its consumer task spells it. A table a wave ahead of its repository is
  harmless in SQLite; a shared 1 691-line file edited in four waves is a serial dependency on
  a number. A consumer that finds a column wrong adds `082_…` and reports; it never edits S0's.
- **`docs/book/src/user-guide/agent-runners.md` has one writer per wave.** The task named in
  the wave table writes it; every other task puts its paragraph, ready to paste, in its
  report, and the integrating session pastes it. The docket chain's paragraphs wait for A5.
- **docket is negotiated, never pinned (decided 2026-10-03).** At runner boot the docket
  grammar asks the installed binary what it speaks: the highest harness contract Tack knows
  that the binary accepts, and one probe per optional flag (`--answers`, `--token-file`,
  `--max-tokens`, `--policy`, `--recipe`). Each capability line is derived from that probe, so
  the same Tack build is correct against `0.2.0-beta.3`, `0.2.0-beta.4` and whatever follows,
  and a docket upgrade needs no Tack release. Fixtures are keyed by **contract** version
  (`fixtures/docket/contract-1.1/`), not by binary version, each with a provenance file naming
  the docket commit that produced it.
- **The docket under test is a scratch install, not the operator's.** Agents build docket's
  `develop` into `/var/tmp/tack-measure/docket-venv` (`python -m venv` then `pip install
  ../rack-cli`, the commit recorded) and run it with a scratch `DOCKET_HOME`. The operator's
  `~/.local/bin/docket` is never touched, nothing is copied from `../rack-cli`, and no A task
  waits for a docket release — only for the docket card that ships its flag.
- **Nothing is passed to docket as an interim.** A flag is used when the probe finds it and the
  full document or value can be written; otherwise the capability reads `Unsupported` with the
  reason. No partial policy, no stand-in.
- **A deferred capability is shown disabled, never hidden (ADR 0072, decided 2026-10-03).**
  Every step of the flow — brief, approvals, verification, pushed branch, pull request — has
  its control in the "Run with agent" dialog from P1 on. A control whose integration has not
  landed, or that the selected runner does not attest, is rendered disabled with one line
  saying why and what enables it. The task that lands the integration enables the control.

## Wave 0 — decided by the maintainer, 2026-10-03

| # | What | Decision |
|---|---|---|
| 0.1 | ADR 0069 — the brief is a first-class entity | **Accepted.** |
| 0.2 | ADR 0070 — docket contract 1.1; cancel as per-harness evidence; `--recipe` is one flag | **Accepted**, with two amendments: docket versions are handled dynamically (below), and nothing is passed to docket as an interim — a feature is used when the docket under test has it, and not before. |
| 0.3 | ADR 0071 — evidence, the MRP, the verifier boundary, the pushed branch and its pull request | **Accepted.** The runner may push a branch with the operator's own git credentials; off by default. |
| 0.4 | ADR 0072 — the "Run with agent" flow | **Accepted, as a priority.** The interface and the clarity of the whole flow come first. A capability that is deferred is still shown in the form, visibly disabled, with the reason; it is never hidden. That includes a verification checkbox, disabled until the verifier integration is complete. Tasks P0, P1, P2. |
| 0.5 | ADR 0073 — installation guidance | **Not an ADR; withdrawn as one.** The work stays: the pitch and the voice make the desktop app the default and say what `tack serve` is for (R1), and the existing landing page gets a content refactor and the release download links (R2). |
| 0.6 | Which docket | **No install gate.** docket is in continuous development and is about to cut its next beta; Tack is built to be compatible with that version and the ones after it. Named from docket's own tree on 2026-10-03: tags run `v0.2.0-beta.1` → `.3`, `pyproject.toml` says `0.2.0-beta.3`, and Phases 34–35 sit under `[Unreleased]`, so the next is **`v0.2.0-beta.4`**. Tack never pins a docket version: it negotiates the contract and probes each flag (A1). |
| 0.7 | docket's `--policy` | **No interim.** A4 writes a full `kind: policy` document in the shape docket already publishes (`../rack-cli/docs/contracts/config-v1/policy.schema.json`: `kind`, `name`, `then` required) once the docket under test accepts `--policy`; until then nothing is passed and the capability says so. |

## The waves

A wave is a set of tasks with no source file in common. Generated files
(`docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`) are exempt: the merge driver
resolves them and the post-merge hook regenerates (`scripts/git-merge-generated.sh:1-9`).
`crates/tack-db/src/migrations.rs` is edited by S0 alone. A task starts when the tasks it
names are in `develop` (Wave 0 is decided; no gate is left there). Two agents build at once, so a wave of
four is two rounds; within a wave the task on the longest remaining chain is dispatched first
— the order in each cell.

| Wave | Tasks, in parallel (longest chain first) | Starts after | Model |
|---|---|---|---|
| **1** | **B1** evidence before deletion · **P0** measure each harness's tool list · **S0** the four migrations · **I1** `status_map_policy_id` resolves · **E1** `mrp-v1` contract · **C1a** brief types and contract · **R1** README pitch · **R2** landing page | now | B1, I1, P0, R2 Sonnet · S0, E1, C1a, R1 Haiku |
| **2** | **P1** the whole flow in the dialog, deferred controls disabled · **C1b** brief persistence, routes, export · **D1** CRP contract and board · **F1** verifier step | P1: P0 · C1b: C1a, S0 · D1: S0 · F1: B1, E1 | Sonnet |
| **3** | **C3** brief travels · **E2** MRP review record · **D2** CRP in the runner and inbox · **C2** brief editor | C3: B1, C1b, D1, P1 · E2: E1, I1, S0 · D2: B1, D1 · C2: C1b | Sonnet |
| **4** | **H1** runner pushes the branch · **E3** MRP panel | H1: F1 · E3: E2 | Sonnet |
| **5** | **H2a** pull request: open, follow, record · **P2** verification and push become live controls | H2a: H1, E2 · P2: P1, F1, H1 | Sonnet |
| **6** | **G1a** factory metrics endpoint · **H2b** pull-request badge | G1a: D1, E2, H2a · H2b: H2a | G1a Sonnet · H2b Haiku |
| **7** | **G1b** factory metrics page | G1a | Haiku |
| **A** | **A1** docket negotiated, contract 1.1 → **A3** process events, `cancel: Supported` → **A2** docket asks → **A4** limits, policy and files → **A5** recipe (serial, this order) | A1: now · A3: A1, B1 (docket P35-3 is merged) · A2: A1, docket P35-5 · A4: A1, docket P35-6 · A5: A4, docket P35-9 | A1–A4 Sonnet · A5 Haiku |

```
S0 ──┬──► C1b ──┬──► C3                           H2b (6)
     │          └──► C2                            ▲
     ├──► D1 ───┬──► C3                            │
     │          ├──► D2                            │
     │          └──► G1a (6)                       │
     ├──► E2 ──┬──► E3 (4)                         │
     │         └──► H2a (5) ──► G1a (6) ──► G1b (7)
     └──► H2a
C1a ──► C1b
I1 ───► E2
E1 ──┬──► F1 ──► H1 ──► H2a
     └──► E2
B1 ──┬──► F1
     ├──► C3
     ├──► D2
     └──► A3
docket P35-2 ✓ ──► A1 ──► A3 ──► A2 ──► A4 ──► A5
                        (P35-3 ✓) (P35-5) (P35-6) (P35-9)
P0 ──► P1 ──┬──► C3          R1, R2: now, alone
            └──► P2 (also after F1, H1)
```

Why this order. S0 takes `migrations.rs` off the critical path: in the 2026-09-29 cut D1
waited on C1, E2 on D1 and H2 on E2 only to take the next number, which put E2 in Wave 3 and
the metrics behind five serial merges. B1 still owns `engine.rs` first and every later runner
task hangs off what it stages (C3 writes the brief into it, F1 feeds it to the verifier, H1
pushes what it committed, A3 adds to the same `run_claimed`); `engine.rs` has one owner per
wave: B1 (1), F1 (2), D2 (3), H1 (4), A3 (A, never beside 2–4). The route registration files
(`handlers.rs`, `router.rs`, `openapi.rs`) are C1b (2), E2 (3), G1a (6) — one per wave, which
is why E2 is not in Wave 2 beside C1b. `runner_protocol.rs` is I1 (1), D1 (2), H2a (5);
`attempt_lists.rs` is D1 (2), H2a (5); `repo.rs` is C1b (2), H2a (5), G1a (6); the runner-v1
pin table is D1 (2), C3 (3); `AttemptList.tsx` is E3 (4), H2b (6); `execution/api.ts` is
D2 (3), E3 (4); `agent-runners.md` is C3 (3), E3 (4), G1b (7), then A5. C2 moved from Wave 2
to Wave 3 because it needs C1b's regenerated schema, not C1a's types. The docket chain floats
on docket's board, not on a docket release: A1 and A3 need only what docket has merged, so
they start now against the scratch install; A2, A4 and A5 each wait for one docket card. A3
edits `engine.rs`, `process.rs` and `local_process.rs`, so it runs beside Wave 6 or 7, or
alone — not beside P2, which owns `engine.rs` in Wave 5. The interface is first: P0 and P1 run
in Waves 1 and 2, and C3, the other task that edits `RunWithAgentModal.tsx`, comes after P1
and fills the brief's slot in the flow P1 laid out. R1 and R2 touch no file any task touches
(the README; another repository), so they run in Wave 1.

Counted: 15 tasks in the first cut are 23 now (S0, C1a/C1b, H2a/H2b, G1a/G1b, A4, A5, P0, P1,
P2, R1, R2), 7 of them Haiku and 16 Sonnet. The three tasks most likely to pass the 150-call
ceiling and be resumed (C1 with fifteen files in four crates and the frontend, H2 with ten, G1
with eight) are gone, and `migrations.rs`, the one file four waves shared, is edited once.

## Aligned with docket's board

Read on 2026-10-03 from `../rack-cli` at `f394be2` (`TODO.md`, `ROADMAP.md`,
`specs/api/harness-mode.spec.md`). docket's next release is `v0.2.0-beta.4`; the rows below
do not wait for it.

| docket | State there | Tack task | What Tack does when it lands |
|---|---|---|---|
| P35-2 — contract 1.1, `--contract`, schema, fixtures | merged (Wave 71) | **A1** | negotiates the contract; task by file |
| P35-3 — `process_started` / `process_exited` | merged (Wave 71) | **A3** | tracks groups; `cancel: Supported` on evidence |
| P35-5 — `--answers stdin` | Wave 72, ready to claim | **A2** | `decisions: Supported` |
| P35-6 — `files`, `--token-file`, `--max-tokens`, `--policy` | Wave 73 | **A4** | passes limits and a full policy; `artifacts: Supported` |
| P35-9 — `--recipe` | Wave 74 | **A5** | one flag from the agent profile |
| P35-10 — docket's close of Phase 35 | Wave 75 | — | re-run the docket real-binary tests against the closed tree; re-capture fixtures if a shape moved |
| Phase 36 — operator-v1.1 (question `kind`, options with `description`, `risks`, `estimatedTokens`, `recommendation`), `consult` tool, evidence-v1 in harness results | planned, not open | D1, D2, B1 (Tack's side, built now) | Parked: map docket's pack onto Tack's when Phase 36 ships (see Parked) |
| Phases 37–38 — verification recipes, the L4 envelope, `kind: autonomy` | planned | none | Parked with the ODD view |

The two packs are the same idea with two spellings, on purpose kept apart until docket's is
published: Tack's decision option is `description`, `risks`, `estimated_tokens` and the
recommendation `option_id`, `rationale`, `evidence_refs` (D1); docket's is camelCase. The
mapping lives in the docket grammar, one function, when Phase 36 exists.

## How a task is handed to an agent

Exactly as `docs/plans/phase-65.md`, "How a task is handed to an agent" — the same eight
brief lines, the same limits (two agents building at once, `--build-jobs 4 --test-threads 4`,
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<task>`, one gate once, review against the
task's *done when*, `--no-ff` merge, `renice` from the launcher's loop, no GUI). What this
phase adds, each line against a measured cause of Part IX's burn (5.9 B cache-read tokens on
mechanical work, 13 agents over 500 calls, one card resumed three times):

- **The brief is the task's section, extracted — never this file.** The launcher cuts it with
  `awk '/^### <id> /{p=1;next} /^### |^## /{p=0} p' docs/plans/phase-66.md` and pastes it into
  the brief after the eight lines. An agent that opens this file reads 800 lines it does not
  need, once per resume. The brief names the ADR the task serves and says "do not open it":
  the section already carries the decision.
- **Haiku when the shape is written here; Sonnet when the agent has to choose it.** A task is
  Haiku when every type, column, flag and test name it needs is spelled in its section, it
  edits at most five files in one layer, touches no hot handler, no `engine.rs` and no
  migration it has to design, and its RED test is named. S0, E1, C1a, H2b, G1b, R1 and A5
  qualify; nothing else does. A Haiku agent that reaches 60 tool calls stops and reports — at
  that size, 60 calls means the brief was wrong, not the model.
- **Ceilings by size, reported.** Haiku tasks: 60 calls. Sonnet tasks of one layer: 100.
  Sonnet tasks of two layers (C3, D2, H2a, A3): 150. The agent reports its count at the end;
  one that hits the ceiling hands off and is replaced fresh with a shorter brief, never
  resumed with a large context.
- **Split at the layer boundary; the regenerated schema is the hand-off.** C1, H2 and G1
  crossed `tack-core` → `tack-db` → `tack-api` → frontend in one agent; each is now two tasks,
  the second starting from `schema.gen.ts` as the first regenerated it. An agent that finds
  itself editing the other layer stops: that is a finding.
- **Only the test binary being changed, while working.** `cargo nextest run --workspace -E
  'binary(<name>)'`; the frontend with `npx vitest run <file>`. The full gate is the one
  `.githooks/pre-push` at the end, and once more by the integrator on the merged tree — never
  per branch in between.
- **Docs are pasted, not merged.** One writer per wave for `agent-runners.md` (the Decided
  row); every other task's paragraph travels in its report, and the integrator pastes it.
- **Mechanical edits are the integrator's.** A rename, a comment trim, a pasted paragraph, a
  version bump: done directly or by script, never dispatched.

Two lines added to every brief of this phase:

```
Every contract change is a fixture first (docs/contracts/<name>/), then the type that reads it, then the pin.
A number in a doc or a capability reason carries the command that produced it; never estimate a dollar.
```

## The tasks

### B1 — evidence before deletion, for every harness

**Files:** `crates/tack-runner/src/evidence.rs` (new) and `evidence/tests.rs`;
`crates/tack-runner/src/lib.rs` (`pub mod evidence`); `crates/tack-runner/src/workspace.rs`
(one trait method `WorktreeProvisioner::capture_evidence(&self, workspace, exclude) ->
Result<Option<GitEvidence>, WorkspaceError>` with a default of `Ok(None)`, and the delegating
`WorkspaceManager::capture_evidence`); `crates/tack-runner/src/git.rs` (the implementation on
`GitWorktreeProvisioner`, through the existing `git_ok`, so redaction and the timeout stay
in one place); `crates/tack-runner/src/engine.rs::run_claimed` (one call before each of the
two cleanups that follow a terminal outcome, `:627` and `:698`; never before `fail_before_spawn`'s,
nothing ran) and `::submit_terminal_evidence` (`:942-957`: reads `terminal_reason.artifacts`
as a list, keeping `artifact` for the log); `docs/contracts/evidence-v1/schema.json`,
`README.md`, `attempt-evidence.example.json`; `crates/tack-runner/tests/evidence_contract.rs`
(the example round-trips through the Rust type and its FNV pin, the way
`runner_contract/fixtures.rs` pins).

What is captured, in the workspace, before it is deleted, regardless of harness kind:

1. `git add -A -- . ':(exclude).tack-runner'` (the clone is disposable; staging is how untracked
   files and deletions enter one diff);
2. `changes.patch` = `git diff --cached --binary <base_revision>`, capped at 8 MiB with a
   `truncated: true` flag in the manifest rather than a silent cut;
3. `files.json` = `git diff --cached --name-status <base_revision>` as `[{path, op}]`,
   `op ∈ added|modified|deleted|renamed`;
4. `head_commit` = `git rev-parse HEAD` (the harness may have committed), `base_commit` =
   `workspace.base_revision`, `worktree_dirty` = the staged diff against `HEAD` is non-empty;
5. `evidence.json` — `{v: "1", attempt_id, harness_kind, base_commit, head_commit,
   worktree_dirty, files, patch: {sha256, size_bytes, truncated}, brief: null, branch: null,
   terminal_reason, usage}`. `brief` is filled by C3, `branch` by H1.

Each of the three is staged through `harness::artifact::ArtifactStager` from the scratch
directory (never inside the workspace) with kind `patch` / `files` / `evidence`, and uploaded
by the existing manifest-then-content path. A git failure or an unavailable provisioner yields
`evidence.json` with `captured: false, reason` and no patch — the attempt's outcome never
changes. Cancellation captures too: a partial diff is evidence.

**Done when:** in `engine/tests.rs` a fake-harness attempt whose harness writes one file and
deletes one yields three artifacts in the fake protocol's record, the patch's sha256 equals
the uploaded bytes, `files.json` has two entries with the right `op`, and the workspace
directory is gone afterwards; an attempt with no change stages a 0-byte patch and `files: []`;
a cancelled attempt stages what it had; the example fixture round-trips and its pin holds;
`.githooks/pre-push` passes.

**RED:** `evidence/tests.rs::a_changed_workspace_yields_a_patch_and_a_file_list` (a real
temp `git init` repo; skips with a named reason when `git` is absent) fails at the base because
the module does not exist.

**Model:** Sonnet.

### S0 — the four migrations of this phase, together

**Files:** `crates/tack-db/src/migrations.rs` (four entries appended to `all_migrations()`,
in this order, with exactly these shapes and the file's timestamp convention `TEXT NOT NULL
DEFAULT (datetime('now'))`: `078_item_briefs` — `CREATE TABLE item_briefs (item_id TEXT
PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE, acceptance TEXT NOT NULL DEFAULT '[]',
constraints TEXT NOT NULL DEFAULT '[]', definition_of_done TEXT, risk TEXT, created_at,
updated_at)`; `079_execution_decisions_pack` — `ALTER TABLE execution_decisions ADD COLUMN
recommendation TEXT` and `ADD COLUMN viewed_at TEXT`; `080_mrp_reviews` — `CREATE TABLE
mrp_reviews (attempt_id TEXT PRIMARY KEY REFERENCES execution_attempts(id) ON DELETE CASCADE,
artifact_id TEXT NOT NULL, verdict TEXT, reason TEXT, viewed_at TEXT, reviewed_at TEXT,
reviewed_by TEXT)`; `081_pull_requests` — `CREATE TABLE pull_requests (attempt_id TEXT
PRIMARY KEY, repo TEXT NOT NULL, number INTEGER NOT NULL, url TEXT NOT NULL, state TEXT NOT
NULL, opened_at TEXT NOT NULL, merged_at TEXT, closed_at TEXT, reverted_by_number INTEGER,
UNIQUE(repo, number))`); `crates/tack-db/tests/migrations.rs` (one assertion per new table
and column through `PRAGMA table_info`, in the test that already applies every migration on
a fresh pool). No repository, no model, no handler: the readers are C1b, D1, E2 and H2a.

Written before the tables are read, on purpose (the Decided row). The shapes are copied from
C1b, D1, E2 and H2a; a consumer that needs a different one adds `082_…`, it never edits these.

**Done when:** the four assertions pass; `grep -o '"0[78][0-9]_[a-z_]*"'
crates/tack-db/src/migrations.rs | sort -u | tail -1` prints `081_pull_requests`; `cargo nextest
run -p tack-db` is green; pre-push passes.

**RED:** the four `PRAGMA table_info` assertions fail at the base (no such table or column).

**Model:** Haiku. Four statements spelled above, one source file, one test file.

### I1 — `status_map_policy_id` moves the item

**Files:** `crates/tack-core/src/workflow.rs` (`pub enum StatusMapPolicy { DoneOnSuccess,
DoneOnMrpAccepted }`, `FromStr` over the two ids, and `fn target_status(&self, workflow:
&WorkflowConfig, event: StatusMapEvent) -> Option<String>` using `find_first_done_status`;
unit tests beside it); `crates/tack-api/src/handlers/executions.rs::create_execution` (an id
that is not one of the two → `400 invalid_request` naming the field); `crates/tack-api/src/handlers/runner_protocol.rs::submit_completion`
(after `complete_execution` succeeds and `terminal_state == "succeeded"`, resolve the policy
from the request snapshot and move the item with `update_item_atomically`, logging a refused
transition at `warn`, never failing the completion); `crates/tack-api/src/handlers/decisions.rs:16-22`
(the header paragraph now says what resolves it); `crates/tack-cli/src/mcp.rs:612` and
`crates/tack-cli/src/execution.rs` (help text names the two ids); `crates/tack-api/tests/runner_protocol/`
(one completion test: `done_on_success` → item in the first Done status; `null` → untouched;
and one create test: `"anything_else"` → 400); `docs/openapi.json` via `./scripts/regen-generated.sh`
if a description changed. `DoneOnMrpAccepted` is defined here and acted on by E2.

**Done when:** the three tests pass; `tack execution create --help` (or the MCP tool
description) lists the two ids; `git grep 'read back nowhere'` finds nothing; pre-push passes.

**RED:** the completion test fails at the base (the item never moves).

**Model:** Sonnet (the completion handler is hot; the core enum alone would be Haiku).

### C1a — the brief: types, validation, rendering and the contract (ADR 0069)

**Files:** `crates/tack-core/src/models.rs` (`ItemBrief { item_id, acceptance:
Vec<AcceptanceCriterion>, constraints: Vec<Constraint>, definition_of_done: Option<String>,
risk: Option<Risk>, created_at, updated_at }`; `AcceptanceCriterion` tagged by `kind`:
`command { run, expect_exit: u8, cwd: Option<String> }`, `test { name, runner: Option<String> }`,
`metric { name, op: lte|gte|eq, threshold: f64, unit: Option<String> }`, `file { path }`,
`absent { path }`, `manual { text }`; every variant has `id: String` (client-chosen, unique
within the brief) and `title`; `Constraint` tagged by `kind`: `forbidden_path { glob }`,
`allowed_dependency { name }`, `max_changed_files { n }`, `note { text }`; `Risk = low|medium|high`;
`UpsertItemBrief` with `validator` limits: ≤ 50 criteria, ≤ 100 constraints, `run` ≤ 2 000
chars, ids unique); `crates/tack-core/src/brief.rs` (new, pure: `validate`, and
`render_markdown(item: &Item, brief: &ItemBrief) -> String` — one heading per section, one
line per criterion with its `id`, `manual` criteria last under "needs a human"); `docs/contracts/brief-v1/schema.json`,
`README.md`, `example.json` (pinned by a test in `crates/tack-core/src/brief.rs` that
round-trips the example), and `crates/tack-core/src/lib.rs` (`pub mod brief`).

Why an entity and not custom fields is ADR 0069's question; the task does not re-argue it.
Nothing here touches the database or a handler: that is C1b.

**Done when:** the example round-trips byte-for-byte after normalisation; a duplicate
criterion id fails `validate`; `expect_exit` above 255 is unrepresentable (`u8`); the schema
example pin holds; `render_markdown(example)` is a stable snapshot with `manual` last;
`cargo nextest run -p tack-core` is green; pre-push.

**RED:** `brief.rs::the_example_round_trips_and_its_pin_holds` fails at the base (no module).

**Model:** Haiku. Every variant and limit is spelled above; one crate, three files, one fixture.

### C1b — the brief: persistence, routes, export (ADR 0069)

**Files:** `crates/tack-db/src/repo/briefs.rs` (get, upsert, delete) and `repo.rs`;
`crates/tack-api/src/handlers/briefs.rs` (`GET`/`PUT`/`DELETE /api/items/{id}/brief`, 404 when
the item is missing, 404 on `GET` when no brief), `handlers.rs`, `router.rs`, `openapi.rs`;
`crates/tack-api/src/handlers/export.rs` (the project export carries `briefs`, the import
restores them — a backup that loses briefs is a data-loss bug, not a later slice);
`crates/tack-api/tests/handlers/briefs.rs`; `./scripts/regen-generated.sh`. The table is S0's `078_item_briefs`; the `400` on a bad body comes from
`tack_core::brief::validate`, not from a second validator.

**Done when:** `PUT` then `GET` round-trips `docs/contracts/brief-v1/example.json`
byte-for-byte after normalisation; a duplicate criterion id → 400; deleting the item deletes
the brief; export → import on a fresh database restores it; generated files regenerated, not
edited; pre-push.

**RED:** `tests/handlers/briefs.rs::put_then_get_round_trips_the_example` fails at the base
(no route).

**Model:** Sonnet (export/import and the handler conventions are judgement; the types are C1a's).
Ceiling 100.

### E1 — the `mrp-v1` contract, drafted by the consumer

**Files:** `docs/contracts/mrp-v1/schema.json` (JSON Schema 2020-12), `README.md` (who
produces it, who reads it, how Assay supersedes it), `fixtures/ready.json`,
`fixtures/not-ready.json`, `fixtures/manual-only.json`, `fixtures/verifier-failed.json`;
`crates/tack-core/src/mrp.rs` (new: the types, `render_markdown` for a pull-request body,
`summary_line`), `crates/tack-core/src/lib.rs`; `crates/tack-core/tests/mrp_contract.rs`
(each fixture deserialises, re-serialises to the same canonical JSON, and its FNV pin holds).

The pack, minimum fields (ADR 0071 §3 has the rationale):

```
{ v: "1", attempt_id, evidence_sha256, brief_sha256 | null,
  criteria: [{ id, kind, status: passed|failed|manual|skipped, evidence_ref, note }],
  verify:  { command, exit_code, output_tail, duration_s } | null,
  mutation:{ scope: "changed_lines", score, killed, survived } | null,
  static_analysis: { sarif_sha256, counts: { error, warning, note } } | null,
  judge:   { model_family, blind: true, rubric: [{ criterion_id, verdict: pass|fail, reason }] } | null,
  risk:    { tier: low|medium|high, reasons: [] },
  recommendation: { decision: merge|review|reject, rationale },
  usage:   { tokens_in, tokens_out },        // the verifier's own spend
  signature: { kind: "none" } | { kind: "in-toto", statement_sha256 },
  produced_by: { program, version } }
```

`null` means "not run", never "passed". Every number is measured by the producer; the pack
carries no dollar field.

**Done when:** four fixtures validate against the schema (the pin test also runs
`jsonschema`-free structural checks through the Rust types); `render_markdown(ready.json)`
is a stable string snapshot; README names the three consumers (E2's route, E3's panel, H2a's
pull-request body) and the one producer.

**RED:** `mrp_contract.rs::every_fixture_round_trips` fails at the base (no module).

**Model:** Haiku. The pack's fields are spelled above and in ADR 0071 §3; the agent types
them, it does not design them. `render_markdown` is one heading per section and one table
row per criterion, snapshot-tested, so any wording choice is the snapshot's.

### C2 — the brief editor

**Files:** `frontend/src/features/item-detail/tabs/BriefTab.tsx` and `BriefTab.test.tsx`;
`frontend/src/features/item-detail/ItemDetailDrawer.tsx:20-29` (one tab row, `brief`, after
`details`); the API client where `FieldsTab` gets its own (`frontend/src/shared/api/`), typed
from the regenerated `schema.gen.ts`; `frontend/e2e/brief.spec.ts` (new, one journey);
`docs/book/src/user-guide/items.md` (a "Brief" section: the six criterion kinds, `manual`
as last resort, what "definition of done" is for).

The editor is a list of criteria with a kind selector and per-kind fields, a constraints list,
a definition-of-done textarea, a risk selector. `manual` is visually marked as the kind that
costs a human. Save is one `PUT`.

**Done when:** Vitest: adding a `command` criterion shows its expected exit; save sends the
typed shape; validation errors from the server render beside the field; Playwright: create an
item, add two criteria, reload, both persist; pre-push (Vitest, typecheck, build).

**RED:** `BriefTab.test.tsx::save_sends_the_typed_brief` fails at the base (no component).

**Model:** Sonnet.

### D1 — the consultation pack: contract and board

**Files:** `docs/contracts/runner-v1/decision.create.request.json` (additive: each option
gains optional `description`, `risks: []`, `estimated_tokens`; the request gains optional
`recommendation: { option_id, rationale, evidence_refs: [] }`; `kind` already exists);
`crates/tack-orch/tests/runner_contract/fixtures.rs:5` (the pin) and `protocol.rs` if it
validates the decision shape; `crates/tack-db/src/repo/execution.rs::create_execution_decision`
(stores `recommendation` in the column S0's `079` added) and a new
`mark_execution_decision_viewed` (first write wins, over S0's `viewed_at`);
`crates/tack-api/src/handlers/runner_protocol.rs::create_decision` (accepts the new fields;
`recommendation.option_id` must name one of `options` → else 400; sizes bounded by
`limits.json`'s existing decision bounds — extend `limits.json` only if a new bound is needed,
and pin it); `crates/tack-api/src/handlers/attempt_lists.rs:168-197` (`DecisionOptionSummary`
+3 optional fields; `DecisionSummary.recommendation`, `.viewed_at`); `crates/tack-api/src/handlers/decisions.rs::routes`
(`POST /attempts/{attempt_id}/decisions/{decision_id}/viewed`: idempotent, principal
required, no decision token — it resolves nothing); `crates/tack-api/tests/runner_protocol/decisions.rs`
(create with an unknown recommended option → 400; viewed twice → first timestamp kept;
poll response unchanged); `./scripts/regen-generated.sh`.

**Done when:** the pin table has the fixture's new hash and the fixture is the only thing
that changed shape; the three tests pass; `docs/openapi.json` shows the new fields; pre-push.

**RED:** the unknown-recommended-option test fails at the base (accepted, stored as-is).

**Model:** Sonnet. Ceiling 100.

### F1 — the verifier step in the runner (ADR 0071)

**Files:** `crates/tack-runner/src/config.rs` (a `[verify]` table: `enabled: bool = false`,
`program: String = "assay"`, `args: Vec<String> = ["verify"]`, `timeout_seconds: u64 = 1800`;
`deny_unknown_fields` like `ProviderFileConfig`; no environment variable); `crates/tack-runner/src/verify.rs`
(new: builds `<program> <args…> --evidence <scratch>/evidence --workspace <workspace>
--output <scratch>/evidence/mrp.json`, runs it as a `ProcessSpec` under the runner's process
limits and the configured timeout, with the attempt's `SecretMaterial` for redaction and an
environment of `PATH` only; reads the output through `tack_core::mrp` — a pack that does not
parse is a failure with a bounded stderr prefix; stages `mrp.json` with kind `mrp`);
`crates/tack-runner/src/engine.rs::run_claimed` (after `capture_evidence`, only when
`terminal_state == Succeeded` and `evidence.captured`; a failure submits one
`attempt.verify_failed` event and never changes the terminal state); `crates/tack-runner/src/bootstrap.rs`
(wires the config); the `doctor` output in `crates/tack-runner/src/main.rs` (prints the
`[verify]` table as read); `crates/tack-runner/src/harness/fixtures/fake_verifier.sh` (copies
`docs/contracts/mrp-v1/fixtures/<TACK_FAKE_VERIFIER_FIXTURE>.json` to `--output`, or exits
`TACK_FAKE_VERIFIER_EXIT_CODE`); `verify/tests.rs`; one row in `engine/tests.rs`;
`docs/CONFIG.md` (the table, and the sentence "the board never runs it").

**Done when:** with `enabled = true` and the fake verifier, a succeeded fake-harness attempt
uploads four artifacts, the fourth `application/vnd.tack.mrp+json` with the fixture's bytes;
with the fake exiting 1, the attempt still completes `succeeded` and one event names
`verify_failed` with the exit code; with the default config nothing is spawned and `tack
runner doctor` prints `verify: disabled`; a program that is not on `PATH` is a `warn` at boot
and `verify_failed` per attempt, never a crash; pre-push.

**RED:** `verify/tests.rs::a_fixture_pack_is_staged_as_an_artifact` fails at the base.

**Model:** Sonnet.

### C3 — the brief travels: rendered text to the harness, structured file to the evidence

**Files:** `crates/tack-orch/src/execution/types.rs::ExecutionRequestSnapshot` (`brief:
Option<serde_json::Value>`, `#[serde(default, skip_serializing_if = "Option::is_none")]`);
`docs/contracts/runner-v1/claim.response.json` (the snapshot in the example gains a `brief`
built from `docs/contracts/brief-v1/example.json`) and the pin in `fixtures.rs`;
`crates/tack-api/src/handlers/executions.rs::create_execution` (server-side, for every client:
`instructions` becomes profile instructions + a rendered section with the item's title and
description + `tack_core::brief::render_markdown` when a brief exists; the structured brief
goes into the snapshot; the rendering marks the item text as data from `item.source`, since
`ItemSource::is_trusted` has no caller in any handler today — a finding, not a fix);
`crates/tack-runner/src/evidence.rs` (writes `brief.json` from the snapshot into the evidence
directory and fills `evidence.json.brief`); `crates/tack-api/tests/handlers/` (an execution
created for an item with a brief snapshots it and its instructions end with the rendered
criteria; an item without a brief still carries its title and description — the behaviour
change is stated in the test name); `frontend/src/shared/runWithAgent/shared.ts:131` and
`RunWithAgentModal.tsx` (the preview shows what the server will send; nothing is composed
client-side any more); `docs/book/src/user-guide/agent-runners.md` (what the harness receives).

**Done when:** the two HTTP tests pass; a fake-harness run in `engine/tests.rs` with a brief in
the request stages `brief.json` and `evidence.json.brief` equals it; the claim fixture's pin is
updated in the same commit as the fixture; pre-push.

**RED:** the "instructions carry the item's title" HTTP test fails at the base — today they do
not (`RunWithAgentModal.tsx:399`).

**Model:** Sonnet.

### D2 — the consultation pack in the runner and the inbox

**Files:** `crates/tack-runner/src/transport.rs:710-720` (`DecisionOption` +3 optional
fields; `DecisionCreateReport.recommendation: Option<Recommendation>`), the wire test beside
it; `crates/tack-runner/src/engine.rs:40-46` (`Question.recommendation`) and `::open_decision`
(copies it, scrubbed with the prompt like `metadata`); `crates/tack-runner/src/harness/claude_code.rs::signal`
(fills `description` from the `control_request`'s tool input when the captured fixture shows
one; otherwise `None` — nothing invented); `crates/tack-runner/src/harness/local_process/tests.rs::a_question_is_answered_and_the_run_continues`
(one more row: a fake `ASK:` line carrying a JSON pack — extend `fake_harness.sh`'s `ask`
mode to echo `TACK_FAKE_HARNESS_ASK_JSON` when set); `frontend/src/shared/runWithAgent/DecisionInbox.tsx`
and its test (a `kind` badge; per option its description, risks and estimated tokens; the
recommended option preselected and labelled with the rationale and evidence refs; on first
render of a pending decision, one `viewed` call); `frontend/src/shared/execution/api.ts`,
`types.ts`. The "pausing to ask" paragraph for `docs/book/src/user-guide/agent-runners.md`
goes in the report, ready to paste — C3 writes that file in this wave.

**Done when:** the fake-harness row shows the recommendation reaching the fake protocol; Vitest:
recommended option preselected, `viewed` called exactly once per decision id across re-renders;
the two claude-code fixture rows still pass; pre-push.

**RED:** the fake-harness row fails at the base (`Question` has no `recommendation`).

**Model:** Sonnet. Ceiling 150 (runner wire and inbox).

### E2 — the MRP review record and routes

**Files:** `crates/tack-db/src/repo/execution.rs` (`get_mrp_review`, `mark_mrp_viewed`,
`record_mrp_review` over S0's `mrp_reviews` — a second review is a 409, the first is the
record); `crates/tack-api/src/handlers/mrp.rs` (new: `GET /api/executions/{id}/attempts/{n}/mrp`
reads the newest artifact with media type `application/vnd.tack.mrp+json` from artifact
storage, parses it with `tack_core::mrp`, returns `{pack, review}`; 404 when none; `POST
…/mrp/viewed` idempotent; `POST …/mrp/review { verdict: accept|reject, reason }` — `reason`
required and non-blank; on `accept`, if the request's `status_map_policy_id` is
`done_on_mrp_accepted`, move the item through `update_item_atomically` (I1's enum));
`handlers.rs`, `router.rs`, `openapi.rs`; `crates/tack-api/tests/handlers/mrp.rs` (upload
`ready.json` through the runner artifact route with the media type, then `GET` returns it
parsed; review with a blank reason → 400; a second review → 409; accept under the policy
moves the item; accept without the policy does not); `./scripts/regen-generated.sh`.

**Done when:** the five tests pass; `docs/openapi.json` carries the three routes; pre-push.

**RED:** `tests/handlers/mrp.rs::get_returns_the_uploaded_pack_parsed` fails at the base.

**Model:** Sonnet.

### E3 — the MRP panel

**Files:** `frontend/src/shared/runWithAgent/MrpPanel.tsx` and `MrpPanel.test.tsx`;
`frontend/src/shared/runWithAgent/AttemptList.tsx` (mounts the panel for an attempt whose
artifact list has an `mrp` kind, beside `DecisionInbox`); `frontend/src/shared/execution/api.ts`;
`frontend/e2e/execution-attempt-detail.spec.ts` (one more test: upload `ready.json` as the
artifact, open the attempt, the criterion map has one row per criterion, accept with a reason,
the review shows); `docs/book/src/user-guide/agent-runners.md` (a "Reviewing a merge-readiness
pack" section).

The panel, top to bottom: the recommendation and risk tier; the criterion map (id, title,
kind, status, evidence ref); verify (command, exit, tail); mutation; static analysis counts;
the judge's rubric; then accept / reject with a required reason. A `null` section renders
"not run", never a green tick. On first render of an unreviewed pack, one `viewed` call.

**Done when:** Vitest covers the four fixtures (each renders without a fabricated pass);
accept is disabled until the reason is non-blank; `viewed` once; Playwright passes on
Chromium; pre-push.

**RED:** `MrpPanel.test.tsx::a_null_section_says_not_run` fails at the base (no component).

**Model:** Sonnet.

### H1 — the runner pushes the branch (ADR 0071)

**Files:** `crates/tack-runner/src/config.rs` (a `[git]` table: `push_branches: bool =
false`, `branch_prefix: String = "tack/"`, `author: String = "Tack Runner <tack-runner@localhost>"`);
`crates/tack-runner/src/git.rs` (`publish_branch(workspace, branch, author, message,
secrets)`: `git checkout -b <branch>`; `git -c user.name=… -c user.email=… commit -m
<message>` only when the index B1 staged is dirty; `git push origin <branch>`; all through
`git_ok`, the remote is the one `provision` fetched from, so the operator's own credential
helper or SSH agent answers — Tack holds no git credential); `crates/tack-runner/src/workspace.rs`
(trait method `publish_branch` with a default `Ok(None)`; manager delegate);
`crates/tack-runner/src/engine.rs::run_claimed` (after F1's verify, only on `Succeeded`
with a captured, non-empty patch; the result `{branch, head_commit, pushed}` goes into
`evidence.json.branch` and into `actual_execution.additional["git"]` so the completion report
carries it — additive, the completion fixture is unchanged); `crates/tack-runner/src/evidence.rs`
(the field); `git/tests.rs` (a temp bare repository as `origin`); `docs/CONFIG.md`.

Branch name: `<prefix><item short id>-a<attempt_number>`. Commit message: the item title and
the attempt id, nothing from the untrusted description.

**Done when:** with `push_branches = true` and a local bare origin, a succeeded fake-harness
attempt leaves `tack/<id>-a1` on origin at the commit the report names; with the default
config no push happens and `evidence.json.branch` is `null`; a failed push is one `warn`
event and the attempt still completes; pre-push.

**RED:** `git/tests.rs::a_dirty_workspace_is_committed_and_pushed_to_origin` fails at the base.

**Model:** Sonnet.

### H2a — the pull request: open it, follow it, record it

**Files:** `crates/tack-db/src/repo/pull_requests.rs` (new, over S0's `pull_requests`) and `repo.rs`;
`crates/tack-api/src/github_sync.rs` (`open_pull_request(base, token, repo, head, base_branch,
title, body)` shaped like `push_issue_comment`, best-effort; `poll_repo` reads the entries of
the `/issues?state=all&since=` response that carry a `pull_request` key — the same call Phase 65's G1 already makes — and updates a stored PR's `state`, `merged_at` from `pull_request.merged_at`,
`closed_at`; a PR whose title starts with `Revert` and whose body cites `#<n>` of a stored
merged PR marks `n` as `reverted` with `reverted_by_number`); `crates/tack-api/src/handlers/runner_protocol.rs::submit_completion`
(after a succeeded completion whose `actual_execution.git.pushed` is true and whose item has
a `github_links` row: spawn the best-effort open, body = `tack_core::mrp::render_markdown`
of the attempt's MRP artifact when one exists, else the evidence summary; the target base
branch is the linked repository's default branch, read once per poll and cached like the
etags); `crates/tack-api/src/handlers/attempt_lists.rs` (the attempt summary gains
`pull_request: {number, url, state} | null`); `docs/GITHUB-SYNC.md:17-30` ("Data" gains pull requests; the poll
section gains the PR rows; "out of scope" loses nothing it names); wiremock tests in
`crates/tack-api/tests/handlers/` (the open request's body equals the rendered MRP; a poll
seeing `merged_at` marks merged and no write happens on a 304; a Revert PR marks the original
reverted); `./scripts/regen-generated.sh`. No frontend file: that is H2b.

**Done when:** the three wiremock tests pass; a completion for an item with no GitHub link
opens nothing and logs at `debug`; `docs/openapi.json` shows `pull_request` on the attempt
summary; pre-push.

**RED:** the open-request-body test fails at the base (no call is made).

**Model:** Sonnet (two layers: the poll's state machine and the completion handler). Ceiling 150.

### H2b — the pull-request link and badge on the attempt

**Files:** `frontend/src/shared/runWithAgent/AttemptList.tsx` (a link to `pull_request.url`
and a state badge — `open`, `merged`, `closed`, `reverted` — when the summary carries one;
nothing when `null`) and its test; `frontend/src/shared/execution/types.ts` only if the
regenerated `schema.gen.ts` is not already what the component reads.

**Done when:** Vitest: a summary with `pull_request` renders the link and the badge text; one
with `null` renders neither; typecheck and build pass; pre-push (frontend jobs).

**RED:** `AttemptList.test.tsx::a_pull_request_renders_its_link_and_state` fails at the base.

**Model:** Haiku. One component, one test, the type already regenerated by H2a.

### G1a — factory metrics: the aggregates and the endpoint

**Files:** `crates/tack-db/src/repo/metrics.rs` (new: the aggregates below as SQL over
`execution_requests`, `execution_attempts`, `execution_decisions`, `execution_artifacts`,
`mrp_reviews`, `pull_requests`, per project and `since`); `crates/tack-api/src/handlers/metrics.rs`
(`GET /api/projects/{id}/metrics/factory?since=<rfc3339>`; the response carries every
definition's inputs, not only its ratio, and `null` where a denominator is zero), `handlers.rs`,
`router.rs`, `openapi.rs`; `crates/tack-api/tests/handlers/metrics.rs` (seeded rows → exact
numbers written out in the test); `./scripts/regen-generated.sh`. No frontend file: that is G1b.

Definitions, exactly:

| Metric | Definition | Source |
|---|---|---|
| escalation rate | decisions ÷ attempts | `execution_decisions` ÷ `execution_attempts` in the window |
| human minutes per decision | median and p90 of `resolved_at − coalesce(viewed_at, created_at)`, with counts of which start was used | D1's columns |
| human minutes per MRP | same over `mrp_reviews.reviewed_at − coalesce(viewed_at, artifact.created_at)` | E2 |
| verification tax (tokens) | (verification + rework) ÷ implementation; implementation = tokens of each request's first attempt; rework = tokens of later attempts plus of requests whose `metadata.rework_of` names another request; verification = the sum of every MRP's `usage` | `execution_attempts.usage`, MRP packs |
| MRP acceptance rate | accepted ÷ reviewed; also produced, unreviewed | `mrp_reviews` |
| outcomes | pull requests opened, merged, closed, reverted; PQC = merged and not reverted in the window | `pull_requests` |

No dollar field is read or summed anywhere on this page. `cost_usd` stays where it is.

**Done when:** the HTTP test's numbers match by hand; every `null` carries its reason
(`denominator_zero` or `not_measured`) and every minutes figure names its start (`viewed_at`
or `created_at`); `docs/openapi.json` shows the route; pre-push.

**RED:** `tests/handlers/metrics.rs::seeded_rows_yield_the_expected_ratios` fails at the base.

**Model:** Sonnet (the SQL for rework and the verification tax is judgement). Ceiling 100.

### G1b — factory metrics: the page

**Files:** `frontend/src/features/dashboard/FactoryMetrics.tsx` and its test;
`frontend/src/app/routes.tsx:30` (`/projects/:id/factory`) and the nav entry beside
Overview; `docs/book/src/user-guide/agent-runners.md` ("Factory metrics": each definition
and its source columns, copied from G1a's table).

One card per metric from G1a's response: the ratio, its inputs, "not measured" for every
`null` with the response's reason, and the start-of-timer source beside the minutes. No
computation on the client; the endpoint carries the inputs for that reason.

**Done when:** Vitest: a response fixture with one measured and one `null` metric renders
every card, the `null` as "not measured" and never `0`; typecheck and build pass; pre-push
(frontend jobs).

**RED:** `FactoryMetrics.test.tsx::a_null_metric_says_not_measured` fails at the base.

**Model:** Haiku. One page from a typed response; the definitions are written.

### A1 — docket is negotiated, and speaks contract 1.1 when it can (ADR 0070 decisions 1 and 4, amended 2026-10-03; revives U8)

**Gate:** none. docket's P35-2 is merged in its `develop` (`7b42f86`); the docket under test is
the scratch install (the Decided row). `../rack-cli/docs/contracts/harness-v1.1/schema.json`
and `../rack-cli/tests/fixtures/harness-contract/v1.1/` are read, never copied.

**Files:** `crates/tack-runner/src/harness/docket/probe.rs` (new: `DocketFeatures { contract:
Contract, answers, token_file, max_tokens, policy, recipe: bool, version: String }`, filled
once at runner boot by running the located binary — the first step of the task is to measure,
against `0.2.0b1` and against the scratch install, which probe is reliable and zero-spend: a
refused `harness run --contract 1.1` (exit 2, one `refused` result) is the candidate for the
contract, an unknown-option usage error for each flag; the measured commands and outputs go in
`fixtures/docket/README.md`; a probe that cannot be made reliable is a finding, not a guess);
`crates/tack-runner/src/harness/docket.rs::invocation` (on contract 1.1: `--contract 1.1`, the
task written to `run.scratch.join("task.md")` and passed as `--task-file <path>`, `prompt()`
returning no bytes; on 1.0 the shape ADR 0066 measured, unchanged), `::capabilities` (every
line derived from `DocketFeatures`; the `additional` note names the contract and the docket
version found); `crates/tack-runner/src/harness/local_process.rs::prepare` (one line:
`create_dir_all(&scratch)` before `invocation`, so a grammar may write there);
`docket/tests.rs` (rows: the 1.1 argv; the 1.0 argv, still; a features value with every flag
false yields today's capability lines exactly; a captured `ok` fixture on 1.1 → `Finished`;
the real-binary test negotiates rather than assuming); `fixtures/docket/contract-1.1/`
(captured from the scratch install, with a provenance file naming the docket commit).

**Done when:** both argv rows pass; the same Tack build reports contract 1.0 against the
operator's `0.2.0b1` and 1.1 against the scratch install (`tack runner doctor`, both outputs
in the report); no docket version string appears in a comparison anywhere in `crates/`
(`rg -n '0\.2\.0' crates/tack-runner/src/harness/docket*` finds none); pre-push.

**RED:** the argv row asserting `--contract 1.1` fails at the base.

**Model:** Sonnet. Ceiling 150 (a measurement and a new module).

### A3 — process events and `cancel: Supported` (ADR 0070 decisions 2 and 8)

**Gate:** A1; B1; docket's P35-3 (merged in its `develop`, `e7dc098`), present in the docket under test.
Never concurrent with a wave that owns `engine.rs` (2, 3, 4): beside Wave 5, 6 or 7, or alone.
It runs before A2 because its docket half is already shipped and A2's is not.

**Files:** `crates/tack-runner/src/engine.rs:55-59` (`StreamSignal::ProcessStarted { pgid }`
and `ProcessExited { pgid }` — the type only); `crates/tack-runner/src/harness/process.rs::wait_with_capture_and_questions`
(keeps the set of live pgids from those signals) and `::cancel` (after the main group:
SIGTERM then SIGKILL to each tracked group; `ProcessResult`/`CancelOutcome` report
`groups: { tracked, killed, survived }`); `crates/tack-runner/src/harness/local_process.rs::cancel`
(the evidence `details` carry those counts); `crates/tack-runner/src/harness/mod.rs:156,290-306`
(`PROCESS_GROUP_CANCEL_CEILING` is replaced by a descriptor fact: `HarnessDescriptor::reports_process_groups:
bool` — the struct lives at `local_process.rs:69`; the guard accepts `cancel: Supported` only from a grammar whose descriptor says `true`
— ADR 0070 §2); `crates/tack-runner/src/bootstrap.rs:160-198` (the doctor line); `docket.rs`
(`signal` maps `process_started`/`process_exited`; the descriptor flag; `cancel: Supported`);
`crates/tack-runner/src/harness/fixtures/fake_harness.sh` (a `spawn_detached` mode: `setsid
sleep …` in its own session, then prints a `process_started` line the test grammar maps);
`crates/tack-runner/tests/crash_matrix.rs` (one row: cancel an attempt in `spawn_detached`
mode → the detached grandchild is dead within the grace period and the evidence says
`survived: 0`; and one row without the event → the grandchild survives and the evidence says
so — the honest `Advisory` case); `fixtures/docket/<version>/cancelled-process.ndjson`
(captured). The paragraph for `agent-runners.md` goes in the report; A5 writes it.

**Done when:** both crash-matrix rows pass; the registry accepts docket's `Supported` and
still rejects a test grammar claiming it without the flag; `docs/book/src/user-guide/recovery-runbook.md`
names the new evidence field; pre-push.

**RED:** the `spawn_detached` crash-matrix row fails at the base (the grandchild survives).

**Model:** Sonnet. Ceiling 150 (the one task that edits three runner core files).

### A2 — docket asks (ADR 0070 decision 5; revives U8)

**Gate:** A1; docket's P35-5 (its Wave 72, `TODO` on 2026-10-02) merged and in the docket under test.

**Files:** `docket.rs::invocation` (`--answers stdin` and `stdin_stays_open: true` when
`permission_policy.approvals == Some(Ask)`; `refuse` stays the posture otherwise),
`::signal` (an event line whose `event.type == "approval_requested"` →
`StreamSignal::Question { vendor_id: approvalToken, kind: "tool_permission", prompt: from
`tool` and `callId`, options: `accept` / `decline`, metadata: { token, tool, callId } }`; the
result line → `Finished`), `::answer` (one `AnswerLine`: `{"v":"1.1.0","token":<from
metadata>,"answer":{"approvalToken":…,"action":"accept"|"decline","content":null}}` — the
deny option maps to `decline`), `::capabilities` (`decisions: Supported`, reason quoting the
measured fixture and version); `docket/tests.rs` (signal row from the captured
`asked-answered` fixture; answer row asserting exact bytes; the real-binary test gains a
`#[ignore]`d live row that gates one `bash` call and accepts it); `fixtures/docket/contract-1.1/`. The rewritten "Asking before acting" row goes in the report; A5 writes it.

**Done when:** the fixture rows pass; the "Run with agent" dialog offers "Ask me" for docket
because the runner reports it (`RunWithAgentModal.tsx:321` reads the attestation); pre-push.

**RED:** the signal row fails at the base (`signal` returns `None` for every line).

**Model:** Sonnet. Ceiling 100.

### A4 — the caller's limits and policy go to docket, and its files come back (ADR 0070 decisions 6 and 7, amended 2026-10-03)

**Gate:** A1; docket's P35-6 (its Wave 73) merged and in the docket under test. No part of
this task lands before that: nothing is passed as an interim.

**Files:** `docket/probe.rs` (the `token_file`, `max_tokens` and `policy` probes become real);
`docket.rs::invocation` (each only when its probe is true: `--token-file <scratch>/token.json`;
`--max-tokens` from `budgets.tokens` when it is a positive integer; `--policy
<scratch>/policy.yaml`, a full `kind: policy` document built from `permission_policy` —
`network: false` and the tool list both — in the shape of
`../rack-cli/docs/contracts/config-v1/policy.schema.json`, read there, never copied; if
`permission_policy` holds something that shape cannot express, the run is refused before
spawn with the field named, never silently narrowed), `::report` (`files[]` →
`terminal_reason.files` after dropping `.tack-runner/` entries; `limits.maxTokens` echoed into
the reason), `::capabilities` (`artifacts: Supported` and `permission_policy: Supported` when
the probes say so, `Unsupported` with the reason otherwise — no `Advisory` middle);
`docket/tests.rs` (rows: the argv with every flag; the policy document for a request with
`network: false` and two tools, byte for byte; a captured `ok-files` fixture → the files, none
under `.tack-runner/`; a features value without `policy` passes no `--policy`);
`fixtures/docket/contract-1.1/`.

**Done when:** the rows pass; the generated policy validates against docket's schema in the
real-binary test (docket itself refuses an invalid one); `tack runner doctor` prints the
capability lines for both dockets; pre-push.

**RED:** the argv row asserting `--token-file` fails at the base.

**Model:** Sonnet. Ceiling 100.

### A5 — the recipe is one flag (ADR 0070 decision 3), and the docket rows of the book

**Gate:** A4; docket's P35-9 (its Wave 74) merged and in the docket under test.

**Files:** `docket.rs::invocation` (`--recipe <name>` from
`resolved_agent_profile.tool_policy.docket.recipe` when it is a non-empty string; nothing
else), `::report` (a `task` block in the result line is kept in `terminal_reason`, not read);
`docket/tests.rs` (one argv row, one captured `recipe-ok` fixture row);
`fixtures/docket/contract-1.1/`; `docs/book/src/user-guide/agent-runners.md:101` and `:136`
(the docket rows of "Choosing a harness" and "Asking before acting", plus the paragraphs A1–A4
left in their reports; "claude-code, codex and opencode honour `ask`; docket does not" is
rewritten).

**Done when:** both rows pass; the doc rows say what `tack runner doctor` prints; pre-push.

**RED:** the argv row asserting `--recipe` fails at the base.

**Model:** Haiku. One flag, two rows, and four paragraphs already written.

### P0 — measure what each harness exposes as a tool list (ADR 0072)

**Gate:** none. No product code: a measurement, like Phase 65's M1–M3.

**Files:** `docs/plans/measurements/tools-<harness>.md`, one per harness (claude-code, codex,
opencode, docket), each a captured command and its output: does the CLI list its tools
(`--help`, a `tools` subcommand, an MCP `tools/list`, its row in
`docs/contracts/runner-v1/capabilities.json`), with what names, and is the list stable across
versions. Nothing under `crates/`. Every CLI runs against a scratch `HOME`, zero spend.

**Done when:** four files, each ending in one of three sentences — "lists its tools by
`<command>`", "exposes no list; the names below come from `<doc>`", "no list and no doc" —
and P1's brief is cut to match.

**Model:** Sonnet (it runs the real CLIs and reads what they say). Ceiling 60.

### P1 — the whole flow in the "Run with agent" dialog; what is deferred is shown disabled (ADR 0072)

**Gate:** P0. C3 comes after this task and fills the brief's slot.

**Files:** `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx`, split so no file passes
300 lines: `RunFlow.tsx` (the dialog's body as the flow, top to bottom, one section each:
**Who runs it** — harness, runner, agent profile, model; **What it gets** — the item's text
and its brief; **How far it may go** — approvals, allowed tools, budget, timeout; **What
happens after** — verification, pushed branch, pull request), `Prerequisite.tsx` (one row:
state `ok | missing | deferred`, a label, and for `missing` a link to the page that fixes it —
`/agents` for harness, login and default model; `/projects/:id/settings` for the project's
Agents panel; the agent-profiles panel for a profile — for `deferred` a disabled control and
one line saying why and what enables it), `RunWithAgentModal.test.tsx`;
`frontend/e2e/run-with-agent.spec.ts` (one journey: open with nothing configured, follow the
first link, come back, the row is `ok`). In this task the brief row, the verification
checkbox, the push-branch checkbox and the pull-request row are all rendered `deferred`:
"available when the brief editor lands", "available when a runner reports a verifier",
"available when a runner reports branch push". Allowed tools: a checklist where P0 found a
list for the selected harness, text with P0's sentence as help where it did not. The
repository block shows the project's value first and its fields collapsed. Run is enabled
only when no row is `missing`; `deferred` never blocks. No new route or column.

**Done when:** Vitest: every `missing` row links to the page named above; every `deferred`
row renders a disabled control and its reason, never nothing; "Ask me" stays gated by
`decisionsAttested()`; Run is disabled with one `missing` row and enabled with only
`deferred` ones; Playwright passes on Chromium (headless); pre-push (frontend jobs).

**RED:** `RunWithAgentModal.test.tsx::a_deferred_capability_is_shown_disabled_with_its_reason`
fails at the base (the dialog has no verification control at all).

**Model:** Sonnet. Ceiling 150 (a restructure of a 731-line component).

### P2 — verification and branch push become live controls (ADR 0072; ADR 0071 decisions 4 and 5)

**Gate:** P1, F1, H1. Owns `engine.rs` in Wave 5.

**Files:** `crates/tack-runner/src/bootstrap.rs` (the capability report's `additional` gains
`verify_configured` and `push_configured`, read from `[verify] enabled` and `[git]
push_branches` — additive, the pinned `capabilities.json` fixture is unchanged);
`crates/tack-orch/src/execution/types.rs::ExecutionRequestSnapshot` (`verify: Option<bool>`,
`push_branch: Option<bool>`, both `skip_serializing_if = "Option::is_none"`);
`crates/tack-api/src/handlers/executions.rs::create_execution` (accepts and snapshots them);
`crates/tack-runner/src/engine.rs::run_claimed` (the verifier runs when the runner has one
**and** the request did not say `false`; the push likewise — the runner's TOML decides whether
the capability exists, the request decides whether this run uses it; a request cannot turn on
what the TOML has off); `frontend/src/shared/runWithAgent/RunFlow.tsx` (the two rows are
`ok` with a live checkbox when the selected runner reports the flag, `deferred` with "this
runner has no verifier configured — see `docs/CONFIG.md`" otherwise; the pull-request row
follows the push row and the item's GitHub link) and its test; one row each in
`engine/tests.rs` and `crates/tack-api/tests/handlers/`; `docs/CONFIG.md` (the sentence that
the request can decline, never enable); `./scripts/regen-generated.sh`.

**Done when:** a runner with `[verify] enabled = true` and a request with `verify: false`
spawns no verifier; a request with `verify: true` against a runner without one changes
nothing and the attempt records why; Vitest: the checkbox is live only with the flag reported;
pre-push.

**RED:** the `verify: false` engine row fails at the base (the verifier runs).

**Model:** Sonnet. Ceiling 150 (two layers).

### R1 — the README says the desktop app is Tack, and what `tack serve` is for

**Gate:** none.

**Files:** `README.md` (the opening pitch and "Get started", `:1-15` and `:116-150`: the
desktop app is the default way to use Tack, in the product's voice and in one sentence; one
short paragraph says what the server form is for — a machine with no desktop, a shared or
remote host, automation — and that it is the same binary and the same data; a "Which one do I
have?" list of at most three questions, each answer written from the tree (`docs/CONFIG.md`,
`crates/tack-desktop/src/paths.rs`, the book's "Enrolling a runner"), an answer the tree does
not support is left out and reported); `docs/LAUNCH-CHECKLIST.md` (the release-notes step
says the desktop bundles are the first links). No phase, board or ADR is named in either file.

**Done when:** every path and command in the text exists on the tree (`ls`, `rg`); every
number is re-measured or removed; the docs job of pre-push passes.

**Model:** Haiku. Two files; every fact given or pointed to.

### R2 — the landing page: content refactor and the release downloads

**Gate:** none. Another repository: `../newPortaflio` (the studio site, live at yielab.com),
branch from its `main`, its own checks, never Tack's gate. The maintainer named the page as
`yielab.com/docket`; on 2026-10-03 that route is docket's own landing
(`app/[lang]/docket`, `content/docket-landing.ts`) and Tack has only a product entry
(`content/products/tack*`, under `/products`). The task works on Tack's entry and reports the
question — a dedicated `/tack` landing like docket's is a second task if the maintainer wants
it.

**Files:** `../newPortaflio/content/products/tack*` (the pitch leads with the desktop app and
says what the server form is for, in the voice R1 set; the stale limits are corrected against
Tack's README — the entry still says the latest tag is `0.1.0-beta.7` and that the runner has
not shipped in a release, both false since `v0.1.0-beta.9`; a "Download" link of kind
`release` to `https://github.com/yielab/tack/releases/latest` is added beside "GitHub", first
in the list); the type in `../newPortaflio/content/types.ts` only if `kind: 'release'` does
not exist; that repository's own content check (`scripts/check-content.ts`).

**Done when:** the content check and the build of that repository pass; every claim on the
page is true of `v0.1.0-beta.9` (each checked against Tack's `README.md` and `CHANGELOG.md`);
the download link resolves (`curl -sI`); nothing is deployed — deploying is the maintainer's.

**Model:** Sonnet (voice and a second repository's conventions). Ceiling 100.

## Parked — not scheduled, and why

One line each, with the trigger that turns it into a task.

- **Autonomy by domain (ODD) view.** Trigger: four weeks of G1a baseline exist and at least one
  domain has fifty reviewed MRPs (business plan §4.1). Until then there is nothing to display.
- **Automatic merge authority.** Trigger: the ODD view exists and the human acceptance rate of
  MRPs marked `merge` is measured above the plan's threshold for one domain. Needs its own ADR;
  it reverses "human in every merge".
- **Autonomous intake from signals.** Trigger: an ODD is promoted. It also re-opens
  "DAG-ordered sprint dispatch is gone" (ADR 0068), so it is an ADR first.
- **Multi-user (ADR 0059).** Trigger: a second approver is required by an ODD's policy. Out of
  this plan, as the business plan §8 says.
- **Signed MRPs (in-toto).** `signature.kind = "none"` is valid in `mrp-v1`. Trigger: a pull
  request check consumes the signature, not a human.
- **A verifier as a service, best-of-N, mutation and SARIF producers.** Assay's; Tack only
  renders what the pack carries.
- **Real dollar ingestion from provider usage APIs.** Trigger: a hosted provider is the
  majority of attempts and the operator asks. Never an estimate meanwhile.
- **A review category in workflows.** `StatusCategory` has `Todo`, `InProgress`, `Done`
  (`workflow.rs:43-47`). Trigger: I1's `done_on_success` proves wrong for a real board — an item
  should sit in review while its MRP is open.
- **Enforcing `ItemSource::is_trusted` at dispatch.** Finding from C3: it has no caller.
  Trigger: the maintainer decides what an untrusted item may not do.
- **docket's consultation pack and evidence block (docket Phase 36).** Trigger: docket
  publishes `docs/contracts/operator-v1.1/` and puts evidence-v1 in harness results. Then one
  task maps docket's `consult` question onto Tack's pack in the docket grammar and reads the
  evidence block instead of keeping it opaque in `terminal_reason`.
- **A dedicated `/tack` landing page.** Trigger: the maintainer asks for one after R2.
- **Anything in the "Decided" table.**

## Questions for the maintainer

All six were answered on 2026-10-03; the answers are the Wave 0 table. One is open, raised by
R2 and blocking nothing: whether Tack gets a dedicated landing page like docket's, or its
product entry is the landing.

## Status

| Task | State |
|---|---|
| Wave 0 | decided 2026-10-03 |
| all tasks | not dispatched. Wave 1 (B1, P0, S0, I1, E1, C1a, R1, R2) and A1 have no gate left |
