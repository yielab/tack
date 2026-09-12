# IV-A2 handoff

- Base SHA / branch / final SHA: base `81e66e5` (develop tip at worktree creation), branch
  `agent/iv-a2-readiness-seam`, final SHA — see the commit this handoff ships in (last commit
  on the branch).
- Files changed (must equal ownership list): `crates/tack-api/src/server.rs`,
  `docs/agent-handoffs/part-iv/IV-A2.md`. Nothing else touched.
- Contract fixtures consumed: none. This card is entirely in-process plumbing; it never touches
  `docs/contracts/runner-v1/`, a handler, or the OpenAPI surface.
- Behavior implemented:
  - `serve()`'s public signature and behavior are unchanged. It now delegates to a private
    `serve_inner(ready_tx: Option<oneshot::Sender<SocketAddr>>)`, called with `None`.
  - New public entry point: `pub async fn serve_with_ready(ready_tx: tokio::sync::oneshot::Sender<SocketAddr>) -> anyhow::Result<()>`,
    exported from the crate root as `tack_api::serve_with_ready` (`server` is already `pub mod`,
    and the function follows the same `pub use`-free access pattern `serve` uses today — callers
    reach it as `tack_api::server::serve_with_ready` or, once `lib.rs` re-exports it, whichever
    path integration chooses; `lib.rs` was not touched by this card, see escalation below).
  - `serve_inner` sends the real bound `SocketAddr` (from `TcpListener::local_addr()`, not the
    configured host:port string) over `ready_tx` immediately after `TcpListener::bind` succeeds
    and before the `axum::serve` call that blocks until shutdown. A bound Tokio listener is
    already in the OS-level LISTEN state at that point — the kernel queues incoming connections
    even before `axum::serve`'s accept loop starts polling — so this is the earliest point at
    which "accepting connections" is actually true, not just imminent.
  - If the receiver was dropped, the `oneshot::Sender::send` returns `Err` which is discarded
    (`let _ = tx.send(...)`); the server boots and runs identically either way. No caller can
    make the send observably fail the boot.
  - Incidental fix, same file, required to make the seam usable at all: the orchestration-enabled
    branch had `state.orch_runtime.live_task_count().await` inline inside a `tracing::info!(...)`
    call. That leaves a non-`Send` formatting temporary held across an `.await`, which makes the
    whole `serve_inner` future non-`Send` — invisible under the old `runtime.block_on(serve())`
    call in `tack-cli`, but a hard compile error the moment anything tries
    `tokio::spawn(serve_with_ready(..))`, which is the exact shape ADR 0058 describes for the
    embedded-runner card ("a task in the server's own process"). Hoisted the await into a local
    binding before the macro call; log fields and values are unchanged.
- Tests added and exact commands/results:
  - `crates/tack-api/src/server.rs`, `#[cfg(test)] mod tests`:
    - `serve_with_ready_signals_the_real_bound_address` — the acceptance test. Sets
      `TACK_PORT=0` and `TACK_DATABASE_URL=sqlite::memory:` (serialized against other tests via
      a `tokio::sync::Mutex` static, restored via an RAII guard on drop so a panic can't leak env
      state into a later test), spawns `serve_with_ready` via `tokio::spawn`, `.await`s the
      oneshot with **no sleep, no retry loop**, asserts the returned port is not `0` (proving the
      OS-assigned port was actually reported, not the configured one), then issues one real
      `reqwest::get` against `http://{addr}/api/health` and asserts success.
    - `serve_still_takes_no_arguments` — pins `serve`'s call shape (`fn() -> _`) so `serve_inner`'s
      `None` path can't silently grow a parameter that would break `tack-cli`'s existing call site.
  - `cargo test -p tack-api --lib server::` → `test result: ok. 5 passed; 0 failed`.
  - `cargo test -p tack-api` (full crate, all integration test binaries) → every binary reports
    `test result: ok`, 0 failed across all of them (104 in `api_test`, 44 in `c1_handlers_test`,
    etc. — no test in the crate regressed).
  - `cargo test -p tack-api --test openapi_contract` → `5 passed; 0 failed`, including
    `openapi_spec_matches_committed_file` — run plain, without `UPDATE_OPENAPI=1`, proving zero
    spec drift.
  - `cargo clippy -p tack-api --all-targets -- -D warnings` → clean.
  - `cargo fmt -p tack-api` → no diff produced.
  - `cargo build --workspace` → clean, confirms `tack-cli` (and `tack-runner`, sibling card
    IV-A1's crate) still compile against the changed `server.rs`.
  - Manual verification per the card: `TACK_PORT=39217 TACK_DATABASE_URL="sqlite::memory:"
    cargo run -p tack-cli -- serve` in a scratch directory — printed the same startup banner as
    before, logged `Server listening addr=127.0.0.1:39217`, and `GET /api/health` returned
    `{"status":"ok",...}`. Confirms the old `serve()` call path in
    `crates/tack-cli/src/main.rs::run_server` is byte-for-byte unchanged in behavior.
- Failure/adversarial case proved: the acceptance test's port-0 assertion is the load-bearing
  one — if the seam signaled the *configured* address instead of the real bound one (the bug
  this card exists to prevent), `addr.port()` would read back `0` and the test would fail before
  ever making the HTTP request. Also manually confirmed the test is unconditionally load-bearing
  by temporarily reverting the `TcpListener::local_addr()` capture to reuse the pre-bind `addr`
  variable instead — the test then failed on `assert_ne!(addr.port(), 0, ...)` as expected, then
  reverted back.
- Schema/API/contract change requested from another owner: one soft escalation, not a schema
  change. `serve_with_ready` is reachable today as `tack_api::server::serve_with_ready` (the
  `server` module is `pub mod server;` in `lib.rs`) but is **not** re-exported at the crate root
  next to `pub use server::serve;`. I deliberately did not touch `lib.rs` — it's outside this
  card's ownership list and a one-line addition is a judgment call for whoever wires the actual
  embedder (IV-A6 per the dependency graph in `TODO.md`), not for this card to make unilaterally.
  Flagging it so that card doesn't have to rediscover it: add `pub use server::serve_with_ready;`
  to `crates/tack-api/src/lib.rs` when wiring the embedder, or call the fully-qualified path.
- Known limitations or `not_measured` fields:
  - Binary-size delta: **not applicable to this card.** Nothing new links into the `tack` binary
    — `serve_with_ready` has no caller yet (that's a later card's job per the ADR's own sequencing).
  - Which role executed what: **not applicable.** No live attempt, no runner, nothing executed.
  - Loopback/gating proof: **not applicable.** This card adds no new gate, no new default-off
    flag, and no non-loopback refusal — `AppConfig::validate_security`/`binds_loopback` are
    untouched. The safety posture ADR 0058 describes belongs to the embedder card that consumes
    this seam, not to the seam itself.
- Secrets/logging review: no new logging added. The one `tracing::info!` touched by the `Send`
  fix logs the same two fields it always did (`control_planes`, `poll_secs`), just with the
  first one's value now read into a local before the macro call instead of inline — no new field,
  no credential, no token, no env value anywhere near it. The readiness channel carries only a
  `SocketAddr`.
- Safe merge order and likely conflicts: no conflicts expected. This card's only production diff
  is inside `crates/tack-api/src/server.rs`, a file no other in-flight card (IV-A1 is
  `crates/tack-runner/**`, a separate crate; V-B1 is `crates/tack-api/src/middleware.rs`, a
  different file in the same crate) touches. Safe to land in any order relative to those two.
- Checklist: no unowned files touched (only `server.rs` and this handoff) — no live secret
  introduced or logged — no panic stub (`serve_with_ready`'s only new fallible-looking line,
  `listener.local_addr()?`, propagates via `?` like every other call in this function; the
  `oneshot::send` explicitly swallows its `Result` rather than unwrapping) — no blind retry
  anywhere in the new code or its test.
