# IX-M5-prune-codex handoff

**Phase 3 of 3 for card IX-M5 (Wave 30), the "codex" half.** Phases 1-2 extracted
`LocalProcessHarness<G: HarnessGrammar>` and migrated both `codex.rs` and `claude_code.rs`
onto it with no behavior change. This phase prunes `codex`'s tests to the shape
`docs/plans/harness-maintainability-audit.md` §5 specifies, working only inside
`codex.rs`, `codex/tests.rs`, `fixtures/codex/**`, and a new `tests/live/**`. A second,
parallel agent pruned `claude_code.rs` in a separate worktree/branch
(`/tmp/ix-m5-prune-claude_code`, `agent/ix-m5-prune-claude_code`) at the same time — its
result is not visible here and is not assumed.

- Base SHA / branch / final SHA: `461256c` (`develop`, the phase-2 merge commit) /
  `agent/ix-m5-prune-codex` / `ac24504` (worktree: `/tmp/ix-m5-prune-codex`,
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m5-prune-codex`).
- Files changed:
  - `crates/tack-runner/src/harness/codex/tests.rs` — rewritten: 26 non-live tests
    consolidated to 17 (see "What was removed/consolidated" below); the 2 live tests
    removed entirely (moved, not deleted).
  - `crates/tack-runner/src/harness/codex.rs` — one inline comment shortened (the
    `wire_api` finding moved to the fixture README, per rule 6); no other change. Still
    699 → 700-ish production lines, unchanged by this phase (see "Budget check").
  - `crates/tack-runner/src/harness/fixtures/codex/README.md` — one new bullet
    (`wire_api` default, moved out of `codex.rs`'s inline comment).
  - `crates/tack-runner/tests/live/main.rs` — **new**. The integration-test binary entry
    point (`tests/live/main.rs` is cargo's documented multi-file-test-binary convention);
    declares `mod codex;` and re-exports `tests/common/mod.rs` via `#[path]` since a
    module path inside `tests/live/` does not resolve to the top-level `tests/common/`
    directory on its own.
  - `crates/tack-runner/tests/live/codex.rs` — **new**. The two live tests, rewritten
    against the crate's public API only (an integration-test binary cannot see private
    items — `spec_with`/`deterministic_fixture_repo`/`test_secret_store` and friends had
    to be reconstructed from `tack_runner::{client, config, harness, SecretStore}`
    rather than reused, since the originals are private to `codex/tests.rs`).
  - `claude_code.rs`, `claude_code/tests.rs`, `fixtures/claude_code/**`,
    `local_process.rs`, `harness/mod.rs` — **not touched**, per the card's explicit
    scope for this phase.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/claim.response.json` is
  read by both `codex/tests.rs`'s `spec_with` and the new `tests/live/codex.rs`'s own
  copy of it, unchanged from before).
- Behavior implemented: none — every claim a removed test proved is still proven, either
  unchanged or folded into a table-driven case row.

## Why `tests/live/main.rs` exists as a separate file

An integration test file under `tests/` is its own crate root; a bare `mod live;`
declaration was not an option because `tests/live/codex.rs` needed the crate's
public API only, and cargo auto-discovers `tests/<name>/main.rs` as a target named
`<name>` (the same convention `cargo`'s own test suite uses for a multi-file
integration-test binary) — so `tests/live/main.rs` becomes the "live" binary, and
`mod codex;` inside it pulls in `tests/live/codex.rs` as a submodule. The parallel
`claude_code` pruning agent independently needs the same entry file to add its own
`mod claude_code;` line; this is an expected merge conflict for whoever integrates both
branches (a two-line `mod` list, trivial to combine), not a design flaw — flagged here so
the integrator does not have to rediscover it.

## What was removed / consolidated, mapped to §5 rules

All by **rule 2** ("one test per contract claim; variants are rows") unless noted.
Every case array/vec literal lives in its own free function returning `Vec<CaseStruct>`
(the rustfmt-explosion workaround this workspace's CLAUDE.md documents), so the counted
`#[test]`/`#[tokio::test]` function body is just a `for case in helper() { ... }` loop.

| New test (rows/claims it carries) | Replaces (old tests) |
|---|---|
| `validate_rejects_pre_spawn_selection_problems` (3 rows: mismatched harness kind, auto-selected model, unresolvable binary) | `validate_rejects_a_mismatched_harness_kind`, `validate_rejects_an_auto_selected_model_pre_spawn`, `validate_rejects_an_unresolvable_binary` |
| `wait_classifies_terminal_state_from_the_exit_code_alone` (2 rows: exit-code failure, malformed-but-exit-0) | `fake_binary_failure_completes_failed_with_the_exit_code_in_terminal_reason`, `fake_binary_malformed_output_does_not_panic_and_still_produces_a_typed_result` |
| `probe_reports_version_or_an_explicit_error_never_a_fake_success` (5 rows: recognized version, program-prefixed version, unrecognized version, malformed, nonzero exit) | `probe_reports_a_recognized_version_with_no_error` (version-parsing half only — see next row), `probe_recognizes_a_program_name_prefixed_version_string`, `probe_reports_an_unrecognized_version_string_as_an_explicit_probe_error`, `probe_reports_malformed_version_output_as_an_explicit_probe_error`, `probe_reports_a_nonzero_exit_as_an_explicit_probe_error` |
| `probe_attests_model_passthrough_when_no_models_are_enumerable` (new, split out) | the `model_passthrough`/`model_combinations` half of `probe_reports_a_recognized_version_with_no_error` — a distinct contract claim (schedulability attestation, not version parsing), so it kept its own test rather than becoming a per-row special case |
| `reconcile_handles_a_missing_or_unrecognized_process_id` (2 assertions, same adapter instance) | `reconcile_with_no_recorded_process_id_reports_stopped_without_dispatch`, `reconcile_rejects_an_unrecognized_handle_encoding_as_explicitly_unavailable` |
| `reconcile_reports_real_process_liveness_by_pid` (both directions, one test) | `reconcile_observes_a_real_alive_process_as_running`, `reconcile_observes_a_dead_pid_as_stopped` (`#[cfg(unix)]`, unchanged) |
| `provider_endpoint_credential_reaches_the_process_only_when_the_request_names_it` (2 rows: direct model, configured provider) | `a_direct_model_request_spawns_with_no_provider_endpoint_variable_present`, `a_configured_provider_request_spawns_with_its_endpoint_variable_present` |

**Rule 4** ("a test name names the claim; no articles"): renamed
`a_disabled_provider_rejects_the_request_before_any_process_spawns` →
`disabled_provider_rejects_the_request_before_any_process_spawns` (the leading-subject
article was the only clear instance in this file; the file's existing style already uses
mid-name "the"/"an" idiomatically elsewhere — e.g. `cancel_kills_the_whole_descendant_tree_via_the_adapter`,
carried over unchanged from phases 1-2 — and renaming those would be cosmetic churn on
names phase 1/2 already approved, not a narrative-vs-fact fix).

**Rule 3** ("vendor output is a file, never a string literal") — checked, nothing to do:
per phase 1's handoff, codex never parses harness output content (`classify_exit` reads
only the process exit), so no test holds an inline captured-transcript string to move.
`fixtures/codex/README.md` still correctly says "no captured transcripts live here yet."

**Rule 4 (live tests) / rule 4 numbering in the card, "anything that runs a real binary
lives under `tests/live/`"**: `live_probe_and_artifact_staging_against_a_real_codex_binary_when_present`
and `live_codex_through_the_configured_provider_when_opted_in` moved to
`crates/tack-runner/tests/live/codex.rs`, `#[ignore]` preserved verbatim (same reason
strings), both still self-skip with no real `codex` binary present (verified below).

**Rule 6** ("vendor-behaviour findings go in the fixture README, not a module preamble"):
`codex.rs`'s module doc (top of file) already pointed at the README and held no vendor
finding of its own — nothing to move there. One inline finding *did* need moving: the
`wire_api="responses"` non-load-bearing-on-0.149.1 note inside `prepare()`, now a bullet
in `fixtures/codex/README.md`'s "Measured" section, with the inline comment shortened to
point at it. Everything else that looked like a vendor finding embedded in the file
(the `FeatureCapabilities` `reason` strings) is not a comment at all — it is production
data returned by `feature_capabilities()` as part of the actual capability response, so
it cannot move to a doc without changing behavior; left alone.

**Rule 1** ("the lifecycle is tested once, in `harness/mod.rs`") — explicitly **not**
acted on, per the card's own instruction that this is a later, separate phase. See
"Suspected cross-adapter/lifecycle duplicates" below for what was found and left alone.

## No behavior change to what's proven

Every claim the original 26 non-live tests proved is still asserted by the new 17 — each
consolidation table above lists the exact old test names a new one replaces, and every
assertion from each old test (including the ones on `model_combinations`/`model_passthrough`
that got split out to their own test rather than dropped) is present somewhere in the new
file. Nothing was silently removed. The 2 live tests are relocated verbatim (same
assertions, same `#[ignore]` reason strings, same self-skip preconditions), not rewritten
in substance — only their construction of `ExecutionSpec`/`SecretStore`/etc. changed, from
private in-crate helpers to the public API an integration test binary can see.

## Tests

- `cargo build --workspace --tests` — clean, no warnings.
- `cargo nextest run --workspace -E 'test(codex::)'` — `17 tests run: 17 passed` (down
  from 26; the 2 live tests are gone from this filter's binary and now live in the `live`
  integration binary, matched separately below). `--run-ignored ignored-only` on the same
  filter — `2 tests run: 2 passed`: nextest's test id includes the binary and module path
  (`tack-runner::live codex::live_...`), so `test(codex::)` still finds the relocated live
  tests in the new binary; both still self-skip (no `codex` binary on this machine).
- `cargo nextest run --workspace -E 'binary(live)'` — `0 tests run, 2 skipped` (both
  `#[ignore]`d, correctly not run by default). `--run-ignored ignored-only` on the same
  filter — `2 tests run: 2 passed` (both self-skip at runtime and exit successfully,
  exactly as before the move).
- Full suite: `cargo nextest run --workspace` — `1430 tests run: 1430 passed, 8 skipped`.
  This is `1439 - 9` against the phase-2 baseline (`1439 tests run: 1439 passed, 8
  skipped`) — the exact drop expected from consolidating 26 non-live codex tests down to
  17 (9 fewer test *functions*, zero fewer proven claims); the 8 skipped count is
  unchanged (the 2 relocated live tests are still ignored, just in a different binary).
  One unrelated test, `tack-cli::local_runner::tests::setting_a_provider_secret_while_running_stops_the_old_task_before_anything_else`,
  failed once under full-workspace parallel load and passed 3/3 in isolation and on a
  full-suite rerun — a pre-existing flaky test in a crate this phase never touched, not a
  regression from this change.

## Claim → evidence

| Claim | Evidence |
|---|---|
| `cargo build --workspace --tests` compiles clean | ran after every edit; final state clean, no warnings |
| No codex claim silently dropped | consolidation table above maps every one of the 26 old non-live tests to exactly one new test; the 2 live tests are relocated, not rewritten in substance |
| `claude_code`/other crates unaffected | this phase touched only `codex.rs`, `codex/tests.rs`, `fixtures/codex/README.md`, and new `tests/live/**` files; full-suite total (1430 + 8 = 1438) accounts for exactly codex's own reduction (see "Tests") |
| Full-suite baseline held (modulo the expected, documented reduction) | `cargo nextest run --workspace` — 1430/1430 passed, 8 skipped, matching `1439 - 9` |
| `check --changed` / explicit `check` on the touched+new files passes | `✓ maintainability budgets hold` (both invocations — `--changed` sees only the modified `codex/tests.rs` since git treats the new `tests/live/` directory as one untracked unit; an explicit `check` naming all 4 files also passes) |
| `cargo fmt --all -- --check` clean | ran after every edit (one real reformat needed after the first table-driven draft — multi-line `matches!`/`assert_eq!` wrapping); final state clean |
| `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml -- --check` clean | ran once; clean |
| `cargo clippy --workspace --all-targets -- -D warnings` clean | ran after every edit; final state clean |
| `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D warnings` clean | ran once; clean |
| `.githooks/pre-push` (the actual gate) passes end to end | ran directly twice (once mid-change, once final); `✓ pre-push checks passed` both times |
| No board archaeology introduced | `scripts/check-comments.sh` — clean |
| No test-hygiene regression | `scripts/check-test-hygiene.sh` — clean (`✓ tests take their temporary paths from a guard`) |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/src/harness/codex.rs crates/tack-runner/src/harness/codex/tests.rs crates/tack-runner/tests/live/codex.rs crates/tack-runner/tests/live/main.rs`

Before (base `461256c`, the phase-2 merge — `tests/live/**` did not exist):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/codex/tests.rs                   0     0   0%   1149   26    22   54   88    0
crates/tack-runner/src/harness/codex.rs                       700   160  18%      0    0     0    0    0   11
```

After (final SHA `ac24504`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/codex/tests.rs                   0     0   0%   1005   17    26   51   88    0
crates/tack-runner/src/harness/codex.rs                       700   159  18%      0    0     0    0    0   11
crates/tack-runner/tests/live/codex.rs                          0     0   0%    286    0     0    0    0    6
crates/tack-runner/tests/live/main.rs                           0     0   0%     12    0     0    0    0    6
```

`codex/tests.rs`: 26 → 17 tests (−9, exactly the consolidation table above), 1149 → 1005
test lines, max test-fn length 54 → 51 lines (still comfortably under the 60-line
target). `tests/live/codex.rs` reports `#t=0` despite holding 2 real `#[tokio::test]`
functions: this is the **same known tool limitation phase 2's handoff already
documented** for multi-line `#[ignore = "...\n...")]` attributes (the line-based scanner
in `scripts/maintainability.py::tests_in` gives up when an attribute's string literal
wraps onto a second line that doesn't itself start with `#[`) — the trustworthy count is
nextest's own report (2 tests, both ignored, both passing under `--run-ignored
ignored-only`), not this column. `codex.rs` itself is essentially unchanged (700 → 700
production lines; 160 → 159 comment lines, the one-line net shrink from moving the
`wire_api` finding to the README).

## Budget check (audit §5 rule 7: ≤400 production lines, ≤15 tests)

```
✓ maintainability budgets hold (2 files checked)     # check --changed
✓ maintainability budgets hold (4 files checked)     # explicit check on all touched+new files
```

Both pass — but "budgets hold" here means the *ratchet* (no file got worse than its own
baseline), not the audit's absolute §5 target. Measured against that target directly:

- **Tests: 17, not ≤15.** Two over. Getting to exactly 15 without either (a) duplicating
  what the later shared-lifecycle-suite phase will test in `harness/mod.rs`, or (b)
  merging tests that prove genuinely distinct claims into one unfocused test, was not
  found. The two most plausible further cuts — folding
  `reconcile_handles_a_missing_or_unrecognized_process_id` and
  `reconcile_reports_real_process_liveness_by_pid` into one, or merging
  `cancel_and_wait_on_an_untracked_handle_are_typed_rejections` into the provider-env
  table — were tried and reverted: both produced a test whose name could no longer state
  one claim, which is what rule 5 is for. 17 is the real number; forcing it to 15 would
  have cost either rule 1's scope (this phase's) or rule 5's.
- **Production: 700 lines, not ≤400.** This phase did not reduce `codex.rs`'s production
  line count at all (only a 1-line net comment shrink) — nothing in this phase's mandate
  (test consolidation, live-test relocation, vendor-finding relocation) touches
  `CodexGrammar`'s actual logic. Phase 1's own handoff already flagged this explicitly:
  "This phase was not required to bring codex under the audit's final §5 target... that
  is phase 3's job" — but re-reading the audit's §5 rule 7 and §6 exit criteria against
  what this phase's own instructions actually enumerate (six numbered items, all about
  tests/fixtures/live-tests/comments), there is no step in this phase's mandate that
  would shrink `CodexGrammar`'s ~500-line `HarnessGrammar` impl block (`prepare`'s
  provider-injection argument construction, `feature_capabilities`'s five justified
  `CapabilityValue`s, `outcome`'s classification) without either moving genuine
  per-adapter logic into the shared core (a `local_process.rs` design change explicitly
  out of scope for this phase) or deleting a capability-reasoning string that is itself
  load-bearing production behavior. Per this repo's "measured, not promised" rule: 700 is
  the real, re-measured number, not a promise the 400 target was met.

## Suspected cross-adapter / lifecycle duplicates (for the later phases, not acted on here)

Per the card's explicit instruction, these are flagged for the shared-lifecycle-suite
phase and the cross-adapter dedup phase, not moved or removed by this one:

- **`disabled_provider_rejects_the_request_before_any_process_spawns`** (this file) and
  **`a_disabled_provider_rejects_the_request_before_any_process_spawns`**
  (`claude_code/tests.rs`) — `scripts/maintainability.py duplicate-tests` still flags
  these as near-identical (ratio > 0.75) even after the rule-4 rename dropped the leading
  article. Both prove the same shape of claim (a disabled provider entry rejects at
  `validate`) with adapter-specific set dressing; a real candidate to fold into one
  cross-adapter acceptance test once both files are stable.
- **`cancel_kills_the_whole_descendant_tree_via_the_adapter`** (this file) and
  **`cancel_kills_the_whole_descendant_tree_not_only_the_direct_child`**
  (`harness/process/tests.rs`) — also flagged by `duplicate-tests`. The `process.rs` test
  proves the primitive (`SupervisedProcess::cancel` kills a process group); this file's
  test proves the same outcome reached through the full adapter (`start`/`cancel`). Both
  are arguably legitimate at different layers (unit vs. adapter-level acceptance), but
  worth a second look once `harness/mod.rs`'s shared suite exists.
- **`reconcile_handles_a_missing_or_unrecognized_process_id`** and
  **`reconcile_reports_real_process_liveness_by_pid`** — not flagged by
  `duplicate-tests` (names diverged enough after this phase's consolidation), but per
  phase 1's own handoff, `local_process.rs`'s shared `reconcile()` does the pid-decode
  and `process_alive`/no-handle checks generically before ever calling into
  `CodexGrammar`; what these two tests actually exercise is mostly that shared code path,
  not anything codex-specific (codex's own `reconcile_alive` hook is a one-line
  unconditional `ProcessRunning`). Strong candidates to become `harness/mod.rs`-level
  tests run once against `fake_harness.sh`, per audit rule 1, rather than being pinned
  per-adapter.
- **`cancel_and_wait_on_an_untracked_handle_are_typed_rejections`** and
  **`unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever`**
  — both assert behavior of `local_process.rs`'s shared `take_running`/pre-spawn-rejection
  plumbing (an untracked handle is `Err(HarnessError::Process)`; a pre-spawn rejection
  never populates `running`) rather than anything `CodexGrammar` decides. Same rule-1
  candidacy as the reconcile pair above.
- **`harness_kind_matches_what_probe_itself_reports`** and
  **`declared_cancel_capability_is_advisory_not_supported`** — the first is fully generic
  (`LocalProcessHarness::probe()`'s `harness_kind` field always equals
  `HarnessProbe::harness_kind`, for any grammar); the second pins codex's own
  `feature_capabilities().cancel` value, which is genuinely adapter-specific data, not
  shared-core behavior — only the first is a duplicate candidate.

`python3 scripts/maintainability.py duplicate-tests` at this phase's end: `tack-runner: 4
near-identical pairs across files` (2 of which touch codex, both listed above; the other
2 — `reconcile_with_no_recorded_process_id_needs_no_dispatch` vs
`harness/tests.rs`'s test of the same name, and a `provider/anthropic.rs` vs
`provider/vercel_ai_gateway/tests.rs` pair — are pre-existing and outside this phase's
files). Workspace-wide total: 52 pairs (`tack-db` 3, `tack-orch` 5, `tack-runner` 4,
others 0) — unchanged in the other crates by this phase, and not expected to hit zero
until claude_code's parallel pruning lands and the cross-adapter dedup phase runs, per
the audit's own exit criteria in §6.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` and an explicit `check` on all
4 touched/new files both pass on the final tree without touching
`scripts/maintainability-baseline.json`. Every changed file's numbers either shrank
(`codex/tests.rs`: fewer tests, fewer lines) or stayed within the default budget for a
new file (`tests/live/codex.rs`, `tests/live/main.rs`).

## What a stranger still cannot do

- **Cannot see the codex adapter under the audit's 400-production-line target.** It is at
  700, unchanged by this phase, for the reasons in "Budget check" above — reaching 400
  requires either a `local_process.rs` design change (out of scope here) or cutting
  genuine per-adapter behavior/reasoning strings.
- **Cannot see the lifecycle tested once in `harness/mod.rs`.** All 17 remaining codex
  tests still live in `codex/tests.rs`; the "Suspected cross-adapter / lifecycle
  duplicates" section above is written so that phase does not have to rediscover which
  ones are candidates from scratch.
- **Cannot see `claude_code`'s own phase-3 pruning** — that ran in parallel, in a
  different worktree/branch, and its result is not visible from here. Whoever integrates
  both branches needs to: merge `tests/live/main.rs`'s `mod` list (both agents add a line
  to the same new file — see "Why `tests/live/main.rs` exists" above), then re-run
  `duplicate-tests` once both landed, since several of this handoff's "suspected
  duplicate" pairs name a `claude_code/tests.rs` test this agent never saw post-pruning.
- **`bootstrap.rs` was not touched at all** (this phase had no reason to) —
  `CodexAdapter::discover`'s signature and behavior are exactly as phase 1 left them.

## Context spent

- Read in full before making any change: `docs/agent-handoffs/part-ix/IX-M5-harness-core-codex.md`,
  `docs/agent-handoffs/part-ix/IX-M5-harness-core-claude_code.md` (both, per the card's
  instruction), `docs/plans/harness-maintainability-audit.md` §5-§6, the full
  pre-change `codex.rs` (918 lines) and `codex/tests.rs` (1148 lines),
  `fixtures/codex/README.md`.
- Read to determine what an integration-test binary can and cannot see (since the live
  tests needed rewriting against the public API): `crates/tack-runner/src/lib.rs`,
  `harness/mod.rs`'s `pub mod` list, `client.rs`'s `pub use` list, `config.rs`,
  `harness/{locate,artifact,sha256,process}.rs`'s pub surfaces, `engine.rs`'s
  `ExecutionSpec`, `workspace.rs`'s `Workspace`, and two existing integration tests
  (`tests/h3_checkout.rs`, `tests/crash_matrix.rs`) as worked examples of the same
  claim-fixture-construction pattern and the `mod common;` convention.
  `tests/common/mod.rs` itself (24 lines) was read but not extended — its two existing
  helpers (`temp_dir`, `usage`) didn't cover codex's own fixture-repo/secret-store/
  spec-building needs, and duplicating those three small helpers into
  `tests/live/codex.rs` was judged cheaper and clearer than growing a shared file two
  crates(-worth of live tests) might disagree about the shape of later.
- One real surprise: `cargo build -p tack-runner --test live` compiled and ran cleanly on
  the *first* attempt after cross-referencing every pub path against source rather than
  guessing — the only iteration needed was two import-organization fixes (a
  `client::HarnessAdapter` duplicate `use` path merged into the existing `client::{...}`
  group, and a stray reference to a nonexistent `HarnessAdapterExt`/`crate::VERCEL_AI_GATEWAY_PROVIDER`
  from an early draft, both caught by the compiler before the first successful build).
- The `scripts/maintainability.py` multi-line-`#[ignore]`-attribute undercount
  (`tests/live/codex.rs` reporting `tests=0`) was recognized immediately as the same
  quirk phase 2's handoff already named for codex's own pre-pruning count, rather than
  re-diagnosed from scratch.
- Files opened and not used for editing: `docs/TESTING.md`, `TODO.md`'s IX-M5 section (to
  confirm phase scope and the recorded 1439-test baseline), `docs/plans/harnesses.md`
  (out of scope, unrelated to this phase).

## Amendments

*(none yet)*
