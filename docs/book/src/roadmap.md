# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles).

**Tack is delivered through Phase 91.** A project is planned and tracked on a board, a list, a
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
Phases 83 to 91 are under `[Unreleased]` in the changelog. The `v0.1.0-beta.10` tag (2026-10-14)
never had a release and is not re-cut; its changes ship in `v0.2.0-beta.1`.

**Where this is going.** Tack keeps the same three neighbours it never contains: an agent
runtime such as docket, which runs the work; an independent verifier such as prill, which judges
it; and GitHub. Each still works alone, and without a verifier a project stays at Level 3.

---

## Open

Phases 73 to 91 are built and in `develop`. Tagging `v0.2.0-beta.1` is the maintainer's own step,
and it is not a card.

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

**Phase 92 — a question is answered in one place** (2026-11-04, no plan file, no ADR; from task
4C9BAB, where the person could not tell where to type their answer). One card, built in `develop` the
same day except the screenshot `decision-inbox.png`, which waits for the next billed recording:

- **A — the answer box, redesigned as one piece.** Today the choices come first, the text field is
  one line under them called "Details (optional)", the recommendation is a line with a raw option id
  below that, and a question with no choices asks for an "option id". When the question takes text
  (docket's `acceptsText`) the text is often the answer, and nothing on the screen says so. Rethink
  it from the person's side, in this order: the question; the choices, the recommended one first and
  marked once with its reason as its own line; the field where they write, opening under the choice
  it belongs to, named for what is asked ("Your answer"), required when the question requires it and
  absent when it takes none; one primary button that says what it does ("Send answer"), with Decline
  a plain secondary action and not a choice card. Risks and token estimates are a quiet line on the
  choice, not red blocks, and the screen adds no sentence that is not a label or a verb: the question
  does the explaining. The words are the product's own ("question", "answer"; no "resolve", no
  "option id"). The same component serves approvals (Approve once, Approve for this task, Deny with a
  reason) and inputs, so the three read alike. Done when a person with a question that takes text
  answers it without searching for the field, by keyboard too; the order, the field's label and the
  button's words are pinned by component tests; and the live suite's locators and the tutorial's
  screenshot of a question are updated.

**Phase 93 — an item is a group or work** (2026-11-05, no plan file; ADR 0036 amended with
decisions 10 to 13, proposed). An epic is a card in the To Do column beside the tasks it holds, and
it never leaves that column. Auto-start can start it as if it were a task, and the Dashboard counts
it as one. "Subtask" is a type and a position at once. The Board, the List, the Timeline and the
Orchestration view each work out on their own what a parent is, and the List cannot move a row under
another parent. The board also has two derivations, the web's `deriveBoard` and the server's saved
boards, which only the CLI reads.

The rule is one: a type has a level, an item is a group when its type is a group or it has
children, work runs, a group is planned, and each view shows the level it is for. Nothing old stays
beside the new: no reader for the old type words, no alias, no second board.

There are eight cards in five rounds: A; B; C ∥ D; E; F ∥ G ∥ H. Each card writes its own lines in
`[Unreleased]` and in the book. The full suites and `.githooks/pre-push` run once per round. The
phase is done when the live suite passes with its locators updated by the card that changed each
screen. The screenshots of the Board and the List wait for the next billed recording.

- **A — a project's types are a catalog.** A table `item_types` (project, key, name, level `group`
  or `work`, colour token, order, and one default work type per project) replaces:
  - the `ItemType` enum with its `Custom` variant, and `parse_item_type`;
  - the six type words of `VOCABULARY_KEYS` and of every `vocabulary_for_type` preset (each preset
    becomes a catalog);
  - the web's `ITEM_TYPE_META` emojis, `typeBadgeTone`'s switch and `typeKey`'s `{custom}` branch.

  `items.item_type` references the catalog. The migration builds each project's catalog from its
  preset and from the types its items use, taking the names from its vocabulary; `epic` is a group
  and every other type is work. It rewrites the `subtask` rows to the project's default work type,
  and they keep their parent.

  `GET/POST/PATCH/DELETE /api/projects/{id}/types` serve the catalog; a type in use is removed only
  by merging it into another.

  Every reader of a type reads the catalog:
  - creation and update;
  - the outline, the CSV and the project file, which gains `types:`. A word the catalog lacks is
    added as work, and the preview lists it;
  - the GitHub and Linear imports, which use the default work type;
  - plan-v1 and docket's records: an entry with no type takes the default work type, no longer its
    parent's;
  - change sets, recurrence, triggers and threads;
  - the autonomy domains, which refuse an unknown key;
  - the run's `kind`, the MCP and the CLI;
  - templates, which carry their catalog.

  The User guide's vocabulary page becomes types and terms. In the Developer guide, "Hierarchy is
  free" and the crate tour's `ItemType` section are rewritten.

  Done when the tree holds no `ItemType` enum, no `{custom}` shape and no type word in the
  vocabulary; `openapi.json` and `schema.gen.ts` are regenerated; and an upgrade test migrates a
  database from today's `develop` with its item counts unchanged.
- **B — one rule for the tree, and moving an item.** `tack-core/src/hierarchy.rs` holds the rule:
  - An item is a group when its type is a group or it has children, else work (`Node`).
  - A group holds groups and work; work holds work; a group never sits under work.
  - The tree has at most five levels and no loop, and stays inside one project.
  - A group takes no sprint.

  `UpdateItem` gains `parent_id` (null means the top), applied through `update_item_atomically`.
  Change sets and the MCP gain the operation `parent {item, parent}`, and the CLI gains
  `tack edit --parent`. Every creation path checks the same rule: a person, the outline, a plan, a
  change set and triggers. A refusal carries a stable code and names the rule, for example "A
  group cannot sit under work: Epic X under Task Y".

  Done when the rule's unit tests cover every pair of levels, and every refusal has its code.
- **C — work runs; a group is planned.** `hierarchy::runnable(node, profile_kind)` decides:
  - work runs with any profile;
  - a group runs only with `planner` or `researcher`;
  - anything else is refused before a request exists, with the code `group_not_runnable` and the
    sentence "X is a group: run its work, or plan it".

  `start_run`, auto-start, the Run dialog, change sets and the MCP read this rule. Auto-start's
  query leaves groups out in SQL. The Run dialog gets from the server which profiles a group
  accepts, and the others stay visible but disabled, with the reason.

  A work item that a plan splits becomes a group as soon as its first child exists; its own runs
  stay in its history.

  `run_defaults::resolve` reads who does a piece of work from the item, then from its nearest
  ancestor that names a role or profile, then from the project. `/routing` says where the answer
  came from ("from Epic X"), and auto-start counts the inherited routing.

  Done when:
  - a test proves an epic with a role in the start status is never auto-started;
  - a Planner run on an epic proposes children that land under it once accepted;
  - a task that inherits its team from its epic shows it on Flow and on its page.
- **D — the facts every screen reads.** `ItemCard` gains:
  - `node`;
  - `path`: the ancestors, top first, each with id, title and type;
  - `progress`, which replaces `children_progress`. It covers every work item under the group, not
    only its children: done of total, estimate done of total when the items carry estimates, the
    earliest start and the latest due, the teams, the open runs, and how many need a person.

  One recursive query per project in `tack-db` serves these (`queries/children.rs` becomes
  `queries/tree.rs`), and the counting rule lives in `tack_core::children`. `GET /api/items`
  filters with `node=work|group` and `under=<id>` (the subtree).

  The saved boards are deleted: the `boards` table (dropped by a migration), `BoardGrouping`,
  `repo/boards.rs`, `handlers/boards_multi.rs`, the `/boards` routes and the templates'
  `default_boards`. The live socket becomes `/projects/{id}/live`, with its one caller updated.
  `tack board` reads `GET /api/items?node=work` and groups by status, or by lane with `--by parent`.

  Done when `perf_test` lists a project of 2 000 items in five levels, with `path` and `progress`
  filled, within its current budget, and no route named `boards` remains.
- **E — the board shows work in lanes; the pieces every screen shares.**

  `shared/ui` gains three pieces:
  - `GroupHeader`: the path, the type, a status chip that opens the status menu, a progress bar
    (in points when the work carries estimates, else in items), the teams, how many need you, a
    fold, Add item and Plan with agent;
  - `Path`: the ancestors as a breadcrumb, each one a link;
  - the type tile, coloured from the catalog, with a group drawn as a stacked tile.

  On the Board:
  - The cards are work. The columns, their counts and the WIP limits count work only.
  - The board is in **lanes** by parent, the default when the project has groups. The other choices
    are None, Role, Priority and Assignee; this browser remembers the choice per project.
  - A lane's header is `GroupHeader`. Lanes are ordered needs-you first, as cards are inside a
    column. A lane whose group is in a done status starts folded. Work with no parent goes last, in
    "No parent".
  - A group with nothing in it shows an empty row with Add item and Plan with agent.
  - A card shows its parent only when the board is not in lanes.
  - Dragging a card into another lane moves it under that lane's group. A refusal from B's rule
    stays on screen, with its reason.
  - A project with no groups looks as it does today.

  Deleted: `deriveBoard`, `BoardState` and `boards.ts`, and `Board.tsx`'s `parentTitleOf`.

  Done when component tests pin the lanes, the drag across lanes and the empty group.
- **F — the List is the tree; the item page says what the item is.**

  On the List:
  - A group's row is drawn by its level: a stacked tile, heavier type, and a progress bar in the
    Work column instead of a run chip.
  - Dropping a row onto another moves it under that row. The "Re-parenting isn't supported yet"
    toast and the comment in `types/index.ts` that excuses it are deleted.
  - A choice shows everything, only work, or only groups.
  - Adding a child gives it the default work type.

  On the item page:
  - The path is at the top.
  - A group's page leads with Progress: the bar, its work by state, Add item and Plan with agent.
    Its Run offers only the profiles C allows; the others stay disabled, with the reason.
  - Work with children says "Split into n items" and links them.
  - "Move to…" in the header picks a parent among those B allows.

  The create dialog lists the types grouped by level, always has a Parent field, and its inline
  `subtask` creation is deleted. `ItemHeader` and `CreateItemModal` stop filtering children
  themselves and read D's `progress` and `under=`.

  Done when component tests pin the row shapes, a move by drag and by "Move to…", and a group's Run
  choices.
- **G — the views of time and effort.**
  - **Timeline:** a group is a summary bar over D's span, with its work nested under it and folding.
    The group's own due date is a marker, and the bar turns red when its work ends after that date.
    A summary bar cannot be dragged.
  - **Calendar:** work by due date; a group's due date shows as a milestone chip of its own style.
  - **Sprints:** the backlog and every sprint hold work only. The backlog is set out under compact
    `GroupHeader`s, and a group dropped on a sprint is refused by B's rule.
  - **Dashboard:** the counts, the points and the breakdown by type are over work, plus a Groups
    panel with each group's bar.

  Done when each view's tests pin its level.
- **H — orchestration, search and Settings → Types.**
  - Flow and Map read `node` and `path`, and `flow.ts`'s `hasChildren` and `parents` are deleted. A
    group's planning run is a token marked Planning in its lane.
  - Search and the command palette show each result's path.
  - **Settings → Types** shows the catalog: name, level, colour and the default work type, with
    add, rename, and merge before remove. The Vocabulary panel keeps only the other words.
  - The autonomy domains' types become a picker from the catalog.
  - Templates show their catalog.

  Done when tests pin the Types panel, and no screen works out by itself whether an item is a
  parent.

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
- Limits that stay as limits, each shown in the product with its reason: only docket stops a run at
  its token limit; the tokens a verifier spends count whole, cache included; only docket reports a
  running agent's steps on its timeline; a team's pending memory reaches the inbox in about five
  minutes (the runner looks at docket again every 300 s, the board reads the reports every 30 s) and
  is decided from the sheet only with a docket that lists `memoryDecide`, and with docket's own command
  otherwise; closing a thread in Tack does not close its docket conversation, which ends by its
  own turn cap or when its `turn` question expires; **Resume team** clears a pause but not the spent
  budget, so a team out of budget is paused again until the budget is raised.

---

## Delivered

Phases 73 to 87, newest first. The detail is in each phase's plan under
[`docs/closed-cycles/plans/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/plans),
each decision in its ADR under [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr),
and what a user can now do in the [changelog](https://github.com/yielab/tack/blob/develop/CHANGELOG.md).

- **Phase 91 — what docket offers, Tack uses or says why not.** No ADR of its own (it amends ADR 0034's note
  on `conversation` and ADR 0035's decision 1). docket 0.3.0-beta.1 lists 21 features and Tack used 16.
  A thread on a docket team continues one task turn by turn, and the conversation ends by docket's own
  turn cap or when its `turn` question expires, because the board's closing of a thread does not reach the
  runner. A team's pending memory is a **needs-you** notification in each project that uses the team,
  and its sheet shows what it learnt and what waits, with **Approve** and **Refuse** (which asks for a
  reason) when docket lists `memoryDecide`. A task docket refuses at claim for a method it cannot
  staff reads **Not started** with docket's reason. A person chooses what an agent profile does, so a
  profile that does not change code can run on a team that delivers a report. docket's read-only token
  is not used: every call goes through the runner, which holds the full token. Tack names docket
  0.3.0-beta.1 (the release docket declares) instead of a version never tagged.
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
