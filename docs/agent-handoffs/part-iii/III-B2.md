# III-B2 handoff

- Base SHA / branch / final SHA: `f042085` / `agent/iii-b2-execution-store` /
  reported with the final commit (a commit cannot contain its own content-addressed SHA).
- Files changed (must equal ownership list): `crates/tack-db/src/migrations.rs`,
  `crates/tack-db/src/repo/execution.rs`, `crates/tack-db/src/repo.rs` (one module export),
  `crates/tack-db/tests/execution_repo_test.rs`, this handoff.
- Contract fixtures consumed: `runner-v1` lifecycle, limits, protocol, claim, event-batch,
  completion, capability and stable-error fixtures. IDs remain opaque text; no fixture changed.
- Behavior implemented: additive migrations 039–048 for the ten neutral tables; transactional
  idempotent enqueue; eligible exact-runner/fleet claim with request state, fence, attempt and
  capacity in one transaction; fake-clock heartbeat/expiry classification; fenced event batches,
  completion replay, cancellation request, artifact/decision metadata writes; queue/history
  indexes.
- Tests added and exact commands/results: `cargo test -p tack-db --test execution_repo_test`
  (6 passed); `cargo fmt -p tack-db -- --check`; `cargo test -p tack-db` (121 passed,
  1 performance test intentionally ignored); `cargo clippy -p tack-db -- -D warnings`.
- Failure/adversarial case proved: orphan item FK is rejected; changed idempotency payload is a
  conflict; stale fence cannot heartbeat; event/completion replay is idempotent; terminal state
  cannot reopen; expiry permits only `lost`/`needs_operator`, never automatic requeue; query
  plans select the queue/timeline indexes.
- Schema/API/contract change requested from another owner: C1/C2 must translate their frozen
  protocol DTOs into these opaque repository inputs. No route/OpenAPI/fixture edits made.
- Known limitations or `not_measured` fields: profiles/snapshots and actual execution are
  persisted opaque JSON at this layer; validation/serialization authority remains B1/A0/C5.
- Secrets/logging review: runner credential is accepted only as an already-derived hash; no
  credential, prompt, metadata or environment value is logged by this module.
- Safe merge order and likely conflicts: merge after A3 (already in base); no other Wave 1 card
  may modify migrations or `repo.rs`. B1/B3/B4 are otherwise independent.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Wave 2 persistence amendment

- Base / branch / final SHA: `f14019b` / `agent/iii-b2-wave2-amend` / reported with commit.
- Added migrations 049–051: runner credential metadata plus hash-only, single-use,
  expiry/revocation-aware enrollment tokens; durable `(runner_id, claim_request_id)` replay
  records; and a recovery-audit table reserved for the C4 recovery transaction.
- Added atomic token issue/redeem/revoke and the strong
  `claim_execution_idempotent_with_snapshot` API. Its one transaction includes replay lookup,
  capacity reservation, request transition, attempt insert and replay-key insert; replay returns
  the original lease and stored immutable request fields. The existing lease-only method remains
  as a compatibility wrapper.
- Hardened report paths: event/artifact/decision/completion require an unexpired lease; terminal
  completion restores runner capacity in its successful terminal transaction only, so replay
  cannot restore it twice.
- Focused verification: `cargo test -p tack-db --test execution_repo_test` (8 passed),
  `cargo fmt -p tack-db`, `cargo clippy -p tack-db --test execution_repo_test -- -D warnings`.
- Remaining blocker, deliberately not guessed: the requested all-in-one heartbeat batch and C4
  recovery transition/audit APIs still need their dedicated repository methods and fault tests.
  The tables are present but C2/C4 must not treat their absence as a completed recovery protocol.

### Reporting/recovery follow-up

- Added migration 052 and atomic `heartbeat_batch`: runner capacity/heartbeat, every fenced
  lease renewal and cancellation flags commit with a durable runner+heartbeat-id replay record;
  a stale lease rolls the whole transaction back.
- Added `recover_attempt` with a keyed durable audit. `SafePreSpawnRequeue` is accepted only
  for expired attempts with no `started_at`; all ambiguous/post-spawn recovery must request
  `NeedsOperator`. Each successful recovery normalizes capacity and writes exactly one audit.
- New fake-clock test proves heartbeat replay and recovery audit idempotency (focused DB suite:
  9 passed; Clippy and format passed).
- Still required before C2/C4 treats the repository as feature-complete: structured event-batch
  accepted-vs-duplicate result and durable cancellation-observation replay APIs. Existing event
  checkpoint and completion-id persistence remain safe compatibility paths, but do not expose
  the richer protocol result yet.

### Runner-admin follow-up

- Added transactionally-created pending runner plus token issuance, runner/token-id-scoped token
  metadata and revocation, and runner revocation that revokes outstanding tokens. Redemption now
  requires `pending_enrollment` and writes the full identity/capability exchange atomically.
- Added idempotent audited operator requeue for `needs_operator`; its key includes recovery key,
  actor and reason fingerprint and it never increments capacity.
- Focused `execution_repo_test` passed 10 tests; format, focused Clippy and diff checks passed.

### Final hardening follow-up

- Migration 053 stores the canonical immutable runner-v1 request snapshot. Claim fresh/replay
  reads the same JSON fallibly and fails closed for the legacy `{}` default rather than silently
  reconstructing a lossy payload.
- Enrollment redemption now atomically consumes its hash-only token and persists runner name,
  version, labels, capacity, capabilities, protocol, credential hash and expiry; revocation is
  never cleared by enrollment.
- Added structured event checkpoint results and durable cancellation observation replay, with
  focused fake-clock repository coverage. Final focused command: `cargo test -p tack-db --test
  execution_repo_test` (10 passed), plus `cargo fmt -p tack-db` and focused Clippy.

### Event replay correction

- Migration 054 persists each event-batch replay keyed by `(attempt_id, checkpoint)` with an
  authenticated request fingerprint and the authoritative accepted/duplicate response. The
  strong event API now returns `Applied`, `ReplayConflict`, or `Stale`; runner/fence/lease
  validation happens before a replay record may be used.
- Payload JSON is recursively canonicalized before fingerprinting, so object-key order does not
  turn an equivalent retry into a conflict. Malformed replay rows fail closed with a database
  protocol error. A legacy pre-054 attempt whose checkpoint already advanced but has no replay
  row is a `ReplayConflict`; no response is fabricated from incomplete historical state.
- Focused coverage proves canonical retries, changed-payload conflict with no extra write, and
  foreign-fence stale rejection.

### Completion replay correction

- Migration 055's completion replay record is now authoritative: `Completion` carries the
  required final event checkpoint, and the terminal compare-and-set validates attempt, runner,
  fence, active lease and that checkpoint in one statement. Completion fingerprints bind every
  immutable request field (including runner/fence/checkpoint) and recursively canonicalize the
  structured terminal-reason, execution and usage JSON.
- Authentication precedes replay lookup. A matching authenticated terminal retry returns the
  parsed stored response; malformed stored data fails closed. A stale or foreign runner/fence
  cannot read a response. Pre-055 terminal attempts with no durable response are `Conflict`,
  even if their historical completion ID matches: the repository deliberately does not invent a
  response from incomplete state.
- Focused tests cover response-loss replay, foreign-fence rejection, semantic JSON replay,
  changed fields/checkpoint conflicts, corrupt response handling, exactly-once capped capacity,
  and rollback when persisting the replay record fails. The boolean compatibility completion API
  delegates to the hardened typed path.

### Heartbeat replay correction

- Migration 056 adds the durable canonical request fingerprint to heartbeat replay rows. The
  heartbeat response now preserves its ID, acceptance time, and every renewed
  attempt/fence/expiry/cancellation flag; exact retry returns those original typed fields even
  after the clock has advanced. Same-ID input changes return `Conflict`; malformed stored
  responses fail closed.
- Heartbeat capacity is checked against all active unexpired reservations, not merely the
  submitted leases. A runner cannot report free capacity while it still holds a lease, and the
  persisted value is constrained to `total_capacity - active_reservations`.
- Pre-056 replay rows have the empty migration default fingerprint and therefore return
  `Conflict`; their incomplete historical response is not treated as authoritative. Focused
  coverage includes false-free capacity, multi-lease canonical replay, stale lease, response
  loss after clock advance, changed-input no-write, and replay-insert rollback.
- Heartbeats now also bind a frozen client `sent_at` timestamp into that fingerprint. Existing
  pre-`sent_at` fingerprints therefore conflict rather than replaying. Capacity reservations
  count every unresolved active attempt even after its lease expires: expiry alone cannot make a
  slot available; a recovery/terminal transition must release it.
- Each heartbeat active-attempt entry now also freezes and persists its reported `state`,
  `journal_state`, and `last_event_checkpoint`; any same-ID mutation of one of those fields is a
  `Conflict` rather than a replay.

### Cancellation observation correction

- Migration 057 gives cancellation replay records a canonical fingerprint and authoritative
  response. `CancellationObservationInput` binds runner/fence, cancellation ID, `observed_at`,
  and canonical JSON `details` plus `observation`; the response records attempt, ID, state and
  committed timestamp. Authentication occurs before replay lookup, so a foreign/stale fence
  cannot obtain a replay response, while an authenticated terminal retry can.
- The typed outcome separates `Cancelled` and `Replayed` from `Conflict`, `Stale`,
  `AlreadyTerminal`, and `Ambiguous`; terminal or unsupported/missing-cancellation states are
  never reported as a successful cancellation. Capacity restoration is capped at total capacity.
- Pre-057 cancellation rows have empty fingerprint/response defaults and return `Conflict` on a
  retry rather than fabricating a response. Focused coverage includes response-loss replay after
  time advance, semantic JSON replay, changed-input no-write, foreign fence, corrupt response,
  capped capacity, transaction rollback, and terminal/ambiguous classifications.
- Cancellation `observation` is intentionally constrained to the exact JSON string
  `"process_stopped"`; any other JSON value fails before opening a transaction or changing state.

### Recovery observation v1

- Migration 058 makes recovery audits durable replay records with a fingerprint and response.
  `recover_attempt` now accepts a runner/attempt/fence/key observation, authenticates an active,
  non-revoked owning runner before lookup, and returns typed applied/replayed/conflict/stale
  results with the original committed timestamp.
- Only `process_stopped` with server `started_at` absent plus validated details
  `journal_state="prepared"` and `process_observed=false` can safely mark the attempt `lost`
  and requeue its request. Started, spawned, running, process-observed, or ambiguous evidence is
  conservatively `needs_operator`. Terminal attempts create an auditable/replayable
  `already_terminal` disposition with no lifecycle or capacity mutation.
- Pre-058 audit records have the empty migration-default fingerprint/response and return
  `Conflict`; their historical audit is not fabricated into a recovery response. Focused tests
  cover every disposition, semantic replay and changed input, foreign/revoked access, capped
  release, and rollback of lifecycle, capacity, and audit together.

### Request snapshot hardening

- Enqueue now parses the complete runner-v1 frozen execution request shape before opening a
  transaction: required nested objects and scalar types, RFC3339 `created_at`, selector/profile,
  repository, policy, budget, environment, and metadata fields are validated and cross-checked
  against every normalized request column. The snapshot `created_at` must be the same injected
  clock instant written to the row, and the validated JSON is what is persisted.
- Migration 059 quarantines queued legacy M053-default `{}` snapshots as `needs_operator` while
  preserving the row and all original data. It never invents a `created_by` or a replacement
  immutable snapshot. Claim continues to fail closed for any incomplete persisted snapshot.
- Focused coverage uses full fixtures and proves removed, malformed, cross-field, and clock
  mismatch snapshots create no row; it upgrades through migrations 049–058 then proves M059
  quarantines the legacy queued record without data loss.
- Exact enqueue retries compare the canonical immutable snapshot stored with the original request
  before the fresh-request clock check. They therefore replay after time advances, while a
  changed `created_at` (or any other frozen field) is a conflict. Fresh inserts still require
  `created_at` to match the injected clock.
- Migration 060 expands quarantine to every nonterminal malformed or partial snapshot, including
  pre-existing three-key cohorts, without altering terminal historical rows. Snapshot validation
  now requires typed `created_by`, object-valued budgets/metadata/profile policies, a string
  repository kind, and exactly one literal value or secret reference for each environment entry.

### Legacy boundary cleanup

- The unsafe public single-lease heartbeat, boolean event append, and expiry-classification
  compatibility paths are removed. C2 and C4 must use `heartbeat_batch`,
  `append_execution_events_result`, and `recover_attempt`; their typed outcomes preserve the
  authenticated fence, replay, and conservative-recovery semantics that the old shortcuts
  could bypass.
- Artifact and decision creation require `lease_expires_at > now`; equality is expired and
  creates no row. Operator requeue additionally requires a durable authoritative
  `needs_operator` recovery audit for that exact attempt, so a manually corrupted lifecycle
  state cannot release or requeue work.
- Capacity restoration remains capped by `total_capacity` and is owned by terminal/recovery
  transitions only. Recovering to `needs_operator` releases one reservation; the subsequent
  operator requeue only changes request state and cannot release capacity again.
