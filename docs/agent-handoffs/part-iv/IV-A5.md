# IV-A5 handoff

- Base SHA / branch / final SHA: base `cace43f0ac02ac189339b554c78024d6fcbfa35d` (develop tip at
  worktree creation, includes IV-A1/IV-A2/IV-A3), branch `agent/iv-a5-runner-doctor`, final SHA —
  see the last commit on this branch.
- Files changed (must equal ownership list, plus the one additive exception the card text calls
  out): `crates/tack-cli/src/doctor.rs` (new), `crates/tack-cli/src/main.rs`,
  `crates/tack-cli/Cargo.toml` (moved the existing `tack-orch` path dependency from
  `[dev-dependencies]` to `[dependencies]` — `Cargo.lock` is unaffected since the resolved
  dependency graph does not change, only which build profile can name the crate's types),
  `crates/tack-runner/src/bootstrap.rs` (the minimal, additive `pub` surface the card's own
  acceptance criteria require — see below), `docs/agent-handoffs/part-iv/IV-A5.md`. Nothing else
  touched. `git log --oneline develop..HEAD -- crates/tack-runner` was run before touching
  `bootstrap.rs` and returned nothing — no other in-flight card had modified it.
- Contract fixtures consumed: none. This card never touches a handler, the OpenAPI surface, or
  `docs/contracts/runner-v1/`; `RunnerCapabilities` is consumed as an existing Rust type, not a
  wire fixture.
- New `pub` surface added to `crates/tack-runner/src/bootstrap.rs` and why:
  - `pub struct DiscoveryReport { pub capabilities: RunnerCapabilities, pub
    claude_code_discovery_error: Option<String> }` — the second field exists because
    `build_adapter_registry` never registers an adapter or probe for a harness whose `discover`
    fails (documented behavior, unchanged). Codex and OpenCode are *always* registered regardless
    of whether their binary exists — their absence surfaces as a `probe_error` string on their own
    `HarnessCapability` entry. Claude Code is different: a missing `claude` binary means
    `ClaudeCodeAdapter::discover()` never runs at all, so `capabilities.harnesses` has **no entry**
    for it whatsoever. A caller that wants to report Claude Code's absence honestly (the card's own
    "named honestly as absent, not silently omitted" requirement) needs that error captured
    somewhere; this field is the only place it exists.
  - `pub async fn probe(staging_root: &Path, process_limits: &ProcessLimits) -> DiscoveryReport` —
    calls the exact same `build_adapter_registry`/`report_capabilities` pair `build_runtime`
    already calls, then calls `ClaudeCodeAdapter::discover()` once more (already imported into
    `bootstrap.rs`; a cheap, side-effect-free PATH scan, not a subprocess) to capture its error for
    the field above. No discovery/capability logic is duplicated — this reuses A1's probe path
    literally, it does not re-implement it.
  - Both are additive; nothing in `build_runtime`/`run`/`build_adapter_registry`/
    `report_capabilities` was changed. `cargo test -p tack-runner` (233 passed, 0 failed, 3
    ignored — unchanged from before this branch) confirms nothing existing regressed.
- Behavior implemented (`crates/tack-cli/src/doctor.rs`, wired as `RunnerAction::Doctor { json:
  bool }` in `main.rs`, intercepted in the same pre-`TackClient` match arm as `RunnerAction::Start`
  since it never needs a live API client or a server):
  - `tack runner doctor` calls `tack_runner::bootstrap::probe` in its own single Tokio runtime
    (mirroring `local_runner::run_standalone`'s pattern) and renders a human-readable report by
    default, or the raw `RunnerCapabilities` JSON with `--json`.
  - For each of the three known harness kinds (`codex`, `claude-code`, `opencode`, in that fixed
    display order — never registry `BTreeMap` order), `classify()` determines one of three states:
    - **Present** — `probe_error: None`; prints the real detected version.
    - **Absent** — either (a) the harness kind has no entry in `capabilities.harnesses` at all
      (only reachable for `claude-code`, whose reason comes from `claude_code_discovery_error`), or
      (b) `probe_error` is `Some(reason)` where `reason` contains the literal substring `"not found
      on PATH"` — the exact wording every `*Locator::resolve` in `codex.rs`/`opencode.rs` produces
      for "never found on PATH", already locked in by those files' own existing tests
      (`.contains("not found")`).
    - **ProbeError** — `probe_error: Some(reason)` where `reason` does *not* contain that
      substring: the binary was found and a subsequent probe step failed (unparseable version,
      nonzero exit, timeout, or — OpenCode specifically — a confirmed version followed by a failed
      model-enumeration call). Prints the confirmed version if one exists, plus the error text,
      never conflated with "absent".
  - Each harness section also prints `model_combinations`/`model_passthrough` verbatim from its
    `HarnessCapability` entry (when one exists) and a grounded, adapter-specific credential note
    (see below).
  - A single "Runner-wide capabilities" section prints `cancel`/`resume`/`decisions`/`artifacts`/
    `usage` from `RunnerCapabilities.features` once — these are genuinely per-runner, not
    per-harness, in the real struct (`bootstrap::report_capabilities` computes them once,
    independent of which harnesses are installed), so the report does not fabricate a per-harness
    breakdown that doesn't exist on the wire.
  - Credential notes (`credential_note`, one string per harness kind) are grounded directly in each
    adapter's own environment-forwarding code, not invented:
    - `codex`: `CodexAdapter::start`'s `env` map is built *only* from `spec.work.request.environment`
      (`codex.rs` lines ~765–774) — no ambient host environment, not even `HOME`, is forwarded into
      an actual run. Doctor states this plainly, alongside "Codex authenticates itself; Tack never
      reads, stores, or forwards it."
    - `claude-code`: `ClaudeCodeAdapter::base_environment` forwards exactly `HOME` and `PATH` from
      the runner process's own environment (`claude_code.rs`'s own doc comment: "`claude` needs
      `HOME` to find its OAuth session/config").
    - `opencode`: `OpenCodeAdapter::default_passthrough_env` forwards `PATH`, `HOME`, and the four
      `XDG_*` variables (`opencode.rs`'s own doc comment, observed fact 6).
  - The report closes with one line paraphrasing ADR 0050/0058's "Tack does not proxy model
    providers" position, naming both ADRs.
- Tests added and exact commands/results:
  - `crates/tack-cli/src/doctor.rs`, `#[cfg(test)] mod tests` (8 new tests, all deterministic, no
    subprocess, no network, no PATH mutation):
    - `a_healthy_probe_is_present_with_its_real_version`
    - `a_binary_never_found_on_path_is_absent_not_a_probe_error`
    - `a_present_binary_with_unparseable_version_output_is_a_probe_error_not_absent` — installed
      version empty, reason text mirrors `codex.rs`'s own unrecognized-version-string fixture
      wording, asserts `ProbeError`, not `Absent`.
    - `a_present_binary_with_a_later_probe_failure_keeps_its_confirmed_version` — mirrors
      `opencode.rs`'s real "installed_version confirmed; provider/model enumeration failed" arm;
      asserts the confirmed version survives into `ProbeError { version: Some("1.18.0"), .. }`.
    - `claude_code_missing_from_the_harness_list_is_reported_absent_using_its_own_discovery_error`
    - `claude_code_registered_and_healthy_is_present`
    - `every_known_harness_kind_has_a_credential_note`
    - `render_does_not_panic_on_a_populated_report` — exercises `render`/`render_model_info`
      end-to-end against a hand-built `DiscoveryReport` with real model combinations, so a future
      field added to `ModelCombination`/`RunnerCapabilities` fails loudly here.
  - `cargo fmt --all` → clean (reformatted the new file's struct/enum literals once, no semantic
    change).
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean (workspace-wide, since this
    card touches two crates).
  - `cargo test -p tack-runner` → 233 passed (lib) + 6 (h3_checkout) + others, 0 failed, 3 ignored
    (opt-in live tests, unchanged) — no regression from the additive `bootstrap.rs` change.
  - `cargo test -p tack-cli` → 57 passed (lib) + 13 passed (`main.rs`/`doctor`+`local_runner` unit
    tests, 8 new + 5 pre-existing) + 11 passed (`cli_test.rs`) + 5 passed
    (`e6_scheduler_e2e_test.rs`), 0 failed.
  - `cargo build --workspace` → clean.
- Failure/adversarial case proved (probe-error-vs-absent, live, not just unit-tested):
  - Built the release-profile-free debug binary once (`cargo build -p tack-cli`,
    `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/IV-A5`), then ran the real `tack` binary three
    times with different `PATH`s, no source changes between runs:
    1. Normal `PATH` (this machine's real `codex`/`claude`/`opencode`, all installed) → all three
       report `status: present` with their real versions (`0.149.1`, `2.1.252`, `1.18.0`).
    2. `PATH=/usr/bin:/bin` (none of the three present) → all three report `status: absent`, each
       with its own real discovery-failure text (`` `codex` was not found on PATH ``, `no
       executable named \`claude\` was found on PATH`, `` `opencode` was not found on PATH ``).
    3. A throwaway shell script named `codex` (`#!/bin/sh\necho "not-a-version-string, something
       broke" >&2\nexit 3`), `chmod +x`, placed in a directory prepended to `PATH` ahead of the
       real `codex` → doctor reports `status: present, probe error` /
       `probe_error: codex --version exited with status 3` — never `absent`, because the binary
       genuinely was found and spawned. This is the live, real-process proof that the
       absent/probe-error distinction is load-bearing in the actual code path, not only asserted by
       a unit test against a hand-built fixture.
- Schema/API/contract change requested from another owner: none.
- Which role executed what for the live runs (Part IV addition): N/A in the literal sense — this
  card never claims or executes an attempt, so there is no runner role to attribute a run to. The
  live runs performed were process-level: the `tack` binary invoked three times directly (see the
  adversarial-case proof above) with varying `PATH`, and once more against a live `tack serve` (see
  the capability-match proof below) purely to enroll a runner and read back what it reported — no
  execution request was ever created or claimed in either case.
- Capability-match proof (the card's third acceptance item — doctor's output must equal exactly
  what a real enrollment sends a server, not a second independent derivation):
  1. Started a throwaway `tack serve` (`TACK_DATABASE_URL` pointed at a scratch sqlite file,
     `TACK_PORT=32109`, `TACK_API_TOKEN` set) via `/var/tmp/tack-agent-targets/IV-A5/debug/tack
     serve`.
  2. `tack runner enroll doctor-proof-runner --total-capacity 1 --available-capacity 1 --json` →
     real one-time `enrollment_token` + `runner_id`.
  3. `TACK_RUNNER_ENROLLMENT_TOKEN=<token> TACK_RUNNER_API_URL=http://127.0.0.1:32109/api/runner/v1
     TACK_RUNNER_ID=<runner_id> tack runner start` — a real, separate runner process, redeeming the
     token and reporting its real capability snapshot at enrollment.
  4. `GET /api/runners` (Bearer-authenticated) on the live server returned the stored
     `capability_snapshot` for that runner — `harnesses` (all three, real versions, real opencode
     `model_combinations`), `features`, `limits`, `concurrency`, `labels` — this is the literal
     value `HttpRunnerClient`'s `embedded_capabilities` sent over the wire at enrollment, stored
     server-side.
  5. `tack runner doctor --json` run independently (no server involved) on the same machine.
  6. Diffed both JSON documents programmatically (`python3`, field-by-field): `harnesses` (with
     `probed_at` excluded — it necessarily differs between two separate probe invocations
     performed seconds apart), `features`, `limits`, `concurrency`, and `labels` are **byte-for-byte
     identical** between doctor's own probe and what the server actually received and stored.
     `harnesses match (excluding probed_at): True / features match: True / limits match: True /
     concurrency match: True / labels match: True`.
  7. Cleaned up both throwaway processes afterward; nothing from this proof was committed (scratch
     files live under
     `/tmp/claude-1000/-home-ox-Sites-objetivosMios/b3baec54-86ac-4990-a4f2-730beab1cbc7/scratchpad/iv-a5-live-proof/`,
     not in the repo).
- Loopback/gating proof (Part IV addition): N/A. `tack runner doctor` never binds a socket, never
  starts an embedded runner, never touches `TACK_LOCAL_RUNNER_ENABLE`/`--with-runner`, and gates
  nothing — it only reads this machine's own `PATH` and spawns short-lived `--version`/model-list
  probes of already-installed binaries, exactly as `tack runner start`'s own startup capability
  probe already does. There is no off-by-default posture to prove here because there is no new
  network or destructive capability being gated.
- Binary-size delta (Part IV addition): N/A for this card in the load-bearing sense IV-A3 measured
  — `tack-orch`'s types were already linked into `tack` transitively via `tack-runner`
  (`tack-runner` depends on `tack-orch`) before this card; moving `tack-orch` from a
  dev-dependency to a direct dependency of `tack-cli` only makes an already-linked crate's types
  nameable from new source, it does not add a new crate to the dependency graph. `cargo build
  --workspace` succeeding with no new crates appearing in the `Compiling`/`Checking` list beyond
  what IV-A3's own build already produced is the evidence; no separate before/after release build
  was taken since there is nothing new to measure.
- Known limitations or `not_measured` fields: none introduced. Every capability value doctor prints
  is read directly off the real `HarnessCapability`/`FeatureCapabilities` structs a live runner
  would report — nothing is estimated or rounded up. The absent/probe-error distinction for
  Codex/OpenCode relies on matching the literal `"not found on PATH"` substring in `probe_error`
  text rather than a dedicated boolean field on `HarnessCapability` — this is the existing,
  already-tested wording those adapters produce (see `codex.rs`'s/`opencode.rs`'s own
  `.contains("not found")` assertions), not a new convention invented for this card, but a future
  wording change to either adapter's error text would need a matching update here.
- Secrets/logging review: `tack runner doctor` never reads, logs, or prints a credential of any
  kind — it never touches `TACK_RUNNER_ENROLLMENT_TOKEN`, `TACK_API_TOKEN`, or any harness-specific
  env var; the only environment values it (indirectly, via the adapters' own `discover`/`probe`
  paths) inspects are `PATH`/`HOME`/`XDG_*`, none of which are secrets, and none of those values are
  ever printed back — only the fact that a binary was found/absent and its detected version are.
  The credential notes are static, hardcoded prose naming *mechanisms* (OAuth session location,
  env-var convention), never a value read from this machine.
- Safe merge order and likely conflicts: `main.rs` gets a new `mod doctor;` line, a new
  `RunnerAction::Doctor` variant, one new match arm in the pre-`TackClient` dispatch, and one new
  `unreachable!()` arm in the exhaustive `Commands::Runner` match — IV-A4 (zero-touch enrollment,
  concurrent, separate branch) is expected to also touch `main.rs` to add its own subcommand arm;
  this is the normal, anticipated overlap the integrator resolves, not a real conflict (the two
  cards' hunks are in different, non-adjacent parts of the enum/match). `crates/tack-runner/src/
  bootstrap.rs` was confirmed untouched by any other in-flight branch before this card edited it
  (see the ownership note above); the change is a pure addition at the end of the file's public
  surface, unlikely to conflict with anything.
- Checklist: no unowned files touched (the one exception — `bootstrap.rs` — is explicitly
  authorized by this card's own acceptance criteria and explained above) — no live secret
  introduced or logged — no panic stub (`debug_assert!(false, ...)` in `credential_note`'s
  unreachable `other` arm only fires in debug builds if a fourth harness kind is ever added to
  `KNOWN_HARNESS_KINDS` without a matching credential note, and still returns a value rather than
  panicking in release) — no blind retry anywhere in the new code.
