# III-F6 handoff — Wave 5 integration

- **Base SHA / branch / final SHA:** base `cbdd4a3` (Wave 4 close-out on
  `plan/harness-agnostic-agent-fleet`); branch `agent/iii-f6-integration`; final SHA recorded
  in `TODO.md`'s Wave 5 status row.
- **Cards integrated:** III-F1 `7ce2e5f`, III-F3 `802d4c3`, III-F5 `b3e8b3c`, III-F2 `9df4c6a`,
  plus four integrator sub-cards dispatched during integration (F6a/F6b/F6d/F6e, below).

## Integration commits

| SHA | What |
|---|---|
| `3e85977` `d9eff69` `75f5ad8` `2d6ef18` | The four card merges (F1 → F3 → F5 → F2), all clean |
| `2689ed7` | Mount F1's decision-resolve route + `TACK_EXECUTION_DECISION_TOKEN`; flip `TACK_EXECUTION_RETENTION_ENABLE` to default off. **Also silently carried F2's two wiring requests** — see amendment 1 |
| `d09680b` | III-F6a: real-router proof of the artifact storage/download wiring |
| `5e8a92c` | Correct `artifact_download.rs`'s stale "not mounted" doc claims |
| `2cf3000` | III-F6b: F3 model policy on the live create path + `AttemptSummary` economics |
| `251ce55` | III-F6e: document the Wave 5 route surface; regenerate both artifacts; fix the runner-v1 error envelope |
| `e31c97a` | III-F6d: wire the orphaned artifact sweep + decision expiry into `ExecutionRuntime` |
| `2490c8a` | Correct `CLAUDE.md`'s retention scope; `cargo fmt` |

## Two integrator decisions that were not in any card

**1. `TACK_EXECUTION_DECISION_TOKEN` (new).** III-F1 stopped and escalated rather than
inventing config, correctly: `docs/contracts/runner-v1/protocol.json` specifies decision
resolution behind a `separately_scoped_operator_credential` with
`required_scope: "operator:decisions"`, and **Tack has no scope system**. Resolved by adding a
second, independent shared secret layered on top of the operator token, mirroring
`TACK_ORCH_APPROVAL_TOKEN` exactly — distinct from `TACK_API_TOKEN`, **fail-closed when
unset** (the route rejects rather than falling back to the operator token), never logged.
This is a narrower grant than the contract's scope model, not an implementation of it; a real
scope system remains open.

**2. `TACK_EXECUTION_RETENTION_ENABLE` defaults to `false`.** III-F5 shipped it `true`. This
sweep deletes rows and, since F6d, on-disk blobs — data deletion must be an explicit operator
opt-in, matching `TACK_ORCH_ENABLE`'s own off-by-default posture.
`TACK_EXECUTION_HEALTH_ENABLE` stays default `true`: it reads and logs only.

## Behavior implemented (integrator sub-cards)

- **F6a** — proved F2's artifact storage root and download route are wired on the *real*
  production router. Tests drive only public `build_router`/`AppState` with zero card-local
  scaffolding, unlike every prior artifact test.
- **F6b** — granted F3's wiring requests 3 and 4. `resolve_request_model_policy` now runs from
  `POST /api/executions` when the client omits both model fields; an explicit client choice is
  never overridden. `AttemptSummary` carries `model_provenance`/`usage_economics`.
- **F6d** — wired `sweep_events`, `sweep_artifacts` (F2) and `expire_overdue_decisions` (F1)
  into `ExecutionRuntime` as a third cancellable task, joined on shutdown. See amendment 2.
- **F6e** — documented three mounted-but-unspecified paths and regenerated both artifacts. See
  amendment 3.

## Amendments — claims made earlier in this wave that turned out false

**Amendment 1 — `2689ed7`'s commit message under-describes its own diff.** It says it mounts
F1's decision route and gates retention. It *also* carried both of III-F2's production-wiring
requests (artifact storage root + download route mount). The message was written from an
interrupted session's stopping point, not from the diff. `III-F2.md` still frames both
requests as outstanding; they are granted. Left standing rather than rewritten, per III.2's
rule that corrections are appended.

**Amendment 2 — III-F2's `retention.rs` claimed its sweeps were "exercised by this card's own
`f2_artifact_events_test.rs`". False.** A repo-wide grep found `sweep_events`/`sweep_artifacts`
had **zero callers anywhere**, including in that test — only `RetentionPolicy::default()` was
covered. Combined with F5 having been authored before F2 existed (so F5's retention mechanism
never knew `execution_artifacts` existed), an operator enabling retention got replay and event
purging while artifact rows and their blobs — the largest consumers in this domain — grew
without bound. Closed by F6d.

**Amendment 3 — `CLAUDE.md`'s "13 `/api/runner/v1` paths" was aspirational, not measured.**
`docs/openapi.json` documented only 12; `PUT /attempts/{id}/artifacts/{artifact_id}/content`
was mounted and unspecified. Separately, **all 13 runner-protocol operations documented their
errors as `ErrorEnvelope`** (`{status, message, code?}`, the ordinary REST shape) when every
runner-protocol handler returns `ProtocolErrorEnvelope`
(`{code, message, request_id, retryable, details}`). Any client generated from the spec would
have failed to parse every runner error. Pre-existing, predating Wave 5; fixed in `251ce55`.

**Amendment 4 — giving `sweep_artifacts` a production caller exposed a race that could not
exist before.** `list → delete blob → delete rows` is not one transaction. An artifact listed
as unresolved can have its content uploaded between the list and the delete; the unconditional
delete then drops the row while leaving the blob orphaned forever, since nothing will
reference it again. Closed with a guarded atomic
`DELETE FROM execution_artifacts WHERE content_reference IS NULL AND id IN (...)`. A test pins
the *old* method's behavior to prove the defect was real rather than theoretical. A narrower
residual window remains (the guarded `DELETE`'s instant vs. the upload handler's
blob-write-then-commit two-step) — judged acceptable, and documented, because it shrinks the
window from a whole batch pass over up to 500 rows to a single SQL statement.

## Tests added and exact commands/results

```
cargo test --workspace                  1289 passed, 0 failed (74 test binaries)
cargo clippy --workspace --all-targets  clean
cargo fmt --all -- --check              clean
cargo test -p tack-api --test wave2_gate        5/5
cargo test -p tack-orch --test runner_contract  18/18 (all 46 frozen fixtures byte-pinned)
cargo test -p tack-api --test openapi_contract  5/5, no drift
cargo test -p tack-cli --test e6_scheduler_e2e_test -- --test-threads=1   5/5 × 3 consecutive runs
cd frontend && npm run type-check       exit 0
```

New test files: `f6a_artifact_wiring_test.rs`, `f6b_model_wiring_test.rs`,
`f6d_execution_sweep_wiring_test.rs`, plus additions to
`crates/tack-db/tests/f2_event_artifact_retention_test.rs`.

**Observed flake, recorded not hidden:** `e6_scheduler_e2e_test` failed 1/5 once, during a run
that overlapped a sibling agent's concurrent build — that run took 15.0s against 6.6s
unloaded. It spawns real subprocesses on real timeouts, so CPU contention reproduces its
documented `--test-threads=1` flakiness class. 15/15 across three isolated runs. **Not
weakened, not skipped.**

## Failure/adversarial cases proved

Each guard below was proved load-bearing by reverting it and watching the test fail:

- Unmount the download route → 404; remove the storage-root call → content silently falls back
  to `./storage` and the configured dir is never created.
- Disable the model-policy branch → the stored DB row's model columns are `None`; hardcode
  provenance to null → the two provenance tests fail while the in-flight-attempt test
  correctly still passes (null is honest there), showing the tests discriminate on real signal.
- Revert the artifact-delete guard to unconditional → the racily-resolved row is deleted
  (`left: 1, right: 0`).
- Force the sweep's `enabled` argument to `false` (the pre-F6d state) → the three
  retention-enabled tests fail; the two disabled-path tests still pass.
- An unconfigured `TACK_EXECUTION_DECISION_TOKEN` rejects every resolve and writes nothing; a
  valid *runner* bearer credential cannot self-resolve a decision.

## Schema/API/contract changes requested from another owner

None outstanding from this card. `docs/contracts/runner-v1/` was not touched — no fixture was
added or edited, and `runner_contract.rs`'s pin table is unchanged.

## Known limitations / `not_measured` fields

- `usage_economics.runner_time_cost.cost_usd_estimated` is **always**
  `{value: null, source: "not_measured"}` in production. No runner infra cost-rate is stored
  anywhere in the schema. Documented as `type: [number, null]`; a `0` here would be a lie about
  money.
- `ModelPolicySources.project_default` is modeled but always `None` — `projects` has no
  default-model-policy storage. Deferred to a future migration batch, as III-F3 recorded.
- `model_profiles` (migration 043) is still consulted by nothing.
- Decision expiry shares `TACK_EXECUTION_RETENTION_ENABLE` rather than having its own
  always-on gate. Pinned by a test so changing that posture is a reviewed diff.
- `#[allow(dead_code)]` remains on `SweepOutcome`/`sweep_events`/`sweep_artifacts` — not a
  revert of the wiring. Two pre-existing test binaries load `runner_protocol.rs` via `#[path]`
  without `execution_runtime.rs`, so the functions are genuinely dead *in those binaries*. Same
  per-compiled-binary precedent `artifact_download.rs` already established.

## Secrets/logging review

`TACK_EXECUTION_DECISION_TOKEN` is never logged and is fail-closed when unset. Redaction tests
in `f1_decisions_test.rs` and `f2_artifact_events_test.rs` capture real `tracing` output,
assert secret markers never appear, **and** assert an id *does* appear — a positive control
proving the capture rig is not vacuously observing nothing. Logs carry ids only.

## Safe merge order and likely conflicts

Merged F1 → F3 → F5 → F2, then F6a → F6b → F6e → F6d; all clean. The only overlapping files
across cards were `crates/tack-db/src/repo/execution.rs` and `crates/tack-orch/src/lib.rs`,
both append-style (union resolution).

## Checklist

No unowned files edited by any sub-card · no live secret · no panic stub · no blind retry ·
no fixture edited · both generated artifacts regenerated by the documented commands, never
hand-edited.
