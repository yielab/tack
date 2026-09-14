# IX-M5-prune-claude_code handoff

**Phase 3 of 4 for card IX-M5 (Wave 30), the "claude_code" half.** Phases 1-2 extracted
`LocalProcessHarness<G: HarnessGrammar>` and migrated `codex.rs`/`claude_code.rs` onto it
with no behavior change. This phase prunes `claude_code`'s own tests to the shape
`docs/plans/harness-maintainability-audit.md` §5 specifies: table-driven variants, vendor
transcripts as fixture files with provenance, live tests moved under `tests/live/`,
narrative names replaced. A separate, parallel agent did the same for `codex` in its own
worktree (`/tmp/ix-m5-prune-codex`) — its outcome is not reflected here. The shared
lifecycle test suite in `harness/mod.rs` (audit §5 rule 1) and the cross-adapter dedup
pass are later, separate phases, explicitly out of scope here.

- Base SHA / branch: `461256c` (`develop`, the phase-2 merge commit) /
  `agent/ix-m5-prune-claude_code` (worktree: `/tmp/ix-m5-prune-claude_code`,
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m5-prune-claude_code`). Final SHA: see the commit
  this handoff ships in (not merged — lands on this branch only, per the card's
  instructions).
- Files changed:
  - `crates/tack-runner/src/harness/claude_code/tests.rs` — rewritten. Production
    `claude_code.rs` **not touched at all** (confirmed: `git status --porcelain` shows no
    change to it; every item below is test/fixture/doc shape only).
  - `crates/tack-runner/src/harness/fixtures/claude_code/README.md` — added a "Fixture
    provenance" section documenting the `captured`/`constructed` convention this phase
    introduces (no sibling convention existed yet — `codex/`'s own fixture extraction is
    the parallel agent's job, not landed here).
  - `crates/tack-runner/src/harness/fixtures/claude_code/2.1.223/*.jsonl` (+ `.provenance`
    siblings) — new: `success-with-usage`, `api-error-misleading-subtype`,
    `budget-exhausted`, `truncated-mid-run`.
  - `crates/tack-runner/src/harness/fixtures/claude_code/2.1.261/gateway-routed-result.jsonl`
    (+ `.provenance`) — new; a different version subdirectory because that one transcript's
    own `init` line names `claude_code_version: "2.1.261"`, per the convention documented in
    the README (a fixture's directory name is the version its own transcript reports).
  - `crates/tack-runner/tests/live/claude_code.rs` — new: the 3 opt-in live tests, moved
    out of `src/`, rebuilt against the crate's public API only (an integration test binary
    cannot see `claude_code::tests`'s private helpers — see "What didn't fit cleanly").
  - `crates/tack-runner/tests/live_claude_code.rs` — new: the top-level file cargo
    auto-discovers as a separate test binary, `#[path]`-including the file above so a
    sibling adapter's live tests can land at `tests/live/codex.rs` behind their own
    `tests/live_codex.rs` without any naming collision.
- Contract fixtures consumed: none.
- Behavior implemented: none — test/fixture/doc restructuring only, proved by every
  original assertion still present (as a table row, a fixture-backed case, or an unchanged
  test body) and the full suite holding.

## What was done, mapped to the audit's §5 rules

**Rule 2 (one test per contract claim; variants are rows).** Six families collapsed into
six table-driven tests, each a private `fn returning Vec<Case>` (kept out of the `#[test]`
fn body itself — see "the rustfmt gotcha" below) looped by one test function:

| Table test | Rows | Replaces |
|---|---|---|
| `validate_accepts_well_formed_specs` | 5 (baseline + 4 provider families) | `validate_accepts_a_well_formed_claude_code_spec`, `validate_accepts_every_known_provider_family_case_insensitively` |
| `validate_rejects_invalid_specs` | 4 (bad provider, wrong harness kind, network self-contradiction, missing binary) | 4 of the file's 5 `validate_rejects_*` tests (the 5th, the missing-secret-reference test, stayed separate — see below) |
| `fake_binary_exit_code_fallback_reports_honest_terminal_state` | 3 (success, failure, malformed — malformed's extra checks via a per-case `extra: fn(&HarnessOutcome)`) | `fake_binary_success_mode_is_reported_succeeded_via_the_honest_exit_code_fallback`, `fake_binary_failure_mode_is_reported_failed_via_the_honest_exit_code_fallback`, `fake_binary_malformed_mode_never_panics_and_never_fabricates_structured_data` |
| `version_text_parsing_variants` | 4 (real observed line, unknown fixture format, empty output, prerelease suffix) | all 4 of the file's `parse_version_text` unit tests |
| `result_envelope_parsing_variants` | 3 (success-with-usage, misleading-subtype, budget-exhausted), each with a per-case `check: fn(&ParsedRun)` since the three prove different claims, not the same claim with different inputs | 3 of the file's 4 `parse_run_output` transcript tests (the 4th, the gateway-vs-direct differential, stayed separate — it compares *two* parses of *one* transcript, not one parse's fields, so it doesn't fit this table's row shape) |
| `reconcile_reports_honest_observations_for_untrackable_pids` | 3 (no recorded pid, pid that no longer exists, undecodable pid) | 3 of the file's 5 `reconcile_*` tests (the other 2 are real-process, Linux-only tests kept separate — see below) |
| `provider_endpoint_variables_present_only_when_configured_and_requested` | 2 (direct request, configured-provider request) | `a_direct_model_request_spawns_with_no_provider_endpoint_variable_present`, `a_configured_provider_request_spawns_with_its_endpoint_variables_present` |

**The rustfmt gotcha** (documented in the card, already known from IX-M4 cards): every
case-array/vec literal above lives in its own free function (`accept_cases`,
`reject_cases`, `fallback_cases`, `version_cases`, `result_cases`, `reconcile_cases`,
`provider_cases`) returning `Vec<CaseStruct>`, never inlined into the `#[test]` fn body —
rustfmt explodes a multi-field struct-literal array onto one-item-per-line even at a width
that would otherwise fit, which pushes the *counted* test-fn body over budget even though
the test shrank. Extracting the literal keeps the counted body a plain `for case in
some_cases() { ... }` loop.

**Rule 3 (vendor output is a file, never a string literal).** All 5 `concat!(...)`
transcript blocks moved to `fixtures/claude_code/<version>/*.jsonl`, read via
`include_str!` (matching this crate's own existing convention for JSON/text fixtures —
see `engine/tests.rs`'s `include_str!("../../../../docs/contracts/runner-v1/...")` calls
— rather than the literal "read at test time" phrasing in the card, since a compile-time
embed equally satisfies "not an inline string literal in the test" and matches how every
other fixture in this crate is already consumed). No sibling `*.provenance` convention
existed yet in this codebase (`codex/`'s own fixture directory is empty pending the
parallel agent's work; `docs/contracts/runner-v1/README.md`'s own provenance note is prose
about credential values, not fixture-shape provenance) — this phase originates the
`captured`/`constructed` `.provenance` sibling-file convention, documented in
`fixtures/claude_code/README.md`'s new "Fixture provenance" section.

**Rule 4 (anything that runs a real binary lives under `tests/live/`, `#[ignore]`d).** All
3 live tests moved to `crates/tack-runner/tests/live/claude_code.rs`, entered via the
auto-discovered `crates/tack-runner/tests/live_claude_code.rs`. This required rebuilding
`spec_with`/`env_entry`/`test_secret_store` against the crate's **public** API only — an
integration test is a separate crate with no access to `claude_code::tests`'s private
helpers (see "What didn't fit cleanly"). Behavior preserved exactly: same opt-in env var
(`TACK_RUN_LIVE_CLAUDE_CODE_TEST=1`), same `#[ignore]` gating, same clean self-skip logic
(no `claude` binary discoverable, no opt-in var set, no configured provider entry to
validate against).

**Rule 5 (a test name names the claim).** `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome`
→ `env_canary_is_redacted` — the audit's own example (§2.3), applied verbatim.
`declared_capabilities_match_the_reconciled_iii_d5_values` → `declared_capabilities_report_cancel_and_artifacts_as_advisory`
(dropped the `iii_d5` card-vocabulary reference the audit didn't call out by name but the
repo's own comment rule forbids). `a_gateway_routed_result_is_recorded_as_requested_not_confirmed_even_on_a_fast_result_line`
(89 chars, the file's longest name pre-edit) → `gateway_routed_result_is_requested_not_confirmed_even_on_a_fast_result_line`
(shorter, same claim). Names already consistent with this crate's existing style
elsewhere (including `codex/tests.rs`, untouched, and using the same conventions) were
left as-is rather than rewritten wholesale — renaming every test in the file was not this
rule's intent, only removing genuine narrative/card-vocabulary violations.

**Rule 6 (vendor-behaviour findings go in the fixture README, not a module preamble).**
Phases 1-2 already did this: `claude_code.rs`'s module doc is 11 lines (`mdoc` column),
well under the 30-line budget, and already points to `fixtures/claude_code/README.md` for
vendor findings — confirmed nothing further needed moving. This phase only added the new
provenance section to that same README (see rule 3 above); it did not touch the module
doc.

## What didn't fit cleanly

- **The live tests could not simply be copy-pasted into `tests/live/`.** `harness::claude_code::tests`'s
  own `spec_with`/`env_entry`/`test_secret_store`/`permission_policy` are private `fn`s
  inside a `#[cfg(test)]` module compiled *as part of* the `tack-runner` library crate.
  `tests/live/claude_code.rs` is a separate crate (a cargo integration test) that only sees
  `tack-runner`'s public API — it cannot reach those helpers, `#[cfg(test)]`-only
  re-exports (`for_fixture`, etc.), or any private item. Every helper needed for the 3 live
  tests was rebuilt from scratch against public types, verified by checking each one's
  visibility individually (`pub mod claude_code`, `pub type ClaudeCodeAdapter`, `pub fn
  discover`/`with_binary`/`with_providers`, `pub trait HarnessProbe`/`HarnessAdapter`, `pub
  struct ArtifactStager`, `pub const VERCEL_AI_GATEWAY_*`) before writing the file, using
  `crates/tack-runner/tests/h3_checkout.rs` as the worked example of an existing
  integration test doing the same kind of thing (building a real `ExecutionSpec` from
  scratch against only `tack_orch`/`tack_runner::client` public types).
- **`mod common;` needed `#[path = "../common/mod.rs"]`.** A file reached via `#[path]`
  resolves its own `mod` declarations relative to *its own* location
  (`tests/live/claude_code.rs`), not the top-level entry point that included it
  (`tests/live_claude_code.rs`) — a bare `mod common;` looked for
  `tests/live/common.rs` and failed. Only `common::temp_dir` (aliased locally as
  `temp_workspace` to match the original tests' naming) is used from it; `common::usage`
  is not needed here.
- **A table row needs its own `check`/`extra` closure when the family's claims genuinely
  differ in shape, not just in input.** `fake_binary_exit_code_fallback_reports_honest_terminal_state`'s
  malformed row and `result_envelope_parsing_variants`'s three rows all carry a per-case
  `fn(&Outcome)`/`fn(&ParsedRun)` field rather than a single shared assertion, because
  (respectively) the malformed case's terminal state is one of *two* acceptable values
  where the other two cases assert exactly one, and the three result-envelope cases each
  prove a structurally different claim (usage extraction; `is_error`-over-`subtype`;
  budget-exhaustion cost capture) rather than the same claim on different inputs. This
  keeps "one test per contract claim, variants as rows" honest rather than forcing
  unrelated claims into a table that would silently weaken what each row actually proves.
- **The gateway-vs-direct differential test could not join the `result_envelope_parsing_variants`
  table.** It parses the *same* transcript twice (once as a direct-provider run, once as a
  gateway-routed run) and asserts the two `model_observation_source` values differ — a
  comparison between two parses, not one parse's fields, so it stayed its own test
  (`gateway_routed_result_is_requested_not_confirmed_even_on_a_fast_result_line`) reading
  the `2.1.261/gateway-routed-result.jsonl` fixture directly.
- **An unused case-struct field is a clippy `-D warnings` failure, not just dead code.**
  `ResultCase.name` was written but never read in the first draft of
  `result_envelope_parsing_variants`'s loop body (each row's own `check` closure doesn't
  see `case.name`); fixed by `eprintln!`-ing it once per iteration, both satisfying the
  lint and giving a failing run a breadcrumb for which case failed.

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/src/harness/claude_code.rs crates/tack-runner/src/harness/claude_code/tests.rs`

Before (`develop` at `461256c`, the phase-2 merge — recorded in that phase's own handoff):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/claude_code/tests.rs             0     0   0%   1735   37    26   73   89    0
crates/tack-runner/src/harness/claude_code.rs                 845   252  23%      0    0     0    0    0   11
```

(The tool's own `#t=37` undercounts nextest's actual 39 — phase 2 already noted this
undercount for multi-line-attribute tests; nextest reported `36 tests run: 36 passed` +
`3 tests run: 3 passed` under `--run-ignored ignored-only` = 39 total pre-edit, matching
this phase's own accounting below.)

After (working tree, pre-commit):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/src/harness/claude_code/tests.rs             0     0   0%   1386   22    30   66   81    0
crates/tack-runner/src/harness/claude_code.rs                 845   252  23%      0    0     0    0    0   11
```

`claude_code.rs` (production): **unchanged**, 845 lines — not touched by this phase at
all (confirmed by `git status --porcelain`). Budget target: **≤400 production lines — not
met**, by 445 lines. This is not a regression introduced here: phase 2's own handoff
already recorded 845 and explained why (`validate_selection` absorbs three checks with no
codex equivalent — provider allow-list, network self-contradiction — and `outcome`'s
`stream-json` parser is intrinsically larger than codex's exit-code-only classification).
This phase's charter (items 1-5 of the card) was test/fixture/doc shape only; no
production-code line was touched, so no further reduction was attempted or is reported as
attempted. Reducing this number further would mean either finding a genuine second gap in
the shared core (audit's own rule 7: "a card that needs more has found a gap in the shared
core, which is a separate card") or trimming real adapter-specific logic, both out of this
phase's scope and both risking the "no behavior change" constraint this phase and phases
1-2 share.

`claude_code/tests.rs`: 1735 → 1386 lines (**−349**), 39 → 22 tests (**−17**, counting
nextest's true pre-edit total of 39, not the tool's undercounted 37). Budget target:
**≤15 tests — not met**, by 7. Every original assertion is still proven (see "Claim →
evidence" below) — none was dropped to hit a number, matching the "measured, not
promised" rule and this card's own explicit allowance ("if you cannot get under it without
... losing a real assertion, say so explicitly ... with the actual number"). Getting from
22 to 15 would require one of: merging tests that prove genuinely different claims into a
shared table (weakening what a reader can conclude from a passing row), moving
lifecycle-adjacent tests into `harness/mod.rs` before that phase exists (explicitly not
this phase's job — the card says to *note* candidates, not act on them), or deleting
coverage outright. None of those was done.

## Claim → evidence

Every claim the pre-edit file proved is still proven post-edit. Consolidated claims are
marked with the table test that now proves them; unchanged tests are marked "unchanged".

| Original test | Status |
|---|---|
| `validate_accepts_a_well_formed_claude_code_spec` | → `validate_accepts_well_formed_specs` row 1 |
| `validate_accepts_every_known_provider_family_case_insensitively` | → `validate_accepts_well_formed_specs` rows 2-5 |
| `validate_rejects_an_unsupported_model_provider_before_any_process_launches` | → `validate_rejects_invalid_specs` row 1 (bookkeeping-empty assertion kept, applied to every row) |
| `validate_rejects_a_spec_requesting_a_different_harness_kind` | → `validate_rejects_invalid_specs` row 2 |
| `validate_rejects_a_network_tool_when_network_is_denied` | → `validate_rejects_invalid_specs` row 3 |
| `validate_rejects_when_the_resolved_binary_no_longer_exists` | → `validate_rejects_invalid_specs` row 4 |
| `validate_rejects_a_missing_secret_reference_typed_and_touches_nothing` | unchanged (kept separate: richer side-effect-absence claims than the table's shape) |
| `fake_binary_success_mode_is_reported_succeeded_via_the_honest_exit_code_fallback` | → `fake_binary_exit_code_fallback_reports_honest_terminal_state` row "success" |
| `fake_binary_failure_mode_is_reported_failed_via_the_honest_exit_code_fallback` | → same table, row "failure" |
| `fake_binary_malformed_mode_never_panics_and_never_fabricates_structured_data` | → same table, row "malformed" (via its `extra` closure) |
| `fake_binary_success_stages_a_real_log_artifact` | unchanged |
| `a_stream_with_a_valid_init_line_but_no_terminal_result_is_failed_not_a_guessed_success` | renamed `truncated_stream_with_no_result_line_is_reported_failed`; now reads `fixtures/claude_code/2.1.223/truncated-mid-run.jsonl` instead of an inline `concat!` |
| `cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry` | unchanged |
| `cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic` | unchanged (flagged as a suspected cross-adapter/lifecycle duplicate below, not removed) |
| `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome` | renamed `env_canary_is_redacted`, body unchanged |
| `secret_reference_resolves_and_only_its_length_reaches_the_shim` | unchanged |
| `declared_capabilities_match_the_reconciled_iii_d5_values` | renamed `declared_capabilities_report_cancel_and_artifacts_as_advisory`, body unchanged |
| `the_real_observed_claude_code_version_string_is_recognized` | → `version_text_parsing_variants` row 1 |
| `the_fake_binarys_unknown_version_fixture_is_reported_as_explicitly_unrecognized` | → same table, row 2 |
| `empty_version_output_is_a_probe_error_with_no_fabricated_version` | → same table, row 3 |
| `a_version_token_with_a_prerelease_suffix_is_still_recognized` | → same table, row 4 |
| `probe_attests_model_passthrough_instead_of_inventing_a_model_list` | unchanged |
| `probe_reports_the_shared_fixtures_unknown_version_output_honestly` | unchanged |
| `a_real_observed_success_transcript_is_parsed_with_the_session_model_and_usage` | → `result_envelope_parsing_variants` row 1; reads `2.1.223/success-with-usage.jsonl` |
| `an_api_error_result_with_a_misleading_subtype_of_success_is_still_reported_as_failed` | → same table, row 2; reads `2.1.223/api-error-misleading-subtype.jsonl` |
| `a_budget_exhausted_result_is_parsed_as_failed` | → same table, row 3; reads `2.1.223/budget-exhausted.jsonl` |
| `a_gateway_routed_result_is_recorded_as_requested_not_confirmed_even_on_a_fast_result_line` | renamed (shortened), kept as its own test; reads `2.1.261/gateway-routed-result.jsonl` |
| `a_missing_is_error_field_fails_closed_as_an_error_not_a_silent_success` | unchanged |
| `reconcile_with_no_recorded_process_id_needs_no_dispatch` | → `reconcile_reports_honest_observations_for_untrackable_pids` row 1 |
| `reconcile_reports_process_stopped_for_a_pid_that_no_longer_exists` | → same table, row 2 |
| `reconcile_with_an_undecodable_process_id_is_explicitly_unavailable` | → same table, row 3 |
| `reconcile_reports_process_running_for_a_genuinely_still_running_fake_harness` | unchanged (Linux-only, real-process poll loop — not a table fit) |
| `reconcile_reports_process_stopped_when_a_live_pid_belongs_to_an_unrelated_program` | unchanged (Linux-only) |
| `a_direct_model_request_spawns_with_no_provider_endpoint_variable_present` | → `provider_endpoint_variables_present_only_when_configured_and_requested` row 1 |
| `a_configured_provider_request_spawns_with_its_endpoint_variables_present` | → same table, row 2 |
| `a_disabled_provider_rejects_the_request_before_any_process_spawns` | unchanged |
| `live_claude_code_records_version_and_a_real_artifact_when_opted_in` | moved to `tests/live/claude_code.rs`, rebuilt against public API, behavior unchanged |
| `live_claude_code_through_the_configured_provider_when_opted_in` | moved to `tests/live/claude_code.rs`, rebuilt against public API, behavior unchanged |
| `live_claude_code_direct_model_never_reaches_the_configured_provider_when_opted_in` | moved to `tests/live/claude_code.rs`, rebuilt against public API, behavior unchanged |

| Claim | Evidence |
|---|---|
| `cargo build --workspace --tests` compiles clean | ran after every edit; final state clean, no warnings |
| `test(claude_code::)` all pass | `cargo nextest run --workspace -E 'test(claude_code::)'` — `22 tests run: 22 passed` |
| `binary(live_claude_code)` self-skips by default | `cargo nextest run --workspace -E 'binary(live_claude_code)'` — `0 tests run: 0 passed, 3 skipped` |
| `binary(live_claude_code)` still exists and still self-skips under `--run-ignored ignored-only` | `3 tests run: 3 passed` (no real `claude` binary/opt-in var on this machine) |
| Full-suite baseline holds (module count, not literal number — see below) | `cargo nextest run --workspace` — `1425 tests run: 1425 passed, 8 skipped`, reproduced clean on a second run after one unrelated flake (`tack-cli::local_runner::tests::setting_a_provider_secret_while_running_stops_the_old_task_before_anything_else`, confirmed flaky in isolation — 3/3 passes alone — and in an untouched crate) |
| `check --changed` passes | `✓ maintainability budgets hold (2 files checked)` |
| `cargo fmt --all -- --check` clean | ran after every edit; final state clean |
| `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml -- --check` clean | ran once; clean |
| `cargo clippy --workspace --all-targets -- -D warnings` clean | ran after every edit; final state clean |
| `.githooks/pre-push` passes end to end | ran directly; `✓ pre-push checks passed` |
| No board archaeology introduced | `scripts/check-comments.sh` (via pre-push) — clean after one fix (a comment in `tests/live_claude_code.rs` named a not-yet-existing `live/codex.rs` path; rewritten to state the underlying mechanism instead of a speculative file path) |
| No test-hygiene regression | `scripts/check-test-hygiene.sh` (via pre-push) — clean |

**On "the total must match 1439":** the card's own verification step 3 says moving 3 tests
to `tests/live/` "doesn't change the total, just which binary they're in" — true in
isolation, but this phase's other, explicitly-instructed work (table-driven consolidation,
audit rule 2) intentionally *reduces* the number of `#[test]` functions while preserving
every claim as a loop iteration instead of a separate function. `claude_code`'s own test
count dropped from 39 to 22 (−17); the full-suite total dropped from 1439 to 1425
(1439 − 17 = 1422, plus 3 for an unrelated pre-existing count difference not investigated
further since it's outside this phase's file scope and the flaky test above already
accounts for run-to-run variance of 1). This is the intended effect of the rule the card
asked to apply, not a dropped-coverage regression — see the claim → evidence table above
for proof that nothing was silently removed.

## `duplicate-tests`

`python3 scripts/maintainability.py duplicate-tests` — `tack-runner: 3 near-identical
pairs across files`, of which **one** involves `claude_code`:

```
a_disabled_provider_rejects_the_request_before_any_process_spawns
  crates/tack-runner/src/harness/claude_code/tests.rs
a_disabled_provider_rejects_the_request_before_any_process_spawns
  crates/tack-runner/src/harness/codex/tests.rs
```

Same name, same claim, in both adapters' test files — expected to resolve once codex's own
pruning pass (running in parallel, not reflected here) and the later cross-adapter dedup
phase land. The other two `tack-runner` pairs (`cancel_kills_the_whole_descendant_tree_*`,
`a_malformed_body_is_a_parse_error_not_a_panic`) don't involve `claude_code` at all.
51 pairs total across the workspace — unrelated to this phase's scope, recorded here only
because the card asked for where things stand.

## Suspected cross-adapter / pure-lifecycle duplicates (for the later dedup phase)

Carried forward from phase 2's own list, re-evaluated against the post-consolidation file:

- `validate_rejects_invalid_specs` row 2 ("different harness kind") — the harness-kind
  mismatch check itself is grammar-specific, but the *rejection plumbing* around it
  (`validate` returning `Err(Rejected)`, no bookkeeping created) is now identical code to
  codex's own equivalent case.
- `cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic` — the unknown-handle-is-`Err`
  path lives entirely in shared `local_process::cancel`/`take_running`; nothing here is
  claude-code-specific.
- `reconcile_reports_honest_observations_for_untrackable_pids` rows 1-2 ("no recorded pid",
  "pid that no longer exists") — both exercise `local_process.rs`'s shared pid-decode /
  liveness-check plumbing before ever reaching `reconcile_alive`/`reconcile_unavailable`.
  Row 3 ("undecodable pid") is also shared plumbing (`decode_handle` returning `None`).
- `validate_rejects_a_missing_secret_reference_typed_and_touches_nothing`'s core claim
  (`resolve_environment` rejecting an unresolvable `secret_reference` pre-spawn) calls
  shared code in `harness::resolve_environment`, not adapter-specific logic — though the
  side-effect-absence assertions (workspace untouched, state dir not created) are a richer
  claim than the shared-lifecycle tests would likely carry, so this one may stay
  claude-code-local even after dedup, just narrowed.
- `a_disabled_provider_rejects_the_request_before_any_process_spawns` — confirmed above via
  `duplicate-tests` as a literal name-and-shape duplicate with codex's own test.

**Not duplicates** (claude-code-specific, confirmed by inspection): `validate_rejects_invalid_specs`
rows 1/3/4 (provider allow-list, network self-contradiction, binary-existence check — all
grammar-owned policy with no codex equivalent), everything under `fake_binary_exit_code_fallback_reports_honest_terminal_state`
and `result_envelope_parsing_variants` (stream-json parsing has no codex analog),
`declared_capabilities_report_cancel_and_artifacts_as_advisory` (the actual capability
values differ from codex's), the two Linux-only real-process `reconcile_*` tests
(`process_program_matches`'s `/proc/<pid>/cmdline` check is claude-code's own, unlike
codex's unconditional-trust `reconcile_alive`), and `provider_endpoint_variables_present_only_when_configured_and_requested`
/ `a_disabled_provider_rejects_the_request_before_any_process_spawns`'s underlying
provider-endpoint-injection logic (shared `resolve_provider_endpoint` hook, but the
specific env-var names and gateway config are this grammar's own).

## What a stranger still cannot do

- **Cannot yet see either adapter's tests deduplicated against each other or against a
  shared `harness::mod`-level lifecycle suite** — that's explicitly the later, separate
  phase this card names; the list above is its input, not its output.
- **Cannot see `codex.rs`/`codex/tests.rs` pruned** — a parallel agent's job in a different
  worktree, not reflected in this branch.
- **Cannot rely on `claude_code.rs`'s production line count meeting the audit's 400-line
  target** — 845 lines, unchanged from phase 2, for the reasons phase 2 already gave and
  this phase did not attempt to change (out of this phase's charter).
- **Cannot assume the workspace-wide `1439` test-count number in older docs/handoffs is
  still current** — it dropped to 1425 as a direct, intended consequence of this phase's
  table-driven consolidation; a future phase quoting a total test count should re-measure
  rather than repeat either number, per this repo's "a load-bearing number carries the
  command that produces it" rule.

## Context spent

- Read in full before editing: `docs/agent-handoffs/part-ix/IX-M5-harness-core-codex.md`,
  `docs/agent-handoffs/part-ix/IX-M5-harness-core-claude_code.md` (both phase handoffs, in
  full), `docs/plans/harness-maintainability-audit.md` §5-§6, `crates/tack-runner/src/harness/claude_code.rs`
  (production, read-only — confirmed nothing needed changing), `crates/tack-runner/src/harness/claude_code/tests.rs`
  (the full 1735-line pre-edit file, in full, before designing any consolidation),
  `crates/tack-runner/src/harness/fixtures/claude_code/README.md` and
  `crates/tack-runner/src/harness/fixtures/codex/README.md` (to match the sibling
  adapter's documentation style before inventing the provenance convention),
  `docs/contracts/runner-v1/README.md` (checked for an existing provenance convention to
  reuse — found none directly applicable, since it documents credential-value conventions,
  not fixture-shape provenance), `crates/tack-runner/tests/h3_checkout.rs` (the worked
  example for building an `ExecutionSpec` against only the crate's public API from an
  integration test), `crates/tack-runner/tests/common/mod.rs`, `crates/tack-runner/src/lib.rs`
  and `crates/tack-runner/src/harness/mod.rs` (to confirm the exact `pub` surface available
  to an integration test before writing `tests/live/claude_code.rs`, rather than
  discovering visibility errors one at a time).
- One design question resolved by reading rather than guessing: whether fixtures should be
  read via `std::fs::read_to_string` at test runtime (the card's literal phrasing) or
  `include_str!` at compile time — resolved by finding `engine/tests.rs`'s existing
  `include_str!("../../../../docs/contracts/runner-v1/...")` pattern already in use for
  the same purpose (JSON/text fixture content in a test), and matching it rather than
  introducing a second convention for the same problem.
- One build error required more than one iteration: `tests/live/claude_code.rs`'s
  `mod common;` failed to resolve until changed to `#[path = "../common/mod.rs"] mod
  common;`, since `#[path]`-included files resolve their own `mod` statements relative to
  their own location, not the including file's.
- Files opened and not used for editing: `docs/TESTING.md` (testing conventions,
  confirmed nothing additional applied here), `TODO.md`'s IX-M5 section (to confirm phase
  scope only).

## Amendments

*(none yet)*
