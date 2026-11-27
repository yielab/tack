# ADR 0033: the screen says what needs you first, and a project shows its teams at work

**Status: ACCEPTED 2026-11-01** (proposed and accepted the same day). Executed by Phase 84
(`docs/closed-cycles/plans/phase-84.md`). Amended 2026-11-26, accepted the same day: decisions 6 to 16 below. Keeps ADR 0032's rule: nothing on the screen names a harness; a team, its
members and its steps are what any agent reported, and what it did not report is shown as not reported.

**The rule behind every decision.** Every control, setting and fact on screen today stays. What changes
is order, weight and form: one run-state language, one primary action per place, summary before detail
before raw, and a graphic wherever a sentence was drawing a diagram. The one exception is the Table
view, which is removed, and its abilities move to the List.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **Starting a run can move the item.** A project setting "When a run starts" sits beside "When a run finishes", with the same choices. New projects start with the workflow's first in-progress status; existing projects keep "Do not move" | A task whose run is working is in In Progress, not in Backlog |
| 2 | **A card shows priority only when it is not the project's default.** The List, the item and the filters always show it. "Always show priority on cards" in Display & accessibility brings back today's card | Every card no longer says "Medium" |
| 3 | **"Propose items from a file" moves** from the top of every item to the item's "⋯" menu and to a drop zone on the Execution tab. It opens the same accept panel | The first screen of an item is what to do now, not a file input |
| 4 | **The Table view is removed; the List does what it did.** The List gains sorting by a column, a filter, editing a cell in place (title, status, priority, assignee, due date), choosing columns and two row heights. `/table` and a remembered "table" view open the List | Five work views instead of six, and the one that shows the hierarchy can also sort and triage |
| 5 | **A new work view, Orchestration** (a project can rename it), in two modes. **Flow**: one lane per team that works in the project, its stations being the team's own steps as the agent reported them, each task placed at the step it is in, with what waits on a person, what nobody will pick up and what each team is spending. **Map**: who does what, from roles to profiles to teams to members, and the teams compared side by side. A project where no agent has ever run shows the view disabled with its reason | The work of several teams can be followed as it moves, and the reason a task goes to one team and not another can be seen |

**Amended 2026-11-02 (Phase 86, `docs/closed-cycles/plans/phase-86.md`).** Decision 1 is reversed: a run that starts no
longer moves its item, and neither does a run that finishes. A task's status is a person's, and its
work is what its runs report; the two are shown apart. Nothing else in this ADR changes.

**Why now.** The live suite's screenshots of 2026-11-01 and the My SaaS onboarding of 2026-10-31 show
that the board keeps tasks in Backlog while their runs work, that the item and run screens put the one
thing to do below a dozen sections of equal weight, and that nothing on screen shows a team's process,
its queue or the path from a role to a team: those are read today from three settings pages and an
Execution tab at a time.

**If you do nothing.** A person running several teams reads every card's pills to find what needs
them, opens runs one by one to learn where each stands, and cannot see which tasks no team will take
until auto-start fails to start them.

## Amendment of 2026-11-26: the work surface is cut by what a person must do (accepted)

**Decided:** the project's destinations are cut by the question each one answers, now that what is
scarce is a person's time to specify, decide and verify, not the hours of writing code. Two
destinations that answer the same question become one, with no ability lost; what waits for a
person is counted and can be limited; and a task that cannot be verified says so before anyone
runs it. Board, List, Orchestration, Sprint and Threads stay. This serves delegation (Level 3) and
the factory (Levels 4 and 5); live supervision of an agent's session is not Tack's.

| # | Decision | What a person sees |
|---|---|---|
| 6 | **Needs you comes first and takes in Change sets.** One project destination lists every wait for a person in this project: questions, runs ready for review, failures not yet looked at, change sets to apply and thread turns that wait, oldest first, each with its item and how long it has waited. A filter shows what was decided (applied, turned down, answered), which is what the Change sets page showed. The sidebar's **Waiting for you** stays as the same list for every project | The day starts on one page that holds only exceptions |
| 7 | **Calendar becomes a layout of the Timeline.** One destination, **Timeline**, with **Bars** (today's Timeline) and **Month** (today's Calendar, with its tray and its drag to a day). Both are kept | Nine destinations instead of eleven with decision 8 |
| 8 | **Factory metrics become part of the Overview.** The Overview has three parts: **Progress** (today's Overview), **Agents' work** (today's Factory metrics) and **Autonomy by domain** (each domain's level, what merged by itself and what was undone, read from the ladder). It is the retrospective: what kinds of task are accepted alone, and where a level should go up or down | One page to read how the project is going |
| 9 | **What waits for a person is the work in progress.** The Board's row and Needs you count the waits, and a project may set **At most N waiting for a person**. At the limit, auto-start starts nothing new, and the board says why. A column's WIP limit stays as it is | "4 of 5 waiting for you: auto-start is paused until one is decided" |
| 10 | **A task says whether it can be verified.** A task with no Done when, or with neither a Done when nor a check a machine runs, reads **Not ready** on its card, its List row and its page, with what is missing. The run dialog shows the same line and still lets the person run it. Its risk shows as a chip when it is medium or high. The List can filter **Not ready**: that filter is refinement | "Not ready: no Done when" on a card. "Risk: high" beside the type |
| 11 | **A sprint shows the decisions it will ask for.** Each lane says, beside its points, how many of its items wait for a person and the median minutes a decision has taken. Sprints and continuous flow stay side by side; nothing ends a sprint by itself | "12 of 20 points · 3 waiting for you · a decision takes 14 min" |
| 12 | **The item page says which fields are the plan's.** In a project with a revision, the fields the project folder holds (title, description, type, parent, blockers, roles, fields, the brief) carry a small "In the project folder" mark. Status, assignee, priority, sprint and due date read "The board's". A plan field changed on the board since the last revision says the next export writes it | "Changed on the board since revision #7: the next export writes it" |

| 13 | **The old addresses go.** `/calendar`, `/factory`, `/change-sets` and `/table` are no destinations and are not redirected; a view remembered under an old name opens the Board. Nothing on screen says where a page used to be | A bookmark to an old page answers "Page not found" |
| 14 | **Four radii and four kinds of button.** Corners are 4 px (a tag, a chip), 6 px (a button, a field), 8 px (a panel, a dialog) and round only for what is round (an avatar, a dot, a switch). A button is primary (solid, one per place), secondary (outlined), ghost (text) or destructive (red text); rare actions go in a "⋯" menu. A status, a button, a field and a tag each have their own shape | Nothing nested in three rounded cards; the one action to take is the one solid button |
| 15 | **A heading says where to start reading.** The text scale is title 34, key step 21, section 19, subsection 15, reading text 16 to 17 and meta 13 to 14; each level rises by at least 2 px or changes weight. Sections are 48 px apart, a heading is 16 px above its content, a main surface has 40 to 48 px of padding and a panel 20 to 24. Colour says what a thing is for: solid teal for the next action, amber for what needs a person, soft teal for what the agent reads, green for done, and white content on a tinted page | A page reads top down: what it is, what to do now, then the detail |
| 16 | **A task is written for a person first and for the agent apart.** The item page and its drawer go: title, then the next step (or what needs you, in amber), the description, Done when with its progress, **For the agent** in its own panel (the checks a machine runs, the constraints, the risk and the run's limits, folded in the drawer), and the activity; the properties stand on the right. The run dialog says in a few rows what will happen (agent, where, checked by, ask first, network) and folds the rest into **Options**. An approval asks one question in words, lists what it changes, and keeps the raw call behind a link | The description is the story; nothing the agent needs is mixed into it |

**If you do nothing:** a person still reads three places (the inbox, Change sets, Threads) to learn
what waits for them, two pages that both lay work out by date and two that both count it, and a
task without a way to verify it looks like any other until its run fails.

**The maintainer's answers (2026-11-26).** The destination is named **Needs you**. Auto-start
pauses at the waiting limit (decision 9). **Not ready** is a warning that never blocks a run
(decision 10). Decisions 13 to 16 come from the mock-ups the maintainer reviewed the same day.

### What each destination answers, and why the others stay

| Destination | The question | Verdict |
|---|---|---|
| Board | Where is each task in my workflow, and what is its work doing | Stays. Its lanes are by parent, priority or assignee; the team's own steps are Orchestration's |
| Orchestration | Which team has it, at which of that team's steps, and why that team | Stays. Its lanes are teams and their reported steps, never a column of the workflow |
| List | The whole tree, sorted, filtered, edited in place | Stays, and gains the **Not ready** filter |
| Timeline (+ Month) | When work is due and how long it spans | Decision 7 |
| Sprint | What this cycle commits to, and what it will ask of a person | Stays, decision 11 |
| Needs you | What waits for me | Decision 6 |
| Threads | Where intention comes in, as a conversation with a manager | Stays: it is the door, not a queue. A turn that waits for a person is listed in Needs you too |
| Overview | How the plan is going and how well the agents' work is going | Decision 8 |
| Project settings | How the project is set up | Stays |

### Evidence for the amendment

- 11 project destinations today (`frontend/src/shared/ui/destinations.ts`): Board,
  Orchestration, List, Calendar, Timeline, Sprint, Change sets, Threads, Overview, Factory
  metrics, Project settings, plus the Inbox drawer.
- The user guide describes the Calendar as "Monthly grid positioned by due date" and the
  Timeline with "Week, Month and Quarter" zooms over the same due dates (`views.md`).
- The Overview already links to Factory metrics and already counts "the items that need you now"
  (`views.md`, Overview).
- "Where they wait for you": change sets that wait appear in **Waiting for you** and on the
  **Change sets** page at once (`reviewing-runs.md`).
- `Item` carries no brief, Done when or risk (`docs/openapi.json`, schema `Item`), so no card can
  say it cannot be verified today. The risk exists only in the brief.
- `/autonomy` already serves `merged` and `undone` per domain; `work_quality` already serves
  `revert_after_merge` and `verifier_agreement`; `human_minutes_per_decision` is a factory figure.

### Not adopted in the amendment

- **Board lanes by team in place of Orchestration's Flow.** The Board's columns are the person's
  workflow and the Flow's stations are a team's reported steps; one view cannot be both.
- **Ending sprints.** Most teams still plan in sprints, and the formal process is the last thing
  to change. The sprint gains its decision load instead (decision 11).
- **Threads inside Needs you.** A thread is where a person says what they want; only a turn that
  waits is a wait.
- **Roles named Lead, Implementer and Reviewer on screen.** A team's methods are what it reports
  (ADR 0036); no view assumes a fixed set of roles.

## Evidence

- Rerun `rerun-local-00e2b25d-b` (git-ignored `onboarding-lab/results/docket-prill/`), shots 13, 16, 30,
  37: items with Running, Finished, Needs your review and Failed runs stay in Backlog; In Progress shows
  0 until auto-start moves the proposed items.
- Shots 22, 23, 26: a decision id with no break widens the item drawer and pushes its content off the
  left edge.
- Shots 11, 14, 22: the item's first screen is properties and a file input; "Run again" appears twice.
- Shots 15, 17, 20, 24, 25: a run is about twelve sections in one scroll; the decision-token field shows
  on runs that asked nothing.
- Shots 12, 19, 21: the run dialog repeats one capabilities sentence per team; teams with no profile
  take a full card each.
- Shots 04, 07, 08: the agents' capabilities are measurement notes in four columns; the merge ladder,
  its blockers and the ceiling say the same thing in three places.
- Shot 36: the accept panel's dependencies are numbered checkboxes.
- The user guide (`views.md`) sends a person to the Table to sort and filter because the List cannot;
  that is the Table's only use the List lacks.
- The data for Flow exists: `GET /api/teams` (members, steps, queue, works in, decides), the item's
  latest execution, and the hop events `stepMatch.ts` already matches to steps. What is missing is one
  call that returns a project's open runs with the step each is in.

## Considered and not adopted

- **A run-state swimlane on the Board.** The Board's columns are the person's workflow; a team's steps
  differ per team, so they get a view of their own.
- **Keeping the Table and hiding it.** Two views over the same rows, one of them hidden, is two things
  to keep for one use.
- **A free-form graph canvas of agents.** Teams report ordered steps, not arbitrary graphs; lanes of
  steps show exactly what was reported.
