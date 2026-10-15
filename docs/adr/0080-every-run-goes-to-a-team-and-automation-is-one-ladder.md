# ADR 0080: every run goes to a long-lived team, and a project's automation is one ladder

**Status: ACCEPTED 2026-10-14 (the maintainer: "aprobado"); executed by Phase 73
(`docs/plans/phase-73.md`).** Amends ADR 0078 decision 7 (docket runs from contract 1.2) and
ADR 0074 decision 1 (what the project's Automation tab holds). Reads docket ADR 0027 and its
operator API version 5 as the contract on the other side.

**Decide:** that Tack sends every run of the docket agent to a **team** that already exists and
keeps existing (a docket pod with its own members, models per role, pipeline and gates), over HTTP,
instead of starting a throwaway agent per run; that a project's **role** says which agent profile,
and through it which team, takes its tasks; that a task can **start by itself** when it reaches a
status the project chooses; that Tack's automatic merge **never rises above what the team allows**
(a team that needs a person stays at L3); and that all of this is set in **one screen, four
rungs**: Teams, Start, Merge, Ceiling. Tack still decides what is done, with what priority and
budget, and owns the push, the pull request, the merge and the level. docket runs the agent and
its team. The verifier signs the verdict. Nothing is computed twice.

**Why now:** docket 0.2.0-beta.6 deletes `docket exec` and the harness contracts Tack uses today,
with nothing kept for compatibility (docket ADR 0027, answered 2026-10-14). The first real project
that needs this, Fabulada, is five teams by discipline on one checkout, with a role in Tack
pointing at each. And the automation a person can set is spread over three screens and a run
dialog, with no way to say "this kind of work goes to that team" or "start it without me".

**If you do nothing:** the docket agent stops running the day beta.6 ships; every run is one agent
with no reviewer, no pipeline and no gate, whatever the team has; a person starts every run by
hand and chooses the team every time; and L4 can be set for a team that will stop to ask, so the
merge waits for a decision nobody sees in Tack.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **One docket harness, no modes.** The runner starts and supervises `docket start --dispatch` on this machine, reads `apiVersion` from `GET /status.json` and accepts only `"5"`; anything else is a probe error naming docket 0.2.0-beta.6. Each attempt is one `POST /tasks/<pod>` with the item's text, the brief, the project's definition of done as the gate, the profile's token budget, the request's restrictions and an idempotency key equal to the attempt id. The runner follows the pod's trace with a cursor, answers approvals and parked questions through Tack's existing decision inbox, cancels over HTTP, and at the end brings the task's branch into its own worktree so evidence, verification and push work exactly as for every other agent. `docket exec`, the contract probe, the NDJSON parser, the stdin answers, the throwaway home and the generated policy are deleted. | "Doesn't run here: needs docket 0.2.0-beta.6" until it does; then a timeline that reads "Implementer (local model) started", "Reviewer: request changes, cycle 1". |
| 2 | **The team is the profile's.** An agent profile may name its team (`tool_policy.docket.team`) and the machines it runs on (a fleet). Without a team the pod is `<project-slug>` with docket's lean default (lead + implementer), created in the project's checkout with `POST /pods {ensure: true}`. A repository that carries `.docket/teams/<pod>/` is applied from the Teams rung with `POST /pods/<p>/apply`; a directory that runs things answers with a review code the person pastes back, never applied without it. A team change is always a person's act (L3). | A profile says "Team: fab-content · lights-out". |
| 3 | **A role chooses the profile.** A project role may point at a profile. A run started with no profile named takes, in order: the task's own saved run settings, the profile of the task's role, the project's default profile, the built-in Implementer. Two roles on one task with different profiles fall back to the project default and say so. The source is recorded on the request and shown in the dialog. | "Chosen by the role Content" in the run dialog, with Change one click away. |
| 4 | **A task can start by itself.** Per project, off by default: a task that enters the trigger status with a role that resolves to a profile starts a run, if it has no open run, none of its blockers is unfinished, the next column's WIP limit allows it, the project's daily budget is not spent and fewer than the project's cap of open runs exist. The request carries `started_by: auto_start`; the task moves to the in-progress column only once the run exists. Everything after that (waiting reasons, the 30-minute unclaimable failure, the finish column, the loop back to the agent, the merge) is what already exists. When a demotion leaves the project at L3, auto-start switches off and the rung says why. | A switch, a status, a number. A badge "Started by itself" on the card. |
| 5 | **The team sets a ceiling on the merge.** A docket pod reports its profile; a `lights-out` pod sets no ceiling; an `attended` pod, or one with profile violations, caps the project's automatic merge at L3 for the runs it took. The ceiling is one more fact the merge policy reads, from the runner's live capability at merge time; the Ceiling rung lists every team that caps the project with the fix docket reports. Other agents report no ceiling, as today. | "fab-research runs attended: its tasks stay at L3. Fix: docket pod set network none". |
| 6 | **The runner runs several attempts at once.** `max_concurrent_attempts` (default 1) in the runner's config, environment and command line; one heartbeat per runner carries every lease and delivers a cancellation to the attempt it names (today a cancellation asked mid-run is read only once). A folder worked in place stays one at a time, by the lock that already exists. The server does not change. | "Up to 3 runs at a time on this computer" on the Agents page. |
| 7 | **Cost is known per model, and a task can repeat.** Usage keeps its tokens per model (Claude Code already reports them; docket reports `usage.byModel`); the Factory page and the dollar budget price each model from the one rate card, through one SQL. A task may repeat weekly, monthly or quarterly: closing it creates the next one once, due at the next period's end, in the first to-do column. | A cost table by model; "Repeats: monthly" on a card. |
| 8 | **One screen, four rungs, Tack's words.** Settings → Automation keeps what it has and adds the ladder: **Teams** (role → profile → team/machine), **Start** (the switch, the status, the cap), **Merge** (today's autonomy section, moved up), **Ceiling** (read-only: what the project can reach and what blocks it, including the teams). A control that cannot be used is shown disabled with its reason, never hidden. Board, list and table carry a small badge ("Started by itself", "Team: fab-content", "Merged by itself"); the task's activity says each step in one sentence; the task drawer shows an Automation row with the task's own state on the ladder. No docket word reaches the screen. | Four rungs on one page; a badge; a sentence per step. |

If you accept this table, you have accepted the ADR. Everything below is evidence.

---

## What this amends

- **ADR 0078 decision 7** (docket runs from contract 1.2): replaced by decision 1. docket's
  setup, login and model list are read from the daemon's own `GET /status.json` and `GET /pods`,
  not from `docket setup harness --json`, which beta.6 deletes.
- **ADR 0074 decision 1**: the project layer (Settings → Automation) gains the ladder; the
  machine layer (`/agents`) keeps profiles, fleets and the runner. A profile's team and fleet are
  machine-layer facts set on the profile and shown read-only in the project's Teams rung.
- **ADR 0079** is unchanged. The ceiling (decision 5) is one more `Hold` in the policy's existing
  order; the merge, the level and the domains stay Tack's; the verdict stays the verifier's; the
  team, its models and its gates stay docket's.
- **ADR 0077** holds: the task's `restrictions` can only narrow what the pod has (docket ADR 0027
  point 3); a `422` fails the attempt and is never retried wider.

## Evidence (read 2026-10-14 on `develop` at `6565415f`, docket at `e880bbfd`)

| Fact | Where |
|---|---|
| The runner fixes one attempt at a time: `Concurrency { total: 1 }` and `run_once` awaited inline | `crates/tack-runner/src/bootstrap.rs:212-216`; `transport.rs:1224-1289` |
| The periodic heartbeat is a `select!` arm, not a task, and its response is discarded, so a cancellation asked mid-run is read only from the first heartbeat | `crates/tack-runner/src/engine.rs:682-703, 1031-1111` |
| The server rejects a heartbeat whose `available != total - active`; it accepts 64 leases per heartbeat | `crates/tack-db/src/repo/execution.rs:1983-1993`; `crates/tack-api/src/handlers/runner_protocol.rs:285, 1009` |
| A folder worked in place is guarded by a per-folder file lock | `crates/tack-runner/src/workspace.rs:219-259` |
| Roles are created, listed and deleted; there is no update route, no `UpdateRole` | `crates/tack-api/src/router.rs:318-325`; `crates/tack-db/src/repo/roles.rs` |
| An agent profile has `tool_policy` and `limits`; no fleet; the request's selector is `exact_runner` or `fleet`, never `Any` | `crates/tack-db/src/migrations.rs:1408`; `crates/tack-api/src/handlers/executions.rs:1017` |
| The API requires `agent_profile_id`; the project default is applied in the browser only | `executions.rs:277`; `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:128-138` |
| Nothing starts a run but `POST /api/executions`, the test run and `enqueue_rework`; the only periodic board loop is the GitHub poll, off without a token, and `autonomy::tick` lives inside it | `crates/tack-api/src/lib.rs:38-60`; `server.rs:123` |
| The merge policy is a pure function with typed holds; `automatic_merge_blocked_by` is a list of sentences built in `GET /autonomy`, and `PUT /autonomy` answers with an empty one | `crates/tack-core/src/autonomy.rs:137-215`; `crates/tack-api/src/handlers/autonomy.rs:168-195, 98` |
| A harness capability has no autonomy or team field; unknown fields land in `additional` | `crates/tack-orch/src/execution/capabilities.rs:54-91` |
| Usage per model is summed into one figure; only the model names survive | `crates/tack-runner/src/harness/claude_code.rs:108-140`; `local_process.rs:245` |
| Token spend and the dollar budget each carry their own copy of the same SQL | `crates/tack-api/src/handlers/metrics.rs:230`; `crates/tack-orch/src/scheduler/wiring.rs:175` |
| The docket harness is a local-process grammar that launches `docket exec --contract 1.2` with a throwaway `DOCKET_HOME`, a generated policy and stdin answers; it refuses a recipe | `crates/tack-runner/src/harness/docket.rs:334-431, 339-351, 532-548` |
| `HarnessAdapter` and `HarnessProbe` are traits with no process in them; only `LocalProcessHarness` implements them in production | `crates/tack-runner/src/engine.rs:351-385`; `harness/mod.rs:151-173, 311` |
| Evidence is captured from the runner's own worktree only; a branch is published from it with `publish_branch` | `crates/tack-runner/src/git.rs:870-876, 897-905` |
| The attempt journal has no cursor field and adapters cannot write to it | `crates/tack-runner/src/journal.rs:63-92` |
| Event and decision kinds are free strings on the wire; every grammar sends `tool_permission` | `transport.rs:689-696, 738-748`; `runner_protocol.rs:1244, 1441` |
| docket's version 5 is planned route by route and not implemented; `SERVE_API_VERSION` is still `"4"` | `~/Sites/rack-cli/specs/api/operator-api.spec.md` (Status); `src/docket/serve.py` |
| docket: a pod runs one task at a time; no push, no SSE | docket ADR 0027 points 4 and 10 |

**Measurement: clicks and screens** (`RunFlow.tsx`, `RunWithAgentModal.tsx`, `AutomationPanel.tsx`,
a project with a folder and the runner on):

| Task | Today | After this ADR |
|---|---|---|
| Start a run by hand | 2 clicks, 1 dialog | 2 clicks, 1 dialog; the profile comes from the role and says so |
| A task starts by itself | does not exist | 2 clicks on the Start rung, then none per task |
| One team per role (5 roles) | does not exist | 1 select per role, 1 screen |
| Raise a project to L4 | 9 interactions at the bottom of 5 sections, plus 2 prerequisites with no UI (GitHub token and poll) | 9 interactions (the prerequisites are real) on the Merge rung, with the Ceiling rung naming what blocks and its fix |
| Screens that decide automation | 3 (Agents, Automation, the run dialog) | 1 (the ladder); Agents only creates profiles |

The clicks to reach L4 do not fall. What falls is the number of places a person has to know, and
what used to be invisible (a team that will stop to ask) is now a line with a fix.

## Alternatives not taken

- **Keep `docket exec` behind a flag until beta.6 is stable.** docket deletes it; a kept path is
  code with no partner to test against, and the owner asked for the cutover in the same step.
- **Let the runner pick the pod per run from a list in the dialog.** It makes every run a
  question. The role already says what kind of work this is; the profile already says who does
  it. A person who wants another team changes the profile, one click, as decision 3 says.
- **A new table for auto-start rules (status, role, team).** Three columns on `projects` cover
  the one trigger a project has. A second trigger is a measured need, not a default.
- **Freeze the team's ceiling into the request at enqueue time.** The runner is not chosen at
  enqueue (a fleet selector), and a team's profile can change between the run and the merge. The
  merge reads the live capability, which is what will actually happen next time.
- **Have the runner apply docket's diff into its workspace.** The branch already lives in the
  same repository; `reset --hard` to it reuses every step that exists. Untracked files docket
  reports are named on the timeline, not silently lost.
- **A per-attempt heartbeat task per slot.** N requests every 20 seconds for N slots, and the
  server's capacity check needs one consistent view. One ticker, one batch.

## Questions for the maintainer, with the default built

Blocking: none.

1. The default pod lives in the project's checkout (docket ADR 0027 §2). Built so.
2. The Start rung takes one trigger status. Built so.
3. The docket harness works only "on a new branch of your folder" in this phase; "in the folder"
   and "a fresh clone per run" are refused at validation with a sentence.
4. "Web tools" off becomes `restrictions.network: "none"` on the docket task: stronger than for
   the other agents (it cuts the hop's network, including a package install inside a hop), and
   the literal reading of ADR 0077.
5. The server accepts `sandbox` as `bwrap` or `none`; docket's runs get a third value, `harness`,
   shown as "Sandboxed by docket in its own jail".
6. The ceiling is read at merge time from the live runner capability, not frozen in the request.
7. The recurrence half of decision 7 can be deferred without touching anything else.
8. A docket task blocked by its budget ends the attempt with docket's text as the reason, rather
   than writing `waiting_reason`, which only the scheduler writes for queued requests today.
