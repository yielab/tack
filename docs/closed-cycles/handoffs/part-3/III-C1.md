# III-C1 handoff

- Integrated base / branch / C1 commits: `422b751` / `agent/iii-c1-operator-api` / `a44dca1`, `cef3324`, `5109931`.
- Files changed (must equal ownership list): new `crates/tack-api/src/handlers/{executions,runner_admin}.rs`, focused `crates/tack-api/tests/c1_handlers_test.rs`, and this handoff.
- Contract fixtures consumed: `protocol.json`, `lifecycle-transitions.json`, cancellation and stable-error envelopes. IDs remain opaque and all responses use the v1 error envelope shape; no fixture changed.
- Behavior implemented: card-local operator routers for execution create/list/get/cancellation/requeue plus fleet, pending-runner enrollment, enrollment-token revocation, runner revocation, agent-profile, and model-profile management. Creation requires the C5-provided authenticated-principal header, scopes idempotency to that principal, constructs a complete canonical B1 `ExecutionRequestSnapshot`, and uses a fixed per-request B2 clock so `created_at` is exactly the persisted instant. New creates validate the Tack item and exact-runner availability; exact durable replays remain available after mutable runner status changes. Cancellation records a request only. Requeue delegates to B2's typed, authoritative-recovery-gated `operator_requeue_needs_operator` API with the client recovery key, principal actor, and a reason fingerprint.
- Tests added and exact commands/results: `cargo test -p tack-api --test c1_handlers_test` — 4 passed, covering principal-scoped exact replay/conflict, cancellation-as-requested, authoritative needs-operator requeue, and one-time/revocable hash-only enrollment tokens with response redaction. `cargo clippy -p tack-api --test c1_handlers_test -- -D warnings`, `cargo fmt --all --check`, and `git diff --check` passed.
- Failure/adversarial case proved: changed execution inputs under the same principal/key conflict rather than silently merging, while a second principal gets a distinct request; cancellation leaves request state queued; a manually-set `needs_operator` attempt cannot requeue without a B2 authoritative recovery audit; exact replay survives a later runner revocation; the raw enrollment token is returned only at issue time, never stored, and cannot be redeemed after use or revocation.
- Schema/API/contract change requested from another owner: none. This card now consumes B1's frozen snapshot domain and B2's accepted enrollment, replay, recovery, and revocation APIs directly.
- Known limitations or `not_measured` fields: these handlers are intentionally unregistered; C5 owns global router/OpenAPI wiring and must inject the authenticated principal into `x-tack-principal` before mounting these card-local routes. List/detail DTOs are card-local and will need C5's generated-schema integration. Fleet membership is represented by B2's table but awaits a dedicated membership route in the C5 integration shape.
- Secrets/logging review: the raw enrollment token exists only in the issuance response; only its SHA-256 hash reaches B2 storage. No list/detail/revoke response exposes token hashes, raw tokens, or runner credentials, and handlers do not log them.
- Safe merge order and likely conflicts: merge after B1/B2, before C5. C5 should register the two `routes(OperatorExecutionState)` routers, provide an authenticated principal scope, and own any OpenAPI/generated-schema reconciliation. No global router, handler registry, migration, repository, or fixture file was touched.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Error-envelope conformance amendment

- Base / contract / final: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`, on top of the already-uncommitted B1
  retryability-authority amendment to `crates/tack-orch/src/execution/types.rs`.
  Per this task's instructions the work is left uncommitted, so no final SHA
  is recorded here.
- Owned changes: `crates/tack-api/src/handlers/executions.rs`,
  `crates/tack-api/src/handlers/runner_admin.rs`,
  `crates/tack-api/tests/c1_handlers_test.rs`, and this handoff. No router,
  OpenAPI, migration, `tack-orch`, or `tack-db` file changed.
- **The original claim above — "all responses use the v1 error envelope
  shape" — was wrong, and this section does not rewrite it away.** An
  independent adversarial verifier found that both files' local `error()`
  helpers hand-rolled `serde_json::json!({"error":{...,"retryable":false,
  "details":{}}})` and hardcoded `retryable:false` and `details:{}` for
  **every** error code. `docs/contracts/runner-v1/errors/*.json` — the frozen
  authority — classifies `conflict`, `internal_error`, `rate_limited`, and
  `artifact_checksum_mismatch` as `retryable:true`, and gives structured
  `details` to `not_found` (`{"resource":...}`), `idempotency_conflict`
  (`{"idempotency_key":...}`), `invalid_transition` (`{"from":...,"to":...}`),
  `runner_revoked` (`{"runner_id":...}`), and `invalid_request`
  (`{"field":...}`). C1 emitted `internal_error` pervasively and `conflict`
  in `create_fleet`/`create_profile`/`create_model_profile`, so a conformant
  client — per `docs/contracts/runner-v1/README.md`, "clients branch only on
  `code` and `retryable`" — would never have retried a transient
  `internal_error` or an optimistic-concurrency `conflict` from these
  endpoints. No test in the original `c1_handlers_test.rs` asserted on
  `retryable` or `details` at all, which is why it shipped and passed
  adversarial review the first time.
- What is actually shipped now: both `error()` helpers were replaced with
  B1's `tack_orch::execution::ProtocolErrorEnvelope::new(code, message,
  request_id, details)`, which derives `retryable` from
  `StableErrorCode::retryable()` internally, so a caller cannot supply an
  inconsistent value. Every call site was audited individually, moved from a
  bare `&str` code to the typed `StableErrorCode` enum, and given the
  contract-shaped `details` for its code (field name for `invalid_request`,
  resource name for `not_found`, the idempotency/recovery key for
  `idempotency_conflict`, the runner id for `runner_revoked`, a `from`/`to`
  pair for `invalid_transition`, `{}` for `conflict`/`internal_error`/
  `unauthorized`). `grep -n "retryable"` across both files now matches only
  the doc comment on `error()`; no literal `retryable` value remains in C1's
  source.
- Call sites whose *code* (not just `retryable`/`details`) was found simply
  wrong for the situation, fixed and not left for the caller to infer:
  - `create_execution`'s five-branch parse of a stored idempotency-replay
    snapshot (`serde_json::from_str`, `.as_object()`, missing
    `request_id`/`created_at`, bad `created_at` RFC 3339) reported
    `idempotency_conflict`. A row *we* persisted failing to parse back is a
    server-side data-integrity fault, not evidence the caller reused an
    idempotency key with a different payload (that distinct, genuinely
    client-caused case is `EnqueueResult::Conflict`, a few lines below, and
    correctly kept `idempotency_conflict`). Changed to `internal_error`.
  - `request_cancellation`: `request_execution_cancellation` returns a single
    `bool` that is `false` both when the request id is unknown and when it
    exists but already reached a terminal state. The handler reported
    `not_found` for both. Added one `SELECT state ... WHERE id = ?` to
    disambiguate: genuinely missing stays `not_found`
    (`{"resource":"execution_request"}`); already-terminal is now `conflict`
    (`{}`), matching the `conflict` fixture's own description, "The resource
    changed before this operation committed."
  - `revoke_enrollment_token`: `revoke_enrollment_token_by_id` returns
    `false` both when the token id is unknown and when it exists but was
    already consumed (its `UPDATE ... WHERE consumed_at IS NULL` simply
    matches zero rows). The handler reported `not_found` for both. Added an
    existence check to split them the same way: missing stays `not_found`
    (`{"resource":"enrollment_token"}`); already-consumed is now `conflict`.
  - `create_pending_runner`'s call into
    `create_pending_runner_and_issue_token` mapped *every* repository error —
    the intentional `sqlx::Error::Protocol` the repo returns for out-of-range
    capacity/expiry, a `UNIQUE(agent_runners.name)` violation, or any other
    database fault — to `invalid_request`. Only the first is actually a
    client input problem. Split on the error value:
    `sqlx::Error::Protocol(_)` stays `invalid_request`
    (`{"field":"total_capacity"}`); a unique-constraint violation
    (`sqlx::Error::as_database_error().is_unique_violation()`) is now
    `conflict`; anything else is `internal_error`. The same
    any-error-is-a-name-conflict imprecision existed in `create_fleet`,
    `create_profile`, and `create_model_profile` (each treated *any* insert
    failure as "name already exists"); all three now use the same
    unique-violation check before falling back to `internal_error`, via a
    shared `is_unique_violation` helper in `runner_admin.rs`.
  - `requeue_needs_operator`'s `InvalidTransition | NotFound` arm shipped
    `invalid_transition` with `details:{}` — a code the fixture itself
    requires `{"from":...,"to":...}` for. The repo layer does not report
    which attempt state blocked the requeue, so a `SELECT state FROM
    execution_attempts ... ORDER BY attempt_number DESC LIMIT 1` was added to
    populate `from` (falling back to the string `"unknown"` if no attempt
    row exists at all), with `to` fixed at `"queued"`, the state a successful
    requeue would have set. The *code* here was already correct; only
    `details` was previously empty.
- Tests added and exact commands/results:
  `cargo test -p tack-api --test c1_handlers_test` — 7 passed (the original 4
  plus 3 new): `duplicate_fleet_name_conflict_is_retryable_with_empty_details`
  drives a real duplicate `POST /runner-fleets` and asserts
  `error.retryable == true` and `error.details == {}` on the actual response
  body — the retryable-code proof.
  `missing_execution_not_found_is_not_retryable_with_resource_detail` drives
  `GET /executions/{unknown-id}` and asserts `error.retryable == false` and
  `error.details == {"resource":"execution_request"}` — the
  structured-details, non-retryable proof.
  `changed_payload_idempotency_conflict_is_not_retryable_with_key_detail`
  drives the same changed-payload-same-idempotency-key scenario the original
  suite already covered for `code`, and additionally asserts
  `error.retryable == false` and
  `error.details == {"idempotency_key":"same-key"}`. All three drive the
  handler through `tower::ServiceExt::oneshot` and inspect the real
  `serde_json::Value` body, not the envelope constructor in isolation.
  `cargo clippy -p tack-api --test c1_handlers_test -- -D warnings` — clean.
  `rustfmt --check --edition 2024` on all three owned Rust files — clean.
  `git diff --check` — clean.
  `cargo test -p tack-orch --lib` — 103 passed (B1/B2 unaffected).
  `cargo test -p tack-orch --test runner_contract` — 18 passed (B4's
  byte-pinned fixture harness unaffected).
- Failure/adversarial case proved: flipping any `error()` call site back to a
  literal `retryable` value is no longer possible without also deleting the
  `StableErrorCode`-typed `code` parameter, since `ProtocolErrorEnvelope::new`
  derives `retryable` from `code` and does not accept a `retryable` argument
  at all. The three new tests fail if a future edit reverts to hand-rolled
  `json!` envelopes, drops a code's structured `details`, or restores a
  hardcoded `false`.
- Schema/API/contract change requested from another owner: none. This
  amendment consumes B1's `ProtocolErrorEnvelope`/`StableErrorCode` exactly as
  landed; no fixture, schema, or another card's file was touched. C2 was
  explicitly left untouched (`runner_protocol.rs`, `runner_protocol/`,
  `c2_handlers_test.rs` are another agent's in-progress, uncommitted work).
- Known limitations or `not_measured` fields: the `invalid_transition` `from`
  value for the requeue-rejection path is best-effort — it reads the latest
  attempt's *current* state, not necessarily the exact state the repository
  layer's internal decision was based on a moment earlier, since the repo
  does not surface that state itself. The `invalid_request` detail for
  `create_execution`'s whole-snapshot deserialization failure carries the
  underlying `serde_json::Error`'s display text under `"field"` rather than a
  single clean field name, because that failure can originate from any
  nested field, and `serde_json::Error` does not expose a structured field
  path.
- Secrets/logging review: unchanged from the original handoff; no new log or
  response surface was added, and the enrollment-token and runner-credential
  redaction guarantees are untouched. The new diagnostic queries
  (`execution_requests.state`, `agent_enrollment_tokens` existence,
  `execution_attempts.state`) read only non-secret lifecycle state.
- Safe merge order and likely conflicts: unchanged — merge after B1/B2,
  before C5. This amendment depends on B1's `ProtocolErrorEnvelope::new` and
  `StableErrorCode::retryable` landing first.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
