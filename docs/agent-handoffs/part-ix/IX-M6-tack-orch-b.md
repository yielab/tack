# IX-M6-tack-orch-b handoff

**One of a disjoint pair of tack-orch comment-trimming sub-cards under IX-M6.** A sibling
agent worked a different file set (`adapters/docket.rs`, `adapters/github_actions.rs`,
`adapters/registry.rs`, `execution/capabilities.rs`, `execution_retention.rs`, `lib.rs`,
`reconciler.rs`, `scheduler/batch.rs`) in another worktree/branch at the same time; its
result is not visible here and is not assumed. This sub-card's scope is comments only — no
production logic, function signature, or behavior changes.

- Base SHA / branch / final SHA: `35deddf` (`develop`) / `agent/ix-m6-tack-orch-b` /
  recorded at commit time (worktree: `/tmp/ix-m6-tack-orch-b`,
  `CARGO_TARGET_DIR=/tmp/ix-m6-tack-orch-b-target`).
- Files changed (equals the card's ownership list, verified via `git diff --stat`):
  - `crates/tack-orch/src/scheduler/types.rs`
  - `crates/tack-orch/src/scheduler/wiring.rs`
  - `crates/tack-orch/src/usage_provenance.rs`
  - `crates/tack-orch/tests/docket_live_test.rs`
  - `crates/tack-orch/tests/docket_tick_contract_test.rs`
  - `crates/tack-orch/tests/docket_wire_contract_test.rs`
  - `crates/tack-orch/tests/model_policy_contract.rs`
  - Plus one `docs/dev-notes/tack-orch/scheduler/wiring.md` deletion (see *Dev-notes
    resolution*) — not on the card's `.rs` ownership list, but the card's acceptance
    criteria explicitly calls for resolving any dev-notes entry that references one of the
    seven files above.
- Contract fixtures consumed: none.
- Behavior implemented: none — comment-only card. Verified per file: `git diff` for every
  `.rs` file above touches only `//`, `///`, `//!` lines (and, in `types.rs`, whitespace
  that `cargo fmt` re-flowed after four enum variants lost their doc comments and became
  eligible for single-line collapse, then were expanded back to multi-line by `cargo fmt`
  itself — see *Measured numbers*). No function signature, match arm, assertion, or golden
  fixture changed.
- Tests added and exact commands/results: none added or removed. No nextest run performed,
  per the card's own instruction (comment-only change needs no behavior re-proof).
- Failure/adversarial case proved: n/a (no behavior change).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced or touched.
- Secrets/logging review: n/a — no log line or secret-handling code was touched.
  `docket_tick_contract_test.rs`'s `PLANE_TOKEN`/`assert_never_leaks_token` doc comments
  were expanded (folding in content moved out of the module preamble), not the token value,
  the assertion, or the golden-comparison logic.
- Safe merge order and likely conflicts: no overlap with the sibling `tack-orch` IX-M6
  sub-card's file set (disjoint by construction) or with any other open card touching
  `tack-orch`. Should merge cleanly in either order.
- Checklist: no unowned `.rs` files touched, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Every flagged violation in the 7 owned files is resolved | `python3 scripts/maintainability.py comment-worklist --json` filtered to the 7 files returns `[]` after the edits (was 7 entries before: 3 comment-share, 4 module-doc) |
| No production logic changed | `git diff` for all 7 files touches only comment/doc lines, blank lines, and (in `types.rs`) `cargo fmt`'s own re-layout of enum-variant braces — verified by reading every hunk before committing |
| Workspace still compiles | `cargo check --workspace` — clean, `Finished` |
| No board-archaeology language introduced | `./scripts/check-comments.sh` — `✓ no board archaeology in crates/ frontend/src frontend/e2e` |
| No test-hygiene regression | `./scripts/check-test-hygiene.sh` — `✓ tests take their temporary paths from a guard` |
| Formatting clean | `cargo fmt --all -- --check` — no output (clean) |
| Maintainability budgets hold, ratchet and absolute | see "Budget check" below |

## Measured numbers

`python3 scripts/maintainability.py measure <file>`, ground-truthed against
`comment-worklist --json` at the start of the card and re-measured after every edit and
after `cargo fmt`:

| File | prod | cmnt before → after | cm% before → after | mdoc before → after |
|---|---:|---|---|---|
| `scheduler/types.rs` | 101 → 114 (fmt re-expanded 4 variants) | 119 → 51 | 54% → 30% | 9 → 7 |
| `scheduler/wiring.rs` | 147 → 147 | 82 → 78 | 36%\* → 34% | 8 → 21 |
| `usage_provenance.rs` | 151 → 151 | 85 → 76 | 36% → 33% | 18 → 13 |
| `tests/docket_live_test.rs` (test file) | n/a | — | — | 35 → 9 |
| `tests/docket_tick_contract_test.rs` (test file) | n/a | — | — | 139 → 9 |
| `tests/docket_wire_contract_test.rs` (test file) | n/a | — | — | 68 → 9 |
| `tests/model_policy_contract.rs` (test file) | n/a | — | — | 19 → 9 |

\*`wiring.rs`'s worklist entry displayed as "35% of 229 lines" (rounded) but the underlying
share was `82/229 = 0.358`, over the `0.35` budget the `check` script actually compares
against (`>`, not rounded) — confirmed by re-running `comment-worklist --json` and reading
the raw `lines`/`budget` fields rather than trusting the rounded `head` string.

`prod` for the three `src/` files is unchanged in intent — `types.rs`'s +13 lines are
`cargo fmt` re-expanding `NotFleetMember { fleet_id: String }`-style single-line variants
back into their multi-line brace form once the `///` doc comments that used to sit above
some of them were shortened enough to change line-wrapping eligibility; every match arm,
field, and function body is byte-identical to `develop` at `35deddf`. `wiring.rs`'s module
preamble grew (8 → 21 lines) because it was found truncated mid-sentence — see *What was
removed and moved*; it is now complete and self-contained, still well under the 30-line
production-file budget.

`wiring.rs`'s and `usage_provenance.rs`'s `mdoc` counts (21, 13) don't need to be ≤10: that
ceiling applies only to test-file preambles (`docs/plans/human-maintainability.md` §3); a
production file's preamble budget is ≤30.

## What was removed and moved

Reason classes from `docs/plans/human-maintainability.md` §3: `share` (over the 35%
comment-share cap), `preamble` (over the test-file's 10-line cap), `design → dev-notes
resolved`.

- **`scheduler/types.rs` (54% → 30%, share):** every doc comment on the file's enums,
  variants, and struct fields was shortened — migration numbers, invariants, and the
  unsupported-is-typed/unknown-is-explicit rationale were kept in compressed form; dropped
  only phrasing that restated the same fact twice across a struct and its field (e.g. the
  module doc's own summary of "the scheduler never grants the lease" no longer repeats
  inside `Selection`'s doc comment — it points there instead).
- **`scheduler/wiring.rs` (36%\* → 34%, share) — module preamble also completed (8 → 21
  lines, not a violation on its own, but load-bearing):** the preamble was found already
  truncated mid-sentence ("`choose_request_for_runner` is the one entry point: ... calls"
  then a blank line, then a bare `Design notes: docs/dev-notes/tack-orch/scheduler/wiring.md`
  pointer) — an artifact of an earlier extraction pass (matching the pattern
  `IX-M6-tack-api-a`'s handoff records for `tack-api`). The dev-notes file's full content
  (the `RequestSelection` two-step reasoning, the "two gaps this module resolves" summary)
  was folded back into a complete, self-contained module doc, then the file's other
  function-level doc comments (`runner_state_from_str`, `priority_from_metadata`,
  `fleet_is_saturated`, the inline heartbeat-fallback comment inside
  `choose_request_for_runner`) were each trimmed to remove restated detail now stated once
  in the module doc, netting a share drop despite the preamble's growth.
- **`usage_provenance.rs` (36% → 33%, share):** the module doc, `ModelProvenance`'s
  variants, and `compare_model_provenance`'s doc comment were each shortened by merging
  two-sentence explanations into one and dropping restated framing ("all three variants
  carry the full observed facts" was stated once, not once per variant).
- **`tests/docket_live_test.rs` (35 → 9 lines, preamble):** the full `uv build`/`uv venv`
  recipe for building a `docket` binary from `../rack-cli` was compressed to a pointer at
  that repo (its own build instructions are that repo's concern, not this test file's); the
  safety property under test (no provider credential forwarded, isolated `DOCKET_HOME`, the
  `#[ignore]` run command) was kept — it was already restated in the `#[ignore = "..."]`
  attribute string immediately below, so nothing was lost, only de-duplicated.
- **`tests/docket_tick_contract_test.rs` (139 → 9 lines, preamble) — by far the largest
  cut, redistributed rather than deleted:**
  - "Never leaks the token" → `PLANE_TOKEN`'s doc comment, extended with the exact route
    split it proves and a pointer to `assert_never_leaks_token`.
  - "Normalisation — what stays literal, what does not" → a new doc comment on
    `NOW_PLACEHOLDER`/`CONTROL_PLANE_PLACEHOLDER`/`WALL_CLOCK_COLUMNS` stating the two
    normalized dimensions and the "never normalize a wire value" invariant;
    `normalize_generated_id`'s existing doc comment was extended with the
    `orch_events.id`/`orch_metrics.id` specifics it used to get from the module doc.
  - "Pattern copied, not invented" → folded into `TestRepoStore`'s existing doc comment
    (now states the shared-shape-not-shared-code reasoning and the deliberately-uncorrelated
    `item_id: null` design in one place).
  - "Exactly one tick, deterministically" → folded into `TICK_POLL_SECS`'s doc comment
    (the `spawn_one`/no-up-front-sleep fact).
  - "Proving the oracle is real" (the by-hand verification procedure) → folded into
    `REQUEST_WAIT_CAP`'s doc comment, next to the cap-not-panic rationale it explains.
  - "Why a secondary, per-method wire test is not enough on its own" → split across
    `three_linked_projects_issues_three_per_project_calls_each`'s doc comment (the
    re-scoped-poll-loop regression, plus the "mirrors
    `zero_linked_projects_issues_no_per_project_calls`" cross-reference) and
    `rewound_cursor_re_delivers_overlapping_events_without_resurrecting_a_purged_row`'s
    existing doc comment (already covered the retention-guard regression in full; left
    untouched). The `derive_event_id` bullet (out of this file's scope, pinned in
    `reconciler.rs`) is still named in the rewound-cursor test's doc comment.
  - Two dangling `see the module doc's "..."` cross-references (on `GoldenRequest::headers`
    and `PLANE_TOKEN`'s doc's own self-reference) were repointed to the doc comments the
    content actually moved to, so nothing points at a deleted section.
- **`tests/docket_wire_contract_test.rs` (68 → 9 lines, preamble):**
  - "What each golden file records" → a new doc comment on `MethodGolden` stating the
    `requests`/`result` shape directly.
  - "Header names only, never values" + "Body canonicalisation is free" →
    `RequestTranscript`'s doc comment (the grep-clean invariant) and its `body` field's doc
    comment (the `BTreeMap`-gives-sorted-keys-for-free fact), replacing the removed
    struct-level pointer to the module doc.
  - "# `dispatch`" → a new doc comment directly on the `dispatch_wire_contract` test.
  - "# Auth split, preserved in the golden" → the existing inline comment inside
    `health_wire_contract` (which already said "see the module doc") was expanded in place
    to state the split and name the three tests that prove it, rather than pointing
    elsewhere.
  - The historical "only 4 of the 37 tests in `docket_adapter_test.rs` asserted the
    outgoing request" count was dropped rather than re-verified or moved — per
    `CLAUDE.md`'s rule that a load-bearing number is re-measured before being quoted, and
    this card's scope is comment-only, re-running that count was out of scope; the
    surviving text states the file's purpose without depending on the number.
- **`tests/model_policy_contract.rs` (19 → 9 lines, preamble):** merged two sentences
  describing the shared fixture and its consumer, and shortened the regenerate-command
  formatting; no content dropped.

## Dev-notes resolution

`docs/dev-notes/tack-orch/scheduler/wiring.md` — its subject is `scheduler/wiring.rs`, one
of this card's seven files, and it was explicitly a "moved out of the module preamble; trim
or delete freely" parking file (Part IX's own stated purpose for `docs/dev-notes/`, per
`.claude/scope-discipline.md`'s comment budget table). Its content — the
`choose_request_for_runner`/`RequestSelection` two-step reasoning and the "two gaps this
module resolves" summary — was folded back into `wiring.rs`'s own module doc (see *What was
removed and moved*), and the file was deleted (`git rm`). No other reference to it existed
outside `IX-M6-tack-api-a.md`'s own historical handoff note (which recorded it as
"left untouched" at the time, for a different card's file scope).

`docs/dev-notes/tack-orch/adapters/legacy_bridge.md` also matched a grep for this card's
test-file names (`docket_wire_contract_test.rs`, `docket_tick_contract_test.rs`), but its
own subject is `adapters/legacy_bridge.rs` — not one of this card's seven files, and not
even on the sibling `tack-orch` sub-card's list. It names the two test files only once each,
in a bulleted list of existing regression coverage for the legacy Docket bridge decision.
Left untouched — resolving it would mean editing content whose primary subject belongs to
neither `tack-orch` IX-M6 sub-card.

No other `docs/dev-notes/**` file references any of the seven owned files.

## Budget check

`python3 scripts/maintainability.py check --changed`:

```
✓ maintainability budgets hold (7 files checked)
```

`python3 scripts/maintainability.py check` (full workspace, second opinion):

```
✓ maintainability budgets hold (292 files checked)
```

`python3 scripts/maintainability.py comment-worklist --json` filtered to the 7 owned files:
`[]` (zero remaining violations; 7 before this card's first edit).

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` is untouched by this card — every file's
measured numbers moved down or stayed flat relative to its existing baseline entry, so the
ratchet check passes without lowering any recorded ceiling.

## Full gate

```
cargo check --workspace                              # Finished, clean
./scripts/check-comments.sh                          # ✓ no board archaeology in crates/ frontend/src frontend/e2e
python3 scripts/maintainability.py check --changed   # ✓ maintainability budgets hold (7 files checked)
python3 scripts/maintainability.py check             # ✓ maintainability budgets hold (292 files checked)
cargo fmt --all -- --check                           # clean, no output
./scripts/check-test-hygiene.sh                      # ✓ tests take their temporary paths from a guard
```

nextest was not run, per this card's own instruction (comment-only change, no behavior
proof needed).

## What a stranger still cannot do

Nothing new — this card removed no capability and added none. A stranger reading
`scheduler/wiring.rs` today gets more than before: the module preamble used to cut off
mid-sentence and hand them to a dev-notes file that (per Part IX's own stated intent) was
always meant to be emptied. Every other file reads the same operational facts in less text,
with each fact stated exactly once (in the doc comment closest to the code it describes)
instead of once in a module-doc essay and again on the function itself.

## Context spent

- Read before the first edit: this card's own file list via `comment-worklist --json`
  (ground truth for exact violations, not the paraphrased kinds/lines in the dispatch
  prompt), `.claude/scope-discipline.md` in full (the Comments section), each of the 7
  files in full, `docs/dev-notes/tack-orch/scheduler/wiring.md` and
  `adapters/legacy_bridge.md` in full (to decide resolution), and one prior handoff
  (`IX-M6-tack-api-a.md`) for template and to confirm the truncated-preamble pattern this
  card also hit was already precedented.
- Files opened and not used for editing: `docs/adr/` was not searched — no design-essay
  content in this card's files needed a new ADR; every removed section was either
  restatement, a dev-notes parking note with a known resolution, or content redistributable
  in-file.
- Read-list lines that were wrong: none — the task's own worklist filter matched the
  dispatch prompt's summary exactly (7 violations: 3 comment-share in `src/`, 4 module-doc
  in `tests/`).

## Amendments

*(none yet)*
