#!/usr/bin/env python3
"""Renumber Tack's ADRs once, so they run from 0001 without gaps.

A dry run by default: it prints the table from old to new numbers, how many
changes each file gets and every bare number it read as an ADR. --apply writes
the changes and renames the files with `git mv`. It never commits; review
`git diff -M` and commit.

The ADRs listed in ARCHIVED have nothing left to decide: they move to
ARCHIVE_DIR and keep the number they had, which no live ADR can have, since
the live ones end below 0030. A citation of one is left as it is.

What it rewrites, in one pass per file so that no number is rewritten twice:
  - citations: a number after "ADR" or "ADRs" (also "**ADR:**"), lists and
    ranges of them ("and", commas, "–", "/"), with "§ N" or "decision N"
    between the items, "ADR-NNNN", "ADR number `NNNN`", and a citation wrapped
    onto the next line of a comment or a quote;
  - a bare Tack number on a line that names an ADR, at the start of a line
    that continues one, or in a table whose header names one; each is listed
    so a person can check it;
  - ADR file names anywhere, so links and paths follow the rename or the move;
  - the numeric cells of the index table in the decisions page.
It adds a table from old to new at the end of the decisions page, so commit
messages and anything written before keep a way back.

A citation of docket's or prill's ADRs is left as it is. It stops --apply only
when a file it would change has uncommitted changes.

Usage: scripts/renumber-adrs.py [--apply]
"""

import argparse
import datetime
import re
import subprocess
import sys
from pathlib import Path

ADR_DIR = "docs/adr"
INDEX = "docs/book/src/developer/decisions.md"
SELF = "scripts/renumber-adrs.py"
ARCHIVE_DIR = "docs/closed-cycles/adr"
ARCHIVED = ("0060", "0063", "0065", "0073")

ADR_NAME = re.compile(r"^(\d{4})-[a-z0-9][a-z0-9-]*\.md$")
FILE_REF = re.compile(r"(?<![\w-])(\d{4}-[a-z0-9][a-z0-9-]*\.md)(?![\w-])")
# A line break inside a citation, with the next line's comment or quote marker.
GAP = r"[ \t]*(?:\n[ \t]*(?://[/!]?|#|\*|>)?[ \t]*)?"
HEAD = re.compile(r"\bADRs?\b(?:\*\*)?:?(?:\*\*)?(?:[ \t]+(?:number|no\.))?" + GAP + r"(?:-" + GAP + r")?")
WORD = re.compile(r"\bADRs?\b")
NUM = re.compile(r"`?(00\d\d)(?!\w)`?")
# Not the start of a file name: those change only when they name an ADR.
BARE = re.compile(r"(?<![\w.-])(00\d\d)(?![\w.]|-[A-Za-z])")
# What may sit between two numbers of one list: "'s", "§ 5", "decision 7", or
# a short parenthesis such as "0070 (docket contract 1.1), 0071 (...)".
DECO = re.compile(
    r"(?:'s|’s)?"
    r"(?:[ \t]*§[ \t]*\d+(?:\.\d+)*)?"
    r"(?:[ \t]+decisions?[ \t]+\d{1,2}(?!\d)(?:[ \t]*(?:,|-|–|and|to)[ \t]*\d{1,2}(?!\d))*)?"
    r"(?:[ \t]*\([^()]{0,200}\))?"
)
SEP = re.compile(
    GAP + r"(?:,|and\b|or\b|y\b|to\b|[–—/-])" + GAP
    + r"(?:(?:and|or|y)\b" + GAP + r")?(?:ADRs?\b" + GAP + r")?"
)
OTHER_PROJECT = re.compile(r"\b(?:docket|prill|its)(?:'s|’s)?[ \t]+$", re.I)
INDEX_ROW = re.compile(r"^\|[ \t]*00\d\d[ \t]*\|", re.M)
NUMBER_CELL = re.compile(r"^[ \t]*00\d\d(?:[ \t]*,[ \t]*00\d\d)*[ \t]*$")


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True).stdout


def dirty(repo):
    """Tracked paths with uncommitted changes, staged or not."""
    tokens = git(repo, "status", "--porcelain=v1", "-z", "--untracked-files=no").decode().split("\0")
    paths, skip = set(), False
    for token in tokens:
        if skip:
            skip = False
            continue
        if token:
            paths.add(token[3:])
            skip = token[0] in "RC"
    return paths


def read_text(path):
    if path.is_symlink() or not path.is_file():
        return None
    data = path.read_bytes()
    if b"\0" in data[:8192]:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def number_list(text, pos):
    """The numbers of the list that starts at pos, as [(number, start, end), ...],
    and where its last number ends (so a citation inside a parenthesis is read
    on its own); no list when no number starts there."""
    num = NUM.match(text, pos)
    if not num:
        return [], pos
    nums = [(num.group(1), num.start(1), num.end(1))]
    pos = num.end()
    while True:
        deco = DECO.match(text, pos)
        pos = deco.end() if deco else pos
        sep = SEP.match(text, pos)
        nxt = sep and NUM.match(text, sep.end())
        if not nxt:
            return nums, nums[-1][2]
        nums.append((nxt.group(1), nxt.start(1), nxt.end(1)))
        pos = nxt.end()


def citations(text):
    """Each citation as (where its "ADR" starts, [(number, start, end), ...])."""
    reached = 0
    for head in HEAD.finditer(text):
        if head.start() < reached:
            continue
        nums, reached = number_list(text, head.end())
        if nums:
            yield head.start(), nums


def line_no(text, pos):
    return text.count("\n", 0, pos) + 1


def line_bounds(text, pos):
    start = text.rfind("\n", 0, pos) + 1
    end = text.find("\n", pos)
    return start, len(text) if end < 0 else end


def reads_as_adr(text, pos):
    """Whether a bare number reads as an ADR number: its line names an ADR, it
    opens a line that continues one that does, or it sits in a table whose
    header does."""
    start, end = line_bounds(text, pos)
    if WORD.search(text[start:end]):
        return True
    if start and re.fullmatch(r"[ \t>*/#!`(|-]*", text[start:pos]):
        prev_start, _ = line_bounds(text, start - 1)
        if WORD.search(text[prev_start : start - 1]):
            return True
    if text[start:end].lstrip().startswith("|"):
        top = start
        while top:
            prev_start, _ = line_bounds(text, top - 1)
            if not text[prev_start : top - 1].lstrip().startswith("|"):
                break
            top = prev_start
        return bool(WORD.search(text[top : line_bounds(text, top)[1]]))
    return False


def rewrite(text, mapping, renames, moved, is_index=False):
    """The rewritten text and the bare numbers it read as ADRs, as (line, old, new)."""
    edits, loose, covered, settled = [], [], [], set()

    for m in FILE_REF.finditer(text):
        new = renames.get(m.group(1))
        if new:
            edits.append((m.start(1), m.end(1), new))
            covered.append((m.start(1), m.end(1)))
        elif m.group(1) in moved and text.endswith(ADR_DIR + "/", 0, m.start(1)):
            edits.append((m.start(1) - len(ADR_DIR) - 1, m.start(1), ARCHIVE_DIR + "/"))
            covered.append((m.start(1), m.end(1)))

    def inside(pos):
        return any(a <= pos < b for a, b in covered)

    for start, nums in citations(text):
        settled.update(a for _, a, _ in nums)
        if OTHER_PROJECT.search(text[max(0, start - 40) : start]):
            continue
        if not all(n in mapping or n in ARCHIVED for n, _, _ in nums):
            continue
        edits += [(a, b, mapping[n]) for n, a, b in nums if n in mapping and not inside(a)]

    if is_index:
        for row in INDEX_ROW.finditer(text):
            offset = row.start()
            for cell in text[row.start() : line_bounds(text, row.start())[1]].split("|"):
                if NUMBER_CELL.match(cell):
                    for n in re.finditer(r"00\d\d", cell):
                        if n.group() in mapping:
                            edits.append((offset + n.start(), offset + n.end(), mapping[n.group()]))
                            settled.add(offset + n.start())
                offset += len(cell) + 1

    for t in BARE.finditer(text):
        pos = t.start(1)
        if t.group(1) not in mapping or pos in settled or inside(pos) or not reads_as_adr(text, pos):
            continue
        # The rest of its list follows it even onto lines that do not name an
        # ADR, so a list is never left half renumbered ("0002 and 0058").
        for old, a, b in number_list(text, pos)[0]:
            if old in mapping and a not in settled and not inside(a):
                settled.add(a)
                edits.append((a, b, mapping[old]))
                loose.append((line_no(text, a), old, mapping[old]))

    edits.sort()
    for (_, b1, _), (a2, _, _) in zip(edits, edits[1:]):
        if a2 < b1:
            raise RuntimeError(f"overlapping edits at line {line_no(text, a2)}")
    out = text
    for a, b, new in reversed(edits):
        out = out[:a] + new + out[b:]
    return out, loose, len(edits)


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--apply", action="store_true", help="write the changes and rename the files")
    args = ap.parse_args()

    repo = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    every = sorted(p.name for p in (repo / ADR_DIR).iterdir() if ADR_NAME.match(p.name))
    olds = [n[:4] for n in every]
    if len(set(olds)) != len(olds):
        sys.exit("two ADR files share a number; fix that first")
    if missing := set(ARCHIVED) - set(olds):
        sys.exit(f"ARCHIVED names numbers with no file: {sorted(missing)}")
    names = [n for n in every if n[:4] not in ARCHIVED]
    moved = [n for n in every if n[:4] in ARCHIVED]
    rows = [(name[:4], f"{i:04d}", name, f"{i:04d}{name[4:]}") for i, name in enumerate(names, 1)]
    mapping = {old: new for old, new, _, _ in rows}
    renames = {name: new_name for _, _, name, new_name in rows if name != new_name}
    today = datetime.date.today().isoformat()

    print(f"== {len(rows)} ADRs in {repo / ADR_DIR}")
    for old, new, name, _ in rows:
        print(f"  {old} -> {new}  {name[5:]}")
    print(f"== {len(moved)} ADRs moved to {ARCHIVE_DIR}, keeping their number")
    for name in moved:
        print(f"  {name}")
    if not renames and not moved:
        print("They already run from 0001 without gaps; nothing to do.")
        return 0

    unclean = dirty(repo)
    changes, loose, blocked = {}, [], []
    for rel in git(repo, "ls-files", "-z").decode().split("\0"):
        if not rel or rel == SELF:
            continue
        text = read_text(repo / rel)
        if text is None:
            continue
        new, found, count = rewrite(text, mapping, renames, set(moved), is_index=(rel == INDEX))
        if rel == INDEX:
            new = "".join(l for l in new.splitlines(keepends=True) if not INDEX_ROW.match(l) or l[2:6] not in ARCHIVED)
            table = "\n".join(f"| {old} | {new_} | `{new_name}` |" for old, new_, _, new_name in rows)
            table += "".join(f"\n| {n[:4]} | archived | `{ARCHIVE_DIR}/{n}` |" for n in moved)
            new = new.rstrip("\n") + (
                f"\n\n## Numbers before {today}\n\n"
                f"On {today} the ADRs were renumbered to run from 0001 without gaps, and the ones with nothing "
                f"left to decide moved to `{ARCHIVE_DIR}` and kept their number. Commit messages and "
                "anything written before that date use the old numbers; this table translates them.\n\n"
                f"| Old | New | File |\n|---|---|---|\n{table}\n"
            )
        loose += [(rel, *f) for f in found]
        if new != text:
            changes[rel] = (new, count)
            if rel in unclean:
                blocked.append(rel)

    print(f"\n== Files to change: {len(changes)}")
    for rel, (_, count) in sorted(changes.items()):
        print(f"  {count:4d}  {rel}")
    if loose:
        print(f"\n== Bare numbers read as ADRs (check them): {len(loose)}")
        for rel, line, old, new in loose:
            print(f"  {rel}:{line}: {old} -> {new}")
    if blocked:
        print(f"\n== Uncommitted changes in files it would change (blocks --apply): {len(blocked)}")
        for rel in blocked:
            print(f"  {rel}")

    if not args.apply:
        print("\nDry run; nothing written.")
        return 1 if blocked else 0
    if blocked:
        print("\nNot applied: commit or stash those files first.")
        return 1

    for rel, (new, _) in changes.items():
        (repo / rel).write_text(new, encoding="utf-8")
    (repo / ARCHIVE_DIR).mkdir(parents=True, exist_ok=True)
    for name in moved:
        git(repo, "mv", f"{ADR_DIR}/{name}", f"{ARCHIVE_DIR}/{name}")
    for name, new_name in renames.items():
        git(repo, "mv", f"{ADR_DIR}/{name}", f"{ADR_DIR}/{new_name}")
    print(f"\nApplied: {len(changes)} files changed, {len(renames)} ADRs renamed, {len(moved)} moved. Nothing is committed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
