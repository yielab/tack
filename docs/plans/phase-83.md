# Phase 83: a team is shown as it is, a task can go to a team, and what a run finds can become items

**Status: PROPOSED 2026-10-31.** ADR 0032 (proposed) holds the decisions; this plan is the cards.
No card starts before the ADR is accepted. docket's two asks (ADR 0032 "What this asks of docket")
block no card: each card reads them when present and says so on screen when absent. Tack edits
nothing in docket's or prill's repositories.

Seven cards, sized so that one Sonnet or Haiku agent finishes each in one sitting with the files it
names. Four rounds.

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
| 1 | **A ∥ D** (the fake docket is split: A edits only the pod routes, D only the records routes and the plan fixtures; `harness/docket.rs`: D only the settle; whoever merges second rebases) | full suites once on the merged tree |
| 2 | **B ∥ C** (B owns `features/agents/`, C owns `features/board/` and `features/item-detail/`; both read A's `GET /api/teams`) | full suites once |
| 3 | **E ∥ F** (E owns `PlanPanel.tsx`, F the run card and timeline) | full suites once |
| 4 | **G** | full suites once; the local suite of the three |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | Tack reads the whole team | Sonnet | `tack-runner/src/harness/docket/` (`wire.rs`, `client.rs`, `probe.rs`), `tack-protocol` `capabilities.rs` (`TeamReport`), a new `tack-api` `handlers/teams.rs` and its route, the fake's pod routes, `docket_v5.rs` (pod tests), the developer guide's docket contract | — |
| D | A run's findings become a proposal, from any agent | Sonnet | `tack-runner/src/evidence.rs` (`PlanV1`), `docs/contracts/plan-v1/`, `tack-runner/src/harness/docket/records.rs` and `docket.rs` (settle only), `tack-api` `handlers/items.rs` (`create_subtasks_from_plan`), `tack-db` `repo/execution/claim.rs` (Planner and Researcher instructions), the fake's records routes, `plan_contract.rs`, `onboarding-lab/skill/` references | — |
| B | Agents → Teams, and teams come from docket | Sonnet | `frontend/src/features/agents/` (a new `TeamsPanel.tsx`, `AgentsPage.tsx`, `runnerFleet/AgentProfilesPanel.tsx`), `settings/panels/automation/Teams.tsx` (options only), the user guide's teams section | A |
| C | A task goes to a team | Sonnet | `frontend/src/features/board/CreateItemModal.tsx`, `features/item-detail/AutomationRow.tsx`, `tack-app/src/auto_start.rs`, the `tack-db` query behind `waiting_with_a_role`, the user guide's automation "Start" section | A |
| E | The person chooses what becomes an item | Haiku | `frontend/src/shared/runWithAgent/PlanPanel.tsx`, `AttemptList.tsx` (the plan line only), the user guide's "A Planner run" section | B, D |
| F | A run shows the team's steps as it goes | Haiku | `frontend/src/shared/runWithAgent/` (`ExecutionTimeline.tsx`, `LatestRunSummary.tsx`), `shared/execution/types.ts`, the user guide's Execution tab section | A |
| G | The local suite proves it against docket | Sonnet | `onboarding-lab/e2e/` (script, spec), recaptured fixtures under `crates/tack-runner/src/harness/fixtures/docket/`, `TESTING.md`'s live-suite section | A–F |

## Left out, and why

Each is in ADR 0032 "Considered and not adopted": a team writing items through Tack's MCP; creating
a team from a shape in Tack; reading the pipeline file from the repository; the project file as the
proposal format; a pipeline chooser per task and `variables`; creating proposed items without a
person. There is no docs card and no measurement card.

**Done when:** Agents → Teams shows every team a runner reports with its members, models, where it
works, how it decides, its queue, its records, and its steps when docket reports them; a profile's
Team field offers those teams and a team with no profile can get one in one click; a new item can be
given a team in the dialog and starts by itself when it reaches the trigger status without a role;
a Planner, a Researcher or a docket team with a `tack_proposals` set ends with a proposal whose
accepted entries become items under the run's item with their type, role, team, nesting and
blockers; a docket run shows its steps in order with the one running; and the local suite passes
against docket's current checkout, naming its commit and which of the two asks it found.

---

## Cards

### A. Tack reads the whole team

**Model:** Sonnet. **Waits on:** nothing. ADR 0032 decisions 1 and 2 (the server side).

**Why.** `wire.rs` reads three keys of `GET /pods/<p>` and one of `GET /pods`; everything else
docket says about a team is dropped, and the board has no list of teams, only the names inside
profiles.

**Read first:** ADR 0032; `harness/docket/wire.rs`, `client.rs`, `probe.rs`; `tack-protocol`
`execution/capabilities.rs` (`TeamReport`); `tack-api` `handlers/autonomy.rs` (`TeamLine`, how it
reads runner capabilities); the fake's pod routes; docket's `hops/application/report.py`
`pod_summary`, `pod_report`, `pod_detail` (read only).

**Does.**
1. `PodRow` reads `codebase`, `members` (count) and `queue`; `PodReport` reads `members` (`id`,
   `role`, `model.value`), `approvalMode.effective`, `network.mode`, and, when present, `blueprint`
   and `pipeline.steps`. Every new key is optional; an older docket still probes.
2. `TeamReport` gains `members: Option<Vec<TeamMember>>` (`role`, `model`), `approval`,
   `network`, `queue` (the counts docket sends), `shape: Option<String>` (docket's `blueprint`, shown
   as docket names it, never sent) and `steps: Option<Vec<TeamStep>>` (`id`, `kind`, `role`,
   `model_class`, `gate`, `rework_to`, `rework_max`). Absent means "docket did not say".
3. The probe fills them for every pod `GET /pods` lists, not only those a profile names.
4. `GET /api/teams`: every team any active runner reports, once per name (the newest report wins),
   each with the runner that reports it and the profiles whose `tool_policy.docket.team` names it;
   plus the names profiles use that no runner reports, marked as such. Regenerate.
5. The fake's pod routes answer the new keys; one variant omits `blueprint` and `pipeline.steps`.
6. Tests: a pod with every key reads into a full `TeamReport`; a pod without the asks reads with
   `shape` and `steps` absent; `GET /api/teams` joins two runners and a profile-only name.

### D. A run's findings become a proposal, from any agent

**Model:** Sonnet. **Waits on:** nothing. ADR 0032 decisions 4 and 5 (the server side).

**Why.** The plan file is the one way a run proposes items, but accepting drops `depends_on`, forces
the parent's type and cannot nest; the Researcher is never told about it; and a docket team, whose
work is not in the runner's work tree, cannot propose at all.

**Read first:** ADR 0032; `docs/contracts/plan-v1/` (README, schema, example); `evidence.rs`
(`PlanV1`, the plan read in `capture`); `handlers/items.rs` `create_subtasks_from_plan`;
`tack-db/src/repo/dependencies.rs` (how a blocker is written); `claim.rs` (built-in instructions);
`harness/docket/records.rs` and the settle in `harness/docket.rs`; the fake's records routes.

**Does.**
1. `PlanSubtask` gains optional `type`, `role`, `team` (strings) and `parent` (an index into the
   list, earlier than the entry). Schema and README say so; `v` stays `"1"`; the frozen example is
   unchanged and a second example carries the new keys, pinned like the first.
2. `CreateSubtaskFromPlan` carries the same four plus `depends_on`. Creating: in list order, each
   under its `parent`'s new item or under the run's item; `type` when the project's workflow has it,
   else the parent's; `role` by name when the project has it; `team` resolved to the one profile
   naming it into the item's run settings (several or none: left empty and named in the response);
   `depends_on` written as blockers between the new items. An unknown role, type or team is
   reported per entry in the response, never a failure of the whole request.
3. The Researcher's built-in instructions gain one sentence: when the findings lead to work, write
   `tack-plan.json` as the Planner does. The Planner's gain the four keys.
4. docket: at settle, when the team declares a set named `tack_proposals`, its rows for this task
   (already read) become a plan-v1 document (`ref`→index, `parent` and `depends_on` refs→indexes,
   `acceptance` split on new lines) staged as the attempt's `plan` artifact, exactly as the file
   would be. Rows that do not convert yield `attempt.plan_invalid` with the row's `ref`.
5. `onboarding-lab/skill/` references: the `tack_proposals` record document, added to every team the
   skill makes that researches or plans.
6. Tests: the second example round-trips; a nested plan with dependencies creates the tree and the
   blockers; an unknown role is reported, not refused; a fake docket task with `tack_proposals` rows
   stages a plan artifact; a task with none stages nothing.

### B. Agents → Teams, and teams come from docket

**Model:** Sonnet. **Waits on:** A. ADR 0032 decisions 1 and 2 (the screen).

**Read first:** ADR 0032; `features/agents/AgentsPage.tsx`, `runnerFleet/AgentProfilesPanel.tsx`
(`teams()`, the Team field), `FleetsPanel.tsx` (the card pattern); `settings/panels/automation/
Teams.tsx`; `shared/help/texts.ts`.

**Does.**
1. A **Teams** section on the Agents page, one card per team from `GET /api/teams`: the name and,
   when reported, the shape; "Works in this repository" or "Works in a folder"; the members as
   "Lead · cheap model", "Implementer · claude-sonnet-…"; "Runs to the end by itself" or "Stops to
   ask a person" with the fix lines; the network; "3 waiting, 1 running"; the record sets; the steps
   in order with who decides at each gate (or, disabled, "This docket does not report its steps");
   the profiles that name it, each a link; the computer that reports it.
2. A team with no profile: **Make a profile for this team** creates it (agent docket, the team, the
   Implementer's tools) and opens it for editing.
3. The profile's Team field and the Teams rung's notes read `GET /api/teams`: reported teams first,
   then names only profiles use, marked "not reported by any computer".
4. No docket word on screen: pod, hop, blueprint and archetype are never shown; docket's own role
   and shape names are shown as data, capitalised.

### C. A task goes to a team

**Model:** Sonnet. **Waits on:** A. ADR 0032 decision 3.

**Read first:** ADR 0032; `CreateItemModal.tsx`; `item-detail/AutomationRow.tsx`; how the run dialog
saves run settings (`RunWithAgentModal.tsx`, read only); `tack-app/src/auto_start.rs` and the
query `waiting_with_a_role`; `start_run.rs` `resolve_profile` (read only).

**Does.**
1. A **Team** field in the new-item dialog and on the item's Automation row: the teams of
   `GET /api/teams` with a profile, grouped "Works in this repository" / "Works in a folder", each
   showing its members' roles. Choosing one stores that profile in the item's run settings; a team
   named by several profiles asks which; "No team" clears it. A team without a profile is shown
   disabled with "Make a profile for it on the Agents page".
2. The Automation row says where the team came from: "Team saas-research, chosen on this task" or
   "… from the role Content".
3. Auto-start takes items that wait in the trigger status with a role **or** with a profile in run
   settings; everything else it checks is unchanged. The activity sentence says "with the team X"
   when there is no role.
4. Tests: an item created with a team carries the profile; an item with run settings and no role
   starts by itself; one with neither does not.

### E. The person chooses what becomes an item

**Model:** Haiku. **Waits on:** B, D. ADR 0032 decision 4 (the screen).

**Read first:** `PlanPanel.tsx`, its test, `AttemptList.tsx` (where it is shown); D's response
shape in the regenerated schema.

**Does.**
1. The panel title reads "The team proposes N items" (or "The Planner …" for a profile run).
2. Each row shows, editable, its type (the project's types), its role (the project's roles), its
   team (the teams with a profile) and what it waits on; rows nest under their parent; unticking a
   parent unticks its children.
3. After creating: the list of created items as links, and one line per entry the server reported
   (unknown role, team with several profiles).

### F. A run shows the team's steps as it goes

**Model:** Haiku. **Waits on:** A. ADR 0032 decision 6.

**Read first:** `ExecutionTimeline.tsx`, `LatestRunSummary.tsx`, `shared/execution/types.ts`
(`RunStep`), how the timeline already names a hop from the trace.

**Does.**
1. When the run's team reports its steps: a line of steps in order above the timeline, each done,
   running or waiting, the gate in words ("checks by command", "a reviewer can send back, 1 of 2
   used", "asks you"), from the hop events the timeline already has.
2. Without reported steps: the steps seen so far, as today, and "This docket does not report its
   steps".

### G. The local suite proves it against docket

**Model:** Sonnet. **Waits on:** A–F.

**Does.** Recapture the pod fixtures from docket's current checkout; add to
`docket-prill-e2e.sh` one folder team with a `tack_proposals` set whose task adds three rows (one
nested, one depending on another); assert the plan artifact, accept it through the API and check
the three items, their nesting, their blocker and their team's profile; give one of them no role and
check it starts by itself. `--check` passes. The run names docket's commit and whether `blueprint`
and `pipeline.steps` were present.
