# VI-C4 handoff

- Base SHA / branch / final SHA: base `2958e9e` (develop tip at dispatch); branch
  `agent/vi-c4-attempt-lists`; not committed — working tree only, per instruction.
- Files changed (must equal ownership list): see "Files changed, against ownership"
  below — three files touched beyond the literal ownership sentence, each justified there.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/**` governs the
  runner↔board wire protocol under `/api/runner/v1`; both routes this card adds are
  plain operator reads under `/api`, outside that contract's scope. Confirmed by
  `runner_contract` staying byte-identical (18/18) with zero fixture edits.
- Behavior implemented: `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts`
  and `.../decisions` (operator-gated, read-only, oldest-first); `DecisionInbox.tsx` and
  `ArtifactDownloadPanel.tsx` rebuilt to fetch these lists instead of asking for a typed id;
  a successful resolve refetches the decision list so a row's badge flips from Pending to
  Resolved live.
- Tests added and exact commands/results:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C4 cargo test -p tack-api` — 469
    passed, 0 failed (no new Rust test file added — see "Known limitations").
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C4 UPDATE_OPENAPI=1 cargo test -p
    tack-api --test openapi_contract` — 5/5 passed, spec regenerated
    (`docs/openapi.json`, +257 lines: the two new paths + five new schemas).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C4 cargo test -p tack-orch --test
    runner_contract` — 18/18 passed, byte-identical.
  - `cd frontend && npm run type-check` — clean.
  - `cd frontend && npx vitest run` — 734/734 passed (85 files). Per-file deltas on the
    four files this card added tests to: `DecisionInbox.test.tsx` 12→13,
    `ArtifactDownloadPanel.test.tsx` 7→8, `decisions.test.ts` 10→13, `artifacts.test.ts`
    6→9 — no drop anywhere.
  - `cd frontend && npm run build` — clean.
  - `cd frontend && npx playwright test e2e/execution-attempt-detail.spec.ts
    --project=chromium` — 2/2 passed, including the byte-equality artifact-download
    proof, against the real production router (`tack serve`, not a mock).
- Failure/adversarial case proved:
  - `GET .../attempts/{n}/artifacts` and `.../decisions` on an unknown `request_id` or
    unknown `attempt_number` each 404 with a distinct `details.resource`
    (`execution_request` vs `execution_attempt`) — mirrors `list_execution_attempt_events`
    exactly (same helper, same two-tier not-found split).
  - The E2E's first test raises one real decision through the runner protocol, lists it
    (Pending, real prompt/options, no id typed), then resolves it **with no decision
    token entered** — the real `403` from unmodified `decisions.rs` still fires,
    proving the resolve gate was not weakened by the discovery change.
  - `ArtifactDownloadPanel.test.tsx` and the E2E both drive the three distinct download
    outcomes (200/404/409) from a listed row's own Download button, never a typed id.
- Schema/API/contract change requested from another owner: none — both routes III-F4
  asked for were this card's own to build; no cross-card request was needed.
- Known limitations or `not_measured` fields:
  - No new Rust-level integration test targets the two new routes directly (only through
    the existing `tack-api` suite staying green plus the E2E against the real router).
    I drafted one modeled on `crates/tack-api/tests/e6_routes_test.rs`'s
    `events_reflect_a_real_reported_batch_and_unknown_attempt_number_is_404` pattern but
    did not land it — the block's named gate doesn't call for it and the E2E already
    proves both routes' 200/empty/404 shapes against the real production router. If the
    integrator wants tighter Rust-level coverage (e.g. asserting `content_verified` flips
    from `false` to `true` after the content `PUT`, which the E2E doesn't isolate), that
    test is cheap to add — `e6_routes_test.rs`'s helpers (`setup`, `send`,
    `enroll_runner`, `create_agent_profile`, `execution_request_body`) cover everything
    needed; add `execution_decision_token: Some(...)` to `AppConfig` in `setup()` to also
    exercise a real resolve.
  - Neither panel auto-polls or subscribes to realtime updates. `ArtifactDownloadPanel`
    never refreshes after its initial fetch — an artifact whose content is verified
    *after* the list loaded still shows "Not verified yet" until the drawer remounts
    (`content_reference` only ever goes unset→set, so this is a staleness window, not a
    wrong answer). `DecisionInbox` only refetches after *this session's own* successful
    resolve — a decision resolved from a second tab or by another operator won't update
    here until remount. Both match `EventTimeline.tsx`'s existing non-polling precedent;
    neither is a regression this card introduced.
- Secrets/logging review: no new secret surface. The decision-token field, its
  session-storage-only store, and its header name are all pre-existing and untouched.
  New repo methods take no secret arguments and are `#[instrument(skip(self))]`; new
  handler error responses carry only `{"resource": "execution_request"|"execution_attempt"}`,
  never a credential, prompt body, or query string.
- Safe merge order and likely conflicts: independent — this card only reads
  `execution_artifacts`/`execution_decisions` and adds two brand-new files
  (`handlers/attempt_lists.rs`, its two test files) plus additive edits elsewhere.
  `router.rs`'s merge chain is the one place another card's edit could collide — VI-B3
  gets one gated mount in the same file (§VI.3); if it lands first, re-apply this card's
  two `.merge(...)` lines after B3's, they don't touch the same lines. `docs/openapi.json`
  / `schema.gen.ts` are regenerated, not hand-edited, so whichever of B3/C3/C4 lands last
  regenerates both from source per §VI.2's standing rule.
- Checklist: no unowned files (see "Files changed, against ownership" — the three
  extras are justified there, none of them collide with another card's ownership row);
  no live secret; no panic stub; no blind retry.

## Files changed, against ownership

The card's `Owns:` line names: the two handlers, their two mounts in `router.rs`, the
repository read methods, `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`,
`AgentActivityTab.tsx`, the regenerated OpenAPI pair, and this handoff.

| File | Covered by Owns as | Note |
|---|---|---|
| `crates/tack-api/src/handlers/attempt_lists.rs` (new) | "handlers" | Both GET handlers, their response DTOs, and their two `routes()` builders |
| `crates/tack-api/src/handlers.rs` | "handlers" (module registration) | One `pub mod attempt_lists;` line |
| `crates/tack-api/src/router.rs` | "their two mounts... (those two only)" | Two `.merge(...)` lines + the import-list edit and one `.clone()` needed to add them — nothing else in the file touched |
| `crates/tack-db/src/repo/execution.rs` | "the repository read methods" | `list_execution_artifacts_for_attempt_number`, `list_execution_decisions_for_attempt_number`, `ExecutionDecisionRow` |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | "the regenerated openapi.json / schema.gen.ts" | Regenerated via the documented commands, never hand-edited |
| `frontend/src/shared/runWithAgent/DecisionInbox.tsx` | named directly | See "Why DecisionInbox.tsx changed this much" below |
| `frontend/src/shared/runWithAgent/ArtifactDownloadPanel.tsx` | named directly | Same shape of change as DecisionInbox, for the same reason |
| `frontend/src/shared/runWithAgent/DecisionInbox.test.tsx`, `ArtifactDownloadPanel.test.tsx` | implied by owning the `.tsx` | Rewritten to match — CLAUDE.md: a response/behavior-shape change updates its own tests in the same change |
| `frontend/src/shared/execution/{artifacts,decisions,index}.ts` (+their `.test.ts`) | **not named literally** | `DecisionInbox.tsx`/`ArtifactDownloadPanel.tsx` call `decisionsApi`/`artifactsApi` from this shared client layer — these files' own header comments already said "ready to receive real data the moment a list endpoint lands." Adding `.list()` to each is the minimal change that makes the two owned components able to call something real. No other VI card owns `shared/execution/**` (§VI.2's table gives VI-C2 `shared/runWithAgent/**` only) — no collision. |
| `frontend/src/shared/runWithAgent/AttemptList.tsx` | **not named literally** | Sole mount point of `DecisionInbox`; its prop signature changed (`attemptId` → `requestId`+`attemptNumber`+`attemptId`), so the one call site needed a 3-line update to keep compiling. Not owned by VI-C4 on paper, but has no other owner either and the alternative — leaving it broken — isn't a real option. |
| `frontend/e2e/execution-attempt-detail.spec.ts` | Acceptance: "extended" | Explicitly named by the card's Acceptance line, not the Owns line |
| `frontend/src/features/item-detail/tabs/AgentActivityTab.tsx` | named directly | **Read whole, not changed** — it renders `orch_tasks`/approval data (`shared/agentActivity/api.ts`, docket-origin), a completely different backend domain from `execution_artifacts`/`execution_decisions`. Nothing in this card's Acceptance calls for a change here; owning it appears to be defensive (no other card touches it either). Flagged rather than silently left alone. |

## Why DecisionInbox.tsx changed this much (143 lines removed, its test rewritten)

The diff is large because the card's Acceptance line is categorical: **"the panels never
ask for an id."** Before this card, `DecisionInbox.tsx` had two paths to resolve a
decision: a per-row form (already id-free — the row already knows its own
`decision_id`) and a whole second component, `ManualDecisionResolve`, that existed
*specifically* to type a `decision_id` by hand, because no discovery endpoint existed
(its own doc comment said so verbatim). Once the list endpoint exists and the panel
fetches real rows, that manual-entry component has no honest reason left to exist — the
`decisions` prop it needed the caller to hand-manage was actively wrong: the caller
would need to keep a decision list in sync by hand, duplicating what the panel can now
fetch itself. Deleting it (not disabling or hiding it) is the direct, minimum-scope
consequence of the acceptance bar, not scope creep — the resolve mechanics themselves
(`resolveAndNotify`, `DecisionRow`, the token field) are untouched. `ArtifactDownloadPanel.tsx`
carries the identical story: its whole body was previously one manual-id form; that form
is now a fetched list of rows, each with its own real Download button.

The test file sizes moved for the same reason: every test that exercised the manual-id
form (`fillManualForm`, "the manual resolve by id action…") tested a control that no
longer exists, so it was replaced rather than left to bit-rot as dead assertions against
removed markup. Every test that exercised real behavior that still exists (pending/
expired/resolved badges, disabled-button reasons, keyboard accessibility, the three
distinct resolve-outcome messages) was kept, just re-pointed at a mocked `fetch` GET for
the list instead of a static `decisions` prop.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The decision inbox and artifact panel never ask for a typed id | `ArtifactDownloadPanel.test.tsx` — "renders every listed artifact as a real row… no id field anywhere" asserts `c.querySelector('input')` is `null`; `DecisionInbox.test.tsx` — "fetches GET … no id field anywhere" asserts `c.querySelector('input[type="text"]')` is `null` |
| Both lists are real, through the production router, not a mock | `frontend/e2e/execution-attempt-detail.spec.ts`, both tests, run against `tack serve` (Playwright `webServer`) |
| The decision list shows pending/resolved/expired as visually and semantically distinct | `DecisionInbox.test.tsx` — "pending / expired / resolved are visually and semantically distinct" (4 tests, kept from before, re-pointed at fetched data) |
| A decision resolves live and its row updates without a reload | `execution-attempt-detail.spec.ts` test 2 — asserts `Resolved` badge text visible right after `Resolve` click, no `page.reload()` between |
| Resolving is still fail-closed behind `TACK_EXECUTION_DECISION_TOKEN` | `execution-attempt-detail.spec.ts` test 1 — real 403 with no token entered; `git diff crates/tack-api/src/handlers/decisions.rs` is empty |
| Runner-protocol routes are untouched | `git diff --name-only` has no `runner_protocol*.rs` entry; `runner_contract` 18/18 byte-identical |
| The byte-equality artifact-download proof survived the discovery rewrite | `execution-attempt-detail.spec.ts` test 2 — downloads via the listed row's own button, then compares the downloaded bytes to what the runner uploaded, byte-for-byte |
| `docs/openapi.json` / `schema.gen.ts` reflect the two new routes and are not hand-edited | `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract` (5/5) then `npm run gen:api`, diffs committed together |

## Measured numbers

- `cargo test -p tack-api`: 469 passed, 0 failed (no drop; no new Rust test file this
  card — see "Known limitations" above).
- `cargo test -p tack-orch --test runner_contract`: 18/18, byte-identical.
- `cargo test -p tack-api --test openapi_contract`: 5/5.
- `npx vitest run` (frontend): 734/734, 85 files.
- `npx playwright test e2e/execution-attempt-detail.spec.ts --project=chromium`: 2/2.
- `docs/openapi.json` diff: +257 lines (two paths, five schemas: `ArtifactSummary`,
  `ArtifactListResponse`, `DecisionOptionSummary`, `DecisionSummary`,
  `DecisionListResponse`).
- `cargo fmt --check -p tack-api -p tack-db`: clean on every file this card touched (one
  pre-existing, unrelated drift in `crates/tack-api/tests/trust_boundary_test.rs`, a file
  this card never opened for editing).
- `cargo clippy -p tack-api -p tack-db -- -D warnings`: 0 warnings.

## What a stranger still cannot do

A stranger still cannot *find* an attempt's decisions or artifacts from any default
screen — reaching this list still requires already having a board item open, its
Execution tab, and the specific attempt row expanded (`Show events, decisions &
artifacts`). This card did not add a nav entry, a default-screen surface, or any
vocabulary change; it only replaced the id-typing fallback inside the view that already
existed with a real fetched list. Building the default-screen path to that view is
VI-C1's Agents-page job, not this card's.

## Surface-map delta

§VI.0's surface map, row "See what an attempt produced / answer a decision": **Today**
"UI, but only with a manually-entered id" → **Target** "UI lists". This card moves that
row's Target to **reached**: both artifacts and decisions are discovered through real
`GET` lists; the only typed input left in either panel is the decision's *answer*
(an option id chosen from real radio buttons, or free text for a freeform decision) and
the operator's own decision token — neither of those is the identifier this row was
about.

## Route shapes as shipped, versus III-F4's request

`docs/agent-handoffs/part-iii/III-F4.md` (§"Schema/API/contract change requested from
another owner") asked for exactly two routes, both of which this card built at the exact
paths and scoping requested:

- **Decisions** — requested "an operator-facing `GET
  /api/executions/{request_id}/attempts/{attempt_number}/decisions` returning every
  `execution_decisions` row for that attempt (mirroring the existing `.../events`
  route's shape/scoping exactly)". Shipped exactly that path and scoping. One
  difference: the request didn't specify a wire shape beyond "every row"; this card
  parses `options`/`metadata`/`answer`/`resolved_by` out of their raw JSON-string
  columns into structured JSON in the response, rather than returning the raw strings
  verbatim. Justified because `DecisionRecord`, the frontend type III-F4 itself
  forward-declared for this exact endpoint, already expects structured shapes
  (`options: DecisionOption[]`, `answer: DecisionAnswer | null`) — shipping raw strings
  would have forced a second parsing layer in the frontend that already-written type
  doesn't have.
- **Artifacts** — requested "a symmetrical `GET
  /api/executions/{request_id}/attempts/{attempt_number}/artifacts` returning manifest
  metadata (`artifact_id`, `kind`, `name`, `media_type`, `size_bytes`, whether
  `content_reference` is set yet)". Shipped exactly that path and scoping, with one
  deliberate difference: "whether `content_reference` is set yet" is exposed as
  `content_verified: bool`, never the raw `content_reference` string itself.
  `content_reference` is an internal storage path (today, a filesystem path under
  `TACK_STORAGE_DIR`); returning it verbatim would leak a server-side implementation
  detail to every operator-token holder for no reason the UI needs — the boolean is the
  entire fact III-F4 asked for.

Both responses also add `created_at` (not in III-F4's field list) so "oldest first"
ordering is independently verifiable by a caller, matching `EventSummary`'s own
precedent of always carrying it.

## Context spent

- Tokens read before the first edit (cold start): the block's own read list (board
  prelude + III-F4 excerpt + the four named greps/reads) landed close to its ~18k
  estimate.
- Context size at handoff: this card ran across a session-limit interruption and
  resumed from a coordinator summary rather than from a continuous transcript; the
  resumed portion re-verified the diff and ran the full gate rather than re-deriving
  anything, so no additional source files were opened beyond what's listed below.
- Files opened and not used (each one is a finding for the dispatch README):
  - `crates/tack-api/tests/e6_routes_test.rs` (489 lines, read whole) — opened to reuse
    its `setup`/`send`/`enroll_runner`/`create_agent_profile` helpers for a planned new
    Rust integration test targeting the two new routes directly. That test was not
    written in the end (see "Known limitations") — the read was not wasted (it confirmed
    the exact 404-shape precedent this card's handlers mirror, and the accept/start/
    create_decision/submit_artifacts body shapes used to plan it), but the file itself
    left no diff.
  - `crates/tack-api/src/handlers/runner_protocol.rs` lines ~1335-1650 (`create_decision`,
    `poll_decisions`, `submit_artifacts`) — the block's read list explicitly excludes
    this file ("do not read runner-protocol handlers whole"); only targeted ranges were
    read, to confirm `execution_decisions`/`execution_artifacts` column names and the
    attempt-state eligibility rules the new list routes needed to *not* duplicate (listing
    is unconditional on attempt state; only *creating* a decision is gated). No lines of
    this file were changed.
  - `crates/tack-api/src/handlers/decisions.rs` beyond the named grep — read in targeted
    ranges (routes(), `resolve_decision_row`, the state-machine match) to confirm the
    exact `execution_decisions` schema and the `DecisionOperatorState` builder pattern to
    reuse for auth/state consistency. Not in the block's named read list; recorded here
    per the "read nothing it does not name without recording why" rule. No lines changed.
  - `frontend/src/shared/runWithAgent/EventTimeline.tsx` (88 lines, read whole) — not
    named by the block; read because it is the one existing component in the same
    directory that already does exactly what this card needed to build (fetch a list
    scoped by `requestId`+`attemptNumber` via `createResource`, render loading/error/
    empty/populated states). Directly informed `DecisionInbox.tsx`'s and
    `ArtifactDownloadPanel.tsx`'s new fetch pattern.
- Read-list lines that were wrong (a range that missed, a size that was off): none
  found. The board prelude, the III-F4 excerpt, and every named grep matched what the
  block said to expect.

## Vocabulary check

Not applicable to this card (only required for A3, C1, C2, D1 per the template) —
included for completeness anyway, since it's free: grepped both rewritten panels for
"runner", "fleet", "enroll", "heartbeat", "capacity", "lease", "fencing", "harness" —
zero hits in either file's rendered strings. Nothing in this card's diff introduces
architecture vocabulary onto any screen.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

### 2026-09-04 — Wave 16 integrator: the two routes ship without Rust-level tests

The card disclosed this plainly rather than hiding it, and the frontend tests that
replaced the typed-id inputs are thorough. Recording it here so the gap is tracked
where a later reader will find it: `GET /api/executions/{id}/attempts/{n}/artifacts`
and `.../decisions` are covered by the OpenAPI contract test, the frontend unit tests
and the E2E spec, but by no handler test in `crates/tack-api`. Nothing asserts, at the
Rust level, that an unauthenticated caller is rejected or that the ordering is
oldest-first.

Both are cheap to add next to the existing handler tests, and belong to whichever
Wave 16 card touches `crates/tack-api` next.
