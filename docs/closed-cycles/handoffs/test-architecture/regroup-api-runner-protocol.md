# Regroup tack-api's runner-protocol and wiring test binaries

ADR 0064 rollout step 4, `tack-api` first slice. Scope: six files only —
`c2_handlers_test.rs`, `f1_decisions_test.rs`, `f2_artifact_events_test.rs` into a
new `runner_protocol` binary; `f6a_artifact_wiring_test.rs`, `f6b_model_wiring_test.rs`,
`f6d_execution_sweep_wiring_test.rs` into a new `wiring` binary.

## What moved where

```text
crates/tack-api/tests/runner_protocol.rs          (new binary root)
crates/tack-api/tests/runner_protocol/
  log_capture.rs   (new — the unified log-capture guard)
  lifecycle.rs     (was c2_handlers_test.rs)
  decisions.rs     (was f1_decisions_test.rs)
  artifact_events.rs (was f2_artifact_events_test.rs)

crates/tack-api/tests/wiring.rs                    (new binary root)
crates/tack-api/tests/wiring/
  artifact.rs        (was f6a_artifact_wiring_test.rs)
  model.rs           (was f6b_model_wiring_test.rs — pure rename, no content change)
  execution_sweep.rs (was f6d_execution_sweep_wiring_test.rs)
```

`c2_handlers_test.rs` was renamed to `lifecycle.rs`, not `handlers.rs`: the product
file `crates/tack-api/src/handlers.rs` is itself referenced by that bare name
throughout the codebase's doc comments (`c1_handlers_test.rs` and others), and this
file's own top doc comment already used "`handlers.rs`" to mean that product file.
Naming the test module `handlers.rs` too would have made every such cross-reference
ambiguous. `lifecycle.rs` matches its main test,
`full_runner_protocol_lifecycle_enroll_through_completion`.

## The hazard: unifying the three log-capture copies

`c2_handlers_test.rs`/`f1_decisions_test.rs`/`f2_artifact_events_test.rs` each
declared their own byte-for-byte-identical copy of a `LOG_CAPTURE` thread-local, a
`GLOBAL_LOG_CAPTURE_INIT: Once`, `ensure_global_log_capture_installed()`, a
`GlobalLogWriter`, and a `CaptureGuard` RAII type. All three are now one shared
module, `runner_protocol/log_capture.rs`, with `pub(crate)` on
`ensure_global_log_capture_installed` and `CaptureGuard` so `lifecycle`, `decisions`
and `artifact_events` can all `use crate::log_capture::{CaptureGuard,
ensure_global_log_capture_installed};`. No test body or assertion changed — only the
declaration site of this shared infrastructure moved.

The module's doc comment keeps the original technical explanation (why a
process-global default subscriber, not a thread-local `set_default`, is what
actually closes the `tracing` callsite-interest race) and adds one paragraph on what
the merge changes:

> This module used to be three separate copies, one per file, because each was its
> own compiled test binary and a `tracing` global default is process-wide — two
> binaries can't share one guard. Now that `lifecycle`, `decisions` and
> `artifact_events` are modules of the same binary, one copy covers all three, and
> more of this binary's tests share the one process it protects. Under nextest,
> where each test is its own process, the race this guard exists to close cannot
> occur at all — but under `cargo test`, which runs a whole binary's tests in one
> process, the guard is still doing real work.

Proved both ways, ten runs each, from a clean rebuild of just this binary:

```bash
for i in $(seq 1 10); do cargo nextest run --workspace -E 'binary(runner_protocol)' || echo "NEXTEST FAIL $i"; done
# run 1: OK ... run 10: OK — 0 failures, 94 tests/run

for i in $(seq 1 10); do cargo test -p tack-api --test runner_protocol >/dev/null 2>&1 || echo "CARGO TEST FAIL $i"; done
# run 1: OK ... run 10: OK — 0 failures, 94 tests/run ("test result: ok. 94 passed")
```

## A second, related hazard the ADR didn't name: `clippy::duplicate_mod`

`c2_handlers_test.rs` and `f2_artifact_events_test.rs` each independently load
`handlers/runner_protocol.rs` (and, transitively, its `artifact_storage.rs`,
`artifact_download.rs`, `retention.rs`, `runner_auth.rs` submodules) via their own
`#[path]` `mod runner_protocol;`. That was fine pre-merge — two separate crates,
each loading the file once. Merged into one `runner_protocol` binary, that becomes
two independent `#[path]` loads of the same file *in one crate*, which
`clippy::duplicate_mod` (part of `-D warnings`) rejects outright.

The clean fix — declare `mod runner_protocol;` once, at the binary's crate root, and
have both `lifecycle` and `artifact_events` `use crate::runner_protocol;` — compiles
and passes clippy, but **changes the test count**: `runner_protocol.rs`'s own
colocated unit tests (and its three submodules') stop being compiled twice, so
`cargo nextest list --workspace -E 'package(tack-api)'` drops from 469 to 448 — a
real reduction of 21, exactly the number of unit tests in that module tree. Those 21
were already being counted twice before this card (once via `tack-api`'s own lib
unit-test binary, once via each of `c2_handlers_test`'s and `f2_artifact_events_test`'s
independent `#[path]` copies) — the invariant's baseline of 469 already includes that
pre-existing duplication.

Hard invariant 1 requires the count stay exactly 469 both before and after, so the
clean fix is wrong here: I kept the two independent `#[path]` loads (matching
pre-merge behavior byte-for-byte) and instead silenced the now-visible-only-because-merged
lint with a targeted, commented `#[allow(clippy::duplicate_mod)]` on each of the two
`mod runner_protocol;` declarations (`lifecycle.rs` and `artifact_events.rs`).
`decisions.rs` hit a smaller version of the same thing —
`#[path = "../../src/handlers/decisions.rs"] mod decisions;` inside a file that is
itself the `decisions` module trips `clippy::module_inception` — fixed the same way,
with `#[allow(clippy::module_inception)]` and a one-line comment that the collision
is a coincidental subject-name match, not real nesting.

**Not fixed, flagged for whoever next touches this binary:** collapsing the
duplicate `runner_protocol.rs` load into one shared copy is the more correct
long-term shape (it's the same file, same types, no genuine reason for two
independent compilations of it) — it's blocked here only by the fixed-469 invariant.
If a future card is allowed to change that count deliberately (documenting the drop
to 448 as fixing this exact duplication), the two `#[allow(clippy::duplicate_mod)]`
sites in this handoff are exactly what to remove, together with the two
`mod runner_protocol;` declarations, replaced by one shared load.

## Private `setup()`/`real_app()` functions: evaluated, not swapped

All six files had (or, for the wiring three, still have) their own
file-private setup function. None were replaced with `common::test_app*`:

- `lifecycle.rs`, `decisions.rs`, `artifact_events.rs` each build a
  directly-constructed, isolated router from the loaded handler module
  (`runner_protocol::routes(...)` / `decisions::routes(...)`), deliberately bypassing
  `router.rs`/`require_token` — their own doc comments say so explicitly. That is not
  equivalent to `common::test_app*`, which builds the full production router.
- `wiring/artifact.rs`'s `real_app()` needs a caller-supplied `storage_dir`, which
  `common::test_app_with_config` doesn't expose a path for without extending it.
- `wiring/model.rs`'s `setup()` and `wiring/execution_sweep.rs`'s `setup()` both do
  materially more (or less) than `common::test_app*`: the former seeds a project/item
  after building the router: the latter builds no router at all, returning only
  `(Repository, String)`.

Per the card's own rule ("where a private setup differs in a way that matters, keep
it and say so") — all six are kept as-is.

## Test counts

Command: `cargo nextest list --workspace -E 'package(tack-api)' | wc -l` (stdout
only — `2>&1` merges in a `Finished`/warning/note line from cargo and inflates the
count by 3).

| | Count |
|---|---|
| Before (branch base `db113b8`) | 469 |
| After (this branch, final state) | 469 |

`ls crates/tack-api/tests/*.rs \| wc -l`: 36 → 32 (six files removed, two binary
roots added).

## Full verification run

```bash
./scripts/check-comments.sh                                       # pass
cargo fmt --all --check                                            # pass
cargo clippy --workspace --all-targets -- -D warnings               # pass
cargo nextest run --workspace                                       # 1392 run, 1392 passed, 6 skipped
cargo nextest list --workspace -E 'package(tack-api)' | wc -l        # 469
```

## Stale cross-references left alone (out of scope for this card)

Renaming the six files leaves their old names in comments elsewhere in the tree —
explicitly out of scope ("touch only the 6 files above and the new files you
create"). Not fixed here:

- `TODO.md:12498` — cites `c2_handlers_test.rs:1719` in a CI-run note (historical).
- `crates/tack-api/tests/wave2_gate.rs:37`, `crates/tack-api/tests/c5_integration_test.rs:3`,
  `crates/tack-api/tests/g2_chaos_security_test.rs` (lines 7, 34, 315) — all cite
  `c1_handlers_test.rs`/`c2_handlers_test.rs`/`f6a_artifact_wiring_test.rs` (the first
  of those three, `c1_handlers_test.rs`, is untouched and still correctly named).
- `crates/tack-api/src/execution_runtime.rs:412`, `crates/tack-api/src/handlers/decisions.rs`
  (lines 38, 68, 174), `crates/tack-api/src/handlers/executions.rs` (lines 39, 54, 85),
  `crates/tack-api/src/handlers/runner_protocol.rs` (lines 21, 123, 124, 505),
  `crates/tack-api/src/handlers/runner_protocol/retention.rs` (lines 42, 47, 48, 54),
  `crates/tack-api/src/handlers/runner_protocol/artifact_download.rs` (lines 10, 27, 28)
  — product-source doc comments citing the old test filenames.
- `docs/book/src/user-guide/agent-runners.md:409` — cites `f1_decisions_test.rs`/
  `f2_artifact_events_test.rs`.

None of these are wrong in substance (the described behavior is unchanged), only in
the filename they cite.
