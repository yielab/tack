# VI-<card> handoff

Copy this file to `VI-<card>.md`, fill every line, delete the two optional headings that do
not apply to your card. Keep the `## ` headings exactly — VI-D1 and the integrator extract
sections by heading, never by reading the file whole.

- Base SHA / branch / final SHA:
- Files changed (must equal ownership list):
- Contract fixtures consumed:
- Behavior implemented:
- Tests added and exact commands/results:
- Failure/adversarial case proved:
- Schema/API/contract change requested from another owner:
- Known limitations or `not_measured` fields:
- Secrets/logging review:
- Safe merge order and likely conflicts:
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| | |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

Every number this card produced, with the command that produced it.

## What a stranger still cannot do

One short paragraph. Not what is unimplemented in general — specifically what someone
arriving from outside this repository would try, and fail at, after this card landed.

## Surface-map delta

Which rows of §VI.0's surface map this card moved from console to UI, or proved cannot
move — with the reason, and whether that reason is already in the table's last column. If
it is not, that is an escalation for ADR 0061, not a new row.

## Secret-path proof

*(B1, B2, B3, D1 only — delete this heading otherwise.)* The exact commands showing the
key in the runner's store and nowhere else: the `sqlite3 tack.db .dump | grep -c`, the
captured log output with the name present and the value absent, the `stat -c '%a'`.

## Vocabulary check

*(A3, C1, C2, D1, D2 only — delete this heading otherwise.)* The grep for §VI.1 rule 8's
words over what this card rendered or wrote, every hit, and why each is under *Advanced*
or in the developer book. The README is allowed "runner" — it is telling the story.

## Context spent

- Tokens read before the first edit (cold start), against the block's estimate:
- Context size at handoff:
- Files opened and not used (each one is a finding for the dispatch README):
- Read-list lines that were wrong (a range that missed, a size that was off):

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
