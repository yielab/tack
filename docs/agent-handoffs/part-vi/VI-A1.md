# VI-A1 handoff

- Base SHA / branch / final SHA: base `develop@4740ee0` (Wave 14's dispatch table in this
  README records `c6407dc`, the commit two before it; `4740ee0` is a planning-only commit
  on top that recorded that same base SHA into the board, so branching from the later tip
  is equivalent — noted here rather than silently picking one) / branch
  `agent/vi-a1-agent-runner-docs` / final SHA: not committed — the card instructions
  said to leave the branch uncommitted, so this handoff describes a dirty working tree,
  not a commit.
- Files changed (must equal ownership list): `docs/book/src/user-guide/agent-runners.md`,
  `docs/book/src/user-guide/cli.md`, `docs/book/src/user-guide/configuration.md`,
  `docs/book/src/user-guide/quick-start.md`, `docs/API-REFERENCE.md`, `docs/CONFIG.md`,
  `docs/agent-handoffs/part-vi/VI-A1.md` (this file). Matches the card's Owns list
  exactly — confirmed with `git status --porcelain` showing no other file touched.
- Contract fixtures consumed: none. Pure documentation; no `docs/contracts/runner-v1/`
  fixture was read or changed.
- Behavior implemented: none — this card owns no runtime code. The five doc changes are
  listed above.
- Tests added and exact commands/results: no automated test suite exists for prose;
  "tests" here means the live verification runs, all against a real
  `tack serve --with-runner` on a scratch SQLite DB with a stand-in `claude` shim binary
  on `PATH` (codex/opencode were real installed binaries on this machine, unused for the
  worked examples to keep them free and deterministic). Every command below actually ran:
  - `mdbook build docs/book` — clean (no warnings, no errors).
  - `mdbook build docs/book 2>&1 | grep -i "error\|broken"` (the exact CI check in
    `.github/workflows/ci.yml:292`) — no output, i.e. passes.
  - `grep -rn claude_code docs/book/src/` — two matches remain, both explained under
    "Known limitations" below, neither is the harness-id bug the gate is checking for.
  - `tack execution create` (full 13-field CLI invocation) → `exec_0fe725...` →
    `succeeded`. Raw JSON `POST /api/executions` with the identical shape →
    `exec_1ecec9...` → `succeeded`.
  - Tier-2 precedence probe: `tack agent-profile create --limits
    '{"default_model":{"provider":"anthropic","model_id":"claude-sonnet-4-5"}}'` then
    `tack execution create` with no `--model-provider`/`--model-id` → `exec_949e94...` →
    `succeeded`, `actual_execution.model_provider: "anthropic"`.
  - Tier-4 precedence probe: `tack fleet create --policy
    '{"default_model":{"provider":"anthropic","model_id":"claude-opus-4-1"}}'`, runner
    added as a fleet member, `tack execution create --fleet <id>` with no model flags and
    an agent profile with no default of its own → `exec_8509b4...` → `succeeded`,
    `actual_execution.model_provider: "anthropic"`.
  - Auto-select probe: raw JSON POST with `requested_model_provider`/`requested_model_id`
    both `null` and no tier resolving → `exec_9b3c67...` accepted as `queued`; after
    settling, `GET .../attempts` returned zero attempts, request still `queued`.
  - `curl -X POST .../runner-fleets/{id}/members` → `{"state":"added"}` (live proof the
    fleet-membership write route works).
  - `tack runner doctor` — real output on this machine (codex 0.149.1, claude-code
    2.1.252, opencode 1.18.0 all present).
  - Every `--help` shown in `cli.md` (`execution`, `execution create/list/get/cancel/
    reconcile`, `runner`, `runner doctor`, `runner start`, `runner enroll`, `fleet
    create/list`, `agent-profile create/list`, `model-profile create/list`) was run, not
    paraphrased from memory or the OpenAPI spec.
  - `python3 -c "import json;...['CreateExecution']['required']"` against
    `docs/openapi.json` — confirmed the 13 required fields cited on the page.
- Failure/adversarial case proved: the auto-select probe above. Leaving every model tier
  empty is not rejected at request-creation time (the request is accepted and stored as
  `queued`), but the scheduler rejects every candidate for it
  (`IneligibleReason::AutoSelectNotVerified`,
  `crates/tack-orch/src/scheduler/select.rs`), so the request never gets an attempt and
  no error is ever surfaced to an operator. This was asserted directly (attempts list
  stayed empty, request state stayed `queued`) rather than inferred from a status code.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - The worked examples' `actual_execution.model_id` reads `"unknown"` with
    `model_observation_source: "not_observed"` — an artifact of the stand-in `claude`
    shim never emitting the structured output a real Claude Code CLI does, not a product
    defect. The page says this explicitly rather than editing the real output to hide it.
  - `grep -rn claude_code docs/book/` is **not** literally empty: `developer/crate-tour.md:406`
    names the real Rust source file `claude_code.rs` (correct — that really is the
    filename; changing it would be wrong), and `roadmap.md:3284` is the roadmap's own
    prospective description of the very bug this card fixes ("VI-A1 fixes the first
    three"), written before the fix landed — `roadmap.md` is explicitly out of this
    card's ownership and on the dispatch block's do-not-read list, so it was left
    untouched. Both are explained false positives, not the wire-id bug.
  - `configuration.md` keeps its "Example tack.toml", "API Token" and "Logging"
    subsections even though "Logging" overlaps `docs/CONFIG.md`'s "Debugging" section —
    only the stale "Full Reference" variable table (missing every runner/orchestration/
    execution row) was removed and replaced with a pointer, since that table was the
    actual claim-to-be-the-reference defect; the small Logging overlap is low-risk
    (three unchanging one-line examples) and removing it read as scope creep beyond the
    card's explicit ask.
- Secrets/logging review: no secret touched. The demo server ran on a scratch SQLite DB
  under the session scratchpad directory with a fake enrollment flow
  (`tack serve --with-runner` self-provisions); nothing from that DB is referenced by
  path in committed doc content except the demo's own throwaway ids, which are
  meaningless outside that ephemeral database.
- Safe merge order and likely conflicts: Wave 14 cards (A1/A2/A3) may land in any order
  per the dispatch README. The one adjacency worth the integrator's attention:
  `docs/CONFIG.md` — VI-A2 owns lines 89–108 (the "Vendor/provider credentials" bullet);
  this card inserted a new bullet immediately **after** line 108, before "Log
  visibility." If VI-A2's edit changes that bullet's line count, the insertion point
  will still merge cleanly (it's an insertion between two bullets, not a line-range
  edit), but a diff tool may show it as adjacent-context noise — worth a visual check,
  not a real conflict.
- Checklist: no unowned files touched (verified via `git status --porcelain`); no live
  secret (all ids are from an ephemeral local demo database); no panic stub (no code);
  no blind retry (no code).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A reader can reach a completed attempt using only the new "Running an item with an agent" section | `tack execution create` (13 fields) → `exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1` → `tack execution get` shows `state: succeeded (done)` |
| The same request as raw JSON works identically | `POST /api/executions` with the equivalent body → `exec_1ecec9359d33039dedfd637df7fa4af87dca282c23437509516332e0adf243aa` → attempt `succeeded` |
| The harness id is `claude-code`, not `claude_code` | `crates/tack-runner/src/harness/claude_code.rs:105` (`const HARNESS_KIND: &str = "claude-code"`); live `tack runner doctor` output lists `claude-code`; `docs/contracts/runner-v1/capabilities.json` uses `codex`/hyphenated ids |
| Agent-profile `limits.default_model` (tier 2) has real runtime effect | `tack agent-profile create --limits '{"default_model":{"provider":"anthropic","model_id":"claude-sonnet-4-5"}}'` then `tack execution create` with no model flags → attempt's `actual_execution.model_provider == "anthropic"`; resolution call site `crates/tack-api/src/handlers/executions.rs:566-596` |
| Fleet `default_policy.default_model` (tier 4) has real runtime effect, only for fleet-targeted requests | `tack fleet create --policy '{"default_model":{...,"claude-opus-4-1"}}'`, request via `--fleet`, no model flags, profile with no default → `actual_execution.model_provider == "anthropic"` |
| Project default model tier has no storage | `crates/tack-orch/src/model_policy/wiring.rs` doc comment + `resolve_request_model_policy` body: always passes `None` for `project_default` |
| Auto-select is accepted at creation but never schedules, with no visible error | Raw JSON POST with both model fields `null` → `queued`; `GET .../attempts` returns `{"data":[]}` after settling; `IneligibleReason::AutoSelectNotVerified` in `crates/tack-orch/src/scheduler/select.rs` |
| `model_combinations`/`model_passthrough` gate eligibility at claim time | `crates/tack-orch/src/scheduler/select.rs:139-175`; live `tack runner doctor`: codex/claude-code declare zero combinations and `model_passthrough: supported`, opencode declares real combinations and `model_passthrough: unsupported` |
| `model_profiles` is a UI convenience list, never read by the scheduler | `crates/tack-orch/src/model_policy/wiring.rs` (no reference to the `model_profiles` table); `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:178-183` (modal copies the selected profile's pair into `requested_model_provider`/`requested_model_id`) |
| `agent_fleet_members` has a working write route the UI doesn't call | `curl -X POST http://127.0.0.1:3457/api/runner-fleets/fleet_64ab2a19-.../members -d '{"runner_id":"runr_8fa0dfb9-..."}'` → `{"state":"added"}`; route at `crates/tack-api/src/handlers/runner_admin.rs:60,390` |
| No decision/artifact list route exists (unchanged claim, re-verified) | `grep -rn "route(" crates/tack-api/src/handlers/runner_admin.rs crates/tack-api/src/handlers/runner_protocol.rs` shows only `POST .../decisions`, no `GET .../decisions` or `GET .../artifacts`; `DecisionInbox.tsx`/`ArtifactDownloadPanel.tsx` still exist at `frontend/src/shared/runWithAgent/` |
| `cli.md`'s 5 new sections reflect real `--help` output | Every command in the "Tests added" list above was executed with `cargo run -q -p tack-cli -- <cmd> --help` or the built binary, not written from memory |
| `configuration.md` and `docs/CONFIG.md` no longer both claim to be the reference | `docs/CONFIG.md` remains the complete table; `configuration.md`'s "Full Reference" table (stale — missing every `TACK_LOCAL_RUNNER_*`/`TACK_RUNNER_*`/`TACK_EXECUTION_*`/`TACK_BACKUP_*` row) was removed and replaced with a pointer sentence naming `docs/CONFIG.md` as authoritative |
| `mdbook build docs/book` is clean | `mdbook build docs/book` and the CI's own `... \| grep -i "error\|broken"` check both ran with no output/errors |

## Measured numbers

- Files changed: 6 doc files + this handoff. `git diff --stat`: `636 insertions(+), 34
  deletions(-)` across the 6 doc files.
- `mdbook build docs/book`: 0 errors, 0 warnings.
- Live execution requests created during verification: 7 (`exec_9b3...` queued/auto-select
  probe, `exec_1ec...`, `exec_0fe...`, `exec_14a...`, tier-2 probe `exec_949e9...`, tier-4
  probe `exec_8509b...`, plus one earlier noisy run discarded for a cleaner transcript).
  6 of 7 reached `succeeded`; the 7th (auto-select) stayed `queued` by design — see
  "Failure/adversarial case proved."
- `tack runner doctor` on the verification machine: 3/3 harnesses present (`codex`
  0.149.1, `claude-code` 2.1.252, `opencode` 1.18.0).
- `docs/openapi.json` `CreateExecution.required`: 13 fields (unchanged from the board's
  own evidence table — re-confirmed, not re-derived from scratch).

## What a stranger still cannot do

After this card, a stranger can go from an installed `tack` binary to a completed agent
attempt using only the book — that gap is closed. What they still cannot do: use the web
UI's own "Run with agent" modal without first finding a runner id by hand (no runner
picker; the modal's runner field is still free text) or knowing in advance that leaving
the model on the modal's default "Auto" setting will create a request that silently never
runs (accepted, queued forever, no error anywhere in the UI) — both are documented here as
known behavior, not fixed, since fixing either is UI/scheduler work outside a pure-docs
card. A stranger also still cannot add a runner to a fleet from the UI (the route exists;
nothing calls it) or discover a decision/artifact id without already having it from the
event timeline.

## Surface-map delta

None. This card changed no runtime surface — every row of §VI.0's evidence table it
touches was a documentation-accuracy correction (the harness id, the two false
Known-gaps bullets, the two-configuration-reference conflict), not a console-to-UI move.
Say so explicitly per the dispatch block: no surface-map row moved.

## Context spent

- Tokens read before the first edit (cold start): close to the block's ~22k estimate for
  the named read list (README header + card block, `agent-runners.md`/`cli.md`/
  `configuration.md`/`quick-start.md` whole, `CONFIG.md` whole, the two small
  `API-REFERENCE.md`/`MCP.md` excerpts, `model_policy/{wiring,mod}.rs` excerpts, the CLI
  `--help` runs, the `openapi.json` required-fields query).
- Context size at handoff: comfortably under the ~150k stop threshold; the largest
  additional cost was the live-verification round (roughly a dozen `curl`/CLI round
  trips against the scratch server), each individually small.
- Files opened and not used: none discarded outright, but three source files **beyond**
  the named read list turned out to be load-bearing and are a correction to this read
  list for the next docs card: `crates/tack-orch/src/scheduler/select.rs` (the
  `model_combinations`/`model_passthrough`/`AutoSelectNotVerified` gating logic — not
  visible from `model_policy/wiring.rs` alone), `crates/tack-orch/src/scheduler/types.rs`
  (the `IneligibleReason` enum's doc comments, which state the auto-select policy
  explicitly), and **`crates/tack-api/src/handlers/executions.rs`** (the actual
  request-creation call site that invokes `resolve_request_model_policy` — this is the
  single fact that makes tiers 2 and 4 real rather than theoretical, and it is not
  mentioned anywhere in the named read list's `model_policy` pointers). A future card
  touching model precedence should read `executions.rs`'s model-resolution block
  directly rather than only the `model_policy` module's own doc comments.
- Read-list lines that were wrong: not wrong so much as incomplete — the
  `model_policy/{wiring,mod}.rs` pointers describe the pure resolution function and its
  *scheduler*-side integration, but the block never names the *API-handler* call site
  that actually wires tiers 2–4 into every real request. Also read, beyond the block, and
  worth recording rather than hiding: `TODO.md` lines 95–135 (§VI.0's cold-start
  statement and evidence table in full, ~40 lines) — needed verbatim to satisfy this
  card's own Task 1 ("the page opens with the §VI.0 statement"), and `docs/book/src/roadmap.md`
  lines 3275–3295 (~20 lines) — on the block's do-not-read list, opened briefly to confirm
  the exact wording of the "docs contradict the code in four places" claim before
  rewriting the Known-gaps bullets; both were small, targeted, and paid for themselves
  by cross-verifying claims against live behavior rather than trusting the roadmap's own
  prose.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
