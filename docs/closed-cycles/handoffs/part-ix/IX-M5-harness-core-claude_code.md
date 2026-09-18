# IX-M5-harness-core-claude_code handoff

**Phase 2 of 3 for card IX-M5 (Wave 30).** Phase 1 designed the shared harness core
(`local_process.rs`) and migrated `codex.rs` onto it; this phase migrates `claude_code.rs`
onto the same `HarnessGrammar` trait, per its own "What didn't fit cleanly" section. Phase 3
(two agents, one per adapter, running in parallel) prunes both adapters' tests to the
audit's §5 shape and moves the live tests under `tests/live/` — **not done here**, per the
card's explicit scope for this phase.

- Base SHA / branch / final SHA: `2324724` (`develop`, the phase-1 merge commit) /
  `agent/ix-m5-core-claude_code` / `d0b172b` (worktree: `/tmp/ix-m5-core-claude_code`,
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m5-core-claude_code`).
- Files changed:
  - `crates/tack-runner/src/harness/claude_code.rs` — rewritten: shrinks to
    `ClaudeCodeGrammar` (the `impl HarnessGrammar for ClaudeCodeGrammar` block), the
    `ClaudeCodeAdapter` type alias, and what remains genuinely claude-code-specific
    (`HarnessBinary`, the provider allow-list, the `stream-json` parser, the version-token
    scanner, `feature_capabilities`).
  - `crates/tack-runner/src/harness/claude_code/tests.rs` — **3 lines changed, not
    byte-identical** (see "Tests" below for why, and why it could not be zero).
  - `crates/tack-runner/src/harness/local_process.rs` — the `cancelled`-tracking gap phase
    1 left open is closed here (see "The trait as it stands now"); `grammar` is now
    `pub(crate)`.
  - `crates/tack-runner/src/harness/codex.rs` — one mechanical change: `outcome`'s new
    `cancelled: bool` parameter, ignored (`_cancelled`), with a comment explaining why.
    `codex/tests.rs` is untouched and confirmed still green (26/26 + 2 ignored).
  - `crates/tack-runner/src/bootstrap.rs`, `crates/tack-runner/src/harness/tests.rs`,
    `crates/tack-runner/src/harness/mod.rs` — **not touched**. `ClaudeCodeAdapter::discover`,
    `::for_fixture`, `::with_binary`, `.with_cancel_grace(..)`, `.with_providers(..)` all kept
    their exact pre-migration signatures, so no caller needed editing.
- Contract fixtures consumed: none.
- Behavior implemented: none — structural extraction with a no-behavior-change constraint,
  proved by the near-total identity of `claude_code/tests.rs` and by the full suite holding
  at the recorded baseline.

## The trait as it stands now

Phase 1's trait is unchanged in shape except for `outcome`'s new final parameter and one
field's visibility:

```rust
fn outcome(
    &self,
    state: Self::RunState,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    result: ProcessResult,
    cancelled: bool,
) -> HarnessOutcome;
```

`LocalProcessHarness<G, C>` gained one field:

```rust
cancelled: tokio::sync::Mutex<std::collections::BTreeSet<String>>,
```

`cancel()` inserts the handle into it (right after `take_running` removes the entry from
`running`, matching the order the pre-migration claude-code adapter already used); `wait()`
removes and reads it back, passing the result to `grammar.outcome(...)` as the new
`cancelled` parameter. This is shape (a) from phase 1's two proposed designs — a
generic set on `LocalProcessHarness` itself, not folded into `G::RunState` — chosen because
it needed no change to `take_running`'s "take ownership, do not put it back" discipline,
and because claude-code's own pre-migration design (a `cancelled: Mutex<BTreeSet<String>>`
field on the adapter, independent of its `processes` map) already proved this exact shape
correct for one real adapter's needs.

`grammar: G` is now `pub(crate)` (was private): claude-code's `with_cancel_grace(mut self,
grace: Duration) -> Self` builder needs to reach `self.grammar.cancel_grace` after
construction, and a generic accessor method would exist for that one caller alone — the same
reasoning phase 1 already used for `running`'s `pub(crate)` visibility.

Every other hook, and codex's own implementation of them, is untouched. `codex.rs`'s only
change is the new `_cancelled: bool` parameter on its `outcome` impl, ignored with a comment
(codex's `classify_exit` never produces `AttemptState::Cancelled`) — `codex/tests.rs` needed
no changes at all and is still byte-identical to `develop`.

## Asymmetry → hook mapping (claude-code's side)

| # | Asymmetry (from the phase-1 card/handoff) | Hook(s) | claude-code's answer |
|---|---|---|---|
| 1 | Model-selection policy is inverted: claude-code allows an absent requested model (auto-selection) | `validate_selection` | No model-presence check at all (contrast codex's rejection). Bundles three checks with no codex equivalent: harness-kind match, a provider allow-list (`is_known_provider`/`NATIVE_PROVIDER_FAMILIES` plus the live provider registry), and a network self-contradiction check (`NETWORK_TOOLS` vs `permission_policy.network`) — all entirely inside this one hook, exactly as phase 1 said it should be ("the hook has no opinion on what's inside it"). |
| 2 | `cancel`'s failed-signal policy: `Ambiguous`, not `Err` | `cancel_outcome` | Never returns `Err` — a failed signal maps to `(CancelObservation::Ambiguous, {"process_outcome": "signal_failed", ...})`. This is also the hook whose caller (`local_process::cancel`) now feeds the shared `cancelled` set (see above). |
| 3 | `reconcile`'s identity check: a real check, not an unconditional trust | `reconcile_alive`, `reconcile_unavailable` | `reconcile_alive` calls the pre-existing `process_program_matches` (Linux-only `/proc/<pid>/cmdline` vs the resolved binary path): `Some(true)` → `ProcessRunning`, `Some(false)` → `ProcessStopped`, `None` (non-Linux Unix, or `/proc` unreadable) → `Ambiguous`. `reconcile_unavailable` (the non-Unix fallback) returns `Ok(RecoveryObservation::Ambiguous)` — a different *value* than codex's `Err(RecoveryUnavailable)`, both correct for their own adapter, deliberately not unified (per the phase-1 handoff's explicit instruction). |
| 4 | Handle encoding: a bare pid string, no counter | `encode_handle`, `decode_handle` | `pid.to_string()` / `process_id.parse::<u32>().ok()` — no counter added; codex's disambiguation scheme was not "fixed onto" a file that never had it. |
| 5 | `probe`'s caching side effect (codex-only) | `after_probe` | Left at the trait's default no-op — claude-code never cached a probe result before, and still doesn't. |
| 6 | `detect_version`'s arity/shape | `detect_version` | `detect_version_impl` (renamed from the pre-migration `detect_version`) keeps its exact original 2-tuple body (spawn `--version`, parse via `parse_version_text`) unchanged; the trait's `detect_version` wraps it into the 3-tuple shape. The `BTreeMap` is **not** empty, unlike codex's — see "What didn't fit cleanly" below for why. |
| 7 | `stage_run_log`'s staging root: always `<workspace>/.artifacts`, no adapter-wide root | *(no hook — see phase 1)* | Stayed an inherent associated function on `ClaudeCodeGrammar`, called inline from `outcome()`, exactly matching codex's shape and the phase-1 handoff's explicit instruction not to look for a hook that isn't there. |
| 8 | `prepare`'s command-line construction shares nothing with codex | `prepare` | Confirmed: the `-p --output-format stream-json ...` argument list, the `ANTHROPIC_*` environment injection, and the prompt-on-stdin wiring are entirely this grammar's own, moved verbatim from the pre-migration `start()`. |
| 9 | `outcome`'s classification: `stream-json` parsing keyed on `is_error`, not exit code | `outcome` | Confirmed: `parse_run_output`/`parsed_from_result_line`/`malformed_outcome`/`fallback_from_exit_code` all moved unchanged as free functions; `outcome()` calls `parse_run_output` and then applies the `cancelled` parameter on top (the one thing codex's own classification structurally cannot produce). |
| 10 | `feature_capabilities`/`harness_kind` are per-grammar data | `feature_capabilities`, `harness_kind` | Moved verbatim; `feature_capabilities()` stayed a free function (unlike codex's, which was already inline), with the trait method delegating to it — a smaller diff to a function whose text is almost entirely load-bearing prose (5 capability statements with reasons). |

## What didn't fit cleanly

- **The `cancelled`-tracking gap (phase 1's item 1) is resolved as shape (a)**: a generic
  `cancelled: Mutex<BTreeSet<String>>` on `LocalProcessHarness` itself, populated by `cancel()`
  and consumed by `wait()`, threaded into `outcome()` as a new `cancelled: bool` parameter.
  Shape (b) (folding it into `G::RunState`) was rejected for the same reason phase 1's handoff
  already flagged: `cancel()` calls `take_running`, which *removes* the entry from `running`
  before a concurrent `wait()` could read any state back out of it — there is nothing left to
  fold a flag into. Shape (a) needed no change to that discipline.
- **`detect_version`'s `additional` map is not empty**, contrary to the letter of the card's
  own instruction ("claude_code's own version-token recognizer ... returns no additional data
  today, so pass an empty map"). Reading the instruction against the actual pre-migration
  behavior surfaced a conflict: the generic `probe()` in `local_process.rs` builds
  `HarnessCapability.additional` from *exactly* what `detect_version()` returns as its third
  tuple element — there is no other injection point. But the pre-migration `probe()` always
  attached a `model_discovery_note` entry to `additional`, *unconditionally*, regardless of
  whether the version probe itself succeeded — and
  `probe_attests_model_passthrough_instead_of_inventing_a_model_list` asserts
  `capability.additional.contains_key("model_discovery_note")`. An empty map would have
  silently dropped that note and failed the test. Resolution: `detect_version_impl` (the
  renamed original body — the actual version-token scanner) still returns no diagnostic data
  of its own, exactly as instructed; the trait's `detect_version()` wrapper adds the
  `model_discovery_note` on top, since it is a probe-level attestation ("this adapter never
  lists models"), not a version-parsing diagnostic. This reads as the intent behind the
  card's instruction (no *version-scanner* diagnostic data) rather than its literal text (no
  data of any kind) — flagged explicitly here because it is exactly the kind of small
  divergence a stranger reading only the card text would not expect.
- **The `#[cfg(test)] pub(crate) use` re-export trick (phase 1's item 3) was needed, and one
  more re-export was needed beyond phase 1's own list.** `HarnessAdapter`, `HarnessProbe`,
  `LocalRunHandle`, `AttemptJournal`, `Timestamp` are re-exported behind `#[cfg(test)]`,
  exactly as in `codex.rs` — claude-code's own production code stopped needing them the
  moment its `HarnessAdapter`/`HarnessProbe` impls moved into `local_process.rs`, but
  `claude_code/tests.rs`'s `use super::*` still does. One addition beyond codex's list:
  `process_alive` (from `harness::process`, `#[cfg(unix)]`-gated) is now re-exported behind
  `#[cfg(all(test, unix))]` — the pre-migration `reconcile()` called it directly in its own
  `#[cfg(unix)]` branch, but the shared `reconcile()` in `local_process.rs` now does that
  liveness check generically before ever asking the grammar anything, so claude-code's own
  production code has no remaining call site. Several tests
  (`cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry`, both `reconcile_reports_*`
  Linux tests) still call it directly to assert real process liveness, so it had to move to
  the re-export list rather than being deleted.

## Tests

- `claude_code/tests.rs` is **not byte-identical to `develop` — 3 lines differ**, all the
  same substitution: `adapter.processes` → `adapter.running`.
  `diff <(git show develop:crates/tack-runner/src/harness/claude_code/tests.rs) crates/tack-runner/src/harness/claude_code/tests.rs`
  shows exactly these 3 lines and nothing else. This was not avoidable the way it was for
  codex: codex's pre-migration adapter already named its own bookkeeping field `running`
  (phase 1's handoff says so explicitly, in its own "what was removed" section), so
  `codex/tests.rs`'s direct field accesses (`adapter.running.lock().await...`) needed no
  change at all. claude-code's pre-migration adapter named the *same kind* of field
  `processes` instead — a real, pre-existing naming difference between the two adapters, not
  something this phase introduced — and `LocalProcessHarness`'s shared field is `running`
  (phase 1's name, inherited from codex). The 3 tests that read this field directly
  (`validate_rejects_an_unsupported_model_provider_before_any_process_launches`,
  `cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry`, twice) had no way to keep
  compiling under the old name without adding a second, `processes`-named alias field to
  `LocalProcessHarness` purely for this one file's benefit — which would have been the kind
  of dead-code-for-one-caller `.claude/scope-discipline.md` flags, not a smaller diff.
- Command: `cargo nextest run --workspace -E 'test(claude_code::tests::)'` — `36 tests run:
  36 passed` (default). `--run-ignored ignored-only` on the same filter: `3 tests run: 3
  passed` (all three self-skip: no real `claude` binary/opt-in env var on this machine,
  exactly as before) — `live_claude_code_records_version_and_a_real_artifact_when_opted_in`,
  `live_claude_code_through_the_configured_provider_when_opted_in`,
  `live_claude_code_direct_model_never_reaches_the_configured_provider_when_opted_in`.
  36 + 3 = 39, matching the file's own test count before and after (unchanged: see "Measured
  numbers").
- `cargo nextest run --workspace -E 'test(codex::tests::)'` — `26 tests run: 26 passed`
  (default), `2 tests run: 2 passed` (`--run-ignored ignored-only`) — unaffected, confirming
  the `outcome` signature change did not silently change codex's own behavior.
- Full suite: `cargo nextest run --workspace` — `1439 tests run: 1439 passed, 8 skipped` —
  matches `develop`'s recorded baseline exactly (confirmed on top of the phase-1 merge commit
  `2324724`, not a stale base).

## Claim → evidence

| Claim | Evidence |
|---|---|
| `cargo build --workspace --tests` compiles clean | ran after every edit; final state clean, no warnings |
| No claude-code test silently dropped | 39/39 accounted for (36 run + 3 ignored, live-skip) vs 39 in the pre-migration file (37 `#[...test]`-tagged items measured by the maintainability tool, which undercounts multi-line-attribute tests by 2 — the same shape of discrepancy phase 1 noted for codex's own count; the nextest-reported 39 is the trustworthy number, and it is identical before and after) |
| `codex` tests unaffected | `test(codex::tests::)` — 26/26 + 2 ignored, matches `develop`; `codex/tests.rs` byte-identical |
| Full-suite baseline held | `cargo nextest run --workspace` — 1439/1439, 8 skipped, matches `develop` |
| `claude_code/tests.rs` diff from `develop` is minimal and fully explained | 3 lines, `adapter.processes` → `adapter.running`, justified above — no assertion changed |
| `check --changed` passes | `✓ maintainability budgets hold (4 files checked)` after trimming `local_process.rs`'s doc comments once (see "Measured numbers" — first pass landed at 35.8%, over the 35% budget) |
| `cargo fmt --all -- --check` clean | ran after every edit; final state clean (one real reformat needed, a wrapped `use` line in `claude_code.rs`) |
| `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml -- --check` clean | ran once; clean (this crate does not depend on `tack-runner`'s harness code) |
| `cargo clippy --workspace --all-targets -- -D warnings` clean | ran after every edit; final state clean |
| `.githooks/pre-push` (the actual gate) passes end to end | ran directly; `✓ pre-push checks passed` |
| No board archaeology introduced | `scripts/check-comments.sh` — clean |
| No test-hygiene regression | `scripts/check-test-hygiene.sh` — clean |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/src/harness/claude_code.rs crates/tack-runner/src/harness/claude_code/tests.rs crates/tack-runner/src/harness/local_process.rs crates/tack-runner/src/harness/codex.rs`

Before (`develop` at `2324724`, the phase-1 merge — `claude_code.rs`/`tests.rs` untouched by
phase 1, `local_process.rs`/`codex.rs` as phase 1 left them):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/claude_code/tests.rs             0     0   0%   1735   37    26   73   89    0
crates/tack-runner/src/harness/claude_code.rs                 903   257  22%      0    0     0    0    0    8
crates/tack-runner/src/harness/codex.rs                       699   156  18%      0    0     0    0    0   11
crates/tack-runner/src/harness/local_process.rs               270   137  33%      0    0     0    0    0   15
```

After (final SHA `d0b172b`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/claude_code/tests.rs             0     0   0%   1735   37    26   73   89    0
crates/tack-runner/src/harness/claude_code.rs                 845   252  23%      0    0     0    0    0   11
crates/tack-runner/src/harness/codex.rs                       700   160  18%      0    0     0    0    0   11
crates/tack-runner/src/harness/local_process.rs               282   149  34%      0    0     0    0    0   15
```

`claude_code.rs` shrank (903 → 845 production lines, −58) — a bigger drop than codex's own
phase-1 shrink (−79 on a smaller starting file, proportionally similar), consistent with
moving the same category of lifecycle plumbing (bookkeeping maps, the `validate`/`start`/
`cancel`/`wait`/`reconcile` method bodies) out into the now-shared core. `local_process.rs`
grew by 12 production lines (270 → 282, all from the `cancelled` field/logic) and 12 comment
lines (137 → 149, net of the trims described below) — the amortization the phase-1 handoff
predicted is now visible: `claude_code.rs` + `local_process.rs` combined is 845 + 282 = 1127,
*below* `claude_code.rs`'s own pre-migration total of 903 + `local_process.rs`'s 270 = 1173,
a net −46 lines with two real consumers now sharing the core, where phase 1 (one consumer)
could only report a net +191. `codex.rs` grew by exactly 1 production line and 4 comment
lines (the new `_cancelled: bool` parameter and its justifying comment) — the smallest
possible footprint for absorbing a new required trait parameter it does not use.
`claude_code/tests.rs` is unchanged in every column the tool reports (1735 test lines, 37
tests, avg/max/name identical) — the 3-line field-name substitution does not change line
count or test count, only three tokens.

`local_process.rs`'s comment share peaked at 35.8% mid-edit (over the 35% budget) after the
first pass of doc comments for the `cancelled` field, the `grammar` field's new visibility,
and `outcome`'s new parameter; trimmed to 34% by shortening those three doc comments without
dropping the load-bearing "why" each one carries (see the file itself — `check --changed`
passed cleanly after the trim).

## What was removed

Only code that became dead because it moved into the shared core, or that the trait made
redundant — no test pruning (explicitly phase 3's job):

- `ClaudeCodeAdapter`'s own `HarnessAdapter`/`HarnessProbe` trait impls (the entire
  `validate`/`start`/`cancel`/`wait`/`reconcile`/`probe`/`declared_capabilities` bodies,
  minus the claude-code-specific logic each one wrapped) — now
  `LocalProcessHarness<ClaudeCodeGrammar, C>`'s single generic impl.
- `RunningEntry` (the old per-attempt bookkeeping struct) — replaced by the generic
  `RunningProcess<S>` (process/secrets/limits/started_at) plus the new, claude-code-only
  `ClaudeCodeRunState` (workspace path, attempt id, requested provider — exactly the fields
  `RunningEntry` had beyond what is now generic).
- `ClaudeCodeAdapter`'s own `processes: Mutex<BTreeMap<...>>`, `cancelled: Mutex<BTreeSet<...>>`,
  `secrets: SecretStore`, `providers: BTreeMap<...>` fields — the first two folded into
  `LocalProcessHarness`'s own `running`/`cancelled` (the latter newly added this phase); the
  last two were already generic since phase 1.
- `now_rfc3339` — the free helper that built `cancel`'s `observed_at` timestamp; the shared
  `cancel()` in `local_process.rs` already had its own identically-shaped `rfc3339` helper
  from phase 1 (built for codex, reused here with no changes needed).
- The inline `HarnessCapability { ... }` construction inside `probe()` — now built once,
  generically, by `LocalProcessHarness::probe()`; this grammar only supplies
  `detect_version()` (including the `model_discovery_note`, see above) and
  `model_passthrough()`.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree without
touching `scripts/maintainability-baseline.json`. Every changed file's production line count
either shrank (`claude_code.rs`) or grew only as much as the default (non-ratcheted) budget
already allows (`local_process.rs`, `codex.rs`) — `check` reported no violation once
`local_process.rs`'s comment share was trimmed back under 35%.

## Budget check

```
✓ maintainability budgets hold (4 files checked)
```

Not required to bring `claude_code.rs` under the audit's final §5 target (≤400 production
lines, ≤15 tests) this phase — that is phase 3's job, same as it was for codex in phase 1.
`claude_code.rs` is at 845 lines today, larger than codex's own 700 post-phase-1, because
this grammar's `validate_selection` alone absorbs three checks with no codex equivalent
(provider allow-list, network self-contradiction) and its `outcome`'s `stream-json` parser
(`parse_run_output` plus three helper functions) is intrinsically larger than codex's
exit-code-only `classify_exit`.

## What a stranger still cannot do

- **Cannot yet see either adapter pruned to the audit's §5 shape** — phase 3 (two agents in
  parallel, one per adapter) still needs to: move lifecycle-only tests into a shared
  `harness::mod`-level suite, cut duplicated near-identical test pairs, move vendor
  transcripts to fixture files, and move the three (now confirmed) live tests under
  `tests/live/`.
- **Candidate lifecycle-duplicate tests in `claude_code/tests.rs`** (useful context for
  phase 3, not acted on here): `validate_rejects_a_spec_requesting_a_different_harness_kind`,
  `cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic`,
  `reconcile_with_no_recorded_process_id_needs_no_dispatch`,
  `reconcile_reports_process_stopped_for_a_pid_that_no_longer_exists`, and
  `validate_rejects_a_missing_secret_reference_typed_and_touches_nothing` all now exercise
  `local_process.rs`'s own shared code paths (harness-kind mismatch in `validate_selection`
  is grammar-specific, but the *rejection plumbing* around it, the unknown-handle-is-`Err`
  path in shared `cancel`/`wait`, the no-pid/dead-pid paths in shared `reconcile`, and the
  `resolve_environment` call inside shared `validate`, are now identical code to codex's
  equivalent tests) rather than anything claude-code-specific. `declared_capabilities_match_the_reconciled_iii_d5_values`
  is claude-code-specific (the actual capability values differ from codex's) and is not a
  duplicate.
- **`bootstrap.rs` was not touched at all** — `ClaudeCodeAdapter::discover(secrets)` kept its
  exact fallible, eager-resolution signature and behavior via the type alias, so its two call
  sites (`bootstrap.rs:179`, `:225`, `:230`) needed no editing. Verified by the full-workspace
  build and by `harness/tests.rs`'s own `ClaudeCodeAdapter::for_fixture`/`::discover` call
  sites (used directly, boxed as both `HarnessAdapter` and `HarnessProbe`) still compiling
  and passing unchanged.
- **Cannot yet re-measure the audit's four-harness cost claim** with any more confidence than
  phase 1 could — two grammars now exist, but the audit's own "≈250–350 production lines per
  new harness" prediction is about a *third* harness landing on an already-amortized core, not
  about the second one (this phase). The net −46 combined-lines number above is evidence the
  amortization direction is right, not yet a measurement of the steady-state per-harness cost.

## Context spent

- Read in full before designing: `docs/agent-handoffs/part-ix/IX-M5-harness-core-codex.md`
  (the fixed trait boundary and its own "what didn't fit cleanly" section — treated as given,
  not re-derived), `crates/tack-runner/src/harness/local_process.rs`,
  `crates/tack-runner/src/harness/codex.rs` (the worked example),
  `crates/tack-runner/src/harness/claude_code.rs` and `claude_code/tests.rs` (both, in full,
  before any edit), `crates/tack-runner/src/bootstrap.rs`'s `ClaudeCodeAdapter` call sites,
  `crates/tack-runner/src/harness/tests.rs`'s cross-adapter acceptance tests (to confirm
  `for_fixture`'s exact call shape needed no change), `crates/tack-runner/src/harness/process.rs`
  (`ProcessLimits.termination_grace`'s default, to resolve the `cancel_grace` question below),
  and `crates/tack-runner/src/provider/mod.rs` (`resolve_endpoint`/`Wire`/`registry`/
  `requires_unconfirmed_model_recording` signatures).
- One build-time-adjacent question resolved by reading rather than guessing: the pre-migration
  adapter's own `cancel_grace` field (test-overridable, default 5s) was never plumbed through
  `ProcessLimits` before — `cancel()` used it directly. The shared `cancel()` in
  `local_process.rs` reads the grace period from `running.limits.termination_grace` instead,
  so `prepare()` now builds `limits` with `termination_grace: self.cancel_grace` explicitly
  overriding `ProcessLimits::new`'s own 5s default — otherwise the fast-cancel test
  (`cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry`, using a 150ms grace to
  stay fast) would have silently reverted to a 5s grace against a `hang`-mode fixture process,
  which would not have failed the test but would have made it ~30x slower for no visible
  reason — worth flagging since nothing would have caught this except noticing the test still
  ran in milliseconds.
- The `detect_version`'s `additional`-map question (see "What didn't fit cleanly") took the
  most back-and-forth: the card's literal instruction and the pre-migration test's actual
  assertion pointed in different directions, resolved by tracing exactly one interface
  (`local_process::probe`) to see there was no second place the `model_discovery_note` could
  live.
- Files opened and not used for editing: `docs/TESTING.md`, `TODO.md`'s IX-M5 section (to
  confirm phase scope and the recorded baseline number), `docs/plans/harnesses.md` (out of
  scope — no `tack-runner` file it plans for exists yet).

## Amendments

*(none yet)*
