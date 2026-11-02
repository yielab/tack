# Views

Tack has six **work tabs** — Board, Orchestration, List, Calendar, Timeline and Sprint (the last
takes your project's word for a sprint) — all showing the same items. An item created in one appears at once
in the others. Two more screens, **Overview** and **Factory metrics**, are in the sidebar and show
project statistics.

Every view shares the same shell: a sidebar to switch project and view, a top bar with item
[search](command-palette.md) and a **+ New** button, and the [command palette](command-palette.md)
on `Ctrl+K`. Theme, palette, text size and zoom are set in Settings → Display & accessibility — see
[Appearance](appearance.md).

---

## Status and Work

Every task has two things to read, and the screen keeps them apart.

- **Status** is the column the task sits in. It is yours: you set it by hand, by dragging the card, by applying
  a change set or by importing a file. Nothing an agent does moves it.
- **Work** is what the agents report about the task's newest run: **Queued**, **Running**, **Needs you**,
  **Ready for review**, **Finished**, **Failed** or **Cancelled**. You do not set it; it follows the run.

So a task in Backlog with a run that is Running shows both, and neither is wrong. A work word is never a
status name: "Ready for review" is the work, "In Review" is a column. On the task, the header names **Status**
with its buttons and the **Work** panel below it names the run's state and the one thing to do next. The
Board's strip, the sidebar's list and the List give the same count for the same run.

---

## Board

Kanban columns driven by the project's workflow statuses.

- Each column is one workflow status. Names, order, and WIP limits come from **Settings → Workflow**; there is no separate "create a board" step.
- **Drag and drop** a card to change its status. The change saves at once.
- **Move a card by keyboard.** Tab to the small arrow on a card, or hover the card to see it, and press Enter or Space to open **Move to…**. The arrow keys choose a column, Enter moves the card and the page announces where it went, and Escape closes the menu. Each column is a named region that scrolls inside itself: Tab into its list and use the arrow keys or Page Down to reach the last card, while the board's own scroll bar stays at the bottom of the screen.
- **WIP limit** is shown in the column header when set. Dragging a card into a full column is blocked.
- **Click a card** to open the item drawer (tabs Overview, Activity, Execution, Dependencies, Files and Fields; see [Working with Items](items.md)).
- **A card reads top to bottom.** The title comes first. Under it one line says the card's work, where its latest run is (**Queued**, **Running**, **Needs you**, **Ready for review**, **Finished**, **Failed** or **Cancelled**; press it to open the task's Execution tab, or the run's question when it is asking one), then the team that ran it, then the priority, then the type. The priority is left out when it is **Medium**, the default; turn on **Always show priority on cards** in Settings, under Display & accessibility, to show it on every card (this browser remembers the choice). A card whose run **needs you** or **failed** has a coloured bar on its left edge, and so does one whose run is going in a column no work has started in, so the board does not look idle. The card's short id shows when you hover it and at the top of its **Move to…** menu. The ▶ button, **Run with agent**, is hidden while a run is going and comes back when it ends; a card whose run is ready for review shows a **Review and merge** button instead, which opens the Execution tab.
- **Which cards need you.** Under the board title, when the project has run anything, a row titled **Work** counts the runs by state: **Queued**, **Running**, **Needs you**, **Ready for review** and **Failed** (the sidebar's **Waiting for you** gives the same counts, and also counts failures you have not looked at yet). Press a count to show only those cards, and again to show all. Each column header has a small bar of its cards' work, with a segment for each; pressing a segment filters the same way, and the column's count then reads "2 of 5".
- **More on a card.** **Repeats** means closing the task creates the next one. Two more show what Tack did without you: **Started by itself** and **Merged by itself**. The List shows the same badges in its Details column. See [Automating a Project](automation.md).
- **Add an item** with the **Add item** button in a column header (it starts in that status), the **+ New** button in the top bar, or the `n` key (first column).
- Press `r` to refresh the board. Changes made in another browser tab or by another person appear without refreshing.
- An empty project shows a three-step checklist until the first item exists.

---

## Orchestration

Where the Board answers "where is this item in my workflow", Orchestration answers who is doing it,
how far along its team's own process it has got, what is waiting for you and what nobody will take. It
is a picture of the agents' work in this project, drawn from the items, their runs, the profiles and
roles, and what each team reports about itself. It follows changes as they happen, and its tab stays
off, with the words "No agent has worked in this project yet", until a task has run; opened by its address
before then, the page says the same and links to the Agents page.

**The summary row.** Four counts of the tasks' **Work** sit under the title: **Needs you** (runs that ask you something plus
runs ready for review), **Running**, **Queued** and **Nobody will take it**. Press one to show only the
lanes with a task in that state, and press it again to show all. Under them one line says how many runs
are open in this project against the number it keeps open by itself, and how many runs the computers
take at a time.

**One lane for each team.** Every team that has a profile this project uses gets a lane, and so does
each single agent it uses; a single agent is a lane with one station. Lanes that need you come first,
then lanes with work running, then idle lanes, which fold to their header (press the arrow to unfold
one). The header names the team (press the name to open its card on the Agents page) and shows what the
team reported about itself: the shape it gave itself, whether it works in the repository or in a folder,
whether it runs to the end or stops to ask a person, its network, and, as a line of words, how its
changes are merged here (you merge them, or the team runs lights-out). A fact the team did not report is
named once as not reported. The team's members are a row of round marks, one for each role: a ring means
the member changes the project, a small dot means it can propose items, and hovering or focusing a mark
tells its model. Two meters finish the header: a bar of what the team is running and what waits, and the
tokens its runs counted today against the project's daily limit, or the count alone when the project has
no daily limit.

**The stations.** A lane reads left to right. **Up next** holds the tasks routed to the team that have
not started, most urgent first, the order the project takes them in; each says whether it **starts by
itself** (and at which column) or is **by hand**, and a queued task says why it has not started. Then
come the team's own steps, as it reported them. A check between two steps is a small mark between them:
a command, a reviewer, a person or a check outside the team; a step that runs side by side with another
is drawn as a stack. Then **Needs you** and **Done today**, whose count unfolds to the tasks that ended
in the last day. A team that reports no steps has one station, **Working**, which says the agent reports
no steps; so does a run whose current step the team cannot name.

**A task is a token.** It shows its type, its title, the title of its parent, the run's state in the same
words and marks as everywhere else, and how long the run has been going. A task a reviewer or a check
sent back shows a small arrow with how many times out of how many it may go back. A task that waits for
another says **after:** with that task's title and lane; pointing at the line, or tabbing to it, lights
up the task it waits for. A task whose run proposed items says how many were created, and a task
created from a proposal says **from:** with the task it came from. Press a token's title to open the
task on its Execution tab (on the run's question when the run is asking).

**Nobody will take it.** When a task goes to no lane, a lane at the top, in the warning colour, lists it
with the reason: it has no role and the project has no default profile, its role has no profile, the
profile it uses no longer exists, or no computer reports the team its profile names. Each has the
**Who does it** choice beside it, so a choice made there is saved on the task at once.

**Teams not used here.** A folded row at the foot lists the teams no profile of this project uses.
**Make a profile for it** is off for each, with the reason; profiles are made on the Agents page.

**Sending a task to another team.** A task that has not started can be dragged to another lane's Up next,
or sent with the small arrow on the token, **Move to…**, which opens a menu of the other lanes (arrow
keys choose, Enter picks, Escape closes). Either way Tack asks first, then changes the task's **Who does
it** to a profile of that lane; nothing starts because of it, and the choice can be changed from the task
as before. A task a run holds, a running one among them, cannot change hands: its arrow is off and says
why, and a lane that cannot take the task will not accept the drop. A screen reader hears each move,
and each task that changes station, in a polite announcement such as "Collect the notes: review, saas-notes".
With motion reduced, in Tack's display settings or by the system's, a task simply appears at its new
station instead of sliding to it.

**Flow and Map.** A switch beside the title chooses between **Flow**, which is everything above, and
**Map**. Flow is what you see first, and Tack remembers in this browser which of the two you left on.

**The Map.** Where Flow shows work as it happens, the Map shows the structure behind it: how a task finds
its team. Four columns read from left to right, **Roles**, **Profiles**, **Teams** and **Members**. A
line joins a role to its profile, a profile to the team (or single agent) that runs it, and a team to the
members it reported, and a line is as wide as the number of open tasks routed along it, so the busy
routes stand out and a route nothing uses is a hairline. Under each team's name the Map says where it
runs, on this computer or on another. The project's default profile is marked, and a task that names no
role and no profile of its own goes to it. A role with no profile and a team no profile names are drawn
with a dashed edge, say so in words, and carry the fix as a link: **Settings → Automation → Teams** to
give the role a profile, **Agents → profiles** to make one for the team. The Map is a view; nothing is
edited on it.

Press a box, or a column's title, to go to Flow narrowed to what passes through it: the lanes that role,
profile, team or member can reach, and the tasks nobody will take because of it. A line above the lanes
says what Flow is narrowed to, and **Show everything** takes it off. With the keyboard, Tab enters the
Map at one stop; the arrow keys move between boxes (up and down along a column, left and right to a
box it is joined to), and Enter opens the box in Flow. The same routes are written out as sentences, one
for each line, under **Routes as text**, together with what is dashed and what is the default.

**Compare.** Under the Map, a table sets the teams side by side, one column each, with these rows: where
the team works, what it delivers, whether it runs to the end or stops for a person, its network, its
members, which of them change the project, its steps and what checks each, which can propose items, how
its changes are merged here, the token limit for one run and the tokens counted today. A fact a team did
not report reads **not reported**, never a guess; hover it, or read on, for why. What a team delivers
and the limit for a run come from the profiles that name the team, and a run's limit falls back to the
project's provisional one when the profile sets none.

---

## List

Every item as a row, children indented under their parent, with a header row to sort by and the
controls to filter, edit and rearrange the columns.

- Rows are ordered by creation date within each level until you sort. A parent with children shows how many it has; use the arrow to expand or collapse it, or **Expand all** and **Collapse all** in the header.
- Each row shows the title, priority, status, assignee, due date, its work (where the item's latest run is), and the badges the Board shows. Click the title to open the item drawer.
- **New Item** in the header adds an item at the top level. On a row, **+** (shown on hover or keyboard focus) adds a child under it and the bin deletes it after a confirmation.
- Moving the last child of an item to Done does not move the parent: a status is only set by a person.
- **Sort.** Press a column header to sort by it, again to reverse, and a third time to go back to creation order. Priority sorts Critical, High, Medium, Low, None; **Work** puts what needs you first, then what is going, then what is over, and items with no run last. Sorting reorders the rows within each parent, so a child never leaves its parent.
- **Filter.** **Filter…** matches as you type against the title, assignee and status, and the work list beside it keeps only items whose latest run is in that state (**Your turn**, **Queued**, **Running**, **Needs you**, **Ready for review** or **Failed**). A parent that holds a match stays in view, open, so the match is never shown without where it belongs; **Clear filter** brings every row back.
- **Work.** The Work column sits beside Status and says where the item's latest run is, in the same words and symbols as the Board (**Queued**, **Running**, **Needs you**, **Ready for review**, **Finished**, **Failed**, **Cancelled**); an item that has never run shows a dash. Status is the column you chose; Work is what the agents report, so a row can be in Backlog and Running at once.
- **Edit in place.** Press a status, priority, assignee or due date to change it where it is, and the title's pencil to rename it (the title itself opens the item). Enter or leaving the field saves, Escape puts the value back, and the keyboard returns to the cell.
- **Columns** shows or hides columns (the title stays: rows are named and nested by it), and **Compact** and **Comfortable** switch the row height. Both choices are remembered in this browser. **Type** is hidden until you show it, because each row already carries a type tile.
- A row cannot be dragged under another parent.

---

## Calendar

Monthly grid positioned by due date. Drag to reschedule.

- Items appear on the day of their due date. Items without one are listed in the **Unscheduled** tray under the grid.
- **Drag** an item from one day to another to change its due date, or from the tray onto a day to schedule it. Drop an item on the tray to clear its due date.
- **← Previous**, **Next →** and **Today** move between months.
- Click an item to open the item drawer.

---

## Timeline

Gantt-style horizontal bar chart. Drag to reschedule.

- Each item with a start date or a due date is a bar spanning its dates.
- **Drag a bar** sideways to shift both dates. **Drag either edge** to change the start date or the due date on its own.
- **Week**, **Month** and **Quarter** in the toolbar set the zoom; **Today** returns to the present.
- An item blocked by another carries a marker.
- Scroll sideways to see a longer project. Click a bar to open the item drawer.

---

## Sprint

Two-pane sprint planning. The tab and the words below follow your project's [vocabulary](vocabulary.md): a construction project sees "Phase", for example.

- **Left pane (backlog):** items not assigned to any sprint, sorted by priority.
- **Right pane:** one lane per sprint that is not closed, showing its dates, the done and total items, the points done and planned, and a progress bar. **New sprint** adds one; **Edit** on a lane changes its name, goal and dates.
- **Drag from the backlog** into a lane to assign the item. Drag between lanes to reassign. Drag back to the backlog to unassign. A sprint accepts items only while it is planning or active.
- A sprint moves through *planning*, *active*, *review* and *closed*. The button on each lane takes it to the next one: **Start**, **Complete** (to review), then **Close**.

---

## Overview

Read-only project statistics, in the sidebar.

- Tiles for the total number of items, how many are completed, the completion rate and what was completed in the last 7 and 30 days.
- Once the project has run anything, a **Runs** row: a bar of the runs started in the last 7 days by outcome, with the count of each in words beside it; the number of items that need you now (a question to answer, or a run ready for review); and a link to **Factory metrics**. Each item counts once, by its latest run. **Tokens per team** is shown disabled, with the reason, because this page does not load the attempts the tokens are counted from.
- A breakdown by workflow status, by priority and by item type, as bars.
- Progress in story points (or your project's word for them), when items carry estimates.
- Everything is computed when you open the page, from the items and each item's latest run.

---

## Factory metrics

Read-only, per project — **Factory metrics** in the sidebar. It answers how well agent work is
going on this project: how often a run needed a person, how long the person took, what verification
cost, how many changes were accepted and merged, and what the runs cost. A card with nothing to
measure says **Not measured** and why, never `0`. Every card is defined in
[Factory metrics](reviewing-runs.md#factory-metrics).

<img src="../screenshots/factory-metrics.png" width="98%" alt="The Factory metrics page: escalation rate 50% (1 decision / 2 attempts), human minutes per decision Not measured with its reason, human minutes per pack review, pack acceptance rate 100%, verification tax 4.2% in tokens with its inputs, and pull-request outcomes.">
