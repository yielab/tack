# Views

Tack has six **work tabs** — Board, List, Table, Calendar, Timeline and Sprint (the last takes your
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
- **Badges on a card** say what has happened to it. **Needs your review** means a finished run is waiting for you to accept or reject it. **Repeats** means closing the task creates the next one. Three more show what Tack did without you: **Started by itself**, **Team: name** (the docket team that ran it) and **Merged by itself**. The List shows the same badges, and the Table shows them in its Review column. See [Automating a Project](automation.md).
- **Add an item** with the **Add item** button in a column header (it starts in that status), the **+ New** button in the top bar, or the `n` key (first column).
- Press `r` to refresh the board. Changes made in another browser tab or by another person appear without refreshing.
- An empty project shows a three-step checklist until the first item exists.

---

## List

Every item as a row, children indented under their parent.

- Rows are ordered by creation date within each level. A parent with children shows how many it has; use the arrow to expand or collapse it, or **Expand all** and **Collapse all** in the header.
- Each row shows the type, title, priority, status, tags and the badges the Board shows. Click the title to open the item drawer.
- **New Item** in the header adds an item at the top level. On a row, **+** (shown on hover or keyboard focus) adds a child under it and the bin deletes it after a confirmation.
- When every child of an item reaches a Done status, the parent moves to Done too, and so on up the hierarchy.
- The List does not filter or sort, and a row cannot be dragged under another parent. Use the [Table](#table) to sort and filter.

---

## Table

A dense spreadsheet view of every item — title, type, status, priority, assignee, due date and a
Review column that flags runs waiting for you.

- **Click a column header** to sort by it; click again to reverse.
- **Filter…** matches as you type against the title, assignee and status.
- **Inline edit:** click an editable cell (title, status, priority, assignee, due date) to change it in place; the edit saves at once.
- **Columns** shows or hides columns, and **Compact** and **Comfortable** switch the row height. Both choices are remembered in this browser.
- Best for scanning or triaging a large backlog where the card layout is too tall.

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
