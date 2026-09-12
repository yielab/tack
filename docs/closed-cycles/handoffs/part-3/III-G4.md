# III-G4 handoff

- **Base SHA / branch / final SHA:** base `5c6842f` (Wave 5 close-out on
  `plan/harness-agnostic-agent-fleet`); branch `agent/iii-g4-ci-release`; final SHA recorded
  by the commit that lands this handoff (see `git log -1` on the branch).

- **Files changed (must equal ownership list):**
  - `.github/workflows/ci.yml` (owned; sole owner this cycle)
  - `.github/workflows/release.yml` (packaging/release script)
  - `packaging/systemd/tack-runner.service` (new — runner service example)
  - `packaging/systemd/tack-runner.env.example` (new — companion env-file template)
  - `docs/agent-handoffs/part-iii/III-G4.md` (this file)

  No other file touched. `Cargo.lock` was deliberately **not** changed — see the
  `cargo audit` escalation below.

- **Contract fixtures consumed:** none edited. `docs/contracts/runner-v1/` (47 tracked
  files, confirmed via `git ls-files docs/contracts/runner-v1 | wc -l`) is consumed
  read-only by the CI step that runs `cargo test -p tack-orch --test runner_contract`; one
  fixture (`capabilities.json`) was deliberately mutated and reverted locally to prove the
  gate bites (below), never committed.

## Behavior implemented

**`ci.yml`** (all steps verified locally; exact commands/results under "Tests" below):

- Fixed `cargo clippy --all-targets` → `cargo clippy --workspace --all-targets` (missing
  `--workspace` meant non-workspace-root crates could carry unlinted warnings; matches the
  command CLAUDE.md already documents).
- Added explicit `set -euo pipefail` as the first line of every multi-line `run:` block, plus
  a workflow-level `defaults: run: shell: bash` so no step silently falls back to a
  non-pipefail shell.
- Added explicit, separately-named steps for what "Test" already exercises inside
  `cargo test --workspace`, so each required behavior in the III-G4 task list has its own
  visible pass/fail line rather than being buried in one aggregate job:
  - **Runner-v1 fixture drift gate** — `cargo test -p tack-orch --test runner_contract`
    (byte-pins all 46 frozen fixtures; III-G4 proved this fails on a deliberate mutation,
    see below).
  - **Runner lifecycle gate** — `cargo test -p tack-api --test wave2_gate`.
  - **Runner fake-adapter and crash-matrix tests** — `cargo test -p tack-runner` (fake-binary
    harness adapters + `crash_matrix.rs`'s spawn/ack/completion/cancellation-loss and
    ambiguous-recovery cases; never a live harness).
  - **Migration crash-recovery test** — `cargo test -p tack-db --test orch_migrations_test`.
  - **Security/chaos subset** — `trust_boundary_test`, `cors_test`,
    `board_drag_wip_race_test`, `wip_limit_race_test`, `alexa_wip_race_test`,
    `runner_vertical_slice` (all `-p tack-api`).
  - **Scheduler E2E gate (isolated)** — `cargo test -p tack-cli --test e6_scheduler_e2e_test
    -- --test-threads=1`, matching III-F6's isolated-run instruction for this
    contention-sensitive suite.
- Added a `tack-runner` coverage floor to the `coverage` job (previously had none, unlike
  every other production crate). Set to `--fail-under-lines 85` after measuring the crate
  locally at 93.66% via fake-adapter/unit tests only (live-harness tests are `--ignored` and
  excluded from a normal `cargo llvm-cov` run, so the floor never depends on a billed live
  call).
- Added a **binary-size budget** to the `embed-spa` job: it now also builds
  `tack-runner --release` and fails if either binary exceeds a budget. The budgets
  (`tack` 25 MB, `tack-runner` 5 MB) are **measured**, not copied from documentation — see
  "Known limitations" below for a doc-drift finding this surfaced.

**`release.yml`**:

- Added `set -euo pipefail` to every multi-line bash `run:` block and a workflow-level
  `defaults: run: shell: bash` (the one Windows `pwsh` step keeps its explicit
  `shell: pwsh` override, since PowerShell has its own stop-on-error semantics).
- Added a second, parallel packaging path per platform: `tack-runner` is now built with
  `cargo auditable build --release --target <target> -p tack-runner` (same auditable-build
  treatment as `tack`) and packaged into its own
  `tack-runner-<tag>-<platform>.{tar.gz,zip}` archive, containing the binary, `LICENSE`,
  a `QUICKSTART.txt`, and (Linux/macOS archives) the new
  `packaging/systemd/tack-runner.service` + `tack-runner.env.example`. Both archives per
  platform are uploaded under the existing per-platform artifact name, so the existing
  `SHA256SUMS` generation (globs every file under `dist/`), the `sbom` job (already
  `cargo cyclonedx --all`, which covers the whole workspace including `tack-runner`), and
  the `attest-build-provenance` step (already globs `dist/*.tar.gz` / `dist/*.zip`) cover
  the new runner archives automatically — no changes needed to those three mechanisms
  beyond what packaging the extra archive required.

**`packaging/systemd/tack-runner.service`** (new): an installable-but-not-applied systemd
unit template — `StateDirectory=tack-runner` (owner-only `/var/lib/tack-runner`, matching
`TACK_RUNNER_STATE_DIR`'s documented "owner-only directory" contract), `EnvironmentFile` for
the enrollment token (never inlined in the unit), `Restart=on-failure`, and a conservative
sandboxing block (`ProtectSystem=strict`, `NoNewPrivileges`, etc.) that stops short of
anything that would interfere with the runner's own harness subprocesses. Comments document
the full install sequence.

**`packaging/systemd/tack-runner.env.example`** (new): the companion env file template,
covering the four `TACK_RUNNER_*` variables from `docs/CONFIG.md`, explicitly warning never
to commit a filled-in copy.

## Tests added and exact commands/results

All run locally with `CARGO_TARGET_DIR=/home/ox/Sites/.cargo-targets/iii-g4` (mandatory
per-worktree isolation) against the unmodified worktree at base `5c6842f` plus this card's
own two workflow files (no production Rust/TS touched, so these are baseline-preservation
proofs, not new-behavior proofs):

```
cargo test --workspace
  → 1289 passed, 0 failed, 0 measured, several ignored (matches III-F6's Wave 5 close-out
    figure exactly — no regression from this card, which is expected since it touches no
    production code)

cargo fmt --all --check                                          → clean, exit 0
cargo clippy --workspace --all-targets -- -D warnings             → clean, exit 0

cargo test -p tack-orch --test runner_contract                    → 18/18 passed
cargo test -p tack-api  --test wave2_gate                         → 5/5 passed
cargo test -p tack-runner                                         → 183 (lib) + 2 (cli) + 7
                                                                      (crash_matrix) passed,
                                                                      0 failed, 3 ignored
                                                                      (live-harness, correctly
                                                                      excluded)
cargo test -p tack-db   --test orch_migrations_test                → 31/31 passed
cargo test -p tack-api  --test trust_boundary_test                 → 3/3 passed
cargo test -p tack-api  --test cors_test                           → 2/2 passed
cargo test -p tack-api  --test board_drag_wip_race_test            → 2/2 passed
cargo test -p tack-api  --test wip_limit_race_test                 → 1/1 passed
cargo test -p tack-api  --test alexa_wip_race_test                 → 1/1 passed
cargo test -p tack-api  --test runner_vertical_slice                → 7/7 passed
cargo test -p tack-cli  --test e6_scheduler_e2e_test -- --test-threads=1
                                                                     → 5/5 passed

cargo llvm-cov -p tack-runner --summary-only
  → TOTAL line coverage 93.66% (region 92.88%, function 93.93%) — basis for the 85% CI floor

cd frontend && npm run type-check      → tsc -b, exit 0
cd frontend && npx vitest run          → 724 passed / 85 files (matches Wave 5 close-out)
cd frontend && npm run lint:tokens     → 0 raw color literals, 0 inline-style hex, gate passed
cd frontend && npm run build           → succeeds; dist/ produced

cargo build -p tack-cli --release --features embed-spa
  → tack binary: 19,237,784 bytes (18.34 MB) — see "Known limitations" for the doc-drift
    this measurement surfaced
cargo build -p tack-runner --release
  → tack-runner binary: 1,898,168 bytes (1.81 MB)

actionlint .github/workflows/ci.yml .github/workflows/release.yml   → 0 findings
python3 -c "import yaml; yaml.safe_load(open(f))" for both workflow files → parses clean
```

Live-harness runner tests (`cargo test -p tack-runner -- --ignored`) were **not** run —
per instructions they are billed and must never run in CI; this card only had to confirm
they stay excluded from every gate above, which the "3 ignored" count confirms.

`make e2e` (Playwright cross-browser) was **not** run in this sandbox — this machine is
missing `libwoff2dec.so.1.0.2`, the same pre-existing, documented (III-E6/III-F6) webkit
blocker, unrelated to this branch. The CI `e2e` job itself was not changed in a way that
would affect this: it already runs `npm run test:e2e` unfiltered (all three
`playwright.config.ts` projects — chromium, firefox, webkit) on `ubuntu-latest` with
`npx playwright install --with-deps`, which installs webkit's system libraries in the GitHub
Actions image (this sandbox's missing library is a local-environment fact, not a repo
config gap) — so webkit is expected to run for real in CI, not be silently dropped. This
could only be verified in CI itself, not here.

`cargo test --workspace` for the `release.yml`'s own `test` job was covered by the
workspace-wide run above (same command, same result).

## Failure/adversarial case proved

Deliberately mutated a frozen runner-v1 fixture to prove the byte-pin drift gate actually
fails, then reverted and re-proved green — the mutation and both test runs were never
committed:

```
$ sed -i 's/"protocol_version": 1/"protocol_version": 999/' docs/contracts/runner-v1/capabilities.json
$ cargo test -p tack-orch --test runner_contract
FAILED. 15 passed; 3 failed
  - domain::opaque_model_ids_survive_punctuation_byte_for_byte
    panicked: "capability fixture must match the domain: Error(\"unsupported runner
    protocol version 999\", ...)"
  - domain::frozen_domain_fragments_round_trip_exactly
    panicked: "capabilities.json did not deserialize into the domain: unsupported runner
    protocol version 999"
  - fixtures::fixture_field_state_or_error_mutation_fails_the_frozen_manifest
    panicked: "assertion `left == right` failed: runner-v1 fixture bytes changed"
    (prints the full 46-entry hash table; only capabilities.json's hash differs)

$ cp <saved original> docs/contracts/runner-v1/capabilities.json
$ git diff --stat docs/contracts/runner-v1/capabilities.json     → (empty — byte-identical)
$ cargo test -p tack-orch --test runner_contract
ok. 18 passed; 0 failed        # green again, fixture verified byte-identical to git HEAD
```

This proves three independent things fail on the same mutation (not just a status code):
the domain deserializer, the round-trip assertion, and the hash-manifest comparison that
`cargo test -p tack-orch --test runner_contract` runs on every CI push via the "Runner-v1
fixture drift gate" step. Deliberately **not** committed as a permanent CI step that mutates
and expects failure — CI never carries a step designed to fail; this proof lives here and in
this session's shell history only.

## Schema/API/contract change requested from another owner

None. This card touched no schema, router, OpenAPI spec or generated client.

## Known limitations or `not_measured` fields

- **`cargo audit` currently fails on this branch for reasons unrelated to Part III.**
  `Cargo.lock` is unmodified from base `5c6842f` (confirmed: `git diff --stat Cargo.lock` is
  empty), yet `cargo audit` reports 3 hard failures because the RustSec advisory database
  itself updated very recently: `RUSTSEC-2026-0258` (h2, "unbounded empty DATA frames",
  published 2026-08-17 — two days before this session), and `RUSTSEC-2026-0194` /
  `RUSTSEC-2026-0195` (quick-xml, quadratic-runtime and unbounded-allocation DoS, both
  7.5/high). This means the `security` job (which this card did not create — it predates
  Wave 6) will fail on a clean checkout of this exact commit regardless of anything in this
  card, and blocks the "clean checkout passes" acceptance criterion until someone bumps
  dependencies.
  - **`h2` has a safe compatible fix**: `cargo update -p h2` (dry-run confirmed) moves
    0.4.15 → 0.4.16 with zero manifest changes. I ran it, then reverted it — the resulting
    `Cargo.lock` diff also silently downgraded five unrelated `windows-sys` entries (0.61.2
    → 0.60.2/0.52.0/0.48.0 in different subtrees), which I could not verify safe for the
    Windows release target without building it, and `Cargo.lock` is not in III-G4's `Owns`
    list. Escalating rather than landing an unreviewed, only-partially-tested lockfile
    change through a CI-only card.
  - **`quick-xml` has no compatible fix available today**: `cargo update -p quick-xml
    --dry-run` resolves 0 packages — the fix requires `object_store` (the only consumer,
    used by the S3-compatible cloud-backup client) to bump its own dependency past a range
    that currently pins quick-xml to `^0.40`. That is a real upstream-crate-version decision,
    not a patch bump, and out of this card's scope.
  - I deliberately did **not** add these three advisories to `.cargo/audit.toml`'s ignore
    list (the project's own sanctioned exception mechanism, used today for
    `RUSTSEC-2023-0071`) — I could partially reason about exploitability (h2 is pulled
    server-side via axum/hyper; quick-xml is client-side, parsing only responses from the
    operator-configured S3-compatible backup endpoint, not arbitrary attacker input) but a
    security-exception call felt like it belonged to whoever picks up the fix, not to a
    packaging/CI card working around a red gate. **This needs an explicit decision from
    G5 or a dedicated maintenance card**: either land the `h2` bump (and separately verify
    the `windows-sys` fallout doesn't affect the Windows release build) and add a dated,
    justified `.cargo/audit.toml` entry for the two `quick-xml` advisories, or accept a
    temporarily-red `security` job.
- **`CLAUDE.md`'s "~10 MB" binary-size figure is stale.** A real
  `cargo build -p tack-cli --release --features embed-spa` on this exact branch produced an
  **18.34 MB** binary (`tack-runner` alone is 1.81 MB, well under any reasonable budget).
  The gap is plausible dependency growth since Part III added `reqwest`+`rustls`,
  `object_store`/S3, `tokio-tungstenite`, etc. — not a regression introduced by this card
  (no production code was touched). I set the CI binary-size budget (25 MB / 5 MB) from the
  **measured** value with headroom rather than trusting the doc figure, but `CLAUDE.md`'s
  claim itself is now wrong and I don't own that file (III-G3 owns architecture/crate-tour
  doc updates) — recording this as a doc-correction request for G3 or G5.
- `tack-runner`'s protocol client is `UnavailableProtocolClient` in `main.rs` — a stub that
  always returns "unavailable" for every runner-protocol call
  (`crates/tack-runner/src/client.rs`). The packaged `tack-runner` binary this card ships in
  release archives therefore **builds and runs, but cannot yet perform a real
  enroll/claim/heartbeat cycle against a live Tack server** — there is no
  `RunnerProtocolClient` implementation wired into `main.rs` today. This is a pre-existing
  product gap (not something III-G4 owns or can fix — it's execution-domain wiring, not
  CI/packaging) but matters for anyone reading "packaged runner artifacts" as "a working
  runner": it is an honestly-built, honestly-non-functional-over-the-wire binary today.
  Flagging for whichever card/wave owns wiring a real HTTP client into `tack-runner`'s
  binary entry point.
- Coverage/size numbers above are point-in-time measurements from this session on this
  sandbox; CI's own runs (different hardware/toolchain patch version) may differ slightly —
  budgets were set with headroom for exactly this reason.

## Secrets/logging review

No secret is referenced by name or value in any new or edited file. Grepped both workflow
files and both new `packaging/systemd/*` files for `secrets.`, `token`, `password`,
`credential` — the only matches are: (a) GitHub Actions' own built-in `permissions:` grants
(`contents: write`, `id-token: write`, `attestations: write` — not user secrets, and
pre-existing), and (b) the *names* `TACK_RUNNER_ENROLLMENT_TOKEN` /
`enrollment_credential` in the new systemd env-file template and its comments, which
document where an operator puts their own secret — no value is present, and the template
file itself is explicitly commented "never commit a filled-in copy." No required CI job
reads a `secrets.*` context anywhere in either workflow (confirmed via
`grep -n "secrets\." .github/workflows/ci.yml .github/workflows/release.yml` → no output).

## Safe merge order and likely conflicts

- This card's diff is additive/mechanical (workflow YAML + two new packaging files) and
  touches no production Rust/TS, so it should merge cleanly against any of III-G1/G2/G3's
  branches, none of which touch `.github/workflows/**` or `packaging/**` per III.3's
  shared-file ownership table (`.github/workflows/ci.yml` → G4 only this wave).
- **Land after G1–G3** land their own product/doc changes if any of them touch code this
  card's new explicit CI steps exercise (unlikely, since G1/G2/G3 own legacy-bridge,
  chaos-audit, and docs respectively — not production behavior the runner-contract/wave2/
  security-subset tests assert on) — but there is no ordering *requirement* from this card's
  side; it can land first or last.
- The only real conflict risk is with a hypothetical concurrent CI edit, which III.3
  structurally forbids (single owner this wave).

## Proposed status-board row text (G5 to apply, not applied here)

> III-G4 — CI, packaging and release gates: complete on branch `agent/iii-g4-ci-release`
> (base `5c6842f`, final `<commit-sha>`). `ci.yml` gained explicit runner/fixture-drift/
> migration-crash/security-chaos/isolated-scheduler steps (previously only implicit inside
> `cargo test --workspace`), a `tack-runner` coverage floor (85%, measured 93.66%), and a
> measured binary-size budget for both `tack` (25 MB, measured 18.34 MB) and `tack-runner`
> (5 MB, measured 1.81 MB). Every multi-line shell step in both workflows now runs under
> explicit `set -euo pipefail`. `release.yml` now packages `tack-runner` alongside `tack`
> per platform (auditable build, checksummed, SBOM'd and provenance-attested via the
> existing wildcard globs — no new SBOM/checksum/provenance machinery needed) plus a new
> `packaging/systemd/tack-runner.service` + env-file template. A deliberate mutation of
> `docs/contracts/runner-v1/capabilities.json` was proved to fail the fixture-drift gate
> (3 independent test failures) and reverted clean (see handoff). **Two escalations block
> full "clean checkout passes" acceptance and need a decision from G5 or a dedicated
> maintenance card**: (1) `cargo audit` currently fails on unmodified `Cargo.lock` due to
> RustSec advisories published within the last two days against `h2`/`quick-xml` — a safe
> `h2` patch exists but had an unreviewed `windows-sys` side effect; `quick-xml` has no
> compatible fix without an `object_store` bump; (2) `CLAUDE.md`'s "~10 MB" binary-size
> claim is stale (measured 18.34 MB) — a doc-correction request for G3. `tack-runner`'s
> shipped binary has no real network protocol client wired into `main.rs`
> (`UnavailableProtocolClient` stub) — pre-existing, not this card's to fix, flagged for
> whichever card owns that wiring.

## Checklist

- [x] No unowned files — diff is exactly `.github/workflows/ci.yml`,
      `.github/workflows/release.yml`, two new files under `packaging/systemd/`, and this
      handoff.
- [x] No live secret — verified by grep; no required job reads `secrets.*`.
- [x] No panic stub — no production Rust/TS touched; the systemd unit and env template
      contain no executable stub logic.
- [x] No blind retry — `Restart=on-failure` in the systemd unit is a supervised process
      restart (standard for a long-lived service), not a masked-failure retry loop; no CI
      step retries a failing gate.
