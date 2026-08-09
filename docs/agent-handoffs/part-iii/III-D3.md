# III-D3 handoff

- **Base SHA / branch / final SHA:** base `67acd9e` (`feat(runner): add the Codex
  harness probe and adapter`, D1) on `plan/harness-agnostic-agent-fleet`, itself a
  descendant of D4's `ecb3437` and the accepted Wave 2 integration SHA `f931fc0`.
  Worked directly in the main checkout, no worktree, per instructions. **Not
  committed** — this handoff describes the uncommitted working tree; there is no
  final SHA. Two sibling agents (D1 Codex, D2 Claude Code) were writing their own
  adapters in this same checkout throughout; see "Safe merge order" below for what
  was observed of their concurrent state.
- **Files changed (must equal ownership list):**
  - New: `crates/tack-runner/src/harness/opencode.rs` (~1,650 lines incl. tests).
  - New: this handoff.
  - Modified: `crates/tack-runner/src/harness/mod.rs` — **exactly one line**,
    `pub mod opencode;`, inserted alphabetically. At the time of my edit the file
    already carried D1's `pub mod codex;`; D2's `pub mod claude_code;` landed
    concurrently in the same working tree (not authored by me — see below).
  - `git status --porcelain` at the end of this card: `M
    crates/tack-runner/src/harness/mod.rs`, `?? crates/tack-runner/src/harness/opencode.rs`,
    plus D2's own uncommitted `?? crates/tack-runner/src/harness/claude_code.rs`
    (D1's `codex.rs` was committed at `67acd9e` before I started). No other file
    touched.
  - No fixture files added anywhere. "OpenCode-specific fixtures under a
    card-owned path" (my ownership grant) turned out not to need any: every test
    either drives D4's shared `fake_harness_command()`, a pure Rust function with
    literal string fixtures, or a small inline `/bin/sh -c '...'` string
    constructed at test time (never written to disk as a tracked file). This
    mirrors D1's own explicit choice, made for the identical reason: avoiding any
    new path under `harness/` while D1/D2/D3 write concurrent sibling files in the
    same directory.

## Ownership discipline

- Did not touch `crates/tack-runner/src/harness/{codex,claude_code}.rs`,
  `engine.rs`, `workspace.rs`, `journal.rs`, `client.rs`, `registry.rs`, or any
  other file in `harness/` besides my own new file and the one-line `mod.rs`
  insertion.
- `crates/tack-db/**`, `crates/tack-orch/**`, `crates/tack-api/**`,
  `docs/contracts/**`, `TODO.md`, other handoffs, root `Cargo.toml`/`Cargo.lock`:
  untouched.
- Encountered one transient concurrent-write side effect: `cargo test --workspace`
  intermittently failed on `harness::claude_code::tests::*` assertions (different
  expected/actual values and even different source line numbers across
  consecutive runs — e.g. `left: Ambiguous / right: ProcessStopped` at line 1532,
  then `left: Ambiguous / right: ProcessRunning` at line 1808, then `left: None /
  right: Some(ProcessRunning)` at line 1826). This is D2 actively iterating on
  `claude_code.rs` in real time in the shared checkout, not a defect connected to
  this card. Per instructions I did not touch that file; I waited ~20s and
  re-ran — the workspace suite came back fully green. See "Tests added and exact
  commands/results" for the final, stable numbers.

## Contract fixtures consumed

- `docs/contracts/runner-v1/claim.response.json` (via `spec_with`'s
  `ExecutionRequestSnapshot`/`AttemptSnapshot` construction, same pattern
  `harness::tests` and D1's `codex.rs` already use).
- `tack_orch::execution::{HarnessCapability, ModelCombination, ActualExecution,
  Usage, FeatureCapabilities, CapabilityValue, CapabilitySupport, Measurement,
  MeasurementSource}` — all frozen B1 types, none modified.
- D4's `crate::harness::{process, redact, artifact, fixtures}` — composed
  unchanged: `ProcessSpec`/`SupervisedProcess` for every subprocess this adapter
  spawns (the real `run`/`--version`/`models` invocations and every fake-binary
  test alike), `SecretMaterial` for redaction, `ArtifactStager` for the staged
  run log, `fake_harness_command()` for the shared fixture.

## Behavior implemented

`OpenCodeAdapter<C>` implements the frozen `crate::client::engine::HarnessAdapter`
(`validate`/`start`/`cancel`/`wait`/`reconcile`) and D4's `HarnessProbe`
(`harness_kind`/`probe`) for `harness_kind = "opencode"`.

- **Version detection** (`detect_version`): runs `opencode --version`, requires
  the *entire* trimmed stdout to match a strict `X.Y.Z` numeric pattern (observed
  fact: the real binary prints exactly `"1.18.0\n"`, nothing else). Any deviation
  (missing, non-zero exit, timeout, or unrecognizable text) becomes an explicit
  `probe_error`, never a fabricated version.
- **Capability reporting without assuming models** (`probe`/`list_model_combinations`):
  runs `opencode models`, parses one `<providerID>/<modelID>` line per model,
  splits only on the *first* `/` (provider is a known namespace field; everything
  after stays a fully opaque model id, even if it itself contains further `/`
  characters), and groups into `Vec<ModelCombination>` — one entry per provider,
  each carrying only the models genuinely offered under that provider. A line
  that doesn't match `provider/model` invalidates the whole batch rather than
  being silently dropped.
- **Validating the requested spec** (`check_selection` + `check_pairing_supported`):
  a request must name both a provider and a model, or neither (opencode's
  `--model` flag has no way to express "just a provider" or "just a model"); a
  named pair is checked against a *fresh* `probe()`'s real `model_combinations` —
  **the exact pair**, never "is this model known to any provider." Auto-selection
  (`None, None`) is rejected pre-spawn, mirroring D1's `codex.rs` (see "Observed
  vs. assumed" below for why).
- **Executing a deterministic fixture repo** (`start`): spawns
  `opencode run --format json --model <provider>/<model>`, prompt delivered over
  **stdin** (confirmed to work; never on argv, so it never appears in
  `ps`/`/proc/<pid>/cmdline`), `working_directory`/`workspace_root` set to the
  attempt's own workspace path (D4's `ProcessSpec` confinement applies
  unchanged), `PATH`/`HOME`/`XDG_*` forwarded from the runner's own environment
  (required — opencode needs `PATH` for its own tool subprocesses and
  `HOME`/`XDG_*` for its config/credential/session store; `ProcessSpec` always
  clears the child's environment otherwise).
- **Normalizing output and result** (`wait`): `terminal_state` is derived solely
  from process exit code (`0` → `Succeeded`, else → `Failed`, timeout → `Failed`)
  — trusted because the one real error case this card tested agreed with it (see
  below). Stdout is best-effort parsed as newline-delimited JSON; when it parses,
  the last `step_finish` event's real `tokens.{input,output}`/`cost` populate
  `Usage` as genuinely `Measured` (not estimated), and any `{"type":"error"}`
  event is embedded in `terminal_reason` for diagnosis. When it doesn't parse
  (including the shared fixture's non-JSON `success`/`malformed` stdout), usage
  falls back to `not_measured` and `terminal_reason` carries an explicit note —
  content-parse failure never overrides the exit-code-derived `terminal_state`.
  The raw, already-redacted combined stdout/stderr is staged as a `log` artifact
  via D4's `ArtifactStager`, tagged `application/x-ndjson` when it parsed and
  `text/plain` otherwise.
- **Cancelling the process tree** (`cancel`): delegates to D4's
  `SupervisedProcess::cancel` (SIGTERM → grace → SIGKILL to the whole process
  group), already proven generically by D4's own tests; this adapter's own test
  proves it end-to-end through `start`/`cancel`, not just the raw primitive.
- **Reporting the actual selection**: `ActualExecution.model_provider`/`model_id`
  echo the *validated* request (never fabricated) with
  `model_observation_source = "requested_not_confirmed"` — the exact same literal
  D1 independently introduced for the identical reason (see below), reused here
  deliberately for cross-adapter consistency ahead of D5's reconciliation.
- **Reconciling the journal only when genuinely supported** (`reconcile`): the
  opaque handle embeds the real OS pid (`opencode:<pid>:<counter>`); on Unix,
  `crate::harness::process::process_alive(pid)` is a real liveness probe, not a
  guess. Non-Unix has no portable primitive and reports
  `HarnessError::RecoveryUnavailable` honestly rather than pretending. Documented
  pid-reuse caveat: a long-dead attempt's pid could in principle be reassigned to
  an unrelated process, producing a false `ProcessRunning` — this is the *safe*
  direction of error (`RecoveryDisposition::SafePreSpawnRequeue` only ever
  unlocks on a confident `ProcessStopped`), never a dangerous double-launch.

## Tests added and exact commands/results

- `cargo test -p tack-runner --lib -- opencode::` — **32 passed, 0 failed, 1
  ignored** (the opt-in live test), stable across 3 repeated runs at
  `--test-threads=8`.
- `cargo test -p tack-runner` — **176 lib + 2 CLI + 7 crash_matrix = 185 passed,
  0 failed, 3 ignored** (my opt-in live test + two more opt-in live tests, D1's
  and D2's own). Stable across 3 repeated runs.
- `cargo test -p tack-runner --lib -- --ignored opencode::tests::live_ --nocapture`
  — **1 passed**, against the real, installed `opencode` 1.18.0:
  ```
  live opencode probe: installed_version="1.18.0" probe_error=None model_combinations=1
  live opencode run: terminal_state=Succeeded tokens_in=Some(7885) tokens_out=Some(3) cost_usd=Some(0.0) harness_version=1.18.0
  ```
- `cargo test --workspace` — **1039 passed, 0 failed** (after the transient
  `claude_code.rs` concurrent-edit noise described above resolved itself on
  re-run).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt -p tack-runner -- --check` — clean (only my own files formatted,
  per rule 10).
- `git diff --check -- crates/tack-runner/src/harness/mod.rs
  crates/tack-runner/src/harness/opencode.rs` — clean.
- `git status --porcelain` — only my two files plus D2's concurrent
  `claude_code.rs`, as expected.

### Acceptance gate → test mapping

| Acceptance bullet | Test(s) |
|---|---|
| Fake-binary success | `fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact` |
| Fake-binary failure | `fake_binary_failure_completes_failed_with_the_exit_code_in_terminal_reason` |
| Fake-binary cancel | `cancel_kills_the_whole_descendant_tree_via_the_adapter` |
| Fake-binary malformed output | `fake_binary_malformed_output_does_not_panic_and_falls_back_to_not_measured_usage` (exec path) + `probe_reports_malformed_version_output_as_an_explicit_probe_error` (probe path) |
| Fake-binary unknown version | `probe_reports_an_unrecognized_version_string_as_an_explicit_probe_error` |
| **Unsupported selection fails pre-spawn** | `unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever` (structural: auto-selection, proved against a `hang`-mode process that would block for an hour if ever actually spawned) |
| **...specifically a valid model paired with the wrong provider** | `validate_rejects_a_valid_model_id_paired_with_the_wrong_provider` — requests `anthropic/gpt-4` (a real model id, only ever offered by `openai` in the synthetic multi-provider capability data), asserts `Err(Rejected)`, then asserts the *same* model id under its real provider (`openai/gpt-4`) validates successfully — proving the rejection is about the pairing, not the id. Reinforced by `validate_rejects_a_provider_with_no_discovered_models_at_all`. |
| Arguments/environment redacted, canary absent | `secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact` — two canaries (env value, prompt/stdin), `echo_canary` fake mode, asserted absent from `terminal_reason` *and* the staged artifact file on disk |
| Opt-in live test records version and artifact | `live_probe_and_a_real_free_run_record_version_and_artifact` (see real output above) |

### This card's differentiator, specifically tested

- `parse_model_combinations_groups_by_provider_and_preserves_pairing` — uses the
  **exact six lines this card observed** from a real, zero-credential
  `opencode models` run, plus two synthetic providers, proving grouping across
  more than one provider.
- `parse_model_combinations_keeps_a_model_id_with_extra_slashes_fully_opaque` —
  proves only the *first* `/` is ever a delimiter.
- `probe_end_to_end_reports_installed_version_and_grouped_model_combinations` —
  version detection and model enumeration succeeding together, through one real
  subprocess-driving adapter instance (an inline branching `/bin/sh -c` fixture,
  not just the pure parser in isolation).
- `wait_extracts_real_token_and_cost_usage_from_an_observed_shaped_event_stream` —
  drives a literal, observed-shaped JSONL sample and asserts `Usage` is
  genuinely `Measured` (tokens_in=21, tokens_out=3, cost_usd=0.0021), which
  `codex.rs` cannot do (never observed a parseable usage shape for Codex).
- `wait_embeds_a_parsed_error_event_into_terminal_reason_without_changing_exit_code_semantics` —
  reproduces the real wrong-provider error shape observed live.

## Failure/adversarial case proved

- The pre-spawn hang-guard test (above) is a genuine adversarial proof, not just
  a `matches!` assertion: if `start()`'s structural check were broken, the test
  would hang for up to an hour and only fail via its own `tokio::time::timeout`
  wrapper turning that into a fast, loud failure.
- `validate_rejects_when_the_capability_probe_itself_fails` — a probe failure
  (version command exits nonzero) makes pairing validation fail closed
  (`Rejected`), never "assume the pair is fine since we couldn't check."
- `cancel_and_wait_on_an_untracked_handle_are_typed_rejections` — a handle this
  adapter instance never produced (e.g. stale after a runner restart) is a typed
  `HarnessError::Process`, never a silent success or a panic.
- `probe_never_hangs_past_its_own_timeout` — a `hang`-mode fake binary is killed
  and reported as an explicit `probe_error` containing "timed out" within the
  configured `probe_timeout`, proven with `tokio::time::timeout` around the
  whole call so a broken timeout would fail the test fast rather than hang CI.
- `reconcile_observes_a_real_alive_process_as_running` /
  `reconcile_observes_a_dead_pid_as_stopped` — real, independently-spawned
  `sleep 30` / `true` processes (not spawned via the adapter), proving the
  liveness probe against genuine OS state, not a mock.

## The observed OpenCode contract, exactly as tested

Every command below was run inside an isolated `HOME`/`XDG_CONFIG_HOME`/
`XDG_DATA_HOME`/`XDG_CACHE_HOME` sandbox pointed at a fresh temp directory (never
this repository, never the real `~/.local/share/opencode/auth.json` this machine
already has configured with unrelated live credentials), using only opencode's
own zero-credential `opencode/*` "zen" models. No real provider credential was
ever passed, read, or logged.

```
$ opencode --version
1.18.0

$ env -i HOME=<sandbox> XDG_CONFIG_HOME=<sandbox>/config XDG_DATA_HOME=<sandbox>/data \
    XDG_CACHE_HOME=<sandbox>/cache PATH="$PATH" opencode models
opencode/big-pickle
opencode/deepseek-v4-flash-free
opencode/hy3-free
opencode/mimo-v2.5-free
opencode/nemotron-3-ultra-free
opencode/north-mini-code-free

$ env -i HOME=<sandbox> ... opencode run --model opencode/big-pickle --format json \
    "Reply with exactly the single word: banana"
{"type":"step_start","timestamp":...,"sessionID":"ses_...","part":{...}}
{"type":"text","timestamp":...,"sessionID":"ses_...","part":{"type":"text","text":"banana",...}}
{"type":"step_finish","timestamp":...,"sessionID":"ses_...","part":{"type":"step-finish",
  "reason":"stop","tokens":{"total":7972,"input":7957,"output":3,"reasoning":12,
  "cache":{"write":0,"read":0}},"cost":0}}
(exit 0)

# Wrong provider, real model id (only ever offered by `opencode`, not `anthropic`):
$ env -i HOME=<sandbox> ... opencode run --model anthropic/big-pickle --format json "hello"
{"type":"error","timestamp":...,"sessionID":"ses_...",
  "error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server
  logs for details.","ref":"err_..."}}}
(exit 1)

# Prompt via stdin (no positional message argument):
$ echo "Reply with exactly the single word: kiwi" | env -i HOME=<sandbox> ... \
    opencode run --model opencode/big-pickle --format json
{"type":"text",...,"part":{"type":"text","text":"kiwi",...}}
(exit 0)

# SIGTERM mid-run:
$ opencode run --model opencode/big-pickle --format json "<long prompt>" & sleep 1.2; kill -TERM $!
(exit 143, no partial stdout, no orphaned processes)

# Authoritative post-hoc model confirmation (NOT wired into this adapter — see below):
$ env -i HOME=<sandbox> ... opencode export ses_...
{"info": {"model": {"id": "big-pickle", "providerID": "opencode", "variant": "default"},
  "cost": 0, "tokens": {"input": 23, "output": 510, ...}, ...}, "messages": [...]}
```

## Observed vs. assumed — explicit split

**Observed** (exact commands above, run against the real, installed `opencode`
1.18.0; each also cited at its point of use in `opencode.rs`'s module docs):

1. `opencode --version` prints a bare `X.Y.Z\n`, exit 0, empty stderr.
2. `opencode models` prints one `<providerID>/<modelID>` line per model; with
   zero configured credentials, exactly six zero-cost `opencode/*` models.
3. `opencode run --format json [--model provider/model]` reads the prompt from
   **stdin** when no positional message is given; stdout is newline-delimited
   JSON events; a successful `step_finish` carries real `tokens`/`cost`.
4. A **valid model id paired with the wrong provider is not rejected by opencode
   itself** — it starts a session and only fails afterward with a generic
   `{"type":"error",...}` event and exit 1. This is the direct justification for
   this adapter's own pre-spawn pairing check.
5. `SIGTERM` cleanly stops a plain `opencode run` (single process, its own
   process-group leader for a conversational prompt); a `bash` tool call
   proceeded without any interactive-approval event and without `--auto`, in
   this sandboxed environment.
6. `opencode` needs a working `PATH` (for its own tool subprocesses) and
   `HOME`/`XDG_*` (for config/credentials/session store) — it does not get a
   shell's implicit default-`PATH` fallback, since it is a Bun/Node binary.
7. `opencode export <sessionID>` returns genuine JSON (not NDJSON;
   `Exporting session: ...` goes to *stderr*) whose `info.model.{providerID,id}`
   names the model **actually** used — confirmed, but **not wired into this
   adapter** (see below).

**Assumed / deliberately not attempted** (flagged, not silently guessed):

- Real (non-free, credentialed) provider behavior — never tested, deliberately,
  to avoid any risk of touching a real API key on this machine.
- Whether `opencode`'s permission/approval behavior always auto-proceeds for
  *every* tool, or only for the specific `bash` call this card happened to
  trigger under this machine's default `opencode.json` — only one scenario was
  tested. The attempt's own configured timeout is the safety net regardless.
- True offline/no-network behavior of `opencode models`: setting
  `http_proxy`/`https_proxy` to an unroutable address had no observable effect
  (Bun's `fetch` may not honor those env vars), so genuine network isolation was
  not achieved without modifying system DNS/firewall state, which was judged out
  of scope for a sandboxed probe. Not claimed as verified either way.
- Whether the JSONL event stream is *always* strictly one-object-per-line under
  every circumstance (only a handful of short runs were observed). The adapter
  treats any parse failure as "not the expected shape" rather than crashing.

## Falsifying facts / observations for D5

1. **Two of three real adapters independently reject auto-selected models
   pre-spawn**, for the identical reason: `ActualExecution.model_provider`/
   `model_id` are non-nullable (III.1.3), and neither Codex's nor OpenCode's
   non-interactive JSON output names the actually-used model for *any* observed
   event type. This is now corroborated evidence, not a single card's guess,
   for whether the frozen contract should either (a) make these fields nullable
   for the auto-selection case, or (b) formally document
   "confirm the actual selection via a harness-specific follow-up call" (see
   point 2) as the expected adapter shape for supporting auto-selection at all.
2. **A real, verified path exists to close that gap for OpenCode specifically**:
   `opencode export <sessionID>` returns authoritative `info.model.{providerID,id}`.
   This card deliberately did not wire it in (it would require capturing the
   `sessionID` from the JSONL stream and a *second* subprocess call inside every
   successful `wait()`, with its own timeout/failure handling — real,
   additional scope, not a two-line change). Recommended as the concrete shape
   for whichever future card wants full auto-selection support for OpenCode.
3. **`docs/contracts/runner-v1/capabilities.json`'s `HarnessCapability.model_combinations`
   (`ModelCombination { model_provider, model_ids: Vec<ModelId>, discovery }`)
   already faithfully expresses paired provider→model combinations** — grouped
   by provider, never flattened. This card did **not** need to request any
   change to the frozen capability shape; it is exactly right for this card's
   distinguishing requirement, confirmed by actually implementing a full,
   real-CLI-backed pairing-validation adapter against it. This is worth D5
   recording explicitly: the shape was already correct before any real adapter
   existed to prove it.
4. **`HarnessAdapter::validate`'s `Result<(), HarnessError>` cannot carry a
   reason.** `HarnessError` is a bare 3-variant enum with static `#[error(...)]`
   messages, no payload. This card's pairing check has several genuinely
   distinct rejection reasons (wrong provider, unknown provider entirely,
   partial selection, probe failure, unresolvable binary) that all collapse to
   the same `HarnessError::Rejected` at the trait boundary. Mitigated with
   `tracing::warn!` at each rejection site (including, for the wrong-provider
   case specifically, a `model_id_known_under_a_different_provider` field for
   operator diagnosis) — but this is a real interface limitation, not a style
   choice; D1 independently hit the analogous gap for its own rejection
   reasons. Not proposing a fix unilaterally (rule 6) — flagging for D5.
5. **`model_observation_source` has no closed vocabulary.** It is a bare
   `String` on `ActualExecution`. This card reused D1's exact literal
   `"requested_not_confirmed"` for cross-adapter consistency, but nothing
   structurally prevents a third adapter from inventing a fourth, incompatible
   string for the same concept. Worth D5 considering whether this should become
   a small closed enum once three real adapters' actual values are known (mine
   and D1's already agree; D2's is unread by this card).

## Known limitations or `not_measured` fields

- `Usage.tokens_in`/`tokens_out`/`cost_usd` are `Measured` only when the run's
  stdout parses as the expected newline-delimited JSON event stream and a
  `step_finish` event is present; otherwise `not_measured` — never a fabricated
  number.
- `Usage.duration_ms` is always `Measured`, computed from the runner's own
  clock (`started_at`/`ended_at`), not a harness-reported figure.
- `FeatureCapabilities.decisions` and `.resume` are `unsupported` with reasons —
  no mid-run operator-decision event was ever observed, and session resume after
  cancellation was never tested (opencode's `--continue`/`--session` flags exist
  but their behavior relative to a cancelled non-interactive run is unverified).
- `FeatureCapabilities.artifacts` is `advisory`: only the raw captured event
  stream is staged; no opencode-specific artifact discovery (e.g. a git diff of
  files it changed) is implemented.
- `PermissionPolicy`/`spec.work.request.permission_policy` is not translated
  into any opencode-specific flag or config. OpenCode's own permission model
  (`opencode.json`, `--auto`) does not map cleanly onto the generic
  `{tools, network}` shape without more evidence than this card gathered;
  inventing a mapping risked looking like enforcement without being
  enforcement. `--auto` (explicitly "(dangerous!)" per its own `--help`) is
  never passed; the attempt's configured timeout is the safety net.
- `secret_reference` entries in `spec.work.request.environment` are never
  resolved (no secret-store client exists in `tack-runner` yet) — only literal
  `value` entries reach the child's environment. Identical, independently
  reported gap to D4's event/artifact-transport limitation and D1's own
  `codex.rs` note.
- `reconcile`'s liveness probe cannot distinguish "the original process is still
  running" from "the OS reused this pid for an unrelated process" after a long
  gap — documented in code as the safe-conservative direction of error (see
  "Behavior implemented" above).
- Non-Unix builds: `reconcile` reports `HarnessError::RecoveryUnavailable`
  unconditionally (no portable liveness primitive), matching `process.rs`'s own
  documented non-Unix limitation and D1's identical choice.

## Secrets/logging review

- `SecretMaterial` registers the prompt (`resolved_agent_profile.instructions`)
  and every literal environment `value` before the process is even spawned;
  captured stdout/stderr is scrubbed by D4's `finalize_capture` before this
  adapter ever sees it, and the staged artifact is written from that
  already-scrubbed text.
- `secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact`
  plants two independent canaries (env value, prompt/stdin) and asserts absence
  from both `terminal_reason` (embedded, scrubbed JSON) and the artifact file on
  disk, using the shared fixture's `echo_canary` mode as a worst-case leaky
  harness simulation.
- No `tracing::*!` call anywhere in `opencode.rs` passes a raw environment
  value, prompt body, or credential — only ids, model provider/model strings
  (not secret), reasons, and structured diagnostic flags.
- The opt-in live test never reads or logs a real credential: it runs entirely
  under a sandboxed `HOME`/`XDG_*`, using only opencode's zero-credential
  `opencode/*` models, and prints only version/terminal-state/token-count/cost —
  never a credential, prompt body, or raw stdout dump.

## Dependency needed but not added

None. `chrono`, `serde_json`, `tokio`, `tracing`, `thiserror`, `async-trait` were
all already `tack-runner` dependencies (used identically by D4/D1). No PATH
resolution crate (`which`) was added — `locate_in_dirs`/`system_path_dirs` are a
small, dependency-free `PATH` scan, mirroring D1's identical `codex.rs` helper
functions (each adapter keeps its own copy rather than a shared one, since
sharing would require editing `harness/mod.rs` beyond the one permitted line).

## Safe merge order and likely conflicts

- No conflicts expected with D1 (`codex.rs`, committed) or D2 (`claude_code.rs`,
  in-flight): each adds a new file this card never touches.
- `harness/mod.rs`: my single `pub mod opencode;` line is alphabetically
  positioned and independent of D1's/D2's own lines; no textual overlap, but
  simultaneous uncommitted edits mean whoever commits second should `git diff`
  the file before committing to confirm all three lines (`claude_code`, `codex`,
  `opencode`) are present and nothing was clobbered.
- Merge before D5: D5's "register all three without ordering behavior" task
  needs `OpenCodeAdapter::discover(process_limits, artifact_staging_root)` (same
  constructor shape as `CodexAdapter::discover`, deliberately) plus
  `HarnessKind::new("opencode")` as the registration key.
- `registry.rs` (D5-owned) was not read in depth beyond what D4's handoff
  already described; not touched.

## Checklist

- No unowned files: confirmed via `git status --porcelain` above (only
  `mod.rs` + `opencode.rs` are mine; `claude_code.rs` is D2's own concurrent
  work, untouched by me).
- No live secret: `SecretMaterial` scrubbing audited above; canary test passes;
  live test uses only zero-credential models under a sandboxed `HOME`/`XDG_*`.
- No panic stub: no `unimplemented!()`/`todo!()` anywhere in `opencode.rs`;
  every error path is a typed `Result` variant (the frozen `HarnessError`
  enum, or a `String` reason folded into `HarnessCapability.probe_error`).
- No blind retry: `cancel`/timeout paths delegate to D4's already-audited
  `SupervisedProcess` (SIGTERM → grace → SIGKILL exactly once); this adapter
  adds no retry loop of its own anywhere.
