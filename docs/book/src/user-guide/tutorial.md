# Step-by-Step Tutorial

From a fresh machine to a finished agent run: install Tack, create a project, add tasks, say
what "done" means for one of them, turn on the agent that will work on it, hand the task to
Claude Code, watch it run, and read the result. Every image below is a real capture from one
live session against a clean database — including the runs, which are real model calls, not
mocks.

The whole loop in one recording — a new project pointed at a folder, a task with its acceptance
criteria, a Claude Code run tracked from the card, the finished run reviewed and accepted, and
the files it changed (the wait for the agent is shortened):

<img src="../screenshots/workflow.gif" width="98%" alt="The whole workflow, recorded: a new project 'Docs Site' created with an existing folder on this computer as its code; a task 'Add a changelog page' added on the board with two acceptance criteria; Run with agent with Claude Code; the card's chip and the Execution tab following the run until it reads 'Finished — needs your review'; Accept; what the agent did; the files from the run; and the board again.">

This page walks **one** path: the release binary, the web UI, and the embedded runner. The
[Quick Start](quick-start.md) covers the alternatives — desktop app, CLI-only agent runs,
building from source.

---

## 1. Install and start Tack

Tack is a single self-contained binary — web UI, REST API, and SQLite engine in one
file. One line installs it:

```sh
TAG=v0.1.0-beta.10
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/yielab/tack/releases/download/$TAG/install.sh" | TACK_VERSION="$TAG" sh
tack            # starts the server + web UI at http://localhost:3210
```

Verify it's up:

```sh
curl http://localhost:3210/api/health
# {"migrations_applied":…,"status":"ok","version":"0.1.0-beta.10"}
```

`version` is the build you installed, and `migrations_applied` is how many database migrations
it has run. If this fails, see [Troubleshooting](troubleshooting.md).

Homebrew, Windows, and the other install methods are in the [Quick Start](quick-start.md).

## 2. First open

Open **`http://localhost:3210`** in a browser. On a fresh database there is nothing
yet — just the invitation to create a project:

<img src="../screenshots/tutorial/01-first-open.png" width="98%" alt="Tack's first open on an empty database: the Projects page with a 'Create your first project' button, a Browse templates link, and the sidebar showing All projects, Templates, Agents and Settings.">

## 3. Create a project

Click **New Project** (or **Create your first project**). Give it a name, optionally a
description, say whether it has code, and pick a project type — the type is a template that
pre-loads a matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can change
later. **Does this project have code?** offers an existing folder on this computer, a new
folder Tack creates, or no code yet; you can change the answer later under **Settings →
Automation**. This tutorial creates *Website Relaunch* as a Software (Scrum) project with no
code yet, and points it at a folder in step 7:

<img src="../screenshots/tutorial/02-new-project.png" width="98%" alt="The Create New Project modal with name 'Website Relaunch', a description, 'Does this project have code?' answered 'No code yet', 'Start blank' selected, and the Software (Scrum) project type chosen.">

Click **Create Project** and the new board opens — empty columns from the Scrum
workflow, plus a three-step onboarding card:

<img src="../screenshots/tutorial/03-board-empty.png" width="98%" alt="The empty board of the new project: a 'Your project is ready' card with three steps — Add your first item, Make it yours, Plan a sprint — above the Backlog, To Do, In Progress and In Review columns with zero items.">

## 4. Add tasks

Click the **+** in any column (or the onboarding card's **+ Add Item**). Only the
title is required; type, priority, description, story points, subtasks and tags are
there when you want them. The first task is the one an agent will write:

<img src="../screenshots/tutorial/04-new-item.png" width="98%" alt="The Create New Item modal: title 'Draft the launch announcement', type Task, priority Medium, and a one-line description of the post in the rich-text editor.">

A few tasks later the board is a board. The banner on top offers to let this board run
its items with an agent; this tutorial gets there in step 6:

<img src="../screenshots/tutorial/05-board-tasks.png" width="98%" alt="The board with four tasks in Backlog — Draft the launch announcement, Redesign the pricing page, Migrate DNS to the new host, Refresh the screenshots in the docs — each with a run button, and a banner reading 'This board can run its items with an agent' with Turn on and Dismiss.">

## 5. Say what "done" means

Click the task to open its drawer. Its **Overview** tab is the task itself: its description,
its **Done when** list (the acceptance criteria), and a collapsed **For the agent** block. **Done when** is a
checklist — one line per thing a person checks before calling the task done. **Add check** adds
a line, and each line saves when you leave it:

<img src="../screenshots/tutorial/06-brief.png" width="50%" alt="The drawer of 'Draft the launch announcement' on its Overview tab: the Run with agent button and the task's fields above, then the description, a Done when list of three checks — About 400 words, Ends with a call to action, A post the team can publish on launch day — each with Up, Down and Remove, an Add check button, a collapsed For the agent section, and Link GitHub issue.">

**For the agent** holds what only the agent needs — checks a machine runs, limits on what it
may touch, and the risk you see — and this announcement needs none of it. The agent receives the
criteria with the task's title and description, and every run keeps a copy. See
[Acceptance criteria and For the agent](items.md#acceptance-criteria-and-for-the-agent).

## 6. Turn agent execution on

The agent that does the work — Claude Code here — is a program on your computer. Tack does not
install it or sign you in to it: install Claude Code and sign in once with its own `claude`
command, exactly as you would to use it by hand. Tack then finds it.

By default **nothing here executes anything** — the server is a full project manager with agent
execution off. Open the **Agents** page from the sidebar. Step 1 is the one switch that matters:

<img src="../screenshots/tutorial/07-agents-off.png" width="98%" alt="The Agents page with execution off: step 1 'Agent execution on this machine' shows a Stopped badge, 'Off since this server started' and a Turn on button; step 2 says to turn execution on to see what is installed; step 3 Sign in waits on step 2; the Test run section begins below.">

Click **Turn on**. This starts an embedded runner inside the same server process — no restart, no
second binary. Step 2 lists the agents it found on this machine with their installed versions;
this machine has Codex and Claude Code, and the tutorial uses Claude Code. Step 3 is **Sign in**,
one row per installed agent. Each asks the agent itself, so a row reads **Signed in**, or **Not
signed in. Run `<command>` in a terminal.** with a link to that agent's own documentation; Tack
holds no model key and never signs in for you. **Run test** at the bottom runs the agent once to
prove it:

<img src="../screenshots/tutorial/08-agents-on.png" width="98%" alt="The Agents page after Turn on: execution Running with its start time and a Turn off button; Codex v0.149.1, Claude Code v2.1.304 and OpenCode installed (docket not installed), the Sign in step with one row per installed agent, saying Signed in or giving the command to run; the Test run section with a Run test button; and a collapsed Advanced section.">

## 7. Set up the project's agent

Everything a run needs from the project lives in its **Settings → Automation** tab: where the
code is, how the agent works on it (on a new branch of your repo, or in the folder directly),
what happens when a run finishes, and the **Agent** section — which agent, which model, and the
default profile. Each section is a single line until you open it; the picture shows **Agent** open:

<img src="../screenshots/tutorial/09-default-model.png" width="60%" alt="The Agent section of a project's Automation settings, opened: 'No default agent'; agent choices Claude Code, Codex and OpenCode; the Model field reading 'Choose an agent first'; default profile choices Implementer, Planner, Researcher and Reviewer; and a Manage profiles link.">

Once an agent is chosen, the **Model** field offers **Agent's default** (recommended: the agent uses whatever model it
uses on its own), the models the agent listed or suggested, and **Another model…**, a text field
for a name in the agent's own spelling. A line under it says where the list came from, or how to
name a model, with a link to that agent's own documentation; Tack never checks a typed name. This tutorial leaves the agent unchosen and picks it per run in step 8. It points the
project's **Where the code is** at a local git repository.

An **agent profile** is the agent's standing orders: a name, the instructions sent with every
run that uses it, and the tools it may use per agent. Four come built in — **Implementer**,
**Planner**, **Researcher** and **Reviewer**. **Manage profiles** (or **Advanced → Profiles** at
the bottom of the Agents page) opens the list; **+ Create agent profile** adds one. This one
writes announcements, is told not to call any tool, and is given only the read tools (**Read**,
**Grep** and **Glob** for Claude Code, with **All tools** left unticked). That also tells Tack it
is not expected to change files:

<img src="../screenshots/tutorial/10-agent-profile.png" width="40%" alt="The create-profile form: name 'Announcement writer', instructions to write a 400-word launch announcement and reply with the text only without calling any tool, an empty Summary shown as the profile's tooltip, Tools with All tools unticked and only Read, Grep and Glob ticked for claude-code, and Create and Cancel buttons.">

Click **Create**.

## 8. Assign the task to an agent

Back on the board, every card carries a **Run with agent** button (the ▶ on the card, or the
same button in the task's drawer). The dialog opens with a one-line summary of the run — the
profile, the model and where the agent works — and asks only what you might want to change:

- **What the agent will read** shows exactly the text it will be given: the task, its acceptance
  criteria and the project's definition of done.
- **Profile** — the one Tack picks for the task (the profile of the task's role, else the
  project's default, else the built-in **Implementer**), with a line saying which; this tutorial
  picks *Announcement writer*.
- **Ask before each action** pauses the agent before each tool call until you answer on the
  task; **Web tools** lets it use its own web-fetch and web-search tools. **Web tools** starts
  off. **Ask before each action** starts off when the agent runs inside a sandbox, as Claude
  Code does here, and on when it does not.
- **Advanced for this run** is where this run differs from the project: the **Agent** (here
  **Claude Code**), the model (the same field as in the project's settings; **Agent's default** works for Claude Code), the branch,
  the tools allowed, and the timeout. **Verify the result**, **Push the branch** and **Open a pull
  request** are shown disabled, each with the reason and the setting that turns it on (see
  [After a succeeded attempt](../admin-guide/runners.md#after-a-succeeded-attempt)).

<img src="../screenshots/tutorial/11-run-with-agent.png" width="60%" alt="The Run with agent dialog for 'Draft the launch announcement': the summary 'Announcement writer · the agent's default model · on a new branch of' a local repository; What the agent will read; the profile pills with Announcement writer selected; Ask before each action and Web tools unticked; Advanced for this run open with Agent Claude Code, the Model field set to Agent's default, Branch, Claude Code's tool checkboxes, Allowed tools left blank to use the profile's Read, Grep and Glob, Token limit empty for no limit, a 3600-second timeout, and Verify the result, Push the branch and Open a pull request disabled with their reasons; Cancel and Run.">

## 9. Run it and track it

Click **Run**. The request queues, the runner picks it up and the agent starts — and the board
says so without being asked: the card carries a live status chip, here still **Queued**.
Clicking the chip opens the task's **Execution** tab:

<img src="../screenshots/tutorial/12-board-tracking.png" width="98%" alt="The board right after Run: the 'Draft the launch announcement' card carries a live 'Queued' chip.">

The drawer follows the run as it happens: the **Last run** panel at the top reads **Running** —
*The agent is working on it* — with **Follow live** and **Stop**, and the Execution tab shows **Run 1**
with the runner that took it. What the agent did, the tokens it used and its cost appear when it
finishes:

<img src="../screenshots/tutorial/13-execution-running.png" width="98%" alt="The task drawer mid-run: the card's chip on the board reads Running, and the Last run panel at the top of the drawer reads Running, 'The agent is working on it', 'Used 0 of 2 000 000 tokens', with Follow live and Stop buttons and a Run again button below it.">

## 10. See the result

When the run finishes, the same tab is the record of what happened: one run card with its status,
the verdict it waits for (**Accept** or **Reject…**), what the agent concluded, what it did and
what it got, and its usage — tokens counted against the limit, folded under **Usage**; opened, it
also gives the approximate cost, which is the agent's own estimate at list price, not your bill.
(The **Last run** panel at the top of the drawer gives the same in short.) This agent only wrote text and its profile allows only read tools, so no change was expected
and the run reads **Finished — no changes recorded**, and **What the agent concluded** is the announcement itself:

<img src="../screenshots/tutorial/14-execution-succeeded.png" width="98%" alt="The finished run in the Execution tab: Run 1 'Finished — no changes recorded', a 'This run is waiting for your verdict' banner with Accept and Reject…, the agent and runner, 'The agent ended without changing any files. What it said is below.' with a Copy path button, What the agent concluded showing the opening of the announcement with a Read full report link, What the agent did as a timeline of three steps (Install skipped, agent.model_call, The run ended), What the agent got (Sandboxed with bwrap, no environment variables, the project's folder), a collapsed Usage line reading about 3 000 tokens counted and 0% of its limit, the request and attempt ids with Copy, a Raw log button, and Show questions & files; the board card behind carries a Finished chip.">

**Show questions & files** expands the run's full record. **Files from this run** are
what it produced, each one click from viewing or downloading: the agent's own run log (the
announcement is in it), and the record of the change — `changes.patch` (empty here: this agent
only wrote text), `files.json`, the `brief.json` it was given, and the `evidence.json` manifest
that ties them together:

<img src="../screenshots/tutorial/15-artifacts.png" width="40%" alt="Files from this run: claude-code-run.log (log, 19.1 KB), changes.patch (patch, 0 B), files.json (files, 2 B), brief.json (brief, 721 B) and evidence.json (evidence, 7.0 KB), each with View and Download.">

A run that changes files reads **Finished — needs your review** instead (and a run that was
allowed to change files but changed nothing reads **Failed**, with that as its reason), and the card carries a
**Needs your review** flag until you **Accept** or **Reject** it — the recording at the top of
this page shows that, end to end.

That's the whole loop: a task on a board with its definition of done, handed to a real agent,
tracked live, and closed with a result you can check. From here:

- [Running Agents on Tasks](running-agents.md) — choosing an agent and a model, asking
  before each action, and the same run from the CLI.
- [Automating a Project](automation.md) — let Tack start a task when it reaches a column,
  choose the profile from the task's role, and merge finished work by itself.
- [Quick Start — Run an item with an agent](quick-start.md#run-an-item-with-an-agent)
  — the same flow driven entirely from the CLI.
- [Workflows & Statuses](workflows.md) and [Vocabulary](vocabulary.md) — make the
  board speak your domain's language.
