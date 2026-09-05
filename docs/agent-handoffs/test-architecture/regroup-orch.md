# Regroup tack-orch's test binaries — ADR 0064 rollout step 4

Branch: `agent/regroup-orch`, based on `develop` at `db113b8`.

## What changed

`crates/tack-orch/tests/` went from 10 files (10 nextest binaries beyond the crate's own
lib tests) to 6: three left untouched because CI selects them by binary name, one left
standalone by choice (see below), and the remaining six regrouped into two.

| Before | After |
|---|---|
| `runner_contract.rs` + `runner_contract/` | unchanged |
| `docket_tick_contract_test.rs` | unchanged |
| `docket_wire_contract_test.rs` | unchanged |
| `docket_adapter_test.rs` | unchanged, standalone (see "docket_adapter_test.rs" below) |
| `scheduler_test.rs` | `scheduling.rs` → `scheduling/scheduler.rs` |
| `scheduler_wiring_test.rs` | `scheduling.rs` → `scheduling/wiring.rs` |
| `model_policy_test.rs` | `scheduling.rs` → `scheduling/policy.rs` |
| `ingestion_test.rs` | `ingestion.rs` → `ingestion/runs.rs` |
| `traces_ingestion_test.rs` | `ingestion.rs` → `ingestion/traces.rs` |
| `execution_retention_prod_test.rs` | `ingestion.rs` → `ingestion/retention.rs` |

Both `scheduling.rs` and `ingestion.rs` follow `runner_contract.rs`'s own shape exactly:
a thin entry point with `#[path = "…"]  mod …;` declarations, no logic of its own.

New shared fixture modules, both `pub(crate)`, both following the same
`use crate::support::{…}` convention `runner_contract`'s submodules already use for
`crate::fixtures::…`:

- `scheduling/support.rs` — `FixedClock`, `setup_repo`, `codex_capability_snapshot`,
  used by `wiring.rs` and `policy.rs`. `scheduler.rs` needs none of it (no repository at
  all — pure property tests against `tack_orch::scheduler`'s public functions).
- `ingestion/support.rs` — `setup_repo`, `seed_workspace`, `seed_project`, `seed_item`,
  `TestRepoStore` (the full `ControlPlaneStore` impl), `seed_control_plane_and_link`,
  and the `HEALTH_BODY`/`STATUS_BODY`/`EMPTY_APPROVALS_BODY` wiremock bodies, used by
  `runs.rs` and `traces.rs`. `retention.rs` needs none of it — it drives the spawned
  retention/health-watch tasks directly against its own seed.

## Why these groupings

`scheduling` bundles the scheduler's pure selection logic, its wiring against a real
repository, and model-policy resolution — three depths of the same question ("which
runner does a request land on"). `ingestion` bundles runs/approvals ingestion, trace
ingestion, and the production retention/health-watch tasks — three consumers of the same
`reconciler`/`tack_db::Repository` machinery, exercised end-to-end rather than against the
fake stores each module's own `#[cfg(test)]` block already covers.

## docket_adapter_test.rs — left alone, not folded into ingestion

The card's default placement was `tests/ingestion/`, with an explicit invitation to
reconsider. The code argues against the default: `docket_adapter_test.rs` shares **no**
fixture with the other three ingestion-group files — no `Repository`, no migrations, no
reconciler, nothing seeded. It drives `DocketAdapter` directly against `wiremock`
fixtures. Both `docket_wire_contract_test.rs` and `docket_tick_contract_test.rs` — the two
protected oracles — cross-reference it by name in their own module docs as the
predecessor suite their per-method and per-tick goldens supersede for wire-shape
assertions (`grep -n docket_adapter_test.rs crates/tack-orch/tests/docket_*.rs`). It reads
as the third leg of that trio, not as an ingestion-pipeline test. Since folding it into
either protected binary is explicitly forbidden (it would silently widen what
`binary(docket_wire_contract_test)`/`binary(docket_tick_contract_test)` select in CI), the
only change consistent with the card's rule is: leave it exactly as its own binary. It was
not moved, renamed, or edited.

## Setup helpers: unified vs. kept separate

Both `scheduler_wiring_test.rs`/`model_policy_test.rs` and
`ingestion_test.rs`/`traces_ingestion_test.rs` had "near-identical setup helpers" per the
card's framing. Diffing each pair by hand (byte-for-byte, not by eye) before merging:

**Unified** (genuinely duplicate — identical except for cosmetic literals or a doc
comment, never asserted on):
- `scheduling`: `FixedClock` (byte-identical in both files) and `codex_capability_snapshot`
  (byte-identical) moved as-is. `setup_repo` differed only in the workspace/project name
  (`"Wiring"` vs `"F3"`) and the seeded item's title (`"Scheduler wiring proof"` vs
  `"Model policy proof"`) — confirmed by grep that neither string is read or asserted
  anywhere outside the function itself, so one canonical version
  (`"Scheduling"`/`"Scheduling fixture item"`) now serves both call sites. Every call site
  (`setup_repo().await`, `FixedClock(now)`, `codex_capability_snapshot(now)`) is
  byte-identical to before the move — only the `use` block changed, confirmed by
  `git diff` showing every hunk lands before each file's first `#[tokio::test]`.
- `ingestion`: `setup_repo`, `seed_workspace`, `seed_project`, `seed_item`,
  `seed_control_plane_and_link`, and the `HEALTH_BODY`/`STATUS_BODY`/`EMPTY_APPROVALS_BODY`
  consts were byte-identical between the two files. `TestRepoStore`'s trait impl was also
  byte-identical except for two comments in `ingestion_test.rs` explaining why
  `upsert_metrics`/trace-cursor methods look unused *in that file* — those comments no
  longer apply once the impl is shared by a file that does exercise them, so they were
  dropped from the shared impl (nothing said there was lost: it's restated, file-neutral,
  in `support.rs`'s own module doc). The "mount health+status" half of `mount_common` was
  also identical between the two files' original definitions and was extracted as
  `mount_health_and_status`; `runs.rs` imports it aliased back to the name `mount_common`
  (`use crate::support::mount_health_and_status as mount_common;`) so its test bodies
  still read `mount_common(&server).await`, unchanged.

**Kept separate** (differ meaningfully — the card's own escape hatch):
- `scheduling`: `register_active_runner` — `wiring.rs`'s version takes a `capacity: i64`
  parameter and registers a runner at that capacity; `policy.rs`'s version has no such
  parameter and always registers at capacity 1. Forcing one shape onto the other would
  mean editing every call site inside a test body to add or drop an argument, which the
  card's invariant on test bodies forbids; the two stay as same-named, differently-shaped
  private functions in their own modules (no collision — different modules). Likewise
  `enqueue` (11 params, any selector kind, JSON `metadata` parsed at call time) vs.
  `enqueue_via_fleet` (9 params, fleet selector only, metadata always `{}`) — different
  enough in both shape and the JSON they build that unifying would change what gets
  written to `execution_requests.request_snapshot`, which is exactly the kind of behavior
  change the card forbids touching.
- `ingestion`: `traces.rs`'s `mount_common` mounts two more endpoints
  (`/runs`, `/approvals`, both stubbed empty) than `runs.rs`'s ever needed, because a
  linked project makes the reconciler's `poll_runs` fire even when a test only cares about
  traces — without those extra mocks, `traces.rs`'s tests would generate mock-miss noise
  from unrelated polling. `traces.rs` keeps its own `mount_common` that calls the shared
  `mount_health_and_status` first and then mounts the two extra stubs itself, rather than
  duplicating the health/status pair — only the genuinely-different part stays local.
  `EMPTY_RUNS_BODY` (only ever used by that extra mock) stayed local to `traces.rs` rather
  than joining the shared consts, matching the same logic.

## Invariant checks

```
$ export CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/regroup-orch
$ cargo nextest list --workspace -E 'package(tack-orch)' | wc -l
282                                                                    # before, on db113b8
282                                                                    # after, post-regroup
```

```
$ ./scripts/check-comments.sh
✓ no board archaeology in crates/
```

```
$ cargo fmt --all --check
(clean, no output)
```

```
$ cargo clippy --workspace --all-targets -- -D warnings
(clean — only the pre-existing proc-macro-error2 future-incompat notice, unrelated to this change)
```

```
$ cargo nextest run --workspace
Summary [ 25.224s] 1392 tests run: 1392 passed, 6 skipped
```

```
$ cargo nextest run --workspace -E 'binary(runner_contract)'
Summary [ 0.022s] 18 tests run: 18 passed, 0 skipped
```

```
$ UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test) | binary(docket_wire_contract_test)'
Summary [ 0.298s] 18 tests run: 18 passed, 0 skipped
$ git diff --exit-code crates/tack-orch/tests/golden/
(clean — golden files byte-identical; exit 0)
```

`git status --porcelain` touches only `crates/tack-orch/tests/**` and this handoff — no
`.config/nextest.toml`, `.github/`, `Cargo.toml`, or `docs/contracts/` diff.

## Fixed waits found (not fixed here — step 5's job)

Nine `tokio::time::sleep` calls at 200 ms or more across the three files this card moved
(`grep -rnE 'sleep\(.*from_millis\(([0-9_]+)' crates/tack-orch/tests/ingestion/{runs,traces,retention}.rs`,
read by hand since the ADR's own grep pattern — see finding below — misses four of them):

| File:line | Wait | Looks like it should become |
|---|---|---|
| `ingestion/runs.rs:107` | 400 ms | bounded poll on `get_orch_run("run-1")`/`get_orch_run("run-cli-only")` returning `Some` |
| `ingestion/runs.rs:144` | 1,400 ms | bounded poll on the post-tick row counts staying stable across N ticks (harder — see below) |
| `ingestion/runs.rs:268` | 400 ms | bounded poll on `get_orch_run("run-1")` reaching the first-poll attribution |
| `ingestion/runs.rs:277` | 2,400 ms | same shape as `runs.rs:144` — proving idempotency across several ticks, not one condition |
| `ingestion/traces.rs:155` | 2,600 ms | bounded poll on `orch_events` count reaching 2 |
| `ingestion/traces.rs:195` | 1,400 ms | bounded poll on the rewound-cursor re-poll producing no new rows (same "prove nothing changed" shape as below) |
| `ingestion/traces.rs:271` | 400 ms | bounded poll on `orch_events` count reaching 1 |
| `ingestion/traces.rs:327` | 400 ms | bounded poll on the re-ingest attempt's count staying at 0 |
| `ingestion/retention.rs:208` | 200 ms | not obviously a poll target — this one asserts a *negative* (nothing purges after the sweep's `JoinHandle` was joined), so there is no "true" condition to poll for; shrinking the fixed wait or restructuring the proof (e.g. a second explicit tick attempt that must observably no-op) is a smaller, different change than the other eight |

`retention.rs` already has the *right* pattern twice (lines 184 and 294): a bounded
`for _ in 0..300 { if <condition> { break; } sleep(10ms) }` loop with a real timeout and a
message naming what was waited for. That's the shape ADR 0064 step 5 should turn the
eight `runs.rs`/`traces.rs` waits above into — most of them wait for one row to appear,
which is a direct fit; the four marked "prove nothing changed"/"across N ticks" are
proving an *absence* or a *repeated* poll's idempotency, which don't reduce to "poll until
X is true" as directly and may need a different shape (e.g. poll for the expected state,
then hold for one more short window confirming it doesn't regress) — flagging that
distinction rather than picking a design for whoever takes step 5.

**A load-bearing-number finding along the way:** the ADR's own count ("12 sleep(200-500 ms)
… `grep -rnE 'sleep\(.*from_millis\([2-9][0-9]{2}' crates/*/tests`") undercounts within
this crate's three files. That exact pattern only matches a milliseconds literal that
starts with a bare digit 2-9 immediately followed by two more digits — so it catches the
four plain `400`/`200` literals here but misses `1_400`, `2_400`, `2_600` and the second
`1_400` entirely, because Rust's digit-separator underscore sits right after the first
digit and breaks the `[0-9]{2}` match. Re-running the grep against this crate's three
files gives 5 hits; reading every `sleep(` call by hand gives 9 real fixed waits of 200 ms
or more (8 in `runs.rs`/`traces.rs`, 1 in `retention.rs`) — the same undercount shape
CLAUDE.md's own rule warns about, here in the ADR's supporting measurement rather than in
a doc's headline claim.

## What was not touched, and why

- `runner_contract.rs` + `runner_contract/`, `docket_tick_contract_test.rs`,
  `docket_wire_contract_test.rs`: untouched, per the card's hard rule (CI selects them by
  binary name).
- `docket_adapter_test.rs`: untouched, left standalone — see above.
- No test body, assertion, test name, or `#[test]`/`#[tokio::test]` attribute changed
  anywhere; every diff in the six moved/edited files lands strictly before that file's
  first test function (verified per-file with `git diff`, not just asserted).
- `.config/nextest.toml`, `.github/`, `Cargo.toml`, `docs/contracts/`: no diff.
