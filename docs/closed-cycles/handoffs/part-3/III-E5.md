# III-E5 handoff

- **Base SHA / branch / final SHA:** base `b6dd0370564a3a4461b05d98d51d9e77c6d231c0`
  ("docs: bring CLAUDE.md up to date with the runner fleet", tip of
  `plan/harness-agnostic-agent-fleet` / accepted Wave 3 integration SHA
  `6a53a18`) on branch `agent/iii-e5-cli-mcp`, worked in an isolated worktree
  at `/tmp/tack-iii-e5`. Implementation commit: `b8da750` ("feat(cli): add
  execution and fleet commands to CLI and MCP" — all code/tests/docs listed
  below). This handoff is added in a follow-up commit on top of it, since a
  commit cannot reference its own hash; that follow-up commit is the actual
  branch tip.
- **Files changed (must equal ownership list):**
  - New: `crates/tack-cli/src/execution.rs` (request-body builders +
    display helpers shared by the CLI and MCP), `crates/tack-cli/src/secure_fs.rs`
    (atomic owner-only file writer).
  - Modified: `crates/tack-cli/src/main.rs` (five new subcommand groups:
    `execution`, `fleet`, `runner`, `agent-profile`, `model-profile`),
    `crates/tack-cli/src/mcp.rs` (seven new MCP tools), `crates/tack-cli/src/client.rs`
    (`error_msg` now also decodes the runner-v1 protocol error envelope),
    `crates/tack-cli/src/config.rs` (`save` now writes `~/.tackrc` through
    `secure_fs::write_owner_only_atomic` instead of a bare `fs::write` —
    see "Secrets/logging review" below for why this was in scope),
    `crates/tack-cli/src/lib.rs` (module registration),
    `crates/tack-cli/tests/cli_test.rs` (one assertion added to the existing
    `config_save_and_reload` test, proving the real `~/.tackrc` path is
    owner-only end-to-end, not just the isolated `secure_fs` unit tests),
    `crates/tack-cli/Cargo.toml` (added `tack-orch` as a **dev-dependency
    only**, to import the real nested snapshot types in tests — see below),
    `docs/MCP.md` (documents the 7 new tools and which admin/secret-bearing
    actions were deliberately kept CLI-only).
  - Mechanical: `Cargo.lock` (+1 line — `tack-orch` added to `tack-cli`'s
    dependency list; no new crate entered the lockfile since `tack-orch` was
    already a resolved workspace member).
  - `git status --porcelain` on this branch shows exactly this set. Nothing
    under `crates/tack-api/**`, `crates/tack-db/**`, `crates/tack-orch/src/**`
    (non-test), `docs/openapi.json`, `frontend/**`, `TODO.md`, or any other
    handoff was touched.

## Contract fixtures consumed

None directly. `docs/contracts/runner-v1/**` is the frozen authority for the
**runner** wire protocol (`/api/runner/v1/*`) — a different domain from the
**operator** execution/fleet/runner/profile API this card targets
(`/api/executions`, `/api/runner-fleets`, `/api/runners/*`,
`/api/agent-profiles`, `/api/model-profiles`), per III.0's "Vocabulary that
must remain distinct." That operator surface has no fixture directory of its
own; its shape authority is the request structs the C1 handlers themselves
deserialize (`tack_api::handlers::executions::{CreateExecution,
RecoveryConfirmation}`, `tack_api::handlers::runner_admin::{CreateFleet,
CreateProfile, CreateModelProfile, CreatePendingRunner}`) and, one level
deeper, `tack_orch::execution::{AgentProfileSnapshot, RepositorySnapshot,
PermissionPolicy}`. `execution.rs`'s module doc explains this explicitly so
nobody mistakes a hand-written CLI/MCP DTO for a second authority. The one
shared, genuinely frozen piece this card does depend on is `StableErrorCode`
and `ProtocolErrorEnvelope` (`tack_orch::execution::types`) — `client.rs`'s
`error_msg` decodes that exact envelope shape.

## Behavior implemented

**CLI** — five new subcommand groups, following the existing `tack <noun>
<verb>` convention (`Sprint`, `Role`, `Template`, `Field`):

- `tack execution create|list|get|cancel|reconcile` — maps to
  `POST/GET /executions`, `GET /executions/{id}`, `POST
  /executions/{id}/cancel`, `POST /executions/{id}/requeue`.
- `tack fleet create|list` — `POST/GET /runner-fleets`.
- `tack runner enroll|revoke|revoke-token` — `POST /runners/enrollment`,
  `POST /runners/{id}/revoke`, `POST
  /runners/{id}/enrollment-tokens/{token_id}/revoke`.
- `tack agent-profile create|list` — `POST/GET /agent-profiles`.
- `tack model-profile create|list` — `POST/GET /model-profiles`.

No `tack runner list` — **there is no backend route for it**; see "Schema
change requested" below.

**MCP** — 7 new tools (`list_fleets`, `list_agent_profiles`,
`list_model_profiles`, `list_executions`, `get_execution`,
`cancel_execution`, `create_execution`), bringing the server from 8 to 15
tools. Deliberately a *subset* of the CLI surface: `runner enroll`/`revoke`,
`fleet create`, `agent-profile create`, `model-profile create`, and
`execution reconcile` are CLI-only. Rationale (also in `docs/MCP.md` and a
dispatch-time comment in `mcp.rs`):
  - `runner enroll` returns a one-time secret (the raw enrollment token).
    Keeping it off the MCP tool surface means that secret can never appear
    in an MCP client's tool-call transcript/log — a stronger property than
    "not in argv," but in the same spirit as this card's "avoid enrollment
    secrets ever appearing in process arguments" requirement, and consistent
    with `tack-runner`'s own credential-redaction bar.
  - `execution reconcile` is an operator's explicit, audited recovery
    decision after reviewing an ambiguous `needs_operator` state (III.1.1:
    "Operator: explicitly requeue/abandon a `needs_operator` request with an
    audit event") — not something that should be triggerable by an agent's
    own inference.
  - Fleet/profile/runner *creation* is admin-ish setup, in the same spirit
    as this server's existing precedent of keeping `backup`/`restore`/
    `template`/`role`/`field` management off the agent-facing tool surface.

**Shared shape guarantee (the acceptance criterion "no divergent DTO"):**
Every write body is assembled by one function in `execution.rs`, called by
both `main.rs` and `mcp.rs`:
  - `create_execution_body(selector, CreateExecutionValues)` is the single
    place that builds the `POST /executions` JSON object. The CLI's
    `build_create_execution_body` parses argv strings into
    `CreateExecutionValues` and calls it; MCP's `create_execution` handler
    extracts native JSON-RPC values into the same struct and calls it
    directly (no stringify/parse round trip needed, since MCP args are
    already `serde_json::Value`). Neither entry point can build a
    differently-shaped request for the same operation because there is
    only one function that builds it.
  - `build_create_fleet_body`/`build_create_agent_profile_body`/
    `build_create_model_profile_body`/`build_enroll_runner_body`/
    `build_requeue_body` are the equivalent single builders for their
    routes (CLI-only today, since those operations aren't MCP tools, but
    structured the same way for when/if that changes).
  - Every builder is tested by deserializing its output into the **real**
    backend request struct (`tack_api::handlers::executions::CreateExecution`,
    `tack_api::handlers::runner_admin::{CreateFleet, CreateProfile,
    CreateModelProfile, CreatePendingRunner}`, `RecoveryConfirmation`) —
    `tack-cli` already depends on `tack-api` (to run `tack serve`
    in-process), so this import was free. A renamed/dropped field fails a
    test in this crate instead of only surfacing as a live 400 later.

**Conditional writes:** GET `/executions/{id}` and the fleet/profile list
routes send no `ETag` (verified by reading the handlers directly — no
`ETag`/`If-Match` support exists on this operator surface yet), so this
card cannot honestly claim to use `If-Match` there; using it anyway would
be exactly the kind of capability-not-verified claim III.2 rule 7
forbids. The equivalent safety property this surface *does* provide is
idempotency-key-based conditional creation, which the backend already
implements (`idempotency_key` on `execution create`, `recovery_key` on
`execution reconcile`) — both are exposed as CLI/MCP arguments with a
freshly-generated default so a caller can opt into safe retry by supplying
a stable value, matching "conditional writes ... or an equivalent already
used elsewhere in this client."

**Distinct outcomes (acceptance: conflicts and `needs_operator` are not
collapsed into one generic error):**
  - `client.rs::error_msg` previously only handled `{"error": "text"}` /
    `{"message": "text"}`. The execution/fleet/runner/profile routes answer
    errors with `{"error": {"code": ..., "message": ..., "details": ...}}`
    — `error` is an *object* there — which the old code silently downgraded
    to the generic string `"server error"` (`Value::as_str()` on an object
    returns `None`, falling through every branch). Fixed by trying the
    legacy string shapes first (byte-identical behavior for every existing
    command — pinned by `error_msg_reads_the_plain_string_shape_unchanged`/
    `..._message_key_shape_unchanged`), and only when `error` is an object,
    surfacing `"{code}: {message}"` — e.g. `409 Conflict:
    idempotency_conflict: The idempotency key was used with a different
    request` instead of `409 Conflict: server error`. Verified against a
    live server (see "Failure/adversarial case proved").
  - `execution.rs::describe_state` annotates `needs_operator`/`lost` with a
    visible marker in `execution list`'s table and `execution get`'s
    detail view; `execution get` additionally prints operator guidance
    (what `needs_operator` means, and the exact `execution reconcile`
    invocation) — nothing else in the lifecycle gets that treatment.
  - `execution cancel`'s output explicitly says the request is **not**
    terminal (the server's response literally returns
    `"state":"cancellation_requested"`, which is not one of the frozen
    III.1.1 lifecycle states — it is the handler's own acknowledgment
    label, since "cancellation is recorded as a request only"). Printing
    that string as if it were a lifecycle state would misrepresent the
    system's own honesty-about-delivery-semantics stance (III.0).

**Credential handling:**
  - `secure_fs::write_owner_only_atomic` (new) writes via a sibling temp
    file created with mode `0600` from the start, `fsync`s it, `rename`s it
    over the target, then re-asserts `0600` — mirroring
    `tack-runner/src/journal.rs`'s `atomic_create_private`/
    `atomic_replace_private` pattern rather than inventing a second one.
  - `tack runner enroll --out <path>` writes the enrollment response
    (including the one-time raw token) through this function instead of
    printing the token to stdout. Without `--out`, the token prints once
    with an explicit "shown once, cannot be retrieved again" notice —
    matching the backend's own stated intent ("The raw token is
    deliberately emitted once here").
  - `config::save` (`~/.tackrc`, which can carry `TACK_API_TOKEN`) was
    switched from a bare `fs::write` to the same `secure_fs` function. This
    file predates this card and is arguably outside "execution additions,"
    but it is the CLI's other on-disk credential, sits directly next to the
    new enrollment-token writer, and the acceptance criterion says *any*
    credential/config file this CLI writes — leaving it on the old,
    non-atomic, umask-dependent path right next to the new hardened one
    would be an inconsistent bar for the same property in the same binary.
    Recorded here rather than left silent.
  - No enrollment secret is ever a CLI/MCP **input** — the raw token is
    generated server-side and only ever appears in the response, so there
    is no code path by which it could reach `argv`/`ps` on the way in. This
    was verified structurally (there is no `--token`-shaped flag on
    `runner enroll`), not just asserted.

## Tests added and exact commands/results

`cargo test -p tack-cli` — **68 passed, 0 failed** (57 lib + 11 existing
integration in `tests/cli_test.rs`, one of which gained a new assertion):

- `crates/tack-cli/src/execution.rs` (17 tests): every body builder proven
  against the real backend struct via `serde_json::from_value`, including a
  test one level deeper (`create_execution_nested_blobs_satisfy_the_deeper_snapshot_types`)
  that deserializes the module's example JSON into the actual
  `tack_orch::execution::{AgentProfileSnapshot, RepositorySnapshot,
  PermissionPolicy}` types — the exact check that would have caught the bug
  described below before it reached a live server. Also: selector
  mutual-exclusivity, invalid-JSON error messages name the flag, the
  enrollment-request body is asserted to contain no `token`/`credential`-
  named field, and `describe_state` distinctness.
- `crates/tack-cli/src/secure_fs.rs` (4 tests): full-content round trip,
  overwrite leaves no `.tmp` file behind, file ends at `0600`, and —
  specifically pinning the property a plain `fs::write` would not have —
  overwriting a pre-existing `0644` file still ends at `0600` (proven via
  `overwriting_a_looser_permission_file_still_ends_owner_only`).
- `crates/tack-cli/src/client.rs` (+6 tests): `error_msg`'s new
  object-shape branch, that the two legacy shapes are byte-identical to
  before, that three different codes (`conflict`/`stale_lease`/
  `invalid_transition`) produce three different messages, and one
  `wiremock`-backed test that a real `409` response with the protocol
  envelope surfaces the `code` through `TackClient::post`'s public API, not
  just the internal helper in isolation.
- `crates/tack-cli/src/mcp.rs` (+9 tests): `create_execution` rejects a
  missing `agent_profile_snapshot` and a wrong `runner_id`/`fleet_id`
  combination before any network call (existing convention); a
  `wiremock`-backed test asserts the POST body via
  `body_partial_json` — proving the object-typed JSON blobs an MCP caller
  supplies reach the wire as objects, not double-encoded as strings;
  `list_executions` unwraps the `{data:[...]}` envelope into
  `{"executions":[...], "count": n}`; `cancel_execution` hits the right
  route; `tools_list_advertises_all_fifteen` pins both the new tool names
  *and* the deliberate absence of the six CLI-only actions.
- `crates/tack-cli/tests/cli_test.rs`: `config_save_and_reload` (existing)
  gained an assertion that the real `~/.tackrc` file is `0600` after
  `config::save`, proving the `secure_fs` wiring end-to-end rather than
  only in isolation.

`cargo build --workspace` — clean.
`cargo test -p tack-orch --test runner_contract` — 18/18 (B4's fixture pin
unaffected — this card added no dependency edge into `tack-orch`'s
production code, only a test-only import from `tack-cli`).
`cargo clippy -p tack-cli --all-targets -- -D warnings` — clean (after
boxing `ExecutionCreateArgs` behind the flattened `Box<...>` pattern
clippy's `large_enum_variant` asked for on `ExecutionAction::Create`).
`rustfmt --edition 2024 --check` on every file this card touched — clean.

## Failure/adversarial case proved

**Live smoke test against a real running server** (`tack serve` against a
throwaway SQLite file, `TACK_ORCH_ENABLE` unset), not just mocks:

1. `execution create` with an empty-object default for `--permission-policy`/
   `--agent-profile-snapshot` failed server-side with `missing field
   \`network\`` — the server's `ExecutionRequestSnapshot` validates these
   blobs against typed nested structs (`PermissionPolicy` requires
   `network`; `AgentProfileSnapshot` requires `name`/`instructions`/
   `tool_policy`/`timeout_seconds`/`budgets`), a level deeper than
   `CreateExecution`'s own untyped `Value` fields, which accept `{}` for
   anything. **This card's first draft had wrongly defaulted both flags to
   `{}`** — a default guaranteed to fail, which just moves the error one
   step later instead of catching it at the CLI. Fixed by making both
   flags required (no default), with `--help` text stating their minimum
   required sub-fields, and pinned by
   `create_execution_requires_agent_profile_snapshot_and_permission_policy`
   plus the deeper-type test named above. `budgets`/`environment`/
   `metadata` genuinely can default to `{}` (untyped `Value` fields
   end-to-end) and were left alone.
2. Idempotency replay and conflict, both against the real server: same
   `--idempotency-key` twice → `Replayed existing execution request`
   (`replayed: true`); same key with a changed harness → `409 Conflict:
   idempotency_conflict: The idempotency key was used with a different
   request` (readable, not `409: server error`).
3. `execution reconcile` against a request that was never `needs_operator`
   → `409 Conflict: invalid_transition: Only authoritatively recovered
   needs_operator attempts may be requeued` — a different, correctly
   distinct message from the idempotency-conflict case above, both readable
   without `--json`.
4. `execution cancel` → explicit "not yet terminal" wording, confirmed via a
   follow-up `execution get` showing `cancellation_requested_at` set while
   `state` remained `queued` — i.e. cancellation really is request-only, and
   the CLI's own claim about that was checked against the live state, not
   assumed.
5. `runner enroll --out /tmp/....json`: confirmed with `stat -c '%a'` that
   the file is `600`, that the raw token appears in the file but **not** on
   stdout, and that a second `runner enroll` call without `--out` does
   print the token (proving the branch, not just its absence).

## Schema/API/contract change requested from another owner

Two genuine gaps on the operator API, found by reading
`crates/tack-api/src/handlers/{executions,runner_admin}.rs` directly (not
inferred) and corroborated by C1's own handoff ("List/detail DTOs are
card-local... Fleet membership is represented by B2's table but awaits a
dedicated membership route in the C5 integration shape"):

1. **No `GET` route lists runners.** `/api/runners/*` only has
   `POST /runners/enrollment`, `POST /runners/{id}/revoke`, and
   `POST /runners/{id}/enrollment-tokens/{token_id}/revoke` — there is no
   way for an operator (CLI, MCP, or a future UI) to list existing runners,
   their capacity, health, or fleet membership. This card's `tack runner`
   subcommand therefore has `enroll`/`revoke`/`revoke-token` but no `list`,
   and no MCP `list_runners` tool exists either — both would be
   straightforward once a route exists. Smallest fix: a
   `GET /api/runners` (optionally `?fleet_id=`) route returning the same
   shape `list_fleets`/`list_agent_profiles` already use
   (`{"protocol_version":1,"data":[...]}`), or embedding a `runners` array
   in each `GET /runner-fleets` row.
2. **No operator-facing read path for execution attempts or events.**
   `execution_attempts`/`execution_events` are written by the runner
   protocol (`runner_protocol.rs`) but nothing under `/api/executions/*`
   returns them — `GET /executions/{id}` returns only
   `{request_id, item_id, state, cancellation_requested_at, created_at}`.
   This card's Tasks line ("inspect attempts/events for a request") has no
   route to call. Do not confuse this with the unrelated, already-shipped
   `GET /items/{id}/agent-activity` (`handlers/orch.rs`) — that is the
   legacy Docket/`orch_events` domain, a deliberately distinct vocabulary
   per III.0, not the runner-v1 execution domain this card targets; using
   it would have been exactly the kind of vocabulary collision III.0 warns
   against. Smallest fix: a `GET /executions/{id}/attempts` (and/or
   `/attempts/{n}/events`) route backed by new `tack-db` repository
   methods (none currently exist for reading, only writing, these tables).

Both are wiring gaps in already-shipped C1 code, not something introduced by
this card, and both are outside this card's `Must not
edit: backend/router/OpenAPI` boundary. Whoever owns the next backend touch
on this surface (E6 is the only Wave-4 card authorized to touch
`router.rs`/`openapi.rs` after C5) should pick these up; this handoff is the
record of the request.

## Known limitations or `not_measured` fields

- `tack runner list` and any `list_runners` MCP tool are absent, not
  stubbed — see above. No `unimplemented!()`, no empty-array fake success.
- Attempt/event inspection (`execution.rs`'s original task) is entirely
  absent for the same reason — there is nothing to build a CLI/MCP surface
  on top of yet.
- `execution create`'s conditional-write story is idempotency-key-based,
  not `If-Match`/`ETag`-based, because the routes send no `ETag` today.
  This is stated as fact (verified by reading the handler), not implied by
  omission.
- `revoke_runner`'s repository method appears to report success (`changed:
  true`) even when called twice against an already-revoked runner (observed
  live: two consecutive `tack runner revoke <id>` calls both printed
  "Revoked runner"). This is backend behavior this card did not write and
  must not edit; noted here only because it briefly looked like a
  CLI-side bug during smoke testing and a future adversarial reviewer might
  rediscover it. Not fixed, not worked around, not hidden.
- MCP tool coverage is a deliberate subset (see "Behavior implemented"). If
  a later card decides an agent *should* be able to enroll runners or
  reconcile `needs_operator` requests, that is a scope decision for that
  card to make explicitly, not something this handoff silently leaves room
  for.

## Secrets/logging review

- The one CLI-issued secret (the runner enrollment token) is never a
  request **input** on any path this card added — verified structurally,
  not just tested — and is written to disk (when `--out` is given) only
  through `secure_fs::write_owner_only_atomic` (`0600`, atomic
  rename-into-place). Printed to stdout at most once, with an explicit
  "cannot be retrieved again" notice, when `--out` is not given — this is
  the intended, documented one-time reveal (the backend's own comment:
  "deliberately emitted once here"), not a leak.
- `~/.tackrc`'s `TACK_API_TOKEN` now goes through the same atomic
  owner-only path (previously a bare `fs::write`, relying on umask alone).
- No new `tracing`/log call was added anywhere in this card — the CLI has
  no logging framework wired up; all output is either the deliberate
  one-time-secret stdout print described above or ordinary non-secret
  operational text (ids, states, counts).
- MCP tool arguments/results: none of the 7 new tools accept or return a
  credential field. `create_execution`'s `agent_profile_snapshot`/
  `repository`/`permission_policy`/`environment`/`metadata` are
  caller-supplied structured data (instructions, tool policy, repo
  reference, network policy) — the same fields the operator API itself
  accepts from any client — not a place a raw vendor credential belongs;
  `environment` in particular is documented server-side
  (`tack_orch::execution::EnvironmentValue`) as "a value or a secret
  reference, never a raw runner credential," which this card does not
  change or attempt to enforce further (that's the backend's job, one
  layer down).

## Safe merge order and likely conflicts

- No overlap with E1 (`crates/tack-orch/src/scheduler/**`), E2/E3/E4
  (`frontend/**`), or the backend/router/OpenAPI files reserved for E6.
  This card's only cross-crate touch is a **dev-dependency** edge
  (`tack-cli` → `tack-orch`, test-only) and the resulting one-line
  `Cargo.lock` change; that edge already existed transitively (`tack-cli`
  → `tack-api` → `tack-orch`), so it should merge without conflict against
  any other Wave-4 branch that doesn't also touch `crates/tack-cli/Cargo.toml`.
- `docs/MCP.md` was last touched by the original Phase 20 MCP work, well
  before Part III; no Wave-4 card besides this one should be editing it, so
  no conflict expected there either.
- If E6's later route/spec work adds the two missing routes requested
  above, this card's `tack runner list` / attempts-events gap is the
  natural small follow-up — flagged here so it isn't rediscovered from
  scratch.

## Checklist

- [x] No unowned files touched (`git status --porcelain` matches the
      ownership list above exactly).
- [x] No live secret committed, logged, or reachable via `argv`/`ps`.
- [x] No panic stub / `unimplemented!()` — the two backend gaps are absent
      commands, not fake ones.
- [x] No blind retry — `error_msg` surfaces the stable `code` precisely so
      a caller (human or agent) can distinguish `idempotency_conflict`
      (do not resend unchanged) from `conflict`/`internal_error`
      (retryable per `StableErrorCode::retryable`) instead of retrying
      everything the same way.
