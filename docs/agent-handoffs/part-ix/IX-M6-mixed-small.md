# IX-M6-mixed-small handoff

- Base SHA / branch / final SHA: `8613a05` (`develop`) / `agent/ix-m6-mixed-small` /
  `5940af7` (worktree: `/tmp/ix-m6-mixed-small`,
  `CARGO_TARGET_DIR=/tmp/ix-m6-mixed-small-target`).
- Files changed (must equal ownership list):
  - `crates/tack-runner/src/bootstrap.rs`
  - `crates/tack-runner/src/engine.rs`
  - `crates/tack-runner/src/harness/mod.rs`
  - `crates/tack-runner/src/provider/mod.rs`
  - `crates/tack-runner/src/provider/vercel_ai_gateway.rs`
  - `crates/tack-cli/src/client.rs`
  - `crates/tack-cli/src/local_enrollment.rs`
  - `crates/tack-cli/src/local_runner.rs`
  - `crates/tack-db/src/repo/execution.rs`
  - `crates/tack-db/src/repo/items.rs`
  - `crates/tack-db/src/repo/orch.rs`
  - `crates/tack-core/src/dependency.rs`
  - Plus one `docs/dev-notes/tack-runner/harness/mod.md` deletion (see *Dev-notes
    resolution*) — not on the card's `.rs` ownership list, but the card's acceptance
    criteria explicitly calls for resolving any dev-notes entry that references one
    of the twelve files above.
- Contract fixtures consumed: none changed. `docs/contracts/runner-v1/` untouched.
- Behavior implemented: none — comment-only card. No production logic, function
  signature, or behavior changed in any file; confirmed by `measure`'s `prod` column
  (non-comment line count) being byte-identical before/after for all twelve files
  (see *Measured numbers*), and independently by `git diff` showing every changed
  line starts with `///`, `//!`, or `//`.
- Tests added and exact commands/results: none added or changed — out of scope for
  a comment-only card; no test file is on this card's list.
- Failure/adversarial case proved: n/a (no behavior change).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced.
- Secrets/logging review: n/a — no logging or secret-handling code touched, only
  doc/line comments (including the credential-exposure note in
  `provider/vercel_ai_gateway.rs`, which was trimmed but its content preserved).
- Safe merge order and likely conflicts: touches only the twelve files this card
  owns, spanning four crates (`tack-runner`, `tack-cli`, `tack-db`, `tack-core`),
  plus one `docs/dev-notes/tack-runner/harness/**` deletion. No other Part IX
  comment-trim card in this cycle owns any of these twelve files. Low conflict risk.
  `harness/mod.rs` and `engine.rs` are the two files IX-M5 rewrote most recently;
  only their comments were touched here, and the frozen `HarnessAdapter`/
  `HarnessProbe` trait boundary (signatures, trait methods, dispatch logic) is
  unchanged — confirmed by `cargo check --workspace` and `cargo clippy --workspace
  --all-targets -- -D warnings` passing with zero diagnostics.
- Checklist: no unowned `.rs` files touched; no live secret; no panic stub; no
  blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Every `///` doc block in the twelve owned files is ≤ 15 lines | `python3 scripts/maintainability.py comment-worklist --json` filtered to the twelve files returns zero rows (was 24: see *What was removed* for the per-file breakdown) |
| Every module/file preamble in the twelve files is ≤ 30 lines | same worklist run, zero `module doc` rows for these files, both before and after |
| Comment share ≤ 35% in every owned file where the check applies (`prod` > 100 lines) | `python3 scripts/maintainability.py measure <12 files>` — max is `local_enrollment.rs` at 62% but it stays exempt (`prod` = 31, ≤ 100); the highest non-exempt file is `local_runner.rs` at 33% (see *Measured numbers*) |
| No behavior change | `prod` column (non-comment lines) in `measure`'s output is identical before/after for all twelve files; `git diff` shows every added/removed line begins with a comment marker; `cargo check --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are both clean |
| No card/wave/phase/date/attribution reference introduced | `./scripts/check-comments.sh` passes: `✓ no board archaeology in crates/ frontend/src frontend/e2e` |
| The frozen `HarnessAdapter`/`HarnessProbe` trait boundary in `engine.rs`/`harness/mod.rs` is structurally untouched | Trait definitions, method signatures, and `AdapterRegistry`'s dispatch logic are byte-identical except for the comments directly above them; `cargo check --workspace` confirms the crate graph and trait implementations are unchanged |

## Measured numbers

`python3 scripts/maintainability.py measure <file>` for each of the twelve files,
before (`git stash` back to `8613a05`, measured, then `git stash pop`) and after
this card's edits. Columns: `prod` = non-comment production lines (unchanged
everywhere, confirming no logic touched), `cmnt` = comment lines, `cm%` = comment
share, `mdoc` = module/file preamble length in lines.

| File | prod | cmnt before → after | cm% before → after | mdoc before → after |
|---|---|---|---|---|
| `tack-db/src/repo/execution.rs` | 2861 | 547 → 498 | 16% → 14% | 3 → 3 |
| `tack-db/src/repo/orch.rs` | 1550 | 407 → 378 | 20% → 19% | 26 → 26 |
| `tack-runner/src/engine.rs` | 999 | 207 → 189 | 17% → 15% | 5 → 5 |
| `tack-db/src/repo/items.rs` | 898 | 135 → 118 | 13% → 11% | 0 → 0 |
| `tack-cli/src/local_runner.rs` | 454 | 262 → 232 | 36% → 33% | 28 → 28 |
| `tack-runner/src/provider/mod.rs` | 246 | 148 → 132 | 37% → 34% | 25 → 25 |
| `tack-runner/src/harness/mod.rs` | 211 | 137 → 112 | 39% → 34% | 8 → 12 |
| `tack-cli/src/client.rs` | 207 | 60 → 54 | 22% → 20% | 0 → 0 |
| `tack-runner/src/bootstrap.rs` | 201 | 107 → 99 | 34% → 33% | 10 → 10 |
| `tack-core/src/dependency.rs` | 142 | 37 → 29 | 20% → 17% | 0 → 0 |
| `tack-runner/src/provider/vercel_ai_gateway.rs` | 136 | 68 → 55 | 33% → 28% | 4 → 4 |
| `tack-cli/src/local_enrollment.rs` | 31 | 55 → 51 | 64% → 62% (exempt, prod ≤ 100) | 12 → 12 |
| **totals (these 12 files)** | 10106 | 2170 → 1947 | — | — |

`harness/mod.rs`'s `mdoc` *grew* (8 → 12) because its module preamble was a
**broken stub** before this card: it ended mid-sentence
(`each concrete harness adapter (\`codex.rs\`,`) and pointed at
`docs/dev-notes/tack-runner/harness/mod.md`, which this card deletes. It was
rewritten as a complete, self-contained preamble pointing to ADR 0066 instead,
still well under the 30-line budget.

`python3 scripts/maintainability.py comment-worklist --json`, filtered to these
twelve files: 24 violations before this card (21 `doc block` + 3 `comment share`:
`harness/mod.rs`, `provider/mod.rs`, `local_runner.rs`), 0 after.

## What a stranger still cannot do

Nothing changed here — this card is comment-only. A stranger reading these twelve
files gets the same capabilities and the same trait/adapter/repository surface as
before. What changed is where one piece of knowledge lives:

- `harness/mod.rs`'s design rationale for why `HarnessProbe` is a separate trait
  from `HarnessAdapter`, and the still-open "kind-key typed twice" interface gap
  (`AdapterRegistry` keys on `tack_orch::execution::HarnessKind`; `registry.rs`
  defines its own `HarnessKind` enum) used to require a trip to
  `docs/dev-notes/tack-runner/harness/mod.md`. The `HarnessProbe`-split rationale
  is now covered by ADR 0066 (already existed, already cited this exact module doc
  section as evidence for its own decision table); the kind-key gap, which ADR 0066
  explicitly declines to close, is now stated directly and concisely on
  `AdapterRegistry`'s own struct doc instead of pointing at a file this card
  deletes.

## What was removed

Reason-class key from `docs/plans/human-maintainability.md` §3: `doc block` (over
the 15-line `///` budget), `comment share` (file over 35%), `design → dev-notes
resolved` (a `docs/dev-notes/**` entry whose subject was one of the twelve files,
folded into an existing ADR and a struct doc), `history/narrative` (a comment
narrating how the code came to be, not what it does).

**21 `doc block` trims** (lines before → after, budget 15):
- `local_runner.rs:227` `ensure_runner_credential` doc: 35 → 14
- `execution.rs:3154` `delete_unresolved_execution_artifacts_by_row_ids` doc: 34 → 15
- `items.rs:733` `update_item_status_checked` doc: 32 → 14
- `execution.rs:3303` `purge_stale_execution_replays` doc: 31 → 11
- `orch.rs:1480` `rollup_and_purge_orch_events` doc: 29 → 12
- `orch.rs:2008` `active_docket_task_for_item` doc: 27 → 12
- `execution.rs:3392` `purge_stale_terminal_execution_events` doc: 26 → 14
- `harness/mod.rs:155` `HarnessProbe::declared_capabilities` doc: 25 → 13
- `dependency.rs:121` `topological_order` doc: 23 → 14
- `bootstrap.rs:247` `report_capabilities` doc: 23 → 13
- `engine.rs:561` `wait_with_lease_renewal` doc: 20 → 11
- `engine.rs:989` `fail_before_spawn` doc: 20 → 13
- `vercel_ai_gateway.rs:14` `TEST_BASE_URL_OVERRIDE_VAR` doc: 19 → 11 (dropped the
  restated "not a new privilege" reasoning's redundant clauses; kept the actual
  security argument — see *why not dropped entirely* below)
- `local_runner.rs:701` `serve` doc: 19 → 13 (also dropped a `history/narrative`
  sentence: "Replaces the old `serve_with_embedded_runner`, which only ever
  existed for `--with-runner`...")
- `harness/mod.rs:38` `ModelObservationSource` doc: 19 → 10
- `vercel_ai_gateway.rs:187` `parse_catalog` doc: 18 → 11 (dropped vendor-lore
  measurement counts — "373 models," "21 entries," "18 entries," a named
  example model id — as trivia that will rot; kept the three non-obvious parsing
  choices those measurements were illustrating)
- `local_enrollment.rs:54` `stored_session_placeholder` doc: 18 → 13
- `client.rs:220` `error_msg` doc: 17 → 14
- `engine.rs:38` `HarnessError` enum doc: 17 → 10 (dropped a `history/narrative`
  paragraph: "Two harness adapters independently hit the same gap... Both worked
  around it with a `tracing::warn!`..."; kept the actual "why `String`, not a
  typed taxonomy" design rationale)
- `client.rs:86` `patch_if_match` doc: 16 → 12
- `local_runner.rs:100` `migrate_legacy_state_dir` doc: 16 → 11

**3 `comment share` trims** (file cm% before → after, budget 35%):
- `harness/mod.rs`: 39% → 34% — beyond the two doc-block trims above, also
  trimmed the module preamble (see *dev-notes resolution*), the `AdapterRegistry`
  struct doc, `resolve_environment`'s doc, `PROCESS_GROUP_CANCEL_CEILING`'s doc,
  `HarnessRegistrationError`'s doc, `registered_kinds`'s doc, and one inline `//`
  comment in the `reconcile` impl
- `provider/mod.rs`: 37% → 34% — trimmed `CatalogEntry`'s doc,
  `confirms_served_model_from_init_line`'s doc,
  `requires_unconfirmed_model_recording`'s doc, `attach_catalog`'s doc, the
  `Provider` trait-level doc, and the `Configured` variant doc
- `local_runner.rs`: 36% → 33% — covered entirely by the three doc-block trims
  above (`ensure_runner_credential`, `serve`, `migrate_legacy_state_dir`); no
  separate module-level trim was needed

None of the trims above dropped a "why" or a "what breaks if changed" — each kept
the non-obvious rationale (why `BEGIN IMMEDIATE`, why a placeholder credential,
why a capability defaults to the conservative answer, why a field stays raw
instead of normalized) and cut restated context, exhaustive enumeration, or
narrative of how the code arrived at its current shape.

## Dev-notes resolution

`docs/dev-notes/**` has one entry whose own subject is a file this card owns:
`docs/dev-notes/tack-runner/harness/mod.md` (58 lines), named after and describing
`crates/tack-runner/src/harness/mod.rs`. **Deleted.** Its two sections resolve as
follows:

- **"Why `HarnessProbe` is not a sixth `HarnessAdapter` method"** — already
  covered. `docs/adr/0066-docket-as-a-third-harness.md` (proposed, not yet
  accepted) already cites this exact rationale as evidence in its own decision
  table (`"Capability reporting is a second, separate trait, because the first
  five all require a claimed attempt | crates/tack-runner/src/harness/mod.rs:
  187-225, and its 'Why HarnessProbe is not a sixth method' section"`). The
  module doc's own `HarnessProbe` trait comment now states the one-sentence
  version of the same fact inline (needed before any attempt exists; the five
  `HarnessAdapter` methods all require one) and points to ADR 0066 for the full
  argument, rather than to the file this card deletes.
- **"Two open interface gaps, proven by two real adapters"** — gap 1
  (`LocalRunHandle` cannot name its own harness kind) is also already covered:
  ADR 0066 decision 5 explicitly closes it as a precondition of adding docket as
  a third harness (`"`LocalRunHandle` gains a `harness_kind` field, and the
  literal construction in `tests/crash_matrix.rs` that blocks it is fixed"`).
  Gap 2 (the kind-key typed twice — `AdapterRegistry` keys on
  `tack_orch::execution::HarnessKind`; `registry.rs` defines its own separate
  `HarnessKind` enum) is explicitly **not** covered by ADR 0066 (decision 5's own
  "why" column: `"Unifying the two kind types is a refactor with no functional
  trigger here"`) and is not a decision, just a current-state fact — folded
  directly into `AdapterRegistry`'s own struct doc as a two-line note instead of
  an ADR.

No fixture directory or README exists for this module (`harness/mod.rs` has no
associated `tests/fixtures/`), so there was no vendor-behavior content to
relocate there.

**Dev-notes entries citing one of the twelve files but NOT resolved** (out of
scope — their own subject is a file this card does not own):
- `docs/dev-notes/tack-orch/adapters/legacy_bridge.md` — its own subject is
  `tack-orch/adapters/legacy_bridge.rs` (not owned by this card); it cites
  `repo/orch.rs` once, in passing, to describe where its own claimed invariant is
  enforced. Left untouched.
- `docs/dev-notes/tack-orch/scheduler/mod.md` — its own subject is
  `tack-orch/scheduler/mod.rs` (not owned by this card); it cites
  `repo/execution.rs`'s claim query once, in passing. Left untouched.
- `docs/dev-notes/tack-db/repo/economics.md` — its own subject is
  `tack-db/repo/economics.rs` (not owned by this card); it cites `repo/orch.rs`
  twice, in passing, to justify why economics queries live in a separate module.
  Left untouched.

## Re-baselined?

`no`. `scripts/maintainability-baseline.json` is unchanged (`git status
--porcelain -- scripts/maintainability-baseline.json` is empty) — every file in
scope was already under its budgets in every dimension the baseline tracks; only
the comment-worklist's per-block/per-file live checks (not baseline-relative) were
failing, and those are now clean.

## Gate — final green output

```
$ CARGO_TARGET_DIR=/tmp/ix-m6-mixed-small-target cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.13s

$ ./scripts/check-comments.sh
✓ no board archaeology in crates/ frontend/src frontend/e2e

$ python3 scripts/maintainability.py check --changed
✓ maintainability budgets hold (12 files checked)
```

Second opinions, also green:

```
$ python3 scripts/maintainability.py check
✓ maintainability budgets hold (292 files checked)

$ cargo fmt --all -- --check
(no output — clean)

$ (cd crates/tack-desktop && cargo fmt --all -- --check)
(no output — clean)

$ CARGO_TARGET_DIR=/tmp/ix-m6-mixed-small-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.18s

$ ./scripts/check-test-hygiene.sh
✓ tests take their temporary paths from a guard
```

## Amendments

*(none yet)*
