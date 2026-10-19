# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/boards/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/boards).

**Tack is delivered through Phase 73.** A project is planned and tracked on a board, a list, a
table, a calendar or a timeline, with or without agents. A task can be handed to an agent on
your computer (Claude Code, Codex, opencode or docket), which brings its own sign-in and model;
Tack holds no model key. The run's work comes back as evidence to review, an independent
verifier can judge it, and Tack can push the branch, open the pull request and merge it: by a
button, when the task moves to Done, or by itself at Level 4 or 5 under the verifier's signed
verdict. A project can start its tasks by itself, send each role's tasks to a long-lived docket
team, repeat tasks, and travel whole in one file.

**Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. `v0.1.0-beta.10`,
which carries Phases 66 to 72, is tagged (2026-10-14) but its release is not published yet.
Phase 73 is in `develop` and ships in the release after it.

**Where this is going.** Phase 74 closes what the first real project found when it was put
into Tack and docket end to end, and ends with that project's first runs through its teams,
which is also the live proof of Phase 73. Phase 75 puts every limit in tokens, with dollars
only as an estimate, and keeps what an agent is sent free of repeated material. Phase 76 makes
the project file a place the configuration is kept, not only imported from: a key that survives a
rename, an import that removes what the file removed and keeps what the board changed, the cost of
every removal shown first, and a revision to go back to. Tack
keeps the same three neighbours it never contains: an agent runtime such as docket, which runs
the work; an independent verifier such as prill, which judges it; and GitHub. Each still works
alone, and without a verifier a project stays at Level 3.

---

## Open

What is still to do, by who can do it. Each item is one line here and has its detail in the
plan or the page named beside it.

**The maintainer's, by hand** (refused to agents, costs money, or needs another machine):

- Publish the `v0.1.0-beta.10` release. The tag's release run did not start (2026-10-14), so
  there is no release yet, and the install commands in `README.md`, which name
  `v0.1.0-beta.10`, answer 404 until there is. `docs/LAUNCH-CHECKLIST.md` is the order of work.
- The publish list (seven issues, four posts, two Discussions), one install on macOS and one on
  Windows, and the code-signing decision: Wave 0 of
  [`docs/plans/phase-65.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-65.md).
- The GitHub settings and workstation items of Wave 0 of
  [`docs/plans/phase-69.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-69.md):
  a tag ruleset, a `release` environment holding the Homebrew token, `security-boundaries` as
  a required check, a garbage collection asked of GitHub support, the tunnel and its DNS
  record, the local permission list, and the `.env` file, which Tack no longer reads since it
  holds no model key.

**Waiting on a partner:**

- docket's context and its token count (Phase 75's requests to docket) are fixed in docket's
  `develop` (its Phase 44): a rerun of Fabulada's first steps counted at most 59 575 tokens a task,
  and no step reached docket's own per-step limit. The live run of `scripts/docket-prill-e2e.sh`
  passes against it (2026-10-18), inside docket's sandbox with a read-only grant for the
  toolchain. Fabulada's code runs from the board wait on their dependencies: ADR 0086 (accepted
  2026-10-19) has whoever makes the agent's copy install them from the lockfile, docket for a
  docket run and the runner for the others, and is not built yet; the end-to-end scenario with
  two teams and a rework is still to write.
- The verifier: directives learnt from a person's corrections. When the verifier proposes one,
  Tack applies an approved one to `AGENTS.md` through a pull request.

**Gaps in what shipped**, each shown in the product with its reason:

- "Apply the repository's teams" is disabled: the board has no channel to the runner, so the
  Teams rung tells you the `docket init` command to run in the project's folder.
- A profile's team can be set from a project file or the CLI, not from the profile form.
- Retrying a run request after its task's type or priority changed answers 409, because the
  task's metadata is part of the request's fingerprint.
- Only docket stops a run at its token limit. Claude Code, Codex and opencode are measured, not
  enforced: their tokens are read when they finish, and the screen says so. Their runs keep their
  whole limit reserved against the day until they end; a docket run reserves only what its
  steps have not yet used.
- The run dialog's limit line says "No limit set in Tack" for a run without one even in a project
  with a daily limit, where the run will be given what is left of the day when it is chosen.
- The tokens a verifier spends count whole, cache included: prill reports no cache figure.
- Only docket reports a running agent's steps on its timeline; Codex, Claude Code and opencode
  need their tool lines captured from the real programs first.
- `docs/screenshots/automation.png` and `factory-metrics.png` predate the four-rung Automation
  screen and the cost cards.

---

## Phase 76 — the project file is kept in sync

**Status:** proposed 2026-10-18; ADR 0084 proposed the same day, with its four defaults. Checked
against Phase 75's Rounds 1 and 2 the same day: nothing in Phase 75 changes; Phase 76 takes
migrations 093 and 094, and an older file's dollars-per-day warning reaches its preview.
**ADR:** 0084 (amends 0081 §2 and §6). **Plan:**
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

## Phase 75 — limits in tokens, and a lean context

**Status:** opened 2026-10-18 from the measurements of Fabulada's first runs; ADR 0083 accepted
the same day. Every round is in `develop` the same day, with the guides: Round 4 on one capture
of Claude Code's and opencode's live usage (four short paid calls), so Tack now stops both at a
run's token limit and counts docket's cache reads as docket does; Round 5's screens print the
attempt's counted tokens and the run dialog's limit line. Fabulada's runs wait on docket's context
and count. What is left is in Open above.
**ADR:** 0083 (amends 0079 §5, 0080 §7 and 0070 §6). **Plan:**
[`docs/plans/phase-75.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-75.md).

Dollars are a token count multiplied by a price list someone typed. A model with no price
counts as free and slips past a dollar limit, and the same work costs differently on every
provider. Tokens are what every agent measures. So:

- **A limit is in tokens, and nothing has one by default.** A profile can set tokens per run
  and a time limit, a run can change them, and a project can set tokens per day. Dollars per
  day goes away, and Tack stops asking Claude Code for a dollar limit.
- **A token counted is a new one:** uncached input, cache writes and output. Cache reads are
  shown beside them and never count. In Fabulada's measured runs they were 70% to 92% of the
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
- **What docket sends the model is docket's.** In Fabulada's runs, 64% of the tokens counted
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
| 5 | The screens that read them; the guides; Fabulada's runs again once docket's context and count are fixed. | Integrator |

---

## Phase 74 — what the first real project found

**Status:** opened 2026-10-17; ADR 0082 accepted the same day, with its three defaults. Rounds 1
to 3 landed in `develop` on 2026-10-18, with Round 4's pages in the book. The first runs on the
real board reached their teams the same day and ended on docket's per-step token limit; the
next ones, on paid models, wait on docket's context and token count (Phase 75).
**ADR:** 0082 (inside ADRs 0050, 0062 §5 and 0080). **Plan:**
[`docs/plans/phase-74.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-74.md).

On 2026-10-17 Fabulada went into Tack and docket end to end, on the maintainer's own board: 86
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
| 4 | Docs and the gate; then Fabulada's first runs through its teams on the real board. | Integrator |

---

## Phase 73 — every run goes to a team, and automation is one ladder

**Status:** opened 2026-10-14 and built: the six waves landed in `develop` on 2026-10-14, the
seventh, a whole project in and out through one file (ADR 0081), on 2026-10-17, followed the
same day by the book's three guides. The live proof, `scripts/docket-prill-e2e.sh`, passes on
docket's `develop` since 2026-10-18; Fabulada's runs through its teams are Phase 74's last round.
**ADRs:** 0080 (amends 0078 decision 7 and 0074 decision 1), 0081 (extends 0076). **Plan:**
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
