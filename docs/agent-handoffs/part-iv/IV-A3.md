# IV-A3 handoff

- Base SHA / branch / final SHA: base `2cc73b22c52c749191df4cad685dba2b95634c5d` (develop tip at
  worktree creation, includes IV-A1 and IV-A2), branch `agent/iv-a3-single-binary`, final SHA —
  see the last commit on this branch.
- Files changed (must equal ownership list): `crates/tack-cli/src/main.rs`,
  `crates/tack-cli/Cargo.toml`, `Cargo.lock`, `crates/tack-cli/src/local_runner.rs` (new),
  `docs/agent-handoffs/part-iv/IV-A3.md`. Nothing else touched — `tack-api` and `tack-runner`
  internals are untouched, as owned.
- Contract fixtures consumed: none. This card wires two existing composition roots
  (`tack_runner::bootstrap`, `tack_api::serve_with_ready`) together; it never touches a handler,
  the OpenAPI surface, or `docs/contracts/runner-v1/`.
- Behavior implemented:
  - `tack runner start` (new `RunnerAction::Start` variant, flags mirroring
    `crates/tack-runner/src/main.rs`'s own CLI exactly: `--config`, `--api-url`, `--runner-id`,
    `--state-dir`, `--enrollment-token`) loads `RunnerConfig` via a new `local_runner::
    load_runner_config` helper that calls `RunnerConfig::from_sources`/`RunnerConfig::
    environment_overrides` — the same types and the same file → environment → command-line
    precedence the standalone binary uses, not a re-parse. It then runs `tack_runner::bootstrap::
    run` in its own Tokio runtime with the same ctrl-c-to-`ShutdownHandle` wiring
    `tack-runner`'s `main` uses (reimplemented, not called into, because that binary's `main` is
    not library code — `tack-runner` is not owned by this card).
  - `tack serve --with-runner` (new field on the `Serve` variant, which changed from a unit
    variant to `Serve { with_runner: bool }`) is also readable as `TACK_LOCAL_RUNNER_ENABLE=1`
    via `local_runner::with_runner_enabled`, checked for both `tack serve --with-runner` and bare
    `tack` (no subcommand) — the same gate applies to both entry points to the server role.
  - `local_runner::serve_with_embedded_runner` is the `--with-runner` path:
    1. Loads `tack_api::config::AppConfig::load()` and calls `ensure_loopback`, which wraps
       `AppConfig::binds_loopback()` — refuses immediately (before any socket or DB) on a
       non-loopback host.
    2. Loads `RunnerConfig` the same way `tack runner start` does and calls
       `require_enrollment_credential()` — refuses immediately if no credential is configured,
       before opening a socket or DB, exactly the same failure `tack runner start` already gives.
    3. Spawns `tack_api::serve_with_ready(ready_tx)` as a `tokio::spawn`ed task and awaits the
       oneshot for the real bound `SocketAddr` (never a hardcoded guess — this is what makes a
       dynamic `TACK_PORT=0` bind work).
    4. Overwrites `runner_config.api_base_url` with `http://{bound_addr}/api/runner/v1` —
       whatever `api_base_url` came from file/environment is discarded, because only the address
       the listener actually bound is authoritative for an embedded runner.
    5. Spawns `tack_runner::bootstrap::run(runner_config, limits, shutdown)` as a second
       `tokio::spawn`ed task and hands both `JoinHandle`s to `supervise`.
  - `supervise` (`local_runner.rs`) is a `tokio::select!` over the two task handles:
    - server task finishes first (the normal path — `tack_api::server`'s own `shutdown_signal`
      already owns ctrl-c internally) → calls `shutdown_handle.request()`, joins the runner task,
      and only then returns — clean shutdown of both.
    - runner task finishes first (it died, failed to enroll, or panicked) while nothing asked it
      to → calls `server_task.abort()` immediately and returns an `Err` naming the runner
      failure. `RunnerRuntime::run`'s own contract (`crates/tack-runner/src/runtime.rs`) only
      ever returns `Ok(())` when `shutdown.is_requested()` is true, so any completion of the
      runner branch before the server branch fires is unconditionally treated as fatal — there is
      no code path where an early runner exit is silently accepted.
  - The embedded runner never calls a handler in-process, never shares `AppState`, and never
    constructs a second `RunnerProtocolClient` — it is `tack_runner::bootstrap::run` given a
    plain `http://127.0.0.1:<port>/api/runner/v1` base URL, identical in shape to what
    `tack-runner --api-url` would be given for a remote server. No shortcut was taken here; ADR
    0058's central constraint holds.
- Tests added and exact commands/results:
  - `crates/tack-cli/src/local_runner.rs`, `#[cfg(test)] mod tests` (5 new tests, all deterministic,
    no subprocess, no network):
    - `embedded_runner_refuses_non_loopback_bind` / `embedded_runner_accepts_the_default_loopback_bind`
      — directly exercise `ensure_loopback` against constructed `AppConfig` values (the same
      struct-literal style `tack-api/src/config.rs`'s own tests use), proving the exact guard the
      real `--with-runner` path calls.
    - `with_runner_enabled_reads_the_environment_gate` — proves `--with-runner` and
      `TACK_LOCAL_RUNNER_ENABLE=1` are both honored and the flag defaults off.
    - `supervise_aborts_the_server_when_the_runner_dies_first` — spawns a `server_task` that never
      completes (`std::future::pending()`) and a `runner_task` that immediately returns
      `Err(RunnerError::ClientStopped)`, calls the real `supervise`, and asserts (a) the returned
      error names the embedded runner, and (b) `server_task.await` itself now returns a
      cancelled `JoinError` — i.e. the server task was actually aborted, not merely reported as
      wrong in a message. This is the automated proof of the "loud, not silent" failure mode ADR
      0058 §Consequences requires.
    - `supervise_stops_the_runner_once_the_server_stops` — spawns a `server_task` that finishes
      immediately (simulating the server's own ctrl-c-triggered graceful stop) and a `runner_task`
      that blocks on `shutdown.requested()`, and asserts `supervise` returns `Ok(())` — proving
      the handoff (`shutdown_handle.request()` on server completion) actually reaches the runner.
  - `cargo fmt -p tack-cli` → clean (ran once, reformatted `local_runner.rs`, no semantic change).
  - `cargo clippy -p tack-cli --all-targets -- -D warnings` → clean.
  - `cargo test -p tack-cli` → `62 passed` in the lib target (57 pre-existing + 5 new), `5 passed`
    in the `main.rs` unit-test target (the 5 `local_runner` tests, compiled as part of the binary
    crate since `local_runner` is `mod`-declared from `main.rs`, not the library), `11 passed` in
    `cli_test.rs`, `5 passed` in `e6_scheduler_e2e_test.rs` (unaffected — `tack serve` with the new
    optional `--with-runner` flag still parses correctly when the flag is omitted, proving the
    signature change is backward compatible). 0 failures anywhere.
  - `cargo build --workspace` → clean; confirms the new `tack-runner` dependency on `tack-cli`
    does not disturb any other crate.
  - Verified no dependency cycle exists: `cargo build --workspace` alone proves it (a cycle is a
    hard compile error), consistent with the card's own pre-verification.
- Failure/adversarial case proved:
  - The non-loopback refusal is proved twice: the unit test above, and live, in the process-level
    proof below (Proof 3) — the process exits non-zero, the stderr names "loopback", and no TCP
    listener is ever opened on the configured port (`std::net::TcpStream::connect` fails against
    it after the refusal).
  - The runner-dies-loud claim is proved by `supervise_aborts_the_server_when_the_runner_dies_first`
    asserting the server `JoinHandle` itself reports `is_cancelled() == true` after `supervise`
    returns — not just that an error string was produced, the actual task teardown.
  - `embedded_runner_refuses_non_loopback_bind` and `embedded_runner_accepts_the_default_
    loopback_bind` assert on the literal function the production path calls (`ensure_loopback`)
    with both a positive and a negative fixture, so the pair cannot pass by tautology — the
    positive case alone would not catch the guard being inverted or dropped, but the negative
    case (`host: "0.0.0.0"`) only passes if `ensure_loopback` actually returns `Err` containing
    "loopback" for a real non-loopback value, which is exactly the behavior being claimed.
- Schema/API/contract change requested from another owner: none.
- Which role executed what for the live runs (Part IV addition):
  - **Proof 1** — a real `tack serve` (server role, no `--with-runner`) on port 3591, and a
    separate `tack runner start` process (standalone runner role, distinct process) enrolled
    against it with a real one-time token from `POST /api/runners/enrollment`, reached `active`
    with a heartbeat, claimed a real execution request, ran it through a shim `opencode` binary
    (same fake-harness pattern `scripts/smoke.sh` uses), and the attempt reached `state: succeeded`.
  - **Proof 2** — a single `tack serve --with-runner` process on port 3592 hosted both roles: the
    server bound the port, and the embedded runner (same process, `tokio::spawn`ed task) enrolled
    against `http://127.0.0.1:3592/api/runner/v1` — the address the server itself reported over
    the readiness oneshot — reached `active` with a heartbeat, claimed a real execution request,
    and the attempt reached `state: succeeded`, all from one binary, one process. `SIGINT` then
    stopped the whole process (server + embedded runner) within the poll window.
  - Full transcript (PASS lines) below; script preserved at
    `/tmp/claude-1000/-home-ox-Sites-objetivosMios/b3baec54-86ac-4990-a4f2-730beab1cbc7/scratchpad/iv-a3-live-proof.sh`
    (not part of this repo — a throwaway proof harness, same shape as `scripts/smoke.sh` steps
    3–7, reduced to what this card needs):
    ```
    == PROOF 1: tack runner start against a live, separate tack serve ==
       PASS server 1 up on 3591
       PASS pending runner runr_1c24184c-... enrolled
       PASS 'tack runner start' active, heartbeat at 2026-08-31T21:53:12.030859293+00:00
       PASS execution request exec_d715f84a... queued
       PASS PROOF 1: attempt succeeded via 'tack runner start' against separate 'tack serve'

    == PROOF 2: tack serve --with-runner completes an attempt from one process ==
       PASS bootstrap server up on 3592
       PASS pending runner runr_80167e0b-... enrolled
       PASS default 'tack serve' (no flag) started no runner: runner runr_80167e0b-...
            state='pending_enrollment' (never went active)
       PASS GET /api/runners: zero active runners under default 'tack serve'
       PASS 'tack serve --with-runner' up on 3592
       PASS embedded runner active, heartbeat at 2026-08-31T21:53:19.057072994+00:00
            (one process, one binary)
       PASS execution request exec_2feb7d8f... queued against the embedded runner
       PASS PROOF 2: attempt succeeded via embedded runner inside 'tack serve --with-runner'
            (attempt_id att_c0f271ae-..., state: succeeded, fencing_token 1)
       PASS SIGINT stopped 'tack serve --with-runner' (server + embedded runner) cleanly

    == PROOF 3: non-loopback bind + --with-runner refuses to start ==
       PASS PROOF 3: non-loopback bind + --with-runner refused to start (exit 1):
            Error: refusing to start --with-runner: 0.0.0.0 is not a loopback address. An
            embedded runner executes arbitrary agent processes on this host, so it is
            restricted to a server bound to loopback
       PASS PROOF 3: no listener was ever opened on the refused bind

    ALL PROOFS PASSED
    ```
- Loopback/gating proof (Part IV addition, named explicitly):
  - Off-by-default: `crates/tack-cli/src/local_runner.rs::tests::with_runner_enabled_reads_the_
    environment_gate` (`cargo test -p tack-cli`) plus the live proof's "default `tack serve`
    (no flag) started no runner" step, which queries `GET /api/runners` directly (not a log line)
    and finds the pre-enrolled runner still `pending_enrollment` with zero `active` runners after
    a multi-second settle window.
  - Non-loopback refusal: `crates/tack-cli/src/local_runner.rs::tests::embedded_runner_refuses_
    non_loopback_bind` (unit, deterministic) plus the live proof's "PROOF 3" (process-level: exit
    code, stderr message, and proof the port was never opened).
- Binary-size delta (Part IV addition, mandatory, measured not estimated):
  - Method: `cargo build -p tack-cli --release` (this repo's real release profile — `lto = true`,
    `codegen-units = 1`, `opt-level = "z"`, `strip = true`) built twice: once at base SHA
    `2cc73b2` in a disposable `git worktree` (`/var/tmp/tack-agent-worktrees/IV-A3-base`, its own
    `CARGO_TARGET_DIR`), once on this branch's final commit, both on the same machine back to
    back.
  - Base (`2cc73b2`, `tack` only, no runner role): 18,626,200 bytes (17.76 MiB / 18.63 MB)
  - After (this branch, `tack` with `tack runner start` + `tack serve --with-runner`):
    19,495,576 bytes (18.59 MiB / 19.50 MB)
  - Delta: **+869,376 bytes (+0.83 MiB / +0.87 MB, +4.67%)**
  - For context, the standalone `tack-runner` binary built the same way on this branch is
    4,732,160 bytes (4.51 MiB) — matching ADR 0058's own "4.5 MB at 0.1.0-beta.6" figure closely
    (this build is `0.1.0-beta.7`). The delta this card adds to `tack` (0.83 MiB) is well under a
    fifth of `tack-runner`'s own size, confirming the ADR's prediction that most of the runner's
    weight (tokio, reqwest, serde, `tack-orch`) was already linked into `tack` before this card.
- Known limitations or `not_measured` fields: none. Every acceptance item in the card was
  demonstrated with a real run or a real test; nothing here is asserted from plausibility.
- Secrets/logging review: the enrollment credential flows through
  `tack_runner::EnrollmentCredential` (redacted `Debug`/`Display`, `CLAUDE.md`'s own documented
  type) exactly as it does for the standalone binary — `local_runner.rs` never formats it, logs
  it, or puts it in an error. `RunnerAction::Start`'s `enrollment_token` CLI field carries
  `hide_env_values = true` for parity with `tack-runner`'s own `Cli` struct (defensive; this field
  has no `env = ` clap attribute, so the flag is a no-op today, matching the field it mirrors).
  `ensure_loopback`'s error message includes `config.host`, which is operator-supplied
  configuration (e.g. `"0.0.0.0"`), never a credential.
- Safe merge order and likely conflicts: no conflicts expected. This card's only production diff
  is `crates/tack-cli/src/main.rs`, `crates/tack-cli/Cargo.toml`, `Cargo.lock`, and the new
  `crates/tack-cli/src/local_runner.rs` — no other in-flight Part IV or Part V card touches
  `tack-cli`. `tack-api` and `tack-runner` are read-only dependencies from this card's point of
  view; nothing in either was changed.
- Checklist: no unowned files touched — no live secret introduced or logged — no panic stub (every
  fallible call in the new code propagates via `?` or is explicitly matched; `unreachable!()` arms
  are only reached for `Commands`/`RunnerAction` variants this same `main()` already returned out
  of earlier in the same call, the same pattern the pre-existing `Serve`/`Config`/`Completions`
  arms already used) — no blind retry anywhere in the new code.
