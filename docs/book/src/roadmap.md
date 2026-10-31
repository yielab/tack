# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/boards/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/boards).

**Tack is delivered through Phase 78.** A project is planned and tracked on a board, a list, a
table, a calendar or a timeline, with or without agents. A task can be handed to an agent on
your computer (Claude Code, Codex, opencode or docket), which brings its own sign-in and model;
Tack holds no model key. The run's work comes back as evidence to review, an independent
verifier can judge it, and Tack can push the branch, open the pull request and merge it: by a
button, when the task moves to Done, or by itself at Level 4 or 5 under the verifier's signed
verdict. A project can start its tasks by itself, send each role's tasks to a long-lived docket
team, repeat tasks, and travel whole in one file.

**Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. The next is
`v0.2.0-beta.1`, which carries Phases 66 to 82: the version is bumped and the changelog section
is written, and it waits for the review and merge of the Phase 79 branch and for the maintainer to
tag it. The `v0.1.0-beta.10` tag (2026-10-14) never had a release and is not re-cut; its changes
ship in `v0.2.0-beta.1`.

**Where this is going.** Phases 79 and 80 give the runner a crate of its own and make
a run that waits on a person wait without a clock. Phase 81 fixes what an audit of that tree
found. Tack keeps the same three neighbours it never contains: an agent runtime such as docket,
which runs the work; an independent verifier such as prill, which judges it; and GitHub. Each
still works alone, and without a verifier a project stays at Level 3.

---

## Open

Only a card of a phase the maintainer has opened is listed here, with its detail in the plan or
the section named beside it. What is not a card is in "Cancelled" below.

- **[Phase 81](#phase-81--what-the-audit-found)**, what the 2026-10-26 audit found: every card (B, C, D, S, R, A, H, E, F, I, J and K)
  is built and merged on the Phase 79 branch. What is left is the review and merge of Phases 79, 80 and 81 into `develop`.
- **[Phase 82](#phase-82--the-task-goes-to-docket-as-fields-and-a-run-says-which-prompts-made-it)**,
  the task goes to docket as fields, and a run says which prompts made it; widened 2026-10-30 with
  what docket's Phases 55 and 56 ask of Tack: all six cards (A, B, E, F, C, D) are built and merged on the
  Phase 79 branch. What is left is the review and merge of that branch. It reads what docket shipped and
  changes nothing in docket.

## Future, not planned

Phase 76 is the only one. It has no date, no owner and no card in progress; it stays written so
it can be opened later.

- **[Phase 76](#phase-76--the-project-file-is-kept-in-sync)**, the project file kept in sync: a
  key that survives a rename, an import that removes what the file removed and keeps what the
  board changed, the cost of every removal shown first, and a revision to go back to. Until it is
  opened, the project file stays what Phase 73 made it: an import that creates and updates by
  name and never deletes.

## Cancelled

Cancelled on 2026-10-27 because none of these was approved as a card. They are not scheduled and
do not wait on anything; each can come back as a card of its own.

- Publishing the release, the publish list, the installs on macOS and Windows, the
  code-signing decision and the GitHub and workstation items of Phases 65 and 69 (Wave 0). The
  release is the maintainer's own step, written in `docs/LAUNCH-CHECKLIST.md`.
- The live proof of My SaaS's code card through a team with a real install, which needs a paid
  model and docket's cooperation.
- Directives learnt from a person's corrections (ADR 0020, decision 6): Tack depends on no
  verifier, and the feature waited on a contract from one. The reasons a person gives stay
  stored (ADR 0013, decision 7).
- Measuring docket's polling, and the retakes of `agents.png`, `agents-flow.gif`, the two desktop
  screenshots and `recovery-demo.gif`.
- Four limits that stay as limits, each already shown in the product with its reason: "Apply the
  repository's teams" is disabled; only docket stops a run at its token limit; the tokens a
  verifier spends count whole, cache included; only docket reports a running agent's steps on its
  timeline.

---

## Phase 82 — the task goes to docket as fields, and a run says which prompts made it

**Status:** cards A to F built on the Phase 79 branch (2026-10-30). Proposed 2026-10-29, from the prompt audit of 2026-10-28 and what docket shipped the next
day (its Phases 52 to 54: a prompt is a versioned file with a hash, a step closes with a handoff, a
task can arrive as fields, every step records which prompts composed it, and the Anthropic provider
reports its cache reads). docket's API number is unchanged and every addition is optional, so
nothing here waits on docket or on the verifier. Widened 2026-10-30 with docket's Phase 55 (a team
works outward: in a folder instead of a repository, with steps that wait, and actions that leave the
machine only after a person approves them) and its Phase 56 (a declared service, a team's own
records, one judge instead of a critic and a tester, the team shapes renamed). **ADRs:**
[0030](https://github.com/yielab/tack/blob/develop/docs/adr/0030-the-task-goes-to-docket-as-fields-and-a-run-says-which-prompts-made-it.md),
[0031](https://github.com/yielab/tack/blob/develop/docs/adr/0031-a-docket-team-may-work-outside-the-repository-and-what-leaves-is-seen-before-it-leaves.md).
**Plan:** [`docs/plans/phase-82.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-82.md).

**The local suite of the three (2026-10-30).** Run on the local model against docket `57ec0546`, prill
`6b95423` and Tack `21f81e08` with the checklist fix: 14 of 15 tests pass. The fifteenth, the answered
lights-out run, fails on the model's own lead step (it repeats itself or exhausts its iterations; seen
twice), so the run-again-keeps-the-answer assertion at its end has not run live. Every hop docket ran
carried its prompt manifest (5 of 5, then 1 of 1). `--check` finds no drift between docket's answers, the
captures and the fake. The captures are of docket `115e9027`.

**What changes for a person.** A docket run no longer sends the card twice (as prose and as a
brief): the title, the checks, the constraints, the risk, the kind, the paths the checks name and
the done-when go as fields, and the agent reads them once in a fixed order. Pressing Run again
carries the answers a person already gave, so the same question is not asked twice. When a run
settles, the Execution tab shows, per step, which prompts ran at which version, what the agent
assumed and what it asked, in words. The verifier's evidence names the model's family and the steps
that produced the change. Tack still writes no prompt for a team: a team's prompts are edited where
docket keeps them, and Tack reads them back by name, version and hash. An item given to a team that
works in a folder ends Succeeded with what the team did, nothing to merge, and the records it wrote
shown on the Execution tab; the Teams panel says which teams work in a folder and what they keep. A
step that waits reads as a wait with its time, never as a question, and the run's time limit does
not run meanwhile. Before approving an action that leaves the team (an email, a post, a write to a
service), the person sees the whole call; Tack keeps no copy once it is answered. Tack creates a
team without naming a shape, so docket's renaming does not break it.

| Id | Card | Model | Waits on |
|---|---|---|---|
| A | The item goes to docket as fields: a typed task document in the protocol, the prose renderer kept for the other agents, `task` sent instead of a brief in prose, previous answers on a rerun, the fake refusing what docket refuses | Sonnet | — |
| B | A run keeps what each step was made of, and the verifier gets it: the hop record read at settle into one `steps` value (prompts by layer, verdict, assumptions, questions), the family and a trajectory in the attempt evidence, the fake writing the same shape | Sonnet | — |
| E | A wait is not a question, and an action that leaves is seen before it is approved: a wait as a timeline line with its time, the whole call of an outward approval shown while pending and erased when it settles | Sonnet | — |
| F | A team may work in a folder, and a run keeps the records it wrote: a folder team's run succeeds with its summary and no merge, the rows the task wrote kept beside the steps, the Teams panel's two lines, no shape named on create | Sonnet | B |
| C | The Execution tab says which prompts ran, what the agent asked and what the team recorded: a section per run, a row per step, a table per record set, a line on the run card | Haiku | B, F |
| D | The local suite proves it against the new docket: fixtures recaptured, three tests assert the fields and the manifest, the onboarding skill on docket's new home layout with one judge and its shapes | Sonnet | A, B, C, E, F |

Rounds: **1** = A beside B; **2** = E beside F; **3** = C beside D (D's local run after C merges).
The full suites run once per round on the merged tree; the local suite of the three runs inside D,
at its end.

**Left out:** Tack writing prompt files or a team overlay, versioning a profile's instructions in
Tack, a model class per item type, reading docket's metrics, claims to the verifier, a second
timeline line per step, and a measurement card (ADR 0030); configuring services, secrets, quotas or
a team's network from Tack, showing routines, task variables, a "Resume now" for a wait, reading every
record of a team, and creating a folder team from Tack (ADR 0031): each with its reason in the ADR.
The measurements the audit asks for become possible with B and are the maintainer's run.

---

## Phase 78 — one pass over what is still open

**Status:** Rounds 1 to 4 are in `develop` (2026-10-19): every task, and the gate once on the merged
tree. The recordings that need a paid model were retaken on 2026-10-20: the tutorial's screenshots,
`workflow.gif`, `hero.gif` and `attempt.png`. The retakes that were left (`agents.png`, `agents-flow.gif`, the desktop shots and
`recovery-demo.gif`) are cancelled. One small defect stays, a retried request's fingerprint that still
includes the composed instructions (so a title edit answers 409): card E of Phase 81 fixes it. It gathers
what the first runs from the desktop app found, the board screen audit, the gaps of earlier phases
that are a fix, and the item screen's new design, and orders them so each screen and each module is
changed by one task, once. No ADR: nothing here changes a decision; the one model change is a tick
on a manual check.

| Id | Task | Owns | Waits on |
|---|---|---|---|
| R | **The runner prepares only what is needed.** The folder it installed is excluded from the agent's change only when git does not already ignore it: today a project whose `.gitignore` lists `node_modules`, almost every one, ends every run "changes could not be read", because `git add` refuses the exclusion. Dependencies are installed only when the agent can run commands: a profile whose tools cannot (Read, Grep, Glob, WebFetch, WebSearch) with no check installs nothing, and the request says why; for docket its pipeline decides. The sandbox stays on for every run. | the runner's prepare and evidence code, the request snapshot | — |
| D | **The desktop app finds, stops and names what it starts.** Every agent is looked for in the same places, so opencode under Homebrew is found when the app opens from the desktop menu, whose `PATH` leaves out per-user folders. Ending `tack-desktop` with a signal stops the server it started, so the next app does not attach to an orphan. The app's runner keeps this computer's runner identity instead of enrolling anew, and is named "This computer (host name)"; a runner on another machine is named after that machine. | `tack-desktop`, agent discovery, the runner's enrollment and state folder | — |
| W | **Where a run waits, and why.** The runner list carries each runner's last heartbeat. A run queued for a runner that does not answer says "The computer this run asked for has not answered since" and the time. Retrying a request after its task's type or priority changed no longer answers 409: the task's metadata leaves the request's fingerprint. A run with no limit, in a project with a daily limit, is told it gets what is left of the day. | the execution and runner handlers, the queue's reason, the limit sentence | — |
| P | **A profile says what it delivers.** Each profile carries one line: what it delivers (a change on a branch, a document in a folder, an answer) and on which agent; whether it changes files, runs commands or uses the network is read from its tools, not typed. The profile form also sets its team, which today only the project file or the CLI can. | the profile model and its migration, profile handlers, the project file, the profiles panel | — |
| B | **The board scrolls and works by keyboard.** Reproduced in a browser first. The page that holds the five views gets a height chain, so each column scrolls inside itself and the horizontal bar sits at the bottom of the screen. Scrollbars use `scrollbar-color` and `scrollbar-width` with a thumb of 3:1 in every palette, light and dark, and a stable gutter. Containers that must not scroll are `overflow: clip`, and a column keeps its scroll (`overscroll-behavior: contain`). Each card has a "Move to…" menu, reachable by keyboard and announced. Each column is a named region the keyboard reaches. "Add item" is 24 px and has a visible name. | the work layout, the board and its columns, the scrollbar tokens | — |
| I | **The item has a page of its own.** The drawer's body becomes one component shown two ways: in the drawer, as today, and as a page, `/projects/:id/items/:itemId`, with a breadcrumb, a main column and a side column. The drawer gets "Expand" and the page "Copy link". The header keeps the type, id, creation date, title and status. The fields (priority, due date, assignee, now editable, estimate, sprint, repeats, last update, tags, automation, what it blocks) become one properties list: the side column on the page, the same grid as today in the drawer. Tabs show a count where the number is already loaded. | the item detail's drawer and header, a properties component, `Drawer`, `Tabs`, `Breadcrumb`, the routes | — |
| O | **Overview, and "Done when" can be ticked.** Details is renamed Overview, and `?tab=details` still opens it. The description is edited in place, with "Click to edit". The manual checks become a "Done when" list a person ticks, with its progress ("1 of 3"). "For the agent" is a folded card. The manual criterion gains `checked`, false by default, so older items and files read as today; the brief and the verifier ignore it. | the details tab, the manual criterion in core and the items handler, the regenerated schema | — |
| Q | **The Run dialog follows the profile.** Picking a profile picks its agent, and says so when it replaces the one the task remembered. The project's profiles are listed first, each with its "delivers" line, and the built-in ones apart. A profile that installs nothing says "No install: this profile runs no commands". "Machine or group" names this computer and the others by machine. A runner that does not answer is shown disabled, with since when. With one live runner and no group the field is one fixed line, "Runs on this computer". | the Run dialog and its limit note | R, D, W, P |
| C | **The run card.** The latest-run summary becomes one card whose colour and actions follow the run. Running: its last steps, its tokens against the limit, "Follow live" and "Stop". Needs review: the opening of the report, duration, steps and files changed, "Accept", "Reject…" and "See what it did". Failed and merged each get their own state. Queued: W's reason and "Run on another computer". A run that changed no file shows the agent's answer as what it delivered and offers Accept, not Merge. Merge, when it cannot run, says why in plain words ("No branch was pushed"), disabled, never hidden. The review actions leave the attempt list for a component of their own. The card sits in the page's side column and at the top of the drawer. | the latest-run summary, the merge button, the review actions | W, P, I |
| E | **The Execution tab.** Chips pick the run, with "Run again". Above the run sits the review banner with an optional note. "What the agent concluded" shows the report as written, with "Read full report". "What the agent did" groups the steps by tool, targets as chips, or shows the timeline. Paths are shown inside the project, not the runner's workspace. On the page, a side column holds the run, what the agent got, the usage folded (tokens and the share of the limit), and the ids with the raw log. | the execution timeline, the attempt list, the run log parser | C |
| Z | **Integration, once.** The one gate on the merged tree. `e2e/a11y.spec.ts` learns the board's checks: a column of thirty cards scrolls to its last card, the horizontal bar is inside the screen, and every scrollable region takes keyboard focus. One end-to-end test opens the item page. The screenshots are retaken once, after every screen has changed: `automation.png` and `factory-metrics.png` (both older than the four-rung Automation screen and the cost cards), the item page, the Run dialog, and the tutorial shots that show them. | the gate, e2e, `docs/screenshots/` | all |

Rounds: **1** = R, D, W, P, B, I, O, which share no file; **2** = Q and C; **3** = E; **4** = Z.

Each task writes its own guide paragraph and changelog line; there is no separate docs task. While a
round is building, a task runs only the tests of what it touched: its crate's `cargo nextest run -p`
and `npx vitest run` on its own files. It fixes the tests its change breaks, and adds a test for a
behaviour that changes, never for a restyle or a component that moved. The full suites, the
end-to-end tests and `.githooks/pre-push` run once, in Round 4.

Left out because nothing records it: who created an item and who last changed it (the page shows
the dates), a "short answer" cut out of the agent's report (the report is shown as written), and the
time each group of steps took (a step has no time).

---

## Phase 79 — the crates say what depends on what

**Status:** built 2026-10-26, from an architecture audit of `develop`; the full suite and the
pre-push gate pass; not yet in `develop`. **ADR:** 0028, accepted 2026-10-26.
**Plan:** [`docs/plans/phase-79.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-79.md).

The runner, the process that holds credentials, compiles SQLite because the types it shares with
the board sit in a crate that depends on the database. An attempt's state is written in three
places. About 200 SQL queries sit in the HTTP crate beside the repository made to hold them. The
phase moves the runner-v1 contract into a crate of its own, puts every SQL line in `tack-db`,
puts the rules of a run into use cases that take the store and a clock, puts every background
loop in one crate as a function the binary schedules, and adds a script to the pre-push hook
that fails on any crate edge the plan does not allow. Nothing on the wire changes.

There are no store traits and no in-memory fakes: every test already runs on in-memory SQLite in
milliseconds, and a fake would hide the transactions and fencing the store exists for.

What is left is the review and merge of the branch, the last step of Phase 81. The two points §8 of
the plan left open are settled: the dead-code allowances on test fixtures are gone, and the `Host`
trait is split into small ports by card J of Phase 81.

---

## Phase 80 — a run that waits on a person, and one record for every wait

**Status:** built 2026-10-26, on top of Phase 79; not yet in `develop`. **Plan:**
[`docs/plans/phase-80.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-80.md).

A docket run parked on a question was cancelled when the attempt's own time passed, and the run read
as a timeout. docket then closed its own phase (2026-10-26, version 0.2.0-beta.6, unreleased): every
wait for a person is one escalation record, one route answers it, and the old routes answer 404.
That made the adapter fail for every question, so the whole phase was built against the real docket:

- the clock of a task that waits on a person stops; it ends only at the question's own expiry
  (`not_answered`) or when docket expires it;
- one reader of escalations and one answer route; the three question shapes, the two answer paths and
  the inbox reads are gone, and the probe asks for the escalation record instead of a version number;
- the autonomy level goes on each task, never above the team's ceiling; a task docket ends as
  `needs_human` has its own outcome, and the run card offers "Run again at L3";
- every escalation event is a line on the timeline, and the factory metrics count what docket decided
  alone beside, never inside, what a person answered.

The polling measurement stays deferred (nothing can replace polling while Tack has no inbound
endpoint) and the A2A client was cut (the one answer route removes its trigger). The maintainer
decided that lights-out is L5 and docket's own, so Tack sends an `autonomy` level only when a
person asks for one ("Run again at L3"), never from the project's merge level, and that a
question with no `expiresAt` holds its run until it is answered, resolved or stopped. Card G, the
contract guard, is closed: docket removed the old shapes, Tack's code and fixtures no longer have them,
and the live contract check is card A of Phase 81.

---

## Phase 81 — what the audit found

**Status:** proposed 2026-10-26, from an audit of the tree with Phases 79 and 80 built (tests,
structure, code design, screens, and the seams with docket and prill). Every card is built and merged on the
Phase 79 branch, 2026-10-27: Round 1 (B, C, D, S, R, A, H), Round 2 (E, F) and Round 3 (I, J, K). The full
suites ran on the merged tree: `pre-push`, 1613 Rust tests, 821 Vitest tests, Playwright on Chromium (68) and the
smoke script all pass. What is left is the review and merge into `develop`. No ADR. **Plan:**
[`docs/plans/phase-81.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-81.md).

**The rule it keeps:** CI never needs docket or prill. CI tests Tack's code with the fake docket,
the fake prill and captured fixtures; everything that talks to the real programs is one local
command, the local `onboarding-lab/e2e/docket-prill-e2e.sh`, with a `--check` mode that diffs the captures and prill's
contracts. The live suite now has fifteen tests and asks docket for its escalation record before it
builds. On 2026-10-27 it passed fourteen against docket `52c22f89` and prill `6b95423`; the fifteenth, the answer that
completes a lights-out run, fails when the local model's lead step repeats itself, which is the model and not Tack.
`--check` passes, and the same result held on the tree with every card merged. The run found five Tack defects, all fixed in the same round.

| Id | Card | Model | Waits on |
|---|---|---|---|
| A | The local suite of the three can pass and says what it proved | Sonnet | C |
| B | Background work has one owner and stops: one loop per job, all stopped on shutdown, one purge of run events, and the board move and rework inside the completion's transaction | Sonnet | — |
| C | The docket and prill adapters read typed answers and say things plainly: no lock held across a network call, structs for docket's answers, shared event kinds, plain timeline text, a fake at API 6, fixtures re-copied from prill | Sonnet | — |
| D | CI and the gate check what the docs say: no port picked by bind-and-drop, no fixed waits, smoke in CI, Vitest in the hook, one coverage formula | Sonnet | B |
| S | Screens show errors, plain words and disabled controls: an error boundary, no false empty states, no prill, variable or command on screen, `Button` with a reason | Sonnet | — |
| R | The run surface: a level only reaches docket and is never dropped in silence, labels for the escalation events, one "needs a person" state, levels in words, a link per run | Sonnet | S |
| H | The routes, tools and upgrade nobody tests, including a beta.9 database migrated to now | Haiku | — |
| E | The execution store is typed: ids, token and state from the protocol, guards built from the lifecycle, a typed run request | Sonnet | Round 1 |
| F | One wire format and one error model: protocol structs on both sides, a code on every error, no panic on a corrupt row | Sonnet | E |
| I, J, K | One UI kit and tokens; `Host` split and config read once; the item apart from its read model | Haiku, Sonnet | S, R; F; J |

Rounds: **1** = B, C, D, S, R, A, H, at most two building at once; **2** = E then F; **3** = I, J, K. While a card works it runs only the tests of what it touched; the full suites run
once per round on the merged tree, and the local suite of the three after Round 1 and at the end.

---

## Phase 76 — the project file is kept in sync

**Status:** future, not planned (postponed 2026-10-27). Proposed 2026-10-18; ADR 0025 proposed the
same day, with its four defaults. Nothing is built, and the plan was written before Phases 79 to
81 moved the code: re-read it against the tree before opening it. **ADR:** 0025 (amends 0022 §2 and §6). **Plan:**
[`docs/plans/phase-76.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-76.md).

The project file of Phase 73 is a one-way door: it creates and updates by name and never
deletes, so a role renamed in the file becomes a second role, a role removed from it stays for
ever, and a file imported twice doubles the items. Keeping a project beside its code, which is
why the file exists, needs the board to follow the file. Drupal keeps configuration in a sync
directory and content in the database; Tack's file already has the two halves. So:

- **The configuration is everything in the file but the items**, and only it is kept in sync.
  Items keep their additive import.
- **Every role, field, sprint and profile has a key**, made from its name when it is created and
  never changed, so a rename in the file is a rename on the board. A file without keys works as
  today.
- **An import compares three things:** the file, the board and the last revision. Changed only in
  the file: applied. Changed only on the board: kept. Removed from the file and untouched on the
  board: removed. Changed on both: a conflict, which stops the import unless you say which side
  wins. With no revision yet nothing is removed.
- **A removal shows its cost first:** how many cards lose the role, how many values go with the
  field. A profile another project uses, or with a run open, is not removed.
- **Every export and import leaves a revision**, shown in Settings → Data → History, and going
  back to one is an import like any other.
- **The preview and the apply are one plan:** the apply carries the preview's hash and is refused
  if the board moved in between. Everything an import writes lands in one transaction — the
  outline import too — so "import again to finish" goes away.
- The file still cannot switch auto-start on, raise the autonomy level, change the workflow or
  the project type, or change an item.

| Round | What lands | Who |
|---|---|---|
| 1 | Every import in one transaction; the key and the three-way comparison as pure code with their tests. | Sonnet |
| 2 | Keys on the four tables, in the file and on the wire; the status, plan, apply and revision routes; the revisions table. | Sonnet |
| 3 | `tack config export · status · import · history · revert`; the MCP tool `sync_config`; the Configuration card, the plan dialog and the history in Settings → Data; the key under each name. | Sonnet |
| 4 | The guides, the changelog, the gate; a run against a copy of the real database. | Integrator |

Left for a later phase, each shown disabled or named in the preview: choosing a side per
conflict instead of for all; items matched by key and archived when removed; the workflow in
the file.

---

## Phase 77 — the agent's copy is prepared, and every run has a limit learnt from its kind

**Status:** built 2026-10-19 and in `develop`. **ADR:** 0027, accepted 2026-10-19. **Plan:**
[`docs/plans/phase-77.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-77.md).

An agent that starts in a copy with no dependencies spends its first steps failing to run the
project's check. Whoever makes the agent's copy now installs the dependencies from the lockfile:

- **The project's install command** is detected from its lockfile, can be replaced or turned off
  in Settings → Automation, and the Run dialog says what a run prepares with.
- **The runner prepares the copy it makes** for Claude Code, Codex and opencode, caches the install
  per lockfile, and ends the attempt with the command's output if the install fails.
- **A docket run makes no runner copy:** the runner reads docket's branch from the project's
  repository, and docket installs for itself. Tack sends it the command from docket's API 6 on.
- **Every run has a limit and says where it comes from.** A profile with five successful runs
  offers a limit proposed from them; until then a provisional one applies.

The live proof with a real install is cancelled (see [Cancelled](#cancelled)).

---

## Phase 75 — limits in tokens, and a lean context

**Status:** opened 2026-10-18 from the measurements of My SaaS's first runs; ADR 0024 accepted
the same day. Every round is in `develop`, with the guides: Round 4 on one capture of Claude Code's
and opencode's live usage, so Tack stops both at a run's token limit and counts docket's cache
reads as docket does; Round 5's screens print the attempt's counted tokens and the run dialog's
limit line. Nothing is left.
**ADR:** 0024 (amends 0020 §5, 0021 §7 and 0012 §6). **Plan:**
[`docs/plans/phase-75.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-75.md).

Dollars are a token count multiplied by a price list someone typed. A model with no price
counts as free and slips past a dollar limit, and the same work costs differently on every
provider. Tokens are what every agent measures. So:

- **A limit is in tokens, and nothing has one by default.** A profile can set tokens per run
  and a time limit, a run can change them, and a project can set tokens per day. Dollars per
  day goes away, and Tack stops asking Claude Code for a dollar limit.
- **A token counted is a new one:** uncached input, cache writes and output. Cache reads are
  shown beside them and never count. In My SaaS's measured runs they were 70% to 92% of the
  input.
- **Every limit says who applies it and where it is changed**, and "no limit" names the brake
  that still applies: "No limit set in Tack · docket stops each step at its own limit (docket's
  setting)". Claude Code and opencode say "Tack stops ... run there"; an agent that only reports
  its tokens says "measured, not enforced".
- **The daily limit is reserved when a run is admitted:** a run starts only if today's use,
  the open runs' limits and its own fit, so the day's limit is never crossed.
- **Dollars are an estimate, shown last:** "≈ $0.42 (estimated)", "not priced" for a model with
  no price, and how many of the models used have one.
- **What Tack sends an agent carries nothing twice.** The brief goes to docket once instead of
  three times; a rework replaces the previous round's fixes instead of piling them up; a card's
  comments have a size limit. Claude Code gets only the tools its role needs. Its seven
  default tools cost about 6 300 tokens on every call.
- **What docket sends the model is docket's.** In My SaaS's runs, 64% of the tokens counted
  were old tool results sent again: one listing of the whole tree, with absolute paths, was 84%
  of a step. The fixes asked of docket are to condense old results, keep listings relative and
  bounded, hand the next step what the previous one wrote, and make the member's files (its
  soul, tools, memory and project instructions) agree with each other and with the checkout
  it works in.

| Round | What lands | Who |
|---|---|---|
| 1 | The profile's limit reaches the run; one token count for the day, the run and the factory; the brief once, rework fixes replaced, comments bounded. | Sonnet |
| 2 | The daily limit with its reservation and bar; dollars per day removed from the schema, the screen and the project file; dollars shown as an estimate. | Sonnet |
| 3 | A profile's limit checked where it is saved and shown before a run starts; a run's tokens while it runs, from docket's steps, so its reservation shrinks and its timeline shows them. | Integrator, Sonnet |
| 4 | Claude Code and opencode stopped at their limit from what they report, and Claude Code's tools per role, from one capture of the real programs. | Sonnet |
| 5 | The screens that read them; the guides; My SaaS's runs again. | Integrator |

---

## Phase 74 — what the first real project found

**Status:** opened 2026-10-17; ADR 0023 accepted the same day, with its three defaults. Rounds 1
to 6 are in `develop` (2026-10-18), with Round 4's pages in the book. The first runs on the real
board reached their teams the same day; nothing is left but the live proof, which is cancelled (see [Cancelled](#cancelled)).
**ADR:** 0023 (inside ADRs 0002, 0006 §5 and 0021). **Plan:**
[`docs/plans/phase-74.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-74.md).

On 2026-10-17 My SaaS went into Tack and docket end to end, on the maintainer's own board: 86
tasks, ten roles, eight profiles, three teams. The project, its roles and profiles, the teams and
the choice of a run's team all worked. No run reached a team, and the board did not say why.
Twenty defects in Tack were written down, each with its evidence. This phase closes them where
each fact is owned, so no rule is computed twice:

- A run that cannot start ends Failed with the reason, and a run nobody holds any more asks a
  person instead of sitting in "Preparing"; cancel always ends it.
- The board's runner keeps the name the board gave it, so it survives a restart; one rule says
  whether a computer is online, and every screen and the scheduler read it.
- What a computer reports about its teams stays current without a restart, the merge ceiling is
  each run's own team's, and the Ceiling rung never promises more than what it lists.
- Everything Tack runs is in the log by default; the server says which database it opened and
  warns when it is not the one the desktop app uses; the doctor reads the runner's own file.
- A project file carries the auto-start switch's state both ways.
- The docket contract is proven by answers recorded from a real docket, not a fake written from
  its spec.
- The screens name pages and item types in the project's words, a settings tab follows its
  address, and a new project can be created from a project file.

| Round | What lands | Who |
|---|---|---|
| 1 | A run that cannot start ends; one name and one liveness rule for runners; the screens' words and the import. | Sonnet |
| 2 | Current team reports and the per-team ceiling; logs, messages and the file's switch. | Sonnet |
| 3 | The docket contract from captures, once docket's `POST /pods` is fixed. | Sonnet |
| 4 | Docs and the gate; then My SaaS's first runs through its teams on the real board. | Integrator |
| 5 | From those runs: a docket task that outlives the runner is followed again, and the board makes its own decision token. | Sonnet |
| 6 | A run that ended says why; a question ends with its run and the board finds its runner's verifier; the tutorial's screenshots. | Sonnet |

---

## Phase 73 — every run goes to a team, and automation is one ladder

**Status:** opened 2026-10-14 and built: the six waves landed in `develop` on 2026-10-14, the
seventh, a whole project in and out through one file (ADR 0022), on 2026-10-17, followed the
same day by the book's three guides. The live proof, the local `docket-prill-e2e.sh`, passes on
docket's `develop` since 2026-10-18; My SaaS's runs through its teams are Phase 74's last round.
**ADRs:** 0021 (amends 0019 decision 7 and 0015 decision 1), 0022 (extends 0017). **Plan:**
[`docs/plans/phase-73.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-73.md).

Every run of the docket agent goes to a team that already exists and keeps existing, over
docket's HTTP API, instead of a throwaway agent per run. A project role says which profile, and
through it which team, takes its tasks. A task can start by itself when it reaches a status the
project chooses. Tack's automatic merge never rises above what the team allows: a team that
needs a person during a run keeps its tasks at Level 3. Usage is kept per model, and a task can
repeat weekly, monthly or quarterly. All of it is set on one screen, Settings → Automation, as a
ladder of four rungs: Teams, Start, Merge, Ceiling. A control that cannot be used is shown with
its reason, never hidden.

Tack still decides what is done, with what priority and budget, and owns the push, the pull
request, the merge and the level. The agent runtime runs the agent and its team: members,
models per role, pipeline, gates, approvals. The verifier signs the verdict. Nothing is computed
twice.

| Round | What lands | Who |
|---|---|---|
| 1 | The runner runs several attempts at once; a role chooses the profile and the fleet. | Sonnet |
| 2 | The team's ceiling on the automatic merge; usage per model priced through one SQL. | Sonnet |
| 3 | A task starts by itself; a task can repeat. | Sonnet |
| 4–5 | The docket harness over HTTP against a fake docket, then the daemon, the doctor and the deletions; the ladder screen beside. | Sonnet |
| 7–9 | A whole project in and out through one file: the outline carries fields, profiles, roles, sprints and automation; the export writes the same file back; a template is that file without items. | Sonnet |

---

## Archived

- [Phases 0–57](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-0-57.md) — the original
  engineering phases, the audit-driven cycle (26–32), the Agent-Factory Control Center
  (33–38), the Agnostic Control Plane (39–49), and the Harness-Agnostic Runner Fleet
  (50–57).
- [Phases 58–63](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-58-63.md) — standalone
  single-binary packaging, the first public release, agent onboarding,
  the desktop app and background service, and human maintainability.
- [Phase 64](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phase-64.md) — the smallest
  codebase that does everything a user can do, which retired the docket control-plane bridge.
- [Phases 65–72](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/boards/roadmap-phases-65-72.md) — the
  `v0.1.0-beta.9` release; briefs, evidence and the verifier's merge-readiness pack; a run a
  normal person can start; the first live docket + prill run; the security phase after the
  withdrawn tag; the agent bringing its own model; and merging from Tack.

---

## Contributing

See [CONTRIBUTING.md](https://github.com/yielab/tack/blob/develop/CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.
