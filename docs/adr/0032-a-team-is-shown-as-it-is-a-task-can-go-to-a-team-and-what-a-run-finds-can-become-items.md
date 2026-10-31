# ADR 0032: a team is shown as it is, a task can go to a team, and what a run finds can become items

**Status: PROPOSED 2026-10-31.** Executed by Phase 83 (`docs/plans/phase-83.md`) once accepted.
Amends ADR 0021 decisions 4 and 8. Leaves ADR 0031 decision 5 (Tack names no team shape) and its
"Considered and not adopted" as they are.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **A team is shown as it is, in Tack's words.** The runner reads `GET /pods` and `GET /pods/<p>` and reports, per team: its members (role and model), where it works (repository or folder), whether it runs lights-out or needs a person, its network, its queue, its record sets, and — when docket reports them — its steps and the shape it was made from. The board lists every team any runner reports (`GET /api/teams`) with the profiles that name it. ADR 0021 decision 8 changes from "no docket word reaches the screen" to "every docket word is translated, and nothing about the team is hidden" | Agents → **Teams**: one card per team, saying who is in it, where it works, how it decides, what it keeps and what it does step by step |
| 2 | **Teams come from docket, not from other profiles.** The profile's Team field offers every team a runner reports, plus the names other profiles already use (a machine that is off). A reported team with no profile has **Make a profile for this team**: agent docket, that team, the Implementer's tools and the limits any new profile gets | A team made on the team's computer appears in Tack without typing its name anywhere |
| 3 | **A task can go to a team directly.** A **Team** field on the new-item dialog and on the item's Automation row stores, in the item's run settings, the profile that names that team (the first source of ADR 0021 decision 3; the order does not change). A team named by several profiles asks which. Auto-start also takes an item whose run settings name a profile, not only an item with a role (amends ADR 0021 decision 4) | "This task goes to saas-research" chosen on the card itself, with or without a role, and it can start by itself |
| 4 | **What a run finds can become items, and a person chooses which.** The proposal is `tack-plan.json` (plan-v1), which gains four optional keys per entry: `type`, `role`, `team` and `parent` (the index of another entry, for a sub-item). `v` stays `"1"`; a file without them reads as today. Accepting creates the chosen entries under the run's item with their type (today: the parent's), their role, their team's profile in run settings, nested by `parent`, and `depends_on` as blockers (today dropped). The Planner's and the Researcher's instructions say how to write it | A research run on a feature or a campaign ends with "The team proposes 9 items"; the person ticks, edits and creates them, each already going to its team |
| 5 | **A docket team proposes through its records.** A team that declares a record set named `tack_proposals` (columns `ref`, `title`, `description`, `type`, `role`, `team`, `parent`, `depends_on`, `acceptance`) proposes items by adding rows to it. Tack already reads, at settle, the rows a task wrote (ADR 0031 decision 4); it turns that set's rows into the same plan artifact, so there is one way to accept. The onboarding skill adds the set to every team that researches or plans | A folder team (research, campaign) proposes items exactly as the Planner does, with nothing new in docket |
| 6 | **A run shows the team's steps as it goes.** When docket reports the team's steps, the run card lists them in order with the one running, who decides at each gate, and the rework cycles used; without them, the steps seen so far, as today, and a line saying this docket does not report its steps | "Research ✓ → Draft (running) → Review (can send back, 0 of 2) → Act (asks you)" |

**Why now.** The analysis of 2026-10-31 found that Tack knows of a team only its name, its verdict,
the folders it reads and its record sets. Members, models, pipeline, gates, approval mode and queue,
all in docket's `GET /pods/<p>`, are not read (`harness/docket/wire.rs` reads `profile`,
`readOnly`, `repoConfigDir`; `GET /pods` only `name`). The profile's Team field offers only the
teams other profiles already name, so a team nobody named cannot be chosen. A task reaches a team
only through a role; auto-start ignores a task without one. And the plan a Planner proposes loses
its dependencies and takes the parent's type; a docket team cannot propose at all, because the plan
file is read only from the runner's own work tree and a folder team has none.

**If you do nothing.** A person cannot see what a team does before giving it work; a team made on
its computer is invisible until someone types its name; a research or campaign team's findings stay
prose that a person copies into items by hand.

## What this asks of docket

Neither ask blocks a card; each is read when present, and the screen says so when absent.

1. `blueprint` on `GET /pods` and `GET /pods/<p>`: the shape the pod was provisioned with (the
   Lead's `blueprint` meta, `teams/blueprints.py`). Without it Tack shows where the team works and
   its members' roles.
2. `pipeline.steps` on `GET /pods/<p>`: the effective pipeline as data (per step: id, kind —
   role, command, input, wait, parallel —, role, model class, gate kind, rework target and
   maximum). Today `pipeline` carries only `source`.

## Evidence (read 2026-10-31 on `develop` at `9bab0965`, docket at its working tree)

| Fact | Where |
|---|---|
| `GET /pods/<p>` returns `members` (`id`, `role`, `model.value`, `model.source`, `verifyCmd`), `settings`, `approvalMode`, `pipeline.source`, `network`, `profile`, `repoConfigDir`, `readOnly` | docket `hops/application/report.py` `pod_report`, `pod_detail` |
| `GET /pods` returns `name`, `codebase`, `members` (a count), `profile`, `queue` | same, `pod_summary` |
| Tack reads `profile`, `readOnly`, `repoConfigDir` of one, `name` of the list | `crates/tack-runner/src/harness/docket/wire.rs` `PodReport`, `PodRow` |
| `TeamReport` is `name`, `profile`, `violations`, `read_only`, `works_in`, `records` | `crates/tack-protocol/src/execution/capabilities.rs` |
| The profile's Team options are the teams the board's profiles name | `frontend/src/features/agents/runnerFleet/AgentProfilesPanel.tsx` `teams()` |
| A profile is resolved from run settings, then role, then project default, then Implementer | `crates/tack-app/src/runs/start_run.rs` `resolve_profile` |
| Auto-start reads only items waiting with a role | `crates/tack-app/src/auto_start.rs` (`waiting_with_a_role`) |
| The new-item dialog has no role, profile or team field | `frontend/src/features/board/CreateItemModal.tsx` |
| Subtasks from a plan take the parent's type and keep title, description and acceptance; `depends_on` and `estimate` are dropped | `crates/tack-api/src/handlers/items.rs` `create_subtasks_from_plan` |
| The plan file is read only when the workspace has a work tree (`head` is `None`) | `crates/tack-runner/src/evidence.rs` `capture` |
| Rows a docket task wrote are read at settle, at most 50 per set | ADR 0031 decision 4; `harness/docket/records.rs` |
| The Planner's instructions name `tack-plan.json`; the Researcher's do not | `crates/tack-db/src/repo/execution/claim.rs` |

## Considered and not adopted

- **Giving a team Tack's MCP server so it creates items itself.** The board's API has no
  authentication on loopback (security audit 2026-10-12), and items would appear without anyone
  choosing them. A proposal a person accepts is the one way in.
- **Creating a team from a shape in Tack.** ADR 0031 decision 5 and its "Creating a folder team from
  Tack" stand: the onboarding skill or docket creates teams; Tack's `ensure` keeps making the default.
- **Reading the pipeline file from the team's repository.** A second reader of docket's format,
  which drifts from it; docket reporting its steps is the one source.
- **The project file as the proposal format.** It carries configuration (fields, profiles,
  automation) and has no "these, not those"; plan-v1 already has the accept panel and the
  run-to-items link.
- **A pipeline chooser per task, or `variables`.** ADR 0031 stands: `pipeline_by_type` is the lever.
  Editing it on the profile form waits until docket reports which pipelines a team has.
- **Creating proposed items without a person.** Even at L4: a proposal changes the board's scope,
  not the code, and the ladder governs merges.
