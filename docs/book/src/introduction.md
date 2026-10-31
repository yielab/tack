# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)

**A project board that can hand its own items to an AI coding agent — Claude Code,
Codex, opencode, or docket — and track the run as part of the item's history.** Self-hosted, one binary,
no cloud account.

<img src="screenshots/workflow.gif" width="98%" alt="The whole workflow, recorded: a new project pointed at a folder on this computer, a task added with two acceptance criteria, Run with agent with Claude Code, the card's chip and the Execution tab following the run to 'Finished — needs your review', Accept, what the agent did, and the files from the run">

Tack tracks work for any domain — software sprints, a kitchen renovation, thesis chapters, a
maintenance schedule — through configurable vocabulary and workflow columns, and can hand any
task to a real coding-agent run instead of just tracking it. No accounts. No cloud. No
subscriptions. One binary, one SQLite file.


## The board and the runner

Under the hood, Tack is two components, built to be one product.

> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, prepares
> a workspace for the agent, launches the coding agent you already use — Claude Code,
> Codex, opencode, or docket — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS hands work to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board: the board
> stops accepting its updates. A board that restarts does not lose a run: the runner remembers
> what it started. **One developer runs both in one process with one command.**

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/two-components-dark.svg">
  <img src="diagrams/two-components-light.svg" alt="Two components: the board (one) on the left holds workflows, timelines and history; runners (many) on the right each launch an agent — Claude Code, Codex, opencode, or docket — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
</picture>

---

## Core concepts

Six terms recur throughout this documentation:

| Term | What it means |
|------|---------------|
| **Item** | The basic unit of work. One kind of record backs everything — a task, bug, feature, epic, building, work order, assignment — and your project's vocabulary decides how it's labeled. |
| **Workflow** | The set of named status columns an item moves through (e.g. To Do → Doing → Done), each with a category and an optional WIP limit. See [Workflows & Statuses](user-guide/workflows.md). |
| **Project type** | A template chosen at creation that pre-loads a matching workflow *and* vocabulary (software, construction, legal, …). Everything stays editable afterward. |
| **Vocabulary** | Per-project label overrides that rename built-in terms to your domain — "Task" → "Work Order", "Sprint" → "Phase". The UI, CLI, and API all follow your terms. See [Vocabulary](user-guide/vocabulary.md). |
| **Runner** | A small worker process that lives where your code and credentials already are, pulls work from the board, launches a coding agent, and reports back. See [Running Agents on Tasks](user-guide/running-agents.md) and the [Administrator Guide](admin-guide/runners.md). |
| **Run** | One execution of an item by a runner. At most one attempt is active at a time, and every run stays in the item's history: what the agent did, the questions it asked, the files it produced, and what it used. |

## How this documentation is organized

The book has three guides, one for each way of meeting Tack, a learning path and the roadmap. Start
with the one that matches what you are doing today; each page links across to the others instead of
repeating them.

| Guide | For whom | You will find |
|-------|----------|---------------|
| **[User Guide](user-guide/quick-start.md)** | Anyone who works in Tack: managing projects, running agents on tasks, reviewing and merging their work | The board and its views, items, workflows, running an agent, reviewing a run, automating a project (teams, starting by itself, repeating tasks), merging, project files, the CLI |
| **[Administrator Guide](admin-guide/index.md)** | Whoever installs, secures and operates a board and its runners | Deployment, configuration and the full variable reference, tokens and network exposure, installing runners and agents, what an agent can reach, backup, restore and recovery |
| **[Developer Guide](developer/index.md)** | Contributors and integrators | Architecture, how runs, teams and automation are built, the runner contract, the project file format, the API and MCP, testing, the decisions behind it |
| **[Learning Path](developer/learning/index.md)** | Developers new to Rust, Axum or SolidJS | The stack explained with analogies |
| **[Roadmap](roadmap.md)** | Everyone | What is delivered and what is still open |

## Quick links

- [Quick Start](user-guide/quick-start.md) — up and running in five minutes
- [Running Agents on Tasks](user-guide/running-agents.md) — handing a board item to Claude Code, Codex, opencode, or docket
- [Automating a Project](user-guide/automation.md) — teams, starting tasks by itself, repeating tasks, merging
- [Administrator Guide](admin-guide/index.md) — deploy, configure, secure and operate
- [Architecture Overview](developer/index.md) — the mental model behind the codebase
- [Frontend & Design System](developer/frontend.md) — tokens, palettes, and the UI kit
- [Rust Primer](developer/learning/rust-primer.md) — start here if Rust is new to you
- [API Reference](developer/api-reference.md) — every REST endpoint

## Improving these pages

Every page has an **Edit this page** link that opens its file on GitHub. Change the page in the
same pull request as the code it describes.
