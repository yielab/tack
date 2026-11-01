# Phase 84: the screen says what needs you first, and the rest is one click away

**Status: PROPOSED 2026-11-01.** No ADR yet; the three decisions below go into one when the plan is
accepted. Front end only, except card N, which exists only if decision 1 is taken.

## The ask, in one screen

The live suite's screenshots (the docket-and-prill reruns of 2026-11-01 and the My SaaS onboarding of
2026-10-31, in the git-ignored `onboarding-lab/results/`) show that Tack already *has* every fact a person
needs to run several kinds of work through docket teams. What it does not have is an order. Every fact
is shown at the same weight, in the same sage card, in the order the code produces it:

- **The board shows the wrong state.** Items stay in Backlog while their run is Running, Finished,
  "Needs your review" or Failed, and In Progress reads 0. The run state is a 9-px pill among four other
  pills of the same size.
- **The item page buries the work.** The first screen is a property form (priority, due date,
  assignee, estimate, sprint, repeats, tags) and a bare `Choose File` input; the description, "Done when"
  and the Execution tab are below the fold. "Run again" appears twice.
- **The Execution tab reads like a log.** A run is about twelve sections in one scroll (events, steps,
  inputs, usage, ids, questions, decision token, eight files with two buttons each, the verifier's pack,
  criteria, the verify output). The one thing to do (merge, answer, run again) is at no fixed place.
- **The run dialog repeats itself.** Every team card repeats the same capabilities sentence; teams with
  no profile take as much room as the ones you can pick; the single-agent block carries raw adapter
  notes (`--resume <session-id>`, SIGKILL).
- **Agents and Automation are walls of text.** "What each agent can do" is four columns of measurement
  notes; the merge ladder L3–L5, its blockers and the ceiling are three places that say the same thing
  in fine print.
- **Two layout defects.** A long decision id or prompt line pushes the whole drawer sideways off
  screen (rerun shots 22, 23, 26), and an item title is cut at one line ("divide() returns the product
  inste").

The plan keeps every feature, control and fact that is on screen today. It changes **order, weight and
form**: one run-state language used everywhere, one primary action per place, summary before detail
before raw, and graphics where a sentence was doing a diagram's job (a ladder, a step track, a matrix, a
checklist, a meter, a tree). Nothing technical is deleted; it moves behind "Details" or "How we know".
Deferred controls stay visible and disabled with their reason (Phase 67's rule).

**What you decide:** the three questions in "Decisions", then "continúa".

## Decisions

1. **Does starting a run move the item?** Today only "When a run finishes" can move an item; nothing
   moves it on start, which is why In Progress reads 0 while three runs work. Proposed: a sibling setting
   "When a run starts" (same control, same "Do not move" option), set to the workflow's in-progress
   status for **new** projects and left at "Do not move" for existing ones. Card N. If declined, card E's
   run-state summary on the board is the only fix, and N is dropped.
2. **Priority on a card when it is the default.** Every card in both projects reads "Medium" because
   nobody set it. Proposed: a card shows priority only when it is not the project's default; the List,
   Table and item page always show it. A display preference ("Always show priority") restores today's
   card. No field is removed.
3. **Where "Propose items from a file" lives.** Today it is a raw file input above the properties of
   every item. Proposed: an item's "⋯" menu and a drop zone on the Execution tab's proposals section,
   both opening the same accept panel (card I). The file still works the same way.

## The rules every screen follows

1. **One run-state language.** Seven states, each a fixed pair of icon and tone, never colour alone:
   Queued (hollow clock, neutral), Running (spinner, accent), Needs you (hand, warning), Ready for review
   (eye, accent2), Merged / Done (check, success), Failed (cross, danger), Cancelled (slash, muted).
   "Needs a person", "Needs your review" and "Cancellation requested" map into these with their own words
   kept as the label's second line. Tokens live beside the existing `--color-*` ones in `index.css`;
   the component is `shared/ui/RunState.tsx`.
2. **One primary action per place.** The card, the item page and the run each show one filled button,
   chosen by state: Run → Answer the question → Review and merge → See why it failed → Run again.
   Everything else is secondary or in a menu.
3. **Summary, then detail, then raw.** Each run section opens with one sentence and a number; the
   detail is a fold; raw JSON, ids and adapter notes are a third level ("Technical").
4. **Graphics carry structure, text carries meaning.** A sequence is a step track, a hierarchy is a
   tree with guide lines, a comparison is a matrix, a limit is a meter, a checklist is ticks. The
   sentence stays as the accessible label.
5. **No card inside a card inside a card.** Two levels of surface at most; sections inside a surface
   use a heading and a hairline, not another rounded panel.
6. **Long values never widen a container.** Ids are middle-truncated with Copy; free text wraps
   (`overflow-wrap: anywhere`, `min-width: 0` on every flex and grid child of the drawer).

## Rounds

| Round | Cards | Gate |
|---|---|---|
| 0 | **A ∥ B** | `npx vitest run` on the merged tree; the pre-push hook |
| 1 | **C ∥ E** | same |
| 2 | **D ∥ F** | same |
| 3 | **G ∥ H** | same |
| 4 | **I ∥ J**, then **K**, (**N** if decision 1) | same; `cargo nextest` only if N |
| 5 | **M** | the live suite once, screenshots retaken |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | One run-state language | Haiku | a new `shared/ui/RunState.tsx`, its tokens in `index.css` (light and dark), `shared/ui/index.ts`, `shared/runWithAgent/RunChips.tsx` (maps to it) | — |
| B | The drawer never scrolls sideways, a title wraps, a description renders | Haiku | `shared/ui/Drawer.tsx`, `features/item-detail/ItemHeader.tsx`, `tabs/DetailsTab.tsx` (Markdown render only), `shared/runWithAgent/AttemptList.tsx` (wrap and truncate only) | — |
| C | The item page leads with what to do now | Sonnet | `features/item-detail/` (`ItemBody.tsx`, `ItemProperties.tsx`, `ItemDetailDrawer.tsx`, `ProposeFromFile.tsx` placement), `shared/runWithAgent/LatestRunSummary.tsx`, `RunWithAgentButton.tsx` | A, B |
| E | The board shows the run state and what needs you | Sonnet | `features/board/Board.tsx`, `MoveMenu.tsx`, the card markup, a new `features/board/AgentStrip.tsx` | A |
| D | A run reads top-down: verdict, then why, then raw | Sonnet | `shared/runWithAgent/` (`AttemptList.tsx`, `ExecutionTimeline.tsx`, `EventTimeline.tsx`, `TeamSteps.tsx`, `MrpPanel.tsx`, `ArtifactDownloadPanel.tsx`, `ReviewActions.tsx`, `MergeButton.tsx`) | A, B, C |
| F | The run dialog picks a team in one glance | Sonnet | `shared/runWithAgent/RunWithAgentModal.tsx`, `RunFlow.tsx`, `WhoDoesIt.tsx`, `ProfilePicker.tsx`, `RadioRow.tsx`, `Prerequisite.tsx` | A |
| G | Automation is a ladder you can see | Sonnet | `features/settings/panels/AutomationPanel.tsx`, `panels/automation/` (`Ladder.tsx`, `Ceiling.tsx`, `Merge.tsx`, `Start.tsx`, `Teams.tsx`, `parts.tsx`) | A |
| H | Agents: a matrix of what each can do, a team drawn as a team | Sonnet | `features/agents/` (`AgentsPage.tsx`, `AgentCapabilities.tsx`, `TeamsPanel.tsx`, `steps/`, `runnerFleet/` cards only) | A |
| I | The accept panel is a tree | Haiku | `shared/runWithAgent/PlanPanel.tsx`, `features/item-detail/ProposeFromFile.tsx` (drop zone) | C, D |
| J | One inbox for everything that waits on a person | Sonnet | `shared/runWithAgent/DecisionInbox.tsx`, `shared/ui/Sidebar.tsx`, the top bar's indicator in `app/Layout.tsx` | A, D |
| K | List and Overview know about runs | Haiku | `features/list/List.tsx`, `features/dashboard/Dashboard.tsx`, `computeStats.ts` | A, E |
| N | When a run starts, the item can move (decision 1) | Sonnet | `on_start_status` beside `on_finish_status` (migration, `tack-db` project repo, `tack-api` project handler, where the run starts in `tack-app`), `AutomationPanel.tsx`'s "When a run starts" fold, regenerated API files | G |
| M | The screenshots show it | Sonnet | `onboarding-lab/e2e/` capture steps (git-ignored), the user guide's screenshots it names | all |

## How every card runs

Phase 83's rules, unchanged: read only the files the card names; while working run only the tests
of what it touched (`npx vitest run <paths>`); the full suite once per round on the merged tree; at most
two cards build at once, each in its own worktree cut from `develop`; no card opens a window; each card
updates the user-guide paragraph its change makes true and adds its line to `CHANGELOG.md`'s
`[Unreleased]`; one commit per card, `Type: what is now true`, no attribution trailer. Every card keeps
existing test ids and accessible names unless it says otherwise, so the live suite's selectors hold.
Every new graphic is checked in both themes and at 200% zoom, and has a text equivalent.

## Cards

### A. One run-state language

`RunState` takes a run's state (and the optional docket words: "Needs a person", "Cancellation
requested", "no changes recorded") and renders icon + label, in three sizes: `dot` (icon only, with the
label as `aria-label` and tooltip), `chip` (today's pill), `banner` (a full-width row with the second
line). Tokens per state: `--run-<state>-ink`, `-soft`, `-line`, each pair at AA on its own soft
background, measured in a test the way `--color-warning-600` already is. `RunChips.tsx` renders through
it. No other file changes in this card; C, D, E, J and K adopt it.

### B. The drawer never scrolls sideways, a title wraps, a description renders

The drawer's body gets `min-width: 0` down its flex and grid children and `overflow-x: clip`; a decision
id, request id and attempt id are middle-truncated (`dec_6174…3d6f`) with Copy; prompt and question text
wraps with `overflow-wrap: anywhere`. The item title wraps to three lines and then clamps, with the full
title as tooltip. A description is stored as the rich-text editor's HTML, but one that arrived through
a project file, `tack import` or MCP is Markdown and shows its `**` (seen in the My SaaS run): a
description with no HTML tags renders as sanitized Markdown, and becomes the editor's HTML on its first
edit. The card picks the smallest maintained Markdown renderer under the version-floor rule and names
it in its commit. A test renders a 200-character unbroken id in the drawer and asserts no horizontal
overflow.

### C. The item page leads with what to do now

New order, the same in the drawer and on the page:

1. Header: type, short id, title (B), and the workflow status as the existing segmented control.
2. **Now** panel, one per item: the latest run's `RunState` banner, one sentence ("The implementer is
   waiting for you to approve a shell command"), the usage meter (used / limit as a bar, not a
   sentence), and the one primary action (rule 2) with at most two secondary ones (Stop, Follow live).
   An item with no run shows "Run with agent" and, under it, who would do it ("Code agent · team
   saas-code"), taken from `WhoDoesIt`.
3. Tabs: Overview (description, Done when), Execution, Activity, Dependencies, Files, Fields. Execution
   carries the latest run's state dot in its tab label.
4. Properties: in the drawer, a compact two-column definition list (label left, value right, edited in
   place) in a "Details" fold that remembers whether it was open; on the page, the existing right column.
   Every property that is there today stays.

"Run again" exists once (in the Now panel). "Propose items from a file" moves to the item's "⋯" menu
(decision 3). The Automation row ("By hand: this project doesn't start tasks by itself") moves into the
Now panel's footer as one muted line with its (?) help.

### E. The board shows the run state and what needs you

- **Card hierarchy.** Title first and largest; one meta line below it: `RunState` dot + label, team as a
  small avatar with its name, then priority only if not the default (decision 2), then the type chip.
  The short id moves to the card's hover and its menu. "Needs you" and "Failed" give the card a 3-px
  left rail in the state's ink. The ▶ button becomes an icon button with the label "Run with agent"
  (tooltip and `aria-label`) and is hidden on a card with a live run (the Now action replaces it).
- **Column header.** Beside the count, one stacked mini bar of the run states of the column's cards
  (e.g. 2 running, 1 needs you), each segment a filter.
- **Agent strip.** Under the board title, one row of counters: Queued · Running · Needs you · Ready for
  review · Failed, each a toggle filter on the board, with "Waiting for you" (J) as the first and
  accented one. Hidden when the project has never run anything.
- Until decision 1 lands, a card whose run is live while its column is a "not started" column shows
  the strip and rail so the board no longer reads as idle.

### D. A run reads top-down: verdict, then why, then raw

The Execution tab, per run:

1. **Verdict header.** Run number, `RunState` banner, the verifier's recommendation and risk as two
   chips, tokens (meter), duration, and the merge state ("Not merged: no branch was pushed") with the one
   primary action (Merge / Review / Answer / Run again). A folder team says "Works in a folder: nothing to
   merge" here, not further down.
2. **Needs you** (only when it does): the question in docket's words, the command or call it asks
   about in a code block, Approve / Deny, and the decision-token field **inside this section only** (the
   empty "Your decision token" box no longer shows on runs that asked nothing). "The agent has asked
   nothing yet" becomes one muted line in the Activity fold.
3. **Checks.** The criteria as a checklist: ✓ passed, ✗ failed, ◐ needs a person, with kind and
   evidence as secondary text; "Done when" ticks and the verifier's criteria in the same list with their
   source named; the verify command and its output in a fold.
4. **Changes.** The patch through `DiffView` (already there), with files changed and lines ±.
5. **Steps.** The team's steps as a horizontal step track (one node per member: role, model, verdict,
   tokens), the events timeline in a fold under it.
6. **Files.** One compact table (name, kind, size, View · Download as icon buttons), not eight cards.
7. **Technical.** Request and attempt ids, prompts used, what the agent read, raw log, the signature
   fingerprint.

Order is fixed; every section that exists today is still there. A second run of the same item is a row
above with its own state dot, opened one at a time.

### F. The run dialog picks a team in one glance

- **Who does it** becomes a compact list grouped by team: one row per profile (radio), the team's name,
  its members as small role chips, where it works (a branch of the repo / a folder) as an icon, and the
  capabilities as a row of five icons (stop, resume, answer, propose, usage) where only a missing one is
  drawn struck through with its reason in the tooltip. The shared capabilities sentence is said once, at
  the top, not per team.
- Teams with no profile collapse into one disabled row: "3 teams have no profile yet → Agents" (they stay
  visible, disabled with the reason).
- Single agents (Claude Code, Codex, OpenCode) are a second group with the same row shape; their
  adapter notes move to the capability icon's tooltip.
- The profile chosen by the item's role is preselected and says "Chosen by the role Implementer".
- **What will happen** summary, one line each with an icon: where (branch / folder), prepare command
  (or "nothing is installed"), checks ("2 Done when · verifier on"), sandbox, network. The toggles stay.
- A sticky footer: token limit, Cancel, **Start run**. "Advanced for this run" stays as it is.

### G. Automation is a ladder you can see

The top of the Automation tab is one graphic: the rungs L1–L5 as a horizontal ladder, the project's
current rung filled, its ceiling marked, and each rung above the current one showing what blocks it (the
three ✗ lines of today's Ceiling, each with its fix as a link: "Turn on GitHub polling", "Add the public
key", "Run saas-code lights-out"). Below it, the sections in the order a person sets them up, with a
sticky section index on wide screens: **Who** (agent, model, default profile, teams by role), **When**
(start, when a run starts — N — and finishes), **Merge** (rungs, domains, stages, key, retries), **Limits**
(tokens per day, merges per day, items waiting, provisional tokens per run, with the 2 000 000 default
written as a number with its source). Ceiling's separate section folds into the ladder; its text is the
ladder's accessible description. Every field that exists stays.

### H. Agents: a matrix of what each can do, a team drawn as a team

- "What each agent can do" becomes a matrix: agents as columns, the five capabilities as rows, each cell
  ✓ / ✗ / — with the measurement note behind a "How we know" disclosure in the cell. The text is
  unchanged, only moved.
- The sign-in step's failures ("`opencode debug config` exited with status 1") become a warning row with
  the command to run in a code block and the raw message in a fold.
- A team card: name and "Run by docket", shape as a badge, where it works as an icon, members as rows
  with a role chip, model, and two icons (reads only / changes the project; can propose items), steps as
  a small pipeline (`hold · waits` as a node), work now as two counters, the record set with its row
  count, and the profiles that use it as links.

### I. The accept panel is a tree

The proposed items as a tree with indentation guides; each row: checkbox, title (editable), type and
role as compact selects, profile as a chip, and "Waits on" as chips of the other rows' titles (picked
from a menu), never bare numbers. A header line: "3 items · 1 sub-item · 1 dependency", Select all /
none, and the one primary action "Create 3 items". A file dropped on the Execution tab's proposals
section opens the same panel.

### J. One inbox for everything that waits on a person

"Waiting for you" stays in the sidebar while empty (with a 0 and its meaning on hover), and lists
three groups: questions to answer, runs ready for review, runs that failed since you last looked. Each
row: `RunState` dot, item title, project, age, and the primary action inline. The top bar gets the same
count as a badge. The decision token is asked here once per session, not per run.

### K. List and Overview know about runs

The List gets a run-state column (`RunState` dot + label) and a filter on it; the Overview gets one row
of tiles from data the board already loads: runs this week by outcome (a stacked bar), items that need
you, and tokens counted per team. Factory metrics is linked from that row, not merged into it.

### N. When a run starts, the item can move (decision 1 only)

`on_start_status` beside `on_finish_status`: a nullable column, the same validation, applied where a run
moves to Running. New projects get the workflow's first in-progress status; existing rows stay null.
Settings shows it as "When a run starts" next to "When a run finishes".

### M. The screenshots show it

The live suite's capture steps are kept and their files renamed only where a screen moved; the shots of
the item page, Execution tab, run dialog, Automation, Agents, accept panel and board are retaken against
the rounds' merged tree, and the guide screenshots that show those screens are replaced.

## Left out, and why

- **A new colour palette or typeface.** The token architecture and the brand stay; this plan changes
  weight and order. A darker surface for the drawer is in scope only where rule 5 needs it.
- **A run-state swimlane board.** Card E's strip and filters give the same answer without a second
  layout to keep.
- **Removing any control, setting or fact.** Every one stays; what moves is listed in its card.
