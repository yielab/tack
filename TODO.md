# TODO — Tack cycle boards

> **Read this header, then jump. Don't read this file whole** — it is ~4.8k lines covering
> six Parts (IX down to IV); Part VI alone is ~2.5k of that. Parts I–III's boards, ~10.2k
> lines, are archived at `docs/closed-cycles/boards/part-1.md` … `part-3.md` and no longer
> live here. Reading a whole Part costs more context than any card needs. The extraction
> recipe is in `.claude/context-budget.md`; the short version is
> `grep -n "^# \|^## " TODO.md`, then `sed -n '<start>,<end>p'` for the one section you
> need.

## Which board is live

| Part | Cycle | Phases | Status | Where |
|---|---|---|---|---|
| **IX** | **Human maintainability** | 63 | **OPEN 2026-09-11 — priority over every other card, including the release tag.** Ten cards IX-M0…IX-M9 in seven waves (27–33); Waves 27's three cards are mechanical and run first. IX-M9 (was IX-M8) split off IX-M8 (test-volume gap) on 2026-09-12, after audit found M4's own −25k-line delta was never measured to have landed. Specification: `docs/plans/human-maintainability.md`. | [§IX](#part-ix--human-maintainability-phase-63) |
| **VIII** | **Docket bridge hardening** | 62 | **Done 2026-09-08 — Waves 24–26 integrated; one small card open (VIII-C3, blocking nothing).** ADR 0060's four gaps closed and ADR 0065 accepted; detail in this Part's own section below. | [§VIII](#part-viii--docket-bridge-hardening-phase-62), top of this file |
| **VII** | **Desktop app & background service** | 61 | **Done 2026-09-07** (reopened once for VII-B5, then closed again). ADR 0062 accepted; Linux/Windows bundles build, macOS not re-run; a stranger completed a full attempt via the tray/AppImage. One product defect spun out as VI-C36. | [§VII](#part-vii--desktop-app--background-service-phase-61), top of this file |
| **VI** | **Agent Onboarding & Provider UX** | 60 | **Done 2026-09-07** — every card has an accepted integration and a handoff; nothing is open. Not card work, but still true: no harness offers in-app login, so the completed-attempt claim rests on a fake-harness shim, and the live tray/AppImage walks aren't re-run. | [§VI](#part-vi--agent-onboarding--provider-ux-phase-60), top of this file |
| **V** | **Adoption & First Public Release** | 59 | **Done 2026-09-06** — every card integrated. What remains is publishing, a human action outside this repo: see `docs/LAUNCH-CHECKLIST.md`, whose first item is tagging a release (nothing downloadable since `v0.1.0-beta.7`). | [§V](#part-v--adoption--first-public-release-phase-59) |
| **IV** | **Standalone Single-Binary Operation** | 58 | Done — Wave 10 integrated at `83fefab`. | [§IV](#part-iv--standalone-single-binary-operation-phase-58) |
| III | Harness-Agnostic Runner Fleet | 50–57 | Feature-complete, **tag refused**. | `docs/closed-cycles/boards/part-3.md` |
| II | Agnostic Control Plane | 39–49 | Superseded after Wave B by Part III. | `docs/closed-cycles/boards/part-2.md` |
| I | Agent-Factory Control Center | 33–38 | Complete 2026-08-05. | `docs/closed-cycles/boards/part-1.md` |

**Part IX is the live board and takes priority; Part VIII keeps one small open card (VIII-C3).** Parts I–VII are closed. Part V is distribution and launch — everything between
"it works here" and "a stranger can use it". Part VI is the agent onboarding and provider
flow — everything between "a stranger installed it" and "a stranger ran an item with the
model they chose, without opening this file". Part IV is done. They share `README.md` and
`docs/screenshots/**`; the conflict rule is in
[§VI.3](#vi3-dependency-graph-cross-part-conflicts-and-merge-policy), which defers to
[§V.3](#v3-dependency-graph-cross-part-conflicts-and-merge-policy) for the files Part V
still owns. Read both before branching a card in either Part.

## Where closed cycles live

Parts I, II and III are archived verbatim at `docs/closed-cycles/boards/part-1.md`,
`part-2.md` and `part-3.md`. Their internal numbering (`§0`…`§6` for Part I, `§II.*`,
`§III.*`) is unchanged, so an existing citation to one still resolves once the file name is
updated — `docs/book/src/roadmap.md`'s one such citation has been repointed, and
`grep -rn "TODO\.md" crates/ --include='*.rs'` still returns 0. Parts IV through VIII stay
in this file; only their handoffs moved, to `docs/closed-cycles/handoffs/part-<n>/`. See
the "Which board is live" table above for the current state of every Part.

## Conventions that hold across every Part

- One card, one worktree, one branch: `agent/<card-id-lowercase>-<slug>`, branched from the
  integration line the active board names — today `develop` for every Part.
- Stay inside your card's `Owns`. A change you need in someone else's file is a request
  written into your handoff, not an edit.
- Each card writes exactly one handoff in `docs/agent-handoffs/<part>/<ID>.md`. Corrections
  are appended as amendments; the original claim stays, because the history of what was
  believed and later falsified is the point.
- No card edits a status board. The wave integrator does that, after independent
  verification by someone who did not author the code.
- Never `git commit` without the user asking; never add AI attribution to a commit message.

---

# Part IX — Human maintainability (Phase 63)

**Status: OPEN 2026-09-11 — priority over every other open card, including the release tag.**
Created from `docs/plans/human-maintainability.md`, which is the specification; this board
carries only the cards, their order and their acceptance. Nothing here changes what the code
does, what the wire contract says or what CI verifies. It removes copies, prose and
scaffolding, and puts a size budget on what remains so that nothing an agent adds later can
undo it.

Dispatch plan: **`docs/agent-handoffs/part-ix/README.md`** — read its header and your card's
block only.

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 27 — The tool and the mechanical moves | IX-M0 → IX-M1 → IX-M2 | 63 | done — `7ffd192`, `45edaba`, `dfa62aa`; 284 files baselined, budgets hold |
| 28 — One place for a helper | IX-M3-db, then IX-M3-orch ∥ IX-M3-api ∥ IX-M3-runner ∥ IX-M3-cli | 63 | done — all five sub-cards integrated, 1502/1502 tests hold, budgets hold |
| 29 — Prune per binary | IX-M4-<crate>-<binary>, 28 sub-cards, largest first | 63 | done — all sub-cards integrated (renames, table-driving, sleep→poll), 1439/1439 tests hold (8 skipped), budgets hold |
| 30 — Harness core | IX-M5 (= T0 of `docs/plans/harness-maintainability-audit.md`) | 63 | done — `LocalProcessHarness<G>`/`HarnessGrammar` extracted, both adapters migrated, cross-adapter test dedup landed, 1413/1413 tests hold; 3 of 4 audit exit criteria met, per-adapter 400-line budget still open (codex 671, claude_code 816) — a follow-on card, not a further pass of this one |
| 31 — Comments and docs | IX-M6 batches ∥ IX-M7 | 63 | done — 5 IX-M6 batches (44 files) integrated, 0 comment-worklist violations remain; IX-M7 landed in two halves (generated docs: book includes, `scripts/gen-api-reference.py`, `cargo doc` CI gate; archival: Parts I-III + closed-Part handoffs moved to `docs/closed-cycles/`, live table trimmed to ≤300 chars/row); 1413/1413 tests hold, budgets hold, `pre-push` green |
| 32 — Close the test-volume gap | IX-M8-dedup, then IX-M8-<crate> ×5 ∥ IX-M6-dev-notes ∥ IX-M7-roadmap | 63 | done — IX-M6-dev-notes integrated `4655b6b` (12 notes resolved, directory deleted), IX-M8-dedup integrated `c90bbcc` (49 pairs → 0, all by rename, 0 tests removed), IX-M7-roadmap integrated `c9339ee` (roadmap 3 683 → 948 lines). **Finding:** `cargo llvm-cov` skips `src/**/tests.rs` but counted the inline modules IX-M1 moved, so line coverage on `develop` is now honest and under three floors — core 83.08 % (85), db 69.89 % (70), api 69.25 % (70); missed-line counts are identical to `main`'s, nothing lost coverage; CI's job never runs on `develop`. Crate cards hold "after ≥ before"; floors re-set in IX-M9 or by decision. IX-M8-runner integrated `8be6c78` (test lines 13 358 → 12 885, 0 tests removed, coverage 91.55 % → 91.55 %, crate's only fixed wait gone; **unmet, recorded:** `src/engine/tests.rs` 2 587 lines and `src/harness/claude_code/tests.rs` 1 344 lines with one 43-line body — brought down only by splitting files, which the card forbids). IX-M8-orch integrated `7225c5e` after two rejections (a five-file split of `reconciler/tests.rs`, reverted; then a helper that dropped its `MockServer` guard and made `docket_adapter_test` flaky under `cargo test`/llvm-cov — fixed, 10/10 green): crate test lines 11 837 → 11 848 (net +11, the guard bindings), tests 277 → 276, 7 fixed waits → 0, coverage 91.17 % → 91.10 % (194 missed lines both; denominator shrank with an inline module); **unmet, recorded:** `src/reconciler/tests.rs` 1 921 lines. IX-M8-db integrated (tests 189 → 192, test lines 11 272 → 10 859, 0 removed, coverage 69.89 % → 69.89 %); **unmet, recorded:** `tests/orch_migrations.rs` 1 190 lines, `tests/execution_repo.rs` 4 176 lines with 38 bodies over 40 (25 over the 60 cap, largest 174). IX-M8-api integrated after one integrator fix (a new helper dropped its `MockServer` guard and made `max_in_flight_bounds_actual_dispatch_concurrency` fail under `cargo test`): tests 447 → 440, test lines 28 645 → 28 371, line coverage 69.25 % → 69.27 %; **unmet, recorded:** 13 files with bodies over 40 (none over 60), 6 of them over 1 000 lines (largest `runner_protocol/lifecycle.rs` 1 821). IX-M8-cli integrated (tack-desktop's inline supervisor tests moved to `supervisor/tests.rs`, its 2 sleeps gone; a cwd-mutating helper in `local_runner/tests.rs` made safe under `cargo test` with a lock; coverage 44.73 % → 44.73 %); **unmet, recorded:** `tack-cli` test lines 4 228 → 4 271 (+43) — its long bodies were single end-to-end tests, and named helpers cost more than they saved. integrator fix after merge: three `local_runner` secret tests wrote to the real keychain over D-Bus and flaked under a full workspace run (`d8fd3ce`). IX-M8 closed |
| 33 — Ratchet down | IX-M9 (was IX-M8) | 63 | done — IX-M9 integrated: `check` is a hard gate at 40-line bodies, 1 000-line test files and 30 % comment share; 89 named exclusions in the script, each with its reason, still ratcheted against the baseline (89 = failures with the list emptied, verified); `check --changed` fails a change adding more than 15 tests or 600 test lines; workspace ratio ceiling 1.261 (gap to 0.8 named in `docs/TESTING.md`). **Findings for a decision:** 50 of the 89 were never recorded by an IX-M8 handoff — 33 comment-share files surfaced by the 35 → 30 % tightening, 13 files with short sleeps, 3 `tack-core` files never carded, 2 `tack-desktop` files. CI coverage floors (core 85, db 70, api 70) still fail on honest numbers; not decided. **Follow-up 2026-09-14 (off-board, IX-X1/X2/X3):** 33 comment-share files to ≤ 30 % (`0b2e539`), 21 fixed sleeps to bounded polls + 8 names + log-file test race (`ffb60a1`), db execution-repository tests on one fixture split by operation (`0723297`), api runner-protocol/chaos bodies 270 → ≤ 97 (`0bb4738`); exclusions 89 → 44, pre-push green. Runner card abandoned at the Sonnet session limit; `engine/tests.rs`, `claude_code/tests.rs`, `reconciler/tests.rs`, `orch_migrations.rs` and the rest of api stay as accepted exclusions per §IX.5 |

**Audit note, 2026-09-12 — Wave 32 was not ready as originally scoped.** Before dispatching
Wave 32 every number in the plan was re-measured instead of trusted (commands in §IX.0's
second table). What the audit found, in order of weight:

1. **The volume never moved.** Workspace test:production ratio is **1.284** (44 455 code +
   10 993 comment lines of production, 71 211 test lines) against IX-M0's baseline of
   **1.316**; tests 1 498 → 1 413. The plan's §5 priced IX-M4 alone at **−25 000 lines,
   ≈ −400 tests**. The 13 IX-M4 handoffs that recorded totals sum to **41 704 → 40 511 test
   lines (−2.9 %)**, three binaries grew, and the largest sub-card (`tack-db-repository`)
   went 8 805 → 8 620 with `execution_repo.rs` at 4 323 → 4 308 lines, 27 bodies still over
   the 60-line cap and a 174-line maximum. Every sub-card's scoped gate passed because the
   gate is a ratchet ("not worse"), and the volume number was never an acceptance criterion.
2. **IX-M4's own acceptance is not met, though Wave 29 is marked done.** Today: 135 test
   bodies over the 60-line hard cap (110 of them in the binaries IX-M4 owned), 207 test
   names over 60 characters, 17 test files over 1 000 lines. 13 of the 17 handoffs say so
   in their own words ("not worsened", "remains over the hard cap") and were integrated on
   that basis. The plan said 28 sub-cards; 29 top-level test binaries exist and 17 handoffs
   were written, so ten binaries had no card at all — `wave2_gate`, `openapi_contract` and
   the `tack-orch` contract/live binaries (deliberately exempt), but also
   `docket_tick_contract_test` (1 156 lines), `runner_contract` (967), `cli_test` (317),
   `perf_test`, `runner/cli`. The coverage-floor guard IX-M4 relied on never ran: that CI
   job is gated to pull requests and pushes to `main`, and every wave here was integrated
   by a local merge to `develop`.
3. **A third of the test corpus was in no card's scope.** `src/<module>/tests.rs` unit
   modules — 74 files, **23 070 lines, 747 tests** — belong to no IX-M4 binary. `engine/
   tests.rs` alone is 3 032 lines with a 102-line test; `reconciler/tests.rs` 1 927.
4. **`docs/adr/0064-fixed-waits.txt` was wrong twice.** The committed file listed 25 stale
   entries. An earlier pass of this same audit regenerated it to **2** and wrote that here —
   also wrong: `scripts/list-fixed-waits.py` only recognised `tests/` and inline `mod tests`
   tails, so after IX-M1 moved unit modules to `<module>/tests.rs` their waits vanished from
   the inventory. Script fixed the same day; the true count is **11 waits, 11.8 s**, nine of
   them in unit modules (a 5 s `thread::sleep` in `secrets/tests.rs`, 2.5 s in
   `reconciler/tests.rs`). `measure --totals` separately counts 38 `sleep(` calls, most of
   them bounded polls — the two numbers measure different things and both are cited below.
5. **`docs/dev-notes/` was never emptied.** 12 files, 762 lines remain. IX-M6's batches
   were cut from `comment-worklist`, which lists only files *currently* over budget; a
   module whose preamble IX-M2 had already parked is under budget, so nine of the twelve
   notes never appeared in any batch. No card owns them now.
6. **Smaller gaps.** `roadmap.md` (3 683 lines) was to keep only its forward-looking
   sections — never carded, IX-M7's text omitted it. `crates/tack-desktop/src/supervisor.rs`
   still carries a 482-line inline test module (budget 150): IX-M1 ran on the workspace and
   `tack-desktop` is outside it. Plan §2.3's per-card budget (≤ 15 new tests, ≤ 600 new
   test lines) is prose only — `check` does not implement it. A hard-mode `check` today
   would fail **96 of 292 files** at the current budgets and **129** at the 40-line target.
7. **What did land.** Comment share 23 % → **19.8 %**, 0 blocks and 0 preambles over
   budget; near-identical test pairs 72 → 49; `engine.rs` 4 295 → 1 242 lines; `tests/common`
   in five crates with one stray `project()` left; documentation in the read path (`.md`,
   excluding `docs/closed-cycles/` and generated pages) **29 878 lines** against the ≈ 30 000
   target; IX-M5 three of four exit criteria.

**Fix:** the old IX-M8 (hard-lock at ratio ≤ 0.8) would have failed on 96 files the moment it
ran. It is split into **IX-M8** — a real volume pass over *everything* the first pass missed:
unit modules, the ten uncarded binaries, the 135/207/17 acceptance leftovers, the 11 waits,
the 49 pairs — and **IX-M9**, the ratchet-lock, which now also has to implement the per-card
budget and lock the ratio at the number IX-M8 measures. Two small orphaned items get their own
sub-cards in the same wave: **IX-M6-dev-notes** and **IX-M7-roadmap**. Wave 29's sub-cards
stay merged; their handoffs already say what they left.

## §IX.0 Cold-start context capsule

**What this Part is for, in one sentence.** A person can read any production file, any test
file and any doc page in one sitting, and a script — not a prompt — keeps it that way.

**The specification is the plan**, `docs/plans/human-maintainability.md` (~240 lines, ~4k
tokens): §2 the test layout and the nine rules, §3 the comment budgets, §4 the documentation
rules, §5 the cards, §6 the decisions still open. Read the section your card cites, not the
plan whole. The harness adapters have their own audit,
`docs/plans/harness-maintainability-audit.md`; IX-M5 is its card T0.

### Re-measured 2026-09-12, after Waves 27–31 (`git log --oneline f95fbc2..HEAD | wc -l` = 159)

| Fact | 2026-09-11 | 2026-09-12 | Target | Command |
|---|---|---|---|---|
| Production code / comment lines | 44 079 / 13 374 (23 %) | 44 455 / 10 993 (**19.8 %**) | ≤ 20 % | `measure --totals` |
| Test lines, ratio | 74 014, 1.29 | **71 211, 1.284** | ≤ 0.8 (≈ 35 500 at today's code) | `measure --totals` |
| Tests | 1 498 | 1 413 (+ 8 ignored) | ≈ 1 000 | `cargo nextest run --workspace` |
| Unit-test lines in `src/**/tests.rs` (no card owned them) | — | 23 070 in 74 files, 747 tests | in IX-M8 | `measure --json`, filter `/src/` |
| Test bodies over 60 lines / over 40 | — | **135** / 353 | 0 / 0 outside exclusions | parser in the Wave 32 audit, or `measure --json` `max` |
| Test names over 60 characters | — | **207** | 0 | same |
| Test files over 1 000 lines | — | **17** | 0 outside exclusions | `measure --json` |
| Files a hard-mode `check` would fail | — | **96** (129 at the 40-line cap) | the exclusion list's length | `measure --json` against `BUDGETS` |
| Fixed waits ≥ 200 ms (`sleep(` calls of any length) | 25 recorded, stale (74) | **11**, 11.8 s (38) | 0 | `scripts/list-fixed-waits.py` (`measure --totals`) |
| Near-identical test names across files | 72 | **49** | 0 | `duplicate-tests` |
| Comment blocks / preambles over budget | 149 / 25 | **0 / 0** | 0 | `comment-worklist`; `extract-module-docs` |
| `docs/dev-notes/` | 25 created by IX-M2 | **12 files, 762 lines** | empty | `find docs/dev-notes -type f \| xargs wc -l` |
| Docs `.md` in the read path (no `closed-cycles/`, no generated) | ≈ 91 000 | **29 878** | ≈ 30 000 | `git ls-files 'docs/**/*.md' \| grep -v closed-cycles \| grep -v api-reference \| xargs cat \| wc -l` |
| `docs/closed-cycles/` | — | 66 302 lines, 214 files | archived, not read | same, inverted |
| Inline test module over 150 lines | 40 | **1** (`tack-desktop/src/supervisor.rs`, 482) | 0 | `extract-tests` (dry run) |

### Evidence base, measured 2026-09-11 (`scripts/maintainability.py`, dry runs only)

| Fact | Value | Command |
|---|---|---|
| Production Rust, no inline tests | 57 453 lines, 23 % comments | `measure --totals` |
| Test lines | 74 014 (ratio 1.29), 1 498 tests, 18.8 s | `measure --totals`; `cargo nextest run --workspace` |
| Test lines inside `src/*.rs` over budget | 20 882 in 40 modules | `extract-tests` (dry run) |
| Comment blocks over budget | 149 in 104 files; 25 preambles over 30 lines | `comment-worklist`; `extract-module-docs` (dry run) |
| Near-identical test names across files | 72 pairs | `duplicate-tests` |
| Fixed waits in test code | 74 `sleep(` | `measure --totals`; `docs/adr/0064-fixed-waits.txt` |
| Test binaries | 28 (api 8, orch 7, cli 6, runner 5, db 2) | `cargo nextest list --workspace` |
| Helper copies in test files | 18 `project()`, 12 `request()`, 6 `runner()`, 4 `claim()` | plan Appendix A |
| Non-generated docs | 91 089 lines; 59 986 of them handoffs in 230 files | plan Appendix A |
| CI coverage floors (line %) | core 85 · db 70 · api 70 · orch 70 · runner 85 | `.github/workflows/ci.yml` |

### Vocabulary

*A budget* is a number in `BUDGETS` in `scripts/maintainability.py`. *The baseline* is
`scripts/maintainability-baseline.json`. *The ratchet* is `check`: a file may exceed a budget
only if it already did and is not worse; new files meet every budget. *A scratch test* is
`crates/*/tests/scratch_*.rs`: gitignored, run locally, never tracked. *A layer* is one of
repository, router, contract, gate. *A preamble* is a file's leading `//!` block.

---

## §IX.1 Rules for simultaneous agents

**All of §III.2, §V.1, §VI.1, §VII.1 and §VIII.1 apply unchanged.** Seven rules are specific
to this Part:

1. **Behaviour is frozen.** No card changes what a function returns, what a route answers or
   what a fixture pins. A test that fails after a card's change is a defect in the card, never
   a test to edit. Two named exceptions: IX-M4 replaces a fixed wait with a bounded poll or
   paused time, and IX-M5 migrates the adapters with their existing tests as the proof.
2. **Mechanical cards are one command.** IX-M1 and IX-M2 run the script, format, run the full
   suite once, commit. If the script refuses a file, the refusal goes in the handoff; the file
   is not edited by hand under that card.
3. **Coverage floors are the guard against over-pruning.** The `llvm-cov` thresholds in CI
   must hold after every IX-M4 sub-card. A sub-card that drops one removed a test that was
   doing work; put it back and say so.
4. **A pruning card keeps two layers** — the repository and one router-level test — plus
   every contract, golden and gate test. `wave2_gate`, `runner_contract`, `openapi_contract`
   and the two `docket_*_contract_test` binaries are never pruned.
5. **The budget is checked, not promised.** Every handoff's *Budget check* carries the output
   of `scripts/maintainability.py check --changed` and of `measure --totals` before and
   after. A card that re-baselines names the files it brought down.
6. **No new mechanism.** This Part deletes and moves. The only additions are
   `crates/tack-test-support` (IX-M3) and `scripts/gen-api-reference.py` (IX-M7). A card
   that wants a third has left the Part.
7. **Load cap.** IX-M4 sub-cards run two at a time; each compiles one test binary and runs
   only that binary until its final gate.

## §IX.2 Shared-file ownership

| File | Owner | Others |
|---|---|---|
| `scripts/maintainability.py`, `scripts/maintainability-baseline.json` | IX-M0; re-baselined only by IX-M1, IX-M2, IX-M3, IX-M8, IX-M9 | run, never edit |
| `.githooks/pre-push`, `.github/workflows/ci.yml` | IX-M0 (the `check` step), IX-M7 (the `cargo doc` step) | request in handoff |
| `crates/*/src/**` | IX-M1 (moves), IX-M2 (preambles), IX-M5 (adapters), IX-M6 (comments, per batch) | never two open cards on one file: M6 batches exclude M5's files |
| `crates/*/tests/**` | IX-M3 per crate, then IX-M4 per binary, then IX-M8 (same per-binary boundary, largest-first again) | a binary belongs to exactly one open sub-card at a time |
| `crates/tack-test-support/` | IX-M3-db creates; the other M3 sub-cards add | — |
| `docs/dev-notes/**` | IX-M2 creates, IX-M6 empties | — |
| `docs/adr/0064-fixed-waits.txt`, `scripts/list-fixed-waits.py` | the generator was blind to `<module>/tests.rs` until 2026-09-12 (fixed); 11 waits remain, IX-M8-<crate> empties its crate's lines | regenerate with the script, never hand-edit |
| `docs/dev-notes/**` (the 12 notes left) | IX-M6-dev-notes | — |
| `docs/book/src/roadmap.md` | IX-M7-roadmap | — |
| `docs/TESTING.md`, `CONTRIBUTING.md`, `CLAUDE.md`, `.claude/**`, the skills | rules written 2026-09-11; IX-M0 and IX-M9 adjust the numbers only | — |
| `docs/book/**`, `docs/API-REFERENCE.md`, `.gitattributes`, `scripts/regen-generated.sh` | IX-M7 | — |
| `TODO.md` Parts I–III, closed Parts' handoffs, `docs/closed-cycles/**` (new) | IX-M7 | — |

## §IX.3 Dependency graph and merge policy

```
M0 → M1 → M2 → M3-db → {M3-orch, M3-api, M3-runner, M3-cli} → M4 ×28 → M5 → M8 ─┐
                 │                                                              ├→ M9
                 └──── M6 batches (files no open M4/M5 card owns) ── M7 ────────┘
```

Integration line `develop`; branch `agent/ix-<card-lowercase>-<slug>`; one integrator per
wave, who regenerates generated files once and re-runs `check` on the integrated tree.

### Cross-Part conflicts

VIII-C3 (one stale unit test) is the only other open card. It owns one test file in
`tack-orch`; IX-M4's `tack-orch` sub-cards wait for it or take it over with the
integrator's agreement, noted in both handoffs.

## §IX.4 Cards

### IX-M0 — the tool lands, the gate warns

**Owns:** `scripts/maintainability.py`, `scripts/maintainability-baseline.json`,
`.githooks/pre-push`, `.github/workflows/ci.yml`, `.gitignore`.
**Acceptance:**
- `python3 scripts/maintainability.py baseline` output is committed; `check` is green on the
  untouched tree.
- `pre-push` and CI run `python3 scripts/maintainability.py check --changed` right after
  `check-test-hygiene.sh`; CI runs `check` without `--changed`.
- A tracked `crates/*/tests/scratch_*.rs` fails `check` — proven once with `git add -N` and
  undone; the pattern is gitignored.
- `docs/TESTING.md`, `/gate`, `/card`, `/feature` and `/integrate` name the command (the
  rules are already there).
**Verification:** `.githooks/pre-push` green on the untouched tree.

### IX-M1 — inline test modules move next to their module

**Owns:** every `crates/<workspace crate>/src/**/*.rs` whose trailing test module is over
150 lines, and the `<module>/tests.rs` files the script creates.
**Acceptance:**
- Proven first in a throwaway worktree: `extract-tests --apply` on the six workspace crates,
  `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo nextest run --workspace`, `scripts/check-comments.sh`, `scripts/check-test-hygiene.sh`
  — all green, same test count as before (1 502 run, 8 skipped, or the current number).
- Then once on `develop`, one commit, re-baseline.
- `measure` shows no production file with `inline_test_lines` over budget; the production
  line total is unchanged.
- `crates/tack-desktop` handled the same way with its own `cargo fmt` and `cargo test`, or
  the handoff says it was left and why.
**Verification:** the full suite, once; `check` green.

### IX-M2 — over-budget preambles leave the source

**Owns:** the 25 production files `extract-module-docs` lists, `docs/dev-notes/**`, the
`README.md` of each `crates/tack-runner/src/harness/fixtures/<kind>/` directory.
**Acceptance:**
- `extract-module-docs --apply`; the vendor findings in the two adapter preambles go to the
  fixture READMEs (harness audit §2.4), not to dev-notes.
- `cargo check --workspace`, `scripts/check-comments.sh` green; re-baseline.
- `comment-worklist` lists no `module doc` item in a production file.
- Every `docs/dev-notes/` file starts with the line that says it is transitional and must be
  resolved by IX-M6.
**Verification:** `cargo check --workspace`, `check-comments.sh`, `check`.

### IX-M3 — one place for a helper

**Owns:** `crates/tack-test-support/` (new; depends on `tack-core` and `tack-db` only, so no
crate is built twice), `crates/<crate>/tests/common/mod.rs`, and the test files of the
sub-card's crate. Sub-cards: IX-M3-db first (creates the crate: migrated pool, seeded
workspace/project/item, controllable clock, fixture loading), then the other four in parallel.
**Acceptance:**
- No test file in the crate defines its own `project()`, `request()`, `runner()`, `claim()`
  or pool helper; the plan's Appendix A `grep` lines return only `tests/common` and the
  support crate.
- No test removed, no assertion changed; the crate's `nextest` count is identical before and
  after.
- `wave2_gate.rs` keeps its own infrastructure by design and is excluded.
**Verification:** `cargo nextest run --workspace -E 'package(<crate>)'`, `check --changed`.

### IX-M4 — prune one test binary to the rules

**Owns:** one test binary (its file and directory). Sub-cards `IX-M4-<crate>-<binary>`,
largest first: `tack-db-repository` (`execution_repo.rs` alone is 4 323 lines),
`tack-api-runner_protocol`, `tack-api-handlers`, `tack-api-security`, then the rest by
`measure` size.
**Input per sub-card:** its `measure` rows, its `duplicate-tests` pairs, the layer map for
`replay`/`stale`/`idempotent` (plan Appendix A), and the entries of
`docs/adr/0064-fixed-waits.txt` that live in it. Nothing else.
**Acceptance (plan §2.2):**
- Variants are rows: no family of `<claim>_*` functions where one table-driven test would do.
- Every body ≤ 60 lines (target 40); every name ≤ 60 characters and states the claim without
  articles, narrative or board vocabulary.
- Preamble ≤ 10 lines; captured vendor output is a fixture file with a provenance line.
- An invariant already pinned at the repository layer and at one router-level test is not
  pinned again in this binary.
- No `sleep(` remains in the binary; its lines leave `0064-fixed-waits.txt`.
- CI coverage floors hold; `check --changed` green; no production file touched.
**Verification:** `cargo nextest run --workspace -E 'binary(<name>)'`, the coverage job,
`check --changed`.

### IX-M5 — the harness core

Card T0 of `docs/plans/harness-maintainability-audit.md` §6, unchanged: extract
`LocalProcessHarness` and `HarnessGrammar`; migrate `claude_code` and `codex` with no
behaviour change, their current tests as the proof; captured transcripts become fixture
files; the three live tests move under `tests/live/` as `#[ignore]`; then prune to the
audit's §5. Runs after IX-M4 has pruned `tack-runner`'s binaries. Exit criteria are the
audit's, measured by its Appendix A and by `check`.

### IX-M6 — comments trimmed to the budget, per batch

**Owns:** one batch of about ten production files from `comment-worklist --json`, grouped by
crate; a batch never includes a file an open IX-M4 or IX-M5 card owns.
**Acceptance (plan §3):**
- In the batch's files: no `///` block over 15 lines, no preamble over 30 lines, comment
  share ≤ 35 %. Kept: what the code does, why the non-obvious choice, what breaks, what is
  not true yet. Removed or moved: restatements, narratives, vendor lore (→ fixture README),
  design essays (→ ADR or the book's developer guide).
- The batch's `docs/dev-notes/` entries are resolved — ADR section, fixture README, or
  deleted — and say which in the handoff.
- `cargo check --workspace`, `scripts/check-comments.sh`, `check --changed` green. No test
  run is needed and none is run.

### IX-M7 — documentation generated, included, archived

**Owns:** `docs/book/**`, `docs/API-REFERENCE.md`, `scripts/gen-api-reference.py` (new),
`scripts/regen-generated.sh`, `.gitattributes`, the `cargo doc` step in
`.github/workflows/ci.yml`, `.claude/context-budget.md`, `TODO.md` Parts I–III, the closed
Parts' handoffs, and `docs/closed-cycles/**` (new).
**Acceptance (plan §4):**
- The book's developer pages for testing, architecture, configuration, MCP and deployment
  are `{{#include}}`s of the `docs/*.md` authorities; `mdbook build docs/book` green with
  the link check.
- `developer/api-reference.md` is generated from `docs/openapi.json`, listed in
  `.gitattributes` and produced by `regen-generated.sh`; `docs/API-REFERENCE.md` keeps only
  what a spec cannot say (auth surfaces, WebSocket, worked examples).
- CI runs `cargo doc --workspace --no-deps` with `-D rustdoc::broken_intra_doc_links`; no
  `missing_docs` lint anywhere.
- Closed Parts live in `docs/closed-cycles/boards/part-<n>.md` and their handoffs in
  `docs/closed-cycles/handoffs/part-<n>/`; `docs/closed-cycles/README.md` opens by saying
  nothing under it is current. The directory is excluded from the book, from
  `.claude/context-budget.md` and from `check-comments.sh`'s dead-pointer scan.
  `TODO.md`'s live table has one row per Part, each ≤ 300 characters.
- `.claude/context-budget.md` re-measured and reduced to the files that remain.
  (`git-cliff` is already in place: `cliff.toml`, `make changelog`, the release workflow.)

### IX-M8 — close the test-volume gap

Added 2026-09-12 by the Wave 32 audit note. The first pass (IX-M4) covered 19 of 29
integration binaries, none of the 74 unit-test modules, and left its own acceptance open
(135 bodies over 60 lines, 207 names over 60 characters, 17 files over 1 000 lines, 11 fixed
waits). This is the second pass, over everything, held to the acceptance below with no
ratchet excuse — a file that is over budget when the sub-card ends is a finding, not "not
worsened".

**Owns:** per crate, **both** `crates/<crate>/tests/**` and `crates/<crate>/src/**/tests.rs`
(plus any inline `mod tests`). Sub-cards `IX-M8-<crate>`, dispatched largest ratio first:
`tack-orch` (test:code 3.16 — `reconciler/tests.rs` 1 927, `docket_tick_contract_test.rs`
1 156, `docket_adapter_test.rs` 1 099, `runner_contract` 967), `tack-runner` (1.75 —
`engine/tests.rs` 3 032, `harness/claude_code/tests.rs` 1 299, `transport/tests.rs` 1 030,
`harness/tests.rs` 1 000), `tack-api` (1.66 — `runner_protocol/lifecycle.rs` 1 821,
`handlers/crud.rs` 1 294, `security/chaos_recovery.rs` 1 243, `remote_backup/tests.rs` 837),
`tack-db` (1.29 — `repository/execution_repo.rs` 4 308, `integration.rs` 1 285,
`migrations/orch_migrations.rs` 1 182), `tack-cli` (0.97, small; also takes
`crates/tack-desktop/src/supervisor.rs`'s 482-line inline module out to `supervisor/tests.rs`
with `extract-tests --apply`, since `tack-desktop` is outside the workspace IX-M1 ran on).
Re-measure before trusting any number here. One workspace-wide sub-card, **`IX-M8-dedup`**,
runs first and owns the 49 `duplicate-tests` pairs (39 `tack-api`, 5 `tack-orch`, 3
`tack-db`, 1 `tack-cli`, 1 `tack-runner`): merging a pair shrinks what a crate card then
reads.

**Named exclusions, carried from plan §7 — do not cut these for volume:** `wave2_gate.rs`;
`docs/contracts/runner-v1/` and every `tests/contract/` or `*_contract*` binary that
byte-pins a wire shape (`openapi_contract`, `runner_contract`, `docket_wire_contract_test`,
`model_policy_contract`; `docket_tick_contract_test` pins timing *behaviour*, not a shape,
and is **not** exempt); `tests/live/` and `docket_live_test`; any test whose removal would
drop a crate under its CI floor (core 85, db 70, api 70, orch 70, runner 85) — and because
that CI job runs only on pull requests and `main`, the sub-card **runs `cargo llvm-cov -p
<crate> --fail-under-lines <floor>` itself** before and after and records both numbers. A
file mostly made of exempt fixtures may stay large; the handoff says which exclusion
applies, per file.

**Input per sub-card:** its crate's `measure` rows and `measure --json` per-file budgets,
its `duplicate-tests --json` pairs, its lines of `0064-fixed-waits.txt`, and plan §2.2's
rules: rows for variants, ≤ 40-line bodies (the 60-line cap is the interim gate, 40 is the
acceptance here), ≤ 60-character names, an invariant pinned in at most two layers (`stale`
is still pinned in eight crate/layer combinations, `replay` in seven — Appendix A's loop
lists them).

**Acceptance, per sub-card, measured by `measure --json` on its crate, no ratchet:**
- No test body over 40 lines outside the named exclusions; none over 60 anywhere.
- No test name over 60 characters.
- No test file over 1 000 lines outside the named exclusions.
- Its lines of `0064-fixed-waits.txt` are gone (regenerate the file; do not hand-edit).
- `duplicate-tests` for the crate returns 0 pairs (`IX-M8-dedup` gets the workspace to 0
  first; a crate card keeps it there).
- `cargo llvm-cov -p <crate> --fail-under-lines <floor>` green after, number recorded.
- Scoped `nextest -E 'package(<crate>)'` green; `check --changed` green.
- `measure --totals` before and after in the handoff, **as a number, not a promise**: the
  achieved workspace ratio after the last sub-card is what IX-M9 locks. If it is still above
  0.8 once the exclusions are honoured, the handoff says which exempt files carry the gap.

### IX-M6-dev-notes — empty `docs/dev-notes/`

Added 2026-09-12. IX-M6's batches came from `comment-worklist`, which only lists files
currently over budget; the twelve notes IX-M2 parked belong to modules that are *under*
budget precisely because their preamble was moved out, so nine of them were never in any
batch. **Owns:** the 12 files under `docs/dev-notes/` (762 lines) and the `//!` preamble of
each module they came from. **Acceptance (plan §3):** each note has become an ADR section,
a fixture README paragraph, a ≤ 30-line preamble that carries what was load-bearing, or
nothing — and the handoff says which, per note; `docs/dev-notes/` is deleted;
`cargo check --workspace`, `check-comments.sh`, `check --changed` green. Same gate as an
IX-M6 batch; no test run.

### IX-M7-roadmap — the roadmap keeps only what is ahead

Added 2026-09-12. Plan §4 said `roadmap.md` "keeps only its forward-looking sections; the
rest archives with the Parts it records"; IX-M7's card text never carried it. **Owns:**
`docs/book/src/roadmap.md` (3 683 lines) and `docs/closed-cycles/boards/`. **Acceptance:**
the sections recording Phases 0–57 (Parts I–III) move verbatim to
`docs/closed-cycles/boards/roadmap-phases-0-57.md` with the same one-line archived notice
the board files carry; what stays is the status board, the Part IV–IX sections and every
section that names a phase not yet shipped; every link into the moved text is repointed
(`grep -rn "roadmap.md#" docs/ README.md` lists them); `mdbook build docs/book` green with
no dead anchor; `.claude/context-budget.md`'s row re-measured.

### IX-M9 — ratchet to the targets

Was IX-M8 before the 2026-09-12 split; unchanged in kind, changed in what it locks and in
what the tool has to grow first.

**Owns:** `BUDGETS` and `check` in `scripts/maintainability.py`, the baseline, the numbers
in `docs/TESTING.md` and `CLAUDE.md`.
**Acceptance:**
- `check` gains the per-card budget plan §2.3 promised and never had: with `--changed`, more
  than 15 new tests or 600 new test lines against the baseline is a failure (today it is
  prose only).
- `BUDGETS` set to the targets — 40-line bodies, 30 % comment share (met: 19.8 % on
  2026-09-12), test files ≤ 1 000 lines — and `check` switched from "over budget *and*
  worse than baseline" to "over budget", with an explicit exclusion list in the script for
  the files IX-M8's handoffs named (each entry carries the file and the reason). Today that
  switch would fail 96 files at the old budgets and 129 at these; after IX-M8 the number
  must be the exclusion list's length, and the handoff shows it.
- The workspace ratio ceiling is **IX-M8's measured, achieved ratio**, not the plan's
  original 0.8; if a gap to 0.8 remains, `docs/TESTING.md` names it and the reason.
- Green on the tree; `measure --totals` recorded in the handoff and in the plan's §1 table.

## §IX.5 Definition of done, and deliberate exclusions

Done when `check` is a hard gate at the target budgets and is green; the test : production
ratio is at or below whatever IX-M8 measures as achievable within its named exclusions
(target remains 0.8; a recorded, reasoned gap to it is an accepted outcome, not a failure);
`comment-worklist`, `duplicate-tests` and `0064-fixed-waits.txt` are empty; the book builds
from included sources with a generated API reference; and the closed cycles are out of the
tree.

Excluded: the frontend (inside the target ratio already; the rules bind new tests only);
any change to behaviour or wire contracts; any abstraction beyond the two named; IX-M8's own
named exclusions (`wave2_gate.rs`, contract fixtures, live tests, coverage-floor-guarded
tests); release, signing and launch work, which resume after IX-M9.

## §IX.6 Handoff additions for this Part

`docs/agent-handoffs/part-ix/TEMPLATE.md` adds three sections: *Budget check* (the
`check --changed` output, `measure --totals` before and after), *What was removed* (tests or
comment lines, each with its reason class from plan §2.2 or §3), *Re-baselined?* (yes or no;
if yes, which files and why).

---

# Part VIII — Docket bridge hardening (Phase 62)

**Status: CARDS DONE 2026-09-08 — all three waves integrated; ADR 0065 accepted the same
day.** One card is open and unwaved: **VIII-C3**, a stale unit test VIII-C2's live capture
exposed. It blocks nothing. Original opening note follows. Opened 2026-09-08 from a CTO/CEO
review of the docket ⇄ Tack integration. ADR 0060
decided the Docket bridge is maintained, optional and never the owner of a runner-v1
request. It left four things unfinished, and this Part finishes them: a trait method with
no caller, a guard enforced in one direction only, a compatibility decision no operator can
see, and a documented claim about the schema that two documents disagree about.

This Part adds **no new capability to the bridge's shape**. It closes gaps ADR 0060 named
and one it created. A card that finds itself designing a new Docket surface has left the
Part — say so in the handoff and stop.

Dispatch plan: **`docs/agent-handoffs/part-viii/README.md`** — read its header and your
card's block only.

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 24 — Close the four gaps | VIII-A1 · VIII-B1 · VIII-B2 · VIII-C1 | 62 | **Integrated 2026-09-08** on `integrate/viii-wave-24`. Four branches, **zero file overlap**, no conflicts. Gate green: `pre-push` clean, 1485 Rust tests (7 skipped), 857 frontend tests, type-check clean, no generated-file drift. **A1's finding is the wave's real result and it changed the ADR:** docket's dispatch route creates the run record and answers *before* the pipeline runs, on a daemon thread, so a `pre_input` block is never an HTTP error there the way it is on `enqueue_task` — a run id says a run started, never that it was permitted, and the reconciler `/runs` poll is the only place the verdict ever becomes visible. **C1 settled a claim two documents disagreed about:** neither staging table exists in a migrated database (48 tables), and the sibling "11 `orch_*` tables" was the same naive grep counting the staging names — the real figure is 9 plus `control_planes`. The integrator corrected both archive copies and replaced the wrong command beside one of them. **B1 was returned once** for reimplementing the orchestration-enabled resolver with a hardcoded `'orch_config'` where the original binds `ORCH_KEY` — it failed *open*, silently, which is the failure the card exists to prevent; now one resolver, injected as a callback because `executions.rs` must keep compiling standalone under `#[path]`. **C1 was returned once** for leaving the sibling miscount standing. One finding routed to VIII-B3 | 
| 25 — The caller | VIII-A2 · VIII-B3 | 62 | **Integrated 2026-09-08** on `integrate/viii-wave-25`. Two branches, **zero file overlap**, no conflicts. Gate green: `pre-push` clean, 1497 Rust tests (7 skipped), 857 frontend tests, type-check clean, no generated-file drift. **A2 shipped the caller** — the route, the fail-closed `TACK_ORCH_DISPATCH_TOKEN` on its own header, and `tack orch dispatch`; every operator-facing string was checked and each use of "permitted" is a negation. **A2's open question turned out to be a real hole and is now VIII-A3:** a dispatched run is stored in `orch_runs` with a null item correlation (the store permits that explicitly), but the only route reading that table is item-scoped, so the run id the CLI hands back is unusable through any Tack route. That is a gap between ADR 0065's decisions 5 and 7, not a defect in A2, which correctly refused to invent a read surface. **B3 was returned twice.** First for duplicating the active-status literal into a third query, which made a neighbouring doc comment's "defined once" claim false twelve lines away. The composition fix then orphaned `has_active_docket_task_for_item` and falsified a module doc in `legacy_bridge.rs` — the integrator called the removal (§VIII.1 rule 6 governs pre-existing dead code, not code a card just killed) and B3 recorded it as the integrator's call. Collapsing the guard to one read also retired the untestable race B3 had disclosed |
| 26 — Proof and the missing read | VIII-C2 · VIII-A3 | 62 | **Integrated 2026-09-08** on `integrate/viii-wave-26`. Two branches, zero file overlap, no conflicts. Gate green: `pre-push` clean, 1502 Rust tests (8 skipped — the seventh skip is C2's new opt-in live test), 857 frontend tests, type-check clean, no generated-file drift. **C2 stood a real `docket serve` up at `v0.2.0-beta.2`** and re-dated every live claim: `enqueue_task`, all three `pre_input` outcomes with the `trusted` boundary, and `provision_pod` are confirmed unchanged; `decide_approval`'s `deny`, its 409 replay and the unknown-token 404 move from source-read to captured, so all four approval outcomes now have live evidence. `~/.docket` was never touched (mtime unchanged) and the whole capture cost nothing — the isolated server had no reachable provider credential, so the pipeline failed on local model resolution with `costUsd` flat at 0. **ADR 0065's "a block is not synchronously observable" claim held, and the capture sharpens it:** `pre_input` structurally cannot fire on this route at all (it evaluates once at a task's own enqueue), so the only guardrail that could ever fire here asynchronously is `pre_output`, which needs a real costed hop and is recorded `not_measured`. **C2 also falsified an assumption this Part carried in: an unknown project does not 404 on `/dispatch/{project}`** — the route never checks for a pod before creating the run record, so it returns the same 200 and the failure surfaces only on the asynchronous run read. Nothing decodes wrongly, so no code changed; the stale unit test modelling that 404 is left standing and carded as **VIII-C3**. **A3 closed the observability gap** Wave 25 found: `GET /api/orch-runs/{run_id}` plus `tack orch run`, reusing the existing repository query. It answers 200 with `mirrored: false` for a run it has not seen rather than 404 — the card asked for an un-polled run to be distinct from an unknown id, and it cannot be, because nothing on this side records that a dispatch happened; the integrator accepted the deviation as the more honest reading of the card's own "never a fabricated state" rule. The two cards compose: the failure mode C2 discovered is exactly what A3's route now makes readable. **Verifying A3 found a documented falsehood** — `docs/CONFIG.md` and ADR 0065's measured evidence table both said orch routes 404 with the flag unset; they answer `409 orchestration_disabled`, which eight test sites already asserted. Corrected in both at integration, sourced to the code this time. **Last.** C2 re-verifies the adapter live against a real `docket serve` built from `../rack-cli` at `v0.2.0-beta.2`, including what A1 and A2 landed. A3 closes the observability gap Wave 25 found. Both dispatchable now that Wave 25 is integrated; they own disjoint files |

## §VIII.0 Cold-start context capsule

**What this Part is for, in one sentence.** The Docket bridge is maintained by decision;
this Part makes the code, the guard and the documentation say the same thing the decision
does.

**The decisions of record are ADR 0060** (`docs/adr/0060-docket-control-plane-disposition.md`,
accepted 2026-08-31 — the bridge is maintained, optional, never the owner of a runner-v1
request) **and ADR 0065** (`docs/adr/0065-docket-pipeline-dispatch-trigger.md`, **accepted**
2026-09-08 — the pipeline-dispatch trigger's caller, token and non-claim on an item). Read
0065's decision table, not a paraphrase. No card re-decides one; a card that finds a
decision impossible stops and says so in its handoff.

### Evidence base, measured 2026-09-08

| Fact | Value | How it was checked |
|---|---|---|
| `ControlPlane::dispatch` | returns `OrchError::Disabled` unconditionally; the recorded reason is "no consumer in Tack yet", not a missing docket route | read `crates/tack-orch/src/adapters/docket.rs` module doc |
| docket's own route | `POST /dispatch/{project}`, bearer-authenticated, alongside `/tasks/{project}`, `/approvals/{token}`, `/pods` | read `../rack-cli/src/docket/serve.py` `do_POST` |
| docket's read routes | `/status.json`, `/metrics`, `/health` unauthenticated; `/runs`, `/runs/{id}`, `/approvals`, `/tasks/{project}`, `/traces/{project}` bearer | same file, `do_GET` |
| docket knows Tack by name | `APPROVAL_CHANNELS` includes `"tack"` beside `cli`/`http`/`mcp`/`telegram`/`timeout` | `../rack-cli/src/docket/core/approval.py:65` |
| The legacy→runner-v1 guard **is** enforced | `dispatcher.rs:329` calls `has_active_execution_request_for_item` and returns a `Conflict` | read |
| The mirror guard is **not** | `handlers/executions.rs` (`POST /api/executions`) does not consult `orch_tasks`; `legacy_bridge.rs`'s module doc says so in bold and a test documents it | read `crates/tack-orch/src/adapters/legacy_bridge.rs`, "One scheduling owner" |
| The compatibility label | `LEGACY_DOCKET_COMPATIBILITY_LABEL = "legacy-docket:maintained-bridge-v1"`, plus a prose `..._POLICY`; **no route surfaces either** | `grep -rn LEGACY_DOCKET_COMPATIBILITY` — hits only `legacy_bridge.rs` |
| `orch_runs_new` / `orch_approvals_new` | `.claude/scope-discipline.md` calls them leftovers still in the schema; **ADR 0060 measured the opposite** — transient staging names inside the 037/038 rebuild, `ALTER TABLE orch_runs_new RENAME TO orch_runs`. Two documents disagree; VIII-C1 measures | `grep -n 'orch_runs_new' crates/tack-db/src/migrations.rs` |
| docket on this machine | `~/.local/bin/docket`, version `0.2.0b1` — **older than the repo's `v0.2.0-beta.2`**. VIII-C2 builds from `../rack-cli`, it does not use this one | `docket --version` |
| Docket state dir in use | `~/.docket` holds real approvals and an audit log, and a worktree of *this* repo | `ls ~/.docket` |

### Vocabulary

*The bridge* is the legacy Docket control plane. *runner-v1* is the native execution
domain. *A pipeline dispatch* is docket's project-level `POST /dispatch/{project}` — it is
never called "running an item", because it claims no item (ADR 0065 decision 5).

---

## §VIII.1 Rules for simultaneous agents

**All of §III.2, §V.1, §VI.1 and §VII.1 apply unchanged.** Six rules are specific to this Part:

1. **This Part closes gaps; it does not design surfaces.** Every card's Acceptance is
   satisfiable by reading ADR 0060, ADR 0065 and the code. A card that needs a new decision
   stops and writes the question in its handoff — it does not decide.
2. **docket's source is the contract, not Tack's types.** When a card needs to know what
   docket sends or accepts, the answer comes from reading `../rack-cli/src/docket/`, never
   from a Tack DTO, a fixture, or this board's prose. `../rack-cli` is a **separate
   repository — read it, never write to it**, and never commit anything from it into this tree.
3. **Never touch `~/.docket`.** It holds the user's real approvals and audit log. Any card
   that runs docket sets an isolated `DOCKET_HOME` to a temporary directory it owns, and
   says in its handoff which one.
4. **`cargo` writes outside `/home`.** That partition is 94% full (41G free) and this tree's
   `target/` is already 87G. Every card exports `CARGO_TARGET_DIR` to the path its dispatch
   prompt pins, on `/`. A card that fills the disk fails the whole wave, not just itself.
5. **Generated files are regenerated once, by the integrator.** VIII-B2 changes an API
   response shape. It regenerates `docs/openapi.json` and `schema.gen.ts` with the documented
   commands so its own branch is coherent, and the integrator regenerates once at the end.
   Neither is ever hand-edited or hand-merged.
6. **No removal without a decision record.** Scope-discipline rule 6 is live in this Part
   because VIII-C1 looks like a deletion card and is not. Finding dead code is a handoff
   note; deleting it needs its own card and its own decision.

---

## §VIII.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `crates/tack-orch/src/adapters/docket.rs` (the `dispatch` method and the "Write methods" paragraph of its module doc), the `dispatch` trait doc in `crates/tack-orch/src/lib.rs`, new cases in `crates/tack-orch/tests/docket_adapter_test.rs` and `docket_wire_contract_test.rs` | VIII-A1 |
| `crates/tack-api/src/handlers/executions.rs`, one read-only query in `crates/tack-db/src/repo/orch.rs`, `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`, and the "One scheduling owner" paragraph of `crates/tack-orch/src/adapters/legacy_bridge.rs` | VIII-B1 |
| One response shape in `crates/tack-api/src/handlers/orch.rs`, its frontend renderer under `frontend/src/features/settings/orchestration/`, `docs/openapi.json` + `frontend/src/shared/api/schema.gen.ts` (regenerated, never hand-edited) | VIII-B2 |
| `.claude/scope-discipline.md` (the `orch_*_new` bullet only) and any other document its own grep finds repeating that claim | VIII-C1 |
| `crates/tack-api/src/handlers/orch.rs` (one new route), `crates/tack-api/src/config.rs` (`TACK_ORCH_DISPATCH_TOKEN`), the `tack orch dispatch` arm in `crates/tack-cli/`, `docs/CONFIG.md`, `docs/API-REFERENCE.md` | VIII-A2 — **after A1, B2 and ADR 0065 acceptance** |
| The idempotent-replay case in `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs` and the conflict payload in `crates/tack-api/src/handlers/executions.rs` | VIII-B3 |
| One read route in `crates/tack-api/src/handlers/orch.rs`, its `tack orch run` client in `crates/tack-cli/`, `docs/API-REFERENCE.md`, and the regenerated `docs/openapi.json` + `frontend/src/shared/api/schema.gen.ts` | VIII-A3 |
| The "Verified live against a real docket server" section of `crates/tack-orch/src/adapters/docket.rs`, `docs/agent-handoffs/part-viii/VIII-C2.md` | VIII-C2 — **last** |
| `TODO.md`, `docs/book/src/roadmap.md` statuses, ADR status lines | wave integrator only |

---

## §VIII.3 Dependency graph and merge policy

```text
ADR 0060 (accepted) ─┬─ VIII-A1 (adapter dispatch) ────────┐
                     ├─ VIII-B1 (mirror guard) ─────────┐  │
                     ├─ VIII-B2 (compatibility label) ──┼──┴─ VIII-A2 (route + token + CLI) ── VIII-C2 (live proof)
                     └─ VIII-C1 (measure orch_*_new) ───┘
ADR 0065 (accepted) ──────────────────────────────────────── required before VIII-A2 only
```

**Wave 24's four cards are independent and own disjoint files.** A1 is `tack-orch/adapters/
docket.rs`; B1 is `tack-api/handlers/executions.rs` plus `legacy_bridge.rs`; B2 is
`tack-api/handlers/orch.rs` plus the frontend; C1 is documentation. The only crate two cards
share is `tack-orch`, and A1 and B1 touch different files in it.

**A2 waits for three things**, not one: ADR 0065 accepted, A1's method to call, and B2's
`handlers/orch.rs` edit to merge before it adds a route to the same file.

### Cross-Part conflicts

None. Parts IV–VII are closed and no card here writes `README.md`, `docs/screenshots/**`
or any file those Parts' §.3 sections reserve. A card that finds a collision anyway states
it in the handoff and stops.

---

## §VIII.4 Cards

### VIII-A1 — `DocketAdapter::dispatch`, implemented

Wave 24, parallel. **Does not need ADR 0065 accepted** — decision 1 only restates what
docket's route already is, and this card adds no caller.

**Owns:** the `dispatch` method in `crates/tack-orch/src/adapters/docket.rs` and the
"Write methods" paragraph of its module doc; the `dispatch` trait doc in
`crates/tack-orch/src/lib.rs`; new cases in `crates/tack-orch/tests/docket_adapter_test.rs`
and `docket_wire_contract_test.rs`; the VIII-A1 handoff.

**Acceptance**

1. `dispatch(project, vars)` POSTs to `/dispatch/{project}` with the bearer token.
   **Read `../rack-cli/src/docket/serve.py`'s `do_POST` for the exact body shape and
   response before writing a line of it** — docket's source is the contract (§VIII.1 rule 2).
   State in the handoff what you read and what it said.
2. It returns docket's run id as `Ok(String)`, matching what `enqueue_task` does with a task id.
3. A `pre_input` policy block maps to `OrchError::PolicyBlocked` by **reusing**
   `parse_policy_block` — do not write a second parser. If docket's dispatch route reports
   blocks differently from its task route, that is a finding: say so and map it honestly.
4. 401 → `Auth`, 404 → `NotFound`, other non-2xx → `Http`. No new error variant.
5. A wiremock case in `docket_wire_contract_test.rs` pins the request Tack sends — method,
   path, headers, body — and the response it decodes, and the file's oracle table gains its row.
6. The module doc no longer says `dispatch` returns `Disabled`. It says what the method does,
   in the present tense, with no account of what it used to do.
7. **Reverting the method body fails exactly the new test**, and the handoff records that run.

**Must not:** touch `legacy_bridge.rs`, any `tack-api` or `tack-cli` file, `docs/openapi.json`
or `schema.gen.ts`. The route and the CLI are VIII-A2's, and adding them here is the
scope-widening §VIII.1 rule 1 forbids.

### VIII-B1 — the mirror guard, enforced

Wave 24, parallel.

**Owns:** `crates/tack-api/src/handlers/executions.rs`; one read-only query in
`crates/tack-db/src/repo/orch.rs`; `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`;
the "One scheduling owner" paragraph of `crates/tack-orch/src/adapters/legacy_bridge.rs`;
the VIII-B1 handoff.

**Acceptance**

1. `POST /api/executions` refuses to create a request for an item an active Docket
   `orch_tasks` row already owns, with a conflict whose message names the collision —
   mirroring the shape `dispatcher.rs:332` already returns in the other direction.
2. **"Active" is defined once, in the query, and its doc comment says which
   `orch_tasks.status` values count and why.** Derive that list by reading the statuses the
   reconciler actually writes, not by guessing from names.
3. **The guard consults `orch_tasks` only when orchestration is enabled.** With
   `TACK_ORCH_ENABLE` off, a stale row from a previously-enabled bridge must never block
   runner-v1 — that would invert "runner-v1 is the plan of record". A test pins both states.
4. `dual_scheduling.rs` stops documenting the asymmetry and proves the guard in **both**
   directions. Prove the absence directly (no `execution_requests` row written), not by
   status code alone.
5. **Reverting the guard fails exactly that test**, and the handoff records the run.
6. `legacy_bridge.rs`'s "**The mirror guard is not implemented**" sentence is replaced by
   what is now true. Change nothing else in that module doc.

**Escalate, do not decide:** if adding a conflict response to `POST /api/executions`
requires a new documented status code or error code in `docs/openapi.json`, **stop** — that
is a response-shape change this card does not own. Write the question in the handoff.

**Must not:** touch `adapters/docket.rs`, `dispatcher.rs`'s existing guard, or any file
VIII-B2 owns.

### VIII-B2 — the compatibility decision reaches an operator

Wave 24, parallel.

**Owns:** one response shape in `crates/tack-api/src/handlers/orch.rs`; its renderer under
`frontend/src/features/settings/orchestration/`; the regenerated `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts`; one new case under `crates/tack-api/tests/orchestration/`;
the VIII-B2 handoff.

**Acceptance**

1. One existing orch response carries `LEGACY_DOCKET_COMPATIBILITY_LABEL`, **imported from
   `tack_orch::adapters::legacy_bridge` and never re-typed as a literal**. A test asserts the
   response value equals the constant, so the wire cannot silently diverge from the decision.
2. Which response, decided by reading `handlers/orch.rs`. Name the one you chose and the one
   you rejected in the handoff, with the reason.
3. Decide where `LEGACY_DOCKET_COMPATIBILITY_POLICY` (the prose) belongs — rendered beside
   the label in the UI, or carried in the JSON. Either is acceptable; **say which and why**.
4. The frontend renders the label from the API, never from a hardcoded string. A unit test
   with a mocked response pins that, and colours come from `--color-*` tokens only.
5. Regenerate both generated files with the documented commands (`UPDATE_OPENAPI=1 cargo
   nextest run --workspace -E 'binary(openapi_contract)'`, then `cd frontend && npm run
   gen:api`, or `./scripts/regen-generated.sh`). Never hand-edit or hand-merge either.

**Must not:** add a route (that is A2's), or change any behaviour the label describes. This
card surfaces a decision; it does not alter one.

### VIII-C1 — measure the `orch_*_new` claim, then correct what repeats it

Wave 24, parallel. **Documentation only.**

**Owns:** the `orch_runs_new`/`orch_approvals_new` bullet in `.claude/scope-discipline.md`,
any other in-tree document its own grep finds repeating that claim, and the VIII-C1 handoff.

**Acceptance**

1. Run the measurement ADR 0060 documents — migrate a fresh in-memory database and read
   `sqlite_master`. **Record the exact command and its output in the handoff.** A load-bearing
   number carries the command that produces it.
2. Read the 037/038 rebuild in `crates/tack-db/src/migrations.rs` and state *why* the answer
   is what it is — not just that it is.
3. Correct every document the grep finds. **If the claim turns out to be true, correct
   nothing and say so.** This card is the measurement; the edit is whatever the measurement
   licenses.
4. **Delete no table, migration or code.** If the measurement shows a real leftover, that is
   a finding for a new card — §VIII.1 rule 6.
5. The handoff carries the command, its output, and the list of documents changed.

**Why this card exists:** `.claude/scope-discipline.md` and ADR 0060 state opposite things
about the same two names, and the scope-discipline bullet is cited as evidence for how much
the bridge costs. One of the two is wrong and has been quoted since.

### VIII-A2 — the trigger gets a caller: route, token, CLI

Wave 25, parallel with VIII-B3. **Binding decision record: ADR 0065, accepted 2026-09-08 —
read its decision table and its "A block is not synchronously observable on this route"
section, not a paraphrase.** Needs VIII-A1's method and VIII-B2's `handlers/orch.rs` edit,
both merged.

**Owns:** one new route in `crates/tack-api/src/handlers/orch.rs` and its registration in
`crates/tack-api/src/router.rs`; `TACK_ORCH_DISPATCH_TOKEN` in `crates/tack-api/src/config.rs`;
a new `orch dispatch` command in `crates/tack-cli/`; `docs/CONFIG.md`; `docs/API-REFERENCE.md`;
the regenerated `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts`;
one new case under `crates/tack-api/tests/orchestration/`; the VIII-A2 handoff.

**Acceptance**

1. `POST /api/projects/{id}/orch-dispatch` calls `ControlPlane::dispatch` with the docket
   project resolved from that project's **existing** `orch_links` row (decision 2). No second
   way to name a docket project is invented; a project with no link is a `404`, never a guess.
2. It requires **`TACK_ORCH_DISPATCH_TOKEN`**, checked inside the handler on top of
   `require_token`, and **fail-closed when unset** — mirror `require_approval_token`
   (`handlers/orch.rs`, ~line 2688) including its "unset means nothing on this server is
   dispatchable" reasoning. A test proves a server with the variable unset refuses.
3. It sits inside `orch_routes`, so with `TACK_ORCH_ENABLE` off the route is not reachable at
   all (decision 4). No new gate shape is invented.
4. **It writes nothing about a Tack item** (decision 5): no `orch_tasks` row, no
   `execution_requests` row, no item id in the request body, and `decide_scheduling_owner` is
   not consulted. **Assert the absence directly** — row counts unchanged, not a status code alone.
5. The `variables` body is passed through as opaque JSON and **never logged** (decision 6). A
   test asserts the redaction, the way this tree's existing redaction tests do.
6. **The response says a run started, never that it was permitted.** ADR 0065's
   "not synchronously observable" section binds the wording of the response, the handler doc
   comment, the CLI output and `docs/API-REFERENCE.md`. Each points the reader at the
   reconciler `/runs` poll as the only place the verdict appears (decision 7).
7. `tack orch dispatch <project>` is the in-tree caller (decision 8) — HTTP-only like every
   other CLI command, never opening the database. It reads its token from the environment and
   says plainly when it is unset.
8. `docs/CONFIG.md` gains `TACK_ORCH_DISPATCH_TOKEN` in the orchestration table, stating
   fail-closed-when-unset; `docs/API-REFERENCE.md` gains the route.
9. Generated files are **regenerated with the documented commands**, never hand-edited.
10. **Reverting the token check fails exactly the test from acceptance 2**, and the handoff
    records that run.

**Must not:** build a UI, attach the run to a Tack item, touch `decide_scheduling_owner`, add
retry/schedule/cancel (`capabilities.cancel` is `false` and the route must not imply
otherwise), add a second ingestion path for the run, or change
`crates/tack-orch/src/adapters/docket.rs` — VIII-A1 settled that file.

### VIII-B3 — prove the replay case, and name the collision

Wave 25, parallel with VIII-A2. **Not blocked by ADR 0065.**

**Owns:** the idempotent-replay case in `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`;
the conflict payload in `crates/tack-api/src/handlers/executions.rs`; the VIII-B3 handoff.

**Why this card exists.** The mirror guard skips itself for an idempotent replay — the
`existing_snapshot.is_none()` arm — and the handoff that shipped it records that this is
**not separately tested**. It is the case most likely to regress silently: a client retrying
a create it already made must never start getting a conflict because Docket claimed the item
in between.

**Acceptance**

1. A test proves an idempotent replay of an existing execution request still succeeds while
   the item has an active Docket task. Prove it load-bearing: removing the
   `existing_snapshot.is_none()` arm must fail exactly this test.
2. The `409` payload names **which** Docket task collided and its status, not only the
   `item_id` — diagnosing a collision today means reading `orch_tasks` by hand.
3. Adding fields to that payload must not change the documented error code: it stays
   `StableErrorCode::Conflict`, which `create_execution`'s existing `responses(...)` already
   covers. **If the payload change needs a spec edit, regenerate — never hand-edit — and say
   so in the handoff.**
4. Logs carry ids only. The task id and its status are ids; nothing else from the row is added.

**Must not:** change the guard's own condition, its status set, or the enablement check —
those are settled and are not re-decided here.

---

### VIII-A3 — a dispatched run's outcome can be read back

Wave 26, parallel with VIII-C2. **Closes a gap between ADR 0065's decisions 5 and 7**, found
while integrating Wave 25. It does not re-open either decision.

**Owns:** one read route in `crates/tack-api/src/handlers/orch.rs`; its `tack orch run` client
in `crates/tack-cli/`; `docs/API-REFERENCE.md`; the regenerated `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts`; one new case under `crates/tack-api/tests/orchestration/`;
the VIII-A3 handoff.

**The gap, measured 2026-09-08.** `reconciler::persist_runs` correlates each polled run to a
Tack item and stores it either way — `ControlPlaneStore::find_item_for_remote_task`'s own doc
says CLI-dispatched work must not error there, and `upsert_orch_runs` `COALESCE`s so a `None`
never clobbers a learned attribution. So a pipeline run dispatched by VIII-A2 **is** ingested,
with `orch_runs.item_id` null by ADR 0065 decision 5. But the only route that reads that table
is `list_orch_runs_for_item` (`handlers/orch.rs`), which is item-scoped. The run id
`tack orch dispatch` returns therefore reaches no Tack route at all: the outcome is recorded
and unreachable, and only docket can answer for it.

**Acceptance**

1. An operator can read a dispatched run's mirrored state by its run id through Tack's own API,
   without an item. `Repository::get_orch_run` already exists and is called only by
   `orch_store.rs` — reuse it; do not add a second query.
2. The route lives inside `orch_routes`, so `TACK_ORCH_ENABLE` off means unreachable. It is
   **read-only**, so it takes no privileged token beyond `require_token` — the dispatch token
   gates spending money, not reading what was spent.
3. It reports the run's state as **mirrored at the last poll**, and says so. It never fetches
   from docket inline: that would be the second ingestion path ADR 0065 decision 7 forbids.
   An un-polled run is a legitimate answer, distinct from an unknown run id.
4. `tack orch run <run_id>` is the in-tree caller, pairing with `tack orch dispatch`.
5. **Unmeasured is nullable.** A run polled before its outcome exists reports null, never a
   fabricated state, and never `0`/`success` standing in for "not yet known".
6. Generated files regenerated with the documented commands, never hand-edited.

**Must not:** attach the run to a Tack item, add a docket fetch on the read path, add a second
`orch_runs` query, or widen this into a runs *listing* — one run by id is the gap; a fleet-wide
run browser is a separate decision.

### VIII-C2 — re-verify the adapter against a real docket server, at the version the repo ships

Wave 26, parallel with VIII-A3. **Last card of this Part.** Every live claim in
`adapters/docket.rs` was captured against docket `0.2.0b1`; the repository is now at
`v0.2.0-beta.2`, and two methods that shipped in this Part — `dispatch` (VIII-A1) and the
route that calls it (VIII-A2) — have never met a real server at all.

**Owns:** the "Verified live against a real docket server" section of the module doc in
`crates/tack-orch/src/adapters/docket.rs`, and the VIII-C2 handoff.

**Acceptance**

1. Build and run `docket serve` from `../rack-cli` at tag `v0.2.0-beta.2` — not
   `~/.local/bin/docket`, which is `0.2.0b1` and older than the repository. Record in the
   handoff how it was built and how the version was confirmed from the running process.
2. **`DOCKET_HOME` points at a temporary directory the card owns.** Record `~/.docket`'s
   mtime before and after and show it unchanged, the way the `provision_pod` capture already
   in that section does.
3. Exercise `dispatch` **through this crate's own compiled adapter**, not a hand-built
   `curl` — the standard the `provision_pod` capture set. Confirm against the live wire: the
   run id arrives under `run` and not `task`; an absent body means `{}`; an unknown project
   404s; and a request with no `Authorization` header is rejected.
4. **Confirm or refute ADR 0065's central claim** that a guardrail block is not
   synchronously observable on this route: the response arrives before the pipeline runs, so
   no verdict can reach the caller. If the live server contradicts that, it is a finding
   against the ADR — write it in the handoff and change no decision.
5. Re-verify every claim already in that section against `v0.2.0-beta.2` and mark each one
   **confirmed**, **changed** (with the new capture), or **not re-run** (with why). A claim
   carried forward untested is recorded as carried forward, never as verified.
6. Close or re-record the two gaps that section admits: `decide_approval`'s `deny` and
   409/`ApprovalNoop` paths, read from source and never captured live. If they are now
   reachable, capture them; if not, say what blocks it.
7. **Unmeasured is nullable.** Anything the run could not reach — a pipeline that needs a
   real provider key, a route the isolated instance cannot serve — is written as
   `not_measured` with the reason. No claim is upgraded from "read the source" to "verified
   live" without a capture behind it.

**The trap: a dispatch runs a real pod pipeline, and a pipeline spends money.** The route
answers before the work does, so the run id — the observable under test — is obtained
without any of the pipeline succeeding. Give the isolated instance no working provider
credential, so the pipeline fails locally instead of reaching a paid API, and say so in the
handoff. A capture that cost real model spend is a failure of this card even if the numbers
are right.

**Must not:** write to `../rack-cli` or commit anything from it; touch `~/.docket`; add or
change any Tack route, type or test; or fix a defect it finds. The single exception, because
this card is last and holds the only live server: if a capture proves `dispatch`'s own
response decoding wrong, correct that decode and name the ownership widening in the handoff.
Anything larger — a reconciler behaviour, an API shape, a second adapter method — is a
finding for a new card.

**Why this card exists:** the "Verified live" section is the reason a reader trusts this
adapter over docket's own documentation, which it contradicts in four places. It is evidence
with a date on it, and two of the methods it vouches for are older than the code beneath
them.

### VIII-C3 — a unit test models a response the real server never sends

Unwaved, small, dispatchable any time. **Found by VIII-C2's live capture**, which is the
only reason anyone knows.

**Owns:** `dispatch_404_maps_to_not_found` in `crates/tack-orch/tests/docket_adapter_test.rs`,
and whatever the measurement licenses in `DocketAdapter::dispatch`'s own `NOT_FOUND` arm.

**The finding.** `POST /dispatch/{project}` never checks whether the project has a pod
before creating the run record. Dispatching a project docket has never heard of returns the
ordinary `200` with a run id; the failure surfaces only on the asynchronous run read. So the
adapter's `404 → OrchError::NotFound` mapping, and the unit test that pins it, describe a
response this route does not produce. Neither is wrong — the decode is correct if a `404`
ever arrived, and docket does return `404` from other authenticated routes on the same
server — but a test whose scenario the real server cannot reach proves nothing about it.

**Acceptance**

1. Re-measure against a live server rather than trusting this card: confirm no branch of
   `/dispatch/{project}` returns `404`, and record the command.
2. Decide, and record the reasoning, between: keeping the arm as documented defence with the
   test renamed to say it pins a decode and not a server behaviour; or removing both. **A
   removal needs its own decision record** — §VIII.1 rule 6.
3. Whatever is decided, the module doc and the test must not leave a reader believing the
   live server produces a `404` here.

**Must not:** change what `dispatch` sends, or touch any other adapter method.

**Why this card exists:** the tree's recurring defect is a mechanism with no caller. This is
its smaller cousin — a test with no reachable scenario, which reads as coverage and is not.

## §VIII.5 Definition of done, and deliberate exclusions

**Done** is `.githooks/pre-push` green on the integration branch, every card's handoff
written, and the wave's generated files regenerated once by the integrator. A green test
suite is not a finished change: `cargo fmt` for the workspace *and* `crates/tack-desktop`
separately, `check-comments.sh`, and `check-test-hygiene.sh` are part of the gate, and
formatting is invisible until a push is attempted.

**Deliberately not in this Part**

- **A UI for pipeline dispatch.** ADR 0065 decision 8 makes the CLI the caller. A settings
  panel is a separate decision.
- **Attaching a Docket pipeline run to a Tack item.** ADR 0065 decision 5 is a boundary, not
  a first step; crossing it re-opens the ADR.
- **Deleting anything from the bridge.** ADR 0060 decided maintain. This Part does not
  re-litigate it, and VIII-C1 explicitly may not act on what it finds.
- **`github_actions.rs`.** The compile-only second adapter stays exactly as it is. Removing
  it needs its own decision record.

## §VIII.6 Handoff additions for this Part

Every handoff in `docs/agent-handoffs/part-viii/` adds, beyond the standard sections:

1. **What you read in `../rack-cli`**, if anything — file, what it said, and what you
   changed because of it.
2. **The revert proof**, where the card asks for one: the command, and the test that went red.
3. **The question you did not answer**, if §VIII.1 rule 1 stopped you.

---

# Part VII — Desktop app & background service (Phase 61)

**Status: ACTIVE — ADR 0062 accepted 2026-09-03; Wave 18 dispatched the same day (VII-A2
and VII-B1, Sonnet agents in worktrees off `2958e9e`). The Tauri Linux prerequisites are
installed and verified on the dispatch machine.** The runner and the board already survive a closed
browser tab; nothing survives a closed terminal. This Part makes Tack a background service
with a window on top of it — an application, like Docker Desktop — and gives the terminal
path the same daemon.

Dispatch plan: **`docs/agent-handoffs/part-vii/README.md`** — read its header and your
card's block only. It names what to read, how much it costs, the gate, and when to stop.

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 18 — The daemon, two ways | VII-A2 · VII-B1 | 61 | **Integrated 2026-09-04** — both cards on `develop`; handoffs `docs/agent-handoffs/part-vii/VII-A2.md`, `VII-B1.md`. Integration found the workspace membership itself was wrong: Tauri pulls GTK/WebKit/glib into whatever workspace holds it, so `cargo build --workspace` first demanded a staged sidecar and committed icons, then failed three CI jobs on `glib-2.0 not found` — the server would have needed desktop system libraries to compile. **`crates/tack-desktop` is now excluded from the root workspace and is its own**, with its own lockfile, CI job and Dependabot entry; `externalBin` moved to a bundle-only overlay and the icon set is committed, which also settles the `.gitignore` question `7cc6221` had routed to VII-C1. VII-B2 and VII-B3 work inside that workspace and must not touch the root lockfile. VII-B2 and VII-B3 are dispatchable in parallel from the integration tip |
| 19 — Lifecycle and data | VII-B2 · VII-B3 | 61 | **Integrated 2026-09-04** — both cards on `develop`; handoffs `docs/agent-handoffs/part-vii/VII-B2.md`, `VII-B3.md`. The two shared `main.rs`, and the damaging half of that merged without a conflict: B2 passed the tray's first-run marker a `root` local that B3 had deleted when it replaced the temporary data root, and git saw no overlap because the two edits sat far enough apart. Caught by compiling, not by reading the diff — resolved to B3's real `paths.root`. `capabilities/default.json` took the union of both permission sets; a missing entry there fails at runtime, not at build. **Left for VII-C1:** the tree now carries two independent first-run signals in the same directory, B2's `.autostart-initialized` marker and B3's `settings.json`. Both work and neither reads the other. |
| 20 — Ship | VII-C1 → VII-C2 | 61 | **Both integrated** — VII-C1 2026-09-05 at `03df038`, **VII-C2 2026-09-06** (handoff `docs/agent-handoffs/part-vii/VII-C2.md`). C2 puts the download first in the README with two screenshots of the shipped app, and states the tray's "closing the window does not stop it" contract where someone meets it. Integration checked both screenshots by opening them. C2's own correction to `crate-tour.md` — the tray makes no HTTP request — is what produced **VII-B4** |
| 22 — Corrections | VII-B4 | 61 | **Integrated 2026-09-06** (handoff `docs/agent-handoffs/part-vii/VII-B4.md`, with three screenshots of the running app beside it). The menu is a status line, not a switch — a switch needs write and reconciliation machinery a three-second poll does not, and a correct line beats a control that races the server. Six typed labels; a server that has not answered is never rendered as *off*, proven by killing the sidecar under a running tray. `make desktop-sidecar` now copies from the directory `cargo` wrote to. The integrator corrected `crate-tour.md` in the same change — VII-C2 owns it, is closed, and had just corrected it in the other direction. **VII-D1 is unblocked.** Original note follows: not started, **before D1** — the tray's agent-execution entry is a hard-coded "unknown" whose doc comment says `GET /api/local-runner` does not exist. It does (VI-B3), and the Agents page shipped (VI-C1), so the app's own menu now contradicts the README screenshot beside it. Carries `make desktop-sidecar`'s `CARGO_TARGET_DIR` bug, found the same way |
| 21 — Proof | VII-D1 | 61 | **Unblocked 2026-09-06 — VI-C9 landed and a dispatch from the dialog now reaches a real attempt.** Original note follows. **Waiting on Part VI's VI-C9.** Wave 22 landed, so the tray D1 walks is the one that ships, and VI-D2 has freed the display. But D1's transcript requires the stranger to *run an agent on an item*, and today no dispatch from that dialog can succeed — VI-D2 hit exactly that wall and had to go around the button. Dispatch D1 the day C9 lands, not before |
| 23 — Loose ends | VII-B5 | 61 | **Integrated 2026-09-07** (`5108233`, plus one integrator fix). A pure `watch_tick` — previous state, server kind, health answered, child exit → next state and at most one event — driven by the tray's existing three-second poll: a started sidecar's exit becomes `Server stopped (exit …)`, one dialog, and a `Stopped` mode so Quit no longer signals a dead pid; an attached server that misses five ticks gets `Server not responding` and one dialog, and recovers silently. Revert of the exit arm fails exactly its test; 34 + 3 desktop tests, clippy and fmt green. **The integrator found and fixed one defect the tests could not see:** the card kept the plugin's event receiver and drained it once per tick, but that channel has capacity 1 and the plugin's stdout/stderr readers block on every send, so a chatty server would have stalled on its own output within seconds of the window opening; the receiver is now drained continuously by a task that keeps only `Terminated`. Also folded in at the integrator's request: the `free_port()` bind-then-drop race in `refuses_to_spawn_when_the_port_is_held_by_something_else`, which failed VI-C43's CI run. Live behaviour `not_measured` — no GUI while the user is at the machine; the manual recipe is in the handoff. Dispatched 2026-09-07 from `8db262b` from the `develop` tip the prompt pins — VII-C3's recorded gap: a sidecar that dies mid-session is never detected or reported, and Quit would signal a dead pid. Unit-proven only; the live proof is `not_measured` until the user is away from the machine. Handoff `docs/agent-handoffs/part-vii/VII-B5.md` |

## §VII.0 Cold-start context capsule

**What this Part is for, in one sentence.** Tack must run as a background service and
install like an application — its own window, icon and tray — so that closing the window
never stops an agent attempt and nobody opens a terminal to start it.

**The decision of record is ADR 0062** (`docs/adr/0062-desktop-app-and-background-service.md`,
accepted 2026-09-03): eight decisions in one table at the top of the file. Read that table,
not a paraphrase. No card re-decides one; a card that finds a decision impossible stops and
says so in its handoff.

### Evidence base, measured 2026-09-03

| Fact | Value | How it was checked |
|---|---|---|
| Server + embedded-runner composition root | `crates/tack-cli/src/local_runner.rs` — `with_runner_enabled` (l.67), `ensure_loopback`; started by `tack serve --with-runner` | grep |
| The UI survives a dropped connection | `frontend/src/shared/realtime/boardSocket.ts` reconnects with capped exponential backoff and re-fetches | read |
| Server path defaults — all relative to the current directory | `sqlite:tack.db?mode=rwc`, `./storage`, `logs/`; runner state `.tack-runner` | `crates/tack-api/src/config.rs:229-239`, `crates/tack-runner/src/config.rs:9` |
| Config-file lookup | `tack.toml` in the current directory only; when it exists, `TACK_*` env is ignored | `config.rs:351`, the file's own header comment |
| Release targets already built | `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`; `tack` with `embed-spa`, `tack-runner` as its own archive | `.github/workflows/release.yml:112-170` |
| Tauri 2 pieces the app uses | sidecar: `bundle.externalBin` + `tauri-plugin-shell` (`app.shell().sidecar("tack")`), binaries named with the target triple suffix; tray: `tauri` feature `tray-icon`, `TrayIconBuilder` / `Menu` / `MenuItem`; plugins `tauri-plugin-autostart`, `tauri-plugin-single-instance`, `tauri-plugin-window-state`, `tauri-plugin-opener`, `tauri-plugin-dialog` | v2.tauri.app, fetched 2026-09-03 |
| Tauri tray on Linux | icon **click events are not emitted**; the menu works | v2.tauri.app/learn/system-tray |
| Linux build prerequisites | `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` | v2.tauri.app/start/prerequisites |
| On the dispatch machine | **absent:** `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `libxdo-dev`, `cargo tauri`; present: `librsvg2-dev`, `libssl-dev`, `build-essential`; rustc 1.96.0; node v22.17.1; host `x86_64-unknown-linux-gnu` | `dpkg-query -W`, `cargo tauri --version` |
| OS-directory crates in the lock file | none — `dirs` and `directories` absent | `Cargo.lock` |
| Where in-flight attempts are listed (for the Quit warning) | `GET /api/executions` — `crates/tack-api/src/handlers/executions.rs:807`; read the handler for its status filter | grep |
| Frontend API base | `VITE_API_URL ?? '/api'` — same-origin by default, so the window loads the served UI unchanged | `frontend/src/shared/api/client.ts:9` |

### The data folders (ADR 0062 decision 5), fixed here so A2 and B3 agree without sharing code

| OS | Root (`dirs::data_dir()` + `tack`) | Under it |
|---|---|---|
| Linux | `$XDG_DATA_HOME/tack`, default `~/.local/share/tack` | `tack.db` · `storage/` · `runner/` (the runner state dir) · `logs/tack.log` · the app's own `settings.json` |
| macOS | `~/Library/Application Support/tack` | same |
| Windows | `%APPDATA%\tack` | same |

Handed to the server as `TACK_DATABASE_URL=sqlite:<root>/tack.db?mode=rwc`,
`TACK_STORAGE_DIR=<root>/storage`, `TACK_RUNNER_STATE_DIR=<root>/runner`,
`TACK_LOG_FILE=<root>/logs/tack.log`. The root is created `0700` on Unix. The `tack`
binary's own defaults do not change. The folder name is lowercase `tack` on every OS —
pinned here; the ADR's prose used a capital on two of them and this table wins.

### Vocabulary

Default screens and the tray say *Tack*, *agent execution*, *launch at login*, *Quit*.
"Sidecar", "supervisor", "webview", "attach" live in code and the developer book. §VI.1
rule 8 applies as written.

---

## §VII.1 Rules for simultaneous agents

**All of §III.2, §V.1 and §VI.1 apply unchanged.** Six rules are specific to this Part:

1. **The server is never re-implemented or re-linked.** `tack-desktop` spawns the bundled
   `tack` binary. No crate under `crates/tack-desktop` depends on `tack-api`, `tack-db`,
   `tack-orch` or `tack-runner`; a test asserts the dependency list from `cargo metadata`.
2. **No webview or GTK dependency enters an existing crate — or the workspace holding
   them.** `cargo tree -p tack-cli -e normal | grep -ci "tauri\|webkit\|gtk"` is `0`
   before and after every card; the musl release job is untouched. **Amended 2026-09-04:**
   this rule was written about crates, and the first violation happened one level up —
   `tack-desktop` as a workspace member made `glib-2.0` a prerequisite for building the
   server, red in three CI jobs. `crates/tack-desktop` is now `exclude`d from the root
   workspace and is its own; nothing in it may be added back to the root `members`.
3. **The work outlives the window — proven, never claimed.** Every lifecycle claim is a
   measured sequence: start an attempt, close the window, observe `/api/health` and the
   attempt's state from a *second process* while the window is closed, reopen, observe the
   same state rendered.
4. **Never stop a server you did not start.** Attach mode drives a foreign Tack only
   through its API and never signals its process. Proven by starting `tack serve` by hand,
   launching the app, quitting the app, and asserting the hand-started server still answers.
5. **No build output in git.** Sidecar binaries, `target/` and Tauri's `gen/` are produced
   by the build and gitignored. Icons are the exception and are committed: one source SVG
   plus the sizes `cargo tauri icon` derives from it, because a bundle build needs them
   present and regenerating them on every machine buys nothing. VII-C1 owns them; before
   it lands, a placeholder source SVG is enough.
6. **Signing secrets do not exist here.** No card adds a certificate, key or notarization
   credential to CI or the tree. C1 ships unsigned and documents the one-time warning.

---

## §VII.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `crates/tack-cli/src/service.rs` (new), one `Commands::Service` arm in `crates/tack-cli/src/main.rs`, the `dirs` dependency line in `crates/tack-cli/Cargo.toml`, `docs/book/src/user-guide/cli.md` §"tack service", the user-service rows of `docs/DEPLOYMENT-GUIDE.md` | VII-A2 |
| `crates/tack-desktop/**` (new): `Cargo.toml`, `tauri.conf.json`, `build.rs`, `src/main.rs`, `src/supervisor.rs`, `icons/`, `binaries/.gitkeep`; the workspace `members` line in `Cargo.toml`; `.gitignore` lines for `crates/tack-desktop/binaries/*` and `target/`; `Makefile` target `desktop-sidecar` | VII-B1 |
| `crates/tack-desktop/src/tray.rs`, `src/lifecycle.rs`, plugin registration for autostart and single-instance in `main.rs`, the Quit dialog | VII-B2 — **after B1** |
| `crates/tack-desktop/src/paths.rs`, `src/first_run.rs`, `settings.json` handling, the version check in `supervisor.rs`, the `dirs` dependency line in `crates/tack-desktop/Cargo.toml` | VII-B3 — **after B1** |
| `.github/workflows/release.yml` (one new `desktop` job), `.github/workflows/ci.yml` (one `tack-desktop` check step), `crates/tack-desktop/icons/tack.svg`, the release-notes paragraph about unsigned builds | VII-C1 |
| `README.md` §"Run it" (that section only), the book's install page, `docs/book/src/developer/crate-tour.md` entry for `tack-desktop`, `docs/screenshots/desktop-window.png` + `desktop-tray.png`, `CHANGELOG.md` `[Unreleased]` desktop lines | VII-C2 — **`README.md` and `docs/screenshots/**` are shared with Parts V and VI; see §VII.3** |
| `crates/tack-desktop/src/{main,supervisor,first_run}.rs` | VII-C3 — **after C1 merges**. Disjoint from `tray.rs` and `lifecycle.rs`, which VII-B2 owns and this card must not change |
| `crates/tack-desktop/src/tray.rs`, the `desktop` and `desktop-sidecar` targets in `Makefile` | VII-B4 — **after VII-C2**, and **before VII-D1**. Must not touch `lifecycle.rs`, `main.rs` or `supervisor.rs` |
| `docs/agent-handoffs/part-vii/VII-D1.md` (the transcript), the per-platform `measured / not_measured` table in the book's install page | VII-D1 |
| `crates/tack-desktop/src/{supervisor,tray,main}.rs` (the watch, the poll loop, `DesktopState`), the tray-states paragraph in `docs/book/src/user-guide/quick-start.md` | VII-B5 — **after VII-D1**; not `lifecycle.rs`, `first_run.rs` or `paths.rs` |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |

---

## §VII.3 Dependency graph, cross-Part conflicts and merge policy

```text
ADR 0062 (accepted) ──┬── VII-A2 (tack service) ──────────────────────────────────────┐
                      └── VII-B1 (tack-desktop: sidecar + supervisor + window) ─┐      │
                                    ├── VII-B2 (tray, lifecycle, autostart) ────┤      │
                                    └── VII-B3 (data folders, first run, attach)┴── VII-C1 (release bundles) ── VII-C2 (README, install, screenshots) ── VII-D1 (stranger proof)
Part VI · VI-B3 (runner switch) ─── read by VII-B2's tray status; B2 does not wait for it
Part VI · VI-C1 (Agents page) ───── required before VII-C2 (the first-run screenshots show the real page)
```

**Wave 18 is two independent cards.** A2 touches only `tack-cli`; B1 creates a new crate.
**Wave 19 is two independent cards on B1's result** — B2 and B3 own disjoint files inside
the new crate. **C1 waits for both; C2 waits for C1 and Part VI's C1; D1 is last.**

### Cross-Part conflicts — read before branching

1. **`README.md`** — Part V (V-C3), Part VI (A3 landed; D2, D1 pending) and this Part
   (C2) write it. VII-C2 rewrites §"Run it" in place and nothing else; if VI-D2 is in
   flight, C2 waits for it; VI-D1 still takes the final merge. A card that finds another
   writer in flight escalates instead of racing — §V.3 and §VI.3 both say so.
2. **`docs/screenshots/**`** — V-C2's recording and VI-D2's three files keep their slots;
   VII-C2 adds two files and touches nothing else there.
3. **`crates/tack-cli/src/main.rs`** — VI-B1 adds `tack runner secret …`; VII-A2 adds
   `tack service …`. Disjoint arms; the integrator merges sequentially and builds once with
   both.
4. **`.github/workflows/release.yml`** — VII-C1 adds a job; no Part VI card touches it.
5. **The runner switch** — VII-B2's tray *shows* the state VI-B3 persists
   (`GET /api/local-runner`). Until VI-B3 lands, the tray shows *unknown* with the typed
   reason. B2 builds no second switch.

A card that discovers another collision states it in the handoff and stops.

---

## §VII.4 Cards

### VII-A2 — `tack service install | uninstall | status`

**Needs nothing on the machine.** Wave 18, parallel with B1.

**Owns:** `crates/tack-cli/src/service.rs` (new), one `Commands::Service` arm, the `dirs`
dependency in `crates/tack-cli/Cargo.toml`, `docs/book/src/user-guide/cli.md` §"tack
service", the user-service rows of `docs/DEPLOYMENT-GUIDE.md`, and the VII-A2 handoff.

**Context.** The terminal path's daemon (ADR 0062 decision 8). Linux: a systemd *user*
unit at `~/.config/systemd/user/tack.service` — `ExecStart=<absolute path of the running
binary> serve --with-runner`, `Environment=` the four folder variables from §VII.0,
`WorkingDirectory=<root>`, `Restart=on-failure`, `WantedBy=default.target`; `install`
writes it, then `systemctl --user daemon-reload && systemctl --user enable --now tack`.
macOS: `~/Library/LaunchAgents/com.yielab.tack.plist` with `RunAtLoad`, `KeepAlive`,
`EnvironmentVariables`, loaded with `launchctl bootstrap gui/$UID`. Windows: a typed
`service_unsupported_on_platform` error naming the desktop app. `status` prints the
unit's state and the health URL; `uninstall` disables, removes the unit, and leaves the
data root untouched. `--with-runner` in the unit makes the runner *available*; the switch
stays off (decision 6). The unit's working directory is the data root, which holds no
`tack.toml`, so a `tack.toml` in some other directory is not consulted — the doc says so.

**Acceptance:** on this machine, `tack service install` → `systemctl --user is-active tack`
prints `active`, `/api/health` answers; the shell that ran `install` is closed → still
active; `tack service uninstall` → unit file gone, no `tack serve` process, data root's
file count unchanged. The unit file and the plist are byte-asserted in unit tests (no live
systemd or launchd in CI). launchd's live proof is `not_measured` here and the handoff
says so. The Windows error is unit-tested. `cli.md` shows the three commands with real
output; `DEPLOYMENT-GUIDE.md`'s system-level unit is untouched and the new rows point at
this for the per-user case.

---

### VII-B1 — `crates/tack-desktop`: the app that supervises `tack`

**Needs the Tauri prerequisites on the machine** (§VII.0 evidence row). Stop before any
edit if `cargo tauri --version` fails or `pkg-config --exists webkit2gtk-4.1` fails, and
say so — nothing in this card can be proven without them.

**Owns:** everything under `crates/tack-desktop/` listed in §VII.2, the workspace
`members` line, the `.gitignore` lines, the `desktop-sidecar` Makefile target, and the
VII-B1 handoff.

**Context.** A Tauri 2 application with no frontend of its own: the main window's URL is
the local server (decision 4). Sidecar (decision 2): `bundle.externalBin` names
`binaries/tack`; `make desktop-sidecar` builds `tack` (`cargo build -p tack-cli --release
--features embed-spa`) and copies it to `crates/tack-desktop/binaries/tack-<host triple>`
(gitignored). Supervisor: on launch, `GET http://127.0.0.1:<port>/api/health`; if it
answers, **attach** (record it, never signal that process — §VII.1 rule 4); otherwise
spawn `tack serve --with-runner` through `app.shell().sidecar("tack")` with
`TACK_HOST=127.0.0.1`, `TACK_PORT`, and — until B3 lands — the four folder variables
pointed at a temporary root under the OS data dir; wait for health with a bounded, typed
timeout; open the window at the URL. On exit in *started* mode: terminate the child
(SIGTERM, bounded wait, then kill) and prove no orphan — a signal alone does not reap a
child, so the shutdown path must always finish with the reap. Single instance is **VII-B2's**
per §VII.2, not this card's. Port default `3210`;
a port held by something that is not Tack → native dialog naming the port, no retry loop.
In this card, closing the window quits (B2 makes it hide).

**Acceptance:** `cargo tauri build` on this machine produces a `.deb` and an `.AppImage`
(sizes recorded). Launching the AppImage shows the board in its own window; `pgrep -af
"tack serve"` shows exactly one child; `/api/health` answers from a second shell. Closing
the window terminates the child (assert no process, bounded). Attach: start `tack serve`
by hand, launch the app → attached, quit → the hand-started server still answers. §VII.1
rules 1–2 asserted by tests that read `cargo metadata` / `cargo tree`. Single instance is
not tested here — it is B2's. `cargo nextest run --manifest-path
crates/tack-desktop/Cargo.toml` runs the supervisor against a fake sidecar (a script answering `/api/health`)
so CI needs no webview.

---

### VII-B2 — Tray and lifecycle: close hides, Quit stops, launch at login

**Needs VII-B1.** Wave 19, parallel with B3.

**Owns:** `crates/tack-desktop/src/tray.rs`, `src/lifecycle.rs`, the autostart and
single-instance registration in `main.rs` (B1 deliberately left single instance to this
card), the Quit dialog, and the VII-B2 handoff.

**Context.** Decision 3. Tray menu: *Open Tack* · *Agent execution: on / off / unknown*
(read from `GET /api/local-runner` once VI-B3 exists; until then *unknown — the switch
arrives with the Agents page*, and B2 builds no second switch) · *Launch at login*
(checkbox, `tauri-plugin-autostart`, enabled on first run) · *Quit*. Window close →
`prevent_close` + hide; *Open Tack* → show + focus. *Quit* → list in-flight attempts
through `GET /api/executions` (read the handler for its filter); if any, a native dialog
"N agent attempts are running. Quit anyway?"; on confirm, stop the child as B1 does; in
attach mode, Quit closes the app and leaves the server alone. Linux: no icon-click
handler — the event is not emitted; every action is a menu item.

**Acceptance:** the daemon proof (§VII.1 rule 3), scripted: start a shim attempt, close
the window, from a second shell observe `/api/health` and the attempt advancing while the
window is closed, reopen from the tray → the same state rendered. Quit with an in-flight
attempt shows the dialog (screenshot in the handoff); Quit with none stops within a
bounded time and leaves no process. Launch-at-login on → the platform entry exists
(`~/.config/autostart/Tack.desktop` on Linux, capital T — the name comes from `productName` in `tauri.conf.json`; assert the file); off → it is gone. Single
instance still holds after B2's changes.

---

### VII-B3 — Data folders, first run, and the attach version check

**Needs VII-B1.** Wave 19, parallel with B2.

**Owns:** `crates/tack-desktop/src/paths.rs`, `src/first_run.rs`, `settings.json`
handling, the version check in `supervisor.rs`, the `dirs` dependency in the desktop
crate, and the VII-B3 handoff.

**Context.** Decision 5 and the §VII.0 folder table. `paths.rs` computes the root with
`dirs::data_dir()` + `tack`, creates it `0700` on Unix, and hands the four variables to
the sidecar (replacing B1's temporary root). The app's own `settings.json` in the root
holds a database-path override and the port — nothing else. First run: a native dialog
(`tauri-plugin-dialog`; no second frontend) that shows the data root and offers *Use an
existing tack.db…* through a file picker; afterwards, silent launches. Attach mode gains a
version check: read the server's version from the least invasive existing source (the
card measures what `GET /api/health` and `GET /api/openapi.json` carry today and chooses;
it adds no route) and refuse to attach to a server older than the bundled `tack
--version`, with a message naming both.

**Acceptance:** a fresh user on this machine → files appear exactly under the pinned
Linux path; the app's working directory gains no `tack.db` (assert); an override → the
server opens the chosen database (assert by item count through the API); version
mismatch → the typed refusal is shown, proven with a stubbed older version string in the
fake sidecar. macOS and Windows paths are unit-tested from the crate's own computation
and reported `not_measured` for a live run.

---

### VII-C1 — Release bundles for three operating systems, unsigned and said so

**Needs VII-B2 and VII-B3.** Wave 20.

**Owns:** the `desktop` job in `.github/workflows/release.yml`, one `tack-desktop` check
step in `ci.yml`, `crates/tack-desktop/icons/tack.svg` (source; generated sizes via
`cargo tauri icon`, committed as the CLI produces them — **VII-B1's `.gitignore` currently
excludes them under the old reading of rule 5; this card removes those two lines** and
deletes B1's `placeholder-source.svg`), the release-notes paragraph, and the VII-C1 handoff.

**Context.** Decision 7. Matrix: `ubuntu-22.04` → `.deb` + `.AppImage`; `macos-latest` →
`.dmg` for `aarch64-apple-darwin` and `x86_64-apple-darwin`; `windows-latest` → `.msi`.
Each job installs its prerequisites, builds the sidecar for the matrix target, runs
`cargo tauri build`, and uploads the bundles next to the existing archives. `ci.yml` runs
`cargo check -p tack-desktop` on Ubuntu with prerequisites and no bundle step. No signing
(§VII.1 rule 6); the release notes carry the two one-time warnings verbatim (macOS:
right-click → Open; Windows: SmartScreen → More info → Run anyway). The auto-updater
plugin is **not** wired: unsigned updates are worse than none.

**Acceptance:** one real workflow run — `workflow_dispatch` on the branch, or the cheapest
real trigger available, recorded — produces every artifact; the Linux artifacts are
downloaded and B1's launch acceptance is re-run against the AppImage; macOS and Windows
artifacts exist, are the sizes recorded, and are `not_measured` for launch. The musl
`tack` job's output is byte-identical to the previous release's build recipe (rule 2).

---

### VII-C2 — "Run it" says: it's an app; closing it keeps working; Quit stops it

**Needs VII-C1 and Part VI's VI-C1.** Wave 20, after C1.

**Owns:** `README.md` §"Run it" (that section only), the book's install page,
`docs/book/src/developer/crate-tour.md` entry for `tack-desktop`,
`docs/screenshots/desktop-window.png` and `desktop-tray.png`, the `[Unreleased]` desktop
lines in `CHANGELOG.md`, and the VII-C2 handoff.

**Context.** The app first (download → open → the board in its own window), the binary
second (servers, the terminal, `tack service install`), and the daemon promise in one
paragraph in the user's words. Two screenshots from the release build on this machine:
the window with the Agents page (VI-C1's, which is why this waits), and the tray menu.
Vocabulary check (§VI.1 rule 8) over everything written. README conflicts per §VII.3.

**Acceptance:** a stranger reading §"Run it" alone reports the three sentences in this
card's title without prompting (transcript). Screenshots are real, commands recorded,
alt text describes what is shown. `mdbook build docs/book` clean. `README.md` diff
touches §"Run it" only.

---

### VII-C3 — The app opens a window, or it says why it did not

**Needs VII-C1 merged** (there is a built artifact to launch). Wave 20. **Blocks VII-D1**,
which cannot walk a stranger through an app that shows nothing.

**Owns:** `crates/tack-desktop/src/{main,supervisor,first_run}.rs`, and the VII-C3 handoff.

**Context — two separate things, and only one of them is a mystery.** VII-C1 launched its
own built AppImage and the one CI produced. In both, the process starts and no window ever
appears.

The part that is not a mystery is in `main.rs`. The window is built only after
`attach_or_start` returns `Ok`, and of the error arms, two — a port held by something else,
an attached server older than the bundle — show a dialog before exiting. The third, the
catch-all, calls `handle.exit(1)` after a `tracing::error!` and nothing else. A GUI
application's log goes nowhere a user will look, so **every failure but those two is
indistinguishable from the app not starting at all**. That is a defect on its own terms,
independent of what triggered it here, and it is the reason nobody can say what triggered
it here.

The part that is a mystery is why the supervisor failed. Nothing was listening on the port,
so the app had to spawn its bundled sidecar. Whether the sidecar is missing from the bundle,
named something the launcher does not look for, not executable, or simply slower to answer
`/api/health` than the wait allows, is unmeasured — do not guess in the handoff.

**Tasks:** make every exit path say something a person sees, with the specific reason, not a
generic apology; find the real cause by running the built artifact with the app's own log
visible; fix it if it is in this card's files, and escalate with the measurement if it is
not. If the cause is a timeout, the fix is not a longer sleep — it is a bounded wait whose
expiry is reported as an expiry.

**Acceptance:** launching the built Linux artifact on a machine with no `tack` running opens
a window showing the board, stated with the artifact's name and where it came from. Every
`Err` arm of `attach_or_start`'s match is proven to render something visible, by forcing
each one — including the catch-all, proven by injecting a failure and watching the dialog,
then reverting. A sidecar that never answers is reported as that, with its own wait bound
named, not as silence. The tray still behaves as VII-B2 left it: closing hides, Quit stops.

**Stop if:** the cause turns out to be in the bundling rather than the app — the sidecar
absent from the artifact, or staged under a name Tauri does not resolve. That is VII-C1's
file, not this card's: record the exact path you expected and what the artifact actually
contains, and hand it over.

---

### VII-B4 — The tray says what is true, and `desktop-sidecar` honours `CARGO_TARGET_DIR`

**Needs nothing more than what is on `develop`.** Wave 22, **before VII-D1** — D1's
transcript walks this menu.

**Owns:** `crates/tack-desktop/src/tray.rs`, the `desktop` and `desktop-sidecar` targets in
`Makefile`, and the VII-B4 handoff. **Does not touch** `lifecycle.rs`, `main.rs`,
`supervisor.rs`, `paths.rs` or `first_run.rs`.

**Context, one.** `tray.rs` carries a hard-coded menu entry reading *"Agent execution:
unknown — the switch arrives with the Agents page"*, built `enabled: false`, above a doc
comment stating that `GET /api/local-runner` "does not exist yet". Both statements were true
when they were written and neither is true now: VI-B3 shipped the route, VI-C1 shipped the
Agents page, and VII-C2 put a screenshot of that page in the README — beside a tray
screenshot whose second line still says the feature is coming. The app contradicts its own
download page.

**Context, two.** `make desktop-sidecar` builds `tack` and then copies it from
`target/release/tack` literally, so with `CARGO_TARGET_DIR` set — which every agent working
this tree is told to set — the build succeeds and the copy fails, or worse, stages a stale
binary that is still there from an earlier run.

**Tasks:**
- Make the tray entry reflect the runner's real state, read from the route that now exists.
  Decide and record whether it is a live switch or a status line; a status line that is
  correct beats a switch that races the server. Whichever it is, the **failure** states —
  server not answering yet, request failed — are typed and shown, never rendered as "off".
- Fix the `Makefile` copy so it reads the same target directory `cargo` wrote to.

**Acceptance:** the tray's real text is proven by a screenshot of the running app with
execution on and a second with it off — not by reading the source. The unknown/unreachable
state is proven by launching with no server answering. `make desktop-sidecar` is run twice,
once with `CARGO_TARGET_DIR` set and once without, and the staged binary's `sha256` matches
the one `cargo` just built in both — paste both hashes. `make desktop` still builds.

**Stop if:** making the tray honest needs a change in `main.rs` or `lifecycle.rs` — those are
VII-B2's and VII-B3's files. Record what you needed and hand it over.

---

### VII-D1 — The stranger installs from the release page and never opens a terminal

**Needs everything.** Wave 21, last.

**Owns:** `docs/agent-handoffs/part-vii/VII-D1.md` (the transcript), the per-platform
`measured / not_measured` table on the install page.

**Context.** A clean Linux user account on this machine: download the AppImage from the
release page (or C1's artifact — say which), open it, first run, Agents page → turn
execution on, run an agent on an item, close the window, wait, reopen from the tray,
attempt finished with artifacts listed, Quit warns while something runs. Timestamps at
each step. macOS and Windows rows say `not_measured` unless a machine exists.

**Acceptance:** the transcript reproduces from the handoff alone; every claim in §VII.5
points at a line of it; nothing was typed in a terminal by the stranger after the
download.

---

### VII-B5 — The app notices when the server it shows has gone

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vii/VII-B5.md`, live proof `not_measured` (integrator amendment: the plugin's event channel is drained by a task, not per tick). **Needs nothing.** Wave 23 — reopens this Part for one card. Runs alongside Part VI's Wave 18.

**Owns:** `crates/tack-desktop/src/supervisor.rs` (a watch beside `attach_or_start`, and
`SidecarHandle` gains a non-blocking exit check fed by the plugin's `Terminated` event), the poll loop in `src/tray.rs`, the
`DesktopState` handling in `src/main.rs`, their tests, the tray-states paragraph in
`docs/book/src/user-guide/quick-start.md`, `CHANGELOG.md` `[Unreleased]` (one desktop line), and
the handoff. Not `lifecycle.rs`, `first_run.rs` or `paths.rs`.

**Context.** VII-C3's handoff: once the window is open, a sidecar that dies mid-session is not
detected or reported — `ServerMode::Started` is read again only at shutdown, nothing polls it
while running. The tray's three-second `GET /api/local-runner` poll (VII-B4) already renders a
"not answering" label when the server is gone, so the *attached* case has a symptom on screen
but no explanation and no exit status; the *started* case has a zombie child nobody reaps, a
window showing a dead page, and a Quit that will try to kill a pid that is already gone.

**Tasks.**
- `SidecarHandle` gains a non-blocking `fn exited(&mut self) -> Option<ExitReport>` (code or
  signal). `tauri_plugin_shell`'s `CommandChild` has no `try_wait`; its exit arrives as
  `CommandEvent::Terminated { code, signal }` on the receiver `spawn` returns — which
  `TauriLauncher::spawn` in `main.rs` currently drops as `_events`. Keep it, drain it on the
  plugin's async runtime, and surface the terminated payload through the handle. The
  `std::process::Child` test double answers from `try_wait`.
- A pure transition function in `supervisor.rs` — previous state, this tick's health answer,
  this tick's child status → what changed — so the tests never need a window. The tray's
  existing loop drives it; do not add a second timer.
- On a started server exiting: the tray's status line reads `Server stopped (exit <status>)`,
  `DesktopState` moves to a `Stopped` mode so the `ExitRequested` arm no longer signals a dead
  pid, and one dialog, shown once and never repeated, says the server stopped with its exit
  status and that reopening Tack starts it again. Quit stays available.
- On an attached server not answering for five consecutive ticks: the same single dialog,
  worded for a server this app did not start, and the status line says so; if it answers again
  later, the line recovers and the dialog is not shown a second time.
- Tests on the transition function: exit reported exactly once; five failed ticks, not four,
  trigger the attached dialog; recovery after failure; a started server that keeps answering
  changes nothing.

**Acceptance:** `cd crates/tack-desktop && cargo test` green, `cargo fmt --check` and `cargo
clippy --all-targets -- -D warnings` there green (CI's own three commands); a revert of the
exit arm fails exactly the exit test; the live behaviour is recorded as `not_measured` with
the reason (no GUI while the user is at the machine) and the manual recipe to measure it;
`cargo tree -p tack-cli -e normal | grep -ci "tauri\|webkit\|gtk"` → `0`; `.githooks/pre-push`
green.

**Stop if:** the plugin's event receiver cannot be kept alongside the child without a type
another card owns — record the version (2.3.6 today) and its API, and fall back to health
polling alone with the child's exit status `not_measured`.

---

## §VII.5 Definition of done, and deliberate exclusions

| Claim | Proof |
|---|---|
| Closing the window never stops an attempt; reopening shows its state | VII-B2 daemon proof, re-run by VII-D1 |
| Quit is the only stop, and it warns when something runs | VII-B2 |
| The app never re-implements or re-links the server; the musl `tack` job is untouched | VII-B1 dependency tests; VII-C1's unchanged musl job |
| A terminal user gets the same daemon without the app | VII-A2 live proof |
| Data lives in the OS folders; the working directory stays clean; an existing database can be chosen | VII-B3 |
| Unsigned bundles exist for Linux, macOS and Windows, and the one-time warning is documented where the download is | VII-C1 |
| The download page states each bundle's real size, including the AppImage's, rather than implying the server binary's | VII-C2 — measured 2026-09-03 on Linux: `.deb` 12.4 MB (uses the system webview), `.AppImage` 87.9 MB (carries its own). The product's "one small binary" line is about `tack`, and the app page must not blur the two |
| A stranger installs from the release page and reaches a finished attempt without a terminal | VII-D1 transcript |

**Deliberately not in this Part**, recorded so no card adopts them by drift:

- **Mobile or remote access** of any kind.
- **Code signing and notarization** — a separate decision with money attached.
- **A second frontend or an app-only API.** The window shows the served UI.
- **Turning the runner on by itself.** ADR 0061 decision 6's switch is the only way.
- **A Windows service.** `tack service` returns a typed unsupported there; the app is the
  Windows path.
- **Auto-update.** The plugin exists; it is not wired until signed builds exist.
- **Fixing tray-less Linux desktops.** Where no appindicator host exists the icon does not
  show; documented as a limitation, with the window still reachable from the launcher.

---

## §VII.6 Handoff additions for this Part

Use `docs/agent-handoffs/part-vii/TEMPLATE.md` — it points at Part VI's template and adds
three sections: **Platform measured** (OS, desktop environment, Wayland or X11 — tray and
autostart behave differently), **Daemon proof** (the exact sequence and observations of
§VII.1 rule 3), and **Process proof** (`pgrep -af tack` before and after each lifecycle
step: no orphan, no foreign server killed — rule 4).

---

# Part VI — Agent Onboarding & Provider UX (Phase 60)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Agent Onboarding & Provider
UX*, opened by the **agent-UX audit of 2026-09-03**. This Part has its own numbering
namespace (`§VI.0` … `§VI.6`) so the archive's load-bearing numbers stay put.

Like Parts III–V, this board is written to be picked up cold by parallel agents in isolated
worktrees. Every card is bounded, names every shared-file owner, and has an acceptance gate
verifiable without trusting its author's handoff.

**Dispatch plan:** [`docs/agent-handoffs/part-vi/README.md`](docs/agent-handoffs/part-vi/README.md)
— per card, the exact read list with measured sizes, what not to read, the gate, the stop
conditions, and the dispatch prompt; per wave, the integrator's adversarial checklist. A
card agent reads that file's header and its own block, then this board by anchor (§VI.0–§VI.3
and its card, ~7k tokens), and nothing else without recording why. Handoffs are written
from [`TEMPLATE.md`](docs/agent-handoffs/part-vi/TEMPLATE.md) there.

**This Part adds product features.** Part V §V.5 deferred every product feature to "a
post-adoption cycle"; this is that cycle, **for the agent surface only**. Notifications,
i18n, time tracking and in-UI diff review stay deferred — see §VI.5.

## Status board — Part VI

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 14 — Truth first | VI-A1 · VI-A2 · VI-A3 | 60 | **Integrated** at `927f850` on `develop` (handoffs: `docs/agent-handoffs/part-vi/VI-A1.md`, `VI-A2.md`, `VI-A3.md`). All three adversarially verified against the real tree, not just their own reports — A1's live worked example re-checked, A3's stranger-read test and render proofs opened, A2's ADR cited by line against 0050/0058. `mdbook build` clean; the only `docs/CONFIG.md` conflict (A1's new bullet vs. A2's rewritten paragraph, both anticipated it) resolved keeping both contributions. **ADR 0061 is `Status: proposed` — Wave 15 (VI-B1/B2/B3) does not branch until the user records acceptance with a date in `VI-A2.md`.** One non-blocking finding routed to VI-D1: `docs/book/src/roadmap.md:3273` wants a forward reference to ADR 0061 once accepted (not fixed here — outside every Wave-14 card's ownership). |
| 15 — Provider at the runner boundary | VI-B1 · VI-B2 · VI-B3 | 60 | **VI-B1 and VI-B2 integrated 2026-09-04** (handoffs `docs/agent-handoffs/part-vi/VI-B1.md`, `VI-B2.md`). **VI-B3 and VI-B4 integrated 2026-09-05** at `03df038` (handoffs `VI-B3.md`, `VI-B4.md`). Both were green alone and neither built after the merge: B4 made the discovery report's catalog one status per provider and B3 called it expecting a single one. Reconciled at integration, which is the first moment either card could have seen the other. Two comments the merge falsified were repointed, one caught by `check-comments.sh` rather than by reading. **VI-B4 corrected its own central claim under challenge:** its first vendor-name grep covered only the dispatch functions and reported zero; run over the whole tree with each file split at its own `#[cfg(test)]`, two production sites still named Vercel, one of which would have recorded an unobserved model as harness-reported for the next gateway added. VI-B5 follows B4 alone, because it changes the contract B4's acceptance holds byte-identical. **opencode was removed from the tree the same day** (ADR 0063 decision 8, handoff `docs/agent-handoffs/part-vi/opencode-removal.md`) — Tack ships adapters for two harnesses and is not limited to two: an unknown harness name still parses and is refused at claim with the same typed reason any undeclared one gets. **VI-B2 landed narrower and better-shaped than its card:** opencode's gateway path was measured working but deliberately not built, because it is the only harness that could not be credentialed by per-spawn injection — it needed a written config file. That card also claimed the provider machinery carries no vendor name and that a second endpoint is three data rows and no code path. **Measured 2026-09-05, it is not:** `attach_catalog` looks up the Vercel config key by name rather than walking the configured providers, and the three rows only suffice for a provider whose catalog is a bearer-authenticated `data[].id` list. VI-B4 makes the claim true. Two card errors corrected by measurement: the default secret name had to be `vercel-ai-gateway/default`, since `SecretStore::resolve` never appends the label, and the card's vendor URL for codex 404s. **Escalated, now carded:** the catalog publishes per-model pricing, context windows and modalities, and `ModelCombination` has nowhere to put them — a reviewed wire field, not the `additional` map. ADR 0063 decision 5 records it; **VI-B5 owns it**. ADR 0061 accepted by the user on 2026-09-03 (recorded in `docs/agent-handoffs/part-vi/VI-A2.md`, amendments). Sequential B1 → B2 → B3; base `2958e9e`. Decision 1 of ADR 0061 was refined on 2026-09-03 before acceptance (platform keychain first, owner-only file where none answers, backend reported); VI-B1's card and dispatch block already match. |
| 16 — UI-first flow | VI-C1 · VI-C2 · VI-C3 · VI-C4 | 60 | **Wave complete — VI-C1 integrated 2026-09-05** (handoff `VI-C1.md`): the Agents page, the sidebar entry, the Board's first-run banner, and the Fleet page's runner management moved under *Advanced*. Its stated blocker was wrong and its own amendment says so — the runner-v1 completion route is at `handlers/runner_protocol.rs:198`, in `tack-api` and not in either adapter; the eight live 404s came from guessed names and from percent-encoding that never delivered the real path. The real residual limit is narrower: a genuine harness subprocess reporting its own completion needs that adapter's argv and output shape. **VI-C2, VI-C3 and VI-C4 integrated 2026-09-04** (handoffs `docs/agent-handoffs/part-vi/VI-C2.md`, `VI-C3.md`, `VI-C4.md`). C2 merged with no conflict — it is frontend-only and shared no file with the desktop cards it integrated alongside. C1 still needs B2 + B3. VI-C4's two routes ship with no handler test in `tack-api` — the next Wave 16 card touching that crate owns adding them (see its amendment). |
| 17 — Proof | VI-C5 … VI-C23 · VI-D1 · VI-D2 | 60 | **VI-C17 integrated 2026-09-06 — the badges finally ask the question they mean.** `GET /api/executions` takes `item_ids` and returns exactly one row per id, its latest request, picked by a window function with an `id DESC` tiebreak because two requests for one item can share a `created_at`. Badges register interest on mount and every registration in one microtask coalesces into a single request: **one round trip per screen either way, but a row per card instead of up to 2000** — smaller than even the old 200-row default. Absence from the response is now conclusive, so VI-C14's `Unknown` chip and the hand-copied row cap are both **deleted rather than bound** — with badges asking by exact id there was nothing left for that constant to guard, which is the outcome VI-C17's own card said to check for first. The card **checked rather than assumed** the thing VI-C14's pricing had gotten wrong: this `serde_urlencoded` cannot deserialize a repeated query key into a sequence, verified in a scratch crate, so ids go comma-separated. The integrator verified the 2000-id cap against SQLite's own bind-variable limit — the bundled build is 3.46.0 with `SQLITE_MAX_VARIABLE_NUMBER 32766`, a 16× margin — and fixed one defect: the coalesced fetch was launched with `void` and had no `catch`, so a failed badge poll became an unhandled rejection. **Its 'pre-existing flake' claim was true and was checked rather than accepted** — on `develop` without this card, `two_servers_on_two_databases` failed 2 of 5 runs, and it still fails at `-j 1`, so it is machine load, not test parallelism. That test arrived with VI-C15 and its 10s deadline was set on an idle machine; `bootstrap_entrypoint`'s 5s shutdown budget took 110s under the same load. Both are now **VI-C23** — the same unreadable-signal disease V-C4 found in CI, from the other end. Earlier notes follow. | **VI-C18 and VI-C19 integrated 2026-09-06, and the E2E suite is green — 70/70, three consecutive full chromium runs from a clean state, for the first time this cycle.** C18 answered its own question the right way round: **the gate was never wrong, the test was.** It left the model selector on its silent default and assumed `getOrCreateProject` returns a stable project; `list_projects` orders `updated_at DESC` and the helper takes `[0]`, so under `fullyParallel` the shared project is whichever one any concurrent test last touched. It proved the leak instead of inferring it — the failing dialog showed a default model minted only by `scheduler-e2e.spec.ts`'s own fixture. The integrator's revert reproduced the failure in 4 of 5 runs. The helper itself is **VI-C20**. C19: `remove_secret` now reverses exactly the flip `set_secret` makes and nothing else, so a provider the operator enabled in config survives a key removal; the revert failed exactly one test, as claimed, and VI-C16's E2E workaround is gone. **What chasing the last failure found is worth more than either card.** The suite's outcome tracks how much state its persistent database has accumulated — **9 failures at 383 runners, 1 at 42, 0 from clean**, different tests at each level, all passing alone. That is why this cycle produced four irreconcilable flake rates for the same tests; none were wrong, none recorded the starting state. Now **VI-C22**. And deleting `e2e.db` without `storage-e2e/` leaves a runner credential for an id the new database has never seen — the failure class VI-C15 was carded for, on the axis its `storage_dir` scoping does not cover — now **VI-C21**. Earlier notes follow. | **VI-C10 and VI-C16 integrated 2026-09-06.** C10: `check-comments.sh` only ever scanned `crates/**/*.rs`, so the rule it enforces had never applied to the frontend — 219 citation lines across 80 files (the card re-measured; the board said 221, and it said so). **Four reached rendered operator text, three of them asserting gaps that had since been filled** — `GET /runners`, the fleet-member routes, and the precedence walk — so the UI was telling people that shipped features were missing. The integrator verified all three routes exist before accepting the rewrites. A callerless provenance helper whose only output was one of those strings is deleted, not reworded. The gate now understands JSX and block comments and single quotes, and flags a bare `docs/agent-handoffs` path. **The integrator found one more hole and fixed it: the card-id pattern was `[0-9]` , so the word boundary fell between the digits of every two-digit id and VI-C10 through VI-C19 passed straight through** — proven with a probe file, `VI-C9` caught and `VI-C10` not, then fixed and the whole tree still clean. **Its first catch after merging was VI-C14**, which landed minutes earlier and had never been scanned. C16: a lock directory keyed by API origin, so the three specs sharing one server-wide switch stop racing; it also measured a real ambiguity in `ProviderKeyPanel` (`stored()` is falsy while loading *and* when empty — 4 of 10 solo runs failed on the unmodified file) and escalated two things it could not own. **Both escalations were real and the integrator acted on both**: `agent-assets.spec.ts` is a recorder needing a release build, was never added to `playwright.config.ts`'s recorder exclusion list, and was failing every capability-reading spec alongside it; and `remove_secret` never undoes the `enabled` flag `set_secret` sets, now **VI-C19**. Full chromium suite, measured: **5 of 5 runs failed before, 7 of 7 fail exactly once after**, always `run-with-agent.spec.ts:98`, which passes alone — deterministic, reproduces at the pre-merge tip, and is now **VI-C18**. Earlier notes follow. | **VI-C14 integrated 2026-09-06.** The badges' shared preload now asks for the server's hard cap (2000) instead of taking its 200-row default, and an item absent from a response that came back *at* the cap renders `Unknown` rather than nothing — the two cases are indistinguishable from the client and silence claims the wrong one. One request for a screen of items, before and after. The card priced the real fix and stopped where it was told to: no route returns the latest execution for a set of item ids. The integrator's own revert failed **five** tests, not the four the handoff counted — an undercount in the safe direction, the unmentioned one being the `Unknown` chip itself. Two things the card did not measure, measured at integration: the cap raise costs **≈343 KiB more per board load** on an install big enough to fill it (195 B × 1800 rows, `ExecutionSummary` has five fields), and the `Unknown` chip lands on *every* uncached item once an install passes 2000 requests, including items that never ran — correct, since the client cannot tell those apart, but a visible change to a full board. Its escalation plus a duplicated constant became **VI-C17**. Earlier notes follow. | **VI-C15 integrated 2026-09-06.** The embedded runner's default state directory now derives from `storage_dir` instead of the process's working directory, so two `tack serve --with-runner` processes started from one shell against two databases no longer resolve to one credential. `TACK_RUNNER_STATE_DIR` still wins when set. State written under the old default is moved once, only when the new directory does not exist, and never reused when the rename fails — the card argued that choice against a silent migration and an unconditional refusal, and recorded the case it does not solve (several installs already sharing one legacy directory: the first to boot keeps it, the rest correctly self-provision). The integrator re-ran the revert proof and it failed as claimed, at `embedded_runner_state_scoping.rs:166` — but **at server A, not server B**, one assertion before where the test's own doc comment said it would, and server B's `expect` narrated a resume-then-refuse sequence the revert never reaches. Both texts were corrected to what reverting actually produces before the merge. Earlier notes follow. | **VI-C13 integrated 2026-09-06.** `a11y.spec.ts` had been failing since the modal redesign, the same disease `scheduler-e2e.spec.ts` had. The `agents-page.spec.ts` fix is the one worth reading: the test used to **revoke every other active runner in the database** for determinism — measured unsafe and about half effective — and three further approaches were measured and rejected before one that touches no other file's state. Three full parallel runs, clean. Its amendment corrects its own description of the auto-selection rule after review. Two escalations became **VI-C15** (the embedded runner's state directory does not follow the database — a credential outliving the database it was made against) and **VI-C16** (three specs driving one server-wide switch). Earlier notes follow. | **VI-C11 and VI-C12 integrated 2026-09-06.** C11: `docs/contracts/model-policy/precedence-table.json`, generated from the Rust walk and read by both sides. The integrator proved both directions personally — swap two entries in `ModelPolicyTier::ORDER` and leave the fixture, the Rust gate fails; regenerate it, and **twelve TypeScript cases fail with no TypeScript touched**. C12: the executions list takes an `item_id` and a bound (200, capped 2000, applied unscoped too); dropping the `WHERE` fails exactly the one test, naming the leaked row. Its bound made a pre-existing badge-staleness question reachable, now **VI-C14**. Earlier notes follow. | **VI-C9 integrated 2026-09-06** — the dialog can dispatch again, in both directions. The gate mirrors the scheduler's `declared \|\| passthrough` arm, and Auto is resolved rather than guessed, so a request that would queue forever is refused with the consequence named and a link to the fix. It also found the gate being fed every runner's capabilities instead of the target's. `scheduler-e2e.spec.ts` had been failing since VI-C2's picker redesign; its mechanics are updated and **its assertions are untouched** — the integrator checked the whole spec diff and found exactly one changed assertion line, an addition. **VI-C10 and VII-D1 are unblocked.** Three findings became **VI-C11** (nothing binds the client's copy of the precedence walk to the Rust one, and the dialog now blocks on it), **VI-C12** (`GET /api/executions` returns every row ever, and the browser filters) and **VI-C13** (two more specs the same redesign stranded). Earlier notes follow. | **VI-D2 integrated 2026-09-06** (handoff `VI-D2.md`, two dated amendments). It shipped `agents.png` and `attempt.png` and **deliberately did not replace `hero.gif`**: the recording it made showed the dispatch modal with a live *Unsupported* badge three seconds before a run that had been dispatched around the button with `POST /executions`. The recorder is guarded so it cannot regenerate silently and the re-recording recipe sits beside the guard. `two-machines.png` is produced and not shipped — every capability row reads *not supported*, and the enrollment panel prints a handoff path above the list, which is **VI-C10**. **VI-C9 is now the critical path: VI-C10 and VII-D1 both wait behind it**, D1 because a stranger's transcript has to dispatch an agent on an item and today cannot. Earlier notes follow. | **VI-C8 integrated 2026-09-06** (handoff `VI-C8.md`) — the attempt events route and the artifact download now have the scoping tests nothing was giving them. The download's proof is the one that matters and the integrator re-ran it alone: revert that single site's `request_id` predicate and exactly its test fails, carrying the other execution's actual file bytes back to a caller who asked under a different execution id. Earlier notes follow. | **VI-C6 and VI-C7 integrated 2026-09-06.** C6: both attempt-list routes answer `404` for an unknown attempt *and* for one belonging to another execution, and the integrator's independent revert showed the cross-execution test failing with the other execution's artifact row in the body — it catches a leak, not a status code. That revert also showed **two more sites with the same scoping and no test at all**, now **VI-C8**; the artifact *download* is one of them. C7: an `AutoSelect` request is rejected by the scheduler unconditionally and by design, so the dialog's own default queues forever on any install with no explicit default model at any tier — **VI-C9** carries the fix and waits for VI-D2, which is filming that dialog. C7's first verdict overstated the scope and its amendment says so; it also retracts its own proposed fix. Wave 17 original note follows. |
| 17 (original note) | — | 60 | **VI-C5 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-vi/VI-C5.md`) — six handler tests, each proven load-bearing; the integrator re-ran the ordering proof independently by flipping `repo/execution.rs`'s `ORDER BY` to `DESC` and watching exactly the two ordering tests fail. It declared one gap, now carded as **VI-C6**: neither route's unknown-attempt path is covered. **VI-C7** was added from V-C2's surviving escalation. Original note follows. **VI-C5 dispatched 2026-09-05** — it needs nothing and shares no file with the other two, so it runs now rather than waiting for them. D2 (assets) needs C1, C2 and Part V's V-C2, which owns `docs/screenshots/`; D1 goes last and needs everything |
| 18 — Loose ends | VI-C41 · VI-C42 · VI-C43 | 60 | **All three integrated 2026-09-07** (`2d05762`, `16d6b0e`, `f47b566`, then one integrator fixup `74b01c4`). C43: the roster is derived from `GET /runners`' `fleet_ids`, every write refetches and renders the server's roster, `already_member` is a notice; 856/856 unit, 43/43 on the agents-page and a11y specs. Its CI run failed only in the desktop job, on a pre-existing port-race flake in `supervisor.rs` (`free_port()` bind-then-drop, re-bound a moment later) that VII-B5 fixes in the same file. C41: one pure locator, `PATH` then a fixed list (`~/.local/bin`, `~/.cargo/bin`, `~/.bun/bin`, `~/.npm-global/bin`, `~/.npm/bin`, every nvm version's `bin`, Homebrew's two prefixes; `%APPDATA%\npm` and `%LOCALAPPDATA%\nvm` on Windows, uncompiled here), the error names every directory searched; 1468/1468, contract byte-identical. The integrator changed one behaviour: an empty `PATH` entry is skipped rather than searched as the working directory. C42: the platform-store attempt runs on its own thread with a 3 s `recv_timeout`; a hung probe leaves a detached thread and the file backend is chosen with the reason in the existing `warn`; proven load-bearing by reverting the bound (the test then runs the fake 5 s hang to completion and fails). The real D-Bus stall was not reproduced on demand, only its shape. Dispatched 2026-09-07 from `8db262b` from the `develop` tip the prompts pin — three uncarded findings the last three stranger walks and handoffs left on the board: harness discovery reads only the launcher's `PATH` (VII-D1's reason for a fake shim), the platform secret store can block the embedded runner's boot forever with no log line (VI-C32's known limitation), and the fleet-member routes have no caller (VI-D1's escalation). Two at a time under the machine's load cap; no card shares a file with another, and no card changes a type another calls. Handoffs `docs/agent-handoffs/part-vi/VI-C41.md`, `VI-C42.md`, `VI-C43.md` |

**Integration line:** `develop`, the repository's default branch — same as Parts III–V.
Branch every card from `develop`. Do not create a `plan/*` line.

---

## §VI.0 Cold-start context capsule

**What this Part is for, in one sentence.** The agent-execution pipeline works end to end and
is unusable by anyone who did not build it, because the steps to reach a completed attempt
are split across three surfaces — a harness's own vendor login, the `tack` CLI, and the web
UI — with no screen or page that shows the whole path, and because the one question every
user asks first, *"which model, from which provider, and where do I put the key?"*, is
answered today by a negation.

**Read the posture correctly.** ADR 0050 ("the Tack API never starts a coding harness and
never becomes a model proxy") and ADR 0058 ("vendor credentials remain outside Tack") are
**right and stay**. They are statements about the API server. They have been read — by the
docs that cite them and by users — as "Tack cannot help you configure a provider", which is
a different claim and a false one. The runner already owns credentials (its own), already
owns the harness subprocess and its environment, and already accepts `secret_reference`
environment entries it cannot resolve. This Part moves the *guidance* and the *provider
configuration* to where the posture already puts them — the runner — and puts one screen
in front of both.

### Evidence base, measured 2026-09-03

Do not re-derive these; act on them. Each row names how it was checked so any card can
re-check one cheaply.

| Fact | Value | How it was checked |
|---|---|---|
| Worked example of creating an execution anywhere in `docs/` | **none** — one table row, `docs/MCP.md:99` | `grep -rn "execution create\|agent_profile_snapshot" docs/ --include='*.md' \| grep -v agent-handoffs` |
| `docs/API-REFERENCE.md` on the execution surface | delegates to `agent-runners.md` (§"Runner, Fleet & Execution", ~l.1273); that page has no example either | read both |
| Required fields on `POST /api/executions` | **13** (`agent_profile_snapshot`, `repository_snapshot`, `permission_policy`, `budgets`, `environment`, `metadata`, `timeout_seconds`, …) | `python3 -c` over `docs/openapi.json` → `CreateExecution.required` |
| Quick Start / Working with Items mention agents | **0 lines** | `grep -in "agent\|runner\|harness\|model" docs/book/src/user-guide/{quick-start,items}.md` |
| CLI reference documents `tack execution` / `runner` / `fleet` / `agent-profile` / `model-profile` | **no** — all exist, `crates/tack-cli/src/main.rs:252-278` | `grep -n "^## " docs/book/src/user-guide/cli.md` |
| `tack runner doctor` in the book | **no** — only `docs/CONFIG.md:91` (which says "run it yourself") and the roadmap | `grep -rn "runner doctor" docs/` |
| Book configuration page, runner/model rows | **only `TACK_ORCH_ENABLE`** (`configuration.md:22`); `docs/CONFIG.md` is the real reference and is not in the book | grep |
| Harness id in `agent-runners.md:32` | `claude_code` — **wrong**; wire id is `claude-code` | `crates/tack-runner/src/harness/claude_code.rs:105`, `frontend/src/shared/runWithAgent/shared.ts:52` |
| `agent-runners.md:378-381` "`model_profiles` … no runtime effect" | **false as written** — the modal copies the chosen profile into `requested_model_provider`/`requested_model_id`, the highest-precedence tier | `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:178-183` |
| `agent-runners.md` Known gaps: "`agent_fleet_members` has no write route" | **false** — `POST /api/runner-fleets/{fleet_id}/members` and `…/members/{runner_id}` exist; the UI still says nothing calls it | `docs/openapi.json` paths; `frontend/src/features/fleet/runnerFleet/FleetsPanel.tsx:117` |
| Model precedence documented for users | **nowhere** — only a Rust module doc | `crates/tack-orch/src/model_policy/mod.rs:1-3`, `wiring.rs:1-25`; `roadmap.md:2868` records it as intent |
| Project default-model tier | **no storage; always `None`** | `crates/tack-orch/src/model_policy/wiring.rs:17-23` |
| Agent-profile default model | a JSON convention `{"default_model": …}` inside `agent_profiles.limits`; **no UI field** | `wiring.rs:9-14`; `AgentProfilesPanel.tsx` |
| Hand-typed fields per run in the modal | **five** — runner id (free text), remote, base revision, subdirectory, timeout; **no memory** between runs | `RunWithAgentModal.tsx:298,386,393,399,412`; `rg localStorage frontend/src/shared/runWithAgent` → 0 |
| Project-level repository storage | **none** — only per-item `github_links` (migration 018) | `crates/tack-db/src/migrations.rs` |
| `secret_reference` environment entries | accepted by the contract, **never resolved** — "no secret-store client exists in tack-runner yet" | `claude_code.rs:62-64`, `codex.rs:772-775`, `opencode.rs:1156-1158` |
| Decision / artifact list routes | **none**; the UI takes a typed id | `agent-runners.md` Known gaps; `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx` |
| Harness auth state observable by Tack | **no** — `doctor` prints where a credential *lives*, not whether one exists | `crates/tack-cli/src/doctor.rs:223-244` |
| The model choice reaches the harness | yes — `--model` on all three adapters | `codex.rs:753`, `claude_code.rs:968`, `opencode.rs:1135` |
| Last migration | `061_execution_attempt_start_facts` → this Part's is **062** | `grep -n '"0[0-9][0-9]_' crates/tack-db/src/migrations.rs \| tail -1` |
| Vercel AI Gateway, per-harness endpoints | documented by the vendor for all three harnesses in this tree (table in VI-B2) | `vercel.com/docs/ai-gateway/coding-agents`, fetched 2026-09-03 — **re-fetch before relying on it** |
| README hero asset | alt text *"Board, Timeline, and vocabulary editor"* — a PM view | `README.md:12` |
| README screenshots that show an agent doing anything | **zero of five** (board, timeline, dashboard, list, vocabulary editor) | `ls docs/screenshots/`; `README.md:61-76` |
| README section order | *Features* lists project management first (`:32`) and agent execution second (`:42`); *Architecture* — where the two components are explained — is at `:169`, after *Status* | `grep -n "^#" README.md` |
| Book introduction's core concepts | item, workflow, project type, vocabulary — neither *runner* nor *run* | `docs/book/src/introduction.md` |

**The single most expensive line in that table is the provider one.** `docs/CONFIG.md:102-104`
says *"there is no `TACK_*` variable for a model provider or endpoint, by design"*. True for
credentials, and read by every user as "I cannot choose a model" — while the request body,
the CLI (`--model-provider`/`--model-id`) and the modal all route the choice. The two facts
never appear on the same page. Fixing that sentence is VI-A2; fixing the page is VI-A1.

### The surface map — the design authority for Waves 15 and 16

Every step between "installed the binary" and "an item completed as an attempt", where it
happens today, where it must happen after this Part, and — when the answer is not "the UI"
— the structural reason, so nobody re-litigates it per card.

| Step | Today | Target | Why not fully UI |
|---|---|---|---|
| Turn on agent execution | console: `tack serve --with-runner` | **UI** — one switch, on a loopback bind (ADR 0061 decision 6); the command is rendered only where the switch cannot exist (a non-loopback bind) | — starting Tack itself is the one console step left |
| Install a harness binary | outside Tack, undocumented | console, rendered in the UI: installed / absent per harness, with the vendor's install command | external binaries |
| Authenticate a harness with its own vendor login | console, outside Tack | console, rendered in the UI with the exact command and a re-check | OAuth device flows need a TTY; Tack never holds vendor logins (ADR 0050/0058) |
| Authenticate through **Vercel AI Gateway** | impossible | **UI** with the embedded runner — one key, pasted once, write-only, runner-local; console (`tack runner secret set`) for a remote runner, rendered in the UI | none in the embedded case |
| Enroll a runner | zero-touch (embedded) / UI token → console (remote) | unchanged | — |
| Create a fleet / add a member | fleet: UI; member: API only | UI | — |
| Create an agent profile | UI | UI, with a default created on first use | — |
| Choose a default model | impossible (no storage) | UI: a project setting, chosen from a **measured** catalog | — |
| Run an item | UI modal, five hand-typed fields | UI, defaults from project settings, **zero hand-typed identifiers** | — |
| See what an attempt produced / answer a decision | UI, but only with a manually-entered id | UI lists | — |

The last column is closed. A card that finds a step it cannot move to the UI for a reason
not in this table escalates in its handoff; it does not add a row.

### Why one provider, and why this one

Rule 3 of `.claude/scope-discipline.md` — build for the second case, not the fifth. The
harnesses' own logins are the first case and already work. Vercel AI Gateway is the second
because it is the only provider that is (a) a single API key rather than a vendor OAuth
flow, (b) documented by its vendor with a dedicated endpoint for **each** of the three
harnesses in this tree, and (c) a catalog endpoint, so the model picker can show what a
runner *measured* rather than a list someone typed. That combination is the only path to
"a UI-only user authenticates without a console". OpenRouter, direct Anthropic/OpenAI keys
and local endpoints stay where they are — the harness's own configuration — until the
gateway path has been proven live and a second gateway actually differs from it.

### The two-component story — the statement every page reuses

Written once here; VI-A3 applies it verbatim, and every page that introduces agents
(README, `introduction.md`, `agent-runners.md`, `CLAUDE.md`'s overview) opens with it or
links to it. Three pages currently say three different things; this is the one thing.

> **Tack is two components, built to be one product.**
>
> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, checks out
> an isolated workspace, launches the coding agent you already use — Claude Code or Codex
> — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

Two consequences, so no card re-derives them:

- **The runner is named in the story, never on a default screen.** The README and the docs
  explain two components because that *is* the product. The UI's default screens say
  *agent*, *model*, *provider*, *run* — "runner", "fleet", "enroll", "heartbeat",
  "capacity", "lease", "fencing", "harness" appear only under *Advanced* and in the
  developer book (§VI.1 rule 8).
- **The recovery demo is the visual proof of the third paragraph.** V-C2's recording
  (kill the runner mid-run → `needs_operator` → no duplicate → requeue → success) is not a
  competing hero asset; it is the picture of "they fail differently", and the README gives
  it a named slot next to that paragraph (VI-A3).

### Working-tree state at the time this board was written (2026-09-03)

`develop` is at `e5206c7` and the tree is clean. Part IV is done (Wave 10 at `83fefab`).
Part V Wave 13 is in flight: **V-C2** (the demo; owns `docs/screenshots/**`) is unblocked and
**V-C3** (launch preparation) waits on it. Both may touch `README.md`. See §VI.3.

---

## §VI.1 Rules for simultaneous agents

**All fourteen rules of §III.2 and all five of §V.1 apply unchanged** — in particular §V.1
rule 1 (no claim ships that a clean machine cannot demonstrate) and rule 4 (measure, never
estimate). Seven rules are specific to this Part:

1. **Every console step stays inside the flow.** If a step cannot be done from the UI, the
   UI renders it as a numbered, copyable command, says in one line *why* it is a console
   step, and verifies it afterwards with a real observation. A link to a documentation page
   is not a step. The strings the UI shows are the strings the docs show.
2. **Green means observed.** "Installed" means the probe ran the binary. "Authenticated"
   means a real attempt, or a real catalog call, succeeded with that credential. A session
   file that exists is "present, unverified" — the honest state — never a check mark. No
   status on any screen is derived from file existence.
3. **Provider secrets never enter `tack.db`, a log line, or the operator API** — with
   exactly one exception, which ADR 0061 (VI-A2) authorizes and bounds: a loopback-only,
   embedded-runner-only, write-once route that hands a key to the co-located runner's
   owner-only store without persisting it. Every card touching a secret asserts the
   *absence* directly (row counts, captured log output) with a positive control proving the
   assertion can fail.
4. **The gateway is a provider, not a proxy.** Tack still never calls a model API to execute
   an attempt. The runner's probe may call the gateway's catalog with the runner's own key —
   that is runner-side network, already the runner's domain. No `TACK_*` variable on the API
   server names a model endpoint; that sentence in `docs/CONFIG.md` becomes precise rather
   than absolute.
5. **Catalogs are measured, never typed in.** No static list of gateway or vendor models
   anywhere in the tree. What a picker shows is what a probe returned, with its timestamp,
   or a typed reason why nothing was returned.
6. **Requested and actual stay distinct.** When the gateway serves a model, `actual` records
   what the harness reported, with its `model_observation_source`; the requested pair is
   never copied into the actual columns. Reaching the same underlying model directly and
   through the gateway are two different `(provider, model_id)` pairs.
7. **Docket is out of scope.** Nothing here touches `orch_*`, `/approvals`, `/provision` or
   the ProvisioningWizard (ADR 0060 keeps them; it does not invite edits). "Agents" in this
   Part means runner-v1.
8. **Architecture vocabulary stays out of default screens.** The story names two components
   because that is the product (§VI.0, the statement). The UI's default screens say *agent*,
   *model*, *provider*, *run*. "Runner", "fleet", "enroll", "heartbeat", "capacity",
   "lease", "fencing" and "harness" appear only under *Advanced* and in the developer book.
   A test greps the rendered default screens for them; VI-C1 owns it.

---

## §VI.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `docs/book/src/user-guide/{agent-runners,cli,configuration,quick-start}.md`, `docs/API-REFERENCE.md` §"Runner, Fleet & Execution", the model-resolution rows of `docs/CONFIG.md` | VI-A1 — **VI-D1 amends after Waves 15–16 ship** |
| `docs/adr/0061-*.md`, the one provider-credentials bullet in `docs/CONFIG.md` | VI-A2 only |
| `crates/tack-runner/src/secrets.rs` (new), the store path in `crates/tack-runner/src/config.rs`, the `secret_reference` branch of each adapter, `tack runner secret …` in `crates/tack-cli/src/main.rs` (one subcommand arm + one module) | VI-B1 |
| The `[provider.*]` section of `RunnerConfig`, each adapter's spawn environment/args, `bootstrap::probe`, `crates/tack-cli/src/doctor.rs`, the gateway rows of `docs/CONFIG.md` | VI-B2 — **after B1 merges**. Exception recorded 2026-09-03: the refined ADR 0061 decision 1 requires `tack runner doctor` to report which store backend is in effect, so **VI-B1 owns that one `doctor.rs` line** and the store's wiring in `bootstrap.rs`. B2 owns everything else in both files and must not revert it |
| `crates/tack-runner/src/provider.rs` and the module tree replacing it, the catalog half of `crates/tack-cli/src/doctor.rs`, the registry | VI-B4 — **after B2 merges**. Disjoint from VI-B3 and may run alongside it. B4 must not touch the wire type or the fixtures; that is B5's, and B4's own acceptance depends on them staying byte-identical |
| The per-model field on `ModelCombination`, `docs/contracts/runner-v1/**`, the pin table in `crates/tack-orch/tests/runner_contract.rs` | VI-B5 — **after B4 merges**, and the only card that may change the wire contract in this Part |
| `crates/tack-api/src/handlers/local_runner.rs` (new), its mounts in `router.rs` (those only), the control handle in `crates/tack-cli/src/local_runner.rs`, the persisted on/off flag, `frontend/src/features/agents/{ExecutionToggle,ProviderKeyPanel}.tsx` + their client and tests | VI-B3 |
| `frontend/src/features/agents/**` except B3's two panels, `frontend/src/app/routes.tsx`, the nav entry, the *Advanced* section re-mounting `RunnerFleetSection`, the one mounting line in `features/fleet/FleetPage.tsx`, the first-run banner on the Board | VI-C1 |
| `frontend/src/shared/runWithAgent/**`, the attempt-state chip on Board cards | VI-C2 |
| `migrations.rs` (**062 only**), `crates/tack-core` project model, `crates/tack-db/src/repo/` project reads/writes, the project handlers, `crates/tack-orch/src/model_policy/wiring.rs` project tier, `frontend/src/features/settings/**` Agents tab, `frontend/src/features/fleet/runnerFleet/{AgentProfilesPanel,FleetsPanel}.tsx` | VI-C3 |
| The two attempt list handlers, their mounts in `router.rs` (those two only), `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`, `frontend/src/features/item-detail/tabs/AgentActivityTab.tsx` | VI-C4 |
| New handler tests in `crates/tack-api/tests/` for those same two routes | VI-C5 — production files stay untouched |
| The unknown-attempt cases in `crates/tack-api/tests/handlers/attempt_lists.rs` | VI-C6 — additions only; VI-C5's tests stay byte-identical |
| Nothing in the tree. `docs/agent-handoffs/part-vi/VI-C7.md` only | VI-C7 — a measurement card; a fix it finds is written as a diff in the handoff, not applied |
| New tests for the attempt-scoped events and artifact-download routes | VI-C8 — production files stay untouched |
| `frontend/src/shared/runWithAgent/**` | VI-C9 — **after VI-D2 merges**, which is recording this dialog. Shares the directory with VI-C2, which is closed |
| `scripts/check-comments.sh` and the citation cleanup across `frontend/src` | VI-C10 — **after VI-C9 merges**; both edit `shared.ts` |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | **generated** — regenerated by whichever of B3 / C3 / C4 lands; the integrator re-runs `UPDATE_OPENAPI=1 … openapi_contract` and `npm run gen:api` after each merge. Never hand-edited |
| `docs/contracts/runner-v1/**`, `crates/tack-orch/tests/runner_contract.rs` | **VI-B5, and nobody else.** VI-B2 escalated the field and VI-B5 carries it; every other card holds these files byte-identical and escalates with evidence instead. Smuggling one through `additional` to dodge the pin is forbidden — that is a contract change without a review |
| `scripts/smoke.sh`, the amendments to every page VI-A1 wrote, the final pass on `docs/CONFIG.md`, the one "Run it" sentence in `README.md` that names the Agents page (and the final README merge), `CHANGELOG.md` `[Unreleased]` | VI-D1 |
| `README.md` (whole-file restructure), `docs/book/src/introduction.md`, the opening paragraph of `docs/book/src/developer/README.md`, `docs/diagrams/**` (new), the first sentence of `CLAUDE.md`'s overview | VI-A3 — **`README.md` collides with Part V's V-C3; see §VI.3** |
| `docs/screenshots/hero.gif`, `agents.png`, `attempt.png`, `two-machines.png`, and the README image markup for them | VI-D2 — **`docs/screenshots/**` is V-C2's until it lands; see §VI.3** |
| `crates/tack-runner/src/harness/locate.rs` (new), the binary discovery in `harness/{claude_code,codex}.rs`, the "not installed" guidance in `agent-runners.md` | VI-C41 — **not** `doctor.rs`, `bootstrap.rs`, the contract or the frontend |
| `SecretStore::open`/`platform_store` in `crates/tack-runner/src/secrets.rs` (signature of `open` unchanged), the backend line in `crates/tack-cli/src/doctor.rs`, the secret-store rows of `docs/CONFIG.md` | VI-C42 — **not** the harness files, `bootstrap.rs` or `local_runner.rs` |
| `frontend/src/features/agents/runnerFleet/FleetsPanel.tsx` + test, `fleetsApi` in `frontend/src/shared/execution/api.ts` + test (additions), the fleet paragraph of `agent-runners.md` | VI-C43 — frontend only; no backend change, no new endpoint |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |

---

## §VI.3 Dependency graph, cross-Part conflicts and merge policy

```text
VI-A1 (docs: the path, the truth) ─────────────────────────────────────────────┐
VI-A3 (README + introduction + diagram) ───────────────────────────────────────┤
                                                                               │
VI-A2 (ADR 0061) ──┬── VI-B1 (secret store) ── VI-B2 (Vercel AI Gateway) ──┬── VI-B4 (provider trait) ── VI-B5 (catalog on the wire)
                   │                                                      │   │
                   │                        └── VI-B3 (embedded: on/off, key) ──┼── VI-C1 (Agents page) ──┐
                   │                                                             │                          │
                   ├── VI-C3 (project agent settings) ── VI-C2 (modal) ──────────┴──────────────────────────┼── VI-D1 (proof + docs)
                   │                                                                                        │
                   └── VI-C4 (artifact / decision lists) ───────────────────────────────────────────────────┘

VI-C1 + VI-C2 ─────────────────┐
Part V · V-C2 (recovery demo) ─┴── VI-D2 (hero + screenshots) ── feeds VI-D1's final README merge
```

**Wave 14 is a gate, not a sprint.** VI-A1 and VI-A3 are pure documentation and can land
any time — A3 first if Part V's V-C3 is about to start, since A3 restructures the file V-C3
proposes text for. VI-A2 is a decision record the **user approves**; no Wave 15 card
branches until it is accepted, because B1–B3 implement its choices and would otherwise
encode a guess.

**Wave 15 is sequential.** B1 first (the store). B2 needs the store to read the key. B3 needs
the store to write it and is opaque to what the key is for, but merges after B2 so its
"catalog appears after save" observation has something to observe.

**B4 and B5 extend Wave 15, and only B4 is parallel.** B4 (the provider trait, ADR 0063
decision 4) needs B2 and shares no file with B3, so the two run together. B5 changes the wire
contract B4's acceptance pins byte-identical, so it cannot run beside B4 — it follows it.

**Wave 16 has two independent starts.** C3 and C4 need only the ADR's vocabulary and can run
alongside Wave 15 — they are the widest diffs on the board and should start early. C1 waits
for B2 + B3; C2 waits for C3.

**Wave 17 is two cards, in order.** D2 (assets) needs C1, C2 and Part V's V-C2; D1 (proof and
docs) goes last and takes the final README merge.

### Cross-Part conflicts — read before branching

Part V Wave 13 is in flight. Two files are shared:

1. **`docs/screenshots/**` — V-C2 owns it until it lands, and its recording keeps its
   slot.** VI-D2 does not start before V-C2 has landed; it replaces the PM hero and adds
   three files, and never touches V-C2's recording, which VI-A3 gives a named slot
   (*Durable by design*). V-C2 hands its markdown to VI-A3, not to V-A4 (closed).
2. **`README.md` — three writers, one order.** VI-A3 restructures the whole file **first**,
   now, while V-C3 waits on V-C2. V-C3 proposes launch text against A3's structure. VI-D2
   swaps the image markup; VI-D1 adds one sentence and takes the final merge. A Part V card
   that finds A3 in flight escalates rather than racing — §V.3's own rule, applied back.

`docs/CONFIG.md` is touched by three cards here — A1 (model-resolution rows), A2 (one
bullet), B2 (gateway rows) — in disjoint sections. Each names the rows it touched in its
handoff so the integrator verifies disjointness rather than assuming it.

`router.rs` is touched by B3 (one gated mount) and C4 (two operator routes). Sequential
merge; the integrator regenerates `openapi.json` after each.

A card that discovers another collision states it in the handoff and stops.

---

## §VI.4 Cards

### VI-A1 — Write the path from a board item to a completed attempt, and delete what is false

**Pure documentation. Can start immediately.** Written against the product **as it is
today** — five hand-typed fields and all — because a stranger installing beta.7 gets that
product. VI-D1 amends this card's pages after Wave 16 ships.

**Owns:** `docs/book/src/user-guide/agent-runners.md` (one new section, the Known-gaps
rewrite, the harness-id fix), `cli.md` (five new sections), `configuration.md` (the
runner rows or a first-line pointer — see Tasks), `quick-start.md` (one section),
`docs/API-REFERENCE.md` §"Runner, Fleet & Execution" (one complete worked example), the
model-resolution rows of `docs/CONFIG.md`, and the VI-A1 handoff. **Does not own** any
runtime code, `README.md`, or the provider-credentials bullet in `docs/CONFIG.md` (VI-A2's).

**Context.** See the §VI.0 evidence table — every row about `docs/` is this card's. The
shape of the failure: three documents point at each other (API-REFERENCE → agent-runners →
CONFIG) and none of them keeps the question "how do I make this item run with Sonnet?".
Four statements are false today (`claude_code`; "model profiles have no effect"; "fleet
members have no write route"; two pages each claiming to be the configuration reference).

**Tasks:**
- The page opens with the §VI.0 statement — or its first paragraph and a link — so
  `agent-runners.md`, the README and the introduction say one thing, not three.
- `agent-runners.md`, new section **"Running an item with an agent"** placed *before*
  "Enrolling a runner": the four entry points (the modal, `tack execution create`,
  `POST /api/executions`, MCP `create_execution`), then **one complete, copy-pasteable
  `tack execution create` invocation** with all thirteen fields filled with real values
  against `tack serve --with-runner`, and its real output. The same body as raw JSON goes in
  `API-REFERENCE.md`. Run it; paste the transcript; do not construct it from the schema.
- `agent-runners.md`, new section **"Choosing a model and a provider"**: the four-tier
  precedence (request override → agent profile `limits.default_model` → project *(no
  storage today; say so)* → fleet `default_policy.default_model` → auto-select) with the
  exact JSON each tier accepts, how `model_combinations` / `model_passthrough` gate it at
  claim time, and — **in the same paragraph** — the rule "Tack never holds a provider
  credential" next to the rule "Tack does route the model choice". These two sentences
  have never appeared together, and that is the whole reported confusion.
- Fix `claude_code` → `claude-code` at l.32. Rewrite the Known-gaps list: the
  `model_profiles` bullet becomes what is true (a saved pair the modal copies into the
  request override; not read by the scheduler as a *default* tier); the fleet-members
  bullet is deleted and replaced by "the route exists; the Fleet panel does not call it
  yet"; the decision/artifact-list bullet stays and is re-cited.
- `cli.md`: sections for `tack execution`, `tack runner` (including `doctor` and `start`),
  `tack fleet`, `tack agent-profile`, `tack model-profile`, each generated from real
  `--help` output with one executed example.
- `configuration.md`: either add the `TACK_LOCAL_RUNNER_ENABLE` / `TACK_RUNNER_*` rows, or
  replace the page body with a first-line pointer naming `docs/CONFIG.md` as the single
  authority. Choose, and defend the choice in the handoff; two pages disagreeing is the
  defect, not the missing rows.
- `quick-start.md`: one section "Run an item with an agent" reaching a completed attempt in
  the fewest steps the product supports today, every console step shown.

**Acceptance:** a reader with a fresh `tack serve --with-runner` and one authenticated
harness reaches a completed attempt following **only** the new agent-runners section —
proven by a transcript in the handoff of doing exactly that, commands copied from the page.
`grep -rn claude_code docs/book/` returns nothing. Every remaining Known-gaps bullet cites a
`file:line` that still holds. Every new CLI section's example was executed and its output
pasted. `configuration.md` and `docs/CONFIG.md` no longer both claim to be the reference.

---

### VI-A2 — ADR 0061: provider credentials and model catalogs at the runner boundary

**A decision card. Prepares and stops; the user accepts the ADR.** No Wave 15 card branches
before that.

**Owns:** `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`, the one
provider-credentials bullet in `docs/CONFIG.md` (l.89-108 today), and the VI-A2 handoff.
**Owns no code.**

**Context.** ADR 0050 l.29-30 and ADR 0058 l.80-83 are quoted in §VI.0. Both are correct
about the API server. Both are being cited to mean "Tack cannot help you configure a
provider", which produced the reported confusion (recorded in IV-A5's own Context, which
called the situation "correct but invisible" and fixed only the visibility). The runner
side of the boundary was never decided: the runner owns its enrollment credential, owns the
harness subprocess and its environment, and accepts `secret_reference` entries with no
resolver. This ADR decides what the runner may hold and how a key reaches it, so that Wave
15 encodes a decision rather than a guess.

**Decisions the ADR must make**, each with the options rejected and their cost:

1. **Where a provider key lives.** Recommended: a runner-local, owner-only store in
   `TACK_RUNNER_STATE_DIR`, alongside `session.json`, with the same `0600` posture.
   Rejected for the record: `app_meta` like the backup secret (crosses the line ADR 0050
   drew; the backup key is the *server's own* secret, a provider key is not); a Tack-side
   vault (a proxy by another name).
2. **The one exception.** An embedded-runner, loopback-only, write-once operator route that
   hands a key to the co-located runner's store. Its exact preconditions (`--with-runner`
   on, loopback bind — already enforced before the server opens a socket), what it returns
   (never the key, never a fingerprint), and that it **does not exist** otherwise — a 404,
   the same shape as the `TACK_ORCH_ENABLE` routes, not a route that refuses. Decide which
   auth applies: the recommendation is the ordinary operator token, matching Settings →
   Cloud Backup's secret-key write, and the ADR says why a higher-privilege token is or is
   not warranted.
3. **Catalog fetching.** The runner's probe may call the gateway's model list with the
   runner's key; confirm this sits inside the runner's existing network domain and that the
   API server still makes zero model-provider calls. Decide what a catalog entry records
   (provider, model id, probed-at) and what it does not (any price the catalog reports is
   `catalog_reported`, never `measured` — it is not a measurement of a run).
4. **Vocabulary.** The provider id string for the gateway in `model_combinations`
   (recommendation: `vercel-ai-gateway`), that a gateway model id is the gateway's own
   `creator/model` form verbatim, and that the same underlying model reached directly and
   through the gateway are **different** `(provider, model_id)` pairs (§VI.1 rule 6).
5. **The surface map as product rule.** Adopt §VI.0's table: the "never UI" column is
   closed and named; a future step that cannot be UI needs an ADR amendment, not a card's
   judgement.
6. **Turning the embedded runner on from the UI — an amendment to ADR 0058.** Today the
   gate is a startup flag. Recommended: on a loopback bind, one switch in the UI starts or
   stops the in-process runner and persists the choice (recommendation: `app_meta`, read by
   the server after the database opens); **off by default is unchanged** — a fresh install
   runs nothing until a person on the same machine chooses — and **loopback-only is
   unchanged** — the switch's route does not exist on any other bind. `--with-runner` stays
   as the flag-only equivalent for scripts and the smoke. Rejected for the record: keeping
   the flag as the only path (it is the single console step a UI-only user cannot avoid,
   and it costs the whole "self-explanatory binary" claim); making bare `tack` mean
   `serve --with-runner` on loopback (changes a security default to save one click the UI
   can offer instead — recorded as an open question in §VI.5, not decided here). This
   decision rewrites the first row of the surface map.

**Tasks:** write the ADR in the house format (see 0059/0060: Status, Date, Supersedes,
Contract, Context, Measurement, Decision, Consequences). Replace the `docs/CONFIG.md`
sentence *"there is no `TACK_*` variable for a model provider or endpoint, by design"* with
the precise rule the ADR states. List in the handoff every sentence in `docs/` and
`README.md` the ADR makes imprecise (`grep -rn "never becomes a model proxy\|no TACK_\*
variable for a model provider\|never reads, stores, or forwards"`) for VI-A1 / VI-D1.

**Acceptance:** the ADR names each decision, each rejected option and its cost, and cites
0050 and 0058 by line for what it reaffirms and what it bounds. The user has accepted it
(recorded in the handoff with the date). The CONFIG.md sentence is replaced. The handoff's
sentence list is complete — the grep in Tasks returns only the ADRs.

---

### VI-A3 — Two components, one product: the README, the introduction and the diagram

**Pure documentation and one diagram. Can start immediately.** Supersedes V-A4's README
structure, keeps V-A4's four questions and its claim → evidence rule, and reuses the §VI.0
statement verbatim.

**Owns:** `README.md` (whole-file restructure), `docs/book/src/introduction.md`,
the opening paragraph of `docs/book/src/developer/README.md`, `docs/diagrams/**` (new),
the first sentence of `CLAUDE.md`'s Project Overview, and the VI-A3 handoff. **Prepares,
does not apply,** the GitHub repository description (outward-facing — §V.1 rule 5).
**Does not own** `docs/screenshots/**` (V-C2 now, VI-D2 later), `agent-runners.md`
(VI-A1), or any crate.

**Context — the README shows a project manager.** Measured 2026-09-03: the hero asset's
own alt text is *"Board, Timeline, and vocabulary editor"* (`README.md:12`); the Features
section lists *Project management* first and *Agent execution* second (`:32`, `:42`); all
five screenshots — board, timeline, dashboard, list, vocabulary editor — are PM views and
**zero** show an agent doing anything (`docs/screenshots/`); the two-component architecture
is explained at line 169, after *Status*; and the book's introduction lists four core
concepts — item, workflow, project type, vocabulary — with neither *runner* nor *run*
among them. A reader who arrives from the category Tack is trying to leave sees exactly
that category, and the differentiator reads as one bullet among many. V-A4's sentence was
right about the *what*; the page's shape still says the opposite.

**Tasks:**
- **First screen, in this order, before any PM screenshot:** the H1 and a first line that
  names both components (proposal, for the card to finalise: *"A self-hosted project board
  that dispatches its items to coding agents — Claude Code or Codex — through
  runners that live where your code lives. One binary. The board plans and records; the
  runner executes."*), then the **two-component diagram**, then the §VI.0 statement, then
  a three-step *How it works* strip (plan it on the board → a runner picks it up where the
  code is → the run is recorded on the item). Until VI-D2 lands, the diagram occupies the
  hero slot and the current GIF moves down to *Screenshots* under its honest alt text.
- **The diagram**, `docs/diagrams/two-components.svg` with its source alongside: two
  planes side by side — *the board (one)* / *runners (many)* — with the pull arrow pointing
  the only direction it ever goes, the harnesses beneath each runner, and two callouts:
  *credentials stay here* on the runner side, *leases · fencing · history* on the board
  side. It must render on GitHub, in the published book, and in dark mode — an SVG pair
  behind a `<picture>` does all three; a mermaid block does GitHub only unless the book
  gains a preprocessor. Measure, then choose.
- **A *Two components* table** — rows: what it does · what it holds · how many · what
  happens when it dies · how you run it; columns: board, runner. This is the scalability
  story the user asked for, in one glance.
- **A named slot, *Durable by design*,** directly under the statement's third paragraph,
  for V-C2's recovery recording. If V-C2 has not landed, the slot carries the sentence and
  a link to the smoke step that proves it, and no placeholder image.
- Reorder *Features*: agent execution, then project management, then automation surfaces.
  Keep every claim → evidence row V-A4 established; add none that VI-D1 cannot prove.
- `introduction.md`: open with the statement; the core-concepts table gains **Runner** and
  **Run**; the agent-runners link moves to the top of *Quick links*.
- `developer/README.md`: open with the statement's third paragraph, then the crate map.
- `CLAUDE.md`, Project Overview, first sentence only: name both components and that
  `--with-runner` embeds one in the other — so every agent that writes a doc here carries
  the same frame.
- Draft the new GitHub description in the handoff for the user to apply.

**Acceptance:** an agent with no context on this repository reads only the README's first
screen (through the *Two components* table) and reports back that the product has **two
parts**, what each holds, and that one board serves many runners — the same test V-A4
used, with the answer it must now produce. `README.md` contains no PM screenshot above the
diagram. The statement appears byte-identical in the four places (`diff` in the handoff).
The diagram renders in GitHub light, GitHub dark and the built book — three screenshots in
the handoff. Nothing outward-facing was applied.

---

### VI-B1 — A runner-local secret store, and `secret_reference` finally resolves

**Needs VI-A2 accepted.** First card of Wave 15.

**Owns:** `crates/tack-runner/src/secrets.rs` (new), the store path in
`crates/tack-runner/src/config.rs`, the `secret_reference` branch in each of the three
adapters, `tack runner secret set|list|remove` (one subcommand arm in
`crates/tack-cli/src/main.rs` plus one small module), the `keyring` dependency line in
`crates/tack-runner/Cargo.toml` (and the `Cargo.lock` it moves), and the VI-B1 handoff.

**Context.** `EnvironmentValue { value | secret_reference }` is in the contract and in the
claim fixture; every adapter warns and skips a `secret_reference` entry because "no
secret-store client exists in tack-runner yet" (`claude_code.rs:62-64`, `codex.rs:772-775`,
`opencode.rs:1156-1158`). That is a documented gap with a contract-level caller already
waiting. The store is the smallest thing that closes it and is what VI-B2 and VI-B3 both
need — one mechanism, three callers, all in this wave.

**Design, fixed here so B2/B3 do not re-decide it:** one `SecretStore` behind two
backends, chosen once at runner start and reported by `tack runner doctor` (`backend:
keychain | file`): the platform credential store through the `keyring` crate (service
`tack-runner`, account = the entry name), and — only when no platform store answers — a
single owner-only file (`secrets.json`, mode `0600`, in `TACK_RUNNER_STATE_DIR`) mapping
name → value. Names are `<provider>/<label>` identifiers (`vercel-ai-gateway/default`) —
safe in logs, errors and `Debug` output; values are never in any of them, and the value
type's `Debug`/`Display` are hardcoded to `[REDACTED]` exactly like `RunnerCredential`.
`tack runner secret set <name>` reads the value from stdin or from
`TACK_RUNNER_SECRET_VALUE`, **never from argv**. A `secret_reference` string takes one
optional scheme: `store:<name>` (the default when no scheme is present, so the frozen
fixture's bare name stays valid) resolves from the store; `env:<VARIABLE>` reads the
runner's own environment at spawn — the path for a systemd-started runner with no
keychain and no wish to keep a file. Resolution happens in the adapter's `validate` step; a missing name or unset variable is
a typed pre-spawn failure (`secret_reference_unresolved`, naming the reference only).
**Corrected 2026-09-03 by VI-B1, which measured the engine rather than trusting this
paragraph:** `validate` does *not* run before the journal record and the worktree exist.
`engine.rs::run_claimed` persists and fsyncs the journal entry and provisions the real
checkout *first*, on purpose — a documented crash-safety boundary that a secret-store card
must not reorder. The true claim, and the one to assert, is narrower: `validate` itself
writes nothing, so a rejection there leaves whatever the engine already did untouched.
**Today's warn-and-skip becomes an error**, because once a resolver exists a silently
missing variable is a fake success. No KEK-encrypted file, no third backend, no
per-project label pinning: the roadmap's "How the runner keeps a provider key" names
each with its trigger.

**Acceptance:** a live attempt with an environment entry `{"secret_reference": "demo"}`
reaches the fake-harness shim with the variable set — the shim prints the value's *length*,
never the value. Captured `tracing` output for that run contains the name `demo` and never
the value (positive control: the name is asserted present). A missing reference fails at
`validate` with the typed reason, and the state directory and workspace root are asserted
untouched. `stat -c '%a'` on the store file is `600`, measured on a real run. The
three adapters' behaviour is proven by reverting the resolver once and watching the
"variable is set" assertion fail. The keychain backend is proven live on the dev machine
with the platform's own tool (`secret-tool lookup service tack-runner account <name>` on
Linux, `security find-generic-password -s tack-runner -a <name>` on macOS) — the handoff
records the command and exit status, never the output. The file fallback is proven by
running with the platform store unreachable (`DBUS_SESSION_BUS_ADDRESS=/dev/null` on
Linux) and asserting `doctor` reports `backend: file`. Unit tests run against the file
backend (and `keyring`'s in-crate mock store if 4.x ships one — check, don't assume), so
CI needs no Secret Service. An `env:` reference reaches the shim the same way a `store:`
one does. `runner_contract` byte-identical: no fixture changes.

---

### VI-B2 — Vercel AI Gateway as a runner provider

**Needs VI-B1 merged.** The runtime heart of this Part.

**Owns:** a `[provider.vercel_ai_gateway]` section of `RunnerConfig` (`enabled`; `secret`,
the B1 store name, default `vercel-ai-gateway`), each adapter's spawn environment and
arguments, the catalog step in `bootstrap::probe`, the provider block in
`crates/tack-cli/src/doctor.rs`, the gateway rows of `docs/CONFIG.md`, and the VI-B2
handoff. **Escalates, does not edit,** `docs/contracts/runner-v1/**`.

**Context — vendor facts, fetched 2026-09-03.** Re-fetch `vercel.com/docs/ai-gateway/
coding-agents` before relying on any cell; this table is a starting point, not a source.

| Harness | Endpoint | Mechanism the vendor documents |
|---|---|---|
| `claude-code` | `https://ai-gateway.vercel.sh/claude-code` — **no `/v1` suffix**; the SDK appends `/v1/messages` and a double suffix 404s | env: `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN=<key>`, **`ANTHROPIC_API_KEY=""`** (must be set *empty* — a non-empty value wins over the auth token); optional `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`. Model ids like `anthropic/claude-sonnet-5` |
| `codex` | `https://ai-gateway.vercel.sh/codex/v1` | `~/.codex/config.toml`: `model_provider = "vercel"` and `[model_providers.vercel] base_url = …, env_key = "AI_GATEWAY_API_KEY", wire_api = "responses"` (required — Codex no longer speaks Chat Completions). The codex adapter forwards **no** ambient environment, so the key is injected per spawn. **First thing this card measures:** whether the provider can be selected per invocation (`-c model_provider=…` overrides) rather than by writing the user's own config file — the runner must never edit a user's `~/.codex/config.toml` |
| `opencode` | native provider `vercel`, via `/connect` in the TUI | credential lands in `~/.local/share/opencode/auth.json`. Whether an environment variable or an `opencode.json` entry can carry it **non-interactively** is measured, not assumed. If it cannot, opencode's gateway path is a console step rendered by VI-C1, and this card says so in its handoff and in `docs/CONFIG.md` |

Catalog: the gateway's model list (`GET /v1/models`, Bearer key) becomes
`model_combinations` for each harness that can reach the gateway — provider
`vercel-ai-gateway` (or what ADR 0061 chose), model id verbatim, probed-at recorded. Only
when the provider is enabled **and** the secret resolves; otherwise zero entries and a typed
reason (`provider_not_configured` / `catalog_unreachable` with status), never a stale list.

**Tasks:** the config section; the probe step; spawn injection per harness **only when the
request's resolved model provider is the gateway** — a request for a direct vendor model on
the same runner must receive none of the gateway environment, or the harness silently
routes through the gateway and `actual` lies; the actual-model observation for a
gateway-served run (what does each harness report, and with which
`model_observation_source`?); `doctor` prints `provider: vercel-ai-gateway — configured /
not configured / catalog error <status>` plus the catalog count and timestamp, key never
printed; usage on a gateway-served attempt is still `measured` only from harness output,
never from the gateway's dashboard.

**Acceptance:** at least `claude-code` and one other harness complete **live** end-to-end
attempts through the gateway, with `actual.model_provider` and `actual.model_id` recorded
as the harness reported them — transcripts in the handoff, on the real binaries, not the
shim. A request for a direct model on the same runner spawns with no gateway variable in
its environment, proven by the shim dumping variable *names*, and proven load-bearing by
reverting the guard once. With the provider disabled the capability snapshot has zero
gateway combinations, and a request naming one is refused at claim by the existing
scheduler intersection — untouched. Captured logs of a full gateway run never contain the
key; `sqlite3 tack.db .dump | grep -c <key>` is `0` after the run, with the shim proving the
harness did receive it. `tack runner doctor --json` byte-matches the live capability
snapshot including the catalog, as IV-A5 established. `runner_contract` byte-identical, or
an escalation with the exact field the contract lacks.

---

### VI-B3 — The embedded runner, controlled from the UI: turn it on, hand it a key — persisting neither in the database

**Needs VI-B1 (the store) and VI-A2 (decisions 2 and 6).** Opaque to VI-B2 — the key is
bytes to this route — but merges after B2 so "catalog appears after save" has a catalog.

**Owns:** `crates/tack-api/src/handlers/local_runner.rs` (new), its mounts in `router.rs`
(those only), the server→embedded-runner seam in `crates/tack-cli/src/local_runner.rs` —
a control handle (`start`, `stop`, `set_secret`, `reprobe`) the server calls, **not** a
path string; the server must not learn where the store lives — the persisted on/off flag,
`frontend/src/features/agents/ExecutionToggle.tsx` and `ProviderKeyPanel.tsx` with their
client and tests, and the VI-B3 handoff.

**Context.** Today the embedded runner is spawned once, at startup, from the `--with-runner`
flag or `TACK_LOCAL_RUNNER_ENABLE`, and `AppState` knows nothing about it. ADR 0061
decision 6 makes it startable from the UI on a loopback bind. Two things must stay
exactly as they are: **off by default** (a fresh install runs nothing until a person on
the same machine turns it on) and **loopback-only** (the toggle is refused — route absent,
404 — when the server is bound anywhere else; that check already runs before any socket
opens). Where the on/off state persists across restarts is decision 6's call; the
recommendation is `app_meta`, read by the server after the database opens, so a later
`tack serve` on loopback resumes what the person chose, and `--with-runner` remains the
flag-only equivalent for scripts and the smoke.

**Routes** (final shapes follow the ADR): `PUT /api/local-runner` `{"enabled": bool}` →
`204`, starts or stops the in-process runner and persists the choice; `GET /api/local-runner`
→ `{enabled, state, since}`; `PUT /api/local-runner/secrets/{name}` `{"value": …}` → `204`,
never echoes the value or a hash; `DELETE …/{name}`; `GET /api/local-runner/secrets` →
names and `set_at` only. After a successful secret `PUT` the server signals the runner to
re-probe so the catalog appears without a restart — if the runtime has no re-probe entry
point, this card adds one, and this route is its caller. All of it exists **only** on a
loopback bind — plain `tack serve` on `0.0.0.0` → 404, the same shape as the
`TACK_ORCH_ENABLE` routes. Prove the routes are *absent* there, not present-and-refusing.

**Panels:** `ExecutionToggle` — one switch, *Agent execution on this machine*, with the
observed state beneath it; when the route is 404 it renders the console command and the
one-line reason (non-loopback bind). `ProviderKeyPanel` — one write-only field, *Vercel
AI Gateway key*; after save, a "Set <date> · Replace · Remove" line and the observation
"Catalog: N models as of <ts>" or the typed error, verbatim; a link to where a key is
created. When the route is 404 the panel renders the console steps instead —
`tack runner secret set vercel-ai-gateway` on the runner host, then *Re-check* — because
that is a remote-runner deployment, and §VI.1 rule 1 applies.

**Acceptance:** on a loopback `tack serve` with no flag: `GET /api/runners` is empty; after
`PUT {"enabled": true}` an active runner appears and a shim attempt completes; after
`PUT {"enabled": false}` no new claim happens (assert on a queued request that stays
queued); restart without the flag → the runner comes back because the choice persisted.
On a non-loopback bind all routes are 404. After a secret `PUT`, in one test with a
positive control: `app_meta` row count unchanged by the secret (the on/off flag is the only
new row), `sqlite3 tack.db .dump | grep -c <value>` = 0, and the runner store holds the
value at mode `600`. Request bodies never reach a log line (capture, with the name
asserted present). Playwright: toggle on → paste → save → catalog count rendered → the key
never appears in the DOM. The re-probe is proven by the catalog count changing in the same
test, with no restart.

---

### VI-B4 — A provider is a module, and a second module proves it

**Needs VI-B2 merged.** Independent of VI-B3 — disjoint files, may run alongside it.
Implements ADR 0063 decisions 4 and 5 (accepted 2026-09-05).

**Owns:** `crates/tack-runner/src/provider.rs` and whatever module tree replaces it, the
provider block of `crates/tack-cli/src/doctor.rs` **only where it prints the catalog**, the
`[provider.*]` config shape where the registry requires it, and the VI-B4 handoff.
**Escalates, does not edit,** `docs/contracts/runner-v1/**`, `crates/tack-orch/src/execution.rs`
and any frontend file.

**Context — what is actually there today, measured.** `provider.rs` is 399 lines and holds
the whole mechanism as `match` arms: `known_endpoint(provider, wire)` returns a static row,
`catalog_url(provider)` returns a static URL, and `attach_catalog` does not consult either —
it looks up `VERCEL_AI_GATEWAY_CONFIG_KEY` by name and asks that one provider. So the claim
that a second endpoint is "three data rows and no code path" holds only for a provider whose
catalog is a `{"data":[{"id":…}]}` list behind a bearer header, and only for a provider the
config happens to be keyed under. Anthropic's own API is neither: its catalog is
`GET /v1/models` with `x-api-key` and `anthropic-version` headers, not `Authorization:
Bearer`. That difference is the whole reason this card exists.

`fetch_catalog_ids` also discards everything the catalog publishes except the id — the
prices, context windows and modalities that ADR 0063 decision 5 calls the point of asking.

**Tasks:**
- A `Provider` trait, one implementation per provider, and a registry the machinery walks.
  A provider knows its own config key, its endpoint per `Wire`, its catalog request
  (URL, headers, auth placement) and how to parse its own catalog body into one common
  shape. **No vendor name may appear outside a provider module** — assert it: a grep for
  each vendor string over the non-module files is part of the gate, and the current
  `attach_catalog` fails it today.
- The common shape: model id, plus what the provider publishes and typed as absent when it
  does not. Unmeasured is nullable — a provider that publishes no price yields `None`, never
  a zero. This is decision 5's shape; **it stops at the runner's own surface in this card**
  (`doctor --json`), because carrying it to the board is a wire change VI-B5 owns.
- `attach_catalog` walks the enabled entries of `RunnerConfig::providers` and asks each
  registered provider for its own catalog, instead of naming one. `CatalogStatus` becomes
  per-provider; a provider whose secret does not resolve must not suppress another's
  catalog.
- **A second real provider module: Anthropic's own API** (`api.anthropic.com`,
  `x-api-key`, `anthropic-version`). Fetch the vendor's current docs; do not trust this
  paragraph's header names without checking them. It is in scope precisely because its auth
  header and catalog shape differ from Vercel's — a trait with one implementation proves
  nothing.
- `doctor` prints one block per configured provider, with the model count, the timestamp and
  the price/limit fields where the provider published them and `Not measured` where it did
  not. Keys are never printed, by any path.

**Acceptance:** the Vercel path is unchanged in observable behaviour — the four existing
`provider.rs` tests pass untouched, `runner_contract` is byte-identical, and a live gateway
attempt still completes (transcript in the handoff). Adding the Anthropic module touched no
file outside its own module and one registry line, stated as a `git diff --stat` in the
handoff — that stat **is** the card's proof, so if it is not true, say so rather than
reshaping the diff. A vendor-string grep over the non-module files returns nothing, proven
load-bearing by putting `vercel` back into `attach_catalog` once. A provider configured with
an unresolvable secret yields its own typed status and the other provider's catalog still
arrives, in one test. `tack runner doctor --json` byte-matches the live capability snapshot
as IV-A5 established.

**Stop if:** the common catalog shape cannot be filled from Vercel's and Anthropic's bodies
without one of them needing a field the other cannot express. Record both bodies and the
field, and hand the shape to VI-B5 rather than inventing a union type.

---

### VI-B5 — The catalog reaches the board: price, context window and modality on the wire

**Needs VI-B4 merged.** Not dispatched with it: it changes the frozen wire contract that
B4's acceptance holds byte-identical, and the two cannot both be true at once.

**Owns:** the per-model field on `ModelCombination` in `crates/tack-orch/src/execution.rs`,
`docs/contracts/runner-v1/**` and the pin table in `crates/tack-orch/tests/runner_contract.rs`,
the runner side that fills it, and the VI-B5 handoff.

**Context.** VI-B2 escalated this and it is still open: the catalog publishes per-model
pricing, context windows and modalities, and `ModelCombination` carries a provider, a bare
list of ids and a `discovery` string. The `additional` map must not be used — §VI.2 records
that smuggling a field through it is a contract change without a review, which is the thing
being avoided.

**Acceptance:** the fixture revision and the pin table land in the same change as the field.
A runner that reports no metadata still round-trips, so an older runner against a newer
board is not a break. Rendering it is VI-C1's or a later card's; this card carries it no
further than the API response.

---

### VI-C1 — The Agents page: one screen, in the user's words, that owns the whole path

**Needs VI-B2 and VI-B3 merged.** This is the "the binary is self-explanatory" card. Route
`/agents`, in the nav where *Fleet* is today.

**Owns:** `frontend/src/features/agents/**` except `ExecutionToggle.tsx` and
`ProviderKeyPanel.tsx` (B3's, composed here), `frontend/src/app/routes.tsx`, the nav entry,
the *Advanced* section that re-mounts `RunnerFleetSection` and its panels, the one line in
`features/fleet/FleetPage.tsx` that mounts `RunnerFleetSection` today (nothing else in that
file — the rest is docket, §VI.1 rule 7), a first-run banner on the Board, and the VI-C1
handoff. Reuses `RunnerHealthCard`, `EnrollmentPanel`, `ExecutionTimeline`; does not edit
them.

**Context — what the UI can observe.** `GET /api/local-runner` (on/off, or its 404, which
is itself an observation: "this is a remote deployment"); `GET /api/runners` — each
runner's state, heartbeat, capacity and `capability_snapshot` (per harness
`installed_version` or a probe error, `model_combinations`, `model_passthrough`);
`GET /api/local-runner/secrets`. What it **cannot** observe, and must say so: whether a
vendor login exists (§VI.1 rule 2 — "present, unverified"), and anything about a machine
that has no runner.

**Vocabulary is the card.** §VI.1 rule 8: the default screen says *agent* (Claude Code,
Codex), *model*, *provider*, *run*. "Runner", "fleet", "enroll", "heartbeat",
"capacity", "lease", "harness" do not appear outside *Advanced*. A unit test greps the
rendered default screen for that list and fails on a hit.

**The page is a state machine rendered as numbered steps**, each with an observed status
and, when a step is a console step, the exact command in a copy block with one line saying
why:

1. **Agent execution.** Three states: **Off** — B3's toggle, one click; **On, this
   machine** — since when, and a *Turn off*; **On, other machines** — a count and a link to
   *Advanced*. When the toggle route is 404: the command `tack serve --with-runner`, the
   reason (the server is not bound to this machine only), and a poll that flips the step
   without a reload.
2. **Agents on this machine.** Per agent, from the snapshot: *installed vX* · *not found*,
   with the vendor's install command · *could not check*, the probe error verbatim.
   *Re-check* triggers the re-probe B3 added.
3. **Provider.** Two paths, side by side, labelled as what they are — **"Use the agent's
   own login"** (per installed agent: the exact command, `claude` / `codex login`, the
   sentence that Tack cannot see the result, and "confirm with a
   test run below") and **"Use Vercel AI Gateway"** (B3's panel — one key, every model, no
   terminal). Neither is labelled *recommended* unless ADR 0061 said so.
4. **Default model.** A picker over the union of every active runner's
   `model_combinations`, plus *Auto*, plus *Type a model id* only where an agent attests
   `model_passthrough`; saves to the project default (C3). If C3 has not landed, this step
   is **hidden**, not stubbed.
5. **Test run.** Creates an item titled *Agent test* on the current project, runs the
   default profile with the instruction "print which model you are and exit" against the
   project's default repository (C3), and renders the run timeline inline. **This is the
   only way step 3 is ever shown green** for an agent's own login.

**Advanced — run agents on other machines.** Collapsed by default. Today's
`RunnerFleetSection` (enrollment, runner health, fleets and members, agent profiles, model
profiles) moves here unchanged. The docket Fleet page is not edited; whether its nav entry
should stay visible when `TACK_ORCH_ENABLE` is off is a one-line question for the
integrator, recorded in the handoff, not decided here.

**First-run banner** on the Board when execution is off and the project has items: "This
board can run its items with an agent — *Turn on*" → `/agents`. Dismissable per browser
(`localStorage` is fine; it is a convenience, not state).

**Acceptance:** Playwright, from a loopback `tack serve` with no flag: step 1 shows *Off*
and a switch, nothing is green; click → step 1 flips to *On, this machine* without a
reload; step 2 lists the fake-harness shim as installed with its version and the others as
*not found* with commands; after B3's panel is used, step 3 shows the catalog count; step 5
completes a run with the shim and shows the actual model. From a non-loopback bind: step 1
shows the command and the reason. Every status string is asserted against the API
response, never hard-coded. An agent with a session file and no completed run renders
*present, unverified* — the shim can stage that. The vocabulary grep test passes, and is
proven load-bearing by putting the word "runner" on the default screen once. Every command
string the page renders lives in one exported constant; VI-D1 cites it from the docs.

---

### VI-C2 — Run with agent: zero hand-typed identifiers

**Needs VI-C3 merged** (the storage its defaults come from).

**Owns:** `frontend/src/shared/runWithAgent/**`, the attempt-state chip on Board cards, and
the VI-C2 handoff.

**Context.** Today the modal asks, on every run, for a runner id typed by hand, a fleet from
a list with no membership UI, a remote, a base revision, a subdirectory and a timeout
(`RunWithAgentModal.tsx:298,386,393,399,412`), remembers none of them, and its only model
control is a saved pair. The product knows, or after C3 can know, every one of these.

**Tasks:**
- **Where it runs:** hidden when exactly one machine is active — the common case;
  otherwise a dropdown of active machines and groups from `GET /api/runners` and the fleets
  route, labelled by name, never by id. No free-text id anywhere.
- **Agent profile:** dropdown, preselecting the project's default (C3); when none exists,
  "Create default profile" inline via the existing `POST /api/agent-profiles`.
- **Harness:** only kinds the selected target reports installed, through the existing
  `gateHarnessModelSelection` helper — **unchanged**.
- **Model:** three modes with provenance visible — *Project default — `<provider/model>`*
  (C3), *Choose…* (the selected runner's `model_combinations` for that harness; free text
  only where `model_passthrough` is attested), *Auto*.
- **Repository:** a read-only summary from the project's agent repository (C3) with "Change
  for this run" expanding the three fields. Whether `base_revision` may be a ref rather
  than a SHA is measured against the runner's checkout, not assumed.
- **Timeout:** from the agent profile's `timeout_seconds`.
- After submit the modal closes and the Board card shows a small attempt-state chip next
  to the existing ▶ trigger, fed by the existing execution store; opening it lands on the
  item's Agent Activity tab.
- With execution off the modal body is "Agent execution is off — Turn it on" linking to
  `/agents`, not a form.

**Acceptance:** E2E: with C3 defaults set and one embedded runner, open the modal on an
item, change nothing, submit → the `POST /api/executions` body equals the project defaults
field for field (asserted on the intercepted request). No free-text field is visible by
default. Existing modal and helper tests are updated, not deleted; `shared.ts`'s gate
helper is byte-identical. The state chip is asserted to change from *Queued* to a terminal
label during a shim run.

---

### VI-C3 — Project-level agent settings: the tier with no storage, and the UI the tiers never had

**Needs VI-A2 accepted** (vocabulary only). Independent of Wave 15; **start early** — it is
the widest diff on the board, by construction, and that is one feature, not two cards: a
column, its repository read/write, its handler, the generated files, one settings tab. Do
not split it into a mechanism card and a caller card.

**Owns:** migration **062** — `ALTER TABLE projects ADD COLUMN agent_settings TEXT` (JSON,
nullable; **one `ALTER`, one migration name**), the typed `ProjectAgentSettings` in
`crates/tack-core`, the project repository reads/writes in `crates/tack-db/src/repo/`, the
project handlers and `UpdateProject`, the project tier in
`crates/tack-orch/src/model_policy/wiring.rs` (replace the always-`None`), a new **Agents**
tab in `frontend/src/features/settings/`, a real "Default model" field in
`AgentProfilesPanel.tsx` (writing the existing `limits.default_model` convention), add /
remove member in `FleetsPanel.tsx` calling the existing member routes, the regenerated
`openapi.json` / `schema.gen.ts`, and the VI-C3 handoff.

**Context.** `wiring.rs:17-23` says the project tier "needs either a real
`projects.default_model_policy` column or an explicitly documented reuse of an existing
column, decided by whoever owns the next migration batch". This card is that batch. One
JSON column holds the three project-level facts an attempt needs and nothing else stores:
`{ "default_model": <selector or "auto">, "default_agent_profile_id": <id>, "repository":
{ "kind", "remote", "base_revision", "subdirectory" } }` — validated server-side against
the typed struct; a malformed blob is rejected at write, never tolerated at read. The
agent-profile default and fleet default already exist as conventions with no UI; this card
gives them a field, not a new mechanism.

**Acceptance:** `crates/tack-orch/tests/model_policy_test.rs` gains the live project-tier
case — no request override, a profile with no default, a project default set → the resolved
pair with provenance `project`, through the real repository. The Settings → Agents tab
round-trips all three fields in Playwright. Adding a member through the Fleet panel is
proven by the scheduler leasing a fleet-targeted request **only after** the member was
added via the route (today's `scheduler_wiring_test.rs` does this against the database;
this one goes through the API). Migration 062 passes `run_all` on a fresh database and on
an upgraded copy of a beta.7 `tack.db`. `openapi_contract` regenerated, never hand-edited.
`runner_contract` byte-identical.

---

### VI-C4 — List what an attempt produced: artifacts and decisions without a typed id

**Independent. Needs nothing.** Can run alongside Wave 15.

**Owns:** `GET /api/executions/{request_id}/attempts/{n}/artifacts` and
`GET /api/executions/{request_id}/attempts/{n}/decisions` — handlers, their two mounts in
`router.rs`, the repository read methods (reuse the runner-side ones where they exist),
`DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`, `AgentActivityTab.tsx`, the regenerated
`openapi.json` / `schema.gen.ts`, and the VI-C4 handoff.

**Context.** `agent-runners.md` Known gaps: no list route exists for either; the panels
accept a manually-entered id. `docs/agent-handoffs/part-iii/III-F4.md` names the concrete
route shape requested to close this — follow it rather than inventing a second one. A
UI-only user cannot answer a decision or download an artifact today without an id copied
from an event payload; that is the last step of the surface map.

**Acceptance:** the panels never ask for an id. `execution-attempt-detail.spec.ts` (III-F4)
is extended to discover the artifact through the list rather than the known id, keeping
its byte-equality download proof. The decision list shows pending / resolved / expired;
resolving stays behind `TACK_EXECUTION_DECISION_TOKEN`, fail-closed, untouched. Runner
protocol routes untouched; `runner_contract` byte-identical.

---

### VI-C5 — The two attempt-list routes get the handler tests they shipped without

**Needs nothing.** Wave 17, alongside the proof cards — it touches no file any of them owns.

**Owns:** the new handler tests under `crates/tack-api/tests/` for the two routes below, and
the VI-C5 handoff.

**Context.** VI-C4 shipped `GET /api/executions/{id}/attempts/{n}/artifacts` and
`.../decisions` covered by the OpenAPI contract test, the frontend unit tests and the E2E
spec — and by no handler test in `crates/tack-api`. The card disclosed that plainly; the
Wave 16 integrator recorded it in `VI-C4.md`'s amendment of 2026-09-04 and assigned it to
"the next Wave 16 card touching that crate". No such card remains, so the debt has no owner.
Nothing asserts at the Rust level that an unauthenticated caller is rejected, or that the
ordering the frontend renders is the ordering the handler returns.

**Tasks:** add handler tests for both routes covering at least rejection without a token, the
empty case, and ordering with more than one row. Follow the pattern the neighbouring tests in
`crates/tack-api/tests/` already use rather than inventing a harness.

**Acceptance:** each new test is proven load-bearing by reverting the behaviour it asserts
once and watching exactly that test fail — name the test, paste the failure. The ordering
assertion states the order the handler actually guarantees, read from the handler, never
inferred from what the frontend expects. No production file changes: `git diff --stat`
outside `crates/tack-api/tests/` and the handoff is empty. `runner_contract` and
`openapi_contract` stay byte-identical.

**Stop if:** making a route testable would need a production change. That is a finding, not
this card's file — record what you needed and hand it over.

---

### VI-C6 — What the two attempt-list routes do with an attempt that does not exist

**Needs nothing** — VI-C5's file is on `develop`. Wave 17, alongside the other proof cards.

**Owns:** additions to `crates/tack-api/tests/handlers/attempt_lists.rs` and the VI-C6
handoff. **Production files stay untouched**, `git diff --stat` proves it.

**Context.** VI-C5 covered auth, the empty case and ordering on
`GET /api/executions/{id}/attempts/{n}/artifacts` and `.../decisions`, and declared one gap
in its own handoff: neither route's unknown-attempt path is tested. An id nobody ever created
and an id belonging to a *different* execution are different questions and both are open.

**Tasks:** find out what each route actually does for an attempt that does not exist, and for
an attempt number that exists under another execution, then pin it. Read the answer from the
handler, never from the OpenAPI spec — the spec records intent and this card records
behaviour.

**Acceptance:** each new test proven load-bearing by reverting the behaviour it asserts once
and watching exactly that test fail — name the test, paste the failure. If a route answers
`200` with an empty list for an attempt that was never created, that is the finding: say so
in the handoff, with the handler line that does it and what a caller would conclude, and
**pin the current behaviour anyway** so a later change is visible.

**Stop if:** the honest answer needs a production change. VI-C4 owns those handlers and is
closed — record what you needed and hand it over rather than changing it.

---

### VI-C7 — Does dispatching to Codex ever produce a claimable request on `develop`?

**Needs nothing.** Wave 17. **Measure and report — this card changes no production file.**

**Owns:** `docs/agent-handoffs/part-vi/VI-C7.md` and nothing else.

**Context.** V-C2 recorded that dispatching through *Run with agent* with harness = Codex and
model mode = *Auto* never produced a claimable request in over three minutes — the attempt sat
in `queued`. That was measured against the published `v0.1.0-beta.7` artifact, not this line,
and V-C2's own amendment says so: two of the three observations it filed alongside this one
were already false on `develop`. This one has never been re-measured, and it sits directly
under Part VI's promise — a stranger picks an agent and a model and gets a run.

**Tasks:** reproduce it on today's `develop`, with a real runner, from the dialog rather than
from the API. If it reproduces, follow it until you can name the component that stops:
whether the request is written at all, whether the runner's claim filter matches it, and what
`Auto` resolves to at each hop. If it does not reproduce, that is just as much a result —
record the exact conditions under which it does not, because the next card will rely on them.

**Acceptance:** a verdict with timestamps and ids, not an impression: the request id, the
attempt id, what each hop held, and the command that shows it. Whichever way it goes, the
handoff ends with the one-line answer someone can act on. If you find the cause, name the
file and line and put the smallest fix **in the handoff as a diff** — do not apply it. Another
card is recording live video against this same tree while you work.

**Stop if:** reproducing needs a credential you do not have, or a change to the runner. Both
are findings, not this card's files.

---

### VI-C8 — The same scoping, on the two routes nothing tests

**Needs nothing** — VI-C6's file is on `develop`. Wave 17.

**Owns:** new tests for the two routes below under `crates/tack-api/tests/` and the VI-C8
handoff. **Production files stay untouched.**

**Context.** `crates/tack-db/src/repo/execution.rs` looks an attempt up by
`(request_id, attempt_number)` together at **four** sites. VI-C6 covered two of them. The
integrator's own revert — dropping the `request_id` predicate at all four at once — made
exactly VI-C6's two cross-execution tests fail and nothing else, which is the measurement
that matters: the other two sites are `list_events_for_attempt_number` and
`get_execution_artifact_by_attempt_number`, and **no test anywhere notices when their scoping
goes away.** The second is the serious one. A listing that loses its filter leaks metadata; a
*download* that loses its filter hands over another execution's file.

**Tasks:** cover both routes for an attempt number that belongs to a different execution, and
for one that does not exist. Follow `crates/tack-api/tests/handlers/attempt_lists.rs` — VI-C6
left the pattern, including how it seeds two executions in one test.

**Acceptance:** the revert-once proof is the *cross-execution* one, not a status-code flip:
drop the `request_id` predicate from each site and show that your test fails **and that the
other execution's row appears in the response body** — paste it. `git diff --stat` shows
nothing outside your tests and the handoff.

**Stop if:** either route turns out not to scope by execution at all. That is a security
finding, not a test — write it up and stop.

---

### VI-C9 — "Auto" must not be a choice that silently never runs

**Done — integrated 2026-09-06.** Handoff `docs/agent-handoffs/part-vi/VI-C9.md`, with an amendment recording what its own fix costs.

**Owns:** `frontend/src/shared/runWithAgent/**` (the submit gate and whatever surface the fix
needs) and the VI-C9 handoff. **Does not touch** the scheduler: its rejection is correct given
the contract.

**Both directions are closed, and three cards found it independently.** VI-C7 measured the
Auto path; VI-D2 hit the explicit path while trying to record the hero and had to dispatch
around the button with `POST /executions`; V-C4 found `scheduler-e2e.spec.ts`'s four tests
failing deterministically on chromium and firefox — roughly six minutes per browser of CI's
E2E budget — waiting on this very gate to resolve. The suite has been reporting this defect
for some time, buried inside a job that was cancelled before it could say so.

**The explicit half.** `isCombinationSupported`
(`frontend/src/shared/execution/capabilities.ts:184-207`) iterates only
`harness.model_combinations` and never consults `model_passthrough`. Both bundled harnesses
ship `model_combinations` empty and attest `model_passthrough: Supported`
(`codex.rs:740`, `claude_code.rs:913`), and the scheduler honours that attestation — its arm
rejects only when `!declared && !passthrough`. So the server accepts an explicit model on
both harnesses and the dialog disables the button for exactly that case. **The gate is
stricter than the contract, and it blocks the only path that works.**

**Context.** VI-C7 measured it end to end. The dialog defaults every harness to *Auto*. A
request reaches the scheduler as `ModelSelector::AutoSelect` when no tier — agent profile,
project, fleet — names an explicit model, and also when a tier is pinned to the literal
`"auto"`, which stops the precedence walk where it stands. `evaluate_candidate` then rejects
it unconditionally, before any harness or runner check. The result is a request that sits in
`queued` with no error, no toast, no attempt row, and no wait long enough — on exactly the
install a stranger has before completing the Agents page's default-model step. The submit gate
meanwhile calls the choice allowed, on the promise that *"the scheduler will still validate at
claim time"*. It does not validate; it always refuses.

**The gate cannot decide this alone** — `gateHarnessModelSelection` receives only capabilities,
harness and the model pair, and no tier's configuration. VI-C7's own proposed fix was retracted
for that reason: blocking every unspecified model would break the installs where Auto works.

**Tasks:** make the dialog tell the truth about what *Auto* will do on this install, and make
the default-model step reachable from the place the operator hits the wall. Whether that needs
a resolution preview on the wire or only what the client already knows is this card's to
decide and to record — decide it from the code, not from this paragraph.

**Acceptance:** an explicit model on a harness attesting `model_passthrough: Supported`
submits and runs — prove it live, with the request id, through the button and not around it.
On an install with no default model at any tier, an operator cannot submit a dispatch that
will never run without being told so in the dialog, in words naming the fix. On an install
where a tier names an explicit model, the same dispatch still submits and still runs. The
false "the scheduler will still validate at claim time" string is gone from the tree.
`scheduler-e2e.spec.ts`'s four tests pass without being weakened — they were right.

**Stop if:** the honest fix needs a server route that does not exist. Name it and stop.

---

### VI-D1 — Prove the stranger's path, and make the docs match what shipped

**Last card. Needs everything, VI-D2 included.**

**Owns:** `scripts/smoke.sh` (new steps), amendments to every page VI-A1 wrote, a final
pass on `docs/CONFIG.md`, the one "Run it" sentence in `README.md` that names the Agents
page (and the final README merge — §VI.3), `CHANGELOG.md` `[Unreleased]`, and the VI-D1
handoff. **Does not own** the README's structure (VI-A3) or its assets (VI-D2).

**Tasks:**
- **Smoke step 13 — the gateway path with a local fake gateway:** an HTTP shim serving
  `/v1/models` and the per-harness endpoint, in the same pattern as the fake-harness shim,
  proving key → catalog → spawn environment → actual model. It must be able to `FAIL`;
  prove it by injecting a wrong key once.
- **Smoke step 14:** plain `tack serve` → `GET /api/local-runner/secrets` is 404.
- **The stranger test — the deliverable of this Part.** On a clean container with the
  release binary and the fake harness, a person — or a Playwright script that only clicks
  and copies commands **the UI shows it** — reaches a completed attempt with the UI as the
  sole guide — starting Tack is the only command typed; turning execution on is a click. The transcript, with every copied command and every observed status, goes in
  the handoff.
- Docs: the quick-start agent section now describes the Agents page; the CLI path stays
  documented as the second path; the README's "Run it" gains the one sentence that names
  the Agents page; every §VI.0 evidence-table row is re-measured, the README rows included.

**Acceptance:** `./scripts/smoke.sh` green with steps 13–14 proven load-bearing by injected
failure. The stranger transcript. `grep -rn "no TACK_\* variable for a model provider\|never
becomes a model proxy" docs/ README.md` returns only the ADRs and VI-A2's amendment. The
§VI.0 surface map's Target column is marked, row by row, *reached* or *not reached with
reason*, in the handoff — that table is what the integrator verifies.

---

### VI-D2 — Assets that show the execution plane: the hero and three screenshots

**Needs VI-C1 and VI-C2 merged** (there is nothing honest to record before them) **and
V-C2 landed** (it owns the directory; see §VI.3).

**Owns:** `docs/screenshots/hero.gif` (replacement), `docs/screenshots/agents.png`,
`attempt.png`, `two-machines.png`, the README image markup those four need, and the
VI-D2 handoff. **Does not touch** V-C2's recording or its slot.

**Context.** VI-A3 restructured the README around two components with a diagram in the
hero slot because no asset existed that showed an agent doing anything. This card makes
that asset. §V.1 rule 1 applies without exception: **every frame is real** — a real
harness (claude-code, proven live), a real item, a release build, no staged data.

**Tasks:**
- **`hero.gif`, twenty to thirty seconds, no narration:** Board → ▶ *Run with agent* on an
  item → the modal, defaults already filled, one click → the card's state chip goes
  *Queued* → *Running* → the item's Agent Activity tab with events streaming and the actual
  model named → *Done* → the artifact in the list. That is a day in the product; the
  recovery demo (V-C2) is the day something goes wrong, and the README shows both.
- **`agents.png`:** the Agents page, execution on, one agent detected, the provider
  configured, a default model chosen, the smoke run completed — every status green *by
  observation*, which the screenshot must have earned.
- **`attempt.png`:** the Agent Activity tab: events, requested vs actual model, usage
  marked `measured`, the artifact list.
- **`two-machines.png`:** the *Advanced* section with two runners on two hosts, different
  agents and models — the picture of "one board, many runners". Two real hosts (a container
  is a host). If only one host is available, two runner processes on it with different
  agents is still real and the alt text says so.
- Replace the README markup: hero in the hero slot (the diagram moves directly beneath
  it), the three screenshots **first** in *Screenshots*, PM views after. Alt text
  describes what is shown, not what is claimed.

**Acceptance:** each asset is produced from a release build on a machine named in the
handoff, with the commands used; no frame is edited beyond cropping; the GIF's terminal
state is a real completed run whose request id is recorded. `README.md` shows an agent
executing an item **before** it shows a Kanban board. V-C2's recording is untouched and
still in its slot. The hero is under 3 MiB (the current one is 2.4 MiB — measure, do not
exceed it by much without saying why).

---

### VI-C10 — The comment rule was never enforced on the frontend, and it shows

**Unblocked — VI-C9 merged 2026-09-06.** Independent of every other Wave 17 card.

**Owns:** `scripts/check-comments.sh` (its scope), and the citation cleanup in `frontend/src`.

**Context.** `check-comments.sh` scans `--include='*.rs'` under `crates/` and nothing else, so
the frontend has never been checked. Measure it before quoting this:

```
grep -rnE 'agent-handoffs|TODO\.md|\b(I{1,3}|IV|V|VI|VII)-[A-Z][0-9]\b' frontend/src | wc -l
```

**221 lines across 80 files** on 2026-09-06. The rule exists because this decay happened once
on the Rust side and reached operator log lines and API error responses before anyone noticed.
It happened again here, unwatched, and it has reached the screen:

- `EnrollmentPanel.tsx:210` **renders `docs/agent-handoffs/part-iii/III-E3.md` in the UI.** A
  screenshot of that paragraph was about to ship in `README.md`; VI-D2 was told to crop it out.
- `shared.ts:253` puts `docs/agent-handoffs/part-iii/III-E2.md, Gap 1` in the dispatch dialog's
  advisory text. **VI-C9 deletes that string for a different reason** — it is also false.
- `shared.ts:183` is the worst of the three, because it is wrong as well as archaeological:
  an operator-facing reason reading *"profile/project/fleet default precedence (TODO.md III-F3,
  Wave 5) has not landed yet."* It has landed —
  `crates/tack-orch/src/model_policy/mod.rs` walks exactly those tiers, and VI-C7's amendment
  cites the code. A stranger reads that sentence and concludes a feature is missing that is not.

**Tasks:** extend the gate to the frontend, then fix everything it flags. **User-visible
strings first** — those are defects, not untidiness. Rewriting is almost always right and
deleting almost always wrong: the citation usually wraps something true in board scaffolding,
so keep the knowledge and drop the pointer.

**Acceptance:** the extended gate is green, and proven load-bearing by re-adding one citation
of each kind it should catch — a `TODO.md §`, a card id, a handoff path — and watching it
fail on each. Every user-visible string that named a card or a handoff now says the same thing
without one, and `shared.ts:183` says what is actually true today or is deleted with the
mechanism it describes. Its runtime is stated (the Rust half is ~0.2s; say what the whole is).

**Stop if:** the gate flags something whose citation is load-bearing and cannot be rewritten
without losing meaning. Name it and leave it — an argued exception beats a silent deletion.

---

### VI-C11 — Nothing keeps the client's copy of the precedence walk honest

**Needs VI-C9 merged** (it is). Wave 17.

**Owns:** the shared fixture and the tests on both sides of it, plus the VI-C11 handoff.

**Context.** VI-C9 had to teach the dispatch dialog what an *Auto* request will resolve to,
and did it by hand-writing `resolveAutoModelPolicy` in TypeScript against three Rust
decisions: `ModelPolicyTier::ORDER`'s precedence, `resolve_model_policy`'s walk over it, and
`parse_model_default_convention`'s reading of the `limits`/`default_policy` blobs. Nothing
builds or fails when the two drift. The stakes changed with that card: the dialog now
**blocks** a dispatch on this answer instead of commenting on it, so a Rust-side change that
resolves *more* requests leaves the dialog confidently refusing work the server would run.
The function's own doc comment says all of this; what it cannot do is notice.

**Tasks.** Give the two sides one source of truth to be checked against, the way
`docs/contracts/runner-v1/` already does for the wire. A table of input tiers to expected
outcome, read by a Rust test and a TypeScript test, is enough — the walk is small and total
(the Rust side already has an exhaustive 2^4 test to derive the table from). **A table copied
into a `.test.ts` is not this card**; the point is a file both sides read.

**Acceptance:** proven load-bearing from the Rust side, which is the direction that matters —
reorder `ModelPolicyTier::ORDER` or add a tier, and the TypeScript test fails **without anyone
touching TypeScript**. Show it failing, then restore. Include the `pinned_auto` case: a tier
holding the literal `"auto"` stops the walk rather than falling through.

**Stop if:** the fixture would have to encode anything the wire does not already carry.

---

### VI-C12 — `GET /api/executions` hands back the whole table

**Needs nothing.** Wave 17.

**Owns:** `list_executions` in `crates/tack-api/src/handlers/executions.rs`, its mount, the
generated spec regeneration, the matching client call and its mocks, and the VI-C12 handoff.

**Context.** The handler takes no query parameters and runs
`SELECT … FROM execution_requests ORDER BY created_at DESC` with no filter and no limit; the
spec agrees, documenting it as "Every execution request, newest first". Every consumer then
filters in the browser — `frontend/src/shared/execution/store.ts:339` narrows by `item_id`
after the fact. So opening one item's Execution tab pulls every execution the install has ever
recorded. This was raised as "the endpoint ignores its own `item_id` parameter"; **it does not
have one**, which is the actual finding and a different fix.

**Tasks:** give the route the filter its callers already want, and a bound. Take the client
past it in the same change — a filter no caller uses is this tree's recurring defect.

**Acceptance:** the item-scoped call fetches only that item's rows, proven by asserting the
row count the handler returns rather than what the component renders. The spec and
`schema.gen.ts` are regenerated, never hand-edited. Existing callers still work.

**Stop if:** an unbounded default cannot be changed without breaking a caller you can name.

---

### VI-C13 — Two specs the picker redesign left behind

**Needs VI-C9 merged** (it is). Wave 17.

**Owns:** `frontend/e2e/a11y.spec.ts` and `frontend/e2e/agents-page.spec.ts`, and the handoff.

**Context.** VI-C2 redesigned the *Run with agent* target picker and collapsed the repository
fields. `scheduler-e2e.spec.ts` was never updated and failed from that day until VI-C9 fixed
its mechanics; VI-C9 escalated two more it did not own. `a11y.spec.ts:1106` drives the same
retired picker. `agents-page.spec.ts` has a test that fails only under cross-file parallelism —
which means it depends on state another file owns, and passes alone for a reason that is not
correctness.

**Acceptance:** both pass under the full parallel run, repeated three times, and the
parallelism-dependent one is fixed by removing the shared state it leans on rather than by
serialising the file. If serialising is genuinely the only fix, say why in the handoff — that
is an argued exception, not a default.

---

### VI-C14 — The Board's execution badges read from a list that now has an end

**Needs VI-C12 merged** (it is). Wave 17.

**Owns:** the Board and Sprint execution badges' data path
(`RunWithAgentButton.tsx`, `TestRunStep.tsx` and the store call behind them), and the handoff.

**Context.** VI-C12 gave `GET /api/executions` a default limit of 200, applied to the
unscoped call too, because before it there was no bound at all and one item's tab pulled the
whole table. The badges on Board and Sprint cards deliberately share that one app-wide fetch
rather than making a request per card, which is the right trade — but past 200 requests, an
item nobody has touched recently falls off the end of the preload and its badge goes stale.
The item's own Execution tab is unaffected: VI-C12 gave it a scoped fetch of its own.

This is not a regression VI-C12 introduced so much as one it made **reachable**: the previous
source was unbounded and wrong for other reasons. Making the bound honest is what exposed it.

**Tasks:** decide what the badges should read and make them read it. A per-item scoped fetch
per card is the obvious wrong answer — the reason they share one call is real. A projection
the server can answer in one request (latest execution per item, for the items on screen) is
the shape worth pricing first.

**Acceptance:** with more than `DEFAULT_LIMIT` execution requests in the database and the
oldest belonging to an item on screen, that item's badge shows its real state — proven by
asserting what the data path returns with the table seeded past the bound, not by looking at
a rendered card. State the request count the Board makes for a screen of items, before and
after.

**Stop if:** the honest fix needs a server route that does not exist. Name it and stop —
naming it is a result.

---

### VI-C15 — The embedded runner's state directory does not follow the database

**Needs nothing.** Wave 17.

**Owns:** `EmbeddedRunnerControl::new` and `load_runner_config`'s state-directory argument in
`crates/tack-cli/src/local_runner.rs`, its tests, and the VI-C15 handoff.

**Context.** VI-C13 found this while measuring, and it is the more serious of the two things
it could not fix. The embedded runner resolves its state directory independently of
`AppConfig.storage_dir`, so pointing the server at a different database does **not** point the
runner at different state: an enrollment made against one database is still on disk for a
server opened on another. The E2E suite is where it was noticed — `frontend/.tack-runner/`
persisting across a recreated `e2e.db` — but the suite is not the problem. An operator running
`tack serve --with-runner` twice against two databases is the same shape, and the credential
that leaks between them is a credential.

**Tasks:** make the runner's state directory derive from the same configuration the database
does. Decide and record what happens to state written under the old, unscoped path — a silent
migration, or a refusal that says what to do.

**Acceptance:** two servers on two databases, started in turn, each see only their own
runner's enrollment — proven from the state on disk and the API, not from a UI. A test pins
it. Prove it load-bearing by reverting the derivation once.

**Stop if:** the fix would change where an existing install's state lives without a migration
path you can describe. That is the finding.

---

### VI-C16 — Three specs, one server-wide switch

**Needs nothing.** Wave 17, and it will not stay small if it waits.

**Owns:** the agent-execution toggle's use in `frontend/e2e/execution-toggle.spec.ts`,
`provider-key-panel.spec.ts` and `agents-page.spec.ts`, and the handoff.

**Context.** Those three files each drive the same single, server-wide agent-execution switch,
with no per-test scoping available. VI-C13 made one cleanup step tolerate a sibling flipping it
first, which stops that particular race and names the rest. There is no fourth file today;
there will be, and it will arrive as an intermittent failure nobody can reproduce alone.

**Tasks:** give the suite one owner for that switch. A fixture that serialises access, a
worker-scoped setup, or a decision that one file owns it and the others read — the choice is
this card's, and the reasoning belongs in the handoff more than the mechanism does.

**Acceptance:** all three files pass under the full parallel run, repeated three times, on
chromium and firefox, and the mechanism makes a fourth file's author do the right thing without
reading this card. Say what happens if they do not.

---

### VI-C17 — The badges still guess, and one number is copied instead of derived

**Needs VI-C14 merged** (it is). Wave 17.

**Owns:** `ListExecutionsQuery` and `list_executions` in
`crates/tack-api/src/handlers/executions.rs`, the repository call behind it in
`crates/tack-db/src/repo/execution.rs`, `EXECUTION_LIST_PRELOAD_LIMIT` and `loadList` in
`frontend/src/shared/execution/store.ts`, `executionsApi.list` in
`frontend/src/shared/execution/api.ts`, the generated spec and client types, and the handoff.

**Context.** VI-C14 raised the badges' shared preload from the handler's 200-row default to its
2000-row hard cap and made an item absent from a capped response render `Unknown` instead of
silence. Both are correct and neither is the fix. The badges still ask "give me the most recent
N execution requests install-wide" and hope the items on screen are inside N — a question whose
answer degrades with install size no matter what N is. The question they want is "the latest
execution for each of these item ids", and no route answers it: `list_executions` takes one
`item_id`, not many. VI-C14 stopped there and priced two shapes; its handoff recommends
extending the route that exists over adding one, because that route already grew a query once.

The second half is smaller and worse. `EXECUTION_LIST_PRELOAD_LIMIT = 2000` in `store.ts` is a
hand-copy of `ListExecutionsQuery::MAX_LIMIT`, and its own doc comment says the two can drift.
**VI-C11 built the machinery that stops exactly this**, one card earlier in this wave — a
fixture generated from the Rust constant and read by a TypeScript test, which fails when either
side moves alone. A duplicated constant with a comment admitting it can drift is the defect that
card was created to end, reintroduced by the next card that needed a number from the server.

**Tasks:** make the badges ask the question they mean, and make the number derived rather than
copied. If the batch query removes the preload's need for a row cap at all, delete the constant
instead of binding it — that is the better outcome and it costs nothing to check first.

**Acceptance:** with the table seeded past `MAX_LIMIT` and the oldest execution belonging to an
item on screen, that item's badge shows its real state — asserted from what the data path
returns, not from a rendered card. State the request count and the response row count for a
screen of items, before and after; VI-C14 measured one request carrying up to 2000 rows
(≈343 KiB more per load than the 200-row version, computed from `ExecutionSummary`'s five
fields, uncompressed), so beating it means beating that, not just matching the request count.
Whatever binds the client's constant to the server's must fail when either side moves alone —
prove both directions, the way VI-C11's fixture was proven.

**Stop if:** the batch query cannot be answered without an N+1 in the repository layer. Say what
the query would have to look like and stop — naming it is a result.

---

### VI-C18 — One `run-with-agent` test fails every full run and passes alone

**Needs nothing.** Wave 17.

**Owns:** `frontend/e2e/run-with-agent.spec.ts` and the handoff. The gate under it
(`frontend/src/shared/runWithAgent/shared.ts`, `shared/execution/capabilities.ts`) is
**read-only** for this card — if the fix belongs there, that is an escalation, not an edit.

**Context.** `run-with-agent.spec.ts:98` ("item-detail: submitting a run creates the request and
it appears in the Execution tab without navigation") fails on **every** full parallel chromium
run and passes on its own. Measured at integration, after the two fixes that removed everything
else: 7 of 7 full runs failed exactly this test and nothing else; the same file run alone passes
4 of 4. It is deterministic, not flaky, which makes it cheap to work.

It enrolls its own runner with `opaque/model-alpha`, selects it in the target picker, and asserts
the Run button is enabled. Under the full suite the button stays disabled through all 24 retries.
Two things were already ruled out and should not be re-derived: the server-wide execution switch
(giving this test `executionToggleLock` fixed its sibling at `:153` — 7 of 7 — and did nothing
for this one) and today's merges (the same failure reproduces at `25819e0`, before any of them,
where the whole suite failed 5 of 5).

The interesting question is what a *freshly enrolled, never-connected* runner reports for
capabilities, and whether the gate VI-C9 built (`declared || passthrough`) is right to block it.
If it is right, this test has been asserting something false since it was written and the test is
what changes. If it is wrong, the gate is, and that is worth far more than the test.

**Acceptance:** the full parallel chromium suite passes three times consecutively. Say which of
the two the answer turned out to be, and if it is the test, say what it should have been
asserting instead.

**Stop if:** the honest answer is that the gate blocks a combination the scheduler would accept.
Name it precisely and stop — that is the same class of defect VI-C9 fixed and it outranks this
card.

---

### VI-C19 — Removing a provider key leaves the provider marked enabled

**Needs nothing.** Wave 17.

**Owns:** `EmbeddedRunnerControl`'s `set_secret`/`remove_secret` pair in
`crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

**Context.** `set_secret` flips `providers[vercel_ai_gateway].enabled = true` when the default
secret name is written, so a UI-only user never hand-edits a TOML flag after pasting a key. Its
opposite does not exist: `remove_secret` deletes the secret and its metadata and returns, leaving
`enabled` true with nothing behind it. Found by VI-C16 while serialising the E2E specs — once
they stopped racing, whoever ran second deterministically found a provider still marked enabled,
and it had to route around that with "Replace" instead of "Remove" in setup. That workaround is a
second reason to fix this: it is currently load-bearing for the suite.

An `enabled` provider with no credential is a capability claim that is not true, which this tree
treats as a defect on its own terms, not a cosmetic one.

**Tasks:** make the pair symmetric, and decide what `enabled` should mean for a provider whose
secret was configured under a *different* name via
`TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` — `set_secret` deliberately does not touch those,
so `remove_secret` must not silently disable them either.

**Acceptance:** set then remove leaves the provider exactly as it was before the set, proven from
the runner config and from `GET /api/local-runner`, not from a UI. Prove it load-bearing by
reverting the reset once. Then remove VI-C16's "Replace instead of Remove" workaround in
`frontend/e2e/provider-key-panel.spec.ts` and show the suite still passes — if it cannot be
removed, say why, because that means the bug had a second half.

---

### VI-C20 — `getOrCreateProject` returns whichever project ran last

**Needs nothing.** Wave 17.

**Owns:** `getOrCreateProject` in `frontend/e2e/helpers.ts` and every call site of it, and the
handoff.

**Context.** VI-C18 found this while fixing one symptom of it. `getOrCreateProject` does
`GET /api/projects` and returns `existing[0].id`; `list_projects`
(`crates/tack-db/src/repo/projects.rs`) orders `ORDER BY updated_at DESC`. Under
`fullyParallel: true`, "the shared E2E project" is therefore whichever project *any* concurrently
running test most recently created or patched — including its default model, its type, and its
vocabulary. Every spec calling this helper inherits that.

VI-C18 proved the leak directly rather than inferring it: on a fresh database, the failing dialog
displayed a project default model of `openai / opaque/model-not-declared-<timestamp>`, a fixture
string minted only by `scheduler-e2e.spec.ts`. It fixed its own test by not depending on the
ambient project at all. The helper is still wrong for everyone else.

**Tasks:** give the helper a stable identity. A name-scoped lookup, a per-worker project, or a
fixture that creates one and hands it down are all plausible; the choice is this card's and the
reasoning belongs in the handoff more than the mechanism does. Whatever you pick, a spec that
needs *its own* project must be able to say so, and one that genuinely wants a shared one must
still get the same one every time.

**Acceptance:** the full parallel chromium suite passes three times consecutively from a clean
state, and a test that asserts the shared project's identity gets the same id under
`--repeat-each=3` while other spec files are running. Say how many spec files were depending on
the old behaviour without knowing it.

---

### VI-C21 — The runner's credential outlives a database that was deleted, not reconfigured

**Needs VI-C15 merged** (it is). Wave 17.

**Owns:** the embedded runner's boot-time identity check in
`crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

**Context.** VI-C15 scoped the embedded runner's state directory to `storage_dir`, which fixes
the case it was carded for: two servers, two databases, two storage dirs, one working directory.
It does not fix the case where `storage_dir` stays put and the *database* is replaced — and that
case is not hypothetical. Deleting `frontend/e2e.db` without also deleting `frontend/storage-e2e`
leaves `storage-e2e/runner/session.json` holding a credential for a runner id that no longer
exists in the recreated database. The embedded runner then never reaches `active`, the harness
probe never runs, and `agents-page.spec.ts` fails on a missing version string — a symptom three
steps from the cause. Measured at integration: with both removed, the full chromium suite passes
70/70 three times; with only the database removed, that test fails alone and in the suite.

VI-C15's own reasoning anticipated the shape of this — it chose `storage_dir` partly because this
tree varies it *together* with `TACK_DATABASE_URL` everywhere it stands up a second install. That
is true of the config files and false of a person at a shell, which is where this bites.

**Tasks:** make the runner notice that the credential on disk does not belong to the database it
is now talking to, and recover rather than hang. The information exists — the runner id in
`session.json` either resolves against this database or does not.

**Acceptance:** with a valid `session.json` for a runner id absent from the database, the embedded
runner reaches `active` under a new identity, and says once, at `info`, that it did — with no
path, id, or credential in the line. Prove it load-bearing by reverting the check once. State what
happens to the abandoned journal beside it.

**Stop if:** distinguishing "this credential is for another database" from "this database is
temporarily unreachable" cannot be done without guessing. Say what signal would settle it and
stop.

---

### VI-C22 — The E2E suite's "throwaway" database is never thrown away

**Needs nothing.** Wave 17.

**Owns:** `frontend/playwright.config.ts`'s database and storage lifecycle, whatever setup or
`make` target that implies, `docs/TESTING.md`'s E2E section, and the handoff.

**Context.** `playwright.config.ts` calls `e2e.db` "a throwaway SQLite file". Nothing throws it
away. Across one integration session it reached **383 runners, 100 projects, 277 execution
requests and 723 items**, and the suite's failure set tracked that growth: **9 failures at 383
runners, 1 at 42, 0 from a clean state** — with different tests failing at each level, and every
one of them passing when its own file was run alone.

This is why this cycle kept producing contradictory flake measurements. VI-C13 reported three
clean parallel runs, VI-C16 measured 4-of-10 solo failures, VI-C18 measured 2-of-2, and the
integrator measured 7-of-7 and then 1-of-2 for the same test within an hour. None of those numbers
were wrong; they were taken at uncontrolled accumulation levels nobody was recording. **A flake
rate measured against this database is not reproducible unless the database's starting state is
stated with it.**

Note that the state is in two places, not one: the database *and* `storage-e2e/` (which now holds
the runner's credential and journal — VI-C15 moved it there). Removing one without the other is
its own failure mode, carded as VI-C21.

**Tasks:** decide what the suite's starting state is and make it that, every run. A clean database
per run is the obvious answer and may be too slow — measure it before ruling it in or out, since
a full run is currently ~17s from clean and ~47s at 383 runners, which is an argument in its
favour rather than against.

**Acceptance:** two consecutive full chromium runs produce byte-identical row counts for
`agent_runners`, `projects`, `items` and `execution_requests` at their end, and the suite passes
three times consecutively. State the wall-clock cost of whatever reset you add, measured, against
the ~30s the accumulated database was already costing.

---

### VI-C23 — Two tests hold wall-clock deadlines that a busy machine misses

**Needs nothing.** Wave 17.

**Owns:** `wait_for_ready`/`wait_for_active_runner` in
`crates/tack-cli/tests/embedded_runner_state_scoping.rs`, the shutdown timeout in
`crates/tack-runner/tests/bootstrap_entrypoint.rs`, and the handoff.

**Context.** Both tests assert against a stopwatch rather than against a state, and both fail on
a machine doing anything else. Measured at integration with three agent builds running (load
average 5–6): `two_servers_on_two_databases_each_see_only_their_own_runner_enrollment` failed 2 of
5 full-suite runs and then failed with `-j 1`, so it is not nextest's parallelism — it is the
box. `the_composition_root_stops_on_an_injected_shutdown_with_no_process_signal` budgets 5
seconds for the runtime to stop and took **110**, a 22× overshoot, then failed. On an idle
machine both pass, and the full suite is 1443 green.

The first is the worse of the two because it is new — it arrived with VI-C15 and its 10-second
`wait_for_active_runner` deadline was set on an idle machine by a card that had the machine to
itself.

**This is the same disease V-C4 found in CI from the other end.** There, a job cancelled at its
own 25-minute timeout hid a deterministic failure underneath it for days. Here, a green suite
turns red for a reason that has nothing to do with the code. Both make the signal unreadable;
both were fixed by looking at the budget rather than at the test.

**Tasks:** decide, per test, whether the thing being asserted is "this happens" or "this happens
within N". If it is the former — and for both of these it plainly is — the deadline is only a
liveness backstop and should be generous enough that reaching it means something is genuinely
wrong, not that a build was running. If any of it really is a latency claim, say so and keep a
tight bound with the load conditions it assumes written beside it.

**Acceptance:** both tests pass with the full suite running while the machine carries a load
average of at least 4 — say how you produced the load and what it measured. And they must still
fail promptly when the behaviour is actually broken: prove that by reverting VI-C15's derivation
once (which the first test already pins) and showing it still fails, and by naming what the
second test would catch.

**Stop if:** a generous deadline makes a genuine hang take longer to report than a developer will
wait. Say what the two numbers are and stop.

---

### VI-C24 — A serious contrast failure has been reported three times and carded zero times

**Needs nothing.** Wave 17.

**Owns:** the light palettes' `--color-text-secondary` in `frontend/src/index.css`, whatever
renders it at reduced opacity in the item detail drawer, and the handoff. Does **not** own
`a11y.spec.ts` — the test is already correct and already failing; making it pass by changing
what it asserts is the one outcome this card forbids.

**Two** tests in `a11y.spec.ts` fail on axe's `color-contrast` rule, not one: "item detail
drawer has no accessibility violations" (`:192`) and "item detail drawer with the dispatch
control visible" (`:442`). Same foreground in both, `#5f736e`, against two backgrounds —
`#e9edec` at **4.27:1** and `#eef1f0` at **4.43:1**, against WCAG AA's 4.5:1. Axe impact
`serious`. Fixing one without the other fixes neither: they share a cause.

**It is not a flake and not accumulation.** VI-C1's handoff reproduced it against an unmodified
base by stashing every change that card made. VI-C13's handoff carried it forward. VI-C22's
integrator measured it reproducing from a freshly emptied database. Three handoffs named it,
each correctly concluding it was outside their own card's ownership, and no card was ever
written — so it has been the standing failure that makes "the E2E suite passes clean" untrue
for weeks, and it absorbed part of the blame that belonged to database accumulation.

`#5f736e` is not a declared token. It is `--color-text-secondary` (`#566b66` in the default
light palette) composited at roughly 94% opacity over the surface beneath it — which is what
turns a token that passes on paper into a rendered colour that fails. The dark palette already
carries the fix for exactly this shape: `index.css`'s dark block documents lightening
`--color-text-secondary` from `#67807a` because it measured 3.52:1 on `--color-bg-subtle`. The
three light palettes (default, `clay`, `graphite`) never got the same treatment.

**Tasks:** decide whether the fix belongs in the token or in the thing that reduces its opacity —
they are different bugs with the same symptom, and the choice is the card's. Check all three
light palettes, not only the default: the failing test exercises one, and a fix that leaves
`clay` and `graphite` short has fixed a test rather than a defect. Contrast is measured against
what is actually composited, so verify the rendered pair, not the declared token values.

**Acceptance:** `npx playwright test --project=chromium -g "item detail drawer"` passes — that
pattern catches both failing tests, and passing only one is not passing — and the full chromium
suite's only remaining failures are unrelated to
`color-contrast` — state what is left. Report the measured ratio for every pair you changed, in
all three light palettes, with the command. Prove the test is load-bearing by reverting the
colour once and showing it fails again. Say whether the dark palettes were already compliant or
were quietly relying on the same opacity trick.

**Stop if:** meeting 4.5:1 requires a colour that no longer reads as secondary text against
primary — that is a design decision, not a contrast calculation. Say what the two constraints
are and stop.

---


### VI-C25 — A journal record whose attempt no longer exists is retried, and its checkout kept, forever

**Needs VI-C21 merged** (it is). Wave 17.

**Owns:** `report_recovery_and_apply_disposition` and the restart scan's disposition handling in
`crates/tack-runner/src/engine.rs`, its tests, and the handoff. Does **not** own the boot-time
credential check VI-C21 added, and does **not** own the workspace manager's cleanup contract.

VI-C21 made the embedded runner recover when its stored credential names a runner id a recreated
database has never seen. The journal is keyed by `state_dir` + `attempt_id` and never by runner
id, so it survives that swap intact — which is correct, and is why nothing is lost. But a record
written *before* the swap names an attempt the current database has no row for. The
acknowledgement comes back absent or mismatched, and `engine.rs` treats that the way it treats
any unsettled evidence: it leaves the record in the restart scan, because "an absent or
mismatched acknowledgement cannot settle local process evidence". That is the right default for
its intended case — a server that was briefly unreachable — and the wrong one here, because this
attempt is never coming back.

**VI-C21's handoff called this harmless. It understates it.** A `RecoveryPending` outcome also
deliberately keeps the workspace checkout, as the evidence an operator would need. So each such
record costs a retry on every boot *and* holds its checkout on disk indefinitely, with nothing
that ever revisits either. On a machine where agent worktrees and build targets already compete
for the disk, an unbounded set of checkouts belonging to attempts that provably cannot resume is
not a rounding error.

**Tasks:** separate "the server does not know this attempt" from "the server did not answer" —
these are the same code path today and must not be. The first is settled information and the
record can be retired; the second is not and must keep its current behaviour exactly. Decide what
"retired" means: quarantine keeps the checkout as evidence and stops the retry, deletion reclaims
the space and destroys the evidence, and the choice belongs to this card with its reasoning in
the handoff. Say explicitly what an operator loses under whichever you pick.

**Acceptance:** a journal record naming an attempt absent from the server is retired on the next
boot rather than rescanned, and a record whose acknowledgement merely failed to arrive is **not**
— prove both, and prove the second by making the server unreachable rather than by making it
answer "unknown". Assert the absence directly: the record is gone from the scan and the checkout
is in whatever state you chose, not merely that no error was logged. State what happens to
records already stranded by the time this ships. Prove load-bearing by reverting once.

**Stop if:** the server's response cannot distinguish "no such attempt" from "not authorised to
see this attempt" — retiring evidence on an authorisation error would destroy exactly what an
operator needs. Say what the responses are and stop.

---


### VI-C26 — Two of the three palettes are never scanned, and one of them is badly broken

**Needs nothing.** Wave 17.

**Owns:** `frontend/e2e/a11y.spec.ts`'s palette coverage, `graphite`'s `--color-primary-600` in
`frontend/src/index.css`, and the handoff.

The frontend's design tokens are two-axis — mode × palette — and `a11y.spec.ts` scans exactly one
cell of that grid: the default light palette. `clay`, `graphite` and every dark variant are
shipped untested. That is not a theoretical gap. Probing `data-palette="graphite"` by hand while
fixing VI-C24 found a `color-contrast` violation that reproduces **statically, 4 times out of 4**
— no animation, no timing, no flake: `--color-primary-600` (`#84cc16`) on the white sidebar
background at **1.97:1**, against WCAG AA's 4.5:1. That is less than half the required ratio, and
CI cannot see it because nothing asks it to look.

The coverage gap is the more important half. VI-C24 fixed a real defect the suite *did* catch and
that three handoffs still spent weeks dismissing as a flake; this one the suite cannot catch at
all, and it is far worse than the one that took all that effort.

**Tasks:** decide what palette coverage is affordable. Scanning every page in every palette
multiplies an already 30-second suite; scanning one representative page per palette probably does
not. The choice is this card's, and the reasoning belongs in the handoff — say what the new
coverage does **not** cover, so the next person inherits a known boundary rather than a false
sense of completeness. Then fix `graphite`'s violation. Check whether `clay` and the dark variants
hide anything similar **before** deciding the coverage shape, so the shape is informed by what is
actually there.

**Acceptance:** the new scans fail before the colour fix and pass after — prove both, in that
order, and give the measured ratio for every pair you change with the command that measured it.
The full chromium suite still passes, and say what it costs in wall-clock against the 32s it runs
in today. State plainly which palette × mode combinations remain unscanned after this card.

**Stop if:** fixing `graphite`'s primary requires changing what the palette *is* rather than
correcting a value — a primary that no longer reads as that palette's identity is a design
decision, not a contrast calculation. Say what the constraints are and stop.

---


### VI-C27 — The key reaches the page, not the runner, and the page says it worked

**Needs nothing.** Wave 17. **This one blocks Part VI's own definition of done.**

**Owns:** `EmbeddedRunnerControl`'s handling of configuration changes against an already-spawned
runner task in `crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

`EmbeddedRunnerControl::start()` clones `runner_config` once, at the moment the runner task is
spawned. `set_secret()` mutates only the control's own stored copy — never the spawned task's
independent snapshot. So a gateway key pasted while agent execution is already on never reaches
the runner that is actually running.

**What makes this worse than a missing feature is the feedback.** The Agents page's "Catalog: N
models" line updates immediately and looks correct, because `catalog()` re-reads the control's
copy fresh on every call. The one piece of UI an operator gets says *done*, while the dispatch
path still resolves the provider through the stale, disabled config and the attempt sits at
`preparing` forever. Nothing errors. Reproduced twice, deterministically, with full-debug logs
confirming the harness binary was never invoked for the real dispatch — only `--version`-probed.

The workaround is clicking "Re-check", which restarts the embedded runner. **Nothing in the UI
says so.** A stranger who reads the Agents page top to bottom and follows it in the order it is
laid out gets a dispatch that silently never completes — which is the exact failure this Part
exists to rule out, so this Part is not done while it stands.

**Tasks:** decide whether the spawned task should read configuration through a shared handle, or
whether a configuration change should restart the runner the way "Re-check" already does. The
second is smaller and honest; the first is better and larger. **Say which you chose and what it
costs**, rather than picking the small one silently. Whichever you choose, the invariant is that
no UI state may report a key as usable while the dispatch path cannot use it.

**Acceptance:** a key set while the runner is already running is used by the very next dispatch,
with no "Re-check" and no restart by the operator. Prove it end to end — an attempt that actually
reaches the harness, not a config assertion. Assert the absence directly for the failure case:
before the fix, the harness binary is never invoked for the dispatch, and your test must show
that, not merely that a status differed. Prove load-bearing by reverting once. Say what happens
to a key set while the runner is *stopped* — that path works today and must keep working.

**Stop if:** sharing the configuration handle would let a half-applied change be observed
mid-dispatch. Say what the interleaving is and stop; a silently wrong key is bad, a torn config
during an attempt is worse.

---

### VI-C28 — The board's own WebSocket is rejected by the browser, and only by the browser

**Needs nothing.** Wave 17.

**Owns:** `board_live`'s handshake in `crates/tack-api/src/handlers/websocket.rs`, whatever test
proves it, and the handoff. Touch `frontend/src/shared/realtime/boardSocket.ts` only if the fix
genuinely belongs on that side — decide which end is wrong before changing either.

`board_live` never selects or echoes a `Sec-WebSocket-Protocol` response header. The frontend
always offers one (`tack.v1`). RFC 6455 §4.1 requires a client that offered a subprotocol to fail
the connection when the server's response omits it — so a real browser closes it, and a stranger
who creates their first item watches it not appear on the board they just created it on, with no
visible error.

**Why nothing caught it, which is the part worth fixing properly:** a raw `curl` handshake does
not enforce §4.1 and succeeds. Every check this repo had was of that shape. The defect was found
only by driving a real browser end to end.

**Tasks:** decide which end is wrong — a server that ignores an offered subprotocol, or a client
that offers one nothing needs. Then close the test gap, because the fix without it just resets
the clock: whatever proves this must fail against the current server, and a `curl`-shaped check
provably cannot.

**Acceptance:** a real browser client that offers `tack.v1` stays connected and receives a live
update, and the same test fails against the unfixed server — run it both ways and show both.
State explicitly why the test you wrote catches what a `curl` handshake cannot. Prove load-bearing
by reverting once.

**Stop if:** echoing the subprotocol would commit the server to a versioning contract it has no
implementation for — a header that claims `tack.v1` while nothing enforces v1 semantics is a
capability claim, and those are load-bearing here. Say what the claim would be and stop.

---

### VI-C29 — The comment gate has never looked at the E2E suite

**Needs nothing.** Wave 17.

**Owns:** `scripts/check-comments.sh`'s roots and its path-resolution for the "pointers to files
that do not exist" category, the comments it then flags in `frontend/e2e/`, and the handoff.

`check-comments.sh` scans `crates/` and `frontend/src`. It has never scanned `frontend/e2e`, and
that is where board archaeology accumulates fastest, because tests get written directly against
acceptance criteria. Running the gate against that directory today exits 1 with, by its own
categories: **28 card ids, 28 board-vocabulary, 17 board citations, 4 handoff paths** —
`./scripts/check-comments.sh frontend/e2e`.

An earlier card extended this gate from `crates/` to `frontend/src` precisely because the rule had
decayed unenforced. It stopped one directory short.

**One complication you must handle, not work around:** that same run reports **22 "pointers to
files that do not exist"**, and most are false — they name sibling spec files that do exist, which
the check fails to resolve from an `e2e/` root. Adding the directory without fixing resolution
turns a real gate into one people learn to ignore.

**Tasks:** fix the pointer resolution first, then add the root, then rewrite what it legitimately
flags. **Rewriting is almost always right and deleting is almost always wrong** — these comments
usually wrap something real in board scaffolding. Keep the knowledge, drop the pointer.

**Acceptance:** `./scripts/check-comments.sh` with `frontend/e2e` among its default roots exits 0,
and every previously-false pointer is either resolved correctly or shown to be genuinely stale.
Report the count per category before and after, with the command. Prove the extended gate is
load-bearing by reintroducing one card id and showing it fires.

**Stop if:** a comment's only content is a card reference and the knowledge it wrapped is already
gone. Say which, and delete only those.

---


### VI-C30 — A developer running the UI the documented way never receives a live board event

**Needs nothing.** Wave 17.

**Owns:** the decision and its implementation — one of `default_allowed_origins()` in
`crates/tack-api/src/config.rs`, the `npm run dev` recipe (`Makefile`, `CLAUDE.md`,
`docs/book/src/developer/frontend.md`, `docs/book/src/user-guide/quick-start.md` step at
"Vite starts at `http://localhost:5173`"), or `board_websocket_is_authorized` in
`crates/tack-api/src/middleware.rs` — plus a test and the handoff. Do not widen more than one
of them.

`board_websocket_is_authorized` refuses the upgrade whenever the request carries an `Origin`
outside `TACK_ALLOWED_ORIGINS`, and the Vite dev proxy forwards the page's own origin
unchanged (proven at VI-C28's merge: the E2E suite's `localhost:5199` was refused with the
default list, and passing the origin in fixed it — 1 failed / 1 passed, same spec, no other
change). The default list holds `8080`, `3210` and `tack.test`; it has never held `5173`. So
the quick-start's own developer path — `tack serve`, then `npm run dev`, open `:5173` —
completes REST calls through the proxy and silently never receives a live event. Nothing
told anyone: the socket closes without an error the page shows, exactly as in VI-C28, and the
E2E suite only started exercising this path when VI-C28 landed (its config now sets the
variable for port `5199`, which is the suite's fix, not the developer's).

**Tasks:** decide where the fix belongs. Adding `5173` to the product's default allow-list
puts a dev port into every install's CORS posture; teaching the dev recipe to set
`TACK_ALLOWED_ORIGINS` keeps the product's defaults clean but adds a step the quick-start
must carry; treating a loopback-bound server's loopback origins as same-machine changes
what the check means. Weigh them in the handoff and pick one. Whatever you pick, a test must
fail without it: the E2E config's `TACK_ALLOWED_ORIGINS` line is the model of a from-scratch
proof, and `docs/CONFIG.md` (already corrected to name the WebSocket gate) must match.

**Acceptance:** following `docs/book/src/user-guide/quick-start.md`'s developer steps verbatim
on a tree with your change, an item created in one tab appears live in another without a
reload; the same steps on `develop` before your change do not. Show both. Prove
load-bearing by reverting once.

**Stop if:** the only shape that works also widens what a non-loopback bind accepts by
default. Say what it would accept and stop.
### VI-C31 — The Execution tab unmounts itself every four seconds

**Needs nothing.** Wave 17. **Blocks VI-C28's merge.**

**Owns:** `frontend/src/shared/execution/store.ts` (`loadAttempts`, `loadList`, the
`AttemptAvailability` vocabulary), `store.test.ts`, `frontend/src/shared/runWithAgent/ExecutionTimeline.tsx`
where it reads that vocabulary, one E2E proof (a new spec or an addition to
`execution-attempt-detail.spec.ts`), and the handoff.

`connectRealtime` re-runs `loadAttempts` for every watched request on every poll tick
(`realtime.ts`, 4 s). `loadAttempts` begins with `attemptsCache.set(id, {status: 'loading'})`,
discarding the `ready` data it already holds, and `ExecutionTimeline` renders `AttemptList` only
under `<Show when={attempts().status === 'ready'}>`. So every four seconds the attempt panel —
decisions, artifacts, the option an operator just chose, the decision token they typed, the
Resolve button under their cursor — is destroyed and rebuilt after the round trip. An operator
sees a flicker and loses in-progress input; a Playwright click that lands in the gap hits a
detached element. Found integrating VI-C28: once the board socket delivered events in the E2E
suite for the first time, `execution-attempt-detail.spec.ts` failed in 2 of 2 full chromium runs
(`locator.click` → "element was detached from the DOM"; `locator.check` → the radio never
reappeared within 30 s) and passed 2 of 2 with the socket refused. Check whether `loadList` has
the same shape for the list scope before deciding it does not.

**Tasks:** a refresh must keep what is already known: `ready` data stays rendered while the
next fetch is in flight, and the vocabulary says so honestly (a `refreshing` flag on `ready`, or
`loading` only when nothing is held — pick one and say why). Do not fix it by keying the list
differently or by making the spec wait; the defect is that a refresh forgets. Then the proof:
in the Execution tab, with a real pending decision, choose an option and type a token, wait
longer than one poll interval, and assert both survive and the same DOM node is still
attached. That is deterministic without the board socket, so it needs nothing from VI-C28.

**Acceptance:** the E2E proof passes with the fix and fails without it (show both);
`store.test.ts` pins that a refresh never transitions a `ready` entry through `loading`;
`npx vitest run` and the full chromium suite green. Prove load-bearing by reverting once.

**Stop if:** keeping stale data rendered would let a terminal attempt keep showing a live
Resolve control after the server already resolved or expired the decision. Say how the store
would know, and stop.

---

### VI-C32 — The embedded runner's boot misses its own liveness backstops on CI, and nobody knows what it is waiting on

**Needs nothing.** Wave 17.

**Owns:** the startup path of `tack serve --with-runner` as far as readiness is concerned
(`crates/tack-cli/src/local_runner.rs` and whatever in `crates/tack-runner/src/bootstrap.rs`
or the secret store it waits on), the three sibling integration tests
`crates/tack-cli/tests/embedded_runner_{orphaned_credential,state_scoping,live_secret}.rs`
only where their waits are concerned, and the handoff.

Two of the last fourteen CI runs on `develop` failed in this family and nowhere else:
`embedded_runner_orphaned_credential.rs:127` — `/api/health` not up within 15 s of spawning
the process — and `embedded_runner_state_scoping.rs:176` — the embedded runner not `active`
within 30 s. Both passed on the next push untouched, and both pass locally in well under a
second of wait. VI-C23 already rewrote these waits as liveness backstops "a genuine success
is never expected to approach"; CI approached them twice in one day, so either the number
is wrong for a shared runner or the boot does something there it never does here.

One measured lead, not a conclusion: `embedded_runner_live_secret.rs` sets
`DBUS_SESSION_BUS_ADDRESS=/dev/null` for its child and has never failed on CI, while the two
that failed do not set it. The secret store is keychain-first with a file fallback (ADR
0061); on a headless runner with no session bus, a keychain probe over D-Bus is a
plausible place to block for seconds before falling back — and if readiness or enrollment
waits behind that probe, the health endpoint itself is late. That would make it a product
fact, not a test fact: a server that is slow to answer `/api/health` on any headless host.

**Tasks:** measure first. Reproduce headless (no session bus, `DBUS_SESSION_BUS_ADDRESS`
unset) and time `spawn → health 200` and `spawn → runner active`, then the same with the
variable pointed at `/dev/null`; report both. If the probe is what blocks, decide where the
fix belongs — the probe should not sit in front of the listener, or should fail fast when
there is no bus — and only then touch the tests: a backstop that reflects the measured
boot, not a bigger number. If the probe is not it, say what is, with the numbers.

**Acceptance:** the two waits are explained by a measurement, the origin is fixed or
declared out of scope with its owner named, and ten consecutive CI runs of the three tests
(`workflow_dispatch` on the card branch is enough) show zero misses. Prove load-bearing by
reverting once.

**Stop if:** the boot is slow because the keychain probe is *correct* to wait — say what a
fast failure would cost a real desktop user whose bus is merely slow to start.

---

### VI-C33 — The coverage job fails on every pull request because tracing is initialised twice in one process

**Needs nothing.** Wave 17.

**Owns:** `init_tracing` in `crates/tack-api/src/server.rs` and its callers, the one test that
collides, and the handoff.

`init_tracing` calls `.init()` on the global tracing subscriber unconditionally. Under nextest
every test is its own process and nobody notices; under `cargo llvm-cov` (the Coverage job)
tests share one process, the second `.init()` panics, and
`server::tests::serve_with_ready_signals_the_real_bound_address` fails — on every pull request
and every push to `main`, since that job is gated `if: event != push || ref == main` and never
runs on a `develop` push. Found landing the actions-major bump (run 34113961066), confirmed
identical on the untouched Dependabot run 33982205066 two days earlier: not the bump's doing.

**Tasks:** make initialisation idempotent at the origin (`try_init`, or a once-cell that owns the
subscriber for the process), not by skipping the test under coverage. Then run the Coverage job's
exact command locally (`cargo llvm-cov` with the job's flags — read them from `ci.yml`) and show
it green.

**Acceptance:** the Coverage job green on a `workflow_dispatch` of the card branch; the panic
reproduced first with the job's own command and gone after. Prove load-bearing by reverting once.

**Stop if:** the collision is not the subscriber but two servers binding the same fixed port in
one process — say which, and stop.

---

### VI-C34 — With the SPA embedded, an unmatched `/api` route answers 200 with HTML

**Needs nothing.** Wave 17. **Blocks any merge to `main`** — "Embed SPA" is a required check.

**Owns:** the `fallback(spa::serve_spa)` wiring in `crates/tack-api/src/router.rs` and
`spa.rs`, the two `local_runner` handler tests that expose it, and the handoff.

Under `--features embed-spa` the router's catch-all fallback serves the SPA's `index.html` for
**every** unmatched path, `/api/...` included. Two tests in
`crates/tack-api/tests/handlers/local_runner.rs` (~lines 126 and 143) expect a genuine 404 from
an unmounted route and get the SPA's 200 instead — so the "Embed SPA" job is red on every pull
request and every push to `main` (gated the same way as VI-C33's job), and `main` cannot take a
merge until it is green. It is also a product fact for the single binary a stranger installs: a
typo in an API path, or a route a newer client knows and an older server does not, returns a
web page with status 200 instead of `404 Not Found`, which every client in this tree reads as
"the request worked".

**Tasks:** scope the SPA fallback to non-API paths at the router (`/api` and `/api/runner/v1`
keep the API's own 404), keep the deep-link behaviour the SPA needs for its routes, and make
the two tests run under both feature sets. Say in the handoff whether the CLI (`tack-cli` is
HTTP-only) ever depended on a 200 it should not have.

**Acceptance:** `cargo nextest run --workspace -E 'package(tack-api)' --features embed-spa` green
and the default build green, both shown; the "Embed SPA" job green on a `workflow_dispatch` of
the card branch. Prove load-bearing by reverting once.

**Stop if:** the SPA's own routing needs a 200 for a path under `/api` — name it and stop.

---

### VI-C35 — `cargo deny check` fails locally on advisories and licenses that CI never checks

**Needs nothing.** Wave 17.

**Owns:** `deny.toml`, the `cargo-deny` job's arguments in `ci.yml`, whatever dependency change
resolves a finding, and the handoff.

CI's job is named "cargo-deny (licenses + duplicate deps)" and is green; a plain `cargo deny
check` on `develop` is red on `advisories` (`proc-macro-error2` unmaintained, via
`validator_derive`; two yanked crates via `object_store` and `sqlx-sqlite`) and on `licenses`
(two MIT rejections via `secret-service` and `zbus`). Found landing the cargo-major bump; all
findings pre-date it. A gate that is green in CI and red on a developer's machine is two gates.

**Tasks:** decide which checks the repo means to enforce and make CI and `deny.toml` say the
same thing; resolve or explicitly `ignore` each finding with its reason and a review date;
`proc-macro-error2` also trips the compiler's future-incompat warning on every build, so its
resolution is worth naming on its own.

**Acceptance:** `cargo deny check` and the CI job agree, both green, with every ignore justified
in `deny.toml`. Prove load-bearing by removing one ignore and watching it fire.

**Stop if:** a license rejection is real and the crate has no replacement — say which and stop.

---

### VI-C36 — "Could not enqueue execution" says retry, and retrying never works

**Stopped 2026-09-07 (its own stop clause: unreproduced)** — handoff `docs/agent-handoffs/part-vi/VI-C36.md`; the real error is logged now, so the next occurrence names itself. **Needs nothing.** Wave 17.

**Owns:** the enqueue path of `POST /api/executions` (`crates/tack-api/src/handlers/executions.rs`
and what it calls in `tack-orch`), the error it returns, one test, and the handoff.

Found by VII-D1's stranger walk and reproduced twice over plain `curl`, no window involved:
against one item, a request with a fresh, never-used `idempotency_key` answered
`{"code":"internal_error","message":"Could not enqueue execution","retryable":true}`; a second
request with another fresh key against the same item answered identically six seconds later;
a brand-new item with the same payload shape succeeded instantly and finished seconds after.
Capacity was not it (`GET /api/runners` showed `available_capacity: 1` throughout). The
transcript is in `docs/agent-handoffs/part-vii/VII-D1.md`, "Product findings", with item ids
and timestamps. Whatever the cause, the response is wrong twice: `internal_error` hides a
condition the server evidently knows (it is per-item and persistent), and `retryable: true`
tells the client to do the one thing that does not help.

**Tasks:** find the origin — the likely shape is a per-item invariant (one live request per
item, a state the item is in, a row the previous attempt left) surfacing through a catch-all
arm — and give it a typed, non-retryable answer that names the condition (`409` with a code
the UI can render, or whatever the API's existing vocabulary has for it). Then the UI: the
Run-with-agent modal must show that reason rather than "try again". If the cause is a real
bug rather than an invariant, fix it at the origin and say so.

**Acceptance:** the reproduction from the handoff, replayed with a test, yields the typed
answer with `retryable: false`; the modal shows it; `internal_error` never carries
`retryable: true` for a condition the server can name. Prove load-bearing by reverting once.

**Stop if:** the condition turns out to be transient after all — show the timing, and stop.

---

### VI-C37 — The desktop app's dependency tree carries fifteen unmaintained crates nobody has reviewed

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C37.md`; re-measured, the count is sixteen. **Needs nothing.** Wave 17.

**Owns:** `crates/tack-desktop/Cargo.toml` and its own `Cargo.lock`, the `tack-desktop` step of
CI's `cargo-deny` job and the policy `scripts/gen-deny-toml.sh` generates for it, and the
handoff.

`cargo deny check` against `crates/tack-desktop` reports fifteen "unmaintained" advisories,
all inside Tauri's Linux GTK/WebKit bindings (`gtk`, `gdk`, `atk`, `glib`-adjacent crates and
their `-sys` siblings), with no upgrade path this repository controls. VI-C35 left that
workspace's advisories deliberately outside the gate rather than paper over them with fifteen
unexplained ignores. That is honest but it is not a review: nobody has said, crate by crate,
whether each finding is an accepted cost of Tauri 2 on Linux today, a sign the app should move
to a newer Tauri line, or a real exposure.

**Tasks:** enumerate the fifteen with `cargo deny check advisories` in that workspace and
`cargo tree -i <crate>` for each path; check what the current Tauri release and its
`tauri-build`/`wry` line pull in (a newer Tauri may already have moved off some); for each
finding decide and write the reason — `ignore` with a review date in the generated policy, or
a bump. Then bring the workspace's advisories into the same unrestricted check the root has,
so the two workspaces are gated the same way.

**Acceptance:** `cargo deny check` green in `crates/tack-desktop` with every ignore justified
in the policy source; CI's `tack-desktop` step runs the same unrestricted check; `make desktop`
still builds and VII-D1's walk still holds (the AppImage opens, closes to tray, reopens). Prove
load-bearing by removing one ignore and watching it fire.

**Stop if:** the only way to green is a Tauri major upgrade — say what it would change for
the app and stop.

---

### VI-C38 — graphite's `on-accent` goes white: the design decision VI-C26 escalated, taken

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C38.md`. **Needs nothing.** Wave 17. **Decided by the user 2026-09-07: option 1 of VI-C26's handoff — `on-accent` white.**

**Owns:** graphite's tokens in `frontend/src/index.css` (`--color-primary-600`, `--color-on-accent`
and whichever siblings the handoff names), `Sidebar.tsx`'s hardcoded swatch preview, the
`Button.tsx` comment that explains the dark on-accent, the graphite/light scan in
`frontend/e2e/a11y.spec.ts`, and the handoff.

VI-C26 proved no value of `primary-600` alone clears both of its roles in graphite/light (text on
white needs `L <= 0.1833`, background under `#1a2e05` needs `L >= 0.2733`) and stopped, as its
card told it to, because the only shape that clears both flips `on-accent` to white and changes
what the palette is. The user has taken that decision: graphite becomes dark olive with white text
on it, the way teal and clay already pair a dark primary with a white on-accent.

**Tasks:** pick the dark `primary-600` (the handoff suggests the `#3f6a0c`–`#4d7c0f` range; keep
graphite's hue), flip `on-accent` to white, walk every pairing VI-C26's table measured and re-measure
it with the same `contrast.js` command, update the swatch preview and the `Button.tsx` comment so
neither describes the old identity, and check graphite/**dark** still passes — its tokens may share
`on-accent`. Then remove `GRAPHITE_LIGHT_KNOWN_ISSUES` and fold graphite/light into
`OTHER_MODES_AND_PALETTES`, so all six cells are unsuppressed gates.

**Acceptance:** the graphite/light scan, unsuppressed, fails on the old tokens and passes on the new
ones — prove both in that order; every changed pair's ratio is in the handoff with the command that
measured it; the full chromium suite passes; no raw hex enters a component (tokens only). The
handoff says in one line what graphite looks like now versus before.

**Stop if:** graphite/dark breaks in a way that needs its own identity decision.

---

### VI-C39 — Raise the dependency floor to Rust 1.94 and land `sqlx` 0.9

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C39.md`. **Needs nothing.** Wave 17. **Decided by the user 2026-09-07: the floor rises to the lowest version
the latest stable `sqlx` needs, not to the newest Rust.**

**Owns:** `rust-version` in `Cargo.toml` and `crates/tack-desktop/Cargo.toml`, the MSRV job in
`.github/workflows/ci.yml` (name, action pin, `RUSTUP_TOOLCHAIN`, its comment), the README badge and
line 152, `CONTRIBUTING.md`'s table, the `sqlx` line in `Cargo.toml` and everything in `tack-db`,
`tack-api` and `tack-orch` its 0.9 changes touch, the `sqlx` ignore in `.github/dependabot.yml`,
`CHANGELOG.md`, and the handoff.

`sqlx` 0.9.0 (stable, 2026-05-21) declares `rust-version = 1.94.0`; the floor here is 1.89. The
user's rule: the newest *stable* line of the dependency, on the *lowest* Rust that supports it, so
the floor never runs ahead of what the rest of the graph is known to build on.

**Tasks:** install the 1.94 toolchain (`rustup toolchain install 1.94`), read `sqlx`'s 0.9 changelog
and migration notes, bump the workspace dependency, fix what breaks (SQLite pool options, `query!`
macros, `Error` variants, `FromRow` derives — whatever 0.9 renamed), and prove the whole workspace
builds `--locked` on 1.94 exactly as the MSRV job will:
`RUSTUP_TOOLCHAIN=1.94.x cargo build --workspace --locked`. Then move every floor reference listed
above in the same commit, delete the Dependabot ignore, and update the MSRV job's comment to say
which dependency now sets the floor. Run the full suite on the pinned 1.98 toolchain as well.

**Acceptance:** `cargo build --workspace --locked` succeeds under `RUSTUP_TOOLCHAIN=1.94.x` and fails
under 1.89 (show the error, so the new floor is proven real, not nominal); `cargo nextest run
--workspace` green; `.githooks/pre-push` green; no `1.89` remains anywhere but `TODO.md`,
`CHANGELOG.md` and the handoffs (`grep -rn "1\.89" --exclude-dir=target --exclude-dir=node_modules`).
The handoff names every `sqlx` API that changed and what it was replaced with.

**Stop if:** `sqlx` 0.9 needs a migration-runner change (one `ALTER` per migration, no wrapping
transaction — see `CLAUDE.md`) or changes SQLite's journal/WAL behaviour; say what and stop.

---

### VI-C40 — CI moves to Node 22 LTS, and `jsdom` 30 follows

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C40.md`. **Needs nothing.** Wave 17. **Decided by the user 2026-09-07: Node 22 LTS.**

**Owns:** every `node-version` in `.github/workflows/{ci,release,scheduled-audit}.yml`, the `jsdom`
ignore in `.github/dependabot.yml`, `jsdom` in `frontend/package.json` and the lockfile,
`CONTRIBUTING.md`'s prerequisites row if it names Node, and the handoff.

`jsdom` 30 needs Node `^22.22.2 || ^24.15.0 || >=26`; CI pins `20` in eight places. The user chose
22 LTS: the current maintenance line, not the newest major.

**Acceptance:** `npm run type-check`, `npx vitest run` and the e2e suite green on Node 22.22+;
Dependabot's ignore for `jsdom` gone; CI green on the card branch. The handoff records the Node
version the local machine runs (22.17.1 today, below `jsdom` 30's floor) and what that means for
a developer who has not upgraded.

---

### VI-C41 — A harness installed where the launcher's `PATH` cannot see it reads as "not installed"

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C41.md` (integrator amendment: empty `PATH` entries are skipped, never searched as the working directory). **Needs nothing.** Wave 18. Runs alongside VI-C42 and VI-C43.

**Owns:** `crates/tack-runner/src/harness/locate.rs` (new — the one binary search both
adapters share), `discover_installed_binary` in `crates/tack-runner/src/harness/claude_code.rs`,
the `CodexLocator::Search` arm, `system_path_dirs` and `locate_in_dirs` in
`crates/tack-runner/src/harness/codex.rs`, their tests, the "not installed" guidance in
`docs/book/src/user-guide/agent-runners.md`, `CHANGELOG.md` `[Unreleased]` (one line), and the
handoff. **Not** `crates/tack-cli/src/doctor.rs` (VI-C42 is editing it beside you), not
`bootstrap.rs`, not the wire contract, not the frontend.

**Context.** Both adapters resolve their binary from the runner process's own `PATH`,
snapshotted once at discovery, each with a private copy of the same loop. That is the right
source when the runner is started from a shell and the wrong one for the two ways Part VII
starts it: the desktop app launched from a `.desktop` entry, Finder or the Start menu, and
`tack service` under systemd's user manager. Both inherit the session's minimal `PATH`, which
on most machines does not contain where `claude` and `codex` are actually installed — nvm
(`~/.nvm/versions/node/<v>/bin`), npm's global prefix (`~/.npm-global/bin`, `~/.npm/bin`),
`~/.local/bin`, `~/.cargo/bin`, `~/.bun/bin`, Homebrew (`/opt/homebrew/bin`, `/usr/local/bin`),
and `%APPDATA%\npm` on Windows. VII-D1's transcript recorded this as the reason the stranger's
walk needed a fake shim: "the Agents page only detects binaries on `PATH`". The page then tells
that user the harness is not installed, and the only fix is one a UI-only user cannot know:
start the app from a terminal.

Two fixes were weighed and this card takes the first. (1) The runner searches `PATH` first and
then a fixed, documented list of well-known per-user install directories, and its "not found"
error names everything it searched. (2) The desktop app asks the login shell for its `PATH`
(`$SHELL -lc 'echo $PATH'`, what VS Code does). (2) fixes only the app, runs the user's rc
files with their side effects, and does nothing for `tack service`; (1) fixes both entry
points with one function and no subprocess. The list is fixed in code, not configurable — a
`TACK_*` knob for it would be a flag whose off-state nothing exercises.

**Tasks.**
- One locator, `harness/locate.rs`: `fn locate(program: &str, path: Option<&OsStr>, home:
  Option<&Path>) -> Result<PathBuf, NotFound>`, pure over its arguments so tests never touch
  the process environment (`set_var` is `unsafe` in Rust 2024 and this crate's tests run in
  parallel). `NotFound` carries the directories it searched; its `Display` is what both
  adapters put in `probe_error`. The Unix executable-bit check stays; symlinks are
  canonicalized as `claude_code.rs` already does (nvm installs are symlinks). Order: `PATH`
  entries, then the well-known list, first hit wins — a machine whose shell can see one
  install behaves exactly as before.
- The well-known list lives in one place, each entry with a comment naming the installer
  that puts a binary there; on Unix derived from `home`, on Windows from `APPDATA` /
  `LOCALAPPDATA`; nothing panics when `home` is `None`.
- Both adapters call it and delete their private copies. One `info` line at discovery with
  the resolved path (a path, not a secret).
- Tests, every temp path from a `tempfile` guard: found on `PATH` wins over a fallback dir;
  found only under `<tmp home>/.local/bin` with an empty `PATH`; not found → the error names
  every directory searched; a non-executable file in a fallback dir is skipped.
- `agent-runners.md`'s "not installed" guidance says where Tack looks, in the user's words.

**Acceptance:** a fake `claude` under `<tmp home>/.local/bin` with an empty `PATH` is
discovered and version-probed in both adapters' tests; `probe_error` for an absent binary lists
the searched directories; `grep -rn 'split_paths\|var_os("PATH")' crates/tack-runner/src/harness/`
shows exactly the one locator; `docs/contracts/runner-v1/**` byte-identical; `cargo nextest run
--workspace -E 'binary(runner_contract)'` and the whole suite green; `.githooks/pre-push` green.

**Stop if:** sharing the locator requires changing a type in `harness/mod.rs` that
`bootstrap.rs` or `doctor.rs` construct — write the required change into the handoff and leave
those files alone.

---

### VI-C42 — The embedded runner's boot says what it is waiting on when the platform secret store never answers

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C42.md`. Landed with a 3 s bound rather than the card's 5 s, and `doctor` prints the bound and the backend on every run instead of a separate fallback reason; the timeout's reason reaches the log's existing `warn` line. **Needs nothing.** Wave 18. Runs alongside VI-C41 and VI-C43.

**Owns:** `SecretStore::open` and `platform_store` in `crates/tack-runner/src/secrets.rs` (the
signature of `open` stays — `bootstrap.rs` and `local_runner.rs` call it and are not yours),
the backend line `tack runner doctor` prints in `crates/tack-cli/src/doctor.rs`, the
secret-store rows of `docs/CONFIG.md` if they describe the fallback, `CHANGELOG.md`
`[Unreleased]` (one line), and the handoff. **Not** the harness files (VI-C41's), not
`bootstrap.rs`, not `local_runner.rs`.

**Context.** `SecretStore::open` tries the platform credential store and falls back to the
owner-only file only when that attempt returns an error. On Linux the attempt is
`zbus_secret_service_keyring_store::Store::new()`, a D-Bus call that blocks for as long as
activation takes — and a broken or absent Secret Service under a systemd user session, a
container or a headless CI host can make that indefinite. VI-C32's handoff, "Known
limitations": the embedded runner then never reaches `active`, `GET /api/local-runner` reports
`starting` forever, and nothing in the log says why. VI-C32 added liveness backstops around the
boot and named this one as outside its scope. Read that section of `VI-C32.md` only.

ADR 0061 decision 1: keychain first, file where *none answers*. A store that has not answered
within a bound is one that did not answer; the fallback is the one an error already takes, and
`doctor` reports it the same way. The split brain this permits — a key stored in the file on one
boot and a keychain that answers on the next — already exists for the error path and is not
new; the handoff states it and `doctor` makes it visible.

**Tasks.**
- Run the platform-store attempt on its own thread and wait on it with a bound (a constant,
  5 s, with a comment; not a `TACK_*` knob). On timeout: `warn!` with `backend = "file"` and a
  `reason` naming the bound, then the file fallback. The blocked thread is detached and the
  comment says why: it cannot be cancelled, it holds nothing, and a late answer is dropped.
- Make the attempt injectable for tests without touching the process environment —
  `open_with(probe, bound, fallback_path)` with `open` calling it, following the pattern
  `with_store` already sets. `SecretStore` records why it fell back: `fallback_reason() ->
  Option<&str>` beside `backend()`, which stays as it is.
- `tack runner doctor`'s backend line says *why* the file backend was chosen when it was a
  timeout rather than an error.
- Tests: a probe that never returns yields a `File` store within the bound (elapsed measured,
  well under 2× the bound) and the caller continues; a probe that returns a mock store yields
  `Keychain` and the existing `with_store` tests stay byte-identical; the warn line carries the
  backend and the reason and never a value.

**Acceptance:** the timeout test passes and, with the bound removed, exceeds nextest's
slow-test limit (proven once and recorded in the handoff); `doctor` output shows the reason;
`grep -n "pub fn open" crates/tack-runner/src/secrets.rs` shows the signature unchanged; whole
suite green; `.githooks/pre-push` green.

**Stop if:** the platform store's `new()` cannot be run on another thread (the returned store is
`Arc<dyn CredentialStoreApi + Send + Sync>`, so it should) — record the type that blocks it and
stop; the bound then belongs at the caller in `bootstrap.rs`, which is not yours.

---

### VI-C43 — The fleet-member routes get their caller: a roster you can read, then edit

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C43.md`. **Needs nothing.** Wave 18. Runs alongside VI-C41 and VI-C42 — the only card of this wave that
uses ports 3399/5199.

**Owns:** `frontend/src/features/agents/runnerFleet/FleetsPanel.tsx` and its test, `fleetsApi`
in `frontend/src/shared/execution/api.ts` and its test (additions only), any E2E mock that
answers `/runner-fleets` and now needs the member routes, the fleet paragraph of
`docs/book/src/user-guide/agent-runners.md` if it says membership is API-only, `CHANGELOG.md`
`[Unreleased]` (one line), and the handoff. **No backend change and no new endpoint.**

**Context.** `POST /api/runner-fleets/{fleet_id}/members` and its `DELETE` have existed since
migration 041 and have no caller in `frontend/src` — VI-D1 re-verified it and escalated it, and
`.claude/scope-discipline.md` names exactly this shape of defect. `FleetsPanel.tsx`'s header
explains why it stopped: no endpoint returns a fleet's roster, so an editor would have nothing
to read back. That premise is half true. `GET /api/runners` returns every runner with its
`fleet_ids` — the same table from the other side — and `runnersApi.list()` already wraps it. A
roster per fleet is a filter over that list; it is a real read-back, and it makes the editor
honest.

**Tasks.**
- `fleetsApi.addMember(fleetId, runnerId)` and `removeMember(fleetId, runnerId)` calling the two
  routes, typed from `schema.gen.ts` (`FleetMemberResponse.state` is `added` /
  `already_member` / `removed`), with the unit tests `api.test.ts` gives `list` and `create`.
- `FleetsPanel` shows, under each fleet, its members derived from `runnersApi.list()` — the
  name and health chip the Agents page already renders for a runner, reused not restyled — a
  select of runners not yet in the fleet with *Add*, and *Remove* on each member. After a
  write the panel refetches the runners list and renders the server's roster, never an
  optimistic client-side list. `already_member` is a notice, not an error. An empty fleet
  says so in plain words.
- Delete the header paragraph that states the gap; write what the panel does now.
- Vitest: add calls `POST` with the right body and refetches; remove calls `DELETE`; the roster
  is derived from `fleet_ids`; a runner in two fleets appears under both.
- Colors from `--color-*` tokens only.

**Acceptance:** `grep -rn "/members" frontend/src --include='*.ts' --include='*.tsx' | grep -v
test` finds the two callers; `npm run type-check` and `npx vitest run` green; `npx playwright
test agents-page a11y --project=chromium --workers=2` green headless (the panel changes the
Agents page's DOM, and the a11y spec scans it); `.githooks/pre-push` green.

**Stop if:** `grep -n "fleet_ids" frontend/src/shared/api/schema.gen.ts` is empty — then the
roster has no read path, the card is a backend card, and you write that and stop.

---

## §VI.5 Definition of done, and deliberate exclusions

| Claim | Proof |
|---|---|
| A UI-only user who started `tack serve` on their machine and has a Vercel AI Gateway key reaches a completed attempt without touching a terminal again | VI-D1 stranger transcript |
| Turning agent execution on is one click on a loopback bind, and impossible on any other | VI-B3 |
| The README's first screen shows two components — a diagram before any PM screenshot — and a stranger reading it reports two parts, what each holds, and one board / many runners | VI-A3 acceptance test, re-run by VI-D1 |
| The hero asset shows an agent executing an item, and the recovery recording sits beside the paragraph it proves | VI-D2, with V-C2's slot untouched |
| A UI-only user with a harness's own login sees every console step inside the flow and never a documentation link | VI-C1 E2E |
| A provider key exists in exactly one place — the runner's owner-only store — and in no table, log or response | VI-B2 / VI-B3 absence tests with positive controls |
| A project default model is honoured with `project` provenance and shown as such in the modal | VI-C3 `model_policy_test.rs` + VI-C2 E2E |
| Nothing is green without an observation | VI-C1 |
| One page answers "item → attempt" and "model → provider → key", and it is the same page | VI-A1, amended by VI-D1 |
| Docket untouched; `runner_contract` byte-identical unless VI-B2 escalated and the integrator accepted a revision | every card's gate |

**Deliberately not in this Part**, recorded so no card adopts them by drift:

- **Any provider beyond Vercel AI Gateway.** OpenRouter, direct Anthropic / OpenAI keys and
  local endpoints stay in the harness's own configuration. B1's store plus
  `secret_reference` already lets a request carry any key into a harness's environment;
  a second gateway earns its own config section only when it measurably differs from this
  one (scope-discipline rule 3).
- **Tack calling any model API itself**, including "just to validate a key" — the runner's
  probe does that.
- **A harness-login broker.** Vendor OAuth flows stay in the terminal, rendered in the UI.
- **Multi-user accounts** (ADR 0059) and **anything docket** (ADR 0060).
- **Notifications, i18n, time tracking, in-UI diff review** — still deferred from Part V.
  C4 lists artifacts; reading a diff in the UI is a separate, later card.
- **Exercising the `decisions` path with a real harness** — still no harness in this tree
  asks a mid-run question; C4 lists decisions, it does not manufacture one.
- **Recorded, not decided:** whether bare `tack` (no subcommand) should mean
  `serve --with-runner` on a loopback bind. The recommendation is no — the UI switch (ADR
  0061 decision 6) removes the click's cost without changing a security default. Reopen only
  with evidence that the switch is not enough.

---

## §VI.6 Handoff additions for this Part

Use `docs/agent-handoffs/part-vi/TEMPLATE.md` — it carries the §III.2 template, Part V's
three sections (claim → evidence table, measured numbers, what a stranger still cannot do),
the three below, and a **Context spent** section (cold-start tokens against the dispatch
block's estimate, context at handoff, files opened and not used). Never open the archive
for the template. The three sections specific to this Part:

1. **Surface-map delta.** Which rows of the §VI.0 table your card moved from console to UI,
   or proved cannot move — with the reason, and whether that reason is already in the
   table's last column. If it is not, that is an escalation for ADR 0061, not a new row.
2. **Secret-path proof** (B1, B2, B3, D1 only). The exact commands showing the key in the
   runner's store and nowhere else: the `sqlite3 … | grep -c`, the log capture, the `stat`.
   A handoff for one of these cards without this section is incomplete.
3. **Vocabulary check** (A3, C1, C2, D1, D2). The grep for §VI.1 rule 8's words over what
   your card rendered or wrote, with every hit and why it is under *Advanced* or in the
   developer book. The README is allowed the word "runner" — it is telling the story.

---

# Part V — Adoption & First Public Release (Phase 59)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Adoption & First Public
Release*, opened by the adoption audit of **2026-08-30**. This Part has its own numbering
namespace (`§V.0` … `§V.6`) so the archive's load-bearing numbers stay put.

Like Parts III and IV, this board is written to be picked up cold by parallel agents in
isolated worktrees. Every card is bounded, names every shared-file owner, and has an
acceptance gate verifiable without trusting its author's handoff.

## Status board — Part V

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 11 — Release blockers | V-A1 · V-A2 · V-A3 · V-A4 | 59 | **Done, all four integrated** at `45416e7` on `develop` (handoffs: `docs/agent-handoffs/part-v/V-A1.md` … `V-A4.md`). Local-only artifacts awaiting explicit user approval to publish: `main` branch (V-A1), tag `v0.1.0-beta.7` (V-A3), GitHub description/topics/Pages (V-A4) — see each handoff's Next step. |
| 12 — Honest posture & prune | V-B1 · V-B2 | 59 | **Done, both integrated** at `990349f` on `develop` (handoffs: `docs/agent-handoffs/part-v/V-B1.md`, `V-B2.md`). V-B1: ADR 0059 (single-operator), non-loopback+no-token startup refusal now has an explicit opt-out (`TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`); README paragraph proposed in the handoff, not yet merged (V-A4 already shipped this cycle). V-B2: ADR 0060 decides **keep** docket as a maintained optional bridge (not gate/delete), backed by measured numbers; no code/schema change shipped. |
| 13 — Distribution & launch | V-C1 · V-C2 · V-C3 · V-C4 | 59 | **V-C4 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C4.md`). *Verify install URLs* now runs `install.sh` for real into a scratch directory and asserts a runnable `tack` lands; the integrator re-ran its revert-once proof independently — with the `grep -v` reverted it downloads 2.14 MB and fails with `no 'tack' binary found in archive`. E2E's budget went 25 → 40 minutes on measured numbers (3.45 overhead + 8.40 chromium + 7.00 firefox, webkit unmeasurable on this machine and recorded as such, not as a webkit fact). **That will turn a cancellation into an honest failure, which is the point:** `scheduler-e2e.spec.ts`'s four tests fail deterministically on VI-C9's gate. **Bring the budget back down once VI-C9 lands** — about twelve of those minutes are the failing tests. Original note follows. | **V-C3 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C3.md`) — and it found the launch's own blocker while walking the stranger's path instead of reading it: **`install.sh` had installed nothing since `v0.1.0-beta.7`.** Two archives per release end in the same platform suffix, the releases API lists `tack-runner-…` first, so the advertised one-liner downloaded the runner archive and died on `no 'tack' binary found in archive`. One `grep -v`; verified against the live API, broken before and right after. Everything else it prepared is unpublished by design. Its amendment corrects three of its own claims after review. Two green-but-blind CI checks it surfaced are now **V-C4**. Original note follows. | V-C1 **done, integrated** at `135b941` (handoff: `docs/agent-handoffs/part-v/V-C1.md`) — Homebrew/AUR/Nix/ghcr.io recipes + cargo-binstall metadata, all four verified with real installs in disposable containers; found and fixed a pre-existing `Dockerfile` toolchain-ordering bug along the way. **V-C2 is now unblocked** — Part IV Wave 10 (all six cards, including IV-A4 zero-touch enrollment) and V-A2 are both done, so the demo can now be recorded as one command (`tack serve --with-runner`) with no hang risk. **V-C2 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C2.md`) — `docs/screenshots/recovery-demo.gif`, an attempt reaching *Needs operator*, an operator decision, and the same attempt reaching *Succeeded*, re-recordable with `scripts/record-recovery-demo.sh` against a published artifact in Docker. **That unblocks V-C3, dispatched 2026-09-06, and Part VI's VI-D2.** V-C2's escalation carries a dated amendment worth reading before trusting any card that measures a release artifact: recording from a published tag measures a *past* tree, and two of its three original product observations were already false on `develop` by the time it wrote them. The surviving one is carded as **VI-C7**. |

**Integration line:** `develop`, the repository's default branch — same as Parts III and IV,
and for the same reason. Branch every card from `develop`. Do not create a `plan/*` line.

---

## §V.0 Cold-start context capsule

**What this Part is for, in one sentence.** Tack has been a public repository since
2026-03-15 and has zero stars, zero forks, zero human issues and one human contributor;
this Part closes the distance between "it works on this machine" and "a stranger can use
it".

**Read that number correctly.** It is not a verdict on the code. This repository holds
~122k lines of Rust across six crates, ~45k lines of SolidJS, 1380 passing workspace tests,
57 migrations, a 92-path documented API, a ~113 ms cold start at ~11.7 MiB RSS, and durable
execution semantics — fencing tokens, leases, replay tables, recovery audits — that **no
competitor in its category has**. It is a verdict on the fact that the project has never
been released, published, positioned or shown to anybody. Every card here is about
distribution, proof and truthfulness. **None of them adds a product feature.**

### Evidence base, measured 2026-08-30

Do not re-derive these; act on them. Each row names the command that produced it, so any
card can re-check one cheaply.

| Fact | Value | How it was checked |
|---|---|---|
| README's headline install one-liner | **HTTP 404** | `curl -sI https://raw.githubusercontent.com/yielab/tack/main/install.sh` |
| branch `main` | **does not exist**; default branch is `develop` | `git ls-remote --heads origin main` → empty |
| stars / forks / human issues / human contributors | **0 / 0 / 0 / 1** | `gh repo view`, `gh issue list --state all`, `git shortlog -sn --all` |
| only release | `v0.1.0-beta.6`, **2026-06-22**, four `tack-*` archives, **no runner archive** | `gh release view v0.1.0-beta.6 --json assets` |
| `release.yml` packages `tack-runner` | **yes, already** — since `7d78de3`, 2026-08-19 | the workflow's `RUNNER_STAGE` block |
| mdBook published | **no** — `yielab.github.io/tack` returns 404 | curl |
| repo `homepageUrl` / topics | **empty** | `gh repo view` |
| live smoke | **`SMOKE FAILED`**, 2026-08-26 | Part III Wave 9 amendment, in the archive below |
| identity model | **none** — one shared bearer token; no token configured = allow-all | `crates/tack-api/src/middleware.rs::require_token` |
| docket legacy surface still in the tree | 9 `orch_*` tables + `control_planes` (10 Docket-specific), an Approvals page, a ControlPlanesManager | migrate a fresh `sqlite::memory:` with `tack_db::migrations::run_all()`, then `SELECT name FROM sqlite_master WHERE type='table'`. **The grep this row used to name counted 11** by matching the `orch_runs_new`/`orch_approvals_new` staging names in the migration source, which no migrated database ever contains |

**The single most embarrassing line in that table is the first one.** The install command
printed in the README of a public repository has never worked, because the branch it names
was never created. That is V-A1, and it is a few minutes of work that has been costing
every visitor since March.

**A correction the audit made to itself, so no card repeats it.** The first reading of the
release gap was "CI does not package the runner". That is **false** — `release.yml` has
built and packaged `tack-runner` into its own per-platform archive since `7d78de3`
(2026-08-19). The gap is that **no tag has been cut since**, so the only downloadable
release predates every Phase 50–58 capability. V-A3 cuts a tag; it does not fix CI.

### Competitive context, and why the timing is the whole argument

The nearest thing to Tack that ever existed — **Vibe Kanban**, a kanban board that
orchestrates Claude Code / Codex / Gemini agents — **shut down on 2026-04-10** when its
company (Bloop) closed. Its own farewell post says it had thousands of engineers using it
daily and never found a business model. **Crystal**, the other popular open-source
orchestrator in that space, **was deprecated in February 2026**.

What remains above Tack in that category is either closed-source (Conductor — macOS-only,
$22M Series A; Sculptor — Claude-only, by Imbue) or is not a board at all (OpenHands,
Emdash). Meanwhile the mature open-source project managers — Plane (~54.6k ★), Huly
(~26.9k ★), Focalboard (~26.3k ★), Leantime (~9.4k ★), Vikunja (~3.8–5k ★) — **execute
nothing**.

Tack is the only thing that is both. **There is a real, identifiable, currently-orphaned
audience**, and it is being collected by Emdash (YC W26) while this board sits unstarted.
That is the adoption thesis, and it has an expiry date.

The category's history also carries a warning worth stating once: it killed its leader
through failure to monetize, not failure to be useful. For a project with no company
behind it that is an advantage, and the positioning card (V-A4) should not shy from it.

### Working-tree state at the time this board was written (2026-08-30)

`develop` is at `277868a`, and the tree is **not clean**:

- `crates/tack-runner/src/harness/mod.rs` — the uncommitted
  `registering_all_three_real_adapters_is_order_independent` fix inherited from Part IV
  §IV.0. **Land or discard it deliberately before branching any card**; do not let it ride
  into a card's diff unexamined. It is the same change Part IV's capsule describes; the two
  Parts must not both claim it. **Landed** at `f4941f0`, before any Wave 11 card branched —
  the fix matched a real `codex` binary now being installed on this machine.
- `docs/adr/0058-standalone-single-binary-runner.md`, `docs/agent-handoffs/part-iv/` —
  untracked Part IV material.
- `tack.db.before-037_orch_runs_rebuild.sqlite` — an untracked database snapshot sitting in
  the repository root. It is not a card, but it should not be there when a stranger clones.
  Whoever lands V-A4 removes it or gitignores it.

---

## §V.1 Rules for simultaneous agents

**All fourteen rules of §III.2 apply unchanged** — one card / one worktree / one branch,
stay inside `Owns`, no `unimplemented!()` or hidden fake success, tests ship with the card,
no blocking sleeps, logs carry ids and never credentials, stop on contract ambiguity, and
each wave ends with adversarial verification by someone who did not author the code. Read
them in the archive; they are not restated here.

Five rules are specific to this Part, and they exist because this Part's output is read by
strangers rather than by the team:

1. **No claim ships that this repository cannot demonstrate on a clean machine.** The
   standing failure mode here is documentation that describes the *intended* product rather
   than the delivered one — the README currently implies three working harnesses when
   `codex` has never completed a live attempt. A card that cannot prove a claim deletes the
   claim; it does not soften the wording.
2. **Do not fix a documentation gap by weakening the product, and do not fix a product gap
   by rewording the documentation.** If V-A2 cannot make the live smoke pass, V-A4 changes
   what the README claims. If V-A4 finds a claim it wants to keep, it asks V-A2 to prove
   it. Neither card resolves the tension alone.
3. **Removal needs a decision record, not a preference.** Anything this Part gates, hides or
   proposes deleting gets an ADR in `docs/adr/` naming the option chosen and the options
   rejected, with the measured cost of each. "It felt like dead weight" is not a reason this
   repository accepts.
4. **Measure, never estimate.** Every number that reaches a handoff, a doc or a release note
   arrives with the command that produced it. Binary sizes, install times, test counts,
   surface counts — all of them. This rule already exists in Part IV for binary size; here
   it covers every number.
5. **Nothing outward-facing is published without the user's explicit approval.** Creating a
   git tag, pushing a release, publishing a Pages site, opening a package-manager PR, or
   posting anywhere are all outward-facing. Cards prepare them and stop. V-C3 is entirely a
   preparation card for this reason.

---

## §V.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `install.sh`, the `main`-branch decision, the doc-URL CI check | V-A1 only |
| `scripts/smoke.sh` | V-A2 — **and IV-A6 in Part IV. Real conflict, see §V.3** |
| `.github/workflows/release.yml`, release notes, the tag itself | V-A3 only |
| `README.md` | V-A4 — **and IV-A6 in Part IV. Real conflict, see §V.3** |
| GitHub repo description / topics / `homepageUrl`, the Pages workflow, `docs/book/src/introduction.md` | V-A4 only |
| `crates/tack-api/src/middleware.rs`, `docs/book/src/user-guide/administration.md` | V-B1 only |
| `crates/tack-orch/src/adapters/**` gating, `frontend/src/features/approvals/**`, the control-planes UI | V-B2 only |
| `docs/CONFIG.md` | V-B1 for the auth rows, V-B2 for the docket rows — **disjoint sections, coordinate in handoffs**; and IV-A6 in Part IV |
| `packaging/**`, brew/AUR/Nix recipes, `cargo-binstall` metadata in `Cargo.toml` | V-C1 only |
| `docs/screenshots/**`, the demo asset | V-C2 only |
| `CHANGELOG.md` | V-A3 for the release section; every other card proposes text in its handoff |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |
| `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json` | **nobody — out of scope for this Part** |

---

## §V.3 Dependency graph, cross-Part conflicts and merge policy

```text
V-A1  (install path) ──────────────────┐
                                       │
V-A2  (smoke tells the truth) ──┬──────┼── V-A3  (cut the tag) ──┐
                                │      │                         │
                                └──────┴── V-A4  (positioning) ──┼── V-C1 (distribution) ──┐
                                                                 │                         │
V-B1  (identity posture) ────────────────────────────────────────┘                         ├── V-C3 (launch prep)
                                                                                           │
V-B2  (docket decision) ─────────────────────────────────────────  V-C2 (demo) ────────────┘
                                                                        ▲
                                        Part IV Wave 10 ────────────────┘
```

**Wave 11 parallelism.** V-A1 is independent of everything and should land first — it is the
cheapest fix on the board with the largest per-visitor cost. V-A2 is the long pole and
should start at the same time. V-A3 and V-A4 both consume V-A2's verdict about what is
actually provable, so they merge after it.

**Wave 12 is not blocked by Wave 11** and can run concurrently. Only V-B1's README paragraph
depends on V-A4 owning that file, and it is handed over as proposed text rather than edited.

### Two genuine cross-Part conflicts — read before branching

Part IV's §IV.2 assigns `scripts/smoke.sh`, `README.md` and `docs/CONFIG.md` to **IV-A6**.
Part V assigns the first two to **V-A2** and **V-A4**. This is a real collision, not an
oversight, and the resolution rule is:

1. **`scripts/smoke.sh` — V-A2 goes first, IV-A6 rebases onto it.** IV-A6 adds a standalone
   `--with-runner` step to the smoke; V-A2 fixes a smoke that is currently reporting a
   misleading cause for its own failure. Adding a step to a script that lies is strictly
   worse than fixing the lie first. If Part IV Wave 10 is already in flight when V-A2
   starts, **V-A2 escalates to the integrator rather than editing** — it does not race.
2. **`README.md` — V-A4 goes last and takes the merge.** IV-A6 documents `--with-runner`;
   V-A4 restructures the whole file. Whichever lands second resolves, and V-A4 is written to
   expect inbound Part IV text.
3. **`docs/CONFIG.md` — disjoint sections, no ordering constraint.** V-B1 owns the auth rows,
   V-B2 the docket rows, IV-A6 the local-runner rows. Each states in its handoff which rows
   it touched so the integrator can verify the disjointness rather than assume it.

A card that discovers a third collision states it in the handoff and stops. It does not
choose a winner.

---

## §V.4 Cards

### V-A1 — The install path a stranger actually takes

**The smallest card on this board and the most overdue.** Its risk is scope creep into V-A4.

**Owns:** `install.sh`, the install section of `README.md` (**only** that section — the
restructure is V-A4's), the `main`-branch decision, a new CI check, and the V-A1 handoff.
**Does not own** the README's positioning, the release workflow, or any crate.

**Context — the exact shape of the problem.** `README.md:166` prints, as the headline
install method for a public project:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

There is no `main` branch. `git ls-remote --heads origin main` returns empty; the repository's
default branch is `develop`. The URL returns **404**, and has since the repository was made
public on 2026-03-15. The same file served from `develop` returns 200, so the script itself
is fine — only the path is wrong. `cargo install --git` happens to work because cargo follows
the default branch, which is why this was never noticed locally.

**Tasks:**
- Decide, and record the reasoning in the handoff: (a) create `main` as a branch tracking
  released state, or (b) point every advertised URL at `develop`. **The recommendation is
  (a)** — a stranger's `curl | sh` should not follow the development tip, and every other
  project's convention says `main` exists. But this is the card's call to make and defend,
  because (b) is one line and (a) adds a branch to keep in sync.
- Fix every URL in `README.md` and `docs/` that names a branch, not just the one that was
  found. `grep -rn 'githubusercontent.com/yielab/tack' . --include='*.md' --include='*.sh'`.
- **The card's real deliverable:** a CI job that resolves every install URL the docs
  advertise and fails the build on any non-200. Without it this rots again the next time the
  branch layout changes, which is exactly how it broke the first time.

**Acceptance:** on a machine with no checkout of this repository, the exact command printed
in the README installs a `tack` that starts and serves the UI — **proven by running it in a
clean container and pasting the transcript**, not by reading the URL. The new CI check is
proven load-bearing by pointing one documented URL at a bad path once, watching CI go red,
and reverting. `git ls-remote --heads origin` output is recorded in the handoff before and
after, so the branch decision is visible.

---

### V-A2 — Make the live smoke tell the truth

**The long pole of this Part, and the only card here that may touch runtime code.** It is
also the card most likely to discover that the honest answer is "this does not work yet".
That is an acceptable outcome; hiding it is not.

**Owns:** `scripts/smoke.sh`, plus whatever narrow runner-side fix the diagnosis lands on
(`crates/tack-runner/**`), and the V-A2 handoff. **Does not own** the contract directory,
the scheduler, migrations, `router.rs` or the frontend. A diagnosis that points at any of
those is an escalation with evidence, not an edit.

**Context.** On 2026-08-26, with all three harnesses installed for the first time
(`codex-cli 0.149.1`, `claude 2.1.236`, `opencode 1.18.0`) and both binaries rebuilt in
release, `./scripts/smoke.sh --live` returned **`SMOKE FAILED`**. Steps 1–6 and 9 passed,
including the complete restart-recovery proof (SIGKILL mid-attempt → `needs_operator` → no
blind duplicate → operator requeue → attempt #2 success) and capacity-1 saturation. Two
steps failed:

- **Step 7** — the live attempt never reached a terminal state. The `opencode` +
  `llamacpp/qwen3.6-35b-uncensored` pairing was claimed and checked out at the exact
  requested commit with an isolated workspace, then produced `attempt ended '' —
  terminal_reason: null` after the 300 s budget.
- **Step 8** — all three harness kinds FAILed, printing canned text that the board had
  **already recorded as stale on 2026-08-20** and never reworded. It names a pre-III-H5
  structural cause that no longer applies, and it has now misled two separate readers into
  seeing a regression that is not there.

**The leading hypothesis is explicitly unverified.** The runner is enrolled at capacity 1;
step 7's attempt never terminated, so its lease was very likely still held when step 8
created three more requests — under which none could be claimed by anyone, which is exactly
the symptom all three printed. That would make step 8 a **cascade of step 7**, not three
independent defects. Nobody has re-run step 8 against an idle runner, and the runner-side
log was not preserved (`SMOKE_KEEP=1` was not set).

**Tasks — the four open questions from the Wave 9 amendment, each answered or explicitly
re-opened with what was ruled out:**
1. Re-run with `SMOKE_KEEP=1` and preserve the runner log and journal before anything else.
   The previous run's evidence was lost; do not lose this one's.
2. **Why the live attempt hangs.** Try a lighter declared pairing first — `opencode/big-pickle`,
   `hy3-free` and `mimo-v2.5-free` were all declared — to separate "the product hangs" from
   "the model was too slow for a 300 s budget". **Note carefully:** even if the cause is a
   slow model, an attempt that exceeds its budget and reports `terminal_reason: null` is a
   product defect. A budget expiry must produce a terminal state. Fix that regardless of
   which model was chosen.
3. **Whether step 8 survives an idle runner.** If the three failures vanish, the smoke
   saturates its own capacity-1 runner and then reports scheduling failures — in which case
   the defect is in the smoke, and the handoff says so plainly.
4. Reword step 8's canned FAIL text so it names the cause it actually observed.

**Acceptance:** `./scripts/smoke.sh --live` on a three-harness machine either passes end to
end, or fails with a message that names the real cause and is proven to do so by inducing
that cause deliberately. Each of the four questions above is answered in the handoff with
preserved evidence, or re-opened with what was eliminated. Any runner-side fix is proven
load-bearing by reverting it once and watching the smoke fail again. **Codex is reported
honestly**: it has never completed a live attempt, and if this card does not change that,
the handoff says so in those words and hands V-A4 the instruction to stop implying
otherwise.

---

### V-A3 — Cut a release that contains the product

**Owns:** `.github/workflows/release.yml`, the `[Unreleased]` → release section of
`CHANGELOG.md`, the tag, and the V-A3 handoff. **Needs V-A2's verdict** — the release notes
must not claim what the smoke could not show.

**Context — read this before touching CI.** The audit's first reading was that CI does not
package the runner. **That is false.** `release.yml` has built `tack-runner` with
`cargo auditable` and packaged it into its own per-platform archive (`RUNNER_STAGE`, with
the systemd unit and env example) since `7d78de3`, 2026-08-19. **This card does not need to
fix the workflow.** The gap is purely that no tag has been cut since: the only downloadable
release is `v0.1.0-beta.6` from 2026-06-22, whose four assets are `tack-*` only and which
predates every Phase 50–58 capability. A visitor who follows the README today downloads a
Tack with no runner fleet at all — the entire differentiator is undownloadable.

**Tasks:**
- Verify the workflow's artifact set on a dry run before tagging — that both `tack-*` and
  `tack-runner-*` archives are produced for all four platform targets, and that checksums
  and build provenance are unchanged.
- Cut `v0.1.0-beta.7` from `develop`. **Preparation only — the user approves the tag push**
  (§V.1 rule 5).
- Write release notes that state exactly what is proven and name what is not. If V-A2 could
  not prove a codex live attempt, the notes say two of three harnesses are live-proven. The
  notes are the first thing a stranger reads about the state of this project; §V.1 rule 1
  applies to them more than to anything else on this board.

**Acceptance:** the produced release carries both a `tack-*` and a `tack-runner-*` archive
per platform. A downloaded pair, on a machine with no repository checkout, enrolls a runner
against a served instance and completes an attempt — **or the notes state that it cannot and
why**, with the transcript in the handoff either way. No claim in the notes lacks a
corresponding proof in V-A2's or this card's evidence.

---

### V-A4 — Say what Tack is in one sentence, and publish it where it can be read

**Owns:** `README.md` (whole file — takes the merge against IV-A6, see §V.3), the GitHub repo
description / topics / `homepageUrl`, a new GitHub Pages workflow for the mdBook,
`docs/book/src/introduction.md`, and the V-A4 handoff. **Does not own** any crate, the
release workflow, or `docs/CONFIG.md`.

**Context.** The repository's GitHub description reads *"Self-hosted project manager in a
single binary — no Docker, no database server. Rust + SolidJS."* It does not mention agents.
That sentence places Tack in the category where it competes against Plane (~54.6k ★), Huly
(~26.9k ★) and Vikunja and loses on maturity — while omitting the one capability none of
them has. The README's opening line then says *"Local-first project management with a
harness-agnostic control plane for agent work"*, which is two products in one sentence and
requires the reader to already know what a harness is.

Meanwhile the mdBook under `docs/book/` — a genuinely good user and developer guide with a
recovery runbook — is **not published anywhere**. `yielab.github.io/tack` returns 404 and
`homepageUrl` is empty. The documentation exists and is invisible.

**Tasks:**
- Write one sentence that leads with the differentiator, not the category. It should be
  comprehensible to someone who has never heard the word "harness". Apply it identically to
  the GitHub description, the README's first line, and the book's introduction — three
  places currently saying three different things.
- Restructure the README so the first screen answers four questions in order: what is it,
  what does it do that nothing else does, how do I run it, **and what does it not do yet**.
  That fourth question is not a disclaimer to bury — a stranger who discovers the single-user
  limit after installing feels misled, and one who reads it up front feels informed. Take the
  identity paragraph from V-B1's handoff and the proven-harness list from V-A2's.
- Publish the mdBook to GitHub Pages from CI; set `homepageUrl` and repository topics.
- Remove or gitignore `tack.db.before-037_orch_runs_rebuild.sqlite` from the repository root
  (§V.0) — a database snapshot in the root of a project a stranger is about to clone.
- Record, but do not act on, the positioning question the audit raised and this card cannot
  settle alone: the ten project-type presets (construction, legal, homework, events) pull the
  story toward generic project management, which is the losing category. State the trade-off
  in the handoff for the user to decide; **change no preset code**.

**Acceptance:** the README's first fifteen lines, read by someone who has never heard of
Tack, state the category, the differentiator, and the current limits — verified by having an
agent with no prior context on this repository read only those lines and report back what it
believes the product is and cannot do. **Every capability claim on the first screen traces to
a proof from V-A2 or V-A3**, listed in the handoff as claim → evidence. The book is reachable
at a public URL published by CI, and the repository's homepage points at it.

---

### V-B1 — Declare the identity posture instead of leaving it ambiguous

**Owns:** `crates/tack-api/src/middleware.rs`, `docs/book/src/user-guide/administration.md`,
the auth rows of `docs/CONFIG.md`, a new ADR, and the V-B1 handoff. **Hands** its README
paragraph to V-A4 as proposed text; does not edit `README.md`.

**Context.** Tack has **no identity model**. There is no users table, no sessions, and no
per-user permissions. `assignee` is a free-text column added in migration 015; `roles` is a
per-project colour-and-icon label attached to items, not an identity. Authorization is a
single shared bearer token compared in `require_token` — and when no token is configured,
that function returns `Ok(next.run(req).await)` for everything, by design, for pure-local
mode.

**That is a defensible design for a single operator. It is not "self-hosted for a team", and
nothing in the docs distinguishes the two.** A reader who sees "self-hosted" reasonably
assumes accounts exist. This ambiguity is worse than either honest answer.

The posture also aged badly against Part III. Phase 27.2 made a non-loopback bind with no
token log a **warning**. When the product's job was storing task text, a warning was
proportionate. Now the same server schedules coding agents that execute arbitrary code, and
ADR 0058 already chose a startup **error** for exactly this shape of risk on the embedded
runner. The two postures contradict each other.

**Tasks:**
- **Decide and record in an ADR:** Tack v1 is single-operator. Name what was rejected (full
  accounts, OIDC, per-user tokens) and why now is not the time. This is the card's primary
  deliverable — the code change is small; the decision is the point.
- Make the code agree: binding a non-loopback address with no `TACK_API_TOKEN` becomes a
  startup error rather than a warning. Loopback with no token is unchanged — that is the
  pure-local mode the design intends.
- **Handle the deployment constraint honestly.** `Dockerfile` and `docker-compose.yml` exist
  in this repository and a container necessarily binds `0.0.0.0`. Turning this into a hard
  error breaks every existing container deployment on upgrade. Provide a single explicit
  opt-out (an env var whose name states what it means), document it, and make the error
  message name it. A change that silently bricks existing deployments is not shippable.
- State the model plainly in the administration guide's first paragraph, including that
  `assignee` is a label rather than an account.

**Acceptance:** the ADR exists and names the rejected options with reasons. A non-loopback
bind with no token fails to start, proven by a test; the same bind with the documented
opt-out starts, also proven by a test; loopback with no token is byte-identical to today,
proven by the existing tests passing untouched. The administration guide states the limit
before it states any feature. The README paragraph is written and handed to V-A4, not merged
here.

---

### V-B2 — Decide the fate of the docket control plane

**This card decides and gates. It does not delete.** A card that starts deleting has
misunderstood its scope.

**Owns:** a new ADR, the feature-gating of `crates/tack-orch/src/adapters/**`, the
control-plane and approvals UI routes in `frontend/src/features/`, the docket rows of
`docs/CONFIG.md`, and the V-B2 handoff. **Does not own** migrations, the scheduler, the
runner-v1 execution domain, or any `TODO.md` archive section.

**Context.** Parts I and II built a complete integration against exactly one backend,
[docket](https://github.com/yielab/docket): a `ControlPlane` trait, a reconciler and health
machine, a docket adapter, 9 `orch_*` tables, a `control_planes` table, a
fleet-wide Approvals inbox and a ControlPlanesManager. Part III then replaced that entire
model with the native pull-based runner and kept docket only as "an optional legacy bridge".

Both models now coexist in the schema, in the UI and inside `tack-orch` — where the docket
half sits beside the runner-v1 execution domain that Part III *does* depend on. A reader
encountering two `fleet` concepts cannot tell which one is current, and neither can a new
contributor.

**Read this before scoping anything.** `tack-orch` is ~19k lines and **is not all legacy** —
the neutral runner-v1 execution domain lives there too and is load-bearing. Furthermore,
Deletion is still not an `rm` — the crate mixes closed-cycle code with the load-bearing
execution domain, and separating them is the work. It is explicitly **not this card's job**.

**Tasks:**
- **Measure the surface first, before proposing anything.** Tables, rows in the live schema,
  LOC attributable to the docket half of `tack-orch` (as distinct from the runner-v1 domain),
  UI routes, config variables, and the release binary delta with the adapter compiled out.
  Numbers, with commands (§V.1 rule 4).
- Write the ADR deciding one of three options, each costed from those numbers: **keep** as a
  supported optional bridge; **gate** behind a default-off cargo feature plus config flag; or
  **schedule deletion**, with the migration plan for the `orch_*` tables and the citation
  updates written out.
- **Implement only the gating**, and only if that is the decision. Deletion, if chosen, is a
  future card that this ADR authorizes.

**Acceptance:** the handoff carries measured numbers, not adjectives. The ADR names the
option chosen and both rejected, with the cost of each. If gating shipped: a default build
exposes no docket concept in the UI, the CLI or the config table; `cargo nextest run --workspace` is
green in **both** feature states, with the test counts for each recorded; and no migration was
altered, proven by the migration list being unchanged.

---

### V-C1 — Distribution beyond `curl | sh`

**Needs V-A3.** There is nothing to distribute until a release exists that contains the
product.

**Owns:** `packaging/**`, a Homebrew formula/tap, an AUR `PKGBUILD`, a Nix derivation, the
container-publish step of `release.yml` (**coordinate with V-A3**, which owns that file), and
`cargo-binstall` metadata in `Cargo.toml`.

**Context.** Today there are exactly two install paths: a `curl | sh` one-liner (broken until
V-A1) and `cargo install --git`, which requires a Rust toolchain and a slow LTO build. The
audience this Part targets installs things with `brew`, `paru`, `nix` or `docker run`. A
`Dockerfile` exists but no image is published anywhere.

**Tasks:** add the recipes; publish the container image to `ghcr.io` from the release
workflow; add `cargo-binstall` metadata so `cargo binstall tack-cli` fetches the release
binary instead of compiling. **Preparation only for anything requiring an external account or
a PR to a third-party repository** — §V.1 rule 5 covers Homebrew taps and AUR submissions.

**Acceptance:** at least three channels install a working `tack` that starts and serves,
**each verified by an actual install in a clean container**, with the command and transcript
in the handoff. A channel that could not be tested on this machine is listed as **untested**
in the handoff and is not mentioned in the README until it is. Binary size and install time
are recorded per channel as real measurements.

---

### V-C2 — The demo that shows what nothing else can

**Needs Part IV Wave 10** (so the demo is one command, not four steps and a copied token) and
**V-A2** (so it does not hang on camera).

**Owns:** `docs/screenshots/**`, the demo recording and its script, the README hero asset
(hand the markdown to V-A4), and the V-C2 handoff.

**Context — this is the most under-used asset in the repository.** Every competitor can show
"assign a card to an agent and watch it work". **None of them can show durable recovery**,
because none of them has leases, fencing tokens or replay tables. Tack's own smoke already
proves the sequence end to end: SIGKILL mid-attempt → `needs_operator` → **no blind
duplicate** → operator requeue → attempt #2 succeeds. That is the entire competitive
argument, it is already tested, and it has never been shown to a single person outside this
repository.

**Tasks:** record roughly sixty seconds, no narration required: create an item → dispatch it
to an agent → watch the attempt live → kill the runner → see `needs_operator` and the absence
of a duplicate → requeue → success → open the artifact. Record it from a **release artifact on
a clean machine** running `tack serve --with-runner`, not from a development tree.

**Acceptance:** the recording is made from a release artifact on a machine with no repository
checkout, stated as such in the handoff with the version used. It contains no cut that hides a
failure or a retry. **If a step cannot be shown honestly, it is dropped from the demo rather
than staged** — a demo that shows five real steps beats one that shows seven with one faked.

---

### V-C3 — Launch preparation

**Needs V-A1 through V-A4, V-C1 and V-C2.** This card **prepares and stops**; §V.1 rule 5 is
absolute here.

**Owns:** a launch checklist under `docs/`, the issue and PR templates, `good first issue`
labelling, seeded Discussions topics, and the V-C3 handoff.

**Context.** Five months public, zero stars. Nothing on this board converts unless somebody
sees it, and the audience is unusually identifiable: the users Vibe Kanban orphaned when Bloop
shut down on 2026-04-10, and Crystal's after its February 2026 deprecation. Those people are
in known places asking a known question. A launch that reaches them is a different act from a
generic "Show HN".

**Tasks:**
- Draft the posts (HN / r/selfhosted / r/rust / Lobsters) — **drafts only, published by the
  user**.
- Write the comparison table, and be honest in it about what Tack lacks: no accounts, no
  notifications, English only, one contributor. A comparison table that only lists wins is
  read as marketing and discounted entirely; one that names its own gaps is read as
  engineering. Given this audience, the second converts better and is also simply true.
- Seed five to ten `good first issue`s from the real gaps this board and the audit named — i18n
  scaffolding, SMTP notifications, time tracking, in-UI artifact diff review. Each with enough
  context that a stranger could start.
- Review the issue and PR templates for someone who has never contributed here.

**Acceptance:** **nothing is published.** The deliverable is the prepared material plus a
repository that survives its first hundred visitors, verified by walking the stranger's path
end to end on a clean machine: the install command works, the docs load, the demo plays, the
limits are stated before they are discovered, and a `good first issue` can be understood
without reading `TODO.md`.

---

### V-C4 — Two green checks that were not measuring anything

**Needs nothing.** Found while preparing the launch, and a launch cannot ship over either.

**Owns:** the `e2e` job in `.github/workflows/ci.yml`, `.github/workflows/verify-install-urls.yml`
and `scripts/verify-install-urls.sh`, and the V-C4 handoff. **Does not touch** `install.sh`,
whose fix already landed with V-C3.

**Context, one — the badge.** On each of the last pushes to `develop`, eight CI jobs passed
and one did not: *E2E (Playwright, cross-browser)*, cancelled at 25m15s and 25m16s against its
own `timeout-minutes: 25`. That is a job exceeding its budget, not a suite failing, and not
anyone pressing cancel — but the badge a stranger sees on the repository front page reads the
same either way. Either the job fits its budget or the budget is wrong; decide which from what
the job actually spends its time on, and say so.

**Context, two — the check that checked the wrong thing.** `install.sh` installed nothing at
all from `v0.1.0-beta.7` until V-C3 fixed it: two archives per release end in the same
platform suffix and the API lists the runner's first, so the installer downloaded that one and
died on `no 'tack' binary found in archive`. Through every one of those pushes, *Verify install
URLs* was green in ten seconds. Its own header explains why it exists — "the OpenAPI/frontend/
coverage gates would all stay green while the public install path was broken" — and it was
right about the risk and blind to the instance, because resolving a URL is not installing from
it.

**Acceptance, all of it measured locally — this card does not push.** The E2E suite's real
cost is measured on this machine, per project, with the command and the numbers pasted, and
the job is changed so that cost fits with margin. The install check is proven load-bearing the
way this repo proves any other guard: revert `install.sh`'s `grep -v` once, run
`scripts/verify-install-urls.sh`, watch it fail, restore it. A check that cannot fail on the
exact bug it exists to catch is not evidence. **The confirming CI run is the user's step, not
this card's** — `origin/develop` is behind and pushing is theirs to authorize; the handoff ends
by naming what they should see when they do.

**Stop if:** making the E2E job fit needs a change in the frontend or the server. Record what
you measured and hand it over — the budget is yours, the code under test is not.

---

## §V.5 Deliberately not in this Part

Recorded so no card adopts them by drift, and so the roadmap keeps them visible:

- **Any new product feature.** Outbound notifications and SMTP (zero references today),
  i18n (the UI is English-only, with zero locale infrastructure), time tracking (`estimate`
  exists, `time_spent` does not), and in-UI diff review of agent artifacts are all real gaps
  the audit found. They belong to a post-adoption cycle and are recorded in the roadmap, not
  carded here.
- **Multi-user accounts.** V-B1 decides and documents the posture; it does not build identity.
- **Deleting the docket surface.** V-B2 decides and gates; deletion is a future card that its
  ADR authorizes.
- **Changing or removing the ten project-type presets.** V-A4 records the positioning
  trade-off for the user to decide and changes no preset code.
- **Anything Part IV owns.** `tack serve --with-runner`, `tack runner doctor` and the runner
  composition root are Phase 58 and stay there.
- **The Alexa integration.** The audit flags it as surface area with no adoption value, but it
  works, it is documented, and removing working features to tidy a story is not a trade this
  Part is authorized to make. **Superseded 2026-09-03:** the user directed its removal outright;
  it is gone from code, config, CI and docs (CHANGELOG, Unreleased → Removed), outside any card.

---

## §V.6 Handoff additions for this Part

Use the §III.2 template verbatim, plus three sections specific to Part V:

1. **Claim → evidence table.** Every user-visible claim your card added or kept, and the
   command, test or transcript that proves it. A row with no evidence column is a claim to
   delete, not a row to leave blank.
2. **Measured numbers.** Every number you produced, with the command that produced it
   (§V.1 rule 4).
3. **What a stranger still cannot do.** One short paragraph. Not what is unimplemented in
   general — specifically what someone arriving from outside this repository would try, and
   fail at, after your card landed.

---

# Part IV — Standalone Single-Binary Operation (Phase 58)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Standalone Single-Binary
Operation*, and decided in [`docs/adr/0058-standalone-single-binary-runner.md`](docs/adr/0058-standalone-single-binary-runner.md).
**Parts I, II and III remain historical context.** This Part has its own numbering
namespace (`§IV.0` … `§IV.6`) so Part I's load-bearing section numbers stay put.

Like Part III, this board is written to be picked up cold by parallel agents in isolated
worktrees. Every card is bounded, names every shared-file owner, and has an acceptance
gate verifiable without trusting its author's handoff.

## Status board — Part IV

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 10 — Standalone single binary | IV-A1 · A2 · A3 · A4 · A5 · A6 | 58 | **Done, all six integrated** at `83fefab` on `develop` (handoffs: `docs/agent-handoffs/part-iv/IV-A1.md` … `IV-A6.md`). `tack runner start` and `tack serve --with-runner` both proven live end-to-end, including zero-touch self-enrollment (A4, no token ever displayed/copied) and `tack runner doctor` (A5, verified byte-identical to a live server's capability snapshot); default `tack serve` starts no runner; non-loopback bind refuses to start `--with-runner`; binary-size delta measured at +0.83 MiB (+4.67%). A6 added three load-bearing `scripts/smoke.sh` steps (10-12, proven by an inject-a-real-failure-and-watch-it-FAIL run) and documented the embedded runner in `docs/CONFIG.md`/`docs/book/src/user-guide/agent-runners.md`/README, including a confirmed-live fix for a `RUST_LOG` visibility gap. Full workspace gate green: 1,413 tests, 0 failed, `cargo fmt`/`clippy -D warnings` clean. One pre-existing gap remains open, outside any card's ownership: `bootstrap::build_runtime` requires a credential even when a valid stored session exists on disk (A4 worked around it with a provably-inert placeholder rather than fixing the file it doesn't own) — a real but low-severity fix for whoever next touches `crates/tack-runner/src/bootstrap.rs`. |

**Integration line:** `develop`, the repository's default branch — unchanged from Part III
Wave 7 onward, and for the same reason (two naming failures in a row cost that cycle a
real trunk). Branch every card from `develop`. Do not create a `plan/*` line for this Part.

> **Part V is active at the same time, and shares three files with this Part.**
> `scripts/smoke.sh`, `README.md` and `docs/CONFIG.md` are assigned to **IV-A6** in §IV.2
> below *and* to Part V cards. That collision is real, and the resolution rule lives in
> **[§V.3](#v3-dependency-graph-cross-part-conflicts-and-merge-policy)** — read it before
> branching IV-A6. In short: **V-A2 fixes `scripts/smoke.sh` first and IV-A6 rebases onto
> it** (adding a step to a smoke that misreports its own failure is strictly worse than
> fixing the misreport first), and **V-A4 takes the `README.md` merge last**. If IV-A6 is
> already in flight when a Part V card starts, the Part V card escalates rather than racing.

---

## §IV.0 Cold-start context capsule

**What this Part is for, in one sentence.** Today a developer who wants an agent to run
against their own board needs two binaries, four manual steps and a copied one-time token;
after this Part they need one command, `tack serve --with-runner`.

**Read before touching anything:** `docs/adr/0058-standalone-single-binary-runner.md`. It
records why the runner is separate at all (ADR 0050), why that separation is about *roles
and not binaries*, and — most importantly — why the embedded runner must still speak
runner-v1 over loopback HTTP instead of calling handlers in-process. A card that "optimizes
away" that HTTP hop has broken the whole point of the design; escalate instead.

**Working-tree state at the time this board was written (2026-08-26).** `develop` is at
`277868a`, but the tree carries one **uncommitted** change: a fix to
`crates/tack-runner/src/harness/mod.rs::registering_all_three_real_adapters_is_order_independent`.
That test asserted all three adapters reject a fixture spec identically; that was only true
for `codex` *by accident*, because the binary was absent from the machine. With `codex`
installed the test failed legitimately — the codex adapter is a pass-through harness
(III-H5) and accepts any explicit model pre-spawn. The assertion now expects codex to
accept and the other two to reject, matching real behaviour instead of an environmental
artifact. **Land or discard this deliberately before branching cards** — do not let it ride
into a card's diff unexamined.

**Part III's tag is still refused, and NOT for the reason the board previously recorded.**
Wave 9 said the tag was blocked on one thing: `codex` not being installed. It has since
been installed (`codex-cli 0.149.1`, alongside `claude` 2.1.236 and `opencode` 1.18.0 — 3
of 3) and `./scripts/smoke.sh --live` was run on 2026-08-26. It **failed**, for a different
reason. See the Wave 9 amendment in the Part III board above before assuming the release is
one smoke run away. This Part does not depend on that being resolved, and must not be
blocked waiting for it.

**What is already true and must stay true.** The runner protocol, scheduler, fencing,
decisions, artifacts, retention and the operator API/CLI/UI are all built and tested; a
runner enrolls, claims, checks out, runs a real harness, submits events and artifacts, and
completes against a live server. This Part adds **packaging and first-run experience**. It
is not permitted to change behaviour that Part III proved.

---

## §IV.1 Rules for simultaneous agents

**All fourteen rules of §III.2 apply unchanged** — one card / one worktree / one branch,
stay inside `Owns`, no `unimplemented!()` or hidden fake success, tests ship with the card,
no blocking sleeps, logs carry ids and never credentials, stop on contract ambiguity, and
each wave ends with adversarial verification by someone who did not author the code. Read
them; they are not restated here.

Four rules are specific to this Part:

1. **The embedded runner uses the same protocol client as a remote runner.** No card may add
   an in-process shortcut, a privileged bypass, a second `RunnerProtocolClient`, or shared
   access to `AppState`. If loopback HTTP appears to be a problem, that is an escalation,
   not a design freedom.
2. **`tack-api` must not gain a dependency on `tack-runner`.** The composition root is
   `tack-cli`. A card that finds itself wanting `tack-api` to know a runner exists has
   mis-placed the work — escalate.
3. **Off by default, loud on failure.** No card may ship the embedded runner enabled by
   default, and none may let `tack serve` continue silently after the embedded runner has
   failed to start or has died. A server running without the runner the operator asked for
   is indistinguishable from a scheduler bug and must be an error, not a log line.
4. **No contract, scheduler, fleet, migration or frontend changes.** This Part is packaging.
   `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json` and
   `frontend/**` are all out of scope for every card here. A card that believes it needs one
   states the need in its handoff and stops.

**Handoff:** each card writes exactly one `docs/agent-handoffs/part-iv/IV-<card>.md`, using
the template in §III.2 verbatim, plus the three Part IV additions in §IV.6. Corrections are
appended as amendments, never rewritten. No card edits this board — the wave integrator
does that after independent verification.

---

## §IV.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `crates/tack-runner/src/main.rs`, `lib.rs`, the new bootstrap module | IV-A1 only |
| `crates/tack-api/src/server.rs` | IV-A2 only, and **only** the readiness/bound-address signal |
| `crates/tack-cli/src/main.rs`, `crates/tack-cli/Cargo.toml`, root `Cargo.lock` | IV-A3, then IV-A5 for its one subcommand arm |
| `crates/tack-cli/src/local_runner.rs` (new) | IV-A3 |
| `crates/tack-cli/src/local_enrollment.rs` (new) | IV-A4 |
| `crates/tack-api/src/handlers/runner_admin.rs` | IV-A4 only, and **only** to extract a reusable provisioning function without changing the route's behaviour |
| `scripts/smoke.sh` | IV-A6 only |
| `docs/CONFIG.md`, `docs/book/src/user-guide/agent-runners.md`, `README.md` | IV-A6 only |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |
| `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json`, `frontend/**` | **nobody — out of scope for this Part** |

---

## §IV.3 Dependency graph and merge policy

```text
IV-A1  (tack-runner entry point) ──┐
                                   ├── IV-A3 ──┬── IV-A4 ──┐
IV-A2  (tack-api readiness seam) ──┘           │           ├── IV-A6
                                               └── IV-A5 ──┘
```

- **A1 and A2 run in parallel** — different crates, no shared file, neither depends on the
  other.
- **A3 needs both.** It is the card that makes `tack` one binary with both roles.
- **A4 and A5 run in parallel** after A3 lands; both touch files A3 created, so neither may
  start before A3 is merged.
- **A6 is last** — it proves the whole thing live and writes the operator docs. It needs A4
  (auto-enrollment) to exist for the standalone claim to be true; A5 is optional to it.

**Merge order:** A1 → A2 → A3 → (A4, A5 in either order) → A6. A1 and A2 may merge in either
order. Gates run once on the integrated tree, not per card, per the Part III precedent.

---

## §IV.4 Cards

### IV-A1 — Runner composition root as a reusable entry point

**No behaviour change. This card is a refactor and must prove it changed nothing.**

**Owns:** `crates/tack-runner/src/main.rs`, `crates/tack-runner/src/lib.rs`, a new
bootstrap/composition module in `crates/tack-runner/src/`, and the IV-A1 handoff.
**Does not own** any other crate, the contract directory, or any harness adapter.

**Context — the exact shape of the problem.** Everything that composes a working runner
lives in the binary's `main.rs::run()`: `build_adapter_registry`, `report_capabilities`,
`HttpPullProtocol`, `RunnerEngine`, `OwnerOnlyJournal`, `WorkspaceManager` +
`GitWorktreeProvisioner`, `HttpRunnerClient`, `RunnerRuntime`, and the `with_data_protocol`
wiring III-H6 added. None of it is reachable from the library, so an embedder would have to
copy it — and a copied composition root is a copy that drifts. `RunnerRuntime::run` already
takes an injected `Shutdown`, so the seam for an embedder mostly exists; what is missing is
a public function that builds the whole thing.

**Tasks:** extract the composition into a public library entry point taking a
`RunnerConfig` and a `Shutdown` and returning the same typed `Result` the binary returns
today. The `tack-runner` binary becomes argument parsing plus a call to it, and keeps its
own signal handling. Keep `ProcessLimits` and `PROTOCOL_REQUEST_TIMEOUT` explicit rather
than defaulted — they are deliberate operational choices with no `Default` for that reason.
Preserve the honest capability reporting verbatim: `cancel` advisory, `decisions`
unsupported, `artifacts`/`usage` advisory, and the "not registered when the binary is
absent" behaviour for each adapter.

**Acceptance:** the `tack-runner` binary's observable behaviour is unchanged — it enrolls,
claims, runs and completes against a live `tack serve` exactly as before, and
`./scripts/smoke.sh` (fake mode) reaches the same steps with the same outcomes as on the
base SHA, recorded side by side in the handoff. The new entry point is callable from
outside the crate with an injected shutdown, proven by a test that starts it and stops it
without a process signal. `cargo nextest run --workspace -E 'package(tack-runner)'` is green with no test deleted.

### IV-A2 — Server readiness and bound-address seam

**Small card. Its scope is one signal, and its risk is scope creep.**

**Owns:** `crates/tack-api/src/server.rs` (the readiness/bound-address signal only) and the
IV-A2 handoff. **Does not own** anything else in `tack-api`, and explicitly not
`router.rs`, `config.rs`, any handler, or the OpenAPI surface.

**Context.** `tack_api::serve()` loads config, migrates, binds, and blocks until shutdown.
An embedder must know two things it cannot know today: *when* the listener is actually
accepting, and *what address* it bound. The port is configurable and may differ from the
requested one, so the embedder cannot assume `127.0.0.1:3210`. Without this, the embedded
runner would have to poll-and-hope against a guessed URL — a race and a wrong-target bug
waiting to happen.

**Tasks:** add a way for an in-process caller to observe readiness and the real bound
`SocketAddr`, without changing `serve()`'s existing signature or behaviour for every
current caller. Signal readiness **after** the listener accepts, never before — an early
signal recreates the race this card exists to remove.

**Acceptance:** an in-process test starts the server through the new seam, receives the
bound address, and issues a successful request to it with no retry loop and no sleep. The
existing `serve()` entry point still works unchanged, proven by the CLI's `tack serve`
starting exactly as before. No route, handler, config field or spec path changes — asserted
by `openapi_contract` staying 5/5 drift-free.

### IV-A3 — One binary: `tack runner start` and supervised `tack serve --with-runner`

**Needs IV-A1 and IV-A2 merged.** This is the card that delivers the headline capability.

**Owns:** `crates/tack-cli/src/main.rs`, `crates/tack-cli/Cargo.toml`, root `Cargo.lock`, a
new `crates/tack-cli/src/local_runner.rs`, and the IV-A3 handoff. **Does not own**
`tack-api` or `tack-runner` internals — if either needs a change, that is an escalation to
IV-A2 or IV-A1's owner, not an edit.

**Context.** `tack-cli` already has a `runner` subcommand namespace (`enroll`, `revoke`,
`revoke-token`), so `tack runner start` slots in beside them. `tack-cli` already depends on
`tack-api`; it gains `tack-runner`. Verified before this board was written: **no dependency
cycle** — `tack-runner` depends on `tack-orch` only, never on `tack-api`.

**Tasks:**
- `tack runner start` — run A1's entry point with the same configuration precedence and the
  same flags the standalone binary accepts. Prefer `TACK_RUNNER_ENROLLMENT_TOKEN` over a
  flag, as the standalone binary already does, so the secret stays out of shell history.
- `tack serve --with-runner` (gate also readable as `TACK_LOCAL_RUNNER_ENABLE`) — start the
  server, wait on A2's readiness signal, then start an embedded runner **as a task in the
  same process**, pointed at the real bound loopback address, speaking ordinary runner-v1
  HTTP. Off unless explicitly enabled.
- Supervise it honestly: shutdown stops both roles cleanly; an embedded runner that fails to
  start or dies takes the process down with an operator-visible error rather than leaving a
  server running with no runner.
- Refuse to start the embedded runner when the server is not bound to loopback — reuse the
  existing `AppConfig::binds_loopback()`. This is a startup error, never a silent downgrade.
- Enrollment stays manual in this card: it consumes a credential from the environment.
  Zero-touch enrollment is IV-A4 and must not be pre-empted here.

**Acceptance:** `tack runner start`, given a credential, enrolls and completes a real
attempt against a live `tack serve` — the same proof the standalone binary carries.
`tack serve --with-runner`, given a credential, does the same from **one process and one
binary**, with the attempt visible through the operator API. Default `tack serve` starts no
runner — asserted by the absence of a runner in `GET /api/runners`, not merely by a missing
log line. A non-loopback bind refuses with a typed error, proven by a test. Killing the
embedded runner surfaces an error rather than a quiet server. **The binary-size delta is
measured and recorded as a real number**, before and after, never estimated.

### IV-A4 — Zero-touch local enrollment

**Needs IV-A3 merged.** Without this card the standalone claim is not true — the user still
copies a token by hand.

**Owns:** a new `crates/tack-cli/src/local_enrollment.rs`,
`crates/tack-api/src/handlers/runner_admin.rs` (**only** to extract a reusable provisioning
function — the HTTP route's behaviour, auth and response shape must not change), and the
IV-A4 handoff.

**Context.** Enrollment is deliberately two-step: an operator creates a pending runner and
receives a one-time token, and the runner redeems it for a durable credential. Only hashes
are stored. That design is not being weakened — it is being *automated for the local case*,
where the operator and the runner are the same person on the same machine.

**Tasks:** on `tack serve --with-runner`, after readiness and before starting the runner —
(1) if the runner's state directory already holds a durable credential, use it; (2)
otherwise self-provision: create the pending runner in-process (this is a bootstrap/admin
concern, not the runner protocol, so in-process is legitimate *here* and only here), obtain
the one-time token, and hand it to the embedded runner, which redeems it **over loopback
HTTP through the ordinary protocol path** like any other runner. Keep the durable credential
owner-only in the runner state directory. Auto-provisioning is gated by the same
loopback-only rule IV-A3 established, checked again here rather than assumed.

**Acceptance:** on a machine with no prior runner state, `tack serve --with-runner` reaches
a completed attempt with **no token ever displayed, copied or configured** — the headline
proof of this Part. A second start reuses the stored credential and does not create a
second runner, asserted against `GET /api/runners` row counts, not logs. The redemption is
shown to have gone through the real HTTP protocol path, not a bypass. No credential appears
in any log or terminal output, asserted with a positive control (the test proves it *can*
observe output by asserting an id does appear). `runner_admin.rs`'s route behaviour is
byte-identical — proven by its existing tests passing untouched.

### IV-A5 — `tack runner doctor`

**Needs IV-A3 merged.** Independent of IV-A4; the two may run in parallel.

**Owns:** a new doctor module in `crates/tack-cli/src/`, one subcommand arm in
`crates/tack-cli/src/main.rs`, and the IV-A5 handoff.

**Context — why this is in scope for a packaging Part.** The information an operator needs
in order to answer "why can't I run this model?" exists only inside a capability snapshot
that a runner posts to a server. There is no way to ask the local machine what it can do.
That gap is the direct cause of a real, reported confusion: it is not discoverable how one
configures Claude, Codex, OpenRouter or a local model. The answer — that provider
credentials live in each harness's own environment and Tack never proxies them (ADR 0050,
reaffirmed in ADR 0058) — is correct but invisible.

**Tasks:** report, for the local machine: which harness binaries are on `PATH` and their
versions; what each probe declares (`model_combinations`, `model_passthrough`, and each
feature's `cancel`/`resume`/`decisions`/`artifacts`/`usage` support with its reason); and
which are absent. Reuse A1's probe path — do not re-implement discovery. State plainly, for
each harness, where its provider credentials come from and that Tack neither stores nor
forwards them.

**Acceptance:** on a machine with a harness installed and one absent, the output names each
honestly — present with a version, absent as absent, never rounded up and never invented. A
probe error is reported as a probe error, distinct from "not installed". The declared
capabilities shown match exactly what the same probe reports to a server, proven by
comparing against a real capability snapshot rather than by re-deriving them.

### IV-A6 — Standalone smoke, configuration and operator docs

**Needs IV-A4 merged.** Last card of the wave.

**Owns:** `scripts/smoke.sh`, `docs/CONFIG.md`,
`docs/book/src/user-guide/agent-runners.md`, `README.md`, and the IV-A6 handoff.

**Context and a standing warning.** `scripts/smoke.sh` has shipped a false green once
already in this repository's history: steps 7–9 printed `SKIPPED` unconditionally and could
never fail, so the script reported `SMOKE PASSED` while proving less than it claimed. Any
step this card adds must be able to fail. A step that cannot pass because the product cannot
do the thing is a `FAIL`, never a `SKIP`; environmental absence is `ABSENT` and named in the
verdict, never counted as a pass. Additionally: `docs/CONFIG.md` today documents no
harness/model configuration at all, and the model-configuration story is exactly what users
report as unclear — this card is where that gets written down.

**Tasks:** add a standalone-mode step proving one binary, one command, zero manual
enrollment reaches a completed attempt; assert that default `tack serve` starts no runner;
assert the non-loopback refusal. Document the gate, the loopback rule, the state-directory
location, and — as its own section — how provider credentials actually work for each
harness (env/CLI login per harness, OpenRouter and local-model endpoints via the harness's
own configuration, and that Tack is never a model gateway, with the ADR cited). Update
`README.md`'s getting-started path to lead with the standalone command.

**Acceptance:** the new smoke step is proven load-bearing by breaking the feature once and
watching it `FAIL` — the same discipline III-H9 and III-H6 used, and the specific defense
against the false green above. The documented commands are executed exactly as written on a
clean state directory and the transcript recorded. Every capability claim in the new docs
cites the test or run that proves it. Nothing in this card changes compiled behaviour.

---

## §IV.5 Acceptance matrix

| Invariant | Owner | Must remain green through |
|---|---|---|
| One protocol-client implementation; no in-process bypass | A3 | A6 |
| `tack-api` never depends on `tack-runner` | A3 | A6 |
| Embedded runner is off unless explicitly enabled | A3 | A6 |
| Non-loopback bind refuses to auto-enroll, as a startup error | A3/A4 | A6 |
| A failed or dead embedded runner is loud, never silent | A3 | A6 |
| Durable credential owner-only; no credential in any log or terminal | A4 | A6 |
| Runner-v1 contract, scheduler, fleets, migrations, frontend untouched | all | A6 |
| Standalone smoke step can actually fail | A6 | release |

---

## §IV.6 Definition of done

On a machine with one harness installed and no prior Tack state, **`tack serve
--with-runner` is the only command needed** to go from nothing to a completed agent attempt
visible in the UI — one binary, one process, no second artifact to install, no pending
runner to create, no one-time token to copy.

Additionally:

- The embedded runner is off unless explicitly enabled, and refuses to auto-enroll on any
  non-loopback bind.
- The embedded runner speaks runner-v1 over loopback HTTP with **no second code path** —
  the same client a remote runner uses, against the same routes.
- A fleet of remote runners still works exactly as before; `tack-runner` remains shippable
  and useful on a machine with no server.
- `tack runner doctor` answers "what can this machine run, and where do its model
  credentials come from" without a server round trip.
- An operator can find out how to point a harness at Claude, Codex, OpenRouter or a local
  model from the documentation, including the fact that Tack never proxies model traffic.
- Full Rust gates green; `runner_contract`, `wave2_gate` and `openapi_contract` unchanged
  and drift-free, because nothing in this Part may touch what they pin.

**Handoff additions for this Part** — each card's handoff carries §III.2's template plus:

- **Binary-size delta**, measured before and after, for any card that changes what `tack`
  links.
- **Which role executed what**, for any card claiming a live run — whether the attempt was
  claimed by an embedded or a standalone runner, and over which address.
- **Loopback/gating proof**, naming the test that shows the off-by-default and
  non-loopback-refusal behaviours, not merely asserting them.

---

Parts I, II and III are archived verbatim at `docs/closed-cycles/boards/part-1.md`,
`part-2.md` and `part-3.md` — see the "Which board is live" table near the top of this
file.
