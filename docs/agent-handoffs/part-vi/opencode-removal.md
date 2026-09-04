# opencode removal handoff

Not a board card — dispatched directly by the user, implementing decision 8 of
`docs/adr/0063-harness-credential-modes.md` ("opencode is removed from the tree —
adapter, tests, fixtures and documentation. Tack supports claude-code and codex."),
plus the adjacent dead `HarnessRegistry` the user asked to be removed in the same pass.
Uses `docs/agent-handoffs/part-vi/TEMPLATE.md` as the body; *Surface-map delta*,
*Secret-path proof* and *Vocabulary check* are skipped — this card touches none of
Part VI's provider/credential surface.

- Base SHA / branch / final SHA: base `1ea54b7` (`docs: record VI-B2 and open ADR 0063
  on harness credential modes`, `develop` tip at dispatch), branch
  `agent/adr0063-remove-opencode`, final SHA is this branch's own single commit (`git log
  agent/adr0063-remove-opencode -1`).
- Files changed: 32 (`git diff --stat` against the base). Rust: `crates/tack-runner/src/{bootstrap.rs,
  engine.rs, lib.rs, registry.rs, harness/mod.rs, harness/claude_code.rs, harness/codex.rs,
  harness/fixtures/mod.rs, harness/fixtures/fake_harness.sh}` (opencode.rs deleted),
  `crates/tack-orch/src/scheduler/types.rs`, `crates/tack-cli/src/{doctor.rs, main.rs,
  mcp.rs}`. Frontend: `frontend/src/shared/runWithAgent/{shared.ts, shared.test.ts}`,
  `frontend/src/shared/execution/capabilities.test.ts`,
  `frontend/src/features/fleet/runnerFleet/RunnerHealthCard.test.tsx`. Docs: `CLAUDE.md`,
  `docs/{API-REFERENCE.md, ARCHITECTURE.md, CONFIG.md, MIGRATION-GUIDE.md}`,
  `docs/book/src/{introduction.md, roadmap.md, developer/crate-tour.md,
  user-guide/{agent-runners.md, cli.md, quick-start.md}}`,
  `docs/diagrams/two-components-{dark,light}.svg`. Tooling: `scripts/smoke.sh`.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/**` and
  `crates/tack-orch/tests/runner_contract.rs` are untouched — `git diff --stat` against
  both shows zero changes, and `cargo test -p tack-orch --test runner_contract` (18
  tests) is green. This matches the ADR's own prediction ("no fixture names opencode, so
  decision 8 changes no fixture byte"); no contract change was found or needed.
- Behavior implemented: removed the `opencode` harness adapter (`crates/tack-runner/src/harness/opencode.rs`,
  2,555 lines) and every reference to it; removed `HarnessKind::OpenCode` (keeping
  `HarnessKind::Other(String)` and the live `Codex`/`ClaudeCode` variants); removed the
  dead `HarnessRegistry` struct/impl/tests/re-export from `crates/tack-runner/src/registry.rs`
  and `lib.rs`, and fixed the one doc-comment in `harness/mod.rs` that mentioned it.
- Tests added and exact commands/results: none added (a removal, not a feature). Existing
  tests updated in place where they named `opencode` as one of two synthetic/real adapters
  under test — `crates/tack-runner/src/harness/mod.rs`'s `registry_with_two_kinds` helper
  and its callers now use `codex`/`claude-code`; `the_same_fixture_completes_through_all_three_real_adapters`
  and `registering_all_three_real_adapters_is_order_independent` are renamed to
  `..._through_both_real_adapters` / `registering_both_real_adapters_is_order_independent`
  and drop their opencode legs; `crates/tack-cli/src/doctor.rs`'s
  `render_does_not_panic_on_a_populated_report` and
  `a_present_binary_with_a_later_probe_failure_keeps_its_confirmed_version` were re-pointed
  at kinds that still exist in `KNOWN_HARNESS_KINDS` (the second test used to depend on
  `opencode` being a *known* kind to exercise the "confirmed version, later probe failure"
  path at all — silently degrading to a no-op otherwise — so it now uses a synthetic
  `"future-harness"` kind directly). Full results below.
- Failure/adversarial case proved: see "Already-enrolled runner / historical attempt"
  below — traced the code path for a runner or attempt row that still names `opencode`
  after this change, rather than asserting it by inspection alone.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see "What I did not touch, and why" below —
  historical `agent-handoffs/**`, the three ADRs, the closed-phase bulk of
  `docs/book/src/roadmap.md`, `CHANGELOG.md`, `TODO.md` and `README.md` all still name
  `opencode`, deliberately. Token/context spend for this session is `not_measured` (no
  baseline call was made available to this agent).
- Secrets/logging review: not applicable — no secret-handling, credential, or logging
  code was touched. The one file this card was told not to touch,
  `crates/tack-runner/src/provider.rs`, already documents `opencode`'s exclusion from
  `CATALOG_ELIGIBLE_HARNESSES` correctly and needed nothing.
- Safe merge order and likely conflicts: `docs/book/src/roadmap.md` and
  `docs/book/src/user-guide/agent-runners.md` are long, frequently-touched files — expect
  line-based conflicts, not semantic ones, against other Part VI work. `README.md` was
  deliberately left unedited (see below) even though it carries the same "Claude Code,
  Codex or OpenCode" copy as the two `docs/book/` pages this card did fix — whoever next
  touches `README.md` under §VI.3/§V.3's conflict rules should carry the same three-word
  edit over. No other file in this diff is on either Part's active-card ownership list as
  far as this agent could determine without reading `TODO.md` whole.
- Checklist: no unowned files touched knowingly (see merge-order note above for the one
  judgment call); no live secret; no panic stub (`RunnerError::UnsupportedHarness` is now
  unconstructed within the crate but stays — it is part of `RunnerError`'s public surface
  and removing it was not asked for); no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `opencode.rs` and every live code reference to it are gone | `grep -rIn "opencode\|OpenCode" --include="*.rs" --include="*.ts" --include="*.tsx" crates/ frontend/src/` returns nothing except the one deliberately-untouched comment in `crates/tack-runner/src/provider.rs:42` |
| `HarnessKind::OpenCode` is gone; `HarnessKind::Other(String)` and the live variants stay | `crates/tack-runner/src/registry.rs` (17 lines, was 58) |
| `HarnessRegistry` (struct, impl, tests, re-export) is gone | same file; `pub use registry::HarnessKind;` in `crates/tack-runner/src/lib.rs` (was `HarnessKind, HarnessRegistry`) |
| The doc-comment in `harness/mod.rs` that named `HarnessRegistry` alongside the open reconcile gap is fixed, not just silent | `crates/tack-runner/src/harness/mod.rs` — "see the module docs on the open 'kind-key type duplication' gap against `registry.rs`'s own `HarnessKind`" |
| `cargo build --workspace` is clean | ran; `Finished` with only the pre-existing `proc-macro-error2` future-incompat warning (unrelated, present on the unmodified base too) |
| `cargo test --workspace` is green | 1392 passed, 0 failed, ignored count unchanged, across two full consecutive runs. One test (`harness::codex::tests::a_configured_provider_request_spawns_with_its_endpoint_variable_present`) failed intermittently under `-p tack-runner --lib` during this pass; reverted to the unmodified base (`git stash`) and reran it 4× — it failed there too (a different test failed on one of those runs), confirming a pre-existing, order-dependent flake in this suite, not a regression from this change |
| `cargo clippy --all-targets -- -D warnings` is clean | ran twice (mid-pass and final); zero warnings both times |
| `cargo fmt --check` is clean | exit 0 |
| `docs/contracts/runner-v1/**` and `runner_contract.rs` are byte-identical | `git diff --stat` against both paths: no output; `cargo test -p tack-orch --test runner_contract`: 18 passed |
| Frontend type-check and tests are green | `npm run type-check`: clean; `npx vitest run`: 85 files, 756 tests passed |
| `mdbook build docs/book` succeeds | ran twice (mid-pass and final); `HTML book written to .../docs/book/book` both times |
| The two architecture SVGs no longer show a third runner card | `docs/diagrams/two-components-{dark,light}.svg` — two mini-cards (Claude Code, Codex), recentered at x=632/748 inside the unchanged 550–930 runner plane, replacing three at x=575/690/805 |
| `scripts/smoke.sh` still makes sense to run, not just still parses | `bash -n scripts/smoke.sh` (syntax); walked every step by hand — step 7/10's old "read the pairing from opencode's declared local model" mechanism has no equivalent now (neither remaining adapter declares real `model_combinations`), so `--live` without `SMOKE_LIVE_MODEL` now fails closed with an explicit message instead of silently defaulting to a billed vendor call |

## Measured numbers

- `crates/tack-runner/src/harness/opencode.rs`: 2,555 lines, removed whole
  (`git show HEAD:crates/tack-runner/src/harness/opencode.rs | wc -l` against the base).
- Diff shape: `git diff --stat` — 32 files changed, 254 insertions(+), 2968 deletions(-).
- Rust source files with a live `opencode`/`OpenCode` string match, measured by
  `grep -rIn "opencode\|OpenCode" --include="*.rs" crates/` before any edit: 12 —
  `crates/tack-cli/src/{doctor.rs, main.rs, mcp.rs}`,
  `crates/tack-orch/src/scheduler/types.rs`,
  `crates/tack-runner/src/{bootstrap.rs, engine.rs, registry.rs, provider.rs,
  harness/mod.rs, harness/claude_code.rs, harness/codex.rs, harness/opencode.rs}`. This
  matches the task's own file list exactly, plus `provider.rs` (in scope only to verify
  it needed no change — it did not, see "Checklist" above). Two more turned up in a
  deeper sweep the task's list did not name: `crates/tack-runner/src/harness/fixtures/fake_harness.sh`
  (a shared shell fixture whose top comment described "D3 (OpenCode)") and
  `crates/tack-runner/src/lib.rs` (no literal string match, but it re-exported the
  now-removed `HarnessRegistry`, in scope for that same edit).
- Frontend files with a live reference: 4, exactly the task's list
  (`runWithAgent/{shared.ts, shared.test.ts}`, `execution/capabilities.test.ts`,
  `runnerFleet/RunnerHealthCard.test.tsx`).
- Docs under `docs/` mentioning `opencode`/`OpenCode` before this change: 41 files by
  `grep -rIl`, not 39 — the difference is the two SVG diagrams (`docs/diagrams/two-components-{dark,light}.svg`),
  which the task's "39" count evidently did not include as "documentation" but which
  needed the same fix (a text label in a diagram, not prose). Both were fixed.
- Of those 41, 12 were edited: `docs/{API-REFERENCE.md, ARCHITECTURE.md, CONFIG.md,
  MIGRATION-GUIDE.md}`, the 2 SVGs above, and 6 files under `docs/book/src/`
  (`introduction.md`, `roadmap.md`, `developer/crate-tour.md`, `user-guide/{agent-runners.md,
  cli.md, quick-start.md}`). 11 of those 12 are now fully clean of the string; the
  twelfth, `roadmap.md`, was only partially cleaned — its two genuinely live sections
  ("Vercel AI Gateway, and why it is the one provider this phase adds" and the reused
  "two-component story" blockquote) were fixed, but its closed-phase historical log
  (roughly its own Phases 39-56 section, which the file itself calls "implementation and
  decision history") was deliberately left naming opencode. The remaining 29 of the 41
  received no edit at all: the 26 `agent-handoffs/**` files and all 3 ADRs (`0050`,
  `0061`, and `0063` itself, which the task explicitly forbade editing) — see "What I did
  not touch, and why" below for the reasoning. Plus `CLAUDE.md` (outside `docs/`,
  not in either count, but the single most load-bearing live doc in the tree — every
  future agent session loads it) was also fixed.
- `cargo test --workspace`: 1392 passed (summed across every test binary's own "test
  result: ok. N passed" line), 0 failed, ignored counts unchanged from the base.
- `npx vitest run`: 85 test files, 756 tests, all passed.

## What a stranger still cannot do

A stranger who reads `docs/agent-handoffs/part-iii/III-D3.md` — opencode's own historical
build record, deliberately left untouched (see below) — would still believe opencode is a
live, shipped harness, because that handoff's entire content is a factual account of what
was built and measured against the real `opencode` 1.18.0 binary. Nothing in this card
corrects that impression there, on purpose: it is a historical record, not living
documentation, and CLAUDE.md's own rule ("corrections are appended as amendments, never
rewritten") applies. A stranger who instead reads any *current* doc — `CLAUDE.md`,
`docs/ARCHITECTURE.md`, `docs/book/src/user-guide/agent-runners.md` — will correctly learn
Tack ships adapters for `codex` and `claude-code` only, with the vocabulary left open for
more. A stranger who runs `README.md`'s or `TODO.md`'s copy of the "two-component story"
blockquote will still read "Claude Code, Codex or OpenCode" — those two files were
deliberately not touched (see below) and now disagree with the `docs/book/` copies of the
identical, supposedly-verbatim quote.

## Already-enrolled runner / historical attempt (the investigation this card asked for)

Traced from the actual code, not asserted:

- **The wire-level `HarnessKind` was never a closed enum.** `tack_orch::execution::HarnessKind`
  (`crates/tack-orch/src/execution/types.rs`) is built with the `opaque_id!` macro — a
  bare `String` newtype with no enum, no `CHECK` constraint, no validation against a
  fixed vocabulary anywhere in `tack-db` or `tack-core` (confirmed: neither crate
  mentions `opencode`, and neither ever mentioned any harness name — this type was
  always open). Only `tack-runner::registry::HarnessKind` (the one this card edited) is a
  closed-ish Rust enum, and it is entirely internal to the runner binary; it never
  reaches the server, the database, or the wire.
- **A runner still reporting `opencode` in its stored `capability_snapshot`** (captured
  at a past enrollment or refresh, before its binary was rebuilt from this change) is
  just a JSON blob sitting in the `agent_runners.capability_snapshot` column
  (`crates/tack-api/src/handlers/runner_protocol.rs`, `UPDATE agent_runners SET ...
  capability_snapshot=?`). Reading it back — `tack runner doctor`-style rendering, the
  fleet UI's `RunnerHealthCard`, `listReportedHarnessKinds` — deserializes and displays
  it exactly like any other string; nothing panics or 500s. It is simply stale: the
  runner claims a harness it can no longer actually run, until its *next* refresh cycle.
  Once the runner is rebuilt with this change and reports capabilities again,
  `build_adapter_registry` (`crates/tack-runner/src/bootstrap.rs`) no longer creates,
  registers, or probes an `opencode` adapter at all, so the entry silently disappears
  from the snapshot on that next refresh — no migration, no operator action, no special
  handling needed or added.
- **An `execution_attempt` row whose `requested_harness_kind` or
  `actual_execution.harness_kind` already names `"opencode"`** is permanent historical
  fact — a `TEXT` column with no foreign key or check constraint against a harness list
  (confirmed by the same `tack-db`/`tack-core` search above). It reads back and renders
  in history/timeline views forever, unaffected, exactly as it always would have for any
  harness string.
- **A *new* execution request naming `harness_kind: "opencode"`** (an old client, a
  cached UI value, a saved script or MCP call) against a runner that no longer declares
  it: `evaluate_candidate` in `crates/tack-orch/src/scheduler/select.rs` (the
  `candidate.harnesses.iter().find(...).ok_or_else(|| IneligibleReason::HarnessNotDeclared
  { ... })` call) returns a typed, non-fatal `HarnessNotDeclared` rejection — the exact
  same path every other harness kind a runner doesn't declare already takes. There is
  nothing opencode-specific about this failure mode; it was never a special case.

**Conclusion: this degrades gracefully, not badly.** No panic, no 500, no silent data
loss, in any of the four cases above. The only soft edge is display staleness — an
operator could see a stale "opencode: present" in a cached capability view for up to one
refresh cycle after upgrading a runner — and that self-heals on its own; nothing in this
card fixes it because nothing is broken enough to need fixing.

## What I did not touch, and why

The task said "39 files under docs/ mention opencode. Sweep them," with the explicit
caveat that documentation must be rewritten honestly, not just deleted. Measured: 41
files under `docs/` (39 `.md` plus the 2 SVGs) mentioned it. I edited 12 of the 41 (11
fully cleaned, `roadmap.md` partially — see "Measured numbers" above) plus `CLAUDE.md`
outside `docs/`, and deliberately left 29 untouched:

- **All 26 files under `docs/agent-handoffs/**`** (`part-iii/` × 15 including its own
  `README.md`, `part-iv/` × 3, `part-v/` × 2, `part-vi/` × 5 including its own
  `README.md`, `part-vii/VII-B2.md` × 1). These are historical, dated build and audit
  records — several (`III-D3.md` most directly) *are* the historical account of
  building the opencode adapter itself. CLAUDE.md's own rule — "corrections are appended
  as amendments, never rewritten" — applies directly, and rewriting them would falsify a
  measured historical record rather than correct a stale current one. One specific
  finding worth flagging: `docs/agent-handoffs/part-vi/README.md` (an active Part VI
  dispatch-plan document, not a closed card's own handoff) cites
  `sed -n '1125,1170p' crates/tack-runner/src/harness/opencode.rs` and similar line-range
  reads at least twice (around its own lines 262 and 311) as a way to size the remaining
  Part VI work — those commands will now fail outright since the file is gone. I did not
  edit this document myself (it is Part VI's own dispatch plan, not mine to rewrite under
  the same "never rewritten" logic), but whoever next works from it should know those two
  citations are dead.
- **`docs/adr/0050-runner-control-plane.md` and `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`**
  (both accepted, dated ADRs, treated the same as `0063` itself, which the task explicitly
  said not to touch) — their `opencode` mentions are dated context-at-decision-time
  statements ("Codex, Claude Code and OpenCode are coding harnesses, not remote
  project-management schedulers"), not claims about today's shipped adapter set.
- **The closed-phase bulk of `docs/book/src/roadmap.md`** (everything from its own
  "Phase 39-56... implementation and decision history" self-description onward, roughly
  lines 2400-2950) — the document says of itself "Phases 39-42 and every earlier section
  remain in this document as implementation and decision history." I fixed the two
  genuinely *live* sections further down the same file (the Vercel AI Gateway rationale
  and the reused "two-component story" blockquote, both part of the still-active Phase
  60 write-up) and left the historical Phase 53 log alone.
- **`CHANGELOG.md`** — Keep a Changelog format; every `opencode` mention is inside the
  already-tagged, dated `[0.1.0-beta.7] - 2026-08-31` entry, not the `[Unreleased]`
  section at the top (which has none). Rewriting a shipped release's own changelog entry
  would misstate what that release actually contained.
- **`TODO.md`** — explicitly out of scope per this card's own instructions ("Do not touch
  TODO.md... the integrator owns it"). It carries the canonical, "applied verbatim by
  VI-A3" master copy of the two-component-story blockquote at its own §VI.0 (around line
  587), which `README.md`, `docs/book/src/introduction.md` and `docs/book/src/roadmap.md`
  all copy. I fixed the two `docs/book/` copies (they were both in scope and both stale)
  but could not touch the master.
- **`README.md`** — outside the task's stated `docs/` scope, and flagged by the
  workspace-root `CLAUDE.md` as a file Parts V and VI share under explicit conflict rules
  (`TODO.md` §VI.3/§V.3) precisely because concurrent active branches touch it. It carries
  the same stale "Claude Code, Codex or OpenCode" copy as the two book pages I did fix.
  Left alone to avoid an out-of-process conflict with concurrent Part V/VI work; flagged
  above under "safe merge order."
- **`scripts/smoke.sh`'s three `§III.6` "attempts through Codex, Claude Code and
  OpenCode" unmet-reason strings** (lines ~317/320/323) — verified these are exact,
  character-for-character quotes of `TODO.md`'s own frozen §III.6 acceptance-criterion
  text (`grep -n "attempts through Codex" TODO.md` → line 12602, in the archived Part III
  section). Left as literal quotes of a document I am not authorized to touch, rather
  than silently drifting from what it actually says.

## Context spent

- Tokens read before the first edit (cold start), against the block's estimate: not
  measured — no `.claude/token-baseline.md` figure was consulted for this ad hoc card.
- Context size at handoff: not measured.
- Files opened and not used: none of note — every file opened contributed either an edit
  or a scoping decision recorded above.
- Read-list lines that were wrong: not applicable — this card had no dispatch read-list;
  the file set was discovered by grep sweeps rather than assigned in advance.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
