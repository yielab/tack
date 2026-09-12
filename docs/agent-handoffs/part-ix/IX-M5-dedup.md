# IX-M5-dedup handoff

**Phase 4 of 4 for card IX-M5 (Wave 30), and its closing phase.** Phases 1-2 extracted
`LocalProcessHarness<G: HarnessGrammar>` and migrated both adapters onto it with no
behavior change. Phases 3a/3b (parallel) pruned each adapter's own tests to the audit's
§5 shape independently and flagged suspected cross-adapter duplicates for later. This
phase re-derives the real duplicate list with the actual command (not just the two
phase-3 lists, which predate the other adapter's pruning), consolidates genuine
duplicates into `harness/tests.rs`'s existing shared-suite precedent, and — as
best-effort — extracts two more genuinely-identical pieces of `codex.rs`/`claude_code.rs`
production code into `local_process.rs`.

- Base SHA / branch: `b04a295` (the tip of `agent/ix-m5-dedup` at the start of this
  phase — the branch already carried phases 1-3's merged work, including the
  `tests/live/` unification fix-up commits) / `agent/ix-m5-dedup` (worktree:
  `/tmp/ix-m5-dedup`, `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m5-dedup`). Final SHA: see
  the commit this handoff ships in (not merged — lands on this branch only, per the
  card's instructions).
- Files changed:
  - `crates/tack-runner/src/harness/codex/tests.rs` — 4 tests removed (consolidated
    into `harness/tests.rs` or genuinely superseded), 1 renamed and narrowed
    (`reconcile_reports_real_process_liveness_by_pid` →
    `reconcile_trusts_a_live_pid_unconditionally`, keeping only its alive-pid half),
    2 now-orphaned helpers (`wait_for_pidfile`/`wait_until_dead`) deleted (moved to
    `process/tests.rs`, made `pub(crate)`, reused from `harness/tests.rs`), the
    now-unused `LocalRunHandle` test re-export dropped, `resolve_provider_endpoint`
    and `stage_run_log` bodies replaced with calls to two new shared
    `local_process.rs` functions.
  - `crates/tack-runner/src/harness/claude_code/tests.rs` — 3 tests removed
    (consolidated or superseded), same now-unused `LocalRunHandle` re-export dropped.
  - `crates/tack-runner/src/harness/codex.rs` — the `LocalRunHandle` test re-export
    line and an unused `ArtifactStager` import removed; `resolve_provider_endpoint`
    and `stage_run_log` shrunk to thin wrappers over the new shared functions.
  - `crates/tack-runner/src/harness/claude_code.rs` — same shape of change as
    `codex.rs`: `LocalRunHandle` re-export removed, `resolve_provider_endpoint` and
    `stage_run_log` shrunk to wrappers.
  - `crates/tack-runner/src/harness/local_process.rs` — two new `pub(crate)`
    functions: `resolve_provider_endpoint` (the resolve-and-reject-typed wrapper both
    grammars called identically around `provider::resolve_endpoint`) and
    `stage_run_log` (the stage-a-log-artifact body both grammars implemented
    near-identically, parameterized by staging root, log filename and harness kind).
  - `crates/tack-runner/src/harness/tests.rs` — 4 new cross-adapter acceptance tests
    (see "What moved where" below), a new `real_adapters_for`/`real_adapter_spec_with_env`
    builder pair, reusing `process::tests`'s pidfile-poll helpers rather than adding a
    third copy.
  - `crates/tack-runner/src/harness/process.rs` — `mod tests;` → `pub(crate) mod
    tests;`, so `harness::tests` can reach its helpers.
  - `crates/tack-runner/src/harness/process/tests.rs` — `wait_for_pidfile`/
    `wait_until_dead` made `pub(crate)`, with a one-line doc note each explaining why.
  - `crates/tack-runner/tests/live/**`, `harness/fixtures/**` — **not touched**, per
    the card's explicit scope.
- Contract fixtures consumed: none.
- Behavior implemented: none — every claim a removed test proved is still proven,
  either as a consolidated cross-adapter test or (for the two production-code
  extractions) as the exact same logic behind a thinner call site, proved by the full
  suite holding and by `codex/tests.rs`'s/`claude_code/tests.rs`'s own remaining
  provider-endpoint and artifact-staging tests still passing unchanged.

## The real duplicate list, re-derived

The two phase-3 handoffs' own "suspected duplicate" lists were written before the
*other* adapter's pruning landed, so per the card's instruction they were not trusted
as exhaustive. `python3 scripts/maintainability.py duplicate-tests` at this phase's
start (default ratio 0.75), restricted to what phase 3 left behind:

```
tack-runner: 3 near-identical pairs across files (ratio > 0.75)
  a_disabled_provider_rejects_the_request_before_any_process_spawns (claude_code/tests.rs)
  disabled_provider_rejects_the_request_before_any_process_spawns (codex/tests.rs)

  cancel_kills_the_whole_descendant_tree_via_the_adapter (codex/tests.rs)
  cancel_kills_the_whole_descendant_tree_not_only_the_direct_child (process/tests.rs)

  a_malformed_body_is_a_parse_error_not_a_panic (provider/anthropic.rs)
  a_malformed_body_is_a_parse_error_not_a_panic (provider/vercel_ai_gateway/tests.rs)
```

The third pair is outside this phase's files (`provider/`, not `harness/`) and is not
touched. Re-running at a looser ratio (`--ratio 0.55`, scoped to `crates/tack-runner/src/harness`)
to check the tool was not silently hiding lower-similarity duplicates the phase-3
handoffs had already named by hand surfaced 7 more candidates. Each was inspected by
reading both bodies, not by name alone:

| Pair | Verdict | Why |
|---|---|---|
| `cancel_and_wait_on_an_untracked_handle_are_typed_rejections` (codex) / `cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic` (claude_code) | **Duplicate — consolidated.** | Both assert `take_running`'s rejection (`Err(HarnessError::Process)`) on a handle never returned by `start()` — 100% `local_process.rs` shared code, no grammar involved at all. |
| `reconcile_handles_a_missing_or_unrecognized_process_id` + the dead-pid half of `reconcile_reports_real_process_liveness_by_pid` (codex) / `reconcile_reports_honest_observations_for_untrackable_pids` (claude_code, 3 rows) | **Duplicate — consolidated.** | Traced each row against `local_process.rs::reconcile`: "no recorded pid" short-circuits before any decode; "undecodable" and "decodable-but-dead" both resolve inside the shared `process_alive`/`decode_handle` plumbing, entirely before `reconcile_alive`/`reconcile_unavailable` are called. Codex's own "dead pid" case (spawn+reap a real process) is a stronger proof than claude-code's magic-number sentinel (`"2000000000"`) — the consolidated test uses the real-process technique for both adapters. |
| `disabled_provider_rejects_the_request_before_any_process_spawns` (both, already flagged by phase 3) | **Duplicate — consolidated.** | The actual "disabled → reject" check is `provider::resolve_endpoint`'s own (already one shared function); the claim under test is that `local_process.rs`'s shared `validate` surfaces that rejection before any spawn, for either grammar. |
| `cancel_kills_the_whole_descendant_tree_via_the_adapter` (codex) / `..._not_only_the_direct_child` (process/tests.rs, already flagged by phase 3) | **Not the same layer, but codex's copy moved anyway.** | `process/tests.rs`'s version proves the raw primitive (`SupervisedProcess::cancel` kills a process group) — a real, distinct, still-needed claim, left alone. Codex's version proves the same outcome reached through the *full* `start`/`cancel` path; re-reading it, that is entirely `local_process.rs`'s shared `cancel()` plus `process.rs`'s shared signal — nothing codex-specific — so it moved to `harness/tests.rs` and was extended to cover *both* real adapters (claude-code never had this test at all; its own comment already said the primitive-level proof was sufficient). This resolves the pre-existing pair as a side effect, but the reason for moving it is audit rule 1, not the pair count. |
| `probe_attests_model_passthrough_when_no_models_are_enumerable` (codex) / `probe_attests_model_passthrough_instead_of_inventing_a_model_list` (claude_code) | **Not a duplicate — left as-is.** | Both share one generic assertion (`capability.model_combinations.is_empty()`, truly shared `LocalProcessHarness::probe()` behavior), but the load-bearing part of each — `model_passthrough().reason` text, and claude-code's additional `model_discovery_note` assertion — is genuinely per-grammar attestation data. Extracting only the shared one-liner into `harness/tests.rs` would proliferate coverage, not consolidate it, for a claim so cheap it is not worth a second test. |
| `provider_endpoint_variables_present_only_when_configured_and_requested` (claude_code) / `provider_endpoint_credential_reaches_the_process_only_when_the_request_names_it` (codex) | **Not a duplicate — left as-is.** | Both drive a real spawn and check for a specific env var name: codex checks `AI_GATEWAY_API_KEY` (one name, its own convention); claude-code checks `ANTHROPIC_BASE_URL` **and** `ANTHROPIC_AUTH_TOKEN` (two names, its own convention). The env-var names and count are `prepare()`'s own construction — entirely grammar-owned, not shared plumbing — despite the near-identical test shape. |
| `secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact` (codex) / `secret_canaries_never_survive_into_captured_output_or_spec_debug` (process/tests.rs) | **Not a duplicate — left as-is.** | Different layers: `process/tests.rs` proves redaction of the raw `ProcessResult`/`Debug` output; codex's proves redaction survives all the way through `outcome()`'s `terminal_reason` and the staged log artifact — a materially different, still-necessary claim (an adapter-level regression here would not be caught by the primitive-level test alone). |
| `probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success` (codex) / `capabilities_reports_an_honest_probe_error_for_an_uninstalled_harness_never_a_fake_success` (harness/tests.rs) | **Not a duplicate — left as-is.** | The `harness/tests.rs` version drives a synthetic `FakeProbe` through `AdapterRegistry` (dispatch-level: "the registry reports whatever the probe says"); codex's own version drives a *real* `CodexAdapter::probe()` against a genuinely absent binary path (adapter-level: "this adapter's own probe path never fabricates a version"). Different layers, different failure modes covered. |
| `reconcile_handles_a_missing_or_unrecognized_process_id` / `reconcile_reports_real_process_liveness_by_pid` (codex, pre-consolidation) vs `reconcile_with_no_recorded_process_id_needs_no_dispatch` (harness/tests.rs) | **Not a duplicate — no action needed (the codex tests these named no longer exist post-consolidation).** | `harness/tests.rs`'s pre-existing test proves `AdapterRegistry::reconcile`'s own, separate "no pid → no dispatch at all" short-circuit (a registry-level optimization, distinct code from `local_process.rs`'s own identical-shaped check) — a real, different layer, confirmed by reading `AdapterRegistry::reconcile` directly. |

`duplicate-tests` at this phase's end (default ratio, full workspace):

```
tack-runner: 1 near-identical pairs across files (ratio > 0.75)
  a_malformed_body_is_a_parse_error_not_a_panic (provider/anthropic.rs)
  a_malformed_body_is_a_parse_error_not_a_panic (provider/vercel_ai_gateway/tests.rs)

49 pairs in total (workspace-wide)
```

**Zero pairs remain involving `codex`/`claude_code`/`local_process`/`harness` files.**
The one remaining `tack-runner` pair is in `provider/`, pre-existing, and out of this
phase's (and this card's) scope. Before this phase, the workspace total stood at the
same shape reported at the end of phase 3b (~51-52 pairs, `tack-db` 3, `tack-orch` 5,
`tack-runner` 3-4, others 0); after, `tack-runner` dropped to 1 and the workspace total
to 49 — every crate outside `tack-runner`'s harness files is unaffected by this phase
and not this phase's job.

## What moved where, and why

Four new tests in `harness/tests.rs`, all following the file's own pre-existing
cross-adapter-acceptance convention (`for_fixture`, real adapters, not the trait-level
`TaggedFakeAdapter`/`FakeProbe` used by the dispatch-level tests above them in the same
file):

1. **`disabled_provider_rejects_both_real_adapters_before_any_process_spawns`** —
   replaces `codex/tests.rs::disabled_provider_rejects_the_request_before_any_process_spawns`
   and `claude_code/tests.rs::a_disabled_provider_rejects_the_request_before_any_process_spawns`.
2. **`cancel_and_wait_on_an_untracked_handle_are_typed_rejections_for_both_real_adapters`**
   — replaces `codex/tests.rs::cancel_and_wait_on_an_untracked_handle_are_typed_rejections`
   and `claude_code/tests.rs::cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic`
   (the codex version's own `wait` assertion is now checked for both adapters, not
   dropped).
3. **`cancel_kills_the_whole_descendant_tree_via_both_real_adapters`** — replaces
   `codex/tests.rs::cancel_kills_the_whole_descendant_tree_via_the_adapter`; claude-code
   gets this coverage for the first time (it never had its own copy).
4. **`reconcile_reports_shared_pid_plumbing_identically_for_both_real_adapters`** —
   replaces `codex/tests.rs::reconcile_handles_a_missing_or_unrecognized_process_id` in
   full, the dead-pid half of `reconcile_reports_real_process_liveness_by_pid` (which
   stays, narrowed and renamed `reconcile_trusts_a_live_pid_unconditionally`, for its
   own alive-pid claim), and all three rows of
   `claude_code/tests.rs::reconcile_reports_honest_observations_for_untrackable_pids`.

A `real_adapters_for(program, args, secrets_dir) -> (CodexAdapter, ClaudeCodeAdapter,
TempDir)` helper backs all four, mirroring the file's own existing
`cross_adapter_fixture_command`/`cross_adapter_secret_store` helpers rather than
duplicating adapter-construction boilerplate a third time. `real_adapter_spec` gained a
sibling, `real_adapter_spec_with_env`, so the descendant-tree test can drive the shared
fake-harness fixture's `TACK_FAKE_HARNESS_*` mode switches through the same builder the
file's other tests already use — `real_adapter_spec` itself is now a thin wrapper
calling it with an empty environment, so its two pre-existing call sites needed no edit.

`process/tests.rs`'s own `wait_for_pidfile`/`wait_until_dead` (a pidfile-polling
helper pair `codex/tests.rs` used to duplicate) are now `pub(crate)` and reused
directly by the new descendant-tree test, rather than adding a third copy — `codex/tests.rs`'s
own copies are deleted (no longer used, since its `cancel_kills_the_whole_descendant_tree_*`
test moved).

## No claim silently dropped

| Original test | Status |
|---|---|
| `codex/tests.rs::disabled_provider_rejects_the_request_before_any_process_spawns` | → shared test 1 |
| `claude_code/tests.rs::a_disabled_provider_rejects_the_request_before_any_process_spawns` | → shared test 1 |
| `codex/tests.rs::cancel_and_wait_on_an_untracked_handle_are_typed_rejections` | → shared test 2 (both `cancel` and `wait` claims kept) |
| `claude_code/tests.rs::cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic` | → shared test 2 (its `wait` counterpart added, not previously proven for claude-code — a strict addition, not a loss) |
| `codex/tests.rs::cancel_kills_the_whole_descendant_tree_via_the_adapter` | → shared test 3 (now proved for claude-code too) |
| `codex/tests.rs::reconcile_handles_a_missing_or_unrecognized_process_id` | → shared test 4 |
| `codex/tests.rs::reconcile_reports_real_process_liveness_by_pid` | split: dead-pid half → shared test 4; alive-pid half stays as `reconcile_trusts_a_live_pid_unconditionally` |
| `claude_code/tests.rs::reconcile_reports_honest_observations_for_untrackable_pids` (3 rows) | → shared test 4 |
| Every other test in both files | unchanged, including the ones the "not a duplicate" table above explicitly kept |

## Claim → evidence

| Claim | Evidence |
|---|---|
| `cargo build --workspace --tests` compiles clean | ran after every edit; final state clean, no warnings |
| Zero cross-adapter/`local_process`/`harness` duplicate-test pairs | `python3 scripts/maintainability.py duplicate-tests` — `tack-runner: 1` (the pre-existing, out-of-scope `provider/` pair); before this phase: `3` |
| `codex::` tests pass, reduced count reflects the consolidation table | `cargo nextest run --workspace -E 'test(codex::)'` — `13 tests run: 13 passed` (down from 17) |
| `claude_code::` tests pass, reduced count reflects the consolidation table | `cargo nextest run --workspace -E 'test(claude_code::)'` — `19 tests run: 19 passed` (down from 22) |
| `harness::tests::` (the shared suite) all pass, including the 4 new ones | `cargo nextest run --workspace -E 'test(harness::tests::)'` — `17 tests run: 17 passed` (up from 13) |
| Full-suite arithmetic reconciles | `cargo nextest run --workspace` — `1413 tests run: 1413 passed, 8 skipped`. `1416 − 4 (codex) − 3 (claude_code) + 4 (harness::tests) = 1413`. Matches exactly. |
| Crash matrix unaffected | `cargo nextest run --workspace -E 'binary(crash_matrix)'` — `6 tests run: 6 passed` |
| `check` (full, non-`--changed`) passes | `✓ maintainability budgets hold (292 files checked)` |
| `check --changed` passes | `✓ maintainability budgets hold (8 files checked)` |
| `cargo fmt --all -- --check` clean | ran after every edit; final state clean |
| `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml -- --check` clean | ran once; clean |
| `cargo clippy --workspace --all-targets -- -D warnings` clean | ran after every edit; final state clean (one real fix needed — a `clippy::type_complexity` on an inline 3-tuple array type in the reconcile test, resolved with a local `type ReconcileCase<'a> = (...)` alias) |
| `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D warnings` clean | ran once; clean |
| `.githooks/pre-push` (the actual gate) passes end to end | ran directly, twice (once after the dedup pass, once after the shared-helper extraction); `✓ pre-push checks passed` both times |
| No board archaeology introduced | `scripts/check-comments.sh` (via pre-push) — clean |
| No test-hygiene regression | `scripts/check-test-hygiene.sh` (via pre-push) — clean |
| No production behavior changed by the `stage_run_log`/`resolve_provider_endpoint` extraction | Both adapters' own remaining provider-endpoint-injection and artifact-staging tests (`provider_endpoint_credential_reaches_the_process_only_when_the_request_names_it`, `provider_endpoint_variables_present_only_when_configured_and_requested`, `fake_binary_success_stages_a_real_log_artifact`, and equivalents) still pass unchanged — each now exercises the same logic through one shared function instead of two copies |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/src/harness/{codex.rs,codex/tests.rs,claude_code.rs,claude_code/tests.rs,local_process.rs,mod.rs,tests.rs,process.rs,process/tests.rs}`

Before (base `b04a295`, the phase-3b merge state this phase started from):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
harness/claude_code/tests.rs                                   0     0   0%   1386   22    30   66   81    0
harness/codex/tests.rs                                         0     0   0%   1005   17    26   51   88    0
harness/claude_code.rs                                       845   252  23%      0    0     0    0    0   11
harness/codex.rs                                              700   159  18%      0    0     0    0    0   11
harness/process/tests.rs                                       0     0   0%    395    9    31   47   67    0
harness/local_process.rs                                     282   149  34%      0    0     0    0    0   15
harness/process.rs                                            266   105  28%      0    0     0    0    0   27
harness/mod.rs                                                211   137  39%      0    0     0    0    0    8
harness/tests.rs                                                0     0   0%    772   13    26  125   90    0
totals: prod=2735 (comments 697)  test=3558  tests=61
```

After (working tree, pre-commit):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
harness/claude_code/tests.rs                                   0     0   0%   1299   19    31   66   81    0
harness/tests.rs                                                0     0   0%   1000   17    29  125   90    0
harness/codex/tests.rs                                          0     0   0%    884   13    26   51   88    0
harness/claude_code.rs                                       816   249  23%      0    0     0    0    0   11
harness/codex.rs                                              671   158  19%      0    0     0    0    0   11
harness/process/tests.rs                                        0     0   0%    401    9    31   47   67    0
harness/local_process.rs                                     343   161  31%      0    0     0    0    0   15
harness/process.rs                                            266   105  28%      0    0     0    0    0   27
harness/mod.rs                                                211   137  39%      0    0     0    0    0    8
totals: prod=3117 (comments 810)  test=3584  tests=58
```

- **`codex.rs`: 700 → 671 (−29)**, from the `resolve_provider_endpoint`/`stage_run_log`
  extraction — the entire resolve-and-reject-typed wrapper and the entire
  stage-a-log-artifact body moved into `local_process.rs`; each grammar's own version
  is now a 5-10 line call site.
- **`claude_code.rs`: 845 → 816 (−29)** — the identical shape of reduction, same two
  extractions.
- **`local_process.rs`: 282 → 343 (+61)** — the two new shared functions, now used by
  both grammars. Combined `codex.rs + local_process.rs`: 982 → 1014 (+32); combined
  `claude_code.rs + local_process.rs`: 1127 → 1159 (+32) — both grow slightly, not
  shrink, matching the audit's own §3 prediction that a shared core's cost amortizes
  across *more than two* consumers, not two. `local_process.rs` now has two genuinely
  reused pieces beyond the lifecycle trait itself; a third harness landing on this core
  is the point at which this net-positive flips to net-negative, per §3's own table.
- **`codex/tests.rs`: 1005 → 884 test lines (−121), 17 → 13 tests (−4)** — exactly the
  four tests the consolidation table names.
- **`claude_code/tests.rs`: 1386 → 1299 test lines (−87), 22 → 19 tests (−3)** — exactly
  the three tests the consolidation table names.
- **`harness/tests.rs`: 772 → 1000 test lines (+228), 13 → 17 tests (+4)** — the four
  new cross-adapter tests plus their shared `real_adapters_for` builder. Test-line
  growth (+228) exceeds the sum of what was removed from the two adapters' files
  (−121 −87 = −208) by 20 lines: each new test does strictly more work than either of
  its predecessors (constructs *two* real adapters, not one, and in three of the four
  cases drives *both* through the same assertions) — the file crossed its own 1000-line
  budget mid-edit (peaked at 1021) and was trimmed back under it by shortening doc
  comments and replacing verbose per-case boilerplate with a `real_adapters_for` helper
  and (for the reconcile test) named `fn`s instead of inline closure-cast tuples, without
  cutting any assertion.
- **`process/tests.rs`: 395 → 401 test lines (+6), 9 → 9 tests (unchanged)** — the two
  `pub(crate)` visibility changes plus their one-line doc comments; no test added or
  removed here, only two helpers exposed to a sibling module.
- **`process.rs`: 266 → 266 (unchanged)** — the `mod tests;` → `pub(crate) mod tests;`
  visibility change is a same-length line.
- **Full-suite test count: 1416 → 1413 (−3)**, reconciling exactly as shown in "Claim →
  evidence" above.

## Budget check (audit §5 rule 7: ≤400 production lines, ≤15 tests per adapter)

```
✓ maintainability budgets hold (292 files checked)     # full check
✓ maintainability budgets hold (8 files checked)        # check --changed
```

Both pass — the *ratchet* (no file worse than its own baseline; `harness/tests.rs` grew
but stayed under the absolute 1000-line ceiling once trimmed). Measured against the
audit's own absolute targets directly, per adapter:

| Adapter | Production lines | vs. 400 target | Tests | vs. 15 target |
|---|---|---|---|---|
| codex | 671 | **FAIL** (271 over) | 13 | **PASS** |
| claude-code | 816 | **FAIL** (416 over) | 19 | **FAIL** (4 over) |

**Codex now passes the test-count target** (13 ≤ 15, down from 17); **claude-code does
not** (19, down from 22, but still 4 over). Getting claude-code to 15 without either
losing a genuinely distinct claim or duplicating something `harness/mod.rs` now already
covers was not found — every remaining test in that file was checked against the
"not a duplicate" table above or against phase 3b's own reasoning, and each proves
something the file alone still needs to prove (the `stream-json` parsing family, the
provider allow-list/network-contradiction rows, the two Linux-only real-process
reconcile tests, the artifact-staging test, the redaction canary, the version-parsing
table). Forcing this further would mean merging tests that prove different claims into
one table (weakening what a passing row lets a reader conclude) — not done, per this
card's own explicit instruction not to trade a real assertion for a smaller number.

**Neither adapter meets the 400-production-line target.** This phase's secondary
mandate found two genuine, safe extractions (`resolve_provider_endpoint`,
`stage_run_log`, together −29 lines per adapter) and, having read both files in full
looking for more, found no further one that would not either (a) duplicate the newly
extracted logic in a different shape for one caller, or (b) move genuinely per-grammar
behavior (the `-c` flag construction and TOML-quoting in `CodexGrammar::prepare`, the
`stream-json` parser and provider allow-list in `ClaudeCodeGrammar`, each grammar's own
`feature_capabilities()` reasoning strings) into a shared core that would then need a
grammar-specific branch inside it — the exact anti-pattern `.claude/scope-discipline.md`
and the audit's own §4 trait design were built to avoid. Per this repo's "measured, not
promised" rule: 671 and 816 are the real, final numbers, not a claim that 400 was met or
nearly met. Reaching it would require a design change to what `prepare()`/`outcome()`
own — the audit's own rule 7 names this as "a card that needs more has found a gap in
the shared core, which is a separate card," not a further pass of this one.

## What didn't fit cleanly

- **`cancel_kills_the_whole_descendant_tree_via_both_real_adapters` is not, strictly, a
  "codex ∩ claude_code" duplicate** — claude-code never had its own copy of this claim.
  It was consolidated anyway because re-reading codex's version against
  `local_process.rs`'s actual `cancel()` implementation showed the claim it proves
  (going through the full `start`/`cancel` path kills the whole descendant tree, not
  just the direct child) is 100% shared-core behavior, and audit rule 1 says exactly
  this class of test "lives in `harness/mod.rs`... run against the fake harness" — not
  that it must have had two copies first. Extending it to cover claude-code too (rather
  than just relocating codex's copy unchanged) was judged worth the small extra cost
  since the infrastructure (`real_adapters_for`, `fake_harness_command`,
  `real_adapter_spec_with_env`) already existed for the other three tests.
- **The `resolve_provider_endpoint`/`stage_run_log` extraction is a "secondary,
  best-effort" deliverable, not required** — it was attempted because the card
  explicitly named "provider-endpoint injection plumbing" and "any remaining
  boilerplate around `resolve_binary`/`resolve_provider_endpoint`'s call sites" as
  candidates worth checking, and reading both grammars' full bodies side by side (done
  for this handoff's duplicate-review table anyway) surfaced these two as genuinely
  identical modulo parameters — a cheaper find than a from-scratch search would have
  been, since the review work for the primary task already required reading every
  method in both files.
- **`ArtifactStager::new`'s doc-link in `codex.rs`'s `discover` doc comment needed
  updating to a fully-qualified path** (`crate::harness::artifact::ArtifactStager::new`)
  once the `use ... artifact::ArtifactStager` import it relied on became unused and was
  removed — a one-line fix, caught by re-reading the file after the edit rather than by
  any check (intra-doc-link resolution failures are warnings, not `-D warnings` errors,
  in this workspace's clippy/rustdoc configuration, so this would not have failed CI —
  fixed anyway since a broken doc link is exactly the kind of thing a stranger relying on
  `cargo doc` would trip over).
- **`resolve_provider_endpoint`'s new shared function takes `harness_kind: &str` as an
  explicit parameter for its warn-log line**, rather than deriving it from `self` (which
  it has no access to, being a free function): each grammar's own trait-method wrapper
  passes its own `CODEX_HARNESS_KIND`/`HARNESS_KIND` constant. This is a one-parameter
  cost for de-duplicating roughly ten lines of identical `map_err`/`tracing::warn!`
  boilerplate per grammar — judged worth it, matching the same shape `stage_run_log`
  already needed for its own log line.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check` (full) passes on the final tree without
touching `scripts/maintainability-baseline.json`. Every file that changed either shrank
against its own baseline (`codex.rs`, `claude_code.rs`, `codex/tests.rs`,
`claude_code/tests.rs`) or grew within the tool's absolute (non-ratcheted) ceiling for
that column (`local_process.rs`, `harness/tests.rs` — trimmed under its 1000-line
ceiling before this phase's final commit; `process/tests.rs`'s +6 lines is far under any
ceiling).

## IX-M5's four exit criteria (audit §6) — final status

1. **Zero near-identical test pairs across adapters** — **MET.** `duplicate-tests`
   reports 0 pairs involving `codex`/`claude_code`/`local_process`/`harness` files (1
   pair remains in `tack-runner`, in `provider/`, pre-existing and out of this card's
   scope).
2. **Each adapter under 400 lines of production code** — **NOT MET.** `codex.rs`: 671
   lines (271 over). `claude_code.rs`: 816 lines (416 over). This phase reduced both by
   29 lines via two genuine shared-logic extractions and found no further safe
   extraction after reading both files in full — closing this gap needs a `local_process.rs`
   design change (e.g. a shared command-line-building or capability-declaration
   abstraction) that the audit's own rule 7 names as a separate, later card, not a
   further pass of this one.
3. **No live test under `src/`** — **MET.** Confirmed unaffected by this phase;
   `tests/live/main.rs` (`mod codex; mod claude_code;`) remains the only live-test
   entry point, unchanged.
4. **The crash matrix and `harness/tests.rs` green** — **MET.** `binary(crash_matrix)`:
   6/6 passed. `test(harness::tests::)`: 17/17 passed (including the four new
   cross-adapter tests this phase added).

**3 of 4 criteria are met. Criterion 2 is not**, and per this repo's own rules this is
reported as a real gap, not rounded up to "essentially done" — IX-M5 as a card should
either accept 671/816 as the final numbers (with the dedup and shared-lifecycle-testing
work this phase and phases 1-3 did being the actual, delivered value) or open a follow-on
card scoped explicitly to a `local_process.rs` design change, per audit rule 7's own
framing.

## What a stranger still cannot do

- **Cannot see either adapter under the audit's 400-line target** — see criterion 2
  above; 671 and 816 are the honest final numbers for this card.
- **Cannot see claude-code's test count at or under 15** — 19, down from 22; every
  remaining test was checked against a "why it's not a duplicate" reason in this
  handoff's own table, so a future agent does not need to re-derive which of them are
  candidates (none are, without losing a distinct claim).
- **Cannot assume `docs/plans/harness-maintainability-audit.md`'s own numbers (§1, §3)
  are current** — they were measured 2026-09-11, before any of IX-M5's four phases
  landed; a future reader citing "775 lines" or "27 tests" for codex, or the audit's own
  §3 cost table, should re-run Appendix A's script rather than repeat either number, per
  this repo's "a load-bearing number carries the command that produces it" rule.
- **Cannot see a third or fourth harness's actual cost** — `docs/plans/harnesses.md`
  (docket, opencode) still waits for this card to close, per `MEMORY.md`'s own note;
  this phase's `local_process.rs` growth (+61 lines, two new shared functions with two
  real callers each) is now slightly better amortized evidence than phase 2's own
  measurement, but still short of the audit's own three-or-more-consumer prediction.

## Context spent

- Read in full before making any change: all four prior handoffs
  (`IX-M5-harness-core-codex.md`, `IX-M5-harness-core-claude_code.md`,
  `IX-M5-prune-codex.md`, `IX-M5-prune-claude_code.md`), `docs/plans/harness-maintainability-audit.md`
  in full, the current `local_process.rs`, `harness/mod.rs`, `harness/tests.rs`,
  `codex.rs` + `codex/tests.rs`, `claude_code.rs` + `claude_code/tests.rs`, all in full,
  before designing any consolidation.
- The duplicate-review table above required reading every flagged pair's actual test
  body (both phase-3 handoffs' own "suspected" lists plus the tool's own `--ratio 0.55`
  output) rather than trusting name similarity — this is where most of this phase's time
  went, and is also what made the two production-code extractions (`resolve_provider_endpoint`,
  `stage_run_log`) findable at near-zero extra cost, since the same close reading that
  proved test-duplication also showed the production-code duplication underneath it.
- One real surprise: `harness/tests.rs` crossed its own 1000-line test-file budget twice
  mid-edit (once from the four new tests' first draft, once again after adding the
  `type ReconcileCase` alias clippy required) — each time fixed by shortening doc
  comments and factoring out repeated adapter-construction boilerplate into
  `real_adapters_for`, never by cutting an assertion or reverting a consolidation.
- One clippy finding not anticipated: `clippy::type_complexity` on an inline
  `[(&str, &dyn HarnessAdapter, fn(u32) -> String); 2]` array type in the reconcile
  test — fixed with a local type alias, the same technique this crate already uses
  elsewhere for case-struct arrays (per the "rustfmt gotcha" both phase-3 handoffs
  documented for a different reason).
- Files opened and not used for editing: `docs/TESTING.md`, `TODO.md`'s IX-M5 section
  (to confirm this is phase 4 of 4 and the recorded 1416-test baseline), `docs/plans/harnesses.md`
  (out of scope — still waits for this card to close, per its own header).

## Amendments

*(none yet)*
