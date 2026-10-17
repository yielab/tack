> **Archived 2026-10-17 from `docs/book/src/roadmap.md`, verbatim.** The chapters of Phases 65 to 72 and of the first live docket + prill run. Phases 66 to 72 shipped as `v0.1.0-beta.10`; what Phases 65 and 69 left to the maintainer is listed under "Open" in the roadmap. Relative links are as they were in the roadmap. One status line was stale when it was archived: Phase 72 closed on 2026-10-14, and its last task read the required stages and what to fix off the pack instead of waiting for the verifier.

## Phase 65 — the release, and every open item in one sequence

**Status:** open 2026-09-20; Waves 1–4 landed 2026-09-21 and `v0.1.0-beta.9` shipped the
same day — the first tag with the four-harness fleet and the desktop bundles
(`.AppImage`, `.deb`, two `.dmg`, `.msi`). What remains of Wave 0 is the publish list
and the platforms this machine cannot verify.
**Plan:** `docs/plans/phase-65.md`.

| Wave | What lands | Who |
|---|---|---|
| 0 | Ruleset required checks, branch and build-dir cleanup, the Dependabot decision, the repository description, the `v0.1.0-beta.9` tag, the publish list, one macOS and one Windows install, the signing decision. | the user; refused to agents or costs money |
| 1 | Measurements of codex, opencode and docket's contract, each a captured fixture; `tack start` / `tack open`; inbound GitHub issue state by polling. | Sonnet agents |
| 2 | The capture cap leaves the harness descriptor; opencode's served model; GitHub comments both ways. | Sonnet agents |
| 3 | codex reads usage and the served model, then applies the permission policy; opencode attempts share one package cache; per-project GitHub token and manual issue link. | Sonnet agents |
| 4 | A run that asks before it acts on codex and opencode where the measurement allows it; docket cancel, artifacts and asking once docket ships `harness-v1.1`. | Sonnet agents |

What Waves 1–4 build ships as `v0.1.0-beta.9`. Nothing in the phase waits on a decision:
the two it needed — inbound sync polls rather than receiving webhooks; opencode attempts
share a package cache and nothing else — are taken in the plan and are one line each to
reverse.

---

## Phase 66 — close the Level 3 loop: evidence, briefs, escalation packs, merge-readiness

**Status:** closed 2026-10-05; every task in the table below is built and merged, and ships as `v0.1.0-beta.10`. What is still open is listed under the table. **Plan:** [`docs/plans/phase-66.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-66.md).
**ADRs:** 0069 (the brief is an entity on the item and travels), 0070 (docket contract 1.1,
negotiated at boot; cancel becomes per-harness evidence), 0071 (evidence before deletion, the
verifier boundary, the pushed branch and its pull request), 0072 (the "Run with agent" flow).

After this phase every attempt leaves a patch, both commits and a file list; an item can
carry a typed brief that reaches the harness and an independent verifier; a question arrives
as a pack with options and a recommendation; a verifier's merge-readiness pack is rendered
and accepted or rejected with a reason; a branch is pushed and its pull request followed to
merged, closed or reverted; and one page shows escalation rate, human minutes, the
verification tax in tokens and the acceptance rate. The verifier itself is a separate program
Tack never contains.

The interface comes first. The "Run with agent" dialog shows the whole flow from the second
wave on — who runs it, what it gets, how far it may go, what happens after — and a step whose
integration has not landed is shown disabled with the reason, never hidden.

docket is a moving target by design. Tack negotiates the harness contract and probes each
optional flag when the runner starts, so one Tack build works with docket `0.2.0-beta.3`, the
coming `0.2.0-beta.4` and what follows; each docket link below starts when docket's own board
merges the card it needs, not when docket cuts a release.

| Wave | What lands | Who |
|---|---|---|
| 0 | Decided 2026-10-03. | the maintainer |
| 1 | Evidence captured before the workspace is deleted; the phase's four migrations; `status_map_policy_id` moves the item; the `mrp-v1` and `brief-v1` contracts; each harness's tool list, measured; the README leads with the desktop app. | Sonnet and Haiku agents |
| 2 | The whole flow in the dialog, deferred steps disabled; the brief's routes and export; the consultation pack; the verifier step behind `[verify]`. | Sonnet agents |
| 3 | The brief travels to the harness and the evidence; the brief editor; the merge-readiness review record; the pack in the runner and the inbox. | Sonnet agents |
| 4–5 | The merge-readiness panel; the runner pushes the branch behind `[git]`; the pull request and its fate from the poll that already runs; verification and push become live controls in the dialog. | Sonnet agents |
| 6–7 | Factory metrics, endpoint then page; the pull-request badge. | Sonnet, then Haiku |
| A | docket in five links, following docket's Phase 35: negotiation and contract 1.1; process events and a proven cancel; asking over stdin; limits, policy and files; the recipe flag. | Sonnet; the last link Haiku |

Left open by the work, each listed in the
[Agent Runners](user-guide/running-agents.md) chapter's "Known gaps" or beside the feature it limits: a docket recipe does not run,
because docket runs one only on a contract that needs a model endpoint Tack does not hold; a docket run that times out, rather than being cancelled, stops
only its main process group; the branch push runs git in a workspace the agent could have
reconfigured; a docket request must name docket's own tools; and
`tack runner doctor` does not yet print each harness's `artifacts` and `permission_policy`
lines. Phase 69 closed the push (hardened git, a pinned push URL); Phase 70 the timeout and the doctor items. Opening a pull request as a choice in the "Run with agent" dialog is shown disabled:
a pull request is opened for a pushed branch whose item is linked to a GitHub issue.

---

## Phase 67 — a run a normal person can start

**Status:** closed 2026-10-08; every task built and merged, and ships as `v0.1.0-beta.10`.
**Plan:** [`docs/plans/phase-67.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-67.md).
**ADR:** 0074 (amends 0069, 0071 and 0072).

A manual run of the Phase 66 build produced fourteen findings; three lost or hid work. After this phase
a project is created knowing where its code is, and the agent works on a new branch of that
folder, in the folder itself, or in a clone for another computer. No model is
asked for: the agent's own is the default. Four built-in profiles (Implementer, Reviewer,
Researcher, Planner) carry their tools. A task is a description plus acceptance criteria,
with what only the agent needs collapsed. The dialog is a pre-flight that shows what the
agent will read. A finished run flags the task **Needs your review** and hands over the diff,
the folder, the branch and the result. Every word on screen is Tack's, with a `(?)` beside it
and a help button into this book.

| Wave | What lands | Who |
|---|---|---|
| 0 | Stop losing data: evidence against the resolved commit; a workspace whose evidence failed is kept; the failure reason typed and shown; the outcome leads the card; native controls follow the palette; the provider derived, never typed. | Sonnet and Haiku agents |
| 1 | The agent's default model: measured ids per agent; the harness says whether it needs one; the scheduler believes it. | Sonnet agents |
| 2 | The project knows where its code is: the migrations; project creation asks; Settings → Automation; the Agents page is about this computer; a test run with nothing typed; four profiles seeded and edited in a form. | Sonnet and Haiku agents |
| 3 | The agent works where the code lives: a worktree on `tack/<task>`; the folder itself; the server fills the repository from the project. | Sonnet agents |
| 4 | The task and the dialog: the form; what the agent will read, on a route; the pre-flight; remembered settings; the review flag and the verdict; the tools to see what it did; the Planner's plan and the subtasks a person creates from it. | Sonnet agents |
| 5 | Tack's words; `(?)` on every field and a help button; the book describes the flow; the e2e specs follow the new screens. | Sonnet agents |

---

## Phase 68 — what Phase 67 left on the table

**Status:** opened and closed 2026-10-08; every round built and merged, and ships with Phase 67
as `v0.1.0-beta.10`. **Plan:** [`docs/plans/phase-68.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-68.md).

Fourteen follow-ups, each found by the agent that built the task beside it, each finishing a
decision ADR 0074 already takes. No new decision. The end-to-end test of the Agents page's
test run found two bugs on the way (the test run picked an agent that needs a model, and the
embedded runner reported its name instead of its id); both are fixed. What this phase left
open is in its plan's Status section; the one that matters most is that a run no runner can
claim waits in Queued without saying why.

| Round | What lands | Who |
|---|---|---|
| 1 | A workspace the runner could not keep is left in place, never deleted; run settings and the folder check are named shapes on the wire. | Sonnet agents |
| 2 | A subtask created from a plan records the plan; the Automation panel writes a specific model and the definition of done, and shows a rejected folder under the field. | Sonnet agents |
| 3 | The embedded runner says which runner it is; the dialog remembers the profile only when a person picked it; the card finds the folder of a run in your own repo. | Haiku, Sonnet |
| 4 | `Select` takes a `(?)`; the popover closes; a question from the agent and a review you owe no longer share a colour; the agents page's test run is asserted end to end. | Sonnet, Haiku |

---

## The first live docket + prill run

**Status:** done 2026-10-10 and 2026-10-11, to ship with Phases 67 and 68.
**ADR:** 0075 (amends 0074 and 0071). **Proof:** `scripts/docket-prill-e2e.sh`.

The whole workflow, run through the UI with docket as the agent and prill as the verifier, both
built from source, against a local model: a project, its runner, its settings, a bug, a
feature, a task and a run that asks, each verified and reviewed. The first run could not finish
without nine changes; the second found five more. After them docket and opencode are offered
wherever an agent is, prill is the runner's own verifier, a local OpenAI-compatible server
reaches docket and opencode, a run that asks reads **Asking you**, a running agent's steps fill
its timeline as it takes them, and docket's question says which command it would allow and why.

| Run | What lands |
|---|---|
| 1 | docket and opencode everywhere; prill spoken natively; `waiting_decision` while a question is open; the request follows its attempt; `tack serve --runner-config`; the Automation model list; one request to create a project; the `mrp-v1` schema. |
| 2 | A running agent's steps on its timeline (docket first); one sequence per runner event; docket's question names the command; the e2e pins prill's image by id; a refresh keeps an opened attempt open. |

Next here: codex, claude-code and opencode report their steps too, once their tool lines are
captured from the real binaries.

---

## Phase 69 — the agent reaches only what you gave it

**Status:** opened and closed 2026-10-12; all four rounds merged into `develop`. The GitHub settings of
Wave 0 and the re-cut tag are the maintainer's.
**ADR:** 0077 (accepted). **Plan:** [`docs/plans/phase-69.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-69.md).
Ships as the re-cut `v0.1.0-beta.10`; the tag of 2026-10-12 was withdrawn the same day.

A security audit on 2026-10-12 found no planted code and no leaked secret, and found four
properties that together let a prompt injection in a card or a repository file read your keys or
merge code unseen: the board answers anyone when no token is set, a web page can post to it, an
agent runs with permissions off and the same file access as Tack, and the evidence you review is
computed from names the agent can move. This phase closes all four and makes every rule a test
in a suite CI requires.

| Round | What lands | Who |
|---|---|---|
| 1 | A request must name this server; a raw body states its type; nothing imported is trusted; no credential reaches a log; the run's environment is the runner's to grant; the runner's git never runs the repository's code; a harness ends when its process ends; the desktop window trusts one origin; a release is built from nothing restored. | Sonnet, Haiku |
| 2 | The API always has a token the browser cannot read; the server records the profile and the repository; the pull request opens when you accept; the evidence is the base you chose and the tree that was verified; the run log leaves the workspace; `serde_yaml` is replaced. | Sonnet, Haiku |
| 3 | A run started over MCP always asks; untrusted text is fenced and the prompt you see is the prompt that runs; the agent is sandboxed with `bwrap` where Linux has it; the browser holds the cookie. | Sonnet, Haiku |
| 4 | The dialog and the run page tell the truth (sandboxed or not, "Web tools", what the agent got and read); the two boundary suites become required checks; `SECURITY.md` and the book say what the agent can reach. | Sonnet, Haiku |

---

## Phase 70 — what is still open before the re-cut

**Status:** opened and closed 2026-10-12; every task merged into `develop`. No ADR: every task
is a follow-up an earlier phase wrote down.
**Plan:** [`docs/plans/phase-70.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-70.md).
Ships with Phase 69 as the re-cut `v0.1.0-beta.10`.

A request no runner can claim stops sitting in Queued without a word: the card says why it waits,
and after half an hour of nobody being able to take it, it fails with that reason. The rest
settles what the book still lists as known gaps: a test decides whether a docket run that times
out leaves a process behind, `tack runner doctor` says per harness what it keeps and whether it
can ask, a run setting that no longer fits is logged, the embedded runner's id is tested against
a real enrolment, and the real-`bwrap` boundary test stops skipping in CI.

| Round | What lands | Who |
|---|---|---|
| 1 | Why a request waits, recorded and timed out; the docket timeout test; per-harness doctor lines; the run-setting warning and the real runner-id test; `bwrap` in CI. | Sonnet, Haiku |
| 2 | The card and the run page say why a request waits. | Sonnet |

---

## Phase 71 — the agent brings its own model

**Status:** opened and closed 2026-10-13; every round merged into `develop`.
**ADR:** 0078. **Plan:** [`docs/plans/phase-71.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-71.md).

Tack stopped holding model keys, endpoints and catalogs: the gateway key, the "Through" control
and the provider settings are gone. Each agent uses its own login and configuration; Tack asks it
which models it offers and whether it is signed in. A run names its model as one string, or none
for the agent's default, and the model field is the same in the run dialog and the Automation
panel. The Agents page's third step is **Sign in**, read from each agent. docket runs from its
own setup on harness contract 1.2 (docket `0.2.0-beta.5`).

| Round | What lands | Who |
|---|---|---|
| 0–1 | Fixtures from the real agents; the runner and the app hold no model credential. | integrator, Sonnet |
| 2–3 | One model string on the wire, in the database and in every agent's command line; each agent's models and sign-in, probed; opencode on its own login. | Sonnet |
| 4–5 | One model field and the Sign in step; docket on contract 1.2; the docs, the changelog and `smoke.sh`. | Sonnet, integrator |

---

## Phase 72 — merging from Tack, by hand or by policy

**Status:** proposed 2026-10-13; the owner answered every open question the same day.
**ADR:** 0079 (amends 0069, 0071 and 0075). **Plan:** [`docs/plans/phase-72.md`](https://github.com/yielab/tack/blob/develop/docs/plans/phase-72.md).

A finished change can be merged from Tack in three ways: a **Merge** button on the task, a
project option that merges when a person moves the task to Done, and, for a project that allows
it, by Tack itself (Level 4 inside authorised paths, Level 5 everywhere). An automatic merge needs
the verifier's `merge` under the stages the project requires, a signature bound to the exact
commit, green CI on that commit and an unspent budget; when any is missing the task waits for a
person and its card says why. A rejected change goes back to the agent with the verifier's next
steps a bounded number of times first. A revert takes the level down by itself. Briefs keep their
history, and the Factory page shows the verifier's quality figures beside Tack's own cost.

Each responsibility has one owner. Tack owns the brief, the loop back to the agent, the merge,
every GitHub read and write, the budgets and the money. The verifier owns the verdict, the
required stages, the signature and every quality figure. The agent runtime owns running the
work. Tack computes nothing the verifier already measures.

| Round | What lands | Who |
|---|---|---|
| 0 | Fixtures from the real verifier and GitHub; the phase's one migration; the merge policy as a pure function. | integrator, Haiku |
| 1–2 | Brief history; reading CI and reverts, and the merge call; signed packs; budgets that stop new work and say why. | Sonnet |
| 3–4 | The automatic merge, its settings and demotion; the verifier's figures and Tack's prices; the Merge button and merge on Done; the loop back to the agent. | Sonnet, Haiku |
| 5 | The Automation panel, the button and the card that says why a merge did not happen; the runner sends the required stages and reads the next steps, once the verifier ships them. | Sonnet |

Waits on: the verifier's next milestone (required stages and next steps) for the automatic merge
and the loop; until then both hand the change to a person. Directives learnt from human
corrections wait for the verifier to propose them; Tack will apply an approved one to `AGENTS.md`
through a pull request.
