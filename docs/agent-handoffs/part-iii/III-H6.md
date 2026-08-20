# III-H6 handoff

**What this card changes, in plain language.** Before it, a real runner could
finish a harness run — succeed, fail, or get cancelled — and the server would
never learn anything happened beyond the bare terminal status. No event ever
appeared on an attempt's timeline, even though the wire protocol for
events/decisions/artifacts had existed since III-H1. The gap was not in the
transport; it was that nothing in the runner ever called it, and (found only
once this card made a live run possible) submitting an event before
completing an attempt broke the attempt's own completion outright — a real
runner had never done both in sequence before. Both are fixed: every
completed, failed or cancelled attempt now submits a real event, proven live
end-to-end, and a retried submission reuses the same event id so the server
sees a safe no-op, not a duplicate. The runner also now attempts to upload
any artifact an adapter staged locally, reading the bytes straight off disk
and checksumming them fresh rather than trusting a report — that call site
is real and exercised, but the upload itself currently fails server-side in
this environment for a separate, pre-existing reason outside this card's
ownership (see the escalation below); it does not block completion either
way.

- **Base SHA / branch / final SHA:** base `0e2da46` (tip of `develop` at
  start of work; the board names `84fabf1`, and `0e2da46` is exactly that
  plus the board's own recording commit — no real drift), branch
  `agent/iii-h6-engine-submit`. Final SHA: uncommitted at the time of
  writing — no commit was requested.
- **Files changed (all within Owns unless flagged):**
  - `crates/tack-runner/src/engine.rs` — the wiring this card owns: a new
    optional `data_protocol: Option<Arc<dyn AttemptDataProtocol>>` field on
    `RunnerEngine`, a `with_data_protocol` builder, and the call sites
    (`submit_terminal_evidence`, `submit_cancellation_event`, `submit_event`,
    `submit_staged_artifact`) invoked from `run_claimed`'s completion and
    cancellation branches. Plus its own tests (5 new, all in this file).
  - **`crates/tack-runner/src/main.rs` — outside `Owns` as literally named,
    flagged not hidden.** Two small changes, both required for the
    acceptance to be provable at all, not stylistic: (1) one line —
    `.with_data_protocol(Arc::clone(&protocol) as Arc<dyn AttemptDataProtocol>)`
    — attaching the engine's new seam to the production `HttpPullProtocol`,
    which has implemented `AttemptDataProtocol` since III-H1 but was never
    attached to anything; without it, `engine.rs`'s new call sites compile
    but never run in the shipped binary, and III-H2's smoke step 7 UNMET
    line cannot disappear. (2) `report_capabilities`'s `artifacts` claim
    flips from `Unsupported` (main.rs's own prior comment: "the runner
    engine has no artifact upload call site yet") to `Advisory` — the
    reason string that made it `Unsupported` is no longer true, and leaving
    a stale `Unsupported` claim after fixing the gap it names would itself
    be a lie of omission under the "capability claims are load-bearing"
    rule. `decisions` stays `Unsupported`, reworded to the real remaining
    reason (see "Known limitations"). `main.rs` is not claimed by any other
    Wave 8 card's `Owns` list; if the integrator disagrees with folding this
    into H6, the fix is one line to revert plus reverting the two capability
    doc comments — not a structural change.

## Contract fixtures consumed

`docs/contracts/runner-v1/event-batch.request.json`, `event-batch.response.json`,
`artifact.request.json`, `artifact.response.json` — read only (not edited; not
this card's to touch). `AttemptDataProtocol`'s existing Rust types
(`EventBatchReport`, `ProtocolEvent`, `EventBatchResponse`,
`ArtifactManifestReport`, `ArtifactManifestItem`, `ArtifactUploadGrant`,
already fixture-round-tripped by III-H1's own tests in `transport.rs`) are
consumed as-is; none were changed.

## Behavior implemented

- **One event per terminal outcome.** On a successful/failed/cancelled
  completion, `submit_terminal_evidence` submits one `attempt.terminal` event
  whose payload is the exact `terminal_reason` JSON the harness adapter
  produced (D1/D2/D3's own `{code, message, ...}` shape) — never a
  runner-invented summary. On a committed cancellation,
  `submit_cancellation_event` submits one `attempt.cancelled` event carrying
  the adapter's own `CancellationEvidence` (`observation`, `details`).
- **Deterministic event id.** `event_id` is derived from
  `(attempt_id, fencing_token, kind)` via a hex digest — never random or
  clock-based — so a caller that resubmits the identical logical event
  (after a transient transport error, for example) reuses the same id.
  `docs/contracts/runner-v1/event-batch.request.json`'s own rule is a unique
  `(attempt_id, event_id)`; the server is the authority on treating a
  resend as `duplicate_event_ids`, which `resubmitting_the_same_terminal_event_is_idempotent`
  proves from the client side against a fake that mirrors that dedup rule.
- **Artifact upload reads real bytes.** D1/D2/D3's adapters already stage a
  `terminal_reason.artifact` JSON object (`kind`, `name`, `media_type`,
  `sha256`, `staged_path`, ...) via `harness::artifact::ArtifactStager` —
  before this card, nothing in the tree ever read that object back out.
  `submit_staged_artifact` reads it, re-reads the file's bytes from
  `staged_path` (never trusting the adapter-reported size), builds the
  manifest item, submits it, and uploads the content to whichever grant the
  server returns — mirroring `ArtifactStager`'s own "checksum from bytes
  actually handled, never a value merely reported" rule.
- **Best-effort, not part of the crash-safe replay path.** Unlike completion
  and cancellation reports (which are journaled and replayed exactly on
  restart via `send_pending_terminal_report`), event/artifact submission is
  fire-and-forget: a transport failure is logged (`tracing::warn!` with the
  attempt id only, never the payload) and the attempt's own terminal report
  proceeds regardless. This was a deliberate choice, not an oversight — see
  "Known limitations."
- **No-op when unconfigured.** `data_protocol` defaults to `None`; every
  existing `RunnerEngine::new`/`with_clock` call site (C4's `crash_matrix.rs`,
  H3's `h3_checkout.rs`) compiles and behaves exactly as before, unchanged,
  proven by the pre-existing 31 engine tests all still passing verbatim.
- **`CompletionReport.final_event_checkpoint` now reflects the real
  server-committed checkpoint, not the adapter's stale `None`.** This was
  found live, not in a unit test — see "The bug this card found and fixed
  in itself" below. It is part of "behavior implemented" because without it,
  submitting an event before completing genuinely breaks completion.

## Tests added and exact commands/results

Command: `cargo test -p tack-runner --lib engine`
Result: 36 passed, 0 failed (31 pre-existing + 5 new), all in
`crates/tack-runner/src/engine.rs::tests`.

New tests:
1. `run_once_with_a_data_protocol_submits_the_terminal_event_and_uploads_the_staged_artifact`
   — full `run_once` through a real temp file staged as an artifact; asserts
   exactly one event batch (kind `attempt.terminal`, correct payload), one
   artifact manifest (correct sha256/size/name), and one upload call whose
   bytes exactly match the file written to disk by the test itself.
2. `run_once_with_a_data_protocol_submits_a_cancellation_event` — a
   cancellation-requested claim reaches `RunCycle::Cancelled`, submits one
   `attempt.cancelled` event carrying the adapter's real `observation`.
3. `without_a_data_protocol_the_attempt_still_completes_and_nothing_is_submitted`
   — documents the no-op-when-absent behavior explicitly.
4. `data_protocol_transport_failure_does_not_block_the_attempts_own_completion`
   — a `FakeDataProtocol` configured to fail `submit_events`; the attempt
   still reaches `Completed`, and the failed submission left no accepted
   event behind (proves best-effort is real, not a swallowed error posing as
   success).
5. `resubmitting_the_same_terminal_event_is_idempotent` — calls the private
   `submit_event` twice with the same journal record; asserts the two
   submissions carry the identical `event_id` and that the fake's
   server-side dedup set (mirroring the frozen contract's unique
   `(attempt_id, event_id)` rule) never grows past one member.

Broader gates run on the full branch:
- `cargo test -p tack-runner` — 231 lib + 2 cli + 7 crash_matrix + 3
  g2_journal_corruption + 6 h3_checkout = 249 passed, 0 failed, 3 ignored
  (live-harness, opt-in, unchanged).
- `cargo test --workspace` — **1373 passed, 0 failed** (was 1368 at the H5
  merge baseline; +5 is exactly this card's new tests, everything else
  byte-identical).
- `cargo clippy -p tack-runner --all-targets -- -D warnings` — clean.
- `cargo fmt -p tack-runner -- --check` — clean (after `cargo fmt`).
- `cargo test -p tack-orch --test runner_contract` — 18/18, unchanged (no
  fixture touched).
- `cargo test -p tack-api --test wave2_gate` — 5/5, unchanged.
- **Live acceptance: `./scripts/smoke.sh`** (fake mode — real server, real
  runner binary, real scheduler/provisioner/adapter, shim harness binary; no
  smoke-script edit). Result: **`SMOKE PASSED`**, all of steps 7, 8 and 9
  green, and III-H2's UNMET line about events/artifacts is gone from the
  release verdict entirely. The verdict's only remaining line is `codex` not
  being installed on this machine (an environmental gap, unrelated to this
  card — see III-H2/H5's own handoffs, unchanged). Exact evidence from step 7:
  `event timeline: 1 events` (was the UNMET line before this card),
  `attempt ... succeeded`, `request state propagated to succeeded`. Run twice
  more for stability (three consecutive full runs, `SMOKE PASSED` every
  time) after the fix described below.

## The bug this card found and fixed in itself

The first live smoke run (before the fix below) did **not** disappear the
UNMET line cleanly — `event timeline: 1 events` passed, but the attempt's own
completion started failing with a `409 Conflict`, retried three times, then
gave up, leaving the attempt stuck non-terminal — and that stuck attempt then
starved the runner's own capacity, cascading into step 8/9 failures that had
nothing to do with them individually (confirmed by running the exact same
smoke script against the pre-card baseline commit, which passed cleanly, and
against this card's first draft, which failed identically on a clean
machine, twice, ruling out contention/flake).

Root cause, found by reading `crates/tack-api/src/handlers/runner_protocol.rs`'s
`submit_completion` and `crates/tack-db/src/repo/execution.rs`'s
`complete_execution_result` (both read-only, neither touched):
`complete_execution_result`'s compare-and-set `UPDATE ... WHERE ... AND
event_checkpoint IS ?` binds the completion request's own
`final_event_checkpoint` and requires it to equal the attempt row's
*current* `event_checkpoint` column. Before this card, that column was
always `NULL` at completion time (nothing ever wrote it), so the adapter's
permanently-`None` `HarnessOutcome.final_checkpoint` always matched. This
card's `submit_terminal_evidence` submits an event *before* building the
completion report — which moves the server's `event_checkpoint` column off
`NULL` — while the completion report still carried the adapter's stale
`None`. The `WHERE` clause stopped matching, `rows_affected() != 1`, and
every completion became a `Conflict`. The fix (in "Behavior implemented"
above) is to source `CompletionReport.final_event_checkpoint` from
`record.last_event_checkpoint` — which `submit_terminal_evidence` already
keeps in sync with whatever the server actually committed — instead of the
adapter's `outcome.final_checkpoint`. Confirmed load-bearing by reverting it
once: the same 409-conflict-then-cascade failure reproduced exactly, three
times, before the fix and reproduced zero times in three runs after.

## Escalation: artifact content upload 500s server-side (not this card's fix)

Also found live, in `tack-api`, outside `Owns`, **not fixed here**: the
artifact *manifest* POST (`/attempts/{id}/artifacts`) succeeds, but the
follow-up content PUT (`/attempts/{id}/artifacts/{artifact_id}/content`)
returns `500 Internal Server Error` in ~1-2ms every time, in every smoke run
(fake mode, this environment). Because this card's artifact upload is
best-effort, the 500 does not block the attempt's completion (proven by
`data_protocol_transport_failure_does_not_block_the_attempts_own_completion`'s
equivalent-shaped unit proof) — it is a genuine, separately-owned gap, not a
regression this card introduced. `put_artifact_content`
(`crates/tack-api/src/handlers/runner_protocol.rs`) returns `500` only from
`ArtifactContentError::UnsafeStorageLocation`/`Io` inside
`state.artifact_storage.store_streaming`; the storage root is
`{TACK_STORAGE_DIR}/execution-artifacts`, wired correctly in `router.rs`, but
nothing in this codebase's own tests exercised a live PUT against a freshly
started server before — `RunnerProtocolState::with_artifact_storage_root`
never had a real caller either, matching this card's own premise that these
routes were "proven only by fake-client tests" (III-H2's own words). Likely
cause: the `execution-artifacts` directory is never created ahead of the
first real write. Not investigated further — out of `Owns` (server-side
storage backend in `tack-api`, not `tack-runner`/`engine.rs`) — flagged here
as a concrete, reproducible finding for whichever card owns
`handlers/runner_protocol.rs::artifact_storage` next. §III.6's "verified
artifacts" criterion is therefore still not fully demonstrable end-to-end —
the manifest half is proven live by this card, the content-bytes half is not.

## Failure/adversarial case proved

`data_protocol_transport_failure_does_not_block_the_attempts_own_completion`:
configures the fake data protocol to fail every `submit_events` call, then
asserts (a) the attempt still reaches `RunCycle::Completed` (the harness's
real success is never held hostage by best-effort evidence upload), and (b)
`state.events` is empty afterward (the failed call really did not get
accepted anywhere — not a hidden success). This is the direct proof of the
"no hidden fake success" rule for this card's own failure mode.

`resubmitting_the_same_terminal_event_is_idempotent` is the direct proof of
the idempotency acceptance line: calling the real submission path twice
against the same journal record — the exact shape of a runner retry after a
transient error — produces the identical `event_id` both times and the
fake's dedup-aware response confirms the server side would see one logical
event, not two.

## Schema/API/contract change requested from another owner

None. `AttemptDataProtocol` and every fixture it round-trips against were
already frozen and correct for this card's needs; no gap was found.

## Known limitations or `not_measured` fields

- **Event/artifact submission is not replayed on restart.** If the runner
  crashes between a harness completing and the event/artifact call
  succeeding, that evidence is lost — unlike the completion/cancellation
  report, which is journaled before send and replayed exactly after a
  restart. This was a deliberate scope decision: making event/artifact
  submission crash-safe would mean extending `AttemptJournal` with pending
  event/artifact state and teaching `recover()` to replay it, which is a
  materially larger change than "give the engine its first call site" and
  was not needed to close the smoke's UNMET line. Recorded here as the next
  natural card if perfect durability is wanted.
- **Exactly one event per terminal outcome**, not a full structured event
  stream. `harness::event_sink::EventSink` (the bounded, backpressured,
  per-line event primitive D4 built) is still never constructed by any real
  adapter — this card did not wire it, because doing so is adapter-owned
  work (`claude_code.rs`/`codex.rs`/`opencode.rs`, D1/D2/D3 lineage, out of
  `Owns`). What ships today is real (a genuine event reaches the server,
  proving the timeline is non-empty and the criterion the smoke checks), but
  a future card could wire `EventSink`'s output through `submit_event` in a
  loop for a genuinely granular timeline instead of one terminal summary.
- **`decisions` stays `Unsupported`.** `AttemptDataProtocol::create_decision`/
  `poll_decisions` are reachable from `engine.rs` now (the trait bound and
  transport exist), but no harness adapter in this tree ever produces a
  question a decision would answer, so there is still no call site that
  would ever open one. Claiming `Advisory`/`Supported` here would be exactly
  the capability lie rule 7 forbids. The §III.6 "resolve a bounded decision"
  criterion remains open after this card — it needs a harness that actually
  asks a question, which is out of scope here.
- **Artifact upload is single-file, best-effort, no retry, no dedup — and
  currently fails server-side in this environment** (500 on the content PUT,
  see the escalation above; the manifest half works). Only the one artifact
  an adapter already stages via `terminal_reason.artifact` is uploaded; if an
  adapter ever stages more than one, or a future adapter encodes an artifact
  differently, only this one convention is read.
- **No usage/cost measurement changed.** Out of scope; untouched.

## Secrets/logging review

Every `tracing::warn!` this card adds carries `attempt_id` only (an opaque
runner-scoped identifier, not a secret) and a static message — never the
event payload, the artifact's file content, `staged_path` in full (it is a
local filesystem path under the runner's own state dir, not logged at all,
only read), or any protocol credential. `submit_event`'s payload is passed
through to the wire but never logged locally. No new secret-bearing field
was introduced.

## Safe merge order and likely conflicts

- Independent of H4, H7, H8 (all touch `tack-api`/`runner_protocol.rs`, this
  card touches only `tack-runner`). No file overlap with any sibling Wave 8
  card's `Owns` list.
- `main.rs`'s edit is small and localized (one builder call, two doc
  comments, two `CapabilitySupport` values); if another Wave 8 or later card
  also needs `main.rs`, the diff is easy to read and rebase around.
- No migration, no router, no OpenAPI change — nothing for C5/B2-style
  chokepoint conflicts to worry about.

## Checklist

- [x] No unowned files edited except the one flagged, justified exception
      (`main.rs`) — no router, OpenAPI, generated schema, migration, or
      other card's handoff touched.
- [x] No live secret in code, logs, or tests.
- [x] No panic stub / `unimplemented!()` — every new code path is either a
      real submission or an explicit, logged best-effort skip.
- [x] No blind retry — resubmission is proven idempotent by construction
      (deterministic id), not by a retry loop this card added.
