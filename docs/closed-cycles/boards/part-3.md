> Archived, no-longer-current board. The live board is `TODO.md` — see its "Which board is live" table.

# Part III — Harness-Agnostic Runner Fleet (Phases 50–57)

Executable board for the active cycle described at the bottom of
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Harness-Agnostic Runner
Fleet*. **Everything in Parts I and II remains historical context. Do not delete, renumber,
or implement a superseded Part II card.**

This board is designed for multiple **Terra agents working simultaneously in isolated
worktrees**. Each card is intentionally bounded, names every shared-file owner, and has an
acceptance gate that can be verified without trusting its author's handoff.

## Status board — Part III

| Wave | Cards | Phases | Status |
|---|---|---|---|
| 0 — Clean boundary and safety | III-A0 · A1 · A2 · A3 · A4 | 50 | complete — accepted integration SHA `f042085`; full Rust/frontend/docs/contracts and all three Playwright projects green |
| 1 — Domain, schema and runner skeleton | III-B1 · B2 · B3 · B4 | 51, 52 | complete — accepted integration SHA `f14019b`; domain/schema/runner/contracts and legacy golden gate green |
| 2 — Pull protocol vertical slice | III-C1 · C2 · C3 · C4 · C5 | 52 | complete — accepted integration SHA `f931fc0`; 913 workspace tests, clippy `-D warnings`, fmt, OpenAPI drift and frontend gates green. Integration gate proven end-to-end through the mounted production router (`crates/tack-api/tests/wave2_gate.rs`, 10/10 stability loop) |
| 3 — Real harness proof | III-D1 · D2 · D3 · D4 · D5 | 53 | complete — accepted integration SHA `6a53a18`; 1046 workspace tests, clippy `-D warnings`, fmt clean; B4 fixture pin and the Wave 2 gate both still green. Contract reconciled once against three independently-built adapters. **Live-proof caveats below — read before Wave 4** |
| 4 — Fleet scheduling and PM UX | III-E1 · E2 · E3 · E4 · E5 · E6 | 54 | complete — accepted integration SHA `8a6e613` (branch `agent/iii-e6-integration`, base `f0d4ac2`). E6 wired E1's pure scheduler to live `agent_runners`/`agent_fleet_members`/`agent_fleets`/`execution_requests` data (replacing the naive `ORDER BY created_at LIMIT 1` claim match), added `GET /api/runners` and `GET /api/executions/{id}/attempts[/{n}/events]` (closing the gap E2/E3/E4/E5 each independently hit), and gave the entire operator execution/fleet/runner/profile domain real typed OpenAPI schemas (`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` regenerated, drift-clean) — the pre-existing `{}` schemas E2 flagged as the "biggest spec-drift item" are gone. Found and fixed a genuine integration deadlock between two individually-correct Wave-4 designs: `RunWithAgentModal.tsx` could only ever submit `Auto`-model requests (no live capability data existed to unblock a specific choice), and E1's scheduler unconditionally rejects `Auto` — so no execution submitted through the landed UI could ever be claimed by any runner. Fixed by wiring the new `GET /runners` route into the modal's live capability fetch. 1134 workspace tests (was 1105; +29 this card), clippy `-D warnings`, fmt clean; frontend 653 tests, `tsc -b` and token-lint clean; Playwright chromium+firefox 83 passed/41 skipped/0 failed (webkit could not be evaluated in the build sandbox — missing system library, unrelated to this branch, confirmed against untouched specs too). Healthy selection, saturation, exact-runner exclusivity, unsupported-model rejection and realtime UI updates are each proven through production routes at the Rust integration, CLI (`crates/tack-cli/tests/e6_scheduler_e2e_test.rs`), and UI (`frontend/e2e/scheduler-e2e.spec.ts`) layers — using the exact-runner selector throughout, since `agent_fleet_members` still has no write route on any API surface (a documented, deliberately-not-widened gap; fleet-membership eligibility itself, including the newly-enforced `agent_fleets.concurrency_limit`, is proven directly against the database in `crates/tack-orch/tests/scheduler_wiring_test.rs`). **Genuinely open for Wave 5:** no `agent_fleet_members`/model-profile-toggle/single-fleet-detail write or read routes; `store.ts#attemptsFor` and `ExecutionTimeline.tsx` not wired to the now-real attempts/events routes (mechanical follow-up); `execution_requests` still has no real `priority` column (E6 added a `metadata`-convention stopgap, documented as non-binding); III-F3 model-resolution/provenance untouched, as instructed. See `III-E6.md` for the complete list. |
| 5 — Decisions, artifacts, models and usage | III-F1 · F2 · F3 · F4 · F5 | 55, 56 | **complete** — accepted integration SHA `073aa4d` (branch `agent/iii-f6-integration`, base `cbdd4a3`). Gate lifted when Wave 4 was accepted (`8a6e613`). Four backend cards delivered and merged onto `agent/iii-f6-integration`: III-F1 scoped decisions (`7ce2e5f`), III-F3 model resolution and usage provenance (`802d4c3`), III-F5 execution retention and observability (`b3e8b3c`), III-F2 events and verified artifacts (`9df4c6a`). None needed a migration — every field they use already existed in 045–048. Integrator commit `2689ed7` mounts F1's decision-resolve route behind a second, independent `TACK_EXECUTION_DECISION_TOKEN` gate (resolving the contract gap F1 escalated rather than invented: `protocol.json` names decision resolution a `separately_scoped_operator_credential` with `required_scope: "operator:decisions"`, and Tack has no scope system — fail-closed when unset, mirroring `TACK_ORCH_APPROVAL_TOKEN`), and flips `TACK_EXECUTION_RETENTION_ENABLE` to default **false** (F5 shipped it `true`; the sweep deletes rows, so deletion must be an explicit operator opt-in — `TACK_EXECUTION_HEALTH_ENABLE` stays default `true`, it reads and logs only). **Backend integration complete and gated green** across four integrator sub-cards: III-F6a proved F2's artifact storage/download wiring through the real `build_router` (the wiring itself had landed silently inside `2689ed7`, whose message described only decisions/retention — recorded as an amendment, not rewritten); III-F6b granted F3's wiring requests, so `resolve_request_model_policy` now runs on the live `POST /api/executions` path and `AttemptSummary` carries `model_provenance`/`usage_economics`; III-F6e documented three mounted-but-unspecified paths (87 → 90) and fixed a pre-existing spec lie in which all 13 runner-protocol operations declared `ErrorEnvelope` instead of the `ProtocolErrorEnvelope` they actually return — any generated client would have failed to parse every runner error; III-F6d wired F2's `sweep_events`/`sweep_artifacts` and F1's `expire_overdue_decisions`, which a repo-wide grep showed had **zero callers anywhere** (F5 was authored before F2 existed, so its retention mechanism never knew `execution_artifacts` existed — artifact rows and their on-disk blobs, the largest consumers in this domain, grew unbounded even with retention enabled), and in doing so exposed and closed a list/delete race that could not exist while the sweep had no caller. **Gate:** 1289 workspace tests / 0 failed, clippy and fmt clean, `wave2_gate` 5/5, `runner_contract` 18/18 (all 46 fixtures byte-pinned), `openapi_contract` 5/5 drift-free, `e6_scheduler_e2e_test` 5/5 × 3 consecutive isolated runs (one contention-induced failure observed under concurrent agent load and recorded rather than hidden — see `III-F6.md`), frontend `tsc -b` exit 0. See `docs/agent-handoffs/part-iii/III-F6.md`. **III-F4 (frontend) then closed the wave:** `AttemptAvailability` became a real `idle/loading/ready/error` state machine backed by `loadAttempts`, replacing E2/E6's placeholder, and `store.ts#attemptsFor` plus the event timeline are finally wired to the attempts/events routes E6 recorded as mechanical follow-up. `Not measured` renders as that exact literal — never `$0.00`, an em dash or a blank cell — asserted both as a unit and against real server data, because `runner_time_cost.cost_usd_estimated` is `null` in production always and a zero there would be a lie about money. The fail-closed "decisions cannot be resolved on this deployment" state (403 when `TACK_EXECUTION_DECISION_TOKEN` is unset) is handled as a real operator-facing condition, proven against the real server and distinguished from a genuine 404. Frontend gate: 724 Vitest tests / 85 files, `tsc -b`, token-lint and build clean, Playwright chromium 65/65 (36 axe scans, 0 violations), firefox 23 passed/42 skipped/0 failed. **Webkit could not be evaluated** — the same pre-existing missing `libwoff2dec.so.1.0.2` III-E6 documented, confirmed to fail identically on untouched specs, so not a regression but genuinely unverified. F4 also fixed two Wave 4 E2E specs that broke mechanically once real attempt data began arriving, documented rather than silently absorbed. **Genuinely open for Wave 6:** no decision-discovery or artifact-discovery/list endpoint exists anywhere in the codebase (confirmed by reading every handler), so both UI list views stay honestly empty by design — concrete route shapes are requested in `III-F4.md`; `projects` still has no default-model-policy storage (`ModelPolicySources.project_default` is modeled but always `None`); no runner infra cost-rate is stored anywhere, so `runner_time_cost.cost_usd_estimated` can never read anything but `not_measured`; `model_profiles` (migration 043) is consulted by nothing; `agent_fleet_members` still has no write route; `execution_requests` still has no real `priority` column. See `III-F4.md` and `III-F6.md`. |
| 6 — Legacy bridge and release | III-G1 · G2 · G3 · G4 · G5 | 57 | **Integrated 2026-08-19 at `04ad6f3`; RELEASE BLOCKED — do not tag.** G1–G4 delivered in parallel from base `5c6842f` and were merged by III-G5 in order (`1979d32` G1 docket bridge, `6e50f75` G2 chaos audit, `c97cee4` G3 operator docs, `04ad6f3` G4 CI/packaging). No file was touched by more than one branch — ownership held. Gates run once on the integrated tree; `openapi_contract` 5/5 drift-free. Webkit still unverifiable (missing `libwoff2dec.so.1.0.2`, unchanged since III-E6). **Open P0 — the cycle's definition of done is NOT met:** `tack-runner`'s HTTP transport is unimplemented. `UnavailableProtocolClient` is the only production `RunnerProtocolClient` in the tree (the other, `FakeClient`, is test-only) and the crate does not depend on `reqwest` at all, so the packaged runner binary cannot enroll, claim, heartbeat or report against a live server. Pinned as a deliberate typed failure by `runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` — not hidden, but unbuilt. Everything server-side (routes, scheduler, fencing, decisions, artifacts, retention, operator API/CLI/UI) and the harness adapters are real and tested. **Next: one new card — `tack-runner` HTTP transport.** After it lands, the three-harness live smoke becomes collectible and the tag becomes honest. Other routed-open follow-ups (none blocking on their own): G1 dual-dispatch mirror in `create_execution`, stale orch-task reconcile not boot-scheduled, legacy compatibility label not over HTTP; G2 F2 corrupt-journal recovery scan (P2), F3 resend labeling (P3), disk-full case accepted as not verified. See `docs/agent-handoffs/part-iii/III-G5.md`. |
| 7 — Release blocker | III-H1 · H3 · H2 · H4 | 57 (cont.) | **H1, H3 and H2 done and merged (H2 integrated 2026-08-19 at `01c7046`); III-H4 remains, joined by the Wave 8 cards the H2 escalations were routed to. Integration line is `develop`, the repository's default branch** — consolidated onto one trunk on 2026-08-19 after two naming failures in a row: the Wave 5 card branch `agent/iii-f6-integration` served as the de facto trunk for three waves while the cycle line sat 21 commits behind, and then `develop` and `plan/harness-agnostic-agent-fleet` drifted apart again within a single session. `plan/harness-agnostic-agent-fleet` is retired at `158980e`; there is now exactly one integration line and it is the branch GitHub already treats as default, so dependabot, CI and the board cannot disagree about where work lives. Branch every card from `develop`. III-H1 (`984bb5f`, merged `45ccafb`) built the runner's HTTP transport — the runner can register, ask for work, heartbeat, stream progress, upload results and complete against a live server. III-H3 (`2fd811c`, merged below) gives every claimed attempt its own private checkout at the requested commit, with the harness running inside it and cleanup on completion, cancellation **and** crash recovery — the last of which the code did not actually have; H3 added it in nine lines of `engine.rs`, outside its ownership, accepted here. Gates on the integrated tree: 1363 workspace tests / 0 failed, clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, `h3_checkout` 6/6. **Warning for III-H2 — `scripts/smoke.sh` now reports SMOKE PASSED while proving less than it did before.** Steps 7–9 (claim→complete, per-harness runs, restart recovery) are unimplemented stubs that print SKIPPED unconditionally and never set failure, so the script cannot fail on them. It was a real gate while step 6 failed; now that step 6 passes it is a false green. H2 owns the file and must implement 7–9 before any release claim rests on it. **Open, routed:** `RepositorySpec` carries only remote and revision, so the contract's `repository.subdirectory` and `kind` never reach the provisioner (owner: the spec's owner; nothing faked). The harness discovery test in `harness/claude_code.rs` mutates PATH process-wide while assuming single-threaded execution and can fail unrelated concurrent tests at random (owner: III-D2). No shared git object cache by design — a mirror would reintroduce the shared mutable state crash-safety exists to prevent. Harness reality unchanged: `claude` and `opencode` present, `codex` absent, so two of three is reported as two of three. **III-H4 added 2026-08-19 from a CI failure, not from a card:** a runner that loses a credential-rotation race is answered 401 (reads as a dead credential, terminal) instead of 409 conflict (retryable), because the winner rotates the credential away before the loser is authenticated — so a healthy runner can stop for no reason. Does not block H2's work; settle it before tagging. Does not reproduce locally (3 isolated + 3 full-binary runs under `--features embed-spa`, all green); only CI's contention opens the window. **Also on `develop`'s first honest CI run:** Coverage and E2E now pass having failed on the stale base, while `cargo-deny` and the security audit fail exactly as they did before — the latter is III-G4's deliberately unlanded `cargo audit` bump. **III-H2 delivered its smoke and refused the tag** (card commit `e7ba233`, merged `01c7046`). `scripts/smoke.sh` steps 7–9 are real in both modes and the script can no longer pass while proving less than it claims: full claim→checkout→harness→completion proven live (real opencode + local model, exact commit verified), restart recovery proven by SIGKILLing the runner mid-attempt (needs_operator, no blind duplicate by on-disk process count and server attempt count, explicit requeue → attempt #2 success), capacity-1 saturation proven under a live lease. Gates on the integrated tree: shellcheck -S warning and bash -n clean; fake-mode smoke re-run at `01c7046` exits 1 with step 8 as the only FAIL — the load-bearing correct outcome; no compiled file changed, so the 1363/0 workspace baseline at the H3 merge stands. **New P0 found by running the product instead of its tests: claude-code and codex requests can NEVER be scheduled** — the adapters declare zero model_combinations, the scheduler requires declared pairings and rejects AutoSelect, so a real installed `claude` sits queued forever (proven live; step 8 FAILs on it and exit 1 is currently correct). Escalation outcomes (integrator, 2026-08-19): (1) schedulability P0 → **routed to III-H5** (decision card, release-blocking); (2) duplicate `runner_name` enrollment 500 → **routed to III-H7**; (3) engine never submits events/decisions/artifacts (H1 escalation 3, now blocking two §III.6 criteria) → **routed to III-H6** (release-blocking); (4) no fleet write route (standing since E6) → **routed to III-H8**. **III-H5 done and merged (2026-08-20, card `fafcf30`, merged `84fabf1`) — the schedulability P0 is closed.** Decision card resolved as a pass-through capability attestation: `HarnessCapability.model_passthrough` (optional, runner-v1 `capabilities.json` re-pinned in the same change) attests the verifiably-true claim that the claude-code/codex adapters forward the operator's model verbatim via `--model`, and the scheduler accepts a supported attestation as eligibility for an explicit pairing; Advisory/Unsupported/absent reject identically, AutoSelect stays refused, probe errors and undeclared harnesses still win, and opencode honestly attests Unsupported because it refuses undeclared models pre-spawn. Proven live with the real `claude` 2.1.236: claimed and completed through the full pipeline; smoke step 8 green in BOTH modes with no smoke edit (the load-bearing outcome), and the 'structurally unschedulable' verdict line is gone. Gates on the integrated tree: 1368 workspace tests / 0 failed (+5 over the H3 baseline, exactly the card's new tests, one proven load-bearing by revert), clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, fake smoke SMOKE PASSED. Escalation outcomes (integrator, 2026-08-20): (1) step 8's canned never-claimable FAIL text still names the pre-H5 structural cause — post-H5 that symptom usually means a broken binary or saturation; routed to the smoke's owner (H2 lineage), reword on next touch; (2) this machine's global `claude` broke mid-card (npm dropped its native package) and the probe/scheduler correctly refused it — accepted as environmental, repaired outside the repo (`npm install -g @anthropic-ai/claude-code-linux-x64`), check `claude --version` before suspecting the scheduler on a live step-8 FAIL; (3) `model_profiles` (migration 043) still consulted by nothing — F4's standing finding, unchanged. One §III.6 nuance for review: 'choose only a supported provider/opaque model' now means, for pass-through harnesses, 'any operator-specified model, validated by the harness at run time with an attributable failure' — the honesty rule holds (nothing fake is offered), but the criterion's wording predates attestation and the release reviewer should sign off on that reading. Remaining Wave 8: III-H4 (credential-rotation race → 409), III-H6 (engine submits events/decisions/artifacts — release-blocking), III-H7 (duplicate runner_name 500), III-H8 (fleet write route). Base SHA for them: `84fabf1` on `develop`. §III.6 remains unmet (events/artifacts, fleet write, codex binary); the tag stays refused. See `docs/agent-handoffs/part-iii/III-H2.md` and `III-H5.md`. |
| 8 — Unblock the tag | III-H4 · H5 · H6 · H7 · H8 | 57 (cont.) | **Integrated, not complete — accepted integration SHA `b848d96` on `develop` (2026-08-20), base `84fabf1`/`0e2da46`.** H5 merged earlier in this wave (recorded above). H4, H6, H7 and H8 ran in parallel worktrees off `develop`'s tip and merged in dependency order H8 → H4 → H7 → H6 with zero textual conflicts — H4 and H7 share `crates/tack-api/src/handlers/runner_protocol.rs` but touched disjoint functions (H4: `refresh`/`reclassify_refresh_auth_error`; H7: `enroll`/`is_unique_violation`), confirmed by diff before merging, not just by luck. **III-H4** — a losing credential-rotation now returns retryable `409 conflict` instead of a false-fatal `401`; reproduced deterministically (no timing dependency) and proven load-bearing by reverting the fix and watching the new test fail. **III-H6 (release-blocking)** — the runner engine now submits a real terminal/cancellation event and attempts an artifact upload on every completed attempt, proven live: III-H2's UNMET events/artifacts line is gone from three consecutive `SMOKE PASSED` runs. Found and fixed, inside its own change, a second bug it exposed: submitting an event before completion moved the server's `event_checkpoint` off `NULL`, which broke every completion's compare-and-set outright (`409`, cascading into stuck attempts) — fixed by sourcing `final_event_checkpoint` from the record the runner already tracks, confirmed load-bearing by reverting once. **III-H7** — a second same-named runner enrolling on one host now succeeds instead of 500ing; root cause was the enroll body's self-reported `runner_name` clobbering the operator-assigned name and colliding on the `UNIQUE` constraint, fixed by no longer writing the self-report into `name`, with a typed `409 conflict` kept as defense-in-depth. **III-H8** — `agent_fleet_members` has a write route; an operator can populate a fleet over the API and a fleet-targeted request now schedules onto a member, proven against real DB rows, not just a 200. `docs/openapi.json`/`schema.gen.ts` regenerated via the contract test, not hand-edited. **Escalation outcomes (integrator, 2026-08-20):** (1) III-H6's `crates/tack-runner/src/main.rs` edit, outside its formal `Owns`, is **accepted** — small, localized (one builder call wiring the engine's new seam to the production protocol, two capability-doc-comment corrections), and not claimed by any other Wave 8 card; (2) III-H6's new finding — the artifact **content** PUT (`/attempts/{id}/artifacts/{artifact_id}/content`) returns a server-side `500` in every smoke run, likely because `execution-artifacts` is never created ahead of the first real write — is **routed to a new card, III-H9** (not yet written up; owns `crates/tack-api/src/handlers/runner_protocol.rs::artifact_storage` and `state.artifact_storage`), release-relevant because it is the other half of §III.6's "verified artifacts" criterion; (3) III-H6's open "decisions stays Unsupported" and "no restart-replay for events/artifacts" are accepted as documented scope limits, not defects — no harness in this tree ever asks a mid-run question, so there is nothing yet to exercise the decision path. **Gates on the integrated tree:** 1380 workspace tests / 0 failed (up from the H5-merge baseline of 1368; the four cards' own claimed deltas sum to 1377, the small extra is accounted for by H8's larger-than-estimated test file), clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, frontend `type-check`/vitest (726/726)/build clean. `cargo audit`/`npm audit` still fail — unchanged `Cargo.lock`/`package-lock.json` (verified by diff), the same standing gap III-G4 already owns, not a Wave 8 regression. **§III.6 status:** events/decisions/artifacts and fleet-or-runner selection are now demonstrable except artifact content bytes (III-H9) and a decision-asking harness (no owner yet); codex binary still absent on this machine; the tag stays refused pending III-H9 and a codex install. No next wave is defined yet — III-H9 is the one open card blocking a re-attempt at III-H2's tag. See `docs/agent-handoffs/part-iii/III-H4.md`, `III-H6.md`, `III-H7.md`, `III-H8.md`. |

| 9 — Artifact content storage | III-H9 | 57 (cont.) | **Integrated — accepted integration SHA `6252f52` on `develop` (2026-08-22), base `b848d96`.** Single card, fast-forwarded (branch tip and `develop` were already identical before this commit). Root cause was not H6's escalation guess (a missing `execution-artifacts` directory) — `safe_attempt_dir` already `create_dir_all`s it on every write. The real bug: `encode_id` hex-encoded every byte of a runner-generated `artifact_id` (already ~220 bytes: `art_<hex of "attempt_id:fencing_token:sha256">`), doubling it past Linux's 255-byte `NAME_MAX`, so every real content upload failed with `Io`/`ENAMETOOLONG` and surfaced as a bare `500`. Fixed by SHA-256-hashing the id instead of hex-encoding it literally — same traversal defense, fixed 64-byte length regardless of input. Proven load-bearing by reverting the fix once (new test fails with `Err(Io)` against the old encoding) and live via `./scripts/smoke.sh`: every artifact content `PUT` now returns `200` where it was `500` on every prior run, bytes confirmed on disk. **Gates on the integrated tree (re-run by the integrator, not just the card's own claim):** `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` 1383/0 failed (+3 over the Wave 8 baseline of 1380, matching the card's own count); `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, all unchanged (no wire-shape or route change); no frontend files touched, frontend gates not re-run; `cargo audit` still shows the same 3 pre-existing advisories against an unchanged `Cargo.lock` (III-G4's standing gap, not a regression). **§III.6 status:** both halves of "verified artifacts" (manifest + content bytes) are now demonstrable live end-to-end. **The tag remains blocked on exactly one thing: the `codex` binary is absent from this machine** (III-H2's standing environmental gap) — no decision-asking harness exists yet either, but that is an accepted scope limit, not a defect. No next wave is defined; whoever installs `codex` and re-runs `./scripts/smoke.sh` closes the cycle's definition of done. See `docs/agent-handoffs/part-iii/III-H9.md`. |

### Amendment to Wave 9 — 2026-08-26: `codex` installed, live smoke still FAILS

Recorded as an amendment rather than by rewriting the Wave 9 row above, per the
corrections-are-appended rule. **Wave 9's closing sentence — "whoever installs `codex` and
re-runs `./scripts/smoke.sh` closes the cycle's definition of done" — has been tested and is
false.** The tag is still refused, for a new reason.

**What was done.** `codex` is now installed (`codex-cli 0.149.1`), joining `claude` 2.1.236
and `opencode` 1.18.0 — smoke step 1 reports **3 of 3 real harnesses present**, the first
time that has been true on any machine in this cycle. Both binaries were rebuilt in release
(`tack` 18.4 MB with `embed-spa`, `tack-runner` 4.5 MB) and `./scripts/smoke.sh --live` was
run in full.

**Result: `SMOKE FAILED`.** Steps 1–6 and step 9 passed, including the complete restart-
recovery proof (SIGKILL mid-attempt → `needs_operator` → no blind duplicate → operator
requeue → attempt #2 success) and capacity-1 saturation. The failures:

- **Step 7 — the live attempt never reached a terminal state.** The `opencode` +
  `llamacpp/qwen3.6-35b-uncensored` pairing (chosen by the script from the runner's own
  declaration) was claimed, checked out at the exact requested commit `29836a98` with an
  isolated workspace, and then produced `attempt ended '' — terminal_reason: null` after the
  300 s live budget. The pipeline did its job up to the harness; the harness run itself did
  not come back.
- **Step 8 — all three harness kinds FAILed.** `codex` and `claude-code` printed the canned
  "declares zero `model_combinations`, therefore structurally unschedulable" text, and
  `opencode` printed "never claimed despite a declared model combination".

**Read step 8's message with the board's own warning in hand.** The Wave 7 row already
routed that exact string as **stale**: escalation (1) of the III-H5 integration explicitly
records that "step 8's canned never-claimable FAIL text still names the pre-H5 structural
cause — post-H5 that symptom usually means a broken binary or saturation; routed to the
smoke's owner (H2 lineage), reword on next touch." III-H5 closed the schedulability P0 via
`model_passthrough` and proved `claude` claimable live. The text was never reworded, so it
fired again here and reads as a regression it is not.

**Leading hypothesis, explicitly NOT verified.** The runner is enrolled at capacity 1. Step
7's attempt never terminated, so its lease was very likely still held when step 8 created
three more requests — under which none of them could be claimed by anyone, which is
precisely the symptom all three printed, `opencode` included. That would make step 8's three
failures a **cascade of step 7's hang**, not three independent defects. This is a hypothesis
consistent with every observation, not a diagnosis: nobody has re-run step 8 against an idle
runner, and the runner-side log was not preserved (`SMOKE_KEEP=1` was not set).

**What is therefore genuinely open** — do not treat any of this as settled:

1. Why the live `opencode` + local-llama.cpp attempt hangs instead of terminating. Re-run
   with `SMOKE_KEEP=1` and inspect the runner log and journal; try a lighter declared
   pairing (`opencode/big-pickle`, `hy3-free`, `mimo-v2.5-free` … were all declared) before
   concluding anything about the product rather than the model.
2. Whether step 8's three failures survive once a runner is idle. If they do not, the real
   defect is that the smoke saturates its own single-capacity runner and then blames
   scheduling.
3. Step 8's canned FAIL text, still unworded since 2026-08-20 and now demonstrably
   misleading a second reader. Owner remains the smoke's (H2 lineage).
4. `codex` has still never completed a live attempt — installing it moved the blocker, it did
   not clear it. Two of three harnesses remain live-proven; three of three is still not a
   claim this repository can make.

**One test was also fixed while running the workspace gate**, and is **uncommitted** at the
time of writing: `crates/tack-runner/src/harness/mod.rs::registering_all_three_real_adapters_is_order_independent`
asserted that all three adapters reject the fixture spec identically. That held only because
`codex` was absent; with the binary installed the codex adapter correctly *accepts* an
explicit model (it is a pass-through harness — III-H5), and the test failed. The assertion
now expects accept-for-codex / reject-for-the-other-two, which is the real post-H5 contract
rather than an artifact of an empty PATH. Gates after the fix: `cargo test --workspace`
green, `clippy -D warnings` and `fmt --check` clean. See §IV.0 — land or discard this
deliberately before branching Part IV cards.

**Current branch:** `plan/harness-agnostic-agent-fleet`.

**Gate 0 — do not skip:** this branch was created while the source worktree already carried
the unreleased Part II changes. Before implementation agents branch or create worktrees, a
human/integration owner must make that baseline reviewable as one or more named commits.
Do not stash, reset, discard, or silently absorb those changes. Record the chosen baseline
SHA in the first Part III handoff.

**Satisfied at Wave 0.** The baseline was recorded as commit `1d71785` ("chore: preserve
unreleased Part II baseline" — 82 paths: 46 previously-tracked modifications + 36
previously-untracked files, reviewed intact rather than stashed/discarded) in
`docs/agent-handoffs/part-iii/III-A0.md`. The wave integrator then accepted
`f042085d585adfdd8386a2120c7429649883e5df` as the exact Wave 1 branch point once the
combined tree passed full workspace tests, clippy, fmt, the frontend suite, the complete
Playwright matrix and `mdbook build`. This gate has not applied to any card since Wave 0
closed.

---

## III.0 Cold-start context capsule

Every Terra prompt begins with this section plus its one card. Do not paste all 9,000 lines
of Parts I/II into a worker context.

### Objective

From a Tack PM item, create a durable execution request that is assigned to an exact runner
or a fleet. A pull-based `tack-runner` launches Codex, Claude Code, OpenCode, or a future
coding harness. The operator selects an agent profile and a supported harness/provider/model
combination. Tack records requested and actual execution facts, events, decisions, artifacts
and nullable usage without making Docket mandatory.

### Architectural boundary

- Tack API: PM source of truth, durable queue, scheduler, fenced leases, normalized history.
- `tack-runner`: local credentials, workspace/worktree, harness subprocess, journal,
  cancellation and recovery observation.
- Harness adapter: one CLI/runtime integration inside `tack-runner`.
- Docket: optional `legacy-docket` bridge only; never the owner of a new runner request.
- GitHub Actions: CI/integration work, not a coding-harness adapter for this cycle.

### Vocabulary that must remain distinct

`Item` ≠ `ExecutionRequest` ≠ `ExecutionAttempt`; `AgentProfile` ≠ `Fleet` ≠ `Runner`;
`Harness` ≠ `ModelProvider` ≠ `ModelId`. A field called only `provider` is rejected unless
its type makes the namespace explicit.

### v1 scope

- One active fenced lease per execution request.
- Exact-runner or fleet assignment.
- Three in-tree harness adapters: Codex, Claude Code, OpenCode.
- One attempt owns one isolated workspace/worktree.
- Optional explicit item-status mapping through the workflow engine.
- No automatic multi-agent fan-out, task decomposition, model proxy, generic plugin ABI or
  GitHub Actions execution.

### Honest delivery semantics

Do not claim exactly-once harness execution. A database transaction can prevent two valid
leases, but a runner/network crash after process launch can leave ownership ambiguous. The
contract is: at most one **valid active lease**, monotonically increasing fencing tokens,
local runner journal, idempotent reports, and `needs_operator` when safe retry cannot be
proved. Lease expiry never blindly launches a second process.

### Required reading by role

| Role | Read before editing |
|---|---|
| Contract/domain | Roadmap Phase 50–57 section; `crates/tack-orch/src/lib.rs`; current `dispatcher.rs` and reconciler store traits |
| Database | `crates/tack-db/src/migrations.rs` runner and migrations 034–038; `repo/items.rs`; `repo/orch.rs`; migration tests |
| API | `router.rs`; `middleware.rs`; `openapi.rs`; error envelope; item/orchestration handlers |
| Runner | Roadmap runner protocol/adapter sections; workspace `Cargo.toml`; existing CLI process/config patterns |
| Frontend | `app/routes.tsx`; shared API client; `projectItemsContext`; shared orchestration capabilities; architecture test |
| Harness adapter | Only the frozen Part III fixtures/types, adapter-owned directory, fake-binary harness and its target CLI's current contract |
| Release | CI workflow; `docs/TESTING.md`; OpenAPI/golden gates; backup/restore tests; Part III exit matrix |

---

## III.1 Frozen v1 contracts

Wave 0 owns these contracts. Later cards consume them and may not edit them independently.
A real adapter may falsify a contract; it reports the gap in its handoff and D5 changes the
contract once for all adapters.

### III.1.1 Lifecycle

`queued | leased | preparing | running | waiting_decision | succeeded | failed | cancelled |
lost | needs_operator`

Terminal: `succeeded`, `failed`, `cancelled`. `lost` is an observed loss of contact with no
known running process; `needs_operator` is an ambiguous side-effect/ownership state and is
not automatically retryable.

Allowed transitions and who may request/observe them live in a committed fixture. At a
minimum:

- API/scheduler: create `queued`; grant `queued → leased`; request cancellation.
- Lease owner: `leased → preparing → running`; `running ↔ waiting_decision`; terminal report.
- Recovery service: expired lease → `lost` only when the runner journal/probe proves no
  active process, otherwise `needs_operator`.
- Operator: explicitly requeue/abandon a `needs_operator` request with an audit event.

### III.1.2 Execution request snapshot

Required immutable fields after enqueue:

- `item_id`, `idempotency_key`, request creator/source and creation time;
- exact runner or fleet selector;
- agent-profile id **and resolved profile snapshot**;
- requested harness kind;
- requested model-provider and opaque model id, each nullable when auto-selection is allowed;
- repository/workspace reference and base revision;
- permission/tool policy, timeout and budgets;
- optional status-map policy id;
- bounded environment/metadata with secret references only.

### III.1.3 Attempt snapshot

- request id + monotonically increasing attempt number;
- runner id, fencing token, lease issue/expiry/heartbeat timestamps;
- actual harness kind/version, provider and opaque model id;
- capability snapshot used for validation;
- isolated workspace identity and base revision;
- started/ended time and typed terminal/recovery reason;
- tokens/time/cost fields nullable, each with `measured | estimated | not_measured` source;
- no raw vendor credential.

### III.1.4 Runner capability snapshot

- protocol version, runner version and labels;
- total/current concurrency;
- harness kind + installed version;
- supported or discoverable provider/model combinations;
- `cancel`, `resume`, `decisions`, `artifacts`, `usage` support values with reasons;
- maximum event/artifact limits;
- last probe time and probe error without secrets.

### III.1.5 Idempotency/fencing

- unique execution-request idempotency key in its declared scope;
- unique `(request_id, attempt_number)`;
- unique `(attempt_id, event_id)`;
- terminal completion compare-and-set and replay-safe;
- every runner mutation checks runner identity, attempt id and current fencing token;
- stale/expired tokens return a stable `stale_lease` error and write nothing;
- event batch rows and its checkpoint commit together.

### III.1.6 Protocol fixture directory

`docs/contracts/runner-v1/` is the language-neutral authority. It contains canonical JSON
for enrollment, capabilities, claim/no-work, heartbeat, event batch, decision, artifact,
completion, cancellation and every stable error. Rust/OpenAPI/frontend types must round-trip
these fixtures. Hand-written feature DTOs are not another authority.

---

## III.2 Rules for simultaneous Terra agents

1. **One card, one isolated worktree, one branch.** Suggested branch:
   `agent/iii-<card>-<short-name>`. Never let two active agents write the same checkout.
2. **Stay inside `Owns`.** A file under `Must not edit` is a hard stop. Record the needed
   change in the handoff; the named owner/integrator performs it.
3. **Shared chokepoints have one owner per wave:** root `Cargo.toml`/`Cargo.lock`,
   `migrations.rs`, `router.rs`, `openapi.rs`, generated schema, CI, and the frozen contract.
4. **No migration numbers outside B2.** Cards request schema changes in handoff notes. B2
   batches them only at wave boundaries after A3 repairs/accepts the runner.
5. **No router/OpenAPI/generated-schema edits outside C5.** Handler agents create modules
   and focused tests; C5 wires routes and regenerates artifacts once.
6. **No shared trait edits from adapter cards.** D1/D2/D3 implement the frozen interface or
   report a falsifying fact. D5 alone reconciles it.
7. **No `unimplemented!()`, hidden fake success, or structural zero.** Unsupported is typed;
   unknown is explicit; unmeasured is nullable.
8. **Tests ship with the card.** Required CI tests use fake clocks, fake binaries and local
   mock HTTP only. Live harness tests are opt-in and never require secrets in CI.
9. **No blocking sleeps in tests.** Lease/heartbeat/retry tests inject time.
10. **No blanket formatting or mechanical rewrite of unowned files.** Run checks, but only
    format files the card owns.
11. **Do not edit Part III status or another handoff.** Each card creates exactly one
    `docs/agent-handoffs/part-iii/<card>.md`; the wave integrator updates this board after
    independent verification.
12. **Security-sensitive logs contain ids, never credentials, prompt bodies, query strings
    or complete environment values.** Tests assert redaction.
13. **Stop on contract ambiguity.** Do not “make it compile” with raw JSON, `String`ly keys
    or provider checks. State the mismatch and the smallest decision required.
14. **Each wave ends with adversarial verification by someone who did not author the code.**

### Handoff file template

```markdown
# III-<card> handoff

- Base SHA / branch / final SHA:
- Files changed (must equal ownership list):
- Contract fixtures consumed:
- Behavior implemented:
- Tests added and exact commands/results:
- Failure/adversarial case proved:
- Schema/API/contract change requested from another owner:
- Known limitations or `not_measured` fields:
- Secrets/logging review:
- Safe merge order and likely conflicts:
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
```

---

## III.3 Shared-file ownership

| Chokepoint | Owner/order |
|---|---|
| `TODO.md`, roadmap Part III statuses | A0, then wave integrator only |
| `docs/adr/**`, `docs/contracts/runner-v1/**` | A0; D5 may revise once after real adapter probes |
| `crates/tack-db/src/migrations.rs` | A3 (repair/decision) → B2 (new execution schema) → no other card |
| `crates/tack-db/src/repo/mod.rs` | B2 |
| `crates/tack-orch/src/lib.rs` | B1 only; prefer new modules over further growth |
| root `Cargo.toml`, `Cargo.lock` | B3 only until runner crate builds; dependency requests go to B3 |
| `crates/tack-api/src/router.rs`, `handlers/mod.rs`, `openapi.rs` | A1 for security correction, then C5 for runner/execution wiring |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | C5, then E6/F4 only at integration boundaries |
| `frontend/src/shared/execution/**` | E2; E3/E4 consume after E2 merges |
| `.github/workflows/ci.yml` | G4; earlier cards provide commands, never edit workflow |
| existing `orch_*`, Docket adapter/reconciler | untouched until G1 except audited Phase-50 fixes explicitly owned below |

---

## III.4 Dependency graph and merge policy

```text
Wave 0:  A0  A1  A2  A3  A4          (parallel after clean baseline)
           │           │
           └────┬──────┘
Wave 1:       B1  B2  B3  B4          (parallel; frozen fixtures are the seam)
                └──┬──┘
Wave 2:       C1  C2  C3  C4 → C5     (C5 is the only route/spec integrator)
                    │
Wave 3:       D1  D2  D3  D4 → D5     (three probes parallel; one reconciliation)
                    │
Wave 4:       E1  E2  E5 → E3 E4 → E6
                    │
Wave 5:       F1  F2  F3  F5 → F4
                    │
Wave 6:       G1  G2  G3  G4 → G5
```

Within a parallel set, merge the card with the least shared surface first, then rebase the
rest. The wave integrator verifies the combined tree; a card's green branch is not evidence
that the wave is green. No later wave begins merely because its favorite dependency merged.
The status board changes only when the wave gate passes on the integration branch.

---

## Wave 0 — Phase 50, clean boundary and safety

### III-A0 — Contract/ADR and clean-baseline owner

**Owns:** new `docs/adr/0050-runner-control-plane.md`, new
`docs/contracts/runner-v1/**`, new `docs/agent-handoffs/part-iii/README.md` and A0 handoff;
Part III status rows only.

**Must not edit:** Rust/TypeScript source, migrations, router, generated files.

**Depends on:** human records a clean baseline SHA.

**Tasks:** record the baseline and inventory unreleased Part II files; write the ADR declaring
Tack the scheduler, runner the process owner and Docket legacy; commit every III.1 fixture,
transition, payload limit, error and protocol compatibility rule; define enrollment,
revocation and redaction; preserve the v1 non-goals.

**Acceptance:** every lifecycle transition has allow/deny fixtures; no field uses an
ambiguous bare `provider`; ADR explicitly rejects exactly-once and dual scheduling; fixtures
parse and mdBook/link checks pass.

### III-A1 — Trust-boundary repair

**Owns:** `crates/tack-api/src/{router,middleware,config,server}.rs`, focused new security
tests; `frontend/src/shared/ui/RichTextEditor.tsx`, token storage modules, `boardSocket.ts`
and focused tests.

**Must not edit:** execution handlers, migrations, OpenAPI registry, generated schema.

**Tasks:** sanitize/encode persisted rich content and add CSP; remove the long-lived
privileged approval token from local storage; introduce a safe API-token/session strategy;
authenticate WebSockets and derive their origin from configured API base; use exact auth
routers; redact queries; merge defaults → file → environment; fail closed for unauthenticated
non-loopback; validate outbound origins/redirects and clear a credential when origin changes.

**Acceptance:** stored script/event-handler cannot execute; authenticated split-origin
WebSocket works; suffix lookalike routes stay protected; Alexa token is absent from spans;
origin change never forwards old token; environment overrides file; unsafe startup fails.

### III-A2 — Atomic mutation and browser concurrency repair

**Owns:** `crates/tack-core/src/models.rs` only for PATCH presence types;
`crates/tack-db/src/repo/items.rs`; `crates/tack-api/src/handlers/items.rs`; focused item
concurrency tests; `frontend/src/shared/api/{client,items}.ts`, affected item mutation callers
and focused tests.

**Must not edit:** router, migrations, orchestration repo/handlers, unrelated UI views.

**Tasks:** one conditional transaction/logical update for version, WIP/status/timestamps and
fields; one increment; body+ETag same snapshot; tri-state nullable PATCH; preserve headers,
send `If-Match`, and expose deliberate 412 refresh/retry UX.

**Acceptance:** multi-field failure writes nothing; same-ETag racers produce one commit;
rejected status does not bump; null clears assignee/description/estimate; body and ETag agree;
browser sends `If-Match`. Force a yield/failure before SQL and prove no partial mutation.

### III-A3 — Migration runner and rebuild recovery

**Owns:** `crates/tack-db/src/migrations.rs`, migration/rebuild tests, migration ADR/addendum
and A3 handoff.

**Must not edit:** repositories, root Cargo, backup implementation.

**Tasks:** decide unreleased 037/038; transactional ordinary migrations; recoverable
copy/verify/swap rebuild; fetch/assert FK check; order/checksum invariant; automatic
pre-upgrade backup contract; inject failure at every rebuild statement.

**Acceptance:** no tested crash creates an unrecoverable boot loop; lossy copy is detected
before source deletion; FK violations fail; all supported old schemas preserve every field;
migration record appears only after commit.

### III-A4 — Green frontend/release baseline

**Owns:** three Blob/object-URL failing tests and setup, relevant settings components only if
required, `frontend/src/index.css`, Vite/PostCSS font config, current failing Playwright specs
and A4 handoff.

**Must not edit:** shared API implementation, runner/execution UI, backend source.

**Tasks:** fix cross-realm Blob tests without weakening behavior; repair stale approval E2E
and ambiguous locator; emit/reference production fonts; record every browser project run.

**Acceptance:** Vitest and all Playwright projects green; dist contains fonts with no
unresolved warning; no skip or weakened assertion hides the failure.

### Wave 0 integration gate

Full Rust/clippy/fmt/frontend unit/type/build/token-lint/Playwright green on the combined
tree; migration crash and security adversarial suites green; fixtures/ADR accepted; A0
records the accepted SHA and Wave 1 branches from exactly it.

---

## Wave 1 — Phases 51–52, domain/schema/runner skeleton

### III-B1 — Neutral execution domain

**Owns:** new `crates/tack-orch/src/execution/{mod,types,lifecycle,capabilities}.rs`, minimal
`crates/tack-orch/src/lib.rs` exports, focused unit/property tests and B1 handoff.

**Consumes:** runner-v1 fixtures verbatim. **Must not edit:** legacy Docket/reconciler,
migrations, API, root Cargo.

**Tasks:** typed ids; request/attempt snapshots; lifecycle validator; capability support
values/reasons; usage provenance; typed errors. Keep I/O out and keep Docket/GitHub nouns out.

**Acceptance:** every fixture round-trips; illegal transitions have stable reasons; unknown
model ids round-trip byte-for-byte; requested and actual values cannot be confused by type.

### III-B2 — Execution schema and repository

**Owns:** `migrations.rs` after A3, new `crates/tack-db/src/repo/execution.rs`, its sole
`repo/mod.rs` export, execution migration/repository tests and B2 handoff.

**Tasks:** add the ten roadmap tables; transactional enqueue, claim/fence, heartbeat,
event-batch checkpoint, completion, cancellation, decision/artifact metadata and explicit
recovery classification; inject a fake clock; prove queue/history indexes with query plans.

**Acceptance:** concurrent claimers produce one lease; stale fence writes nothing; replay is
idempotent; terminal state cannot reopen; ambiguity never auto-requeues; supported prior
schemas upgrade without legacy data loss.

### III-B3 — `tack-runner` skeleton and dependency owner

**Owns:** root `Cargo.toml`, `Cargo.lock`, new `crates/tack-runner/**` excluding Wave 3
adapter files, and B3 handoff.

**Tasks:** binary/library split, redacted config, protocol-client seam, graceful shutdown,
injectable clock/process/filesystem, structured logging and empty registry returning typed
unsupported. Keep dependencies minimal and record binary-size impact.

**Acceptance:** workspace builds/tests; runner `--help` works; missing enrollment credential
fails without logging it; shutdown joins every task; registry never panics; no model-vendor
SDK is added.

### III-B4 — Contract conformance harness

**Owns:** new fixture tests under `crates/tack-orch/tests/runner_contract/**`, test-only fake
clock/fake runner helpers with no shared-source edits, and B4 handoff.

**Tasks:** validate domain serialization against every fixture; reusable race/fencing/replay
drivers; property tests for lifecycle/idempotency; deterministic contract mutation tests.

**Acceptance:** changing a fixture field/state/error fails a named test; fake time advances
expiry without sleeping; helpers contain no global mutable state.

### Wave 1 integration gate

Schema/domain names match fixtures; runner is in workspace; migration upgrade, claim/fence
and contract mutation tests pass; legacy orchestration golden is unchanged. Contract mismatch
returns to A0, never an ad-hoc Wave 2 patch.

---

## Wave 2 — Phase 52, pull protocol vertical slice

### III-C1 — Operator execution/fleet handler modules

**Owns:** new `crates/tack-api/src/handlers/{executions,runner_admin}.rs`, focused tests with a
card-local router, and C1 handoff.

**Must not edit:** global router, OpenAPI registry/spec, generated TypeScript, migrations.

**Tasks:** create/list/get/cancel and operator-confirmed reconcile/requeue; manage fleets,
enrollment tokens, revocation and profile/model references; enforce item existence and
idempotency; never return hashes/stored tokens after enrollment.

**Acceptance:** duplicate create returns same request; cancellation is requested, not falsely
terminal; only `needs_operator` permits explicit audited recovery; revoked runner cannot be
selected; envelopes match fixtures.

### III-C2 — Runner protocol/auth handler modules

**Owns:** new `handlers/runner_protocol.rs`, new runner-auth module/middleware, focused tests
with a card-local router, and C2 handoff.

**Must not edit:** global router/middleware, OpenAPI registry, generated files.

**Tasks:** enrollment exchange, capability refresh, heartbeat, claim, accept/start, event
batch, decision poll, artifact manifest, completion and cancel observation; hash/rotate/revoke
credentials; validate attempt/fence on every write; enforce payload limits.

**Acceptance:** operator auth cannot silently substitute for runner auth; runner cannot
mutate PM objects or resolve decisions; stale fence/replay match stable errors; oversized
batch writes nothing.

### III-C3 — Runner client, journal and isolated workspace

**Owns:** `crates/tack-runner/src/{client,journal,workspace,engine}.rs`, focused tests and C3
handoff.

**Tasks:** enroll/claim/heartbeat/report loop; atomic owner-only journal; deterministic
per-attempt workspace/worktree; cleanup/quarantine; persist journal before spawn; restart
recovery observation; cancellation coordination using a fake adapter.

**Acceptance:** restart recovers journal; attempts never share workspace; credential/journal
files owner-only; cleanup refuses repo root/unresolved paths; expired fence stops reporting
and quarantines ambiguous state.

### III-C4 — Mock end-to-end crash matrix

**Owns:** new tests/fixtures only under `crates/tack-api/tests/runner_vertical_slice/**`
and/or `crates/tack-runner/tests/**`; C4 handoff.

**Tasks:** drive production repository/router seams with fake process/clock; inject before
claim commit, after claim, before spawn, after spawn-before-ack, during events/completion and
during cancellation.

**Acceptance:** no silent loss or two valid fences; safe pre-spawn failure requeues;
post-spawn ambiguity becomes `needs_operator`; replay duplicates nothing; every injected case
leaves an explanatory audit event.

### III-C5 — Router/OpenAPI/generated contract integrator

**Owns:** `handlers/mod.rs`, global `router.rs`, `openapi.rs`, `docs/openapi.json`, generated
frontend schema and C5 handoff. **Depends on:** accepted C1–C4; no concurrent edits.

**Tasks:** mount operator and `/api/runner/v1` routers with distinct auth; expose appropriate
operator contract and version runner contract; regenerate once; exact route/auth enumeration,
CORS and drift tests.

**Acceptance:** runner routes are outside operator auth exemptions; no credential response
field; generated types match fixtures; production router completes the mock vertical slice.

### Wave 2 integration gate

A mock runner enrolled on a clean database can claim, start, stream, complete and survive
API/runner restart. Security, fencing, payload and OpenAPI drift gates pass. No real harness
card starts earlier.

**Passed** at integration SHA `f931fc0`. Proven by `crates/tack-api/tests/wave2_gate.rs`, which
drives the real `build_router` against a from-scratch database and asserts persisted SQL state
at every step, importing no test infrastructure from any card. Handoff:
`docs/agent-handoffs/part-iii/III-wave2-gate.md`.

### Wave 2 carry-forward — read before starting Wave 3

Open items Wave 2 deliberately did not close. None block Wave 3, but each has an owner.

1. **Accept/start have no B2-side idempotency fingerprint.** C2 compensates in the handler.
   If a D-card observes a real runner retrying these, escalate to B2 rather than widening the
   handler workaround.
2. **Decision resolution has no endpoint.** C2's acceptance bullet "a runner cannot resolve
   decisions" is currently vacuously true — nothing can. Wave 5 (F-cards) owns the real surface;
   do not let a D-card invent one.
3. **`logs_never_contain_raw_credentials_only_ids` (C2) is flaky (~1/10) under parallel test
   execution** — a `tracing::subscriber::set_default` thread-local racing `tracing`'s global
   callsite-interest cache. Stable under `RUST_TEST_THREADS=1`. The assertion is sound; the
   harness is not. C2 owns the fix.
4. **The shared in-memory SQLite test harness can mask write-write races.** B2 found the
   decision/artifact race only after switching to a file-backed database. Any new concurrency
   test in `tack-db` should use a file-backed DB and be proven load-bearing by reverting the fix.
5. **`orch.rs` retention-sweep rollups (~lines 1529, 1646) carry the same deferred-transaction
   read-then-write shape** B2 fixed across `execution.rs`. Inspection only, never stress-tested,
   and frozen until card G1 — do not fix opportunistically.

---

## Wave 3 — Phase 53, real harness proof

### III-D1 — Codex probe/adapter

**Owns:** `crates/tack-runner/src/harness/codex.rs`, Codex fake-binary fixtures/tests,
optional live test and D1 handoff. **Must not edit:** shared trait/registry/engine.

**Tasks:** detect version; report capabilities without assuming models; validate frozen spec;
execute deterministic fixture repo; normalize output/result; cancel process tree; report
actual selection; reconcile journal only when proven supported.

**Acceptance:** fake success/failure/cancel/malformed/unknown-version tests; unsupported
selection fails pre-spawn; arguments/env redacted; opt-in live test records version/artifact.

### III-D2 — Claude Code probe/adapter

**Owns:** `crates/tack-runner/src/harness/claude_code.rs`, its fixtures/tests/live test and D2
handoff. Apply D1's tasks/gates using observed Claude Code behavior. Do not emulate
Codex-only resume, usage or approval behavior; report support/reason honestly.

### III-D3 — OpenCode probe/adapter

**Owns:** `crates/tack-runner/src/harness/opencode.rs`, its fixtures/tests/live test and D3
handoff. Apply D1's tasks/gates while preserving OpenCode's explicit provider/model
combinations instead of flattening them into global model availability.

### III-D4 — Common process/event infrastructure

**Owns:** new `crates/tack-runner/src/harness/{mod,process,event_sink}.rs`, common engine
integration, shared fake binary and D4 handoff. May run beside D1–D3 but cannot edit frozen
trait.

**Tasks:** bounded stdout/stderr/event streaming, process-group cancellation, timeouts,
backpressure, redaction, artifact staging and registry; provider parsing stays out.

**Acceptance:** high-volume output stays memory-bounded; cancel kills descendants; adapters
cannot cross-read workspaces; secret canaries absent from logs/events; truncation is explicit.

### III-D5 — Harness-contract reconciliation/integration

**Owns:** shared harness trait/types/registry, runner-v1 fixtures only when real evidence
requires it, registration tests and D5 handoff. Runs after D1–D4.

**Tasks:** compare three observed contracts; make the smallest one-time interface change;
update every adapter/fixture/test together; reject generic methods implemented by only one;
register all three without ordering behavior.

**Acceptance:** no panic/TODO adapter; same fixture completes through all three fake adapters;
two opt-in live adapters pass before Wave 4 and all three before release; lying capability is
caught before invocation.

### Wave 3 integration gate

**Passed** at integration SHA `6a53a18`, with the live-proof caveats recorded below. A lying
capability is now refused at registration, not discovered at dispatch: `HarnessProbe::
declared_capabilities()` plus the ceiling check in `AdapterRegistry::register_probe` rejects any
probe claiming `Supported` cancellation. Handoff: `docs/agent-handoffs/part-iii/III-D5.md`.

### Wave 3 carry-forward — read before starting Wave 4

1. **No harness supports cancellation better than `Advisory`.** All three shell-tool
   subprocesses run in a new session outside the runner's process group — observed with `ps`
   against real `claude` and real `opencode`, independently. Group signalling cannot reach
   them. E1's scheduler must **read** the capability snapshot, never assume cancellation works;
   and Part III's honest-delivery rule still holds — lease expiry never blindly launches a
   second process.
2. **Only Claude Code can confirm which model actually ran** (its `stream-json` `init` event).
   Codex and OpenCode reject auto-select pre-spawn rather than fabricate a value, so a request
   with no explicit model is unschedulable on two of three harnesses. E1 must surface that as a
   named reason, not an empty candidate list. `opencode export <sessionID>` was found to give
   authoritative post-hoc confirmation and is the recommended shape if this becomes a priority.
3. **`codex` was never installed on the development machine.** D1 is proven only against the
   shared fake binary; its seven documented assumptions about the real CLI are unverified. Its
   live test must pass before release (D5 acceptance), and whoever first runs it should read
   D1's assumption list first.
4. **Claude Code's live run reported `terminal_state=Failed`** for reasons unrelated to the
   adapter (its probe and artifact staging both succeeded). Worth one deliberate investigation
   before release, since the live test is billed.
5. **No adapter test captures `tracing` output** to prove redaction there. Every call site was
   manually reviewed and none leak, but the property has no regression test — deliberately left
   because Wave 2's C2 hit real flakiness in exactly that capture mechanism (see the Wave 2
   carry-forward).
6. **Artifact support is `Advisory` everywhere** — all three adapters stage raw run logs only;
   none implement artifact discovery. Wave 5's F-cards own the real surface.

---

## Wave 4 — Phase 54, fleet scheduler and PM UX

### III-E1 — Deterministic fleet scheduler

**Owns:** new `crates/tack-orch/src/scheduler/**`, scheduler tests and E1 handoff.

**Tasks:** filter by membership, health freshness, capacity, labels, harness and valid
provider/model combinations; deterministic ordering/tie-break, priority/fairness and exact
runner path. Pure selection performs no I/O and never grants the authoritative lease.

**Acceptance:** table/property tests cover empty, stale, saturated, heterogeneous and tied
fleets; invalid combinations name reasons; identical input selects identically; only the
repository claim can make a lease valid.

### III-E2 — Shared frontend execution API/state

**Owns:** new `frontend/src/shared/execution/**`, focused shared-state tests and E2 handoff.
**Consumes:** generated types. **Must not edit:** feature UI folders or generated schema.

**Tasks:** API wrappers preserving headers/errors; item execution store; capability selector;
request/attempt cache; one realtime subscription/invalidation path; optimistic cancellation
with rollback and explicit conflict/error state.

**Acceptance:** every consumer sees one consistent state; errors never render as empty data;
no duplicate hand-written wire DTO; subscription is disposed once; stale events cannot
overwrite a newer snapshot.

### III-E3 — Fleet/runner management UI

**Owns:** `frontend/src/features/fleet/**` for runner additions, new runner/profile settings
features, focused/a11y tests and E3 handoff. **Depends on:** E2.

**Tasks:** enrollment/revocation; health/capacity/protocol/harness display; membership;
agent/model profiles; unavailable reasons. Put legacy control planes in a clearly labeled
compatibility section.

**Acceptance:** every support value has visible reason; credential displays once only;
keyboard/a11y pass; stale/unconfigured runner never appears healthy.

### III-E4 — Item/Sprint “Run with agent” and activity

**Owns:** new execution feature UI plus bounded Board, item-detail and Sprint dispatch edits;
focused/a11y/E2E tests and E4 handoff. **Depends on:** E2.

**Tasks:** one shared modal for exact runner/fleet, profile, harness/provider/model; resolved
default provenance; disabled reasons; request/attempt timeline; cancel/reconcile controls;
never directly mutate item status.

**Acceptance:** all three surfaces create the same payload; unsupported combination cannot
submit; request appears without navigation; ambiguous state requires explicit operator
action; keyboard/focus path passes.

### III-E5 — CLI/MCP execution surface

**Owns:** execution additions to `tack-cli` client/commands/MCP, focused tests and E5 handoff.
**Must not edit:** backend/router/OpenAPI.

**Tasks:** list runners/fleets; create/list/cancel/reconcile execution; inspect attempts/events;
use stable shapes/conditional writes; avoid enrollment secrets in process arguments.

**Acceptance:** CLI/MCP request equals UI request; conflicts and `needs_operator` are distinct;
credential/config files are atomic and owner-only.

### III-E6 — Phase 54 integration/spec owner

**Owns:** scheduler service wiring, route/spec/generated updates and cross-surface E2E only
after E1–E5; E6 handoff.

**Acceptance:** healthy fleet selection, saturation, exact runner, unsupported model and
realtime updates pass through production routes in UI and CLI; generated drift clean.

---

## Wave 5 — Phases 55–56, decisions/artifacts/models/usage

### III-F1 — Scoped decisions

**Owns:** new decision repository/service/handler modules and focused tests; no router,
migration or generated edits; F1 handoff.

**Tasks:** runner may raise/read its attempt's decision but never resolve it; operator scope
resolves; expiry fail-closed; replay/idempotency; optional status mapping only after commit
through workflow engine.

**Acceptance:** self-resolution and cross-attempt access denied; restart preserves pending;
expiry records deny/audit and never marks item done.

### III-F2 — Events and verified artifacts

**Owns:** new event/artifact service/storage modules, retention tests and F2 handoff.

**Tasks:** atomic event batch/checkpoint; bounded payload/truncation; artifact manifest,
checksum/size/content-type validation; safe reference/path storage; streaming content and
retention behavior.

**Acceptance:** checkpoint never advances after failed insert; traversal/compression bomb and
oversize rejected; checksum mismatch stages nothing; large fixture is streamed, not buffered
as a whole.

### III-F3 — Model resolution and usage provenance

**Owns:** new pure model-policy/usage modules in core/orch, repository/service handlers,
focused tests and F3 handoff; no router/migration/generated edits.

**Tasks:** request override → agent profile → project → fleet precedence; intersect with
runner capability before claim; opaque ids; actual fact snapshot; nullable measured/estimated
usage with sources; runner time cost separate from model/token cost.

**Acceptance:** all presence combinations deterministic; nonsense id round-trips; unavailable
choice never leases; absent usage never serializes as zero; requested/actual mismatch visible.

### III-F4 — Decisions/artifacts/model frontend integration

**Owns:** execution feature UI additions and one generated-artifact integration after F1–F3
APIs are accepted; focused/a11y/E2E tests and F4 handoff.

**Tasks:** normalized timeline, decision inbox, verified artifact download, model provenance,
honest usage/economics. No provider-kind feature checks.

**Acceptance:** `Not measured` is exact for absent usage; pending/expired differ; artifact
failure visible; disabled controls name reason; interactions keyboard accessible.

### III-F5 — Runtime retention and observability

**Owns:** execution retention/metrics/health modules, startup/shutdown wiring assigned at
integration, soak tests and F5 handoff.

**Tasks:** cancellable retention child, bounded batches, runner/queue/lease/event metrics,
stuck/ambiguous alerts and graceful shutdown; no prompt/model contents in metric labels.

**Acceptance:** stale raw rows roll up/purge in production runtime; shutdown joins task; soak
is bounded; stale lease and `needs_operator` are observable.

---

## Wave 6 — Phase 57, legacy bridge and release

### III-G1 — Docket compatibility decision/bridge

**Owns:** legacy adapter/import code and Docket-specific tests/docs only; no neutral-contract
changes; G1 handoff.

**Tasks:** inventory real legacy value/data; choose maintain/export/deprecate; if maintained,
map to normalized attempts using provider-scoped ids and one scheduling owner; prevent runner
and Docket dual dispatch; reconcile stale task/approval rows.

**Acceptance:** runner path works with Docket absent; legacy golden unchanged without an
approved migration; collision tests across two planes; explicit compatibility label/policy.

### III-G2 — Chaos, fencing, security and recovery audit

**Owns:** adversarial/integration tests and audit report only; return fixes to owning cards;
G2 handoff.

**Tasks:** kill API/runner/harness at every boundary; delay/reorder/replay; stolen/revoked
token; stale fence; disk full; corrupt journal/row; oversized event/artifact; symlink/path
attack; XSS/prompt rendering; multi-runner contention.

**Acceptance:** each case has a safe documented state; no blind duplicate execution,
credential leak, cross-attempt write or silent loss. Any failure reopens its owning phase.

### III-G3 — Operator, migration and recovery docs

**Owns:** public runner/fleet/model/decision docs, recovery runbook, migration guide,
architecture/crate-tour updates and G3 handoff. Preserve Parts I/II.

**Tasks:** install/enroll/revoke; local credentials; workspace/storage; capability matrix;
backup/restore; `needs_operator`; version compatibility; Docket state; non-loopback/security.

**Acceptance:** fresh-machine walkthrough succeeds; every public claim maps to a test;
volatile counts generated/removed; no workstation-specific paths/forensic transcript in user
docs.

### III-G4 — CI, packaging and release gates

**Owns:** `.github/workflows/ci.yml`, packaging/release scripts, runner service examples,
SBOM/checksum/provenance integration and G4 handoff.

**Tasks:** runner tests/coverage; fixture drift; fake adapters; migration crash; security/
chaos subset; full frontend/cross-browser; binary-size budget and packaged runner artifacts;
prove goldens tracked and pipelines use `pipefail`.

**Acceptance:** clean checkout passes; Tack/runner archives carry checksums/SBOM/provenance;
no live secret required; deliberate fixture/golden mutation fails CI.

### III-G5 — Final independent integration/release owner

**Owns:** status updates, explicitly assigned final compatibility fixes, release evidence and
G5 handoff. Must not waive a gate by editing its test.

**Tasks:** integrate waves in order; live smoke all three harnesses; two-runner capacity/
fencing and backup/restore including artifacts; Docket-absent startup; synchronize docs,
status and OpenAPI; require clean tree/version/tag and rehearsed rollback.

**Acceptance:** definition below demonstrated with redacted evidence; every card has handoff;
no open P0/P1; tag matches package/docs; rollback works.

---

## Wave 7 — Phase 57 (continued), release blocker

Created 2026-08-19 after III-G5 refused the tag. Wave 6 merged cleanly but left one P0
open, so the cycle's definition of done is unmet. These two cards close it. **III-H1 must
land before III-H2 can produce evidence** — H2's smoke is the acceptance gate for H1.

### III-H1 — `tack-runner` HTTP transport

**Owns:** `crates/tack-runner/src/client.rs` (a real `RunnerProtocolClient`),
`crates/tack-runner/Cargo.toml` (the `reqwest` dependency), `main.rs`'s wiring, and the
III-H1 handoff. **Does not own** any server-side crate, the contract directory, or
`router.rs` — if the wire shape appears wrong, the fixture is right and the client is
wrong; escalate rather than change either.

**Context — the exact gap.** `UnavailableProtocolClient` is the only production
implementor and returns `Err(RunnerError::ProtocolUnavailable)` for everything;
`FakeClient` (in `runtime.rs`) is test-only; `reqwest` is not a dependency of the crate.
The 14 `/api/runner/v1` routes, their auth, fencing and error envelopes are all built and
server-side tested. This card writes the client half and nothing else.

**Tasks:** implement all 14 operations (enroll, refresh, claim, heartbeat, and the
attempt-scoped accept, start, events, decisions, decisions/poll, artifacts,
artifacts/{id}/content, completion, cancellation-observation, recovery-observation)
against `docs/contracts/runner-v1/`; carry the hashed bearer credential and the fencing
token on every attempt-scoped call; map `ProtocolErrorEnvelope` back to the typed
`RunnerError` variants — `stale_lease` must arrive as `RunnerError`'s stale variant, not a
generic conflict; honor timeouts and bounded retry without ever blind-retrying an
ambiguous post-spawn state; keep `RunnerCredential`'s structural redaction intact.

**Acceptance:** the binary enrolls, claims, heartbeats, streams events, uploads an
artifact and completes against a live `tack serve`. Every error path returns its typed
variant, asserted against the frozen fixtures — not against hand-written JSON.
`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` is updated or
retired deliberately, with the reason recorded (it currently pins the gap this card
closes). No credential appears in any log, and a test asserts it.

### III-H3 — Repository checkout for a claimed task

**Blocks III-H2.** Order is H1 (done) → H3 → H2.

**What is missing, in plain terms.** Nothing in the runner can create a working copy of a
repository. Every task is meant to get its own private checkout so two tasks running at
once cannot overwrite each other's work, and so a failed task can be thrown away without
touching anything else. Until this exists, a task can be assigned to the runner and then
never starts — which means none of the coding tools can be exercised end to end, and
smoke steps 7–9 stay uncollectible.

**Owns:** `crates/tack-runner/src/workspace.rs` (a real `WorktreeProvisioner`), whatever
git plumbing it needs, and the III-H3 handoff. **Does not own** the protocol client, any
server-side crate, or the contract directory.

**Context.** `UnavailableWorktreeProvisioner` is the only production implementor today;
the other two (`FakeProvisioner`, `FakeWorktree`) are test doubles. This is the same shape
of gap III-H1 closed on the protocol side, and it was missed because the board claimed the
transport was the only missing piece — a completeness claim nobody had verified.

**Tasks:** provision an isolated checkout per attempt from the item's linked repository;
clean it up on completion, cancellation and crash-recovery; never leave a half-made
worktree that a later attempt could inherit; keep the runner's owner-only permissions on
anything written to disk.

**Acceptance:** a claimed attempt reaches a real harness process with its own checkout,
proven against a real git repository rather than a fake. Two concurrent attempts cannot
see each other's files. A killed runner leaves no unusable worktree behind — proven by
killing it mid-provision and restarting. `./scripts/smoke.sh` reaches step 7 or beyond.

### III-H2 — Live three-harness smoke and release

**Owns:** `scripts/smoke.sh`, the release evidence, the tag, and the III-H2 handoff.

**Tasks:** run the end-to-end smoke against real harness binaries; collect the evidence
III-G5 listed as uncollectible; only then tag.

**Acceptance:** the definition of done in §III.6 is demonstrated with redacted evidence,
or each unmet criterion is named. **Harness availability is reported honestly** — as of
2026-08-19 this machine has `claude` 2.1.235 and `opencode` 1.18.0 on PATH but **no
`codex` binary**, so a three-harness claim cannot be made here without installing it. Two
of three verified is a two-of-three result, never rounded up.

### III-H4 — A runner that loses a credential-rotation race is told the wrong thing

**Found by CI on 2026-08-19, not by a card.** Does not block III-H2's work, but the
answer should be settled before anything is tagged: the current behaviour can stop a
healthy runner.

**What goes wrong, in plain terms.** When a runner renews its credential twice at the
same moment, one renewal wins and the other must be told "someone beat you, try again".
Under load the loser is instead told "your credential is not valid". Those two answers
mean opposite things to a runner: the first is a retryable conflict it backs off from,
the second reads as a dead credential, which a client would reasonably treat as fatal and
stop on. So a runner that lost a harmless race can shut itself down for no real reason.

**Why it happens.** The winner rotates the credential away before the loser's request is
authenticated, so the request is rejected by the auth layer (401) before the rotation
logic that would have returned the retryable conflict (409) ever runs. The check that
would classify it correctly is downstream of the check that rejects it.

**Owns:** the runner-credential refresh path in `crates/tack-api/src/handlers/` and its
tests, plus the III-H4 handoff. **Does not own** the runner client's retry policy — if
the conclusion is that the client should treat this 401 differently, that is a request to
the `tack-runner` owner, recorded rather than made.

**Evidence.** `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten`
(`crates/tack-api/tests/c2_handlers_test.rs:1719`) failed in CI run `32309301344` with
`conflicts.len()` = 0: one request returned 200 and the other 401, where the test
requires 200 + 409. It does **not** reproduce locally — verified with three isolated runs
and three full-binary runs under `--features embed-spa`, 32/32 green each time. The
GitHub runner is slower and more contended, which is what makes the window observable.

**Tasks:** reproduce the race deterministically rather than by timing — a synchronisation
point in the handler under a test-only hook, or by driving the two requests through the
same ordering the runner would hit; decide what a losing rotation *should* return and
write that decision down; make the outcome deterministic so the loser always learns it
can retry.

**Acceptance:** the race is reproduced by a test that fails reliably before the fix and
passes after — proven by reverting the fix and watching it fail, not by observing CI
luck. A losing concurrent rotation returns one documented, retryable outcome every time,
asserted against the frozen error fixtures. **Muting, deleting, retrying or
`#[ignore]`-ing the existing test is not an acceptable resolution** — its failure is the
only evidence this behaviour exists. If the conclusion is that 401 is in fact correct,
that must be argued in the handoff and the runner's handling of it must be checked, not
assumed.

---

## Wave 8 — Phase 57 (continued), unblock the tag

Created 2026-08-19 by the III-H2 integration (`01c7046`). H2 ran the product live and
refused the tag; these cards carry every escalation it and III-H1 left open. Base SHA
for every card: `84fabf1` on `develop` (was `01c7046` until III-H5 merged, 2026-08-20). III-H4 (above) joins this wave unchanged.
III-H4 and III-H7 both touch `crates/tack-api/src/handlers/runner_protocol.rs` —
coordinate or run sequentially; the other cards are disjoint and may run in parallel.

### III-H5 — Make claude-code and codex schedulable without faking capability

**P0, release-blocking, decision card.** Three individually-principled decisions compose
into an impossible product: the claude-code/codex adapter probes deliberately declare
zero `model_combinations` (D1/D2), the scheduler requires the requested pairing to be
declared (`crates/tack-orch/src/scheduler/select.rs`, `ModelCombinationNotDeclared`),
and `AutoSelect` is rejected for every candidate (`AutoSelectNotVerified`) — so with a
real `claude` installed and a heartbeating runner, a claude-code request sits `queued`
forever (proven live, III-H2 step 8, both modes, every run). **Owns:** the decision and
its implementation across the scheduler (E1/E6 lineage), the two adapters (D1/D2) and
the capabilities contract, plus the III-H5 handoff. **Tasks:** choose between the
candidate resolutions recorded in `III-H2.md` — an operator pass-through capability
attestation; operator-declared model combinations feeding eligibility (note
`model_profiles`, migration 043, is consulted by nothing today — F4 recorded the same);
or a verified auto-select attestation. Bending any single piece silently violates
"capability claims are load-bearing" — that is why this is a decision card.
**Acceptance:** a claude-code request on a runner with the real binary installed is
claimed and completed; III-H2's step 8 stops failing for that harness with **no smoke
edit**; capability claims stay honest (no hardcoded model list, no fake declaration).

### III-H6 — The runner engine submits events, decisions and artifacts

**Release-blocking.** `AttemptDataProtocol` has had transport since III-H1 but zero call
sites in the runner's `engine.rs`, so no real runner ever submits an event, decision or
artifact — two §III.6 criteria ("verified artifacts and an idempotent event timeline",
"resolve a bounded decision") cannot be demonstrated from a real runner. Open since
III-H1 escalation 3, re-escalated unchanged by III-H2. **Owns:** `engine.rs` wiring and
its tests, plus the III-H6 handoff. **Acceptance:** III-H2 step 7's UNMET line about
events/artifacts disappears on a live run with **no smoke edit**; idempotency asserted
against the frozen runner-v1 fixtures.

### III-H7 — Duplicate `runner_name` enrollment returns an unhandled 500

Reproduced by III-H2 with two curl enrollments differing only in token: first 200,
second 500 (ids-only server log shows the route failing). Two defects in one: the
collision deserves a typed protocol error (the fixture set has `conflict`), and the
enroll body's self-reported `runner_name` (defaulted from `TACK_RUNNER_ID`, identical
for any two default-configured runners on one host) silently competes with the
operator-assigned pending-runner name — until fixed, a second same-named runner on a
host cannot enroll at all. **Owns:** the enrollment path in
`crates/tack-api/src/handlers/runner_protocol.rs` and its tests, plus the III-H7
handoff. **Shares that file with III-H4.** **Acceptance:** a losing same-named
enrollment gets one typed, documented outcome every time, asserted against the frozen
error fixtures; III-H2's distinct-`TACK_RUNNER_ID` workaround becomes unnecessary.

### III-H8 — Fleet selection write route

§III.6 requires selecting "an exact runner **or fleet**"; `agent_fleet_members` has no
write route, so the fleet half is undemonstrable — standing since E6, restated by H2
because it is now release-relevant. **Owns:** the fleet-membership write route (API +
repository + OpenAPI regeneration) and its tests, plus the III-H8 handoff.
**Acceptance:** an operator can populate a fleet over the API and a fleet-targeted
request schedules onto a member; `docs/openapi.json` regenerated via the contract test,
never hand-edited.

---

## III.5 Cross-wave acceptance matrix

| Invariant | First owner | Must remain green through |
|---|---|---|
| No stored XSS/query secret log; exact auth routes | A1 | G5 |
| One logical mutation = one atomic version increment | A2 | G5 |
| Migration crash has deterministic recovery | A3 | G5 |
| Runner fixtures are authoritative | A0/B4 | G5 |
| One valid active lease; stale fence writes nothing | B2 | G5 |
| Ambiguous post-spawn crash never blind-retries | C4 | G5 |
| Runner and operator auth cannot substitute | C2/C5 | G5 |
| Same neutral request works through three harnesses | D5 | G5 |
| UI/CLI offer only eligible combinations | E1/E2/E5 | G5 |
| Decision cannot self-resolve; expiry fail-closed | F1 | G5 |
| Event checkpoint never passes failed persistence | F2 | G5 |
| Missing usage is `not_measured`, never zero | F3/F4 | G5 |
| Docket absence does not disable runner execution | G1 | G5 |
| Backup/restore preserves DB/artifacts and scrubs secrets | G2/G5 | release |

## III.6 Definition of done

The cycle is complete only when, from the same Tack item, an operator can create separate
attempts through Codex, Claude Code and OpenCode; select an exact runner or fleet; choose
only a supported provider/opaque model; see requested versus actual facts; resolve a bounded
decision; inspect verified artifacts and an idempotent event timeline; and recover from API,
runner or harness restart without silent loss or blind duplicate execution.

Additionally:

- Docket is optional and has one documented compatibility state.
- Every lease is fenced and every ambiguous attempt requires explicit reconciliation.
- Usage is measured/estimated with provenance or rendered `Not measured`.
- Full Rust/frontend/cross-browser/security/migration/chaos/backup gates are green.
- The integration tree is clean, public docs match runtime, and release artifacts are
  checksummed, signed/provenanced and tagged.

---
