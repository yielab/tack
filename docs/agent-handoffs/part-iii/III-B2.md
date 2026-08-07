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
