# Working with Items

This page is for anyone who works on individual tasks. After reading it you can open a task, edit its header and details, say what "done" means for it in a form an agent can check, make it repeat, give it a role, and use its tabs for runs, dependencies, files and comments.

Every piece of work in Tack — a task, bug, feature, building, work order, or whatever your project's vocabulary calls it — is an *item*. You inspect and edit an item through the **item detail drawer**, a panel with inline header editing and tabs for Overview, Activity, Execution, Dependencies, Files, and Fields.

---

## Opening an item

The drawer opens whenever the `?item=<id>` query parameter is present in the URL. You open it by:

- Clicking a card on the Board, a row in the List or Table, or a search result.
- Navigating directly to a link such as `https://tack.test/board?item=<id>`.

Because the open item lives in the URL, item links are **deep-linkable and shareable** — paste the link to a teammate and the same drawer opens for them. Press `Esc` or use the close control to dismiss the drawer; focus returns to where you were.

At the top of the drawer, the **header** shows:

- A **type badge** (labeled with your project's vocabulary, e.g. "Task" or "Work Order") and the item's **short id** — the first six characters of the id, uppercased.
- The editable **title**. Edit it inline; the change commits when you blur the field or press `Enter`.
- A row of **status pills**, one per workflow status. Click any inactive pill to transition the item to that status. The transition is validated by the project workflow (allowed transitions and WIP limits are enforced server-side); if it is rejected the item reverts and an error toast appears.
- The date the item was created.

Under the header, one **properties** list holds **Priority**, **Due date**, **Assignee**, **Estimate**, **Sprint**, **Repeats**, the **last update**, **Tags** (type a tag and press `Enter` to add; click the `×` on a tag to remove it), the **Automation** row and what the item **blocks**. Every one but the last update can be edited in place.

All edits save immediately (optimistic update: applied locally first, then persisted).

### The item's page

**Expand** in the drawer's top bar opens the same item on a page of its own, at `/projects/<project>/items/<id>`, and closes the drawer. The page has a breadcrumb (Projects, the project, Items, the item), the tabs in a main column and the properties list in a column at the side. **Copy link** at the top right copies the page's address; the open tab is part of it, so a link opens on the same tab. A tab shows a number beside its name when it is already known, as Dependencies does.

---

## Overview tab

The Overview tab (older links and bookmarks that say `?tab=details` still open it) is the task
itself, top to bottom: its **description**, its **Done when** list, what the agent gets beyond
it (**For the agent**), and a linked GitHub issue.

The description is edited in place: it reads as text, and clicking it (or **Click to edit**,
when it is empty) opens the rich-text editor; **Done** closes it. Edits **autosave** — they are
debounced and persisted automatically a short pause (about 0.6 seconds) after you stop typing.
There is no save button.

Core metadata — **assignee, priority, estimate, sprint, due date, and labels (tags)** — lives in the header above the tabs (see [Opening an item](#opening-an-item)), so it is always visible regardless of which tab is active.

<img src="../screenshots/brief.png" width="50%" alt="The Overview tab of an item: the description editor, a Done when checklist with one check and Add check, a collapsed For the agent section summarised as 3 checks, 3 limits and risk noted, and Link GitHub issue.">

---

## Acceptance criteria and For the agent

Together these say what "done" means for the item, in a form a person or a program can check.
The agent receives them with the title and description, and every run keeps a copy.

**Done when** is a checklist: one line per thing a person checks before calling the item
done. Tick a line when you have checked it; the header counts them ("1 of 3"). **Add check**
adds a line, **Up** and **Down** reorder, **Remove** deletes; each line saves when you leave it,
and a tick saves at once. The tick is for people: the agent is not told which lines are ticked,
and no automatic check reads it.

**For the agent** is optional and collapsed by default; its summary line says what it holds
(for example *3 checks · 3 limits · risk noted*). **Save for the agent** writes all three parts:

- **Checks a machine runs.** Each has a title and one of five kinds:
  - **Command** — a shell command and the exit code it must return (0 by default), with an optional working directory.
  - **Test** — a named test, with an optional runner such as `pytest`.
  - **Metric** — a named measurement compared to a threshold: at most, at least, or exactly, with an optional unit.
  - **File exists** — a path that must be present.
  - **File is absent** — a path that must not be present.

  Prefer a check a machine runs over a line in the checklist: it costs nobody's time.
- **Constraints** limit how the work is done: a forbidden path, an allowed dependency, a
  maximum number of changed files, or a free-form note.
- **Risk** is low, medium or high. Leave it *Not set* if you are not sure.

The definition of done in plain sentences is set once per project, in
[Automation](automation.md#automation) — not on each task.

An item holds up to 50 criteria and 100 constraints. If the server rejects a save, the reason
appears next to the part it concerns.

---

## Fields tab (custom fields)

Custom fields capture project-specific data that the built-in fields don't cover — a vendor name, a budget figure, a compliance flag, and so on. They are **defined per project** (in project settings); the Fields tab shows every field defined for the item's project, plus role assignment.

Set a value by typing or selecting in the control next to the field name. Each change is saved as you commit it; clearing a text-like field removes its value. **Values are validated on save** against the field's type — an invalid value is rejected and shown as an error toast, not stored.

### Field types

| Type | Accepts | Example |
|------|---------|---------|
| Text | Any string | `Acme Corp` |
| Long text | Any string (multi-line textarea) | `Multi-paragraph notes…` |
| Email | A string | `info@yielab.com` |
| URL | A string starting with `http://` or `https://` | `https://example.com/spec` |
| Number | A numeric value | `42` |
| Boolean | A checkbox (true/false) | `true` |
| Date | An ISO 8601 date — `YYYY-MM-DD` or RFC 3339 | `2026-06-30` |
| Select | One value chosen from the field's defined options | `In review` |
| Multi-select | An array of values, each from the field's options | `["frontend","urgent"]` |

A field definition may also carry extra **validation rules** that apply on top of the type check: a regex `pattern` and `min_length`/`max_length` for strings, `min`/`max` for numbers, and `max_items` for multi-select. Values that violate these rules are rejected with a descriptive message.

> If a project has no custom fields, the tab shows "No custom fields — define fields in project settings."

---

## Repeating a task

A task that happens every week, month or quarter can repeat by itself. Set **Repeats** in the task's header to **Weekly**, **Monthly** or **Quarterly**; **Never** turns it off. A task that repeats carries a **Repeats** badge on the board and in the list.

You can also set it when you create a task from the command line, or in a file you import:

```sh
tack add "Pay the rent" -p <PROJECT_ID> --recurrence monthly
```

In an outline or CSV import, the column is `recurrence` (CSV also accepts `repeats`). A value that is not weekly, monthly or quarterly is reported as a warning and the task does not repeat; see [Import, Export & Project Files](import-export.md).

**Closing the task makes the next one, once.** When the task moves into a Done status from one that is not Done, Tack creates the next task, unless a task made from it is still unfinished. That is what "once" means: closing, reopening and closing again does not pile up copies.

The next task:

- starts in the first To do column;
- is due at the end of the next period, counted from the closed task's due date, or from today when it had none: a week later for weekly, the last day of the next month for monthly, and the last day of the quarter after the one the date falls in for quarterly;
- copies the title, description, type, parent, priority, estimate, tags, roles and the repeat setting;
- does not copy the sprint or the assignee.

The closed task's activity says "Created the next one, due 2026-11-30" with the new date. See [Repeating a task](automation.md#repeating-a-task) for how this works with a project that starts tasks by itself.

---

## Roles and the Automation row

A **role** says what kind of work a task is for, such as Content or Developer. A project's roles are created under **Settings → Roles**, and each role can be given an agent profile in [Automation](automation.md#teams). On a task, the **Fields** tab lists the project's roles with a checkbox for each; ticking one gives the task that role. A task with no role is never started by itself, but you can always run it by hand.

Between the **Run with agent** button and the tabs, the drawer has an **Automation** row. It says in one line what happens to this task without you:

| The row says | What it means |
|---|---|
| "Starts by itself when it reaches To Do · Team fab-content · Merge L3" | The project starts tasks by itself from that status. The team and merge level are shown when they are known |
| "By hand: this project doesn't start tasks by itself" | Nothing starts it. Press **Run with agent** |
| "By hand: the role Content has no profile" | The task's role has no profile and the project has no default. Choose one in [Teams](automation.md#teams) |

After Tack acts on a task, badges say so on the board, list and table: **Started by itself**, **Team: name** and **Merged by itself**. They are explained in [What Tack shows when it acts](automation.md#what-tack-shows-when-it-acts).

---

## Execution tab

The record of every time an agent was handed this item. The **Last run** card, at the top of the
drawer and in the side column of the item's page, takes the colour and the buttons of the run:

- **Running** shows the agent's last steps, the tokens it has used against its limit, **Follow live**
  and **Stop**.
- **Needs your review** shows the opening of the agent's report, how long it took, how many steps
  it took and how many files it changed, with **Accept**, **Reject…** (which asks for an optional
  reason) and **See what it did**. A run that changed no file shows the agent's answer as what it
  delivered and offers **Accept**, not **Merge**.
- **Failed** says why it stopped. **Merged** says when. **Queued** says why the run still waits
  (for example, that the computer it asked for has not answered) and offers **Run on another
  computer**.

**Merge** shows whenever a run could be merged. When it cannot run it stays disabled and says why
in plain words, such as "No branch was pushed". The board card carries the same status as a chip.

On the tab, each run (**Run 1**, **Run 2**, …) shows one status — *Running*, *Finished — needs
your review*, *Finished — no changes recorded* (a run that was not meant to change files), *Failed*
(including a run that was meant to change files and changed nothing), *Accepted*, and so on — with the agent that ran,
what it did and said, and its usage, tokens first. A finished run that changed files waits for you:
**Accept** or **Reject**, with an optional note, and the card's *Needs your review* flag clears.
When the run's branch became a pull request, a **PR #n** link and its state sit on the run.
**Show timeline, questions & files** opens the timeline, the questions the agent asked, the files
from the run and, when a verifier ran, the merge-readiness pack. While a docket run is going, its
timeline fills as the agent works: each call to the model, each tool it ran (a shell command is
shown as the command) and each call docket refused. A question that is still waiting appears on
the same tab, with the command it is about, why the agent's policy stopped it, the options, their
risks and the agent's recommendation. How to
start a run is in [Running Agents on Tasks](running-agents.md), and how to judge one is in
[Reviewing a Run](reviewing-runs.md). The item's
[acceptance criteria](#acceptance-criteria-and-for-the-agent) are what the agent is given as the
definition of done. An item that was never run shows an empty tab, not a hidden one.

---

## Dependencies tab

Use this tab to record how an item relates to others in the same project. Two directions are supported:

- **Blocks** — this item must be done before the linked item can proceed.
- **Blocked by** — the linked item must be done before this one can proceed.

The tab lists current **Blocks** and **Blocked by** links. Click a linked item's title to open *its* drawer. Click the `×` to remove a link.

To add a dependency, pick a **direction**, choose the other **item** from the picker, and select **Add**.

Tack's dependency graph is a **DAG (directed acyclic graph)**, so:

- **Cycles are rejected.** If adding a link would create a loop (A blocks B, B blocks A), the server refuses it and the error is shown inline beneath the form.
- **Self-references are rejected** — an item cannot depend on itself. The picker only offers other items in the project.

---

## Activity tab

The Activity tab is the item's **comment timeline**. Comments are listed oldest-first, each showing the author (or "Anonymous" when none is recorded) and a relative timestamp (`just now`, `5m ago`, `3h ago`, `2d ago`, then a calendar date for older entries).

To add a comment, type in the box at the bottom and select **Comment**. Posting is optimistic — your comment appears immediately and is rolled back with an error toast if the server rejects it.

---

## Files tab (attachments)

Attach files to an item from the Files tab:

- **Drag and drop** files onto the drop zone, or **click it to browse**. Multiple files at once are supported.
- The **maximum file size is 50 MB** per file. A larger file is skipped with an error toast and the rest continue uploading.

Each uploaded file is listed with its **size and MIME type**. Click a filename to download it (served with the original filename); use **Delete** to remove it.

Deleting an attachment removes the file as well as its record. Where the server keeps the files is the administrator's choice; see [Configuration](../admin-guide/configuration.md). To upload from a script, see the [API Reference](../developer/api-reference.md).

---

## Comments

Comments live in the **Activity tab** (see above) — that tab is the item's discussion thread. Open the item, switch to **Activity**, write in the comment box, and select **Comment** to post.
