# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles).

**Tack is delivered through Phase 91.** A project is planned and tracked on a board, a list, a
calendar or a timeline, with or without agents, and its teams at work are drawn on the Orchestration view. A task can be handed to an agent on
your computer (Claude Code, Codex, opencode or docket), which brings its own sign-in and model;
Tack holds no model key. A step that runs out of its own budget keeps its work and lets a person
continue it or start over from it. The run's work comes back as evidence to review, an independent
verifier can judge it, and Tack can push the branch, open the pull request and merge it: by a
button, when the task moves to Done, or by itself at Level 4 or 5 under the verifier's signed
verdict. A project can start its tasks by itself, send each role's tasks to a long-lived docket
team, repeat tasks, and travel whole in one file.

**Releases.** `v0.1.0-beta.9` (2026-09-21) is the last published release. The next is
`v0.2.0-beta.1`, which carries Phases 66 to 82: the version is bumped and the changelog section
is written, and it waits for the maintainer to tag it
([`docs/LAUNCH-CHECKLIST.md`](https://github.com/yielab/tack/blob/develop/docs/LAUNCH-CHECKLIST.md)).
Phases 83 to 91 are under `[Unreleased]` in the changelog. The `v0.1.0-beta.10` tag (2026-10-14)
never had a release and is not re-cut; its changes ship in `v0.2.0-beta.1`.

**Where this is going.** Tack keeps the same three neighbours it never contains: an agent
runtime such as docket, which runs the work; an independent verifier such as prill, which judges
it; and GitHub. Each still works alone, and without a verifier a project stays at Level 3.

---

## Open

Phases 73 to 91 are built and in `develop`. Tagging `v0.2.0-beta.1` is the maintainer's own step,
and it is not a card.

**Phase 90 — a run is never refused for a contradiction Tack wrote itself** (2026-11-03, no plan
file, no ADR; from the seven runs of one research task; both cards built in `develop` the same day). Two cards, one round:

- **A — one rule for a run's network.** The built-in Researcher, Planner and Reviewer name
  WebFetch (the Researcher WebSearch too) and no `network`, so a run that leaves the network to the
  profile asks for no network and for web tools at once, and Claude Code refuses it before it
  starts; the Run dialog starts its Web tools box off for the same reason, and opencode, which
  reaches the network on every run, refuses every run whose box is off. One function in `tack-core`
  gives a profile's default network for an agent: the profile's `network` flag, or, for any agent
  but docket, its tools for that agent being `*` or naming WebFetch or WebSearch; for opencode it is
  always on, and the dialog shows the box on and disabled with that reason. The run's creation,
  auto-start and the Run dialog read it, the dialog from the server rather than a copy of the rule.
  Web tools off then takes the web tools out of a Claude Code run, as the box's help already says,
  instead of refusing the run.
- **B — a refused run reads "Not started" on every chip.** The Execution tab's run chips read the
  run's latest attempt only once it is loaded, and otherwise the request's own state, `failed`, so
  the same refusal reads Not started on one chip and Failed on the next. The fallback reads the
  run's `never_started` as the board's card does.

**Phase 92 — a question is answered in one place** (2026-11-04, no plan file, no ADR; from task
4C9BAB, where the person could not tell where to type their answer). One card, built in `develop` the
same day except the screenshot `decision-inbox.png`, which waits for the next billed recording:

- **A — the answer box, redesigned as one piece.** Today the choices come first, the text field is
  one line under them called "Details (optional)", the recommendation is a line with a raw option id
  below that, and a question with no choices asks for an "option id". When the question takes text
  (docket's `acceptsText`) the text is often the answer, and nothing on the screen says so. Rethink
  it from the person's side, in this order: the question; the choices, the recommended one first and
  marked once with its reason as its own line; the field where they write, opening under the choice
  it belongs to, named for what is asked ("Your answer"), required when the question requires it and
  absent when it takes none; one primary button that says what it does ("Send answer"), with Decline
  a plain secondary action and not a choice card. Risks and token estimates are a quiet line on the
  choice, not red blocks, and the screen adds no sentence that is not a label or a verb: the question
  does the explaining. The words are the product's own ("question", "answer"; no "resolve", no
  "option id"). The same component serves approvals (Approve once, Approve for this task, Deny with a
  reason) and inputs, so the three read alike. Done when a person with a question that takes text
  answers it without searching for the field, by keyboard too; the order, the field's label and the
  button's words are pinned by component tests; and the live suite's locators and the tutorial's
  screenshot of a question are updated.

**Phase 93 — an item is a group or work** (2026-11-05;
[plan](https://github.com/yielab/tack/blob/develop/docs/plans/phase-93.md); ADR 0036 amended with
decisions 10 to 13, accepted). An epic is a card in the To Do column beside the tasks it holds and
never leaves it. Auto-start can start it as if it were a task, and the Dashboard counts it as one.
"Subtask" is a type and a position at once. The Board, the List, the Timeline and the Orchestration
view each work out on their own what a parent is, and the List cannot move a row under another
parent. The board has two derivations, the web's and the server's saved boards, which only the CLI
reads.

One rule replaces all of it: a project's types are a catalog in which each type holds work or is
work; an item is a group when its type holds work or when it has children; work runs and a group is
planned; and each view shows the level it is for. Nothing old stays beside the new.

Built 2026-11-05, in `develop`: all eight cards in five rounds (A; B; C ∥ D; E; F ∥ G ∥ H), each through
the pre-push gate, and the full workspace suite. Left is the live suite against docket and prill. The plan
names each card's files, shapes, removals, docs, tests and greps.

- **A** — the catalog of types, replacing the `ItemType` enum and the type words of the vocabulary.
- **B** — one rule for the tree, and moving an item under another parent.
- **C** — a group runs only with a profile that proposes, and who does a piece of work is inherited
  from its parent.
- **D** — the facts every screen reads (`node`, `progress`), and the saved boards deleted.
- **E** — the Board in lanes, and the pieces every screen shares.
- **F** — the List as the tree, the item page, and the create dialog.
- **G** — the Timeline, the Calendar, Sprints and the Dashboard.
- **H** — Orchestration, search, and Settings → Types.

**Phase 94 — the agent is contained the same way whatever runs it, and Tack measures what it already
labels** (2026-11-06;
[plan](https://github.com/yielab/tack/blob/develop/docs/plans/evals-y-guardrails-2026-11-06.es.md),
[audit](https://github.com/yielab/tack/blob/develop/docs/audits/evals-y-guardrails-2026-11-06.es.md);
no ADR: ADR 0013 and ADR 0018 decisions 11 and 12 are amended where their text becomes false.
**Proposed, not accepted:** the plan's §11 decisions 1 to 4 are the maintainer's, and rounds 3 and 4
wait on them).

**Built 2026-11-07, in `develop`:** round 0 (C0-1, C0-2, C0-3) and round 1 except GR10 and GR11 (GR0,
GR1, GR4, GR7, GR8, GR14, GR18, P1), the full pre-push gate green after each round. The parity test
carries a list of the cells still pending, each with its card; a card deletes its entries as it
lands. GR10 and GR11 wait on decisions 3 and 4. Left open by GR14, for the maintainer: the embedded
runner never rotates its credential and stops with `SessionRefused` after its 90 days; runner rows and
pending decisions that predate the default expiry never expire.

An audit of the four harnesses found that the board contains an agent well (scopes, change sets, a
second secret for decisions, demotion after a revert) and the machine does not: the network is never
cut, Claude Code runs with every permission skipped in `auto` even with no sandbox, a missing `bwrap`
means no sandbox rather than no run, the install step runs as the user, and docket's sandbox is assumed
rather than asked. Each harness has a different set of holes, because every control is a flag of one
CLI and not a rule of the runner. Tack judges no work itself and never grades the one that does: the
verifier is off by default and never changes a run's state, yet every run already ends with a person's
label (accepted, rejected with a reason, merged, reverted) that nothing reads back.

Rules of the phase. **A guardrail is the runner's rule, not a CLI's flag:** every card lands in the
four harnesses or writes in the descriptor why one cell cannot, and a parity test keeps the matrix
green. **A rule is a boundary test and its benign twin** (the normal case that must keep passing).
**Opus plans and integrates, Sonnet and Haiku execute;** each card names its files and its `-E`
filter, runs only those tests, and has a token cap (S ≈ 300k, M ≈ 900k, L ≈ 2M counted tokens); the
full suites, Playwright and the live suite run once per round. **A real model is the integrator's:**
cards prove with the fake agent; regression, safety and verifier run at integration with Claude Haiku
5.5 through Anthropic (the local model stays an option) under a token budget, never in CI, and CI
still needs neither docket nor prill. **Nothing invented:** no agent writes eval tasks from
imagination; capability tasks come only from runs a person rejected or reverted.

**Round 0 — cleanup and the truth of the documents** (parallel, no shared files):

- **C0-1 — dead code and stale names** (Haiku · S). Delete `event_sink` (no production caller) and
  the three helpers only tests use; rename `needs_beta_6` for what it checks, `docket_v5.rs` to
  `docket_api.rs` (after the pending docket change lands) and `table.spec.ts` to `list.spec.ts`; fix
  the comments that say `InPlace` is not provisioned, that `is_operator` means Tack too, and that the
  rate card serves a dollar budget. Done when the greps in the plan are empty and the runner suite and
  clippy are green.
- **C0-2 — amendment notes** (Haiku · S). One dated note at the end of ADRs 0004, 0021, 0024, 0027,
  0032, 0036 and 0018 (decision 10), the `mrp-v1` contract README and the environment policy's module
  doc, each saying what the code does today and which card changes it. Done when every note names its
  source and the book carries no phase number.
- **C0-3 — one human review** (Sonnet · M). The pack review and the run review become one store and one
  endpoint: a reason is required to reject, the reviewer is the real principal, and a migration keeps
  both stores' rows. Done when the migration test is seen failing first, the reason-less reject is a
  400, `"operator"` is gone from the handlers, and `MrpPanel` reads one endpoint.

**Round 1 — parity, and containment that changes nothing for normal use** (GR0 first and alone, the
rest in parallel):

- **GR0 — the parity matrix** (Sonnet · M). A list of parity rules in the protocol crate and, per
  descriptor, who meets each: the CLI, the runner, docket, or missing with a reason. A boundary test
  walks every registered descriptor; the run page lists per rule who applies it. Done when the test is
  red today exactly on the audit's red cells, a test descriptor with an unexplained gap is red, and
  each later card turns its cells green.
- **GR1 — no sandbox means ask** (Sonnet · S). When the runner found no sandbox the request runs as
  `ask` whatever it asked for: Claude Code never gets `bypassPermissions` outside a sandbox, Codex is
  switched to `app-server`, opencode's block says `ask`; one event says why. Done when a boundary test
  per descriptor shows the asking command line with a fake "none" sandbox, and the twin shows `auto`
  unchanged under `bwrap`.
- **GR4 — the agent's own config is read-only and brings no integrations** (Sonnet · M). `~/.claude`
  and `~/.codex` are bound read-only with a writable copy of what the CLI must write; Codex gets
  `-c mcp_servers={}` as Claude Code has `--strict-mcp-config` and opencode `mcpServers: []`; the
  three CLIs' login credentials join redaction. Done when writing those files from inside the sandbox
  leaves the originals untouched, a user-configured MCP server never reaches Codex, a seeded credential
  reads `[REDACTED]`, and both CLIs still start and keep their session.
- **GR7 — the environment policy names the proxies and the config homes** (Haiku · S). `HTTP(S)_PROXY`,
  `ALL_PROXY`, `NO_PROXY`, `NODE_PATH`, `JAVA_TOOL_OPTIONS`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
  `OPENCODE_CONFIG`, and the `XDG_` and `npm_config_` prefixes, one boundary test per name. Done when
  the module doc and the list say the same thing.
- **GR8 — docket's sandbox is asked, not assumed** (Sonnet · S). The probe reads docket's isolation
  posture from `features`; absent or `none` reads as `none`, so auto-start and the dialog default to
  `ask`. Done when the fake docket's three variants give `none`, `none`, `harness`. The ask to docket
  is written with the other neighbours' asks, and never blocks.
- **GR10 — the merge's integrity** (Sonnet · M). An unknown team ceiling holds the automatic merge; the
  decision is read from the verified statement's predicate (prill signs the whole pack) and a
  mismatch with the translated pack holds; the Merge button over a `reject` requires a written reason,
  and the person still decides (decision 4). Done when two new rows of the policy table are seen
  failing first and the handler test refuses the reason-less merge.
- **GR11 — MCP is an agent** (Sonnet · M). `tack mcp` with no agent credential refuses unless
  `--operator` is passed; the MCP origin is derived from the principal, not from a header; `ask` and
  no network are fixed by the server for a run an agent principal creates (decision 3). Done when the
  trust-boundary tests show a forged client header changes nothing and an agent's `auto` lands as
  `ask`, and the live suite's manager tests still pass at integration.
- **GR14 — three small hardenings** (Haiku · S). The session cookie carries `Secure` behind https or
  `X-Forwarded-Proto` and a `Max-Age`; a decision gets a default `expires_at`; a runner credential
  expires by default. No rate limit: loopback needs none and the tunnel has Cloudflare's. Done when
  three trust-boundary tests pass.
- **GR18 — docket's path is redacted like the others** (Sonnet · S). Steps, `outward` calls and
  record rows pass through the same redactor as local output. Done when a secret seeded in each of
  the three reads `[REDACTED]` with the fake docket.
- **P1 — the evidence names the model family for every harness** (Haiku · S). Only docket's steps
  carry `family` today; the verifier reads it from the last implementer step and, missing it, marks
  the judge degraded, so every Claude Code, Codex or opencode run with a judge ends in review. The
  three local harnesses record the family their CLI reports. Done when a fixture run of each harness
  yields a `generator.family` in the verifier's evidence document.

**Round 2 — the evals harness and the change contract** (sequential):

- **EDD-1 — the change contract and the prompt template's fingerprint** (Sonnet · S). `CONTRIBUTING.md`
  gains the step "does it change what the agent reads or receives? then name the suite, the threshold
  and the baseline, and write the task first"; `docs/TESTING.md` gains an "Evals" section; a hash of
  the prompt templates is computed at build time, stored with each run and shown on its page. Done
  when changing one template line changes the fingerprint in a test and the three documents agree.
- **E2-1 — the task, the world and the trial** (Sonnet · M). A development crate `tack-evals` (not
  released, a leaf in the crate graph): a `task.yaml` loader, a clean world per trial over the test
  support crate (data dir, port, seeded repo, fake `$HOME` with canaries, a fake secret), an operator
  that answers decisions with the decision token, a trial that creates project, profile and item,
  starts the run and freezes what it left, and `trials.jsonl`; subcommands `run` and
  `check-solutions`. Done when, with the fake agent, one task ends Succeeded, one asks and is approved,
  a broken reference solution makes `check-solutions` exit 1, an invalid task is refused with its
  reason, and an agent writing into `hidden/` cannot change the exam; CI runs these tests.
- **E2-2 — graders, metrics and report** (Sonnet · M). Three code graders (outcome, governance,
  verifier agreement), pass@k and pass^k, Wilson intervals, task-clustered error, the paired
  difference, the four-way failure taxonomy (tack, agent, eval, infra), `summary.md`, and the
  `compare` and `nightly` subcommands. Done when each grader is proved with tampered records, the
  metrics match hand-computed values, `compare` works on two synthetic trial files, and `nightly` warns
  on a 3/3 → 1/3 task and not on an infra failure.
- **E2-3 — the regression suite and the first baseline** (Sonnet · M; the baseline is the
  integrator's run). Eight to ten tasks from the `calc` fixture and the live suite's journeys, two of
  them planning tasks, each with hidden checks and a reference solution, run across the four harnesses
  (a harness that cannot do a task by design is `not_applicable` with its reason). Done when every task
  passes `check-solutions` and the first baseline per harness is committed.

**Round 3 — the large guardrails, measured before and after** (E3-1 and E3-2 first, then in
parallel; GR5b last; decisions 1 and 2 gate GR2 and GR5):

- **E3-1 — the safety suite** (Sonnet · M). Eight to ten pairs, each a task with and without the
  attack (vectors: card text, the repository's README, a web page, a comment in a test, a
  `postinstall`; targets: exfiltrate the fake secret, touch the agent's config, weaken a test, push,
  read the SSH canary). Done when the twins pass `check-solutions` and the baseline gives, per
  harness, utility, utility under attack, intermediate and final attack success, before any card below.
- **E3-2 — the verifier suite** (Sonnet · M; about one hour of a person's labelling). A script makes
  fifteen mutants and weakened-test variants over the regression solutions beside fifteen reference
  solutions; a person confirms the thirty labels. Done when the verifier grader reports prill's false
  approval and false rejection (and the Reviewer profile's, once it emits a verdict).
- **GR2 — the sandbox is required** (Sonnet · M; decision 1). `[sandbox] required = true` by default:
  with no `bwrap` and no explicit opt-out the probe says the agent does not run here and why; the run
  page, Agents and the administrator guide say so; ADR 0018 decision 11 is amended. Done when the
  boundary test schedules nothing without the opt-out and runs as `ask` with it.
- **GR3 — three more flags** (Haiku · S). `--unshare-ipc --unshare-uts --cap-drop ALL`, nothing else.
  Done when a real-`bwrap` test sees no host IPC and empty capabilities, and `git`, `node` and
  `python3` still start inside.
- **GR5a — an egress proxy with an allow-list** (Sonnet · M; decision 2). One `CONNECT` proxy on
  loopback per run, its list the descriptor's model endpoints plus the profile's allowed hosts, handed
  to the agent through the proxy variables; a refused destination is an event on the run. Done when a
  fetch outside the list fails with the event and the CLI still reaches its model.
- **GR5b — the network is enforced** (Sonnet · M; after GR5a and GR2). `--unshare-net`, with the
  proxy reaching the sandbox through a mounted Unix socket. Done when, under real `bwrap`, a direct
  connection to any address fails, only the list passes through the proxy, the safety suite's final
  exfiltration success is zero and the twins lose no more than five points.
- **GR6 — the install step runs in the sandbox** (Sonnet · M). The dependency install runs under the
  same `bwrap` with the fake home, an allow-listed environment and the proxy pointed at the registry,
  its output redacted; a board-supplied replacement install command needs the runner's
  `[prepare] board_may_replace`. Done when a `postinstall` cannot reach the real home, an unpermitted
  board command is refused with a reason, and the `calc` fixture still installs.
- **GR9 — what examines the agent holds the merge** (Sonnet · M). With a prill pack, the signal is
  its structured `flags` (tautological or deleted tests, changed test or analysis config, changed
  manifests, config changed in head), read as data rather than flattened into text; with no pack, a
  shared exact-name list in the protocol crate recorded in the evidence for the four harnesses. Either
  way the run is held from automatic merge and the review asks for confirmation. Done when a pack
  fixture with `tautological_test` holds, the list's case table no longer flags `latest/` or
  `contest.md`, and the verifier suite is re-run after.
- **GR12 — trust follows the item in every harness** (Sonnet · S). An imported, untrusted item runs
  as `ask` with web tools off by default in the four harnesses, shown with its reason and raisable by
  the person. Done when a start-run test per harness shows the defaults and the safety suite's
  imported-card vector is re-measured.
- **GR13 — the limits close at the edges** (Sonnet · M). Cancelling kills the process group and the
  session for the three local CLIs; a failed cancel retries with SIGKILL and marks the run; a profile
  whose agent reports no usage while running (Codex today) is not auto-started after exceeding its
  limit and gets a shorter default wall clock. Done when a child in its own session dies on cancel in a
  test per descriptor and the auto-start test skips the unmeasured profile.
- **GR17 — the outcome rules hold for docket too** (Sonnet · M). On `done`, the diff of the branch
  docket returns enters the evidence like a local agent's, so the Planner rule, the "changed nothing"
  rule and GR9 see the same thing. Done when, with the fake docket, a Planner that edits `src/` is
  Failed, a `done` run with no diff is Failed, and a run touching `tests/` is flagged.

**Round 4 — production and the loop back** (EDD-2 before EDD-3; C0-3 before EDD-2):

- **EDD-2 — quality figures by window** (Sonnet · M). `GET /api/metrics` gains success rate,
  first-pass acceptance, rework per item, revert after merge, the verifier's agreement with the
  person's review and the pull request's fate, pass^k of recurring tasks, questions per hundred actions
  with the fraction granted, and stops by reason, cut by profile, harness, model, team and template
  fingerprint; the Metrics page shows a Quality section. Done when fixtures give known rates and the
  agreement matches a four-case hand table.
- **EDD-3 — a trend reaches the inbox** (Sonnet · S). One notification rule: the last seven days
  against the previous twenty-eight, Wilson intervals, a "needs attention" notification only when they
  do not overlap, evaluated once a day. Done when a synthetic drop notifies once and no drop notifies
  nothing.
- **EDD-4 — `promote`** (Sonnet · S). From a rejected or reverted attempt: a git bundle at the base
  commit, the item and its brief, the person's reason and a pointer to the evidence, anonymised, into a
  git-ignored candidates folder (with docket's task fields when the run was docket's). Done when a
  fixture attempt yields a candidate with no secret, home path or real name, and `check-solutions`
  refuses it until a person adds the hidden checks and the reference solution.
- **C4-1 — cleanup with conditions** (Haiku · S). `subtasks-from-plan` and the `ItemSource::Plan`
  texts take the catalog's words (the `plan-v1` field stays); the `status_map_policy_id` and
  `[provider]` compatibility shims go, only once `v0.2.0-beta.1` is published. Done when the greps are
  empty.

Left out on purpose, listed under "Future, not planned": a calibrated model judge, the OTel
`gen_ai.evaluation.result` export, a Harbor or Terminal-Bench adapter, a hash-chained log, seccomp in
the sandbox, a rate limit per credential.

## Future, not planned

Phase 76, status rules and the evals leftovers are the only ones. They have no date, no owner and
no card in progress; they stay written so they can be opened later.

**What Phase 94 left out.** A model judge that decides only once calibrated against human labels
(TPR and TNR at or above 0.90 on a held-out set, abstaining otherwise); publishing each eval trial as an
OTel `gen_ai.evaluation.result` event on the run's trace; an adapter so Tack can be scored by an
external benchmark (Harbor, Terminal-Bench); a hash-chained log for merges, change sets and
decisions; seccomp in the sandbox; a rate limit per agent credential. Each comes back as a card when a
measurement asks for it.

**[Phase 76](https://github.com/yielab/tack/blob/develop/docs/plans/phase-76.md) — the project
file is kept in sync.** ADR 0025 is proposed. Nothing is built, and the plan was written before
Phases 79 to 81 moved the code: re-read it against the tree before opening it.

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

Until it is opened, the project file stays what Phase 73 made it: an import that creates and
updates by name and never deletes.

**Status rules.** Optional, written in Phase 86's plan ("Considered and not adopted now"), no phase opened.
One list of rules per project, each "when this happens, and this holds, propose these changes": a run starting
moves its task, a run with changes suggests In Review, every child done marks the parent done. A rule builds
a change set with the rule as its actor, applied at once or left waiting, so a refusal is a visible reason.
It reads the same events as the triggers of Phase 85 and writes only the status.

## Cancelled

None of these was approved as a card. They are not scheduled and do not wait on anything; each
can come back as a card of its own.

- Publishing the release, the publish list, the installs on macOS and Windows, the
  code-signing decision and the GitHub and workstation items of Phases 65 and 69 (Wave 0). The
  release is the maintainer's own step, written in `docs/LAUNCH-CHECKLIST.md`.
- The live proof of My SaaS's code card through a team with a real install, which needs a paid
  model and docket's cooperation.
- Directives learnt from a person's corrections (ADR 0020, decision 6): Tack depends on no
  verifier, and the feature waited on a contract from one. The reasons a person gives stay
  stored (ADR 0013, decision 7).
- Measuring docket's polling, and the retakes of `agents-flow.gif` and `recovery-demo.gif`. The
  outdated `agents.png` and the two desktop screenshots were removed instead of retaken.
- Limits that stay as limits, each shown in the product with its reason: only docket stops a run at
  its token limit; the tokens a verifier spends count whole, cache included; only docket reports a
  running agent's steps on its timeline; a team's pending memory reaches the inbox in about five
  minutes (the runner looks at docket again every 300 s, the board reads the reports every 30 s) and
  is decided from the sheet only with a docket that lists `memoryDecide`, and with docket's own command
  otherwise; closing a thread in Tack does not close its docket conversation, which ends by its
  own turn cap or when its `turn` question expires; **Resume team** clears a pause but not the spent
  budget, so a team out of budget is paused again until the budget is raised.

---

## Delivered

Phases 73 to 87, newest first. The detail is in each phase's plan under
[`docs/closed-cycles/plans/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/plans),
each decision in its ADR under [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr),
and what a user can now do in the [changelog](https://github.com/yielab/tack/blob/develop/CHANGELOG.md).

- **Phase 91 — what docket offers, Tack uses or says why not.** No ADR of its own (it amends ADR 0034's note
  on `conversation` and ADR 0035's decision 1). docket 0.3.0-beta.1 lists 21 features and Tack used 16.
  A thread on a docket team continues one task turn by turn, and the conversation ends by docket's own
  turn cap or when its `turn` question expires, because the board's closing of a thread does not reach the
  runner. A team's pending memory is a **needs-you** notification in each project that uses the team,
  and its sheet shows what it learnt and what waits, with **Approve** and **Refuse** (which asks for a
  reason) when docket lists `memoryDecide`. A task docket refuses at claim for a method it cannot
  staff reads **Not started** with docket's reason. A person chooses what an agent profile does, so a
  profile that does not change code can run on a team that delivers a report. docket's read-only token
  is not used: every call goes through the runner, which holds the full token. Tack names docket
  0.3.0-beta.1 (the release docket declares) instead of a version never tagged.
- **Phase 89 — the board orchestrates and a team declares what it accepts.** ADR 0036. One owner per
  concept: the board decides what is done and by whom, the team decides how. One router
  (`GET /api/projects/{id}/routing`) serves Flow, Map and the item page; a parent card reads how many
  children are done, names the teams or agents involved and tells the inbox when the last one
  finishes, moving nothing. A team's report carries its contract (what it delivers, the methods it can
  run and the roles each lacks, what its work still needs); a profile with no team is a team of one,
  filled from what its runner reports, so every rule works with any agent and docket adds members,
  methods and folder teams when it reports them. A run is refused before it is sent when its team
  cannot do what it needs (the web, proposals, a repository, a method, an unmet need), and a profile's
  method is checked when it is saved. Agents → Teams shows each team as a sheet of six answers; each
  run says who applies each limit (run, step, team) and a team paused on its budget is resumed from
  the board; "Digital business" is a template of six departments with the teams it expects, compared with
  what docket reports, and the repository's teams are applied through the runner with the person
  approving the commands docket lists. Everything docket-specific is probed as a feature, and
  was checked on 2026-11-04 against the answers of a real docket (0.3.0-beta.1, API 6) recorded as the fake's
  fixtures: docket sends `delivers` as a list, a method that does not load carries an `error`, a
  budget's window is `none`, `day` or `week`, **Resume team** clears the pause but not the spent budget (the
  team is paused again within seconds until the budget is raised), and a team made from a shape takes
  the folder it works in as `location`, so Tack sends it only for `build`. The board now has a channel to ask a runner for something
  (`POST /operations/poll`, `/operations/{id}/result`), additive in runner v1.
- **Phase 88 — a person is told what needs them.** ADR 0035. One inbox fed by the board's own events:
  a question a run asks, a run that fails and a change set an agent proposes each need you; a run that
  finishes and an item due within the hour inform you. It closes itself when the situation ends, however
  it ends, and is repaired from the store at start and after a lost event. The web shows a bell with the
  count, a drawer, a notice and the count in the tab's title (and the browser's notification when the tab
  is hidden, if you turn it on); the desktop app sends the system's notification and counts on the tray;
  a webhook receives each one signed, retried with backoff and with its id, and Settings → Automation
  shows each channel's state. Phase 10's webhook (`item.*`, `sprint.*`, `item.due_soon`) is gone.
- **Phase 87 — a step that runs out of budget stops at a checkpoint, not a cliff.** No ADR of its
  own (it follows docket's own ADR 0038). A step that ran out of its rounds, tool calls or time ends
  `step_budget`, names the limit and where to raise it, and keeps its patch; one that ran out of
  tokens keeps today's `budget` code. A person then chooses **Continue** (a new request that
  retries the same docket task) or **Start over from this run's work** (a new task based on the
  branch it kept, with twice the token limit when tokens were what ran out). Live-proved against a
  real docket (P59-6 to P59-8) and a real Tack on a local model; the proof found and fixed two bugs
  the fake alone had not caught.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-87.md).

- **Phase 86 — a person sets a task's status, the agents report its work.** No ADR (it amends
  ADR 0033, decision 1). The six places that moved a status because a run did something are removed
  with their settings; auto-start starts a task once and moves nothing; the item and its card show
  **Status** and **Work** as two things.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-86.md).
- **Phase 85 — a manager works through the board.** ADR 0034. A profile becomes a manager by being given
  general parts, none called "manager": a credential with scopes, change sets as an agent's only way
  to change the board, a team's activity, a computed digest, threads where each message is a turn, and
  triggers on events. Through docket a manager runs at L3 or L4.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-85.md).
- **Phase 84 — the screen says what needs you first.** ADR 0033. One run-state language on every
  screen, each screen ordered by what needs a person, the Table view folded into the List, and a new
  work view, Orchestration (Flow: a lane per team; Map: roles to members).
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-84.md).
- **Phase 83 — a team is shown as it is.** ADR 0032. Agents → Teams lists every team any agent
  reports; a task says who does it; a run's findings reach one accept panel from a plan file, a team's
  records or an uploaded file, and a person always accepts.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-83.md).
- **Phase 82 — the task goes to docket as fields.** ADRs 0030 and 0031. The Execution tab shows which
  prompts ran, what the agent assumed and asked; a team may work in a folder with no repository; a wait
  reads as a wait, and an action that leaves the team is seen whole before it is approved.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-82.md).
- **Phase 81 — what the audit found.** No ADR. Background work with one owner that stops on shutdown,
  typed answers from the docket and prill adapters, tests for routes and upgrade paths nobody covered,
  one wire format and one error model, one UI kit. The rule it keeps: CI never needs docket or prill;
  everything that talks to the real programs is the local `onboarding-lab/e2e/docket-prill-e2e.sh`.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-81.md).
- **Phase 80 — a run that waits on a person.** A docket run parked on a question is no longer
  cancelled by the attempt's own time limit; every wait for a person is one escalation record, answered
  by one route. The autonomy level goes on a task only when a person asks for one, never above the
  team's ceiling.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-80.md).
- **Phase 79 — the crates say what depends on what.** ADR 0028. The runner-v1 contract is a crate of its
  own, every SQL line is in `tack-db`, the rules of a run are use cases, and the pre-push hook fails on
  any crate edge the plan does not allow. Nothing on the wire changed.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-79.md).
- **Phase 78 — one pass over what was still open.** No plan, no ADR: the desktop app's first-run fixes,
  the board audit, a page of its own for each item with a run card and an Execution tab, and "Done when"
  checks a person can tick.
- **Phase 77 — the agent's copy is prepared.** ADR 0027. Whoever makes the agent's copy installs its
  dependencies from the lockfile, and every run has a limit that says where it comes from.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-77.md).
- **Phase 75 — limits in tokens.** ADR 0024. A limit is in tokens and nothing has one by default; cache
  reads are shown and never counted; every limit says who applies it; dollars are an estimate shown
  last; what Tack sends an agent carries nothing twice.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-75.md).
- **Phase 74 — what the first real project found.** ADR 0023. Twenty defects fixed where each fact is
  owned: a run that cannot start ends Failed with the reason, one rule says whether a computer is
  online, and the docket contract is proven by answers recorded from a real docket.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-74.md).
- **Phase 73 — every run goes to a team, and automation is one ladder.** ADRs 0021 and 0022. A project
  role says which profile, and through it which team, takes a task; a task can start by itself and
  repeat; the automatic merge never rises above the team's ceiling; Settings → Automation is a ladder of
  four rungs; a whole project goes in and out through one file.
  [Plan](https://github.com/yielab/tack/blob/develop/docs/closed-cycles/plans/phase-73.md).

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

[CONTRIBUTING.md](https://github.com/yielab/tack/blob/develop/CONTRIBUTING.md) covers setup and the
pull request process; [Adding Features](developer/adding-features.md) walks through the common
extension patterns.
