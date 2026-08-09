# III-C2 handoff

- Base SHA / branch / final SHA: Wave 1 accepted integration SHA `f14019b` (per
  `TODO.md`'s Wave 2 row) / `plan/harness-agnostic-agent-fleet` / recorded by the commit
  containing this handoff (not committed by this card — see below).
  This card worked directly in the shared main checkout (as instructed), not an isolated
  worktree; branch HEAD at the time of this work was `2850300` (C1's accepted commits already
  present). `crates/tack-db/src/repo/execution.rs` and `crates/tack-db/tests/execution_repo_test.rs`
  carried the pre-existing uncommitted B2 "concurrent enrollment redemption" fix noted in the
  card brief; that diff was read, not reverted or modified. Several other Wave-2/Wave-1 cards
  (C1, B1, C3) were also actively editing this same shared checkout concurrently while this card
  ran — see "Safe merge order" below for how that was handled.
- Files changed (must equal ownership list):
  - `crates/tack-api/src/handlers/runner_protocol.rs` (new)
  - `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs` (new)
  - `crates/tack-api/tests/c2_handlers_test.rs` (new)
  - `docs/agent-handoffs/part-iii/III-C2.md` (new, this file)

  No other file was created or edited by this card. `git status --porcelain` at hand-off time
  also shows `crates/tack-api/src/handlers/{executions,runner_admin}.rs`,
  `crates/tack-api/tests/c1_handlers_test.rs`, `crates/tack-db/src/repo/execution.rs`,
  `crates/tack-db/tests/execution_repo_test.rs`, `crates/tack-orch/src/execution/types.rs`,
  `crates/tack-runner/src/workspace.rs`, and handoffs `III-B1.md`/`III-B2.md`/`III-C3.md` as
  modified — all of that is *other* cards' concurrent work in the same shared checkout (C1, B1,
  C3), none of it touched by this card. `crates/tack-api/tests/c2_handlers_test.rs` reads
  `crates/tack-api/src/handlers/executions.rs` via `#[path]` (same technique C1's own test uses
  on itself) purely to exercise the real operator router in the auth non-substitution test; it
  is not edited.

- Contract fixtures consumed: `protocol.json` (base path, auth model, stable error codes,
  additive-operation path), `limits.json` (every constant, checked against a fixture-equality
  test), `lifecycle-transitions.json` (via B2's `transition_attempt_with_facts` /
  `recover_attempt`, not re-implemented here), `enrollment.request/response.json`,
  `refresh.request/response.json`, `claim.request/response.json`,
  `claim.no-work.response.json`, `heartbeat.request/response.json`,
  `event-batch.request/response.json`, `decision.create.request/response.json`,
  `decision.poll.request/response.json`, `artifact.request/response.json`,
  `completion.request/response.json`, `cancellation.request/response.json`,
  `recovery-observation.request/response.json`, `capabilities.json`, and every file under
  `errors/`. No fixture file was edited.

- Behavior implemented: a card-local, unregistered `/api/runner/v1` handler set —
  enrollment exchange, capability refresh (with credential rotation), claim (with no-work),
  heartbeat batch, accept (`leased -> preparing`) / start (`preparing -> running`), event
  batch, decision create + poll, artifact manifest, completion, cancellation observation, and
  the additive recovery observation — plus `runner_auth.rs`, the sole authentication seam
  every route above passes through. Every write resolves runner identity from a hashed
  `Authorization: Bearer` credential (never a request-body field), cross-checks the body's
  `runner_id`/`attempt_id` against that identity and the path, and passes the *authenticated*
  runner id (not the body's) into every B2 repository call so fencing is enforced by identity,
  not by client claim. Every payload is checked against `limits.json` before any repository
  call. B1's frozen types (`ActualExecution`, `Usage`, `RecoveryObservationRequest`/
  `RecoveryObservationResponse`, the `ProtocolError` envelope shape) are reused directly rather
  than re-defined, per the card brief.

- Tests added and exact commands/results:
  - `cargo test -p tack-api --test c2_handlers_test` — **15 passed, 0 failed.** Covers: full
    enroll→claim→accept→start→events→decision(create+poll)→artifact→completion lifecycle
    through a freshly-enrolled runner (`full_runner_protocol_lifecycle_enroll_through_completion`);
    operator/runner auth non-substitution
    (`operator_auth_cannot_substitute_for_runner_auth_and_vice_versa`); stale fence and expired
    lease writing nothing (`stale_and_expired_fence_write_nothing`); idempotent heartbeat/
    completion replay vs. a same-key conflicting retry
    (`heartbeat_and_completion_idempotent_replay_and_conflicting_replay_are_distinguished`); an
    oversized event batch (count over `event_batch_count_max`, and a payload over the shared
    body-size cap) writing nothing (`oversized_event_batch_writes_nothing`); decision/artifact
    id reuse with different content being a stable `idempotency_conflict` that does not
    overwrite the original row (`decision_and_artifact_id_reuse_with_different_content_is_idempotency_conflict`);
    a proven pre-spawn-stopped recovery observation safely requeuing the request and replaying
    idempotently (`recovery_observation_safe_requeue_requeues_the_request_and_replays_idempotently`);
    and a captured-log assertion that raw enrollment tokens and runner credentials never appear
    in emitted logs while `runner_id` does (`logs_never_contain_raw_credentials_only_ids`).
    Seven more unit tests inside `runner_protocol.rs`/`runner_auth.rs` cover the `limits.json`
    fixture-equality check, protocol-version rejection shape, `Z`-vs-`+00:00` timestamp
    normalization, capability-payload validation, and the runner/attempt cross-check helpers.
  - `cargo clippy -p tack-api --test c2_handlers_test -- -D warnings` — **clean.**
  - `rustfmt --check --edition 2024` on all three owned Rust files — clean after running
    `rustfmt --edition 2024` once on them (rule 10: only files this card owns were formatted).
  - `git diff --cached --check` (files staged only for this check, then immediately
    `git reset` — see below) — **clean**, no whitespace errors. (`git diff --check` alone is a
    no-op for brand-new untracked files; staging was the only way to run the literal check the
    card asked for. Nothing else was staged or touched.)
  - `git status --porcelain` — confirmed to list only this card's four new files plus the
    concurrent, unrelated changes from other in-progress cards described above.

- Failure/adversarial case proved:
  - A runner bearer credential alone cannot reach any operator route (`create_execution`,
    `requeue_needs_operator` both checked — both require `x-tack-principal`, which a runner
    credential never supplies), and an operator-style `x-tack-principal` header alone cannot
    reach any runner route (`runner_auth::authenticate` reads only `Authorization`). Neither
    credential type is even inspected by the other's code path.
  - A stale fencing token on a real attempt, and a correctly-fenced but lease-expired attempt
    (advanced via a fake clock, no sleep), both return `stale_lease` and leave the
    `execution_events`/`execution_attempts` rows completely unchanged.
  - An oversized event batch — either over `event_batch_count_max` (101 events) or over the
    shared body-size cap that also bounds `event_batch_bytes_max` (both 1 MiB in `limits.json`)
    — returns `payload_too_large` and writes zero rows to `execution_events`, and leaves
    `execution_attempts.event_checkpoint` untouched.
  - Reusing a `decision_id`/`artifact_id` with different content returns `idempotency_conflict`
    and does not overwrite the originally-stored row (verified by re-reading the stored
    `prompt`/`sha256`), closing a gap in B2's `ON CONFLICT ... DO NOTHING` inserts (see below).
  - A same-key heartbeat/completion retry with *different* content returns a conflict code and
    writes no second replay record; an *exact* retry returns the original success
    (`accepted_at`/`committed_at` unchanged).
  - Enrollment: the raw runner credential is returned exactly once (issuance response); only
    its SHA-256 hash is queryable in `agent_runners`; a rotated credential immediately
    invalidates the old one.

- Schema/API/contract change requested from another owner:
  1. **~~Capability payload shape ambiguity~~ — RESOLVED, see "Three-review fix-up adoption"
     below (Task 4).** `refresh.request.json`'s embedded `capabilities.features: {}` and
     `harnesses: []` cannot satisfy B1's `FeatureCapabilities` (which requires all five support
     keys), unlike `enrollment.request.json`'s fuller example. Rather than force strict
     `tack_orch::execution::RunnerCapabilities` parsing (which would reject the frozen
     `refresh.request.json` fixture itself), this card originally validated only the
     structurally-needed `concurrency`/`labels` sub-objects and stored the rest of the capability
     payload as an opaque, size-bounded JSON blob. B1 has since added
     `tack_orch::execution::EmbeddedCapabilitySnapshot` for exactly this embedded shape
     (`features` opaque, everything else typed); `validate_capability_payload` now parses against
     it directly. No future B2/A0 action needed for this item.
  2. **~~No B2 repository method for credential rotation~~ — RESOLVED, see "Three-review fix-up
     adoption" below (Task 2).** `redeem_enrollment_token` sets the initial credential hash;
     nothing updated it afterward, so `/refresh` with `rotate_credential: true` originally wrote
     `agent_runners.credential_hash`/`credential_expires_at`/`credential_rotated_at` directly via
     `Repository::pool()` with no compare-and-set. B2 has since added
     `Repository::rotate_runner_credential` (a CAS keyed on the authenticated credential hash);
     `refresh`'s `rotate_credential` branch now uses it. No future B2/A0 action needed for this
     item.
  3. **Decision/artifact inserts are `ON CONFLICT ... DO NOTHING` with no fingerprint.** Unlike
     every other write path in `execution.rs` (heartbeat, events, completion, cancellation,
     recovery), `create_execution_decision`/`record_execution_artifact` silently no-op on an
     id collision regardless of content, so a naive caller could get a false "success" for a
     changed retry. This card compensates by reading the committed row back and comparing it to
     the request, returning `idempotency_conflict` on a mismatch — but this is a per-caller
     workaround, not a database-level guarantee. A future B2 amendment adding a durable
     fingerprint/replay table for these two writes (mirroring the others) would remove the need
     for this compensating read.
  4. **~~`EventApplyResult::ReplayConflict` and `CompletionResult::Conflict` each collapse
     multiple distinct causes into one variant~~ — RESOLVED, see "Three-review fix-up adoption"
     below (Task 1).** (e.g. "same checkpoint, different content" vs. "checkpoint out of order"
     for events). This card originally mapped both to the more conservative, retryable `conflict`
     code rather than the non-retryable `idempotency_conflict`, since the out-of-order case is a
     benign resync a client should retry — but that meant a runner that reused an idempotency key
     with genuinely different content was told to retry forever, which it never could. B2's
     "Three-review fix-up" amendment split both enums into `IdempotencyConflict`/`Conflict`; this
     card now maps each to its correct wire code. No future B2/A0 action needed for this item.
  5. **"Accept"/"start" have no named request/response fixture pair.** Every other endpoint in
     this card is backed by a `docs/contracts/runner-v1/*.request.json`/`*.response.json` pair;
     accept/start are not (only the lifecycle *states* `preparing`/`running` are frozen). This
     card designed their wire shape directly from B2's `AttemptTransitionInput`/
     `AttemptTransitionResponse` and the `leased -> preparing -> running` (`lease_owner`) rule in
     `lifecycle-transitions.json`. A0/D5 should fold this into a future frozen-fixture revision
     if a real adapter needs a different shape; nothing here presumes route *names*, only field
     shapes, since route paths are this card's own choice per `protocol.json`'s own framing.

- Known limitations or `not_measured` fields:
  - Artifact **content** upload/download (`PUT /artifacts/{id}/content`) is out of this card's
    scope; only the manifest endpoint (`manifest_accepted` + an upload URL/expiry) is
    implemented, matching the card task "artifact manifest" specifically.
  - Decision **resolution** (an operator answering a pending decision) has no endpoint anywhere
    in Wave 2 — neither this card nor C1's. This card implements create + poll only, matching
    the card task and `protocol.json`'s note that `decision_resolution` is a *separately scoped
    operator credential* concern for a later wave (F5, per the roadmap capsule).
  - `/refresh`'s reported `concurrency.total`/`available` are validated and stored in the
    capability snapshot but not written into `agent_runners.available_capacity`; that column
    remains authoritative only through `heartbeat_batch`/claim/completion/recovery, matching
    B2's existing capacity-accounting design. This is a deliberate choice, not an oversight —
    letting `/refresh` also mutate capacity would create a second, unfenced writer.
  - A multi-artifact manifest submission is not committed as a single database transaction (B2
    has no batched artifact-insert method, unlike the batched event API); a fencing change
    concurrent with a multi-artifact request could in principle partially apply. This card
    narrows the window with a single upfront attempt/fence/lease pre-check before the loop, but
    does not eliminate it — see gap #3 above.
  - The upload-URL expiry window (10 minutes) and the no-work claim retry hint (5000 ms) are
    this card's own reasonable choices; neither is fixed by any frozen fixture.
  - Runner-credential lifetime (90 days) is likewise this card's own choice, absent a frozen
    constant for it.

- Secrets/logging review: `runner_auth::authenticate` and `enroll`/`refresh` log only
  `runner_id` (via `tracing::debug!`/`tracing::info!`); no handler logs a raw bearer credential,
  enrollment token, request body, or query string anywhere. `logs_never_contain_raw_credentials_only_ids`
  captures real `tracing` output across an enrollment and a failed-auth attempt and asserts the
  issued runner id is present while the raw enrollment token, the issued credential, and a bogus
  bearer value are all absent. Credentials are stored only as SHA-256 hashes
  (`runner_auth::credential_hash`); a raw credential is returned exactly once, in the
  enrollment/rotation response, and is never re-derivable from anything this card persists.

- Safe merge order and likely conflicts: merge after the accepted B1/B2 baseline this card
  built on; before C5 (which is the only card that may register these routes in
  `handlers.rs`/`router.rs`/`openapi.rs`). This card touches no file C1, B1, or C3 touch, so
  there is no line-level merge conflict with their concurrent work observed in this shared
  checkout — the only shared surface is that `crates/tack-api/tests/c2_handlers_test.rs` reads
  (via `#[path]`) the *content* of C1's `executions.rs` at test-compile time, so if C1's final
  accepted version changes `OperatorExecutionState`'s public shape or `executions::routes`'s
  signature, this test file (not the handler it tests) would need a small follow-up edit — that
  risk is inherent to the `#[path]`-inclusion technique the card brief specified and mirrors
  C1's own test doing the same to itself.

- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Retryability-authority amendment

- Base / contract / final: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`, after B1's own "Retryability-authority amendment"
  (`docs/agent-handoffs/part-iii/III-B1.md`) landed uncommitted in the same shared checkout.
  Per this task's instructions the work is left uncommitted, so no final SHA is recorded here.
- Owned changes: `crates/tack-api/src/handlers/runner_protocol.rs`,
  `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`,
  `crates/tack-api/tests/c2_handlers_test.rs`, and this file — the same four files this card
  already owns. No file under `crates/tack-orch/**` was touched; B1's new
  `StableErrorCode::retryable`, `ProtocolError::new`, and `ProtocolErrorEnvelope::new` were
  consumed as-is.
- Defect fixed: `runner_auth::protocol_error` took `code: &'static str` and re-derived
  `retryable` locally via `matches!(code, "conflict" | "rate_limited" | "internal_error" |
  "artifact_checksum_mismatch")`. The classification itself was correct — it agreed with every
  fixture under `docs/contracts/runner-v1/errors/` — but it was a second, hand-maintained copy of
  contract data keyed on a bare string, with no test tying it to the fixtures. Per III.1.6
  ("hand-written feature DTOs are not another authority"), a fixture reclassification would have
  silently drifted from this copy. B1's amendment adds the single authority
  (`StableErrorCode::retryable`, guarded by a fixture-reading conformance test) and explicitly
  calls out "III-C2 should use this API from the start" as the follow-up this section closes out.
- Changed: `protocol_error` now takes `code: StableErrorCode` and builds its body via
  `tack_orch::execution::ProtocolErrorEnvelope::new(code, message, RUNNER_REQUEST_ID, details)`,
  serialized directly — matching the pattern C1 already established in `handlers/executions.rs`
  and `handlers/runner_admin.rs` (their local `error()` helper, same shape). The local `matches!`
  classification is deleted outright, not just deprecated. All four convenience wrappers
  (`invalid_request`, `payload_too_large`, `stale_lease`, `forbidden`) and the seven inline call
  sites inside `authenticate` were converted to pass a `StableErrorCode` variant. In
  `runner_protocol.rs`, all 15 direct `protocol_error(...)` call sites (`check_protocol_version`,
  `internal_error`, `enroll`'s invalid-token rejection, `refresh`'s `revoked_error` closure,
  `heartbeat`'s idempotency conflict, `transition_attempt`'s invalid-transition conflict,
  `submit_events`'s replay conflict, `create_decision`'s state-conflict and idempotency-conflict
  paths, `submit_artifacts`'s idempotency conflict, `submit_completion`'s conflict,
  `observe_cancellation_report`'s three conflict/terminal/ambiguous arms, and
  `observe_recovery`'s idempotency conflict) were likewise converted. `grep -n "retryable"` across
  both files now matches only the doc comment on `protocol_error` and a two-line code comment
  explaining the `conflict`-vs-`idempotency_conflict` choice for `EventApplyResult::ReplayConflict`
  — no executable `retryable` derivation remains in either file.
- Tests added: `event_checkpoint_conflict_response_carries_contract_correct_retryable_true` drives
  the real `/claim` then `/attempts/{id}/events` handlers (not the `protocol_error` helper
  directly) through a committed first event batch followed by a second batch whose
  `previous_checkpoint` no longer matches the attempt's committed stream position — the exact
  `EventApplyResult::ReplayConflict` path this card maps to the retryable `conflict` code rather
  than `idempotency_conflict`. It asserts, on the real serialized JSON response body,
  `error.code == "conflict"`, `error.retryable == true`, and `error.request_id == "req_runner"`,
  and confirms the rejected batch wrote no new `execution_events` row and left
  `execution_attempts.event_checkpoint` at the first batch's committed value.
- Exact commands/results: `cargo test -p tack-api --test c2_handlers_test` — **16 passed, 0
  failed** (the 15 pre-existing tests, unchanged, plus the one new test above).
  `cargo test -p tack-api --test c1_handlers_test` — **7 passed**, confirming C1's card was not
  disturbed. `cargo clippy -p tack-api --test c2_handlers_test -- -D warnings` — clean.
  `cargo test -p tack-orch --lib` — **103 passed**; `cargo test -p tack-orch --test
  runner_contract` — **18 passed**; both confirm B1/B4 were only read, never edited.
  `rustfmt --check --edition 2024` on all four owned files — clean. `git diff --check` — clean.
- Failure/adversarial case proved: the emitted JSON is unchanged for every non-retryable code
  and correctly `true` for the four retryable ones (`conflict`, `internal_error`, `rate_limited`,
  `artifact_checksum_mismatch`), because `StableErrorCode::retryable()`'s match arms are the same
  four codes the deleted local `matches!` already listed — the fix removes a duplicate authority
  without changing any wire output. All 15 pre-existing tests in `c2_handlers_test.rs`, several of
  which assert exact `error.code`/`error.details.*` values on real handler responses (e.g.
  `oversized_event_batch_writes_nothing`, `decision_and_artifact_id_reuse_with_different_content_is_idempotency_conflict`),
  pass byte-for-byte unmodified against the new envelope construction — the strongest available
  evidence that no response shape shifted.
- Schema/API/contract change requested from another owner: none. This section closes the
  follow-up B1's "Retryability-authority amendment" explicitly requested of this card; no new gap
  is opened.
- Known limitations or `not_measured` fields: none beyond what the original handoff above already
  lists (items 1–5 there remain open and are unaffected by this amendment).
- Secrets/logging review: unchanged — no log line, error message, or detail payload in either
  file was touched by this amendment beyond the `code` parameter's type.
- Safe merge order and likely conflicts: same as the original handoff above — merge after the
  accepted B1/B2 baseline (this amendment specifically depends on B1's uncommitted
  `crates/tack-orch/src/execution/types.rs` amendment being present first), before C5. No line in
  this amendment touches a file any other card owns.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Three-review fix-up adoption: conflict-split match arms, credential-rotation CAS, and independent-verifier cleanups

- Base / branch / final SHA: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`, after B2's own "Three-review fix-up" amendment
  (`docs/agent-handoffs/part-iii/III-B2.md`) landed uncommitted in the same shared checkout,
  which is what this amendment exists to adopt. Per this task's instructions the work is left
  uncommitted, so no final SHA is recorded here.
- Owned changes: `crates/tack-api/src/handlers/runner_protocol.rs`,
  `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`,
  `crates/tack-api/tests/c2_handlers_test.rs`, and this file — the same four files this card
  already owns. No file under `crates/tack-db/**` or `crates/tack-orch/**` was touched; B2's new
  `EventApplyResult::{IdempotencyConflict, Conflict}`, `CompletionResult::{IdempotencyConflict,
  Conflict}`, and `Repository::rotate_runner_credential`/`CredentialRotationResult` were consumed
  as-is, and B1's `tack_orch::execution::EmbeddedCapabilitySnapshot` was consumed as-is.

### Task 1 — adopt B2's split conflict variants

`crates/tack-api` did not compile at the start of this amendment: B2 split the collapsed
`EventApplyResult::ReplayConflict` into `{IdempotencyConflict, Conflict}` and
`CompletionResult::Conflict` into `{IdempotencyConflict, Conflict}`, breaking this card's
exhaustive `match`es by design. Fixed with no catch-all `_ =>` arm, per the card brief:

- `submit_events` (the handler for `POST /attempts/{id}/events`, i.e. "`append_events`" in the
  card brief): the single `EventApplyResult::ReplayConflict` arm is now two arms —
  `EventApplyResult::IdempotencyConflict => StableErrorCode::IdempotencyConflict` (409, message
  "The event batch checkpoint was already used with different event content") and
  `EventApplyResult::Conflict => StableErrorCode::Conflict` (409, the original message,
  "The event batch checkpoint does not match the attempt's current stream position"). The stale
  comment claiming B2 "collapses" the two cases (previously just above the old single arm) is
  replaced with one explaining the actual split and pointing at B2's handoff section.
- `submit_completion`: added `CompletionResult::IdempotencyConflict =>
  StableErrorCode::IdempotencyConflict` (409, "The completion_id was already used with different
  content") alongside the pre-existing `CompletionResult::Conflict => StableErrorCode::Conflict`
  arm, unchanged.
- Wire mapping matches the frozen fixtures exactly: `idempotency_conflict` →
  `errors/idempotency-conflict.json` (`retryable: false`); `conflict` → `errors/conflict.json`
  (`retryable: true`, unchanged from before this amendment). Both codes already route through
  B1's single retryability authority (`StableErrorCode::retryable`, consumed via
  `runner_auth::protocol_error`) adopted in the prior amendment above, so no new retryability
  logic was written.
- `crates/tack-api/tests/c2_handlers_test.rs`'s stale comment block above
  `event_checkpoint_conflict_response_carries_contract_correct_retryable_true` (documenting the
  old collapsed behavior as intentional) is rewritten to describe the actual split and to point at
  its new sibling test below.
- **New tests proving the previously-untested fingerprint-mismatch path, on both endpoints:**
  - `event_batch_replay_changed_content_is_idempotency_conflict_and_writes_nothing`: commits a
    first event batch at checkpoint `"cp-1"`, then resubmits the *same* checkpoint (the
    idempotency-scoped key) with the same `previous_checkpoint` but a changed event payload.
    Asserts `error.code == "idempotency_conflict"`, `error.retryable == false`, that
    `execution_events` still has exactly the one original row, and that the stored payload is
    unchanged (`"original"`, not `"CHANGED"`).
  - `completion_replay_changed_content_is_idempotency_conflict_and_writes_nothing`: commits a
    completion, then resubmits the same `completion_id` with `terminal_state` changed from
    `succeeded` to `failed`. Asserts `idempotency_conflict` / `retryable: false`, exactly one
    replay row in `execution_completion_replays`, and the attempt's stored state still
    `succeeded`. The pre-existing `event_checkpoint_conflict_response_carries_contract_correct_retryable_true`
    test is kept unchanged — it drives only the benign, out-of-order-resync `Conflict` path (a
    fresh checkpoint whose `previous_checkpoint` no longer matches, not a reused checkpoint) — so
    the contrast between the two causes is now explicit across two tests instead of one test
    silently exercising only one branch.

### Task 2 — adopt B2's compare-and-set credential rotation

`refresh`'s `rotate_credential` branch previously wrote `agent_runners.credential_hash` directly
through `Repository::pool()` with no predicate on the currently-authenticated hash. Replaced with
`Repository::rotate_runner_credential(runner_id, expected_credential_hash, new_credential_hash,
credential_expires_at, clock)`:

- `runner_auth::RunnerPrincipal` gained a `credential_hash: String` field (the SHA-256 hash of the
  bearer credential that was actually authenticated), populated in `authenticate()` and used as
  `expected_credential_hash` — this avoids re-deriving the hash from the `Authorization` header a
  second time in the handler, and keeps the CAS keyed on the exact identity `authenticate()` just
  verified. The one other `RunnerPrincipal` construction site (a `runner_auth.rs` unit test) was
  updated to supply a placeholder hash.
- `CredentialRotationResult::HashMismatch` is mapped to `StableErrorCode::Conflict` (409, "The
  runner credential changed before this rotation committed"), **not** the pre-existing
  `runner_revoked` error. Justification, recorded in a code comment at the call site: B2's own doc
  comment on `HashMismatch` says it covers *both* "another rotation already won the race" *and*
  "the runner is no longer active/was revoked between authentication and this write" — the CAS
  predicate can't distinguish which. Reusing `revoked_error()` for both would collapse two
  genuinely different causes into one imprecise code, exactly the anti-pattern Task 1 just
  eliminated elsewhere in this file. `conflict.json`'s own message — "The resource changed before
  this operation committed" — is a near-literal match for what actually happened. A caller that
  retries (rotating or not) re-authenticates fresh and will itself surface `runner_revoked` on a
  later call if that was the real cause, so no information is permanently lost by not
  disambiguating synchronously. The CAS runs *before* the (still direct-`pool()`, no dedicated B2
  method) capability-field write, so a rejected rotation never touches `runner_version`/`name`/
  `labels`/`capability_snapshot` either.
- **New test:** `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten`. Two
  concurrent `/refresh` (`rotate_credential: true`) requests, both authenticated against the same
  still-valid `RUNNER_CREDENTIAL` bearer, race for the same runner. Asserts exactly one `200` and
  one `409 conflict` (`retryable: true`), that the stored `credential_hash` is exactly the
  winner's (never the loser's, which was never persisted and is never returned to a caller to
  mistakenly treat as live), and that the pre-race credential no longer authenticates afterward
  either way.
  - **Load-bearing proof, non-determinism found and fixed in the test itself:** a first version of
    this test used bare `tokio::join!` on the two request futures and failed ~1 run in 3 (11/40,
    then 4/40 after a partial fix) — not because no CAS occurred, but because the two requests
    never actually raced: one request's entire rotation (including its own `authenticate` call)
    completed before the other's `authenticate` was even polled once, so the loser saw an
    already-rotated hash and failed with a plain `unauthorized`, not `conflict` — the same
    poll-order non-determinism B2's own equivalent test found (see
    docs/agent-handoffs/part-iii/III-B2.md, "Three-review fix-up... Defect 2"). Fixed the same
    way B2 fixed theirs: fully await opening a manual `BEGIN IMMEDIATE` no-op write against the
    runner row *before* either rotation request is even constructed (this test's `sqlite::memory:`
    pool is shared-cache, so a plain `SELECT` — including `authenticate`'s credential lookup —
    blocks behind any pending write to the same table), `tokio::spawn` both rotation requests as
    independently-scheduled tasks, give the runtime a real `tokio::time::sleep` (not merely
    cooperative `yield_now`, which was tried first and was not sufficient) to let both spawned
    tasks reach their own blocked read, then release the hold. Verified directly: **0/100
    failures** across two stress runs (40 + 60) after this fix, versus **15/15 failures** when the
    fix in `runner_protocol.rs` itself was temporarily reverted to unconditionally treat
    `HashMismatch` as success (simulating the pre-fix unconditional-overwrite behavior) — proving
    both that the test is reliable and that it is load-bearing, not a tautology. The revert was
    made and restored only in this card's own owned file, confirmed byte-identical afterward via
    `diff` against a backup taken before the probe.

### Task 3 — independent-verifier cleanups

- **State-gate asymmetry (fixed).** `submit_artifacts` rejected only `succeeded|failed|cancelled`,
  so `lost` and `needs_operator` attempts — states that exist precisely to mean "stop trusting
  this runner's reports" (TODO.md III.1.1) — could still accept artifact writes despite an
  unexpired lease. Changed to the whitelist form, aligned exactly with `create_decision`'s
  existing gate (`running | waiting_decision`), and additionally split the expired-lease check
  (`stale_lease`) from the ineligible-state check (`conflict`) the same way `create_decision`
  already does, rather than collapsing both into `stale_lease` as before. **New test:**
  `submit_artifacts_rejects_lost_and_needs_operator_states_and_writes_nothing` — drives one
  attempt to `lost` via a proven pre-spawn-stopped recovery observation and a second to
  `needs_operator` via a `process_running` observation (neither attempt's lease expires; the fake
  clock never advances), then asserts both `POST .../artifacts` calls return `409 conflict` and
  write zero rows to `execution_artifacts`.
- **`base_revision` `unwrap_or_default()` (fixed).** `claim`'s response construction silently
  turned a missing/unreadable `request_snapshot.repository.base_revision` — a required immutable
  field per TODO.md III.1.2 — into `""`, a structural zero standing in for unknown (rule 7).
  Replaced with `.ok_or_else(|| internal_error(...))?`. Justification for `internal_error` over a
  4xx: B2's own snapshot-hardening amendment already validates the full request snapshot shape
  before a request can be enqueued (quarantining anything incomplete as `needs_operator` rather
  than leasing it), so reaching `claim`'s response-construction step with an absent/malformed
  `base_revision` indicates a persistence-layer contract violation this card's own request
  validation had no part in, not a client input error.
- **Vestigial `available_capacity` in `claim` (dropped, not cross-checked).** `claim` type-checked
  `available_capacity` via `as_i64(...)？` and then discarded the parsed value entirely;
  `claim_execution_idempotent_with_snapshot` does not accept a capacity argument at all, unlike
  `heartbeat_batch`, which cross-checks reported capacity against actual active reservations
  atomically and can return `Conflict`. Decision: **dropped the validation line**, not
  cross-checked. Reasoning: (1) validating a value that is never used implies an enforcement that
  does not exist — worse than not checking it at all; (2) a real cross-check would have to run
  either inside `claim_execution_idempotent_with_snapshot`'s own atomic transaction (out of this
  card's scope; `tack-db` is B2-owned) or as a separate, non-atomic read of
  `agent_runners.available_capacity`, which would reintroduce, at the handler layer, exactly the
  unfenced read-then-decide race B2's `BEGIN IMMEDIATE` fixes were written to close throughout
  this same file. Left a comment at the (now-removed) call site explaining this. No test needed —
  this is a pure code-quality removal with no observable wire behavior change (the field's value
  never affected any response before or after).
- **Dead-code `Limits` fields (fixed at the root).** Nine of the 27 `Limits` fields
  (`environment_entries_max`, `environment_name_bytes_max`, `environment_value_bytes_max`,
  `heartbeat_grace_seconds`, `event_batch_bytes_max`, `decision_answer_bytes_max`,
  `request_timeout_seconds_max`, `retention_event_days_default`,
  `retention_artifact_days_default`) are read only by
  `limits_constants_match_frozen_fixture_exactly`, never by any request-time check in this file —
  confirmed by grepping every field name for non-test call sites. Each genuinely has no code path
  in this card: the `environment_*`/`request_timeout_seconds_max`/`retention_*` fields bound
  enqueue-time or background-sweep concerns owned by other cards; `decision_answer_bytes_max`
  bounds a decision-resolution endpoint that doesn't exist in this wave (known limitation,
  unchanged); `heartbeat_grace_seconds` has no fixture response field to echo
  (`enrollment.response.json` confirmed); `event_batch_bytes_max` is numerically identical to
  `json_body_bytes_max`, already documented as the reason no separate check exists. Rather than
  inventing enforcement for fields that structurally don't belong to this card, or leaving C5's
  blanket module-level `#[allow(dead_code)]` in place, each of the nine fields now carries its own
  precisely-scoped `#[allow(dead_code)]` plus a comment explaining why, directly on the `Limits`
  struct in `runner_protocol.rs` — the file that actually owns the dead fields. **Verified this
  makes C5's blanket allow redundant**: temporarily removed
  `#[allow(dead_code)]` from `pub mod runner_protocol;` in `crates/tack-api/src/handlers.rs` (a
  C5-owned file, not otherwise edited) and ran `cargo clippy -p tack-api --all-targets -- -D
  warnings` — clean. Restored `handlers.rs` immediately after and confirmed it is byte-identical
  to before via `diff`. **C5 may now remove that `#[allow(dead_code)]`** — this is not a request,
  it is a passed-along confirmation the fix makes it obsolete; C5 owns the file so C5 performs the
  removal.

### Task 4 (optional) — typed embedded capabilities

**Adopted.** `validate_capability_payload` now parses the `capabilities` payload against B1's
`tack_orch::execution::EmbeddedCapabilitySnapshot` (via `serde_json::from_value`) instead of
hand-walking the raw `Value` field by field. Reasoning: this does more than shrink the function —
it resolves originally-open ambiguity #1 above. The old hand-rolled validator treated everything
except `concurrency`/`labels` (i.e. `harnesses`, `features`, `limits`, `reported_at`) as an
opaque, unchecked blob specifically because strict `RunnerCapabilities` parsing rejects
`refresh.request.json`'s sparse `features: {}`/`harnesses: []` example.
`EmbeddedCapabilitySnapshot` is B1's purpose-built answer to that exact mismatch: `features` stays
opaque `serde_json::Value` (so the sparse refresh fixture still parses) while `harnesses`,
`limits`, `reported_at`, and `concurrency` are now genuinely typed and structurally validated — a
strict increase in validation coverage, not a wash. This card kept exactly the two things B1's own
brief said a shape type has no business encoding: the pre-parse `capabilities_bytes_max` byte cap
(still checked *before* the typed parse, so an oversized payload never pays JSON-deserialization
cost) and the `available <= total` business rule.

- Both `enrollment.request.json` and `refresh.request.json` already satisfy
  `EmbeddedCapabilitySnapshot` (confirmed via B1's own
  `embedded_capability_snapshot_parses_full_and_sparse_fixtures` test and directly via a new
  `validate_capability_payload_accepts_refresh_fixtures_sparse_shape` unit test in this file, which
  loads the real fixture file and validates its `capabilities` object). This card's own
  `full_capabilities()` test helper already sent the full shape (`reported_at`/`limits` included),
  so no existing integration test needed changing.
- **New unit tests:** `validate_capability_payload_rejects_incomplete_shape` (a payload missing
  `limits` is now `invalid_request` — strictly more validation than the pre-amendment hand-rolled
  check, which ignored `limits`/`reported_at`/`harnesses` entirely) and
  `validate_capability_payload_accepts_refresh_fixtures_sparse_shape` (described above). The
  pre-existing `validate_capability_payload_rejects_available_over_total_and_oversized_labels` was
  updated to build complete `EmbeddedCapabilitySnapshot`-shaped payloads (adding `reported_at`/
  `limits`/`harnesses`/`features`) rather than the previously-partial ad hoc JSON, since an
  incomplete shape is now rejected before either business rule it tests can even run.

### Verification

- `cargo build -p tack-api` — compiles.
- `cargo test -p tack-api --test c2_handlers_test` — **22 passed, 0 failed** (the 16 from the
  prior amendment, unchanged, plus 6 new: the two Task 1 fingerprint-mismatch tests, the Task 2
  rotation-race test, the Task 3 state-gate test, and the two Task 4 capability-shape tests).
  The Task 2 rotation-race test was additionally stress-run 100 times (40 + 60 across two separate
  runs) with **0 failures**, and proven load-bearing (15/15 failures against a temporarily
  reverted fix, restored byte-identical afterward).
- `cargo test -p tack-api --test c1_handlers_test` — **7 passed**; `cargo test -p tack-api --test
  c5_integration_test` — **6 passed**; `cargo test -p tack-api --test runner_vertical_slice` — **7
  passed** — none of C1/C5's consumers of this card's shape were disturbed.
- `cargo test --workspace` — **905 tests passed across every crate and doctest run, 0 failed**
  (60 result blocks, summed from the full run's output).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean (only this card's three Rust files needed a
  `rustfmt --edition 2024` pass; no other file was reformatted, per rule 10).
- `git diff --check` — clean (verified via the same temporary-stage-then-reset technique the
  original handoff used for untracked files).
- `git status --porcelain` — only this card's owned files (two untracked new modules, the
  untracked test file, this handoff) plus other cards' pre-existing, untouched concurrent work.

- Failure/adversarial case proved: see each task section above — the fingerprint-mismatch tests
  (Task 1), the rotation-race test with its load-bearing revert-and-restore proof (Task 2), and
  the lost/needs_operator state-gate test (Task 3) each directly reproduce the exact defect their
  task describes and prove the fix closes it, not merely that the happy path still works.
- Schema/API/contract change requested from another owner: none new. This amendment closes
  ambiguities 1, 2, and 4 from the original handoff above (each now marked resolved in place);
  ambiguities 3 and 5 remain open and unaffected — B2's amendment did not add a fingerprint/replay
  table for decision/artifact inserts (ambiguity 3), and accept/start still have no named fixture
  pair (ambiguity 5).
- Known limitations or `not_measured` fields: none beyond what the original handoff and its
  "Retryability-authority amendment" already list. `rotate_runner_credential`'s own known
  limitation (no replay/idempotency table for an exact same-hash retry, per B2's handoff) is
  inherited unchanged — this amendment did not ask B2 to add one, since neither review defect it
  fixes described that gap.
- Secrets/logging review: `RunnerPrincipal.credential_hash` carries only a SHA-256 hash (never a
  raw credential) and is never logged; no new log line was added by this amendment.
  `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten` and the fingerprint tests
  assert on hashes/response bodies only.
- Safe merge order and likely conflicts: merge after the accepted B1/B2 baseline this amendment
  depends on (B2's split enums and `rotate_runner_credential`; B1's
  `EmbeddedCapabilitySnapshot`/`StableErrorCode::retryable`), before C5. No line in this amendment
  touches a file any other card owns; C5's own follow-up (dropping its now-redundant
  `#[allow(dead_code)]`) is entirely inside C5's own file and not performed here.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Amendment: runner-v1 body limit respects the operator-configured global limit (integrator-authorized cross-card fix)

- Base SHA / branch / final SHA: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`, after C5's own handoff (`docs/agent-handoffs/part-iii/III-C5.md`)
  landed uncommitted in the same shared checkout. Per this task's instructions the work is left
  uncommitted, so no final SHA is recorded here.
- Authorization: performed directly by the Wave 2 integrator, who explicitly authorized this
  specific cross-card edit and instructed it be recorded here and in `III-C5.md` so a later
  ownership audit does not read it as a III.2 rule 2/rule 5 violation ("Stay inside `Owns`" /
  "No router/OpenAPI/generated-schema edits outside C5"). Rule 2's default path — record the needed
  `router.rs` change here and let C5 perform it — was not followed because both sides
  (`runner_protocol.rs`'s new parameter and `router.rs`'s one-line call-site update to supply it)
  are a single, indivisible defect; the integrator judged splitting the fix across two separate
  card cycles a needless handoff round-trip for a two-line wiring change, and is the party III.2
  itself names as the one authority who may override rule 5 for exactly this kind of cross-card
  chokepoint edit. `crates/tack-api/src/router.rs` is therefore touched by this amendment as a
  **minimal, recorded exception** to this card's ownership boundary — one call site and its
  surrounding doc comment, nothing else in the file — described in full in `III-C5.md`'s own,
  parallel amendment section (that file's owner-of-record for `router.rs`).
- Files changed (owned): `crates/tack-api/src/handlers/runner_protocol.rs`,
  `crates/tack-api/tests/c2_handlers_test.rs` (its one `runner_protocol::routes(state)` call site,
  required by the signature change below, plus a comment explaining the `usize::MAX` argument), and
  this file. Files changed (integrator-authorized exception, not owned): `crates/tack-api/src/router.rs`
  (one call site + doc comment inside `runner_protocol_routes`).
- Contract fixtures consumed: none new. `docs/contracts/runner-v1/limits.json`'s frozen constants
  are unaffected — this fix concerns only the axum-layer transport ceiling that sits *around* the
  protocol's own `json_body_bytes_max` (1 MiB), never the wire shape itself.
- Behavior implemented — the defect and the fix: an independent adversarial verifier proved that
  `router.rs`'s claim in `III-C5.md` ("runner-v1 inherits every layer on `outer` — CORS, CSP/security
  headers, **the global body limit**, tracing") was false for the body limit specifically. This
  router's own `.layer(DefaultBodyLimit::max(RUNNER_ROUTER_BODY_LIMIT_BYTES))` (a hardcoded 4 MiB)
  is *more specific* than the plain `DefaultBodyLimit::max(state.config.max_body_size_bytes)`
  layered on `outer` in `router.rs`, and axum always applies whichever `DefaultBodyLimit` layer sits
  closest to the handler — so the outer, operator-configurable layer never actually bound a
  runner-v1 request. Live proof (the verifier's reproduction, confirmed again here before the fix):
  with the global limit configured to 2 KiB, a 512 KiB body to `/api/runner/v1/claim` was read in
  full and the handler executed (a bad credential returned `401`, not `413`); a 5 MiB body did get a
  genuine `413`, confirming the 4 MiB ceiling itself was real, just not tunable downward. Net effect:
  an operator hardening a deployment could never tighten the runner-v1 surface below 4 MiB, no
  matter what `TACK_MAX_BODY_SIZE` was set to.

  The fix keeps `RUNNER_ROUTER_BODY_LIMIT_BYTES` (4 MiB) as the fixed protocol ceiling — it is a
  deliberate upper bound (per the constant's own pre-existing doc comment: headroom above
  `limits.json`'s 1 MiB `json_body_bytes_max` so this file's own `payload_too_large` envelope is
  never preempted), not something this fix removes. A new private helper,
  `effective_body_limit_bytes(configured_max_body_size_bytes: usize) -> usize`, computes
  `configured_max_body_size_bytes.min(RUNNER_ROUTER_BODY_LIMIT_BYTES)`. `routes` gained a second
  parameter, `configured_max_body_size_bytes: usize`; its own `DefaultBodyLimit` layer now uses
  `effective_body_limit_bytes(configured_max_body_size_bytes)` instead of the bare constant. **The
  exact precedence rule: `min(operator-configured global limit, 4 MiB protocol ceiling)`** — a
  tighter operator config always wins; a looser or default config (`router.rs`'s own default is
  2 MiB, already below the ceiling) can never loosen the runner-v1 surface past 4 MiB in either
  direction.

  `c2_handlers_test.rs`'s own `setup()` builds this card's *local* router (not the production one),
  so it has no real operator `AppConfig` to thread through; its one call site now passes
  `runner_protocol::routes(state, usize::MAX)` — `usize::MAX` means "no additional global-config
  restriction" for that isolated suite, so `effective_body_limit_bytes` still collapses to the
  unchanged 4 MiB ceiling and every pre-existing assertion in that file is unaffected. The real
  `min`-of-configured-and-ceiling precedence is proved separately against the *production* router in
  `III-C5.md`'s parallel amendment (`c5_integration_test.rs`), not here — only that router is ever
  wired with a real, operator-configured `AppConfig`.

- Tests added and exact commands/results:
  - New unit test in `runner_protocol.rs`'s own `#[cfg(test)]` module:
    `effective_body_limit_bytes_is_the_lesser_of_configured_and_ceiling` — asserts a below-ceiling
    configured value wins, an above-ceiling value (and `usize::MAX`) collapses to the ceiling, and
    the exact ceiling value is idempotent (no fencepost error).
  - `cargo test -p tack-api --test c2_handlers_test` — **23 passed, 0 failed** (the 22 pre-existing,
    byte-for-byte unchanged, plus the one new test above).
  - `cargo test -p tack-api` — **361 passed, 0 failed** across every suite in the crate.
  - `cargo test --workspace` — **913 passed, 0 failed**, 2 doctests intentionally ignored
    (pre-existing, `tack-orch`).
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (`rustfmt --edition 2024` run directly against exactly the
    files this amendment touches — `runner_protocol.rs`, `c2_handlers_test.rs`,
    `c5_integration_test.rs`, `router.rs` — no unowned file mechanically reformatted, rule 10).
  - `git diff --check` — clean.
  - **Pre-existing flake noted, not introduced by this amendment:** `logs_never_contain_raw_credentials_only_ids`
    in this same test file failed intermittently (roughly 1 run in 10) under cargo's default
    parallel test execution, with the panic message "runner_id should be logged for observability".
    Root-caused to a `tracing::subscriber::set_default` thread-local guard racing against
    `tracing`'s process-global callsite-interest cache when multiple `#[tokio::test]` functions run
    concurrently on different OS threads — a pre-existing hazard in that test's own design, touching
    no line this amendment changed. Confirmed: **0 failures across 6 runs** with
    `RUST_TEST_THREADS=1` (eliminates the cross-thread race entirely), and reproducible with the
    same frequency independent of anything in this amendment (the mechanism is unrelated to body
    limits, routing, or config). Not fixed here — out of this amendment's scope and not owned by the
    defect it closes.

- Failure/adversarial case proved: see `III-C5.md`'s parallel amendment for the full drive-through-
  the-production-router test. Summarized: **load-bearing revert-and-restore**, performed here in
  this card's own owned file — temporarily changed `effective_body_limit_bytes` to unconditionally
  return `RUNNER_ROUTER_BODY_LIMIT_BYTES` (simulating the exact pre-fix behavior) and reran C5's new
  production-router test: it failed exactly as the live defect predicts — the 512 KiB body under a
  2 KiB configured limit was claimed successfully (`200`, a real lease issued), not rejected. The
  fix was then restored and confirmed **byte-identical** via `diff` against a backup of
  `runner_protocol.rs` taken before the probe.

- Schema/API/contract change requested from another owner: none — no frozen fixture, contract, or
  another card's owned file was asked to change.

- Known limitations or `not_measured` fields: none beyond what the original handoff and its prior
  amendments already list, items 1–5 in the original handoff remain open and are unaffected by this
  amendment.

- Secrets/logging review: unchanged — no log line, credential, request body, or query string is
  touched by this amendment; the only new logic is a pure `usize` arithmetic helper.

- Safe merge order and likely conflicts: merge together with `III-C5.md`'s parallel amendment — the
  two describe one indivisible fix, split only for file-ownership bookkeeping (this card's
  `runner_protocol.rs` change and C5's `router.rs` call site must land as a single unit; either
  alone fails to compile — `router.rs` calls `runner_protocol::routes` with the new arity). No other
  dependency beyond the already-accepted C1/C2/C5 baseline already present in this shared checkout.

- Checklist: no unowned files beyond the integrator-authorized, explicitly recorded exception
  (`crates/tack-api/src/router.rs`, one call site); no live secret; no panic stub; no blind retry.

## Amendment: fix the `logs_never_contain_raw_credentials_only_ids` flake noted (not fixed) above

- Base SHA / branch / final SHA: HEAD `ea7b764` ("docs: accept Wave 2 at f931fc0") on
  `plan/harness-agnostic-agent-fleet`, working tree clean at start. Per this task's instructions
  the work is left uncommitted; no final SHA is recorded here.
- Files changed (must equal ownership list): `crates/tack-api/tests/c2_handlers_test.rs` only.
  `crates/tack-api/src/handlers/runner_protocol.rs` and `runner_auth.rs` were read but not
  touched — the fix did not require a production change. This file.
- Contract fixtures consumed: none — this amendment is test-infrastructure-only and changes no
  wire behavior, error code, or handler logic.

- Behavior implemented — root cause, confirmed by reading `tracing-core` 0.1.36's source directly
  (`dispatcher.rs`, `callsite.rs`) rather than from memory: `tracing`'s callsite-interest cache is
  **process-global**, not per-subscriber. The first time any thread in a process hits a given
  `event!` callsite, the `Interest` returned by whatever subscriber is active *on that thread at
  that exact moment* — the thread-local override if one is set, else the process's global default,
  else a hardcoded no-op `Dispatch::none()` if neither was ever installed — is cached forever
  against that callsite (`DefaultCallsite::interest`/`set_interest`, an `AtomicU8`). A later
  `tracing::subscriber::set_default` call on a *different* thread does trigger a fresh
  `Dispatch::new()` and a corresponding rebuild of every callsite touched *so far*, but any
  callsite a concurrently-running sibling test's thread happens to touch **after** that rebuild
  snapshot — under no subscriber at all, since eleven of the thirteen tests in this file never
  install one — gets `Interest::never()` cached against it permanently (no third `Dispatch` is ever
  constructed later to trigger another rebuild). Once cached `never`, the callsite macro
  short-circuits before ever calling the capturing subscriber's `event()` method again, for any
  thread, for the rest of the process — silently. This matches the observed failure exactly: every
  captured failure's panic message was `runner_id should be logged for observability:` with a
  **completely empty** `log_text`, i.e. the one relevant callsite
  (`tracing::info!(runner_id = %runner_id, "runner enrolled")`, `runner_protocol.rs:511`, also hit
  by `full_runner_protocol_lifecycle_enroll_through_completion`'s own `POST /enroll`) never fired
  for this test's subscriber at all, not merely a formatting/timing artifact.

  Fix: install one real, permanent, **process-global** default subscriber
  (`tracing::subscriber::set_global_default`), exactly once for the whole test binary, guarded by
  `std::sync::Once` and invoked from `setup()` — which literally every test in this file calls
  first, before any HTTP request reaches production handler code, so the `Once` (which blocks
  concurrent callers until the winner finishes) guarantees installation completes before any test
  can race ahead of it. Once a global default exists, `tracing`'s "no one is listening" fallback
  (`Dispatch::none()`) never occurs again on *any* thread for the rest of the process — `EXISTS`
  flips permanently and `get_default()`/`get_global()` always resolve to this one subscriber — so
  no sibling test can ever poison a callsite's cached interest to `never` again. Per-test isolation
  of *which* test's output is actually kept is then a plain thread-local flag (`LOG_CAPTURE`,
  `RefCell<Option<Arc<Mutex<Vec<u8>>>>>`) that the shared writer (`GlobalLogWriter`) consults on
  every write, appending only when the calling thread has an active `CaptureGuard`; the other
  twelve tests' output goes nowhere. A plain thread-local (not `tokio::task_local!`) is correct
  here specifically because every `#[tokio::test]` in this file runs its entire async body on one
  dedicated OS thread under the default current-thread runtime flavor — confirmed by reading the
  file: no test uses `#[tokio::test(flavor = "multi_thread")]` — so there is no work-stealing that
  could move a test's execution to a different thread mid-`.await` and strand the flag.

  The subscriber itself is unchanged in substance from before this amendment: still a real
  `tracing_subscriber::fmt()` builder at `Level::DEBUG`, still formatting genuine `tracing` events
  emitted by the actual `runner_protocol::enroll`/`authenticate` production code paths through
  `app.oneshot(...)` — nothing mocked, nothing hand-constructed. Only the writer plumbing and the
  scope of `set_default` vs. `set_global_default` changed. Considered and rejected:
  - **Serializing this test against the others via a shared mutex.** Rejected: the race is not
    confined to a small, enumerable set of callsites — any `tracing::info!/warn!/debug!` callsite
    inside `runner_protocol.rs`/`runner_auth.rs` shared with any of the other twelve tests
    (currently: enroll, refresh's success path, plus whatever future handler code adds) is
    exposed, and a mutex only serializes this test's *own* execution — it does nothing to stop a
    still-concurrently-running sibling test's thread from touching a callsite for the first time
    under no subscriber while this test's mutex-protected section runs.
  - **A `tracing` layer/filter registered before any callsite is hit**, without also making it
    global, does not close the race by itself — a *thread-local* layer has exactly the same
    single-thread blind spot as `set_default` did; the fix has to be that a subscriber is
    *globally* present, not merely present early on one thread.
  - **A capture mechanism independent of thread-local dispatch entirely** (e.g. hand-rolling a
    non-`tracing` logging shim) was rejected outright per this task's own instruction: the
    assertion must keep capturing real `tracing` output from the production handlers, and
    `tracing_subscriber`/`tracing-appender` (both already present as ordinary, non-dev, workspace
    dependencies of `tack-api` — confirmed via `Cargo.toml`/`Cargo.lock` before writing any code)
    are the only sanctioned way to do that; no new dev-dependency was needed or added.

- Other tests in this file checked for the same hazard: only
  `logs_never_contain_raw_credentials_only_ids` ever installed a subscriber
  (`grep -n "tracing_subscriber\|set_default\|set_global_default" crates/tack-api/tests/c2_handlers_test.rs`
  before this amendment matched exactly the three lines inside this one test/its helper struct).
  None of the other twelve `#[tokio::test]` functions, nor the small `#[test]` unit tests inside
  the `runner_protocol::tests`/`runner_auth::tests` submodules loaded via `#[path]`, read or depend
  on `tracing` output in any way — they assert on HTTP status codes and SQL row state only. No
  other latent instance of this hazard exists in this file.

- Tests added and exact commands/results:
  - Baseline (before this amendment), measured in a disposable worktree at this same HEAD
    (`git worktree add --detach <scratch>/c2-flake-baseline HEAD`, removed after measurement):
    `cargo test -p tack-api --test c2_handlers_test` run **40 times** under cargo's default
    parallel execution — **36 passed / 4 failed (10% failure rate)**, matching the "roughly 1 in
    10" figure in this task's brief and the prior amendment's note above. Every captured failure's
    panic was the same: `runner_id should be logged for observability:` with `log_text` empty.
  - After this amendment, in the main checkout: `cargo test -p tack-api --test c2_handlers_test`
    run **90 times total** across two separate loops (50 + 40) under cargo's default parallel
    execution — **90 passed / 0 failed**.
  - `cargo test -p tack-api --test c2_handlers_test` (single run) — **23 passed, 0 failed**, same
    count as before this amendment; no test added or removed, no assertion weakened.
  - `cargo test -p tack-api` — **361 passed, 0 failed**.
  - `cargo test --workspace` — **913 passed, 0 failed**, 2 doctests intentionally ignored
    (pre-existing, `tack-orch`), matching this task's expected total exactly.
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean (one `missing_const_for_thread_local`
    lint fixed along the way: the `thread_local!` initializer is `const { RefCell::new(None) }`).
  - `cargo fmt --all -- --check` — clean.
  - `git diff --check` — clean.
  - `git status --porcelain` — only `crates/tack-api/tests/c2_handlers_test.rs` and this file.

- Failure/adversarial case proved: the before/after loop counts above **are** the adversarial
  proof this task required — a flake fix that changes behavior only in the code path that mattered
  (interest-cache installation timing), verified by reproducing the original failure mode first
  (same panic message, same empty-buffer signature) on the unmodified code, then showing it is
  genuinely eliminated rather than merely narrowed (0/90 vs. a 10% baseline, using more than 3x the
  minimum 30 runs this task asked for).

- Schema/API/contract change requested from another owner: none.

- Known limitations or `not_measured` fields: none introduced. The global subscriber's
  `max_level_hint` (`DEBUG`) becomes the process-wide max `tracing` level for the remainder of the
  test binary's run once installed (a documented `tracing-core` behavior for any global default,
  not specific to this design) — harmless here since no test in this binary depends on `trace!`-level
  output existing or not.

- Secrets/logging review: unchanged in substance — the assertion still proves, against real
  production log output, that a runner_id is logged and that the raw enrollment token, the issued
  runner credential, and a bogus bearer credential are never logged (Part III rule 12). Nothing
  about *what* is asserted, or *which* production log line is exercised, changed — only *how*
  reliably the test's harness observes it.

- Safe merge order and likely conflicts: standalone; touches only this card's own test file and
  handoff. No interaction with any other card's in-flight work — the fix is confined to test
  infrastructure inside a file no other card owns.

- Checklist: no unowned files; no live secret; no panic stub; no blind retry; security assertion
  strengthened in reliability, not weakened or made vacuous.
