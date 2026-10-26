# Phase 80: a run that waits on a person, and one record for every wait

**Status: built 2026-10-26 on branch `phase-80-impl`, on top of Phase 79; not merged. docket closed its Phase 51 that day (next section), so cards A, C, D, F and G were all built, G folded into D, through the tasks R, RS1, RS2 and BS. B is deferred and E is cut. Proposed 2026-10-25 with one card ready.**

## What changed on 2026-10-26: docket closed its Phase 51

**Read this section first; where it disagrees with the text below, it wins.** docket's `develop`
(`/home/ox/Sites/rack-cli`, version 0.2.0-beta.6, unreleased, commits `2d42ed50`..`9954b4f8`) closed
its Phase 51: every pause is one durable *escalation* record, `POST /escalations/<id>` is the only
route that settles one, and the retired routes now answer `404`. `specs/api/operator-api.spec.md`
(sections "Escalations", "Re-attaching after a restart", "Check changed") is the contract; read it,
never edit docket. What this does to the plan:

| Before | Now |
|---|---|
| Cards C, D, F, G "BLOCKED on docket" | **Released.** P51-2/4/5 (record, list, one answer route), P51-3 (event family), P51-8 (`autonomy`, `failureKind: needs_human`) and P51-9 (old shapes removed) are in docket's tree |
| "Every docket name is a proposal" | The names below are docket's own now; Tack's `not_answered` stays Tack's |
| G runs last, after docket removes the old shapes | **G folds into D**: docket already removed them, so Tack's adapter against docket `develop` is *broken today* for every question (`GET /approvals`, `POST /approvals/<token>`, `POST /tasks/<id>/answer` are 404, the trace events `approval_requested`, `question_asked`, `input_requested`, `input_answered`, `input_expired` no longer exist) |
| Fallback for a docket without escalations | **None.** docket beta.6 is unreleased and the rule is "negotiated, not pinned, no interims": the probe asks `GET /escalations?since=0`; a `404` is the sentence "needs docket 0.2.0-beta.6 with the escalation record". `apiVersion` is still `"6"` on both shapes, so the version number cannot tell them apart |
| Card A reads `expiresAt` from the task view's question and from `GET /approvals` | Both are gone. The wait's deadline is the pending escalation's `expiresAt` (and its `onExpiry`); `blocked` + `blockedReason: input_expired` is in docket's spec, so that name is confirmed, only `not_answered` is Tack's |

**What the retired shapes map to** (docket's table, `operator-api.spec.md` "Escalations"): `GET /approvals` and the
inbox's `needsYou` items → `GET /escalations?since=<seq>&task=<id>` (each record has `id`, `kind`,
`prompt`, `options[]`, `recommendation`, `requestedSchema`, `expiresAt`, `onExpiry`, `policy`, `level`,
`state`, `seq`, `affordances`) and `GET /inbox`'s `escalations` and `decided`; answers →
`POST /escalations/<id>` with `{optionId, content?, actor?}` (`approve_once`, `approve_task`, `deny`,
`decline`); trace events → `escalation_asked`, `escalation_delivered`, `escalation_resolved`,
`escalation_expired`, `escalation_decided`; `POST /tasks/<pod>` takes `autonomy` (`L3|L4|L5`, narrows
only, `422` above the pod profile's ceiling); `failureKind: "needs_human"` ends a task at L5 whose
tool call needs approval, final until `retry`.

**Tasks, sliced by file and run in this order** (one worktree per row; R and BS may run together; RS1 waits on R;
RS2 follows RS1 in the same worktree):

| Id | Does | Owns |
|---|---|---|
| R | Recapture. `scripts/capture-docket-fixtures.sh` stops capturing the retired routes and captures `GET /escalations`, `POST /escalations/<id>` (accepted, unknown id, already settled), the inbox with `escalations[]`/`decided`, and a task view with `escalation`; shapes it cannot produce without a model are copied from docket's own `operator-v2` fixtures (read-only) and marked `constructed` with the docket commit | the script, `fixtures/docket/v5/` and its `README.md` |
| RS1 | Cards D + G + A's rework, runner side: one reader of escalations (cursor `since` kept beside the trace cursor), one `Question` builder, one answer route; the three question builders, the two `Pending` variants, `GET /approvals` and the task-view lookup are deleted; probe asks `GET /escalations?since=0`; the wait's deadline is the pending escalation's `expiresAt` | `harness/docket.rs`, `docket/{client,probe}.rs`, `tests/fake_docket/`, `tests/docket_v5.rs`, runs-and-teams guide step |
| RS2 | Cards C + F, runner side: `autonomy` in the task body (only when the escalation route probed, narrowed to the team's ceiling), `needs_human` ends `failed` with code `needs_human`, a stale probe on `422`, and a timeline line for every `escalation_*` event, `escalation_decided` as kind `escalation.decided` | `harness/docket.rs` (`finish`, `hop_progress`), `docket/task.rs`, the fake, `docket_v5.rs` |
| BS | Cards C + F, board side: the snapshot carries the level to send, the run card offers "Run again at L3" on code `needs_human` (disabled with its reason otherwise), the factory metrics count executions that docket decided alone (events of kind `escalation.decided`) beside, never inside, `escalation_rate` and `human_minutes_per_decision` | the request snapshot (`tack-protocol`), `tack-db/src/queries/` metrics, `tack-api/src/handlers/metrics.rs`, `frontend/src/features/dashboard/FactoryMetrics.tsx` and the run card, `./scripts/regen-generated.sh` |

One gate on the merged tree: `.githooks/pre-push`. The live proof, `scripts/docket-prill-e2e.sh`, is the maintainer's to rerun against docket `develop` once the gate passes.

**Why a phase of its own, and how it sits beside Phase 79.** Phase 79 changes structure and no
behaviour; this phase changes what the docket adapter does and is gated by a partner's acceptance.
Folding one into the other would tie an unblocked refactor to docket's calendar and put behaviour
changes inside a no-behaviour-change series. They do not compete for files: Phase 79 touches
`tack-runner` only in a path rename (`tack_orch::execution` → `tack_protocol::execution`, four
lines across `harness/docket*.rs`) and in the split of `engine.rs`; card A touches
`harness/docket.rs`, `harness/mod.rs` and the fake. **A can run now, in parallel with Phase 79**, on
its own worktree; whichever merges second rebases over a rename. Cards C and F name files Phase 79
moves (`tack-api/src/autonomy.rs` → `tack-app`, the request snapshot → `tack-protocol`,
`handlers/metrics.rs` loses its SQL); their paths are re-read when they are released.

**Where this comes from.** docket's maintainers drafted a "Phase 51" that turns every moment docket
stops for a person (a tool approval, a model `consult`, an `input` step, a gate that needs a human)
into one record, an *escalation*. **That draft is not accepted.** Every docket name below (a card, a
route, a field, an event) is a *proposal* until it is accepted, and each card says "proposed" at the
first use. The agreed rule stands: Tack adapts to docket's contract, and docket never learns Tack's
names. Tack probes at runtime and sends or reads a new field only when docket says it has it, as
Phase 77 K did for `prepare`.

**Done when:** a run parked on a question is neither cancelled nor timed out while a person has not
had the chance to answer; and, once docket ships its cards, the adapter has one reader for
questions, one way to answer, the autonomy level sent per task, what docket decided alone shown,
and no code for the shapes docket removed.

## What docket will offer, and which card uses it

| docket card (proposed) | What Tack can rely on afterwards | Tack card |
|---|---|---|
| P51-1 | pod setting `consultMode: decide`; `lights-out` requires it | none: Tack already reads the team's profile (`probe::capped`) |
| P51-2 | one record per escalation, monotonic `seq` | D reads it |
| P51-3 | one event family `escalation_*`, an OpenTelemetry span whose duration is the wait | F |
| P51-4 | `GET /escalations?since=<seq>` and `escalations[]` on `GET /inbox`, additive | D |
| P51-5 | `POST /escalations/<id>` with `{optionId, content?, actor?}`; old routes stay as adapters | D |
| P51-7 | A2A binding: `message/send`, `tasks/get`, push config, agent card | none (E was cut; see the card) |
| P51-8 | `autonomy: L3\|L4\|L5` on `POST /tasks` (narrows only), `failureKind: needs_human` | C, F |
| P51-9 | removal of the old shapes, only after Tack has moved | G |

## The claims from docket's side, checked in Tack's code (2026-10-25)

| Claim | Verdict | Evidence |
|---|---|---|
| (a) `follow` cancels the task when the attempt's own timeout passes, even while `waiting_input` or `waiting_approval`, and ends the attempt as `timeout` | **True** | `DocketAdapter::follow` sets `deadline` once, on entry, and tests it after the terminal check whatever the task's status is; it then calls `cancel_task` and returns `code: "timeout"`. `pending.is_some()` only adds "while it waited for an answer" to the sentence (`time_ran_out`). The test `a_run_whose_time_ran_out_ends_with_a_message_naming_the_wait` asserts exactly this for an approval gate. |
| (b) docket's deadline is the question's `expiresAt` (72 h for consults and input steps, 24 h for approvals) and on expiry the task goes `blocked` with `blockedReason: input_expired` | **Not confirmed in Tack; half confirmed in docket's code** | Tack reads neither field: `grep -rn 'expiresAt\|blockedReason\|input_expired' crates --include='*.rs'` finds nothing. A `blocked` task ends with code `blocked` and docket's `reason`. The captured approval record has `expiresAt` about 24 h after `created` (`approvals-pending.json`). The captured consult has **`expiresAt: null`** in the inbox and in the task view (`inbox-waiting-input-options.json`, `task-waiting-input-options.json`), so the 72 h is not on any capture. In docket's source (read, not run), `inputExpiryHours` defaults to 72 with a comment that dispatch does not use it yet, the sweep that blocks a task with `input_expired` acts only on a question that has an `expiresAt`, and an expired *approval* is denied by a sweep, not blocked. A consult can therefore have no deadline at all. |
| (c) `follow` polls the inbox every 2 s per attempt | **Partly false** | `POLL_INTERVAL` is 2 s and the loop sleeps *after* its requests. Each pass reads `GET /traces` and `GET /tasks/<pod>/<id>` always, and `GET /inbox` only `if pending.is_none()`. While a question is open the inbox is not read. `GET /inbox` is global, not per task: N attempts make N identical reads. |

## Cards

| Id | Card | Owns | Waits on |
|---|---|---|---|
| A | **A task that waits on a person is not cancelled** | the docket adapter's `follow` and its `blocked` mapping, the fake docket | — |
| B | **Measure polling before replacing it** | one measurement file | **DEFERRED** until a replacement is proposed (see the card) |
| C | **Send the autonomy level per task** | the request snapshot, the docket task body, the run card's action | **BLOCKED on docket P51-8** |
| D | **Read escalations, drop the workaround** | the docket adapter's question reader, the one answer route, fixtures | **BLOCKED on docket P51-2, P51-4 and P51-5** |
| E | ~~One A2A client for docket~~ | — | **CUT 2026-10-25** (see the card) |
| F | **Show what docket decided alone** | the timeline mapping, the factory metrics query, handler and screen | **BLOCKED on docket P51-3 and P51-8** |
| G | **Contract guard** | fixtures, the fake, the client, the probe's floor | **BLOCKED on docket P51-9**, and C, D, F |

**Waves, sized so each file is visited once per wave:**

| Wave | When | Runs | Scope |
|---|---|---|---|
| 1 | now | **A** | `harness/docket.rs` (1 457), `harness/mod.rs` (557), `tests/fake_docket/mod.rs` (1 291), one guide step |
| 2 | one docket build with P51-2/4/5 and P51-8 | **R** recapture: `scripts/capture-docket-fixtures.sh` once, fixtures and `README.md`, nothing else | the fixtures folder |
| 2 | after R | **C+D+F, runner side**: one agent on `harness/docket.rs` (`follow`, `finish`'s `needs_human` arm, `next_question` over `escalations[]`, `hop_progress`'s escalation line), `harness/docket/{client,task}.rs` (the route, the `autonomy` key), `tests/fake_docket/` | ~3 200 lines; split in two by function group if a build ships P51-8 without P51-2/4 |
| 2 | beside the runner side | **C+F, board side**: the snapshot field, the run card's "Run again at L3", `repo/metrics.rs` + `handlers/metrics.rs` + `FactoryMetrics.tsx` and its test, `regen-generated.sh` | ~1 400 lines; no file in common with the runner side |
| 3 | docket releases P51-9 and the live proof passes | **G** | `harness/docket*.rs`, `fake_docket/`, fixtures |

If docket ships its cards in separate builds, wave 2 runs once per build with the same two
slices, each slice taking only what that build released. A card changes only the files it owns;
each writes its own guide paragraph and changelog line, there is no separate docs task. One
gate per wave, on the merged tree: `.githooks/pre-push`.

---

### A. A task that waits on a person is not cancelled

**Status:** ready. **Depends on:** no docket card; it reads `expiresAt`, which docket already sends
on API 5 for approvals and consults, and `blockedReason`, which no capture shows yet.

**Measured trigger.** The table above: (a) is true. A run parked on a question for longer than its
attempt timeout is cancelled and reads as a timeout, so a person who goes to lunch fails the run and
the task. `time_ran_out` already knows a question was open; the code does not. The attempt timeout
is a bound on an *agent's* work, and a parked task is not working. (b) says the deadline of a wait
is docket's `expiresAt`, and Tack reads none.

**Goal.**

1. While docket reports the task `waiting_input` or `waiting_approval`, the attempt's clock does not
   run: the time between two polls that both saw the task waiting is added to the deadline. It
   follows docket's own task status, not Tack's `pending`, which is set only once the question has
   been sent.
2. A wait has a deadline only when docket gives one: the open question's or approval's `expiresAt`,
   read from the same record that made the question (the task view's question; the `GET /approvals`
   record). If the task is *still* waiting at a poll after that time (docket's own sweep has not
   resolved it), cancel the task as today and end the attempt `failed` with a new code (proposed
   `not_answered`) and a sentence that says a person did not answer and when the question expired.
3. When docket itself moves the task to `blocked` with `blockedReason: input_expired` (proposed
   name until a capture shows it), end with the same code and sentence. Any other `blocked` stays
   `blocked`.
4. With no `expiresAt`, nothing ends the wait but an answer, docket's own resolution, or Stop. This
   is the consult case on every capture today; **the maintainer confirms it**, because it means a
   forgotten question holds its run until someone stops it.

**Non-goals.** No expiry shown in the Inbox and no `expires_at` on `Question`. No new setting. No
change to the other agents: their clock is a process timeout and asks nobody. No change to how
docket resolves an expiry. The ordinary timeout of a running task is unchanged.

**Files.** `harness/docket.rs`: `follow` (the deadline), `Pending` and `pending_approval`/`next_question`
(carry `expiresAt`), `finish` (the `"blocked"` arm); `harness/mod.rs`: the sentence beside
`time_ran_out`; `tests/fake_docket/mod.rs`: `Script` or `Gate` learns an `expiresAt` and a `blocked`
task with `blockedReason`; `docs/book/src/developer/runs-and-teams.md`, "Following the task" step 3;
`CHANGELOG.md`. The frontend needs nothing: `attemptFormat.ts` shows `reason.message` before the code.

**Acceptance.** In `crates/tack-runner/tests/docket_v5.rs`:
- `a_question_pending_longer_than_the_attempt_timeout_keeps_the_attempt_alive`: `timeout_seconds = 1`,
  a consult gate and an approval gate in turn, open for at least three polls with `expiresAt` far
  ahead; no `POST .../cancel` is recorded; the answer then ends the attempt `Succeeded`.
- `an_expired_question_ends_with_not_answered`: `expiresAt` a few seconds ahead, nobody answers; the
  attempt is `Failed`, `code` is `not_answered`, the message does not say "time ran out", and one
  cancel is posted.
- `docket_blocking_an_expired_question_ends_with_not_answered`: a `blocked` task with
  `blockedReason: input_expired` gives the same code; a `blocked` task without it still gives
  `blocked`. The fixture is `constructed` and says so in its provenance.
- The approval row of `a_run_whose_time_ran_out_ends_with_a_message_naming_the_wait` is removed or
  rewritten; its `Hold` row stays and still gives `timeout`.

Run: `PATH=$HOME/.cargo/bin:$PATH cargo nextest run -p tack-runner --test docket_v5`.

---

### B. Measure polling before replacing it

**Status: DEFERRED 2026-10-25.** The measurement answers "keep polling or replace it", and nothing
can replace it today: docket's `webhook` channel needs an inbound endpoint, and Tack has none on
the runner's side nor on the board's (ADR 0071). With E cut, no card proposes a replacement. The
measurement is run the day one is proposed, as that proposal's first step, with the threshold
below written before the numbers. Until then the real docket run it needs (N = 20 concurrent
attempts in a throwaway home) is cost without a decision behind it. **Depends on:** a proposal.

**Measured trigger.** None yet, which is the point: a subscription replaces polling only for a
measured cost. From the code, not yet measured: one attempt makes up to three requests a pass
(`GET /traces`, `GET /inbox`, `GET /tasks/<pod>/<id>`) every 2 s plus their round trips, about 90 a
minute, and two while a question is open, about 60; a new question adds one `GET /approvals` or one
task view. N attempts make N identical global `GET /inbox` reads. docket's maintainers say their
`webhook` channel already sends CloudEvents with Standard Webhooks signatures for `needs_you`; Tack
does not use it. A subscription also needs an inbound endpoint, and Tack has none on the runner side
(it runs near credentials) and none on the board's (ADR 0071: "no webhook receiver"), so the price of
the alternative is not only docket's.

**Goal.** A number and a decision, in `docs/plans/measurements/docket-polling.md`, which is written
in the shape of the other files in that folder: the commands first, then the table.

1. Write the threshold first, before running anything. Proposed, for the maintainer to change:
   replace polling when, at the expected N, `GET /inbox` p95 is over 250 ms or docket's daemon uses
   over 10 % of one core.
2. Measure with N = 1, 5, 20 concurrent attempts against a real docket in a throwaway `DOCKET_HOME`
   with the guard of `scripts/capture-docket-fixtures.sh` (no model, no provider key, nothing spent):
   requests per minute for each route, p50 and p95 of `GET /inbox` as the number of tasks grows, and
   the daemon's CPU share. Cross-check the per-attempt counts against the fake's recorded requests.
3. End the file with one line, `Decision: …`, naming the number and the choice. Polling stays as the
   fallback whatever the choice.

**Non-goals.** No subscription code. No change to `POLL_INTERVAL` or to which routes are read. No
edit of docket. Nothing runs against the operator's own docket home.

**Files.** `docs/plans/measurements/docket-polling.md`; a script only if the commands outgrow ten
lines.

**Acceptance.** `rg -n '^Decision:' docs/plans/measurements/docket-polling.md` prints one line; the
table has a row for each N and each route; every number can be reproduced from the commands at the
top of the file.

---

### C. Send the autonomy level per task

**Status: BLOCKED on docket P51-8** (proposed: `autonomy: L3|L4|L5` on `POST /tasks`, narrowing only,
the pod's profile is the ceiling; `failureKind: needs_human`). **Released when:** docket accepts the
phase and a docket build with P51-8 answers a capture of `POST /tasks` with `autonomy` and of a
`422` above the ceiling; the probe says it accepts the field, as `prepare` was gated on API 6.

**Measured trigger.** Today the level reaches docket only as a ceiling read at merge time
(`ceiling_of`), never on the task. And the brief's plan to compute it with `level_for` does not
work as written: `level_for(project, domains, paths, item_type)` is called in one place, the merge
(`tack_api::autonomy::gather`), with the *changed paths*, which do not exist when a task is posted.
With no paths no domain matches, so it returns `L3` for a project at `L4` and `L5` only for a project
at `L5` (`domain_matches` is false for empty `paths`). And a domain can *raise* the level above the
project's (the `docs` row of `level_for_table`), where docket's field only narrows. The level in the
task body can therefore not be `level_for`'s; the path-derived level stays a merge-time fact.

**Goal.**

1. The request snapshot carries the level to send: the project's level at claim time, lowered to
   what `ceiling_of` reports for the profile's team, so a normal run never meets the `422`. The
   task body sends it only when docket says it accepts the field.
2. A `422` above the ceiling is still docket's text and the attempt fails with it (the existing
   rule); it also marks the probe stale and asks for a fresh one.
3. `failureKind: needs_human` is its own outcome, not a plain failure: the attempt ends `failed`
   with code `needs_human` and docket's sentence, and the run card offers "Run again at L3" (an
   attended run), disabled with its reason when the team or the field cannot be narrowed. It never
   re-queues by itself.
4. Until P51-8 lands, `ceiling_of` and the merge hold are unchanged. When it lands, the pod's
   profile read by the probe is the one source for both the request and the hold; delete the second
   computation then, not before.

**Non-goals.** No change to `level_for`, to `evaluate`, or to how a revert demotes the project level
(`demote` stays Tack's). No new setting.

**Files.** `tack-orch` repository snapshot (the field); `harness/docket/task.rs` (the body, the
optional key); `harness/docket.rs` `finish` (the `needs_human` arm); `tack-api/src/autonomy.rs`
(`ceiling_of`); the run card's actions; `tests/fake_docket/` learns the key, the `422`, and the
failure kind.

**Acceptance.** `the_task_body_is_the_specs_example_key_by_key` gains `autonomy` among its optional
keys and checks it is absent on an older API; a project at L4 on an attended team sends `L3`, on a
lights-out team `L4`; the fake answers `422` above the ceiling and the attempt fails with its text;
a `needs_human` task gives code `needs_human` and the run card test finds "Run again at L3". Fixtures
are recaptured with `scripts/capture-docket-fixtures.sh` against that docket build.

---

### D. Read escalations, drop the workaround

**Status: BLOCKED on docket P51-2 and P51-4** (proposed: one record per escalation with a monotonic
`seq`; `GET /escalations?since=<seq>` and `escalations[]` on `GET /inbox`, additive, with options,
recommendation, state and `expiresAt`). **Released when:** a docket build with both answers a
capture that holds an escalation of each kind (approval, consult) with options.

**Measured trigger.** One fact reaches Tack in three shapes and costs two extra reads.
`approval_question`, `held_approval_question` and `consult_question` build the same `Question`
(`grep -n 'fn approval_question\|fn held_approval_question\|fn consult_question' crates/tack-runner/src/harness/docket.rs`
prints three). `next_question` makes a `GET /tasks/<pod>/<id>` for every parked consult and a
`GET /approvals` for every held approval, only because the inbox's copy has no options (fixtures
`README.md`, "Where a parked question's options live").

**Goal.** The adapter reads `escalations[]` from the inbox (or `GET /escalations?since=<seq>`),
keeps the last `seq` per attempt beside the trace cursor (written by rename, so a second adapter over
the same state directory resumes), and builds one `Question` from an escalation. The task-view lookup
is not on the path taken when `escalations[]` is present. It survives only in the fallback for a
docket that sends none, and G deletes it. Answers go through P51-5's `POST /escalations/<id>` with `{optionId, content?, actor?}` as the
one route, so the two `Pending` variants and their two bodies collapse into one; the old routes
survive only in the fallback for a docket that sends no escalations, and G deletes them.

**Non-goals.** No change to `Question` or to the decision channels; the Inbox needs no new route. No
A2A. No deleting the fallback (G).

**Files.** `harness/docket.rs` (`next_question`, `Pending`), `harness/docket/client.rs` (the route),
`fixtures/docket/v5/` and its `README.md`, `tests/fake_docket/`, `docs/book/src/developer/runs-and-teams.md`.

**Acceptance.** Fixtures recaptured with `scripts/capture-docket-fixtures.sh` against that docket
(an escalation with options comes from a run of `scripts/docket-prill-e2e.sh`, as the present ones do,
or is `constructed` and says so). In `tests/docket_v5.rs`: a consult and an approval each produce
their `Question` with `fake.requests_to("GET", "/approvals")` and the task-view read both empty;
a second adapter over the same state directory sends the saved `since`;
`every_route_the_client_sends_has_a_captured_file` and
`the_fake_answers_with_the_keys_of_the_captures` fail if a route or a key changes shape.

---

### E. One A2A client for docket

**Status: CUT 2026-10-25.** Its trigger was "after D there are still two ways to answer"; D now
takes P51-5's `POST /escalations/<id>` as the one answer route, so the trigger is gone. What E
added was a second transport (agent card, `tasks/get`, `message/send`) for facts the REST routes
already carry, with its own fixtures, fake routes and a fallback to the first transport: two paths
to maintain until G deleted one. Tack needs A2A the day it needs push or interop with another
agent runtime, and that is a card of its own, with its own measured trigger, when it comes.

---

### F. Show what docket decided alone

**Status: BLOCKED on docket P51-3 and P51-8** (proposed: the `escalation_*` events and span;
`autonomy` and the policy that answers). **Released when:** a capture of `GET /traces` holds an
escalation event that names the option chosen and why, from a run that docket resolved by itself.

**Measured trigger.** Tack cannot tell. `hop_progress` maps `hop_started` and `hop_finished` and
nothing else, so an escalation event is dropped from the timeline. `escalation_rate`
(`decisions ÷ attempts`) and `human_minutes_per_decision` count rows of `execution_decisions`
(`repo/metrics.rs`), which exist only for questions sent to a person; an escalation docket answers
itself is in neither, and nothing says so.

**Goal.** A line on the task timeline for each escalation docket decided (proposed
`escalation.decided`; "decided alone: option A, because …"). The factory metrics split: the existing
`escalation_rate` and minutes stay "a person answered"; a count and rate of "the policy answered"
are added beside them, with one query next to the `decisions` count. A decided-alone escalation
creates no decision row.

**Non-goals.** No change to how a revert demotes the level; that stays Tack's. No Inbox item for a
decision nobody has to make.

**Files.** `harness/docket.rs` (`hop_progress` or a sibling), `tack-db/src/repo/metrics.rs`,
`tack-api/src/handlers/metrics.rs` (`FactoryMetrics`), `frontend/src/features/dashboard/FactoryMetrics.tsx`,
the generated schema through `./scripts/regen-generated.sh`, never by hand.

**Acceptance.** A `constructed` trace page with one such event gives the timeline line; a handler test
shows a decided-alone escalation leaves `escalation_rate` and `human_minutes_per_decision` unchanged
and raises the new count; `npx vitest run` on `FactoryMetrics.test.tsx` shows both.

---

### G. Contract guard

**Status: BLOCKED on docket P51-9** (proposed: removal of the old shapes, only after Tack has moved),
**and on C, D and F.** **Released when:** docket releases P51-9 and the live proof,
`scripts/docket-prill-e2e.sh`, passes against it on the new path.

**Measured trigger.** After D the old shapes are a fallback kept for a docket that still sends
them; once docket removes them the fallback is code and fixtures that nothing can exercise.

**Goal.** In one change: recapture the fixtures; delete every fixture and every code path of the shapes
docket removed (the list is what the recapture no longer produces, not a guess made here); raise the
probe's floor to the API that has the escalations, with the sentence the present floor uses; rewrite
the fixtures `README.md` sections that describe the removed shapes.

**Non-goals.** No new behaviour. No removal ahead of docket's.

**Files.** `harness/docket.rs`, `harness/docket/client.rs`, `harness/docket/probe.rs`,
`fixtures/docket/v5/`, `tests/fake_docket/`.

**Acceptance.** `git grep -n 'approval_question\|held_approval_question\|consult_question' crates`
prints nothing; a second run of `scripts/capture-docket-fixtures.sh` changes no file; a new test
fails on any fixture no test or fake reads; `every_route_the_client_sends_has_a_captured_file` passes
with the old routes gone.

---

## Rules every card carries

- Worktree from `develop`, never from `main`; `PATH=$HOME/.cargo/bin:$PATH`; `CARGO_TARGET_DIR` under
  the agent-targets folder on the `/` partition; `--build-jobs 4 --test-threads 4`.
- No GUI, no headed browser; never the live board, the operator's docket home, the runner's settings
  file or a `tack.db`. Never edit docket: its side is its maintainers'.
- No `Co-Authored-By`; the change writes its own `CHANGELOG.md` line; code comments carry no phase,
  card or ADR numbers (`scripts/check-comments.sh`); the User and Administrator guides carry no source
  paths.
- A docket field or route is read only when docket says it has it, and the card that adds it names it
  "proposed" in the code's tests until a capture from a docket that accepts it replaces the
  `constructed` fixture.
- Names above were read on 2026-10-25 in the tree of `phase-79-architecture` (`develop` plus Phase 79's
  documents, no Phase 79 code). Phase 79 may move modules; run `rg -n` before trusting a path.

---

## What the execution found (2026-10-26)

- Tasks ran as R (recapture), RS1 (reader, answer route, deletions), BS (board side, in parallel with R) and RS2
  (autonomy, `needs_human`, timeline). Full suite 1517 passed, `.githooks/pre-push` passed on the merged tree with
  Phase 79 and its cleanup.
- docket's `apiVersion` is still `"6"` on both shapes, so Tack asks `GET /escalations?since=0` and fails with a
  sentence when it answers 404; there is no fallback and no version number to compare.
- The saved `since` is the highest `seq` below the first pending record, not `next`: a runner restarted while a
  question is open must read it again, or with no `expiresAt` it would wait for ever.
- The fixtures now hold real captures of a pending escalation, its answers (200, 404, 409, 422, 400), a
  `needs_human` task and a `decided` escalation, taken with no model. A consult or input answer cannot be made without
  a model, so those are `constructed` from docket's own `operator-v2` fixtures and say so.
- A cancelled held task leaves its escalation `resolved` with `resolution: null`.
- docket's `escalation_decided` event carries no label and no rationale, so the timeline reads
  "decided alone: option deny (policy deny, level L5)".
- A docket 422 above the autonomy ceiling fails the attempt with docket's text; there is no probe cache to mark stale.
- Decisions left to the maintainer: (1) a project at the default L3 now sends `autonomy: L3`, so a lights-out team
  runs attended; (2) a lights-out team with unmet requirements counts as attended (L3 at most); (3) a question with
  no `expiresAt` holds its run until answered, resolved by docket or stopped; (4) rerun `scripts/docket-prill-e2e.sh`
  against docket's `develop`.
