# Automating a Project

This page is for the person who decides how much of a project's work Tack does without being asked. After
reading it you can say which profile and team work on which tasks, let a task start by itself, understand
how far Tack may merge on its own, make a task repeat, and read the badges that show what Tack did. It then
covers the project's other agent settings: where the code is, where the agent works, and the default agent
and model.

## Automation

Everything lives in **Settings → Automation**, a tab of the project's settings. A value lives at the
highest place that can own it: the computer (the Agents page), the project (this tab) or the task (the
run dialog). The dialog shows project values read-only, with a link to change them.

The tab has two parts. First come four short sections about the project's other settings, folded to one line
each until you open them: **Where the code is**, **How the agent works on it**, **When a run finishes** and
**Agent**. Below them is the **ladder**, four steps that you climb in order, each handing more work to Tack:

1. **Teams.** Choose a profile for each role, so a task knows who works on it.
2. **Start.** Let a task start a run by itself when it reaches a status.
3. **Merge.** Let Tack merge a finished change by itself, as far as you allow.
4. **Ceiling.** See the most this project can reach, and what holds it down.

The ladder as it looks, with every control that can be disabled shown with its reason:

```text
1. Teams
   Each role works with a profile. The profile names the team and the machine.
   Content      [ Fabulada content writer v ]
                fab-content · lights-out · this computer
   Developer    [ Fabulada implementer v ]
                fab-dev · needs a person · this computer
   No role      [ No profile v ]
   Manage profiles →      [ Apply the repository's teams ]   (disabled)
   Tack cannot reach the team's machine from here yet. In the project's folder, run: docket init --name <team>

2. Start
   [ ] Start a task by itself
   When a task reaches [ To Do v ]   Keep at most [ 3 ] runs open
   (with no profile anywhere: "Choose a profile for at least one role in Teams", and all three
    controls disabled)

3. Merge
   (o) L3 - You merge every change yourself.
   ( ) L4 - Tack merges changes that stay inside the domains below.   (disabled while L4 and L5 are off)
   ( ) L5 - Tack merges any change that passes its checks.            (disabled while L4 and L5 are off)
   L4 and L5 are off: <the reasons>
   [ ] Merge when moved to Done
   Domains, Stages the verifier must run, Public key, Times a rejected change goes back to the agent,
   Daily limits

4. Ceiling
   This project can reach L3.
   ✗ The team fab-dev keeps its tasks at L3. <what to change>
   ✓ The team fab-content runs lights-out.
```

The names above (Fabulada, fab-content) are only an example; yours come from your own profiles.

> **Which tasks does this touch?** Only tasks with a role. A role is how Tack knows a task is meant for an
> agent. Create roles under **Settings → Roles**, and give a task a role on its **Fields** tab.

---

## Teams

The first step answers "who works on a task?". Each role works with a **profile**: the instructions, the
tools and, optionally, the agent, the team and the fleet of machines that run it. You pick a profile for each
role in a dropdown. **No role** is the project's default profile, used by a task whose roles name none. Under
each choice a line says what you chose: the team, whether it runs **lights-out** or **needs a person**, and the
machine ("this computer" or the name of a fleet).

### How a run chooses its profile

When a run does not name a profile, Tack takes the first of these that exists:

1. The settings saved on the task, from an earlier run of it.
2. The profile of the task's role. If the task has several roles with different profiles, the project's
   default profile is used instead, and the run dialog says "More than one role has a profile, so the
   project default is used".
3. The project's default profile.
4. The built-in **Implementer**.

The run dialog says which of these applied: "Chosen by the role Content", "Project default", "Saved on this
task" or "The built-in Implementer". To use another profile for one run, press another name in the dialog's
**Profile** row; see [The Run with agent dialog](running-agents.md#the-run-with-agent-dialog).

### Teams and docket

With docket, a profile names a team, and every run goes to that team: see [Choosing an
agent](running-agents.md#details-per-agent). A profile with no team uses a team named for the project's
folder. A profile also names its agent when you want that profile to always use one agent. The team and the
agent are set in the profile's definition in the [project file](import-export.md); the profile form on the
Agents page does not offer them.

**Lights-out** means the team can run to the end without a person. **Needs a person** means the team stops
for a person during a run, or breaks a rule of its profile. The line says which. An administrator checks
this with `tack runner doctor`; see [Installing and Operating Runners](../admin-guide/runners.md).

### The Apply button

**Apply the repository's teams** is always disabled today. Its reason line reads: "Tack cannot reach the
team's machine from here yet. In the project's folder, run: `docket init --name <team>`". The board has no
channel to the machine where the team lives, so you create the team there yourself. **Manage profiles** links
to the profiles on the Agents page.

A project with no roles says: "This project has no roles yet. Add them under Roles."

---

## Start

The second step lets a task start by itself. It is off until you turn it on. It needs a profile to run with,
so until a role or the project has one, the three controls are disabled and the section says "Choose a profile
for at least one role in Teams".

1. Turn on **Start a task by itself**.
2. Under **When a task reaches**, choose the status that starts a task. The list shows your workflow's
   statuses.
3. Under **Keep at most … runs open**, choose how many runs may be open in the project at once. It starts
   at 3, and the smallest value is 1.

Until you pick a status, the list shows the first To do status. Turning the switch on saves the status
shown, so a project never has the switch on with no status to watch.

About every thirty seconds Tack looks at the tasks in that status and starts the most urgent first (critical, then
high, medium, low, none), and within the same priority in board order. A task starts only when all of these
hold:

- It has a role.
- It has no open run (queued, starting, running or waiting on you).
- It has no unfinished blocker.
- A profile resolves for it, as under [Teams](#teams), and an agent is named by the profile or by the
  project's default agent.
- There is a machine to run on: the profile's fleet, or exactly one active runner.
- The project's daily limits are not spent (tokens, dollars, items waiting for your review; see
  [Budgets](merging.md#budgets)).
- The first In progress column has room under its work-in-progress limit.
- Fewer than the cap of runs are open in the project.

A task that starts moves to the first In progress status, and its activity says "Started by itself: it
reached To Do with the role Content". The run lasts at most an hour. The agent works with the tools its
profile allows and with web tools off.

**It asks before each action unless every machine that could run it sandboxes the agent.** On a machine
where the agent is not sandboxed, a run started by itself pauses at each action and waits for your answer,
as under [Asking before acting](running-agents.md#asking-before-acting).

**It switches itself off when the project drops to L3.** When a change that Tack merged by itself is reverted,
or CI fails on its merge commit, Tack takes the project down a level. If that lands the project at L3, Start
is turned off, and the section says "Auto-start is off: the project dropped to L3". You turn it back on
yourself.

How often Tack looks is the administrator's setting; see
[Configuration Reference](../admin-guide/configuration-reference.md).

---

## Merge

The third step is how much of the merge Tack decides. It has the **Merge** button, **Merge when moved to
Done**, and the automatic merge at L4 and L5, with their domains, required checks, key and daily limits.
All of it is on [Merging from Tack](merging.md). L4 and L5 are shown disabled, with the reasons in "L4 and
L5 are off: …", until what an automatic merge needs is in place.

---

## Ceiling

The fourth step shows the most this project can reach, and what holds it down. It reads "This project can
reach L5." until something lowers it.

A team that needs a person during a run keeps its tasks at **L3**, however high the project's level is. For
each team that your roles' profiles name, the Ceiling lists one line:

- "✓ The team X runs lights-out."
- "✗ The team X keeps its tasks at L3." followed by what to change.
- For a team that no runner reports yet: "✗ The team X keeps its tasks at L3. No runner reports this team
  yet".

While a team needs a person, the ceiling reads L3 and the Merge step shows L4 and L5 disabled, with the same
reason. Other things that hold L4 and L5 off appear below the teams, each with a ✗: GitHub polling is off on
this board, the verifier is not installed where the board runs, or the project has no public key.

The ceiling holds back only the automatic merge. The **Merge** button is never held by it. If the team that
ran a change needs a person, the automatic merge holds the change and the task says "The team that ran this
stays at L3: it needs a person during a run".

---

## Repeating a task

A task can repeat **weekly**, **monthly** or **quarterly**. Closing it creates the next one, once.

Set it in the task's header, in **Repeats** (Never, Weekly, Monthly, Quarterly); see
[Working with Items](items.md#repeating-a-task). Tasks that repeat carry a **Repeats** badge on the board.

When the task moves into a Done status from a status that is not Done, Tack creates the next one if no task
made from it is still unfinished. The next task:

- is in the first To do column;
- is due at the end of the next period, counted from the closed task's due date (from today if it had none):
  a week later, the last day of the next month, or the last day of the next quarter;
- copies the title, description, type, parent, priority, estimate, tags, repeat setting and roles;
- does not copy the sprint or the assignee.

The closed task's activity says "Created the next one, due" with the date. Reopening a task and closing it
again does not make a second one while the first is unfinished.

The next task has the roles of the old one and starts in the first To do column. If that column is the
project's start status, Tack starts it on its next look, so a repeating task can run on its own each period.

---

## What Tack shows when it acts

When Tack acts without you, it tells you in three places.

**Badges** on the board, list and table:

| Badge | Meaning |
|---|---|
| **Started by itself** | The project started the task's latest run, because the task reached the start status |
| **Team: name** | The team named by the profile the latest run used |
| **Merged by itself** | Tack merged the change automatically, under the verifier's signed verdict |

Hover a badge for a one-sentence explanation.

**The task's Automation row**, in the task drawer under the run button, says what happens to this task
without you:

- "Starts by itself when it reaches To Do · Team fab-content · Merge L3", when the project starts tasks by
  itself;
- "By hand: this project doesn't start tasks by itself", when it does not;
- "By hand: the role Content has no profile", when a role of the task has no profile and the project has no
  default.

**The task's activity**, in one sentence per step:

- "Started by itself: it reached To Do with the role Content"
- "Tack merged pull request #12 automatically (domain docs, L4)", or "… with the Merge button", or "… when
  the task moved to Done"
- "Tack did not merge: " followed by the reason
- "Tack took domain docs from L4 to L3: pull request #12 (merged automatically) was reverted", or "went
  red on its merge commit"
- "Auto-start is off: the project dropped to L3"
- "Created the next one, due 2026-11-30"
- "Moved to Done over MCP: not merged; use Merge"

---

## The project's other settings

These are the folded sections above the ladder. Each saves as soon as you change it.

<img src="../screenshots/automation.png" width="60%" alt="The Automation tab of a project's settings: where the code is, how the agent works on it, the push switch, the default profile, the column a finished run moves the task to, and the Definition of done.">

### Where the code is

The answer to **Does this project have code?** from when you created the project: an existing folder on this
computer, or no code yet. Under **Advanced** there is also a repository URL, for a repository the agent
should clone. For a folder, the tab reads it back in one sentence ("A git repository on branch main, no
uncommitted files", "A folder without git: the agent will work in it directly", "That path does not exist").
A **Branch** field names the branch to work from.

### Where the agent works

How the agent works on the code is a project setting with three modes. Tack picks the first two for you from
your folder. You never type a repository address for code on this computer.

| Mode | What it does | When it is the default |
|---|---|---|
| **On a new branch of your repo** | Makes a worktree of your repository on a branch named `tack/<task>-a<n>`. The branch stays in your repository after the run. Your own checkout, your files and your index are untouched | A folder that is a git repository |
| **In the folder, directly** | The agent edits your folder itself, as if you had run it in a terminal. Nothing is ever deleted or moved, and your git index is untouched | A folder without git |
| **In a fresh clone per run** | Clones the repository into a new workspace for each run | Never the default. It is under **Advanced**, for a URL or for a runner on another computer |

For a git folder, the first mode is the default because it costs nothing (no network, shared objects), keeps
the result in your repository, and does not build on top of whatever is half-done in your checkout.
**In the folder, directly** is one click away. In a folder without git it is the only mode. A project with
no code shows "This project has no code. Choose a folder to let agents work on it." in place of these
choices. docket works only on a new branch of your folder.

**Push the branch when a run finishes** is a switch, off by default. Turning it on pushes the run's branch
to the repository's remote, using your own git credentials. It is disabled with a reason when it cannot
work: "Not available when the agent works in the folder directly", or "Push is not configured on this
computer". Push is set up on the runner by an administrator; see
[After a succeeded attempt](../admin-guide/runners.md#after-a-succeeded-attempt). If the project has push on
and the runner does not, the run records "push skipped" instead of failing. The work is not lost either way:
in the first mode it is on the local branch.

### When a run finishes

**Move the item to** is optionally the column a task moves to when a run on it finishes, in addition to
**Needs your review**. **Do not move** is the default. The **Definition of done** is one text the agent reads
on every run of this project.

### Agent and model

- **Agent** is the agent a new run uses. Each pill is an agent that a runner on this computer reports. A line
  below it says whether the agent is sandboxed or has your user's access, or, when the agent cannot be used
  here, why. With no runner reporting an agent, the section says "No runner reports an agent yet".
- **Model** is the same field as in the run dialog: **Agent's default**, the models the agent listed or
  suggested, and **Another model…**. Changing the project's agent clears the model, because a model name
  belongs to one agent. Until you choose an agent it says "Choose an agent first". See
  [Choosing a model](running-agents.md#choosing-a-model).
- **Default profile** is which profile a new run uses when nothing else chose one. It is the same setting as
  **No role** under [Teams](#teams). With no profiles it says "No agent profiles yet".

### Profiles

A profile is what the agent is told to be and which tools it may use. Four are built in:

| Profile | What it does |
|---|---|
| **Implementer** (default) | Reads, edits and runs commands |
| **Reviewer** | Reads and reports. Changes nothing |
| **Researcher** | Reads and searches, and writes a report |
| **Planner** | Plans and designs, and proposes subtasks you can accept |

You pick one with the pills in the run dialog; each pill has a tooltip saying what it may do. You edit a
profile in a form on the Agents page, never as JSON. A profile you change is copied into each request when
the request is created, so editing it later never changes the history of an earlier run. A profile that gives
the agent no tools gets a warning in the dialog: it can only answer in text.
