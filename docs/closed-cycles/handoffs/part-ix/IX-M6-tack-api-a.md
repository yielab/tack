# IX-M6-tack-api-a handoff

- Base SHA / branch / final SHA: `957ffc3` (`develop`) / `agent/ix-m6-tack-api-a` /
  `3afc23f` (worktree: `/tmp/ix-m6-tack-api-a`,
  `CARGO_TARGET_DIR=/tmp/ix-m6-tack-api-a-target`).
- Files changed (must equal ownership list):
  - `crates/tack-api/src/dispatcher.rs`
  - `crates/tack-api/src/execution_runtime.rs`
  - `crates/tack-api/src/handlers/decisions.rs`
  - `crates/tack-api/src/handlers/executions.rs`
  - `crates/tack-api/src/handlers/items.rs`
  - `crates/tack-api/src/handlers/orch.rs`
  - `crates/tack-api/src/handlers/runner_admin.rs`
  - `crates/tack-api/src/handlers/runner_protocol.rs`
  - `crates/tack-api/src/handlers/runner_protocol/artifact_storage.rs`
  - Plus five `docs/dev-notes/tack-api/**.md` deletions (see *What was removed*) — not
    on the card's `.rs` ownership list, but the card's acceptance criteria explicitly
    calls for resolving any dev-notes entry that references one of the nine files above.
- Contract fixtures consumed: none.
- Behavior implemented: none — comment-only card. No production logic, function
  signature, or behavior changed in any file; confirmed by `measure`'s `prod` column
  (non-comment line count) being identical before/after for every file (see *Measured
  numbers*).
- Tests added and exact commands/results: none added or changed — out of scope for a
  comment-only card, and no test asserted on the text of a removed comment.
- Failure/adversarial case proved: n/a (no behavior change).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced.
- Secrets/logging review: n/a — no logging or secret-handling code touched, only
  doc/line comments.
- Safe merge order and likely conflicts: touches only the nine files this card owns
  plus five dev-notes files under `docs/dev-notes/tack-api/`; no other Wave 31 IX-M6
  batch owns any of these paths (per the dispatch prompt). Low conflict risk. Should
  merge cleanly against any sibling IX-M6 batch working a disjoint `tack-api` file set.
- Checklist: no unowned `.rs` files touched; no live secret; no panic stub; no blind
  retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Every `///` doc block in the nine owned files is ≤ 15 lines | `python3 scripts/maintainability.py comment-worklist --json` filtered to the nine files returns zero rows (was 11) |
| Every module/file preamble in the nine files is ≤ 30 lines | same worklist run, zero `module preamble` rows for these files, both before and after |
| Comment share ≤ 35% in every owned file | `python3 scripts/maintainability.py measure <9 files>` — max is `dispatcher.rs` at 30% (see *Measured numbers*) |
| No behavior change | `prod` column (non-comment lines) in `measure`'s output is byte-identical before/after for all nine files; `cargo check --workspace` is green |
| No card/wave/phase/date/attribution reference introduced | `./scripts/check-comments.sh` passes: `✓ no board archaeology in crates/ frontend/src frontend/e2e` |

## Measured numbers

`python3 scripts/maintainability.py measure <file>` for each of the nine files, before
(`git show 957ffc3:<path>`, via `git stash`) and after this card's edits. Columns:
`prod` = non-comment production lines (unchanged everywhere, confirming no logic
touched), `cmnt` = comment lines, `cm%` = comment share, `mdoc` = module/file preamble
length in lines.

| File | prod | cmnt before → after | cm% before → after | mdoc before → after |
|---|---|---|---|---|
| `dispatcher.rs` | 383 | 182 → 170 | 32% → 30% | 8 → 5 |
| `execution_runtime.rs` | 215 | 113 → 82 | 34% → 27% | 8 → 11 |
| `handlers/decisions.rs` | 482 | 133 → 136 | 21% → 22% | 8 → 21 |
| `handlers/executions.rs` | 1104 | 259 → 247 | 19% → 18% | 1 → 1 |
| `handlers/items.rs` | 488 | 96 → 58 | 16% → 10% | 0 → 0 |
| `handlers/orch.rs` | 2088 | 760 → 766 | 26% → 26% | 8 → 21 |
| `handlers/runner_admin.rs` | 955 | 90 → 85 | 8% → 8% | 1 → 1 |
| `handlers/runner_protocol.rs` | 1725 | 445 → 433 | 20% → 20% | 22 → 22 |
| `handlers/runner_protocol/artifact_storage.rs` | 189 | 57 → 61 | 23% → 24% | 8 → 16 |
| **totals (these 9 files)** | 7629 | 2135 → 2038 | — | — |

`mdoc` grew on three files (`execution_runtime.rs`, `handlers/decisions.rs`,
`handlers/orch.rs`, `artifact_storage.rs`) because a truncated module preamble (cut
mid-sentence by IX-M2's earlier extraction, pointing to a now-deleted
`docs/dev-notes/...md` file) was rewritten as a complete, self-contained preamble that
folds in the still-load-bearing rationale from the deleted dev-notes file — every one
stays well under the 30-line budget. `handlers/orch.rs`'s and `handlers/decisions.rs`'s
overall `cm%` rose by ≤1 point for the same reason (net of the 11 trimmed doc blocks
elsewhere in those same files); no file crosses 35%.

`python3 scripts/maintainability.py comment-worklist --json`, filtered to these nine
files: 11 violations before this card, 0 after.

## What a stranger still cannot do

Nothing changed here — this card is comment-only. A stranger reading these nine files
gets the same capabilities and the same API surface as before; the only difference is
that the non-obvious "why" for a given function is now stated directly above it in a
budget-sized block instead of requiring a trip to `docs/dev-notes/tack-api/*.md` (five
of which no longer exist, their content redistributed).

## What was removed

Reason-class key from `docs/plans/human-maintainability.md` §3: `doc block` (over the
15-line `///` budget), `preamble` (over the 30-line module-doc budget — none here, all
nine files were already compliant on this axis), `design → dev-notes resolved` (a
`docs/dev-notes/**` entry whose subject was one of the nine files, folded back into a
compliant in-source comment since no ADR gap existed and nothing was vendor-specific).

**11 `doc block` trims** (11 lines → new line count, budget 15):
- `dispatcher.rs:374` `apply_mapped_status` doc: 27 → 14
- `dispatcher.rs:586` `resolve_default_trust` doc: 18 → 11
- `execution_runtime.rs:220` `spawn_artifact_and_decision_sweep` doc: 47 → 12 (dropped
  the "Artifact storage root" and "No injected clock" sub-sections — both fully
  duplicated by `ExecutionRuntimeConfig::storage_dir`'s own doc comment and by the
  call-site comment referencing it; kept the "one shared enabled/schedule" rationale,
  the safety-relevant one)
- `handlers/decisions.rs:133` `require_decision_token` doc: 21 → 10 (dropped the
  restated "no secret configured, so skip the check" contrast — already stated once,
  identically, on `handlers::orch::require_approval_token`, which this comment already
  points to)
- `handlers/executions.rs:29` `RunnerV1ErrorEnvelope` doc: 27 → 14
- `handlers/items.rs:357` `maybe_auto_dispatch` doc: 53 → 15 (dropped the "Before this
  fix the check below was `!state.config.orch_enable`... see CHANGELOG.md" paragraph —
  historical narrative of a prior bug fix, exactly what `.claude/scope-discipline.md`'s
  Comments section calls out to remove; kept the current-fact statement that the gate
  reads the effective setting, not the raw env value)
- `handlers/orch.rs:2642` `require_approval_token` doc: 22 → 13
- `handlers/runner_admin.rs:760` `provision_local_runner` doc: 19 → 13
- `handlers/runner_protocol.rs:493` `validate_capability_payload` doc: 22 → 13
- `handlers/runner_protocol.rs:688` `reclassify_refresh_auth_error` doc: 17 → 11
- `handlers/runner_protocol/artifact_storage.rs:44` `encode_id` doc: 16 → 11

**5 dev-notes entries resolved** (all `design → dev-notes resolved`, no ADR needed):
- `docs/dev-notes/tack-api/dispatcher.md` (103 lines) — deleted. Its "one scheduling
  owner" section is now adequately covered by the existing inline comment at
  `dispatcher.rs:222-235` (added since IX-M2's extraction) plus ADR 0060/0065's context;
  its "Trust is not optional" and "Idempotency and attempt" sections were folded back as
  short, compliant comments on `dispatch_item`'s doc comment and above the `next_attempt`
  computation, respectively.
- `docs/dev-notes/tack-api/execution_runtime.md` (53 lines) — deleted. Its content
  (why this file is thin, the second tack-api-local sweep) is now covered by the
  rewritten module preamble plus the trimmed `spawn_artifact_and_decision_sweep` doc
  above.
- `docs/dev-notes/tack-api/handlers/decisions.md` (78 lines) — deleted. Its
  `TACK_EXECUTION_DECISION_TOKEN` section was redundant with `require_decision_token`'s
  own doc comment (already present in source); its "Security boundary" and "No
  item-status mapping" sections were folded into a rewritten, complete module preamble
  (the prior one was truncated mid-sentence by IX-M2's extraction).
- `docs/dev-notes/tack-api/handlers/orch.md` (36 lines) — deleted. Folded into a
  rewritten, complete module preamble (same truncation issue as decisions.rs).
- `docs/dev-notes/tack-api/handlers/runner_protocol/artifact_storage.md` (44 lines) —
  deleted. Its three "properties proved by tests" were already substantially covered by
  `store_streaming`'s and `safe_attempt_dir`'s own doc comments in source; folded a
  condensed version into a rewritten, complete module preamble.

**Dev-notes entries referencing one of the nine files but NOT resolved** (out of
scope — their primary subject is a file this card does not own):
- `docs/dev-notes/tack-api/handlers/runner_protocol/artifact_download.md` — its own
  subject is `artifact_download.rs` (not in this card's file list); it mentions
  `artifact_storage.rs` and `runner_protocol.rs` only in passing, to explain its own
  module's relationship to them. Left untouched.
- `docs/dev-notes/tack-orch/scheduler/mod.md`, `scheduler/wiring.md`,
  `model_policy/wiring.md` — each is `tack-orch`'s own dev-notes file (not owned by
  this card, not even in `tack-api`) and each cites `handlers/runner_protocol.rs` or
  `handlers/runner_admin.rs` once, in passing, to explain a `tack-orch` call site.
  Left untouched; editing them would touch files outside this card's crate.

If none of a card's nine files had a dev-notes entry at all, that would be worth
stating plainly — it is not the case here: 5 of 9 did, all now resolved.

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` is unchanged (`git diff --cached
scripts/maintainability-baseline.json` is empty) — every file in scope was already
under its budgets in every dimension the baseline tracks; only the comment-worklist's
per-block/per-file live checks (not baseline-relative) were failing, and those are now
clean.

## Context spent

- Tokens read before the first edit (cold start): read `CLAUDE.md` (project root and
  repo), `.claude/scope-discipline.md` in full, the nine files' relevant line ranges (not
  read whole — targeted via `sed`/`Read` with offsets around each worklist hit), the
  Part IX handoffs README + one example handoff (`IX-M5-harness-core-codex.md`) for
  format, and the five relevant `docs/dev-notes/tack-api/**.md` files in full (needed in
  full to decide what was safe to drop vs. fold back).
- Context size at handoff: moderate — did not read `TODO.md`'s Part IX board section at
  all (the dispatch prompt's acceptance criteria were complete, as advertised), did not
  read any file outside the nine owned plus the dev-notes tree plus a handful of ADRs
  checked for overlap (`0060`, `0065`).
- Files opened and not used: `docs/adr/0060-docket-control-plane-disposition.md` and
  `0065-docket-pipeline-dispatch-trigger.md` were read to check for ADR overlap before
  deciding no new ADR was needed for `dispatcher.md`'s content — used as evidence, not
  wasted.
- Read-list lines that were wrong: none — the dispatch prompt's file list and the
  worklist tool's output matched exactly; no surprise violations turned up outside the 9
  files or the 11 listed lines.

## Amendments

*(none yet)*
