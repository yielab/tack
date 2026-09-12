# III-B1 handoff

- Base SHA / branch / final SHA: `f042085` / `agent/iii-b1-domain` / recorded by the commit containing this handoff.
- Files changed (must equal ownership list): `crates/tack-orch/src/execution/{mod,types,lifecycle,capabilities}.rs`, minimal `crates/tack-orch/src/lib.rs` export, and this handoff.
- Contract fixtures consumed: every file under `docs/contracts/runner-v1/`, including the lifecycle matrix and all stable errors. Fixtures are read only through `include_str!` conformance tests; no fixture was edited.
- Behavior implemented: I/O-free runner v1 execution domain with opaque typed IDs/model IDs, distinct requested-vs-actual model types, request and attempt snapshots, capability support/reason snapshots, usage provenance, protocol-v1 parsing, typed stable errors (including `stale_lease`), and lifecycle validation by requesting actor. Every additive object type preserves unrecognised keys with `serde(flatten)`; opaque model strings are never parsed.
- Tests added and exact commands/results: `cargo test -p tack-orch execution --lib` — 9 passed; this includes every frozen success/error fixture round-trip, exact core snapshot fixture equality, full lifecycle state-pair × actor validation, v1 rejection, opaque model/additive-field preservation, and stable stale-lease code. `cargo test -p tack-orch --lib` — 99 passed. `cargo clippy -p tack-orch --lib -- -D warnings`, `cargo fmt --all --check`, and `git diff --check` passed.
- Failure/adversarial case proved: a terminal attempt cannot reopen and returns `invalid_transition`; an actor not listed in the transition fixture is rejected even if another actor may take that pair; protocol version 2 is rejected; unknown enum values are rejected; `reason: null`, absent nested protocol versions, and additive fields survive fixture round-trip; stale leases retain the stable `stale_lease` code.
- Schema/API/contract change requested from another owner: none. B2 should persist these snapshots as opaque domain values; no schema or dependency change is required by B1.
- Known limitations or `not_measured` fields: this is domain-only and deliberately contains no HTTP, persistence, clocks, credential handling, or runner process code. Capability sub-snapshots embedded in enrollment/refresh inherit the enclosing protocol and intentionally omit `protocol_version`; standalone capability reports preserve it when supplied. Requested/actual harness versions and capability provenance beyond the frozen fields remain opaque strings for later layers to interpret.
- Secrets/logging review: no logs or credentials were added. Credential-like values remain opaque fixture/domain data and are never formatted by errors.
- Safe merge order and likely conflicts: merge before B2/B3/B4. The only shared surface is the B1-owned `tack-orch/src/lib.rs` module export; later cards should consume `tack_orch::execution` without altering legacy orchestration files.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Recovery-observation amendment

- Base / contract / final: this amendment starts at the additive runner-v1
  recovery contract commit `1c189b7`; its final commit records the domain-only
  implementation.
- Owned changes: `crates/tack-orch/src/execution/{types,mod}.rs` and this
  handoff only. No API, database, runner, Cargo, TODO, or fixture file changed.
- Added exported recovery values: opaque `RecoveryKey`; typed
  `RecoveryObservation`, non-secret `RecoveryJournalState` and
  `RecoveryDetails`; server-authoritative `RecoveryDisposition`; and additive
  `RecoveryObservationRequest` / `RecoveryObservationResponse` values. Both
  enclosing values and the details object preserve unknown additive fields.
- Disposition invariants: only `process_stopped` is compatible with the
  necessary public preconditions for `safe_pre_spawn_requeue`; it maps the
  attempt to `lost` and request to `queued`. `needs_operator` maps active
  attempts/requests to `needs_operator`; `already_terminal` has no transition
  and is compatible only with terminal attempts. The safe disposition remains
  necessary-but-not-sufficient: B2 must verify authoritative process and
  `started_at` absence before applying it.
- Verification: the tack-orch test/clippy/fmt/diff commands recorded by the
  amendment commit cover exact request/response fixture round trips, additive
  preservation, and recovery disposition/lifecycle invariants.

## Retryability-authority amendment

- Base / contract / final: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`; per this task's instructions the work is
  left uncommitted, so no final SHA is recorded here.
- Owned changes: `crates/tack-orch/src/execution/types.rs` only. `mod.rs`
  needed no change — `StableErrorCode`, `ProtocolError` and
  `ProtocolErrorEnvelope` were already exported, and the new members are
  inherent methods/tests on those same types, not new types. No API, database,
  runner, Cargo, TODO, or fixture file changed.
- Defect fixed: `retryable` on `ProtocolError` was a bare `bool` with no
  contract-derived source of truth. Card III-C1 hand-rolled its envelope with
  `serde_json::json!` and hardcoded `"retryable":false,"details":{}` for every
  code (`crates/tack-api/src/handlers/executions.rs`,
  `handlers/runner_admin.rs`), so a conformant client would never retry a
  transient `internal_error` or an optimistic-concurrency `conflict`. This is
  a B1 fix per III.1.6 ("Hand-written feature DTOs are not another
  authority"): the classification is contract data and must have exactly one
  owner.
- Added: `StableErrorCode::retryable(self) -> bool`, a `const fn` derived from
  every fixture under `docs/contracts/runner-v1/errors/` — `true` only for
  `conflict`, `internal_error`, `rate_limited`, `artifact_checksum_mismatch`;
  `false` for the remaining eleven codes. Added `ProtocolError::new(code,
  message, request_id, details)` and `ProtocolErrorEnvelope::new(code,
  message, request_id, details)`, both of which set `retryable` from
  `code.retryable()` so a caller cannot supply an inconsistent value. No
  existing field, derive, or serde attribute changed, so the wire shape and
  B4's byte-pinned fixtures are untouched.
- Tests added: `stable_error_code_retryable_matches_every_fixture_and_constructor`
  reads every file in `docs/contracts/runner-v1/errors/` via `include_str!`
  (paired with its filename in the new `ERROR_FIXTURES` table) and asserts,
  per fixture, that `code.retryable()` equals the fixture's `retryable`, and
  that `ProtocolError::new` reconstructed from the fixture's own code/message/
  request_id/details is field-for-field equal to the parsed fixture —
  covering `retryable` and every other field including `additional`.
  `every_error_fixture_file_on_disk_is_in_the_conformance_list` reads the real
  `docs/contracts/runner-v1/errors/` directory at test run time (via
  `CARGO_MANIFEST_DIR`, not `include_str!`, since that macro cannot enumerate
  a directory) and asserts its filename set equals `ERROR_FIXTURES`'s, so a
  fixture added or removed on disk without a matching table entry fails the
  build instead of being silently skipped.
- Exact commands/results: `cargo test -p tack-orch execution --lib` — 13
  passed (the two new tests plus the eleven pre-existing B1 execution tests).
  `cargo test -p tack-orch --lib` — 103 passed. `cargo test -p tack-orch
  --test runner_contract` — 18 passed, unchanged from before this amendment
  (B4's byte-pinned fixture harness was not touched and still passes).
  `cargo clippy -p tack-orch --lib -- -D warnings` — clean. `cargo fmt -p
  tack-orch -- --check` — clean.
- Failure/adversarial case proved: flipping any fixture's `retryable` value,
  or `StableErrorCode::retryable()`'s match arms, fails
  `stable_error_code_retryable_matches_every_fixture_and_constructor` by
  filename; adding a new file under `docs/contracts/runner-v1/errors/` without
  a matching `ERROR_FIXTURES` entry fails
  `every_error_fixture_file_on_disk_is_in_the_conformance_list`.
- Schema/API/contract change requested from another owner: **III-C1 must
  adopt this** — replace its hand-rolled `serde_json::json!` envelope in
  `crates/tack-api/src/handlers/executions.rs` and `handlers/runner_admin.rs`
  with `tack_orch::execution::ProtocolErrorEnvelope::new(code, message,
  request_id, details)` (serialize the result), so `retryable` and future
  codes stay contract-derived instead of hardcoded. **III-C2 should use this
  API from the start** rather than reproduce the same hand-rolled envelope.
  Example call site:

  ```rust
  use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};

  fn error(
      status: StatusCode,
      code: StableErrorCode,
      message: &str,
      request_id: &str,
      details: serde_json::Value,
  ) -> (StatusCode, Json<Value>) {
      let envelope = ProtocolErrorEnvelope::new(code, message, request_id, details);
      (status, Json(serde_json::to_value(envelope).expect("envelope serializes")))
  }
  ```

  `details` must still follow the per-code shape in
  `docs/contracts/runner-v1/README.md` (e.g. `not_found` → `{"resource": ...}`,
  `stale_lease` → `{"attempt_id":..., "current_fencing_token":...}`); only
  `conflict`, `internal_error` and `unauthorized` take `{}`. This constructor
  does not choose `details` for the caller — it only removes `retryable` as a
  place to get it wrong.
- Known limitations or `not_measured` fields: none added by this amendment.
- Secrets/logging review: no logs or credentials touched; `message` and
  `details` remain caller-supplied display/diagnostic data, unchanged from the
  existing `ProtocolError` contract.
- Safe merge order and likely conflicts: additive only inside
  `crates/tack-orch/src/execution/types.rs`; no conflict expected with B2/B3/
  B4 or with C1/C2's in-flight `crates/tack-api/**` work, since neither existing
  field nor derive changed and this amendment does not edit `crates/tack-api/**`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Embedded-capability-snapshot amendment

- Base / contract / final: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`; per this task's instructions the work
  is left uncommitted, so no final SHA is recorded here.
- Owned changes: `crates/tack-orch/src/execution/capabilities.rs`, the
  export line in `crates/tack-orch/src/execution/mod.rs`, and this handoff
  only. No API, database, runner, Cargo, TODO, contract fixture, or test
  file B4 owns was touched.
- Defect fixed: a contract analysis built a throwaway probe crate against
  `tack-orch` and fed it the exact embedded `capabilities` sub-objects from
  `enrollment.request.json` and `refresh.request.json`. Both failed to parse
  as `RunnerCapabilities` — `missing field runner_version` for enrollment,
  `missing field cancel` (i.e. `features` as `FeatureCapabilities`) for
  refresh — because those two fixtures embed a **third, distinct
  capability-shaped wire object**, structurally different from both the
  standalone `capabilities.json` report (`RunnerCapabilities`) and the
  terminal `completion.request.json`'s `actual_execution.capability_snapshot`
  (`FeatureCapabilities`). `runner_version` and `protocol_version` are
  siblings of `capabilities` in the enclosing envelope, never nested inside
  it, so `RunnerCapabilities` (which requires `runner_version`, by design —
  see the original handoff above) cannot parse this shape. This was latent
  because the existing `core_domain_snapshots_match_their_exact_fixture_shapes`
  (this card) and `frozen_domain_fragments_round_trip_exactly` (B4) only
  round-trip the standalone `capabilities.json`; the generic envelope
  fixture test only flattens through `serde_json::Value` and never types the
  nested `capabilities` field for either enrollment or refresh. Card C2 hit
  this first and hand-rolled a JSON workaround
  (`validate_capability_payload` in
  `crates/tack-api/src/handlers/runner_protocol.rs`) rather than a shared
  type existing to catch it.
- Added: `EmbeddedCapabilitySnapshot`, additive and exported alongside the
  other capability types. Field strictness follows directly from the two
  embedding fixtures, not from loosening `RunnerCapabilities`:
  `runner_version` and `protocol_version` have no field at all (both are
  always siblings on the wire, never nested, so there is nothing to
  default); `concurrency` and `labels` stay structurally typed/required,
  matching what `validate_capability_payload` already enforces by hand
  (missing/malformed `concurrency` errors; non-object `labels` or
  non-string label values error); `harnesses` defaults to empty and
  `features` is opaque `serde_json::Value` (not `FeatureCapabilities`) so
  both enrollment's full example (populated harness list, five typed
  support statements) and refresh's sparse one (`"harnesses": []`,
  `"features": {}`) parse unchanged; `reported_at` and `limits` stay
  required, reusing `CapabilityLimits`, since both fixtures shape them
  identically and nothing suggested loosening them. Unrecognised keys
  survive via `serde(flatten)` into `additional`, matching every other
  additive type in this module. `RunnerCapabilities` and
  `FeatureCapabilities` were **not** modified — both remain correctly
  strict where they are actually used (the standalone report and the
  terminal completion snapshot respectively); widening either was
  considered and rejected (see the type's doc comment for the specific
  reasoning: an `Unsupported` default for `FeatureCapabilities` would
  misrepresent an omitted field as a scheduler-safety problem, and an
  optional `runner_version` on `RunnerCapabilities` would silently weaken a
  fixture-tested invariant for every caller to paper over one call site).
- Tests added: `embedded_capability_snapshot_parses_full_and_sparse_fixtures`
  parses `enrollment.request.json["capabilities"]` and
  `refresh.request.json["capabilities"]` via `include_str!` into
  `EmbeddedCapabilitySnapshot`, asserts both first fail to parse as
  `RunnerCapabilities` (proving the gap is real, not assumed), asserts both
  round-trip byte-for-value exactly against the source fixture, asserts
  enrollment's populated `harnesses`/`features` and refresh's empty ones are
  read correctly, and asserts an injected unknown key survives a round trip
  through `additional`.
- Exact commands/results: `cargo test -p tack-orch --lib` — 104 passed (the
  new test plus the 103 pre-existing). `cargo test -p tack-orch --test
  runner_contract` — 18 passed, unchanged (B4's byte-pinned fixture harness
  was not touched and still passes). `cargo clippy -p tack-orch --lib -- -D
  warnings` — clean. `cargo fmt -p tack-orch -- --check` — clean.
- Failure/adversarial case proved: both embedded fixtures are asserted to
  fail `RunnerCapabilities` parsing before being asserted to succeed against
  the new type, so the test would fail loudly if a future edit accidentally
  made `RunnerCapabilities` permissive enough to swallow the embedded shape
  (which would silently erase the distinction this amendment exists to
  preserve).
- Schema/API/contract change requested from another owner: none required.
  Recommended, not required: C2's `validate_capability_payload` in
  `crates/tack-api/src/handlers/runner_protocol.rs` currently hand-rolls
  field-by-field JSON checks (`field(capabilities, "concurrency")`,
  `as_u64`, manual `labels` object/string walks) to get exactly the
  `concurrency`/`labels` shape this type now provides. Swapping in
  `serde_json::from_value::<EmbeddedCapabilitySnapshot>(capabilities.clone())`
  would replace that hand-rolled parsing with one shared, tested type and
  let a bad `concurrency` or non-object `labels` fail via a single `serde`
  error instead of several hand-written checks — but C2 also enforces a
  business rule this type deliberately does not (`available <= total`) and
  a byte-size limit before parsing, so adopting this type would still need
  `validate_capability_payload` to keep those two checks around the typed
  parse. I recommend C2 adopt it (less duplicated shape-checking code, one
  fewer place to keep in sync with the fixtures), but it is optional: C2's
  current checks are already correct for today's fixtures and the type
  didn't exist when C2 was written, so there is no defect forcing the
  change, only a simplification available to it.
- Known limitations or `not_measured` fields: this type is shape-only, like
  every other additive type in this module — it does not enforce
  `available <= total` on `concurrency` or any other cross-field business
  rule; that remains a caller concern (currently C2's).
- Secrets/logging review: no logs or credentials touched; the new type
  carries only capability-shaped fixture/domain data, same as every other
  type in this file.
- Safe merge order and likely conflicts: additive only inside
  `crates/tack-orch/src/execution/capabilities.rs` plus one export line in
  `mod.rs`; no existing field, derive, or serde attribute on
  `RunnerCapabilities` or `FeatureCapabilities` changed, so no conflict is
  expected with B2/B3/B4 or with C1/C2/C3/C5's in-flight
  `crates/tack-api/**` work.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
