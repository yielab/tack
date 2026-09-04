# VI-B1 handoff

- Base SHA / branch / final SHA: base `02aa4e3` (the develop tip the dispatch README named);
  branch `agent/vi-b1-secret-store`; not committed (final SHA n/a — working tree only, per
  instruction not to commit).
- Files changed (must equal ownership list): see "Files changed vs. ownership" below —
  four files beyond the literal Owns wording were necessary to wire the store in; each is
  justified there.
- Contract fixtures consumed: `docs/contracts/runner-v1/claim.response.json` (read only, to
  confirm `EnvironmentValue{value, secret_reference}`'s shape); none edited.
- Behavior implemented: a runner-local `SecretStore` (keychain-first, owner-only-file
  fallback), wired into all three harness adapters' `validate`/`start` so a
  `secret_reference` environment entry actually resolves; `tack runner secret set|list|remove`;
  `tack runner doctor` reports which backend answered.
- Tests added and exact commands/results: see "Test results" below.
- Failure/adversarial case proved: a `secret_reference` the store cannot resolve fails
  `validate` with a typed, name-only reason and touches neither the store's state directory
  nor the workspace it was given (`validate_rejects_a_missing_secret_reference_typed_and_touches_nothing`,
  `crates/tack-runner/src/harness/claude_code.rs`). The reverted-fix proof is recorded below.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see "Not checked" below — the biggest one is a
  pre-existing, repo-wide MSRV break unrelated to this card (see "Escalation: MSRV" — read
  this section first).
- Secrets/logging review: see "Secret-path proof" below.
- Safe merge order and likely conflicts: this is Wave 15's first card; B2 and B3 both need
  it merged first (per the dispatch README). No file in this diff is claimed by any other
  Wave 14/15 card. `doctor.rs` gets a *second* addition from VI-B2 later (the gateway
  provider block) in a different, disjoint section of the same function — no overlap with
  what this card added.
- Checklist: no unowned files edited without justification below, no live secret
  committed, no panic stub, no blind retry.

## Read this first: two escalations

### Escalation 1 — a pre-existing, repo-wide MSRV break (not caused by this card)

While confirming `keyring` 4.x's actual `rust-version` (as instructed, via `cargo info`
rather than memory), every current secret-service/zbus crate turned out to declare
`rust-version` above the workspace's stated floor (1.85) and above the pinned MSRV CI job
(`dtolnay/rust-toolchain@1.85.0`). To find out whether that was actually load-bearing (a
declared `rust-version` field doesn't by itself prove a build fails), I installed rustc
1.85.0 via rustup and tried real builds, twice:

1. **This branch**, `cargo +1.85.0 build -p tack-runner --lib`: fails at dependency
   resolution — `aes@0.9.3 requires rustc 1.89`, plus several `zbus`/`secret-service` crates
   at 1.87–1.88. This is the cost of this card's dependency choice, and it is real.
2. **The unmodified base** (`git worktree add` at `02aa4e3`, no VI-B1 changes at all),
   same command against `tack-runner` (which doesn't even depend on `keyring-core`
   there): **also fails to resolve on rustc 1.85.0** — `serde_with@3.22.0 requires
   rustc 1.88`, plus `icu_collections`/`icu_locale_core`/`icu_normalizer`/`icu_properties`/
   `icu_provider`/`idna_adapter` all requiring rustc 1.86. `serde_with` is an existing
   workspace dependency this card never touched; its lockfile-pinned version is identical
   on both trees.

So: **the `develop` line at `02aa4e3` already cannot build under the CI-pinned MSRV
toolchain, independent of anything in this card.** This card's own additions raise the
floor further (1.88 → 1.89 by way of `aes`), but they are not the reason the floor was
already broken. Given the very recent, dedicated fix in this repo's history for exactly
this class of problem ("revert Dependabot's accidental MSRV-floor bump, stop it
recurring"), this needs attention before Wave 15 lands, not after. I did not touch
`Cargo.toml`'s `rust-version` field, the MSRV CI job, or any dependency outside
`tack-runner`/`tack-cli` — reconciling the floor (bump it deliberately with a decision
record, or pin the offending transitive crates down) is a workspace-wide call outside a
secret-store card's ownership.

Cleanup note: the temporary worktree used for step 2 was removed
(`git worktree remove --force`) after the check; nothing about it persists. The rustc
1.85.0 toolchain itself is still installed via rustup on this machine (harmless, and
useful for whoever picks this finding up).

### Escalation 2 — the "before any journal record or worktree exists" premise is false today

The card's Design paragraph says secret resolution must happen "in the adapter's
`validate` step — before any journal record or worktree exists." I read
`crates/tack-runner/src/engine.rs::run_claimed` (lines 312–346) to place the resolver
correctly, and found the opposite ordering already exists there, on purpose:

```
plan workspace (322) → journal.persist_before_spawn (326, fsync'd)
  → report_start (327–339) → workspaces.provision (340, real git checkout)
  → adapter.validate (346)
```

The comment at that call site is explicit: *"This is the hard ownership boundary: no
worktree or adapter method with a local side effect is called before create+fsync
succeeds."* By the time `validate()` — and therefore secret resolution — runs, the
journal record is already persisted and the workspace is already a real checkout on
disk. `HarnessAdapter::validate` is the *only* production call site
(`grep -rn "\.validate(&spec)"` across `crates/tack-runner/src`), so this isn't an edge
case.

Per the card's own stop condition, I did not move the journal write or reorder
`run_claimed` — that ordering is a documented crash-safety invariant well outside a
secret-store card's scope. What this means concretely: the Acceptance line "a missing
reference fails at `validate`... and the state directory and workspace root are asserted
untouched" is true at the *adapter* boundary I own (`validate` itself performs no
filesystem write of its own, and a rejection there leaves whatever the engine had
already done exactly as it was) — not as a claim that nothing in the whole pipeline was
touched before the failure. My test proves the narrower, true claim; see "Test results."

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A live attempt with `{"secret_reference": "demo"}` reaches the spawned process with the variable set, and only the value's length is ever observable | `cargo test -p tack-runner --lib secret_reference_resolves_and_only_its_length_reaches_the_shim` — passes; reverted-fix run below shows it is load-bearing |
| Captured `tracing` output for that run names the entry and never the value | same test; asserts `captured.contains("demo") \|\| captured.contains("SECRET_VAR")` and `!captured.contains(secret_value)` |
| A missing `secret_reference` fails at `validate`, typed, naming only the reference | `cargo test -p tack-runner --lib validate_rejects_a_missing_secret_reference_typed_and_touches_nothing` — passes |
| The file-fallback store is `0600` on a real run | `stat -c '%a'` on a real `tack runner secret set` run under `DBUS_SESSION_BUS_ADDRESS=/dev/null` → `600` (see "Secret-path proof") |
| The three adapters' resolution is genuinely load-bearing, not a no-op that happens to pass | reverted `resolve_environment`'s `secret_reference` arm to a no-op once; both new tests failed with the exact expected symptom (length `0` instead of `23`; missing-reference test's `expect_err` panicked because resolution silently "succeeded"); restored; full suite green again |
| The keychain backend is live on this dev machine | `secret-tool lookup service tack-runner username <name>` — see "Secret-path proof" for command/exit-status only |
| The file fallback engages when no platform store answers | `DBUS_SESSION_BUS_ADDRESS=/dev/null tack runner doctor` → `backend: file` (measured) |
| `env:` references resolve the same way `store:` ones do | `secrets::tests::resolve_env_scheme_reads_the_process_environment` |
| `runner_contract` is byte-identical (no fixture edited) | `cargo test -p tack-orch --test runner_contract` — 18 passed; `git diff --name-only -- docs/contracts/ crates/tack-orch/tests/runner_contract.rs` → 0 files |
| `cargo audit` gained no new findings from the new dependency tree | measured against both the current tree and a scratch checkout of the unmodified `02aa4e3` lockfile — identical 5 warnings, both pre-existing (see "Not checked") |

A row with no evidence is a claim to delete, not a row to leave blank — every row above
was actually run this session, not assumed.

## Measured numbers

- `cargo test -p tack-runner` (lib + all integration test binaries): **243 lib tests
  passed, 0 failed, 3 ignored** (the 3 ignored are the pre-existing opt-in live-harness
  tests, untouched by this card); `bootstrap_entrypoint` 2/2, `cli` 2/2, `crash_matrix`
  7/7, `g2_journal_corruption_test` 3/3, `h3_checkout` 6/6 — all passing, all pre-existing.
- `cargo test -p tack-cli`: unit 19/19, `cli_test` 11/11, `e6_scheduler_e2e_test` 5/5,
  the crate's own `secure_fs`/`mcp`/`client` unit modules 57/57 — all passing.
- `cargo test -p tack-orch --test runner_contract`: **18/18**, byte-identical (no fixture
  file touched).
- `cargo fmt --check` (workspace): 1 diff, in `crates/tack-api/tests/trust_boundary_test.rs`
  — a file this card never opened or edited; pre-existing drift, not caused here.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean, 0 warnings.
- `cargo audit`: 0 vulnerabilities (errors) on this tree; 5 allowed warnings
  (`proc-macro-error2` unmaintained, `event-listener` unsound-in-narrow-case, `chacha20`
  0.10.0 yanked, `spin` 0.9.8 and 0.10.0 yanked) — **all 5 are present on the unmodified
  `02aa4e3` lockfile too**, verified in a scratch worktree; this card added none.
  `02aa4e3`'s lockfile additionally reports 1 vulnerability (`rsa` 0.9.10, RUSTSEC-2023-0071,
  medium) that this repo's checked-in `.cargo/audit.toml` already allow-lists — also
  pre-existing, unrelated to this card.
- Cargo.lock: 51 new crates locked (the `keyring-core` + `zbus-secret-service-keyring-store`
  transitive closure — D-Bus/zbus, a pure-Rust AEAD stack for the Secret Service session
  cipher, and their supporting crates).
- `git diff --stat` (tracked files): 11 files, 1162 insertions(+), 89 deletions(-); 2 new
  files (`crates/tack-runner/src/secrets.rs`, `crates/tack-cli/src/secret.rs`).

## What a stranger still cannot do

Nothing yet routes a provider key from a UI into this store — `tack runner secret set`
is a console-only command today (by design; VI-B3 is the UI write-once route). A stranger
who has read this card's existence still cannot authenticate Vercel AI Gateway from the
Agents page; that needs VI-B2 (reads from this store) and VI-B3 (writes to it from a
loopback-only UI route) on top of what landed here.

## Surface-map delta

None. §VI.0's surface map's "Authenticate through Vercel AI Gateway" row already names
`tack runner secret set` as the console path for a remote runner — this card is what
makes that command real (it did not exist before), not a row this card itself moves. The
UI-side movement of that row is VI-B3's.

## Files changed vs. ownership

Owned, as listed: `crates/tack-runner/src/secrets.rs` (new); the store path in
`crates/tack-runner/src/config.rs` (`RunnerConfig::secret_store_path`); the
`secret_reference` branch in `claude_code.rs`/`codex.rs`/`opencode.rs`;
`tack runner secret set|list|remove` (`crates/tack-cli/src/main.rs`'s new `RunnerAction::Secret`
arm + `crates/tack-cli/src/secret.rs`, new); the dependency line(s) in
`crates/tack-runner/Cargo.toml` and the `Cargo.lock` they move.

Beyond the literal wording, four files were unavoidable to actually wire the store in,
each recorded here rather than silently expanded:

- `crates/tack-runner/src/harness/mod.rs` — the shared `resolve_environment` function.
  The card's own Context frames this as "one mechanism, three callers"; putting it once in
  the module all three adapters already `use super::{...}` from (rather than tripling the
  match arm) is that same mechanism, not new scope.
- `crates/tack-runner/src/bootstrap.rs` — the read list said not to open this file, but
  constructing a `SecretStore` and threading it into every adapter's `discover(...)` call
  (production wiring lives here, nowhere else) cannot be done blind. I read it in ranges
  only (the two functions that construct adapters), added a `secrets: &SecretStore`
  parameter to `probe`/`build_adapter_registry`, and one new field on `DiscoveryReport`
  (`secret_backend`). Nothing else in the file was touched.
- `crates/tack-runner/src/lib.rs` — two lines: `pub mod secrets;` and the matching
  `pub use`, mirroring how `config`'s public types are already re-exported.
- `crates/tack-cli/src/doctor.rs` — the Acceptance list requires `doctor` to report the
  backend; a small, clearly-labeled block prints it, using the same `RunnerConfig`
  resolution the console `runner start` path uses (so `--state-dir`/`TACK_RUNNER_STATE_DIR`
  behave identically in `doctor` and in a live runner). VI-B2 later adds a *different*
  section to this same function (the gateway provider block) — disjoint from what this
  card added.

## Design deviation, and why: `keyring-core` + a store crate, not `keyring`

The card and ADR name the `keyring` crate 4.x. Following the instruction to confirm with
`cargo info`/`cargo doc` rather than memory, `keyring` 4.2.0's own module docs say
plainly: applications that want to choose their own store-selection fallback order, or
substitute a test double, "should not be linking to this library at all; they should
instead be linking to the keyring-core library and any specific credential stores." That
is exactly this card's situation — try the platform store, catch failure, fall back to a
file; and use `keyring_core::mock::Store` in unit tests rather than a real Secret
Service. `keyring`'s own `v1::Entry` shim hardcodes its own store-selection logic behind
a process-wide `LazyLock` with no hook to intercept "which store, and what happens on
failure," which is precisely the decision this card needs to make itself.

So: `crates/tack-runner/Cargo.toml` depends on `keyring-core = "1.0.0"` directly, plus the
same per-platform store crates `keyring`'s own manifest would have selected — target-gated
exactly like `keyring`'s manifest does, so cross-compiling for another OS is unaffected:
`apple-native-keyring-store` (macOS), `windows-native-keyring-store` (Windows),
`zbus-secret-service-keyring-store` with its `crypto-rust` feature (Linux/BSD Secret
Service, pure-Rust cipher, no system OpenSSL dependency). `linux-keyutils-keyring-store`
(the kernel keyring, ephemeral) is deliberately not used — the ADR asks for the
*persistent* platform store, and the kernel keyring doesn't survive a reboot.

This is where the MSRV floor in Escalation 1 comes from: `zbus-secret-service-keyring-store`
and `windows-native-keyring-store` both declare `rust-version 1.88`; `keyring-core`,
`apple-native-keyring-store` and `linux-keyutils-keyring-store` all declare `1.85` (fine).
There is no lower-MSRV alternative in the same architecture generation — an older
`dbus-secret-service-keyring-store` (0.3.3) exists on crates.io but targets the pre-1.0
`keyring-core` generation and cannot compose with the rest of this dependency tree.

## Secret-path proof

- **Redaction, positive control**: `secrets::tests::debug_and_display_never_print_the_secret_value`
  proves `SecretValue`'s `Debug`/`Display` are hardcoded `[REDACTED]`, mirroring
  `client::RunnerCredential` exactly.
- **Captured log output, name present / value absent**: see "Claim → evidence" row above;
  the assertion is a positive control (it fails if resolution stops logging the entry).
- **`stat -c '%a'` on a real run**: seeded `TACK_RUNNER_STATE_DIR` to a scratch directory,
  forced the file backend with `DBUS_SESSION_BUS_ADDRESS=/dev/null`, ran
  `tack runner secret set <name>` reading the value from stdin, then
  `stat -c '%a' <state_dir>/secrets.json` → **`600`**. Also asserted programmatically:
  `secrets::tests::file_backend_writes_the_secrets_file_owner_only`.
- **Keychain backend, live**: with a real `gnome-keyring-daemon` on this machine's D-Bus
  session bus, `tack runner secret set vi-b1-proof` (value from stdin) reported
  `stored "vi-b1-proof" in the keychain backend`; `tack runner doctor` reported
  `backend: keychain`. Verification command and exit status only, value never recorded:
  `secret-tool lookup service tack-runner username vi-b1-proof` → **exit 0** (found).
  Note the attribute key is `username`, not `account` — the card's prose used "account"
  descriptively; the actual Secret Service attribute this store writes (and every
  `zbus-secret-service-keyring-store` entry writes) is `username`, confirmed by reading
  `cred.rs` in that crate. After `tack runner secret remove vi-b1-proof`, the same lookup
  → **exit 1** (not found) — removal proven, not assumed.
- **Fallback, live**: same commands with `DBUS_SESSION_BUS_ADDRESS=/dev/null` →
  `tack runner doctor` reported `backend: file`; `tack runner secret set`/`list`/`remove`
  round-tripped correctly against the file backend. Scratch state directories were
  removed after each proof; nothing persists on this machine.
- **`sqlite3 tack.db .dump | grep -c`**: not applicable to this card — this store never
  touches `tack.db` at all (that is the entire point of ADR 0061 decision 1, rejecting the
  database as a location); there is nothing to grep for.

## Not checked

- **macOS/Windows code paths are compile-checked only via `cfg` gates, never actually
  built.** Every CI job in `.github/workflows/ci.yml` runs on `ubuntu-latest`; this repo
  has no macOS/Windows runner and I have no such machine. `apple-native-keyring-store`
  and `windows-native-keyring-store` are pulled only on their respective target OS
  (mirroring `keyring`'s own manifest structure), so their code paths inside
  `secrets.rs::platform_store` have never been compiled by anyone, on this branch or any
  other, until someone builds for those targets.
- **Release-build binary size impact of the new dependency tree is not measured.** CLAUDE.md
  asks to use `--release` sparingly (slow, size-optimized profile); I did not build it for
  this card. The 51 new locked crates (D-Bus/zbus stack) are a real, unmeasured size cost
  worth checking before the release pipeline runs for this cycle.
- **`Entry::search`/`get_specifiers`-based `list()` on a real keychain (not the mock) is
  untested beyond the manual `tack runner secret list` run above.** The unit tests exercise
  it against `keyring_core::mock::Store`, which is a faithful implementation of the same
  trait the real backend uses, but I did not write an automated test driving `list()`
  against the live gnome-keyring session (only `set`/`get`/`remove` were proven live, via
  the CLI and `secret-tool`).
- **Windows Credential Manager and macOS Keychain attribute-listing behavior for `list()`**
  is assumed consistent with the Secret Service backend (same `keyring-core` trait,
  `get_specifiers()` is documented store-agnostic) but not verified against either, for
  the same no-runner reason above.

## Context spent

- Cold start: read the dispatch README header + VI-B1 block, ADR 0061 decisions 1 and 4,
  the TODO.md §VI.0/§VI.1/§VI.2 prelude and the VI-B1 card body, the contract's
  `EnvironmentValue` shape, the three adapters' named environment-builder ranges,
  `harness/mod.rs`'s trait definitions, `config.rs` whole, `doctor.rs`'s head, and
  `scripts/smoke.sh`'s shim references — in line with the block's ≈19k estimate.
- Files opened beyond the read list, and why: `bootstrap.rs` (whole, in two range-reads;
  the block said not to — see "Files changed vs. ownership" for why it was unavoidable);
  `client.rs` and `transport.rs` (targeted greps only, to find `RunnerCredential`'s
  redaction pattern and `write_owner_only`'s atomic-write-then-rename technique to mirror
  in `secrets.rs`) — not named by the block but small, targeted, and directly reused
  patterns rather than new invention; `.github/workflows/ci.yml` (grepped, to find the
  MSRV job's exact pinned toolchain once the MSRV question came up) — not part of the
  card's read list at all, opened only because Escalation 1 required knowing the exact CI
  contract.
- Read-list items not used: `scripts/smoke.sh`'s shim references were read but not
  reused directly — the acceptance test's shim is a one-off script matching
  `harness::mod::tests::cross_adapter_fixture_command`'s existing technique instead,
  since the shared fixture command doesn't have a mode for "echo one env var's length to
  a file I name."
- This session ran long — deep, measured verification of the `keyring` ecosystem (crate
  APIs, attribute names, MSRV via two real toolchain installs and two real builds) rather
  than the ~19k-token estimate, because the card's own instructions required confirming
  rather than assuming every one of those facts, and the MSRV question turned out to have
  a much bigger, unrelated answer worth chasing to ground. Exact token accounting wasn't
  captured mid-session; the two extra real builds (one on this branch, one on a scratch
  worktree of `02aa4e3`) were the largest single cost beyond ordinary code-writing.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

### 2026-09-04 — Wave 15 integrator: the secret store costs 4.4 MB of runner binary

CI's binary-size gate failed on the first push after this card landed:

```
tack-runner binary: 6200200 bytes (5.9 MB), budget 5242880 (5 MB)
```

The runner measured 1.81 MB at III-G4. Resolving a `secret_reference` from the OS
keychain pulls secret-service, zbus and a crypto stack — 12 crates that were not in this
binary before — and more than tripled it. The card did not measure this, and the gate is
the only thing that would have caught it.

Nothing here is wrong: the keychain is ADR 0061 decision 1, accepted, and 5.9 MB is still
a small binary. The budget is re-baselined to 8 MB, keeping the same ~35% headroom rule
the gate's own comment states, and the cause is recorded there rather than left as an
unexplained number.

Worth knowing for VI-B2 and anyone sizing the runner for a constrained host: the lever
is making the keychain backend a default cargo feature that such a build turns off. The
file fallback already exists and needs none of those crates. That is a decision for a
card with a caller, not a mechanism to build on spec.
