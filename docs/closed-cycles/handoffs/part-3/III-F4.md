# III-F4 handoff

- **Base SHA / branch / final SHA:** base `251ce55` (tip of
  `plan/harness-agnostic-agent-fleet` at the point Wave 5's F1/F2/F3 APIs and
  the F6 integrator's wiring/mounting of them had all landed — "docs: document
  the Wave 5 route surface and fix the runner-v1 error envelope (III-F6e)") /
  `agent/iii-f4-frontend` / final SHA recorded in the commit that carries this
  handoff (`git log -1` on this branch). Worked directly in the assigned
  worktree (no separate isolated worktree was created for this card).

- **Files changed (must equal ownership list):** 27 files, all under
  `frontend/`, +3003/-49. Card charter: "execution feature UI additions and
  one generated-artifact integration ... focused/a11y/E2E tests and F4
  handoff." No Rust file, no `docs/openapi.json`, no
  `frontend/src/shared/api/schema.gen.ts`, no `docs/contracts/runner-v1/**`,
  no `TODO.md`, no other card's handoff — confirmed via `git diff --stat
  251ce55 --cached` showing exactly the list below.
  - **New wire-format/API layers** (`frontend/src/shared/execution/`):
    `attempts.ts` (+`attempts.test.ts`) — `AttemptSummary`/`EventSummary`
    types and `attemptsApi.list`/`.events`, matching `executions.rs`'s real
    DTOs field-for-field, including the `model_provenance`/`usage_economics`
    fields III-F6b/F6e added. `decisions.ts` (+`decisions.test.ts`) —
    `decisionsApi.resolve`, the session-scoped `decisionTokenStore`
    (mirrors `features/approvals/api.ts#approvalTokenStore` exactly), and
    five error classifiers (`isDecisionTokenRejected`/`isDecisionExpired`/
    `isDecisionIdempotencyConflict`/`isDecisionNotFound`/
    `isDecisionInvalidOption`). `artifacts.ts` (+`artifacts.test.ts`) —
    `artifactsApi.download`/`.contentUrl` and `isArtifactNotFound`/
    `isArtifactContentNotVerified`.
  - **New pure display logic**
    (`frontend/src/shared/runWithAgent/attemptFormat.ts` +
    `attemptFormat.test.ts`): `formatUsdMeasurement` (the literal `"Not
    measured"` renderer this card's acceptance bar is about),
    `formatWallClock`, `formatRunnerTimeCost`/`formatUsageEconomics`,
    `describeModelProvenance`.
  - **New UI components** (`frontend/src/shared/runWithAgent/`):
    `AttemptList.tsx` (+test) — renders every attempt for a request: state,
    model provenance, usage economics, and an on-demand-expanded detail
    section. `EventTimeline.tsx` (+test) — the normalized per-attempt event
    timeline. `DecisionInbox.tsx` (+test) — the decision inbox (pending/
    expired/resolved rows, kept visually and semantically distinct) plus a
    manual "resolve by id" quick action. `ArtifactDownloadPanel.tsx`
    (+test) — verified artifact download by id, with 404/409/success/error
    each a distinct, visible state.
  - **Extended** (both explicitly this card's territory): `store.ts` (+test)
    — `AttemptAvailability` is now a real four-state machine (`idle`/
    `loading`/`ready`/`error`) backed by a new `loadAttempts`/`attemptsFor`
    pair and a realtime-refresh hook, replacing the typed `not_available`
    placeholder E2 built and E6 explicitly left for this card.
    `ExecutionTimeline.tsx` (+test) — wires the new store methods and mounts
    `AttemptList`. `execution/index.ts` — barrel exports for every new
    public type/function above.
  - **E2E** (`frontend/e2e/`, this card's explicit charter — "focused/a11y/E2E
    tests"): `execution-attempt-detail.spec.ts` (new, 2 tests) — proves the
    whole surface against the real production router, not a mock.
    `a11y.spec.ts` (+1 test) — axe scan of the expanded attempt-detail panel.
    `helpers.ts` (+6 functions, additive, following E4/E6's established
    per-feature-helper convention: `claimOnceWithLease`,
    `acceptAndStartAttempt`, `createRunnerDecision`, `submitRunnerArtifact`,
    `createExecution`). `run-with-agent.spec.ts`/`scheduler-e2e.spec.ts`
    (1-line fixes each — see "A mechanical break in two pre-existing tests"
    below). `playwright.config.ts` (+1 env var, additive — see below).
  - **Not touched:** any Rust file, `router.rs`, `openapi.rs`,
    `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
    `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`,
    root `Cargo.toml`, any other card's handoff, `frontend/e2e/helpers.ts`'s
    pre-existing `claimOnce` (a new, separate `claimOnceWithLease` was added
    instead — see "Safe merge order" below for why).

## Contract fixtures consumed

`docs/contracts/runner-v1/decision.create.request.json`/
`decision.poll.response.json` (vocabulary: `answer: {option_id, text}`,
`resolved_by: {kind, subject_id}`) and `event-batch.request.json` (the
free-form `kind`/`payload` shape `EventTimeline.tsx#describeEventPayload`
renders defensively). No fixture was edited — this card owns no
runner-protocol surface, only the operator-facing consumers of routes other
cards already built. Not applicable: `cargo test -p tack-orch --test
runner_contract` is unaffected by this card (no Rust file touched).

## Behavior implemented

### 1. Attempts/events wiring (E6's own "mechanical follow-up," now done)

`store.ts#attemptsFor` no longer returns a permanent `not_available`
placeholder. `AttemptAvailability` is now `{status: 'idle'}` (nothing has
asked yet) → `{status: 'loading'}` → `{status: 'ready', data:
AttemptSummary[]}` | `{status: 'error', error}` — the same explicit
never-conflate-empty-with-error-with-not-yet-fetched discipline
`ExecutionRequestRecord` already established for requests, applied one level
down. `loadAttempts(requestId)` calls the real `GET
/executions/{id}/attempts` (card III-E6); `ExecutionTimeline.tsx`'s
`RequestRow` triggers it once, lazily, the first time a request row mounts
(`createEffect` guarded on `status === 'idle'`), and `connectRealtime`
refreshes it on invalidation **only** for a request some consumer has
already asked about — an unconditional refetch-on-every-invalidation would
fetch attempt data for every visible request whether or not anything reads
it.

`EventTimeline.tsx` reads the real `GET
/executions/{id}/attempts/{n}/events`, lazily, only when an attempt row is
expanded (`AttemptList.tsx`'s "Show events, decisions & artifacts" toggle)
— per-attempt event history is not global reactive state the way
requests/cancellation are, so this stays a local `createResource` rather
than growing the shared store further (a deliberate scope decision, not an
oversight).

### 2. Model provenance and honest usage/economics

`AttemptList.tsx` renders `AttemptSummary.model_provenance`/
`.usage_economics` (added by III-F6b/F6e, wiring III-F3's pure resolution
into `executions.rs`) through `attemptFormat.ts`:

- **`formatUsdMeasurement`** checks `source === 'not_measured'` (or a `null`
  value, defensively, regardless of what `source` claims) FIRST and
  unconditionally renders the literal text `"Not measured"` —
  never `$0.00`, `—`, `0`, or a blank cell. A real `measured`/`estimated`
  zero renders as a genuine `$0.00 (measured)`, never collapsed into the
  same text. This is the exact rule CLAUDE.md's task brief names: "no
  runner infra cost-rate is stored anywhere in this schema ... render that
  as a literal, unmistakable 'Not measured'."
- **The two dollar dimensions are never summed** —
  `formatRunnerTimeCost`/`formatUsageEconomics` return two independent
  strings (`modelTokenCostUsd`, `runnerTime.costUsd`), rendered as two
  separate `<dl>` rows in `AttemptList.tsx`, matching
  `UsageEconomics`'s own doc comment.
- **`wall_clock_ms: null`** renders `"Unknown — attempt has not finished
  yet"`, deliberately worded differently from `"Not measured"` — a plain
  fact not yet known is a different absence reason than "will never be
  measured on this deployment," and the acceptance bar's "no structural
  zero standing in for unknown" spirit extends to not conflating two
  distinct *kinds* of unknown either.
- **`describeModelProvenance(null)`** → `"Not yet reported"` (the attempt
  hasn't completed) — again deliberately distinct wording from "Not
  measured." `matched`/`auto_select_observed`/`mismatched` each get a
  distinct tone (success/info/warning) and `mismatched` always shows both
  the requested and actual provider/model, never silently reconciled.

### 3. Decision inbox

`DecisionInbox.tsx` is genuinely two things composed together, because of a
real backend gap this card discovered and could not close (see "Schema/API/
contract change requested" below — **no decision-discovery/list endpoint
exists anywhere in this codebase**, confirmed by reading every handler in
`runner_protocol.rs`/`decisions.rs`: `create_decision`/`poll_decisions` are
runner-credential-only, and `resolve_decision` is the only operator route,
taking an already-known `(attempt_id, decision_id)`):

1. **A presentational row renderer** (`DecisionRow`), fed by an optional
   `decisions: DecisionRecord[]` prop — always empty in every real
   deployment today, but fully built and tested against synthetic
   pending/expired/resolved fixtures so the acceptance bar's "pending/
   expired differ" is proven directly, and the component is ready to
   receive real rows the instant a list endpoint lands. Pending shows a
   warning-tone "Pending" badge with a live resolve form (radio options
   when the decision declares any, a free-text "Answer (option id)" field
   otherwise); expired shows a danger-tone "Expired" badge, no resolve
   controls, and the disabled reason as visible text ("This decision
   expired without an answer — it can no longer be resolved."); resolved
   shows a success-tone "Resolved" badge and the recorded answer, read-only.
2. **A manual "resolve a decision by id" quick action**
   (`ManualDecisionResolve`) — the genuinely live-usable path today, given
   the real, mounted `POST /attempts/{attempt_id}/decisions/{decision_id}/
   resolve` (III-F1, wired by III-F6). Both share one `resolveAndNotify`
   function that maps every distinct server outcome to a distinct toast:
   token-rejected (403, fail-closed — see below), expired (409
   `decision_expired`), idempotency conflict (409 `idempotency_conflict`),
   not-found (404), invalid-option (400), and success (`"Decision
   resolved."` vs `"Already resolved — this is the previously recorded
   answer."` for a replay).

**The decision-token field mirrors `features/approvals/ApprovalsPage.tsx`'s
own `TACK_ORCH_APPROVAL_TOKEN` entry exactly**, including its documented
reasoning: always render the field and Save button, let a real resolve
attempt's actual 403 answer "is this even configured," rather than guessing
client-side. `decisionTokenStore` is session-storage-scoped by API origin,
never sent except on an actual resolve call — same posture as
`approvalTokenStore`. **"Decisions cannot be resolved on this deployment" is
handled as the real, named, honest state this card's brief calls for**: when
`TACK_EXECUTION_DECISION_TOKEN` is unset server-side, `decisions.rs`'s
`require_decision_token` fails closed on *every* resolve attempt regardless
of what token the client sends, and this UI surfaces that exact case with
its own message (`isDecisionTokenRejected`), never swallowed as a generic
error — proven end-to-end in `execution-attempt-detail.spec.ts`'s first
test (see "Tests" below).

### 4. Verified artifact download

`ArtifactDownloadPanel.tsx` — "one generated-artifact integration" (card
brief) against the real `GET /executions/{request_id}/attempts/
{attempt_number}/artifacts/{artifact_id}/content` (III-F2, mounted by
III-F6/F6a). Uses `fetch` + `Blob` (`shared/api/client.ts#requestBlob`) +a
synthetic `<a download>` click, not a plain `<a href download>` the way
`FilesTab.tsx` downloads ordinary attachments — an anchor tag cannot carry
the `Authorization` bearer header this operator route needs, and more
importantly cannot report *why* a download failed, which this card's
acceptance bar requires ("artifact failure visible"). Same
no-discovery-endpoint gap as decisions (see below): `artifact_id` is
manually entered, with the same honest inline note explaining why.

**404 and 409 are kept visually and semantically distinct**, matching
`artifact_download.rs`'s own documented distinction: 404 means no manifest
exists under this id at all; 409 means the manifest exists but its content
hasn't been verified (streamed + checksummed) yet — worth retrying, not
gone. Both render as their own inline `<p>` with different wording and
tone, alongside a generic-error state and a success state, so every outcome
is a distinct, visible fact.

## Tests added and exact commands/results

**Vitest** (`cd frontend && npm run test`) — **724 passed, 0 failed, across
85 files** (Wave 4's own accepted baseline, per III-E6's handoff: 653
passed, 77 files. 653 + 71 = 724 exactly: 8 new files carrying 68 new tests,
plus a net +3 in `store.test.ts` — the old single `attemptsFor` placeholder
test was replaced by 4 real state-machine tests, and one new
`connectRealtime` test was added — and 0 in `ExecutionTimeline.test.tsx`,
whose one changed test was a 1-for-1 replacement of the old placeholder
assertion). Per new file: `attempts.test.ts` (7), `artifacts.test.ts` (6),
`decisions.test.ts` (10), `attemptFormat.test.ts` (16),
`DecisionInbox.test.tsx` (12), `ArtifactDownloadPanel.test.tsx` (7),
`EventTimeline.test.tsx` (4), `AttemptList.test.tsx` (6).

- `npm run type-check` (`tsc -b`) — clean.
- `npm run lint:tokens` — 0/0, clean (no raw color literal, no inline-style
  hex literal in any new/changed file).
- `npm run build` — clean.

**Playwright E2E** (`CARGO_TARGET_DIR` set per this repo's disk-safety rule
before every run — no `target/` was created inside the worktree):

- `npx playwright test --project=chromium` (full suite) — **65 passed, 0
  failed** (Wave 4's own baseline, per III-E6's handoff: 62. 62 + 3 = 65
  exactly: this card's 2 new tests in `execution-attempt-detail.spec.ts`
  plus 1 new a11y scan in `a11y.spec.ts`). Includes 36 a11y scans (35 +
  this card's 1 new one), all 0 violations.
- `npx playwright test --project=firefox` (full suite) — **23 passed, 42
  skipped, 0 failed** (a11y's own by-design chromium-only skip: 36 a11y
  tests + 6 `api.spec.ts` tests, which also skip on non-chromium per that
  file's own condition — confirmed not a regression, that skip predates
  this card).
- `npx playwright test smoke.spec.ts --project=webkit` — **webkit fails to
  launch in this sandbox**, identically to III-E6's own documented finding:
  `error while loading shared libraries: libwoff2dec.so.1.0.2: cannot open
  shared object file`, every test in the file fails the same way including
  specs this card never touched. Confirmed pre-existing, environment-only,
  not a regression; no `sudo` available in this worktree to install the
  missing library. Webkit's actual pass/fail status against this branch
  remains not established in this environment, exactly as III-E6 left it.

## A mechanical break in two pre-existing tests (found and fixed, not a regression)

Wiring real attempt data into the Execution tab changed two things every
other Wave-4 E2E test that opens that tab now has to account for, and two
pre-existing tests (owned by E4/E6, not this card, but broken by this
card's change) needed a one-line fix each:

1. **`run-with-agent.spec.ts`'s "item-detail: submitting a run..." test**
   asserted the literal placeholder text `"Attempt history isn't available
   yet"` — gone now that the endpoint is real. Fixed to assert `"No
   attempts yet."`, the new honest empty state for a freshly-created,
   unclaimed request.
2. **`scheduler-e2e.spec.ts`'s "healthy exact-runner selection..." test**
   asserted `getByText('Leased', {exact: true})` expecting exactly one
   match. Now that `AttemptList.tsx` renders a real attempt row, BOTH the
   request's own state badge and the attempt's state badge say "Leased"
   right after a claim — two elements, a strict-mode violation. Fixed with
   `.first()`, scoped by this test's own comment to mean "the request-level
   one this test's name is about."

Both fixes are minimal (a changed string / an added `.first()`), verified
by re-running each spec individually before and after, and by the full
`--project=chromium` run above going green.

## Failure/adversarial case proved

- **"Not measured" is exact, not approximate** —
  `attemptFormat.test.ts#formatUsdMeasurement`'s own suite: asserts the
  literal string `'Not measured'` (not merely "contains" or "is falsy"),
  asserts it does NOT contain `'$0'`, is NOT `'—'`, is NOT `''`, and — the
  positive control ruling out "always returns Not measured, vacuously" —
  asserts a real `{value: 0, source: 'measured'}` renders `'$0.00
  (measured)'`, a genuinely different string. `AttemptList.test.tsx`
  reproduces this at the component level against the exact
  `usage_economics` shape every real `GET .../attempts` response returns
  today (both dollar fields `not_measured`), and separately proves a real
  measured value (`$0.42 (measured)`) renders honestly alongside the
  still-`not_measured` runner-time dimension in the same fixture — proving
  the two dimensions are independent, not both flipped by one code path.
- **Pending/expired/resolved are load-bearing distinct states, not just
  differently-labeled** — `DecisionInbox.test.tsx`'s "all three states are
  visually distinct from each other in the same list" test renders all
  three simultaneously and asserts three genuinely different badge
  classifications are found, not just three different strings that could
  coincidentally overlap.
- **Keyboard accessibility of the resolve interaction, proved structurally
  and behaviorally**: `DecisionInbox.test.tsx` asserts (a) every interactive
  control is a native `button`/`input`/`select` element with no negative
  `tabindex` (a `<div onClick>` masquerading as a control would fail this),
  (b) radio options share one `name` attribute (the mechanism that gives
  the browser's own free arrow-key navigation and mutual exclusion — a
  missing/mismatched `name` would silently break this), and (c) an
  end-to-end interaction — focus the radio, select it, focus the Resolve
  button, `.click()` it (the same activation event Enter/Space produces on
  a real button, and the same technique this codebase's own
  `ExecutionTimeline.test.tsx` already uses for "the user activates this
  button") — actually submits the real resolve call with the right body.
  `ArtifactDownloadPanel.test.tsx` proves the equivalent for the download
  field/button pair.
- **Disabled controls name their reason, proved as a state transition, not
  a static snapshot** — `DecisionInbox.test.tsx`/`ArtifactDownloadPanel.
  test.tsx` each assert the control starts disabled with specific visible
  text, THEN assert filling the required field(s) flips `disabled` to
  `false` — ruling out "always disabled" or "never actually wired to the
  signal" as passing vacuously.
- **The fail-closed decision-token case is genuinely enforced, proved
  against the real server** — `execution-attempt-detail.spec.ts`'s first
  test attempts a resolve with NO token entered (the real, un-configured-
  by-the-UI default) and asserts the exact fail-closed message, THEN enters
  the real token and retries against a still-nonexistent decision id,
  asserting a DIFFERENT message (a genuine 404) — proving the 403 case
  isn't just "any resolve always fails," and that entering the correct
  token genuinely changes the server's answer.
- **The artifact download is a real, byte-verified file transfer, not just
  a network call** — `execution-attempt-detail.spec.ts`'s second test has a
  real runner (via the new `helpers.ts` functions) upload real content with
  its real sha256 through the real streaming-verification path
  (`artifact_storage.rs`, III-F2), then downloads it through the UI via a
  real Playwright `download` event, and asserts the downloaded bytes equal
  the uploaded content exactly — not merely that the request returned 200.
- **The decision resolve genuinely lands server-side, not just client-side
  optimism** — the same test's idempotent-replay check: a second, raw API
  call with the byte-identical answer shape returns `replayed: true`, which
  `decisions.rs` can only return by reading back a row this test's own UI
  interaction actually wrote.

## Schema/API/contract change requested from another owner

Recorded per III.2 rule 2 — this card touches no Rust file, so every item
below is a request, not a decision made unilaterally:

1. **No decision-discovery/list endpoint exists anywhere in this codebase.**
   Confirmed by reading every handler that touches `execution_decisions`:
   `create_decision`/`poll_decisions` (`runner_protocol.rs`) are
   runner-credential-only and structurally unreachable from the operator
   surface; `resolve_decision` (`decisions.rs`, III-F1) is the only
   operator route, and it requires an already-known `(attempt_id,
   decision_id)` pair. F1's own handoff names this gap explicitly as
   "likely III-F4's frontend-integration concern once it needs one" — it
   is, and this card could not close it without touching a Rust file
   outside its charter. **Concrete request:** an operator-facing `GET
   /api/executions/{request_id}/attempts/{attempt_number}/decisions`
   returning every `execution_decisions` row for that attempt (mirroring
   the existing `.../events` route's shape/scoping exactly) would let
   `DecisionInbox.tsx`'s already-built, already-tested `decisions` prop
   become genuinely populated — no frontend change needed beyond wiring one
   new `attemptsApi`-style call and passing its result down.
2. **No artifact-discovery/list endpoint exists either**, for the identical
   reason: `execution_artifacts` has exactly one read path
   (`get_execution_artifact_by_attempt_number`, a single-row lookup by a
   caller-supplied `artifact_id`; confirmed via `crates/tack-db/src/repo/
   execution.rs`), no `GET .../artifacts` list route. **Concrete request:**
   a symmetrical `GET /api/executions/{request_id}/attempts/
   {attempt_number}/artifacts` returning manifest metadata (`artifact_id`,
   `kind`, `name`, `media_type`, `size_bytes`, whether `content_reference`
   is set yet) would let `ArtifactDownloadPanel.tsx` list real, clickable
   artifacts instead of requiring a manually-typed id.
3. **Neither gap above is a regression this card introduced** — both are
   pre-existing absences in the runner-v1 protocol's operator-facing read
   surface, confirmed by reading F1/F2/F3's own handoffs (none claims a
   list endpoint) and the real router (`router.rs`'s
   `operator_execution_routes`). This card built the maximum honest UI
   possible against what exists today (manual entry, clearly labeled, with
   a working live resolve/download action once an id is known) rather than
   inventing a fake list or leaving the feature entirely unbuilt.

## Known limitations or `not_measured` fields

- Everything in the numbered list above.
- `DecisionInbox.tsx`'s `decisions` prop is never populated from a live
  fetch anywhere in this codebase today (item 1 above) — every production
  mount of `DecisionInbox` (`AttemptList.tsx`) passes only `attemptId`,
  matching the honest "no list exists" reality. Wiring a real list once
  item 1 lands is mechanical: fetch, pass the array in.
  `ArtifactDownloadPanel.tsx` is analogous for item 2.
- `formatUsdMeasurement`'s small-value precision bump (4 decimals under
  $0.01, else 2) is this card's own display convention, not a contract
  obligation — no fixture specifies dollar-figure formatting precision.
- Webkit's pass/fail status in this sandbox is `not_measured` for the same
  environment reason III-E6 already documented (missing system library, no
  `sudo`) — chromium and firefox are both fully green.
- `AttemptList.tsx`'s expand/collapse state is local component state, not
  persisted — collapsing then reopening the drawer resets which attempts
  were expanded. Not a correctness issue, flagged for completeness.

## Secrets/logging review

- No new frontend code introduces a `console.log`/`console.error` of any
  request/response body, credential, or decision/artifact content — every
  new file was grepped for `console.` before this handoff (none found in
  the committed diff; a debug-only instrumentation pass used during
  development to diagnose an E2E failure was fully removed before
  committing — confirmed via `grep -rn "console.log" frontend/e2e/
  execution-attempt-detail.spec.ts` returning nothing).
- `decisionTokenStore` mirrors `approvalTokenStore` exactly: `sessionStorage`
  only (never `localStorage`), scoped by API origin, cleared on origin
  change, never sent except on an actual `decisionsApi.resolve` call, never
  logged. The raw token value never appears in any test assertion beyond
  the literal string used to *set* it (matching `approvalsApi`'s own test
  precedent of asserting a secret is *absent* from a serialized body, which
  `decisions.test.ts`'s "sends the stored decision token ... never inside
  the JSON body" test does explicitly).
- No frontend code in this card ever logs an artifact's byte content or a
  decision's `prompt`/`answer` text — `EventTimeline.tsx`'s
  `describeEventPayload` renders payload text to the DOM for the operator
  to read (its whole purpose), never to a console/log sink.
- `playwright.config.ts`'s new `TACK_EXECUTION_DECISION_TOKEN:
  'e2e-decision-token'` is a fixed, non-secret, test-only value (identical
  in spirit to the e2e suite's existing throwaway `e2e.db`) — never a real
  deployment credential.

## Safe merge order and likely conflicts

- This card never touched any Rust file, `router.rs`, `openapi.rs`,
  `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
  `docs/contracts/runner-v1/**`, `.github/workflows/ci.yml`, `TODO.md`, root
  `Cargo.toml`, or any other card's handoff — no conflict expected there.
- `frontend/e2e/helpers.ts` gained six new, additive exports appended after
  the existing `claimOnce` — the pre-existing function itself is untouched
  (a new `claimOnceWithLease` was added alongside it rather than changing
  `claimOnce`'s return shape, specifically so no existing caller in
  `scheduler-e2e.spec.ts` needed to change). A same-file merge with a
  sibling card's own additive helpers should be an ordinary adjacent-append,
  not a logical conflict — the same expectation III-E6's handoff recorded
  for its own additions to this file.
- `frontend/src/shared/execution/store.ts`'s `AttemptAvailability` type
  changed shape (from a permanent placeholder to a real state machine) —
  this is a deliberate, acceptance-bar-required breaking change to this
  card's own type, not an accidental one. Every consumer in this repository
  (`ExecutionTimeline.tsx`, both test files) was updated in the same
  change; a future card that also reads `attemptsFor()` should expect the
  new four-state shape, not the old `{status: 'not_available', reason}`
  placeholder — grep found no other consumer as of this card's base SHA.
- `frontend/e2e/run-with-agent.spec.ts` and `scheduler-e2e.spec.ts` each
  received one small, targeted fix (see "A mechanical break in two
  pre-existing tests" above) — flagged explicitly since both files are
  owned by other cards (E4/E6), not this one; the changes are minimal and
  justified by this card's own wiring, matching the precedent of a Wave 5
  card fixing a mechanical break its own work causes in an earlier wave's
  test, documented rather than silently absorbed.
- `frontend/playwright.config.ts` gained one additive environment variable
  on the shared `webServer` block — every other spec is unaffected (nothing
  but a decision-resolve call ever reads that header).

## Checklist

- [x] No unowned files touched — confirmed via `git diff --stat 251ce55
      --cached`: exactly 27 files, all under `frontend/`, matching the list
      above; zero Rust files, zero generated/contract/CI/TODO files.
- [x] No live secret committed, logged, or reachable via `argv`/`ps`/trace —
      see "Secrets/logging review"; the only "credential"-shaped value
      anywhere in this card's new code is `decisionTokenStore`'s
      `sessionStorage`-scoped, never-auto-sent decision token, and the
      fixed non-secret e2e test token in `playwright.config.ts`.
- [x] No panic stub / `unimplemented!()` / fake success — the two
      genuinely-missing backend capabilities (decision/artifact discovery)
      are named, explained gaps with a concrete recorded request, not
      placeholders standing in for success; `DecisionInbox`/
      `ArtifactDownloadPanel` are honest about "no list exists yet" rather
      than fabricating one.
- [x] No blind retry — nothing in this card's new code retries a failed
      resolve/download automatically; every failure is a named, visible,
      terminal state until the operator acts again by hand.
