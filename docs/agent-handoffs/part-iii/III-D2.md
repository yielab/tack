# III-D2 handoff

- **Base SHA / branch / final SHA:** base `67acd9ea8e7a517ba9e56958fed2f367cf4c55cc`
  ("feat(runner): add the Codex harness probe and adapter", D1's landed
  work, itself on top of D4's `ecb3437`) on `plan/harness-agnostic-agent-fleet`.
  Worked directly in the
  main checkout, no worktree, per instructions. **Not committed** — per
  instructions this handoff describes the uncommitted working tree; there is
  no final SHA.
- **Files changed (must equal ownership list):**
  - New: `crates/tack-runner/src/harness/claude_code.rs` (the adapter/probe
    and its full test suite — no separate fixture files needed; every
    fake-binary test drives the shared `fake_harness.sh` via
    `crate::harness::fixtures::fake_harness_command`, and the
    "Claude-Code-specific fixtures" this card owns are the hand-written,
    real-transcript-derived JSON strings inside the test module, not new
    files on disk).
  - New: this handoff.
  - Modified: `crates/tack-runner/src/harness/mod.rs` — exactly one line,
    `pub mod claude_code;`, inserted alphabetically between the existing
    `pub mod artifact;` and `pub mod codex;` lines (D1's `pub mod codex;` and
    D3's `pub mod opencode;` lines are both present from concurrent work; only
    my one line was added, nothing reordered). File was re-read immediately
    before this edit, as instructed.
  - `git status --porcelain` at the time of writing:
    `M crates/tack-runner/src/harness/mod.rs`,
    `?? crates/tack-runner/src/harness/claude_code.rs`,
    `?? crates/tack-runner/src/harness/opencode.rs` (D3's concurrent,
    not-yet-committed work — not touched by this card).
  - Not touched: `engine.rs`, `registry.rs`, `client.rs`, `journal.rs`,
    `workspace.rs`, `crates/tack-runner/src/harness/{codex,opencode,process,
    event_sink,redact,artifact,sha256,fixtures}.rs`, `docs/contracts/**`,
    `TODO.md`, `crates/tack-runner/Cargo.toml`, any other handoff.

## The one thing that makes this card different from D1/D3: `claude` is real

**`claude` version `2.1.223 (Claude Code)` is installed on this machine.**
Every behavioral claim below that is phrased as an *observation* was produced
by actually invoking that binary from a disposable fixture directory under
this session's scratchpad (never this repository, never a tracked file) and
reading its real output. Every claim phrased as an *assumption* was not.
Section-by-section:

### Observed (commands run, output read, during this card's implementation)

1. **Version string shape.** `claude --version` (also `-v`) prints
   `"2.1.223 (Claude Code)\n"` to stdout, exit 0, empty stderr, and needs
   neither `HOME` nor `PATH` set (`env -i "$BIN" --version` still works). Fast
   and side-effect-free — used as-is for `probe`/`detect_version`.
2. **The prompt is read from stdin, not argv.** `echo "prompt text" | claude
   -p --output-format json --tools ""` (no positional prompt argument)
   answered the piped prompt correctly. This adapter always sends the prompt
   via `ProcessSpec::stdin`, never as an argv element, both for the
   `process.rs`-documented `/proc/<pid>/cmdline`-exposure reason and because
   it is what the real CLI actually supports.
3. **`--output-format json` (non-streaming) has no reliable single "model
   used" field.** A trivial single-turn prompt's JSON result object had no
   top-level `model` key at all — only an aggregate `modelUsage` map, and
   that map contained **two** entries even though only one model was
   requested: the requested model, and an unrequested
   `claude-haiku-4-5-...` entry (an internal auxiliary call this session
   never asked for). `--output-format stream-json --verbose` **does** carry
   an authoritative field: the first `{"type":"system","subtype":"init"}`
   event's `model` key, corroborated by each `assistant` message's own
   `message.model`. This adapter always requests `stream-json --verbose` and
   reads `model` from the `system`/`init` event — a design decision directly
   forced by this observation, not a default guess.
4. **`is_error` (boolean) is the only reliable success/failure signal —
   `subtype` is not.** An invalid/decommissioned model name produced a real
   HTTP 404 from the API, surfaced as `"is_error":true` **and**
   `"subtype":"success"`, `"terminal_reason":"api_error"` in the same
   terminal `result` object, process exit code 1. A separate
   `--max-budget-usd` exhaustion run produced a *correctly* distinguishing
   `"subtype":"error_max_budget_usd"`. Since `subtype` is not consistently
   named around success/failure, `parse_run_output`/`parsed_from_result_line`
   in `claude_code.rs` key exclusively off `is_error`, and
   `an_api_error_result_with_a_misleading_subtype_of_success_is_still_reported_as_failed`
   encodes this exact byte-for-byte quirk as a regression test.
5. **CLI-level argument errors produce empty stdout, no JSON envelope at
   all.** `claude -p "..." --this-flag-does-not-exist` → exit 1, empty
   stdout, plain-text stderr (`error: unknown option ...`). This is why
   `parse_run_output` has a distinct "no JSON at all → fall back to the raw
   exit code" branch (`fallback_from_exit_code`), separate from "some JSON
   but never a terminal `result` object" (`malformed_outcome`).
6. **A persisted per-user settings file silently broke an otherwise-valid
   invocation.** This machine's `~/.claude/settings.json` carries
   `"effortLevel": "xhigh"`; a plain `claude -p ... --model haiku` (no
   `--effort` flag, even with the child's entire process environment
   cleared via `env -i`) failed with `API Error: 400 output_config.effort
   'xhigh' is not supported when thinking is disabled on this model`. This is
   not an environment-variable leak (the env was fully cleared) — it is a
   config *file* read directly by the CLI regardless of `env_clear()`. Fixed
   by always passing an explicit `--effort high` (verified compatible across
   every model this card exercised) and `--setting-sources ""` (reduces, does
   not fully eliminate, ambient per-machine configuration influence over a
   supposedly deterministic run) — both baked unconditionally into `start`'s
   argv construction, not left to chance.
7. **Claude Code's own Bash tool runs its command in a new session,
   distinct from the top-level process's own group.** Confirmed twice via
   `ps -o pid,ppid,pgid,sid,stat,cmd`: a real Bash-tool invocation's
   command-execution shell showed `STAT=SNs` (session leader) with a `PGID`/
   `SID` equal to its own pid, different from the top-level `claude`
   process's group — and independently, this very agent's own Bash-tool
   calls (used to run the probes for this card) show the identical pattern.
   A direct SIGTERM to the top-level process's own group *appeared* to result
   in full cleanup of that detached session too (nothing was left running
   afterward), consistent with Claude Code cleaning up its own spawned tool
   sessions as part of a graceful shutdown — the init event advertises
   `"capabilities":["interrupt_receipt_v1","interrupt_cancel_queued_v1", ...]`,
   which is exactly the kind of internal bookkeeping that would make this
   possible. This is the finding behind `feature_capabilities`'s
   `cancel: Advisory` (not `Supported`) — see "Falsifying fact for D5" below.
8. **Real tool execution genuinely spawns a real descendant process tree.**
   Asking Claude Code (via its Bash tool, `--permission-mode
   bypassPermissions`) to run `python3 -c "import time; time.sleep(25)"` in
   the foreground produced an observable three-level process tree (`claude`
   → a `zsh -c` command-execution shell → `python3`), confirming this
   adapter's reliance on `process.rs`'s process-*group* (not merely direct-
   child) cancellation is not hypothetical for this harness.
9. **Provider-switch environment variable names, confirmed by static
   inspection of the installed binary, not a live switch.** `strings` against
   the installed `claude.exe` (a self-contained, not-stripped ELF; `claude` on
   `PATH` is a plain symlink to it, no Node wrapper needed to invoke it
   directly) surfaced `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`,
   `CLAUDE_CODE_USE_FOUNDRY`, `ANTHROPIC_BEDROCK_BASE_URL`,
   `ANTHROPIC_VERTEX_BASE_URL`/`_PROJECT_ID`. This is real evidence the three
   non-default provider families exist in this binary, but it is **not** a
   live provider-switch test (that would need real Bedrock/Vertex/Foundry
   cloud credentials this card will not fabricate or request — explicitly
   out of scope per the dispatch brief). `validate`'s provider allowlist
   (`anthropic`/`bedrock`/`vertex`/`foundry`) rests on this static-inspection
   evidence, labeled as such in `claude_code.rs`'s own doc comment.
10. **No `list-models`-style command exists.** `claude --help`'s `Commands:`
    section is exhaustively `agents`, `auth`, `auto-mode`, `doctor`,
    `gateway`, `import`, `install`, `mcp`, `plugin`/`plugins`, `project`,
    `setup-token`, `ultrareview`, `update`/`upgrade` — nothing enumerates
    available models. This is why `probe` reports zero `model_combinations`
    rather than a static alias list (see "report capabilities without
    assuming models" below).

### Assumed / not independently verified

- **Exact behavior of the three alternate providers** (Bedrock/Vertex/
  Foundry) — accepted as real (per the static-inspection evidence above) but
  never exercised.
- **`--permission-mode manual` / decision-channel behavior under `--print`**
  — not force-tested to hang; the `decisions: Unsupported` capability rests
  on `--help`'s own documentation (the non-interactive trust dialog is
  described as "skipped entirely" in `--print` mode) plus the absence of any
  documented out-of-band decision flag, not on a live reproduction of an
  actual hang.
- **What happens if `SIGKILL` (not `SIGTERM`) is the first signal a hung
  top-level process receives** with a live Bash-tool descendant already
  running — reasoned about (SIGKILL is uncatchable, so Claude Code cannot run
  its own graceful-shutdown cleanup, and `kill(-pgid, SIGKILL)` cannot reach a
  different session's group) but **not independently forced and observed**,
  specifically to avoid leaving a real orphaned process behind as a side
  effect of testing this card. Labeled as reasoning from an observed
  structural fact (point 7 above), not itself a separate observation.

## The seam — confirmed, not redesigned

D4's `crate::harness::HarnessAdapter`/`HarnessProbe` (re-exporting the frozen
`engine::HarnessAdapter`, unchanged since Wave 2) were read in full before
writing this card, along with `process.rs`, `event_sink.rs`, `redact.rs`,
`artifact.rs`, `fixtures/fake_harness.sh`, and `docs/contracts/runner-v1/
{capabilities,limits}.json`. `ClaudeCodeAdapter<C: Clock = SystemClock>`
implements both traits by composing `harness::process::{ProcessSpec,
SupervisedProcess}` and `harness::redact::SecretMaterial` exactly as D4's own
module doc describes adapters should. Nothing about the trait, `AdapterRegistry`,
or `registry.rs` was touched or needed to be.

## Behavior implemented

- **`HarnessProbe::probe`** — spawns `<binary> --version` from a neutral
  (non-attempt) directory with a 10s bound, parses the result via
  `parse_version_text`/`looks_like_a_version_token` (first whitespace token
  must start with a digit and contain only alphanumerics/`.`/`-`), and always
  returns a `HarnessCapability`: `probe_error: None` with a real version on
  success, `Some(reason)` with the best-effort raw text preserved (never
  discarded) on an unrecognized shape, and `Some(reason)` with an empty
  `installed_version` if the binary could not even be spawned/captured.
  `model_combinations` is always empty, with `additional.model_discovery_note`
  explaining why (point 10 above) — "report capabilities without assuming
  models" taken literally: no unverified static alias list.
- **`validate`** — three concrete, evidence-based pre-spawn rejections, all
  `HarnessError::Rejected` before any process launches:
  1. `requested_harness_kind` must literally be `"claude-code"`.
  2. `requested_model_provider`, if present, must case-insensitively match
     one of the four families point 9 above found real evidence for; anything
     else (e.g. `"openai"`, matching `capabilities.json`'s own Codex example)
     is rejected — this harness categorically cannot honor it, independent of
     auth/network state.
  3. A self-contradictory policy — `permission_policy.network == false` while
     `permission_policy.tools` names `WebFetch`/`WebSearch` (case-
     insensitive) — is rejected, since the operator's own request cannot be
     honored consistently.
  Also defensively re-checks the resolved binary still exists on disk
  (`self.binary.program.exists()`).
- **`start`** — builds a real, evidence-driven argv: `-p --output-format
  stream-json --verbose --no-session-persistence --permission-mode
  bypassPermissions --effort high --setting-sources "" --tools
  <policy.tools.join(",")>` plus `--model <requested_model_id>` (opaque,
  never split) and `--max-budget-usd <request.budgets.cost_usd>` when
  present. The prompt (`resolved_agent_profile.instructions`) goes to stdin.
  `working_directory` honors `repository.subdirectory` when set (joined
  under the confined `workspace_root`). Environment: exactly `HOME`/`PATH`
  read from the *runner's own* process (not attempt-supplied data — a
  deliberate, narrow, documented exception to "never inherit ambient
  environment," needed because a real `claude` invocation cannot find its
  OAuth session or shell out without them) plus every resolved
  `environment[*].value` entry from the frozen request (registered into
  `SecretMaterial` and inserted verbatim); a `secret_reference`-only entry
  (no resolver exists in this crate yet) is skipped with a
  `tracing::warn!(name = ...)`, never silently treated as satisfied and never
  fabricated. Spawns via `ProcessSpec::spawn`, stores the `SupervisedProcess`
  in an internal `processes: Mutex<BTreeMap<String, RunningEntry>>` keyed by
  its own pid (as the opaque `LocalRunHandle::process_id`), alongside the
  `SecretMaterial`, `ProcessLimits` (stdout/stderr caps; timeout from
  `request.timeout_seconds`, clamped to `limits.json`'s
  `request_timeout_seconds_max = 86400`), requested provider, and start time.
- **`cancel`** — takes *ownership* of the stored entry (removes it from the
  map) and calls `SupervisedProcess::cancel(self.cancel_grace)` directly
  (D4's own method: SIGTERM to the group, grace wait, SIGKILL escalation,
  and — critically — a real `child.wait()` that actually *reaps* the
  process). See "Failure/adversarial case proved" for why this card's first
  version, which signalled by a raw pid without ever reaping, was wrong.
  Maps `CancelOutcome::{Stopped,Killed}` to `CancelObservation::ProcessStopped`
  (with which one recorded in `details.process_outcome`, non-secret); a
  signal-delivery `Err` maps to `Ambiguous`. An unknown/already-consumed
  handle is a typed `HarnessError::Process`, never a panic.
- **`wait`** — removes its own entry, calls `wait_with_capture` with the
  stored limits/secrets (already-redacted output by the time this adapter
  ever inspects it), and parses via `parse_run_output`: scans every stdout
  line, tolerating and skipping any line that fails to parse; extracts the
  `system`/`init` event's `model`/`claude_code_version` and the terminal
  `result` object. Three explicit outcomes, never a fourth silent one:
  a real `result` object → `parsed_from_result_line` (keyed on `is_error`,
  never `subtype`); some JSON but no `result` object →
  `malformed_outcome` (always `Failed`, sentinel model/usage fields); no JSON
  at all → `fallback_from_exit_code` (exit 0 → `Succeeded`, exit non-zero/
  signalled/timed-out → `Failed`, explicitly labeled in `terminal_reason` as
  an inferred, not structurally-observed, result). `terminal_state` is
  `Cancelled` when this adapter's own `cancelled` set shows `cancel` already
  ran for this handle (defensive — see below), else `Failed`/`Succeeded` from
  `is_error`. `ActualExecution.harness_version` comes from the *same run's*
  `claude_code_version` field (more authoritative than a separate `probe`
  call would be), `model_provider`/`model_id`/`model_observation_source` from
  the init event when found (`"harness_reported"`) or the explicit
  `"unknown"`/`"not_observed"` sentinel pair when not — never a guessed
  model name. `workspace_id`/`base_revision` are placeholders the engine
  overwrites via `HarnessOutcome::normalize_workspace_facts` (documented
  inline, matching `engine.rs`'s own contract).
- **`reconcile`** — `None` process id → `ProcessStopped` (no dispatch
  needed, consistent with `AdapterRegistry`'s own no-dispatch shortcut for
  the same case); unparseable pid string → `HarnessError::RecoveryUnavailable`;
  a pid `kill(pid, 0)` shows as dead → `ProcessStopped`; alive, and (Linux
  only) `/proc/<pid>/cmdline`'s `argv[0]` resolves to the same program this
  adapter would itself have spawned → `ProcessRunning`; alive but resolving
  to a *different* program → `ProcessStopped` (the original attempt process
  is confirmed gone; its pid was simply recycled); alive but identity
  unverifiable (non-Linux Unix, unreadable `/proc`, no Unix liveness
  primitive at all) → `Ambiguous`, honestly — "reconcile the journal only
  when reconciliation is genuinely supported" taken as a per-platform,
  per-outcome distinction, not a single blanket yes/no.

## Capability shape reported, and why each is justified

| Feature | Support | Reason (verbatim in code) |
|---|---|---|
| `cancel` | **Advisory** | Top-level process always reliably signalled; a Bash-tool-spawned descendant runs in its own session (observed) and is only guaranteed cleaned up on a graceful SIGTERM stop, not a SIGKILL escalation (reasoned from the observation, not itself forced) |
| `resume` | **Unsupported** | Headless is a single ephemeral process with no reattachment interface; `--resume` starts a *new* process from stored history — a different guarantee, not this attempt's in-flight state |
| `decisions` | **Unsupported** | No observed/documented out-of-band decision channel; `--print` mode resolves permission prompts locally, non-interactive trust dialog documented as skipped entirely |
| `artifacts` | **Supported** | Built-in `Write`/`Edit` tools produce real workspace files `ArtifactStager` can stage; no caveat found |
| `usage` | **Advisory** | Real token/cost data is reported, but the top-level `usage.input_tokens`/`output_tokens` fields appear (directly observed) to undercount relative to `total_cost_usd`, which folds in an internal auxiliary model's usage the top-level token fields do not reflect |

Every reason string above is the literal text in `feature_capabilities()`
(`claude_code.rs`), so it travels with the code, not only this handoff.

## Falsifying fact for D5

**`cancel` cannot be uniformly `Supported` for a harness whose own tool
execution detaches into a different OS session**, and this is not
Claude-Code-specific in principle — any harness whose "run a shell command"
tool does the equivalent (`setsid`-style detachment, common precisely because
interactive shells want to control their own job control) would have the
same gap against D4's process-*group* SIGTERM/SIGKILL escalation. D4's own
`spawn_child` fake-fixture mode is a weaker model of this than reality: it
deliberately keeps the grandchild in-group (so D4's own acceptance test could
prove group-kill reaches it cleanly), which is the *easy* case. This adapter
worked around the gap by degrading the reported capability to `Advisory`
rather than either (a) silently claiming `Supported` (a real user-facing
correctness gap for a stuck Bash-tool descendant after a hard kill) or (b)
inventing a new engine-level guarantee this card is not positioned to add
(engine.rs is frozen; rule 6). D5 is the only card positioned to decide
whether this is worth a shared-infrastructure change (e.g. `process.rs`
learning to walk `/proc` for the full process tree by pid rather than relying
solely on process-group signal delivery) once D1/D3's own findings are in
hand for comparison — this card does not know whether Codex/OpenCode's tool
execution has the same shape.

## Contract fixtures consumed

`docs/contracts/runner-v1/claim.response.json` (indirectly, via the same
`ExecutionRequestSnapshot`/`AttemptSnapshot` shape the D4 registry tests
already build from it — this card's own tests construct specs by hand rather
than re-parsing the fixture, since no claude-code-specific fixture value was
needed beyond what `spec_with` already covers), `capabilities.json` (as the
shape `HarnessCapability`/`FeatureCapabilities` must round-trip — reused
directly via `tack_orch::execution`'s existing types, never re-derived),
`limits.json` (`request_timeout_seconds_max` copied as `MAX_TIMEOUT_SECONDS`).
No contract file was edited.

## Tests added and exact commands/results

- `cargo test -p tack-runner --lib -- harness::claude_code::` — **30 tests**
  (29 always-run + 1 `#[ignore]`d opt-in live test), 0 failures. Repeated 8×
  at `--test-threads=8` with no flakes after the fixes described below.
- `cargo test -p tack-runner` — **179 lib (176 run + 3 `#[ignore]`d opt-in
  live tests: mine, D1's, D3's) + 2 CLI + 7 crash_matrix = 188 tests, 0
  failures** (this card's share: 30 of the 179 lib tests; the remaining lib
  growth over the dispatch brief's "94 lib" baseline is D1's already-landed,
  committed Codex tests plus D3's concurrent, uncommitted OpenCode tests).
- `cargo test --workspace` — 0 failures; run 6× total (5× as a dedicated
  full-workspace soak plus one final confirmation after this card's last
  edit) specifically for this card's own timing-sensitive tests (see below).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean (only `crates/tack-runner/src/harness/claude_code.rs`, this card's own new file, was ever formatted; no other file touched).
- `git diff --check` — clean (checked against the tracked `mod.rs` diff and
  the new `claude_code.rs` file via a scratch `git add -N` + `diff --check`,
  then unstaged again).

### Acceptance bullet → test mapping

| Acceptance bullet | Test(s) |
|---|---|
| Fake-binary success | `fake_binary_success_mode_is_reported_succeeded_via_the_honest_exit_code_fallback` |
| Fake-binary failure | `fake_binary_failure_mode_is_reported_failed_via_the_honest_exit_code_fallback` |
| Fake-binary cancel | `cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry` (real process, `kill(pid,0)` liveness proof it is actually gone, not merely reported so) |
| Fake-binary malformed output | `fake_binary_malformed_mode_never_panics_and_never_fabricates_structured_data` (real `malformed` fixture bytes, full spawn path) plus the pure-unit `a_stream_with_a_valid_init_line_but_no_terminal_result_is_failed_not_a_guessed_success` (a more realistic "truncated mid-run" malformed shape the generic fake binary cannot itself produce — see the note in `parse_run_output`'s doc comment on why the generic fixture's `success`/`malformed` modes are content-indistinguishable to this adapter's parser by design) |
| Fake-binary unknown version | `the_fake_binarys_unknown_version_fixture_is_reported_as_explicitly_unrecognized` (pure unit) and `probe_reports_the_shared_fixtures_unknown_version_output_honestly` (real spawn through the fixture's dedicated mode) |
| Unsupported selection fails pre-spawn | `validate_rejects_an_unsupported_model_provider_before_any_process_launches` (also asserts the process bookkeeping map stays empty), `validate_rejects_a_spec_requesting_a_different_harness_kind`, `validate_rejects_a_network_tool_when_network_is_denied`, `validate_rejects_when_the_resolved_binary_no_longer_exists` |
| Arguments/environment redacted (rule 12) | `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome` (real canary, `echo_canary` fixture mode, asserts absence **and** that `[REDACTED]` is actually present — proving the leak really happened and really got scrubbed, not merely that nothing leaked because nothing echoed) |
| Opt-in live test records version and artifact | `live_claude_code_records_version_and_a_real_artifact_when_opted_in` |

Additional real-transcript-derived regression tests (not required by the
acceptance table but directly encoding observed findings 3/4/6 above):
`a_real_observed_success_transcript_is_parsed_with_the_session_model_and_usage`,
`an_api_error_result_with_a_misleading_subtype_of_success_is_still_reported_as_failed`,
`a_budget_exhausted_result_is_parsed_as_failed`,
`a_missing_is_error_field_fails_closed_as_an_error_not_a_silent_success`.

## Failure/adversarial case proved

- **Zombie-reaping bug, caught by its own test, fixed before this handoff was
  written.** The first version of `cancel` signalled the target pid directly
  (its own small `kill(2)` FFI declaration, mirroring `process.rs`'s
  documented "not a new dependency" justification) and polled
  `harness::process::process_alive` (a raw `kill(pid, 0)` liveness check) to
  decide when the process had stopped. This is wrong: a `kill(pid, 0)`
  liveness check cannot distinguish "still running" from "exited but not yet
  reaped" — an unreaped child becomes a zombie that still answers
  `kill(pid, 0)` successfully, and nothing was ever calling `waitpid` on it
  (tokio's `Child` only reaps once its own future is driven).
  `cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry` failed
  immediately and reproducibly (`Ambiguous`, not `ProcessStopped`, even alone,
  not just under parallel load) against this — exactly what a load-bearing
  test should do. Fixed by having `cancel` take real ownership of the stored
  `SupervisedProcess` and call D4's own `SupervisedProcess::cancel`, which
  reaps correctly via a genuine `child.wait()`. Left as a documented lesson
  in `cancel`'s own doc comment, not silently corrected.
- **A `reconcile` process-identity test flaked under heavy parallel
  workspace-test load, traced to a genuine fixture-mode mismatch, not a
  logic bug.** The first version of
  `reconcile_reports_process_running_for_a_genuinely_still_running_fake_harness`
  used the fake binary's `hang` mode, which — per its own documented
  contract — `exec`s into `sleep`, replacing the process image; a real
  `claude` process never re-execs into something else over its lifetime, so
  matching `/proc/<pid>/cmdline` against the originally-spawned program is
  only representative when the fixture doesn't itself re-exec. Fixed by
  switching that test to `spawn_child` mode (whose own process, distinct from
  the grandchild it backgrounds, never execs) and adding a short bounded poll
  (rule 9: no fixed sleep) for the residual, genuinely transient case of
  `/proc/<pid>/cmdline` not yet being populated in the instant right after
  `spawn` under heavy system load. 5 full-workspace soak runs and 8 targeted
  `--test-threads=8` runs afterward show no further flakes.
- **Pre-spawn rejection is proved not to spawn anything**, not merely to
  return the right error: `validate_rejects_an_unsupported_model_provider_before_any_process_launches`
  asserts `adapter.processes` (the bookkeeping map only `start` ever inserts
  into) stays empty after a rejected `validate` call.
- **A missing `is_error` field fails closed**, not open:
  `a_missing_is_error_field_fails_closed_as_an_error_not_a_silent_success`
  proves a terminal `result` object lacking the field entirely (never
  observed in practice, but not contractually guaranteed either) is treated
  as an error, never a silent success.

## Schema/API/contract change requested from another owner

None. `docs/contracts/**` was read, never edited (A0/D5-owned). No change to
`engine.rs`, `registry.rs`, or any other shared file was needed — see
"Falsifying fact for D5" above for the one finding D5 should weigh, which is
a *behavior* finding (whether `Advisory` cancel support is acceptable, and
whether shared process-tree cancellation should eventually walk beyond a
single OS process group) rather than a request to change a type or field.

## Known limitations or `not_measured` fields

- **`secret_reference`-only environment entries are not resolved.** No
  secret-store client exists in this crate; such entries are skipped with a
  `tracing::warn!(name = ...)` rather than fabricated or silently dropped —
  `secret_reference_only_environment_entries_are_never_silently_treated_as_satisfied`
  proves the branch is never confused with a satisfied plain value. This
  mirrors D4's own documented event/artifact-transport gap: a future card
  wiring a secret resolver plugs directly into the existing `(None,
  Some(reference))` match arm.
- **Bedrock/Vertex/Foundry are validated as *acceptable requests*, never
  actually exercised.** `validate` will let a Bedrock/Vertex/Foundry-provider
  request through to `start`, but `start` does not set any of the
  provider-switch environment variables point 9 found evidence for — doing so
  would need real cloud credentials this card will not fabricate, and no
  request field in the frozen contract currently carries them either. A
  future card adding that wiring has a clear, evidence-backed list of exactly
  which env vars to set.
- **`usage.tokens_in`/`tokens_out` may undercount true consumption** relative
  to `total_cost_usd` for the reason in the capability table above — reported
  honestly (`Advisory`, not `Supported`) rather than silently accepted as
  exact.
- **`ActualExecution.model_provider`/`model_id` are `"anthropic"`/`"unknown"`
  sentinels, `model_observation_source: "not_observed"`, whenever no `system`/
  `init` event was ever parsed** (the exit-code-fallback and malformed-output
  paths) — never a guessed real-looking model name.
- **No live test exercises the Bedrock/Vertex/Foundry provider paths**, the
  `decisions`/`resume` unsupported findings under a forced hang, or a real
  SIGKILL-after-hang-with-a-live-Bash-descendant scenario, all for the
  reasons given in "Assumed / not independently verified" above.
- **Windows is out of scope structurally**, matching `process.rs`'s own
  documented non-Unix fallback: `reconcile` reports `Ambiguous` unconditionally
  there (no portable liveness primitive at all), never a false confident
  answer.

## Secrets/logging review

- Every `tracing::warn!` call in this file passes only ids, enum-shaped
  values, or booleans (`requested`, `requested_model_provider` — an opaque
  provider *name*, not a credential — `program` as a `Path::display()`,
  `name` for a skipped environment key, `?error` on a typed `ProcessError`/
  transport error). No call formats `args`, `env`, `stdin`, or the prompt
  directly.
- `ProcessSpec`'s own `Debug` redaction (inherited unchanged from
  `process.rs`) covers this adapter's constructed spec automatically; not
  re-implemented here.
- Every resolved plain environment value is registered into `SecretMaterial`
  before the process is spawned, so the same scrubbing D4 already proved
  against captured stdout/stderr also protects whatever ends up in this
  adapter's own `terminal_reason`/`HarnessOutcome` (parsed *from* that
  already-scrubbed text) — `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome`
  proves this end to end, including the positive control that the leak
  really happened and really got scrubbed.
- The prompt body itself is deliberately **not** registered into
  `SecretMaterial` — reasoned explicitly in `claude_code.rs`: a
  harness's legitimate answer can overlap with its own prompt (e.g. "print
  exactly: X"), so scrubbing prompt text from *output* would silently corrupt
  real results. Rule 12's "prompt bodies must never reach a log line" is
  instead satisfied structurally: no `tracing::*!` call in this file ever
  formats `resolved_agent_profile.instructions`.
- The live test never reads, logs, or forwards a credential — it uses
  whatever the installed `claude` binary is already configured with (e.g. an
  OAuth session under the *inherited* `HOME`), and passes `HOME` through only
  as an opaque environment value already handled by the redaction path above.

## Safe merge order and likely conflicts

- No file-level conflicts expected with D1 (`codex.rs`, already committed) or
  D3 (`opencode.rs`, concurrent, untouched by this card).
- `harness/mod.rs`: three `pub mod` lines now present (`codex`, `claude_code`,
  `opencode`) plus D4's original block, no other line touched by any of the
  three cards — this should merge/rebase cleanly regardless of order.
- Merge before D5: D5's "compare three observed contracts" is easiest with
  this card's clearly-separated observed/assumed sections in hand, especially
  the `cancel: Advisory` finding, which is the one behavioral fact this card
  believes is worth a cross-adapter comparison before D5 decides anything
  about `process.rs`.
- `registry.rs` was not read as part of this card's own research beyond what
  D4's handoff already quoted; not touched, exactly as scoped.

## Checklist

- No unowned files: confirmed via `git status --porcelain` above (only
  `harness/mod.rs` modified by exactly one line, `harness/claude_code.rs`
  new, plus D3's untouched concurrent `harness/opencode.rs`).
- No live secret: reviewed above; the live test carries no credential of its
  own.
- No panic stub: no `unimplemented!()`/`todo!()` anywhere in this file; every
  error path is a typed `Result` variant (`HarnessError::{Rejected,Process,
  RecoveryUnavailable}` at the trait boundary, `ProcessError`/`RunningEntry`
  lookups below it).
- No blind retry: `cancel` escalates SIGTERM → SIGKILL exactly once each
  (inside D4's own `SupervisedProcess::cancel`, not reimplemented); no code
  path in this file loops indefinitely on failure.
