# III-B3 handoff

- Base SHA / branch / final SHA: `f042085` / `agent/iii-b3-runner-skeleton` /
  the commit containing this handoff.
- Files changed (must equal ownership list): root `Cargo.toml`, `Cargo.lock`, and new
  `crates/tack-runner/**`; this handoff only.
- Contract fixtures consumed: none. The runner protocol fixture payloads have no Rust domain
  types in this Wave 1 skeleton, so this card exposes a typed protocol-client seam instead of
  inventing alternate DTOs.
- Behavior implemented: library/binary split; deterministic config layering (defaults → TOML
  → environment → CLI); credential redaction; injectable clock, filesystem and process
  boundaries; clonable shutdown signal with every runtime child joined; structured lifecycle
  log fields containing only runner id/time; and an empty harness registry that always returns
  `RunnerError::UnsupportedHarness`.
- Tests added and exact commands/results:
  - `cargo test -p tack-runner -q` — 8 tests passed (configuration/redaction, lifecycle,
    registry, and binary `--help`/missing-credential paths).
  - `cargo run -p tack-runner -- --help` — passed.
  - `cargo test --workspace -q` — passed with local loopback permission (the sandbox alone
    blocks five existing `wiremock` port-binding tests).
  - `cargo clippy --workspace --all-targets -- -D warnings` — passed.
  - `cargo fmt --all -- --check` — passed.
  - `cargo build --release -p tack-runner` — passed; stripped, LTO release binary is
    1,389,672 bytes (about 1.4 MiB). No prior runner binary exists for a delta.
- Failure/adversarial case proved: malformed TOML parser diagnostics are collapsed to a safe
  typed error; config debug/display redacts credentials; a missing enrollment credential fails
  before filesystem/protocol work; protocol absence and every empty-registry lookup fail
  explicitly rather than succeeding; shutdown test proves the client task completes before
  runtime return without sleeping.
- Schema/API/contract change requested from another owner: the Wave 2 runner protocol client
  should implement `RunnerProtocolClient` from the frozen fixture/domain types; no API,
  schema, or fixture change is requested by this skeleton.
- Known limitations or `not_measured` fields: no HTTP transport, journal, workspace/worktree,
  process launch/table, or real harness adapter exists yet. `SystemProcessSupervisor` is an
  explicitly empty process table until an adapter owns launch registration. There is no model
  vendor SDK dependency.
- Secrets/logging review: `EnrollmentCredential` redacts `Debug` and `Display`; parse errors
  do not retain TOML source text; main does not format CLI/config structs; logging fields are
  only `runner_id` and start time. Tests assert the credential is absent from formatting and
  missing-credential output.
- Safe merge order and likely conflicts: merge after the `f042085` Wave 0 baseline. B3 solely
  owns root workspace/dependency files through Wave 1, so later dependency requests should be
  rebased onto this commit; no conflict with B1/B2/B4 is expected outside the root lockfile.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Wave 2 dependency amendment

- Dependency commits: `c64e336` adds the path dependency on B1's `tack-orch` domain and
  `serde_json` for runner tests; `cd398eb` promotes `serde_json` to a runtime dependency for
  the durable terminal-report outbox requested by C3's crash review. The subsequent clock
  amendment adds direct `chrono` access so C3 can turn the injected `SystemTime` into the
  fixture-required RFC3339 heartbeat `sent_at` without asking C5 to fabricate runner time.
- No model/vendor SDK, network client, or provider-specific dependency was added. The lockfile
  changed only when `tack-orch` and `serde_json` first became direct runner dependencies.
- Verification after each amendment: `cargo test -p tack-runner`,
  `cargo clippy -p tack-runner --all-targets -- -D warnings`, `cargo fmt --all --check`, and
  `git diff --check` all passed.
- Merge before the C3 typed seam/outbox commits. C3 remains the source owner and must not edit
  the manifest or lockfile itself.
