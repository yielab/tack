# IX-M6-tack-orch-a handoff

- Base SHA / branch / final SHA: `35deddf` (`develop`) / `agent/ix-m6-tack-orch-a` /
  `5d48d5d` (worktree: `/tmp/ix-m6-tack-orch-a`,
  `CARGO_TARGET_DIR=/tmp/ix-m6-tack-orch-a-target`).
- Files changed (must equal ownership list):
  - `crates/tack-orch/src/adapters/docket.rs`
  - `crates/tack-orch/src/adapters/github_actions.rs`
  - `crates/tack-orch/src/adapters/registry.rs`
  - `crates/tack-orch/src/execution/capabilities.rs`
  - `crates/tack-orch/src/execution_retention.rs`
  - `crates/tack-orch/src/lib.rs`
  - `crates/tack-orch/src/reconciler.rs`
  - `crates/tack-orch/src/scheduler/batch.rs`
  - Plus four `docs/dev-notes/tack-orch/**.md` deletions and one new
    `crates/tack-orch/tests/fixtures/README.md` (see *What was removed*) —
    not on the card's `.rs` ownership list, but the card's acceptance
    criteria explicitly calls for resolving any dev-notes entry that
    references one of the eight files above.
- Contract fixtures consumed: none changed. `docs/contracts/runner-v1/`
  untouched; `crates/tack-orch/tests/fixtures/*.json` (docket wire captures)
  untouched — only a new `README.md` was added alongside them.
- Behavior implemented: none — comment-only card. No production logic,
  function signature, or behavior changed in any file; confirmed by
  `measure`'s `prod` column (non-comment line count) being identical
  before/after for all eight files (see *Measured numbers*).
- Tests added and exact commands/results: none added or changed — out of
  scope for a comment-only card. `registry.rs`'s `test` line count drops by
  2 (74 → 72) because a board-vocabulary comment ("this card") inside an
  existing test's body was removed per the comment rule; no assertion or
  test body changed.
- Failure/adversarial case proved: n/a (no behavior change).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced.
- Secrets/logging review: n/a — no logging or secret-handling code touched,
  only doc/line comments.
- Safe merge order and likely conflicts: touches only the eight files this
  card owns, plus `docs/dev-notes/tack-orch/**` deletions and one new
  fixtures README. A sibling agent (IX-M6-tack-orch-b, working
  `scheduler/types.rs`, `scheduler/wiring.rs`, `usage_provenance.rs`, and
  four test files) owns a disjoint file set in the same crate — no overlap.
  Low conflict risk.
- Checklist: no unowned `.rs` files touched; no live secret; no panic stub;
  no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Every `///` doc block in the eight owned files is ≤ 15 lines | `python3 scripts/maintainability.py comment-worklist --json` filtered to the eight files returns zero rows (was 12: 5 in `lib.rs`, 2 in `reconciler.rs`, 2 in `execution/capabilities.rs`, 1 each in `execution_retention.rs`, `scheduler/batch.rs`, `adapters/registry.rs`) |
| Every module/file preamble in the eight files is ≤ 30 lines | same worklist run, zero `module doc` rows for these files, both before and after — `lib.rs`, `reconciler.rs`, `execution_retention.rs` and `adapters/docket.rs` had *broken/truncated* preambles pointing at now-deleted dev-notes files; each was rewritten as a complete, self-contained preamble, still under budget (see *Measured numbers*'s `mdoc` column) |
| Comment share ≤ 35% in every owned file where the check applies (`prod` > 100 lines) | `python3 scripts/maintainability.py measure <8 files>` — max is `reconciler.rs` at 34% (see *Measured numbers*); `adapters/registry.rs` and `scheduler/batch.rs` stay exempt (`prod` ≤ 100) both before and after |
| No behavior change | `prod` column (non-comment lines) in `measure`'s output is byte-identical before/after for all eight files; `cargo check --workspace` is green; `cargo clippy -p tack-orch --all-targets -- -D warnings` is clean |
| No card/wave/phase/date/attribution reference introduced | `./scripts/check-comments.sh` passes: `✓ no board archaeology in crates/ frontend/src frontend/e2e` (this run also caught and removed one pre-existing `"this card"` reference in `adapters/registry.rs`'s test module, left over from an earlier pass) |
| `tack-orch` still depends only on `tack-core`/`tack-db`, never `tack-api` | The "Dependency direction" rule from the deleted `docs/dev-notes/tack-orch/lib.md` is now inline in `lib.rs`'s own module doc (kept, not trimmed away, per this card's explicit caution about architectural-boundary comments); `cargo check --workspace` confirms the crate graph is unchanged |

## Measured numbers

`python3 scripts/maintainability.py measure <file>` for each of the eight
files, before (`git show 35deddf:<path>`, via a throwaway `ROOT`-shifted
copy of the script) and after this card's edits. Columns: `prod` =
non-comment production lines (unchanged everywhere, confirming no logic
touched), `cmnt` = comment lines, `cm%` = comment share, `mdoc` =
module/file preamble length in lines.

| File | prod | cmnt before → after | cm% before → after | mdoc before → after |
|---|---|---|---|---|
| `reconciler.rs` | 917 | 574 → 481 | 38% → 34% | 8 → 29 |
| `adapters/docket.rs` | 415 | 232 → 183 | 35% → 30% | 8 → 25 |
| `lib.rs` | 362 | 440 → 192 | 54% → 34% | 8 → 24 |
| `execution_retention.rs` | 155 | 65 → 76 | 29% → 32% | 8 → 25 |
| `adapters/github_actions.rs` | 104 | 60 → 34 | 36% → 24% | 26 → 26 |
| `adapters/registry.rs` | 38 | 63 → 59 | 62% → 60% (exempt, prod ≤ 100) | 26 → 26 |
| `execution/capabilities.rs` | 110 | 86 → 58 | 43% → 34% | 1 → 1 |
| `scheduler/batch.rs` | 45 | 36 → 25 | 44% → 35% (exempt, prod ≤ 100) | 10 → 10 |
| **totals (these 8 files)** | 3702 | 1556 → 1108 | — | — |

`mdoc` *grew* on four files (`reconciler.rs`, `adapters/docket.rs`,
`lib.rs`, `execution_retention.rs`) because each one's module preamble was
a **broken stub** before this card: a truncated heading with no body
(`reconciler.rs`, `execution_retention.rs`), an unclosed code fence
(`adapters/docket.rs`), or a preamble that just pointed at a
`docs/dev-notes/...md` file (`lib.rs`) this card deletes. Each was rewritten
as a complete, self-contained preamble folding in the still-load-bearing
architectural rules from the deleted dev-notes file (dependency direction,
the fetch/decide/persist invariant, the auth split, the retention-clock
distinction) — every one stays well under the 30-line budget.
`execution_retention.rs`'s overall `cmnt` count *rose* slightly (65 → 76)
for the same reason: its 7-line broken preamble became a complete 25-line
one, net of trims elsewhere in the same file; it still lands at 32%, under
budget.

`python3 scripts/maintainability.py comment-worklist --json`, filtered to
these eight files: 15 violations before this card (12 doc-block + 3
comment-share: `lib.rs`, `reconciler.rs`, `adapters/docket.rs`), 0 after.

## What a stranger still cannot do

Nothing changed here — this card is comment-only. A stranger reading these
eight files gets the same capabilities and the same trait/adapter surface
as before. What changed is where two kinds of knowledge live:

- The architectural rules that used to require a trip to
  `docs/dev-notes/tack-orch/{lib,reconciler,execution_retention,adapters/docket}.md`
  are now stated directly in each file's own module doc, in budget-sized
  form.
- The docket vendor-version wire captures ("verified live against docket
  `0.2.0b1`/`v0.2.0-beta.2`") that used to live only in
  `docs/dev-notes/tack-orch/adapters/docket.md` now live in the new
  `crates/tack-orch/tests/fixtures/README.md`, next to the fixture files
  those captures actually back (`docket_wire_contract_test.rs`,
  `docket_adapter_test.rs`, `docket_tick_contract_test.rs`,
  `docket_live_test.rs`) — this is new knowledge placement, not new
  knowledge; nothing in that README was not already true before this card.

## What was removed

Reason-class key from `docs/plans/human-maintainability.md` §3: `doc block`
(over the 15-line `///` budget), `module doc` (a broken/truncated preamble
made whole, not over budget either before or after), `restatement` (an
inline `//` comment that only repeated the string literal on the very next
line), `design → dev-notes resolved` (a `docs/dev-notes/**` entry whose
subject was one of the eight files, folded back into source or into the
new fixture README), `board vocabulary` (one `"this card"` reference,
caught by `check-comments.sh`).

**12 `doc block` trims** (lines before → after, budget 15):
- `lib.rs:450` `ProvisionPodParams` doc: 32 → 9 (dropped the exhaustive
  field-by-field walkthrough, including a specific docket commit hash and
  the full blueprint-type enumeration — narrower, still-accurate summary
  kept: what `project`/`path`/`pod`/`budget` mean and why `budget` is
  unsuffixed)
- `lib.rs:789` `ControlPlane::dispatch` doc: 21 → 12
- `lib.rs:830` `ControlPlane::provision_pod` doc: 19 → 11
- `lib.rs:811` `ControlPlane::decide_approval` doc: 17 → 9
- `lib.rs:698` `Capabilities` struct doc: 16 → 7
- `reconciler.rs:802` `derive_event_id` doc: 37 → 14 (dropped the exact
  list of docket trace-record fields, already visible in the function body
  two lines below; kept the canonicalization rationale and the collision
  caveat)
- `reconciler.rs:873` `persist_events` doc: 19 → 10
- `execution/capabilities.rs:157` `EmbeddedCapabilitySnapshot` doc: 36 → 13
  (dropped the five-bullet field-by-field enumeration; kept one paragraph
  naming each field group's actual constraint and the fixture evidence for
  it — `enrollment.request.json` vs `refresh.request.json`)
- `execution/capabilities.rs:101` `model_passthrough` doc: 19 → 12
- `execution_retention.rs:165` `spawn_execution_retention_sweep` doc:
  17 → 9
- `scheduler/batch.rs:20` `schedule` doc: 26 → 13 (merged the "Ordering"
  and "Capacity is consumed within the batch" sub-sections into two
  paragraphs; kept the tie-break rule and the "re-derive `candidates` from
  fresh state" hazard, since a caller relying on stale internal bookkeeping
  is a real correctness trap)
- `adapters/registry.rs:68` `build` doc: 19 → 13

**1 `restatement`/orphaned-doc fix, not a trim** — `adapters/docket.rs`'s
`TasksResponse` struct had **no doc comment at all**; the wrapper-shape
sentence that should have documented it was instead glued onto
`EnqueueTaskResponse`'s doc block above it (a pre-existing placement bug,
not introduced by this card). Split back into two correctly-attached,
budget-sized comments — `TasksResponse` now documents its own `{"tasks":
[...]}` wrapper shape.

**1 `board vocabulary` fix** — `adapters/registry.rs`'s
`github_actions_is_not_registered` test carried `"...not an oversight to
fill in later in this card."` Rewritten to state the fact
(`adapters::registry::build` deliberately excludes `"github-actions"`)
without the card reference.

**4 `design → dev-notes resolved` entries** (all folded into source, no ADR
needed — none of the four described a decision that outranks or duplicates
an existing ADR):
- `docs/dev-notes/tack-orch/lib.md` (38 lines) — deleted. Its "Dependency
  direction" and "Unknown enum values never fail a poll" sections are now
  `lib.rs`'s own module doc; "Money is always an estimate" is folded in
  condensed.
- `docs/dev-notes/tack-orch/reconciler.md` (142 lines) — deleted. Its
  "three-phase shape" argument (fetch/decide/persist, never a write held
  across an HTTP call), the opaque-trace-cursor rule, and the "not wired at
  boot" fact about `spawn_retention_sweep` are now `reconciler.rs`'s own
  module doc. The remaining sections (jitter, panic isolation, persistence
  interface rationale) were either already duplicated in nearby function
  docs or were pure restatement of what the code already shows; dropped
  rather than folded.
- `docs/dev-notes/tack-orch/execution_retention.md` (37 lines) — deleted.
  Its three section headers ("why a sibling module", "what's different from
  orch's sweep", "no roll-up table yet") are now `execution_retention.rs`'s
  own module doc, condensed to one paragraph each.
- `docs/dev-notes/tack-orch/adapters/docket.md` (217 lines) — deleted. Its
  constructor/auth-split/write-methods overview is now `docket.rs`'s own
  module doc. Its much longer "Verified live against a real docket server"
  section — specific HTTP captures at docket `0.2.0b1` and `v0.2.0-beta.2`
  — is vendor behavior at a version, not architecture; moved to the new
  `crates/tack-orch/tests/fixtures/README.md`, next to the fixture files
  (`tests/fixtures/*.json`) and contract test files it backs, per
  `docs/plans/human-maintainability.md` §3's "vendor behaviour at a version
  goes in the fixture directory's README" rule.

**Dev-notes entries referencing one of the eight files but NOT resolved**
(out of scope — their primary subject is a file neither this card nor the
sibling `tack-orch` batch owns):
- `docs/dev-notes/tack-orch/adapters/legacy_bridge.md` — its own subject is
  `adapters/legacy_bridge.rs` (not in this card's file list, not in the
  sibling batch's either); it cites `adapters::docket` and `reconciler.rs`
  only in passing, to argue its own "maintain the Docket bridge" decision.
  Left untouched.
- `docs/dev-notes/tack-orch/scheduler/mod.md` and
  `docs/dev-notes/tack-orch/scheduler/wiring.md` — each is about
  `scheduler/mod.rs` or `scheduler/wiring.rs` (neither owned by this card;
  `wiring.rs` belongs to the sibling `tack-orch` batch); each cites
  `batch::schedule` once, in passing, to describe its own module's call
  site. Left untouched; editing them would touch a file outside this
  card's ownership.

If none of a card's eight files had a dev-notes entry at all, that would be
worth stating plainly — it is not the case here: 4 of 8 did (`lib.rs`,
`reconciler.rs`, `execution_retention.rs`, `adapters/docket.rs`), all now
resolved; the other 4 (`adapters/github_actions.rs`, `adapters/registry.rs`,
`execution/capabilities.rs`, `scheduler/batch.rs`) never had one.

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` is unchanged (`git status
--porcelain -- scripts/maintainability-baseline.json` is empty) — every
file in scope was already under its budgets in every dimension the
baseline tracks; only the comment-worklist's per-block/per-file live checks
(not baseline-relative) were failing, and those are now clean.

## Gate — final green output

```
$ CARGO_TARGET_DIR=/tmp/ix-m6-tack-orch-a-target cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 26.96s

$ ./scripts/check-comments.sh
✓ no board archaeology in crates/ frontend/src frontend/e2e

$ python3 scripts/maintainability.py check --changed
✓ maintainability budgets hold (8 files checked)
```

Second opinions, also green:

```
$ python3 scripts/maintainability.py check
✓ maintainability budgets hold (292 files checked)

$ cargo fmt --all -- --check
(no output — clean)

$ (cd crates/tack-desktop && cargo fmt --all -- --check)
(no output — clean)

$ CARGO_TARGET_DIR=/tmp/ix-m6-tack-orch-a-target cargo clippy -p tack-orch --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.30s
```

## Context spent

- Tokens read before the first edit (cold start): project root and repo
  `CLAUDE.md`, `.claude/scope-discipline.md` in full, the eight files'
  relevant line ranges (not read whole in one shot for `reconciler.rs` —
  read in ~300-line windows), `docs/dev-notes/tack-orch/{lib,reconciler,
  execution_retention,adapters/docket,adapters/legacy_bridge}.md` in full
  (needed in full to decide what was safe to drop vs. fold back vs. move to
  the fixture README), one Part IX handoff (`IX-M6-tack-api-a.md`) for
  format, `docs/adr/{0060,0065}` (checked for overlap before deciding no
  new ADR was needed for the docket vendor-capture content), and
  `docs/contracts/runner-v1/README.md` (checked whether the
  `EmbeddedCapabilitySnapshot` fixture rationale belonged there instead of
  inline — decided against touching a file outside this card's ownership
  without explicit authorization, and trimmed it in place instead).
- Context size at handoff: moderate — did not read `TODO.md`'s Part IX
  board section at all (the dispatch prompt's acceptance criteria were
  complete); did not read any file outside the eight owned files, the
  `tack-orch` dev-notes tree, the two checked ADRs, the runner-v1 contract
  README (read-only), and a handful of `tests/fixtures/*.json` filenames
  (not opened — only listed, to confirm the new README sits next to real
  fixtures).
- Files opened and not used as a source of new content: `docs/adr/0060-
  docket-control-plane-disposition.md` and `0065-docket-pipeline-dispatch-
  trigger.md` — read to confirm neither already covered the docket
  wire-capture content before deciding it belonged in a new fixture README
  rather than a new ADR; used as evidence for that decision, not wasted.
- Read-list lines that were wrong: none outright, but the dispatch prompt's
  claim that `lib.rs` has "5 separate doc-block violations" undercounted
  its actual problem — the file's overall *comment share* (54%) was the
  larger issue and needed roughly 250 lines of trimming beyond the five
  flagged blocks to bring under the 35% ceiling; this is called out here so
  the next `comment-worklist`-driven card budgets effort by measured `cm%`
  headroom, not only by the doc-block count.

## Amendments

*(none yet)*
