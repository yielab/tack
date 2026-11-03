# ADR 0032: a team is shown as it is, a task can go to a team, and what a run finds can become items

**Status: ACCEPTED 2026-10-31** (proposed and rewritten the same day: every decision is harness-neutral).
Executed by Phase 83 (`docs/closed-cycles/plans/phase-83.md`). Amends ADR 0021 decisions 4 and 8.
Leaves ADR 0031 decision 5 (Tack names no team shape) and its "Considered and not adopted" as they
are.

**The rule behind every decision.** Tack describes a team, a task's assignee and a proposal in its
own harness-neutral terms. Each harness's adapter fills what its agent can say, says through its
capabilities what it cannot, and the screen follows what was reported: a control a harness cannot
serve is shown disabled with the harness's reason, never hidden and never faked. docket is the first
harness that reports a team; nothing on the board or in the API names docket.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **One description of a team, from any harness.** `TeamReport` (already a field of every harness's capability) gains neutral fields, all optional: `members` (role, model), `steps` (name, who does it, what checks it: a command, a reviewer, a person, an external check; where it can send work back and how many times), `works_in` (repository or folder), `decides` (runs to the end, or stops for a person), `network`, `queue`, `records`, and `shape` (a label the harness gives, shown, never sent). Absent means "the harness did not say". docket fills them from `GET /pods`; a harness with no teams reports none, and its profile is shown as one agent with its model. `GET /api/teams` lists every team any runner reports, with the profiles that name it. ADR 0021 decision 8 becomes "every harness word is translated, and nothing the harness reports is hidden" | Agents → **Teams**: one card per team, from whichever agent reports it |
| 2 | **A task says who does it.** The new-item dialog and the item's Automation row get **Who does it**: every profile, grouped by the team it names (or by its agent, for a profile with no team), each group saying what it is made of. Choosing stores the profile in the item's run settings, the first source of ADR 0021 decision 3 (order unchanged). Auto-start takes an item with a profile in its run settings as it takes one with a role (amends ADR 0021 decision 4). A team no profile names offers **Make a profile for this team** | A task goes to the research team, the code team or a single agent from the card itself, and can start by itself without a role |
| 3 | **One proposal, several ways in, one place to accept.** The proposal format is plan-v1 (`tack-plan.json`), which gains four optional keys per entry: `type`, `role`, `team` and `parent` (the index of another entry, for a sub-item); `v` stays `"1"`. It reaches an item in one of three ways: a run's file, for an agent that works in a work tree (Claude Code, Codex, opencode); a run's records, for a team that keeps records and declares a set named `tack_proposals` (docket); a file a person uploads on the item, for anything else. Each adapter turns its way into the same `plan` artifact, and reports `features.proposals` (supported, or unsupported with its reason). The accept endpoint and the panel do not know which harness proposed. Accepting creates the chosen entries under the item with their type, role, team (its profile into run settings), nesting, and `depends_on` as blockers. Writing items by hand stays as it is | A research run on a feature or a campaign ends with "The team proposes 9 items", whatever agent ran it; the person ticks, edits and creates them, each already going to whoever does it |
| 4 | **Nothing proposed becomes an item without a person.** No harness, profile, level or MCP tool creates items from a proposal on its own | Scope changes only when someone chooses it |

**Why now.** The analysis of 2026-10-31 found that Tack knows of a team only its name, its verdict,
the folders it reads and its record sets; that a task reaches a team only through a role and
auto-start ignores a task without one; that the plan a Planner proposes loses its dependencies and
takes the parent's type; and that a team which works in a folder cannot propose at all, because the
plan file is read only from the runner's work tree.

**If you do nothing.** A person cannot see what a team does before giving it work; a task cannot be
sent to a team without inventing a role; a research or campaign run's findings stay prose that a
person copies into items by hand.

## What this asks of docket

Neither ask blocks a card; each is read when present, and the screen says so when absent.

1. `blueprint` on `GET /pods` and `GET /pods/<p>`: the shape the pod was provisioned with (the
   Lead's `blueprint` meta, `teams/blueprints.py`), read into `shape`.
2. `pipeline.steps` on `GET /pods/<p>`: the effective pipeline as data (per step: id, kind,
   role, model class, gate kind, rework target and maximum), read into `steps`. Today `pipeline`
   carries only `source`.

On 2026-10-31 docket's `develop` (`df17a145`) carried neither; it did add `workspaceKind` to both
routes (see ADR 0031's evidence), which `works_in` reads first, falling back to `repoConfigDir` and
`codebase` for a docket that does not send it.

## Evidence (read 2026-10-31 on `develop` at `9bab0965`, docket at its working tree)

| Fact | Where |
|---|---|
| Every harness capability has `teams: Option<Vec<TeamReport>>` and `features` (`cancel`, `resume`, `decisions`, `artifacts`, `usage`, each supported or not with a reason) | `crates/tack-protocol/src/execution/capabilities.rs` |
| `TeamReport` is `name`, `profile`, `violations`, `read_only`, `works_in`, `records`; only docket fills it | same; `crates/tack-runner/src/harness/docket/probe.rs` |
| `GET /pods/<p>` returns `members` (`role`, `model`), `approvalMode`, `pipeline.source`, `network`, `profile`, `repoConfigDir`, `readOnly`; `GET /pods` returns `codebase`, a member count and `queue` | docket `hops/application/report.py` |
| Tack reads `profile`, `readOnly`, `repoConfigDir` of one pod, `name` of the list | `crates/tack-runner/src/harness/docket/wire.rs` |
| The profile's Team options are the teams the board's profiles name | `frontend/src/features/agents/runnerFleet/AgentProfilesPanel.tsx` `teams()` |
| A profile is resolved from run settings, then role, then project default, then Implementer | `crates/tack-app/src/runs/start_run.rs` `resolve_profile` |
| Auto-start reads only items waiting with a role | `crates/tack-app/src/auto_start.rs` (`waiting_with_a_role`) |
| The new-item dialog has no role, profile or team field | `frontend/src/features/board/CreateItemModal.tsx` |
| Subtasks from a plan take the parent's type; `depends_on` and `estimate` are dropped | `crates/tack-api/src/handlers/items.rs` `create_subtasks_from_plan` |
| The plan file is read only from a workspace with a work tree | `crates/tack-runner/src/evidence.rs` `capture` |
| Rows a docket task wrote are read at settle, at most 50 per set | ADR 0031 decision 4; `harness/docket/records.rs` |
| The Planner's instructions name `tack-plan.json`; the Researcher's do not | `crates/tack-db/src/repo/execution/claim.rs` |

## Considered and not adopted

- **A docket-shaped team model in Tack** (pods, hops, archetypes, blueprints as Tack types). It
  would bind the board to one harness; the neutral fields carry what a person needs and docket's own
  names arrive as data.
- **Inventing a team for a harness that has none** (a "team of one" with a made-up pipeline). A
  profile with no team is shown as what it is: one agent and its model.
- **Letting a run create items itself**, through Tack's MCP server or any tool. The board's API has
  no authentication on loopback (security audit 2026-10-12), and items would appear without anyone
  choosing them. An MCP tool that *proposes* (a fourth way in to decision 3) is possible later; it is
  not needed for this phase.
- **Creating a team from a shape in Tack.** ADR 0031 decision 5 stands.
- **Reading a pipeline file from the team's repository.** A second reader of a harness's own format;
  the harness reporting its steps is the one source.
- **The project file as the proposal format.** It carries configuration and has no "these, not
  those"; it stays the way to move a whole project.
- **A pipeline chooser per task, or `variables`.** ADR 0031 stands: `pipeline_by_type` is the lever.
