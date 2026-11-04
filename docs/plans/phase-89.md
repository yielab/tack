# Phase 89: the board orchestrates and a team declares what it accepts

**Status: PROPOSED 2026-11-03.** Executes ADR 0036
(`docs/adr/0036-the-board-orchestrates-and-a-team-declares-what-it-accepts.md`): read its decision
table first; this file says only how. Seven cards in five rounds, grouped by the files they touch so
one agent reads a file once. Three cards wait on a docket feature and probe it at runtime; the others
run today.

## How a card is worked

- Read only the files its row names. The ADR's evidence table has the line numbers of everything
  the phase changes; verify each before editing (the tree moves).
- One rule, one place: who does a card lives in `tack-core/src/run_defaults.rs`; what a team is
  lives in `tack-protocol/src/execution/capabilities.rs`; what docket said lives in
  `tack-runner/src/harness/docket/probe.rs`. A card that needs a fact from another layer calls it,
  never copies it.
- **Any agent first.** Tack runs `claude-code`, `codex`, `opencode` and `docket`; only docket has teams
  of several members. Every card's row says what it builds for any agent and what only when docket
  reports; every card's tests include the no-team path (a profile on `claude-code`, with the runner's
  capability snapshot as the only source). A field a harness did not report is absent, never guessed.
- A docket feature is probed, never assumed: `client.has_feature("podContract")` and the like
  (`tack-runner/src/harness/docket/probe.rs`, as `POD_PAUSE` is today). Without the feature the
  card's code path is today's, and the test says so with a fixture that lacks the key.
- CI never needs docket or prill. Every docket test runs against
  `crates/tack-runner/tests/fake_docket/`; a card that adds a key to the wire adds it to the fake
  and to one fixture under `crates/tack-runner/src/harness/fixtures/docket/`.
- Run only the tests of what the card touched (`cargo nextest run -p <crate> <filter>`,
  `cd frontend && npx vitest run <path>`); the full suites and `.githooks/pre-push` run once per
  round, by the integrator. `PATH=$HOME/.cargo/bin:$PATH`.
- Each card writes its `[Unreleased]` line in `CHANGELOG.md` and its guide paragraph in the same
  change (User and Administrator guides carry no source paths, test names or phase numbers).
  No `Co-Authored-By`.
- `./scripts/regen-generated.sh` after any route or schema change (`docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts` are generated, never edited).
- **Sonnet** takes a card that changes behaviour across layers; **Haiku** takes a card whose rows
  are documentation or a label. The model is named per card.

## Shapes shared by every card

```rust
// tack-protocol/src/execution/capabilities.rs — additions to TeamReport (card D, F)
pub enum TeamDelivers { Branch, Report, Records }
pub struct TeamMethod { name: String, source: String, default: bool, missing_roles: Vec<String> }
pub struct TeamNeed { kind: String, name: String, fix: String, satisfied: bool }
pub struct TeamBudget { tokens: Option<u64>, used: u64, window: String, window_start: Option<String> }
// TeamReport gains: delivers: Option<TeamDelivers>, methods: Option<Vec<TeamMethod>>,
//                   needs: Option<Vec<TeamNeed>>, budget: Option<TeamBudget>   (all None = "not said")

// A profile with no team is a team of one, filled by the runner from the harness it runs:
impl TeamReport { pub fn of_agent(harness: &str, cap: &HarnessCapability) -> TeamReport }
//   name = harness, delivers = Some(Branch), methods = Some([TeamMethod { name: "agent", default: true, .. }]),
//   needs = sign_in (satisfied when signed in; fix = cap.docs.login) + "a model" when requires_model and none named;
//   members = Some([TeamMember { role: "agent", model: default_model, .. }]); profile/supervision = None (not said)

pub struct TeamNeeds { web: bool, proposals: bool, repository: bool, method: Option<String> }
pub enum TeamMisfit { NoWeb, NoProposals, NoRepository, UnknownMethod { name }, StepsSkipped { method, roles }, NeedsUnmet { name } }
```

```rust
// tack-api — card A
GET /api/projects/{id}/routing -> Vec<Routed>
pub struct Routed { item_id, profile_id: Option<String>, source: Source, team: Option<String>, harness: Option<String>,
                    nobody: Option<NobodyReason> }           // NobodyReason: NoRole | RoleWithoutProfile | ProfileMissing | TeamNotReported | HarnessNotReported
// Item gains (read paths only): children: Option<ChildrenProgress { total, done, teams: Vec<String> }>
```

```yaml
# project file / template — card D (descriptive; import creates nothing from it)
teams:
  - name: saas-research
    shape: respond            # build | respond | operate | routine
    delivers: report          # branch | report | records
    methods: [research-review]
```

```rust
// runner protocol operations — cards F and G (beside `answered-elsewhere`)
ResumeTeam { harness, team }                         // POST /pods/<p>/resume
ApplyTeam  { harness, team, shape, directory }       // POST /pods {ensure} then POST /pods/<p>/apply {directory}
// a reply carrying `reviewCode` opens a Decision the person answers with the code; the runner resends with `accept`
```

## Cards

| Card | Model | Crate(s) | What | Removes | Tests |
|---|---|---|---|---|---|
| **A** — one router, and the parent follows its children | Sonnet | `tack-core`, `tack-api`, `frontend` | **Server.** `handlers/routing.rs`: `GET /api/projects/{id}/routing` calls `run_defaults::resolve` per open item with the project's roles, default and the built-in Implementer, and joins the profile's team and harness as `handlers/teams.rs:60-65` does (move that `named_team` into `tack-core` beside `resolve`, and make `change_sets.rs:88-91` and `docket.rs:496-507` call it: one `teamOf`). `nobody` is set from the same facts the web used (`flow.ts:37`) plus `HarnessNotReported`: the profile's `tool_policy.agent` is in no active runner's `capability_snapshot` (the lane of a profile with no team is `agent:<harness>`, as `agentLaneKey` draws it today). **Any agent:** everything in this card; a team is one lane among others. `get_item_tree` and `GET /api/items/{id}` fill `children` (total, done = status in the workflow's done category, teams = distinct teams of the children's latest runs). If `tack-core/src/notification.rs` exists (Phase 88 card A), add `Kind::ChildrenFinished` (**info**, key `children:<parent>`) emitted when a child's run finishes and no child has an open run; if it does not exist yet, leave this out and say so in the card's note. **Web.** `features/orchestration/flow.ts`: delete `routeItem` and `laneOfProfile`'s routing half; lanes come from `/routing`; `map.ts` the same. `shared/execution/api.ts:388-392` `teamOf` is the one web copy. The item page header and the board card show "k of n children done · teams" when `children.total > 0`. The team drawer and Agents → Teams label docket's `profile` **Supervision** (`attended` → "Attended", `lights-out` → "Lights-out") | `routeItem` (`flow.ts:68-96`); the `teamOf` in `parts.tsx`; the duplicated team lookup in `change_sets.rs` and `docket.rs` | `tack-core`: `resolve` unchanged; `named_team` with and without `docket.team`. `tack-api` `tests/handlers/routing.rs`: three items (role with profile, two roles disagreeing → project default, no role → Implementer) and one `nobody` per reason; `children` counts on a parent with two children, one done. Vitest: Flow and Map render lanes from a `/routing` fixture; a card with children shows "1 of 2" |
| **C** — words and the amended ADRs | Haiku | `docs/` | ADR 0021 header: four amendments (the apply route was never built → decision 8 of ADR 0036; the default pod is docket's default code team, ADR 0031 d5; the gate is the first command or test criterion, ADR 0030; auto-start moves nothing, Phase 86) and its rung names → the UI's (Who, When, Merge, Limits, Agents). ADR 0024 decision 1 row: "no limit by default" struck through with "superseded by ADR 0027 decision 5: a provisional limit of 2 000 000 tokens, and a learnt one proposed after five runs". User guide: a new page `user-guide/teams.md`, "A team", with the six questions of ADR 0036 d2 as its six headings (what it is for, who is in it, what happens to a card, when it asks, how far it goes, how to send it work), in a person's words, no source paths; linked from `user-guide/running-agents.md`'s team paragraph and `SUMMARY.md`. Developer guide: one table in `developer/crate-tour.md` under its teams section, "Words with two meanings", listing profile (Tack's agent profile / docket's supervision), role (board role / team member), pipeline (team method / the board's workflow), step (team step / docket hop) | — | `mdbook build docs/book` passes; `grep -rn 'crates/\|\.rs' docs/book/src/user-guide/teams.md` prints nothing |
| **D** — the contract reaches the board | Sonnet | `tack-protocol`, `tack-runner`, `tack-api`, `tack-core`, `tack-db` | **Any agent — the team of one.** `TeamReport::of_agent` in `tack-protocol` (shape above); the runner's probe of every local harness (`tack-runner/src/harness/mod.rs`, where the descriptor and `sign_in` are gathered) sets `teams: Some(vec![of_agent(..)])` so `GET /api/teams` lists `claude-code`, `codex` and `opencode` as teams of one beside the pods, and a profile with no team is matched to the team named by its `tool_policy.agent` (`teams.rs:84-174`, the same by-harness match it has for pods). **docket adds — wire.** `probe.rs`: when `podContract` is listed, read `delivers`, `methods`, `needs` into the shapes above; `wire.rs` gains the keys as `Option`. `fake_docket` serves them behind the feature; fixture `pod-contract.json` added, `pod-folder.json` untouched (the no-feature case). **Fit (any agent; method only where methods are reported).** `capabilities.rs`: `TeamNeeds::of(kind, web, item_kind)` sets `repository` for a profile whose kind changes code (Implementer, or any kind that is not Planner or Researcher) and `method` from `pipeline_by_type[item.type]`; `team_misfits` adds `NoRepository` (`delivers == Report` or `Records`, and `repository`), `UnknownMethod`, `StepsSkipped`, `NeedsUnmet` (one per unsatisfied need). `docket.rs:174-200` refuses with the new sentences; `teams.rs:84-174` lists them per profile. **Profile save.** `runner_admin.rs`'s profile PUT: when the team is reported with `methods`, an unknown `pipeline_by_type` value is refused naming the catalogue; missing roles come back as `warnings[]` on the response (open question 1); a `docket.pipeline_by_type` on a profile whose `agent` is not `docket` is refused as a misconfiguration (a local agent has one method). **File.** `outline.rs`: `teams: Vec<BundleTeam>` (name, shape, delivers, methods), `deny_unknown_fields` kept; `bundle.rs` validates the name with the team regex and the shape against the four; the import preview (`handlers/outline.rs`) adds `missing[] { kind: team \| harness, name, shape?, fix }` where a team's `fix` is `docket init <name> --blueprint <shape>` plus `--recipe <m>` for a method that is a shipped recipe, and a harness's `fix` is the doctor's sentence for a harness no runner reports; export writes `teams` from the profiles that name one and the reports. **Template.** `templates.rs`: "Digital business" — roles Software, Operations, Support, Growth, Finance, Insight; one profile per role (Implementer for Software; Researcher-kind for the rest, `network: true` on Support, Growth, Insight), **none naming an agent or a team by default**, so the template works on whichever harness the runner has; `teams` lists the folder teams the departments take *when the person points their profiles at docket*, per the lean proposal of open question 2 (one `build` team; one `respond` for Support and Growth together; one `operate`; one `routine` for Finance and Insight together), with the two-line rule in the template's description: a department gets a team of its own only when its work has a different shape or gate; without docket each department's agent delivers into the repository, a report being a file | — | `tack-protocol`: one test per new misfit, "an absent contract is never a misfit", and `of_agent` from a signed-out `codex` capability yields one unsatisfied need and `delivers: Branch`. `tack-runner`: probe with and without the feature from the two fixtures; `start` refuses `NoRepository` before any `POST /tasks`; a `claude-code` profile with the sign-in missing is refused with `NeedsUnmet` before the agent is spawned. `tack-api`: `GET /api/teams` lists a team of one per local harness an active runner reports; profile PUT refused on an unknown method, warned on a missing role, refused on `pipeline_by_type` with a local agent; import preview lists one missing team and one missing harness, each with its fix; the template imports with no docket in the snapshot and exports round-trip. `tack-core`: `teams` parses, an unknown shape is refused |
| **E** — the structure on the screen | Sonnet | `frontend` | Agents → Teams: each team — a pod or a team of one — as a sheet with the six headings of the User guide page, filled from the report (a team of one reads "one agent, `claude-code`" under who is in it and its sign-in and model under when it asks; a pod reads its members) — purpose (`description`), members (role · model), what happens to a card (the default method's steps, and the catalogue by type from the profiles that name the team), when it asks (Supervision, network, `protects`, the needs not satisfied as "the operator still has to…"), how far it goes (the ceiling from `GET /api/projects/{id}/autonomy`), how to send it work (the roles whose profiles name it); misfits per profile as today's `NoWeb` line. **Run with agent**: the misfit sentence before Start, Start disabled while one holds. Profile editor: `pipeline_by_type` as a select over the team's `methods` when reported, free text otherwise; warnings shown. Orchestration Map: a greyed entry per `missing` of the project (a team nobody reports, a harness no runner reports), with its fix copyable. Import preview: the `missing` list under the profiles. **Any agent:** the sheet, the misfit line, the Map's grey entries; **docket adds:** the method select and the catalogue by type | the hand-written team summary lines in `Teams.tsx` that the sheet replaces | Vitest: a sheet renders six headings from a report fixture; a profile naming an unknown method shows the refusal; Map greys a missing team; Run with agent disables Start on `NoRepository` |
| **F** — each limit with its owner | Sonnet | `tack-protocol`, `tack-runner`, `tack-api`, `frontend` | `probe.rs`: `budget` into `TeamBudget` when `podResume` is listed. Runner protocol: operation `ResumeTeam` → `POST /pods/<p>/resume`; `POST /api/teams/{harness}/{name}/resume` routes it to a runner of that harness as `answered-elsewhere` is routed. Run card (`shared/execution/`): a "Limits" fold with three rows — run (amount, source: profile / run / provisional / day; "applied by Tack's runner" for a local agent, "applied by docket" for a task; **any agent**), step (rounds, tool calls, time; "applied by docket on its machine"; the `raise` hint of Phase 87 when present; **docket**), team (amount, used, window; "applied by docket"; **docket**); rows absent when not said, so a `claude-code` run shows one row. Agents → Teams sheet: the team row with **Resume team** when paused and the feature is listed; disabled with the reason otherwise (a deferred control is shown disabled with its reason, as the Teams rung's apply button is today). `fake_docket`: the route and the key; fixture `pod-budget.json` | — | `tack-runner`: resume hits the route once and returns the report; without the feature the operation is refused with its sentence. `tack-api`: the route needs a runner of the harness. Vitest: three rows render from a run fixture; the button's two states |
| **G** — apply the repository's teams | Sonnet | `tack-runner`, `tack-api`, `frontend` | Runner operation `ApplyTeam { team, shape, directory }`: `POST /pods {name, blueprint: shape, location: <checkout>, ensure: true}` then `POST /pods/<p>/apply {directory: <checkout>/.docket/teams/<team>}`; a reply with `reviewCode` opens a `Decision` (kind `review_code`, the commands docket listed as its text); the person's answer is resent as `accept`. `POST /api/projects/{id}/teams/apply` takes the `teams_missing` names (or all of the file's) and needs the runner to list `podShape`. `Teams.tsx:184-191`: the button enabled when a runner of the harness lists the feature, with the reason otherwise; the import preview's missing team gets **Create** beside its command. `troubleshooting.md:310-313` and `admin-guide/runners.md` rewritten: the board applies through the runner, and the person approves the commands | the disabled-forever state of the button | `tack-runner` with `fake_docket`: ensure then apply; a `reviewCode` reply opens one decision and the answer resends with `accept`; a pod without `codebase` applies (the fake models D1). `tack-api`: the route is refused without the feature. Vitest: the button's two states and the Create action |

Rounds: **A ∥ C**; then **D**; then **E**; then **F**; then **G**. D waits on docket's `podContract`
(and `pipelineChecked` for its refusal test to mean anything live); F on `podResume`; G on
`podShape`. A round whose docket feature is not live is built against the fake with the feature
served, and its live proof moves to the acceptance below. Each round's integrator runs
`cargo nextest run --workspace`, `cd frontend && npm test`, `.githooks/pre-push`, and after round 1
`git grep -nE 'routeItem|laneOfProfile' frontend/src` must print nothing.

## What docket is asked

The text sent to docket's session on 2026-11-03 asks for `podShape`, `podContract`,
`pipelineChecked` and `podResume` as described in ADR 0036 §"What is asked of docket", in the order
D2, D3, D1, D4. Each is additive on API 6 and announced in `features`. Nothing in this plan waits
on a docket release: a card probes the feature and the fake serves it.

## What works with any agent, what docket adds

| | Any agent (`claude-code`, `codex`, `opencode`, `docket`) | Only when docket reports it |
|---|---|---|
| Route a card (A) | role → profile → team or single agent; one function; a harness no runner reports is nobody | — |
| Follow a parent (A) | k of n children, the teams or agents involved, one info notification | — |
| Know a team (D, E) | the six questions, filled from the harness: a team of one with its sign-in and model | members and their models, methods with missing roles, the pod's needs, its shape and folder |
| Refuse a misfit (D) | web, proposals, repository, an unsatisfied need from the doctor | a method unknown or with missing roles, a pod's own needs |
| Choose a method (D, E) | one method, the agent's own loop | `pipeline_by_type` over the team's catalogue, checked at save, before sending and at claim |
| Limits (F) | the run's tokens, with its source, applied by the runner | the step's rounds, tool calls and time; the team's tokens, window and **Resume team** |
| A business structure (D, E) | roles as departments, a profile each, what is missing with its fix | folder teams (`respond`, `operate`, `routine`) and **apply the repository's teams** (G) |

## Acceptance, end to end

**Without docket**, on a board whose only runner reports `claude-code` signed in and `codex` signed
out: a project from "Digital business" lists six roles with their profiles and no team; pointing
Support's profile at `codex` makes Run with agent refuse the card before anything starts ("Codex is
not signed in on the runner"), and the Map greys that profile with the doctor's fix; pointing it at
`claude-code` runs; a parent with two children on two profiles reads "0 of 2", then "2 of 2 ·
claude-code", and one info notification arrives; the run card shows one limit row, applied by Tack's
runner; Flow draws every open card in the lane `GET /api/projects/{id}/routing` names.

**With docket**, on the live suite's docket (`onboarding-lab/e2e/`, local model), with the My SaaS fixture: a project
imported from "Digital business" shows its organisation chart with the `build` team reported and the
folder teams listed as missing with their commands; creating one with the printed command makes it
appear in the Map within one probe. A code card whose role's profile names the folder team is
refused before sending with `NoRepository`; the same card on the `build` team with
`pipeline_by_type: {bug: tdd}` runs `tdd`, and with `{bug: secure-build}` on a team without
`security-vetter` is refused naming the role (docket `pipelineChecked`) — and, without that feature,
Tack's own `StepsSkipped` refuses it first. A parent with two children routed to two teams reads
"0 of 2", then "2 of 2" with both team names, and one **info** notification arrives when the second
run finishes; the parent's status has not moved. A team paused on its budget shows **Resume team**;
pressing it clears the pause on docket and the next task is claimed. Flow draws every open card in
the lane `GET /api/projects/{id}/routing` names.

## Not in this phase

A team table or project-scoped teams; a scheduler that picks a team from capabilities; subtasks or a
DAG in docket; composing topology recipes; editing recipes or pipelines in Tack; a fifth shape;
status rules (the parent moves itself); Phase 76's sync of the project file (`teams:` is imported
additively like the rest). Measuring the lean `respond` roster against the full one is the onboarding
skill's R0, in `onboarding-lab/`, not a card.
