# Views

FlexPM has seven views. Five of them — **Board, List, Tree, Calendar, Timeline** — are lenses
onto the same item set. Switching between them does not move or filter data; an item created
on the Board appears immediately in List, Tree, Calendar, and Timeline. The other two —
**Dashboard** and **Sprints** — are separate screens for statistics and sprint lifecycle
management.

---

## Board

Kanban columns driven by the project's workflow statuses.

- Each column corresponds to one workflow status. Names, order, and WIP limits come from **Settings → Workflow**.
- **Drag and drop** a card to change its status. Changes are saved immediately with optimistic UI.
- **WIP limit** is shown in the column header when set. Dragging a card into a full column is blocked.
- **Click a card** to open the item detail drawer (Details, Fields, Dependencies, Activity, Files tabs).
- **Add an item** via the `+` button inside a column header (pre-sets the status) or the toolbar **+ New** button.
- Board state syncs in real time via WebSocket — opening the same board in two browser tabs shows changes from both.
- Empty projects show a three-step onboarding checklist until the first item is created.

---

## List

Sortable, hierarchical table of all items.

- Items appear as a tree: root items at the top, children indented. Expand/collapse with the ▸ arrow.
- **Inline create:** click `+` at any level to create a new item at that position.
- **Inline edit:** click any field (title, type, priority, status) in the row to edit it.
- **Bulk operations:** check multiple rows, then use the bulk action bar to move all to a new status or delete them.
- Filter by status, priority, and type using the toolbar dropdowns.

---

## Tree

Parent/child hierarchy view.

- Epics at the root, features and tasks nested beneath them.
- Click an item title to open the detail drawer.
- **Auto-complete:** when every child of an item reaches a Done status, the parent auto-moves to Done. This cascades up — a fully completed feature can close its epic automatically.

---

## Calendar

Monthly grid positioned by due date.

- Items appear on the day matching their `due_date`. Items without a due date are not shown.
- Navigate months with `←` / `→` or jump to today.
- Click an item to open the detail drawer and edit it or change the due date.

---

## Timeline

Gantt-style horizontal bar chart.

- Each item with a `start_date` and/or `due_date` is rendered as a bar spanning its date range.
- **View modes:** Week, Month, Quarter — toggle in the toolbar.
- **Dependency overlay:** items blocked by another item are marked with an indicator derived from the dependency graph.
- Scroll horizontally to see longer projects. Click a bar to open the detail drawer.

---

## Dashboard

Read-only project statistics (accessible from the sidebar as **Overview**).

- Throughput chart: items completed per time period.
- Status breakdown: item counts per workflow column with colour-coded category bars.
- Priority breakdown: counts by priority level.
- All statistics are computed live from the item set — no aggregation job.

---

## Sprints

Sprint lifecycle management (accessible from the sidebar).

Sprints move through four states:

```
Planning → Active → Review → Closed
```

- Only one sprint can be Active at a time.
- Items can be assigned to sprints in Planning or Active state only.
- Create with **+ New Sprint**, then use the status buttons to advance the lifecycle.
- Sprint progress (items done vs. total) appears on the Dashboard while a sprint is Active.
