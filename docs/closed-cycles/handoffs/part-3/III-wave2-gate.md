# Wave 2 integration gate — verdict

- Base SHA / branch: HEAD `2850300` on `plan/harness-agnostic-agent-fleet` (same
  commit C5's handoff records as its base), worked directly in the shared main
  checkout per instructions — no worktree, no commit. C1–C5's own uncommitted
  work (`handlers/{executions,runner_admin,runner_protocol}.rs`,
  `runner_protocol/runner_auth.rs`, `middleware.rs`, `router.rs`, `openapi.rs`,
  `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
  `docs/agent-handoffs/part-iii/III-{C1,C2,C3,C4,C5}.md`, plus B1/B2/C3
  amendments) was already present and untouched by this gate; `git diff
  --stat` before and after this work shows zero change to any file this card
  does not own.
- Files changed (must equal ownership list): new
  `crates/tack-api/tests/wave2_gate.rs`; one line removed
  (`#[allow(dead_code)]`, plus its now-stale justification comment) from
  `crates/tack-api/src/handlers.rs`; this handoff. Nothing else.

## Verdict

**Yes — Wave 2 genuinely passes its integration gate (TODO.md:9578-9582).**

> A mock runner enrolled on a clean database can claim, start, stream,
> complete and survive API/runner restart. Security, fencing, payload and
> OpenAPI drift gates pass. No real harness card starts earlier.

Every clause was independently re-proven against `tack_api::router::build_router`
(the actual production router C5 mounted, not a card-local stand-in) from a new
test file written by this integrator, importing no test infrastructure from C1,
C2, or C5. Detail per stage below.

## Step-by-step verdict on the lifecycle

All in `wave2_gate_claim_start_stream_complete_and_survive_restart`, asserting
persisted database rows at every step, not just HTTP status codes:

1. **Operator creates an execution request from a real Tack item** — pass.
   Project and item created over real HTTP (`POST /api/projects`,
   `POST /api/projects/{id}/items`), not a direct repository shortcut;
   `execution_requests.item_id` confirmed to match.
2. **Operator issues an enrollment token; a mock runner redeems it and
   receives a credential (raw token returned exactly once)** — pass.
   `agent_runners.state` confirmed `active` after enrollment;
   `agent_runners.credential_hash` confirmed to differ from the raw
   credential captured from the one-time enrollment response.
3. **Runner reports capabilities, claims the queued request, and receives a
   fencing token** — pass. `/refresh` then `/claim`; fence starts at `1` on a
   fresh request (`execution_attempts.fencing_token`); `execution_requests.state`
   observed `leased`.
4. **Runner accepts (→ preparing) and starts (→ running)** — pass.
   `execution_attempts.state`/`prepared_at`/`started_at` confirmed at each
   step through direct SQL, not just the HTTP response body.
5. **Runner streams at least two event batches with advancing checkpoints**
   — pass. `checkpoint-0001` then `checkpoint-0002`, chained via
   `previous_checkpoint`; `execution_attempts.event_checkpoint` and
   `execution_events` row count confirmed after each.
6. **API restart** — pass. The router/`AppState` (broadcast channel, orch
   runtime, router/middleware closures) is fully rebuilt around the *same*
   `SqlitePool`, mid-flight (attempt still `running`, not terminal), between
   the first and second event batch — a stronger placement than restarting
   only after the whole lifecycle finished. The second batch, the mid-flight
   operator read, the completion, and the completion replay are all driven
   through the *new* router instance, using the runner's original credential
   and fencing token unchanged. No event was lost (count goes 1 → 2 across
   the restart) and no new fence was required.
7. **Runner reports completion; operator observes the terminal state** —
   pass. `execution_attempts.state` and `execution_requests.state` both
   confirmed `succeeded` via direct SQL; `GET /api/executions/{request_id}`
   (through the post-restart router) returns `state: "succeeded"`; a
   completion replay against the same restarted router returns
   `replayed: true` with an identical `committed_at`, proving the guarantee
   lives in the database the restart preserved, not in anything the
   pre-restart process held in memory.

## Adversarial half

- **Fencing** (`superseded_fence_is_rejected_as_stale_lease_and_writes_nothing`)
  — pass. This does not merely present a wrong fencing-token number (that
  much was already covered card-locally by C2); it drives a genuine
  supersession: the runner reports a real recovery observation
  (`/recovery-observation`, `process_stopped` / `journal_state: "prepared"` /
  `process_observed: false`) on an attempt that never started, which B2's
  `recover_attempt` disposes as `safe_pre_spawn_requeue` — the attempt goes
  to `lost`, the request returns to `queued`, entirely without operator
  action. The same runner then reclaims the request, receiving a strictly
  higher fencing token (`2`) on a *new* attempt id. A write against the old
  attempt id with the old (`1`) fencing token is rejected with the stable
  `stale_lease` code (HTTP 409), and is confirmed to write nothing: zero
  `execution_events` rows, `event_checkpoint` still `NULL`, and the
  superseded attempt's own `state` still `lost` (untouched by the rejected
  write). The new fence is then proven to genuinely work (`/accept`
  succeeds), showing the rejection was scoped to the superseded fence
  specifically.
- **Security — credential non-substitution**
  (`runner_and_operator_credentials_are_not_substitutable_across_the_production_router`)
  — pass. A real, just-enrolled runner's own valid credential is rejected by
  every operator route (`GET /api/executions` → 401); the operator's own
  valid `TACK_API_TOKEN` is rejected by `/api/runner/v1/claim` → 401,
  `error.code: "unauthorized"`; neither family accepts no credential either.
  The stored `credential_hash` is confirmed to differ from the raw
  credential throughout.
- **Security — principal spoofing**
  (`client_supplied_principal_header_is_never_trusted`) — pass. A client
  claiming `x-tack-principal: attacker-claimed-identity` on the first call
  and no header at all on an identical-idempotency-key retry both resolve to
  the *same* persisted request (`replayed: true`); the persisted
  `execution_requests.request_snapshot.created_by.subject_id` is confirmed to
  be neither client-supplied string; a genuinely different idempotency key
  from the same (server-derived) principal still creates a distinct request,
  proving the header's absence doesn't disable idempotency scoping
  altogether.
- **Payload** (`oversized_event_batch_is_rejected_and_writes_nothing`) —
  pass. A 101-event batch (over `event_batch_count_max` = 100) is rejected
  with HTTP 413 and `error.code: "payload_too_large"`; `execution_events`
  count and `event_checkpoint` are confirmed unchanged; the same attempt is
  then proven still healthy by a normal batch succeeding on the same fence
  immediately afterward, showing the rejection was scoped to the one
  oversized request rather than corrupting the attempt.

## 10x loop result

`cargo test -p tack-api --test wave2_gate`, run 10 times in a loop: **10/10
passed, 50/50 individual test-cases passed, 0 failed.** No SQLite deadlock or
other flakiness reproduced (this stack's earlier deadlock class was fixed
elsewhere this session, per B2/C2/C4's handoffs' own `BEGIN IMMEDIATE`
serialization notes).

## `#[allow(dead_code)]` removal (Task 2)

Removed from `crates/tack-api/src/handlers.rs`'s `pub mod runner_protocol;`
line, along with its now-stale justification comment (the comment existed
solely to explain that attribute; leaving it behind would misdescribe a
suppression that no longer exists). `cargo clippy --workspace --all-targets
-- -D warnings` is clean without it: C2 has since amended
`runner_protocol.rs`'s `Limits` struct with field-scoped
`#[allow(dead_code)]` annotations on exactly the fields still unread outside
`#[cfg(test)]` (see that file's own comment above the `Limits` struct
definition, which explicitly anticipates this: "C5 may remove that allow now
that the dead fields are excused precisely, here, in the file that owns
them"). The blanket module-level suppression is confirmed no longer needed
and was left removed.

## OpenAPI / frontend gate results (Task 3)

- `cargo test -p tack-api --test openapi_contract` — **5 passed**, including
  `openapi_spec_matches_committed_file`, without regenerating anything.
  `docs/openapi.json` is not stale relative to the code (no C5 finding to
  report).
- `cd frontend && npm run type-check` — clean.
- `cd frontend && npm test -- --run` — **482 passed** (60 files).
- `middleware.rs::is_public_route` — confirmed by direct read: lists only
  `/api/health`, `/api/openapi.json`, `/api/alexa` (and their unprefixed
  forms). No `/api/executions*`, `/api/runners*`, `/api/runner-fleets*`,
  `/api/agent-profiles*`, `/api/model-profiles*`, or `/api/runner/v1/*` path
  appears — matching C5's own structural claim that runner-v1 routes sit
  outside `require_token` entirely (a separate top-level `outer.nest(...)`,
  never merged into the `api` sub-router the middleware layers onto) rather
  than being exempted from it. This was also live-verified: every operator
  and runner-v1 path this file touches returns 401 with no credential
  (folded into the security tests above rather than duplicated as its own
  test, since C5's own `every_execution_and_runner_v1_path_requires_authentication_live`
  already covers the exhaustive enumeration through the production router).

## Full workspace verification

- `cargo test -p tack-api --test wave2_gate` — 5 passed, 0 failed.
- `cargo test --workspace` — **910 passed, 0 failed** (905 pre-existing +
  this card's 5 new tests), 2 doctests intentionally ignored (pre-existing,
  `tack-orch`), across every crate.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.
- `git diff --check` — clean.
- `git diff --stat` on every file this card does not own — zero change,
  confirmed both before and after this card's work.

## Known limitations

- This file deliberately does not re-prove the CORS-preflight or
  route/OpenAPI-enumeration assertions C5's own `c5_integration_test.rs`
  already covers live through the production router (e.g.
  `runner_v1_and_execution_routes_share_the_global_cors_policy`,
  `openapi_document_enumerates_the_mounted_operator_and_runner_v1_routes`) —
  duplicating them here would not strengthen the gate; this file instead
  covers what no other file proves: the full lifecycle through the mounted
  router with a *mid-flight* restart (not a post-completion one), a genuine
  fence-supersession scenario (not a wrong-number probe), and its own
  independent security/payload checks built from a from-scratch fixture set.
- Decision and artifact endpoints are exercised elsewhere (C2/C4) and are not
  part of the gate's stated lifecycle ("claim, start, stream, complete");
  this file does not add coverage for them.
- No real harness card (Wave 3, III-D1–D5) has started — confirmed by
  `TODO.md`'s status board, unedited by this card (only the wave integrator
  updates it, and only after this handoff is reviewed/accepted, per III.2
  rule 11 — this handoff records the gate result but does not itself flip
  the board).

## Secrets/logging review

No log line was added or changed. The new test file's own leak-scan is
narrower than C5's (it checks the specific fields it captures — raw runner
credential, stored `credential_hash` — rather than scanning every response
body verbatim), since exhaustive credential-leak scanning across the whole
lifecycle is already C5's own, more thorough, proven coverage; duplicating it
here would not add signal.

## Checklist

No unowned file edited (verified via `git diff --stat`, reported above). No
live secret. No panic stub — every new function returns typed
results/responses; `unwrap()`/`expect()` in `wave2_gate.rs` follow this
codebase's existing test-only convention (test assertions, fixture setup),
never production code. No blind retry. No sleep of any kind — every state
transition this file needs is driven explicitly over HTTP rather than waited
on.
