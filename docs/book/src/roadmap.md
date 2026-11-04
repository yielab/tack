# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles).

**Tack is delivered through Phase 90.** A project is planned and tracked on a board, a list, a
calendar or a timeline, with or without agents, and its teams at work are drawn on the Orchestration view. A task can be handed to an agent on
your computer (Claude Code, Codex, opencode or docket), which brings its own sign-in and model;
Tack holds no model key. A step that runs out of its own budget keeps its work and lets a person
continue it or start over from it. The run's work comes back as evidence to review, an independent
verifier can judge it, and Tack can push the branch, open the pull request and merge it: by a
button, when the task moves to Done, or by itself at Level 4 or 5 under the verifier's signed
verdict. A project can start its tasks by itself, send each role's tasks to a long-lived docket
team, repeat tasks, and travel whole in one file.

**Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. The next is
`v0.2.0-beta.1`, which carries Phases 66 to 82: the version is bumped and the changelog section
is written, and it waits for the maintainer to tag it
([`docs/LAUNCH-CHECKLIST.md`](https://github.com/yielab/tack/blob/develop/docs/LAUNCH-CHECKLIST.md)).
Phases 83 to 86 are under `[Unreleased]` in the changelog. The `v0.1.0-beta.10` tag (2026-10-14)
never had a release and is not re-cut; its changes ship in `v0.2.0-beta.1`.

**Where this is going.** Tack keeps the same three neighbours it never contains: an agent
runtime such as docket, which runs the work; an independent verifier such as prill, which judges
it; and GitHub. Each still works alone, and without a verifier a project stays at Level 3.

---

## Open

Phases 73 to 90 are built and in `develop`. Tagging `v0.2.0-beta.1` is the maintainer's own step,
and it is not a card.

**Phase 91 — what docket offers, Tack uses or says why not** (2026-11-04, no plan file, no ADR: it
amends ADR 0034's note on `conversation` and ADR 0035's decision 1). docket 0.3.0-beta.1 lists 21
features and Tack used 16. Four cards, in two rounds (D, then A ∥ B ∥ C):

- **A — a thread on a docket team continues.** When the team lists `conversation`, a thread's first turn
  is one task sent with `conversation: {maxTurns}` (the thread's cap, at most 20); each later turn
  answers that task's `turn` escalation with `reply` and the new message; closing the thread answers
  `close`; the team's `converse` reads `continues`. A `conversation_needs_one_step` refusal sends that
  turn as today's replay and the team reads `replays` with docket's reason. The fake answers turns as
  docket does, and also a consult in two parts (the run waits until both are answered), which Tack
  already shows but nothing proved.
- **B — what a team waits to learn needs you.** When the team lists `memoryStatus`, the team's report
  carries its memory (pending and accepted entries). A pending entry opens a **needs-you** notification
  that closes when the entry is decided; the team's sheet shows what it learnt and what waits, with
  docket's `docket pod memory approve` and `refuse` commands to copy and **Approve**/**Refuse** shown
  disabled with that reason. docket is asked for a route to decide an entry, probed when it comes.
- **C — docket's check of a named method is read as a refusal.** A task docket refuses at claim with
  `failureKind: pipeline` (its `pipelineChecked`) reads **Not started** with docket's reason, as a
  `capability_missing` refusal does; Tack's own refusal before sending reads only the team's reported
  missing roles (ADR 0036, decision 3).
- **D — Tack names docket 0.3.0-beta.1.** Every "docket 0.2.0-beta.6", a version that was never tagged,
  becomes docket's declared next release, 0.3.0-beta.1, in the messages, the doctor and the guides.

docket's read-only token (`readScope`) is not used: every call goes through the runner, which must hold
the full token to start tasks, so a second token in the same process limits nothing.

**Phase 90 — a run is never refused for a contradiction Tack wrote itself** (2026-11-03, no plan
file, no ADR; from the seven runs of one research task; both cards built in `develop` the same day). Two cards, one round:

- **A — one rule for a run's network.** The built-in Researcher, Planner and Reviewer name
  WebFetch (the Researcher WebSearch too) and no `network`, so a run that leaves the network to the
  profile asks for no network and for web tools at once, and Claude Code refuses it before it
  starts; the Run dialog starts its Web tools box off for the same reason, and opencode, which
  reaches the network on every run, refuses every run whose box is off. One function in `tack-core`
  gives a profile's default network for an agent: the profile's `network` flag, or, for any agent
  but docket, its tools for that agent being `*` or naming WebFetch or WebSearch; for opencode it is
  always on, and the dialog shows the box on and disabled with that reason. The run's creation,
  auto-start and the Run dialog read it, the dialog from the server rather than a copy of the rule.
  Web tools off then takes the web tools out of a Claude Code run, as the box's help already says,
  instead of refusing the run.
- **B — a refused run reads "Not started" on every chip.** The Execution tab's run chips read the
  run's latest attempt only once it is loaded, and otherwise the request's own state, `failed`, so
  the same refusal reads Not started on one chip and Failed on the next. The fallback reads the
  run's `never_started` as the board's card does.

What remains besides them is one dependency card:

- **The desktop app's Tauri crates** (`dialog` and `shell`, behind their JS packages already updated).
  Run the desktop workflow on the branch first.

## Future, not planned

Phase 76 and status rules are the only ones. They have no date, no owner and no card in progress;
they stay written so they can be opened later.

**[Phase 76](https://github.com/yielab/tack/blob/develop/docs/plans/phase-76.md) — the project
file is kept in sync.** ADR 0025 is proposed. Nothing is built, and the plan was written before
Phases 79 to 81 moved the code: re-read it against the tree before opening it.

The project file of Phase 73 is a one-way door: it creates and updates by name and never
deletes, so a role renamed in the file becomes a second role, a role removed from it stays for
ever, and a file imported twice doubles the items. Keeping a project beside its code, which is
why the file exists, needs the board to follow the file. Drupal keeps configuration in a sync
directory and content in the database; Tack's file already has the two halves. So:

- **The configuration is everything in the file but the items**, and only it is kept in sync.
  Items keep their additive import.
- **Every role, field, sprint and profile has a key**, made from its name when it is created and
  never changed, so a rename in the file is a rename on the board. A file without keys works as
  today.
- **An import compares three things:** the file, the board and the last revision. Changed only in
  the file: applied. Changed only on the board: kept. Removed from the file and untouched on the
  board: removed. Changed on both: a conflict, which stops the import unless you say which side
  wins. With no revision yet nothing is removed.
- **A removal shows its cost first:** how many cards lose the role, how many values go with the
  field. A profile another project uses, or with a run open, is not removed.
- **Every export and import leaves a revision**, shown in Settings → Data → History, and going
  back to one is an import like any other.
- **The preview and the apply are one plan:** the apply carries the preview's hash and is refused
  if the board moved in between. Everything an import writes lands in one transaction — the
  outline import too — so "import again to finish" goes away.
- The file still cannot switch auto-start on, raise the autonomy level, change the workflow or
  the project type, or change an item.

Until it is opened, the project file stays what Phase 73 made it: an import that creates and
updates by name and never deletes.

**Status rules.** Optional, written in Phase 86's plan ("Considered and not adopted now"), no phase opened.
One list of rules per project, each "when this happens, and this holds, propose these changes": a run starting
moves its task, a run with changes suggests In Review, every child done marks the parent done. A rule builds
a change set with the rule as its actor, applied at once or left waiting, so a refusal is a visible reason.
It reads the same events as the triggers of Phase 85 and writes only the status.

## Cancelled

None of these was approved as a card. They are not scheduled and do not wait on anything; each
can come back as a card of its own.

- Publishing the release, the publish list, the installs on macOS and Windows, the
  code-signing decision and the GitHub and workstation items of Phases 65 and 69 (Wave 0). The
  release is the maintainer's own step, written in `docs/LAUNCH-CHECKLIST.md`.
- The live proof of My SaaS's code card through a team with a real install, which needs a paid
  model and docket's cooperation.
- Directives learnt from a person's corrections (ADR 0020, decision 6): Tack depends on no
  verifier, and the feature waited on a contract from one. The reasons a person gives stay
  stored (ADR 0013, decision 7).
- Measuring docket's polling, and the retakes of `agents-flow.gif` and `recovery-demo.gif`. The
  outdated `agents.png` and the two desktop screenshots were removed instead of retaken.
- Four limits that stay as limits, each already shown in the product with its reason: "Apply the
  repository's teams" is disabled; only docket stops a run at its token limit; the tokens a
  verifier spends count whole, cache included; only docket reports a running agent's steps on its
  timeline.

---

## Delivered

Phases 73 to 87, newest first. The detail is in each phase's plan under
[`docs/closed-cycles/plans/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/plans),
each decision in its ADR under [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr),
and what a user can now do in the [changelog](https://github.com/yielab/tack/blob/develop/CHANGELOG.md).

- **Phase 89 — the board orchestrates and a team declares what it accepts.** ADR 0036. One owner per
  concept: the board decides what is done and by whom, the team decides how. One router
  (`GET /api/projects/{id}/routing`) serves Flow, Map and the item page; a parent card reads how many
  children are done, names the teams or agents involved and tells the inbox when the last one
  finishes, moving nothing. A team's report carries its contract (what it delivers, the methods it can
  run and the roles each lacks, what its work still needs); a profile with no team is a team of one,
  filled from what its runner reports, so every rule works with any agent and docket adds members,
  methods and folder teams when it reports them. A run is refused before it is sent when its team
  cannot do what it needs (the web, proposals, a repository, a method, an unmet need), and a profile's
  method is checked when it is saved. Agents → Teams shows each team as a sheet of six answers; each
  run says who applies each limit (run, step, team) and a team paused on its budget is resumed from
  the board; "Digital business" is a template of six departments with the teams it expects, compared with
  what docket reports, and the repository's teams are applied through the runner with the person
  approving the commands docket lists. Everything docket-specific is probed as a feature, and
  was checked on 2026-11-04 against the answers of a real docket (0.3.0-beta.1, API 6) recorded as the fake's
  fixtures: docket sends `delivers` as a list, a method that does not load carries an `error`, a
  budget's window is `none`, `day` or `week`, **Resume team** clears the pause but not the spent budget (the
  team is paused again within seconds until the budget is raised), and a team made from a shape takes
  the folder it works in as `location`, so Tack sends it only for `build`. The board now has a channel to ask a runner for something
  (`POST /operations/poll`, `/operations/{id}/result`), additive in runner v1.
- **Phase 88 — a person is told what needs them.** ADR 0035. One inbox fed by the board's own events:
  a question a run asks, a run that fails and a change set an agent proposes each need you; a run that
  finishes and an item due within the hour inform you. It closes itself when the situation ends, however
  it ends, and is repaired from the store at start and after a lost event. The web shows a bell with the
  count, a drawer, a notice and the count in the tab's title (and the browser's notification when the tab
  is hidden, if you turn it on); the desktop app sends the system's notification and counts on the tray;
  a webhook receives each one signed, retried with backoff and with its id, and Settings → Automation
  shows each channel's state. Phase 10's webhook (`item.*`, `sprint.*`, `item.due_soon`) is gone.
- **Phase 87 — a step that runs out of budget stops at a checkpoint, not a cliff.** No ADR of its
  own (it follows docket's own ADR 0038). A step that ran out of its rounds, tool calls or time ends
  `step_budget`, names the limit and where to raise it, and keeps its patch; one that ran out of
  tokens keeps today's `budget` code. A person then chooses **Continue** (a new request that
  retries the same docket task) or **Start over from this run's work** (a new task based on the
  branch it kept, with twice the token limit when tokens were what ran out). Live-proved against a
  real docket (P59-6 to P59-8) and a real Tack on a local model; the proof found and fixed two bugs
  the fake alone had not caught.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-87.md).

- **Phase 86 — a person sets a task's status, the agents report its work.** No ADR (it amends
  ADR 0033, decision 1). The six places that moved a status because a run did something are removed
  with their settings; auto-start starts a task once and moves nothing; the item and its card show
  **Status** and **Work** as two things.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-86.md).
- **Phase 85 — a manager works through the board.** ADR 0034. A profile becomes a manager by being given
  general parts, none called "manager": a credential with scopes, change sets as an agent's only way
  to change the board, a team's activity, a computed digest, threads where each message is a turn, and
  triggers on events. Through docket a manager runs at L3 or L4.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-85.md).
- **Phase 84 — the screen says what needs you first.** ADR 0033. One run-state language on every
  screen, each screen ordered by what needs a person, the Table view folded into the List, and a new
  work view, Orchestration (Flow: a lane per team; Map: roles to members).
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-84.md).
- **Phase 83 — a team is shown as it is.** ADR 0032. Agents → Teams lists every team any agent
  reports; a task says who does it; a run's findings reach one accept panel from a plan file, a team's
  records or an uploaded file, and a person always accepts.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-83.md).
- **Phase 82 — the task goes to docket as fields.** ADRs 0030 and 0031. The Execution tab shows which
  prompts ran, what the agent assumed and asked; a team may work in a folder with no repository; a wait
  reads as a wait, and an action that leaves the team is seen whole before it is approved.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-82.md).
- **Phase 81 — what the audit found.** No ADR. Background work with one owner that stops on shutdown,
  typed answers from the docket and prill adapters, tests for routes and upgrade paths nobody covered,
  one wire format and one error model, one UI kit. The rule it keeps: CI never needs docket or prill;
  everything that talks to the real programs is the local `onboarding-lab/e2e/docket-prill-e2e.sh`.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-81.md).
- **Phase 80 — a run that waits on a person.** A docket run parked on a question is no longer
  cancelled by the attempt's own time limit; every wait for a person is one escalation record, answered
  by one route. The autonomy level goes on a task only when a person asks for one, never above the
  team's ceiling.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-80.md).
- **Phase 79 — the crates say what depends on what.** ADR 0028. The runner-v1 contract is a crate of its
  own, every SQL line is in `tack-db`, the rules of a run are use cases, and the pre-push hook fails on
  any crate edge the plan does not allow. Nothing on the wire changed.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-79.md).
- **Phase 78 — one pass over what was still open.** No plan, no ADR: the desktop app's first-run fixes,
  the board audit, a page of its own for each item with a run card and an Execution tab, and "Done when"
  checks a person can tick.
- **Phase 77 — the agent's copy is prepared.** ADR 0027. Whoever makes the agent's copy installs its
  dependencies from the lockfile, and every run has a limit that says where it comes from.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-77.md).
- **Phase 75 — limits in tokens.** ADR 0024. A limit is in tokens and nothing has one by default; cache
  reads are shown and never counted; every limit says who applies it; dollars are an estimate shown
  last; what Tack sends an agent carries nothing twice.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-75.md).
- **Phase 74 — what the first real project found.** ADR 0023. Twenty defects fixed where each fact is
  owned: a run that cannot start ends Failed with the reason, one rule says whether a computer is
  online, and the docket contract is proven by answers recorded from a real docket.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-74.md).
- **Phase 73 — every run goes to a team, and automation is one ladder.** ADRs 0021 and 0022. A project
  role says which profile, and through it which team, takes a task; a task can start by itself and
  repeat; the automatic merge never rises above the team's ceiling; Settings → Automation is a ladder of
  four rungs; a whole project goes in and out through one file.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-73.md).

---

## Archived

- [Phases 0–57](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-0-57.md) — the original
  engineering phases, the audit-driven cycle (26–32), the Agent-Factory Control Center
  (33–38), the Agnostic Control Plane (39–49), and the Harness-Agnostic Runner Fleet
  (50–57).
- [Phases 58–63](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-58-63.md) — standalone
  single-binary packaging, the first public release, agent onboarding,
  the desktop app and background service, and human maintainability.
- [Phase 64](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phase-64.md) — the smallest
  codebase that does everything a user can do, which retired the docket control-plane bridge.
- [Phases 65–72](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-65-72.md) — the
  `v0.1.0-beta.9` release; briefs, evidence and the verifier's merge-readiness pack; a run a
  normal person can start; the first live docket + prill run; the security phase after the
  withdrawn tag; the agent bringing its own model; and merging from Tack.

---

## Contributing

[CONTRIBUTING.md](https://github.com/yielab/tack/blob/develop/CONTRIBUTING.md) covers setup and the
pull request process; [Adding Features](developer/adding-features.md) walks through the common
extension patterns.
