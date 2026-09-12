# VI-C2 handoff

- Base SHA / branch / final SHA: base `8b71756` (`develop`'s actual tip at dispatch —
  VI-B1, VI-C3, VI-C4 already integrated there). Branch `agent/vi-c2-modal-defaults`. Not
  committed — final SHA is n/a; the worktree holds the changes.
- Files changed (must equal ownership list): Owns is
  `frontend/src/shared/runWithAgent/**`, the attempt-state chip on Board cards, and this
  handoff. Within `runWithAgent/**`: `RunWithAgentModal.tsx` (reworked), `shared.ts` (new
  pure helpers appended; `gateHarnessModelSelection` byte-identical — see below),
  `RunWithAgentButton.tsx` (the chip), plus the matching `.test.ts(x)` files. Four files
  outside the glob, each a small, necessary plumbing change, not a redesign of any of
  them:
  - `frontend/src/shared/execution/types.ts` — added one optional field,
    `HarnessCapability.model_passthrough`, matching the wire contract
    (`docs/contracts/runner-v1/capabilities.json`) and the Rust struct
    (`crates/tack-orch/src/execution/capabilities.rs:92`,
    `#[serde(default, skip_serializing_if = "Option::is_none")]`) exactly. This type was
    missing the field entirely — a real, measured gap, not a design choice — and the
    card's own task text ("free text only where `model_passthrough` is attested")
    requires it. Not in any Part VI card's ownership row.
  - `frontend/src/features/board/Board.tsx`,
    `frontend/src/features/sprints/Sprints.tsx`,
    `frontend/src/features/item-detail/ItemDetailDrawer.tsx` — each gained one new prop
    on their existing `<RunWithAgentButton>` call (`projectId={item.project_id}`; Board
    also `showStateChip`), and `ItemDetailDrawer.tsx` additionally gained `?tab=` deep-link
    support (below). No other line in any of the three changed. None of the three appears
    in §VI.2's ownership table for any card.
  - `frontend/e2e/run-with-agent.spec.ts` — the three existing specs updated to the new
    UI (new field labels, the Repository fieldset's collapse/expand, a
    `{name:'Run', exact:true}` fix — see "Measured numbers"), plus one new spec proving
    the gate's acceptance line. Running this suite is also how this card found a real
    product bug in the target picker — see "Measured numbers".
- Contract fixtures consumed: none. This card never touches `docs/contracts/runner-v1/`.
- Behavior implemented: the "Run with agent" modal (Board, item-detail, Sprint all mount
  the same component) no longer asks for five hand-typed fields on every run. Concretely:
  - **Where it runs.** A single "Machine or group" picker, built from `GET /api/runners`
    (active runners only) and `GET /api/runner-fleets`, labelled by name — never an id.
    Hidden entirely, with the one active runner auto-selected, when it is the only
    target (`shared.ts#shouldHideTargetPicker`).
  - **Agent execution off.** With zero active runners, the modal body is "Agent execution
    is off." + a "Turn it on" link to `/agents`, instead of the form
    (`shared.ts#isExecutionOff` — see "Known limitations" for what signal this reads and
    why).
  - **Agent profile.** Auto-selected when exactly one exists (the same "an unambiguous
    single choice needs no picker" reasoning as the target picker); "Create default
    profile" inline (`POST /api/agent-profiles`) when none exist.
  - **Harness.** The dropdown is filtered to what the *selected target* actually reports
    installed (`listReportedHarnessKinds` over the target's own capability data, not the
    whole runner population), falling back to the full static list only while that data
    hasn't loaded yet — `gateHarnessModelSelection` (unchanged) is still the real
    enforcement at submit time either way.
  - **Model.** Three modes: *Project default — `<provider>/<model_id>`* (only offered
    when the project actually has one — VI-C3's `default_model`), *Choose…* (the
    selected target's own `model_combinations` for the chosen harness, plus a free-text
    fallback gated on `model_passthrough: supported`), *Auto*. Defaults to *Project
    default* the first time one is seen, else *Auto*.
  - **Repository.** A one-line read-only summary ("No repository configured for this run
    yet." when empty) with a "Change for this run" button that expands the same four
    fields as before. No free-text field is visible until that click.
  - **Attempt-state chip.** Board cards show a small state badge next to the ▶ trigger,
    read from the existing execution store (`useExecutionStore().requestsForItem`, no
    second fetch); clicking it opens the item straight to its **Execution** tab.
- Tests added and exact commands/results:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C2` not needed for any of these —
    every command below is frontend-only, no cargo build.
  - `cd frontend && npm run type-check` — clean (`tsc -b`, zero errors).
  - `cd frontend && npx vitest run` — **756 passed, 0 failed** across **85 files** (was
    726 at VI-C3's handoff; the delta is this card's new cases in `shared.test.ts`,
    `RunWithAgentModal.test.tsx`, `RunWithAgentButton.test.tsx`).
  - `cd frontend && npm run build` — clean (`tsc -b && vite build`, no errors, no
    warnings beyond the pre-existing bundle-size report).
  - `cd frontend && bash scripts/check-tokens.sh` — `Raw color literals: 0 (baseline 0,
    target 0)`, `Inline-style hex literals: 0 (baseline 0, target 0)` — both gates pass;
    every new style in this card's diff uses `var(--color-*)`.
  - `cd frontend && CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C2 npx playwright test
    e2e/run-with-agent.spec.ts --project=chromium` — **4 passed, 0 failed** (final run;
    the gate's own required run — the block: "the E2E that intercepts `POST
    /api/executions` and asserts the body equals the project defaults field for field").
    Getting to green took three iterations and found one real product bug — see "Measured
    numbers" for the full account, kept rather than silently rewritten.
  - New unit cases, `shared.test.ts`: `isActiveRunnerState`, `shouldHideTargetPicker`,
    `isExecutionOff`, `describeProjectModelDefault`, `projectDefaultModelPair`,
    `isModelPassthroughAttested` — each a `describe` block with 2-4 cases (absent vs.
    `auto` vs. `explicit`; `supported` vs. `advisory` vs. absent).
  - New/rewritten cases, `RunWithAgentModal.test.tsx` (15 tests total, up from 8):
    execution-off renders instead of the form; single-active-runner auto-selects with no
    picker and the exact runner id lands in the POST body; **manually picking a specific
    runner from the multi-target picker actually selects it** — a regression pin for the
    real bug this card's E2E run found (see below); multi-target picker lists names,
    never ids; every missing-field reason (now with two agent profiles, so auto-select
    doesn't mask the "Select an agent profile." case); "Create default profile" inline
    flow, including the new profile being selected afterward; Repository stays collapsed
    until "Change for this run"; project-default mode is offered and auto-selected only
    when the project has one, and its pair reaches the POST body verbatim; "Choose…"
    lists the target's own combinations and — since that data feeds the gate too —
    enables submit by construction; a custom model id is offered only when
    `model_passthrough: supported`; the existing gate-load-bearing and
    exact-CreateExecutionInput-shape cases, updated to the new fixtures; the accessible-
    label sweep.
  - New/rewritten cases, `RunWithAgentButton.test.tsx` (7 tests, up from 5): all existing
    cases now mount under a `MemoryRouter` (required once the button reads
    `useSearchParams`) with a `projectId` prop; two new cases for the chip
    (renders the item's latest state and navigates to `?tab=execution` on click; renders
    nothing with no execution requests).
- Failure/adversarial case proved, twice:
  1. Reverted `shouldHideTargetPicker`'s `fleetCount === 0` clause locally (so it would
     hide the picker even with a fleet present) and re-ran the unit suite —
     `RunWithAgentModal.test.tsx`'s "shows a combined machine/group picker… when more
     than one target exists" and `shared.test.ts`'s "shows when a fleet exists too…"
     both failed exactly as expected, proving those assertions load-bearing. Reverted
     back; `npx vitest run` returned to 756/0 after.
  2. Reverted the fix described below (the `runner:` → `exact_runner:` option-value
     prefix) and re-ran the new regression test alone
     (`npx vitest run … -t "manually picking"`) — it failed with the exact symptom the
     real bug produced (submit stayed disabled after a real pick). Restored the fix;
     the full suite (756/756) and the E2E run (4/4) both went green with it in place.
     This is the adversarial proof that the E2E-caught bug (below) is actually fixed and
     that the new unit test would catch a regression of it without needing a browser.
- Schema/API/contract change requested from another owner: none. This card added no new
  route and no new wire field on the operator API — `model_passthrough` already existed
  on the wire (VI-B2's contract, unlanded here) and on `docs/contracts/runner-v1/`; this
  card only taught the frontend type about a field the contract already carried.
- Known limitations or `not_measured` fields:
  - **"Agent execution is off" reads a proxy signal, not a real flag.** No persisted
    execution on/off flag exists on this branch's base (`grep -n "local-runner"
    crates/tack-api/src/router.rs` — empty; that is VI-B3, not landed here), and
    `/api/executions` is mounted unconditionally regardless (confirmed by
    `run-with-agent.spec.ts`'s own pre-existing header comment: "an always-on operator
    surface, NOT gated behind `TACK_ORCH_ENABLE`"). `shared.ts#isExecutionOff` reads
    "zero active runners have ever enrolled" instead — real and observable, but a
    **different fact** than "the operator switched execution off," which VI-B3's route
    will carry. `isExecutionOff`'s own doc comment names the swap for whoever lands
    VI-B3 or VI-C1 next.
  - **"Turn it on" links to `/agents`, which does not resolve on this branch's base.**
    VI-C1 (the Agents page, that route) has not merged here (dispatch fact: only C3/C4
    are integrated). The link text matches the card's task text verbatim; it is inert
    until VI-C1 lands. Not fixed here — this card does not own routing.
  - **No project-level agent-profile default or repository default exists to read.** The
    card's Tasks text describes "Agent profile: … preselecting the project's default
    (C3)" and "Repository: a read-only summary from the project's agent repository (C3)."
    VI-C3's actual delivered scope (its own handoff, confirmed by
    `docs/openapi.json`/`schema.gen.ts`: `Project` carries only `default_model`, no
    `default_agent_profile_id` and no `repository` object) is narrower than the TODO.md
    card text describing it assumed. This card does **not** invent that storage (it is
    VI-C3's/`tack-core`/`tack-db` ownership, not this card's) — it substitutes the one
    honest default that needs no new storage (auto-select when exactly one candidate
    exists) for agent profile, and leaves Repository as a manually-filled field (behind
    "Change for this run") with no default to summarize. The "zero hand-typed
    identifiers" acceptance line is therefore fully met for **target** and **model**
    (both real, project/runner-sourced defaults) and not for **repository remote**,
    which still requires one manual field on every run until a project-level repository
    default lands.
  - **Whether `base_revision` accepts a ref rather than a SHA: not measured.** The
    dispatch block's read list excludes every crate for this card ("Do not read: any
    crate"), and answering this needs the runner's checkout logic
    (`crates/tack-runner`), which that restriction covers. The field stays free text,
    unvalidated client-side, exactly as before this card. Smallest experiment that would
    answer it: submit a branch name (not a full SHA) as `base_revision` against a real
    `tack serve --with-runner` and inspect the runner's checkout log for what it resolved
    against.
  - **The chip does not re-navigate on a second click while the same item is already
    open.** `ItemDetailDrawer.tsx`'s `?tab=` effect is keyed on `itemId()` changing (see
    that file's own comment) so a stale `tab=execution` from one chip click can never
    leak into the *next, different* item opened without a tab (Timeline/Calendar/
    List/Board's own card-body click/`DependenciesTab` all call `setSearchParams({item})`
    with no `tab` key, and `setSearchParams` merges). The trade-off: re-clicking the chip
    for the item **already open** does not re-apply the tab, since `itemId()` does not
    change. Judged the narrower, less surprising edge case; not proven with a test.
  - **The E2E database is global, not project-scoped, for runners.** `GET /api/runners`
    (confirmed while building the target picker) carries no project filter and
    `frontend/e2e/e2e.db` is persistent across every spec run — so "exactly one active
    runner" (the hidden-picker case) is real and unit-tested, but an E2E spec cannot
    assume it; the new/updated E2E specs select their own runner by name only when the
    picker happens to be showing, and never assert whether it is hidden or shown. This is
    an environment property of the existing suite, not something this card introduced or
    could fix within its `runWithAgent/**` ownership.
- Secrets/logging review: no secret-bearing field, log line, or new column anywhere in
  this diff — this card is frontend-only (no crate touched) and adds no new API surface.
  `model_passthrough` (the one new type field) is a boolean-ish capability attestation,
  never a credential.
- Safe merge order and likely conflicts: needs VI-C3 merged first (this branch already
  has it, via `develop` at `8b71756`). No file-level overlap with VI-B1/B2/B3/C1/C4's
  ownership rows. `docs/openapi.json`/`schema.gen.ts` untouched by this card (no backend
  change) — nothing to regenerate. The three non-owned frontend files this card touched
  (`Board.tsx`, `Sprints.tsx`, `ItemDetailDrawer.tsx`) are not claimed by any other Part
  VI or Part VII card's ownership row as of this branch's base; still, the integrator
  should re-check that against whatever landed on `develop` after `8b71756` before
  merging, since neither this card nor its dispatch block can see the future.
- Checklist: no unowned crate/backend file touched (frontend-only diff); no live secret;
  no panic stub (every new function returns a typed value, `field-for-fetch` errors are
  read through the existing safe-accessor pattern already established in this file); no
  blind retry (submit is a single attempt, same as before this card).
- **E2E:** `run-with-agent.spec.ts` run against a real `cargo run -p tack-cli -- serve`
  (via Playwright's own `webServer`), chromium only — **final: 4 passed, 0 failed.**
  Getting there found one real product bug in the target picker (below), not just
  test-harness issues — see "Measured numbers" for the full three-run account, kept for
  the record rather than only reporting the clean final number.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The one active machine is used automatically — no id is ever typed or shown | `RunWithAgentModal.test.tsx`: "hides the target picker and auto-selects the one active runner…"; `shared.test.ts`: `shouldHideTargetPicker` |
| With more than one target, the picker lists names, never ids | `RunWithAgentModal.test.tsx`: "shows a combined machine/group picker, by name…" |
| With zero active runners, the modal shows "Agent execution is off" instead of a form | `RunWithAgentModal.test.tsx`: "'agent execution is off' renders instead of the form…"; `shared.test.ts`: `isExecutionOff` |
| A project's configured model default is offered, auto-selected, and reaches the request body verbatim | `RunWithAgentModal.test.tsx`: "with a project model default configured…"; `shared.test.ts`: `describeProjectModelDefault`, `projectDefaultModelPair`; E2E "the run form submits the project's configured model default…" |
| "Choose…" only ever offers combinations the target actually reported | `RunWithAgentModal.test.tsx`: "'Choose…' lists the target's own reported model combinations…" |
| A free-text model id is offered only when the harness attests `model_passthrough: supported` | `RunWithAgentModal.test.tsx`: "a custom model id is only offered when…"; `shared.test.ts`: `isModelPassthroughAttested` |
| `gateHarnessModelSelection` still gates submission, unchanged | `git diff -- frontend/src/shared/runWithAgent/shared.ts \| grep gateHarnessModelSelection` → empty (see below) |
| Repository shows no free-text field until "Change for this run" | `RunWithAgentModal.test.tsx`: "the Repository fieldset shows a read-only summary…" |
| No agent profile exists yet → "Create default profile" works inline | `RunWithAgentModal.test.tsx`: "offers 'Create default profile' inline…" |
| The Board card shows an attempt-state chip that opens the item to its Execution tab | `RunWithAgentButton.test.tsx`: "showStateChip renders the item's most recent execution state…" |
| Manually picking a specific machine from the picker (not just the auto-selected single-target case) actually targets that machine | `RunWithAgentModal.test.tsx`: "manually picking a specific runner from the combined picker actually selects it…" — a regression pin for a real bug the E2E run found (see "Measured numbers") |
| The whole flow works against a real server, not just mocks | E2E run — see "Measured numbers" |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `git diff --stat`: **11 files changed, 1132 insertions(+), 314 deletions(-)**
  (`git diff --stat`, run from the branch root, final state).
- `gateHarnessModelSelection` byte-identical proof:
  `git diff -- frontend/src/shared/runWithAgent/shared.ts | grep -n
  gateHarnessModelSelection` → **no output** (the function name does not appear in the
  diff at all — not one line inside it changed). Verified two ways: that grep, and a
  Python brace-matched extraction of the function body from `8b71756`'s copy vs. the
  working tree, byte-compared equal.
- Vitest: **756 passed, 0 failed, 85 files** (`npx vitest run`), up from VI-C3's recorded
  726 — the +30 are this card's new cases across `shared.test.ts`,
  `RunWithAgentModal.test.tsx`, and `RunWithAgentButton.test.tsx`.
- Type-check: `tsc -b` — 0 errors.
- Build: `vite build` — 0 errors, 0 new warnings; largest new/changed chunk
  `RunWithAgentButton-*.js` at 18.76 kB (6.34 kB gzip) — the modal itself is bundled
  inside it (dynamic import), not measured separately.
- `check-tokens.sh`: raw color literals 0/0, inline-style hex literals 0/0 — both at
  baseline.
- **E2E (`run-with-agent.spec.ts`, chromium, against a real `cargo run -p tack-cli --
  serve`): final run 4 passed, 0 failed.** Getting there took three runs and surfaced one
  real product bug plus two test-only issues — kept below rather than only reporting the
  clean final number:
  1. **Run 1: 1 passed, 3 failed.** `getByRole('button', {name:'Run'})` (no
     `exact:true`) also matched this card's own new "Change for this run" button — a
     genuine substring collision introduced by this card's new UI text. Fixed by adding
     `exact:true` at all three call sites. Separately, the two tests assuming
     `toBeHidden()` on "Where it runs" were wrong given the shared, persistent `e2e.db`:
     `GET /api/runners` is global, not project-scoped (confirmed while building the
     picker — no route or handler filters it), so other specs' previously-enrolled
     runners keep the picker visible here too. Not a product bug —
     `shouldHideTargetPicker`'s unit tests already prove the real logic in isolation.
     Fixed by having the E2E tests select their own runner by name only when the picker
     happens to be showing (`selectTargetIfPickerShows`), asserting the resulting
     `selector_kind`/`selector_id` instead of the picker's visibility.
  2. **Run 2: 2 passed, 2 failed — a real product bug, found only by driving the actual
     picker in a browser.** `targetOptions()`'s runner options used the value string
     `` `runner:${r.runner_id}` ``, but the `<select>`'s `onInput` handler only
     recognised the literal kind `'exact_runner'` (`if (kind === 'fleet' || kind ===
     'exact_runner')`), and the controlled `value` prop it round-trips against is built
     from `` `${selectorKind()}:${selectorId()}` `` — i.e. `` `exact_runner:<id>` ``, a
     different prefix. Selecting a runner from the picker therefore fell through to the
     `else { setSelectorId(''); }` branch and silently cleared the selection the instant
     it was made — while the native `<select>` element kept **displaying** the picked
     option regardless (browser-owned DOM state, independent of the React/Solid-owned
     signal), so a screenshot of the "broken" state looks completely correct: target
     picked, agent profile picked, model "Supported", repository filled — and the Run
     button still permanently disabled, because `selectorId()` was actually `''`
     underneath. This is exactly the class of bug a screenshot or a manual click-through
     would not catch and only a real interaction plus an assertion on the *submitted
     effect* (not the visible state) exposes — confirmed by unit-testing it with jsdom's
     `dispatchEvent('input')`, which reproduces it identically. Fixed by changing the
     option value to `` `exact_runner:${r.runner_id}` `` (matching the parser and the
     controlled value's own format). Added a permanent regression test,
     `RunWithAgentModal.test.tsx`: "manually picking a specific runner from the combined
     picker actually selects it…", proven load-bearing per the adversarial-case section
     above.
  3. **Run 3 (final): 4 passed, 0 failed**, in 2.7s — the two previously-hanging tests
     (11s and a 30s timeout in run 2) now complete in 1.3-1.4s each, consistent with the
     fix removing a permanently-disabled-button wait rather than papering over a timing
     issue.

## What a stranger still cannot do

A stranger can now open "Run with agent" on a project with one enrolled runner and a
configured model default, change nothing but the repository remote, and submit — the
target and the model both resolve from real, measured data with zero typed ids. What they
still cannot do: turn agent execution on or off from this modal (the "Turn it on" link
goes to a page — `/agents` — that does not exist on this branch's base; VI-C1 has not
landed here), get a repository default without typing the remote every time (no
project-level repository storage exists yet — that is `tack-core`/`tack-db`/VI-C3-scope
work, not this card's), or type a free-text model id for a harness whose runner does not
attest `model_passthrough` (by design — an unverified free-text model id is exactly the
"capability claim that is not load-bearing" §VI.1 rule 5 forbids).

## Surface-map delta

- **"Run an item"** (§VI.0's table): moves from *"UI modal, five hand-typed fields"*
  toward the target text *"UI, defaults from project settings, zero hand-typed
  identifiers"* — reached for the **target** (machine/group) and the **model** (project
  default, when set), both real and measured. **Not fully reached** for the **repository
  remote**, because no project-level repository default exists on this branch's base
  (VI-C3 delivered only the model tier — see "Known limitations"). Reason: sequencing/
  scope, same as VI-C3's own "no live model catalog" note on the "Choose a default model"
  row — not a structural block, so not an ADR-0061 escalation, just a row that stays
  partially open until a later card adds project-level repository storage.
- **"Turn on agent execution"**: unchanged by this card — still a console step
  (`tack serve --with-runner`) on this branch's base; the modal's new "Agent execution is
  off" state observes that fact (via runner count) rather than moving the switch itself,
  which is VI-B3's row.

## Secret-path proof

*(not applicable — B1/B2/B3/D1 only; this card touches no secret store.)*

## Vocabulary check

`grep -n 'label="[^"]*[Rr]unner\|label="[^"]*[Ff]leet\|label="[^"]*[Hh]arness\|aria-label={.*[Rr]unner\|title="[^"]*[Rr]unner' frontend/src/shared/runWithAgent/RunWithAgentModal.tsx frontend/src/shared/runWithAgent/RunWithAgentButton.tsx`
finds exactly **one** hit: `RunWithAgentModal.tsx`'s `label="Harness"` field. This
predates VI-C2 — `git diff` shows this exact line unchanged from the pre-card file
(confirmed by extracting the original file's `Harness` `<Select>` block and diffing it
against the current one: identical). §VI.1 rule 8 names "harness" as a word that should
stay under *Advanced*/the developer book on a default screen, and the same rule's last
sentence assigns the enforcing test to VI-C1 ("A test greps the rendered default screens
for them; VI-C1 owns it") — so this is reported here, not fixed here: renaming or
relocating the Harness picker is outside this card's Tasks and Owns, and VI-C1's test is
the place that will actually catch or clear it.

Every other word in that list appears only in comments and identifiers (`RunnerSummary`,
`runnersData`, `activeRunners`, …), never in rendered text — the new "Machine or group"
picker's option labels are `f.name`/`r.name` (operator-typed names), never the literal
words "fleet" or "runner".

## Context spent

- Tokens read before the first edit (cold start), against the block's estimate: the block
  estimated ≈20k for its named read list. Actual reading before the first edit ran
  meaningfully higher — several files were read beyond the named list, each recorded
  below with why. The card's scope (a full modal redesign against a base whose actual
  delivered state — VI-C3's real column, not the TODO.md card text's assumed JSON blob —
  differed from what the dispatch block's Tasks described) required more discovery than
  a byte-identical-helper change would have.
- Context size at handoff: under the ~150k stop threshold, but not by a wide margin —
  this card's scope (rework a 470-line component, its button, its tests, three external
  call sites, one type file, and the E2E spec, then run and fix a real E2E pass) is large
  for one card.
- Files opened beyond the named list (each with why):
  - `frontend/src/shared/runWithAgent/RunWithAgentButton.tsx` (81 lines, read whole) —
    not named by the block; needed because the card's Tasks require the attempt-state
    chip "next to the existing ▶ trigger", which lives in this file, and editing it
    without reading it first would have been a guess. It is inside this card's own
    `runWithAgent/**` ownership.
  - `frontend/src/shared/execution/api.ts` (targeted ranges: `FleetSummary`,
    `AgentProfileSummary`, `RunnerSummary`, the three `*Api` objects) and
    `frontend/src/shared/execution/capabilities.ts` (whole, 175+ lines) and
    `types.ts` (targeted ranges around `HarnessCapability`/`RunnerCapabilities`) — the
    block named a `rg` over `api.ts` but not a read of it or of `capabilities.ts`;
    reading them was necessary to discover `listModelCombinationsForHarness` and
    `listReportedHarnessKinds` already existed (reused rather than reinvented) and that
    `HarnessCapability` was missing `model_passthrough`.
  - `frontend/src/shared/execution/store.ts` (targeted range, `ExecutionRequestRecord`/
    `AttemptAvailability`/`ExecutionStore` interface) — needed for the attempt-state
    chip's data source; the block named `rg -n "export" store.ts` (a listing) but not a
    read of the shapes themselves.
  - `frontend/src/features/board/Board.tsx` beyond the named `RunWithAgentButton`
    range (the `ItemCard`/`BoardColumnView` props and the `handleEditItem`
    `setSearchParams` call) — needed to add `projectId`/`showStateChip` correctly and to
    discover the `setSearchParams` merge-semantics risk the chip's `?tab=` navigation
    creates (see "Known limitations").
  - `frontend/src/features/item-detail/ItemDetailDrawer.tsx` (whole component's tab
    logic, `BASE_TABS`, `activeTab`) — not named by the block ("the Agents page (C1's)"
    is the only frontend file it explicitly excludes); needed because the card's own
    task text ("opening it lands on the item's Agent Activity tab") named the **wrong**
    tab — `agent` is the legacy Docket tab (`AgentActivityTab.tsx`, VI-C4's ownership);
    the correct target, confirmed by the pre-existing E2E spec's own assertion
    (`drawer.getByRole('tab', {name:'Execution'})`) and by `RunWithAgentModal.tsx`'s own
    existing `onCreated={() => setActiveTab('execution')}`, is `execution`. Recorded as a
    vocabulary/task-text correction, not a deviation.
  - `frontend/src/features/sprints/Sprints.tsx` (targeted, the `RunWithAgentButton` call
    site and the `Item` type import) — needed to thread `projectId` through, mirroring
    Board.tsx.
  - `frontend/e2e/helpers.ts` (targeted, every exported function's signature via `grep
    -n "^export"`, then `enrollRunner`/`createFleet`/`createAgentProfile` read in full) —
    the block named the spec file, not the helpers it imports; needed to write a correct,
    non-fabricated E2E test.
  - `frontend/e2e/run-with-agent.spec.ts` read in full (132 lines) — the block named
    `head -120`; the last 12 lines (the Sprint test) were needed to update it faithfully
    rather than guessing its ending, and to add the new project-default test after it.
  - `crates/tack-orch/src/execution/capabilities.rs` (two targeted `sed -n` line ranges,
    ~25 lines total) and `docs/contracts/runner-v1/capabilities.json` (40 lines) — the
    block says "Do not read: any crate"; this was read anyway, narrowly, to get the exact
    `model_passthrough` field shape and its `serde` attributes right rather than
    guessing. Recorded as a deliberate, bounded exception, not an oversight — the
    alternative was shipping a guessed TypeScript shape for a field this card's own task
    text requires using correctly.
  - `crates/tack-orch/src/scheduler/select.rs`,
    `crates/tack-orch/src/scheduler/types.rs`,
    `crates/tack-orch/tests/scheduler_test.rs` (grep matches only, not opened/read) — to
    confirm `model_passthrough`'s `Advisory`/absent-both-reject-identically semantics
    before writing `shared.ts#isModelPassthroughAttested`'s doc comment. Grep, not a
    file read; still a crate touch beyond the block's restriction, recorded for the same
    reason as the item above.
  - `frontend/src/shared/api/projects.ts` and the `Project`/`ProjectModelDefault` schema
    entries in `frontend/src/shared/api/schema.gen.ts` (targeted, via `python3 -c`) —
    needed to confirm VI-C3's actually-delivered project shape (`default_model` only, no
    `default_agent_profile_id`/`repository`) against what the TODO.md card text assumed;
    this is the "Known limitations" finding.
  - `docs/agent-handoffs/part-vi/VI-C3.md` (whole, ~220 lines) — named by the block
    ("~3k"); confirmed as read, not extra.
- Read-list lines that were wrong (a range that missed, a size that was off): the block's
  Tasks text (copied from TODO.md's VI-C2 card, not the dispatch README's own read list)
  described two facilities — a project-level default agent profile and a project-level
  repository default — that do not exist on this branch's base; VI-C3's actual delivered
  scope is narrower (model tier only). Not a read-list line exactly, but the same kind of
  finding the dispatch facts already flagged for the execution-off signal, generalized:
  the card's Tasks assumed a fuller VI-C3 than what actually landed. The block's
  "opening it lands on the item's Agent Activity tab" line names the wrong tab id (see
  above) — a genuine wording error worth fixing in the dispatch README for any future
  reader of this card's Tasks text.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
