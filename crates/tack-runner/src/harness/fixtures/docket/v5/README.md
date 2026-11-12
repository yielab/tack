# docket — what the harness is tested against

The files in this directory are **captures from a real docket**, plus the answers that are
marked `constructed`. Captured from `docket 0.3.0b1` (operator API version `6`, with the
escalation record, the task as fields, the prompt manifest, the two-level home, the four
blueprints, `workspaceKind`, `blueprint`, `pipeline.steps`, and the team's contract, budget, resume
and shape), source checkout `2ce62d69`, on 2026-11-04. The directory keeps the name `v5` from the
first capture; the answers say `apiVersion: "6"`.

`scripts/capture-docket-fixtures.sh` makes them. It starts docket the way the runner does in a
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
finished task, a hop's trace event, a pending consult or input), or an agent's call that leaves
the machine.
Its keys are the ones docket writes, and its provenance names the exact source: a file of
docket's own `tests/fixtures/operator-v2/` (recorded from a real run on a fake model), a section
of `specs/api/operator-api.spec.md`, or a model in docket's source, always with docket's commit.
Its values are made up or are those of the capture it is built from. No constructed file has a
key the spec or the source does not list. The build is a one-off (the files are not rewritten
by the script); a constructed file is replaced by a capture when the script can produce it.
The keys of the ones that need only a model's reply (a finished task, a running one, the task list
and its cursor, a consult of several questions, the hop events) are compared with a real docket's by
`scripts/compare-constructed-docket-fixtures.py`, which runs docket's own end-to-end world on a
scripted model; its `checked:` line says when.

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
- `POST /pods` names no shape in Tack's body for the default team, and docket's default is `build`; the
  blueprint the first capture sent, `software`, is an unknown name now (the four are `build`, `respond`,
  `operate`, `routine`). For a folder shape docket takes `location` as the team's working folder, so Tack
  sends none.
- `delivers` is a list, not a word; a method that does not load is listed with an `error` and no
  missing role; `window` is `none`, `day` or `week`; a resumed team whose budget is still spent is paused
  again. The features were first built in Tack from the proposal, and each of these differed.
- A task posted with `task` is echoed back as `task`; docket's record has no `brief` key unless the
  caller sent one, and Tack no longer does.
- `GET /pods/<pod>` carries no `codebase`: the list (`GET /pods`) does, `""` for a team that works
  in a folder. The detail says it with `repoConfigDir`, `""` for a folder team, and since docket's
  `df17a145` both routes say it outright with `workspaceKind` (`codebase` or `workdir`). Since
  `4bf26e50` (and, since `cdeabac3`, the task scope below) they also carry `blueprint`, and the detail `pipeline.steps` (a step's `rework` is a
  list of `{when, to, max}`; a parallel step carries its branches under `steps`).
- Since `cdeabac3` a record set carries `scope` (`pod` or `task`, always present, so its presence is how a
  client learns `?task=` exists), every row `_role` (the role of the hop that wrote it, right after `_step`;
  `null` for a row written by no hop) and `_updated_at` is `null` until the row is updated.
  `GET /pods/<pod>/records/<set>?task=<id>` answers the rows that task wrote in write order, for a `pod` set
  too (another task's row of the same ref is not there: `records-rows-pod-shared`), `[]` for a task that wrote
  none, and `limit` counts the rest in `cut` (`records-rows-task-limit`). `task=` empty, and `task=` with `sql=`,
  are the same `400` with the one sentence `task must name a task and cannot be combined with sql`. A repeated
  `ref` is `created: false` (an agent tool's answer, not a route) within the task for a `task` set and across
  the whole team for a `pod` set; the script checks both. Each member of `GET /pods/<pod>` lists
  `capabilities`: `kinds` (`read`, `write`, `exec`, `record`), `tools` and `records` `{write, query}`. A role
  that denies `write` still proposes (`record` is its own kind): the lead reads and proposes
  (`pod-capabilities`), and every role of a team that declares `tack_proposals` can, the implementer too; a
  team with no such set lists `records.write: []` (`pod-attended`), and `deniedTools: record` empties it again
  (`pod-capabilities-denied`).
- `cut` on a set's rows is the count of rows the limit left out (`0` for none), not a flag.
- A hop that cannot reach a model still carries `promptManifest`: docket writes it before it asks.
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
diff (`check_changed`), and none is reachable here. Three are, and the script records them:

- **A wait step.** A pipeline whose only step is `wait: 1h` parks the task in `waiting_input`
  with an escalation of kind `wait`, policy `machine`, one option `resume` and the due time as its
  expiry (`escalation-wait`). No agent turn runs.
- **An enqueue a policy holds.** The script writes one `pre_input` policy to the saas-dev team's
  `config/pods/saas-dev/policies/` in the throwaway home (`then: ask`, matching one word). `POST /tasks/<pod>`
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

## Records with a scope, `?task=` and what a member reaches: captured

docket committed this work as `cdeabac3` (API stays `6`, every key additive) and the script
captures it: it declares `tack_proposals` with `scope: task` on three teams (a folder team, a team
with a codebase, and one that then denies the `record` kind), writes rows through docket's own
records port with a task and a role as their origin, and reads them back.

- `records-scoped`: the listing, every set with a `scope` (`pod` or `task`). Tack sends `?task=`
  for a set only when its entry carries `scope`.
- `records-rows-task`, `-other`, `-limit`, `-pod-shared`, `records-task-unknown`: the answers of
  `?task=<id>`, the task's rows in write order (`_id` ascending), with `_role` right after `_step`.
  `records-rows-sql-columns` is the same set read by statement, to show where `_role` sits.
- `records-task-empty` and `records-task-and-sql`: the `400` of an empty `task=` and of `task=` with
  `sql=`, with the same sentence. Tack sends neither.
- `pod-capabilities`, `-folder` and `-denied`: the reports of a team whose members carry
  `capabilities` (`kinds`, `tools`, `records.write`, `records.query`; no `services`), when the
  team declares the set, in a folder, and when it also denies `record`.

The fake is the docket of these captures by default, and `before_task_scope()` makes it the docket
before them: no `scope`, no `_role`, no `capabilities`, and `task=` ignored (every row of the set
comes back), which is why a test of the older docket proves Tack never sends it. No capture of
that docket exists; the variant is the captures with those keys taken out.

## A team's contract, budget, resume, shape and apply: captured

docket committed this as `fe78d068` and `fb20eed8` (`podContract`), `c948a4b0` and `19e8332a`
(`podResume`), `dffdcf49` (`podShape`) and `ae33098a` (`pipelineChecked`), all additive on API `6`, and
the script captures it. The features are named in `GET /health` exactly as above.

- **`podContract`** puts three keys on every report (`pod-attended`, `pod-folder`, `pod-contract`).
  `delivers` is a **list**: `["branch"]` for a team in a repository, `["report"]` for one in a folder, and
  `"records"` after it when the team declares record sets (`["report","records"]`). `methods[]` is every
  pipeline a task may name, one per name, each `{name, source, default, missingRoles}` (`source` is
  `recipe:<name>`, `pod:<name>`, ...); a lean team in a repository lists fifteen, and most name a role it
  lacks. `default` is true only for the method whose file is the team's bound pipeline, which a team made
  from a shape has none of until a recipe is applied (`pod-contract` has one). A file that does not load
  is listed with `error` (a multi-line parser sentence), `missingRoles: []` and `default: false`, so the
  absence of a missing role is not a sign that it runs. `needs[]` is `{kind, name, fix, satisfied,
  blocking}`, `kind` equal to `name` (`fetch-hosts`, `search-host`, `isolation`), recomputed on every call;
  a met need has an empty `fix`. `blocking` is a fact of the kind of need, served whether or not it is met:
  `fetch-hosts` blocks, `search-host` and `isolation` only degrade the work. A team nothing was recorded for
  says `[]`.
- **`webRead`** (docket's Phase 67) puts `webRead {mode, reason, fix}` on every report: `open` (a folder team
  under the jail reads any public host), `listed` (the hosts it names, or any public host granted with
  `--widen`) or `none` (with docket's `fix`; `fix` is empty otherwise). It agrees with the members'
  `capabilities`: a team that reads the web has `fetch` in each member's tools but the one that sends, and
  `fetchDomains` says `["*"]` under open reading. The fake lists it only after `with_web_read`.
- **`podResume`** puts `budget` on every row of `GET /pods` and on every report: `{tokens, used, window,
  windowStart}`. `tokens` is `null` without a cap, `window` is `none`, `day` or `week` and `windowStart`
  is `null` for `none`; `used` is what the gate would count now, over the window. The key is there with no
  cap (`pod-attended`). A team over its budget is not paused until a task meets the gate: `pod-budget-spent`
  has `used` above `tokens` and `paused: false`; `pod-budget` is the same team after the first hop met
  the gate, `paused: true` and `pausedReason: "budget"` (the key is absent while it runs), and
  `pods-paused` is the list with that row. `POST /pods/<pod>/resume` takes no body, clears the pause,
  puts the team's blocked tasks back to `pending` and answers `200` with the team's whole report plus
  `resumed` (`pod-resumed`: `true`); a team that is not paused answers the same with `resumed: false` and
  writes nothing (`pod-resume-not-paused`); an unknown team is `404` (`pod-resume-unknown`). It changes no
  budget: a team whose budget is still spent is paused again by the next task's first step, within a
  dispatch tick.
- **`podShape`**: `POST /pods` takes `blueprint` (`build`, `respond`, `operate`, `routine`) and `location`.
  `location` is the repository of a `build` team and the **working folder** of the other three, which
  docket otherwise gives a folder of its own (`post-pods-shape` is the body Tack sends for a folder shape:
  no `location`). `ensure` on a team that exists changes nothing, whatever shape it is asked for.
  `pod.yaml` may carry `shape:`, and `POST /pods/<pod>/apply` refuses a folder whose shape is not the
  team's (`pod-apply-shape-mismatch`, `400`). A team in a folder needs an absolute `directory`
  (`pod-apply-no-directory`, `400`, a request Tack never sends); a folder that would run something on this
  machine (here a fetch host) is `409` with a `reviewCode` and what runs (`pod-apply-needs-code`), a wrong
  `accept` is `409` without a code (`pod-apply-code-changed`), the right one `200` with the items
  (`pod-apply-applied`: `action` `add`), and the same folder again is `200` with every item `skip`
  (`pod-apply-again`). The code is a hash of what runs and of the team's working folder, so it is not
  stable between runs and the script redacts it. An unknown team is `404` (`pod-apply-unknown`).
- **`pipelineChecked`**: `POST /tasks/<pod>` with a `pipeline` whose roles the team lacks is accepted
  (`post-task-pipeline-roles`, `201`, `pending`) and refused at claim, before any hop
  (`task-failed-pipeline-roles`): `failed`, `failureKind: "pipeline"`, no hop, and the sentence
  `pipeline '<name>' (<source>) names roles this pod does not have: <role> (step <step>)` in `reason`.

## A team's memory: captured

`GET /pods/<pod>/memory?status=accepted|pending|refused` (the `memoryStatus` feature, named in `GET /health`)
answers `{"entries": [...]}`, oldest first, each `{id, kind, subject, note, status, reason, provenance, stale,
evidence}`. No model writes a team's memory in the capture, so the script has docket's own function
(`record_task_memory`) record one task's `## Memory` section in the throwaway home, with docket deciding every
entry itself: a path present at the task's commit is `accepted` (`reason: "path present at the task commit"`,
`evidence: {}`), one that is not is `refused`, and a pitfall or a convention waits (`pending`, `reason: "awaits
operator approval"`). A second task that finds a file gone flips its accepted entry to `stale: true`. `provenance`
is `{task, date, commit}` (`root` instead of `commit` for a team in a folder). The default status is `accepted`
(`pod-memory-default`), a team with no memory answers `{"entries": []}` (`pod-memory-none`), a status docket does
not have is `400` naming the three (`pod-memory-bad-status`) and a team it does not dispatch is `404`
(`pod-memory-unknown`). `evidence` is `{"exit": n}` or `{"ok": bool}` for an entry about a command or a tool call,
which a path never has; no capture holds one, because it needs a task's trace with a bash call in it.

### Deciding an entry: captured

`POST /pods/<pod>/memory/<id>` (the `memoryDecide` feature) takes `{"decision": "approve" | "refuse",
"reason"?}` and answers `200` with the one entry as the list gives it, not wrapped in `entries`, in its new status
(`pod-memory-approved`: `accepted`, `reason: "approved by the operator"`; `pod-memory-refused-by-http`: `refused`,
`reason` the one sent). `reason` is required for a refusal and ignored for an approval. An entry that is not pending
is `409` and unchanged (`pod-memory-decide-settled`), an entry or a team docket does not have is `404`
(`pod-memory-decide-unknown-entry`, `-unknown-pod`), and `400` is a body that is not JSON or not an object, a
decision that is neither word, a reason that is not a string, and a refusal with no reason or a blank one
(`pod-memory-decide-invalid-json`, `-not-object`, `-bad-decision`, `-bad-reason`, `-no-reason`, `-blank-reason`).
Without the token it is `401`. The capture decides the two entries `saas-dev` holds pending, after every read of them.

`pod-memory-evidence-accepted|pending|refused` are **built, not queued**: no task of the capture runs a command or a
tool, so the script writes the memory file of the team `saas-notes` with docket's own `MemoryEntry` and
`store.write_json`, which is what docket's own HTTP tests do. It is a test-side recipe, not a product API; docket
lists the entries as it lists any. They hold the evidence the task-made ones cannot: `{"exit": 1}` for a command,
`{"ok": true}` for a tool call and `{}` for a path, with one pending and one refused entry.

## Every fixture

| Fixture | Route | Status | Captured or constructed |
|---|---|---|---|
| `cancel-accepted` | `POST /tasks/<pod>/<id>/cancel` | 200 | captured |
| `escalation-accepted-consult` | `POST /escalations/<id>` | 200 | constructed |
| `escalation-accepted` | `POST /escalations/<id>` | 200 | captured |
| `escalation-bad-option-type` | `POST /escalations/<id>` | 400 | captured |
| `escalation-outward` | `GET /escalations/<id>` | 200 | constructed |
| `escalation-pending` | `GET /escalations/<id>` | 200 | captured |
| `escalation-settled` | `POST /escalations/<id>` | 409 | captured |
| `escalation-unknown-option` | `POST /escalations/<id>` | 422 | captured |
| `escalation-unknown` | `POST /escalations/<id>` | 404 | captured |
| `escalation-wait` | `GET /escalations/<id>` | 200 | captured |
| `escalations-consult` | `GET /escalations` | 200 | constructed |
| `escalations-empty` | `GET /escalations` | 200 | captured |
| `escalations-input` | `GET /escalations` | 200 | constructed |
| `escalations-pending` | `GET /escalations` | 200 | captured |
| `escalations-settled` | `GET /escalations` | 200 | captured |
| `escalations-task-since` | `GET /escalations` | 200 | captured |
| `health` | `GET /health` | 200 | captured |
| `hop-finished-cached` | `GET /traces/<pod>` | 200 | constructed |
| `pod-apply-applied` | `POST /pods/<pod>/apply` | 200 | captured |
| `pod-apply-again` | `POST /pods/<pod>/apply` | 200 | captured |
| `pod-apply-code-changed` | `POST /pods/<pod>/apply` | 409 | captured |
| `pod-apply-needs-code` | `POST /pods/<pod>/apply` | 409 | captured |
| `pod-apply-no-directory` | `POST /pods/<pod>/apply` | 400 | captured |
| `pod-apply-shape-mismatch` | `POST /pods/<pod>/apply` | 400 | captured |
| `pod-apply-unknown` | `POST /pods/<pod>/apply` | 404 | captured |
| `pod-attended` | `GET /pods/<pod>` | 200 | captured |
| `pod-budget-spent` | `GET /pods/<pod>` | 200 | captured |
| `pod-budget` | `GET /pods/<pod>` | 200 | captured |
| `pod-capabilities` | `GET /pods/<pod>` | 200 | captured |
| `pod-contract` | `GET /pods/<pod>` | 200 | captured |
| `pod-folder` | `GET /pods/<pod>` | 200 | captured |
| `pod-lights-out` | `GET /pods/<pod>` | 200 | captured |
| `pod-memory-approved` | `POST /pods/<pod>/memory/<id>` | 200 | captured |
| `pod-memory-decide-bad-decision` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-bad-reason` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-blank-reason` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-invalid-json` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-no-reason` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-not-object` | `POST /pods/<pod>/memory/<id>` | 400 | captured |
| `pod-memory-decide-settled` | `POST /pods/<pod>/memory/<id>` | 409 | captured |
| `pod-memory-decide-unauthorized` | `POST /pods/<pod>/memory/<id>` | 401 | captured |
| `pod-memory-decide-unknown-entry` | `POST /pods/<pod>/memory/<id>` | 404 | captured |
| `pod-memory-decide-unknown-pod` | `POST /pods/<pod>/memory/<id>` | 404 | captured |
| `pod-memory-evidence-accepted` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-evidence-pending` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-evidence-refused` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-refused-by-http` | `POST /pods/<pod>/memory/<id>` | 200 | captured |
| `pod-memory-accepted` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-bad-status` | `GET /pods/<pod>/memory` | 400 | captured |
| `pod-memory-default` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-none` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-pending` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-refused` | `GET /pods/<pod>/memory` | 200 | captured |
| `pod-memory-unknown` | `GET /pods/<pod>/memory` | 404 | captured |
| `pod-resume-not-paused` | `POST /pods/<pod>/resume` | 200 | captured |
| `pod-resume-unknown` | `POST /pods/<pod>/resume` | 404 | captured |
| `pod-resumed` | `POST /pods/<pod>/resume` | 200 | captured |
| `pod-unknown` | `GET /pods/<pod>` | 404 | captured |
| `pods-empty` | `GET /pods` | 200 | captured |
| `pods` | `GET /pods` | 200 | captured |
| `pods-paused` | `GET /pods` | 200 | captured |
| `post-pods-conflict` | `POST /pods` | 409 | captured |
| `post-pods-created-second` | `POST /pods` | 201 | captured |
| `post-pods-created` | `POST /pods` | 201 | captured |
| `post-pods-existing` | `POST /pods` | 200 | captured |
| `post-pods-old-fields` | `POST /pods` | 400 | captured |
| `post-pods-shape` | `POST /pods` | 201 | captured |
| `post-task-autonomy-l5` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-conversation-needs-one-step` | `POST /tasks/<pod>` | 400 | captured |
| `post-task-created` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-existing` | `POST /tasks/<pod>` | 200 | captured |
| `post-task-folder-base-ref` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-folder-prepare` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-folder` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-gated` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-minimal` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-pipeline-roles` | `POST /tasks/<pod>` | 201 | captured |
| `post-task-prepare` | `POST /tasks/<pod>` | 201 | captured |
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
| `post-tasks-task-bad-type` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-task-unknown-key` | `POST /tasks/<pod>` | 400 | captured |
| `post-tasks-unknown-pod` | `POST /tasks/<pod>` | 404 | captured |
| `records-rows-task` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-rows` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `pod-capabilities-denied` | `GET /pods/<pod>` | 200 | captured |
| `pod-capabilities-folder` | `GET /pods/<pod>` | 200 | captured |
| `records-rows-pod-shared` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-rows-sql-columns` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-rows-task-limit` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-rows-task-other` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-task-unknown` | `GET /pods/<pod>/records/<set>` | 200 | captured |
| `records-scoped` | `GET /pods/<pod>/records` | 200 | captured |
| `records-task-and-sql` | `GET /pods/<pod>/records/<set>` | 400 | captured |
| `records-task-empty` | `GET /pods/<pod>/records/<set>` | 400 | captured |
| `records` | `GET /pods/<pod>/records` | 200 | captured |
| `status-empty` | `GET /status.json` | 200 | captured |
| `status` | `GET /status.json` | 200 | captured |
| `task-blocked-input-expired` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-cancel-finished` | `POST /tasks/<pod>/<id>/cancel` | 409 | captured |
| `task-cancel-unknown` | `POST /tasks/<pod>/<id>/cancel` | 404 | captured |
| `task-diff-failed` | `GET /tasks/<pod>/<id>/diff` | 200 | captured |
| `task-diff-unknown` | `GET /tasks/<pod>/<id>/diff` | 404 | captured |
| `task-done-turn` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-done` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-failed-after-escalation` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-approval-expired` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-failed-base-ref-folder` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-hop` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-needs-human` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-pipeline` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-pipeline-roles` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-failed-prepare-folder` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-unknown` | `GET /tasks/<pod>/<id>` | 404 | captured |
| `task-usage-by-model` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-usage-cached` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-waiting-escalation` | `GET /tasks/<pod>/<id>` | 200 | captured |
| `task-waiting-turn` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `task-with-manifest` | `GET /tasks/<pod>/<id>` | 200 | constructed |
| `traces-after-failure` | `GET /traces/<pod>` | 200 | captured |
| `traces-empty` | `GET /traces/<pod>` | 200 | captured |
| `traces-escalation-decided` | `GET /traces/<pod>` | 200 | captured |
| `traces-escalation` | `GET /traces/<pod>` | 200 | captured |
| `traces-events` | `GET /traces/<pod>` | 200 | constructed |
| `traces-since-empty` | `GET /traces/<pod>` | 200 | captured |
| `unauthorized` | `GET /pods` | 401 | captured |

`pod-folder`, `records` and `records-rows` are captured from a `routine` team in a plain folder with
the `records` recipe applied by `docket pod update`; the one row of `records-rows` is inserted into
the set's own table, because only an agent's tool call writes one. `post-pods-created` carries
no `blueprint`, as Tack sends none for the default team; `post-pods-shape` carries one and no `location`.

A team that works in a folder has no repository to cut a worktree from. `post-task-folder` is the
task as Tack sends it there (no `baseRef`, no `prepare`; `optional-keys` on it is why the fake does not
require them), and `post-task-folder-base-ref` and `post-task-folder-prepare` are the two it never sends,
which docket claims and settles `failed` before any hop (`task-failed-base-ref-folder`,
`task-failed-prepare-folder`: `failureKind` `base_ref` and `prepare`, the pod and the value in `reason`).
`post-task-prepare` is the one request that carries `prepare` to a team with a codebase.
`task-failed-approval-expired` stays `constructed` on purpose: docket's own deadline for an approval is
hours, and the script waits for nothing.

`pod-folder` and `pod-attended` are the reports of a team in a folder and of one in a repository, with the
keys of the contract and the budget; the fake takes those keys out for a docket before the feature
(`podContract`, `podResume`).

A conversation task (`conversation: {maxTurns}`, one-step pipeline only) is the one task whose turns
need a model. `post-task-conversation-needs-one-step` is captured: a team whose pipeline has several steps
answers `400` with `code: conversation_needs_one_step` and enqueues nothing. The turns themselves are
answers of a real docket on a scripted model, read by `scripts/compare-constructed-docket-fixtures.py`:
`task-waiting-turn` (waiting between two turns, `conversation {maxTurns, turns}`), `escalations-turn` (the
escalation of kind `turn`: `prompt` is the turn's answer, options `reply` and `close`, `policy` and
`onExpiry` `deny`), `escalation-accepted-turn` (a `reply` with `content {text}`: `appliedAt` stays null and
the task goes on), `escalation-accepted-turn-close` (a `close`: `appliedAt` is stamped, the task is `done`;
Tack never sends it, docket's CLI and sweep do) and `task-done-turn` (the last turn, which reaches `maxTurns` and ends the task).
`escalations-consult-parts` is one consult call of two questions: two records with `part {index, count}`, the
task staying `waiting_input` until both are answered.

`captured` was recorded by the script against a real docket; `constructed` is built as its
provenance says. A script run replaces every capture; the constructed files stay.

## What it leaves out

- `GET /tasks/<pod>` and its `externalRef`/`status` filters, `POST .../retry`,
  `GET /pods/<p>/corrections`, `/metrics`, `/runs`,
  `POST /dispatch/<pod>`, `GET .../evidence`.
- The one-second hold-back of `GET /traces` by the clock (the fake holds a line back for a time a test
  sets, `hold_back_traces`, and none by default), the
  `413` for bodies over 1 MiB, and `409` for a task that ran in place.
- `restrictions.policies` validation, `baseRef` failures other than an unresolvable ref, and
  `pipeline` resolution: the fake stores the name and never fails a task with
  `failureKind: "pipeline"` on its own; a test scripts that. A conversation task's turns are the fake's
  own (`with_conversation`), made of the real answers above.
- A task's full lifecycle (running, parked on a consult or an input, done) is constructed; no
  capture runs a model. A consult or an input answered, `check_changed`, an expired question and
  `L4` or `L5` pauses decided by `recommendation` are constructed or absent.

## The hop's prompt manifest

Each hop of a task record may carry `promptManifest` (`layers`, each `layer`, `name`,
`version`, `sha256`, `scope`, `tokens`, then `model`, `family`, `class`, `profile`) and an
`artifact` with `assumptions` and `questions` (`operator-api.spec.md`, "Task answers";
`pod-dispatch.spec.md`, "Hop prompt manifest"). `task-failed-hop` is a capture: the first hop
composes its prompt and then finds no model to ask, so the record has the manifest and an artifact
with neither list. `task-with-manifest` stays `constructed`, because the lists are a reply's; its
hops carry every key of the captured hop, a test compares them, and the local suite's run is where a
real one is read. The fake writes the manifest and the handoff on the record's hop and never on the
trace event.

The evidence Tack gives prill follows `attempt-evidence/v0` at prill's `1cafe8d`
(`contracts/attempt-evidence-v0/schema.json`, 2026-10-13): `generator.family` is the family the
last implementer step's manifest names, and `trajectory` is `{uri, digest}` with the digest as
`sha256:<hex>`. `claims` is not sent.

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
