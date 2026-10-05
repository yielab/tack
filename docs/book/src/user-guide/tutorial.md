# Step-by-Step Tutorial

From a fresh machine to a finished agent run: install Tack, create a project, add
tasks, turn agent execution on, hand one task to Claude Code, watch it run, and
download the result. Every screenshot below is a real capture from one live session
against a clean database — including the run itself, which is a real model call, not
a mock. (The capture recipe lives in `frontend/e2e/tutorial-assets.spec.ts`; see
[Regenerating these screenshots](#regenerating-these-screenshots).)

This page walks **one** path: the release binary, the web UI, and the embedded
runner. The [Quick Start](quick-start.md) covers the alternatives — desktop app,
CLI-only agent runs, building from source.

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
# {"migrations_applied":77,"status":"ok","version":"0.1.0-beta.9"}
```

`migrations_applied` is how many migrations this build actually ran — trust that
field over the number above, which is what the build this tutorial was captured on
reported. If this fails, see [Troubleshooting](troubleshooting.md).

Homebrew, Windows, and the other install methods are in the [Quick Start —
Install](quick-start.md#install) section.

## 2. First open

Open **`http://localhost:3210`** in a browser. On a fresh database there is nothing
yet — just the invitation to create a project:

<img src="../screenshots/tutorial/01-first-open.png" width="98%" alt="Tack's first open on an empty database: the Projects page with a 'Create your first project' button and the sidebar showing All projects, Templates, Agents, and Settings.">

## 3. Create a project

Click **New Project** (or **Create your first project**). Give it a name, optionally
a description, and pick a project type — the type is a template that pre-loads a
matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can change
later. This tutorial creates *Website Relaunch* as a Software (Scrum) project:

<img src="../screenshots/tutorial/02-new-project.png" width="98%" alt="The Create New Project modal with name 'Website Relaunch', a description, 'Start blank' selected, and the Software (Scrum) project type chosen.">

Click **Create Project** and the new board opens — empty columns from the Scrum
workflow, plus a three-step onboarding card:

<img src="../screenshots/tutorial/03-board-empty.png" width="98%" alt="The empty board of the new project: Backlog, To Do, In Progress, and In Review columns with zero items, and a 'Your project is ready' onboarding card on top.">

## 4. Add tasks

Click the **+** in any column (or the onboarding card's **+ Add Item**). Only the
title is required; type, priority, description, story points, subtasks, and tags are
there when you want them:

<img src="../screenshots/tutorial/04-new-item.png" width="98%" alt="The Create New Item modal: title 'Draft the launch announcement', type Task, priority Medium, and a one-line description in the rich-text editor.">

A few items later the board is a board. Note the banner on top — the board itself
offers to turn agent execution on, which is where this tutorial goes next:

<img src="../screenshots/tutorial/05-board-tasks.png" width="98%" alt="The board with four tasks in Backlog, and a banner reading 'This board can run its items with an agent' with a Turn on link.">

## 5. Turn agent execution on

By default **nothing here executes anything** — the server is a full project manager
with agent execution off. Open the **Agents** page from the sidebar. Step 1 is the
one switch that matters:

<img src="../screenshots/tutorial/06-agents-off.png" width="98%" alt="The Agents page with execution off: step 1 'Agent execution on this machine' shows a Stopped badge and a Turn on button; the later steps are waiting on it.">

Click **Turn on**. This starts an embedded runner inside the same server process —
no restart, no second binary. Step 2 then reports which agent harnesses the runner
actually found on this machine, at their real installed versions, and step 3 shows
what it knows about each one's own vendor login (`claude`, `codex login`) — with a
key caveat it states itself: Tack cannot see whether a login succeeded, only a test
run proves it. If you'd rather not rely on a harness's own subscription login, paste
a Vercel AI Gateway key in step 3 instead:

<img src="../screenshots/tutorial/07-agents-on.png" width="98%" alt="The Agents page after Turn on: execution Running with its start time, Codex and Claude Code both detected with installed versions, each login command listed as 'Present, unverified', the Vercel AI Gateway key panel, the empty Default model step, and the Test run form.">

## 6. Save a default model

Step 4 on the same page sets the project's **default model**, so the run dialog in
the next step needs no hand-typed identifiers. Pick **Type a model id**, enter the
provider and model your harness accepts, and **Save**:

<img src="../screenshots/tutorial/08-default-model.png" width="98%" alt="The Default model section with mode 'Type a model id', provider 'anthropic', model ID 'claude-sonnet-4-5', and a Save button.">

## 7. Assign a task to an agent

Back on the board, every card carries a **Run with agent** button (the ▶ on the
card, or the same button inside the item's detail drawer). Clicking it opens the run
dialog with its defaults already earned: the agent profile is picked (this session
created one named *Announcement writer* with `tack agent-profile create` — its
instructions travel with the request), the model radio sits on **Project default**,
and the dialog states why this combination is allowed — the runner reports the
harness forwards the chosen model verbatim. Choose the harness, point **Repository**
at the repo the agent should work in (here a local demo repo), and the **Run**
button is ready. The dialog reads top to bottom as the whole flow: **Who runs it**,
**What it gets** (the item, plus its brief if you wrote one on the item's **Brief** tab),
**How far it may go** (approvals, allowed tools, timeout) and **What happens after**
(verifying the result and pushing a branch, shown disabled with the reason when this runner
has neither set up):

<img src="../screenshots/tutorial/09-run-with-agent.png" width="98%" alt="The 'Run with agent' dialog for 'Draft the launch announcement': profile Announcement writer, harness Claude Code, model radio on 'Project default — anthropic / claude-sonnet-4-5', a git repository with remote and base revision filled, and the permissions and timeout fields below.">

## 8. Track the run

Click **Run**. The request queues, a runner leases it, and the attempt starts — and
the board tells you so without being asked: the card grows a live state chip.
Clicking the chip opens the item's **Execution** tab.

<img src="../screenshots/tutorial/10-board-tracking.png" width="98%" alt="The board while the run is in flight: the 'Draft the launch announcement' card carries a live 'Leased' chip.">

The Execution tab shows the attempt as it happens — state badge, the runner that
holds the lease, and cost tiles that honestly read "Not measured" until the attempt
reports real numbers:

<img src="../screenshots/tutorial/11-execution-running.png" width="98%" alt="The item drawer's Execution tab mid-run: the request is Leased, Attempt #1 shows a Running badge leased just now, provenance reads 'Not yet reported', and all three cost tiles read Not measured.">

## 9. See the result

When the attempt finishes, the same tab is the record of what actually happened:
the state, whether the model that ran **matched** the one requested (reported, not
assumed), and what it cost — measured figures labeled as measured, unmeasured ones
saying so:

<img src="../screenshots/tutorial/12-execution-succeeded.png" width="98%" alt="The finished attempt: request Succeeded, Attempt #1 Succeeded, 'Matched request — ran on anthropic / claude-sonnet-4-5, as requested', model/token cost $0.04 measured, runner time 19s, runner time cost Not measured, and a 'Show events, decisions & artifacts' link.">

**Show events, decisions & artifacts** expands the attempt's full record. The
Artifacts section holds what the run produced — the harness's own run log and, for
a run in a git checkout, the change it made (`changes.patch`, `files.json` and an
`evidence.json` manifest), each one click from download. The screenshot below shows the log:

<img src="../screenshots/tutorial/13-artifacts.png" width="98%" alt="The Artifacts section of the finished attempt: one artifact named claude-code-run.log, tagged log, 12.4 KB, with a Download button.">

That's the whole loop: a task on a board, handed to a real agent, tracked live, and
closed with a verifiable result. From here:

- [Agent Runners & Fleet Execution](agent-runners.md) — remote runners on other
  machines, selectors, budgets, decision policies, the full field-by-field reference.
- [Quick Start — Run an item with an agent](quick-start.md#run-an-item-with-an-agent)
  — the same flow driven entirely from the CLI.
- [Workflows & Statuses](workflows.md) and [Vocabulary](vocabulary.md) — make the
  board speak your domain's language.

---

## Regenerating these screenshots

Every image on this page comes from `frontend/e2e/tutorial-assets.spec.ts`, driven
against an already-running release build (`--features embed-spa`) of this checkout
on a fresh database, with real `claude`/`codex` binaries on PATH. There is no `make`
target for it on purpose: the run in steps 7–9 is a real, live, billed model call.
The config file, `frontend/playwright.tutorial-assets.config.ts`, carries the exact
recipe.
