# Step-by-Step Tutorial

From a fresh machine to a finished agent run: install Tack, create a project, add
tasks, say what "done" means for one of them, set up the agent that will work on it,
hand the task to Claude Code, watch it run, and download the result. Every screenshot below is a real capture from one live session
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
# {"migrations_applied":81,"status":"ok","version":"0.1.0-beta.9"}
```

`migrations_applied` is how many migrations this build actually ran — trust that
field over the number above, which is what the build this tutorial was captured on
reported. If this fails, see [Troubleshooting](troubleshooting.md).

Homebrew, Windows, and the other install methods are in the [Quic## 2. First open

Open **`http://localhost:3210`** in a browser. On a fresh database there is nothing
yet — just the invitation to create a project:

<img src="../screenshots/tutorial/01-first-open.png" width="98%" alt="Tack's first open on an empty database: the Projects page with a 'Create your first project' button and the sidebar showing All projects, Templates, Agents and Settings.">

## 3. Create a project

Click **New Project** (or **Create your first project**). Give it a name, optionally
a description, and pick a project type — the type is a template that pre-loads a
matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can change
later. This tutorial creates *Website Relaunch* as a Software (Scrum) project:

<img src="../screenshots/tutorial/02-new-project.png" width="98%" alt="The Create New Project modal with name 'Website Relaunch', a description, 'Start blank' selected, and the Software (Scrum) project type chosen.">

Click **Create Project** and the new board opens — empty columns from the Scrum
workflow, plus a three-step onboarding card:

<img src="../screenshots/tutorial/03-board-empty.png" width="98%" alt="The empty board of the new project: Backlog, To Do, In Progress and In Review columns with zero items, and a 'Your project is ready' onboarding card on top.">

## 4. Add tasks

Click the **+** in any column (or the onboarding card's **+ Add Item**). Only the
title is required; type, priority, description, story points, subtasks and tags are
there when you want them. The first task is the one an agent will write:

<img src="../screenshots/tutorial/04-new-item.png" width="98%" alt="The Create New Item modal: title 'Draft the launch announcement', type Task, priority Medium, and a one-line description of the post in the rich-text editor.">

A few tasks later the board is a board. The banner on top offers to let this board run
its items with an agent; this tutorial gets there in step 6:

<img src="../screenshots/tutorial/05-board-tasks.png" width="98%" alt="The board with four tasks in Backlog — Draft the launch announcement, Redesign the pricing page, Migrate DNS to the new host, Refresh the screenshots in the docs — and a banner reading 'This board can run its items with an agent' with a Turn on link.">

## 5. Say what "done" means: the brief

Click the task to open its drawer, then the **Brief** tab. A brief is the definition of
done that travels with the task: acceptance criteria, constraints, a definition of done
in your own words, and a risk level. Each criterion has a kind — a command, a test, a
metric, a file that must or must not exist, or a check a person makes. Prefer a kind a
machine can run; this announcement has nothing to run, so both criteria are **Manual**
and the tab marks them as costing a person's time. **Save brief** stores it:

<img src="../screenshots/tutorial/06-brief.png" width="50%" alt="The Brief tab of 'Draft the launch announcement': two Manual criteria, 'About 400 words' and 'Ends with a call to action', each with what a person must check and marked 'Costs a person's time', no constraints, a definition of done, Risk not set, and a Save brief button.">

The agent receives the brief with the task's title and description, and the run keeps a
copy of it. See [Brief tab](items.md#brief-tab) for every kind of criterion.

## 6. Set up the harness: turn agent execution on

A *harness* is the coding agent that does the work — Claude Code here. Tack does not
install it or sign you in to it: install Claude Code and sign in once with its own
`claude` command, exactly as you would to use it by hand. Tack then finds it.

By default **nothing here executes anything** — the server is a full project manager
with agent execution off. Open the **Agents** page from the sidebar. Step 1 is the
one switch that matters:

<img src="../screenshots/tutorial/07-agents-off.png" width="98%" alt="The Agents page with execution off: step 1 'Agent execution on this machine' shows a Stopped badge and a Turn on button; the later steps are waiting on it.">

Click **Turn on**. This starts an embedded runner inside the same server process — no
restart, no second binary. Step 2 lists the harnesses it found on this machine, with
their installed versions; this machine has Codex and Claude Code, and the tutorial uses
Claude Code. Step 3 shows each one's sign-in command and says plainly that Tack cannot
see whether that sign-in worked — the run in step 9 is what proves it. If you'd rather
not use the harness's own subscription, paste a Vercel AI Gateway key there instead:

<img src="../screenshots/tutorial/08-agents-on.png" width="98%" alt="The Agents page after Turn on: execution Running with its start time, Codex and Claude Code both detected with their installed versions, each sign-in command listed as 'Present, unverified', the Vercel AI Gateway key panel, the empty Default model step and the Test run form.">

## 7. Set up the agent: a default model and a profile

Step 4 on the same page sets the project's **default model**, so the run dialog needs no
hand-typed identifiers. Pick **Type a model id**, enter the provider and a model your
harness accepts, and **Save**:

<img src="../screenshots/tutorial/09-default-model.png" width="98%" alt="The Default model section with mode 'Type a model id', provider 'anthropic', model ID 'claude-sonnet-5-5', and a Save button.">

An **agent profile** is the agent's standing orders: a name and the instructions sent
with every run that uses it, plus an optional tool policy and limits. Open **Advanced**
at the bottom of the Agents page, then **Agent profiles**, and **+ Create agent
profile**. This one writes announcements and is told not to call any tool:

<img src="../screenshots/tutorial/10-agent-profile.png" width="98%" alt="The Advanced section of the Agents page on its Agent profiles tab: the create form with name 'Announcement writer', instructions to write a 400-word launch announcement and reply with the text only without calling any tool, empty tool policy and limits, and a Create button.">

Click **Create**. With one profile, the run dialog selects it on its own.

## 8. Assign the task to an agent

Back on the board, every card carries a **Run with agent** button (the ▶ on the card,
or the same button in the item's drawer). The dialog reads top to bottom as the whole
run, and every row says **Ready** or what is missing:

- **Who runs it** — this machine's runner, the *Announcement writer* profile, the
  harness (choose **Claude Code**) and the model, already on **Project default**. The
  dialog states why this pairing is allowed: the runner reports that the harness passes
  the chosen model through as given.
- **What it gets** — the task and its brief, and the repository the agent works in
  (**Change for this run** to point it at one; here a local demo repository).
- **How far it may go** — **Automatic** lets the agent decide on its own; **Ask me**
  pauses it before each tool call until you answer in the task's decision inbox. Claude
  Code's tools are a checklist; none is ticked, matching the profile.
- **What happens after** — verifying the result, pushing a branch and opening a pull
  request. This runner has none of them set up, so each is shown disabled with the reason
  and the config that turns it on (see
  [After a succeeded attempt](agent-runners.md#after-a-succeeded-attempt)).

<img src="../screenshots/tutorial/11-run-with-agent.png" width="60%" alt="The Run with agent dialog for 'Draft the launch announcement'. Who runs it: runner connected, profile Announcement writer, harness Claude Code, model Project default — anthropic / claude-sonnet-5-5, each Ready. What it gets: the item, its brief, and a git repository with remote and base revision. How far it may go: Automatic, Claude Code's tool checklist, network off, timeout 3600. What happens after: Verify the result, Push the branch and Open a pull request disabled with their reasons. The Run button is enabled.">

## 9. Run it and track it

Click **Run**. The request queues, the runner leases it and the attempt starts — and the
board says so without being asked: the card carries a live state chip, here still
**Queued**. Clicking the chip opens the task's **Execution** tab:

<img src="../screenshots/tutorial/12-board-tracking.png" width="98%" alt="The board right after Run: the 'Draft the launch announcement' card carries a live 'Queued' chip.">

The Execution tab shows the attempt as it happens: the request **Leased**, the attempt
**Running** on this machine's runner, and cost tiles that read **Not measured** until
the attempt reports real numbers:

<img src="../screenshots/tutorial/13-execution-running.png" width="98%" alt="The task drawer's Execution tab mid-run: the request is Leased, Attempt #1 shows a Running badge leased just now, provenance reads 'Not yet reported', and the cost tiles read Not measured.">

## 10. See the result

When the attempt finishes, the same tab is the record of what happened: the state,
whether the model that ran **matched** the one requested (reported by the harness, not
assumed), and what it cost — measured figures labelled as measured, unmeasured ones
saying so. This run took 30 seconds and $0.05:

<img src="../screenshots/tutorial/14-execution-succeeded.png" width="98%" alt="The finished attempt: request Succeeded, Attempt #1 Succeeded, 'Matched request — ran on anthropic / claude-sonnet-5-5, as requested', model/token cost $0.05 measured, runner time 30s, runner time cost Not measured, and a 'Show events, decisions & artifacts' link; the board card behind carries a Succeeded chip.">

**Show events, decisions & artifacts** expands the attempt's full record. Its Artifacts
are what the run produced, each one click from download: the harness's own run log
(the announcement is in it), and the record of the change — `changes.patch` (empty
here: this agent only wrote text), `files.json`, the `brief.json` it was given, and the
`evidence.json` manifest that ties them together:

<img src="../screenshots/tutorial/15-artifacts.png" width="60%" alt="The Artifacts section of the finished attempt: claude-code-run.log (log, 26.7 KB), changes.patch (patch, 0 B), files.json (files, 2 B), brief.json (brief, 637 B) and evidence.json (evidence, 6.5 KB), each with a Download button.">

That's the whole loop: a task on a board with its definition of done, handed to a real
agent, tracked live, and closed with a result you can check. From here:

- [Agent Runners & Fleet Execution](agent-runners.md) — remote runners on other
  machines, selectors, budgets, decisions, verification, branch push and pull requests.
- [Quick Start — Run an item with an agent](quick-start.md#run-an-item-with-an-agent)
  — the same flow driven entirely from the CLI.
- [Workflows & Statuses](workflows.md) and [Vocabulary](vocabulary.md) — make the
  board speak your domain's language.

---

make the
  board speak your domain's language.

---

## Regenerating these screenshots

Every image on this page comes from `frontend/e2e/tutorial-assets.spec.ts`, driven
against an already-running release build (`--features embed-spa`) of this checkout
on a fresh database, with a real, signed-in `claude` on PATH. There is no `make`
target for it on purpose: the run in steps 9–10 is a real, live, billed model call.
The config file, `frontend/playwright.tutorial-assets.config.ts`, carries the exact
recipe.
