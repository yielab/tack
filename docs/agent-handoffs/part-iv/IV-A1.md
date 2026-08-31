# IV-A1 handoff

**What this card changes, in plain language.** Before it, everything needed to
compose a working `tack-runner` — the adapter registry, capability reporting,
the real HTTP protocol client, the engine, the journal, the workspace
provisioner — lived only in the `tack-runner` binary's own `main.rs`. Nothing
outside that binary could reach any of it; a future embedder (a later card,
per ADR 0058) would have had to copy the wiring, and a copied composition
root is a copy that drifts from `docs/contracts/runner-v1/`. After it, that
same wiring lives in a public library function,
`tack_runner::bootstrap::run`, taking an existing `RunnerConfig`, the
existing `ProcessLimits`/protocol-timeout values (now bundled as an explicit
`RunnerLimits`, still with no `Default`), and an existing `Shutdown`. The
`tack-runner` binary's `main.rs` is now just argument parsing, config
assembly, and its own signal handling, ending in one call to that function.
Nothing about what the runner does, reports, or logs changed — this is a
pure extraction, proved below.

- **Base SHA / branch / final SHA:** base `81e66e5` (tip of `develop` this
  worktree branched from). Branch `agent/iv-a1-runner-entrypoint`. Final SHA:
  this commit — the current tip of `agent/iv-a1-runner-entrypoint` at
  delivery (see `git log` on the branch; reported exactly in the delivery
  report to the integrator).
- **Files changed (must equal ownership list):**
  - `crates/tack-runner/src/main.rs` — composition removed; now argument
    parsing, config assembly, tracing init, one call to
    `bootstrap::run(config, limits, shutdown)`, and the pre-existing
    `tokio::select!` signal handling, unchanged.
  - `crates/tack-runner/src/lib.rs` — added `pub mod bootstrap;`. No existing
    export changed or removed.
  - `crates/tack-runner/src/bootstrap.rs` — **new**. Holds
    `build_runtime(config, limits) -> Result<ProductionRunnerRuntime,
    RunnerError>`, `run(config, limits, shutdown) -> Result<(), RunnerError>`,
    the `RunnerLimits` struct, the `ProductionRunnerRuntime` type alias, and
    the two private helpers moved verbatim from `main.rs`:
    `build_adapter_registry` and `report_capabilities`.
  - `crates/tack-runner/tests/bootstrap_entrypoint.rs` — **new**. Black-box
    integration test proving the entry point is reachable and
    shutdown-controllable from outside the crate.
  - `docs/agent-handoffs/part-iv/IV-A1.md` — this file.

  No file outside this list changed. No other crate, no contract fixture, no
  harness adapter (`crates/tack-runner/src/harness/{claude_code,codex,opencode}.rs`
  untouched) was edited.

- **Contract fixtures consumed:** `docs/contracts/runner-v1/enrollment.response.json`,
  read (not modified) by the new integration test's mock server to answer the
  real `HttpPullProtocol::enroll` call the composed runtime makes on
  startup — the same fixture `transport.rs`'s own in-crate tests already pin
  against. No fixture file changed.

- **Behavior implemented:** an extraction only, no new behavior.
  `tack_runner::bootstrap::build_runtime` builds the identical stack
  `main.rs` always built — `AdapterRegistry` populated from
  `CodexAdapter`/`ClaudeCodeAdapter`/`OpenCodeAdapter::discover`,
  `report_capabilities` (cancel advisory, resume/decisions unsupported,
  artifacts/usage advisory — same reasons, same doc comment, moved verbatim),
  `HttpPullProtocol` wired to `RunnerEngine` via `.with_data_protocol`,
  `OwnerOnlyJournal`, `WorkspaceManager<GitWorktreeProvisioner>`, all
  assembled into a `RunnerRuntime` — and returns it unstarted.
  `tack_runner::bootstrap::run` calls that and then `.run(shutdown)`, exactly
  what `main.rs`'s old `run(cli)` did inline. `ProcessLimits` and
  `PROTOCOL_REQUEST_TIMEOUT` are unchanged in value and still defined as
  `const`s in `main.rs`; they now travel into the library as an explicit
  `RunnerLimits { harness_process, protocol_request_timeout }` argument with
  no `Default` impl, so a composer (the binary today, an embedder later)
  must still choose them — nothing was quietly defaulted.

- **Tests added and exact commands/results:**
  - `crates/tack-runner/tests/bootstrap_entrypoint.rs`, two tests:
    - `the_composition_root_stops_on_an_injected_shutdown_with_no_process_signal`
      — spawns `bootstrap::run` as an ordinary `tokio::spawn` task (not a
      subprocess) against a local one-shot mock HTTP server that answers the
      real enrollment exchange after a 300ms delay, calls
      `shutdown_handle.request()` immediately (no signal sent to any
      process), then asserts the task resolves `Ok(())` within a 5s bound.
      It resolves in ~300ms — after the server's delayed reply, at the
      runtime's first post-enrollment shutdown check — which is what proves
      the injected `Shutdown` (not an unrelated fast failure) is what
      stopped it.
    - `build_runtime_fails_fast_on_a_missing_enrollment_credential` — asserts
      `build_runtime` returns the typed `RunnerError::MissingEnrollmentCredential`
      synchronously, with no network call attempted, preserving the "checked
      before any filesystem side effect or protocol work" contract the
      original inline code documented.
  - Commands run (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/IV-A1` for
    every invocation):
    - `cargo fmt -p tack-runner` — no diff beyond the files above.
    - `cargo clippy -p tack-runner --all-targets -- -D warnings` — clean, 0
      warnings.
    - `cargo test -p tack-runner` — green, full output below.
    - `cargo build --workspace` — succeeds (confirms `tack-cli` and every
      other workspace member still compile against `tack-runner`'s public
      surface; `grep -rn 'tack_runner::' crates/ --include=*.rs` outside
      `crates/tack-runner/` returns no hits, before or after this change —
      nothing external calls into this crate yet).
  - **Test counts, base `81e66e5` vs this branch** (`cargo test -p
    tack-runner`, summed across every test binary: lib unit tests, `main.rs`
    unit tests, `tests/cli.rs`, `tests/crash_matrix.rs`,
    `tests/g2_journal_corruption_test.rs`, `tests/h3_checkout.rs`, doctests,
    plus the new `tests/bootstrap_entrypoint.rs`):
    - Before: 254 total (251 passed, 3 ignored, 0 failed).
    - After: 256 total (253 passed, 3 ignored, 0 failed).
    - No test removed or renamed; exactly the 2 new tests above account for
      the delta.

- **Failure/adversarial case proved:** the shutdown test is deliberately
  built so a "false pass" (the task finishing for an unrelated reason, e.g.
  an instant connection error, before shutdown is even observed) cannot
  happen — the mock server's 300ms reply delay is longer than any local
  scheduling jitter, so the only way the task can resolve `Ok(())` within
  the bound is by actually observing `shutdown.is_requested()` after
  enrollment succeeds. The missing-credential test proves the fast-fail
  ordering survived the move, not just that it compiles.

- **Schema/API/contract change requested from another owner:** none.

- **Known limitations or `not_measured` fields:**
  - `tack_runner::bootstrap` has no caller outside this crate yet — that is
    the expected state this card leaves for the embedding card (per ADR
    0058, not part of IV-A1's scope), not a defect. Confirmed via
    `grep -rn 'tack_runner::' crates/ --include=*.rs`: no hits outside
    `crates/tack-runner/` before or after this change.
  - Binary-size delta: **not measured** — this card changes nothing that
    `tack`/`tack-cli` links (it touches only `tack-runner`), so there is no
    size claim to make. The ADR itself states the real number belongs to
    "the card that lands it" — the future embedding card, not this one.

- **Secrets/logging review:** no new log line was added anywhere; the moved
  code's existing redaction discipline is unchanged (`require_enrollment_credential`
  still never puts the credential value into an error, log, or diagnostic).
  Verified live during both smoke runs below — `$WORK/server.log` and the
  runner's own stderr carry ids only. The new test's mock server only ever
  sees a fixed literal test string (`"test-only-credential"`), consistent
  with the `SECRET_ENROLLMENT`-style literals already used by this crate's
  other tests; nothing resembling a real credential exists anywhere in the
  diff.

- **Safe merge order and likely conflicts:** this card only touches files
  under `crates/tack-runner/`. IV-A2 (sibling, concurrent) touches
  `crates/tack-api/src/server.rs` — no file overlap, so merge order between
  IV-A1 and IV-A2 does not matter for conflicts. No other card known to be in
  flight touches `crates/tack-runner/src/main.rs` or `lib.rs`.

- **Checklist:** no unowned files touched (exactly the 5 files listed
  above) · no live secret (only literal test strings, never logged) · no
  panic stub (`unwrap`/`expect` appear only in the new test file, matching
  this crate's existing test conventions; zero new `unwrap`/`expect`/
  `unimplemented!` in library code) · no blind retry (no retry logic added;
  `RetryPolicy::default()` usage is the same pre-existing call, untouched by
  this card).

## Part IV additions

- **Binary-size delta:** not applicable. This card does not change what
  `tack` (or any binary other than `tack-runner` itself) links — it moves
  code within `tack-runner`. The embedding card that adds `tack runner
  start` / `tack serve --with-runner` is the one ADR 0058 asks to measure and
  record this number.
- **Which role executed what (for the live runs below):** both smoke runs
  used the **standalone `tack-runner` binary** role exclusively — no
  embedded role exists yet, that is a later card. In each run, `tack-runner`
  enrolled, claimed, and completed every attempt against a real `tack serve`
  over loopback HTTP at `http://127.0.0.1:$PORT/api/runner/v1`, exactly the
  two-process shape ADR 0050 already established; this card changes only how
  that binary's own `main.rs` assembles itself internally.
- **Loopback/gating proof:** does not apply to this card, as anticipated by
  the Part IV README. IV-A1 makes the composition root callable from outside
  the crate; it does not add `--with-runner`, `TACK_LOCAL_RUNNER_ENABLE`, or
  any loopback-bind check — none of that surface exists yet. No test in this
  diff exercises `AppConfig::binds_loopback()` or an off-by-default gate,
  because there is no gate for this card to prove.

## `./scripts/smoke.sh` — before vs. after, side by side

Both runs used fake mode (shim harness binaries; the rest of the pipeline —
server, scheduler, runner, provisioner, adapters, subprocess handling — is
real), each built fresh and run to completion with `SMOKE_KEEP` unset. Base
run used a disposable `git worktree add` at `81e66e5`; after-run used this
branch. Both reached all 9 steps with the same outcomes:

| Step | Base `81e66e5` | This branch |
|---|---|---|
| 1 Harness availability | 3 of 3 present | 3 of 3 present |
| 2 Build tack + tack-runner | PASS | PASS |
| 3 Server healthy, Docket absent | PASS | PASS |
| 4 Project + item created | PASS | PASS |
| 5 Pending runner + enrollment token | PASS | PASS |
| 6 Runner enrolls, heartbeats, polls | PASS (active) | PASS (active) |
| 7 Claim → checkout → harness → completion | PASS (attempt succeeded, fencing_token 1) | PASS (attempt succeeded, fencing_token 1) |
| 8 Same request through codex/claude-code/opencode | PASS × 3 | PASS × 3 |
| 9 Restart recovery (kill mid-attempt, no silent loss, no blind duplicate, operator requeue) | PASS (all sub-assertions) | PASS (all sub-assertions) |
| Final result | `SMOKE PASSED — fake shim harnesses, pipeline real` (exit 0) | `SMOKE PASSED — fake shim harnesses, pipeline real` (exit 0) |

No step's pass/fail shape, ordering, or count differs between the two runs;
only the generated ids (project/item/runner/attempt uuids, commit sha of the
smoke fixture repo) differ, as expected between independent runs.
