# IX-M5-harness-core-codex handoff

**Phase 1 of 3 for card IX-M5 (Wave 30).** This phase designs the shared harness core and
migrates `codex` onto it. Phase 2 migrates `claude_code.rs` onto the trait landed here.
Phase 3 prunes both adapters' tests to the audit's §5 shape and moves the live tests under
`tests/live/`. **Phase 2's agent has not read this conversation and designs nothing
further — it takes the `HarnessGrammar` trait below as fixed** and adapts
`claude_code.rs` to it; the asymmetry mapping and the "didn't fit cleanly" section are
the load-bearing part of this document.

- Base SHA / branch / final SHA: `5766d69` (`develop`) / `agent/ix-m5-core-codex` /
  `d7b3721` (worktree: `/tmp/ix-m5-core-codex`, `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m5-core-codex`).
- Files changed:
  - `crates/tack-runner/src/harness/local_process.rs` — new. `LocalProcessHarness<G>` +
    `HarnessGrammar` trait + the shared lifecycle (`validate`/`start`/`cancel`/`wait`/
    `reconcile`/`probe`).
  - `crates/tack-runner/src/harness/codex.rs` — rewritten: shrinks to `CodexGrammar` (the
    `impl HarnessGrammar for CodexGrammar` block), the `CodexAdapter` type alias, and what
    remains genuinely codex-specific (`CodexLocator`, the provider `-c` flag construction,
    the version-token scanner, `classify_exit`, `stage_run_log`).
  - `crates/tack-runner/src/harness/mod.rs` — one line, `pub mod local_process;`.
  - `crates/tack-runner/src/harness/codex/tests.rs` — **byte-identical to develop.** The
    re-export approach below (see "What didn't fit cleanly") meant this file needed zero
    changes, not even an import edit.
  - `crates/tack-runner/src/engine.rs` — one comment line fixed in a follow-up commit
    (`d7b3721`): a doc comment named `CodexAdapter::stage_run_log`, which moved to
    `CodexGrammar::stage_run_log`.
  - `claude_code.rs`, `claude_code/tests.rs`, `harness/fixtures/claude_code/**` — **not
    touched**, per the card's explicit instruction.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/claim.response.json` is read
  by `codex/tests.rs`'s `spec_with`, unchanged from before).
- Behavior implemented: none — this is a structural extraction with a no-behavior-change
  constraint, proved by `codex/tests.rs` being unmodified and still green.

## The trait landed

```rust
#[async_trait]
pub trait HarnessGrammar: Send + Sync + 'static {
    type RunState: Send + Sync + 'static;

    fn harness_kind(&self) -> DomainHarnessKind;
    fn validate_selection(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;
    fn resolve_binary(&self) -> Result<(PathBuf, Vec<String>), String>;
    fn resolve_provider_endpoint(
        &self,
        spec: &ExecutionSpec,
        secrets: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<Option<ProviderEndpoint>, HarnessError>;
    async fn prepare(
        &self,
        spec: &ExecutionSpec,
        secrets: &SecretStore,
        providers: &BTreeMap<String, ProviderConfig>,
    ) -> Result<PreparedRun<Self::RunState>, HarnessError>;
    fn encode_handle(&self, pid: u32) -> String;
    fn decode_handle(&self, process_id: &str) -> Option<u32>;
    fn cancel_outcome(
        &self,
        pid: u32,
        signal_result: Result<CancelOutcome, ProcessError>,
    ) -> Result<(CancelObservation, serde_json::Map<String, serde_json::Value>), HarnessError>;
    fn reconcile_alive(&self, pid: u32) -> RecoveryObservation;
    fn reconcile_unavailable(&self) -> Result<RecoveryObservation, HarnessError>;
    fn outcome(
        &self,
        state: Self::RunState,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        result: ProcessResult,
    ) -> HarnessOutcome;
    async fn detect_version(&self)
    -> (String, Option<String>, BTreeMap<String, serde_json::Value>);
    fn after_probe(&self, _version: &str, _error: Option<&str>) {}
    fn model_passthrough(&self) -> Option<CapabilityValue>;
    fn feature_capabilities(&self) -> FeatureCapabilities;
}
```

`LocalProcessHarness<G: HarnessGrammar, C = SystemClock>` holds the fields that turned out
to be genuinely identical between the two adapters — `clock`, `secrets: SecretStore`,
`providers: BTreeMap<String, ProviderConfig>` (plus the `with_providers` builder), and the
`running: Mutex<BTreeMap<String, RunningProcess<G::RunState>>>` bookkeeping map — and
implements `HarnessAdapter`/`HarnessProbe` once, generically, for any `G`. `RunningProcess<S>`
bundles the truly shared per-attempt state (`SupervisedProcess`, `SecretMaterial`,
`ProcessLimits`, `started_at`) with `S = G::RunState` for whatever the grammar alone needs
back at `cancel`/`wait` time.

## Asymmetry → hook mapping

| # | Asymmetry (from the card) | Hook(s) | Codex's answer |
|---|---|---|---|
| 1 | `validate`'s model-selection policy is inverted (codex requires an explicit model; claude-code allows auto-select) | `validate_selection` | Rejects when `requested_model_provider`/`requested_model_id` is `None`, after the kind check. claude-code's provider-family allow-list and permission/network self-contradiction checks belong in its own `validate_selection` impl in phase 2 — this hook has no opinion on what "extra policy" contains. |
| 2 | `cancel`'s handling of a failed signal differs (`Ambiguous` vs `Err`); claude-code separately tracks a `cancelled` set `wait` consults | `cancel_outcome` | `Err(HarnessError::Process)` on a failed signal, logged with codex's own message text. **`RunningProcess`/`take_running` in the shared core hold no `cancelled` flag at all** — see "didn't fit cleanly" below; phase 2 needs a small, additive change here, not a redesign. |
| 3 | `reconcile`'s identity check differs (claude-code verifies `/proc/<pid>/cmdline`; codex trusts a live pid unconditionally); the non-Unix fallback differs in *value*, not just reasoning | `reconcile_alive`, `reconcile_unavailable` | `reconcile_alive` returns `ProcessRunning` unconditionally (`_pid` unused); `reconcile_unavailable` returns `Err(RecoveryUnavailable)`. The shared core does the pid-decode + `process_alive` check identically for both platforms' `#[cfg(unix)]`/`#[cfg(not(unix))]` split; only what happens on "alive" and on "no primitive at all" is delegated. |
| 4 | Handle encoding differs (`codex:<pid>:<counter>` vs a bare pid string) | `encode_handle`, `decode_handle` | Wraps the pre-existing free functions `encode_handle(pid, counter)`/`parse_handle_pid`, with the monotonic `AtomicU64` counter kept as a `CodexGrammar` field. Kept per-grammar as instructed — claude-code's bare-pid encoding is not "fixed" to add a counter it never had. |
| 5 | `probe`'s caching side effect (codex only) | `after_probe` (default no-op) | Codex's impl writes into `last_probe: std::sync::Mutex<Option<(String, Option<String>)>>` (a plain, non-async `Mutex` — never held across `.await`, unlike `LocalProcessHarness`'s own `tokio::sync::Mutex` state). `prepare()` reads it back to avoid a redundant `--version` spawn, exactly as before. |
| 6 | `detect_version` arity/grammar differs (3-tuple + a distinct token scanner for codex; claude-code's own 2-tuple + scanner) | `detect_version` | Returns `(String, Option<String>, BTreeMap<String, serde_json::Value>)` — sized for codex's `raw_version_output` `additional` entry. claude-code's phase-2 impl returns the same 3-tuple shape with an empty map, per the card's own instruction. |
| 7 | `stage_run_log`'s staging root differs (codex: adapter-wide `artifact_staging_root`; claude-code: always `<workspace>/.artifacts`) | *(no dedicated hook — see below)* | `stage_run_log` stayed an inherent method on `CodexGrammar`, called from `CodexGrammar::outcome`. It was never part of the `HarnessGrammar` trait at all: the trait's `outcome` hook is already grammar-owned end to end, so the staging-root choice is just a detail inside that one already-per-grammar method, not a boundary this trait needed to draw. |
| 8 | `start`'s command-line construction shares nothing | `prepare` | Confirmed: codex's `-c` overrides, `exec`/`--json`/`--model` args, and stdin-prompt wiring are entirely inside its own `prepare` impl. |
| 9 | `wait`'s classification shares no parsing logic | `outcome` | Confirmed: codex classifies purely from `ProcessExit` (`classify_exit`) and never inspects stdout/stderr content, in contrast to claude-code's `stream-json` parsing (read but not migrated). A **further, previously unlisted asymmetry** surfaced while reading `claude_code.rs`: codex's `usage.duration_ms` is always wall-clock `Measured` (`ended_at - started_at`); claude-code's `build_usage` instead reads `duration_ms` from the parsed JSON result object and reports `NotMeasured` when absent — it never uses the clock at all for this field. `outcome` receives both `started_at`/`ended_at` and can ignore either; no further change needed for phase 2. |
| 10 | `feature_capabilities`/`harness_kind`/`declared_capabilities` are trivial per-grammar data | `feature_capabilities`, `harness_kind` (`declared_capabilities` is `LocalProcessHarness`'s own `HarnessProbe` impl, calling `grammar.feature_capabilities()`) | Moved verbatim. |

Two additional, purely internal hooks exist because the shared `validate`/`start` bodies
needed something to call at the same point each adapter already called it:
`resolve_binary` (validate discards the result; codex's `prepare` calls it again to get the
real program+args) and `resolve_provider_endpoint` (same discard-and-recheck shape, wrapping
`crate::provider::resolve_endpoint` with the grammar's own `Wire` and provider-name choice).
Both existed as inline logic in the original `validate`/`start`, not named methods — naming
them was necessary to share the calling code around them, not evidence of a new asymmetry.

## What didn't fit cleanly

- **The `cancelled: BTreeSet<pid>` tracking (asymmetry #2) has no home in the shared core
  yet.** `RunningProcess<S>` and `take_running` carry no such flag. Codex never needed one
  (`classify_exit` never becomes `Cancelled`), so nothing was built speculatively. When
  phase 2 migrates claude-code, it needs `wait()` to know "was this handle's `cancel()`
  already called" to report `AttemptState::Cancelled`. Two shapes were considered and
  rejected for *this* phase (both would have added dead code with no caller, which
  `.claude/scope-discipline.md` flags as the recurring defect in this tree):
  1. A generic `cancelled: Mutex<BTreeSet<String>>` on `LocalProcessHarness` itself, checked
     by `wait()` and passed to `outcome()` as an extra bool parameter.
  2. Folding "was cancelled" into `G::RunState` and letting `cancel()` mutate the stored
     state before `wait()` reads it back out — awkward, because `cancel()` currently
     *removes* the entry from `running` via `take_running` (matching both adapters' original
     "take ownership, do not put it back" behavior), so there is no entry left for a
     concurrent `wait()` to consult. **Either shape is a small, additive change to
     `local_process.rs`, not a redesign of the trait or `LocalProcessHarness`'s fields** —
     flagging it here so phase 2 does not have to rediscover that constraint from scratch.
- **`stage_run_log` has no trait hook**, as noted in row 7 above. This was a deliberate
  choice, not an oversight: since `outcome()` already owns 100% of `wait()`'s classification
  logic per-grammar, adding a *second*, narrower hook just for the staging root would only
  fragment one already-atomic per-grammar concern into two, for no caller that needs to
  intercept staging independently of the rest of `outcome()`. Phase 2 should give
  `ClaudeCodeGrammar::outcome` the same shape (call its own `stage_run_log` inline) rather
  than expect a hook that isn't there.
- **`AttemptState`, `HarnessAdapter`, `HarnessProbe`, `LocalRunHandle`, `AttemptJournal`,
  `Timestamp` are re-exported from `codex.rs` behind `#[cfg(test)]`, not imported and used
  directly**, because `codex.rs`'s own production code no longer references them (they moved
  into `local_process.rs`) but `codex/tests.rs` still needs them via its `use super::*;` —
  and keeping that file byte-identical to develop was worth a `#[cfg(test)] pub(crate) use`
  line in `codex.rs` rather than editing 30-plus call sites in a 1148-line test file for
  cosmetic reasons. Phase 2 will likely want the same pattern for
  `claude_code/tests.rs` if it turns out any of that file's own imports become unused in
  `claude_code.rs` post-migration — check first; it may not need it at all, since
  `claude_code.rs`'s frozen `HarnessAdapter`/`HarnessProbe` impls are exactly what phase 2
  is deleting from that file, which is the same shape that created the need here.

## Tests

- Command: `cargo nextest run --workspace -E 'test(codex::tests::)'` — `26 tests run: 26
  passed` (default; 2 more are `#[ignore]`d live tests). `--run-ignored ignored-only` on
  the same filter: `2 tests run: 2 passed` (both self-skip: no `codex` binary on this
  machine, exactly as before). Total 28, matching `git show develop:crates/tack-runner/src/harness/codex/tests.rs
  | grep -c '#\[.*test'` minus its 2 comment-only false-positive matches (`#[test]`/`#[ignore]`
  mentioned in prose, not code).
- `cargo nextest run --workspace -E 'test(claude_code::tests::)'` — `36 tests run: 36
  passed` — unaffected, confirming no shared import broke.
- Full suite: `cargo nextest run --workspace` — `1439 tests run: 1439 passed, 8 skipped` —
  matches `TODO.md`'s recorded Wave 29 baseline exactly.
- `codex/tests.rs` is byte-identical to `develop` (`diff <(git show develop:...) crates/tack-runner/src/harness/codex/tests.rs`
  produces no output) — the strongest available proof that no assertion changed, since
  nothing in the file changed at all.

## Claim → evidence

| Claim | Evidence |
|---|---|
| `cargo build --tests` compiles clean | `cargo build --workspace --tests` — clean, no warnings |
| No codex test silently dropped | 28/28 accounted for (26 run + 2 ignored, live-skip) vs 28 in develop's file |
| `claude_code` tests unaffected | `test(claude_code::tests::)` — 36/36, matches develop |
| Full-suite baseline held | `cargo nextest run --workspace` — 1439/1439, 8 skipped, matches `TODO.md` |
| `codex/tests.rs` unchanged | byte-for-byte diff against `develop` is empty |
| `check --changed` passes | `✓ maintainability budgets hold (3 files checked)` |
| `cargo fmt --all -- --check` clean | ran after every edit; final state clean |
| `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml -- --check` clean | ran once; clean (this crate does not depend on `tack-runner`'s harness code, so it was never at risk) |
| `cargo clippy --workspace --all-targets -- -D warnings` clean | ran after every edit; final state clean |
| `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D warnings` clean | ran once; clean |
| `.githooks/pre-push` (the actual gate) passes end to end | ran directly; `✓ pre-push checks passed` |
| No board archaeology introduced | `scripts/check-comments.sh` — clean (two early drafts of the module docs above named "the handoff" and a `docs/agent-handoffs/...` path directly; both were rewritten to state the underlying fact instead before this check passed) |
| No test-hygiene regression | `scripts/check-test-hygiene.sh` — clean |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/src/harness/codex.rs crates/tack-runner/src/harness/codex/tests.rs crates/tack-runner/src/harness/local_process.rs`

Before (`develop` at `5766d69`; `local_process.rs` did not exist):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/codex/tests.rs                  0     0   0%   1149   26    22   54   88    0
crates/tack-runner/src/harness/codex.rs                       778   167  17%      0    0     0    0    0    8
```

After (final SHA `d7b3721`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/codex/tests.rs                   0     0   0%   1149   26    22   54   88    0
crates/tack-runner/src/harness/codex.rs                       699   156  18%      0    0     0    0    0   11
crates/tack-runner/src/harness/local_process.rs               270   137  33%      0    0     0    0    0   15
```

`codex.rs` alone shrank (778 → 699 production lines, −79); `codex.rs` + `local_process.rs`
combined grew (778 → 969, +191). This is expected and named in the audit itself (§3, §6):
a shared core's cost is not amortized until a second consumer exists. `local_process.rs`'s
own comment share (33%) is high because it is one `HarnessGrammar` trait definition, mostly
doc comments explaining *why* each hook exists (the asymmetry it carries) rather than
production logic — exactly the kind of comment `.claude/scope-discipline.md` and this
repo's comment rule want, since the trait boundary itself is the non-obvious design choice
a future reader needs justified. `codex/tests.rs` is byte-identical (1149 → 1149 test
lines, 26 → 26 tests, `max`/`avg`/`name` unchanged) — the strongest confirmation available
that this phase changed structure, not behavior.

## What was removed

Only code that became dead because it moved into `LocalProcessHarness`, per the card's
scope for this phase — no test pruning (that is phase 3's job, explicitly deferred):

- `CodexAdapter`'s own `HarnessAdapter`/`HarnessProbe` trait impls (the entire
  `validate`/`start`/`cancel`/`wait`/`reconcile`/`probe`/`declared_capabilities` bodies,
  minus the codex-specific logic each one wrapped) — now `LocalProcessHarness<G, C>`'s
  single generic impl.
- `RunningCodexProcess` (the old per-attempt bookkeeping struct) — replaced by the generic
  `RunningProcess<S>` (shared shape: process/secrets/limits/started_at) plus the new,
  codex-only `CodexRunState` (workspace facts, model provider/id, harness version — exactly
  the fields `RunningCodexProcess` had beyond what is now generic).
- `CodexAdapter`'s `running: Mutex<BTreeMap<...>>`, `secrets: SecretStore`, `providers:
  BTreeMap<...>` fields and `with_providers` builder — now `LocalProcessHarness`'s.
- The free `rfc3339`/`not_measured` helpers that used to live in `codex.rs` — `rfc3339`
  moved to `local_process.rs` (used once, by the shared `cancel`); `not_measured` is now
  `local_process::not_measured`, reused by `CodexGrammar::outcome` instead of a
  codex-local copy.
- `CodexAdapter::take_running` — now `LocalProcessHarness::take_running`, generic over
  `G::RunState`.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree without
touching `scripts/maintainability-baseline.json`. `codex.rs` got smaller, not bigger;
`local_process.rs` is a new file measured against the default (non-ratcheted) budget, which
it clears (252→270 prod lines against whatever the tool's default ceiling is — `check`
reported no violation).

## Budget check

```
✓ maintainability budgets hold (3 files checked)
```

This phase was **not** required to bring `codex` under the audit's final §5 target (≤400
production lines, ≤15 tests) — that is phase 3's job, after `claude_code` also lands on this
core and the two can be pruned together without re-duplicating whatever moves into
`harness/mod.rs`'s own shared test suite. `codex.rs` alone is at 699 lines today; the
`HarnessGrammar` impl block is the overwhelming majority of that (the `CodexLocator`/
version-scanning/`toml_quoted` helpers are a small remainder), which lines up with the
audit's own prediction that "the harness itself" (grammar + descriptor) is the residual
cost once the lifecycle is shared.

## What a stranger still cannot do

- **Cannot yet see `claude_code` migrated onto this trait** — that is phase 2, and this
  handoff's "didn't fit cleanly" section is written specifically so that agent does not have
  to re-derive the `cancelled`-tracking gap or the `stage_run_log`-has-no-hook decision from
  scratch.
- **Cannot yet measure the audit's four-harness cost claim** (§3: "≈250–350 production lines
  per new harness") — that only becomes a real measurement once a second grammar exists;
  today `local_process.rs` has exactly one consumer, so its own line count is not yet
  amortized evidence of anything.
- **Cannot yet see the shared lifecycle's own test suite** (audit §5 rule 1: "the lifecycle
  is tested once, in `harness/mod.rs`, against `fake_harness.sh`"). All 26 non-live codex
  tests still live in `codex/tests.rs`, several of which (the redaction canary test, the
  untracked-handle-rejection test, `probe_never_hangs_past_its_own_timeout`) are now
  provably exercising `local_process.rs`'s own shared code paths rather than anything
  codex-specific — but this phase's instructions were explicit that this pruning/dedup
  ("tested once") is phase 3's job, done once both adapters are migrated, not this one's.
- **`bootstrap.rs` was not touched at all** — `CodexAdapter::discover(process_limits,
  artifact_staging_root, secrets)` kept the exact same signature and infallible-constructor
  behavior via the type alias, so no caller needed updating. Verified by full-workspace
  build and by `harness/tests.rs`'s own `CodexAdapter::for_fixture` call sites (used
  directly, not boxed, and boxed as both `HarnessAdapter` and `HarnessProbe`) still
  compiling and passing unchanged.

## Context spent

- Read in full before designing: `docs/plans/harness-maintainability-audit.md`,
  `crates/tack-runner/src/harness/codex.rs`, `crates/tack-runner/src/harness/claude_code.rs`
  (read-only, to confirm the trait shape could hold both — not edited), `crates/tack-runner/src/harness/codex/tests.rs`,
  `crates/tack-runner/src/harness/mod.rs`, `crates/tack-runner/src/harness/process.rs`,
  relevant slices of `crates/tack-runner/src/engine.rs` and `crates/tack-runner/src/bootstrap.rs`,
  and `crates/tack-runner/src/provider/mod.rs` (for `Wire`/`ProviderEndpoint`/`resolve_endpoint`'s
  exact signatures).
- The two build-time surprises were both fixed inline rather than by redesigning: a `?Send`
  future in `prepare()` from holding a `std::sync::MutexGuard` across an `.await` (fixed by
  cloning the cached value out in its own statement before the `match`), and the
  maintainability ratchet catching a 1-line net growth in `codex/tests.rs` from a
  since-abandoned import edit (fixed by moving the needed re-exports into `codex.rs` behind
  `#[cfg(test)]` instead, which incidentally also made `codex/tests.rs` end up byte-identical
  to `develop`).
- Files opened and not used for editing: `docs/TESTING.md`, `TODO.md`'s IX-M5 section
  (read to confirm phase scope and the Wave 29 baseline number), `docs/plans/harnesses.md`
  was not opened (out of scope for this phase — no `tack-runner` file it plans for exists
  yet).

## Amendments

*(none yet)*
