# ADR 0036: the board orchestrates and a team declares what it accepts — one owner per concept, a method chosen per task, a limit at each level with its owner

**Status: PROPOSED 2026-11-03.** To be executed by Phase 89 (`docs/plans/phase-89.md`). Builds on
ADR 0021 (every run goes to a team), ADR 0024 (limits in tokens, each naming who applies it), ADR 0030
decision 5 (Tack configures no prompt, service or secret for a team), ADR 0032 (a team is shown as
it is; a person always accepts a plan) and ADR 0034 (a manager works through the board). Amends
ADR 0021's header (its decision 2 route was never built, its default pod and its gate changed) and
ADR 0024's decision table (decision 1 was overturned by ADR 0027 decision 5).

**The ask.** A person sets up a project with several teams — one that fixes code, one that
researches, one that answers customers — and sends each card to the right one, with the right
method, under limits they understand. Today the board and docket agree on the shape of this but not
on the words, Tack checks a team's fit against two facts of the dozen it needs, a method with a
missing role runs without that step and nobody is told, the team's budget pauses it for ever, and the
two places that route a card draw different lanes.

**The rule behind every decision.** Each concept answers one question and has one owner. Tack owns
*what* is done and *by whom* (items, dependencies, roles, profiles, plans a person accepted). docket
owns *how* a task is done inside a team (members, method, policies, step limits). Where one needs a
fact of the other, it reads it live, never copies or writes it. This is what the audit of 2026-11-03
found to work in the industry: one orchestrator holding explicit state contains errors that
independent agents amplify; writes stay in one thread per team; the verifier has a clean context;
a person approves the plan; the runtime, not the prompt, applies every limit.

| Concept | Question it answers | Owner |
|---|---|---|
| Item | What work, and what blocks it | Tack, per project |
| Role | Which area of the board | Tack, per project |
| Profile | Who does it, with which run limit | Tack, for the whole board |
| Team (docket pod) | Where it works, who is in it, under which rules | docket |
| Team contract | What it accepts, what it delivers, what it still needs | docket declares, Tack reads |
| Method (pipeline) | How a *kind* of task is done | the team's catalogue, chosen per task |
| Governance (policies) | What asks a person or is forbidden | the team |
| Plan | How a large piece of work splits across teams | Tack; a person accepts |

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **Tack orchestrates; docket executes.** The plan of a large piece of work is items: a parent, children with `parent_id`, `blocks` between them, each child routed to its own team and started on its own. docket runs one task inside one team and never creates subtasks, delegates to another pod or holds a DAG (its ADR 0016 D-20, kept) | A complex card is split into children a person accepts, each to the team that fits; the board is the one place the whole is seen |
| 2 | **A team stays a docket pod named on a profile, and what Tack knows of it is its live report, now with a contract.** No team table, no project scope, no apply from the board that docket does not offer. The report (`TeamReport`) gains, when docket lists `podContract`: `delivers` (branch, report, records), `methods[]` (every pipeline a task may name, each with the roles the roster lacks, one marked default) and `needs[]` (what the team's work still needs, each satisfied or not). Tack writes none of it; the shape, the members, the recipes and the operator's services stay in `.docket/teams/<pod>/` and docket's own commands | Agents → Teams answers the six questions a person asks of a team: what it is for, who is in it, what happens to a card, when it asks, how far it goes, how to send it work |
| 3 | **The method is chosen per task from the team's catalogue, and checked three times.** The profile's `pipeline_by_type` names a method; saving the profile refuses a name the team's catalogue lacks and warns of one whose roles the team lacks; the runner refuses the run before sending when the method would skip a step; docket refuses at claim (its `pipelineChecked`). No step is ever skipped in silence. One pipeline stays bound per pod as the default; versatility is the catalogue, not composing recipes | A bug goes through `tdd`, a feature through `cross-family-review`, a security change through `secure-build`, all in one team — and a method the team cannot run is refused with the role it lacks |
| 4 | **A misfit is refused before sending, with its reason, for every need the contract answers.** `TeamNeeds` grows from two facts (web, proposals) to five: web, proposals, **repository** (a run that changes code cannot go to a folder team), **method** (unknown, or with missing roles), **needs** (an unsatisfied need of the team). Each misfit is shown on the profile in `GET /api/teams`, on **Run with agent**, and in the refusal text, as `NoWeb` is today. Without `podContract` the two checks of today remain | "Not sent to docket: team `fab-research` works in a folder and this card changes the repository" instead of a run that stops inside the team |
| 5 | **Three levels of limit, one owner each, every one shown with its owner.** The run's tokens are Tack's (profile, run, provisional, day). The step's rounds, tool calls and time are docket's machine guards, shown as such with where to raise them (Phase 87). The team's tokens are docket's `budgetTokens`: Tack shows the amount, what is used, the window and the pause, and offers **Resume team** when docket lists `podResume` (a window on the budget and `POST /pods/<p>/resume`). Tack sets no team budget and keeps no dollars | The run card and Agents → Teams say for each limit who applies it; a paused team is resumed from the board |
| 6 | **One routing function, in the server.** `tack_core::run_defaults::resolve` is the only answer to "who would do this card" and is served per project; Orchestration's Flow and Map consume it and `routeItem` in the web is deleted. `teamOf` lives in one place | The lane a card is drawn in is the lane it runs in |
| 7 | **A parent item shows its children's progress and tells the person when they finish; it moves nothing.** The item page and card read "k of n children done · teams a, b"; when the last child's run finishes the inbox (ADR 0035) gets an **info** notification. Status stays the person's (Phase 86); "every child done marks the parent done" stays a future status rule | A split card is followed from its parent, and the person decides when it is done |
| 8 | **A business structure is a Tack template: roles as departments, their profiles, and the teams it expects — described, not created.** The project file and templates gain `teams:` (name, shape, delivers, methods), descriptive only. Import and the Map compare it with what docket reports and list what is missing with the docket command that makes it; a built-in template "Digital business" carries the departments docket documents (software, ops, support, growth, revenue, finance, insight) with the rule that a department gets a team of its own only when its work has a different shape or a different gate. **Applying the repository's teams** goes through the runner (it has the checkout and talks to docket), with the person pasting docket's `reviewCode` as an Inbox decision, once docket's apply reaches folder pods. Tack never writes a recipe, a service, a secret or a quota (ADR 0030 d5) | Pick "Digital business", see the organisation chart with the teams that exist and the ones to create, each with its command or its button; a support card is drafted and proposed, and the team's sheet says it sends nothing until the operator declares the channel |
| 9 | **Words.** docket's `profile` (attended, lights-out) is shown as **Supervision**; a docket member's `role` is shown as a **member**, never beside a board role; ADR 0021 and ADR 0024 are amended in their headers. The book's User guide gets one page, "A team", with the six questions of decision 2 and no source paths | The same word means one thing on every screen |

**Why now.** Phases 83 to 87 made teams visible, routable and budgeted, each from the side it
touched; the audit of 2026-11-03 read both trees and found the seams: two routers that disagree, a
fit check on two facts, a method that drops steps, a budget with no window and no way back, a
shape kept in a comment, and the design knowledge of all of it in a git-ignored skill. docket closed
its Phases 60 to 62 the same day (a pod says what it can reach; a recipe says what its work needs
at apply time), which is most of the contract, printed where Tack cannot read it.

**If you do nothing.** Every project onboarded needs the skill's author in the room: the rules that
decide which team fits which card live in `onboarding-lab/skill/references/rules.md`, and the product
checks none of them. A card sent with the wrong method ends `Succeeded` without review.

## Open questions for the maintainer

1. **Does saving a profile with a method the team lacks refuse or warn?** Proposed: refuse an
   unknown name, warn about missing roles (the team may gain the role before the first run).
2. **Does "Digital business" start lean?** Proposed: yes — software plus one folder team per
   confirmed department, each with the short roster (one strong doer, one reviewer of another model
   family), and the full `respond` roster only after the onboarding skill's R0 measures both. The
   evidence read says splitting the same work by job title costs more in coordination than it gives.
3. **Is card G (apply the repository's teams) in this phase?** Proposed: yes, last round, gated on
   docket's `podShape`; it re-enables a button that has been disabled since Phase 73.

## What is asked of docket

Four additive changes on API 6, each announced in `features`, so Tack probes them and degrades to
today's behaviour when absent; the text sent to docket's session is in `docs/plans/phase-89.md`
§"What docket is asked". `podShape` (the shape as a `pod.yaml` key; apply reaches folder pods),
`podContract` (`delivers`, `methods[]`, `needs[]` on `GET /pods/<p>`), `pipelineChecked` (a per-task
pipeline with a missing role is refused at claim), `podResume` (`budgetWindow`, `POST /pods/<p>/resume`,
`budget {tokens, used, window}` on the report). Nothing is asked that docket's D-20 or its
anti-overengineering list refuses.

## Evidence (read 2026-11-03 on Tack `develop` at `301b40fa` and docket `develop` at `adf584a6`)

| Fact | Where |
|---|---|
| Tack has no team table; a team is the string `tool_policy.docket.team`, validated by a regex | `crates/tack-core/src/outline/bundle.rs:425-433`; `crates/tack-api/src/handlers/teams.rs:60-65`; `crates/tack-runner/src/harness/docket.rs:496-507` |
| What Tack knows of a team is `TeamReport`, read live from `GET /pods` and `GET /pods/<p>` | `crates/tack-protocol/src/execution/capabilities.rs:162-221`; `crates/tack-runner/src/harness/docket/probe.rs:256-326` |
| The fit check covers web and proposals only | `capabilities.rs:232-283` (`TeamNeeds`, `team_misfits`); refusal at `docket.rs:174-200, 1696-1707` |
| The runner sends `pipeline_by_type[item.type]` unvalidated | `crates/tack-runner/src/harness/docket/task.rs:283-288` |
| Two routers: the server falls back to the project default when roles disagree, the web takes the first role with a profile | `crates/tack-core/src/run_defaults.rs:30-68`; `frontend/src/features/orchestration/flow.ts:68-96` |
| `teamOf` exists in four places | `teams.rs:60-65`, `docket.rs:496-507`, `crates/tack-app/src/change_sets.rs:88-91`, `frontend/src/shared/execution/api.ts:388-392`, `frontend/src/features/settings/panels/automation/parts.tsx` |
| The item tree exists; the last-child-done move was removed on purpose | `crates/tack-api/src/handlers/items.rs:842` (`get_item_tree`); `docs/closed-cycles/plans/phase-86.md:153` |
| A plan's `team` resolves only when exactly one profile names it | `crates/tack-core/src/change_set.rs:627-633, 666-673` |
| The project file has profiles, roles and automation; no teams | `crates/tack-core/src/outline.rs:42-63` |
| Built-in templates: three, none with teams | `crates/tack-db/src/repo/templates.rs:421, 456, 501` |
| "Apply the repository's teams" is disabled: "Tack cannot run this on the team's computer from the board yet" | `frontend/src/features/settings/panels/automation/Teams.tsx:184-191`; `docs/book/src/admin-guide/troubleshooting.md:310-313` |
| The runner already creates the default pod with `POST /pods {ensure}` | `crates/tack-runner/src/harness/docket/client.rs:167-172` |
| ADR 0021 decision 2's apply route was never built | `docs/closed-cycles/plans/phase-73.md:575, 591`; no match in `crates/` or `frontend/src` |
| docket: a pod is a set of agents `<pod>-<role>`; pod-wide settings on the Lead's meta; four blueprints as Python literals, no user format | `rack-cli/src/docket/teams/models.py:48-121`; `teams/blueprints.py:237-291` |
| docket: `pod.yaml` forbids unknown keys, so the shape is a comment the skill parses | `teams/config_models.py:121-134`; `onboarding-lab/skill/references/blueprint-format.md:115-116` |
| docket: one bound pipeline per pod; a second topology recipe replaces the first | `teams/apply/plan.py:365-379` |
| docket: a task may name any pipeline, from the pod, the operator or a recipe | `hops/application/steps.py:134-147` |
| docket: a step whose role the pod lacks is marked `skipped` and the walk continues; only `apply` refuses | `pipeline/domain/plan.py:120-133, 188-198`; `teams/apply/recipes.py:19-33` |
| docket: the pod budget is a lifetime sum; the pause clears only from the CLI | `hops/application/usage.py:45-54`; `hops/gates/pre_hop.py:39-45`; `interfaces/cli/commands/run.py:159-168` |
| docket: apply over HTTP answers 400 for a pod without `codebase`; folder pods are provisioned with `codebase=""` | `interfaces/http/routes/pod_apply.py:28-34`; `teams/pod_provisioning.py:429-438` |
| docket: `needs` is computed at apply and printed, not reported | `teams/config_models.py:135`; its Phase 62 (ROADMAP D-84) |
| docket: no subtasks, no DAG, no cross-pod dispatch | `docs/adr/0016-…:269` (D-20); `hops/application/runner.py:232-237` |
| docket: the departments of a digital business are shapes plus recipes plus what the operator adds; "docket ships no blueprint named for a department" | `rack-cli/docs/AGENT-TEAMS.md` §"The standard teams of a digital business" |
| Industry: split by context, not by job title; one orchestrator with explicit state; writes single-threaded; clean-context verifier; plan approved by a person; runtime-enforced limits | Anthropic, *Building multi-agent systems* (2026-01-23); Cognition, *Multi-Agents: What's Actually Working* (2026-04-22); Google Research, *Towards a Science of Scaling Agent Systems* (2026-01-28); Magentic-One (2024-11); MAST (2025) |

## Considered and not adopted

- **A team table in Tack, or teams scoped by project.** A second copy of docket's truth, to keep in
  sync. The report is live, and the one case a name cannot settle (two profiles, one team) is already
  settled by naming the profile.
- **A scheduler that picks the team from capabilities.** The route is a person's: role → profile →
  team. The contract refuses a misfit with a reason; it never chooses.
- **Subtasks, a DAG or cross-pod delegation in docket.** docket's D-20; and the evidence says the
  orchestrator with the explicit state should be one, which the board already is.
- **Composing topology recipes.** One bound default plus a catalogue chosen per task gives the same
  versatility without a merge semantics nobody has asked for.
- **Editing recipes or pipelines in Tack.** They are files in the project's repository; their editor
  is a text editor and their validator `docket pod validate`.
- **A fifth shape, or a blueprint format.** docket's rule of three; a department is an instance of
  a shape, not a shape.
- **A "manager" type, or automatic status moves.** Decided in ADR 0034 and Phase 86.
- **Dollars anywhere.** ADR 0024.
