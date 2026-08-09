# III-D1 handoff

- **Base SHA / branch / final SHA:** base `ecb3437e06cd3e377ae0c2acef9e53299fc47a06`
  ("feat(runner): add common harness process and event infrastructure", D4's
  landed work) on `plan/harness-agnostic-agent-fleet`. Worked directly in the
  main checkout, no worktree, per instructions. **Not committed** — this
  handoff describes the uncommitted working tree; there is no final SHA.
- **Files changed (must equal ownership list):**
  - New: `crates/tack-runner/src/harness/codex.rs` (the adapter/probe and its
    full test suite — no separate fixture files; see "Where the fixture repo
    lives" below for why).
  - New: this handoff.
  - Modified: `crates/tack-runner/src/harness/mod.rs` — exactly one line,
    `pub mod codex;`, inserted between the existing `pub mod artifact;` and
    `pub mod event_sink;` lines. No other line in that file touched.
  - `git status --porcelain` confirms exactly this:
    `M crates/tack-runner/src/harness/mod.rs` and
    `?? crates/tack-runner/src/harness/codex.rs`.
  - Not touched: `engine.rs`, `registry.rs`, `client.rs`, `journal.rs`,
    `workspace.rs`, any sibling `harness/{claude_code,opencode}.rs` (none
    existed at any point while this card ran), any other `harness/*.rs` file,
    `docs/contracts/**`, `TODO.md`, any other handoff.

## Critical environment fact this handoff exists to document

**`codex` is not installed on the machine this card was implemented on** (the
dispatch briefing said so, and `command -v codex` / the adapter's own PATH
search both confirm it: `codex` was not found on `PATH` at any point during
this card). Every fake-binary test in `codex.rs` drives
`crate::harness::fixtures::fake_harness_command` (D4's shared fixture),
**never** a real `codex` process. Every behavior specific to Codex's actual
CLI in this file is therefore a **documented assumption**, not a verified
fact. They are listed exhaustively below (also in `codex.rs`'s module doc
comment, so they travel with the code).

## Unverified assumptions about Codex's real CLI contract (exhaustive)

1. **Binary name and discovery.** The installed binary is assumed to be
   literally named `codex` and is resolved via a `PATH` search
   (`CodexLocator::Search`/`locate_in_dirs`), never a hardcoded path. Never
   verified to be the real binary name.
2. **Version detection.** Assumed to be `codex --version`, expecting a bare
   `X.Y[.Z]` numeric string on stdout with exit code 0
   (`CodexAdapter::detect_version`/`is_strict_version`). The real flag could
   be `-v`, `version` (subcommand), `--json`-wrapped, or something else
   entirely; the real output could carry a `v` prefix, a build hash, or a
   multi-line banner — none of this was checked against a real binary.
3. **Non-interactive execution shape.** Assumed to be
   `codex exec --json --model <requested model id>` with the agent profile's
   `instructions` piped over **stdin** (`CodexAdapter::start`). The
   subcommand name (`exec`), both flags (`--json`, `--model`), and the
   stdin-vs-argv choice for the prompt are all guesses. Real Codex CLIs (this
   one included, as far as this card could verify) commonly have a
   non-interactive/scriptable mode, but its exact shape was not observed.
4. **Terminal-state classification ignores stdout/stderr content entirely,**
   deliberately, because (3) is unverified: `classify_exit` uses only the
   process exit code (`0` → succeeded, nonzero/signalled → failed,
   timed-out-and-killed → failed). It does not attempt to parse a `--json`
   output stream even though `--json` is passed, because trusting an
   unverified output shape to override a directly-observed exit code would
   be inventing a contract, not reading one. Concretely: the shared fixture's
   `malformed` mode exits `0`, so this adapter reports `Succeeded` for it —
   see the `fake_binary_malformed_output_does_not_panic_and_still_produces_a_typed_result`
   test and its doc comment for the full reasoning.
5. **`ActualExecution.model_provider`/`model_id` are never independently
   observed.** Whether/how Codex reports which model it actually used is
   unverified, so this adapter echoes the **requested** model/provider back
   with a new `model_observation_source` value, `"requested_not_confirmed"`
   — not the fixture-exemplified `"harness_reported"`, which would claim
   something this card cannot prove. See "Frozen-contract observations for
   D5" below — this is also a candidate falsifying fact.
6. **Resume, decisions, and usage are all reported honestly as unverified,**
   not guessed: `CodexAdapter::feature_capabilities` reports `resume` and
   `usage` `unsupported`, `decisions` `unsupported` (compounded by C3's
   documented lack of a wired decision transport), and `artifacts`
   `advisory` (only a raw stdout/stderr log is staged, no structured
   artifact discovery). Every one carries a reason string saying exactly
   why. `duration_ms` in `Usage` is the one field genuinely `measured` (by
   this runner's own wall clock, not Codex); `tokens_in`/`tokens_out`/
   `cost_usd` are `not_measured`, never fabricated.
7. **Model discovery/`model_combinations` is always empty.** Codex's real
   model-list/discovery mechanism (if any) is unverified, so `probe()` never
   hardcodes a model list — "report capabilities without assuming models"
   taken literally. A future card with real Codex access should implement
   live discovery rather than trusting a guessed list.

None of these were treated as load-bearing for any test's pass/fail — every
test either exercises the fake binary (whose contract D4 froze and
documented) or, in the one opt-in live test, asserts only that probing
completed without hanging/panicking (see below).

## Frozen-contract observations for D5

- **`ActualExecution.model_provider`/`model_id` are non-nullable, but a real
  harness may not always be able to report them honestly.**
  `ExecutionRequestSnapshot.requested_model_provider`/`requested_model_id`
  are `Option<...>` — nullable "when auto-selection is allowed" (III.1.2).
  `ActualExecution.model_provider`/`model_id` are plain (non-`Option`)
  `ActualModelProvider`/`ActualModelId` (III.1.3 lists "actual ... provider
  and opaque model id" with no nullability note). This adapter has no
  verified way to observe which model an auto-selected Codex run actually
  used. Rather than fabricate a value into a non-nullable field (rule 7),
  `CodexAdapter::validate`/`start` reject a spec with no explicit
  `requested_model_provider`/`requested_model_id` **pre-spawn**
  (`check_selection`). This is a real, load-bearing design choice this card
  made unilaterally for *its own* adapter (permitted — rule 6 only forbids
  editing the shared trait/types), but it means Codex cannot currently serve
  an auto-select request at all. Whether the fix is (a) making
  `ActualExecution`'s model fields nullable, (b) adding a distinct
  `not_observed` `model_observation_source` value across all three adapters
  with fields staying non-null but semantically "unconfirmed", or (c)
  leaving each adapter to reject auto-select like this one does, is exactly
  the kind of one-time reconciliation decision D5 owns once D2/D3's real
  answers are in hand.
- **`model_observation_source` is an untyped `String`, not an enum**, and the
  only fixture-exemplified value is `"harness_reported"`. This card
  introduces a second value, `"requested_not_confirmed"`, without any
  frozen vocabulary to check it against. If D2/D3 also introduce their own
  ad hoc values for "we echoed the request, we didn't observe it", D5 should
  consider standardizing this into a small closed set (or a real enum) rather
  than three adapters inventing three different strings for the same
  situation.
- **`LocalRunHandle` cannot name its own harness kind** — D4 already flagged
  this and worked around it in `AdapterRegistry` via handle-string encoding.
  This card did not need to work around it a second time (its own
  `start`/`cancel`/`wait`/`reconcile` never see another adapter's handle),
  but confirms D4's finding still holds: nothing in this card's
  implementation would have been simpler with a `harness_kind` field on
  `LocalRunHandle`, so there is no new pressure to add it from this card's
  perspective.
- No other falsifying fact found. `engine::HarnessAdapter`'s five methods
  mapped cleanly onto this card's tasks; `HarnessProbe`/`AdapterRegistry`
  needed no changes.

## Where the "deterministic fixture repo" lives

The card's task list says "execute a deterministic fixture repo." This
card's ownership list allows "Codex-specific fixtures under a Codex-owned
path," but D2 and D3 were adding their own sibling files
(`harness/claude_code.rs`, `harness/opencode.rs`) to the same
`harness/`/`harness/fixtures/` directory concurrently, and D4's own
`harness/fixtures/` is explicitly D4-owned. Rather than claim a new
subdirectory under a directory two other cards were actively writing into
(risking exactly the kind of accidental collision the ownership rules exist
to prevent), `codex.rs`'s test module builds the fixture repo **at test
time**: `deterministic_fixture_repo()` creates a fresh temp directory with a
couple of fixed-content files (`README.md`, `main.rs`) per test, mirroring
D4's own `each_workspace_confined_process_only_ever_sees_its_own_canary_file`
pattern. It is fully deterministic (same fixed bytes every run, asserted
against directly, e.g. the live test's SHA-256 assertion against the exact
literal `# fixture repo\n` content) and workspace-confined (it *is* the
`ExecutionSpec.workspace.path` handed to `start`), without adding any new
on-disk path this card would need to defend as "Codex-owned" against two
concurrently-writing siblings. Flagged here in case D5 or a reviewer expected
a checked-in fixture directory instead.

## Behavior implemented

- **`CodexAdapter<C = SystemClock>`** implements the frozen
  `engine::HarnessAdapter` (`validate`/`start`/`cancel`/`wait`/`reconcile`)
  and D4's `HarnessProbe` (`harness_kind`/`probe`) for `harness_kind =
  "codex"`. Time is injected via `C: crate::Clock` throughout (no
  `SystemTime::now()` in the adapter itself), so every test asserts exact
  timestamps/durations without a real sleep (rule 9).
- **Version detection** (`detect_version`): resolves the binary, runs
  `<program> --version` bounded by `probe_timeout`, and classifies the
  result into exactly one of: a clean recognized version (whole-string
  `X.Y[.Z]`, digits and dots only), an unrecognized-but-present version
  string (raw text preserved in `HarnessCapability.additional.raw_version_output`,
  bounded to 200 chars), or an explicit `probe_error` (binary missing, spawn
  failure, nonzero exit, timeout, or empty output). Never a fabricated clean
  version.
- **`probe()`** never returns `Err` (per `HarnessProbe`'s contract);
  `model_combinations` is always empty (assumption 7). The most recent
  successful probe's version is cached (`last_probe`) and reused by `start()`
  to stamp `ActualExecution.harness_version`, falling back to a one-off
  `detect_version()` call only if `start()` is ever reached before any
  `probe()` call.
- **`validate`/`start`** share `check_selection`: reject (a) a spec whose
  `requested_harness_kind` isn't `"codex"`, (b) a spec with no explicit
  requested model (see the D5 note above), and (c) an unresolvable binary —
  all **before** any process is spawned. Proved empirically, not just by
  code inspection: `unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever`
  configures the fake binary to `hang` for an hour and asserts `start()`
  still returns `Err` within a 5s wrapper timeout — if the pre-spawn guard
  were bypassed, this test would hang instead of failing fast.
- **`start`** builds argv per assumption 3, registers the prompt and every
  literal (non-secret-reference) requested environment value with a
  `SecretMaterial` instance dedicated to that attempt, spawns via
  `ProcessSpec` (workspace-confined, as D4 built it), and tracks the live
  `SupervisedProcess` in an internal `tokio::sync::Mutex<BTreeMap<String,
  RunningCodexProcess>>` keyed by an opaque `codex:<pid>:<counter>` handle
  (never dropped before `cancel`/`wait` consumes it — `SupervisedProcess`
  wraps a `kill_on_drop(true)` child, so losing track of it would silently
  kill the process on drop). Per-attempt `ProcessLimits.timeout` is derived
  from `ExecutionRequestSnapshot.timeout_seconds` when set.
- **`secret_reference` environment entries are never resolved** — no
  secret-store client exists anywhere in `tack-runner` yet (confirmed by
  grep; this is the same class of gap D4 already flagged for event/artifact
  transport). Only literal `.value` entries are passed to the child.
- **`cancel`/`wait`** look up and remove the tracked process by handle
  (`Err(HarnessError::Process)` for an unrecognized handle — proved by
  `cancel_and_wait_on_an_untracked_handle_are_typed_rejections`); `cancel`
  delegates to `SupervisedProcess::cancel` (D4's process-group SIGTERM→SIGKILL)
  and always reports `CancelObservation::ProcessStopped` for either outcome
  (matching D4's own `Stopped | Killed` trust level), never fabricating
  `Ambiguous` itself — a `cancel()` that cannot even deliver the signal
  returns `Err(HarnessError::Process)` instead, letting the engine's own
  higher-level ambiguity handling take over exactly as it does for D4's fake
  adapter.
- **`wait`** captures/scrubs output via D4's `wait_with_capture`, classifies
  the terminal state from exit code alone (assumption 4), stages the
  captured (already-redacted) stdout+stderr as a `log` artifact via D4's
  `ArtifactStager` (best-effort — a staging failure only omits the
  `artifact` key from `terminal_reason`, never fails the attempt), and
  builds `Usage`/`ActualExecution` with the honest measured/not-measured
  split above.
- **`reconcile`** parses the pid back out of the handle format and, on Unix,
  answers `ProcessRunning`/`ProcessStopped` from a real `kill(pid, 0)`
  liveness check (`crate::harness::process::process_alive`); on non-Unix it
  returns `Err(HarnessError::RecoveryUnavailable)` explicitly — "reconcile
  the journal only when reconciliation is genuinely supported," and on
  non-Unix it genuinely is not (no portable liveness primitive, matching
  `harness/process.rs`'s own documented non-Unix limitation). A journal with
  no recorded `process_id` answers `ProcessStopped` without any dispatch (no
  process was ever confirmed running); an unrecognized handle encoding
  answers `Err(RecoveryUnavailable)`, never a guessed answer.

## Tests added and exact commands/results

- `cargo test -p tack-runner --lib -- codex::` — **21 passed, 1 ignored, 0
  failed** (the ignored test is the opt-in live test, by design).
- `cargo test -p tack-runner` — **115 lib + 2 CLI + 7 crash_matrix = 124
  tests, 0 failed, 1 ignored** (baseline from D4 was 94 lib + 2 CLI + 7
  crash_matrix = 103; this card added the 21 `harness::codex::tests`).
- `cargo test --workspace` — **978 passed, 0 failed** (baseline 957 + this
  card's 21).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean (required
  one fix: `CodexLocator::Fixed` is only ever constructed by test code, so
  it is `#[cfg(test)]`-gated, matching the same pattern
  `OwnerOnlyJournal::fail_next_update` already uses in `journal.rs` for a
  field that exists purely to make a test possible).
- `cargo fmt --all -- --check` — clean (ran `rustfmt` on `codex.rs` only,
  the one file this card owns).
- `git diff --check` — clean.
- `git status --porcelain` — `M crates/tack-runner/src/harness/mod.rs`
  (one line), `?? crates/tack-runner/src/harness/codex.rs` only, confirmed
  after every gate above.

## Acceptance gate — test to proof mapping

| Acceptance bullet | Test(s) |
|---|---|
| Fake success | `fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact` |
| Fake failure | `fake_binary_failure_completes_failed_with_the_exit_code_in_terminal_reason` |
| Fake cancel | `cancel_kills_the_whole_descendant_tree_via_the_adapter` (grandchild-killing, through the adapter's own `start`/`cancel`, not raw `ProcessSpec`) |
| Fake malformed output | `fake_binary_malformed_output_does_not_panic_and_still_produces_a_typed_result` (exec-level, proves robustness) plus `probe_reports_malformed_version_output_as_an_explicit_probe_error` (probe-level, proves an explicit `probe_error`) |
| Fake unknown version | `probe_reports_an_unrecognized_version_string_as_an_explicit_probe_error` |
| Unsupported selection fails pre-spawn | `validate_rejects_a_mismatched_harness_kind`, `validate_rejects_an_auto_selected_model_pre_spawn`, `validate_rejects_an_unresolvable_binary`, and — the empirical proof that this happens *before* any spawn — `unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever` |
| Arguments/environment redacted in logs and events | `secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact` (canary planted in a requested env value **and** the prompt/stdin, echoed back by the fake harness on stdout+stderr via `echo_canary` mode, asserted absent from both `terminal_reason` and the staged log artifact) |
| Opt-in live test records version and artifact | `live_probe_and_artifact_staging_against_a_real_codex_binary_when_present` — see "Live test scope" below |
| (task) Detect version | `probe_reports_a_recognized_version_with_no_error`, `probe_reports_a_nonzero_exit_as_an_explicit_probe_error`, `probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success`, `probe_never_hangs_past_its_own_timeout` |
| (task) Report capabilities without assuming models | `probe_reports_a_recognized_version_with_no_error` asserts `model_combinations.is_empty()`; enforced structurally (`probe()` never populates it) |
| (task) Reconcile only when genuinely supported | `reconcile_with_no_recorded_process_id_reports_stopped_without_dispatch`, `reconcile_rejects_an_unrecognized_handle_encoding_as_explicitly_unavailable`, `reconcile_observes_a_real_alive_process_as_running`, `reconcile_observes_a_dead_pid_as_stopped` (Unix-only, matching the `#[cfg(unix)]` code path) |
| (task) Report the actual selection | `fake_binary_success_completes_...` asserts `actual_execution.model_provider`/`model_id` echo the request and `model_observation_source == "requested_not_confirmed"` |
| Self-consistency (`HarnessProbe::harness_kind`) | `harness_kind_matches_what_probe_itself_reports` |
| Untracked-handle safety | `cancel_and_wait_on_an_untracked_handle_are_typed_rejections` |

## Live test scope (deliberately narrowed)

The opt-in live test does **not** attempt a real, non-interactive `codex
exec` run. This card could not verify whether that requires network access
and provider credentials, and rule 8 ("live harness tests are opt-in and
never require secrets in CI") makes guessing wrong on that point an
unacceptable risk. Instead the live test performs two things that are safe
without any credential:

1. Real version probing against whatever `codex` is actually on `PATH`.
2. Staging a real artifact (a fixed local file, not one produced by running
   a task) through the exact same `ArtifactStager` path `wait()` uses.

It is `#[ignore]`d (a plain `cargo test` never runs it) **and**
self-skips at runtime (`eprintln!` + early return, no panic) if `codex` is
not found on `PATH` — double protection against ever failing CI. Both
behaviors it exercises (version probing, artifact staging) are already
independently proven by the fake-binary tests above, so this test is never
the only proof of either.

## Failure/adversarial case proved

- Pre-spawn rejection under a would-hang-forever configuration (above) —
  proves ordering empirically, not just by code inspection.
- Untracked/stale handle on `cancel`/`wait` is a typed `Err`, never a silent
  success.
- An undecodable `reconcile` handle is `Err(RecoveryUnavailable)`, never a
  guessed `ProcessStopped`/`ProcessRunning`.
- Redaction survives a worst-case leaky-harness simulation (`echo_canary`
  mode) on **two** independent surfaces: the returned `terminal_reason` JSON
  and the staged log artifact file actually written to disk.
- `probe()` never hangs past its own configured timeout even when the
  underlying process would otherwise sleep for an hour
  (`probe_never_hangs_past_its_own_timeout`).

## Known limitations or `not_measured` fields

- `tokens_in`, `tokens_out`, `cost_usd` are always `not_measured` — Codex's
  real usage-reporting output format (if any) is unverified. Only
  `duration_ms` is `measured` (by this runner's own wall clock).
- `capability_snapshot.artifacts` is `advisory`, not `supported` — only a
  raw stdout/stderr log is staged; no codex-specific artifact discovery
  (e.g. a real git diff of the workspace) is implemented.
- `capability_snapshot.decisions`/`resume` are `unsupported` — Codex's real
  approval/session behavior is unverified, and the decision transport isn't
  wired anywhere in the runner yet regardless (pre-existing C3 gap).
- `secret_reference` environment entries are never resolved (no secret store
  client exists in `tack-runner`).
- Event-batch/artifact **transport** (wiring the staged artifact onto
  `docs/contracts/runner-v1/artifact.request.json` over the wire) is not
  attempted — same pre-existing C3/D4 gap; this card's artifact staging is
  the local half only.
- Non-Unix: `reconcile` always returns `RecoveryUnavailable` (no portable
  liveness primitive); `cancel` inherits D4's non-Unix direct-child-only
  fallback. Both documented, not silently narrowed. Untested on this card
  (dev machine is Linux), matching D4's own approach.
- The `#[cfg(test)]`-gated `CodexLocator::Fixed` variant means
  `CodexAdapter` cannot be pointed at an arbitrary fixed binary path from
  production code today — only `discover()`'s `PATH`-search constructor is
  public. If a future deployment needs to pin an explicit Codex binary path
  (bypassing `PATH`), that would need a small, real (non-test-only)
  constructor added — not attempted here since nothing in this card's scope
  needed it.

## Secrets/logging review

- `SecretMaterial` is instantiated per attempt in `start()`, seeded with the
  prompt (`resolved_agent_profile.instructions`) and every literal requested
  environment value, and used for **both** `wait_with_capture`'s
  scrub-before-retain step and the staged artifact (which is built from the
  already-scrubbed `CapturedOutput.text`, never raw process output).
- No `tracing::*!` call in `codex.rs` passes a raw prompt, environment
  value, or credential — every `tracing::warn!` logs only handle strings,
  reasons, kind names, or `Debug` of D4's already-redaction-aware error
  types (`ProcessError`).
- `RunningCodexProcess` deliberately does not derive `Debug` (it holds
  `SecretMaterial` and a live process handle) — there is no accidental
  `{:?}` path to a secret through this struct.
- Proved directly: `secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact`.

## Schema/API/contract change requested from another owner

None to `docs/contracts/**` (untouched, correctly — frozen, A0/D5 only). Two
observations flagged for D5 above ("Frozen-contract observations for D5"):
the `ActualExecution.model_provider`/`model_id` non-nullability tension with
an unobservable auto-selected model, and the lack of a closed vocabulary for
`model_observation_source`.

## Safe merge order and likely conflicts

- No conflicts expected with D2/D3: this card touches only new files plus
  one already-reviewed single-line insertion in `harness/mod.rs`, which each
  sibling is independently adding their own single line to (each of us was
  told to re-read the file immediately before editing and touch nothing but
  our own line — confirmed done here: `git diff` for that file shows exactly
  one added line, `pub mod codex;`, nothing reordered).
- Merge before D5: D5's "compare three observed contracts" and "register all
  three" tasks are easiest against a tree that already has this card's
  documented assumptions and the `model_observation_source`/nullable-model
  observations in hand, rather than rediscovering them from scratch once D2
  and D3 land their own (possibly different) answers to the same questions.
- If D2 or D3 independently reached a **different** conclusion on the
  auto-select/non-nullable-model tension (e.g. Claude Code or OpenCode *can*
  honestly report an observed model even without an explicit request), that
  is exactly the signal D5 needs to decide whether this is a Codex-specific
  limitation or a frozen-contract gap affecting all three.

## Checklist

- No unowned files: confirmed via `git status --porcelain` above and after
  every verification gate.
- No live secret: reviewed above; canary test passes on both the
  `terminal_reason` and staged-artifact surfaces.
- No panic stub: no `unimplemented!()`/`todo!()` anywhere in `codex.rs`;
  every error path is a typed `Result` (`HarnessError`'s three existing
  variants, per rule 6 — no new variant requested; `ProcessError`/
  `ArtifactError` stay entirely inside D4's own modules and are collapsed at
  the boundary exactly as D4's handoff already established).
- No blind retry: `cancel`/`wait`/`reconcile` each make exactly one attempt
  and return a typed result; no loop anywhere in this card's code retries a
  failed operation.
