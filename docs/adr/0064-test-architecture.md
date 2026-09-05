# ADR 0064: Tests run under nextest, build once per resolution, and group one binary per subject

**Decide:** approve four changes to how this repository builds, runs and reports its tests —
none of which changes what any test asserts. (1) `cargo nextest` becomes the test runner
everywhere: locally, in the pre-push hook and in CI, configured to print failures and a
one-line summary and nothing else. (2) Tests are always **built with the workspace's feature
resolution and selected with nextest filtersets**, never with `cargo test -p`. (3) The dev
profile keeps line tables instead of full debuginfo. (4) `tack-api`'s 36 integration-test
binaries are regrouped into about five named by subject, and CI runs each test once.

**Why now:** measured on 2026-09-05, a green `cargo test --workspace` prints **84,000 tokens**
of output for 1,399 passing tests — about 40% of what reading `TODO.md` whole costs, which
this repository already treats as prohibitive — spent to learn the single word "ok". The same
suite under nextest, configured to report only failures, prints **152 tokens** and finishes in
**11.5 s instead of 51 s**. The tests were never the expensive part; the output was. And the
way they are invoked compiles every source change twice.

**If you do nothing:** every agent that verifies its work keeps paying ~84k tokens per green
run and re-reads it to find nothing; every change to `tack-api` keeps relinking 36 binaries of
344 MB each (12 GB of the 30 GB `target/`, 84% of it debuginfo); CI keeps running most of the
suite 2.3 times per push; and `docs/TESTING.md` keeps describing a suite of 207 tests that has
1,399.

## The decisions, in short

| # | Decision | Measured reason |
|---|---|---|
| 1 | **nextest is the runner.** `.config/nextest.toml` sets `status-level = "fail"`, `final-status-level = "fail"`, `fail-fast = false`, `retries = 0` and a slow-timeout. `cargo test` stays valid but is not what the docs, the hook or CI call. | Output 84,126 → 152 tokens; wall 50.9 s → 11.5 s. `cargo test` runs the 73 binaries one after another (per-binary sum 48.4 s ≈ wall); nextest runs every test as its own process across all binaries at once. |
| 2 | **Build with the workspace resolution; select with filtersets.** `cargo nextest run --workspace -E 'package(tack-api)'` — never `cargo test -p tack-api`. `/gate`, `docs/TESTING.md` and CI say so. | `-p` and `--workspace` resolve ~20 dependency feature sets differently, so `tack-core`, `tack-db`, `tack-orch` and `tack-api` exist twice in `target/` and every source change is compiled once per form you use. Measured: 13 s of recompilation after switching forms with no source change. |
| 3 | **Dev profile: `debug = "line-tables-only"`.** Backtraces keep file and line. | One test binary: 344 MB with full debuginfo, 54 MB stripped — 84% debuginfo. 28 of the 30 GB in `target/debug` are executables. |
| 4 | **One integration-test binary per subject, not per file.** `tack-api/tests/` goes from 36 files to about five (`handlers`, `orchestration`, `runner_protocol`, `security`, `wiring`); `tack-db` from 12 to two; `tack-orch` from 10 to three with `runner_contract` kept on its own. Files named after board cards are renamed after their subject in the same move; the eight private `fn setup` copies use `common::test_app*`. | Each integration file is its own crate and its own full link: 36 × 4.9 s CPU = 178 s per `tack-api` change, 12 GB of disk. The Rust community's standing advice is one integration binary per crate; nextest makes that free at run time because parallelism is per test, not per binary. |
| 5 | **CI runs each test once.** One `cargo nextest run --workspace` with JUnit output replaces `cargo test --workspace` plus the five re-runs of its own subsets. The two regenerate-and-diff gates (OpenAPI, golden) stay — they do something different. Coverage and the release-profile embed-SPA build leave the per-push path (main, tags, manual). `CARGO_INCREMENTAL=0` in CI. | The re-runs exist, per the workflow's own comment, so each gate shows its own status past a first failure; `fail-fast = false` plus per-test reporting gives that without re-running. Per push today: Rust job 5–6 min, coverage 4–5 min, embed-SPA 5–11 min, E2E 4–5 min. |
| 6 | **Fixed waits become conditions.** The sixteen fixed `sleep`s of 200 ms and up in `tack-api` and `tack-orch` tests become bounded polls, or paused time (`tokio::time::pause`) where the code under test is clock-driven. | Fixed waits are the flakiness that retry policies exist to hide, and ~4 s of wall: `traces_ingestion_test` is 2 tests in 4.07 s. |
| 7 | **A number in `docs/TESTING.md` carries the command that produces it, or goes.** | It says 207 Rust tests and names `tests/api_test.rs` as *the* handler suite; there are 1,399 tests in 65 files. |

Deliberately **not** decided: splitting `tack-api` into crates, `sccache`, `mold`, optimizing
dependencies in the dev profile, sharding. Each was considered; "Considered and not adopted"
below says why not yet.

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail for whoever implements or later audits one
of these calls.

---

- **Status:** accepted 2026-09-05 — see Amendments for what landed and what is open
- **Date:** 2026-09-05
- **Relationship to existing rules:** refines `.claude/skills/gate/SKILL.md`, which already
  orders verification cheapest-first and scoped to the diff — this ADR changes what the cheap
  commands *are*. Extends the CLAUDE.md rule that a load-bearing number carries its command.
- **Wire contract:** none. No fixture, DTO or API shape changes. `runner_contract` stays
  byte-pinned and stays its own binary.

## Rollout, in order

Four steps, each mergeable alone, each measured against the numbers below before the next
starts. Step 4 is the only one that touches test code.

1. **Runner and output** — `.config/nextest.toml`; `/gate`, `docs/TESTING.md`,
   `.githooks/pre-push` and `ci.yml` call nextest; every `-p <crate>` becomes
   `--workspace -E 'package(<crate>)'`. *Proof:* a green run prints ≤ 200 tokens; a red one
   prints the failure and the summary and nothing else.
2. **Profile** — `debug = "line-tables-only"` in `[profile.dev]`. *Proof:* `du -sh
   target/debug/deps` after a clean build; a deliberate panic still shows `file:line`.
3. **CI** — one test step with JUnit; coverage and embed-SPA moved off the push path;
   `CARGO_INCREMENTAL=0`. *Proof:* every test name appears once in the run's JUnit; the Rust
   job's duration recorded before and after.
4. **Regrouping** — one crate per branch, `tack-api` first: move files under
   `tests/<subject>/`, rename by subject, replace the private `fn setup` copies. *Proof:*
   `cargo nextest list` count unchanged (1,392 run + skipped); `ls crates/tack-api/tests/*.rs
   | wc -l` ≤ 6. **The one hazard:** three files (`c2_handlers_test`, `f1_decisions_test`,
   `f2_artifact_events_test`) install a process-global `tracing` subscriber idempotently
   (`ensure_global_log_capture_installed`) to close a race between tests sharing a process.
   Under nextest each test is its own process, so that race cannot occur; under `cargo test`
   one install per merged binary still suffices. Verify the merged binary under both runners.
5. **Fixed waits** — opportunistic: each file when it is next touched, never as a sweep.

## What was measured, and how

Machine: 16 cores, 62 GB RAM, rustc 1.98.1, warm `target/`. `rust-lld` is already the default
linker — verified with `rustc --print link-args` on a hello-world (`-fuse-ld=lld`). Every
number below names the command that produced it; re-run it before quoting.

### Execution

| What | Result | Command |
|---|---|---|
| Full suite, `cargo test` | 1,399 tests, 73 binaries, **50.9 s** wall; per-binary sum 48.4 s | `/usr/bin/time cargo test --workspace` |
| Full suite, nextest | 1,392 run + 6 skipped¹, **11.5 s** (18.7 s first run) | `cargo nextest run --workspace --no-fail-fast` |
| Slowest binaries | `tack-orch` lib 173 tests 6.3 s · `execution_repo_test` 61 tests 4.1 s · `traces_ingestion_test` **2 tests 4.1 s** · `ingestion_test` 2 tests 2.9 s | the `finished in` line per binary of the run above |
| Binaries under 0.1 s | 24 of 73 | same |
| Frontend | 756 tests, 85 files, 8.8 s | `cd frontend && npx vitest run` |

¹ `cargo test` lists 1,399 and reports 7 ignored; nextest reports 1,392 + 6 skipped. The
one-test difference is unreconciled.

### Output (tokens ≈ bytes ÷ 4)

| Invocation | Lines | Tokens |
|---|---|---|
| `cargo test --workspace` | 2,692 | **84,126** |
| `cargo test --workspace -q` | 1,289 | 59,411 |
| `cargo nextest run --workspace` | 1,401 | 42,804 |
| `cargo nextest run --workspace --status-level fail --final-status-level fail` | 8 | **152** |

### Compilation

From `cargo test … --no-run --timings` (`target/cargo-timings/cargo-timing.html`, "Dirty
units" in its header; per-unit durations in its `UNIT_DATA`).

| Change | Dirty units | Wall | Critical path |
|---|---|---|---|
| touch one `tack-api` test file, rebuild that binary | 1 | 2.9 s | compile + link of one test crate |
| touch `tack-api/src/lib.rs`, `cargo test -p tack-api` | 38 | 28.1 s | `tack_api lib (test)` 27.8 s — the `-p` variant was cold |
| touch `tack-core/src/lib.rs`, `cargo test --workspace` | 81 | 21.0 s | `tack` bin 10.2 s; `tack_api lib (test)` 7.5 s warm |
| switch `--workspace` → `-p tack-api`, **no source change** | 4 (core, db, orch, api) | 13.1 s | the duplicate variant |
| switch back | 0 | 1.6 s | — |

Integration binaries: 65 in the workspace, 36 in `tack-api`; mean 2.8 s CPU each (4.9 s in
the `-p` variant); 178–182 s CPU for a full relink. Which feature sets differ:
`cargo tree -e features,normal -f '{p} [{f}]' --prefix none` for `-p tack-api` versus
`--workspace`, sorted and diffed — ~20 crates (`reqwest`, `rustix`, `futures-util`,
`indexmap`, `sha2`, …), each pulled by another workspace member. Not one flag to align.

### Disk

`target/debug` 30 GB; `deps/` 21 GB; executables 28 GB. `api_test` binary 344 MB → 54 MB
after `strip -o` (84% debuginfo). `CARGO_TARGET_DIR` is unset, so every agent worktree builds
its own copy.

### CI

`gh run view <id> --json jobs`, last three green runs on `develop`: Rust (fmt + clippy + test)
5–6 min · Coverage 4–5 min · Embed SPA 5–11 min · E2E 4–5 min · MSRV 1 min · the rest under
a minute. The Rust job runs `cargo test --workspace` and then re-runs `runner_contract`,
`wave2_gate`, all of `tack-runner`, `orch_migrations_test`, five security tests and the
scheduler E2E — all already inside the first step. The coverage job invokes `cargo llvm-cov`
five times, once per crate, each an instrumented build that shares nothing with the normal one.

### Shape of the suite

| Crate | Unit (`src/`) | Integration (`tests/`) | Files | Notes |
|---|---|---|---|---|
| tack-core | 97 | 0 | 0 | pure, no I/O — the right shape |
| tack-db | 5 | 195 | 12 | all use `common/`; repository tests need a DB, so this ratio is right |
| tack-orch | 173 | 109 | 10 | no `common/`; `runner_contract` byte-pins the fixtures — stays separate |
| tack-api | 107 | 320 | 36 | 16 use `common/`, 8 carry a near-identical private `fn setup`; 13 files named after board cards |
| tack-runner | 235 | 20 | 5 | 8 `#[ignore]` (live harnesses, billed) — correct |
| tack-cli | 86 | 16 | 2 | the scheduler E2E is forced to `--test-threads=1` in CI although it already picks a free port |

Counts: `grep -rcE '#\[(tokio::)?test' crates/<crate>/{src,tests}`. Fixed waits ≥ 200 ms in
tests: **16**, of which the four longest are in `tack-orch`'s ingestion pair (2.6 s, 2.4 s and
two of 1.4 s) — which is what makes `traces_ingestion_test` 2 tests in 4.07 s:
`grep -rnE 'sleep\([^)]*from_(millis\([0-9_]+|secs\([0-9_]+)' crates/*/tests`, discarding
the sub-200 ms hits. Isolation is
sound: every `tack-api` and `tack-db` test opens its own `sqlite::memory:` pool; `set_var`
appears only in `tack-api/src/server.rs`'s own unit tests and two runner unit tests.

### Considered and not adopted

- **`mold`** — `rust-lld` is already the linker; the large step is banked.
- **`sccache`** — no external crate recompiled in any measured flow; `Swatinem/rust-cache`
  covers CI.
- **Splitting `tack-api`** — the warm `lib (test)` compile is 7.5 s. After decision 4 it
  becomes the critical path, and 7.5 s does not justify refactoring 49k lines across the two
  auth surfaces. Revisit if it passes ~20 s.
- **`[profile.dev.package."*"] opt-level = 2`** — execution is already 11.5 s; the gain is
  unmeasured and the first build of every fresh worktree gets slower. Measure after step 1.
- **Sharding** — pointless under 20 s.
- **Dropping `--test-threads=1` on the scheduler E2E** — moot under nextest (process per
  test). After step 4, run that binary twenty times under nextest and drop the flag if none
  fails.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-05 — ACCEPTED (decisions 1–7). Rollout steps 1–3 applied the same day; step 4
(regrouping) and step 5 (fixed waits) are open.** What landed: `.config/nextest.toml`;
`[profile.dev] debug = "line-tables-only"`; CI's `rust` job runs the suite once under nextest
with a JUnit artifact and keeps only the two regenerate-and-diff gates; coverage and embed-SPA
run on pull requests, `main` and manual runs; `CARGO_INCREMENTAL=0` throughout; `/gate`,
`CLAUDE.md`, `docs/TESTING.md`, `CONTRIBUTING.md`, the developer book, the `Makefile`, the PR
template, the release workflow, `scripts/regen-generated.sh`, the two live dispatch READMEs and
the three live board rows now name the nextest form and no `-p`. Handoffs, changelog history
and the archived board are untouched — they record what was true when written.

Two corrections to the text above:

- **(a)** The pre-push hook never ran the test suite and still does not — it runs the comment
  check, fmt, clippy and the generated-file freshness checks. A hook that takes a minute is a
  hook people bypass, and the suite is CI's job. Where decision 1 and rollout step 1 say the
  hook calls nextest, read "`make test`, `/gate`, the docs and CI call nextest".
- **(b)** The scheduler E2E's `--test-threads=1` is already gone from CI, on the proof step 4
  asked for: 20 consecutive full-suite runs under nextest with no isolation, zero failures of
  that binary.

Measured after steps 1–3, same machine: a cold build of the whole workspace plus the full run
takes **65 s** (`cargo nextest run --workspace` after a profile change); the freshly built
executables total **6.3 GB against 28 GB** before, `api_test` **344 → 139 MB**; a warm green run
prints ~150 tokens, a run that first rebuilds everything ~2.8k (the `Compiling` lines).

Findings the 20× loop produced beyond what it was run for:

- **One flaky test**, `tack-runner harness::codex::tests::a_configured_provider_request_spawns_with_its_endpoint_variable_present`,
  failed 4 of 20 runs, in 8 ms each — not timing. Cause: the test's shim writes `env > marker`
  on every invocation, and `CodexAdapter::start` invokes the same binary a second time —
  `--version`, for its version probe, with the probe's own empty environment — *after*
  spawning the run, so the two writes race and the marker holds whichever process finished
  last. Fixed in the test: the shim records only when invoked with the run's `exec`
  subcommand. The product path is unchanged. `claude_code.rs`'s sibling tests do not share the
  race — that adapter's `start` does not probe after spawning — and were left alone.
- **The runner's tests never remove their temp directories**: 10,882 `tack-runner-*` entries
  in `/tmp` on this machine (`ls /tmp | grep -c tack-runner-`). Not fixed here; a `TempDir`
  guard in each helper is the fix and belongs with step 4, which touches those files anyway.
- nextest reports 1,392 run + 6 skipped where `cargo test` listed 1,399: the one-test
  difference noted in "What was measured" is still unreconciled.

**Same day — correction to (b) above, which was wrong.** A second 20-run loop, after the
shim fix, failed once in `tack-cli::e6_scheduler_e2e_test` with `409 Agent profile name
already exists`: two of its tests, each in its own process under nextest, were handed the same
port by bind-then-drop and drove one server. The test's own comment calls that window
"acceptable for a single-process test suite" — a precondition nextest's process-per-test model
removes, so `--test-threads=1` on a sequential `cargo test` *was* load-bearing. Restored
declaratively: `.config/nextest.toml` gives that binary `threads-required =
"num-test-threads"`, so each of its tests runs with nothing alongside — the same guarantee,
without a separate CI step. Cost: ~3 s per full run (11.4 → 14.5 s). The durable fix is not in the
test: with `TACK_PORT=0` the server's log line and stdout banner both print the *configured*
port (`http://127.0.0.1:0`), and the bound address travels only over an in-process channel.
Printing the bound address is a small product change that would let the E2E use port 0 and
drop the override; it is not made here. The 40-run tally for that binary is therefore 1
failure, not the "zero of 20" claimed in (b).
With the override in place, a third 20-run loop of the full suite finished with 0 failures
(mean 14.5 s, max 15.0 s); the codex shim fix held across all 40 post-fix runs.

**Same day — a load-bearing number in this ADR was wrong, and the regrouping card that had to
live with it caught it.** "Fixed waits ≥ 200 ms in tests: 12" undercounted: the command beside
it, `sleep\(.*from_millis\([2-9][0-9]{2}`, cannot match a Rust digit separator, so every
`from_millis(1_400)`-style literal was invisible to it. The real count is **16**, and the four
it missed are the *longest* waits in the suite — 2.6 s, 2.4 s and two of 1.4 s, all in
`tack-orch`'s `ingestion`/`traces` pair. The ADR therefore under-reported precisely the sleeps
that explain its own "`traces_ingestion_test` is 2 tests in 4.07 s" measurement. Decision 6 and
the measurement table above are corrected in place, with the working command; this paragraph
records that they were wrong first. A fifth separator-style literal, `from_millis(1_500)` in
`docket_tick_contract_test.rs`, sits in a protected oracle and is outside any regrouping card's
scope.

**2026-09-05 — STEP 4 DONE, and step 5 is still open.** Five branches regrouped one crate's
tests each and merged into `develop` with no conflict; two more repaired what the regrouping
broke. Measured on the merge rather than taken from any card's report:

| | Before | After |
|---|---|---|
| Test binaries | 73 | **32** |
| `tack-api` / `tack-db` / `tack-orch` test files | 36 / 12 / 10 | 8 / 3 / 6 |
| Rebuild the suite after touching `tack-api/src/lib.rs` | 28.1 s | **4.89 s** |
| Rebuild the suite after touching `tack-core/src/lib.rs` | 21.0 s | **6.91 s** |
| Live test binaries on disk | 73, ~344 MB each | 32, 68 MB mean, **2.1 GB total** |
| Full run | 14.5 s | 15.3 s |
| Tests | 1,392 | 1,392 — every per-package count identical |

`target/debug/deps` still measures 34 GB because cargo never evicts the binaries of a layout
that no longer exists; 2.1 GB is the live set, and a `cargo clean` is what reconciles the two.

Decision 4 predicted "about five" binaries for `tack-api`; it is eight, because
`openapi_contract`, `wave2_gate` and `runner_vertical_slice` keep their own identity —
CI and `/gate` select the first two by binary name, so folding them in would have turned
those gates into silent no-ops.

**What the cards found that this ADR did not anticipate:**

- **Regrouping breaks every comment that names a moved file** — 93 of them, in production
  sources as much as in tests. The repair is now mechanical rather than remembered:
  `scripts/check-comments.sh` gained a rule that a filename cited in a comment must be a
  filename that exists, proven load-bearing by breaking a pointer and watching it fire. It
  matches on basename, so a name split across a line wrap hides from it — a reason not to
  wrap one. `foo.rs` is allowlisted as prose.
- **Four comments asserted things that were never true**, found by agents verifying a claim
  before repointing it rather than after: two named tests that never existed at any commit
  (`handlers/decisions.rs`), one claimed a Prometheus round-trip nothing tests
  (`handlers/orch.rs`), and one claimed a wiring test exercises `server.rs` when it builds
  its own config (`orchestration/control_plane/settings.rs`, repointed and flagged, not
  silently tightened). A matching false claim in `openapi.rs` is recorded and unfixed.
- **Two doc comments that fed `docs/openapi.json`** cited internal test files, so an
  implementation detail was published in the API spec. Rewritten to state behaviour, and the
  spec regenerated.
- **`ingestion_test.rs` was split, not renamed**, so git's rename detection could not map it
  and the mapping handed to the cards was silently incomplete for that one file. Both agents
  that hit it went and checked which destination still carried the assertions rather than
  guessing.
- **The runner's tests never remove their temp directories.** Still true, still unfixed:
  `/tmp` was cleared to zero and refilled with 3,252 `tack-runner-*` entries within minutes
  of the wave running the suite. A `TempDir` guard in each helper is the fix.

Step 5 (fixed waits) remains opportunistic and unstarted; the corrected count is 16, not 12.
