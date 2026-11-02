# Working with Items

This page is for anyone who works on individual tasks. After reading it you can open a task, edit its header and details, say what "done" means for it in a form an agent can check, make it repeat, give it a role, and use its tabs for runs, dependencies, files and comments.

Every piece of work in Tack — a task, bug, feature, building, work order, or whatever your project's vocabulary calls it — is an *item*. You inspect and edit an item through the **item detail drawer**: an editable header with the item's **Status**, a **Now** panel that says what the agents are doing and what to do next, and tabs for Overview, Execution, Activity, Dependencies, Files and Fields.

---

## Opening an item

The drawer opens whenever the address carries `?item=<id>`. You open it by:

- Clicking a card on the Board, a row in the List, or a search result.
- Opening a link such as `http://localhost:3210/projects/<project>/board?item=<id>`, or running `tack open <item-id>`, which prints that link.

Because the open item is part of the address, an item link can be shared — paste it to a teammate and the same drawer opens for them. Press `Esc` or use the close control to dismiss the drawer; focus returns to where you were.

At the top of the drawer, the **header** shows:

- A **type badge** (labeled with your project's vocabulary, e.g. "Task" or "Work Order") and the item's **short id** — the first six characters of the id, uppercased.
- The editable **title**. Edit it in place; the change is saved when you leave the field or press `Enter`.
- The item's **Status**: its current status, with the colour dot its board column has and its name beside it. The **Change status** button next to it opens a short menu of the project's statuses in the workflow's order, the current one marked **Current**; any status can be picked from any other, with the arrow keys or the mouse, and `Esc` closes the menu and returns to the button. The status is yours to set: a run never changes it. The transition is validated by the project workflow (allowed transitions and WIP limits are enforced server-side); if it is rejected the item reverts and an error toast appears.
- The date the item was created.
- Under the status, **Part of:** with a link to the item this one belongs under, and **Sub-items:** with a link and the status of each item that belongs under this one, so a parent made from a plan lists the items it was split into.

Under the header, the **Now** panel comes first. It shows the item's latest run as one state, with an icon and a word (Queued, Running, Needs you, Ready for review, Finished, Failed or Cancelled), one sentence about it, a bar for the tokens it has used against its limit, and the one filled button for what comes next: **Run with agent** for an item that never ran (with a line under it saying which agent and team would take it), **Answer the question** when the agent is waiting for you, **Review and merge** when the run is ready, **See why it failed** when it failed, and **Run again** once it is finished. Stopping a run and following it live are quieter buttons beside it, and starting another run is always one press away. At the foot of the panel, the **Automation** fold says in one muted line what happens to the item without you; opened, it holds the full line, its help and **Who does it** (see [Roles and the Automation row](#roles-and-the-automation-row)). On the Overview tab, **For the agent** and **GitHub issue** are folds too.

Every part of the drawer that is history, or that you rarely change, is a fold: a header with a chevron that you press to open, and that says in one line what is inside while it is shut ("Details · High · Ana", "Automation · By hand: this project doesn't start tasks by itself", "GitHub issue · no GitHub issue linked"). The folds are **Details**, **Automation**, **For the agent** and **GitHub issue** on the item, and **Steps**, **Activity**, **Files from this run**, **Usage** and **Technical** on a run. What needs a decision stays open: the status, the Now panel and the Done when list. A fold whose content needs you says so in its header ("Automation · needs you: the role Content has no profile") and opens by itself. The browser remembers whether you opened or shut each kind of fold, and your choice wins over the default.

Below the tabs, the **Details** fold holds the properties: **Priority**, **Due date**, **Assignee**, **Estimate**, **Sprint**, **Repeats**, the **last update**, **Tags** (type a tag and press `Enter` to add; click the `×` on a tag to remove it) and what the item **blocks**, each with its name on the left and its value on the right. Shut, the fold still says the priority, the assignee and the due date. Every one but the last update and what the item blocks can be edited in place.

The **⋯** button at the top right of the header holds actions that are not what to do next. **Propose items from a file** is there: choose a `tack-plan.json` and the proposal opens for you to accept; see [Reviewing a Run](reviewing-runs.md).

All edits save at once.

### The item's page

**Expand** in the drawer's top bar opens the same item on a page of its own, at `/projects/<project>/items/<id>`, and closes the drawer. The page has a breadcrumb (Projects, the project, Items, the item), the Now panel and the tabs in a main column, and the properties list in a column at the side, with no Details fold. **Copy link** at the top right copies the page's address; the open tab is part of it, so a link opens on the same tab. A tab shows a number beside its name when it is already known, as Dependencies does.

<img src="../screenshots/item-page.png" width="98%" alt="An item on its page: the breadcrumb Projects, Product Launch, Items and the item's short id, Copy link at the top right, the title and its status, the tabs Overview, Activity, Execution, Dependencies, Files and Fields over the description and the Done when list, and a side column with the last run's card (Accept, Reject… and See what it did), Run again and the properties list.">

---

## Overview tab

The Overview tab is the task itself, top to bottom: its **description**, its **Done when** list, what the agent gets beyond
it (**For the agent**), and a linked **GitHub issue**.

The description is edited in place: it reads as text, and clicking it (or **Click to edit**,
when it is empty) opens the rich-text editor; **Done** closes it. Edits **autosave** a moment (about half a second) after you stop typing. There is no save button.

Assignee, priority, estimate, sprint, due date and tags are in the Details fold (see [Opening an item](#opening-an-item)), so they are one press away whichever tab is open.

**GitHub issue.** The **GitHub issue** fold links the task to an issue. Type `owner/repo#42` and press **Link**; **Unlink**
removes the link. A linked task can open its pull request and, with a GitHub token on the board, stays in
step with the issue; see [Keeping GitHub issues in sync](import-export.md#keeping-github-issues-in-sync).

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

An item holds up to 50 criteria and 100 constraints. If a save is refused, the reason
appears next to the part it concerns.

---

## Fields tab (custom fields)

Custom fields capture project-specific data that the built-in fields don't cover — a vendor name, a budget figure, a compliance flag, and so on. They are **defined per project** (in **Settings → Fields**, or with `tack field create`); the Fields tab shows every field defined for the item's project, plus its [roles](#roles-and-the-automation-row).

Set a value by typing or selecting in the control next to the field name. Each change is saved as you commit it; clearing a text field removes its value. A value that does not fit the field's type is refused with an error message and not stored.

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

A field definition may also carry extra **validation rules** on top of the type: a pattern and a minimum and maximum length for text, a minimum and maximum for numbers, and a maximum number of choices for multi-select. They are set through the API or a [project file](import-export.md#the-project-file); a value that breaks one is refused with a message saying why.

> A project with no custom fields shows "No custom fields" and a pointer to project settings.

---

## Repeating a task

A task that happens every week, month or quarter can repeat by itself. Set **Repeats** in the task's properties (the Details fold, or the side column on the item's page) to **Weekly**, **Monthly** or **Quarterly**; **Never** turns it off. A task that repeats carries a **Repeats** badge on the board and in the list.

You can also set it when you create a task from the command line:

```sh
tack add "Pay the rent" -p <PROJECT_ID> --recurrence monthly
```

In an outline or CSV import, the key is `recurrence`; see [Import, Export & Project Files](import-export.md).

**Closing the task makes the next one, once.** When the task moves into a Done status from one that is not Done, Tack creates the next task, unless a task made from it is still unfinished. That is what "once" means: closing, reopening and closing again does not pile up copies.

The next task:

- starts in the first To do column;
- is due at the end of the next period, counted from the closed task's due date, or from today when it had none: a week later for weekly, the last day of the next month for monthly, and the last day of the quarter after the one the date falls in for quarterly;
- copies the title, description, type, parent, priority, estimate, tags, roles and the repeat setting;
- does not copy the sprint or the assignee.

The closed task's activity says "Created the next one, due 2026-11-30" with the new date. In a project that [starts tasks by itself](automation.md#start), a repeating task can run on its own each period.

---

## Roles and the Automation row

A **role** says what kind of work a task is for, such as Content or Developer. A project's roles are created under **Settings → Roles**, and each role can be given an agent profile in [Automation](automation.md#teams). On a task, the **Fields** tab lists the project's roles with a checkbox for each; ticking one gives the task that role. A task with no role is never started by itself, but you can always run it by hand.

The **Automation** line at the foot of the Now panel says in one line what happens to this task without you, for example "Starts by itself when it reaches To Do · Team saas-content · Merge L3" or "By hand: this project doesn't start tasks by itself". After Tack acts on a task, badges say so on the board and the list. Both are explained in [What Tack shows when it acts](automation.md#what-tack-shows-when-it-acts).

---

## Execution tab

The record of every time an agent was handed this item. Its tab carries the latest run's state as a small icon. The **Now** panel, under the header in the drawer and on the item's page, shows the state of the latest run and the button for what comes next:

- **Running** shows the team's steps when it reports them, the agent's last steps, the tokens it has used against its limit, **Follow live**
  and **Stop**.
- **Ready for review** shows the opening of the agent's report, how long it took, how many steps
  it took and how many files it changed, with **Review and merge** as the filled button, which opens the
  Execution tab. When the run left no merge-readiness pack, the panel also holds **Accept** and **Reject…**
  (which asks for an optional reason); when it left one, the verdict and its reason are given only on the
  Execution tab, together with the pack's review. A run that changed no file shows the agent's answer as what it
  delivered and offers **Accept**, not **Merge**.
- **Needs you** puts the question first, with its answers already open: choose one and press
  **Resolve** without leaving the panel, or press **Answer the question**. A question the agent asks after the page was opened appears
  by itself.
- **Failed** says why it stopped, with **See why it failed** as the filled button. A run that is finished or merged says when, and **Run again** is the filled button. **Queued** says why the run still waits
  (for example, that the computer it asked for has not answered) and offers **Run on another
  computer**.

**Merge** shows whenever a run could be merged. When it cannot run it stays disabled and says why
in plain words, such as "No branch was pushed". The board card carries the same status as a chip.

Everything that waits on you is in one place. **Waiting for you** is always in the sidebar, and the top
bar shows the same count; with nothing to do it reads 0, and hovering it says so. The count covers every project, and under its name the sidebar spells it out in the same words as the board's **Work** row ("All projects: 1 needs you, 1 ready for review"), so a project's row can read 0 **Needs you** while the sidebar counts a run ready for review. Press either to open the
list, in three groups: **Questions to answer**, **Ready for review**, and **Failed since you last looked**
(runs that ended in failure after the last time you opened the list in this browser). Each row has the
run's state, the item's title, its project and how long ago it happened, and one button for what to do
next: **Answer the question**, **Review and merge** or **See why it failed**. The button opens that run on
its Execution tab, over the page you are on. The decision token is asked for in the list, once for the
browser session, and the run's own field does not ask again. A card on the board whose run is asking opens
that run the same way.

While a run is going, the item reads itself again each time the run changes state, so whatever changed on it
shows without a reload. It stops once every run has ended.

On the tab, **chips** pick the run: **Run 3**, **Run 2**, **Run 1**, newest first, each with a dot in
the colour of its state, and the latest is shown until you pick another. The address names the run you
picked (`?tab=execution&run=…` on the item's own page), so a run can be linked to and opened again. **Run
again** starts a new run of the same task. A run that had several attempts has a second row of chips for
them. The tab shows only the run you picked and reads it from the top down: the verdict, the question it
waits on, the checks, the changes, the steps, the files and the technical detail. [Reviewing a
Run](reviewing-runs.md#what-happens-when-a-run-finishes) explains each part. An item that was never run shows
an empty tab, not a hidden one.

How to start a run is in [Running Agents on Tasks](running-agents.md), and how to judge one is in
[Reviewing a Run](reviewing-runs.md). The item's [acceptance criteria](#acceptance-criteria-and-for-the-agent)
are what the agent is given as the definition of done.

---

## Dependencies tab

Use this tab to record how an item relates to others in the same project. Two directions are supported:

- **Blocks** — this item must be done before the linked item can proceed.
- **Blocked by** — the linked item must be done before this one can proceed.

The tab lists current **Blocks** and **Blocked by** links. Click a linked item's title to open *its* page, which is a link you can also open in a new tab. Click the `×` to remove a link.

To add a dependency, pick a **direction**, choose the other **item** from the picker, and select **Add**.

Tack's dependency graph is a **DAG (directed acyclic graph)**, so:

- **Cycles are rejected.** If adding a link would create a loop (A blocks B, B blocks A), the server refuses it and the error is shown inline beneath the form.
- **Self-references are rejected** — an item cannot depend on itself. The picker only offers other items in the project.

---

## Activity tab

The Activity tab is the item's **comment timeline**. Each entry shows its author ("Anonymous" when none is recorded; "Tack" for the notes Tack writes when it acts, such as "Started by itself") and how long ago it was written.

To add a comment, type in the box at the bottom and select **Comment**. It appears at once and is taken back with an error message if it could not be saved.

---

## Files tab (attachments)

Attach files to an item from the Files tab:

- **Drag and drop** files onto the drop zone, or **click it to browse**. Multiple files at once are supported.
- The **maximum file size is 50 MB** per file. A larger file is skipped with an error toast and the rest continue uploading.

Each uploaded file is listed with its **size and MIME type**. Click a filename to download it (served with the original filename); use **Delete** to remove it.

Deleting an attachment removes the file as well as its record. Where the server keeps the files is the administrator's choice; see [Configuration](../admin-guide/configuration-reference.md). To upload from a script, see the [API Reference](../developer/api-reference.md).
