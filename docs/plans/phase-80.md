# Phase 80: a run that waits on a person, and one record for every wait

**Status: proposed 2026-10-25. Nothing here is started.** Cards A and B depend on no docket card and
can start when the maintainer says so; C to G are `BLOCKED` on docket cards, each with the trigger
that releases it.

**Where this comes from.** docket's maintainers drafted a "Phase 51" that turns every moment docket
stops for a person (a tool approval, a model `consult`, an `input` step, a gate that needs a human)
into one record, an *escalation*. **That draft is not accepted.** Every docket name below (a card, a
route, a field, an event) is a *proposal* until it is accepted, and each card says "proposed" at the
first use. The agreed rule stands: Tack adapts to docket's contract, and docket never learns Tack's
names. Tack probes at runtime and sends or reads a new field only when docket says it has it, as
Phase 77 K did for `prepare`.

**Done when:** a run parked on a question is neither cancelled nor timed out while a person has not
had the chance to answer; the cost of polling docket is a number in the repository and the choice to
keep it or replace it is written beside the number; and, once docket ships its cards, the adapter has
one reader for questions, one way to answer, the autonomy level sent per task, what docket decided
alone shown, and no code for the shapes docket removed.

## What docket will offer, and which card uses it

| docket card (proposed) | What Tack can rely on afterwards | Tack card |
|---|---|---|
| P51-1 | pod setting `consultMode: decide`; `lights-out` requires it | none: Tack already reads the team's profile (`probe::capped`) |
| P51-2 | one record per escalation, monotonic `seq` | D reads it |
| P51-3 | one event family `escalation_*`, an OpenTelemetry span whose duration is the wait | F |
| P51-4 | `GET /escalations?since=<seq>` and `escalations[]` on `GET /inbox`, additive | D |
| P51-5 | `POST /escalations/<id>` with `{optionId, content?, actor?}`; old routes stay as adapters | D (only if the id does not work on the old routes), E |
| P51-7 | A2A binding: `message/send`, `tasks/get`, push config, agent card | E; B may adopt push config |
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
| B | **Measure polling before replacing it** | one measurement file | — |
| C | **Send the autonomy level per task** | the request snapshot, the docket task body, the run card's action | **BLOCKED on docket P51-8** |
| D | **Read escalations, drop the workaround** | the docket adapter's question reader, fixtures | **BLOCKED on docket P51-2 and P51-4** |
| E | **One A2A client for docket** | a new module of the docket adapter, the fake | **BLOCKED on docket P51-7**, and D |
| F | **Show what docket decided alone** | the timeline mapping, the factory metrics query, handler and screen | **BLOCKED on docket P51-3 and P51-8** |
| G | **Contract guard** | fixtures, the fake, the client, the probe's floor | **BLOCKED on docket P51-9**, and C, D, E, F |

Waves: **1** = A, B (now). **2** = C and D, each when its docket card is released. **3** = E and F.
**4** = G. A card changes only the files it owns; each writes its own guide paragraph and changelog
line, there is no separate docs task. One gate, on the merged tree: `.githooks/pre-push`.

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

**Status:** ready. **Depends on:** no docket card.

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
docket that sends none, and G deletes it. Answers keep using the existing routes; if the
escalation's id does not work on them, this card also takes P51-5's `POST /escalations/<id>`.

**Non-goals.** No change to `Question` or to the decision channels; the Inbox needs no new route. No
A2A (E). No deleting the fallback (G).

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

**Status: BLOCKED on docket P51-7** (proposed A2A binding: `message/send`, `tasks/get`, push
configuration, an agent card), **and on D**. **Released when:** a docket build serves the agent card
and a capture shows a task's escalation as a data part of `tasks/get` and an answer accepted as a
data part of `message/send`.

**Measured trigger.** After D the adapter still has two ways to answer with different bodies
(`POST /approvals/<token>` with grant or deny; `POST /tasks/<id>/answer` with accept or decline and
`content.optionId`) behind two `Pending` variants, and three question builders (D leaves them as the
fallback).

**Goal.** One path: probe the agent card first; `tasks/get` for the state, the escalation data part
for the question (the same mapping D wrote, a second transport), `message/send` with the result data
part to answer. The card absent means D's path, untouched. The agent card's location and the part
types are the A2A spec's and docket's P51-7 to fix; take them from a capture.

**Non-goals.** Tack does not become an A2A server. Task creation stays `POST /tasks`. Push
configuration is B's decision, not this card's. No streaming.

**Files.** a new module beside `harness/docket/client.rs`; `harness/docket.rs` (`follow`, `deliver`);
`tests/fake_docket/` serves the card and the two methods from captures; the fixtures `README.md`.

**Acceptance.** With the card, a question is read and answered with no `GET /inbox` for it and no
`POST /approvals` or `POST /tasks/*/answer`; without it, every existing test passes untouched. The
three builders have no caller outside the fallback. Fixtures recaptured by the script, which sends
the new requests.

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
**and on C, D, E and F.** **Released when:** docket releases P51-9 and the live proof,
`scripts/docket-prill-e2e.sh`, passes against it on the new path.

**Measured trigger.** After D and E the old shapes are a fallback kept for a docket that still sends
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
