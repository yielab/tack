---
name: status
description: Answer "where is this project right now and what is next" — reconstructs current state from git, the TODO.md status board, handoffs and the working tree in a few cheap commands. Use at the start of any session, after a break, or when it is unclear what was done and what to do next.
---

# Where am I / what's next

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.


State is NOT stored in one place and no file is kept "up to date" for you. It is derived
from four sources of record. Run these, then report.

```bash
# 1. What actually landed, and is the tree clean?
git log --oneline -8
git status --porcelain          # empty = safe to branch cards from HEAD
git branch --show-current

# 2. The board: which wave is active, what is its base SHA, what is open
head -30 TODO.md                          # names the active cycle
grep -n "^| [0-9] — \|^| Wave " TODO.md   # the status-board rows
# then read only the row for the active wave (sed -n '<line>p' TODO.md)

# 3. The most recent decisions/corrections
ls -t docs/agent-handoffs/*/ | head -5
# read only the newest one or two if the board row is unclear

# 4. DELIVERED-BUT-UNMERGED WORK — the check that ancestry misses.
# `git branch --merged` lies here: rebased/restructured branches report unmerged
# forever, and finished card branches sit invisible. Use patch-equivalence:
for b in $(git branch --no-merged HEAD | sed 's/^..//'); do
  n=$(git cherry HEAD "$b" 2>/dev/null | grep -c '^+')
  [ "$n" -gt 0 ] && printf 'UNLANDED %-40s %s commits  (last %s)\n' \
    "$b" "$n" "$(git log -1 --format=%cd --date=short $b)"
done

git worktree list               # agent worktrees still mounted
```

For every branch that reports UNLANDED, classify it before reporting — never guess:

- **Finished card awaiting integration** — recent date, has a handoff in
  `docs/agent-handoffs/`, matches an open card id. This is real work; say so loudly.
- **Superseded** — old date, and its content exists in HEAD in restructured form.
  Verify by looking for the feature in the tree (a moved file, a renamed module),
  not by trusting the branch name. Report as safe-to-delete; deleting is the user's call.

## Branch conventions in this repo

| Pattern | Meaning |
|---|---|
| `agent/iii-<card>-<slug>` | one board card's work; merged by the wave integrator |
| `worktree-agent-<hash>` | throwaway checkout created by agent isolation; not authored work |
| `plan/<cycle-name>` | the cycle's integration line |
| `develop`, `origin/*` | long-lived; `main` does NOT exist locally — do not assume it |

## Branch health — run this every time, it is how the trunk gets lost

The integration line is named on the board, not chosen by habit. A *card* branch has
served as the de facto trunk here for three waves while the real line sat 21 commits
behind, because nobody compared them.

```bash
# 1. What does the board say the integration line is? (never assume from your CWD)
grep -n "[Ii]ntegration line\|branch from" TODO.md | head -3
LINE=develop                              # confirm against the line above

# 2. Does any branch hold work the line does not have? (content, not ancestry)
git for-each-ref --format='%(refname:short)' refs/heads | while read b; do
  [ "$b" = "$LINE" ] && continue
  # git cherry, not rev-list: ancestry reports every rebased or restructured branch
  # as ahead forever. Here that was 27 false positives against 4 real ones, and a check
  # that cries wolf is a check nobody reads.
  n=$(git cherry "$LINE" "$b" 2>/dev/null | grep -c '^+')
  [ "${n:-0}" -gt 0 ] && printf 'UNLANDED  %-42s %s commits not in the line\n' "$b" "$n"
done

# 3. Is the line backed up? Unpushed commits exist only on this machine.
git show-ref --verify --quiet "refs/remotes/origin/$LINE" \
  && git rev-list --count "origin/$LINE..$LINE" | xargs echo "unpushed on the line:" \
  || echo "the line is NOT on the remote at all — everything here exists on one machine"

# 4. Which branch are you actually on?
git branch --show-current
```

Report, in the status summary:
- the integration line and whether you are on it;
- any branch ahead of it, and whether that is expected (a finished card awaiting merge)
  or drift (a card branch that quietly became the trunk — say so, it is a finding);
- the unpushed count when it is not zero. Work that exists on one machine only is a
  risk worth one sentence, every time.

## Reporting

Answer in this order, briefly:

1. **Done** — the last accepted wave/card and its SHA (from the board row, confirmed
   against `git log`).
2. **Now** — clean tree or not; current branch; anything uncommitted that must land
   before dispatching cards.
3. **Next** — the open cards for the active wave, which may run in parallel, which must
   run last, and **the exact base SHA to branch from**.
4. **Findings** — anything that contradicts itself. Check specifically: does the board's
   stated base SHA still equal HEAD? New commits after the board was written mean cards
   branching from the recorded SHA would silently miss them.

## Rules

- The board is the authority for what shipped; `git log` is the authority for what
  exists. When they disagree, say so — do not quietly trust either.
- Never read TODO.md whole. Extract the rows and the active wave's section only.
- This skill only READS. Updating the board is the wave integrator's job (see `/card`);
  if you find drift, report it and propose the corrected text.
