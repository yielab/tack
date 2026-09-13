# IX-M7-roadmap handoff

- Base SHA / branch / final SHA: `4655b6b` / `agent/ix-m7-roadmap` / `78807da`
- Files changed (must equal ownership list): `docs/book/src/roadmap.md` (trimmed to what is
  ahead), `docs/closed-cycles/boards/roadmap-phases-0-57.md` (new — the moved Phases 0–57
  text), `docs/closed-cycles/README.md` (one-line entry for the new archive file),
  `.claude/context-budget.md` (re-measured `roadmap.md` row), `docs/book/src/developer/
  orchestration.md` and `docs/book/src/user-guide/orchestration.md` (the two links that
  pointed into the moved text). No Rust or test file touched; no `TODO.md`, no
  `docs/agent-handoffs/`, no other `docs/closed-cycles/` file.
- Contract fixtures consumed: none.
- Behavior implemented: none — documentation move + link repoint only.
- Tests added and exact commands/results: none (forbidden by scope; card touches no test
  file). Verification is `mdbook build docs/book` (green, see *Claim → evidence*) and
  `python3 scripts/maintainability.py check --changed` (`✓ maintainability budgets hold
  (0 files checked)` — no `.rs` file changed).
- Failure/adversarial case proved: n/a (mechanical/documentation card).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see *What a stranger still cannot do*.
- Secrets/logging review: n/a, no code behavior changed.
- Safe merge order and likely conflicts: touches only files IX-M7-roadmap owns per §IX.2
  (`docs/book/src/roadmap.md`; `docs/book/**` and `docs/closed-cycles/**` (new) are IX-M7's).
  No open Wave 32 card (`IX-M8-*`, `IX-M6-dev-notes`) touches any of these six files — should
  merge cleanly. `roadmap.md`'s Phase 63 section (`## Cards — the Part IX board`, line 924,
  `| IX-M6 | ... dev-notes resolved | M2 |`) is untouched by this card — Phase 63 is not in
  scope, and the line is accurate, not stale.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Phases 0–57 (Parts I–III) moved out of `roadmap.md` verbatim, byte-identical below the archive notice | `diff <(sed -n '44,310p;345,408p;504,588p;590,621p;623,1143p;1145,1634p;1636,2361p;2394,2957p' <old roadmap.md>) <(tail -n +5 docs/closed-cycles/boards/roadmap-phases-0-57.md, dividers stripped)` → empty (run piecewise per section during the edit; recorded in *Measured numbers*) |
| `roadmap.md` keeps only what is ahead: the intro, an `## Archived` pointer, `## Planned` (now just Phases 21–22, still open), `## Known Gaps`, `## Contributing`, and every `#` chapter for Phase 58 onward | `grep -n "^# \|^## " docs/book/src/roadmap.md` — no `## Completed`, no `## Audit-driven cycle`, no `## Next —`, no `# Harness-Agnostic Runner Fleet`, no `### Phase 20/23/24/25` remain |
| Every link into the moved text is repointed | `grep -rn "roadmap.md#" docs/ README.md \| grep -v closed-cycles \| grep -v agent-handoffs` → empty (was 1 hit, `developer/orchestration.md:10`, before this card) |
| `mdbook build docs/book` is green with no warnings | verbatim tail in *Budget check* below |
| `docs/closed-cycles/README.md` lists the new archive file | `git diff docs/closed-cycles/README.md` adds one bullet for `boards/roadmap-phases-0-57.md` |
| `.claude/context-budget.md`'s `roadmap.md` row is re-measured and its stale note fixed | row now reads `948` lines / `~15k` tokens (was `3,676` / `~56k`); the old note ("The `# Next` sections at the end are the live part") is gone since those sections no longer exist in the file |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `roadmap.md` before (`4655b6b`): `wc -l` → `3,683`; `wc -c` → `225,208` (~56.3k tokens at
  chars/4). (`.claude/context-budget.md` had recorded `3,676` — off by 7 lines; not
  re-verified before this card, per the dependency-floor-style rule that a quoted number is
  re-measured before being repeated.)
- `roadmap.md` after: `wc -l` → `948`; `wc -c` → `61,629` (~15.4k tokens).
- `docs/closed-cycles/boards/roadmap-phases-0-57.md` (new): `wc -l` → `2,774` (2,749 lines of
  moved content + 4 notice/title lines + 21 divider lines added between the 8 moved chunks);
  `wc -c` → `165,082`.
- Net: `roadmap.md` dropped 2,735 lines (−74.3%); nothing left the repository — every line
  either stayed in `roadmap.md` or moved to the new archive file (2,735 removed from
  `roadmap.md` vs. 2,749 moved-content lines in the archive; the 14-line gap is the trailing
  `---` dividers that were *not* carried into the archive, replaced there by dividers I added
  between chunks — content itself is unchanged, confirmed by the piecewise `diff`s below).
- Sections moved (line ranges are in the pre-card `roadmap.md`, `4655b6b`):
  - `## Audit-driven cycle (Phases 26–32) — status board` (44–56) + `## Completed` (60–310)
  - `### Phase 20 — MCP Server` (345–408, from inside `## Planned`)
  - `### Phase 23 — Table View` + `### Phase 24 — Positioning & Proof` (504–588, from inside
    `## Planned`)
  - `### Phase 25 — Plaintext / Local-First` (590–621, from inside `## Planned`)
  - `## Next — Audit-Driven Phases 26–32` (623–1143)
  - `## Next — Agent-Factory Control Center (Phases 33–38)` (1145–1634)
  - `## Next — Agnostic Control Plane (Phases 39–49)` (1636–2361)
  - `# Harness-Agnostic Runner Fleet (Phases 50–57)` (2394–2957, with all its `##` subsections)
- Byte-identity check, per moved chunk (`diff` exit 0 = identical):
  ```
  diff <(sed -n '44,310p' <base>) <(sed -n '5,271p' <archive, after notice/title>)      → identical
  diff <(sed -n '345,408p' <base>) <(<archive Phase 20 chunk>)                          → identical
  diff <(sed -n '504,588p' <base>) <(<archive Phase 23/24 chunk>)                       → identical
  diff <(sed -n '590,621p' <base>) <(<archive Phase 25 chunk>)                          → identical
  diff <(sed -n '623,1143p' <base>) <(<archive Next-26-32 chunk>)                       → identical
  diff <(sed -n '1145,1634p' <base>) <(<archive Next-33-38 chunk>)                      → identical
  diff <(sed -n '1636,2361p' <base>) <(<archive Next-39-49 chunk>)                      → identical
  diff <(sed -n '2394,2957p' <base>) <(<archive Harness chunk>)                         → identical
  ```
  (run individually while assembling the file, then re-verified once against the final
  archive with a single concatenated `diff /tmp/expected_body.md /tmp/archived_body.md` →
  `IDENTICAL`, and again for the "stays" side: `roadmap.md`'s intro, `## Planned` intro,
  Phase 21, Phase 22, `## Known Gaps`, `## Contributing` and Phase 58 onward each diffed
  clean against the corresponding range in `4655b6b`).
- `mdbook build docs/book`: green (see *Budget check*).
- `python3 scripts/maintainability.py check --changed`: `✓ maintainability budgets hold
  (0 files checked)` — this card touches no `.rs` file, so the check is a no-op, as expected.

## What "## Planned" split into

`## Planned` was not a uniform block: Phase 20 (`✅ done`), Phase 23/24/25 (`✅ done`) had
shipped; Phase 21 (`⏳ v1 push-only shipped; inbound + comments pending`, remaining slices
rescheduled as Phases 47/49 — which are themselves archived, since the Agnostic Control Plane
cycle shipped/froze) and Phase 22 (Task 1 done, Task 2 `tack open`/smart start never marked
done) had not. Per the card's instruction ("if it names phases not yet shipped it stays, and
shipped items inside it move"), the section stays and only its four fully-shipped `###` phases
moved; the intro, the three product decisions, the parallelization note, Phase 21 and Phase 22
stayed untouched in `roadmap.md`.

## What a stranger still cannot do

A reader who opens `roadmap.md` and wants the *evidence* behind a shipped Phase 20/23/24/25
claim, or the full task-by-task history of Phases 26–57, now has to follow the `## Archived`
pointer to `docs/closed-cycles/boards/roadmap-phases-0-57.md` rather than finding it inline —
by design, that is the point of the card. What is not fully solved: the six anchors I added
under `## Archived` (to the status board, `Completed`, and the four multi-phase chapters) were
hand-derived from GitHub/mdBook's slug rules (lowercase, strip punctuation, spaces→hyphens,
verified against the one pre-existing anchor this card repointed,
`next--agent-factory-control-center-phases-3338-august-2026`) rather than checked against a
rendered page, because `docs/closed-cycles/` is excluded from the mdbook build and the book has
no link-checker configured (`docs/book/book.toml` has no `mdbook-linkcheck` preprocessor, so
`mdbook build` does not validate any anchor, including the one this card repointed). I did not
add per-phase anchors for Phase 20/23/24/25 inside the `## Archived` list, since their headings
carry emoji and italic markup whose slug behavior I could not verify at all — a reader has to
open the archive file and search for "Phase 20" by name rather than click through.

## Budget check

`mdbook build docs/book`:
```
 INFO Book building has started
 INFO Running the html backend
 INFO HTML book written to `/tmp/ix-m7-roadmap/docs/book/book`
```
No warnings.

`grep -rn "roadmap.md#" docs/ README.md | grep -v closed-cycles | grep -v agent-handoffs`:
```
(no output)
```

`python3 scripts/maintainability.py check --changed`:
```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py measure --totals` before and after: unchanged
(`prod=55599 (comments 11144) test=71211 ratio=1.281 tests=1408`, same as the `4655b6b` base)
— this card touches no `.rs` file.

## What was removed

Nothing was removed — only moved, verbatim, per *Measured numbers* above. No test, no
production line, no comment block. The only new prose is the `## Archived` pointer sections
and the two repointed links, itemized in *Claim → evidence*.

## Re-baselined?

`no`. This card never touches `scripts/maintainability-baseline.json` (not its file to write;
also nothing to re-baseline, since no `.rs` file changed).

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.3 (~2.7k), the
  IX-M7-roadmap card block (~0.3k), `docs/agent-handoffs/part-ix/README.md` header + row
  (~1k), the plan's one `roadmap.md` bullet (~0.1k), `docs/closed-cycles/README.md` +
  `part-1.md` head (~0.3k), the two TEMPLATE.md files (~0.6k), and the roadmap section map +
  link greps + `SUMMARY.md` (~1.5k) — close to the dispatch README's implied budget for this
  card.
- Context size at handoff: single session, no compaction needed.
- Files opened and not used: none beyond the mandated read list and the six files this card
  changed.
- Read-list lines that were wrong: the card's own "today: 3,683 lines" line count in `TODO.md`
  matched exactly; `.claude/context-budget.md`'s `3,676` (7 short) did not — see *Measured
  numbers*.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
