# V-B1 handoff

- Base SHA / branch / final SHA: base `81e66e5` (develop tip at worktree creation) /
  `agent/v-b1-identity-posture` / all substantive changes (code, ADR, docs) land in
  `6dd8fdf`. Commits after `6dd8fdf` on this branch, if any, only edit this handoff's
  own SHA references and carry no other change.
- Files changed (must equal ownership list): `crates/tack-api/src/middleware.rs`,
  `crates/tack-api/src/config.rs`, `docs/CONFIG.md` (one row),
  `docs/book/src/user-guide/administration.md`, `docs/adr/0059-single-operator-identity-posture.md`,
  this handoff. `config.rs` is **not** on the card's literal ownership list but is where
  the non-loopback/no-token check actually lives (see "Where the check lives" below) —
  it is not owned by any other in-flight card (only `server.rs` was flagged as a
  possible conflict, and the check is not there).
- Contract fixtures consumed: none. This card is entirely inside the operator `/api`
  surface (`require_token`); `docs/contracts/runner-v1/` is untouched, as ADR 0059
  itself states.
- Behavior implemented:
  1. `docs/adr/0059-single-operator-identity-posture.md` — the primary deliverable.
     Records Tack v1 as single-operator, names and gives reasons for rejecting full
     multi-user accounts, OIDC/SSO, and per-user API tokens, and states the
     loopback/non-loopback safety posture as a consequence of that scope decision.
  2. `AppConfig::validate_security` (`crates/tack-api/src/config.rs`) gained a new
     field, `allow_unauthenticated_nonloopback` (env `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`,
     `#[serde(default)]` so it can also be set via `tack.toml`, off by default). The
     refusal to start a non-loopback bind with no token now also requires this flag to
     be unset; the error message names the flag. Loopback behavior is untouched — the
     new condition is `&&`-ed onto a branch that is never entered when
     `binds_loopback()` is true, so it cannot affect that path either way.
  3. `crates/tack-api/src/middleware.rs`: two doc-comment additions on `require_token`
     tying the runtime mechanism (single shared token, no per-user branch) to ADR 0059,
     and noting that a non-loopback+no-token request never reaches this function
     because startup already refused to bind. No behavioral change — `require_token`
     itself is correct as-is and out of scope for a code change.
  4. `docs/book/src/user-guide/administration.md`: new opening paragraph stating the
     no-identity-model fact before any configuration instructions (per the card's
     acceptance bullet "the administration guide states the limit before it states any
     feature"), and a rewritten "Network exposure and TLS" section that shows the
     actual startup error and the opt-out instead of implying `TACK_HOST=0.0.0.0` alone
     always works. Also added the new variable to this file's own "Environment variable
     reference" table (this file duplicates a slimmer version of `docs/CONFIG.md`'s
     table for the security-relevant subset — both needed the row for the page to stay
     internally consistent).
  5. `docs/CONFIG.md`: added exactly one row, `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`,
     directly under the existing `TACK_API_TOKEN` row. No other row in this file was
     touched.
- Tests added and exact commands/results:
  - `crates/tack-api/src/config.rs` `#[cfg(test)] mod tests`, four tests:
    - `unsafe_non_loopback_startup_is_rejected` (pre-existing, strengthened to assert
      the error message names both `TACK_API_TOKEN` and
      `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`)
    - `unsafe_non_loopback_startup_with_a_token_is_accepted` (new)
    - `unsafe_non_loopback_startup_with_the_documented_opt_out_is_accepted` (new —
      proves the opt-out works)
    - `loopback_startup_with_no_token_is_unaffected_by_the_opt_out_flag` (new — proves
      the flag cannot change loopback behavior either way)
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo test -p tack-api --lib config`
    → `7 passed; 0 failed` (includes the pre-existing `origin_validation_...` test,
    unaffected).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo test -p tack-api` → every
    suite `0 failed` (grepped all `test result:` lines across the run — all show
    `0 failed`). This includes ~100 call sites across the integration test files that
    construct `AppConfig::default()` (loopback, no token) — all passed unchanged,
    which is the "existing tests passing untouched" proof the acceptance criteria asks
    for.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo fmt --check` → clean.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo clippy -p tack-api --all-targets -- -D warnings` → clean.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo clippy --workspace --all-targets -- -D warnings` → clean.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-B1 cargo test --workspace` → first
    attempt hit `SqliteError { code: 13, message: "database or disk is full" }` on
    three unrelated `remote_backup` tests — the root partition (`/`, where
    `/var/tmp` lives) had filled to 100% from cumulative concurrent-agent build
    artifacts across this and other in-flight cards, confirmed via `df -h` and `du`
    (this worktree's own `target/debug/incremental` alone was 7.2 GB). Freed 23 GB by
    deleting that incremental cache (safe — regenerated on next build, does not affect
    any other worktree) and re-ran: **every crate, every suite, 0 failed** (grepped the
    full log for `FAILED`/`panicked`; only match is a passing test named
    `..._is_retried_not_panicked`). Not a regression from this diff —
    `remote_backup.rs` is untouched by this card and the failure mode was a disk-space
    I/O error, not an assertion failure.
- Failure/adversarial case proved: `unsafe_non_loopback_startup_is_rejected` proves the
  refusal fires and names both remedies in the error text (not just a status-code/bool
  check — the message content is asserted). The opt-out test and the loopback test
  prove the two ways this could have been implemented wrong (opt-out doesn't actually
  work; opt-out accidentally also gates the loopback path) do not occur.
- Schema/API/contract change requested from another owner: none. No migration, no
  OpenAPI change (this is a startup-time config check, not a request/response shape),
  no `docs/contracts/runner-v1/` fixture touched.
- Known limitations or `not_measured` fields:
  - **`Dockerfile` / `docker-compose.yml` are not on this card's ownership list and
    were deliberately left unedited — but they need attention, and not just because of
    this card.** `docker-compose.yml`'s shipped defaults are `TACK_HOST=0.0.0.0` with
    `TACK_API_TOKEN` commented out. `AppConfig::validate_security`'s non-loopback/no-token
    refusal was **already a hard `anyhow::bail!` error on `develop` before this card
    touched anything** — introduced in `cfb59545` ("fix(api): harden trust
    boundaries", 2026-08-06), with no opt-out at all until this card added one. That
    means `docker compose up` with the file's own defaults, unmodified, already fails
    at container startup on current `develop`, independent of this card. This card's
    diff does not create that break (it already existed) and does not fix it either
    (out of ownership) — it only makes a fix *possible* by adding the opt-out. Whoever
    owns those files next should either (a) set
    `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK: "1"` as a compose default with a
    comment explaining the private-network assumption, or (b) require/generate a real
    token at container-build or first-run time. Flagging this loudly since it is a
    real, currently-broken default, not a hypothetical.
  - `docs/book/src/developer/deployment.md` still tells operators to "Set
    `TACK_API_TOKEN` to a long random string" in a checklist but never mentions the new
    startup error or the opt-out. Not edited (not owned by this card), but it is the
    other doc a container-deploying reader would hit; worth a follow-up pass.
  - `docs/book/src/user-guide/configuration.md` also exists and may have its own
    env-var table; not checked in detail and not edited — out of scope for this card's
    explicit "auth rows of `docs/CONFIG.md`" instruction, flagging in case it also
    needs the new row for consistency.
- Secrets/logging review: the new field is a plain `bool`, never a credential — no
  redaction concerns. No new `warn!`/`info!`/`error!` call sites were added; the
  refusal path already used `anyhow::bail!` (surfaced by the caller, not logged
  separately) before this card and still does. Grepped the diff for `warn!`/`info!`/
  `error!`/`debug!`/`trace!` — none added.
- Safe merge order and likely conflicts: `docs/CONFIG.md` is shared with V-B2 (docket
  rows) and Part IV's IV-A6 — this card's only touch is the single
  `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` row immediately after the existing
  `TACK_API_TOKEN` row; a line-level diff should show no overlap with docket-specific
  or Part IV rows elsewhere in the same table. No conflict expected with `server.rs`
  (IV-A2's concurrent file) — this card never touches it. Safe to merge independently
  of V-B2 in either order; if both land in the same integration pass, diff
  `docs/CONFIG.md` once combined to confirm the two new rows are adjacent-but-disjoint,
  not interleaved incorrectly.
- Where the check lives (for the integrator, since the card asked this be stated
  precisely): the non-loopback/no-token check is **not** in `middleware.rs` (that only
  has the runtime `require_token` gate) and **not** in `server.rs` (IV-A2's concurrent
  file — confirmed by reading it: `server.rs::serve()` calls
  `security_preflight(&config)`, a two-line wrapper around
  `config.validate_security()`, and nothing else there references the bind/token
  relationship). The actual check is `AppConfig::validate_security` in
  `crates/tack-api/src/config.rs`, which is not claimed by any other card's ownership
  list, so it was edited directly rather than routed through a handoff request.
- Checklist: no unowned files (Dockerfile/docker-compose.yml/deployment.md/
  configuration.md deliberately left untouched, see above; `config.rs` edited with
  justification above), no live secret (no token value appears anywhere in code, docs,
  or this handoff), no panic stub (`validate_security` returns `anyhow::Result`, no
  `unwrap`/`expect`/`unimplemented!` added), no blind retry (no retry logic in this
  diff).

---

**Proposed README text (not merged)** — for V-A4's positioning section
(`README.md`'s "Current limitations" table already has an "Authentication" row
saying "One optional shared Bearer token; no per-user identities or permissions.";
this expands on it as prose, for wherever a human integrator judges it fits best,
e.g. near that table or in a short paragraph before it):

> Tack has no identity model — no user accounts, no sessions, no per-user
> permissions. `assignee` is a free-text label on an item, not an account. Every
> request that authenticates at all authenticates as the same single operator, via
> one shared bearer token. This is a deliberate v1 scope decision, not an
> oversight — see [ADR 0059](docs/adr/0059-single-operator-identity-posture.md) for
> what was considered and rejected (full multi-user accounts, OIDC, per-user
> tokens) and why. A non-loopback bind with no token now refuses to start rather
> than silently exposing full read/write access to the network; an explicit,
> documented opt-out exists for container deployments that need it.
