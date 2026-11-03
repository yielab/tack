# Phase 87: a step that runs out of budget stops at a checkpoint, and a person chooses to continue or start over

**Status: PROPOSED 2026-11-03.** No ADR of its own: the decisions are below. It follows docket's
Phase 59 (docket's ADR 0038, "a step's budget is a checkpoint, not a cliff"). Every change is read
only when docket lists its feature in `GET /health`'s `features`, as `failureKinds` already is, and the
in-process fake (`crates/tack-runner/tests/fake_docket/`) plays the docket before and after each one.
Keeps ADR 0032's rule: the screen shows what the harness reports, and what it cannot do is shown disabled
with its reason.

## The ask, in one screen

A real Implementer step stopped at 20 rounds with 6 % of its tokens spent. Its change was written,
not checked, and not committed. docket now makes running out of a step's budget its own outcome: it
names the bound, keeps the work on the task's branch, has the model write a report, and lets a retry
continue the same step. docket does not decide whether another budget is worth spending; Tack does
(docket ADR 0038, decision 6).

Tack must say which limit stopped the step and where to raise it, show the work and the report, and
offer the person two ways on: **Continue** the same docket task, or **Start over** with a new one,
from the kept work or clean.

## What docket has (read 2026-11-03, docket `develop`; all five P59 cards DONE)

| docket card | Feature | State | What Tack reads |
|---|---|---|---|
| P59-1 | `retryContinues` | Merged, listed | `POST /tasks/<pod>/<id>/retry` re-runs the failed step and continues its session. Before it, a retry skipped the failed step and could settle `done` with unverified work |
| P59-2 | `budgetOutcome` | Merged, listed | `failureKind: "budget"` for any step bound; the hop carries `budget: {bound, limit, used, source, treeChanged}`; **only an Implementer hop's work is committed** (below) |
| P59-3 | none (a `budget` on the hop) | Merged | The budget hop's `artifact.summary` is the model's report; no verdict is read from it |
| P59-4 | none (the pipeline format's spec version) | Merged | A step declares `maxIterations`, `maxToolCalls`, `timeout`; a role declares `maxIterations`, `maxToolCalls`; the pod keeps `turnTimeoutS`; the rounds default is now 60 |

**Field-proven.** docket closed Phase 59 2026-11-03 (`develop` at `02056b87`): all three live
acceptance runs passed on `claude-haiku-5-5` against its reinstalled service. A step with
`maxIterations: 4` stopped with a notice at round 3, one tool-free landing request,
`failureKind: "budget"`, `hop.budget == {bound: "rounds", limit: 4, used: 4, source: "step",
treeChanged: true}`, the report as `artifact.summary` and the work in the patch; retried at
`maxIterations: 12` it finished `done`. A step with `maxIterations: 80, timeout: 1200` finished
`done` at 42 rounds. The live `saas-dev` task from the incident (`task-0a050f9c`) was retried, its
Implementer continued its session, then parked on a destructive ask the operator denied — it ended
`approval_denied`, not a finished task, which is a different, expected end (ADR 0038's own case
table lists it). Card L here can now run for real; the shapes above match this plan exactly, and
the Implementer-only pin is confirmed unchanged.

**Confirmed against docket's source (`tasks/domain/task.py`, `tasks/domain/parts.py`,
`tasks/application/record.py`) and by the docket session directly:**

- The wire names are exactly `bound: rounds | toolCalls | tokens | time`, `limit`, `used`,
  `source: step | role | pod | default`, plus **`task`** for a `tokens` stop when the task itself
  set `maxTokens` (`_budget_outcome`'s own `source = "task" if …`) — one more source value than the
  ADR's four. `treeChanged` is the one camelCase key (pydantic `by_alias=True` on every `/tasks`
  response, confirmed in `interfaces/http/routes/tasks.py`); the rest stay as written.
- **`treeChanged` is narrower than ADR decision 2 reads.** `_keeps_its_work` pins the tree only for
  a hop whose **role is `implementer`** and whose failure is `budget_exhausted`; every other role
  (a Reviewer included) is never pinned, so its hop's `budget.treeChanged` is always `false`.
  docket's own session flagged this as a deliberate narrowing, not yet escalated past "flagged to
  the maintainer" — Tack reads what ships, not what the ADR proposed. This folds cleanly into
  decision 5's table: a non-Implementer budget stop always falls into the `treeChanged: false` row.
- `timeout` is transport/request-only now and still retried in place; the loop's own wall clock is
  `budget_exhausted` with `bound: "time"`, tried once. Matches the ADR as written.
- Retry's continuation message carries the previous stop, the report and up to 20 changed paths,
  as decision 6 assumed.

Two facts from the ADR text still shape the decisions:

- **`POST .../retry` takes no body.** The task's `maxTokens` is task-wide and cannot be raised on a
  retry, so a step stopped by tokens cannot be continued with more.
- **Without `budgetOutcome`, `failureKind: "budget"` means the task's `maxTokens` ran out**, which
  is the only budget docket reported before. Today's reading is right for that docket.

## What Tack does today

1. `harness/docket.rs` `finish()` sends `failureKind: "budget"` to its last arm: code `budget` and
   "docket said: …". The screen's `stoppedByLimit` (`frontend/src/shared/runWithAgent/runLimit.ts`)
   reads code `budget` as the run's token limit. `RunLimitNote` then prints "Stopped at its limit:
   X of Y" in tokens and offers **Run again with more**, which doubles the tokens. For a step stopped
   by rounds or time, the sentence and the offer are both wrong.
2. A failed task's `terminal_reason` has no `branch` or `head_commit`, so
   `capture_attempt_evidence` (`engine/outcome.rs`) falls back to the base commit and the patch is
   empty. The rework path (`tack-app/src/runs/complete/rework.rs`) takes its base branch from the
   run's `additional.git.branch`, which a failed docket run does not have.
3. Tack never calls `POST .../retry`, and the client has no route for it. Every rerun is a new docket
   task: `externalRef` `<item>/attempt-<n>`, `idempotencyKey` the attempt id (`harness/docket/task.rs`).
4. `harness/docket/steps.rs` reads a step's `verdict` from the hop. It shows the last step's
   `summary` as the task's result on a `done` task.
5. The onboarding skill's pipeline (`onboarding-lab/skill/references/blueprint-format.md`) declares
   no step budget.

## Decisions

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **A step that runs out of its budget is its own end, worded by the bound.** With `budgetOutcome`, a `budget` task whose hop's `bound` is `rounds`, `toolCalls` or `time` ends the run with code **`step_budget`**. One whose bound is `tokens` keeps code **`budget`**, the run's token limit, because that limit is Tack's (`maxTokens` from `budgets.tokens`). Without the feature, `budget` keeps today's meaning. `pod_budget`, the team's spent budget that pauses it, does not change | "Stopped at the step's round limit: 20 of 20" or "Stopped at the step's time limit: 300 s of 300 s". "Token limit" appears only when tokens stopped it |
| 2 | **The limit says who set it and where to raise it.** `limit`, `used` and `source` go into the run's reason. `source` becomes one sentence naming the key: `step` → the step's key in the team's pipeline (`maxIterations`, `maxToolCalls` or `timeout`); `role` → the role's `maxIterations` or `maxToolCalls`; `pod` → the team's `turnTimeoutS`; `default` → the machine's default (`AGENT_LOOP_MAX_ITERATIONS`, `AGENT_LOOP_MAX_TOOL_CALLS`, `AGENT_LOOP_WALL_CLOCK_TIMEOUT_S`). The adapter writes the sentence and the screen prints it, as Phase 75 does for the token limit | The person knows which file to edit without reading docket's code |
| 3 | **The work is kept and shown.** With `treeChanged: true`, the run's reason carries the task's `branch`, `head_commit` and `base_commit`, so the engine captures the patch as it does for any run. The run stays **Failed**: nothing verifies, publishes or merges it. With `treeChanged: false`, it carries no head, and the run says the step changed nothing | A stopped run shows its diff; "Failed" no longer means "nothing to see" |
| 4 | **The report is a report.** A budget hop's `summary` is shown under "What the step reported before it stopped". It is never the run's result, never a thread's reply and never a verdict. Tack reads no verdict from a budget hop, even if docket sent one, so a Reviewer that ran out never approves | The report sits beside the diff, labelled as unfinished |
| 5 | **Continue or Start over: the case decides which is offered first.** See the table under "Continue or Start over" | One block of choices on a stopped run, each disabled with its reason when it does not apply |
| 6 | **Continue is a new run that follows the same docket task.** It is a new execution request, like a rework, with `metadata.continues` = the request it continues. The adapter calls `POST .../retry` instead of `POST /tasks/<pod>`. The new run follows the same task id, whose `externalRef` and `idempotencyKey` stay the first run's. Its usage is the task's usage minus what the task had used when the retry was sent. Its token limit is what the first run's limit has left, because docket's `maxTokens` is task-wide and does not change | The item's history reads "Run 1 · stopped at its step limit → Run 2 · continued", and tokens are counted once |
| 7 | **Start over from this run's work sets the base on the server.** The Run dialog gets "Start from this run's work", checked by default when the stopped run kept work. It sends `from_run: <request id>`. The server takes the branch from that run's evidence and sets `metadata.rework_base`, as a rework already does; it never takes a branch from the client. docket gets a new task with `baseRef` = that branch | A clean context that keeps the code, for a step that went down the wrong path |
| 8 | **A pipeline can declare its long step's budget; Tack sends none.** The onboarding skill writes `maxIterations` and `timeout` on the build (Implementer) step when the docket in use accepts them. Tokens stay Tack's: `maxTokens` per task from `budgets.tokens`, as today | A team built for long work says so in its own pipeline, versioned in the repository |

### Continue or Start over

Continue needs both `retryContinues` and `budgetOutcome`. Without `budgetOutcome`, Tack cannot tell a
budget stop from a confused model, and the kept work is not there to continue.

| Case | Offered first | Also offered | Continue, when it is not offered |
|---|---|---|---|
| Bound `rounds`, `toolCalls` or `time`; `treeChanged: true` | **Continue** | Start over from this run's work; Start over clean | — |
| Bound `tokens`; `treeChanged: true` | **Start over from this run's work**, with twice the limit (replaces "Run again with more") | Start over clean | Disabled: "The task's token limit is spent, and continuing cannot raise it." |
| Any bound; `treeChanged: false` | **Edit the task** (the item's description), then Start over clean | Start over clean | Disabled: "The step changed nothing; the task may be unclear. Reword it before running again." |
| A docket without `retryContinues` or `budgetOutcome` | Start over (today's Run again) | — | Disabled: "This team's docket cannot continue a step. Update docket." |
| Any other failure (`hop_failed`, `needs_human`, `prepare`, …) | Today's choices | — | Not shown; Tack never calls `POST .../retry` |

**`treeChanged: true` only ever happens for an Implementer hop** (docket's own `_keeps_its_work`
pins no other role). So the first row is reached only by a build step; a Reviewer or any other
role that runs out of budget always lands in the `treeChanged: false` row. For that role the
"reword the task" framing is still right in spirit (there is nothing of its own to continue), so
Tack's sentence there stays generic rather than naming the role.

**Why Continue is a new request and not a second attempt of the same one.** An attempt is the engine
retrying the same request under the same snapshot (`UNIQUE(request_id, attempt_number)`). Spending
again is a person's choice with its own limit, as a rework and "Run again with more" already are. The
history already chains requests through `metadata.rework_of` (`tack-db/src/queries/execution_requests.rs`).
`continues` joins that query, and the metrics keep counting a rework only by `rework_of`.

**Why not always Start over.** The step's session holds what the model read and decided. In docket's
incident, a continuation runs the checks the first try never reached. Starting over keeps the code,
not that context.

**Restarts.** If the runner restarts while a continuation runs, `resume` follows the same task. A
`409` from `POST .../retry` on a task that is already `pending` or `running` means the retry was
sent, and the run follows it. Tack never sends a second retry.

## Considered and not adopted

- **Continuing automatically** after a budget stop, up to a cap. That spends tokens without a person,
  and the ladder has no rung for it. It can come later as a trigger rule (Phase 85) if runs show it is
  wanted.
- **Starting over every time, with `baseRef` = the old branch.** It always loses the session's
  context, and that context is what makes continuing worth it.
- **Sending step budgets with each task** (`maxIterations` per task). docket keeps that for later
  (docket ADR 0038, "Not decided here"), and the work's size belongs to the pipeline.

## What this asks of docket

None of these blocks a card.

- **P59-4 has no feature name.** A name on `/health` would let the skill and the screen probe it.
  Until then the skill probes by validating a sample step (card S).
- **`POST .../retry` cannot raise `maxTokens`.** An optional `maxTokens` on the retry would let a
  token stop continue. Until then, a token stop only starts over (decision 5).
- **The hop's `budget` names no step id.** Tack names the hop's role. A `step` field would let the
  sentence name the exact key to raise.

## Cards

Each card starts only when docket's `develop` lists the feature it reads, checked with
`curl -s $DOCKET/health | jq .features` against a docket built from source. Each card adds the fake's
switch for "before the feature", adds the feature to `LATER_FEATURES` in `tests/docket_v5.rs`, and
tests both states. Each writes its `[Unreleased]` line in `CHANGELOG.md` and its guide paragraph in the
same change. A card runs only the tests of what it touched; the full suites run once per round.

| Card | Waits on | What |
|---|---|---|
| **A** — read the budget stop | `budgetOutcome` | **Done** (`6dcf4160`, fixed up `17d9309a`). `wire::TaskHop` gains `budget` (`bound`, `limit`, `used`, `source`, `treeChanged`, `raise`) and `error`. `finish()`'s `step_budget` arm reads `budget.raise` first, falls back to parsing docket's own `"(source: X; raise Y)"` tail out of the hop's `error`, and only then to a source-only sentence — so ADR 0038 decision 5's amendment (deleting per-step/role `maxIterations`/`maxToolCalls`) changes nothing here once docket ships it. `steps::step` carries `budget` and `report: true` on that hop and drops its verdict |
| **B** — Continue | `retryContinues` and `budgetOutcome` | **Done** (`a537fc61`, API route `d9f33a9d`). `finish()` writes `next.continue: {possible, why}` for the whole decision-5 table. `POST /api/executions/{id}/continue` (`continue_run.rs`) builds the continuing request beside `rework.rs`, with what the source run's token limit has left (`counted_by_request`). `start()` reads `continue_task` from metadata and sends `POST .../retry` on the same pod/task instead of enqueuing a new one; a `409` is followed, not resent. Usage subtracts the continuation's baseline so it counts only new hops |
| **C** — Start over from this run's work | `budgetOutcome` | **Done** (`964098f3`). `from_run` on `POST /api/executions`. The server resolves the branch from that run's evidence and sets `rework_base`. Refuses a run without kept work, or from another project, with `422` and the reason |
| **D** — the screen | A, B, C | **Done** (`26746f18`). A new `ContinueOrStartOver` block carries the backend's own sentence and the two actions of decision 5, each disabled with its reason; mounted beside `RunLimitNote`, which steps back (its own "stopped" line stays off) whenever `next.continue` is present at all, so a tokens-bound stop is told once, not twice. `stoppedByLimit` reads the last hop's `budget.bound`. Steps with `report: true` read "Report before it stopped", never a verdict. One paragraph each in the User guide and the Administrator guide (`docs/CONFIG.md`). Not done: a dedicated "continued" label on the item's run history (today it is just another request; nothing in the plan's acceptance table requires more than the block above) |
| **S** — the skill (git-ignored `onboarding-lab/`) | P59-4 in docket's `develop` | **Done.** `blueprint-format.md` documents `maxIterations`/`timeout` on the build step as docket-gated. `apply.sh` probes a disposable step with `docket pod validate` (cached per run; `ONBOARD_STEP_BUDGET_PROBE` overrides it in tests) and patches only a temp copy of the team, never the repo's own pipeline file — a hand-rolled line patcher, not `yaml.safe_dump`, because PyYAML 1.1 turns a bare `on:` key into `true` and would silently break every pipeline's rework trigger. `check.sh`: 46/46 (7 new). Still waiting on P59-6: `maxIterations` is a step/role key only until then |
| **L** — live proof | A–D, S; P59-3 | docket closed Phase 59 2026-11-03 (`develop` `02056b87`): all three of its own live runs passed, shapes match this plan exactly. Still to do once D lands: `onboarding-lab/e2e/` exercises Continue and Start-over end to end, and a recaptured task record replaces the fake's hand-written hop and the constructed `post-task-retry.json` fixture in `fixtures/docket/v5/` |

Rounds: A; then B ∥ C; then D; S whenever P59-4 lands; L last.

### Tests, one per case

**A** (`tests/docket_v5.rs`, fake on and off):
- a rounds stop under the feature ends `step_budget` with `limit`, `used` and `source`, and the
  message names `maxIterations` and never "token";
- the same stop from the docket before the feature keeps today's code (`hop_failed`, or `budget`
  meaning tokens);
- a `tokens` bound ends `budget`;
- `treeChanged: true` captures a non-empty patch with the run Failed and no verify or publish;
  `treeChanged: false` has no head;
- a Reviewer budget hop whose report says "APPROVE" shows no verdict, and the run is Failed;
- `pod_budget` is unchanged (the existing `failure_kinds_say_why_…` test).

**B**:
- with both features, Continue sends exactly one `POST .../retry` and no `POST /tasks`, follows the
  task to `done`, and counts only the new hops' tokens;
- without `retryContinues`, `/continue` answers `409` with the reason, and the request log holds no
  `/retry`;
- a `hop_failed` task, a `tokens` bound and `treeChanged: false` each give `possible: false` with its
  `why`;
- a runner restart during a continuation follows the task, and a `409` on retry sends no second one.

**C**:
- `from_run` sets `rework_base`, and docket's body carries `baseRef` = the stopped task's branch;
- a run without kept work, and a run from another project, are each refused with `422`.

**D** (Vitest): one test per row of the table under "Continue or Start over"; `stoppedByLimit` for
`token_limit`, `budget` without a bound, `budget` + `tokens`, and `step_budget`.
