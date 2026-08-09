# III-C5 handoff

- Base SHA / branch / final SHA: Wave 2 base (C1/C2/C3/C4 branches integrated in the shared
  working tree, HEAD `2850300` on `plan/harness-agnostic-agent-fleet`) / same branch, worked
  directly in the main checkout per instructions / left uncommitted, no final SHA recorded.
  C1 (`executions.rs`, `runner_admin.rs`, `c1_handlers_test.rs`), C2 (`runner_protocol.rs`,
  `runner_protocol/runner_auth.rs`, `c2_handlers_test.rs`), B1/B2 amendments
  (`tack-orch/src/execution/types.rs`, `tack-db/src/repo/execution.rs`,
  `tack-db/tests/execution_repo_test.rs`) and C3's `tack-runner/src/workspace.rs` follow-up were
  already uncommitted in the shared checkout when this card started; none of them were edited by
  this card (verified below).
- Files changed (must equal ownership list): `crates/tack-api/src/handlers.rs` (module
  registration only), `crates/tack-api/src/router.rs`, `crates/tack-api/src/openapi.rs`,
  `crates/tack-api/src/middleware.rs` (small, justified addition per the ownership table's "A1
  for security correction, then C5 for runner/execution wiring" note), `docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts`, `crates/tack-api/tests/c5_integration_test.rs` (new),
  this handoff. `git diff --stat` on the files I own: `handlers.rs` +18, `middleware.rs` +133/-,
  `openapi.rs` +531/-, `router.rs` +87/-, `docs/openapi.json` +4463/-1133 (see below — almost
  entirely line-diff noise from alphabetical re-sorting, not content churn),
  `frontend/src/shared/api/schema.gen.ts` +2322/-0.

## Route table mounted

| Path | Method | Auth | Handler (owner) |
|---|---|---|---|
| `/api/executions` | POST, GET | operator (`require_token`) + injected `x-tack-principal` | `handlers::executions` (C1) |
| `/api/executions/{request_id}` | GET | operator | `handlers::executions` (C1) |
| `/api/executions/{request_id}/cancel` | POST | operator | `handlers::executions` (C1) |
| `/api/executions/{request_id}/requeue` | POST | operator + injected `x-tack-principal` | `handlers::executions` (C1) |
| `/api/runner-fleets` | POST, GET | operator | `handlers::runner_admin` (C1) |
| `/api/runners/enrollment` | POST | operator | `handlers::runner_admin` (C1) |
| `/api/runners/{runner_id}/enrollment-tokens/{token_id}/revoke` | POST | operator | `handlers::runner_admin` (C1) |
| `/api/runners/{runner_id}/revoke` | POST | operator | `handlers::runner_admin` (C1) |
| `/api/agent-profiles` | POST, GET | operator | `handlers::runner_admin` (C1) |
| `/api/model-profiles` | POST, GET | operator | `handlers::runner_admin` (C1) |
| `/api/runner/v1/enroll` | POST | single-use enrollment token in body (no bearer) | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/refresh` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/claim` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/heartbeat` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/accept` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/start` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/events` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/decisions` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/decisions/poll` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/artifacts` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/completion` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/cancellation-observation` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |
| `/api/runner/v1/attempts/{attempt_id}/recovery-observation` | POST | runner bearer credential | `handlers::runner_protocol` (C2) |

"operator" = `require_token` (`TACK_API_TOKEN` bearer, or pure-local/unauthenticated when unset —
unchanged existing behavior). "runner bearer credential" = `runner_protocol::runner_auth::authenticate`,
a completely separate check (hashed `Authorization: Bearer` looked up in `agent_runners`) that never
reads `TACK_API_TOKEN` and is never invoked by `require_token`.

## Structural auth separation (not a shared exemption list)

`router.rs::build_router` merges C1's two card-local routers (`operator_execution_routes`) into
`api` **before** `api.layer(require_token)` is applied, so they share the exact same operator gate
as the other 76 pre-existing endpoints. C2's router (`runner_protocol_routes`) is instead **nested
as a sibling of `/api`** on `outer` — `outer.nest("/api", api).nest("/api/runner/v1",
runner_protocol_routes(&state))` — which places it structurally *outside* `api`'s `require_token`
layer entirely, rather than inside it with a path added to `middleware::is_public_route`'s
exemption list. This is a stronger property than "the exemption list doesn't mention it": there is
no code path by which `require_token` ever runs against a `/api/runner/v1/*` request at all, so a
future edit accidentally adding a runner-v1 path to that list would be a no-op it can't reach, not
a silent hole. Both card-local routers return a concrete `Router` (`Router<()>`, since they call
`with_state` on themselves per their own signature) rather than the generic `Router<AppState>`
`orch_routes` uses; merging/nesting a `Router<()>` into `Router<AppState>` requires matching state
types, so both are re-labelled via `.with_state::<AppState>(())` — the officially documented axum
idiom for merging routers whose state types differ (`axum::Router::merge`'s own doc example does
exactly this). No new `Service`-erasure layer, no extra path segment neither C1 nor C2 chose.

Runner-v1 inherits every layer on `outer` except the operator-token check — CORS, CSP/security
headers, and tracing all genuinely apply to `/api/runner/v1/*`, proven live for CORS by
`runner_v1_and_execution_routes_share_the_global_cors_policy` and confirmed for security headers
and tracing by direct inspection: neither `SetResponseHeaderLayer` nor `TraceLayer` has any
competing, more-specific layer inside `runner_protocol::routes`, so the single instance on `outer`
is the only one that ever runs, for every request through either nest. **The global body limit was
the one exception, corrected by an integrator-authorized cross-card amendment** (see "Amendment:
runner-v1 body limit respects the operator-configured global limit" below and in
`III-C2.md`): this sub-router carries its own, more-specific `DefaultBodyLimit` layer (a fixed
4 MiB protocol ceiling), and axum always applies whichever `DefaultBodyLimit` layer is closest to
the handler — so the plain global layer on `outer` never actually bound a runner-v1 request, and an
operator who tightened `TACK_MAX_BODY_SIZE`/`max_body_size_bytes` below 4 MiB got no effect on
`/api/runner/v1/*` at all. The fix threads `state.config.max_body_size_bytes` into
`runner_protocol::routes`, whose own layer now enforces `min(configured, 4 MiB)` instead of a bare
4 MiB constant.

`middleware.rs::is_public_route` itself was **not edited** — no runner or execution path was added
to it, confirmed by `middleware::tests::no_runner_or_execution_path_is_publicly_exempt` (checks
both `/api/...` and unprefixed forms, plus suffix/prefix lookalikes of the genuinely-public routes)
and the live `c5_integration_test::every_execution_and_runner_v1_path_requires_authentication_live`
(every operator and runner-v1 path, hit with no credential at all through the *production* router,
returns 401, not a silent pass-through).

## Principal injection — how it's enforced, and the test that proves it can't be spoofed

C1's handoff states this as a hard prerequisite: `create_execution` and `requeue_needs_operator`
both read `x-tack-principal` from the request headers and trust it completely for idempotency
scoping (`format!("operator:{principal}")`) and audit `actor`. A spoofable header means one caller
could read another's replay state or collide with their idempotency key.

`middleware::inject_operator_principal` (new, `middleware.rs`) is layered directly on
`operator_execution_routes`'s sub-router — it runs for every request C1's handlers see, on every
path, unconditionally. It does not check whether a client-supplied header is present; it always
calls `req.headers_mut().insert(HeaderName::from_static(OPERATOR_PRINCIPAL_HEADER), value)`, which
**replaces** any existing value for that header name (there is no code path that reads the
client's value first). `value` comes from `operator_principal_value(&state.config)` — a pure
function of the server's own `AppConfig`, never of the request. Tack's operator-auth model
(`docs/contracts/runner-v1/protocol.json`: `operator_session_or_api_token`) is a single shared
bearer token, not per-user sessions, so every request that clears `require_token` with the same
configured token is structurally the same principal today; `operator_principal_value` derives a
stable, non-secret identifier from that token (truncated SHA-256, never the raw token) or a fixed
`"operator:local"` when no token is configured (i.e. `require_token` itself lets every caller
through). The injection point does not change if the auth model later grows real per-caller
sessions — only what this function returns would.

**Test that proves it cannot be spoofed:**
`c5_integration_test::x_tack_principal_from_an_external_client_is_stripped_and_overridden` drives
the *production* router. It sends `POST /api/executions` with the same idempotency key three times
— once claiming `x-tack-principal: victim`, once claiming `x-tack-principal: attacker`, once with
no header at all — and asserts all three return the identical `request_id` with `replayed: true`
on the second and third. If the header were trusted, the second/third calls would have been scoped
to a different principal and created unrelated requests instead of replaying. It also reads the
persisted immutable `request_snapshot` directly from the database and asserts the recorded
`created_by.subject_id` equals neither client-supplied string, and finally proves scoping still
works at all (a genuinely different idempotency key from the real injected principal creates a
distinct request) so the test can't be satisfied by principal injection silently disabling
idempotency altogether. `middleware::tests::operator_principal_is_stable_and_never_the_raw_token`
covers the pure function directly: deterministic for a fixed config, different for a different
token, and never contains the raw token substring.

## Vertical slice through the production router

`c5_integration_test::production_router_completes_the_mock_vertical_slice_and_survives_restart`
builds the real `tack_api::router::build_router` (not a card-local router) over a clean in-memory
database and drives, entirely over HTTP: operator creates an agent profile and a pending runner
with a one-time enrollment token → runner enrolls (`/api/runner/v1/enroll`) → operator creates an
execution request selecting that exact runner → runner claims → accept (`preparing`) → start
(`running`) → streams one event batch → completes → operator reads back `state: "succeeded"`.
It then **rebuilds a fresh `Router`/`AppState`** around the *same* `SqlitePool` (a new broadcast
channel, a new `OrchRuntime`, a freshly constructed router/middleware chain — everything a real
process restart would reset, while only the database persists) and proves the terminal state is
still readable and a completion replay against the *new* router instance is still idempotently
`replayed: true` with the identical `committed_at` — i.e. the guarantee lives in the database, not
in anything the old process held in memory. The same test scans every response body captured
across the whole lifecycle and asserts neither the raw runner credential, the raw enrollment
token, nor either's stored hash appears anywhere except their two documented one-time-issuance
responses.

## Distinct-credential proof

`c5_integration_test::operator_and_runner_credentials_are_not_substitutable_on_the_production_router`
registers one real runner, then proves, through the production router: the operator's own valid
`TACK_API_TOKEN`, presented as a runner-v1 bearer credential, is rejected by `runner_auth`
(`unauthorized` — it never even compares against `TACK_API_TOKEN`, it looks up a runner by hashed
credential and finds none); a real runner's own bearer credential, presented to an operator route,
is rejected by `require_token` (which never looks up runner credentials — a constant-time compare
against `TACK_API_TOKEN` and nothing else); and neither family accepts no credential at all.

## OpenAPI: operator contract exposed, runner-v1 contract versioned

C1's and C2's handler files return raw `Json<serde_json::Value>` with no `#[utoipa::path(...)]`
annotation, and per III.2 rule 2 those files are owned by other cards — I could not add an
annotation there. Instead, `openapi.rs` (owned) defines two small `struct`s that implement
`utoipa::OpenApi` **manually** (not via the derive macro) — `OperatorApiDoc` and
`RunnerProtocolApiDoc` — each building its `Paths` with utoipa's public builder API
(`PathsBuilder`/`PathItemBuilder`/`OperationBuilder`/etc.), and composes them into `ApiDoc` via
`#[openapi(nest((path = "/api", api = OperatorApiDoc), (path = "/api/runner/v1", api =
RunnerProtocolApiDoc)))]` — utoipa's own documented mechanism for assembling a spec from multiple
`OpenApi` impls, which also handles the path-prefixing so each fragment's relative paths
(`/executions`, `/enroll`) become the real mounted ones automatically. Request/response bodies use
the same free-form `serde_json::Value` schema (`<serde_json::Value as
utoipa::PartialSchema>::schema()`) this file already uses for every other ad hoc JSON handler — I
did **not** invent a second, hand-typed shape for the runner-v1 wire format; every runner-v1
operation's `description` instead points back to `docs/contracts/runner-v1/` as the sole authority
for field shapes (III.1.6), and the `runner-protocol-v1` tag description explains why. The operator
surface (Tack's own REST API, not a third-party protocol) is documented more fully — per-operation
summaries, path parameters, and the real request-body field lists — since there's no equivalent
frozen-fixture authority for it to defer to. `x-tack-principal` is deliberately never documented as
a settable request header anywhere, since it isn't one.

Regenerated once, as instructed: `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`
then `cd frontend && npm run gen:api`. Neither file was hand-edited afterward.

- `docs/openapi.json`: 61 → 84 paths (+23, exactly 10 operator + 13 runner-v1), 17 → 19 tags (+2:
  `execution-operator`, `runner-protocol-v1`), **119 → 119 schemas (unchanged)** — by design, no
  new component schema was added (free-form bodies + reusing the existing `ErrorEnvelope` ref).
  `git diff --stat` reports +4463/-1133 lines, which looks alarming but is almost entirely
  line-diff noise: `Paths` is a `BTreeMap<String, PathItem>` (utoipa, sorted by key), so the 23 new
  alphabetically-interleaved keys shift the *positions* of many pre-existing, byte-identical blocks
  in the pretty-printed file without changing their content — verified directly (e.g. the
  `/api/health` block's "-" and "+" hunks are character-for-character identical, just relocated).
  Confirmed via the exact path/tag/schema counts above and by `git diff` inspection, not asserted
  on faith.
- `frontend/src/shared/api/schema.gen.ts`: +2322/-0 — purely additive; `npm run type-check` and
  `npm test -- --run` (482 tests) both pass unmodified, since no existing frontend code references
  the new operator/runner-v1 types yet.

## Tests added and exact commands/results

- `cargo test -p tack-api --test c5_integration_test` — **6 passed**: the vertical
  slice+restart+leak-scan test above; the principal-non-spoofability test; the
  distinct-credential test; a live route/auth enumeration test (every operator and runner-v1 path
  requires authentication, hit through the production router with no credential); an OpenAPI
  route-enumeration/credential-field-absence drift check; a CORS preflight check for one operator
  and one runner-v1 path.
- `middleware.rs`'s own `#[cfg(test)]` block: two new unit tests,
  `no_runner_or_execution_path_is_publicly_exempt` and
  `operator_principal_is_stable_and_never_the_raw_token`, both described above.
- `cargo test -p tack-api` — **345 passed, 0 failed** (includes `c1_handlers_test` 7,
  `c2_handlers_test` 16, `c5_integration_test` 6, `runner_vertical_slice` 7, `trust_boundary_test`
  3, `cors_test` 2, `openapi_contract` 5, and every pre-existing suite unmodified and still green).
- `cargo test --workspace` — **894 passed, 0 failed**, 2 doctests intentionally ignored
  (pre-existing, `tack-orch`), across every crate.
- `cargo clippy --workspace --all-targets -- -D warnings` — **clean** (see the `Limits` dead-code
  note below for the one thing that needed a narrow, owned-file suppression to get there).
- `cargo fmt --all -- --check` — clean after `rustfmt --edition 2024` on exactly the five files
  this card owns/created (`handlers.rs`, `middleware.rs`, `openapi.rs`, `router.rs`,
  `c5_integration_test.rs`) — no unowned file was reformatted (rule 10); confirmed by re-running
  the workspace-wide check afterward and finding zero remaining diffs anywhere.
- `git diff --check` — clean.
- `cargo test -p tack-api --test runner_vertical_slice`, run 10 times in a loop: **10/10 passed,
  0 failed** — no deadlock/contention reproduced.
- `cd frontend && npm run type-check` — clean. `npm test -- --run` — **482 passed** (60 files).

## Failure/adversarial case proved

- A client-supplied `x-tack-principal` never survives to C1's handlers under any of: a spoofed
  value claiming to be another principal, a spoofed value on a *retry* of the same idempotency
  key, or no header at all — all three produce the identical server-derived principal, proven by
  reading the persisted database row directly, not just the response.
- Every operator and runner-v1 path, hit through the real mounted router with zero credentials,
  returns 401 — none are silently public.
- The operator's own valid token cannot authenticate a runner-v1 route; a real runner's own valid
  credential cannot authenticate an operator route — proven with a real, registered runner
  credential (not a guess), through two independent, non-overlapping code paths
  (`require_token`'s constant-time compare vs. `runner_auth`'s hashed DB lookup).
- No raw credential, raw enrollment token, or either's stored hash appears in any response body
  across a full enroll→claim→accept→start→events→completion lifecycle, except the two documented,
  intentional one-time issuance responses (enrollment, and completion is excluded from that
  carve-out — its scan covers `completed`/`replayed` too, matching the code path that goes through
  the real mounted router, not just C2's own card-local test).
- The terminal execution state and idempotent completion-replay behavior both survive a full
  router/AppState rebuild against the same database — the guarantee is durable, not in-memory.
- `runner_vertical_slice` (C4's crash matrix) is unaffected by this card's wiring: 7/7 passing
  both standalone and across 10 repeated runs, confirming no new lock contention was introduced by
  mounting the real routers.

## Schema/API/contract change requested from another owner

None to a frozen contract, fixture, or another card's owned file. One thing worth another owner's
attention, recorded here rather than fixed (rule 2):

**C2's `runner_protocol::Limits` struct has several fields read only by its own
`limits_constants_match_frozen_fixture_exactly` test inside `#[cfg(test)]`** (e.g.
`environment_entries_max`, `heartbeat_grace_seconds`, `event_batch_bytes_max`,
`retention_event_days_default`, and others — not yet enforced by any handler). This was invisible
before this card because `runner_protocol.rs` was previously only ever compiled inside a test
binary via `#[path]` (where `#[cfg(test)]` is always active); now that it's a genuine `pub mod` in
`handlers.rs`, those fields are unread dead code on the plain `lib` target, which
`cargo clippy --workspace --all-targets -- -D warnings` fails on. I did not edit
`runner_protocol.rs` to fix this (not my file); instead I added `#[allow(dead_code)]` directly on
the `pub mod runner_protocol;` line in `handlers.rs` (mine), with a comment explaining why. This is
a narrow, mod-boundary-scoped suppression, not a broad `allow` — it doesn't touch anything else in
`handlers.rs` and doesn't silence any *other* lint. A future C2 amendment could resolve this
properly either by having a handler actually enforce every `Limits` field (e.g. validating
`environment_*`/`retention_*` on the relevant requests) or by restructuring the fixture test so it
doesn't require every field to be "read" outside `#[cfg(test)]`; either fix would let this
suppression be removed.

## Known limitations or `not_measured` fields

- The operator-auth model is currently single-tenant (one shared `TACK_API_TOKEN`, or none in pure
  local mode), so `x-tack-principal` today is effectively constant per deployment — the injection
  point is correct and forward-compatible with real per-caller sessions, but there is no *current*
  scenario where two different legitimate principals exist in the same running server to
  cross-check against each other; the test suite instead proves the property that actually matters
  right now (client input has zero effect), not a multi-tenant scenario the system doesn't yet
  support.
- The OpenAPI documentation for the runner-v1 surface is deliberately shallow (free-form bodies,
  descriptions pointing at `docs/contracts/runner-v1/`) rather than a field-accurate schema — see
  the "OpenAPI" section above for why a more detailed, independently-typed schema here would itself
  violate III.1.6.
- Artifact **content** upload/download, decision **resolution**, and a dedicated fleet-membership
  route were already out of scope per C1/C2's own handoffs and remain unmounted — there is nothing
  for this card to wire for them yet.
- I did not attempt to fix or re-verify any of C1/C2/B1/B2/C3's pre-existing uncommitted amendments
  in this shared checkout beyond confirming (via `git diff --stat` before/after this card's work)
  that none of their files changed as a side effect of mine.

## Secrets/logging review

No log line was added or changed by this card. `operator_principal_value` never logs; it is a pure
function whose only side effect is a header write. `inject_operator_principal` does not log. The
new integration test's leak-scan (see above) is the strongest available evidence that the mounted
routes still meet C1/C2's own redaction guarantees end-to-end.

## Safe merge order and likely conflicts

This card depended on the accepted state of C1–C4's handoffs and B1/B2's amendments already
present in this shared checkout; it does not need to be merged in any particular order relative to
them beyond "after all four are present," since it only adds `pub mod` registrations, router
wiring, and new OpenAPI/test files — it never edits their internals. The likely conflict surface if
any of C1/C2 land further amendments before this integrates: `handlers.rs`'s two new `pub mod`
lines (trivial to re-apply), and `router.rs`/`openapi.rs` if a future amendment changes
`OperatorExecutionState`/`RunnerProtocolState`'s public constructor shape or either card's
`routes()` function signature — both are called by name from `router.rs` exactly as their own
handoffs specify.

## Checklist

No unowned files edited (verified via `git diff --stat` on C1/C2/C3/B1/B2's files showing zero
change from this card). No live secret. No panic stub — every new function returns typed
results/responses; `unwrap()`/`expect()` in `c5_integration_test.rs` follow this codebase's
existing test-only convention (test assertions, fixture setup), never production code. No blind
retry.

## Amendment: runner-v1 body limit respects the operator-configured global limit (integrator-authorized cross-card fix)

- Base SHA / branch / final SHA: this amendment starts at HEAD `2850300` on
  `plan/harness-agnostic-agent-fleet`, in the same shared checkout this handoff's own work landed
  in. Per this task's instructions the work is left uncommitted, so no final SHA is recorded here.
- Authorization: performed directly by the Wave 2 integrator, who explicitly authorized this
  specific cross-card edit and instructed it be recorded here and in `III-C2.md` so a later
  ownership audit does not read it as a III.2 rule 2/rule 5 violation ("Stay inside `Owns`" /
  "No router/OpenAPI/generated-schema edits outside C5"). Rule 5 names this card as the one owner of
  `router.rs`; the integrator is the higher authority that rule serves — the same authority who
  would normally receive this card's own "schema/API/contract change requested from another owner"
  notes and decide how to route them. Recording the edit here, rather than silently landing it,
  keeps that authorization auditable rather than assumed.
- Files changed (owned): `crates/tack-api/src/router.rs` (`runner_protocol_routes`'s call site and
  its doc comment — one call site, nothing else in the file), `crates/tack-api/tests/c5_integration_test.rs`
  (one new test plus two small helper functions), and this file (the correction above plus this
  section). Files changed (integrator-authorized exception, not owned by this card):
  `crates/tack-api/src/handlers/runner_protocol.rs` — owned by, and described in full in, C2's own
  parallel amendment in `III-C2.md`.
- Contract fixtures consumed: none new.
- Behavior implemented: this handoff's own "Structural auth separation (not a shared exemption
  list)" section, above, asserted "Runner-v1 still inherits every layer on `outer` (CORS,
  CSP/security headers, **the global body limit**, tracing)". That sentence was false for the body
  limit specifically, proven live by an independent adversarial verifier, and has been corrected in
  place above rather than left standing with a contradicting note beneath it. The root cause: this
  file's `nest("/api/runner/v1", runner_protocol_routes(&state))` composes a sub-router whose own
  more-specific `DefaultBodyLimit` (C2's fixed 4 MiB `RUNNER_ROUTER_BODY_LIMIT_BYTES`) always won
  over the plain `DefaultBodyLimit::max(state.config.max_body_size_bytes)` this file layers on
  `outer` — axum applies whichever `DefaultBodyLimit` layer sits closest to the handler, and the
  runner-v1 sub-router's own layer is closer. The fix: `runner_protocol_routes` now passes
  `state.config.max_body_size_bytes` as a new second argument to `runner_protocol::routes` (C2's
  signature change), whose own layer computes `min(configured, 4 MiB)` instead of a bare constant.
  No other line in `router.rs` changed. The doc comment on `runner_protocol_routes` is rewritten to
  describe the body limit's real precedence rule instead of a flat inheritance claim, and to
  cross-reference this amendment.

  Of the four layers the original (now-corrected) sentence claimed runner-v1 "inherits": CORS,
  security headers (`SetResponseHeaderLayer` × 4: `x-content-type-options`, `referrer-policy`,
  `x-frame-options`, `content-security-policy`), and tracing (`TraceLayer`) were checked directly
  against this amendment's concern and found to be **genuinely, correctly inherited** — unlike the
  body limit, none of them has any competing, more-specific layer inside
  `runner_protocol::routes`, so the single instance each carries on `outer` is the only one that
  ever runs, for every request through either the `/api` or `/api/runner/v1` nest. CORS was already
  proven live by the pre-existing `runner_v1_and_execution_routes_share_the_global_cors_policy`;
  security headers were confirmed live for this amendment (a `POST /api/runner/v1/claim` response
  carries all four headers with the exact values `build_router` configures, verified with a
  temporary throwaway test, removed after confirming — not left in the suite, since it would only
  re-prove a structural fact the code has no way to violate without a competing layer that does not
  exist); tracing needs no live header check since `TraceLayer` has no override semantics to defeat
  in the first place (grepped both `runner_protocol.rs` and `runner_protocol/runner_auth.rs` for any
  `TraceLayer`/span construction — none exists). The body limit was the *only* one of the four with
  a genuine defect, because it was the only one of the four with a second, competing instance of the
  same layer type mounted closer to the handler.

- Tests added and exact commands/results:
  - `runner_v1_body_limit_is_the_lesser_of_configured_and_protocol_ceiling` (new, this file — the
    only file in Part III that drives the *production* router, which is what a body-limit
    precedence claim about `router.rs` + `runner_protocol.rs` together requires). Builds the real
    `build_router` twice — once with `max_body_size_bytes: 2 * 1024` (2 KiB, the exact
    live-reproduction value), once with `10 * 1024 * 1024` (10 MiB) — and, for each, enrolls a real
    runner and queues one real claimable execution request through the full operator+runner-v1 API
    (not a stub or card-local router).
    - **Direction 1 (2 KiB configured, below the 4 MiB ceiling):** a 512 KiB `/api/runner/v1/claim`
      body — above the 2 KiB configured limit, comfortably below both `limits.json`'s own
      `json_body_bytes_max` (1 MiB, so the rejection can't be mistaken for that pre-existing
      handler-level check) and the 4 MiB ceiling — is rejected with a genuine `413`. "Genuine" is
      asserted structurally, not just by status code: the response body is checked to have no
      `error` key at all, since *every* error this handler stack itself produces (including C2's own
      `payload_too_large` for `json_body_bytes_max`) is `runner_auth::protocol_error`'s JSON
      envelope shape — a body without that shape proves axum's own body-extraction layer rejected
      the request before `runner_auth::authenticate` (or any handler code) ever ran. "Wrote
      nothing" is asserted directly against the database, not inferred: `execution_requests.state`
      for the queued request is still `'queued'`, and `execution_attempts` has zero rows for it — a
      completed claim would have flipped the former to `'leased'` and inserted the latter. A
      normal-sized claim against the identical fixture immediately afterward succeeds, proving the
      rejection above was genuinely about size, not a broken fixture.
    - **Direction 2 (10 MiB configured, above the 4 MiB ceiling):** a 5 MiB claim body — above the
      fixed 4 MiB ceiling, comfortably below the 10 MiB configured limit — is rejected the same way
      (genuine `413`, nothing written), proving a loose or large configured limit can never widen
      the runner-v1 surface past the protocol ceiling. A normal-sized claim against the same fixture
      still succeeds under the looser config.
  - `cargo test -p tack-api --test c5_integration_test` — **7 passed, 0 failed** (the 6 pre-existing,
    byte-for-byte unchanged, plus the one new test above).
  - `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed** — the gate file is untouched
    by this amendment (not owned, not edited) and remains independent evidence.
  - `cargo test -p tack-api` — **361 passed, 0 failed** across every suite in the crate (includes
    C2's own new unit test, described in that card's parallel amendment).
  - `cargo test --workspace` — **913 passed, 0 failed**, 2 doctests intentionally ignored
    (pre-existing, `tack-orch`).
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (`rustfmt --edition 2024` run directly against exactly the
    files this amendment and C2's parallel amendment touch; no unowned file mechanically
    reformatted, rule 10).
  - `git diff --check` — clean.
  - `cargo test -p tack-api --test openapi_contract` — **5 passed**, including
    `openapi_spec_matches_committed_file` — confirms no spec drift. This fix changes only
    Rust-internal router wiring (a function argument, a `.layer()` value) — no route, method,
    handler signature, or anything else visible to `utoipa`/OpenAPI changed, so no regeneration was
    needed or performed.

- Failure/adversarial case proved: **load-bearing revert-and-restore**, performed in C2's own owned
  file (`runner_protocol.rs`, not this card's) and reported here since it directly validates this
  card's new test: `effective_body_limit_bytes` was temporarily changed to unconditionally return
  the 4 MiB constant, discarding the configured argument — simulating the exact pre-fix behavior —
  and `runner_v1_body_limit_is_the_lesser_of_configured_and_protocol_ceiling` was rerun. It failed
  exactly as the live defect predicts: the 512 KiB body under a 2 KiB configured limit was claimed
  successfully (`200`, with a real `lease`/`attempt_id` in the response), not rejected. The fix was
  then restored and confirmed byte-identical via `diff` against a backup of `runner_protocol.rs`
  taken before the probe. This is the strongest available proof that the new test actually exercises
  the defect the verifier reported, not a tautology that would pass regardless of the fix.

- Schema/API/contract change requested from another owner: none.

- Known limitations or `not_measured` fields: none beyond what this handoff's original "Known
  limitations" section already lists. Noted for completeness, not a limitation of this fix: C2's
  parallel amendment separately documents a pre-existing, unrelated test flake
  (`logs_never_contain_raw_credentials_only_ids` in `c2_handlers_test.rs`, a `tracing`
  callsite-interest-cache race under parallel test execution) discovered while verifying this
  amendment's full-suite runs; it is not caused by, and was not fixed by, this amendment.

- Secrets/logging review: unchanged — no log line, credential, request body, or query string is
  touched by this amendment.

- Safe merge order and likely conflicts: merge together with `III-C2.md`'s parallel amendment — the
  two describe one indivisible fix, split only for file-ownership bookkeeping. `router.rs`'s call
  site depends on `runner_protocol::routes`'s new arity; landing one side without the other does not
  compile.

- Checklist: no unowned files beyond the integrator-authorized, explicitly recorded exception
  (`crates/tack-api/src/handlers/runner_protocol.rs`, owned by C2 and described in full in that
  card's own amendment); no live secret; no panic stub; no blind retry.
