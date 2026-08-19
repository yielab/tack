---
name: integrate
description: Merge finished card branches into the integration line and review them as ONE combined change — conflict scan, unified diff summary, escalations collected from handoffs, gates run once. Use when several card branches are done and waiting, or when asked to integrate a wave, land branches, or do an integrator card (e.g. III-G5).
---

# Integrate finished branches (solo-friendly review)

**Do not review branch by branch.** Each card already justified itself in its handoff and
ran its own tests. Reviewing N branches separately re-does that work N times and still
misses the only thing that matters: whether they are correct *together*. Review once, on
the merged tree.

## 1. Establish the set

```bash
# Which branches actually hold unlanded work (patch-equivalence, not ancestry):
for b in $(git branch --no-merged HEAD | sed 's/^..//'); do
  n=$(git cherry HEAD "$b" | grep -c '^+'); [ "$n" -gt 0 ] && echo "$b  ($n commits)"
done
```
Confirm the set against the active wave's board row. A branch with unlanded work that the
board doesn't mention is a finding — surface it, don't silently merge it.

## 2. Predict conflicts BEFORE merging

```bash
# Files touched by more than one branch = the whole risk surface
for b in <branches>; do git diff --name-only HEAD...$b; done | sort | uniq -d
```
Any file in that list is either a legitimate shared-file handoff request or an ownership
violation. Read what each branch did to it before merging, and check the wave's
shared-file ownership table.

## 3. Read the escalations, not the diffs

Each branch carries its own handoff. Pull just the parts that need a decision:

```bash
for b in <branches>; do
  git show $b:docs/agent-handoffs/<cycle>/<CARD>.md \
    | grep -n -iE "escalat|blocked|cannot|could not|unverified|requested|finding|limitation"
done
```
Every escalation gets an explicit outcome: **accepted**, **routed to a card**, or
**rejected with a reason**. An escalation that is silently merged is how a known problem
becomes an unknown one.

## 4. Merge in dependency order, one at a time

Merge the branch that others depend on first (docs and CI usually last, since they
describe what the others did). After each merge, only check that the tree still builds —
save the full gate set for the end.

## 5. Gate once, on the integrated tree

Run `/gate full`. This is the only test result that counts: a card's own green tests are
not evidence that the integrated system works — that is why wave-gate tests exist and
import no per-card test infrastructure.

## 6. Report as one review

Produce a single summary: what landed, what conflicted and how it was resolved, every
escalation with its outcome, gate results with numbers, and what remains open. That
summary — not N branch reviews — is the artifact worth keeping.

## 7. Close out the wave — say what is next and what must be reviewed

An integration is not finished when the merge is clean. End every run by answering these,
in the report:

1. **Is the wave complete?** Update the active wave's board row with: the integration SHA,
   gate numbers, every escalation's outcome, and any acceptance bar NOT met. A wave that
   merged cleanly but left a P0 open is *integrated*, not *complete* — say both words.
2. **What is the next wave?** Read the cycle's dependency graph and board for the next
   row. Name its cards, which may run in parallel, which must run last, and the exact base
   SHA (= the integration SHA you just produced).
3. **If there is no next wave**, check the cycle's *definition of done* line by line
   against the tree. Do not assume the last wave means the cycle is over — report which
   criteria hold and which do not, and what single card would close the gap.
4. **What needs human review, and how.** List only decisions a person must make, each with
   the command or file that shows the evidence:
   - accepted-but-unverified claims (say what was NOT proven, and why)
   - routed-open findings with owners and severity
   - anything that would ship broken if tagged today
   Rank by what blocks a release, then by what blocks the next wave, then by the rest.

Never close a wave by editing a gate's test to make it pass, and never let an escalation
merge without an outcome — those are the two ways a green board stops meaning anything.

## Then clean up

```bash
git worktree list           # remove worktrees for merged cards
git worktree prune
```
Deleting merged branches is the user's call — propose, don't execute. Verify a branch's
work is truly in HEAD by patch-equivalence (`git cherry`) before ever suggesting deletion.
