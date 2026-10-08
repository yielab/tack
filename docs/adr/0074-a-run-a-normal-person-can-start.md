# ADR 0074: a run a normal person can start

**Status: PROPOSED 2026-10-07, waiting on the maintainer.** Amends ADR 0072 (onboarding), ADR
0069 (the brief's place in the UI) and ADR 0071 (where a run's work ends up). The plan is
`docs/plans/phase-67.md`.

**Decide:** that Tack separates three things it mixes today — *this computer can run agents*,
*this project has code here and the agent works on it this way*, and *run this task now* —
and that a person who knows nothing about runners, harnesses or model providers can install
Tack, create a project with a folder, and press **Run with agent** with no other setting.
Everything else stays, visibly, under **Advanced**.

**Why now:** manual QA on 2026-10-07 (beta.10) produced fourteen findings on the run flow. Three
lose or hide data: a run that wrote a file reported **Succeeded** while the file was deleted
with its workspace; a run that failed before starting showed no reason, although the reason was
in the database; the provider the page let the user type was rejected by the agent at launch.
The rest are the same confusion ADR 0072 named a week earlier, still there after its first
task shipped: the dialog is a form in the runner's vocabulary, settings live in three places,
nothing persists, and "Auto" is offered everywhere and works nowhere.

**If you do nothing:** every first run on a default install either never schedules (Auto), fails
at launch with no visible reason (typed provider), or finishes green with its work discarded
(branch-name base revision). A person who does not read the database concludes the product
does not work.

## The decisions, in short

| # | Decision | What a person sees |
|---|---|---|
| 1 | **Three layers, never mixed.** Machine (`/agents`: on/off, installed, signed in, optional gateway key). Project (Settings → **Automation**: where the code is, how the agent works on it, what happens when a run finishes, default profile). Task (the run dialog: what it reads, two switches, Advanced). A value lives at the highest layer that can own it and is shown read-only below with "Change". | One page per question. Nothing asked twice. |
| 2 | **The project knows where its code is, from creation.** "Does this project have code?" — an existing folder here, a new folder Tack creates (with `git init`), or none. That answer sets how the agent works: a git folder → **on a new branch** of your repo; a folder without git → **in the folder**; a URL → clones (Advanced, for other computers). | Pick a folder once. Never type a remote URL. |
| 3 | **The agent works where the code lives.** Three runner modes: `local_branch` (a git worktree of your repo on `tack/<task>`; the branch stays in your repo, your checkout is untouched), `in_place` (your folder, directly, never deleted), `clone` (today's, kept for remote runners). Push is off by default, a per-project switch. | "Your work is on branch tack/1b816e in ~/Sites/prill (1 file)". |
| 4 | **No model, no provider, by default.** The agent's own model (what `claude` uses in a terminal) is the default; the scheduler accepts it when the agent declares it can run without one. A specific model is a list per agent, not typed. The provider is never shown: it is the agent's own, or the gateway when a key is set. | One radio: "The agent's default (recommended)". |
| 5 | **Four built-in profiles, with their tools.** Implementer (default), Reviewer, Researcher, Planner. Pills on the task, each with a tooltip saying what it may do. Editable in a form, never JSON. The Planner only plans: it proposes subtasks that are created in Tack only when a person accepts them. | "Implementer · reads, edits, runs commands". |
| 6 | **A task is written like a professional task.** Description (the story), acceptance criteria (a checklist in plain sentences). What only the agent needs — automatic checks, limits, risk — is a collapsed "For the agent" block. The project carries its definition of done once. The dialog shows the real text the agent will read. ADR 0069's brief stays as storage; the word "Brief" leaves the UI. | Jira-shaped fields. No "brief". |
| 7 | **A finished run needs a person.** A run that changed files sets **Needs your review** on the task — a flag distinct from any workflow column, cleared by Accept/Reject on the run. Optionally the task also moves to a column the project chooses. The run card leads with the outcome and the tools to check it: open the diff, open the folder, copy the branch, read the result, read the log. **Succeeded** is green only when changes were captured; amber when none were or they could not be read; failed runs lead with why. | "Finished — needs your review · Open diff". |
| 8 | **Nothing resets.** Project settings persist on the project; per-task changes persist on the task; only the idempotency key is fresh per open. Every field validates where it is typed (folder exists, is git, branch exists; model is on the agent's list) with a sentence and a link, not a disabled button. | Re-run without retyping. |
| 9 | **Tack's words, not the runner's.** Agent (not harness), profile (not agent profile), this computer (not runner), folder and branch (not remote and base revision), "Ask before each action" (not approvals), "Why it stopped" (not terminal reason), "Questions from the agent" (not decision inbox). Every field has a `(?)` with one sentence and a link into the book; a help button opens the book page for the current screen. | Plain words everywhere. |

## What this changes in earlier ADRs

- **ADR 0072**, amendment of 2026-10-02, said no Setup tab is built because settings already
  exist in three places. Reversed: those three places are the problem. The project gets one
  **Automation** tab; the global page keeps only what is per-computer. Decision 4 (the dialog is
  a pre-flight) is kept and finally built as written.
- **ADR 0069**: the brief remains the stored entity and the contract the runner receives. Its
  `manual` criteria are the task's acceptance checklist; its other kinds, constraints and risk
  are "For the agent". The definition of done moves to the project. No wire change.
- **ADR 0071**: "nothing of an attempt survives as a local branch" (phase-66, Decided) is
  reversed for `local_branch` mode, which exists so that something does. Push stays off by
  default and stays best-effort. The verifier boundary is untouched.

## Full reasoning — the fourteen findings (2026-10-07, `develop` at `c859729`)

Each was read in the tree, not inferred from the screenshot; the run that lost its file was
read from the user's own `tack.db` and runner staging directory.

| # | Finding | Where |
|---|---|---|
| 1 | Auto model offered in three pickers; the scheduler rejects every Auto request; the adapter already declares it needs no model | `select.rs:156`; `claude_code.rs:27` `ModelSelection::Optional`; `local_process.rs:406` |
| 2 | Provider is free text, validated nowhere, rejected at spawn with `harness_rejected`; the UI shows nothing | `claude_code.rs:196`; user's attempt `att_4ad4867d` |
| 3 | Default model edited on two pages with different radio labels; fix link points at the project page from the global one | `ModelDefaultStep.tsx`, `AgentsPanel.tsx`, `RunFlow.tsx:157` |
| 4 | "Harness" and "agent profile" are runner words; the profile is required, created with hidden text, edited as JSON | `RunFlow.tsx:99-116`; `AgentProfilesPanel.tsx` |
| 5 | Remote is required per run, labelled as hosted; a local path works; `kind` is typed and never read; always a fresh clone; no project setting | `RunWithAgentModal.tsx:180`; `RunFlow.tsx:191`; `client.rs:195`; `git.rs:235` |
| 6 | The test run needs remote, provider and model typed to print a model, and creates a board item | `TestRunStep.tsx:73,79` |
| 7 | "Present, unverified" is a login badge set by a run this tab watched, lost on reload | `ProviderStep.tsx:11-17` |
| 8 | Native selects render in the OS light scheme: `color-scheme` is never set | `index.css` (media query only, `:611`) |
| 9 | Description, brief and acceptance are three words for what the agent reads | `executions.rs:562-583`; `BriefTab.tsx` |
| 10 | The dialog is still a form; the form resets on every open by design | `RunWithAgentModal.tsx:75`; ADR 0072 decision 4 |
| 11 | `terminal_reason` is returned by the API, untyped, and never rendered | `executions.rs:441`; `openapi.json:6533`; no reader in `shared/runWithAgent/` |
| 12 | Evidence diffs against the base revision **by name**; after `git init` + fetch the name resolves to nothing; every test uses a sha | `git.rs:452-466` vs `resolve_revision` `:313`; evidence `captured:false` on the user's run |
| 13 | Succeeded = exit 0; with evidence failed and push off, the work is deleted with the workspace | `engine.rs:696-720`; `config.rs:149`; `workspaces/` empty after the run |
| 14 | The timeline prints the raw terminal JSON; the agent's result text is buried | `EventTimeline.tsx:18` |

Why the fixes are shaped as they are:

- **Local branch over in-place as the git default.** In-place is what people expect from
  running the CLI themselves, but it works on top of whatever is half-done in the checkout and
  gives the agent the user's index. A worktree on a new branch costs nothing (no network, shared
  objects), leaves the result in the user's own repo, and keeps the checkout intact. In-place is
  one click away and is the only option without git.
- **Trusting the adapter's `Optional`.** Each adapter already states whether its CLI needs a
  model and what provider it uses natively; the scheduler ignores both and refuses. Reading the
  attestation is smaller than keeping three dead radios honest.
- **A flag, not a column, for "needs your review".** Workflow columns are the user's vocabulary
  (`In Review` may mean a human's review of a human's work). The run's state must not borrow it.
  A flag derived from the latest attempt and its verdict cannot drift from the facts, and the
  optional column move keeps the board habit for those who want it.
- **The Planner proposes, a person creates.** A plan is an artifact (`plan-v1`) the run leaves;
  subtasks come from a button, with the proposed title, description and criteria editable
  before creation. No agent writes to the board.

## Rejected

- *Keep the dialog a form and add tooltips.* Tooltips on fourteen fields do not fix that the
  fields are at the wrong layer.
- *Push by default so the work is never lost.* Push needs credentials and a remote; the local
  branch already keeps the work with neither.
- *A free-text model with validation.* The lists are short and measured; "Other…" under
  Advanced covers the rest.

## Questions for the maintainer

None open: the default mode, the tab name (Automation), the review flag, the default profile
(Implementer, pills with tooltips) and the acceptance/for-the-agent split were decided on
2026-10-07 in conversation.

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*
