# III-F1 handoff

- **Base SHA / branch / final SHA:** base `cbdd4a325a89df3f97bd8bc3009f51024df065fb`
  (`cbdd4a3`, tip of `plan/harness-agnostic-agent-fleet` at Wave 5 start — "docs: close
  out Wave 4 with the III-E6 handoff and accepted integration SHA") / branch
  `agent/iii-f1-decisions` / final (implementation) SHA
  `73d98a3487b7f2fa176406449cfffdbb8c0f272e` (`73d98a3`, "feat(api): add operator-scoped
  decision resolution (III-F1)") in an isolated worktree at
  `.claude/worktrees/agent-af46ba5ef1f620033`. This handoff itself lands in a second,
  docs-only commit on top of `73d98a3`; that second commit is the actual branch tip but
  is not itself a claim about behavior, so `73d98a3` is the SHA every claim below was
  verified against.

- **Files changed (must equal ownership list):**
  - `crates/tack-api/src/handlers/decisions.rs` (new, 675 lines) — the entire card:
    repository layer (`resolve_decision_row`, `expire_overdue_decisions`), service/
    validation layer (`validate_answer`, `canonical_json`/`canonical_string`), and HTTP
    handler (`resolve_decision`, `DecisionOperatorState`, `routes()`), plus a small
    `#[cfg(test)]` unit module (fixture pin, canonicalization, header parsing, answer
    validation).
  - `crates/tack-api/tests/f1_decisions_test.rs` (new, 1,150 lines) — 20 focused tests,
    loaded via `#[path]` exactly like `c1_handlers_test.rs`/`c2_handlers_test.rs` load
    their own card-local, deliberately-unregistered handler modules.
  - `docs/agent-handoffs/part-iii/III-F1.md` (this file).
  - **Nothing else.** `git diff --stat` against base is two files, both new,
    +1,825/-0. No router, migration, generated-schema, contract, or other-card file was
    touched. Neither `crates/tack-api/src/handlers.rs` (the module registrar — the
    card brief's "handlers/mod.rs") nor `crates/tack-db/src/repo.rs` (the analogous
    tack-db registrar — the brief's "repo/mod.rs") has a `pub mod decisions;` line
    added anywhere. `cargo build` (the plain workspace build, no test target) does not
    even compile `decisions.rs` — it is reachable only via the `#[path]` include in
    `f1_decisions_test.rs`, the same mechanism C1/C2 used before C5 wired them in.

## Contract fixtures consumed

- `docs/contracts/runner-v1/decision.create.request.json` /
  `decision.create.response.json` / `decision.poll.request.json` /
  `decision.poll.response.json` — read to align vocabulary: `answer` is
  `{option_id, text}`, `resolved_by` is `{kind, subject_id}` with `kind: "operator"` in
  the frozen example. My resolve response reuses both shapes verbatim.
- `docs/contracts/runner-v1/errors/decision-expired.json` — `resolve_decision` returns
  this exact `code`/`message` (`decision_expired` / "The decision expired without a
  valid answer") on a fail-closed expiry.
- `docs/contracts/runner-v1/errors/{not-found,forbidden,invalid-request,
  idempotency-conflict}.json` — `not_found`, `invalid_request`, `idempotency_conflict`
  codes/shapes reused via `tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode}`
  (the same B1-owned type every other handler module builds its errors from).
- `docs/contracts/runner-v1/limits.json` — `decision_answer_bytes_max` (32,768) is
  mirrored as `DECISION_ANSWER_BYTES_MAX` and pinned against the live fixture by
  `decisions::tests::decision_answer_limit_matches_frozen_fixture`.
- `docs/contracts/runner-v1/protocol.json` — read in full; see "Schema/API/contract
  change requested" below for the one real finding it produced.
- **No fixture was edited.** `cargo test -p tack-orch --test runner_contract` passes
  unmodified, 18/18 (confirmed below) — there is no `decision.resolve.*` fixture pair in
  the frozen directory at all (operator-side decision *resolution* is not part of the
  runner-v1 protocol surface the fixtures document; only runner-side create/poll are).

## Behavior implemented

**New module, not wired anywhere.** `crates/tack-api/src/handlers/decisions.rs` defines
`DecisionOperatorState`, `routes(state) -> Router` (one route: `POST
/attempts/{attempt_id}/decisions/{decision_id}/resolve`), and the handler
`resolve_decision`. It is not declared in `handlers.rs` and not merged into `router.rs`
— per the card's own "no router... edits" instruction and rule 2 (a file under "must
not edit" is a hard stop). The module's own doc comment carries the suggested
integration snippet for the Wave 5 integrator, mirroring exactly how `router.rs`'s
`operator_execution_routes()` already merges C1's `executions`/`runner_admin` routers
(merged into `api` *before* `require_token`, with `inject_operator_principal` layered
directly on top).

**Operator-scoped, structurally separate from the runner surface.** The handler reads
exactly one identity signal: the `x-tack-principal` header. It never reads
`Authorization` at all — there is no code path in this file that can authenticate, or
even inspect, a runner bearer credential. Combined with the runner-protocol surface
(`handlers/runner_protocol.rs`, C2's card, untouched by me) never having a resolve
endpoint at all, this is the enforcement for "runner may raise/read its attempt's
decision but never resolve it": not an exemption entry, a structurally separate route
family with no shared auth path, matching how `CLAUDE.md` already describes every other
operator/runner pair in this codebase.

**Single `BEGIN IMMEDIATE` read-then-write transaction** (`resolve_decision_row`):
SELECT the exact `(attempt_id, decision_id)` row, branch on its `state`:

- `"expired"` → return the already-recorded fail-closed denial, no write.
- `"resolved"` → canonicalize and compare the submitted answer to the stored one.
  Identical → idempotent replay (200, `replayed: true`, same `resolved_at`, no write).
  Different → `idempotency_conflict` (409), no write.
- `"pending"` and `expires_at` is set and has passed → **fail-closed**: CAS-UPDATE to
  `state='expired', answer=NULL, resolved_by={"kind":"system","subject_id":"expiry"}`,
  commit, then still return `decision_expired` to the caller — the operator's own
  answer (even a syntactically valid, option-matching "allow") is never honored once
  expired. Proven directly: `expiry_denies_records_audit_and_never_marks_the_item_done_
  even_against_a_valid_allow_answer` submits `{"option_id":"allow_once"}` (present in
  the decision's own `options`) against an already-overdue decision and asserts the 409,
  the `expired` row state, the `NULL` answer, and the untouched item status.
- `"pending"`, not overdue → validate `answer.option_id` against the decision's own
  recorded `options` (skipped when `options` is empty — a freeform decision), CAS-UPDATE
  to `state='resolved'`, commit, return 200.
- any other stored `state` string → `UnknownState`, mapped to `internal_error()`, never
  silently folded into another outcome (defensive; no code path in this repo or C2's
  ever writes anything but `pending`/`resolved`/`expired` today).

**Bulk fail-closed sweep, exposed but not self-scheduled.** `expire_overdue_decisions`
does the same CAS transition across every overdue `pending` row in one `BEGIN IMMEDIATE`
UPDATE. This card does not call it from anywhere or spin up a background task — see
"Schema/API/contract change requested" for why that's III-F5's job, not mine.

**No item-status write, anywhere, full stop.** `execution_requests.status_map_policy_id`
(migration 044) has zero interpreter anywhere in the codebase — confirmed by grep across
every crate; it is threaded verbatim through CLI args → request snapshot → DB column and
never read back to decide anything. There is no policy schema to honor "optional status
mapping only after commit through the workflow engine" against. Rather than invent an
uncontracted mapping format (rule 13: stop on contract ambiguity), this module
implements the instruction as a structural guarantee instead: no function in
`decisions.rs` ever touches `items.status`, proven negatively by
`expiry_denies_records_audit_and_never_marks_the_item_done_...` and
`expire_overdue_decisions_bulk_sweep_denies_only_overdue_pending_rows`, both of which
snapshot `items.status` before and assert it is byte-identical after.

**Idempotency without a new replay table.** I evaluated the `execution_*_replays`
convention (`execution_claim_replays`, `execution_heartbeat_replays`,
`execution_cancellation_replays`, `execution_event_batch_replays`,
`execution_completion_replays`, migrations 049–055) before writing any resolve logic.
Those tables exist because their operations' *responses* can legitimately need to differ
from a live re-read of the row after some *other*, unrelated operation has since mutated
it (e.g. a completion replay must return the exact original response even if the event
checkpoint advanced afterward). A resolved (or expired) decision row is different: once
`state` leaves `pending`, nothing else in this codebase — not this module, not C2's
`create_decision`/`poll_decisions` — ever writes to that row again. A live re-read after
a failed CAS therefore gives a genuinely correct idempotent replay with no frozen
snapshot needed. I did not add a `execution_decision_resolve_replays` table (I could not
have — `migrations.rs` is out of my scope and B2/A3-successor-only per rule 4 anyway).

## Tests added and exact commands/results

`cargo test -p tack-api --test f1_decisions_test -- --test-threads=4` — **20 passed, 0
failed** (4 pure unit tests inside `decisions::tests`, 16 HTTP-level integration tests):

1. `resolve_a_pending_decision_succeeds_and_matches_operator_answer` — happy path.
2. `resolving_twice_with_the_same_answer_is_idempotent_and_does_not_rewrite` — asserts
   `updated_at`/`resolved_at` are byte-identical across two calls (clock advanced 30s
   between them), not just that both return 200.
3. `resolving_with_a_different_answer_after_resolution_is_idempotency_conflict_and_
   does_not_overwrite` — 409, and the *first* answer is still what's stored.
4. `cross_attempt_decision_id_is_not_found_and_writes_nothing` — a decision that exists
   under attempt B is resolved-against via attempt A's path; 404, and B's row (state,
   `resolved_by`, `answer`) is asserted untouched.
5. `unknown_decision_under_a_real_attempt_is_not_found`.
6. `missing_operator_principal_is_denied_and_writes_nothing`.
7. `self_resolution_via_a_valid_runner_bearer_credential_is_denied_and_writes_nothing` —
   presents a *cryptographically real*, currently-enrolled runner's raw bearer
   credential (SHA-256-hashed exactly as `agent_runners.credential_hash` is populated in
   production) as `Authorization: Bearer ...`, with no `x-tack-principal`. Still 401,
   still untouched — proves the credential grants literally nothing here, not just that
   an arbitrary string fails.
8. `expiry_denies_records_audit_and_never_marks_the_item_done_even_against_a_valid_
   allow_answer` — see "Behavior implemented".
9. `expire_overdue_decisions_bulk_sweep_denies_only_overdue_pending_rows` — 2 overdue +
   1 future + 1 no-expiry decision; asserts exactly the 2 overdue ones flip.
10. `restart_preserves_a_pending_decision_and_it_remains_resolvable` — builds a Router
    from `decisions::routes(...)`, drops it, builds a *second, independent* Router/state
    from the same underlying `Repository`/pool (simulating a process restart — no
    in-memory handler state could survive), and resolves successfully through the new
    instance; a second untouched decision is also proven to have survived.
11. `invalid_answer_shapes_are_rejected_and_write_nothing` — missing/non-object/empty
    `option_id`/non-string `text`, four sub-cases in one test.
12. `answer_option_id_must_match_one_of_the_decisions_declared_options`.
13. `freeform_decision_with_no_declared_options_accepts_any_non_empty_option_id`.
14. `answer_exceeding_the_frozen_byte_limit_is_rejected_as_payload_too_large` — a 40KB
    `text` field against the 32,768-byte frozen limit.
15. `logs_never_contain_the_raw_answer_text_or_prompt_only_ids` — captures real
    `tracing` output (same global-subscriber-plus-thread-local-capture technique as
    `c2_handlers_test.rs`'s `logs_never_contain_raw_credentials_only_ids`, duplicated
    here rather than shared since these are independent test binaries) around a
    successful resolve carrying a unique sentinel string in `answer.text`, and asserts
    the sentinel and the decision's `prompt` text never appear in the log, while
    `attempt_id`/`decision_id` do.
16. `concurrent_conflicting_resolves_serialize_to_exactly_one_winner` — file-backed
    SQLite (not `:memory:`), modeled directly on
    `execution_repo_test.rs`'s `artifact_and_decision_cannot_land_against_concurrently_
    terminal_attempt`: a manually-held `BEGIN IMMEDIATE` transaction forces two
    differently-answered `resolve_decision_row` calls to queue behind it, then releases
    after 150ms via `tokio::join!`. Asserts neither call errors, exactly one is
    `Resolved{replayed:false}`, the other is `IdempotencyConflict`, and the DB holds
    exactly one final answer.

`cargo build` — clean (this card's module compiles into nothing; confirmed the
production binary is unaffected). `cargo test --workspace` — **1154 passed, 0 failed, 6
ignored** (Wave 4's own accepted baseline: 1134 passed, 6 ignored, per III-E6's handoff.
1134 + 20 = 1154 exactly — this card added 20 new tests and changed no other test's
pass/fail count; confirmed with two independent clean full-workspace runs). One
intermediate full-workspace run showed 5 failures in `e6_scheduler_e2e_test.rs`
(unrelated, pre-existing, not touched by this card); re-run in isolation with
`cargo test -p tack-cli --test e6_scheduler_e2e_test -- --test-threads=1` (the exact
invocation III-E6's own handoff documents as required, since that suite spawns real
`tack serve` subprocesses on real ports) passed 5/5 — a known environmental flake under
full-workspace parallel execution, not a regression from this card. `cargo clippy
--workspace --all-targets -- -D warnings` — clean (one `collapsible_if` fixed in
`decisions.rs`, no other lint anywhere). `cargo fmt --check` on this card's two files
only — clean; no other file was formatted. `cargo test -p tack-api --test wave2_gate` —
**5 passed, 0 failed**, unmodified. `cargo test -p tack-orch --test runner_contract` —
**18 passed, 0 failed**, unmodified (no fixture touched).

## Failure/adversarial case proved

Per the card's instruction, every "denied"/"writes nothing"/"never marks done" claim
below was proven load-bearing by temporarily reverting the guard in
`crates/tack-api/src/handlers/decisions.rs`, re-running the specific test, observing it
fail, then restoring the original code (confirmed via `git diff` showing no residual
change and a final clean `cargo test -p tack-api --test f1_decisions_test` — 20/20 —
plus `cargo clippy`/`cargo fmt --check` clean, both run *after* every revert/restore
cycle below):

1. **`BEGIN IMMEDIATE` is load-bearing.** Changed `resolve_decision_row`'s
   `pool.begin_with("BEGIN IMMEDIATE")` to `pool.begin_with("BEGIN")` (deferred).
   `concurrent_conflicting_resolves_serialize_to_exactly_one_winner` (file-backed DB)
   failed deterministically, twice in a row:
   `panicked at ...: resolve A must not error under BEGIN IMMEDIATE: Database(SqliteError
   { code: 5, message: "database is locked" })`. This is the exact class of failure
   CLAUDE.md's "ten sites in `repo/execution.rs`" note describes (a deferred
   read-then-write transaction failing under concurrent contention), now proven for this
   eleventh, out-of-`repo/execution.rs` site too.
2. **The idempotent-replay guard is load-bearing, twice.** Replaced the `"resolved"`
   branch's canonical-comparison-then-no-write logic with an unconditional rewrite
   (bypassing the CAS/comparison entirely). Both dependent tests failed:
   `resolving_twice_with_the_same_answer_is_idempotent_and_does_not_rewrite` failed on
   `assert_eq!(body2["replayed"], true)` (`left: Bool(false)`), and
   `resolving_with_a_different_answer_after_resolution_is_idempotency_conflict_and_does_
   not_overwrite` failed on `assert_eq!(status2, StatusCode::CONFLICT)` (`left: 200`).
3. **Fail-closed expiry is load-bearing.** Forced `is_overdue` to always be `false`
   (`let is_overdue = false && expires_at...`). `expiry_denies_records_audit_and_never_
   marks_the_item_done_...` failed on `assert_eq!(status, StatusCode::CONFLICT)` (`left:
   200`) — the late "allow" answer was honored instead of denied. (Confirmed this
   change does *not* touch the separate bulk-sweep path:
   `expire_overdue_decisions_bulk_sweep_denies_only_overdue_pending_rows` still passed
   during this same reversion, correctly proving the two code paths are independent, as
   designed.)
4. **Cross-attempt scoping (`WHERE attempt_id = ? AND decision_id = ?`) is
   load-bearing.** Dropped the `attempt_id` predicate from the SELECT (matching on
   `decision_id` alone). `cross_attempt_decision_id_is_not_found_and_writes_nothing`
   failed on `assert_eq!(status, StatusCode::NOT_FOUND)` (`left: 200`) — attempt A
   successfully resolved attempt B's decision.

Every reversion above was applied to `crates/tack-api/src/handlers/decisions.rs` only,
one at a time, confirmed to fail its target test (and only that test — I re-ran the full
`f1_decisions_test.rs` suite after restoring each one to confirm no other test's outcome
had been silently relying on the same bug), then restored. `git diff` shows no trace of
any of the four temporary edits in the committed tree.

## Schema/API/contract change requested from another owner

1. **Wiring request (integration, not a schema/contract change):** mount
   `decisions::routes(DecisionOperatorState::with_clock(state.repo.clone(), clock))`
   into `api` in `router.rs`, merged the same way `operator_execution_routes()` merges
   C1's routers — before `require_token`, with `inject_operator_principal` layered
   directly on top — and add `pub mod decisions;` to `handlers.rs`. Exact snippet in
   `decisions.rs`'s own module doc comment.
2. **A genuine, unresolved contract-vs-implementation gap (rule 13):**
   `docs/contracts/runner-v1/protocol.json`'s `authentication` block names
   `decision_resolution` a `"separately_scoped_operator_credential"` — distinct wording
   from the plain `"operator_session_or_api_token"` every other operator route uses —
   and `errors/forbidden.json`'s example fixture carries
   `"required_scope":"operator:decisions"`. Tack's actual operator-auth model
   (`middleware::require_token`/`AppConfig`) is a single shared bearer token with **no**
   scope/claim system at all anywhere in this codebase (confirmed: `orch.rs`'s
   `TACK_ORCH_APPROVAL_TOKEN` is the closest precedent — a wholly *separate* token, not
   a "scope" on the existing one — used for granting/denying docket approvals). Building
   a genuine second, decision-specific credential (mirroring
   `TACK_ORCH_APPROVAL_TOKEN`'s pattern) would require adding a field to
   `AppConfig`/`config.rs`, a shared struct outside this card's "new decision
   repository/service/handler modules" scope and not something I was asked to extend.
   **I did not invent a scope/second-credential system to satisfy this fixture
   language** — I mounted (via the wiring request above) behind the same
   `require_token` gate every other operator route already uses, which literally
   satisfies this card's own instruction ("operator-scoped (`/api` behind
   `require_token`), structurally separate — not an exemption entry on the runner
   surface"). **The smallest decision needed:** should Wave 5/6 add a real
   `TACK_ORCH_DECISION_TOKEN`-style separate credential (A0/config-owner decision), or
   is the frozen fixture's stronger wording aspirational/not binding on v1? I did not
   decide this unilaterally.
3. **A stale comment in C2's `runner_protocol.rs` (informational, not a request — that
   file is not mine to edit):** its own `LIMITS` struct doc comment says
   `decision_answer_bytes_max` bounds "an operator's decision *answer*, and decision
   resolution has no endpoint in this wave (see the handoff's known limitations — it is
   scoped to a later wave, **F5**)". The actual Wave 5 board in `TODO.md` assigns scoped
   decisions to **F1** (this card); F5 is "Runtime retention and observability" and
   never mentions decisions. Whoever wrote that C2 comment (Wave 2, before wave lettering
   past D was finalized) guessed wrong about which later card would own this. Flagging
   for whoever next touches that file's doc comment — not a functional issue, since the
   comment is descriptive prose, not enforced.
4. **`expire_overdue_decisions`** (bulk fail-closed sweep, this card, tested) is exposed
   but never called by anything — I did not wire a periodic caller, since "execution
   retention/metrics/health modules, startup/shutdown wiring" is III-F5's explicit
   charter, not this card's "repository/service/handler modules" one. F5 (or a future
   amendment to C2's `poll_decisions`, which currently has no lazy-expiry check of its
   own on the runner-read path) gets a ready-to-call, tested function.
5. **No OpenAPI annotation on `resolve_decision`/`ResolveDecisionResponse`** — E6's
   `executions.rs`/`runner_admin.rs` precedent carries `#[utoipa::path(...)]` on every
   handler even before integration, but adding it here would need `utoipa::ToSchema`
   derives whose only consumer (`openapi.rs`'s `ApiDoc::paths(...)`) is a file I must not
   edit; I judged the annotation itself low-value without that registration and left it
   out to keep this module's surface minimal. C5-successor integration should add it
   alongside the router wiring.
6. **No real `status_map_policy_id` interpreter exists anywhere** (see "Behavior
   implemented") — this is a pre-existing system-wide gap, not specific to this card,
   surfaced here because this card's own acceptance bar ("optional status mapping only
   after commit through the workflow engine") is the first place anyone was asked to
   actually honor that field for something concrete.

## Known limitations or `not_measured` fields

- Everything in the numbered list above.
- `resolved_by.kind: "system"` (used only for the fail-closed-expiry audit trail) is
  this card's own extension beyond the one frozen example
  (`decision.poll.response.json`'s `{"kind":"operator",...}`) — no fixture pins `kind`'s
  allowed values, and the shape (`{kind, subject_id}`) is preserved exactly, only the
  `kind` value is new. Flagged here rather than silently assumed compatible.
- Answer-vs-declared-options validation (`InvalidOption` → `invalid_request`) is this
  card's own integrity check, not something the card brief or any fixture explicitly
  requires — a nice-to-have I judged worth the ~15 lines, not a contract obligation.
- `GET`/list endpoints for decisions (e.g. "show me every pending decision across
  attempts") do not exist — out of this card's acceptance bar (which only names
  resolve), and likely III-F4's frontend-integration concern once it needs one.

## Secrets/logging review

- Every `tracing::info!`/`tracing::error!` call in `decisions.rs` carries only
  `attempt_id`, `decision_id`, a static `outcome`/`state` label, and (on the DB-error
  path) the `sqlx::Error`'s own `Display` output (ids/SQL-shape only — sqlx errors don't
  embed bound parameter values). **No call ever interpolates `answer`, `prompt`,
  `resolved_by`, or the request body.** Proven by
  `logs_never_contain_the_raw_answer_text_or_prompt_only_ids`, which captures real
  subscriber output around a resolve carrying a unique sentinel string and asserts its
  absence, plus the decision's own prompt text's absence, while asserting the ids *are*
  present (a redaction test that only checked "log is short" or "log has no secrets by
  eyeballing" would not have proven this — this one asserts the specific sentinel is
  absent from the literal captured bytes).
- `ResolveOutcome`'s `IdempotencyConflict.stored_answer` and `Expired`'s fields exist on
  the Rust enum for testability but the HTTP handler's actual `error(...)` calls never
  include `stored_answer`/`answer` content in the JSON `details` body — only
  `{"decision_id": ...}` — deliberately more conservative than strictly required (an
  HTTP response returning an operator's own past answer back to them isn't a credential
  leak, but I kept the error surface minimal anyway).
- No credential of any kind is constructed, read, or referenced anywhere in
  `decisions.rs` — the module's entire identity model is the non-secret
  `x-tack-principal` string (already documented in `middleware.rs` as
  "operator:token:<8-byte hex digest>" or "operator:local", never the raw token).
- The concurrency test's manually-held transaction and the two `resolve_decision_row`
  calls it races operate on synthetic test data only (`raw-f1-runner-credential-...`,
  a literal test string, never a real credential) and the file-backed temp DB is deleted
  at the end of the test (mirrors `execution_repo_test.rs`'s own cleanup convention).

## Safe merge order and likely conflicts

- This branch touches exactly two new files, both entirely outside every other card's
  ownership list (`crates/tack-api/src/handlers/decisions.rs`,
  `crates/tack-api/tests/f1_decisions_test.rs`). It does not touch
  `crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo.rs`,
  `crates/tack-api/src/router.rs`, `crates/tack-api/src/handlers.rs`,
  `crates/tack-api/src/openapi.rs`, `docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts`, `docs/contracts/runner-v1/**`,
  `.github/workflows/ci.yml`, `TODO.md`, root `Cargo.toml`, or any other card's handoff.
  No merge conflict is expected against F2 (events/artifacts), F3 (models/usage), or F5
  (retention) — all three own disjoint new files by the same "new modules" pattern this
  card follows.
- **Integration-time collision to expect, not a merge conflict:** F2 and F3 will each
  also want a `pub mod <theirs>;` line added to `handlers.rs` and a router-merge snippet
  added to `router.rs`'s `operator_execution_routes`-equivalent — the Wave 5 integrator
  should add all of F1/F2/F3's `pub mod` lines and router merges together, in one
  integration pass, the same way C5 did for C1/C2 in Wave 2, rather than as three
  separate sequential edits to the same two files.
- F1's `expire_overdue_decisions` is a natural, ready dependency for F5's retention
  sweep — F5 should branch after F1 is merged (or take the function's signature as a
  stable seam if branching in parallel; it takes only `&SqlitePool` and `DateTime<Utc>`,
  no card-specific state).
- F4 (frontend) depends on this card's resolve endpoint existing on a live route, which
  requires the wiring request above (item 1) to be done first by the integrator — F4
  cannot build a real decision-inbox UI against an unmounted route.

## Checklist

- [x] No unowned files touched — confirmed via `git diff --stat cbdd4a3` (two new
      files only) and re-stated above per-file.
- [x] No live secret committed, logged, or reachable via `argv`/`ps`/trace — see
      "Secrets/logging review"; the only "credential" anywhere in this card's test file
      is a literal test-only string, never an env var or real value.
- [x] No panic stub / `unimplemented!()` / fake success — the one genuinely
      unimplemented thing (item-status mapping) is a named, explained structural
      guarantee with a concrete reason (no policy schema exists to map from) and a
      negative test proving the guarantee, never a placeholder standing in for success.
      `UnknownState` is a typed, logged, `internal_error()`-mapped defensive branch, not
      a panic or a silent success.
- [x] No blind retry — nothing in this module retries anything automatically; a losing
      concurrent resolve reports `idempotency_conflict` once and stops.
