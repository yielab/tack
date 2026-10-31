# Workflows & Statuses

A **workflow** is the set of named status columns that items move through in a project. Each column has:

- **Name** — the label shown on the board (e.g., "In Progress")
- **Category** — `todo`, `in_progress`, or `done`
- **WIP limit** — optional cap on how many items can be in this column at once
- **Order** — left-to-right position on the board

Every project has exactly one workflow, and you can change it at any time. A workflow can also restrict which moves are allowed; see [Strict transitions](#strict-transitions).

---

## Built-in Project Types

When you create a project, its type determines the starting workflow. Everything can be changed afterwards.

| Type | Style | Default Columns |
|---|---|---|
| `software`, `web`, `mobile` | Scrum | Backlog → To Do → In Progress → In Review → Done |
| `construction` | Phase-based, **strict** | Permit → Procurement → Build → Inspect → Handover |
| `legal` | Phase-based, **strict** | Intake → Discovery → Drafting → Review → Closed |
| `research` | Kanban | Hypothesis → Design → Experiment → Analysis → Published |
| `event` | Phase-based | Ideas → Booked → In Progress → Confirmed → Done |
| `personal`, `homework` | Simple | To Do → Doing → Done |
| `maintenance` | Kanban | Queue → In Progress → Review → Done |
| `custom` | Simple | To Do → Doing → Done (fully editable) |

---

## Strict transitions

Most workflows let you move an item to any column. A workflow can instead list the moves it allows. The construction and legal types start that way, and a move that is not listed is refused with an error that names both statuses.

The construction workflow allows:

| From | Allowed next step |
|---|---|
| Permit | Procurement |
| Procurement | Build |
| Build | Inspect |
| Inspect | Handover, or back to Build for rework |

So a "Build" item cannot jump to "Handover". This keeps required phases, such as regulatory sign-offs and physical dependencies, from being skipped. The legal workflow is the same kind of chain, with a step from Review back to Drafting.

The Workflow settings page edits columns, categories and WIP limits, and it keeps whatever moves the workflow already allows. To set or remove the allowed moves, put `transitions` in a [project file](import-export.md#the-project-file) or send them through the API, as below.

---

## Changing a workflow after creation

**In the UI.** **Settings → Workflow**: rename columns, change their category, set WIP limits, **Add Status**, then **Save workflow**.

Existing items keep their current status name. If you rename a column, items in the old status are not moved automatically; move them on the board.

**In a project file.** The `project.workflow` key takes a preset (`scrum`, `kanban`, `simple`, `construction`, `legal`, `research` or `event`) or the statuses in order, with optional `transitions`. A workflow in a file must have at least one `todo`, one `in_progress` and one `done` status and no repeated names, and the file applies it only when it creates a project.

**Through the API.**

```sh
curl -X PATCH http://localhost:3210/api/projects/{id} \
  -H "Content-Type: application/json" \
  -d '{
    "workflow": {
      "workflow_type": "construction",
      "statuses": [
        {"name":"Permit",      "category":"todo",        "wip_limit":null, "order":0},
        {"name":"Procurement", "category":"todo",        "wip_limit":null, "order":1},
        {"name":"Build",       "category":"in_progress", "wip_limit":3,    "order":2},
        {"name":"Inspect",     "category":"in_progress", "wip_limit":1,    "order":3},
        {"name":"Handover",    "category":"done",        "wip_limit":null, "order":4}
      ],
      "transitions": [
        {"from":"Permit","to":"Procurement"},
        {"from":"Procurement","to":"Build"},
        {"from":"Build","to":"Inspect"},
        {"from":"Inspect","to":"Handover"},
        {"from":"Inspect","to":"Build"}
      ]
    }
  }'
```

Leave out `transitions` (or send `null`) to allow every move.

---

## WIP Limits

A WIP limit caps the number of items in a column. A move into a full column is refused (the card snaps back in the UI), and the API answers with an error that names the column and its limit.

WIP limits are enforced on new moves, not retroactively. If you set a limit of 3 on a column that already has 5 items, the existing 5 are unaffected — but no more can enter until the count drops below 3.

Set limits on bottleneck stages (code review, inspection, QA) to surface overload before it compounds.

---

## Status Categories

Every column belongs to one category:

| Category | Meaning | Side effects |
|---|---|---|
| `todo` | Not started | New items start in the first one |
| `in_progress` | Being worked on | The item's start date is set the first time it moves in |
| `done` | Complete | The completion date is set, and the parent is checked (see below) |

Categories are what the Overview, [starting tasks by itself](automation.md#start) and the parent rollup read. The board shows only column names.

---

## Auto-Complete (Parent Rollup)

When an item moves to a `done` column, Tack checks whether all siblings under the same parent are also done. If they are, the parent auto-moves to its own done column. This cascades up the hierarchy.

**Example:**

1. Epic "User Auth" has three tasks: Register, Login, Logout.
2. Complete Register → epic unchanged (Login and Logout still open).
3. Complete Login → epic unchanged.
4. Complete Logout → all children done → **epic auto-completes**. ✓

If the parent cannot move, for example because its workflow has no Done column, it stays where it is and the child's move still succeeds.
