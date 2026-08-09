# III-B4 handoff

- Base SHA / branch / final SHA: `f042085` / `agent/iii-b4-contract-harness` / branch tip
  after rebasing the B4-only commits onto accepted B1 `22f32be`.
- Files changed: new `crates/tack-orch/tests/runner_contract.rs`, new test-only modules under
  `crates/tack-orch/tests/runner_contract/**`, and this handoff.
- Contract fixtures consumed: every JSON fixture under `docs/contracts/runner-v1/**`.
- Behavior implemented: recursive fixture parsing and byte-pinned/value/domain round-trips;
  complete lifecycle/error mutation checks; exact B1 domain conformance for capabilities,
  request/attempt snapshots, actual execution, usage and every stable error; fake time,
  single-winner claim, stale-fence and idempotent-replay drivers.
- Tests added and exact commands/results:
  - `cargo test -p tack-orch --test runner_contract`: 15 passed.
  - `cargo clippy -p tack-orch --test runner_contract -- -D warnings`: passed.
  - `cargo test -p tack-orch`: all unit, integration and legacy docket-golden tests passed;
    only the two existing doctests were ignored.
  - `rustfmt --edition 2021` on every owned Rust file and `git diff --check`: passed.
- Failure/adversarial case proved: all 40 fixture bytes are pinned so any field/state/error
  mutation fails a named test; malformed lifecycle partitions, duplicate allow/deny entries
  and terminal reopening have stable failures; exhaustive production validation covers all
  100 state pairs × four actors. Sixteen concurrent fake claimers yield one fence; expired or
  forged fences write nothing; byte-equivalent completion replay returns the original while
  conflicting idempotency reuse fails.
- Schema/API/contract change requested from another owner: none; B1 exports are required for
  the final domain-serialization seam.
- Known limitations or `not_measured` fields: race/fence/replay helpers are deterministic
  test models, not persistence; B2 owns the same proofs against the real repository.
- Secrets/logging review: helpers contain no credentials, prompt bodies, environment values
  or logging; fixture values remain the visibly invalid `example_` authority.
- Safe merge order and likely conflicts: merge accepted B1 first, then the two B4-only
  commits. B4 does not edit `lib.rs`, Cargo files, fixtures or any shared source.
- Checklist: no unowned source files, no live secret, no panic stub, no blind retry.

## Recovery-observation amendment

- Base / contract / final: this B4-only amendment starts on B1 recovery-domain
  commit `9d6da9a`; its final commit records the conformance coverage.
- Owned changes: only `crates/tack-orch/tests/runner_contract/{fixtures,domain,fakes}.rs`
  and this handoff. No shared source, contract fixture, Cargo, or TODO file changed.
- Added coverage: byte-pinned and exact B1 domain round trips for both recovery
  fixtures; every required request/response field and each new enum rejects a
  mutation; recovery dispositions are checked against the frozen lifecycle
  rules; a local fake ledger proves stable-key original-response replay,
  conflicting reuse, and no cross-test/global mutable state.
- Verification: focused B4 runner-contract test, clippy, formatting, and diff
  checks are recorded by the amendment commit.

## Accept/start fixture sync (integrator-authorized, made by A0)

- **Not a B4-card edit.** A0 (the frozen-contract owner) closed the accept/start fixture gap
  III-C2's handoff flagged (gap #5: `accept`/`start` implemented and tested against
  `lease_owner`-only `leased -> preparing -> running` per `lifecycle-transitions.json`, but with
  no frozen `docs/contracts/runner-v1/*.request.json`/`*.response.json` pair backing them) by
  adding `accept.request.json`, `accept.response.json`, `start.request.json` and
  `start.response.json`. That necessarily changed the frozen fixture manifest this card's harness
  pins — count and byte hashes both — so keeping the tree green required a same-change sync to
  `crates/tack-orch/tests/runner_contract/fixtures.rs`. A0 was explicitly authorized to make
  exactly that minimal sync and nothing else in this card's harness; this section and the mirrored
  one in `III-A0.md` record it so a later ownership audit does not mistake it for a rule-2
  violation (an unauthorized edit to another card's owned files).
- Exact diff made (by A0, in `crates/tack-orch/tests/runner_contract/fixtures.rs` only):
  - `FROZEN_FIXTURE_FNV1A64` gained four entries:
    `("accept.request.json", 0x7c41_cf4c_5a0c_50a0)` and
    `("accept.response.json", 0x9e9d_72b5_565a_783d)`, inserted before `artifact.request.json` to
    preserve the table's alphabetical ordering; `("start.request.json", 0x26b7_1d95_5b02_3895)`
    and `("start.response.json", 0x9f07_b1b7_473e_75a3)`, appended after
    `refresh.response.json` for the same reason.
  - `every_json_fixture_parses_and_value_round_trips_without_loss`'s
    `assert_eq!(paths.len(), 42, "the frozen fixture manifest changed")` became `46`.
  - No other line in `fixtures.rs`, and no line in `runner_contract.rs`, `domain.rs`,
    `lifecycle.rs` or `protocol.rs`, was touched — this card's structure, its four other test
    modules and every existing hash/assertion are unchanged.
- Verification (re-run after the sync): `cargo test -p tack-orch --test runner_contract` — 18
  passed, 0 failed. The test *count* is unchanged from the recovery-observation amendment above
  (this sync adds fixture-table rows and bumps a length literal, not a new `#[test]` function);
  what changed is that those 18 tests now run against 46 pinned fixtures instead of 42.
  `cargo fmt -p tack-orch -- --check` — clean.
- No B4-owned file other than `crates/tack-orch/tests/runner_contract/fixtures.rs` changed by this
  sync. `crates/tack-orch/src/**` was not touched (another card was concurrently mid-edit there;
  A0 read but did not modify it).
