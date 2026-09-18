# IX-M7-archive handoff

This is the **archival** half of IX-M7. The generated-documentation half (book includes,
API reference generator, `cargo doc` CI gate) is a separate card, already merged to
`develop` at `510e21d`, and wrote its own `IX-M7-docs.md` — not touched here.

- Base SHA / branch / final SHA: `510e21d` (`develop`) / `agent/ix-m7-archive` / this
  branch's tip (worktree: `/tmp/ix-m7-archive`).
- Behavior implemented: none — pure documentation move and reorganization. No `.rs` or
  `.ts`/`.tsx` file touched; no production behavior changed.
- Tests added: none (not applicable to this card).
- Secrets/logging review: n/a.
- Checklist: no unowned files touched outside the stated scope; no live secret; no panic
  stub; no blind retry.

## 1. What moved where

### 1a. Board sections (Task 1) — verbatim moves out of `TODO.md`

| Old location in `TODO.md` (original file, 15,056 lines) | New file | Lines moved |
|---|---|---|
| `# Part I — Agent-Factory Control Center (Phases 33–38)`, lines 4871–12406 | `docs/closed-cycles/boards/part-1.md` | 7,536 |
| `# Part II — Agnostic Control Plane (Phases 39–49)`, lines 12408–13937 | `docs/closed-cycles/boards/part-2.md` | 1,530 |
| `# Part III — Harness-Agnostic Runner Fleet (Phases 50–57)`, lines 13939–15056 (end of file) | `docs/closed-cycles/boards/part-3.md` | 1,118 |

10,184 lines moved, matching the plan's "10,200 of its 14,784 lines" figure (plan measured
before the Part IX board existed; today's total was 15,056). Each new file opens with one
prepended notice line + blank line:

> Archived, no-longer-current board. The live board is `TODO.md` — see its "Which board is
> live" table.

Nothing else in the moved text was edited — not even the internal `TODO.md §1.4`
self-citation at what is now `docs/closed-cycles/boards/part-1.md:1386`, which is itself
historical narrative describing the file's state when that sentence was written.

**Also removed from `TODO.md` as connective tissue that no longer described anything real
once the above moved** (not itself Part I/II/III board content, so outside Task 1's literal
scope, but left in place it would have been actively wrong):

- The `# Archive — closed and superseded cycles` heading and its four-sentence blockquote
  (old lines 4857–4869, sitting between Part IV's end and Part I's start) said "Everything
  below this line is history... They stay in this file at their original section numbers."
  That became false the moment Parts I–III left. Replaced with a two-line closing pointer
  at the new end of the file (see below) — no narrative content was unique to that
  blockquote; its only load-bearing fact (Part III's Wave 9 amendment being worth reading
  unprompted) references **V-A2**, a card integrated on 2026-09-06, so the pointer's
  usefulness had already expired.
- The `## Why the archive stays in this file` section (old lines 32–53, ~22 lines) existed
  specifically to argue *against* doing the extraction this card performs — its own text
  says "It has not been done, and nothing here argues for or against it." Replaced with a
  7-line `## Where closed cycles live` section stating the new layout and re-running its
  own measurement commands (`grep -rn "TODO\.md" crates/ --include='*.rs' | wc -l` → still
  `0`).
- `docs/book/src/roadmap.md:1252` cited `` TODO.md §1.4 `` — the one live document the old
  section's own measurement command found. Repointed to
  `` docs/closed-cycles/boards/part-1.md §1.4 ``, verified that heading still exists there
  (`### 1.4 docket HTTP surface`, line 224 of the new file).
- `TODO.md`'s own top-of-file header block (lines 3–7) said "it is ~15k lines and roughly
  90% closed-cycle decision history" — rewritten to state the new size (~4.8k lines, no
  closed-cycle history left in the file) and point at `docs/closed-cycles/boards/`.

**Not touched, flagged rather than guessed at:** `docs/book/src/roadmap.md` has several
other prose references to "`TODO.md`, Part II" / "`TODO.md`'s Part III board" (lines 37,
2404, 2420, etc.) that are now slightly stale — that content no longer lives in `TODO.md`.
The plan's §4 assigns fixing this to a *different*, larger piece of future work ("`roadmap.md`
keeps only its `# Next` sections... the rest archives with the Parts it records"), which
this card's task list does not include. Only the one citation the plan's own measurement
command identified as live (`§1.4`) was repointed.

### 1b. Handoffs (Task 2)

| Old directory | New directory | Files |
|---|---|---|
| `docs/agent-handoffs/part-iii/` | `docs/closed-cycles/handoffs/part-3/` | 47 |
| `docs/agent-handoffs/part-iv/` | `docs/closed-cycles/handoffs/part-4/` | 7 |
| `docs/agent-handoffs/part-v/` | `docs/closed-cycles/handoffs/part-5/` | 10 |
| `docs/agent-handoffs/part-vi/` | `docs/closed-cycles/handoffs/part-6/` | 79 (incl. two subdirs: `vi-a3-render-proof/`, `vi-d1-stranger-proof/`) |
| `docs/agent-handoffs/part-vii/` | `docs/closed-cycles/handoffs/part-7/` | 67 (incl. two subdirs: `vii-b4-proof/`, `vii-d1-proof/`) |

210 files total, all via `git mv` (tracked as renames — `git status --porcelain` shows
210 `R` entries). **Not moved**, per the card's explicit instructions: `docs/agent-handoffs/part-viii/`
(VIII-C3 is still open), `docs/agent-handoffs/part-ix/` (the live board — this file lands
there), and the two non-Part-scoped directories `docs/agent-handoffs/deps/` and
`docs/agent-handoffs/test-architecture/`.

**Part I / Part II handoffs — search outcome: nothing exists.** Before moving, I searched
the whole repo (not just `docs/agent-handoffs/`) for anything documenting these two Parts:
no `part-i/` or `part-ii/` directory existed under `docs/agent-handoffs/`; no root-level
files there either (`docs/agent-handoffs/*.md` matched nothing); a repo-wide grep for
Part-I/Part-II-specific phrases ("Agent-Factory Control Center", "Phases 33–38",
"Agnostic Control Plane", "Phases 39–49") outside `TODO.md` matched only
`docs/plans/agnostic-control-plane.md` (an original planning doc, not a handoff — left in
place, out of scope) and prose mentions inside other Parts' own handoffs/book pages that
merely *refer to* Phases 33–49 in passing. I did not invent a placeholder; there is
genuinely no handoff record for Parts I or II beyond their board text itself (now in
`docs/closed-cycles/boards/part-1.md` and `part-2.md`).

## 2. Byte verification (Task 1)

For each Part, I extracted the exact `sed`-selected line range from the **original**
`TODO.md` into a scratch file, then diffed it against the new archive file's body with the
one prepended notice line and its blank line stripped off (`tail -n +3`):

```
tail -n +3 docs/closed-cycles/boards/part-1.md | diff - /tmp/orig-part1-body.txt   # empty
tail -n +3 docs/closed-cycles/boards/part-2.md | diff - /tmp/orig-part2-body.txt   # empty
tail -n +3 docs/closed-cycles/boards/part-3.md | diff - /tmp/orig-part3-body.txt   # empty
```

All three `diff` invocations produced no output — byte-for-byte identical. (The scratch
files under `/tmp` were working copies only, not committed.)

## 3. Trimmed table (Task 3)

`TODO.md`'s "Which board is live" table, Status column, before → after (characters):

| Part | Before | After | Note |
|---|---|---|---|
| IX | 232 | 232 (untouched) | Already under 300; Part IX's board section itself was never touched, per the card's rule. |
| VIII | 1,509 | 186 | Kept: done date, waves integrated, the one open card (VIII-C3, blocking nothing), ADR 0060/0065 pointer. Detail stays in VIII's own section (untouched, still in `TODO.md`). |
| VII | 2,350 | 230 | Kept: done date + reopen/re-close note, ADR 0062, bundle status, the stranger-walk headline, the one spun-out defect (VI-C36). |
| VI | 18,775 | 264 | The card's own example row (plan measured ~18,906; today's exact figure was 18,775 — re-measured, not re-quoted). Kept: done date, "nothing is open", the one still-true caveat (no in-app login anywhere, fake-harness shim, walks not re-run). |
| V | 741 | 222 | Kept: done date, what remains (publishing, a human action), pointer to `docs/LAUNCH-CHECKLIST.md`. |
| IV | 38 | 39 | Already short; added a trailing period only. |
| III | 33 | 34 | Already short; `Where` column repointed from `[§III](#...)，archive` to `` `docs/closed-cycles/boards/part-3.md` `` since that anchor no longer exists in `TODO.md`. |
| II | 35 | 36 | Same as III — `Where` repointed to `` `docs/closed-cycles/boards/part-2.md` ``. |
| I | 19 | 20 | Same as III — `Where` repointed to `` `docs/closed-cycles/boards/part-1.md` ``. |

All nine rows are ≤300 characters (verified by script, not eyeballed — see below). The
detail cut from VIII/VII/VI/V is not lost: it lives in each Part's own section, still in
`TODO.md` for IV–VIII, or in the new `part-<n>.md` files for I–III, and in the handoffs.

Verification:
```
python3 -c "
with open('TODO.md') as f: lines=f.readlines()
for i in range(12,21):
    status=lines[i].split('|')[4].strip()
    assert len(status)<=300, (i+1, len(status))
print('all 9 rows <= 300 chars')
"
# all 9 rows <= 300 chars
```

## 4. Task 4 — `docs/closed-cycles/README.md`

Created. First line states nothing under the directory is current and the live board is
`TODO.md`. Explains `boards/` and `handoffs/`, and explicitly warns that Parts IV–VIII's
board sections are *not* here (they're still in `TODO.md`) even though their handoffs are —
so a reader doesn't assume "handoffs moved" implies "board section moved" for those five
Parts.

## 5. Task 5 — tooling exclusion findings

- **Book (`docs/book/src/SUMMARY.md`, `book.toml`):** no-op, confirmed. Neither file
  references `docs/closed-cycles/` or `TODO.md` as an include target; `SUMMARY.md` has no
  relative paths reaching outside `docs/book/src/`. `docs/book/src/roadmap.md` links to
  `../../../TODO.md` in several places (prose links, not `{{#include}}`), none of which
  point into `docs/closed-cycles/`.
- **`.claude/context-budget.md`:** real change, not a no-op. It didn't mention
  `docs/closed-cycles/` (so nothing to *exclude* there), but its `TODO.md` and
  `docs/agent-handoffs/**` rows carried line counts and token estimates from before this
  move and were now wrong by a wide margin — re-measured and rewritten:
  - `TODO.md` whole: 15,056 lines / ~263k tokens → 4,849 lines / ~89k tokens.
  - `docs/agent-handoffs/**` all: 46,019 lines / ~820k tokens → 12,548 lines / ~212k tokens
    (55 files remain: `deps/`, `part-viii/`, `part-ix/`, `test-architecture/`).
  - The "largest single handoff" row pointed at `part-iii/III-C2.md`, which just moved out
    of `docs/agent-handoffs/` — repointed to the new largest remaining file,
    `docs/agent-handoffs/part-ix/IX-M4-tack-api-orchestration.md` (539 lines, ~9k tokens).
  - The two "dispatch plan" rows (Part VI's and VII's `README.md`) pointed at paths that
    moved to `docs/closed-cycles/handoffs/part-6/` and `part-7/` — repointed, and reworded
    to say these are archived/closed reading, not active-cycle reading.
  - The extraction-recipe's `head -68 TODO.md` no longer lands on the end of the header
    (table trim + section rewrites shifted it) — corrected to `head -58`.
  - Deliberately **not added**: any new row for `docs/closed-cycles/**` itself. The card
    asks that this directory not be in the read list — adding a row (even one saying
    "never read this") would be listing it. Omission is the compliant outcome.
- **`scripts/check-comments.sh`:** no-op, confirmed after reading the full script (165
  lines) rather than guessing its structure. Its default `ROOTS` are `crates/
  frontend/src frontend/e2e` — `docs/` is never in scope, closed or otherwise (the script's
  own header comment states this: "Scope: comments, doc comments and human-readable
  strings under crates/ (*.rs), frontend/src (*.ts, *.tsx) and frontend/e2e (*.ts)").
  Confirmed both callers (`.githooks/pre-push` and `.github/workflows/ci.yml`) invoke it
  with no path arguments, so no existing invocation ever widens scope to `docs/`. There is
  no "exclude list for similar directories" to extend, because there is no docs-scanning
  behavior to begin with. `./scripts/check-comments.sh` was run anyway as a sanity check
  (not because I edited it) — green: `no board archaeology in crates/ frontend/src
  frontend/e2e`.

## 6. Gate

```
$ python3 scripts/maintainability.py check --changed
✓ maintainability budgets hold (0 files checked)
```
(0 files checked is expected — every change in this card is under `docs/` or `TODO.md`,
outside the script's tracked source/test budgets.)

```
$ git diff --exit-code
```
Run after staging and committing everything below; the working tree was clean afterward —
nothing was auto-regenerated by this change (no `openapi.json`/`schema.gen.ts`/lockfile
touched, none expected since no Rust or frontend source changed).

No `cargo`/`npm` commands were run, per the card's own note that none should be needed for
a pure documentation and file-reorganization change.

## 7. Summary of every file touched

- Moved (content-identical, notice line prepended): `docs/closed-cycles/boards/part-{1,2,3}.md`
- `git mv`'d (210 files, tracked as renames): `docs/agent-handoffs/part-{iii,iv,v,vi,vii}/**`
  → `docs/closed-cycles/handoffs/part-{3,4,5,6,7}/**`
- New: `docs/closed-cycles/README.md`
- Edited: `TODO.md` (header block, "Which board is live" table, "Why the archive stays" →
  "Where closed cycles live" section, removed stale `# Archive` framing block, new closing
  pointer at end of file), `.claude/context-budget.md` (re-measured rows),
  `docs/book/src/roadmap.md` (one citation repointed, line 1252/1253)
- New (this file): `docs/agent-handoffs/part-ix/IX-M7-archive.md`
