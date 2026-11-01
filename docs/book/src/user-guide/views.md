# Views

Tack has five **work tabs** — Board, List, Calendar, Timeline and Sprint (the last takes your
project's word for a sprint) — all showing the same items. An item created in one appears at once
in the others. Two more screens, **Overview** and **Factory metrics**, are in the sidebar and show
project statistics.

Every view shares the same shell: a sidebar to switch project and view, a top bar with item
[search](command-palette.md) and a **+ New** button, and the [command palette](command-palette.md)
on `Ctrl+K`. Theme, palette, text size and zoom are set in Settings → Display & accessibility — see
[Appearance](appearance.md).

---

## Board

Kanban columns driven by the project's workflow statuses.

- Each column is one workflow status. Names, order, and WIP limits come from **Settings → Workflow**; there is no separate "create a board" step.
- **Drag and drop** a card to change its status. The change saves at once.
- **Move a card by keyboard.** Tab to the small arrow on a card, or hover the card to see it, and press Enter or Space to open **Move to…**. The arrow keys choose a column, Enter moves the card and the page announces where it went, and Escape closes the menu. Each column is a named region that scrolls inside itself: Tab into its list and use the arrow keys or Page Down to reach the last card, while the board's own scroll bar stays at the bottom of the screen.
- **WIP limit** is shown in the column header when set. Dragging a card into a full column is blocked.
- **Click a card** to open the item drawer (tabs Overview, Activity, Execution, Dependencies, Files and Fields; see [Working with Items](items.md)).
- **A card reads top to bottom.** The title comes first. Under it one line says where the card's latest run is (**Queued**, **Running**, **Needs you**, **Ready for review**, **Done**, **Failed** or **Cancelled**; press it to open the task's Execution tab, or the run's question when it is asking one), then the team that ran it, then the priority, then the type. The priority is left out when it is **Medium**, the default; turn on **Always show priority on cards** above the board to show it on every card (this browser remembers the choice). A card whose run **needs you** or **failed** has a coloured bar on its left edge, and so does one whose run is going in a column no work has started in, so the board does not look idle. The card's short id shows when you hover it and at the top of its **Move to…** menu. The ▶ button, **Run with agent**, is hidden while a run is going and comes back when it ends.
- **Which cards need you.** Under the board title, when the project has run anything, a row counts the runs: **Waiting for you** (runs that need you plus runs ready for review; failed runs are counted under **Failed**), then **Queued**, **Running**, **Needs you**, **Ready for review** and **Failed**. Press a count to show only those cards, and again to show all. Each column header has a small bar of its cards' states, with a segment for each; pressing a segment filters the same way, and the column's count then reads "2 of 5".
- **More on a card.** **Repeats** means closing the task creates the next one. Two more show what Tack did without you: **Started by itself** and **Merged by itself**. The List shows the same badges in its Details column. See [Automating a Project](automation.md).
- **Add an item** with the **Add item** button in a column header (it starts in that status), the **+ New** button in the top bar, or the `n` key (first column).
- Press `r` to refresh the board. Changes made in another browser tab or by another person appear without refreshing.
- An empty project shows a three-step checklist until the first item exists.

---

## List

Every item as a row, children indented under their parent, with a header row to sort by and the
controls to filter, edit and rearrange the columns.

- Rows are ordered by creation date within each level until you sort. A parent with children shows how many it has; use the arrow to expand or collapse it, or **Expand all** and **Collapse all** in the header.
- Each row shows the title, priority, status, assignee, due date, where the item's latest run is, and the badges the Board shows. Click the title to open the item drawer.
- **New Item** in the header adds an item at the top level. On a row, **+** (shown on hover or keyboard focus) adds a child under it and the bin deletes it after a confirmation.
- When every child of an item reaches a Done status, the parent moves to Done too, and so on up the hierarchy.
- **Sort.** Press a column header to sort by it, again to reverse, and a third time to go back to creation order. Priority sorts Critical, High, Medium, Low, None; **Run** puts what needs you first, then what is going, then what is over, and items with no run last. Sorting reorders the rows within each parent, so a child never leaves its parent.
- **Filter.** **Filter…** matches as you type against the title, assignee and status, and the run-state list beside it keeps only items whose latest run is in that state (**Waiting for you**, **Queued**, **Running**, **Needs you**, **Ready for review** or **Failed**). A parent that holds a match stays in view, open, so the match is never shown without where it belongs; **Clear filter** brings every row back.
- **Run.** The Run column says where the item's latest run is, in the same words and symbols as the Board (**Queued**, **Running**, **Needs you**, **Ready for review**, **Done**, **Failed**, **Cancelled**); an item that has never run shows a dash.
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
- A breakdown by workflow status, by priority and by item type, as bars.
- Progress in story points (or your project's word for them), when items carry estimates.
- Everything is computed from the items when you open the page.

---

## Factory metrics

Read-only, per project — **Factory metrics** in the sidebar. It answers how well agent work is
going on this project: how often a run needed a person, how long the person took, what verification
cost, how many changes were accepted and merged, and what the runs cost. A card with nothing to
measure says **Not measured** and why, never `0`. Every card is defined in
[Factory metrics](reviewing-runs.md#factory-metrics).

<img src="../screenshots/factory-metrics.png" width="98%" alt="The Factory metrics page: escalation rate 50% (1 decision / 2 attempts), human minutes per decision Not measured with its reason, human minutes per pack review, pack acceptance rate 100%, verification tax 4.2% in tokens with its inputs, and pull-request outcomes.">
