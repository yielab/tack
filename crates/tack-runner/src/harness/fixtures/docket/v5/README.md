# docket — what the harness is tested against

The files in this directory are **captures from a real docket**, plus the answers that are
marked `constructed`. Captured from `docket 0.2.0b6` (operator API version `6`, with the
escalation record), source checkout `9954b4f8`, on 2026-10-26. The directory keeps the name
`v5` from the first capture; the answers say `apiVersion: "6"`.

`scripts/capture-docket-fixtures.sh` makes them. It starts `docket start --dispatch` in a
throwaway `HOME` and `DOCKET_HOME` under `/var/tmp`, with no model, no provider key and an
unreachable proxy, and sends every request Tack's docket client sends. It refuses to run when
`DOCKET_HOME` is unset or is `~/.docket`, and stops when `GET /status.json` does not answer
`apiVersion: "6"`. Nothing is spent: every task it queues is refused or failed before any agent
turn, and the two escalations it opens need no model either (see "Where a pause comes from
without a model"). Rerun it when docket changes and commit what differs; a second run on the
same day changes nothing, and on another day only the `date:` lines.

The docket that runs is the installed `docket` on `PATH`; `source checkout` names the commit of
the checkout beside it (`DOCKET_SRC`). The two can differ in files that no route reads (the
MCP server, an internal port module), so a capture is exact for the HTTP routes and the
provenance names the checkout as the source to read, not as the build.

## Files

For each answer, `<name>.json` is the body docket sent and `<name>.json.provenance` says where
it came from. A request with a body has `<name>.request.json` (or `.request.txt` when the body
is not JSON).

A provenance file starts with `captured` or `constructed`, then `key: value` lines: `route`,
`status`, `request`, `tack-sends` (whether this is a request Tack's client sends or a refusal
of one it never sends), `optional-keys` (request keys Tack does not always send), the
`command`, the `docket` version, the `date`, and `redacted`: the scratch folder, timestamps,
commit ids, task ids, escalation ids, approval and sweep-run ids and the claim's process id
are rewritten to fixed values. The bearer token is read from the file docket writes and is
never printed or recorded; the script fails if it finds the token in a capture.

A `constructed` file is an answer docket only sends while a model runs or a person answers (a
finished task, a hop's trace event, a pending consult or input).
Its keys are the ones docket writes, and its provenance names the exact source: a file of
docket's own `tests/fixtures/operator-v2/` (recorded from a real run on a fake model), a section
of `specs/api/operator-api.spec.md`, or a model in docket's source, always with docket's commit.
Its values are made up or are those of the capture it is built from. No constructed file has a
key the spec or the source does not list. The build is a one-off (the files are not rewritten
by the script); a constructed file is replaced by a capture when the script can produce it.

## What `tests/fake_docket/` does with them

- It answers each route from its file, with this run's names and numbers put in. It has the
  keys docket has and no others; `the_fake_answers_with_the_keys_of_the_captures` compares them.
- It checks a request body against the requests Tack sends that the files hold: the captured
  ones, and the one constructed answer that carries its request inline. A key none of them has,
  or one every one of them has that this one lacks (apart from `optional-keys`), is a `400`
  naming it. docket itself ignores unknown keys, so the fake is stricter than docket here, on
  purpose: a field docket does not read is a bug in Tack that docket would not report.
- `every_route_the_client_sends_has_a_captured_file` fails when `client.rs` sends a route with
  no capture.

## What docket showed that a hand-written fake had wrong

- `POST /pods` reads `name` and `location`; the old `project` and `path` answer `400 name is
  required`. The answer still names the team `project`.
- A task's `GET /tasks/<pod>/<id>` carries the sentence in `reason` and the kind in
  `failureKind`; it has no `error` key (only a hop has).
- Every error body is `{"ok": false, "error": "<sentence>"}`; `GET /health` is
  `{"status": "ok", "apiVersion", "dispatch"}`.
- docket accepts a blank description, any priority and unknown keys without comment, and fills a
  brief out to the whole document.
- A held task and an escalation exist without a model: see "Where a pause comes from without a
  model".
- A task's view no longer carries `source`, `claimId`, `approvalToken`, `pregrants` or
  `gateOverridePipelineIndex`; it carries `autonomy`, `escalations`, `pipelineSource`,
  `gateApplied`, `usage` and `tokenBudget` when they apply. A pod's queue counts are `pending`,
  `running`, `waitingApproval`, `waitingInput`, `blocked`, `failed`, `done`, `cancelled` (no
  `completed`).
- `GET /traces/<pod>` holds back the newest second: a read made right after a hop's last event
  is empty, so the script waits two seconds before each one.

## The escalation record

Since docket's Phase 51 every pause is one durable record, an *escalation*, and two routes are
all there is: `GET /escalations?since=<seq>&pod=<pod>&task=<id>` lists records, and
`POST /escalations/<id>` settles one. The routes of the earlier shapes (`GET /approvals`,
`POST /approvals/<token>`, `POST /tasks/<id>/answer`) answer `404`, the task keys
`approvalToken`, `question`, `answers`, `consultAnswer`, `pregrants`, `taskGrants` and
`gateOverridePipelineIndex` are gone, and so are the trace events `approval_requested`,
`question_asked`, `input_requested`, `input_answered` and `input_expired`. `apiVersion` is `"6"`
on both shapes, so only a probe of `GET /escalations?since=0` tells them apart; a `404` there is
a docket without the record.

- **The view** (`escalations-pending`, `escalations-consult`, `escalations-input`): `id`, `task`,
  `pod`, `step`, `role`, `kind` (`approval`, `consult`, `input`, `gate` or `check_changed`),
  `causedBy`, `prompt`, `options` (`id`, `label`, `description`, `risks`, `estimatedTokens`),
  `recommendation` (`optionId`, `rationale`, `evidenceRefs`, or `null`), `requestedSchema` (an
  object or `null`), `askedAt`, `expiresAt`, `onExpiry` (`deny` or `block`), `policy`
  (`person`, `recommendation`, `deny`, `fail`), `level` (`L3`, `L4`, `L5`), `state`
  (`pending`, `resolved`, `decided`), `resolution` (`null`, or `optionId`, `content`, `by`,
  `channel`, `at`, `appliedAt`), `seq` and `affordances` (`options`, `acceptsText`,
  `resolveUrl`). The list is `{"escalations": [...], "next": <seq>}`; `next` is the highest
  `seq` returned, else the `since` asked for, and a record that settled between two reads is
  still returned (`escalations-task-since`, `escalations-settled`).
- **An approval's options** are `approve_once`, `approve_task` and `deny`, with `content`
  `{"reason"}` at most (`escalations-pending`, captured). **A consult's options** are the
  model's own, then `decline`, with `recommendation` and a `requestedSchema` that accepts
  `content` (`escalations-consult`, constructed). **An input's options** are `decline` alone and
  its `requestedSchema` is what `content` must satisfy; the spec lets `optionId` be omitted for
  that question only (`escalations-input`, constructed).
- **Answering** is `POST /escalations/<id>` with `{"optionId"?, "content"?, "actor"?}`; the
  answer is the view, `state: "resolved"`, `resolution` set and `affordances.options` empty
  (`escalation-accepted`, captured for an approval; `escalation-accepted-consult`, constructed).
  `404` an id no record has, `409` an escalation already settled (`escalation-settled`), `422` an
  option the record does not offer (`escalation-unknown-option`), `400` an ill-typed field
  (`escalation-bad-option-type`). A cancel closes the record as `resolved` with
  `resolution: null` (`escalations-settled`, its second record), so a reader must not assume a
  resolved record has a resolution.
- **Where a waiting task shows it.** The task's own record lists the ids opened for it in
  `escalations` (`task-waiting-escalation`, captured); a finished task keeps them
  (`task-failed-after-escalation`). docket's `GET /inbox` carries the same records, and the
  pauses the team settled alone in `decided`, but Tack reads only `GET /escalations` and the
  task, so no inbox answer is kept. An approval's `apr-` token is not an address: no answer and
  no request carries it, except the trace event `approval_granted`, which the script redacts.
- **The trace events** are `escalation_asked`, `escalation_delivered`, `escalation_resolved`,
  `escalation_expired` and `escalation_decided`; the capture holds the real `escalation_asked`
  and `escalation_resolved` of an answered approval (`traces-escalation`) and the
  `escalation_decided` of an L5 task (`traces-escalation-decided`). Every payload is
  `{escalation, kind, task, step, policy, askedAt}` plus `level`; `escalation_decided` adds
  `by: "policy"` and `optionId`, and carries no rationale or label, so Tack's timeline line names
  the option id and the policy. The old `approval_required` and `approval_resumed` still appear
  beside them.

## Where a pause comes from without a model

Four of docket's pauses need a model (a consult, an in-turn tool approval) or a protected-path
diff (`check_changed`), and none is reachable here. Two are, and the script records them:

- **An enqueue a policy holds.** The script writes one `pre_input` policy to the saas-dev team's
  `config/policies/` in the throwaway home (`then: ask`, matching one word). `POST /tasks/<pod>`
  with that word answers `201`, `status: "waiting_approval"` and the id of the escalation it
  opened (`post-task-gated`); a person's answer is then `approve_once` (`escalation-accepted`).
  A second held task is cancelled (`cancel-accepted`).
- **A pipeline's approval step at L5.** The lights-out team's ceiling is `L5`, so the script
  makes its profile met (sandbox on, its network cut, an external gate on the implementer step)
  and queues a task with `autonomy: "L5"` whose first step is an `approval`. docket asks nobody:
  it records a *decided* escalation (`level: "L5"`, `policy: "deny"`, `state: "decided"`) and
  fails the task before any hop with `failureKind: "needs_human"` (`task-failed-needs-human`).
  Before the profile is met the same task fails `dispatch_refused` with the
  profile's unmet requirements as `reason`; that answer is not kept.
- **`autonomy`** is a key of `POST /tasks/<pod>`: `L3`, `L4` or `L5`, narrowing only. Above the
  pod profile's ceiling is `422` naming the ceiling (`post-tasks-autonomy-above-ceiling`, an
  attended team and `L5`), any other value is `400` (`post-tasks-autonomy-invalid`). The task
  view carries `autonomy` when one was asked (`task-failed-needs-human`, `task-failed-pipeline`).
- **`GET /pods/<p>`** still reports the profile, as the verdict object `{"name", "violations"}`
  (`pod-attended`, `pod-lights-out`), and `GET /pods` as a string.

Not reachable, so not here: a `check_changed` escalation and a task `failed` with
`failureKind: "check_changed"` (the spec lists both; the task record's `checkChanged` key is
absent from every fixture), and a consult or input answered.

## When a question expires

`blocked` with `blockedReason: "input_expired"` is docket's own (the spec, "Re-attaching after
a restart" item 5: a question past its `expiresAt` moves the task there, an approval past it
resolves to denied, each by its record's `onExpiry`). The deadline of a wait is the pending
escalation's `expiresAt`: an approval's is a real timestamp (`escalations-pending`, redacted),
the constructed consult's is one too, and `expiresAt` is `null` where docket sets none, as on
the `decided` record. `task-blocked-input-expired` is `constructed` from a capture and docket's
`TaskRecord`; only the attempt code `not_answered` is Tack's own.

## Every fixture

| Fixture | Route | Status | Captured or constructed |
|---|---|---|---|
| `cancel-accepted` | `POST /tasks/<pod>/<id>/cancel` | 200 | captured |
| `escalation-accepted-consult` | `POST /escalations/<id>` | 200 | constructed |
| `escalation-accepted` | `POST /escalations/<id>` | 200 | captured |
| `escalation-bad-option-type` | `POST /escalations/<id>` | 400 | captured |
| `escalation-settled` | `POST /escalations/<id>` | 409 | captured |
| `escalation-unknown-option` | `POST /escalations/<id>` | 422 | captured |
| `escalation-unknown` | `POST /escalations/<id>` | 404 | captured |
| `escalations-consult` | `GET /escalations` | 200 | constructed |
| `escalations-empty` | `GET /escalations` | 200 | captured |
| `escalations-input` | `GET /escalations` | 200 | constructed |
| `escalations-pending` | `GET /escalations` | 200 | captured |
| `escalations-settled` | `GET /escalations` | 200 | captured |
| `escalations-task-since` | `GET /escalations` | 200 | captured |
| `health` | `GET /health` | 200 | captured |
| `hop-finished-cached` | `GET /traces/<pod>` | 200 | constructed |
| `pod-attended` | `GET /pods/<pod>` | 200 | captured |
| `pod-lights-out` | `GET /pods/<pod>` | 200 | captured |
| `pod-unknown` | `GET /pods/<pod>` | 404 | captured |
| `pods-empty` | `GET /pods` | 200 | captured |
| `pods` | `GET /pods` | 200 | captured |
| `post-pods-conflict` | `POST /pods` | 409 | captured |
| `post-pods-created-second` | `POST /pods` | 201 | captured |
| `post-pods-created` | `POST /pods` | 201 | captured |
| `post-pods-existing` | `POST /pods` | 200 | captured |
| `post-pods-old-fields` | `POST /pods` | 400 | captured |
| `post-task-autonomy-l5` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-created` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-existing` | `POST /tasks/<pod>` | 200 | captured |
| `post-task-gated` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-minimal` | `POST /tasks/<pod>` | 201 | captured |
| `post-tasks-autonomy-above-ceiling` | `POST /tasks/<pod>` | 422 | captured |
| `post-tasks-autonomy-invalid` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-brief-without-objective` | `POST /tasks/<pod>` | 422 | captured |
| `post-tasks-denied-tool-unknown` | `POST /tasks/<pod>` | 422 | captured |
| `post-tasks-empty-external-ref` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-gate-without-command` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-invalid-json` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-max-tokens-zero` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-network-open` | `POST /tasks/<pod>` | 422 | captured |
| `post-tasks-no-description` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-unknown-pod` | `POST /tasks/<pod>` | 404 | captured |
| `status-empty` | `GET /status.json` | 200 | captured |
| `status` | `GET /status.json` | 200 | captured |
| `task-blocked-input-expired` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-cancel-finished` | `POST /tasks/<pod>/<id>/cancel` | 409 | captured |
| `task-cancel-unknown` | `POST /tasks/<pod>/<id>/cancel` | 404 | captured |
| `task-diff-failed` | `GET /tasks/<pod>/<id>/diff` | 200 | captured |
| `task-diff-unknown` | `GET /tasks/<pod>/<id>/diff` | 404 | captured |
| `task-done` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-failed-after-escalation` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-needs-human` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-pipeline` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-unknown` | `GET /tasks/<pod>/<id>` | 404 | captured |
| `task-usage-by-model` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-usage-cached` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-waiting-escalation` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `traces-after-failure` | `GET /traces/<pod>` | 200 | captured |
| `traces-empty` | `GET /traces/<pod>` | 200 | captured |
| `traces-escalation-decided` | `GET /traces/<pod>` | 200 | captured |
| `traces-escalation` | `GET /traces/<pod>` | 200 | captured |
| `traces-events` | `GET /traces/<pod>` | 200 | constructed |
| `traces-since-empty` | `GET /traces/<pod>` | 200 | captured |
| `unauthorized` | `GET /pods` | 401 | captured |

`captured` was recorded by the script against a real docket; `constructed` is built as its
provenance says. A script run replaces every capture; the constructed files stay.

## What it leaves out

- `GET /tasks/<pod>` and its `externalRef`/`status` filters, `POST .../retry`,
  `POST /pods/<p>/apply`, `GET /pods/<p>/corrections`, `/metrics`, `/runs`,
  `POST /dispatch/<pod>`, `GET .../evidence`.
- The one-second hold-back of `GET /traces` (the fake's timestamps are all in the past), the
  `413` for bodies over 1 MiB, and `409` for a task that ran in place.
- `restrictions.policies` validation, `baseRef` failures other than an unresolvable ref, and
  `pipeline` resolution: the fake stores the name and never fails a task with
  `failureKind: "pipeline"` on its own; a test scripts that.
- A task's full lifecycle (running, parked on a consult or an input, done) is constructed; no
  capture runs a model. A consult or an input answered, `check_changed`, an expired question and
  `L4` or `L5` pauses decided by `recommendation` are constructed or absent.

## docket's cache figures

Read in docket's source on `develop` at `6fba7057` (2026-10-18); the keys arrived with docket
commit `da556114` ("the turn budget counts new tokens and accepts a paid final reply"). No
capture reaches a hop, so `hop-finished-cached`, `task-usage-by-model` and `task-usage-cached`
are `constructed`, each naming the source it was built from.

- A hop's `usage` is `{"input", "cached", "cacheWrite", "output"}`
  (`tasks/application/record.py::_hop_usage`), and the `hop_finished` event's payload carries it as `usage`
  (the `"hop_finished"` trace write in the same file). The task's `hops[].usage` is the same block.
- `cached` is the part of `input` a cache served and `cacheWrite` the part written to one. Both are
  subsets of `input` (`core/llm.py::TokenUsage`: "`cached_tokens` is a subset of `input_tokens`
  where the endpoint reports one, and 0 where it does not ... `cache_write_tokens` is a subset
  too"), so `input` includes reads and writes. docket writes 0 both for an endpoint that
  reports no figure and for a real zero; the wire cannot tell them apart.
- docket counts `input - cached + output` (`TokenUsage.counted_tokens`): a write is counted, a
  read is not. Tack's `Usage::counted` is `tokens_in - cache_read_tokens + tokens_out` with
  `tokens_in` holding reads and writes, so mapping `input` to `tokens_in`, `cached` to
  `cache_read_tokens` and `cacheWrite` to `cache_write_tokens` makes the two counts equal.
- A finished task's `usage.byModel.<model>` sums its hops' four keys
  (`tasks/application/lifecycle.py::_task_usage`, committed at `da556114`). The task's own top-level
  `usage` carried only `input` and `output` there. Operator API 5.7.0 adds `cached` and
  `cacheWrite` to it, committed at `e187be8a`; `task-usage-cached` is that shape and
  `task-usage-by-model` the earlier one, which the current docket no longer sends. Tack reads the top-level keys when they are there and sums `byModel` when they are not.
- Tack keeps a 0 as 0, so it never claims "not reported" for what may be a real zero; an attempt
  says "cache not reported" only when docket sends no key at all (a docket before `da556114`).
