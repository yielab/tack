# Workflows

A **workflow** is the set of named status columns that items move through in a project. Each column has:

- **Name** — the label shown on the board (e.g., "In Progress")
- **Category** — `todo`, `in_progress`, or `done`
- **WIP limit** — optional cap on how many items can be in this column at once
- **Order** — left-to-right position on the board

Every project stores exactly one `WorkflowConfig` as JSON — no schema migration needed to change it.

---

## Built-in Project Types

When you create a project, its type determines the starting workflow. Everything can be changed afterwards.

| Type | Style | Default Columns |
|---|---|---|
| `software`, `web`, `mobile` | Scrum | Backlog → To Do → In Progress → In Review → Done |
| `construction` | Phase-based, **strict** | Permit → Procurement → Build → Inspect → Handover |
| `legal` | Phase-based | Intake → Discovery → Drafting → Review → Closed |
| `research` | Kanban | Hypothesis → Design → Experiment → Analysis → Published |
| `event` | Phase-based | Ideas → Booked → In Progress → Confirmed → Done |
| `personal`, `homework` | Simple | To Do → Doing → Done |
| `maintenance` | Kanban | Backlog → In Progress → Done (no sprints) |
| `custom` | Simple | To Do → Doing → Done (fully editable) |

---

## Strict Transitions (Construction Workflow)

Most workflows let you move an item to any column. The construction workflow enforces **strict linear transitions**: items must move through columns in order.

| From | Only allowed next step |
|---|---|
| Permit | Procurement |
| Procurement | Build |
| Build | Inspect |
| Inspect | Handover |

Attempting to move a "Build" item directly to "Handover" is rejected by the API (`422 Unprocessable Entity`). This prevents accidentally skipping required phases — regulatory sign-offs, physical dependencies, multi-party hand-offs.

Enable or disable strict transitions on any workflow from **Settings → Workflow → Strict transitions** toggle.

---

## Changing a Workflow After Creation

**In the UI:** Settings → Workflow → add/remove/rename columns, set WIP limits, toggle strict transitions → Save.

Existing items keep their current status name. If you rename a column, items in the old status are not migrated automatically — update them via the board or the API.

**Via API:**

```sh
curl -X PATCH http://localhost:3210/api/projects/{id} \
  -H "Content-Type: application/json" \
  -d '{
    "workflow": {
      "columns": [
        {"name":"Permit",      "category":"todo",        "wip_limit":null, "order":0},
        {"name":"Procurement", "category":"in_progress", "wip_limit":2,    "order":1},
        {"name":"Build",       "category":"in_progress", "wip_limit":3,    "order":2},
        {"name":"Inspect",     "category":"in_progress", "wip_limit":1,    "order":3},
        {"name":"Handover",    "category":"done",        "wip_limit":null, "order":4}
      ],
      "strict_transitions": true
    }
  }'
```

---

## WIP Limits

A WIP limit caps the number of items in a column. Adding a card beyond the limit is blocked in the UI (the card snaps back) and rejected by the API.

WIP limits are enforced on new moves, not retroactively. If you set a limit of 3 on a column that already has 5 items, the existing 5 are unaffected — but no more can enter until the count drops below 3.

Set limits on bottleneck stages (code review, inspection, QA) to surface overload before it compounds.

---

## Status Categories

Every column belongs to one category:

| Category | Meaning | Side effects |
|---|---|---|
| `todo` | Not started | Items default here on creation |
| `in_progress` | Being worked on | Sets `started_at` on first move in |
| `done` | Complete | Sets `completed_at`; triggers auto-complete check on parent |

Categories drive reporting (cycle time, throughput) and auto-complete logic. Only column names are shown in the UI.

---

## Auto-Complete (Parent Rollup)

When an item moves to a `done` column, Tack checks whether all siblings under the same parent are also done. If they are, the parent auto-moves to its own done column. This cascades up the hierarchy.

**Example:**

1. Epic "User Auth" has three tasks: Register, Login, Logout.
2. Complete Register → epic unchanged (Login and Logout still open).
3. Complete Login → epic unchanged.
4. Complete Logout → all children done → **epic auto-completes**. ✓

Auto-complete is best-effort: errors (e.g., parent has no done column) are silently ignored and do not block the child update.
