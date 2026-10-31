# Phase 83: a team is shown as it is, a task can go to a team, and what a run finds can become items

**Status: OPEN 2026-10-31; ADR 0032 accepted the same day.** The ADR holds the decisions; this plan is the cards.
H, I, K and L (round 0) are built and in `develop`; they fixed what onboarding runs on
2026-10-31 found (a folder team, ADR 0031 decision 1; a run queued for 52 minutes behind a question
nobody saw, then failed in half a second), which accepted decisions already cover, and wait on
nothing.
docket's two asks (ADR 0032 "What this asks of docket") block no card: each card reads them when
present and says so on screen when absent. Tack edits nothing in docket's or prill's repositories.

**One rule for every card (ADR 0032).** The board and the API speak of teams, who does a task and
proposals in harness-neutral terms. Only a harness's adapter knows its agent: it fills the neutral
fields it can and reports through its capabilities what it cannot. A screen shows what was reported
and renders the rest disabled with the harness's reason. No card adds a docket word to the API, the
database or the screen; docket's own role and shape names arrive as data.

Twelve cards, sized so that one Sonnet or Haiku agent finishes each in one sitting with the files it
names. Six rounds; round 0 runs in two halves.

## How every card runs

Phase 82's rules, unchanged (`docs/closed-cycles/plans/phase-82.md`, "How every card runs"):
read only the files the card names (about 25k tokens), stop and report at about 150k; while working
run only the tests of what the card touched (`cargo nextest run --workspace -E 'package(<crate>) &
test(/^<module>::/)'`, `--build-jobs 4 --test-threads 4`, `CARGO_INCREMENTAL=0`; `npx vitest run
<paths>`); the full suites run once per round on the merged tree by whoever integrates; a card that
changes the HTTP API runs `./scripts/regen-generated.sh`; at most two cards build at once, each in
its own worktree cut from `develop`; no card opens a window; each card updates the guide paragraph
its change makes true and adds its line to `CHANGELOG.md`'s `[Unreleased]`; one commit per card,
`Type: what is now true`, no attribution trailer. CI never needs docket or prill: the fake docket
in `crates/tack-runner/tests/fake_docket/` answers every new shape, and G recaptures against the
real one.

## Rounds

| Round | Cards | Gate |
|---|---|---|
| 0 | **H ∥ I**, then **K ∥ L** (H owns the run's start in `harness/docket.rs` and `task.rs`, I owns `shared/runWithAgent/RunFlow.tsx` and `install.ts`; K owns the queued run's reason, L the settle's `approval_denied` arm in `harness/docket.rs`, after H is merged); starts now, before ADR 0032 is accepted | full suites once per half on the merged tree |
| 1 | **A ∥ D** (`capabilities.rs` is split: A edits only `TeamReport` and its new types, D only `FeatureCapabilities`; whoever merges second rebases) | full suites once on the merged tree |
| 2 | **B ∥ J** (B owns `features/agents/`; J owns docket's records and settle; the fake docket: J only the records routes) | full suites once |
| 3 | **C ∥ E** (C owns `features/board/CreateItemModal.tsx`, `item-detail/AutomationRow.tsx` and auto-start; E owns `PlanPanel.tsx` and a new `item-detail/ProposeFromFile.tsx`, plus the one line in the item page that places it) | full suites once |
| 4 | **F** | full suites once |
| 5 | **G** | full suites once; the local suite of the three |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | One description of a team, filled by docket | Sonnet | `tack-protocol` `capabilities.rs` (`TeamReport` and new `TeamMember`, `TeamStep`), `tack-runner/src/harness/docket/` (`wire.rs`, `client.rs`, `probe.rs`), a new `tack-api` `handlers/teams.rs` and its route, the fake's pod routes, `docket_v5.rs` (pod tests), the developer guide's capabilities and docket pages | — |
| D | One proposal format, accepted the same way from anywhere | Sonnet | `tack-protocol` `capabilities.rs` (`FeatureCapabilities.proposals`), `tack-runner/src/evidence.rs` (`PlanV1`), where the three local agents report `features`, `docs/contracts/plan-v1/`, `plan_contract.rs`, `tack-api` `handlers/items.rs` (`create_subtasks_from_plan`), `tack-db` `repo/execution/claim.rs` (Planner and Researcher instructions), the developer guide's plan contract | — |
| B | Agents → Teams, from whatever the agents report | Sonnet | `frontend/src/features/agents/` (a new `TeamsPanel.tsx`, `AgentsPage.tsx`, `runnerFleet/AgentProfilesPanel.tsx`), `settings/panels/automation/Teams.tsx` (options only), `shared/execution/` team types, the user guide's teams section | A |
| J | A docket team proposes through its records | Sonnet | `tack-runner/src/harness/docket/records.rs`, `harness/docket.rs` (settle only), docket's `features` report, the fake's records routes, `docket_v5.rs` (records tests), `onboarding-lab/skill/` references | D |
| C | A task says who does it | Sonnet | `frontend/src/features/board/CreateItemModal.tsx`, `features/item-detail/AutomationRow.tsx`, `tack-app/src/auto_start.rs`, the `tack-db` query behind `waiting_with_a_role`, the user guide's automation "Start" section | A, B |
| E | The person chooses what becomes an item, from a run or a file | Haiku | `frontend/src/shared/runWithAgent/PlanPanel.tsx`, `AttemptList.tsx` (the plan line only), a new `features/item-detail/ProposeFromFile.tsx`, the user guide's "A Planner run" section, renamed "When a run proposes items" | A, D |
| F | A run shows the team's steps as it goes | Haiku | `frontend/src/shared/runWithAgent/` (`ExecutionTimeline.tsx`, `LatestRunSummary.tsx`), `shared/execution/types.ts`, the user guide's Execution tab section | A |
| G | The local suite proves it against docket | Sonnet | `onboarding-lab/e2e/` (script, spec), recaptured fixtures under `crates/tack-runner/src/harness/fixtures/docket/`, `TESTING.md`'s live-suite section | all |
| H | A folder team's run starts | Sonnet | `tack-runner/src/harness/docket.rs` (the run's start only: `start`, `has_repository`), `harness/docket/task.rs` (`body`), the fake's `POST /tasks/<pod>` rule, a fixture under `harness/fixtures/docket/v5/`, `docket_v5.rs` (task tests), ADR 0031's evidence line on `GET /pods/<p>`, the administrator guide's folder-team paragraph | — |
| I | The run dialog says where the team works | Haiku | `frontend/src/shared/runWithAgent/RunFlow.tsx` (`where`), `install.ts`, their tests, the user guide's "Run with agent" paragraph | — |
| K | A queued run says what it waits behind | Sonnet | `tack-app/src/scheduler_wiring.rs` (a new pure function beside `waiting_note`), the request view in `tack-api` `handlers/executions.rs`, a `tack-db` query for a runner's open attempts, `shared/runWithAgent/LatestRunSummary.tsx` (the queued line only), the user guide's "waiting" paragraph | — |
| L | A question nobody answered does not read as "denied" | Sonnet | `tack-runner/src/harness/docket.rs` (the settle's failure arms only), `harness/docket/tests.rs`, the fake's approval expiry, the user guide's decisions paragraph | H |

## Left out, and why

Each is in ADR 0032 "Considered and not adopted": a docket-shaped team model in Tack; a made-up
team for a harness that has none; a run creating items itself (an MCP tool that only *proposes* is a
possible later way in); creating a team from a shape in Tack; reading a pipeline file from the
repository; the project file as the proposal format; a pipeline chooser per task and `variables`.
There is no docs card and no measurement card.

**Done when:** a task given to a team that works in a folder starts and ends Succeeded with what
the team handed off, and the run dialog says where it will work before it starts; a queued run
names the run it waits behind, and the question that one waits on; a question nobody answered ends
its run as "nobody answered", never as "denied"; Agents → Teams
shows every team any agent reports with what it reported (members, models, where it works, how it
decides, queue, records, and steps when reported) and every profile with no team as one agent; a
reported team with no profile can get one in one click; a new item can be given whoever does it in
the dialog and starts by itself without a role; a Planner or Researcher run, a docket team with a
`tack_proposals` set, and a file a person uploads all end in the same panel, whose accepted entries
become items under the item with their type, role, assignee, nesting and blockers; every agent
reports whether its runs can propose; a run whose team reports steps shows them with the one
running; and the local suite passes against docket's current checkout, naming its commit and which
of the two asks it found.

---

## Cards

### A. One description of a team, filled by docket

**Model:** Sonnet. **Waits on:** nothing. ADR 0032 decision 1 (the server side).

**Why.** `TeamReport` is already a field of every harness's capability, but it carries only a name,
a verdict, the folders it reads, where it works and its record sets; `wire.rs` drops everything else
docket reports, and the board has no list of teams, only the names inside profiles.

**Read first:** ADR 0032; `tack-protocol` `execution/capabilities.rs` whole; `harness/docket/wire.rs`,
`client.rs`, `probe.rs`; `tack-api` `handlers/autonomy.rs` (`TeamLine`, how it reads runner
capabilities); the fake's pod routes; docket's `hops/application/report.py` `pod_summary`,
`pod_report`, `pod_detail` (read only).

**Does.**
1. `TeamReport` gains, all optional and in neutral words: `members: Vec<TeamMember>` (`role`,
   `model`); `decides` (`to_the_end` or `asks_a_person`, from the profile verdict it already has);
   `network` (`none`, `limited`, `open`); `queue` (`waiting`, `running`); `shape` (a label, shown,
   never sent); `steps: Vec<TeamStep>` (`name`, `kind`: `agent`, `command`, `ask`, `wait`,
   `parallel`; `role`; `checked_by`: `command`, `reviewer`, `person`, `external` or none;
   `sends_back_to` and `sends_back_max`). Absent means "the harness did not say". The doc comment
   of each field says so, and that no field may carry a harness's own key name.
2. docket's adapter fills them: `PodRow` reads `codebase`, the member count and `queue`;
   `PodReport` reads `members`, `approvalMode.effective`, `network.mode` and, when present,
   `blueprint` (into `shape`) and `pipeline.steps` (mapped into `TeamStep`, docket's gate kinds to
   `checked_by`). The probe fills every pod `GET /pods` lists, not only those a profile names. An
   older docket still probes.
3. `GET /api/teams`: every team any active runner reports, once per name and harness (the newest
   report wins), each with its harness, the runner that reports it and the profiles that name it;
   plus the names profiles use that no runner reports, marked as such. Regenerate.
4. The fake's pod routes answer the new keys; one variant omits `blueprint` and `pipeline.steps`.
5. Tests: a pod with every key reads into a full `TeamReport`; a pod without the asks reads with
   `shape` and `steps` absent; `GET /api/teams` joins two runners and a profile-only name; a harness
   with no `teams` contributes none.

### D. One proposal format, accepted the same way from anywhere

**Model:** Sonnet. **Waits on:** nothing. ADR 0032 decisions 3 and 4 (the format, the endpoint, the
local agents).

**Why.** The plan file is the one way a run proposes items, but accepting drops `depends_on`, forces
the parent's type and cannot nest; only the Planner is told about it; and nothing says which agents
can propose, so a screen cannot say why a run did not.

**Read first:** ADR 0032; `docs/contracts/plan-v1/` (README, schema, example); `evidence.rs`
(`PlanV1`, the plan read in `capture`); `handlers/items.rs` `create_subtasks_from_plan`;
`tack-db/src/repo/dependencies.rs` (how a blocker is written); `claim.rs` (built-in instructions);
`capabilities.rs` (`FeatureCapabilities`) and where Claude Code, Codex and opencode fill it.

**Does.**
1. `PlanSubtask` gains optional `type`, `role`, `team` (strings) and `parent` (an index into the
   list, earlier than the entry). Schema and README say so, and that the format belongs to no
   harness; `v` stays `"1"`; the frozen example is unchanged and a second example carries the new
   keys, pinned like the first.
2. The accept request carries the same four plus `depends_on`, and `artifact_id` becomes optional:
   absent means the proposal came from a file the person uploaded, and the items' source says so.
   Creating: in list order, each under its `parent`'s new item or under the item; `type` when the
   project's workflow has it, else the item's; `role` by name when the project has it; `team`
   resolved to the one profile naming it into the item's run settings (several or none: left empty
   and named in the response); `depends_on` written as blockers between the new items. An unknown
   role, type or team is reported per entry in the response, never a failure of the whole request.
3. `FeatureCapabilities.proposals`: Claude Code, Codex and opencode report it supported when the run
   has a work tree and unsupported, with the reason, when it works in place with no tree; a harness
   that does not report it reads as unsupported with "this agent does not say".
4. The Researcher's built-in instructions gain one sentence: when the findings lead to work, write
   `tack-plan.json` as the Planner does. The Planner's gain the four keys.
5. Tests: the second example round-trips; a nested plan with dependencies creates the tree and the
   blockers; an accept with no `artifact_id` works; an unknown role is reported, not refused; each
   local agent reports `proposals`.

### B. Agents → Teams, from whatever the agents report

**Model:** Sonnet. **Waits on:** A. ADR 0032 decisions 1 and 2 (the screen).

**Read first:** ADR 0032; `features/agents/AgentsPage.tsx`, `runnerFleet/AgentProfilesPanel.tsx`
(`teams()`, the Team field), `FleetsPanel.tsx` (the card pattern); `settings/panels/automation/
Teams.tsx`; `shared/help/texts.ts`.

**Does.**
1. A **Teams** section on the Agents page, one card per team from `GET /api/teams`, each line
   rendered only from a reported field: the name, the agent that runs it and the shape when
   reported; "Works in this repository" or "Works in a folder"; the members as "Lead · <model>";
   "Runs to the end by itself" or "Stops to ask a person" with the fix lines; the network; "3
   waiting, 1 running"; the record sets; "Can propose items" or the reason it cannot; the steps in
   order with what checks each one. A field the agent did not report is one disabled line with "Not
   reported by <agent>", never left out.
2. A profile with no team is listed under its agent as "One agent · <model>".
3. A reported team with no profile: **Make a profile for this team** creates one (that agent, that
   team, the Implementer's tools) and opens it for editing.
4. The profile's Team field and the Teams rung read `GET /api/teams`: reported teams first, then
   names only profiles use, marked "not reported by any computer".

### J. A docket team proposes through its records

**Model:** Sonnet. **Waits on:** D. ADR 0032 decision 3 (the docket way in).

**Why.** A team that works in a folder has no work tree, so the plan file is never read; and a team
that works in a repository would merge a plan file with its branch. docket teams keep records, which
Tack already reads at settle.

**Read first:** ADR 0032; D's contract and `PlanV1`; `harness/docket/records.rs` and the settle in
`harness/docket.rs`; `evidence.rs` (`capture`, the plan read); the fake's records routes.

**Does.**
1. At settle, when the team declares a set named `tack_proposals`, its rows for this task (already
   read) become one plan-v1 document (`ref`→index, `parent` and `depends_on` refs→indexes,
   `acceptance` split on new lines) staged as the attempt's `plan` artifact, exactly as a file
   would be. Rows that do not convert yield `attempt.plan_invalid` naming the row's `ref`.
2. A `tack-plan.json` on a docket task's branch is not read as a proposal; check `capture` and make
   it so, with the reason in one comment.
3. docket reports `features.proposals` supported, with the reason "through a team's
   `tack_proposals` record set"; whether a given team uses it is in that team's `records`.
4. `onboarding-lab/skill/` references: the `tack_proposals` record document (columns `ref`,
   `title`, `description`, `type`, `role`, `team`, `parent`, `depends_on`, `acceptance`), added to
   every team the skill makes that researches or plans.
5. Tests: a fake task with `tack_proposals` rows stages a plan artifact equal to the file form; a
   task with none stages nothing; a bad `parent` ref yields `plan_invalid`.

### C. A task says who does it

**Model:** Sonnet. **Waits on:** A, B. ADR 0032 decision 2.

**Read first:** ADR 0032; `CreateItemModal.tsx`; `item-detail/AutomationRow.tsx`; how the run dialog
saves run settings (`RunWithAgentModal.tsx`, read only); `tack-app/src/auto_start.rs` and the
query `waiting_with_a_role`; `start_run.rs` `resolve_profile` (read only).

**Does.**
1. **Who does it** in the new-item dialog and on the item's Automation row: every profile, grouped
   by the team it names (with what the team is made of, from `GET /api/teams`) or by its agent for a
   profile with no team. Choosing stores the profile in the item's run settings; "Nobody yet" clears
   it. A reported team with no profile is shown disabled with "Make a profile for it on the Agents
   page".
2. The Automation row says where the choice came from: "chosen on this task" or "from the role
   Content".
3. Auto-start takes items that wait in the trigger status with a role **or** with a profile in run
   settings; everything else it checks is unchanged. The activity sentence names the team or the
   agent when there is no role.
4. Tests: an item created with a choice carries the profile; an item with run settings and no role
   starts by itself; one with neither does not.

### E. The person chooses what becomes an item, from a run or a file

**Model:** Haiku. **Waits on:** A, D. ADR 0032 decisions 3 and 4 (the screen).

**Read first:** `PlanPanel.tsx`, its test, `AttemptList.tsx` (where it is shown); the item page's
layout; D's request and response shapes in the regenerated schema.

**Does.**
1. The panel title says who proposes: "The team saas-research proposes N items", "The Planner
   proposes …", or "This file proposes …".
2. Each row shows, editable, its type (the project's types), its role (the project's roles), who
   does it (the profiles, grouped as in C) and what it waits on; rows nest under their parent;
   unticking a parent unticks its children.
3. **Propose items from a file** on the item page: the person picks a `tack-plan.json`, it is read
   in the browser against the contract's schema, and the same panel opens; nothing is stored until
   the person creates. A file that does not match says which entry and why.
4. After creating: the created items as links, and one line per entry the server reported.

### H. A folder team's run starts

**Model:** Sonnet. **Waits on:** nothing. ADR 0031 decision 1, the start of the run (the decision
covered its end; nothing said what a run to a folder team must not send).

**Why.** Every task given to a folder team fails in two seconds without spending a token. The
runner reads that the team has no repository (`has_repository`, `repoConfigDir: ""`) and uses it
only when the run ends, while `task::body` always sends `baseRef`, the project's base commit. docket
refuses a `baseRef` it cannot cut a worktree from (`teams/workspace/worktree.py`,
`ensure_task_worktree`, since its Phase 46): "baseRef '<sha>' needs a task worktree, but the
Implementer has no codebase". The fake docket ignores `baseRef` for a folder pod, so no test saw it.
The project-level `prepare` (the install derived from the lockfile) goes too, and docket ignores it
for a task with no worktree.

**Read first:** `harness/docket.rs` (`start`, `has_repository`, `finish`'s first arm),
`harness/docket/task.rs` (`body`), the fake's `post_task` and `check_task_body`, the v5 fixtures
README (`repoConfigDir`), docket's `teams/workspace/worktree.py` `ensure_task_worktree` and
`tasks/application/queue.py` `prepare_claimed_task` (read only).

**Does.**
1. `start` asks `has_repository` before it builds the body, and `body` takes it: a team in a folder
   gets no `baseRef` and no `prepare`; a team with a repository, or a docket that does not say, gets
   both as today. The rework base (`rework_base`) is never sent to a folder team either.
2. The fake does what docket does: a task posted with `baseRef` to a folder pod is claimed and ends
   `failed` with `failureKind: "base_ref"` and docket's sentence, from a new fixture
   `task-failed-base-ref-folder.json` written from the real reply (G recaptures it).
3. ADR 0031's evidence says `codebase` is on `GET /pods/<p>`; the detail carries `repoConfigDir`
   (the list carries `codebase`). Correct that line.
4. Tests: a folder pod's task body has neither key and the attempt ends Succeeded with
   `outside_repository`; a repository pod's body keeps both; a docket that sends no `repoConfigDir`
   keeps both; the fake refuses a folder task that carries `baseRef`.

### I. The run dialog says where the team works

**Model:** Haiku. **Waits on:** nothing.

**Why.** Run with agent says "on a new branch of <repository>" and "Prepares with npm ci (from
package-lock.json)" for a profile whose team works in a folder, where there is no branch and no
install. The board already knows: the project's autonomy settings carry each team's `works_in`, which
Settings → Automation → Teams shows.

**Read first:** `shared/runWithAgent/RunFlow.tsx` (`where`, the profile chips), `install.ts`,
`features/settings/panels/automation/Teams.tsx` (`WorkNotes`, how it finds a profile's team),
`shared/api/autonomy.ts`.

**Does.**
1. When the chosen profile's team reports `works_in: "folder"`: the line says "in the team's own
   folder on this computer, not in this repository" and no install line is shown. With `repository`
   or no report, the line is as today.
2. Changing the profile updates the line.
3. Tests: a folder team's profile shows the folder line and no install; a repository team and a
   profile with no team keep today's lines.

### K. A queued run says what it waits behind

**Model:** Sonnet. **Waits on:** nothing. Phase 70's rule that a queued run says why it waits.

**Why.** On 2026-10-31 a run sat Queued for 52 minutes saying only "Waiting for a runner to pick it
up." The only runner runs one attempt at a time and was holding another run, parked on an approval
nobody saw for 57 minutes. The waiting reason is written only by `record_waiting`, inside
`choose_request_for_runner`, which runs only when a runner asks for work; a runner whose slots are
full does not ask (`transport.rs`, `taken < total`), so a request queued behind a busy runner never
gets a reason, and the screen falls back to the generic line (`LatestRunSummary.tsx`).

**Read first:** `tack-app/src/scheduler_wiring.rs` (`waiting_note`, `record_waiting`,
`describe_ineligible`); `tack-runner/src/transport.rs` (the claim loop, read only); the request
view in `handlers/executions.rs` (`waiting_code`, `waiting_reason`); `tack-db` decisions (a pending
decision per attempt); `LatestRunSummary.tsx`.

**Does.**
1. When the request view finds a queued request with no waiting reason, it works one out at read
   time, writing nothing: every runner the selector names, with its open attempts (item title,
   started at) and, for each, whether it waits on a person (the pending decision's question and
   since when). A pure function turns that into one sentence: "This computer runs 1 at a time and
   is busy with 'Corpus de referencia…' since 15:30, which waits for your answer since 15:33", with
   the busy attempt's item as a link. Several runners: one clause each.
2. The queued line shows that sentence and, when the busy run waits on a person, **Answer it**
   (the decision) beside **Run on another computer**. "Runs at a time" links to where the runner's
   limit is set.
3. Tests: a full runner with an attempt parked on a decision yields the sentence with the question;
   a full runner with a running attempt yields it without; a runner with room leaves the reason the
   poll wrote; the screen shows the sentence and the button.

### L. A question nobody answered does not read as "denied"

**Model:** Sonnet. **Waits on:** H (same file). Phase 80's rule that an unanswered question ends as
`not_answered`.

**Why.** The run that blocked the runner ended with "approval denied". Nobody denied anything: its
second approval was asked at 15:33 and never answered in Tack (no `escalation.resolved`), and
docket's approval timeout resolves to denied. Tack maps docket's `input_expired` to `not_answered`
but passes `approval_denied` through as is, so the person reads a refusal they never gave.

**Read first:** `harness/docket.rs` (the settle's `blocked` and `failed` arms, `not_answered`);
`harness/docket/tests.rs`; how the adapter knows whether it answered an escalation; the fake's
approval route.

**Does.**
1. When docket ends a task as `approval_denied` and the adapter did not send a denial for that
   escalation, the attempt ends with `not_answered`: "Nobody answered '<question>' (asked 15:33); the
   team stopped after <how long>." A denial a person gave keeps `approval_denied`.
2. Any docket failure code the adapter does not translate is shown as docket's text under "docket
   said:", never as Tack's own sentence.
3. Tests: an approval that expires unanswered ends `not_answered` with the question; one a person
   denied ends `approval_denied`; an unknown code keeps docket's text, labelled.

### F. A run shows the team's steps as it goes

**Model:** Haiku. **Waits on:** A. ADR 0032 decision 1 (the run's screen).

**Read first:** `ExecutionTimeline.tsx`, `LatestRunSummary.tsx`, `shared/execution/types.ts`
(`RunStep`), how the timeline already names a step from the run's events.

**Does.**
1. When the run's team reports `steps`: a line of steps in order above the timeline, each done,
   running or waiting, with what checks it in words ("checked by a command", "a reviewer can send it
   back, 1 of 2 used", "asks you"), matched to the step events the timeline already has.
2. Without reported steps, whatever the agent: the steps seen so far, as today, and "<agent> does
   not report its steps in advance".

### G. The local suite proves it against docket

**Model:** Sonnet. **Waits on:** every other card.

**Does.** Recapture the pod fixtures from docket's current checkout, and the folder refusal H
wrote; add to `docket-prill-e2e.sh` one folder team (its task starts with no `baseRef` and ends
Succeeded with no branch, H) with a `tack_proposals` set whose task adds three rows (one
nested, one depending on another); assert the plan artifact, accept it through the API and check
the three items, their nesting, their blocker and their profile; give one of them no role and
check it starts by itself. `--check` passes. The run names docket's commit and whether `blueprint`
and `pipeline.steps` were present.
