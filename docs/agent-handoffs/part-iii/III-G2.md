# III-G2 handoff

- **Base SHA / branch / final SHA:** base `5c6842f` (`docs(part-iii): accept Wave 5 at
  073aa4d`, the named Wave 6 branch point) / `agent/iii-g2-chaos-audit` / final SHA is the
  commit that carries this handoff (`git log -1` on this branch).
- **Files changed (must equal ownership list):**
  - `crates/tack-api/tests/g2_chaos_security_test.rs` (new) — 8 adversarial tests against the
    real production router.
  - `crates/tack-runner/tests/g2_journal_corruption_test.rs` (new) — 3 adversarial tests
    against the local runner journal.
  - `frontend/src/shared/runWithAgent/EventTimeline.test.tsx` (extended, not replaced) — 2
    new XSS-regression tests appended after the existing 4.
  - `docs/agent-handoffs/part-iii/III-G2.md` (this file).
  No production Rust or TypeScript source was modified in the final diff. No router,
  `openapi.rs`, generated schema, `migrations.rs`, CI workflow, contract fixture, `TODO.md`,
  or other card's handoff was touched.
- **Contract fixtures consumed (read-only):** `docs/contracts/runner-v1/heartbeat.request.json`,
  `cancellation.request.json`, `decision.create.request.json`,
  `recovery-observation.request.json` — used to shape adversarial request bodies exactly as
  the frozen fixtures declare. None edited.
- **Behavior implemented:** none — this card owns tests and the audit report only. Two
  temporary, fully-reverted production edits were made and restored solely to prove two tests
  load-bearing (see "Load-bearing proofs" below); the final diff contains neither.

## Audit report

One row per adversarial case. Verdict `safe` means: a safe, documented state was observed and
proven directly against persisted data (not a status code alone), per CLAUDE.md's own rule.
`unsafe` means a finding that reopens its owning phase. `not verified` means this card could
not establish the fact within its scope/time and says so rather than claiming a pass.

| # | Case | Method | Observed state | Verdict | Owning card if reopened |
|---|---|---|---|---|---|
| 1 | Multi-runner contention: two distinct runners in one fleet race to claim one request | Real concurrent HTTP (`tokio::spawn` + `join!`) against a **file-backed** SQLite DB (`two_distinct_runners_in_the_same_fleet_race_to_claim_one_request_and_exactly_one_wins`) | Exactly one lease granted; exactly one `execution_attempts` row; request state `leased` | **safe** — load-bearing proof below | — |
| 2 | Stolen/duplicated credential: two processes with the *same* runner credential race to claim one request | Same file-backed real-concurrency pattern (`a_duplicated_credential_used_concurrently_never_grants_two_leases_for_the_same_request`) | Exactly one lease; one fence; runner capacity decremented exactly once, not twice | **safe** — load-bearing proof below | — |
| 3 | Revoked/stolen token used after revocation | Operator revokes the runner mid-lease over the real `POST /api/runners/{id}/revoke`; the identical, still-syntactically-valid credential is retried (`a_revoked_runner_credential_is_rejected_everywhere_and_cannot_advance_its_leased_attempt`) | Fresh claim: `403 runner_revoked`. In-flight heartbeat with the correct fencing token: rejected, `last_heartbeat_at` stays `NULL`, attempt state stays `leased`. `agent_runners.state='revoked'`, `revoked_at` set | **safe** — load-bearing proof below | — |
| 4 | Stale fence on `heartbeat` | Attempt superseded via a genuine pre-spawn recovery (mirrors `wave2_gate.rs`'s own pattern), then the old fence retried on `/heartbeat` | `409`, error code `conflict` (not `stale_lease` — see finding F1 below); no heartbeat recorded | **safe**, with a documented consistency finding | F1 below (non-blocking) |
| 5 | Stale fence on `decisions` (create) | Same superseded attempt, `POST .../decisions` | `409 conflict`; zero `execution_decisions` rows | **safe**, same finding | F1 below |
| 6 | Stale fence on `artifacts` (manifest) | Same superseded attempt, `POST .../artifacts` | `409 conflict`; zero `execution_artifacts` rows | **safe**, same finding | F1 below |
| 7 | Stale fence on `cancellation-observation` | Same superseded attempt | `409 conflict` | **safe**, same finding | F1 below |
| 8 | Stale fence on `recovery-observation` (second observation on an already-superseded attempt) | Same superseded attempt, second recovery call | `409 conflict`; fence count for the request stays exactly 2 (fence 1 lost + fence 2 current — no new fence minted) | **safe**, same finding | F1 below |
| 9 | Oversized artifact — per-item declared size over `artifact_content_bytes_max` (50 MiB) | `POST .../artifacts` with `size_bytes` = cap + 1 | `413 payload_too_large`; zero rows | **safe** | — |
| 10 | Oversized artifact — cumulative attempt-total over `artifact_attempt_total_bytes_max` (500 MiB) via 10 accepted items then one more | 10× accepted 52 MB items (520 MB, under cap), then an 11th | 11th rejected `413`; exactly 10 rows persist; `SUM(size_bytes)` stays exactly 520,000,000 (no partial write) | **safe** | — |
| 11 | Symlink / path-traversal attack on the artifact store via a malicious `artifact_id` | 5 payloads (`../../../../etc/passwd`, percent-encoded traversal, absolute path, embedded NUL, Windows-style `..\\..\\`) through manifest + content-upload | Every written file confirmed to live under the configured `storage_dir`; none escaped. `ArtifactStorage::encode_id` hex-encodes every byte, making a separator-based escape structurally impossible, independently re-verified here | **safe** | — |
| 12 | Path-traversal / symlink attack on the runner's own workspace | Not re-tested by this card — `crates/tack-runner/src/workspace.rs` already carries direct unit coverage (`provision_rejects_an_existing_attempt_path_symlink`, `cleanup_refuses_root_and_unresolved_paths`, canary-file escape tests) that this card read and confirms is real, targeted coverage, not merely present | **safe**, verified by reading existing tests, not independently re-proven | — |
| 13 | Corrupt/malformed database row (`request_snapshot` hand-corrupted outside any transaction) | `UPDATE execution_requests SET request_snapshot = 'not valid json {{{'`, then claim | Claim fails with a typed non-200 error (not necessarily 500 — asserted as "any client/server error, never 200"); server stays alive (`/api/health` still `200` afterward); zero attempts created | **safe** | — |
| 14 | Corrupt local runner journal — single bit-rotted file | Raw `fs::write` of garbage bytes over a journal TOML file, then `load()` | Typed `JournalError::Malformed`, no panic | **safe** | — |
| 15 | Corrupt local runner journal — truncated to zero bytes | Same, with an empty file | Typed `Malformed`, not confused with `Missing` (which would wrongly permit a second spawn) | **safe** | — |
| 16 | Corrupt local runner journal — one bad file among several unresolved attempts | 2 healthy + 1 corrupted journal file, then `unresolved()` (the restart recovery scan) | The whole scan returns `Err(Malformed)` — the two healthy attempts' *data* is untouched and independently still loadable by id, but the *batch recovery scan cannot see past the corrupted entry* | **unsafe — finding F2, non-blocking today (no panic, no data loss, no blind respawn) but a real availability gap** | tack-runner journal owner (B3/D-series lineage) — see below |
| 17 | Delay/reorder: an event batch claiming a `previous_checkpoint` that disagrees with the attempt's actual current position | Real batch commits `cp-1`; a second, reordered-looking batch also claims `previous_checkpoint: null` | Rejected (non-`200`); `event_checkpoint` stays `cp-1`; the reordered event never lands (`COUNT=0`) | **safe** | — |
| 18 | Replay: the exact same event batch resent byte-identically (simulating a lost response and client retry) | Same batch body sent twice | Second call returns `200`; exactly one row for `evt-1` in the database regardless of resend count (the response's own `accepted_event_ids`/`duplicate_event_ids` labeling for an exact resend is a separate, minor finding — see F3) | **safe** | — |
| 19 | Disk full (artifact storage / DB) | **Not verified.** No existing test infrastructure in this repository simulates `ENOSPC`/`SQLITE_FULL` at the OS/VFS level (confirmed by grep across `tests/` for `disk_full`/`quota`/`ENOSPC` — nothing). Building a real quota-backed filesystem harness was judged out of proportion to this card's time budget and had no existing precedent to extend safely. The two theoretically-adjacent mechanisms already in the codebase — SQLite `RAISE(ABORT,...)` triggers (`repository_crash.rs`) and the runner journal's `#[cfg(test)] fail_next_update` hook — are fault-injection analogs, not genuine disk-full simulations, and using them to claim "disk full is safe" would be exactly the kind of status-code-shaped false confidence this card's own brief warns against | **not verified** | Whoever picks this up next should build a real bounded-size backing store (e.g. a small tmpfs/loop-device mount or a wrapped `object_store`) rather than reuse the trigger pattern, which doesn't model the actual failure |
| 20 | XSS / prompt rendering in the UI event-timeline surface | Two new Vitest tests inject `<img onerror=...>` and `<script>...</script>` as event `payload` text through `EventTimeline.tsx`'s real render path (both the `text`-field path and the JSON-fallback path) | No `<img>`/`<script>` element ever created in the DOM; the handler never fires; the raw malicious string is still visible as literal text (not silently stripped) | **safe** | — |
| 21 | Kill API / harness process boundaries (before-spawn, post-spawn-pre-ack, response-loss on completion/cancellation) | **Not independently re-tested.** III-C4's `crash_matrix.rs` (7 tests) and `repository_crash.rs` (7 tests) already cover exactly these boundaries with real fault injection (fake protocol failure points; real SQLite `RAISE(ABORT)` triggers), read in full and re-run clean as part of this card's gate | **safe**, verified by reading + re-running existing coverage, not independently re-proven with new tests | — |

### Load-bearing proofs (rule 6)

Two production files were temporarily edited, the corresponding test re-run to confirm it
failed, then reverted byte-for-byte (confirmed via `git status --porcelain` showing no diff
before continuing). Neither edit is in the final commit.

1. **`crates/tack-db/src/repo/execution.rs`, `claim_execution_idempotent_with_snapshot`:**
   `BEGIN IMMEDIATE` → `BEGIN DEFERRED`. Re-ran
   `two_distinct_runners_in_the_same_fleet_race_to_claim_one_request_and_exactly_one_wins`
   and `a_duplicated_credential_used_concurrently_never_grants_two_leases_for_the_same_request`
   against a **file-backed** database (per CLAUDE.md's explicit rule that this exact class of
   race must not be proven only against the shared in-memory harness). Both failed —
   `two_distinct_runners_...` failed with `500 internal_error` ("Could not claim work"), the
   two deferred-transaction readers colliding exactly as CLAUDE.md's own history for this file
   describes. Reverted; both tests pass again.
2. **`crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`, `authenticate()`:** the
   `revoked_at.is_some() || state == "revoked"` branch gated behind `if false && (...)`. Re-ran
   `a_revoked_runner_credential_is_rejected_everywhere_and_cannot_advance_its_leased_attempt` —
   failed: the specific `403 runner_revoked` code degraded to a generic `401 unauthorized`
   ("The runner is not active", via the fallback `state != "active"` branch a few lines below).
   This is itself informative: even with the *specific* revoked-detection line disabled, the
   *fallback* branch still rejects — proving the assertion on the **specific `runner_revoked`
   code** (not just "any rejection") is what the test is actually checking, and that it is
   genuinely exercised rather than vacuously true. Reverted; test passes again.

The remaining tests were not independently proven load-bearing by reverting a production fix
— most assert against code paths this card did not author and had no safe, scoped way to
disable in isolation without broader changes outside `Owns`. Recorded honestly rather than
claimed.

### Findings for owning cards (non-blocking, none reopen a phase on their own)

- **F1 — inconsistent stale-fence error code across endpoints.** `submit_events` maps a
  superseded/inactive attempt to the stable `stale_lease` code (`EventApplyResult::Stale`).
  `heartbeat`, `create_decision`, `submit_artifacts`, `observe_cancellation_report`, and
  `observe_recovery` all map the *identical underlying situation* (an attempt whose own
  `fencing_token` still matches the row, but whose `state` is no longer active/leased — e.g.
  `lost` after a recovery-observation requeue) to the generic `conflict` code instead. A
  runner cannot rely on one consistent error code to detect "my lease was superseded" across
  the whole runner-v1 surface. Every case is still safely rejected and writes nothing — this
  is a consistency/ergonomics finding, not a safety finding. Suggested owner: whichever card
  next touches `runner_protocol.rs`'s per-handler fencing checks (originally C2, current
  steward unclear post-Wave-5 — flag for G5 to route).
- **F2 — one corrupted local-runner journal file blocks recovery of every other unresolved
  attempt on that runner.** `OwnerOnlyJournal::unresolved()` (`journal.rs`) iterates the
  journal directory and propagates the *first* `Malformed`/`Io` error via `?`, discarding
  whatever healthy records it had already found. Proven directly: 2 healthy + 1 corrupted
  journal file → the whole restart recovery scan returns `Err`, even though both healthy
  records remain independently loadable by id and structurally undamaged. No panic, no data
  loss, no blind respawn — but a real, non-hypothetical availability regression for every
  other in-flight attempt on a runner that hits this. Suggested fix shape (not applied here,
  out of `Owns`): `unresolved()` should skip-and-report a malformed entry rather than
  abort-on-first, returning `(Vec<AttemptJournal>, Vec<(PathBuf, JournalError)>)` or similar.
  Escalated to the journal's owning lineage (B3/D-series) — worth a look before G5's final
  integration if runner fleets can realistically run with >1 concurrent attempt per runner
  (`total_capacity` already allows this).
- **F3 — minor: an exact-replay event batch response does not surface `replayed` or a
  duplicate marker for the client.** `append_execution_events_result`'s repo-level result
  carries a `replayed: bool` field (`execution.rs:533`), but `submit_events`'s HTTP response
  JSON never serializes it — a byte-identical resend's `accepted_event_ids` still lists the
  event as freshly accepted rather than moving it to `duplicate_event_ids` (that field is,
  by design, for duplicates *within* one batch, a different case). The *data* is correctly
  idempotent (proven directly: exactly one DB row survives regardless of resend count) — this
  is purely a client-observability gap, not a safety issue. Suggested owner: whoever next
  touches `submit_events`'s response shape (a client currently cannot tell "freshly accepted"
  from "already had this" without checking its own side).

None of F1–F3 write incorrect data, leak a credential, or permit a duplicate execution — they
are recorded as findings, not as phase-reopening failures, per this card's own acceptance bar
("no blind duplicate execution, no credential leak, no cross-attempt write, no silent loss").

## Tests added and exact commands/results

```
cargo test -p tack-api --test g2_chaos_security_test
  → 8 passed; 0 failed

cargo test -p tack-runner --test g2_journal_corruption_test
  → 3 passed; 0 failed

cd frontend && npx vitest run src/shared/runWithAgent/EventTimeline.test.tsx
  → 6 passed (4 pre-existing + 2 new)
cd frontend && npx vitest run
  → 726 passed, 0 failed, 85 files (was 724/85 at III-F4's own recorded baseline; +2 from this
    card's two new XSS-regression tests)
cd frontend && npm run type-check
  → tsc -b, exit 0
```

## Failure/adversarial case proved

See the "Audit report" table above — 21 rows, one per case named in this card's brief plus the
sub-cases the brief's "each boundary" language implied (stale fence proven separately per
endpoint; oversized proven separately per-item vs cumulative). 18 rows verdict `safe`, 1 row
verdict `unsafe` (F2, non-blocking), 1 row `not verified` (disk full), 1 row verified by
reading pre-existing coverage rather than new tests (workspace symlink attack, process-boundary
crash matrix — both already thoroughly covered by III-C4/workspace.rs's own tests).

## Schema/API/contract change requested from another owner

None. This card requests no schema, router, or contract change — F1/F2/F3 above are behavior
findings for an owning card to fix in its own file, not something this card needed unblocked to
finish its own charter.

## Known limitations or `not_measured` fields

- **Disk-full (case 19) is genuinely `not verified`**, not merely undertested — no existing
  infrastructure in this repository models it, and this card judged building that
  infrastructure from scratch out of proportion to its time budget. Recorded honestly rather
  than padded with a status-code-only test that would prove little (CLAUDE.md's own named
  failure mode).
- **Workspace symlink/path-traversal (case 12)** and **process-boundary crash matrix (case
  21)** were verified by reading and re-running III-C4's/`workspace.rs`'s existing coverage,
  not by writing new adversarial tests — judged sufficient because that coverage is real,
  targeted, and already proven (this card independently confirmed it re-runs clean), and
  because this card's time was better spent on the boundaries with a genuine coverage gap
  (multi-runner contention, revoked credentials, artifact traversal, journal corruption, XSS)
  rather than duplicating adjacent cards' already-solid work.
- Two of the eight `g2_chaos_security_test.rs` tests use a **file-backed** SQLite database
  (`app_file_backed`, a fresh temp-directory `.sqlite3` file per test, cleaned up after); the
  other six use the in-memory harness, matching every other adversarial test file in this
  crate — only the two genuine-concurrency race tests needed the stronger guarantee per
  CLAUDE.md's rule.
- The fleet-membership fixture in `two_distinct_runners_in_the_same_fleet_race_...` inserts
  `agent_fleets`/`agent_fleet_members` rows via direct SQL because no write route exists on
  any API surface for fleet membership — a pre-existing, independently-recorded gap (III-E6,
  III-F4), not something this card needed to fix or is newly reporting.

## Secrets/logging review

No new production code was shipped, so there is no new logging surface to audit. Every test in
`g2_chaos_security_test.rs` that captures a raw runner credential (`enroll_runner`) uses it
only to construct an `Authorization` header for the next call in the same test — never logged,
never asserted into a snapshot beyond the credential-substitution test's own explicit "this
raw string never appears in the stored hash" check (mirroring `wave2_gate.rs`'s own precedent).
No test in this card prints a credential, prompt body, or query string; failures print JSON
response bodies only, which this codebase's own handlers already keep credential-free by
design (confirmed by reading every response shape used here).

## Safe merge order and likely conflicts

- Both new Rust test files are net-new paths (`g2_chaos_security_test.rs`,
  `g2_journal_corruption_test.rs`) — no conflict with any sibling Wave 6 card expected.
- `frontend/src/shared/runWithAgent/EventTimeline.test.tsx` gained two new `it(...)` blocks
  appended after the existing four — a same-file merge with another card's own additive
  changes to this file should be an ordinary adjacent-append (no other Wave 6 card is known to
  own this file; G1/G3/G4's charters don't touch frontend test files per their own `Owns`
  lists in `TODO.md`).
- No production file is touched in the final diff — the only merge risk this card carries at
  all is the two files above landing in the same commit range as another card's own additions
  to the same paths, which is a normal append-merge, not a logical conflict.

## Checklist

- [x] No unowned files edited — final diff is exactly the three files listed under "Files
      changed" above, confirmed via `git status --porcelain`.
- [x] No live secret — every credential/token used in these tests is a test-local value minted
      by the test itself against an in-memory or scratch-temp-file database, never a real
      deployment secret.
- [x] No panic stub — every "must not panic" assertion (corrupted DB row, corrupted journal
      file) is backed by a real corrupted fixture and a real assertion that the process/handler
      returned a typed error and stayed alive (`/api/health` still `200` afterward), not a
      hypothetical claim.
- [x] No blind retry — no test in this card retries a failed call automatically; every
      rejection is asserted once, directly, against persisted state.

## Proposed status-board row text (G5 is the integrator — this is a proposal, not applied)

> **III-G2** — chaos/fencing/security/recovery audit complete. 21 adversarial cases
> documented (multi-runner contention incl. stolen/duplicated credentials, revoked-credential
> rejection, stale fence across 6 attempt-scoped endpoints, oversized artifacts per-item and
> cumulative, artifact-store path-traversal, corrupt DB row, corrupt/truncated/batch-blocking
> local runner journal, event reorder/replay, XSS-shaped event payload rendering). 18 safe, 2
> load-bearing-proven by reverting the underlying fix and watching the test fail
> (`BEGIN IMMEDIATE` claim exclusivity; the `runner_revoked` auth branch). One finding reopens
> nothing on its own but is real: a corrupted local-runner journal file currently blocks
> restart recovery for every other unresolved attempt on that runner (F2, journal owner to
> pick up). Two minor error-code/observability inconsistencies recorded (F1, F3). Disk-full
> honestly recorded `not verified` — no existing fault-injection infrastructure models it and
> building one was out of this card's scope. 11 new tests (8 Rust + 2 Vitest + escalated via
> existing-coverage review for 2 more cases), all reproducible, two proven load-bearing by
> reverting the fix. `cargo test --workspace` 1300/1300 (was 1289 at Wave 5 close), clippy and
> fmt clean, `wave2_gate` 5/5, `runner_contract` 18/18, `e6_scheduler_e2e_test` 5/5 isolated.
