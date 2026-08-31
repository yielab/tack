## What this is about

The live smoke test — the one script that proves a real coding-agent run works end to end,
not just against fakes — was reporting failures that were either silent (a run just sits
there forever with no explanation) or stale (an error message describing a bug that was
already fixed weeks ago). This work chased both down to their real causes, fixed the two that
were genuine product defects, and made the smoke test describe failures accurately instead of
guessing.

## Where it stands

Two real, previously-undiagnosed defects are fixed and proven against the actual installed
`codex`, `claude`, and `opencode` binaries on this machine, not just against fakes:

- A long-running live task (Codex, Claude Code, or OpenCode — any of them, any model) used to
  get permanently stuck at "running" with no error and no explanation once it ran past about a
  minute. This was not about slow models specifically; any real task that took that long hit
  it. It is fixed.
- The Codex harness could never be scheduled at all, on any machine, no matter what model was
  requested — its own version check silently rejected the real `codex` binary's actual output
  and never told anyone why. Codex is now schedulable, and this run is the first time it has
  ever gone through the complete pipeline (claimed, checked out, spawned, given a real network
  call) instead of being turned away before it started.

What still doesn't work: Codex has still never *completed* a live run successfully on this
machine. Now that it can actually run, it fails for a different and much more mundane reason —
this machine's Codex login doesn't have access to the specific model the test requests. That's
an account/subscription limitation, not a bug in this product, and the smoke test now says so
plainly instead of hiding it behind an unrelated error.

The smoke test itself no longer guesses at causes it can't see. When a request never gets
picked up, it now looks at what the runner actually declared about itself and says which of
three different problems happened — a broken binary, a genuinely unsupported model, or the
runner simply being busy — instead of assuming it's always the same one.

## What is left

**Codex has never completed a live run.** It can now be scheduled and it now executes for
real, which is new, but no successful Codex run has ever been observed on this machine. The
blocker is that this specific Codex login can't use the model the test asks for — trying a
different model, or a different login, is the next person's call to make, not something to
paper over here. Whoever writes user-facing claims about "three harnesses working" needs this
fact.

**A secondary, lower-severity issue found while investigating the first one:** when the runner
fails to report a finished task back to the server, it was capable of retrying so fast and so
often that it could hammer the server with thousands of requests a second. This was only ever
observed as a side effect of the main stuck-task bug above, so once that's fixed it's very
unlikely to happen on its own — but the missing safety pause has been added regardless, since
it's a one-line, obviously-correct fix and leaving it out would mean the same hazard could
resurface from some other, unrelated failure later. It was not given its own dedicated test
(see Not checked below); it is a defensive addition, not the main finding.

## Technical detail

**Where the code lives**
`crates/tack-runner/src/engine.rs` (the stuck-task fix and its test), `crates/tack-runner/src/harness/codex.rs` (the version-parsing fix and its tests), `crates/tack-runner/src/transport.rs` (the retry-hammering fix), `crates/tack-runner/Cargo.toml` (a test-only dependency feature the new test needs), `scripts/smoke.sh` (the reworded diagnostics).

**How the stuck-task bug worked, and how the fix works**
The server grants a runner a 60-second lease on a task (`LIMITS.lease_duration_seconds` in `crates/tack-api/src/handlers/runner_protocol.rs`), renewed by heartbeats that name the task. The runner's own code (`run_claimed` in `engine.rs`) sent exactly one heartbeat right after starting a task, then blocked on `HarnessAdapter::wait(...)` — which can legitimately run for up to 24 hours (`request_timeout_seconds_max`) — without heartbeating again until the harness process itself finished or hit its own timeout. Any task genuinely running longer than 60 seconds therefore had its lease silently expire while the runner still believed it held it. When the harness eventually finished (or was killed by its own timeout), the completion report was rejected by the server as `stale_lease`. The runner's live serve loop had no retry path for that specific failure during normal operation — it just logged it and moved on to claim more work — so the task was permanently stuck `running`/`terminal_reason: null` from the API's point of view. A runner restart does not fix this either: the one-time startup replay resends the exact same (still-expired) lease and fails identically. The fix is `RunnerEngine::wait_with_lease_renewal` (new, in `engine.rs`): it races the harness wait against a 20-second repeating heartbeat (`LEASE_RENEWAL_INTERVAL`) for as long as the harness is still running, so the lease never goes stale regardless of how long the real task takes.

**How the Codex scheduling bug worked, and how the fix works**
Every harness adapter probes its binary's `--version` output at startup to decide whether it's installed and healthy. Codex's probe (`is_strict_version` in `codex.rs`) required the *entire* trimmed output to be a bare `X.Y.Z` number. The real `codex` binary prints `codex-cli 0.149.1` — a program-name word before the version, not a bare number — so the probe always rejected it and recorded a `probe_error`, and the scheduler refuses to place any work on a harness whose probe reported an error, regardless of any other capability it declares. This is why Codex could never be scheduled on any machine, even though its "accepts an operator-specified model at run time" attestation (`model_passthrough: supported`) was already correct. The fix (`find_strict_version_token`, new, in `codex.rs`) scans the output's whitespace-separated tokens for one that is itself a strict version number, rather than requiring the whole line to be one.

**How the retry-hammering fix works**
`crates/tack-runner/src/transport.rs`'s main loop sends a heartbeat when it has no work; if that heartbeat itself fails, it now sleeps for the same backoff already used elsewhere in the same loop before looping again, instead of looping immediately.

**How the reworded smoke diagnostics work**
`scripts/smoke.sh` step 8 no longer assumes a single fixed cause for an unclaimed request. It reads the runner's own declared capability snapshot (already fetched in step 6) for the specific harness kind and reports: a probe failure by name, if one is declared; "declared schedulable but not claimed — likely capacity" if the runner says it should have worked; or the genuine "not declared and no passthrough" case otherwise. For a request that *was* claimed and ran but still failed, it now prints the harness's own reported code, message, and a bounded stdout preview (where an adapter like Codex's, which classifies purely by exit code, puts the real explanation) instead of just the bare state word.

**What is blocking, technically**
Nothing in this repository. Codex's remaining failure is external: the connected account rejects the specific requested model (`gpt-5-codex`) with `"The 'gpt-5-codex' model is not supported when using Codex with a ChatGPT account."` — a real, structured error from Codex's own backend, captured and surfaced correctly by the (now-working) pipeline. There is no scheduler, contract, migration, router, or frontend change implicated by anything found here.

**Test results**

- `cargo fmt --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test -p tack-runner`: 233 passed / 0 failed / 3 ignored (lib), plus 2 (cli), 7 (crash_matrix), 3 (g2_journal_corruption_test), 6 (h3_checkout) — 251 passed, 0 failed overall.
- New/changed tests: `engine.rs::wait_periodically_renews_the_lease_while_the_harness_still_runs` (paused-clock proof that the renewal loop fires repeatedly during a long wait); `codex.rs::probe_recognizes_a_program_name_prefixed_version_string`; the existing `#[ignore]`d live Codex test now asserts `probe_error == None` against the real binary instead of only "didn't panic."
- Both fixes were proven load-bearing by reverting each one individually and re-observing the original failure, then reapplying:
  - Codex probe: reverting `find_strict_version_token` back to the old whole-string check reproduced `probe_error: Some("codex --version output was not a recognizable version string")` against the real binary (`cargo test -p tack-runner --lib -- --ignored codex::tests::live_`).
  - Lease renewal: reverting `wait_with_lease_renewal` back to a bare `self.adapter.wait(...)` call made the new unit test fail (1 heartbeat observed instead of ≥4).
- Live-run evidence (scrubbed of the local `x-tack-principal` test header, which carries no secret; no credentials appear in any of the captured output):
  - Before any fix, live smoke (light model for step 7, `SMOKE_KEEP=1`): steps 1–7 and 9 passed; step 8 printed the stale canned text for `codex` ("declares zero model_combinations... structurally unschedulable") while `claude-code`/`opencode` succeeded — this by itself already showed the "capacity cascade" hypothesis was *not* the whole story, since two of three kinds succeeded under an otherwise-idle runner.
  - Manual reproduction outside the smoke script, isolating the stuck-task bug: a request against the real heavy local model (`opencode` + `llamacpp/qwen3.6-35b-uncensored`) with a 300-second budget reached `running` and then never changed state. The runner log showed the exact mechanism: `run cycle finished outcome=TerminalReportPending`, immediately followed by ~28,600 heartbeat attempts in under a second, all rejected `IdempotencyConflict`, with `stale_lease` on the `/completion`, `/events`, and `/artifacts` calls just before that. The `opencode` process itself had already exited; the runner was alive and spinning, not hung.
  - The same request, same model, same 300-second budget, after the fix: `state: failed`, `terminal_reason.code: timed_out`, `duration_ms: 300060`, runner log shows one clean `run cycle finished outcome=Completed` line and zero `stale_lease` occurrences.
  - Final live smoke run after both fixes (light model for step 7, `SMOKE_KEEP=1`): steps 1–7 and 9 passed. Step 8: `codex` was claimed and ran for the first time, ending with
    `codex exited with status 1 | stdout: ... "The 'gpt-5-codex' model is not supported when using Codex with a ChatGPT account."` — `claude-code` and `opencode` both succeeded. Overall result: **SMOKE FAILED**, on one honestly-reported, externally-caused line.
  - A short (25-second budget) control run against the same heavy model, run before the long reproduction above, completed cleanly (`failed`/`timed_out` at ~25s) — establishing early that the process-level timeout mechanism itself was sound, which is what pointed the investigation at the lease instead of at `ProcessLimits`.

**Not checked**
- The retry-hammering fix in `transport.rs` has no dedicated unit test (proportionate to it being a one-line defensive addition to an already-established pattern in the same function); it is reasoned about and covered indirectly by the fact that the post-fix live reproduction shows zero heartbeat failures at all.
- No attempt was made to get Codex to a successful live completion by trying a different model or account — that would be fitting the test to this one machine's account limits, not a product fix, and is explicitly the next owner's call.
- `crates/tack-orch`, `crates/tack-api`, migrations, and the frontend were not touched and their test suites were not re-run, since nothing in this diff reaches them.

## Next step

Re-run `./scripts/smoke.sh --live` (optionally `SMOKE_LIVE_MODEL=opencode/big-pickle` to avoid the slow local model in step 7) whenever a Codex login with access to the requested model is available, to see whether it now passes end to end.

Branch `agent/v-a2-live-smoke`, committed.
