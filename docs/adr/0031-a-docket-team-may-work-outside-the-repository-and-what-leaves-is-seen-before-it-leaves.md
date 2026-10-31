# ADR 0031: a docket team may work outside the repository, and what leaves is seen before it leaves

**Status: PROPOSED 2026-10-30; built 2026-10-30 on the maintainer's go-ahead (Phase 82, cards E, F), awaiting acceptance.** The maintainer asked what Tack must change for docket's Phase 55
(closed 2026-10-30: a team works outward, on its own schedule) and its Phase 56 (being finished the
same day: an external service is declared, a team keeps records, one judge). docket's API version
stays 6 and every addition is optional, except two removals that land with Phase 56 and that Tack
must not depend on: the blueprint named `software` and the roles `critic` and `tester`. Nothing
here waits on docket or on the verifier. It widens Phase 82 (ADR 0030) by three cards.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **A team may work in a folder.** A team whose docket pod has no repository (`codebase` is `""` on `GET /pods/<p>`) is reported as working in a folder. A run sent to it expects no change to the repository: when docket says the task is `done` and the record has no worktree, the attempt succeeds with what the last step handed off as its result, and nothing is merged, proposed or captured as a commit. A team that has a repository and ends with no worktree still fails as today | Settings → Automation → Teams says which teams work in a folder. An item given to one ends Succeeded with what the team did, instead of failing with "branch not applied" |
| 2 | **A wait is not a question.** A pending escalation of kind `wait` is shown as the team waiting until its time, with nobody asked; it never becomes a decision in Tack, never says "answer it in docket", and the run's time limit does not run while it waits (it already does not while docket waits). A wait that ran out with no route ends the attempt with that sentence | The timeline says "The team waits until 14:00. Nobody needs to answer." The run is not cut by its time limit while it waits |
| 3 | **An action that leaves is approved by seeing it.** For an approval docket marks `outward` (a `send` call: an email, a post, a write to a service), Tack reads the one escalation (`GET /escalations/<id>`) and shows its `call` above the options, titled as an action that leaves the team. The call is kept only in the pending decision and erased from it when the decision is answered or expires, in the same write; no event, log, export, evidence or verifier pack carries it. If docket does not return the call, Tack does not ask the question and says to answer it in docket, where the call is shown | The person sees exactly what will be sent before approving it, and Tack keeps no copy afterwards |
| 4 | **A run shows the records it wrote, and a team its record sets.** When a docket task settles, Tack reads, for each record set the team declares, the rows whose `_task` is that task (at most 50 per set, read-only, through `GET /pods/<p>/records/<set>?sql=`) and keeps them in the run's result beside `steps`. The team's row lists its record sets with their row counts. Every other row stays in docket | The Execution tab shows what the team recorded in that run (a lead, a listing, a line of a register); the team's row says what it keeps |
| 5 | **Tack names no team shape.** `POST /pods` with `ensure` sends no `blueprint`, so docket uses its own default (its code team, named `software` today and `build` after its Phase 56). The one-judge roster (`reviewer` for `critic` and `tester`) and the shape names are the onboarding skill's to write, not the product's | Nothing to configure; a team Tack creates keeps working when docket renames its shapes |

**Why now.** docket's Phase 55 made code the first kind of work a team does, not the only one; its
Phase 56 adds a declared service and a team's own records. Read against Tack's adapter on
2026-10-30: a task done by a folder team has no worktree, so Tack fails it with `branch_not_applied`
(`harness/docket.rs`, `finish`); a `wait` escalation is not among Tack's known kinds, so Tack tells
the person to answer in docket something docket says needs no answer; an `outward` approval is put to
the person with only its redacted, cut prompt, so they approve a send without seeing it; and Tack's
`ensure` names `blueprint: "software"` (`harness/docket/client.rs`), which docket's card P56-21
deletes.

**If you do nothing.** Every item given to a folder team fails; every wait step shows a false
instruction; a person approves an email or a write to a service blind; and once docket's Phase 56
closes, Tack can no longer create a team.

## What docket shipped that this reads (`develop` at `28c3cf65`, 2026-10-30)

Every locator is docket's; Tack reads them and edits none.

- `GET /pods/<p>` and `GET /pods` carry `codebase`, `""` for a pod with no repository
  (`hops/application/report.py`, `pod_summary`, `pod_detail`). A workdir pod's task record has no
  `worktree`.
- Escalation kind `wait` (`operator-loop.spec.md` area 11 item 2; `operator-api.spec.md` "Wait"):
  `policy` `machine`, the one option `resume`, `expiresAt` the due time; the task is
  `waiting_input`; it is never a `needsYou` item. A client "MUST ignore an escalation kind it does
  not know, and MUST NOT answer one it does not render". A wait that runs out with no `on.timeout`
  route blocks the task with `blockedReason: "wait_expired"`.
- `outward: true` on the view of an approval of a `send` call; `GET /escalations/<id>` adds `call`,
  the whole call as text with credentials and stored secret values removed, only while pending
  (`operator-loop.spec.md` area 11 item 16). `GET /escalations` never carries it.
- `GET /pods/<p>/records` (`{records: [{name, description, key, rows, columns}], problems}`) and
  `GET /pods/<p>/records/<set>?limit=&sql=` (`{set, columns, rows, cut}`; one read-only `SELECT`;
  `limit` 1 to 1000); every row carries `_task`, the id of the task whose hop wrote or last updated
  it (`pod-records.spec.md` requirements 33, 50-53).
- `POST /pods`: `blueprint` is optional and defaults to docket's `DEFAULT_BLUEPRINT`
  (`serve-read-api.spec.md` "POST /pods"); P56-21 (TODO on 2026-10-30) makes the registry `build`,
  `respond`, `operate`, `routine`. P56-15 (merged) deleted `critic` and `tester`.
- `hop_finished`'s `verdict` is now only the reviewer's; its payload is otherwise unchanged.

## Considered and not adopted

- **Configuring services, secrets, send quotas, MCP lifetimes or a team's network from Tack.**
  They are operator configuration with their own review in docket (`docket setup service`,
  `docket setup secret`, `pod set network open --widen`, a `[y/N]` no repository can write). A
  second writer is the problem ADR 0030 decision 5 already refused.
- **Showing routines, or the tasks a routine creates.** No HTTP route lists routines, and a task a
  team creates on its own schedule is not an item of the board. It is docket's to show.
- **Sending `variables` on a task.** No field of an item maps to a pipeline variable, and the
  pipeline per type (`pipeline_by_type`) is already the lever.
- **A "Resume now" button for a wait.** It would put in the person's queue something docket says
  needs nobody; ending a wait early is docket's (`docket task answer`).
- **Reading every row of a team's records into Tack, or writing to them.** A run shows the rows it
  wrote; the rest stay where the team keeps them, and only the team's own tools write them.
- **Creating a folder team from Tack.** A folder team's shape, records and services are written by
  the onboarding skill or in docket; Tack's `ensure` keeps creating the default code team.
