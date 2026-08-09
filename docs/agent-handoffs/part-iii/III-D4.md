# III-D4 handoff

- **Base SHA / branch / final SHA:** base `b73aeb3` on `plan/harness-agnostic-agent-fleet`
  (descendant of the accepted Wave 2 integration SHA `f931fc0`, plus two unrelated
  post-acceptance commits — `ea7b764` docs acceptance record, `b73aeb3` an API test
  determinism fix). Worked directly in the main checkout, no worktree. **Not committed** —
  per instructions this handoff describes the uncommitted working tree; there is no final SHA.
- **Files changed (must equal ownership list):**
  - New: `crates/tack-runner/src/harness/mod.rs`, `process.rs`, `event_sink.rs`, `redact.rs`,
    `sha256.rs`, `artifact.rs`, `fixtures/mod.rs`, `fixtures/fake_harness.sh`
  - Modified: `crates/tack-runner/src/lib.rs` (one line: `pub mod harness;`)
  - New: this handoff
  - `crates/tack-runner/src/engine.rs`: **not modified** — see "Falsifying fact and the
    engine.rs decision" below for why, and what was evaluated and rejected.
  - `git status --porcelain` confirms exactly this: `M crates/tack-runner/src/lib.rs` and
    `?? crates/tack-runner/src/harness/`.

## Correcting the card's premise before reading the rest of this handoff

The dispatch briefing for this card stated *"There is no `HarnessAdapter` trait in the tree
today."* That does not match the checked-out tree. C3 (Wave 2, accepted at `f931fc0`) already
added `crates/tack-runner/src/engine.rs::HarnessAdapter` — five methods
(`validate`/`start`/`cancel`/`wait`/`reconcile`) — specifically as the seam later harness cards
implement; its own doc comment says so (*"later harness cards implement this contract in their
own files without changing engine ownership"*), and C3's handoff says explicitly: *"D1–D3
implement `HarnessAdapter` in adapter-owned files."* D1/D2/D3's own task lists (`validate frozen
spec`, `cancel process tree`, `reconcile journal only when proven supported`, ...) map onto that
trait's five methods almost one to one.

Given that, this card does **not** redefine a second, competing `HarnessAdapter` trait — doing so
would recreate exactly the "three incompatible interfaces" failure mode this card exists to
prevent, one level up. `crates/tack-runner/src/harness/mod.rs` re-exports the existing
`engine::HarnessAdapter` unchanged and documents this correction at the top of the file in full,
with reasoning. **D1/D2/D3 should be told this before they start**: the trait they implement is
`crate::client::engine::HarnessAdapter` (also reachable as `crate::harness::HarnessAdapter` via
this card's re-export), already frozen since Wave 2 — not something invented by this card.

What this card *does* supply that is genuinely new: [`HarnessProbe`] (capability discovery,
below), the process/event/redaction/artifact infrastructure the acceptance gate requires, and
`AdapterRegistry` (the "shared registry wiring" the card asks for).

## The `HarnessAdapter` trait — as found, and what's new around it

`engine::HarnessAdapter` (unchanged):

```rust
#[async_trait]
pub trait HarnessAdapter: Send + Sync {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;
    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError>;
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError>;
    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError>;
    async fn reconcile(&self, journal: &AttemptJournal) -> Result<RecoveryObservation, HarnessError>;
}
```

D1/D2/D3 implement this for `CodexAdapter`/`ClaudeCodeAdapter`/`OpenCodeAdapter`, composing
`crate::harness::process::SupervisedProcess` inside `start`/`wait`/`cancel` and
`crate::harness::redact::SecretMaterial` throughout. Nothing about it changed here.

New trait — `crate::harness::HarnessProbe`:

```rust
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    fn harness_kind(&self) -> tack_orch::execution::HarnessKind;
    async fn probe(&self) -> tack_orch::execution::HarnessCapability;
}
```

**Reasoning per method:**
- `harness_kind(&self)`: a method rather than a separate registration key, so a probe cannot be
  registered under a kind it does not itself believe it is reporting for (the same
  self-describing pattern `HarnessCapability.harness_kind` already uses).
- `probe(&self) -> HarnessCapability`: detects installed version and reports capabilities.
  Returns a value, never a `Result` — probe failure (binary missing, version unparseable) belongs
  in the already-existing, already-frozen `HarnessCapability.probe_error: Option<String>` field
  (B1, `tack_orch::execution::capabilities`), not as an `Err`. An absent/broken installation is
  exactly as "successful" a probe result as a healthy one, just less capable — this is rule 7's
  "unsupported is typed" applied to the probe itself, not only to what it reports.

**Why this is a separate trait, not a sixth `HarnessAdapter` method:** every existing method
takes an `ExecutionSpec` or `LocalRunHandle`, both of which only exist once a request has been
claimed. Capability reporting (`RunnerCapabilities.harnesses`, populated at enrollment/refresh)
has to run *before* any attempt exists. There is nowhere on the existing trait to hang that
without fabricating a spec to call it with.

**Capability honesty** is inherited directly from the already-frozen `tack_orch::execution`
types this trait returns, not reinvented: `CapabilityValue { support: CapabilitySupport,
reason: Option<String> }` has three explicit levels (`supported`/`unsupported`/`advisory`), never
a bare `bool`; `HarnessCapability.probe_error` makes "could not check" distinct from "checked, and
it's unsupported." `HarnessProbe` implementations must fill in real reasons — nothing here
enforces that structurally beyond the type shape, which is the same honesty guarantee every other
Wave-2/3 card relies on for this exact type.

**What D5 is expected to reconcile** (also documented at length in the `harness/mod.rs` module
doc, so it travels with the code, not only this handoff):

1. **`HarnessProbe` vs. folding it into `HarnessAdapter`.** If three real adapters end up
   wanting to probe as a side effect of something already spec-shaped, merging may be simpler
   than keeping two traits. Keep it separate only if capability discovery genuinely needs to run
   independent of a claimed attempt (it does today, since nothing else drives it).
2. **`LocalRunHandle` cannot name its own harness kind — a real interface gap, not a style
   choice.** `cancel`/`wait` take only `&LocalRunHandle { process_id: String }`, with no
   harness-kind field. A registry dispatching across multiple adapters (see `AdapterRegistry`
   below) therefore cannot route a bare handle back to the adapter that produced it from the
   handle's shape alone. The straightforward fix — add a `harness_kind` field to
   `LocalRunHandle` — was evaluated and **deliberately not made**: `LocalRunHandle` is
   constructed by struct literal in exactly one place this card may not touch,
   `crates/tack-runner/tests/crash_matrix.rs:277` (C4-owned, `Ok(LocalRunHandle { process_id: ...
   })`), and any new required field breaks that construction without a corresponding C4 edit.
   This is the falsifying fact rule 6 asks for: D5 is the only card positioned to coordinate an
   `engine.rs` + `crash_matrix.rs` change together. `AdapterRegistry` works around the gap for now
   (see below); D5 should judge, once three real adapters are in place, whether the field is
   worth the coordinated change.
3. **Kind-key type duplication.** `AdapterRegistry` keys on `tack_orch::execution::HarnessKind`
   (opaque string, matching `ExecutionRequestSnapshot::requested_harness_kind`).
   `crates/tack-runner/src/registry.rs` (D5-owned; not touched by this card) separately defines
   its own `HarnessKind` enum (`Codex`/`ClaudeCode`/`OpenCode`/`Other(String)`). Whether to unify
   these, and whether `AdapterRegistry` belongs in `registry.rs` rather than `harness/mod.rs`, is
   exactly the registry-shape decision D5 owns. This card left `registry.rs` completely untouched
   — it is not in D4's ownership list, and it is explicitly named as D5's in the Wave 3 board.

## `AdapterRegistry` — the "shared registry wiring"

`AdapterRegistry` (in `harness/mod.rs`) holds `BTreeMap<String, Box<dyn HarnessAdapter>>` plus
`BTreeMap<String, Box<dyn HarnessProbe>>`, and **implements `HarnessAdapter` itself** by
dispatching to whichever registered adapter matches `spec.work.request.requested_harness_kind`.
Concretely, `RunnerEngine::new(protocol, adapter_registry, journal, workspaces)` is a complete,
multi-harness runner with **zero changes to `engine.rs`**: `AdapterRegistry` simply *is* the
engine's one concrete `A: HarnessAdapter` type parameter. D5 registers D1/D2/D3's concrete
adapters into an `AdapterRegistry` instance; this card registers only fakes, to prove the
dispatch/routing mechanism itself (`harness::tests::*`, 12 tests).

The `cancel`/`wait`/`reconcile` kind-routing workaround (point 2 above): `start` wraps the
resolved adapter's returned `process_id` as `<hex(kind)>:<inner-process-id>` before handing it to
the engine; `cancel`/`wait` decode that prefix to route back to the same adapter; `reconcile`
decodes the journal's persisted `process_id` the same way, except when it is `None` (no process
was ever confirmed running for *any* kind — this needs no dispatch and answers
`RecoveryObservation::ProcessStopped` directly, which is the one case genuinely kind-independent).
Hex-encoding the kind (not the inner id) means the inner process id may contain any bytes,
including a literal `:`, without ambiguity — mirrors the existing `journal.rs`/`workspace.rs`
attempt-id hex-encoding convention rather than inventing a new one.
`harness::tests::cancel_and_wait_route_the_start_generated_handle_back_to_its_own_adapter` and
`harness::tests::reconcile_decodes_the_kind_and_routes_to_the_right_adapter` prove routing never
crosses kinds; `harness::tests::decode_handle_rejects_input_with_no_recognizable_encoding` and
`reconcile_with_an_undecodable_process_id_is_explicitly_unavailable_not_ambiguous_success` prove
an undecodable handle fails typed (`HarnessError::RecoveryUnavailable`), never a fabricated
success.

`AdapterRegistry::capabilities()` probes every registered `HarnessProbe` in deterministic sorted
order and returns `Vec<HarnessCapability>`, ready to populate
`RunnerCapabilities.harnesses` at enrollment/refresh (that wiring itself — `client.rs`'s actual
`EnrollmentRequest`/`RefreshRequest` construction — is outside this card's ownership and is not
attempted here).

## Falsifying fact and the `engine.rs` decision

The card explicitly permits "a minimal, surgical edit to `engine.rs`... keep it to what
integration genuinely requires." Two concrete edits were evaluated:

1. **Adding `harness_kind` to `LocalRunHandle`** — the natural fix for the routing gap above.
   Rejected: breaks `crash_matrix.rs:277`'s struct-literal construction, a file this card may not
   edit. Reported to D5 instead (point 2 above).
2. **Widening `HarnessError`'s three variants** (`Rejected`/`Process`/`RecoveryUnavailable`) to
   carry more specific process/event-sink failure detail. Evaluated and found unnecessary: the
   richer `ProcessError`/`ArtifactError` taxonomies this card defines stay entirely inside the
   adapter's own implementation (below the `HarnessAdapter` trait boundary) and only need to
   collapse to one of the three existing variants when crossing it — which they already do
   cleanly (`ProcessError::WorkspaceEscape`/`Spawn`/`Io` → `HarnessError::Process` or `Rejected`
   as the concrete adapter judges appropriate).

Net result: **`engine.rs` is unmodified.** `AdapterRegistry` satisfies "common engine
integration" by implementing the existing trait rather than requiring engine changes; `Checkpoint`,
`Workspace`, `AttemptJournal`, `RecoveryObservation` and every other type this card needed were
already public via `crate::client`'s existing re-exports. Zero lines changed is the strongest
justification available when zero genuinely was correct, and it keeps this card's blast radius
away from C3's heavily-tested 3000-line file entirely.

## Process/event infrastructure

- **`harness/process.rs`** — `ProcessSpec`/`SupervisedProcess`. Spawns via `tokio::process`,
  confines `working_directory` to `workspace_root` (canonicalize-then-`starts_with`, mirroring
  `workspace.rs`'s cleanup guard) before ever spawning. Unix: `process_group(0)` puts the child in
  a **new** group whose id equals its own pid; any descendant the child spawns without calling
  `setpgid` itself inherits that group. Cancellation/timeout send `SIGTERM` then (after a grace
  period) `SIGKILL` to the **group** (`kill(-pgid, sig)`), which is why it reaches grandchildren,
  not only the direct child. Stdout/stderr are captured by two reader tasks that keep draining
  past a configured byte cap (dropping, never storing, the excess) so a chatty child can never
  deadlock on a full OS pipe while capture is bounded.
- **`harness/event_sink.rs`** — `EventSink`/`HarnessEvent`. Bounds the *structured* event stream
  (the `event-batch.request.json` `events[]` shape) two independent ways: a bounded
  `tokio::sync::mpsc` channel (real backpressure — `push` genuinely awaits the consumer, proven by
  a test that asserts the send does *not* resolve within a timeout while the channel is full) and
  a lifetime `max_events` cap (because backpressure alone only bounds the instantaneous buffer,
  not how much a run could produce against a consumer that never drains — proven by a test with no
  consumer at all). Oversized individual payloads are replaced with an explicit
  `{"truncated": true, "original_bytes": N, "text_prefix": ...}` marker, never silently shortened.
- **`harness/redact.rs`** — `SecretMaterial` (exact-value scrubbing of captured text/event
  payloads, longest-first so a secret that's a substring of another isn't partially masked),
  `RedactedEnv`/`PromptSummary` (`Debug`-safe stand-ins, matching the existing
  `EnrollmentCredential` pattern), `redact_query` (strips `?...` from URL-shaped strings).
- **`harness/sha256.rs`** — dependency-free SHA-256 (see "Dependency needed but not added"
  below), tested against three published FIPS 180-4/NIST vectors plus block-boundary edge cases.
- **`harness/artifact.rs`** — `ArtifactStager`. Confines a staged source file to its workspace
  the same way `process.rs` confines a working directory (symlink-checked *before* resolving,
  matching `workspace.rs`'s ordering exactly — the first version of this got that ordering
  backwards and a test caught it, see "Failure/adversarial case proved" below); computes a real
  SHA-256 and byte count from the bytes actually copied, never a harness-reported value; stages
  into an owner-only, per-attempt directory.
- **`harness/fixtures/`** — the shared fake binary; see next section.

## The shared fake binary — how D1/D2/D3 drive it

`crates/tack-runner/src/harness/fixtures/fake_harness.sh`, a POSIX `/bin/sh` script (no
compilation step — the "always-runnable" primary path rule 8 asks for). Rust access:
`crate::harness::fixtures::fake_harness_command() -> (PathBuf, Vec<String>)` returns
`(PathBuf::from("/bin/sh"), vec![<absolute script path>])` — put those directly into
`ProcessSpec::program`/`args`. It is invoked through `/bin/sh` rather than executed directly so it
never depends on the executable bit surviving a checkout, and never needs `PATH` (which
`ProcessSpec::spawn` clears by default).

Every knob is an **environment variable** (`ProcessSpec::env`), never an argv flag — argv stays
free for each adapter's own realistic invocation shape.

| `TACK_FAKE_HARNESS_MODE` | Behavior |
|---|---|
| `success` (default) | stdout `fake-harness-ok`, exit 0 |
| `failure` | stderr message, exit `TACK_FAKE_HARNESS_EXIT_CODE` (default 1) |
| `version` | stdout `TACK_FAKE_HARNESS_VERSION` (default `1.0.0`), exit 0 |
| `unknown_version` | stdout an unrecognized/future version string, exit 0 |
| `malformed` | stdout deliberately unparseable mixed garbage, exit 0 |
| `hang` | sleeps `TACK_FAKE_HARNESS_SLEEP_SECONDS` (default 3600) — caller must cancel/time out |
| `spawn_child` | backgrounds `sleep $TACK_FAKE_HARNESS_SLEEP_SECONDS` *without* detaching (same process group), writes its pid to `TACK_FAKE_HARNESS_PIDFILE`, waits on it — the grandchild fixture for cancellation tests |
| `high_volume` | writes exactly `TACK_FAKE_HARNESS_VOLUME_BYTES` (default 50000000) bytes to stdout, exit 0 |
| `echo_canary` | echoes every env var named in `TACK_FAKE_HARNESS_ECHO_ENV_KEYS` (comma/space separated) and all of stdin, to **both** stdout and stderr, exit 0 — simulates a worst-case leaky harness |
| `read_relative` | reads `TACK_FAKE_HARNESS_READ_PATH` relative to cwd, prints it, exit 0/`TACK_FAKE_HARNESS_EXIT_CODE` on miss — for workspace-confinement tests; does no path confinement of its own, the caller's `ProcessSpec::working_directory` is what confines it |

Full mode reference lives as a doc comment at the top of `fake_harness.sh` itself, so it travels
with the file. `crates/tack-runner/src/harness/process.rs`'s tests use every mode at least once
(`every_documented_fixture_mode_behaves_as_documented` explicitly covers `version`/
`unknown_version`/`malformed`, which the acceptance-gate tests don't otherwise touch) — D1/D2/D3
inherit a fixture already proven to behave as documented, not just as designed.

## Acceptance gate — test to proof mapping

| Acceptance bullet | Test(s) |
|---|---|
| High-volume output stays memory-bounded (real fake-binary drive, not a small-sample extrapolation) | `process::tests::high_volume_output_is_memory_bounded_and_explicitly_truncated` — drives 8 MiB of real stdout through a 64 KiB cap, asserts the captured buffer never exceeds the cap, `truncated=true`, exact `bytes_dropped`, and the child still exits 0 (proving the drain-past-cap loop never deadlocks it on a full pipe). Structurally complemented by `event_sink::tests::events_beyond_the_lifetime_cap_are_dropped_and_counted_not_buffered` for the event-stream path. |
| Cancel kills descendants (grandchild, not just the direct child) | `process::tests::cancel_kills_the_whole_descendant_tree_not_only_the_direct_child` — fake binary in `spawn_child` mode backgrounds a real `sleep` grandchild, writes its pid to a file; test cancels the direct child and asserts (via `kill(pid, 0)` liveness probes, polled to a bound rather than a fixed sleep) that the grandchild is actually gone, not merely unresponsive. |
| Timeouts | `process::tests::a_process_exceeding_its_timeout_is_killed_and_reported_as_timed_out` — a `hang`-mode process outlives a 50 ms timeout, is killed, and reports `ProcessExit::TimedOut` (a distinct, typed outcome — never conflated with a normal exit code). |
| Adapters cannot cross-read each other's workspaces | Structural: `process::tests::spawn_refuses_a_working_directory_outside_its_workspace_root` (a spec whose working directory escapes its declared root is refused pre-spawn). Empirical (non-adversarial case — see limitation below): `process::tests::each_workspace_confined_process_only_ever_sees_its_own_canary_file` (two real workspaces, same-named canary file, each process sees only its own). Reinforced on the staging side by `artifact::tests::distinct_attempts_get_isolated_staging_directories`. |
| Secret canaries absent from logs/events | `process::tests::secret_canaries_never_survive_into_captured_output_or_spec_debug` (canary in env + stdin, echoed by the fake binary to both streams via `echo_canary` mode, absent from captured/scrubbed output *and* from `ProcessSpec`'s own `Debug`) and `event_sink::tests::secret_canaries_are_scrubbed_from_nested_event_payloads` (canary nested inside an array inside an object, still scrubbed). |
| Truncation is explicit | `event_sink::tests::oversized_payloads_are_explicitly_truncated_not_silently_shortened` (typed marker object, never a shortened string) plus the `truncated`/`bytes_dropped` fields asserted throughout the high-volume test above. |

Backpressure specifically (part of the card's task list, not a separate acceptance bullet) is
proven by `event_sink::tests::push_backpressure_blocks_the_producer_until_the_consumer_drains`:
with channel capacity 1, a second `push` is asserted to **not** resolve within a timeout while the
channel is full, then to resolve once the consumer drains — proving genuine blocking backpressure,
not merely a size cap on individual sends.

## Tests added and exact commands/results

- `cargo test -p tack-runner` — **94 lib + 2 CLI + 7 crash_matrix = 103 tests, 0 failures**
  (baseline was 50 lib + 2 CLI + 7 crash_matrix = 59; this card added 44 lib tests — 12 in
  `harness::tests` (`AdapterRegistry` routing/capability), 9 in `harness::process::tests`, 7 in
  `harness::event_sink::tests`, 8 in `harness::redact::tests`, 3 in `harness::sha256::tests`, 5 in
  `harness::artifact::tests`, 2 in `harness::fixtures::tests`).
- `cargo test --workspace` — **957 passed, 0 failed** (baseline 913 + 44).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.
- `git diff --check` — clean.
- `git status --porcelain` — `M crates/tack-runner/src/lib.rs`, `?? crates/tack-runner/src/harness/` only.
- Repeated `cargo test -p tack-runner --lib -- harness::` 5× at `--test-threads=8` with no
  flakes, specifically to stress the timing-sensitive cancellation/timeout/backpressure tests
  under parallel load.

## Failure/adversarial case proved

- **Symlink-check ordering bug, caught by its own test.** The first version of
  `ArtifactStager::stage_file` called `fs::symlink_metadata` on the already-`canonicalize`d path,
  which — because `canonicalize` follows symlinks — meant it was inspecting the resolved *target*,
  never the symlink itself, so a symlinked artifact source silently passed the "not a symlink"
  check. `artifact::tests::refuses_a_symlinked_source` failed immediately against this, which is
  exactly what a load-bearing test should do; the fix (check `symlink_metadata` on the
  **pre-canonicalize** candidate, matching `workspace.rs`'s existing ordering) made it pass. Left
  the failing-then-fixed sequence in this handoff as the concrete evidence the test is real.
- Cross-workspace read: proved for the non-adversarial case this layer actually defends against
  (workspace assignment is never accidentally shared/aliased — see the test table above), not for
  a harness that deliberately path-traverses via `../` once running, which would need real OS
  sandboxing (chroot/namespaces/landlock) — out of this card's scope, documented as a known
  limitation directly in the test's own doc comment, not silently narrowed.
- `AdapterRegistry` routing never crosses kinds even when the same fake process-id shape
  (`"<kind>-process"`) is used by both registered fakes on purpose, specifically to make a
  cross-routing bug (rather than merely a coincidentally-matching string) detectable.
- Every fake-binary mode documented for D1/D2/D3's future use, not only the ones this card's own
  acceptance tests happen to exercise, is proven to behave as documented
  (`every_documented_fixture_mode_behaves_as_documented`).

## Schema/API/contract change requested from another owner

None to `docs/contracts/**` (untouched, correctly — frozen, A0/D5 only). One coordinated change
flagged for D5 as described above: `engine.rs`'s `LocalRunHandle` would benefit from a
`harness_kind` field once real adapters are in place, but making it requires a paired
`crash_matrix.rs` edit only D5 can authorize.

## Known limitations or `not_measured` fields

- Event/artifact **transport** (wiring `HarnessEvent`/`StagedArtifact` onto
  `docs/contracts/runner-v1/event-batch.request.json` / `artifact.request.json` over the wire) is
  not attempted — `PullProtocol` (C3-owned, `client.rs`) has no event-batch or artifact-upload
  method yet, a pre-existing, already-documented C3 limitation. This card is the local half only:
  what an adapter accumulates before a future card wires the upload.
- Non-Unix process-group cancellation falls back to killing only the direct child
  (`tokio::process::Child::kill`), matching the existing non-Unix best-effort fallback pattern
  already used in `workspace.rs`/`journal.rs` for permissions. Documented in `process.rs`'s module
  doc, not silently narrowed.
- Cross-workspace confinement does not defend against a harness that deliberately path-traverses
  once running (needs OS sandboxing, out of scope) — see the acceptance-gate table above.
- `AdapterRegistry::capabilities()` is not wired into `client.rs`'s actual
  `EnrollmentRequest`/`RefreshRequest` construction — that wiring is outside this card's
  ownership (`client.rs` is C3-owned) and is not attempted.
- `harness::HarnessProbe` has no test against a *real* harness CLI (only the trait-level
  `FakeProbe` in `harness::tests`) — appropriate for this card (rule 8: live tests are opt-in,
  never required), and squarely D1/D2/D3's job once they exist.

## Secrets/logging review

- `ProcessSpec` has a custom `Debug` impl (mirroring `EnrollmentCredential`/`RunnerCredential`)
  that redacts `args`, `env` (via `RedactedEnv`, which prints only the key set) and `stdin`;
  proved by `secret_canaries_never_survive_into_captured_output_or_spec_debug`.
- `SecretMaterial::scrub`/`scrub_json` are applied to every captured stdout/stderr byte
  (`process.rs::finalize_capture`) and every event payload (`event_sink.rs::EventSink::push`)
  before either is retained anywhere — proved with a canary planted in credentials/env/prompt/
  stdin that a worst-case leaky fake harness actively echoes back, per the acceptance table.
- `redact_query` strips query strings from URL-shaped text; `PromptSummary` gives a `Debug`/
  `Display`-safe byte-length + truncated-SHA-256 stand-in for a prompt body, never the prompt
  itself.
- No `tracing::*!` call anywhere in this card's files passes a raw `env`/`stdin`/URL value.
- Artifact staging directories and files are created owner-only (`0o700`/`0o600` on Unix),
  matching `journal.rs`/`workspace.rs`; proved by
  `artifact::tests::staged_files_and_directories_are_owner_only`.

## Dependency needed but not added

- **`sha2`** (already a `[workspace.dependencies]` entry at `0.11`, used today by `tack-api` for
  webhook HMAC) would be the natural choice for `artifact.rs`'s SHA-256 digest. Adding it as a
  direct `tack-runner` dependency requires editing `crates/tack-runner/Cargo.toml`, which is
  B3-owned and outside this card's ownership list. Rather than stopping the card on a manifest
  request for an algorithm this small and this standard, `harness/sha256.rs` implements SHA-256
  directly (FIPS 180-4), tested against three published NIST vectors plus block-boundary edge
  cases (0/55/56/57/64/119/120/121/1000-byte inputs). If a future card would rather depend on
  `sha2` directly, swapping `sha256_hex`'s body for the crate call is a one-function change;
  nothing outside `sha256.rs` needs to know which implementation produced the digest. Putting a
  different, non-cryptographic checksum in a field the artifact contract names `sha256` was
  rejected as exactly the "hidden fake success" rule 7 forbids, so this was the only path that
  both respects the manifest boundary and keeps the field honest.
- **Process-group signalling** needed exactly one POSIX syscall (`kill(2)`). Rather than adding
  the `libc` crate (already present transitively via `tokio`, at `0.2.186` per `Cargo.lock`, but
  not directly declared by `tack-runner` — Cargo does not allow calling into an undeclared
  transitive dependency), `process.rs`'s `unix` submodule declares the single symbol via a bare
  `unsafe extern "C" { fn kill(pid: i32, sig: i32) -> i32; }` block. This is not a new dependency
  in any practical sense — `kill(2)`'s signature is part of the stable POSIX ABI already linked
  into every Unix Rust binary — and needed no manifest edit at all.
- No other new dependency was needed. `tokio`'s existing `"full"` feature set already covers
  process spawning/groups (`tokio::process`, `Command::process_group` — stabilized upstream at
  Rust 1.64 / tokio 1.21, this workspace is on tokio `1.52.3`), timeouts (`tokio::time`), and
  bounded channels (`tokio::sync::mpsc`).

## Safe merge order and likely conflicts

- No conflicts expected with D1/D2/D3: they add new files
  (`harness/{codex,claude_code,opencode}.rs`) and do not touch anything this card owns.
- Each of D1/D2/D3 will need to add its own `mod <name>;` + re-export line to
  `harness/mod.rs` to wire its file into the module tree — a shared file this card owns but did
  not lock down with a placeholder, since the files don't exist yet. **Flagging this explicitly
  for whoever dispatches D1/D2/D3 next:** either grant each of them permission for exactly that
  one mechanical line, or have D5 batch all three additions during reconciliation (D5 already
  owns "register all three without ordering behavior," which is a natural place for this). Rule 6
  is about the *trait*, not module wiring, but this card did not want to make that call
  unilaterally on another agent's dispatch.
- Merge before D5: D5's "compare three observed contracts" and "register all three" tasks are
  easiest against a tree that already has `AdapterRegistry` and the documented `LocalRunHandle`
  gap in hand, rather than rediscovering both from scratch.
- `registry.rs` was read but not touched, exactly as scoped.

## Checklist

- No unowned files: confirmed via `git status --porcelain` above.
- No live secret: `SecretMaterial`/`RedactedEnv`/`PromptSummary` audited above; canary tests pass.
- No panic stub: no `unimplemented!()`/`todo!()` anywhere in these files; every error path is a
  typed `Result` variant (`ProcessError`, `ArtifactError`, `HarnessError` via the existing
  frozen enum).
- No blind retry: `process.rs` cancellation/timeout escalate SIGTERM → SIGKILL exactly once each
  with a bounded grace wait, never loop; `event_sink.rs` never retries a full channel, it awaits
  backpressure or hits the explicit lifetime cap.
