# III-E1 handoff

- **Base SHA / branch / final SHA:** base `b6dd0370564a3a4461b05d98d51d9e77c6d231c0`
  (`b6dd037`, tip of `plan/harness-agnostic-agent-fleet` at Wave 4 start — "docs: bring
  CLAUDE.md up to date with the runner fleet") / `agent/iii-e1-scheduler` / the commit
  containing this handoff (a commit cannot contain its own content-addressed SHA; see
  `git log -1` on this branch for the concrete value).
- **Files changed (must equal ownership list):**
  - New: `crates/tack-orch/src/scheduler/mod.rs`, `crates/tack-orch/src/scheduler/types.rs`,
    `crates/tack-orch/src/scheduler/select.rs`, `crates/tack-orch/src/scheduler/batch.rs`.
  - New: `crates/tack-orch/tests/scheduler_test.rs` (black-box tests through the public
    `tack_orch::scheduler` API only).
  - Modified: `crates/tack-orch/src/lib.rs` — exactly one line added, `pub mod scheduler;`,
    next to the existing `pub mod adapters;` / `pub mod execution;` / `pub mod reconciler;`.
    No other line in this file touched.
  - New: this handoff.
  - `git status --porcelain` confirms exactly this: `M crates/tack-orch/src/lib.rs`, plus the
    new `scheduler/` directory and `tests/scheduler_test.rs`. Nothing under `crates/tack-db`,
    `crates/tack-api`, `crates/tack-runner`, `frontend/**`, `docs/contracts/runner-v1/**`, root
    `Cargo.toml`/`Cargo.lock`, `TODO.md`, or any other handoff was touched.
- **Contract fixtures consumed:** `docs/contracts/runner-v1/capabilities.json` (the shape of
  `HarnessCapability`/`ModelCombination`/`Concurrency` — the scheduler reuses
  `tack_orch::execution::{HarnessCapability, ModelCombination}` verbatim rather than
  re-declaring a parallel shape) and `docs/contracts/runner-v1/limits.json` (`
  heartbeat_interval_seconds: 15` + `heartbeat_grace_seconds: 45` — `SchedulingPolicy::default()`'s
  60-second `max_heartbeat_age` is those two fixture values added together, not an invented
  number; it also happens to equal the same fixture's `lease_duration_seconds: 60`, i.e. a
  runner this card judges stale is a runner whose lease the API is independently entitled to
  treat as expired). **No fixture was edited.** `cargo test -p tack-orch --test runner_contract`
  passes unmodified (18/18, including the byte-pin over all 46 fixture paths — confirmed below).

## Behavior implemented

`crates/tack-orch/src/scheduler/` is a pure decision library: no `async`, no I/O, no clock
read internally (every function takes `now: DateTime<Utc>` as a parameter), no lease grant.
Two entry points:

- **`select::select_runner(request, candidates, now, policy) -> SelectionOutcome`** — one
  request against a candidate slice. For each candidate it checks, in order: selector match
  (exact runner id, or fleet membership via `RunnerCandidate::fleet_memberships`), `RunnerState
  == Active` (mirrors `agent_runners.state`'s three real values —
  `'pending_enrollment'`/`'active'`/`'revoked'`, confirmed by reading
  `crates/tack-db/src/repo/execution.rs`'s hand-written SQL literals directly, since no Rust
  enum exists there today), heartbeat freshness against `policy.max_heartbeat_age`, nonzero
  `available_capacity`, every `required_labels` key/value, the requested harness's presence in
  `candidate.harnesses` with no `probe_error`, and (for an explicit model request) that the
  matched harness's `model_combinations` actually lists the requested provider/model pair. The
  first failing check produces a named `IneligibleReason` for that candidate; passing every
  check makes it eligible. Among eligible candidates, the one with the most
  `available_capacity` wins (spreads load rather than always favoring the same runner); a true
  tie is broken by ascending `runner_id` purely for full, order-independent determinism.
  `NoEligibleRunner`'s `reasons` list is itself sorted by `runner_id` before returning, so the
  *whole* outcome — not only a successful pick — is independent of `candidates`' input order
  (proven in `tests/scheduler_test.rs`, not merely asserted).

- **`batch::schedule(requests, candidates, now, policy) -> Vec<(ExecutionRequestId,
  SelectionOutcome)>`** — several requests sharing one candidate pool. Orders requests by
  `Priority` (High → Normal → Low) then FIFO (`created_at` ascending) within a tier, then
  `request_id` as a final determinism tie-break; walks that order calling `select_runner` once
  per request against a *locally mutated* capacity ledger (a runner `select_runner` picks for
  an earlier request has its `available_capacity` reduced by one for every later request in
  the same call) so two same-priority requests contending for one single-slot runner do not
  both get told "yes." This local ledger is not persisted anywhere — the real,
  authoritative ledger stays `agent_runners.available_capacity`, mutated only by the
  repository's fenced claim.

- **`types.rs`** — every input/output type: `RunnerCandidate`, `SchedulingRequest`,
  `RunnerState`, `Priority`, `ModelSelector`, `IneligibleReason`, `Selection`,
  `SelectionOutcome`. `ModelSelector` is `Explicit { provider: RequestedModelProvider, model_id:
  RequestedModelId } | AutoSelect` — deliberately makes "one of provider/model set, the other
  absent" unrepresentable; `ModelSelector::from_parts` reconciles the two independently
  nullable wire fields `execution::ExecutionRequestSnapshot` actually carries (III.1.2) and
  reports the partial case as a typed `SchedulingError::PartialModelSelector` once, rather than
  as an identical `IneligibleReason` repeated against every candidate.

### Vocabulary discipline (III.0)

`SchedulingRequest.requested_model` uses `RequestedModelProvider`/`RequestedModelId` (the
*requested* namespace); `RunnerCandidate.harnesses`' `ModelCombination` uses the bare
`ModelProvider`/`ModelId` a runner *declares* — the two are compared via `.as_str()` inside
`evaluate_candidate`, never conflated into one type, honoring "a field called only `provider`
is rejected unless its type makes the namespace explicit."

## Tests added and exact commands/results

- `cargo test -p tack-orch --lib scheduler` — **24 passed, 0 failed** (table tests inside
  `select.rs`/`batch.rs`: empty, single-healthy, stale-heartbeat, missing-heartbeat, saturated,
  heterogeneous, tied, capacity-beats-lexical-id, exact-runner (present/absent/present-but-
  ineligible), fleet-membership, missing-label, harness-probe-error, undeclared-harness,
  undeclared-model-combination, auto-select-rejected, repeated-call determinism,
  `ModelSelector::from_parts` partial/complete cases, plus `batch`'s priority/FIFO/capacity-
  consumption/order-independence/empty-batch cases).
- `cargo test -p tack-orch --test scheduler_test` — **6 passed, 0 failed** (black-box, through
  `tack_orch::scheduler`'s public API only): full-permutation (4! = 24, and 3! = 6) order-
  independence for both a `Selected` outcome and a `NoEligibleRunner` outcome, full-permutation
  order-independence for `batch::schedule`, a structural "a `Selection` carries no fencing
  token/lease timestamp" pin, and the stale-heartbeat boundary adversarial case below.
- `cargo test -p tack-orch` (whole crate, every existing suite plus these two) — **128 lib +
  37 + 5 + 13 + 2 + 18 + 6 + 2 = 211 passed, 0 failed, 2 ignored** (the two pre-existing opt-in
  doctests). Lib went from 104 (Wave 3 baseline, per `III-D5.md`'s workspace total of 1046)
  to 128 — the 24 new scheduler unit tests, nothing else added or removed.
- `cargo test --workspace` — **1076 passed, 0 failed, 6 ignored**. Wave 3's accepted baseline
  (`III-D5.md`) was 1046 passed; 1046 + 30 (24 + 6, this card's new tests) = 1076, confirming no
  other suite in the workspace changed.
- `cargo clippy -p tack-orch --all-targets -- -D warnings` — clean.
- `cargo fmt -p tack-orch -- --check` — clean (ran against the whole crate; the only files with
  formatting diffs before running `cargo fmt -p tack-orch` were this card's own four new files —
  confirmed via `cargo fmt -p tack-orch -- --check | grep '^Diff in'` before and
  `git status --porcelain` after, which shows only `lib.rs` + the new `scheduler/`/test files
  touched).
- `cargo check --workspace` — clean (all six crates, confirming this card did not break
  `tack-api`/`tack-runner`/`tack-cli`'s compilation even though it is not their owner).

## Failure/adversarial case proved

`stale_heartbeat_wins_over_capacity_when_both_would_otherwise_pass`
(`crates/tack-orch/tests/scheduler_test.rs`) is the load-bearing proof, not just a table-test
entry: a candidate with ample capacity and a perfectly matching harness/model is rejected the
instant its heartbeat crosses `policy.max_heartbeat_age` (one second past → rejected; one
second inside → selected, everything else held equal), directly proving the Wave 3
carry-forward instruction ("E1's scheduler must read the capability snapshot, never assume
cancellation works" — generalized here to freshness) is load-bearing rather than decorative.
I additionally hand-verified this is load-bearing by temporarily deleting the `if stale { return
Err(...) }` block from `evaluate_candidate` and re-running this test: it failed (`NoEligibleRunner`
expected, got `Selected`), then restored the block and re-ran to confirm green again — the same
"revert the fix and watch it fail" discipline `CLAUDE.md` asks for on a "rejects before X"
claim. The full-permutation order-independence tests are the second adversarial class: rather
than asserting determinism on one or two hand-picked reorderings, `selection_is_identical_
across_every_permutation_of_a_heterogeneous_fleet` and
`no_eligible_runner_outcome_is_identical_across_every_permutation` enumerate every one of 4! =
24 and 3! = 6 arrangements of a mixed-eligibility candidate set and assert byte-identical
`SelectionOutcome` for every single one — a bug that depended on iteration order (e.g. a
`HashMap` instead of `BTreeMap`, or a missing final sort) would have a 23-in-24 chance of
surfacing here even if it happened to pass on the first arrangement tried.

## Schema/API/contract change requested from another owner

Not made — recorded here per III.2 rule 2 ("a file under `Must not edit` is a hard stop;
record the needed change in the handoff") for whichever card integrates this module:

1. **No `priority` column exists on `execution_requests`** (migration 044,
   `crates/tack-db/src/migrations.rs` — `state`, `selector_kind`, `selector_id`,
   `requested_harness_kind`, `requested_model_provider`, `requested_model_id`, etc., but no
   priority field of any kind). `batch::schedule` needs a `Priority` value per request; today
   nothing upstream can supply anything but the default (`Priority::Normal` for every request,
   which degrades `schedule`'s ordering to pure FIFO). Whoever wires this module to the real
   queue should either add a migration for a real `priority` column (B2's chokepoint, batched
   at a wave boundary per III.2 rule 4) or define a policy for deriving one from
   `execution_requests.metadata` — I did not choose between these, since both are schema/policy
   decisions outside this card's ownership.
2. **`agent_runners.state` and `execution_requests.selector_kind`/`selector_id` are
   hand-written SQL string literals, not typed Rust enums, in `crates/tack-db/src/repo/execution.rs`.**
   This card's `RunnerState` (`types.rs`) is a typed mirror an integration card can convert
   to/from those three literal strings — I did not add a shared type to `tack-db` itself, since
   `crates/tack-db/src/repo/execution.rs` is not in this card's ownership and rule 2 is explicit
   about not editing across that boundary.
3. **The claim query in `crates/tack-db/src/repo/execution.rs` (around line 1805) is a raw
   `ORDER BY created_at LIMIT 1` match on `selector_kind`/`selector_id` alone** — no capacity,
   label, harness, or model-combination filtering happens there at all today. This module is
   built to be the drop-in replacement for that decision (or a pre-check ahead of it), but
   wiring it in is real integration work (fetching `agent_fleet_members`/`capability_snapshot`
   into `RunnerCandidate`s, calling `select_runner`/`schedule`, and only then performing the
   repository's fenced claim on the chosen runner) that belongs to a later card, not this one —
   "pure selection... never grants the authoritative lease" is this card's explicit boundary.
4. **`agent_fleets.concurrency_limit` (a nullable fleet-wide cap, migration 039) is not
   enforced anywhere in this module.** `RunnerCandidate` carries only per-runner capacity; a
   fleet-wide ceiling would require the caller to also pass current fleet-wide utilization,
   which is not this card's concern to invent a shape for absent a concrete caller. Flagged as
   an explicit gap in "Known limitations" below, not silently ignored.

## Known limitations or `not_measured` fields

- **`ModelSelector::AutoSelect` is unconditionally rejected for every candidate**, with the
  named reason `IneligibleReason::AutoSelectNotVerified`. This is a deliberate v1 decision, not
  an oversight: no field in `docs/contracts/runner-v1/capabilities.json` (or anywhere else in
  the frozen contract) attests that a harness safely accepts an unspecified model, and the
  Wave 3 carry-forward found that two of three real adapters (Codex, OpenCode) reject
  auto-select pre-spawn rather than fabricate a selection — the third (Claude Code) can only
  *confirm* a model post-hoc, which is a different guarantee than *safely accepting* an absent
  one pre-spawn. Rather than have the scheduler guess which harness might tolerate it, every
  auto-select request is refused with a named reason today. If a future capability field ever
  attests to safe auto-select support, `evaluate_candidate`'s `ModelSelector::AutoSelect` arm is
  the one place to change.
- **No fleet-wide concurrency enforcement** — see item 4 above.
- **No dollar/token cost dimension anywhere in scheduling.** This card's "capacity" is purely
  concurrency slots (`Concurrency`-shaped: total/available), never a budget or cost figure —
  consistent with `tack-orch`'s crate-wide "money is always an estimate" discipline, which this
  module simply never touches (no `*_usd_estimated` field appears here at all, because none of
  the frozen runner-v1 fixtures this card reads carry one).
- **This module is not wired to any real data source.** Every type in `types.rs` is built to
  make that wiring mechanical (field names and shapes deliberately mirror the real
  `agent_runners`/`agent_fleets`/`agent_fleet_members` columns), but no caller in this
  repository constructs a `RunnerCandidate` or `SchedulingRequest` from a live database row
  yet — that is explicitly a later integration card's job (E6, per the Wave 4 dependency graph).

## Secrets/logging review

This module contains **no `tracing::*!` call of any kind** — it is a pure, synchronous
function library with no logging surface at all. There is therefore nothing here that could
leak a credential, prompt body, query string, or environment value; the "tests assert
redaction" requirement (III.2 rule 12) does not apply because there is no log output to
redact. Every type in `types.rs`/`select.rs`/`batch.rs` carries only ids, labels, capacity
numbers, timestamps, and harness/provider/model identifiers — the same category of data the
frozen `docs/contracts/runner-v1/capabilities.json` fixture itself exposes on the wire, nothing
from `PermissionPolicy`, `EnvironmentValue`, or any `RunnerCredential`-adjacent type is
referenced anywhere in this module.

## Safe merge order and likely conflicts

- This card branched from `b6dd037` (accepted Wave 3 integration SHA `6a53a18` plus Wave 4's
  doc-update commits) and only ever touched `crates/tack-orch/src/lib.rs` (one added line) plus
  entirely new files. E2 and E5 (the other Wave 4 cards running concurrently, per the prompt)
  own `frontend/src/shared/execution/**` and `tack-cli` respectively — disjoint from every file
  this card touched, so no conflict is expected merging against either.
- The one shared file this card modified, `crates/tack-orch/src/lib.rs`, currently has three
  `pub mod` lines (`adapters`, `execution`, `reconciler`); this card adds a fourth
  (`scheduler`), alphabetically adjacent. Rebasing after another Wave 4 card that also touches
  `lib.rs` (none currently listed as owning it) would be a trivial adjacent-line merge, not a
  semantic conflict.
- E6 (Wave 4's integration/spec owner) is the expected next consumer: it will need to construct
  `RunnerCandidate`/`SchedulingRequest` from real `agent_runners`/`agent_fleet_members`/
  `execution_requests` rows and call `select_runner`/`schedule` ahead of (or as a replacement
  for) the existing naive claim-query match in `crates/tack-db/src/repo/execution.rs` — see
  "Schema/API/contract change requested from another owner" above for the exact seam and the
  gaps it will need to resolve (priority source, fleet-wide concurrency).

## Checklist

- No unowned files: confirmed via `git status --porcelain` — exactly
  `crates/tack-orch/src/lib.rs` (one line), the new `crates/tack-orch/src/scheduler/` module,
  `crates/tack-orch/tests/scheduler_test.rs`, and this handoff.
- No live secret: this module has no logging surface at all (see "Secrets/logging review"
  above); no test constructs or references a credential.
- No panic stub: no `unimplemented!()`/`todo!()`/bare `panic!()` anywhere in non-test code;
  every rejection path returns a typed `IneligibleReason`, `SchedulingError`, or
  `SelectionOutcome` variant.
- No blind retry: this module contains no retry logic of any kind — it is a single pure
  computation per call, by design ("performs no I/O").
