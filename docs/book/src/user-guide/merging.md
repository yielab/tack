# Merging from Tack

When an agent's run finishes on a branch and its pull request is open, you can merge it
from Tack instead of going to GitHub. There are three ways, and a project uses as many of
them as you turn on.

| Way | Who decides | Where |
|---|---|---|
| The **Merge** button | You, one change at a time | On the finished task and on its attempt |
| **Merge when moved to Done** | You, by moving the task to Done | Settings → Automation → Autonomy |
| **Automatic merge** (L4 or L5) | Tack, under rules you set | Settings → Automation → Autonomy |

Every merge uses the exact commit the run produced, so GitHub refuses it if the branch has
moved since. Tack never asks GitHub for more permission than its token has.

## The Merge button

The button is enabled once the run finished with a pushed branch, the task is linked to a
GitHub issue, and the board has a GitHub token. Tack reads CI when you press it, not before:

- CI green: Tack merges and the task follows its workflow.
- CI still running, or failed: nothing is merged, and the button and the task say why. Press
  it again when CI is done.
- The token cannot merge in that repository: the button stays disabled and says the token
  needs **Contents: write**. Fix the token on GitHub, then press again.
- Branch protection or a conflict: GitHub's own reason is shown.

## Merge when moved to Done

With this option on, moving a task to Done merges its latest finished run. The move happens
at once; the merge follows in the background. If CI is still running, Tack waits and merges
when it turns green. If it cannot merge, the task card shows a badge with the reason, so a
task in Done that was not merged never looks finished.

A task moved to Done by an AI tool over MCP is not merged this way: its activity says so,
and the Merge button is there for you.

## Automatic merge

The level says how much of the merge decision Tack takes:

- **L3** — you decide every change. This is where every project starts, and where it stays
  unless you change it.
- **L4** — Tack merges by itself inside **domains** you name: a set of paths, and optionally
  of task types. A change merges by itself only when every file it touches is inside one
  domain at L4 or L5; anything else is L3.
- **L5** — the same for the whole repository.

At L4 and L5 Tack merges a finished run only when all of these hold, in this order. The first
that does not is shown on the task, and a person reviews it as usual:

1. The run did not come through MCP.
2. The verifier said the change can merge.
3. The change's domain is at L4 or L5.
4. The verifier's pack lists every stage the project requires ("Stages the verifier must
   run"; at least one is needed) as passed. A stage it skipped as not applicable does not
   count.
5. The verifier's pack is signed, and the signature checks against the exact commit.
6. No daily budget of the project is spent.
7. The pull request is open, and CI on that commit is green.

The Autonomy section says what on this board stops an automatic merge — GitHub polling off,
no verifier installed where the board runs, no public key — and shows each domain's merged
and undone changes before its level.

**Demotion is automatic; promotion is yours.** When a change Tack merged by itself is
reverted, or CI fails on its merge commit, its domain drops one level and the task's
activity says so. Only you raise a level again.

### What it needs

- The verifier (prill) on the runner, with a signing key (`[verify] sign_key`, see the
  [Configuration reference](../developer/configuration-reference.md)), and on the machine the
  board runs on, to check signatures.
- The project's public key in the Autonomy section.
- The board's GitHub token and poll (`TACK_GITHUB_TOKEN`, `TACK_GITHUB_POLL_SECONDS`): the
  automatic merge runs on the poll's tick.
- A task linked to a GitHub issue, since that is how its pull request is opened.

## Sending a rejected change back to the agent

A project can let a change the verifier rejects go back to the agent first, with the
verifier's list of what to fix (or, when it gave none, the checks that failed), up to a number of times you set (0, the default, is never).
It stops sooner when two rejections in a row give the same reasons, when the verifier asks
for a person instead, or when you cancel the follow-up run; then the task waits for your
review with every attempt kept.

## Budgets

Each project can cap, per day: tokens, dollars (priced from the rate card on the Factory
page), automatic merges, and items waiting for your review. A spent budget stops new runs from
starting, and the card says which budget; runs already going finish.
