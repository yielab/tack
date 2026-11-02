# Reviewing a Run

This page is for whoever checks what an agent did. After reading it you can find a finished run, judge its
changes, accept or reject them, read the merge-readiness pack a verifier leaves, see what the run used and
cost, and read the project's Factory metrics page.

## What happens when a run finishes

A run that left changes marks the task **Needs your review**. This is a flag on the task, separate from your
board columns. The board and the list show it. It clears when you press **Accept** or **Reject** on the
attempt. The task's status does not change: its column is yours, and you move the card when you want to.

Open the task and go to its **Execution** tab. Chips pick the run, each with its state; the run you picked
reads from the top down, in the same order every time: the verdict first, then why, then the raw detail.

1. **The verdict.** The run's state, the verifier's recommendation and risk as two chips when a verifier
   ran, the tokens it used against its limit, how long it took, and whether it was merged ("Not merged: no
   branch was pushed", or "Works in a folder: nothing to merge" for a team that has no repository). One
   button is filled, for what to do next: **Accept** while the run waits for your verdict (when the verifier
   left a pack, a reason is required and is saved with the verdict on the checks; without a pack,
   **Reject…** asks for a note, which is optional), **Merge** once it is accepted, **Run again** when nothing else is left.
   Under it, **Why it stopped** for a run that did not finish and **What the agent concluded**, the agent's
   report as written.
2. **Needs you**, only while the agent waits for an answer: the question, the options and the decision
   token field. A run that asked nothing has no such field.
3. **Checks.** The criteria as a checklist: ✓ passed, ✗ failed, ◐ needs a person, with the kind of check
   and its evidence beneath. The task's **Done when** ticks and the verifier's criteria are one list, each
   saying where it comes from. The verify command and its output are folded.
4. **Changes.** How many files changed and how many lines were added and removed. **Open diff** shows what
   changed; **Open folder** (or **Copy path**, when you are not in the desktop app) shows where the work is
   on this computer; **Copy branch** copies the branch name, when the agent worked on a branch.
5. **Steps.** The team's steps as a track, one node for each with its role, model and verdict. Under it,
   **Activity** (folded) has **What the agent did**, by tool or as a timeline, and the questions the agent
   asked.
6. **Files from this run**, one row for each with **View** and **Download**.
7. **Technical** (folded): the request, attempt and runner ids, what the agent got and read, the usage in
   full, the **Raw log** and whether the verifier signed its pack.

**Succeeded** is green only when the changes were captured. It is amber when a run that was not meant to
change files changed nothing, or when the changes could not be read. A run is meant to change files when its
tools include one that writes, or, when its tools are all tools, unless its profile is a Reviewer, Researcher
or Planner. (A Planner's or Researcher's write tool is for the plan file, so it does not count.) A run that was meant to change files and changed nothing is shown as **Failed**, with the reason
"The agent reported success but delivered no change to review", and the number of files the agent left
uncommitted when there were any. A failed run leads with why it failed.

A team that proposes items and ends with proposals Tack cannot use (for example, one names a parent that is not an
earlier row in the file) is shown as **Finished — proposals could not be used**, not as a plain success, with the
reason in words and docket's own line on hover.

### What you see on the board

After a run, the task's card on the board carries these marks. The list carries the merge and automation
badges and shows the run's state in its own column.

| Badge | Meaning |
|---|---|
| A state chip on the card's second line (Queued, Running, Needs you, Ready for review, Done, Failed, Cancelled) | Where the latest run is. Press it to open the task's Execution tab; for a run that needs you, it opens that run with its question open. A run that needs you or failed also puts a bar on the card's left edge |
| **Ready for review** | The latest run left changes and nobody has accepted or rejected them |
| **Waiting for CI to merge**, **Not merged: …** | Tack tried to merge the change and could not. Press the badge for the reason and what to do |
| **Started by itself** | The project started the run, not a person |
| The team's name, with its initials | The team the run's profile named |
| **Merged by itself** | Tack merged the change automatically |
| **Repeats** | The task is set to repeat when closed |

The badges that start with Started, Team and Merged are explained in
[Automating a Project](automation.md#what-tack-shows-when-it-acts).

<a id="when-a-run-proposes-items"></a>

### When an agent proposes changes

An agent does not edit your board on its own. What it wants to change, it proposes as a **change set**: a
list of changes with the reason it gives, where it came from (a run, a thread or a call to a tool) and who
proposed it, by the name of its credential. A change can be a new item, who does an item, its priority, a
move to another status, a dependency, a comment or a due date. How many of them apply by themselves is
your choice, under [Agents on the board](#how-far-an-agent-may-go).

#### One panel for every change set

Every change set opens in the same panel, whoever proposed it. New items are a tree, with one row per item
and a line that counts them: items, sub-items and dependencies. An item that belongs under another is nested
beneath it, and it is unticked when its parent is unticked. Every other change is a line in a list under the
tree, with the item it is about named by its title. Each row has a checkbox, and each can be edited:

- An item's title, description and acceptance criteria, its **Type** and **Role**, and **Who does it**, which
  names the team that will do it once it exists. **Waits on** lists the titles of the items it must wait for,
  each with a × to remove it, and a menu to add another.
- A change of who does an item, its role and team; a priority; the status to move to; the text of a
  comment; a due date. A dependency has nothing to edit: untick it to leave it out.

**Select all** and **Select none** tick every change that still waits. Then choose:

- **Apply 3 changes** applies the ticked ones, with your edits. **Apply all** applies everything that waits.
- **Reject…** asks why and turns down what still waits. The proposer can read the reason.
- **Save edits** keeps your edits without applying, to come back to them.

A change set applies as one: if the board cannot take it, nothing is applied, and the panel lists every
reason, one line each ("operation 2: the status Ready is full"). Fix what the reasons name, or untick the
changes that cause them, and apply again. After it applies, the panel says what it could not fill: a role or
a type the project does not have, or a team that no single profile names. Such an item is created without
that part. A change set that applied only in part says **Partly applied**, and the rest still waits.

#### Where they wait for you

Change sets that wait appear in **Waiting for you** in the sidebar, under **Changes to apply**, and in its
count. **Review the changes** opens the project's **Change sets** page, which lists them by status:
waiting for you, applied, turned down, or all. A decided change set folds into one line. When agents may
only read in a project, and nothing is waiting, the **Change sets** entry in the sidebar is greyed out and
says why.

#### A run that proposes items

A run with the **Planner** profile changes no project files and can end with a list of items it proposes.
The Planner and the Researcher write one file only, the plan file `tack-plan.json`. A run of either that
also changes any other file ends **Failed** with the reason "This run was meant only to write a plan, but it
also changed:" and the names of the files, so a change nobody asked for is never accepted by default; the
changes are still there to look at. The attempt shows **The Planner proposes N items**, in the panel
described above, and nothing is created until you press the button, which says how many it will create
(**Create 3 items**).

To propose items from a file, open the item and choose **Propose items from a file** in its **⋯** menu.
Choose a `tack-plan.json`, or drop it on the section. Tack checks it and names the first entry that does not
fit, and why. A file that fits opens the same list.

After you create the items, they appear as links. Under them, Tack says what it could not fill: a role or a
type the project does not have, or a team that no single profile names. Such an item is created without that
part.

#### How far an agent may go

In **Settings → Automation**, the **Agents** section has the rung **Agents on the board**. It has four
levels, and the one in use says **Set here**:

| Level | What an agent may do |
|---|---|
| **Read** | Read the board and talk. It changes nothing, and proposes nothing. |
| **Propose** | Every change waits for you to apply it. This is the level a project starts at. |
| **Act** | Everything but new items applies by itself: who does an item, priority, moves, dependencies, comments and due dates. New items wait for you. |
| **Create** | New items apply by themselves too, up to two limits you set: how many in one change set, and how many in a day (UTC). Both are 0 until you set them, so Create is off by default and behaves like Act until you raise them. |

The stricter of the rung and the agent's own credential wins: a credential that may only propose never goes
beyond Propose, whatever the rung says. See [Agent credentials](../admin-guide/administration.md#agent-credentials).

### Merging a reviewed run

When the run's branch was pushed and its pull request is open, press **Merge** on the run. See
[Merging from Tack](merging.md).

---

## Reviewing a merge-readiness pack

When a verifier checks an attempt, it leaves a merge-readiness pack. Its parts are spread through the run,
each where it is read: the recommendation and risk in the verdict, the criteria and the rest of what the
verifier ran under **Checks**, and whether it was signed under **Technical**.

<img src="../screenshots/merge-readiness.png" width="50%" alt="A merge-readiness pack on a succeeded attempt: Recommendation merge, Risk low, three reasons, a criteria table with every criterion passed and its evidence file, the verify command with exit code 0, mutation score, static analysis counts, a blind judge's verdicts, and Your review with a required reason before Accept or Reject.">

It shows:

1. The verifier's **recommendation** and the **risk** level, in the verdict; the risk's reasons are under
   **Checks**.
2. Whether the change was merged and by whom: "Merged by you", or "Merged by Tack" with the level and
   domain, or "Not merged:" with the reason, also in the verdict.
3. Under **Checks**, one line for each criterion from the task's brief, with its status and evidence, joined
   to the task's **Done when** list.
4. **Next steps**, when the verifier lists what to fix.
5. **Verify**: the check command with its exit code and output, folded.
6. **What else the verifier ran**, folded: **Mutation** and **Static analysis**, what the verifier
   measured, and **Judge**, a verdict on each criterion, where a judge ran.
7. **Your review**.

A section the verifier did not run says **Not run**. That is not a pass: nothing was checked.

To record your verdict, write a reason, then choose **Accept** or **Reject**. Both stay disabled until the
reason has text, and the line "Write a reason to enable Accept and Reject" says so. The first review is
final; a second one is refused. While the run waits for your verdict, this is the one block at the top of
the run, and the one reason is saved with the run's verdict, whether you accept or reject (which clears
**Needs your review**), and with the pack. Once it is given, **Your review** shows it here.

If the pack's review is saved but the run's verdict cannot be (for instance, the connection dropped between
the two), the block does not ask for a reason again and does not offer **Reject**: it says the pack was
accepted or rejected and offers **Save the verdict on the run**, which gives the run the pack's verdict and
reason. If saving fails again, the block shows why, and you can press it again.

When the verifier rejects a change, a project can send it back to the agent with the list of what to fix;
see [Sending a rejected change back to the agent](merging.md#sending-a-rejected-change-back-to-the-agent).

---

## Usage and cost

The verdict gives the tokens a finished attempt counted, as a bar against its limit when it has one, and how
long it ran. The full **Usage** panel is under **Technical**. It leads with tokens, because that is what the agent measured:

- **The tokens counted**, the figure every limit uses, with the tokens in and out beside it, and the part
  of the input that was read from the agent's cache (not counted) or written to it. Hover a figure for the
  exact count.
- **The models** the run used, and, where it matters, whether the agent named the model that answered or
  Tack only knows the one you requested.
- **Model calls** and how long the run took.

The panel shows no dollar figure: limits and reports are in tokens. The factory metrics, below, price tokens
from your own rate card.

If an agent does not report tokens, the panel says **Tokens: Not measured**. The task's summary of its latest
run gives the counted tokens, with what the cache served beside them, and the run's token limit with who
applies it; see [Token limits](running-agents.md#token-limits).

The Factory metrics page, below, adds up cost for a whole project in a different way: it prices the tokens of
every run with a rate card you enter once.

---

## Factory metrics

**Factory metrics** is in a project's sidebar. It summarises all the
runs, questions and verification in a project, as one card per measure. The page covers all time. Each card
shows a figure, or **Not measured** with its reason when there is nothing to measure.

| Card | What it tells you |
|---|---|
| **Escalation rate** | How often a run needed you: questions asked divided by runs |
| **Human minutes per decision** | The median and the slowest tenth of the time a question waited for your answer, counted from when someone first opened it (or from when it was asked, if nobody opened it first) |
| **Decided alone** | Runs in which the agent's own policy settled at least one question without asking you, divided by runs; they are not counted in the escalation rate or the human minutes |
| **Human minutes per pack review** | The same wait for a merge-readiness pack and its verdict |
| **Pack acceptance rate** | Accepted packs divided by reviewed packs, with how many are still unreviewed |
| **Verification tax (tokens)** | Tokens spent on verification and rework, divided by tokens spent on first attempts |
| **Outcomes** | Pull requests opened, merged, closed and reverted, and how many were merged and not reverted |
| **Cost** | An estimate in dollars for the project: tokens priced by the rate card, plus the human minutes above at the card's hour rate, with how many of the models used have a price |
| **Quality** | The figures the verifier reports for each repository. Tack shows them and does not compute them. Where the verifier cannot answer, the card says why |

When a card has nothing to count, it says **Not measured** and why, never `0`.

### Cost by model and by team

Under the cards, **Cost by model** lists, for each model, the counted tokens in and out, the tokens the
cache served (shown, not counted), the number of attempts and the estimated cost. A run that named no model and whose agent reported none appears as **No model named**. **By team**
appears when at least one run went to a team, and groups the same figures by team.

A row shows **not priced** in its Cost column when there is no rate card, or when the card does not list
that model. The **Cost** card then says how many attempts were left out.

### The rate card

The **Rate card** form at the bottom of the page tells Tack what things cost. It applies to the whole board,
not to one project.

1. Write the date the prices were read, in **Prices as of**.
2. Add one row per model: the model's name exactly as a run names it, and its price in US dollars per
   million input tokens and per million output tokens.
3. Fill in the price of a CI minute and of a human hour.
4. Save. Every Cost figure on the page updates.

A run is priced as the model the agent reported, or else the model you requested. A model that is not on the
card cannot be priced. The CI-minute price is kept on the card but is not part of the Cost figure.

Dollar figures are estimates for reading only. Limits are in tokens: see [Budgets](merging.md#budgets).
