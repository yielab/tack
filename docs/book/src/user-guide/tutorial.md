# Step-by-Step Tutorial

This page is for someone who has never used Tack. Follow it from top to bottom and you will install
Tack, create a project, add tasks, say what "done" means for one of them, turn on the agent that
will work on it, hand the task to Claude Code, watch it run, and read the result. You need Claude
Code installed and signed in for steps 6 to 10; steps 1 to 5 need nothing else. Every image below
is a capture from one session against a clean database, including the runs, which are real model
calls.

The whole loop in one recording — a new project pointed at a folder, a task with its acceptance
criteria, a Claude Code run tracked from the card, the finished run reviewed and accepted, and
the files it changed (the wait for the agent is shortened):

<img src="../screenshots/workflow.gif" width="98%" alt="The whole workflow, recorded: a new project 'Docs Site' created with an existing folder on this computer as its code; a task 'Add a changelog page' added on the board with two acceptance criteria; Run with agent with Claude Code; the card's chip and the task's Runs following the run until it reads 'Finished — needs your review'; Accept; what the agent did; the files from the run; and the board again.">

This page walks **one** path: the release binary and the web UI. The
[Quick Start](quick-start.md) covers the alternatives — the desktop app, Docker, building from
source.

---

## 1. Install and start Tack

Tack is a single self-contained binary — web UI, REST API, and SQLite engine in one
file. One line installs it:

```sh
TAG=$(curl -fsSL "https://api.github.com/repos/yielab/tack/releases?per_page=1" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/yielab/tack/releases/download/$TAG/install.sh" | TACK_VERSION="$TAG" sh
tack            # starts the server + web UI at http://localhost:3210
```

Verify it's up:

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"…","migrations_applied":…}
```

`version` is the build you installed. If this fails, see [Troubleshooting](troubleshooting.md).

Homebrew, Windows, and the other install methods are in the [Quick Start](quick-start.md).

## 2. First open

Open **`http://localhost:3210`** in a browser. On a fresh database there is nothing
yet — just the invitation to create a project:

<img src="../screenshots/tutorial/01-first-open.png" width="98%" alt="Tack's first open on an empty database: the Projects page with a 'Create your first project' button, a Browse templates link, and the sidebar showing All projects, Templates, Agents and Settings.">

## 3. Create a project

Click **New Project** (or **Create your first project**). Give it a name and, if you like, a
description. Then make two choices:

- **Does this project have code?** An existing folder on this computer, a new folder Tack
  creates, or no code yet. You can change the answer later under **Settings → Automation**.
- **The project type.** A template that pre-loads a matching [workflow](workflows.md) and
  [vocabulary](vocabulary.md), which you can change later.

This tutorial creates *Website Relaunch* as a Software (Scrum) project with no code yet, and
points it at a folder in step 7:

<img src="../screenshots/tutorial/02-new-project.png" width="98%" alt="The Create New Project modal with name 'Website Relaunch', a description, 'Does this project have code?' answered 'No code yet', 'Start blank' selected, and the Software (Scrum) project type chosen.">

Click **Create Project** and the new board opens — empty columns from the Scrum
workflow, plus a three-step onboarding card:

<img src="../screenshots/tutorial/03-board-empty.png" width="98%" alt="The empty board of the new project: a 'Your project is ready' card with three steps — Add your first item, Make it yours, Plan a sprint — above the Backlog, To Do, In Progress and In Review columns with zero items.">

## 4. Add tasks

Click the **+** in any column (or the onboarding card's **+ Add item**). Only the
title is required; type, parent, priority, description, story points and tags are
there when you want them. The first task is the one an agent will write:

<img src="../screenshots/tutorial/04-new-item.png" width="98%" alt="The Create New Item modal: title 'Draft the launch announcement', type Task, priority Medium, and a one-line description of the post in the rich-text editor.">

A few tasks later the board is a board. The banner on top offers to let this board run
its items with an agent; this tutorial gets there in step 6:

<img src="../screenshots/tutorial/05-board-tasks.png" width="98%" alt="The board with four tasks in Backlog — Draft the launch announcement, Redesign the pricing page, Migrate DNS to the new host, Refresh the screenshots in the docs — each with a run button, and a banner reading 'This board can run its items with an agent' with Turn on and Dismiss.">

## 5. Say what "done" means

Click the task to open its drawer. The task reads top down: its description, its
**Done when** list and a collapsed **For the agent** block. **Done when** is a checklist, one line per
thing a person checks before calling the task done (these are the task's *acceptance criteria*).
**Add check** adds a line, and each line saves when you leave it. For this task, add three checks:
*About 400 words*, *Ends with a call to action* and *A post the team can publish on launch day*.

<img src="../screenshots/tutorial/06-brief.png" width="50%" alt="The drawer of 'Draft the launch announcement' with the status Backlog, the next step saying nothing has run yet with a Run with agent button, the description, a Done when list of three checks — About 400 words, Ends with a call to action, A post the team can publish on launch day — each with Up, Down and Remove, an Add check button, and the For the agent, Runs and Properties folds closed.">

**For the agent** holds what only the agent needs: checks a machine runs, limits on what it may
touch, and the risk you see. This announcement needs none of it. The agent receives the criteria
with the task's title and description, and every run keeps a copy. See
[Acceptance criteria and For the agent](items.md#acceptance-criteria-and-for-the-agent).

## 6. Turn agent execution on

The agent that does the work, Claude Code here, is a program on your computer. Tack does not
install it or sign you in to it: install Claude Code and sign in once with its own `claude`
command, exactly as you would to use it by hand. Tack then finds it.

Until you turn execution on, **nothing runs an agent**; Tack is only a project manager. Open the
**Agents** page from the sidebar. Step 1 is the one switch that matters:

<img src="../screenshots/tutorial/07-agents-off.png" width="98%" alt="The Agents page with execution off: step 1 'Agent execution on this machine' shows a Stopped badge, 'Off since this server started' and a Turn on button; step 2 says to turn execution on to see what is installed; step 3 Sign in waits on step 2; the Test run section's Run test button is disabled until an agent is installed.">

Click **Turn on**. It starts at once, with no restart and no second program. Then read the page
from the top:

- **Step 2** lists the agents Tack found on this machine, with their versions. This machine has
  Codex and Claude Code; the tutorial uses Claude Code.
- **Step 3, Sign in**, has one row per installed agent. Each row asks the agent itself, so it
  reads **Signed in**, or **Not signed in. Run `<command>` in a terminal.** with a link to the
  agent's own documentation. Tack holds no model key and never signs in for you.
- **Run test**, at the bottom, runs an agent once to prove the path works. It is disabled, and
  says which agent to sign in, while the agent it would use is not signed in.

Press **Run test** now; if it passes, everything after this step will work.

<img src="../screenshots/tutorial/08-agents-on.png" width="98%" alt="The Agents page after Turn on: execution Running with its start time and a Turn off button; Codex and Claude Code installed and contained by this computer, docket not installed, OpenCode installed but refused here until its containment is available; the Sign in step with Codex and Claude Code signed in; the Test run section with a Run test button; what each agent can do as a table; the Teams section; Add a manager; and a collapsed Advanced section.">

## 7. Set up the project's agent

Everything a run needs from the project lives in its **Settings → Automation** tab: where the
code is, how the agent works on it (on a new branch of your repo, or in the folder directly), the
definition of done, and the **Agent** section: which agent, which model, and the default profile.
Each section is a single line until you open it; the picture shows **Agent** open:

<img src="../screenshots/tutorial/09-default-model.png" width="60%" alt="The Agent section of a project's Automation settings, opened: 'No default agent'; agent choices Claude Code, Codex and OpenCode; the Model field reading 'Choose an agent first'; default profile choices Implementer, Planner, Researcher and Reviewer; and a Manage profiles link.">

Once an agent is chosen, the **Model** field offers **Agent's default** (recommended: the agent
uses whatever model it uses on its own), the models the agent listed or suggested, and **Another
model…**, a text field for a name in the agent's own spelling. Tack never checks a typed name.
This tutorial leaves the agent unchosen here and picks it per run in step 8. What it does set
here is **Where the code is**: open that section and point it at a local git repository
(the agent in this tutorial only writes text, but a run needs somewhere to work).

An **agent profile** is the agent's standing orders: a name, the instructions sent with every run
that uses it, and the tools it may use. Four come built in: **Implementer**, **Planner**,
**Researcher** and **Reviewer**. To make your own, open **Manage profiles** (or **Advanced →
Profiles** at the bottom of the Agents page) and press **+ Create agent profile**. For this
tutorial, fill in:

- **Name**: *Announcement writer*.
- **Instructions**: *Write a 400-word launch announcement and reply with the text only. Do not
  call any tool.*
- **Tools**: leave **All tools** unticked and tick only **Read**, **Grep** and **Glob** under
  claude-code. Giving the profile only read tools also tells Tack it is not expected to change files.

<img src="../screenshots/tutorial/10-agent-profile.png" width="40%" alt="The create-profile form: name 'Announcement writer', instructions to write a 400-word launch announcement and reply with the text only without calling any tool, an empty Summary and Delivers, Team No team, Web access listed, what the profile does, an empty token limit and time limit, the network switch off, Tools with All tools unticked and only Read, Grep and Glob ticked for claude-code, and Create and Cancel buttons.">

Click **Create**.

## 8. Assign the task to an agent

Back on the board, every card carries a **Run with agent** button (the ▶ on the card, or the same
button in the task's drawer). The dialog opens with a summary of the run (the profile, the model and
where the agent works) and asks only what you might want to change. Under **Who does it**, pick
**Claude Code**, then the profile *Announcement writer*, and leave everything else as it is. What each
part of the dialog does, and where its defaults come from, is in
[The Run with agent dialog](running-agents.md#the-run-with-agent-dialog).

<img src="../screenshots/tutorial/11-run-with-agent.png" width="60%" alt="The Run with agent dialog for 'Draft the launch announcement': Who does it with Claude Code chosen among Claude Code, Codex and OpenCode and the profile Announcement writer selected; What will happen: the profile and the agent's default model, a new branch of a local repository, no install, three Done when checks, contained by this computer, web tools off; Ask before each action and Web tools unticked; What the agent will read and Advanced for this run closed; the provisional token limit; Cancel and Start run.">

## 9. Run it and track it

Click **Start run**. The request queues, the runner picks it up and the agent starts. The board says
so without being asked: the card carries a live status chip, here still **Queued**. Clicking the
chip opens the task's **Runs**:

<img src="../screenshots/tutorial/12-board-tracking.png" width="98%" alt="The board right after Run: the strip counts 1 Queued, and the 'Draft the launch announcement' card carries a live 'Queued' chip.">

The drawer follows the run as it happens. The **next step** under the title reads **Running**
(*The agent is working on it*) with **Follow live** and **Stop**, and the **Runs** fold shows **Run 1**
with the runner that took it. What the agent did, and the tokens it used, appear when it finishes:

<img src="../screenshots/tutorial/13-execution-running.png" width="98%" alt="The task drawer mid-run: the card's chip on the board reads Running, and the next step at the top of the drawer reads Running, 'The agent is working on it', 0 of 2 000 000 tokens against the provisional limit, with Follow live and Stop buttons; Run 1 below says its steps appear as it takes them.">

## 10. See the result

When the run finishes, the same **Runs** fold is the record of what happened: one run card with its status,
the verdict it waits for (**Accept** or **Reject…**), what the agent concluded, what it did and what
it got, and its usage (tokens counted against the limit, folded under **Usage**). The **next step**
under the title gives the same in short. This agent only wrote text and its profile allows only
read tools, so no change was expected: the run reads **Finished — no changes recorded**, and **What
the agent concluded** is the announcement itself:

<img src="../screenshots/tutorial/14-execution-succeeded.png" width="98%" alt="The finished run in the task's Runs: Run 1 by Claude Code, 'The agent ended without changing any files. What it said is below.', about 3 800 tokens counted, not merged because no branch was pushed, a 'This run is waiting for your verdict' box with Accept and Reject…, What the agent concluded showing the opening of the announcement with Read full report and Download report.md, three Done when checks waiting for a person, Changes saying no files changed, and the Steps, Files from this run, Technical and Details folds; the board card behind carries a Finished chip.">

**Files from this run** lists what the run produced, each file one click from viewing or downloading:
the agent's own run log (the announcement is in it), `changes.patch` (empty here, because this agent
only wrote text), `files.json`, the `brief.json` it was given, and the `evidence.json` file that
ties them together:

<img src="../screenshots/tutorial/15-artifacts.png" width="40%" alt="Files from this run: claude-code-run.log (log, 19.3 KB), changes.patch (patch, 0 B), files.json (files, 2 B), brief.json (brief, 721 B) and evidence.json (evidence, 7.6 KB), each with View and Download.">

A run that changes files reads **Finished — needs your review** instead, and the card carries a
**Needs your review** flag until you **Accept** or **Reject** it; the recording at the top of this
page shows that, end to end. (A run that was allowed to change files but changed nothing reads
**Failed**, with that as its reason.)

That's the whole loop: a task on a board with its definition of done, handed to a real agent,
tracked live, and closed with a result you can check. From here:

- [Running Agents on Tasks](running-agents.md): choosing an agent and a model, and asking before
  each action.
- [Reviewing a Run](reviewing-runs.md): judging a run that changed files.
- [Automating a Project](automation.md): let Tack start a task when it reaches a column, choose the
  profile from the task's role, and merge finished work by itself.
- [Running from the command line](running-agents.md#running-from-the-command-line): the same run
  started from a terminal.
- [Workflows & Statuses](workflows.md) and [Types and words](vocabulary.md): make the board speak
  your domain's language.
