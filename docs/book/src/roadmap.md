# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles).

**Tack is delivered through Phase 82.** A project is planned and tracked on a board, a list, a
table, a calendar or a timeline, with or without agents. A task can be handed to an agent on
your computer (Claude Code, Codex, opencode or docket), which brings its own sign-in and model;
Tack holds no model key. The run's work comes back as evidence to review, an independent
verifier can judge it, and Tack can push the branch, open the pull request and merge it: by a
button, when the task moves to Done, or by itself at Level 4 or 5 under the verifier's signed
verdict. A project can start its tasks by itself, send each role's tasks to a long-lived docket
team, repeat tasks, and travel whole in one file.

**Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. The next is
`v0.2.0-beta.1`, which carries Phases 66 to 82: the version is bumped and the changelog section
is written, and it waits for the maintainer to tag it
([`docs/LAUNCH-CHECKLIST.md`](https://github.com/yielab/tack/blob/develop/docs/LAUNCH-CHECKLIST.md)).
The `v0.1.0-beta.10` tag (2026-10-14) never had a release and is not re-cut; its changes ship in
`v0.2.0-beta.1`.

**Where this is going.** Tack keeps the same three neighbours it never contains: an agent
runtime such as docket, which runs the work; an independent verifier such as prill, which judges
it; and GitHub. Each still works alone, and without a verifier a project stays at Level 3.

---

## Open

No card is open: Phases 73 to 82 are built and in `develop`. Two things wait on the maintainer,
and neither is a card:

- Accepting [ADR 0030](https://github.com/yielab/tack/blob/develop/docs/adr/0030-the-task-goes-to-docket-as-fields-and-a-run-says-which-prompts-made-it.md)
  and [ADR 0031](https://github.com/yielab/tack/blob/develop/docs/adr/0031-a-docket-team-may-work-outside-the-repository-and-what-leaves-is-seen-before-it-leaves.md),
  which Phase 82 built and which are still proposed.
- Tagging `v0.2.0-beta.1`.

What is not a card is in "Cancelled" below.

## Future, not planned

Phase 76 is the only one. It has no date, no owner and no card in progress; it stays written so
it can be opened later.

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
- Measuring docket's polling, and the retakes of `agents.png`, `agents-flow.gif`, the two desktop
  screenshots and `recovery-demo.gif`.
- Four limits that stay as limits, each already shown in the product with its reason: "Apply the
  repository's teams" is disabled; only docket stops a run at its token limit; the tokens a
  verifier spends count whole, cache included; only docket reports a running agent's steps on its
  timeline.

---

## Delivered

Phases 73 to 82, newest first. Each plan is under
[`docs/closed-cycles/plans/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/plans)
and each ADR under [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr).

### Phase 82 — the task goes to docket as fields, and a run says which prompts made it

**ADRs:** 0030 and 0031 (proposed). **Plan:** [`phase-82.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-82.md).

A docket run no longer sends the card twice (as prose and as a brief): the title, the checks, the
constraints, the risk, the kind, the paths the checks name and the done-when go as fields, and
the agent reads them once in a fixed order. Pressing Run again carries the answers a person
already gave. The Execution tab shows, per step, which prompts ran at which version, what the
agent assumed and what it asked; the verifier's evidence names the model's family and the steps
that produced the change. Tack still writes no prompt for a team: prompts are edited where
docket keeps them, and Tack reads them back by name, version and hash.

A team may work in a folder instead of a repository: its item ends Succeeded with what the team
did, nothing to merge, and the records it wrote appear on the Execution tab. A step that waits
reads as a wait with its time, never as a question, and the run's time limit does not run
meanwhile. Before approving an action that leaves the team (an email, a post, a write to a
service), the person sees the whole call; Tack keeps no copy once it is answered. Tack creates
a team without naming a shape, so docket's renaming does not break it.

Left out, each with its reason in the ADR: Tack writing prompt files, versioning a profile's
instructions, reading docket's metrics, configuring services, secrets, quotas or a team's
network, and creating a folder team from Tack.

### Phase 81 — what the audit found

**Plan:** [`phase-81.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-81.md). No ADR.

An audit of the tree after Phases 79 and 80 (tests, structure, code design, screens, and the
seams with docket and prill) produced twelve cards: background work with one owner that stops
on shutdown; typed answers from the docket and prill adapters; CI and the gate checking what
the docs say; screens that show errors, plain words and disabled controls with a reason; a run
surface that never drops an autonomy level in silence; tests for the routes, tools and upgrade
path nobody covered, including a `v0.1.0-beta.9` database migrated to now; typed execution
state; one wire format and one error model; one UI kit; the `Host` trait split into small ports.

The rule it keeps: CI never needs docket or prill. CI tests Tack's code with the in-process fake
docket, the fake prill and captured fixtures; everything that talks to the real programs is one
local command, `onboarding-lab/e2e/docket-prill-e2e.sh`, with a `--check` mode that diffs the
captures against the programs' contracts. The local suite has fifteen tests; fourteen pass, and
the fifteenth, the answer that completes a lights-out run, fails when the local model's lead step
repeats itself, which is the model and not Tack.

### Phase 80 — a run that waits on a person, and one record for every wait

**Plan:** [`phase-80.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-80.md).

A docket run parked on a question was cancelled when the attempt's own time passed, and the run
read as a timeout. docket then made every wait for a person one escalation record, answered by
one route, and Tack's adapter was rebuilt against the real docket:

- the clock of a task that waits on a person stops; it ends only at the question's own expiry
  (`not_answered`) or when docket expires it, and a question with no expiry holds its run until
  it is answered, resolved or stopped;
- one reader of escalations and one answer route; the probe asks for the escalation record
  instead of a version number;
- the autonomy level goes on each task only when a person asks for one ("Run again at L3"),
  never above the team's ceiling and never from the project's merge level: lights-out is L5 and
  docket's own;
- every escalation event is a line on the timeline, and the factory metrics count what docket
  decided alone beside, never inside, what a person answered.

Polling is kept: nothing can replace it while Tack has no inbound endpoint.

### Phase 79 — the crates say what depends on what

**ADR:** 0028 (accepted). **Plan:** [`phase-79.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-79.md).

The runner, the process that holds credentials, compiled SQLite because the types it shares with
the board sat in a crate that depended on the database. An attempt's state was written in three
places, and about 200 SQL queries sat in the HTTP crate beside the repository made to hold them.
The runner-v1 contract is now a crate of its own, every SQL line is in `tack-db`, the rules of a
run are use cases that take the store and a clock, every background loop is in one crate as a
function the binary schedules, and the pre-push hook fails on any crate edge the plan does not
allow. Nothing on the wire changed. There are no store traits and no in-memory fakes: every test
runs on in-memory SQLite in milliseconds, and a fake would hide the transactions and fencing the
store exists for.

### Phase 78 — one pass over what was still open

No plan file and no ADR: nothing here changes a decision. It gathered what the first runs from the
desktop app found, the board screen audit, the gaps of earlier phases that are a fix, and the item
screen's new design, so each screen and module changed once.

- **The runner prepares only what is needed.** The folder it installed is excluded from the
  agent's change only when git does not already ignore it (a project ignoring `node_modules` used
  to end every run "changes could not be read"). Dependencies are installed only when the agent
  can run commands; the sandbox stays on for every run.
- **The desktop app finds, stops and names what it starts.** Every agent is looked for in the same
  places, so opencode under Homebrew is found when the app opens from the desktop menu. Ending
  `tack-desktop` with a signal stops the server it started. The app's runner keeps this
  computer's identity and is named "This computer (host name)".
- **Where a run waits, and why.** Each runner carries its last heartbeat; a run queued for a
  runner that does not answer says so, with the time. A retried request no longer answers 409
  when the task's type or priority changed.
- **A profile says what it delivers**, in one line, and the profile form sets its team.
- **The board scrolls and works by keyboard.** Each column scrolls inside itself, scrollbars meet
  3:1 contrast in every palette, every card has a "Move to…" menu, and each column is a named
  region the keyboard reaches.
- **The item has a page of its own**, `/projects/:id/items/:itemId`, beside the drawer: the same
  component shown two ways, with a breadcrumb, "Expand", "Copy link" and one properties list.
- **Overview, and "Done when" can be ticked.** Details is renamed Overview (`?tab=details` still
  opens it); manual checks become a list a person ticks. The manual criterion gains `checked`,
  false by default; the brief and the verifier ignore it.
- **The Run dialog follows the profile.** Picking a profile picks its agent and says so; the
  project's profiles come first with their "delivers" line; a runner that does not answer is
  shown disabled with since when.
- **The run card.** The latest-run summary is one card whose colour and actions follow the run:
  running, needs review, failed, merged, queued. A run that changed no file offers Accept, not
  Merge, and Merge says why it is disabled instead of hiding.
- **The Execution tab.** Chips pick the run; a review banner; "What the agent concluded" shows the
  report as written; "What the agent did" groups steps by tool or shows the timeline; paths are
  shown inside the project.

Not built, because nothing records it: who created an item and who last changed it, a "short
answer" cut out of the agent's report, and the time each group of steps took.

### Phase 77 — the agent's copy is prepared, and every run has a limit learnt from its kind

**ADR:** 0027 (accepted). **Plan:** [`phase-77.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-77.md).

An agent that starts in a copy with no dependencies spends its first steps failing to run the
project's check. Whoever makes the agent's copy now installs the dependencies from the lockfile.
The project's install command is detected from its lockfile and can be replaced or turned off in
Settings → Automation. The runner prepares the copy it makes for Claude Code, Codex and opencode
and caches the install per lockfile; a docket run makes no runner copy, and docket installs for
itself. Every run has a limit and says where it comes from: a profile with five successful runs
offers a limit proposed from them, and until then a provisional one applies.

### Phase 75 — limits in tokens, and a lean context

**ADR:** 0024 (accepted). **Plan:** [`phase-75.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-75.md).

Dollars are a token count multiplied by a price list someone typed, so a model with no price
slipped past a dollar limit. Tokens are what every agent measures:

- **A limit is in tokens, and nothing has one by default.** A profile sets tokens per run and a
  time limit, a run can change them, and a project sets tokens per day. Dollars per day is gone.
- **A token counted is a new one:** uncached input, cache writes and output. Cache reads are
  shown beside them and never count.
- **Every limit says who applies it and where it is changed**, and "no limit" names the brake
  that still applies. An agent that only reports its tokens says "measured, not enforced".
- **The daily limit is reserved when a run is admitted**, so the day's limit is never crossed.
- **Dollars are an estimate, shown last**, with "not priced" for a model with no price.
- **What Tack sends an agent carries nothing twice:** the brief goes to docket once, a rework
  replaces the previous round's fixes, a card's comments have a size limit, and Claude Code gets
  only the tools its role needs.

What docket sends the model is docket's: the fixes asked of it (condense old tool results, keep
listings relative and bounded) are docket's to make.

### Phase 74 — what the first real project found

**ADR:** 0023 (accepted). **Plan:** [`phase-74.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-74.md).

Putting a real project (86 tasks, ten roles, eight profiles, three teams) into Tack and docket
end to end found twenty defects in Tack. They were closed where each fact is owned:

- A run that cannot start ends Failed with the reason, and a run nobody holds any more asks a
  person instead of sitting in "Preparing"; cancel always ends it.
- The board's runner keeps the name the board gave it, and one rule says whether a computer is
  online.
- What a computer reports about its teams stays current without a restart, and the merge ceiling
  is each run's own team's.
- Everything Tack runs is in the log by default; the server says which database it opened; the
  doctor reads the runner's own file.
- A project file carries the auto-start switch both ways, and a new project can be created from
  one.
- The docket contract is proven by answers recorded from a real docket, not a fake written from
  its spec.

### Phase 73 — every run goes to a team, and automation is one ladder

**ADRs:** 0021 (amends 0019 decision 7 and 0015 decision 1), 0022 (extends 0017). **Plan:**
[`phase-73.md`](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-73.md).

Every run of the docket agent goes to a team that already exists and keeps existing, over
docket's HTTP API, instead of a throwaway agent per run. A project role says which profile, and
through it which team, takes its tasks. A task can start by itself when it reaches a status the
project chooses. Tack's automatic merge never rises above what the team allows: a team that
needs a person during a run keeps its tasks at Level 3. Usage is kept per model, and a task can
repeat weekly, monthly or quarterly. All of it is set on one screen, Settings → Automation, as a
ladder of four rungs: Teams, Start, Merge, Ceiling. A control that cannot be used is shown with
its reason, never hidden. A whole project goes in and out through one file: the outline carries
fields, profiles, roles, sprints and automation, the export writes the same file back, and a
template is that file without items.

Tack still decides what is done, with what priority and budget, and owns the push, the pull
request, the merge and the level. The agent runtime runs the agent and its team. The verifier
signs the verdict. Nothing is computed twice.

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

See [CONTRIBUTING.md](https://github.com/yielab/tack/blob/develop/CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
