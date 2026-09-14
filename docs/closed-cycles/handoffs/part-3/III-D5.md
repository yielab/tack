# III-D5 handoff

- **Base SHA / branch / final SHA:** base `83ec1748c42271ea59092257ad51bb8d3c0c7f3a`
  ("feat(runner): add the OpenCode harness probe and adapter", D3's committed
  work, itself descending from D2's `a961b2a`, D1's `67acd9e`, D4's `ecb3437`,
  and the accepted Wave 2 integration SHA `f931fc0`) on
  `plan/harness-agnostic-agent-fleet`. Worked directly in the main checkout,
  no worktree, per instructions. **Not committed** — per instructions this
  handoff describes the uncommitted working tree; there is no final SHA.
- **Files changed (must equal ownership list):**
  - Modified: `crates/tack-runner/src/engine.rs` (the `HarnessError` enum
    only — `Rejected` gained a `reason: String` field; `Process`/
    `RecoveryUnavailable` untouched; nothing else in the file touched).
  - Modified: `crates/tack-runner/src/harness/mod.rs` (the shared
    `HarnessProbe` trait, `AdapterRegistry`, a new `ModelObservationSource`
    enum, a new `PROCESS_GROUP_CANCEL_CEILING` constant/`HarnessRegistrationError`
    type, and new registration tests).
  - Modified: `crates/tack-runner/src/harness/codex.rs`,
    `crates/tack-runner/src/harness/claude_code.rs`,
    `crates/tack-runner/src/harness/opencode.rs` (all three adapters,
    updated together with the interface change, per the card's mandate).
  - New: this handoff.
  - `git status --porcelain` confirms exactly this: `M engine.rs`,
    `M harness/{mod,codex,claude_code,opencode}.rs`. No other file touched —
    no fixture under `docs/contracts/runner-v1/**`, no `crash_matrix.rs`, no
    `registry.rs`, no other handoff.
  - Not touched: `crates/tack-api/**`, `crates/tack-db/**`,
    `crates/tack-orch/src/**`, `crates/tack-runner/src/{workspace,journal,client}.rs`,
    `crates/tack-api/tests/wave2_gate.rs`, `TODO.md`, any handoff other than
    this one, root `Cargo.toml`/`Cargo.lock`.

## Neither integrator authorization was needed

- **Authorization 1 (frozen fixtures):** not invoked. Every change in this
  card is internal to `tack-runner`'s own types
  (`HarnessProbe::declared_capabilities`, the new
  `ModelObservationSource` enum, `HarnessError::Rejected`'s `reason` field).
  None of it touches `docs/contracts/runner-v1/**` or any
  `tack_orch::execution::*` wire type. `cargo test -p tack-orch --test
  runner_contract` passes unmodified (18/18, including the `paths.len() ==
  46` byte-pin) — confirmed below. `docs/agent-handoffs/part-iii/III-B4.md`
  was **not** edited, correctly, since there is nothing to record.
- **Authorization 2 (crash matrix):** not invoked. See finding 5 below —
  the encode/decode `LocalRunHandle` workaround D4 built is kept exactly as
  is; `LocalRunHandle` gained no field, so `crash_matrix.rs:277`'s struct
  literal still compiles untouched.
  `cargo test --test crash_matrix` passes unmodified (7/7).
  `docs/agent-handoffs/part-iii/III-C4.md` was **not** edited, correctly.

## The five findings, one by one

### Finding 1 — cancellation cannot be honestly `Supported`

**What was found (corroborated three times, not once):** D2 confirmed twice
with `ps` that Claude Code's Bash tool spawns its execution shell in a new
OS session (`pgid`/`sid` disjoint from the top-level process's own group),
so `harness::process::SupervisedProcess::cancel`'s process-group
SIGTERM/SIGKILL cannot reliably reach it. Mid-card, an adversarial reviewer
ran the equivalent check against this card's own real, installed `opencode`
binary and found the identical pattern for a bash-tool subprocess (disjoint
`pgid`/`sid` from the top-level `opencode run` process). Yet
`codex.rs`/`opencode.rs` both declared `cancel: Supported, reason: None` —
the one capability field in either file with no reason string, and
*unverified* even by their own authors (codex is not installed at all;
opencode's own module docs only tested a plain conversational run with no
live tool subprocess in flight at cancellation time — the easy case D4's own
fixture also tests).

**What changed:**
1. `codex.rs`/`opencode.rs`'s `feature_capabilities().cancel` downgraded
   `Supported` → `Advisory`, each with a reason string naming the observed
   disjoint-session mechanism. Claude Code's own `Advisory` (D2's correct,
   already-evidenced call) is unchanged.
2. `HarnessProbe` gained a new required method,
   `declared_capabilities(&self) -> FeatureCapabilities`. Each adapter's
   implementation is a one-line reuse of its own existing (private)
   `feature_capabilities()` — the same computation `wait()` already stamps
   onto `ActualExecution.capability_snapshot` post-hoc, now also reachable
   *before* any attempt exists. This is the structural gap the finding
   exposed: before this card, nothing in the pre-attempt path could ever
   consult a claimed capability, because the frozen, wire-facing
   `HarnessCapability` (`tack_orch::execution::capabilities`, out of this
   card's ownership) has no `FeatureCapabilities` field at all — only
   `HarnessAdapter::wait`, called *after* a process already ran, ever
   computed one.
3. `harness::mod.rs` gained `PROCESS_GROUP_CANCEL_CEILING: CapabilitySupport
   = Advisory` and `AdapterRegistry::register_probe` now calls
   `declared_capabilities()` at registration and returns
   `Result<&mut Self, HarnessRegistrationError>`, refusing to insert a probe
   whose `cancel` support exceeds the ceiling. This is genuinely a
   registration-time gate, not a runtime check: `capabilities()`/dispatch
   never see a rejected probe.

**Tests:**
- `harness::tests::registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists`
  — a fake probe declaring `Supported` is rejected by `register_probe`,
  never inserted (`capabilities()` stays empty). **This is the acceptance
  gate's "a lying capability is caught before invocation" proof.**
- `harness::tests::registering_all_three_real_adapters_is_order_independent`
  — the three *real* adapters' real probes all register cleanly (none still
  claims `Supported`), proving the fix is coherent end to end, not only
  against a synthetic fake.
- `harness::codex::tests::declared_cancel_capability_is_advisory_not_supported`,
  `harness::opencode::tests::declared_cancel_capability_is_advisory_not_supported`,
  `harness::claude_code::tests::declared_capabilities_match_the_reconciled_iii_d5_values`
  — direct, per-adapter regression pins on the corrected value itself, not
  only on the registration side effect.

### Finding 2 (raised mid-card by the adversarial reviewer, folded in as instructed) — `artifacts: Supported` had no backing implementation in Claude Code

**What was found:** `claude_code.rs`'s `feature_capabilities()` declared
`artifacts: Supported, reason: None`, but `wait()` never called
`ArtifactStager::stage_file` — not even the raw stdout/stderr fallback D1
and D3 both genuinely stage. The only `ArtifactStager` use in the file was
inside the opt-in live test, called directly by the test body, bypassing
the adapter entirely.

**What changed (made the claim true, then matched D1/D3's honest ceiling):**
- `RunningEntry` gained `workspace_path`/`attempt_id` fields (captured in
  `start()`, needed in `wait()`, which only ever sees the opaque
  `LocalRunHandle`).
- A new `ClaudeCodeAdapter::stage_run_log` (mirrors `codex.rs`'s /
  `opencode.rs`'s identical helper) stages the already-redacted combined
  stdout/stderr as a `log` artifact into `workspace.join(".artifacts")`
  (matching this adapter's own live test's existing choice, not an external
  staging root — `discover()` has no such root to give it). `wait()` now
  calls it and merges the result into `terminal_reason["artifact"]`.
  Best-effort: a staging failure only omits the key, never fails the
  attempt — identical contract to D1/D3.
- `feature_capabilities().artifacts` downgraded `Supported` → `Advisory`,
  with a reason naming exactly what is and is not staged — now honestly
  matching what D1/D3 already reported for the identical "raw log only, no
  per-file discovery" reality.

**Tests:**
- `harness::claude_code::tests::fake_binary_success_stages_a_real_log_artifact`
  — proves the fix through the adapter's own `wait()`, via the free
  fake-binary path (asserts `kind`/`media_type`/staged bytes/checksum),
  mirroring `codex.rs`'s/`opencode.rs`'s identical proof shape. The billed
  live test (`live_claude_code_records_version_and_a_real_artifact_when_opted_in`)
  is not the only proof of this — see "Live-test results" below for what it
  additionally confirmed for real.
- `harness::claude_code::tests::declared_capabilities_match_the_reconciled_iii_d5_values`
  pins `artifacts.support == Advisory` directly.

### Finding 3 — `model_observation_source` had no closed vocabulary

**What was found:** D1 introduced `"requested_not_confirmed"`; D3 reused
that exact literal, unprompted; D2 independently produced
`"harness_reported"` (the frozen fixture's own exemplar) and
`"not_observed"`. Three adapters already agreed on three distinct meanings
— genuine convergent evidence, not one author's opinion — but nothing
stopped a fourth adapter from inventing a fourth, incompatible string for
one of the same three situations.

**What changed — centralization, not a semantic redesign:** `harness/mod.rs`
gained `ModelObservationSource` (`HarnessReported` / `RequestedNotConfirmed`
/ `NotObserved`), a `const fn as_str()` returning exactly the three literals
already in use. Every adapter's own constant/inline literal now sources
from this enum instead of a private string literal — `codex.rs`'s and
`opencode.rs`'s `MODEL_OBSERVATION_SOURCE` const, and `claude_code.rs`'s two
call sites in `parsed_from_result_line`/`malformed_outcome`/
`fallback_from_exit_code`. **No adapter's behavior changed** — every value
each adapter reports in each situation is byte-identical to before this
card; only the source of the literal moved to one shared place. I
deliberately did **not** unify Claude Code's failure-path sentinel
("unknown"/`not_observed`, independent of any request value) with Codex/
OpenCode's convention (always echo the request as `requested_not_confirmed`,
since their own `check_selection` guarantees a request value always
exists by the time `wait()` runs) — that would be a real behavior change
with no falsifying fact backing it (no card observed evidence that Claude
Code's fallback *should* echo the request instead of a sentinel; D2's design
reasoning for the sentinel — an internal auxiliary model call the operator
never requested — is real, adapter-specific evidence I have no basis to
override). Left exactly as D2 built it.

**Tests:** the existing per-adapter tests (`assert_eq!(...,
"requested_not_confirmed")` / `"harness_reported"` / `"not_observed"`) are
unchanged and still pass — proving the refactor is behavior-preserving, not
merely that it compiles.

### Finding 4 — `HarnessAdapter::validate` couldn't carry a rejection reason

**What was found:** D1 and D3 both hit several genuinely distinct pre-spawn
rejection reasons (wrong harness kind, an unconfirmable auto-selected model,
an unresolvable binary, an unsupported provider, a provider/model pairing
opencode itself does not offer, a self-contradictory permission policy, a
capability-probe failure...) that all collapsed to the same bare
`HarnessError::Rejected` at the trait boundary. Both cards worked around it
with `tracing::warn!` immediately before returning — the reason reached a
log line, never the caller.

**What changed:** `engine.rs`'s `HarnessError::Rejected` gained a `reason:
String` field (`Rejected { reason: String }`). `Process`/
`RecoveryUnavailable` are untouched — neither D1/D2/D3 nor D4 reported an
analogous need for them, and rule 6 already made `HarnessError` a
deliberately small, closed enum (D4's handoff explicitly evaluated and
rejected widening it into a richer taxonomy). Every one of the ~30
construction sites across all three adapters plus `harness/mod.rs`'s own
`AdapterRegistry::resolve` now carries a real, specific reason string
(reusing the exact wording each site's own `tracing::warn!` already had, so
the operator-facing reason and the log line say the same thing); every
`matches!(_, Err(HarnessError::Rejected))` test pattern across all three
adapters became `Err(HarnessError::Rejected { .. })`. No test's *assertion
intent* changed — this is a mechanical widening, proven by every existing
D1/D2/D3 test still passing unmodified in substance.

**Tests:** every existing pre-spawn-rejection test in all three adapters
(≈20 tests) continues to pass, now matching on the struct-variant pattern;
none needed a new test, since the reason string itself was already the
`tracing::warn!` text these tests never inspected — the widening only makes
that text reachable through the `Result`, not newly true.

### Finding 5 — `AdapterRegistry` routing / `LocalRunHandle.harness_kind`

**Decision: left as D4 built it. No change.** D4 evaluated adding a
`harness_kind` field to `LocalRunHandle` and rejected it because
`crash_matrix.rs:277`'s struct-literal construction (C4-owned, off-limits)
would break. D1 independently confirmed after implementing its own adapter:
*"nothing in this card's implementation would have been simpler with a
`harness_kind` field on `LocalRunHandle`."* Neither D2 nor D3 reported the
workaround as costly either. Given the mandate to reconcile evidence, not
redesign, and given no adapter — real, working, in the tree — ever reported
the encode/decode workaround (`AdapterRegistry::encode_handle`/
`decode_handle`, hex-encoding the kind into the opaque `process_id`) as a
practical problem, there is no falsifying fact to act on here. The
workaround remains: `AdapterRegistry`'s `cancel`/`wait`/`reconcile` continue
to decode the kind prefix and dispatch correctly, proved (already, by D4,
unchanged by this card) by
`harness::tests::cancel_and_wait_route_the_start_generated_handle_back_to_its_own_adapter`
and `harness::tests::reconcile_decodes_the_kind_and_routes_to_the_right_adapter`.
This is a documented open question resolved by *not* acting on it, not a
guess frozen into the contract: if a future wave's adapter count or
dispatch shape ever makes the workaround genuinely costly, that will be new
evidence, and the coordinated `engine.rs` + `crash_matrix.rs` change D4
scoped out is still available to whoever owns that evidence then.

## `HarnessProbe` v1 shape (the only trait this card widened)

```rust
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    fn harness_kind(&self) -> tack_orch::execution::HarnessKind;
    async fn probe(&self) -> tack_orch::execution::HarnessCapability;
    // new in III-D5:
    fn declared_capabilities(&self) -> tack_orch::execution::FeatureCapabilities;
}
```

`engine::HarnessAdapter` (the frozen five-method per-attempt trait) is
**unchanged** — `validate`/`start`/`cancel`/`wait`/`reconcile` keep their
exact Wave-2 signatures. Only `HarnessError::Rejected` (a variant, not the
trait) gained its `reason` field. `AdapterRegistry` gained
`PROCESS_GROUP_CANCEL_CEILING`, `HarnessRegistrationError`, and
`register_probe`'s new `Result` return; `register_adapter` is unchanged
(non-fallible — no evidence demanded fallibility there). `harness/mod.rs`
also gained the `ModelObservationSource` enum (not trait-related — a shared
value vocabulary every adapter's own code reads from).

## Registration in `registry.rs`

Left untouched, deliberately. `registry.rs`'s `HarnessRegistry`/
`HarnessKind` (a separate typed enum from `tack_orch::execution::HarnessKind`)
is not called from anywhere in the runtime (confirmed by grep: only
`lib.rs`'s re-export and one doc-comment reference touch it) — it predates
D1–D4 and was never wired to `harness::AdapterRegistry`, the registry that
actually dispatches. D4 flagged unifying the two as "exactly the kind of
registry-shape decision D5 owns." I decided *not* to touch it: doing so
would be scope creep with no acceptance bullet requiring it (D5's actual
"register all three" mandate is satisfiable, and is satisfied, entirely
within `harness/mod.rs`, where the real dispatcher already lives), and
`registry.rs` is currently inert dead code with its own passing test — safer
left alone than "fixed" without a falsifying fact demanding it. Both new
acceptance-proof tests
(`the_same_fixture_completes_through_all_three_real_adapters`,
`registering_all_three_real_adapters_is_order_independent`, both in
`harness::mod::tests`) construct and register the three *real* adapters
directly through `harness::AdapterRegistry`.

## Acceptance gate — test to proof mapping

| Acceptance bullet | Test(s) |
|---|---|
| No adapter contains a panic or TODO stub | Structural: `cargo clippy --workspace --all-targets -- -D warnings` clean; every error path in all three adapters and `harness/mod.rs` is a typed `Result`, confirmed by re-reading every changed line in this diff |
| Same fixture completes through all three fake adapters | `harness::tests::the_same_fixture_completes_through_all_three_real_adapters` — one deterministic branching `/bin/sh` script (never D4's env-var-driven `fake_harness_command`, which cannot honestly answer OpenCode's three distinct probe/model-list/run purposes from one fixed mode), the three real `CodexAdapter`/`ClaudeCodeAdapter`/`OpenCodeAdapter`, `validate` → `start` → `wait` on each, all three reach `AttemptState::Succeeded` |
| A lying capability is caught before invocation | `harness::tests::registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists` (synthetic proof: rejected, never inserted) + `harness::tests::registering_all_three_real_adapters_is_order_independent` (the three real, now-fixed probes register cleanly) + the three per-adapter `declared_...capabilit...` pins |
| Registration of all three is order-independent | `harness::tests::registering_all_three_real_adapters_is_order_independent` — two `AdapterRegistry` instances, adapters registered in opposite orders, `registered_kinds()` identical and dispatch behavior identical either way |
| Two opt-in live adapters runnable and passing before Wave 4 | See "Live-test results" below: OpenCode and Claude Code both run for real and pass |
| Codex live test stays opt-in and cleanly skips | `harness::codex::tests::live_probe_and_artifact_staging_against_a_real_codex_binary_when_present` — confirmed below, prints `skipping live codex test: codex not found on PATH`, exits `ok`, never claimed as a real pass |

## Fixture changes

**None.** `docs/contracts/runner-v1/**` is byte-for-byte untouched.
`cargo test -p tack-orch --test runner_contract` — **18 passed, 0 failed**,
including `fixtures::every_json_fixture_parses_and_value_round_trips_without_loss`
(the `paths.len() == 46` pin). Authorization 1 was evaluated and found
unnecessary — see "Neither integrator authorization was needed" above.

## Tests added and exact commands/results

- `cargo test -p tack-runner --lib` — **183 passed, 0 failed, 3 ignored**
  (the three opt-in live tests). Baseline from D4 (94) + D1 (21) + D2 (30,
  net of concurrent-edit churn) + D3 (32) = 176 (per D3's own handoff,
  "176 lib" was already the state at D3's commit `83ec174`); this card added
  7: `registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists`,
  `the_same_fixture_completes_through_all_three_real_adapters`,
  `registering_all_three_real_adapters_is_order_independent`,
  `harness::codex::tests::declared_cancel_capability_is_advisory_not_supported`,
  `harness::opencode::tests::declared_cancel_capability_is_advisory_not_supported`,
  `harness::claude_code::tests::declared_capabilities_match_the_reconciled_iii_d5_values`,
  `harness::claude_code::tests::fake_binary_success_stages_a_real_log_artifact`.
  176 + 7 = 183. Confirmed stable across 3 repeated
  `--test-threads=8 harness::tests::` runs (the two new real-subprocess
  cross-adapter tests are the only timing-sensitive additions).
- `cargo test -p tack-runner` — 183 lib + 2 CLI + 7 crash_matrix = 192, 0
  failures.
- `cargo test --workspace` — **1046 passed, 0 failed** (baseline 1039 + this
  card's 7).
- `cargo test -p tack-orch --test runner_contract` — **18 passed, 0
  failed** (B4's pin harness, untouched, confirming no fixture drift).
- `cargo test -p tack-api --test wave2_gate` — **5 passed, 0 failed**
  (Wave 2's gate, untouched, still green).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt -p tack-runner -- --check` — clean (ran `rustfmt` on only the
  five files this card owns: `engine.rs`, `harness/{mod,codex,claude_code,opencode}.rs`).
- `git diff --check` — clean.
- `git status --porcelain` — exactly the five files above modified, nothing
  else, confirmed after every gate.

## Live-test results (the two installed harnesses)

- **OpenCode — real, free, run repeatedly during development:**
  ```
  live opencode probe: installed_version="1.18.0" probe_error=None model_combinations=1
  live opencode run: terminal_state=Succeeded tokens_in=Some(5965) tokens_out=Some(3) cost_usd=Some(0.0) harness_version=1.18.0
  test harness::opencode::tests::live_probe_and_a_real_free_run_record_version_and_artifact ... ok
  ```
  Command: `cargo test -p tack-runner --lib -- --ignored opencode::tests::live_`.
  Passes; no code in this card's diff touches this adapter's live-test
  logic (only its `feature_capabilities`/`MODEL_OBSERVATION_SOURCE`/
  `HarnessError::Rejected` construction — none of which this test exercises
  or asserts on).
- **Claude Code — real, billed, run exactly once as instructed:**
  ```
  live claude-code probe: version=2.1.223
  live claude-code outcome: terminal_state=Failed model_id=claude-opus-5[1m] harness_version=2.1.223
  live claude-code staged artifact content: "tack-d2-live-test-marker\n"
  live claude-code artifact: sha256=e9c7da4af433eee4bf545b1d095a12a9d4852557657b41a72542f80c619f7a75 size_bytes=25
  test harness::claude_code::tests::live_claude_code_records_version_and_a_real_artifact_when_opted_in ... ok
  ```
  Command: `TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo test -p tack-runner --lib
  -- --ignored claude_code::tests::live_`. **Passes** — the test only
  asserts version probing and artifact staging succeeded, per D2's own
  deliberate design (never a specific `terminal_state`), and both did: the
  `Write` tool genuinely produced the exact requested marker text, staged
  and checksummed correctly. Reported honestly rather than silently
  ignored: `outcome.terminal_state` was `Failed`, not `Succeeded`, on this
  one billed run, despite the Write tool visibly succeeding — this is not
  something this card's changes could have caused (`terminal_state` is
  computed by `parse_run_output` entirely before this card's new artifact-
  staging code ever runs, and this card touched no parsing logic in this
  file), and is consistent with D2's own documented observation that an
  internal auxiliary call can affect the top-level result independent of
  the primary visible turn. Not investigated further — reproducing or
  debugging a live Claude Code API interaction is outside this card's scope
  and would cost a second billed run for a question D2's own capability
  table (`usage: Advisory`, not `Supported`) already flags as a known
  imprecision in this exact area.
- **Codex — confirmed not installed, live test cleanly skips:**
  ```
  skipping live codex test: `codex` not found on PATH
  test harness::codex::tests::live_probe_and_artifact_staging_against_a_real_codex_binary_when_present ... ok
  ```
  Never claimed as a real pass — `command -v codex` returns nothing on this
  machine, matching D1's own documented environment.

## Known limitations / consciously left open for Wave 4 or release

- **Finding 2's fix is scoped to the raw-log artifact, not real per-file
  discovery.** All three adapters now honestly report `artifacts: Advisory`
  for the identical reason: only a combined stdout/stderr log is staged; no
  adapter stages a real git diff or the individual files a harness's own
  Write/Edit tools touched. A future card wiring genuine per-file artifact
  discovery (all three CLIs run inside a real git-initialized workspace
  already, per each adapter's own live test) would have a clear, uniform
  starting point across all three files now that the shape (stage into
  `.artifacts`/an external staging root, `kind: "log"`, best-effort) is
  consistent.
- **The auto-select / non-nullable-model tension (D1/D3's original finding
  2 in the dispatch brief) is resolved without a contract change, not left
  open.** Claude Code proves `ActualExecution.model_provider`/`model_id`
  staying non-nullable is achievable — it independently confirms a real
  auto-selected model via the `system`/`init` stream event. Codex/OpenCode's
  inability to do the same is a capability gap specific to those two
  harnesses (no observed mechanism, in Codex's case because it is not
  installed at all), not a defect in the frozen `ActualExecution` shape. No
  change was made to `tack_orch::execution` (out of ownership regardless),
  and none was warranted by the evidence.
- **`opencode export <sessionID>`** (D3's own finding: authoritative
  post-hoc model confirmation) remains unwired, exactly as D3 left it —
  wiring it is real, additional scope (a second subprocess call inside
  every successful `wait()`) this card's mandate (reconcile the interface,
  not redesign adapter behavior) does not cover.
- **The two lower-severity adversarial-review items not folded into a fix:**
  (3) no adapter test captures `tracing` output to prove no leak, unlike
  C2's dedicated regression test — the reviewer manually audited every
  `tracing::*!` call site and found no leak, so the property holds today; a
  new test harness for this risks the exact `tracing::subscriber::set_default`
  thread-local flakiness Wave 2's carry-forward already documented
  (`logs_never_contain_raw_credentials_only_ids`), and adding a second
  instance of that flakiness class for a coverage gap with no known failure
  was judged not worth the risk inside this reconciliation card. Left as a
  documented gap for whoever next touches adapter logging. (4) D1's/D3's
  hang-guard tests now also assert their bookkeeping map stayed empty
  (matching D2's own pattern) — folded in, since it was a one-line addition
  to a file this card was already editing (see `codex.rs`'s and
  `opencode.rs`'s `unsupported_selection_fails_pre_spawn_...` tests).

## Secrets/logging review

- No new `tracing::*!` call site in any of the five changed files formats a
  raw prompt, environment value, or credential — every new/changed call
  passes only `reason` strings this card itself constructed from typed,
  non-secret data (harness kinds, provider names, boolean flags), matching
  every existing call site's own established pattern.
- `HarnessError::Rejected`'s new `reason: String` field is built exclusively
  from operator-facing, non-secret text (the same text each site's
  `tracing::warn!` already logged) — never from `request.environment`,
  `resolved_agent_profile.instructions`, or any `SecretMaterial`-registered
  value.
- The new cross-adapter fixture script (`harness::tests::cross_adapter_fixture_command`)
  never touches a real credential — it is a static, temp-file `/bin/sh`
  script with no environment dependency at all.
- Claude Code's new `stage_run_log` stages from `result.stdout.text`/
  `result.stderr.text` — already scrubbed by `SecretMaterial` inside
  `wait_with_capture` before this card's code ever sees them, identical to
  the existing, already-redaction-proven `codex.rs`/`opencode.rs` pattern.
  The existing `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome`
  test (D2's own, unmodified) continues to pass, confirming the new staging
  path does not reintroduce a leak.

## Safe merge order and likely conflicts

- This is the last Wave 3 card; D1–D4 are already committed at `83ec174`.
  No concurrent-edit conflicts expected.
- Wave 4 (E-cards) can build directly on `AdapterRegistry` (in
  `harness/mod.rs`) as the one real, working, order-independent
  multi-harness dispatcher — `registry.rs`'s separate, unwired
  `HarnessRegistry` should not be mistaken for it.

## Checklist

- No unowned files: confirmed via `git status --porcelain` above and after
  every verification gate — exactly `engine.rs` +
  `harness/{mod,codex,claude_code,opencode}.rs`.
- No live secret: reviewed above; canary tests (D1/D2/D3's own, unmodified)
  all still pass; the live Claude Code test never logged a credential.
- No panic stub: no new `unimplemented!()`/`todo!()`/`panic!()` in
  non-test code anywhere in the diff; every new error path is a typed
  `Result` (`HarnessError::Rejected { reason }` or the new
  `HarnessRegistrationError`).
- No blind retry: no new retry loop anywhere in this card's changes;
  `register_probe`'s rejection is a single check, not a retried operation.
