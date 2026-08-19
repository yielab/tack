# III-H2 handoff — live smoke and release verdict

- **Base SHA / branch / final SHA:** base `e4f6362` (tip of `develop`, the single
  integration line the board names), branch `agent/iii-h2-live-smoke`. Final SHA:
  uncommitted at the time of writing — the working tree is the deliverable; no commit
  was made because none was requested.
- **Files changed (must equal ownership list):**
  - `scripts/smoke.sh` — owned; the whole card. Steps 7–9 implemented (they were
    unconditional `SKIPPED` stubs that could never fail — the false green the Wave 7
    board row warned about).
  - `docs/agent-handoffs/part-iii/III-H2.md` — this file, owned.
  - Nothing else. `git diff --name-only develop` returns exactly `scripts/smoke.sh`.
- **Contract fixtures consumed:** none edited, none pinned differently.
  `enrollment.request.json` was used as a curl body while isolating the
  duplicate-name enrollment defect (escalation 2); `lifecycle-transitions.json`
  supplied the terminal-state vocabulary the polling asserts against.

## The release verdict — do NOT tag

This card owns the tag and refuses it, on the same standard III-G5 used. Two
criteria of §III.6 are structurally unmet and one of them was invisible until this
card ran the pipeline live:

**New P0 — Claude Code and Codex executions can never be scheduled on a real
runner.** Three individually-principled decisions compose into an impossible
product: (1) the claude-code and codex adapters' probes deliberately declare zero
`model_combinations` (D2: "no list-models command; model availability is only
observable via a billed invocation"; D1: "never hardcodes a model list"), (2) the
scheduler requires the requested provider/model pairing to be **declared** by the
candidate's capability snapshot (`crates/tack-orch/src/scheduler/select.rs`,
`ModelCombinationNotDeclared`), and (3) `ModelSelector::AutoSelect` is rejected for
every candidate (`AutoSelectNotVerified`). So with a real `claude` 2.1.236 installed
and an enrolled, heartbeating runner, a claude-code request sits `queued` forever —
proven live in step 8, in both modes, on every run. Only opencode (whose CLI lists
models) is schedulable. §III.6's first sentence — attempts through Codex, Claude
Code **and** OpenCode — is therefore not just unverified here, it is currently
impossible on any machine. The board believed the smoke was collectible once H1+H3
landed; that was a completeness claim nobody had verified, for the second time in
this wave.

**Still unmet, carried:** the runner never submits events, decisions or artifacts
(`AttemptDataProtocol` has transport since III-H1 but zero `engine.rs` call sites —
H1's escalation 3, still open), so "verified artifacts and an idempotent event
timeline" and "resolve a bounded decision" cannot be demonstrated from a real
runner; codex is absent on this machine (2 of 3 binaries, never rounded up);
III-H4 (401-vs-409 rotation race) is open and the board says settle it before
tagging; `cargo audit` still fails on the integrated tree (G4's deliberately
unlanded bump) and webkit remains unverifiable; backup/restore-with-artifacts
evidence stays uncollectible while no artifact ever leaves a runner.

## Behavior implemented

`scripts/smoke.sh` steps 7–9, real in both modes, plus supporting changes:

- **Step 7 — full lifecycle through production routes.** Creates a real git repo at
  a pinned commit, an agent profile, and an execution request whose pairing is read
  from the runner's *own* capability declaration (what the scheduler actually
  checks). Asserts: attempt `succeeded`, `base_revision` equals the requested
  commit exactly, a workspace id was provisioned, the request's own state
  propagates to `succeeded`, and the event timeline — which is empty by a known
  product gap — is reported as UNMET, never as PASS.
- **Step 8 — the same neutral request per harness kind.** opencode must succeed
  end-to-end; a present-but-unschedulable harness is a FAIL naming the exact
  mechanism; an absent binary is ABSENT and an UNMET line, never a FAIL and never
  a PASS.
- **Step 9 — restart recovery.** A dedicated second runner on shim binaries (in
  both modes — kill tests must not burn billed runs) takes a deliberately hanging
  attempt; while it holds the lease, a second request proves capacity 1 is
  respected; runner and harness are SIGKILLed mid-attempt; the restarted runner
  reports the ambiguity and the attempt lands `needs_operator` (no silent loss);
  harness run-markers and the server's attempt count prove no blind duplicate; an
  explicit operator requeue then succeeds as attempt #2, and the queued request
  completes once capacity frees.
- **Shim harness binaries** (`opencode`/`claude`/`codex`, generated per run) answer
  the adapters' real probes and treat any other invocation as a run: record a
  pid marker (hang-runs marked distinctly, so concurrent legitimate runs can never
  pollute the duplicate count), drain stdin, honor `SMOKE_HANG` from the execution
  request's own `environment` field (which proves that plumbing live) until a
  release file appears, exit 0. Fake mode runs the full production pipeline —
  server, scheduler, runner, git provisioner, adapter, subprocess — with only the
  harness binary faked.
- **Honest verdict block.** Exit code = step integrity (0 only when every runnable
  step held). A separate `RELEASE VERDICT` section lists every §III.6 criterion the
  run could not demonstrate; nothing is silently skipped and nothing prints
  `SMOKE PASSED` while proving less than it claims.
- Housekeeping: distinct `TACK_RUNNER_ID` per runner (see escalation 2), `SMOKE_KEEP=1`
  keeps the work dir for debugging, cleanup kills runners, shim children (by
  recorded pid — they live in their own sessions) and the server.

## Tests added and exact commands/results

The smoke script is the test. Final runs on this tree:

- `./scripts/smoke.sh` (fake mode): exit 1 — steps 1–7 and 9 fully PASS
  (7: attempt succeeded at the exact requested commit, request state propagated;
  9: needs_operator recovery, no blind duplicate, capacity, requeue → attempt #2
  succeeded). Step 8: opencode PASS; codex and claude-code FAIL on the structural
  unschedulability. The failure is the finding, exactly as the pre-III-H1 script
  failed on the transport P0.
- `./scripts/smoke.sh --live`: exit 1 — same shape. Step 7 ran **real opencode
  1.18.0 with `llamacpp/qwen3.6-35b-uncensored`** (local llama-server; unbilled):
  attempt `att_2520a0b2-…` succeeded, fencing token 1, commit `449e1899…` verified.
  Step 8: codex ABSENT; claude-code (real binary 2.1.236 installed and probed)
  FAIL — never claimable; opencode PASS. Step 9 all PASS. No billed invocation
  occurred anywhere: the only harness that can be scheduled is opencode, and it ran
  against the local model.
- `shellcheck -S warning scripts/smoke.sh`: clean. `bash -n`: clean.
- No Rust/TS file changed, so no compiled gate moved: workspace baseline stays the
  board's 1363/0 at the H3 merge; not re-run for a shell-only diff.

Full ANSI-stripped transcripts of both final runs are appended at the bottom of
this file as the release evidence.

## Failure/adversarial case proved

- **The new steps can fail, observed not assumed.** During development each new
  assertion failed at least once for a real cause before its final green: step 9's
  runner never enrolled (surfaced escalation 2, the duplicate-name 500), the
  duplicate-execution counter fired on a legitimate concurrent run (fixed by
  hang-marked markers plus the server-side attempt count), and the capacity check
  was shown to pass vacuously when the lease was never held (now guarded — it
  reports "unusable" instead of PASS).
- **Step 8's FAIL is load-bearing today**: the script exits 1 on the current tree
  in both modes. It stops failing only when a claude-code/codex request is actually
  claimed and completed — the precise condition the release needs.
- **Absence asserted directly:** no-blind-duplicate is proven by counting harness
  process markers on disk *and* the attempt count on the server, not by a status
  code; capacity is proven by the absence of a second attempt while the lease is
  live.

## Schema/API/contract change requested from another owner

1. **P0, release-blocking, decision needed — make claude-code/codex schedulable
   without faking capability.** Owners: the scheduler (E1/E6 lineage), the two
   adapters (D1/D2), and the capabilities contract. The pieces cannot all stay as
   they are; candidate resolutions to choose between, not decided here: a
   capability attestation that a harness accepts operator-specified opaque models
   (pass-through) as `capabilities.json` data; or operator-declared model
   combinations feeding eligibility (`model_profiles`, migration 043, is consulted
   by nothing today — F4 recorded the same); or a verified auto-select attestation.
   Bending any single piece silently would violate "capability claims are
   load-bearing", which is why this is escalated rather than patched.
2. **Duplicate `runner_name` enrollment returns an unhandled 500.** Owner:
   `crates/tack-api/src/handlers/runner_protocol.rs`. Reproduced with two curl
   enrollments differing only in token: first 200, second 500 (server log shows the
   route failing, ids only). Two defects in one: the collision deserves a typed
   protocol error (the fixture set has `conflict`), and the enroll body's
   self-reported `runner_name` (defaulted from `TACK_RUNNER_ID`, identical for any
   two default-configured runners on one host) silently competes with the
   operator-assigned pending-runner name. Until it is fixed, a second same-named
   runner on a host cannot enroll at all — worked around in the smoke with distinct
   `TACK_RUNNER_ID`s, flagged not hidden.
3. **Engine wiring for events/decisions/artifacts** — re-escalation of III-H1's
   request 3, unchanged and now blocking two §III.6 criteria. Owner: `engine.rs`.
4. **Fleet selection still has no write route** (`agent_fleet_members`), so the
   "exact runner **or fleet**" criterion is only half demonstrable — standing since
   E6, restated because it is now release-relevant.

## Known limitations or `not_measured` fields

- The smoke does not exercise API-server restart (G2's chaos suite covers server
  crash recovery at the test layer); it proves runner and harness restart live.
- Usage/economics fields are not asserted by the smoke (F3/F4 unit and route tests
  own that); the opencode live run does emit usage but nothing here depends on it.
- The live opencode leg depends on this machine's local llama-server; if it is
  down, step 7's failure would be environmental — the terminal_reason printed by
  the FAIL line makes that distinguishable.
- III-H3's observed request-stuck-at-`leased` propagation gap did **not** reproduce:
  the request reaches `succeeded` when its attempt does (asserted every run now).
- III-H4 was not touched (separate card, server-side files this card does not own).

## Secrets/logging review

The smoke prints ids only: runner/request/attempt/workspace ids and fencing
counters. Enrollment tokens live in shell variables and process environments,
never in output; both appended transcripts were grepped for token/credential/
bearer/secret before inclusion — clean. The shim never sees or echoes a credential
(its only env inputs are `SMOKE_HANG` and the standard cleared-environment
plumbing). Server/runner logs stayed in the deleted work dir.

## Safe merge order and likely conflicts

Merge any time; the only touched file is owned by this card and no other Wave 7
card edits it. III-H4 is independent. Nothing here conflicts with a future fix for
escalations 1–3; the smoke is written to start passing (and its UNMET lines to
disappear) as each one lands, with no further edits.

## Proposed status-board row (integrator applies; this card must not)

> III-H2 delivered its smoke and refused the tag. `scripts/smoke.sh` steps 7–9 are
> real in both modes and the script can no longer pass while proving less than it
> claims: full claim→checkout→harness→completion proven live (real opencode +
> local model, exact commit verified), restart recovery proven by SIGKILLing the
> runner mid-attempt (needs_operator, no blind duplicate by on-disk process count
> and server attempt count, explicit requeue → success), capacity-1 saturation
> proven under a live lease. **New P0 found by running the product instead of its
> tests: claude-code and codex requests can NEVER be scheduled** — their adapters
> declare zero model_combinations, the scheduler requires declared pairings and
> rejects AutoSelect, so a real installed `claude` sits queued forever (proven
> live, step 8 FAILs on it, exit 1 is the current correct outcome). Also found:
> duplicate runner_name enrollment 500s (two default-configured runners on one
> host cannot both enroll). Still open from before: engine never submits
> events/decisions/artifacts (H1's escalation), III-H4, cargo audit, webkit,
> fleet write routes. §III.6 remains unmet; the tag stays refused. See `III-H2.md`.

## Checklist

- [x] No unowned files (diff = `scripts/smoke.sh` + this handoff).
- [x] No live secret in any output or committed file; transcripts grepped.
- [x] No panic stub, no silent SKIP that cannot fail, no fake success.
- [x] No blind retry added anywhere; the recovery path proven is the explicit one.

---

## Appendix A — `./scripts/smoke.sh --live` transcript (2026-08-19, ANSI stripped)

```
== STEP 1: Harness availability (reported honestly, never rounded up)
   ABSENT:  codex      (cannot be part of any coverage claim)
   UNMET harness binary 'codex' is not installed on this machine — its leg of the three-harness criterion is unverifiable here
   present: claude     2.1.236 (Claude Code)
   present: opencode   1.18.0
   real harness coverage: 2 of 3
   NOTE mode: --live (real binaries; a real model run happens in step 7)

== STEP 2: Build tack + tack-runner
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
   PASS tack built
   PASS tack-runner built

== STEP 3: Start the API server (no Docket configured — its absence must not disable runner execution)
   PASS server healthy on 3399, TACK_ORCH_ENABLE unset (Docket absent)

== STEP 4: Create a project and an item (the plan of record)
   PASS project eebe4841-22a0-4581-b269-d2df24a7aad5
   PASS item d901c037-a9ba-4cbf-a47f-d1f46769b2ab

== STEP 5: Register a pending runner and issue its enrollment token (operator surface)
   PASS pending runner runr_1fdef825-479c-4b91-8baa-906671a986c0, raw token issued once

== STEP 6: Runner enrolls, heartbeats and polls against the live server
   PASS runner active, heartbeat at 2026-08-19T23:25:39.877681785+00:00
   declared model combinations per harness:
     claude-code: (none declared)
     codex: (none declared)
     opencode: llamacpp/qwen3.6-35b-uncensored opencode/big-pickle,deepseek-v4-flash-free,hy3-free,laguna-s-2.1-free,mimo-v2.5-free,nemotron-3-ultra-free,nemotron-3.5-lightning-free

== STEP 7: Claim -> checkout -> harness -> completion, through production routes
   PASS agent profile ap_3b3a9f8b-573d-49bd-a4ae-59a42c95ec5b
   NOTE pairing under test: opencode llamacpp/qwen3.6-35b-uncensored (from the runner's own declaration)
   PASS execution request exec_e269915ab730cfae49d9210e2c0b5b5721c23cec258285d6ca947c54a1543658 queued
   PASS attempt att_2520a0b2-58f2-46dc-bceb-c7775e760458 succeeded (fencing_token 1)
   PASS attempt ran against the exact requested commit 449e1899579ff79e85d05240fc3097f73330053d
   PASS isolated workspace ws_6174745f3235323061306… provisioned
   UNMET the runner never submits events or artifacts (engine has no AttemptDataProtocol call site — open since III-H1), so the §III.6 'verified artifacts and idempotent event timeline' criterion cannot be shown from a real runner (server routes are proven only by fake-client tests)
   PASS Docket absent throughout and execution still ran (G1 invariant, collected live)

== STEP 8: The same neutral request through each harness kind, per kind, never rounded up
   codex:       ABSENT — not installed, not claimed, not counted
   FAIL claude-code: request never claimable — the claude-code adapter's probe deliberately declares zero model_combinations and the scheduler requires the requested pairing to be declared (crates/tack-orch/src/scheduler/select.rs, ModelCombinationNotDeclared; AutoSelect is likewise always rejected), so NO claude-code execution can ever be scheduled on a live runner
   UNMET §III.6 'attempts through Codex, Claude Code and OpenCode': claude-code is structurally unschedulable regardless of an installed binary
   PASS opencode: attempt succeeded through the full pipeline

== STEP 9: Restart recovery: kill the runner mid-attempt — no silent loss, no blind duplicate
   PASS second runner runr_52da4ed7-a7b4-48b3-8c09-3f2bcf51417d active
   PASS attempt running, harness process live (1 run marker)
   PASS saturated runner claimed nothing more (capacity respected under a live lease)
   PASS runner and harness SIGKILLed mid-attempt
   PASS restarted runner reported the ambiguity; attempt is needs_operator (explicit reconciliation, not silence)
   PASS no blind duplicate execution: 1 harness run and 1 attempt, before and after restart
   NOTE operator requeue answered: queued
   PASS requeued work succeeded as attempt #2 — recovered with an explicit operator decision
   PASS the queued-while-saturated request completed once capacity freed

== RESULT ==
SMOKE FAILED — live, 2/3 real harnesses installed; see the failing step above

RELEASE VERDICT: criteria of §III.6 this run could NOT demonstrate
 - harness binary 'codex' is not installed on this machine — its leg of the three-harness criterion is unverifiable here
 - the runner never submits events or artifacts (engine has no AttemptDataProtocol call site — open since III-H1), so the §III.6 'verified artifacts and idempotent event timeline' criterion cannot be shown from a real runner (server routes are proven only by fake-client tests)
 - §III.6 'attempts through Codex, Claude Code and OpenCode': claude-code is structurally unschedulable regardless of an installed binary
A release claim resting on this run must carry every line above.
```

## Appendix B — `./scripts/smoke.sh` (fake mode) transcript (2026-08-19, ANSI stripped)

```
== STEP 1: Harness availability (reported honestly, never rounded up)
   ABSENT:  codex      (cannot be part of any coverage claim)
   UNMET harness binary 'codex' is not installed on this machine — its leg of the three-harness criterion is unverifiable here
   present: claude     2.1.236 (Claude Code)
   present: opencode   1.18.0
   real harness coverage: 2 of 3
   NOTE mode: fake (shim binaries stand in for all three harnesses; the rest of the pipeline is real)

== STEP 2: Build tack + tack-runner
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
   PASS tack built
   PASS tack-runner built

== STEP 3: Start the API server (no Docket configured — its absence must not disable runner execution)
   PASS server healthy on 3399, TACK_ORCH_ENABLE unset (Docket absent)

== STEP 4: Create a project and an item (the plan of record)
   PASS project 089342d8-8efb-4dda-99e7-86ec9a06fa3d
   PASS item edff2845-96ba-4ca2-9110-cf8be7de5482

== STEP 5: Register a pending runner and issue its enrollment token (operator surface)
   PASS pending runner runr_d375f130-47c7-47d3-b337-7beb6c6031bf, raw token issued once

== STEP 6: Runner enrolls, heartbeats and polls against the live server
   PASS runner active, heartbeat at 2026-08-19T23:28:46.366862888+00:00
   declared model combinations per harness:
     claude-code: (none declared)
     codex: (none declared)
     opencode: fake/smoke-model

== STEP 7: Claim -> checkout -> harness -> completion, through production routes
   PASS agent profile ap_916da9bc-156c-414a-a479-bb4fe9f8072f
   NOTE pairing under test: opencode fake/smoke-model (from the runner's own declaration)
   PASS execution request exec_1c4165269226b238be544c8a03455e88368f8e75458b45577c2608fb632a924e queued
   PASS attempt att_b3beed0f-08ac-4021-978b-a659e911eecb succeeded (fencing_token 1)
   PASS attempt ran against the exact requested commit a4da52a7af2c08d7017bc774939898059a95844c
   PASS isolated workspace ws_6174745f6233626565643… provisioned
   UNMET the runner never submits events or artifacts (engine has no AttemptDataProtocol call site — open since III-H1), so the §III.6 'verified artifacts and idempotent event timeline' criterion cannot be shown from a real runner (server routes are proven only by fake-client tests)
   PASS Docket absent throughout and execution still ran (G1 invariant, collected live)
   PASS request state propagated to succeeded

== STEP 8: The same neutral request through each harness kind, per kind, never rounded up
   FAIL codex: request never claimable — the codex adapter's probe deliberately declares zero model_combinations and the scheduler requires the requested pairing to be declared (crates/tack-orch/src/scheduler/select.rs, ModelCombinationNotDeclared; AutoSelect is likewise always rejected), so NO codex execution can ever be scheduled on a live runner
   UNMET §III.6 'attempts through Codex, Claude Code and OpenCode': codex is structurally unschedulable regardless of an installed binary
   FAIL claude-code: request never claimable — the claude-code adapter's probe deliberately declares zero model_combinations and the scheduler requires the requested pairing to be declared (crates/tack-orch/src/scheduler/select.rs, ModelCombinationNotDeclared; AutoSelect is likewise always rejected), so NO claude-code execution can ever be scheduled on a live runner
   UNMET §III.6 'attempts through Codex, Claude Code and OpenCode': claude-code is structurally unschedulable regardless of an installed binary
   PASS opencode: attempt succeeded through the full pipeline

== STEP 9: Restart recovery: kill the runner mid-attempt — no silent loss, no blind duplicate
   PASS second runner runr_4f413b4e-cc52-49c9-ad35-cf7deaf65f6b active
   PASS attempt running, harness process live (1 run marker)
   PASS saturated runner claimed nothing more (capacity respected under a live lease)
   PASS runner and harness SIGKILLed mid-attempt
   PASS restarted runner reported the ambiguity; attempt is needs_operator (explicit reconciliation, not silence)
   PASS no blind duplicate execution: 1 harness run and 1 attempt, before and after restart
   NOTE operator requeue answered: queued
   PASS requeued work succeeded as attempt #2 — recovered with an explicit operator decision
   PASS the queued-while-saturated request completed once capacity freed

== RESULT ==
SMOKE FAILED — fake shim harnesses, pipeline real; see the failing step above

RELEASE VERDICT: criteria of §III.6 this run could NOT demonstrate
 - harness binary 'codex' is not installed on this machine — its leg of the three-harness criterion is unverifiable here
 - the runner never submits events or artifacts (engine has no AttemptDataProtocol call site — open since III-H1), so the §III.6 'verified artifacts and idempotent event timeline' criterion cannot be shown from a real runner (server routes are proven only by fake-client tests)
 - §III.6 'attempts through Codex, Claude Code and OpenCode': codex is structurally unschedulable regardless of an installed binary
 - §III.6 'attempts through Codex, Claude Code and OpenCode': claude-code is structurally unschedulable regardless of an installed binary
A release claim resting on this run must carry every line above.
```
