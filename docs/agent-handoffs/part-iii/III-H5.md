# III-H5 handoff

**What this card changes, in plain language.** Before it, asking the system to
run a task with Claude Code or Codex could never work: the request would sit
in the queue forever, even on a machine where the tool was installed and a
runner was healthy — because the runner refused to guess which models those
tools support, and the scheduler refused to place work on a model nobody had
vouched for. Now the runner vouches for the one thing it actually knows to be
true — "whatever model name the operator writes down, I hand it to the tool
unchanged, and the tool itself says yes or no when it runs" — and the
scheduler accepts that as grounds to place the work. A made-up model name now
fails at run time with the tool's own error message instead of never starting,
which is the honest version of the failure. Proven live: a real Claude Code
run was scheduled, executed and completed on this machine.

- **The decision (this is a decision card).** Of the three candidate
  resolutions III-H2 recorded, this card implements the **pass-through
  capability attestation**. The other two were rejected on the card's own
  acceptance terms, not on taste:
  - *Operator-declared model combinations (`model_profiles`, migration 043)*
    would require an operator to seed declarations before anything schedules
    — so III-H2's step 8 could only pass by adding that seeding to the smoke,
    violating "no smoke edit". It also adds server-side plumbing for data
    nobody has, while the pass-through fact is already true in code today.
    `model_profiles` remains consulted by nothing (F4's finding stands).
  - *A verified auto-select attestation* answers the wrong question: step 8
    sends **explicit** provider/model pairs, and auto-select requests would
    still carry no verifiable claim. `AutoSelect` therefore stays rejected
    (`AutoSelectNotVerified`), unchanged.
  - Pass-through is the only claim that was **already verifiably true**: both
    adapters forward `requested_model_id` verbatim via `--model`
    (`claude_code.rs` `run_arguments`, `codex.rs` argument builder), and both
    already report the model as `requested_not_confirmed` rather than
    observed. Attesting it fakes nothing; it names existing behavior.
- **Base SHA / branch / final SHA:** base `7e06ab3` (tip of `develop`; the
  board row names `01c7046`, and `7e06ab3` is exactly that plus the board
  edit itself — no real drift), branch `agent/iii-h5-model-passthrough`.
  Final SHA: uncommitted at the time of writing — no commit was requested.
- **Files changed (all within Owns unless flagged):**
  - `crates/tack-orch/src/execution/capabilities.rs` — the new optional
    `HarnessCapability.model_passthrough: Option<CapabilityValue>` with the
    semantics in its doc comment.
  - `crates/tack-orch/src/scheduler/select.rs` — the eligibility change and
    its in-file tests' helper; `scheduler/types.rs` — doc update on
    `ModelCombinationNotDeclared`; `scheduler/batch.rs` — test-helper field.
  - `crates/tack-runner/src/harness/claude_code.rs`, `codex.rs` — probes
    attest `Supported` with reasons (D1/D2, owned by this card).
  - `docs/contracts/runner-v1/capabilities.json` — the codex harness entry
    exemplifies the field; `crates/tack-orch/tests/runner_contract/fixtures.rs`
    — re-pinned in the same change (`0x5896_310e_67a1_42b6`), as required.
  - `crates/tack-orch/tests/scheduler_test.rs` — four new tests.
  - **`crates/tack-runner/src/harness/opencode.rs` — outside Owns (D3
    lineage).** Its three probe arms attest `Unsupported` with a reason,
    because that adapter genuinely refuses undeclared models pre-spawn
    (`resolve_model`) — leaving it un-attested would have been "unknown",
    and unsupported-is-typed says the truthful value is `Unsupported`. Plus
    one assertion added to its existing probe test. Flagged, not hidden.
  - **`crates/tack-runner/src/harness/mod.rs` — outside Owns.** One field
    (`model_passthrough: None`) on the shared FakeProbe: it now stands in
    for a pre-III-H5 runner, covering the "no attestation" path. Compiler-
    forced; flagged, not hidden.

## Behavior implemented

The scheduler's explicit-model check now has two ways to pass, checked in
order: the pairing is declared in `model_combinations` (unchanged), **or**
the matched harness carries `model_passthrough` with `support: "supported"`.
Everything else is deliberately unchanged:

- `Advisory` and `Unsupported` attestations reject **identically** to no
  attestation at all, with the same `ModelCombinationNotDeclared` reason —
  an unverified claim must not schedule ("capability claims are
  load-bearing").
- `AutoSelect` still rejects every candidate (`AutoSelectNotVerified`):
  pass-through attests acceptance of an *operator-specified* model, not of an
  unspecified one.
- Pass-through does not weaken any earlier gate: the harness must still be
  declared, probe-clean, on an active, fresh, non-saturated runner.
- Old capability snapshots (no field on the wire) deserialize to `None` and
  behave exactly as before III-H5; a `None` field serializes to nothing, so
  every pre-existing fixture round-trips byte-identically.

Per adapter: claude-code and codex attest `Supported` (each reason states the
verbatim `--model` forwarding and run-time validation); opencode attests
`Unsupported` (it enumerates real combinations and refuses anything else
pre-spawn — a pass-through claim there would be false).

## Tests added and exact commands/results

- `cargo test --workspace` — **1368 passed / 0 failed / 7 ignored** (board
  baseline 1363/0 at the H3 merge; the +5 are exactly this card's new tests).
- `cargo test -p tack-orch --test scheduler_test` — 10/10, four new:
  - `undeclared_pairing_selects_when_the_harness_attests_supported_passthrough`
    — III-H2's step-8 failure as a unit test.
  - `advisory_unsupported_and_absent_passthrough_all_reject_identically`.
  - `auto_select_stays_rejected_even_with_supported_passthrough`.
  - `passthrough_does_not_bypass_probe_error_or_harness_declaration`.
- Adapter probes: new
  `probe_attests_model_passthrough_instead_of_inventing_a_model_list`
  (claude-code); assertions added to the existing codex and opencode probe
  tests. `cargo test -p tack-runner` — 244 passed / 0 failed.
- `runner_contract` 18/18 (including exact round-trip of the edited
  fixture), `wave2_gate` 5/5, `openapi_contract` 5/5 — the capability shape
  is not part of the OpenAPI surface, and `docs/openapi.json` /
  `schema.gen.ts` are untouched (verified by diff, not assumption).
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D
  warnings` clean.

### Live evidence — the acceptance criterion itself

`./scripts/smoke.sh` (fake): **SMOKE PASSED**, step 8 `PASS` for codex,
claude-code and opencode — previously exit 1 with step 8 as the only FAIL.
`SMOKE_LIVE_MODEL="opencode/deepseek-v4-flash-free" ./scripts/smoke.sh
--live`: **SMOKE PASSED**; step 8 shows `claude-code: attempt succeeded
through the full pipeline` on the real `claude` 2.1.236 binary, and the
release verdict no longer contains the "structurally unschedulable" line —
its remaining UNMET lines are III-H6's (events/artifacts) and the absent
codex binary, both correctly still surfaced. **`scripts/smoke.sh` was not
edited** (verified by diff); `SMOKE_LIVE_MODEL` is the script's own
pre-existing knob and only selects step 7's opencode pairing.

## Failure/adversarial case proved

- **Load-bearing by revert:** with only `if !declared && !passthrough`
  reverted to `if !declared`, the new step-8 unit test fails
  (`expected Selected, got NoEligibleRunner`); restored, 10/10.
- The identical-rejection test asserts the *reason variant*, not just "no
  runner": Advisory, Unsupported and absent all yield exactly
  `[(_, ModelCombinationNotDeclared { .. })]`.
- The no-bypass test proves a `Supported` attestation cannot resurrect a
  probe-errored harness (`HarnessProbeError` still wins) or a harness the
  runner never declared (`HarnessNotDeclared` still wins).

## Schema/API/contract change requested from another owner

None requested; this card owned the contract change it made (the optional
`model_passthrough` member exemplified in `capabilities.json`, pin updated in
the same change). For the integrator and later cards:

1. **Environmental find, not a code defect:** this machine's global `claude`
   wrapper was broken mid-card ("native binary not installed" — an npm
   update had dropped `@anthropic-ai/claude-code-linux-x64`). The probe
   honestly reported it and the scheduler correctly refused to place work on
   it; the only wrong artifact was step 8's canned FAIL text attributing the
   miss to the old structural cause. Repaired outside the repo with
   `npm install -g @anthropic-ai/claude-code-linux-x64` (claude 2.1.236
   restored). If a future live run FAILs step 8, check `claude --version`
   before suspecting the scheduler.
2. **Step 8's canned diagnosis is now stale** (owner: III-H2's file,
   `scripts/smoke.sh`): its "request never claimable" text still names the
   pre-H5 structural cause, but post-H5 a never-claimed request usually means
   a probe error or saturation. Not edited here — the card forbids smoke
   edits and the text only prints on failure. Worth rewording whenever the
   smoke's owner next touches the file.
3. `model_profiles` (migration 043) remains consulted by nothing — F4's
   finding, restated unchanged. If operator-declared combinations are ever
   wanted *in addition to* pass-through (e.g. to pre-validate model names for
   UX), that is a new card, not a leftover of this one.

## Known limitations or `not_measured` fields

- Pass-through eligibility means a typo'd model name schedules and then
  fails at run time with the harness's own error in `terminal_reason` —
  deliberate: that is where the truth about model validity lives. The
  request's failure is attributable (attempt exists, reason recorded), not
  silent.
- The codex leg is attested and unit-tested but not live-proven here (binary
  not installed on this machine) — same "two of three" honesty as every run
  since D-wave; the fake-mode step 8 proves its scheduling path end to end.
- The claude-code live leg ran once (billed); the live run also depends on
  this machine's opencode credentials and the free remote model chosen for
  step 7.

## Secrets/logging review

No logging added or changed anywhere in this card. The attestation reasons
are static strings (no env values, no credentials, no model lists). The
fixture's new member carries a static example reason.

## Safe merge order and likely conflicts

Disjoint from III-H4/H7 (`handlers/runner_protocol.rs` untouched) and III-H8
(no fleet/OpenAPI surface touched). **One seam with III-H6:** it owns
`engine.rs` (untouched here) but its acceptance references step 7's UNMET
line; no shared files, safe in any order. Merge whenever; nothing here waits
on the other Wave 8 cards.

## Checklist

- Unowned files edited: two, both flagged above with why
  (`harness/opencode.rs`, `harness/mod.rs` — compiler-forced field additions
  plus one truthful attestation).
- Fixture edit and pin-table update land in the same change; no other
  fixture byte moved (17 of 18 pins untouched, round-trip proven).
- No hardcoded model list, no fake declaration, no `unimplemented!()`; the
  attestation names behavior the adapters' own tests already prove.
- No smoke edit (`git diff scripts/smoke.sh` empty).
- Proposed status-board row text (integrator applies it): "III-H5 done:
  claude-code/codex schedulable via a pass-through capability attestation
  (`model_passthrough` in runner-v1 capabilities, re-pinned); scheduler
  accepts supported-attested explicit pairings, AutoSelect still refused;
  proven live (claude 2.1.236 claimed and completed, step 8 green, no smoke
  edit); workspace 1368/0."
