# Step-by-Step Tutorial

From a fresh machine to a finished agent run: install Tack, create a project, add tasks, say
what "done" means for one of them, turn on the agent that will work on it, hand the task to
Claude Code, watch it run, and read the result. Every image below is a real capture from one
live session against a clean database — including the runs, which are real model calls, not
mocks. (The capture recipe lives in `frontend/e2e/tutorial-assets.spec.ts`; see
[Regenerating these screenshots](#regenerating-these-screenshots).)

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
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack            # starts the server + web UI at http://localhost:3210
```

Verify it's up:

```sh
curl http://localhost:3210/api/health
# {"migrations_applied":83,"status":"ok","version":"0.1.0-beta.10"}
```

`migrations_applied` is how many migrations this build actually ran — trust that
field over the number above, which is what the build this tutorial was captured on
reported. If this fails, see [Troubleshooting](troubleshooting.md).

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

Click the task to open its drawer. The **Details** tab is the task itself: its description,
its **Acceptance criteria**, and a collapsed **For the agent** block. Acceptance criteria are a
checklist — one line per thing a person checks before calling the task done. **Add check** adds
a line, and each line saves when you leave it:

<img src="../screenshots/tutorial/06-brief.png" width="50%" alt="The drawer of 'Draft the launch announcement' on its Details tab: the Run with agent button, the description, three acceptance criteria — About 400 words, Ends with a call to action, A post the team can publish on launch day — with Up, Down and Remove, an Add check button, a collapsed For the agent section, and Link GitHub issue.">

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

<img src="../screenshots/tutorial/07-agents-off.png" width="98%" alt="The Agents page with execution off: step 1 'Agent execution on this machine' shows a Stopped badge, 'Off since this server started' and a Turn on button; step 2 says to turn execution on to see what is installed; step 3 Provider explains logins and offers the Vercel AI Gateway key; the Test run section begins below.">

Click **Turn on**. This starts an embedded runner inside the same server process — no restart, no
second binary. Step 2 lists the agents it found on this machine with their installed versions;
this machine has Codex and Claude Code, and the tutorial uses Claude Code. Tack cannot see an
agent's own login, so each reads **Not signed in** with its sign-in command until a run with that
agent succeeds here, and then **Signed in** with the date. **Run test** at the bottom runs the
agent once to prove it. If you'd rather not use the agent's own subscription, paste a Vercel AI
Gateway key under **Provider** instead:

<img src="../screenshots/tutorial/08-agents-on.png" width="98%" alt="The Agents page after Turn on: execution Running with its start time and a Turn off button; Codex v0.149.1 and Claude Code v2.1.291 both installed, each 'Not signed in' with its sign-in command and a Re-check button; the Provider step with the Vercel AI Gateway key; the Test run section with a Run test button; and a collapsed Advanced section.">

## 7. Set up the project's agent

Everything a run needs from the project lives in its **Settings → Automation** tab: where the
code is, how the agent works on it (on a new branch of your repo, or in the folder directly),
what happens when a run finishes, and the **Agent** section — which agent, which model, and the
default profile:

<img src="../screenshots/tutorial/09-default-model.png" width="60%" alt="The Agent section of a project's Automation settings: agent choices Claude Code, Codex, docket and opencode; Model set to 'The agent's default (recommended)', with Specific model waiting on an agent; default profile choices Implementer, Planner, Researcher and Reviewer; and a Manage profiles link.">

**The agent's default** is the recommended model: the agent uses whatever model it uses on its
own. This tutorial leaves the agent unchosen and picks it per run in step 8. It points the
project's **Where the code is** at a local git repository.

An **agent profile** is the agent's standing orders: a name, the instructions sent with every
run that uses it, and the tools it may use per agent. Four come built in — **Implementer**,
**Planner**, **Researcher** and **Reviewer**. **Manage profiles** (or **Advanced → Profiles** at
the bottom of the Agents page) opens the list; **+ Create agent profile** adds one. This one
writes announcements and is told not to call any tool:

<img src="../screenshots/tutorial/10-agent-profile.png" width="40%" alt="The create-profile form: name 'Announcement writer', instructions to write a 400-word launch announcement and reply with the text only without calling any tool, an empty Summary shown as the profile's tooltip, Tools with All tools ticked for claude-code, codex, opencode and docket, and Create and Cancel buttons.">

Click **Create**.

## 8. Assign the task to an agent

Back on the board, every card carries a **Run with agent** button (the ▶ on the card, or the
same button in the task's drawer). The dialog opens with a one-line summary of the run — the
profile, the model and where the agent works — and asks only what you might want to change:

- **What the agent will read** shows exactly the text it will be given: the task, its acceptance
  criteria and the project's definition of done.
- **Profile** — the project's default, or the built-in **Implementer**; this tutorial picks
  *Announcement writer*.
- **Ask before each action** pauses the agent before each tool call until you answer on the
  task; **Allow network access** lets it reach the network. Both start off.
- **Advanced for this run** is where this run differs from the project: the **Agent** (here
  **Claude Code**), the model (the agent's default is **Supported** for Claude Code), the branch,
  the tools allowed, and the timeout. **Verify the result**, **Push the branch** and **Open a pull
  request** are shown disabled, each with the reason and the setting that turns it on (see
  [After a succeeded attempt](agent-runners.md#after-a-succeeded-attempt)).

<img src="../screenshots/tutorial/11-run-with-agent.png" width="60%" alt="The Run with agent dialog for 'Draft the launch announcement': the summary 'Announcement writer · the agent's default · on a new branch of' a local repository; What the agent will read; the profile pills with Announcement writer selected; Ask before each action and Allow network access unticked; Advanced for this run open with Agent Claude Code, The agent's default (recommended) marked Supported, Branch, Claude Code's tool checkboxes, Allowed tools, a 3600-second timeout, and Verify the result, Push the branch and Open a pull request disabled with their reasons; Cancel and Run.">

## 9. Run it and track it

Click **Run**. The request queues, the runner picks it up and the agent starts — and the board
says so without being asked: the card carries a live status chip, here still **Queued**.
Clicking the chip opens the task's **Execution** tab:

<img src="../screenshots/tutorial/12-board-tracking.png" width="98%" alt="The board right after Run: the 'Draft the launch announcement' card carries a live 'Queued' chip.">

The drawer follows the run as it happens: the **Last run** panel at the top reads **Running** —
*The agent is working on it* — with **Follow the run**, and the Execution tab shows **Run 1**
with the runner that took it. What the agent did, the tokens it used and its cost appear when it
finishes:

<img src="../screenshots/tutorial/13-execution-running.png" width="98%" alt="The task drawer mid-run: the card's chip reads Starting, the Last run panel reads Running with 'The agent is working on it' and Follow the run, and the Execution tab shows Run 1 Running, started just now, with a note that what it did, its tokens and its cost appear when it finishes.">

## 10. See the result

When the run finishes, the same tab is the record of what happened: one status, what the agent
said, and its usage — tokens first, then the approximate cost, which is the agent's own estimate
at list price, not your bill. This agent only wrote text, so the run reads **Finished — no
changes recorded**, and **What the agent said** is the announcement itself:

<img src="../screenshots/tutorial/14-execution-succeeded.png" width="98%" alt="The finished run: Run 1 'Finished — no changes recorded', 'The agent ended without changing any files. What it said is below.', Copy path and Log buttons, What the agent said showing the announcement's headline and first paragraph, and usage reading 17K tokens in, 1.8K out, about $0.04, claude-sonnet-5-5 auto-selected, one model call, 16 seconds; the board card behind carries a Finished chip.">

**Show timeline, questions & files** expands the run's full record. **Files from this run** are
what it produced, each one click from viewing or downloading: the agent's own run log (the
announcement is in it), and the record of the change — `changes.patch` (empty here: this agent
only wrote text), `files.json`, the `brief.json` it was given, and the `evidence.json` manifest
that ties them together:

<img src="../screenshots/tutorial/15-artifacts.png" width="40%" alt="Files from this run: claude-code-run.log (log, 17.6 KB), changes.patch (patch, 0 B), files.json (files, 2 B), brief.json (brief, 721 B) and evidence.json (evidence, 6.8 KB), each with View and Download.">

A run that changes files reads **Finished — needs your review** instead, and the card carries a
**Needs your review** flag until you **Accept** or **Reject** it — the recording at the top of
this page shows that, end to end.

That's the whole loop: a task on a board with its definition of done, handed to a real agent,
tracked live, and closed with a result you can check. From here:

- [Agent Runners & Fleet Execution](agent-runners.md) — remote runners on other
  machines, selectors, budgets, decisions, verification, branch push and pull requests.
- [Quick Start — Run an item with an agent](quick-start.md#run-an-item-with-an-agent)
  — the same flow driven entirely from the CLI.
- [Workflows & Statuses](workflows.md) and [Vocabulary](vocabulary.md) — make the
  board speak your domain's language.

---

## Regenerating these screenshots

Every image on this page comes from `frontend/e2e/tutorial-assets.spec.ts`, driven
against an already-running release build (`--features embed-spa`) of this checkout
on a fresh database, with a real, signed-in `claude` on PATH and `ffmpeg` for the recording.
There is no `make` target for it on purpose: the runs in steps 9–10 and in the recording are
real, live, billed model calls. The config file,
`frontend/playwright.tutorial-assets.config.ts`, carries the exact recipe.
