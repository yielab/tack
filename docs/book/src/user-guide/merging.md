# Merging from Tack

This page is for the person who decides how a finished run reaches the main branch. After reading it you
can merge a run from Tack with one button, let a task merge when you move it to Done, and set the rules under
which Tack merges by itself. The merge is the third step of the ladder in
[Automating a Project](automation.md#merge); the rules for starting runs and for teams are there too.

When an agent's run finishes on a branch and its pull request is open, you can merge it
from Tack instead of going to GitHub. There are three ways, and a project uses as many of
them as you turn on.

| Way | Who decides | Where |
|---|---|---|
| The **Merge** button | You, one change at a time | On the finished task and on its attempt |
| **Merge when moved to Done** | You, by moving the task to Done | Settings → Automation → Merge |
| **Automatic merge** (L4 or L5) | Tack, under rules you set | Settings → Automation → Merge |

Every merge uses the exact commit the run produced, so GitHub refuses it if the branch has
moved since. Tack never asks GitHub for more permission than its token has.

## The Merge button

The button is enabled once the run finished with a pushed branch, the task is linked to a
GitHub issue (Tack opens the pull request from that link when there is none), and the board has a
GitHub token. Tack reads CI when you press it, not before:

- CI green: Tack merges and the task follows its workflow.
- CI still running, failed, or reporting nothing: nothing is merged, and the button and the task say
  why. Press it again when CI is done.
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

A project at L5 treats every change as L5 unless a domain that matches it is lower. A change that
matches several domains takes the lowest of their levels.

At L4 and L5 Tack merges a finished run only when all of these hold, in this order. The first
that does not is shown on the task, and a person reviews it as usual:

1. The run did not come through MCP.
2. The verifier said the change can merge.
3. The change's domain is at L4 or L5.
4. The team that ran the change does not need a person during a run (see [The ceiling](#the-ceiling-of-a-team)).
5. The verifier's pack lists every stage the project requires (at least one is needed) as
   passed. A stage it skipped as not applicable does not
   count.
6. The verifier's pack is signed, and the signature checks against the exact commit.
7. The project has not used up its tokens or its automatic merges for today.
8. The pull request is open, and CI on that commit is green.

The Merge step says what on this board stops an automatic merge: GitHub polling off, no verifier
installed where the board runs, no public key, a team that needs a person. While any of them holds,
L4 and L5 are shown disabled with the reasons ("L4 and L5 are off: …"). Each domain shows its merged and
undone changes beside its level.

**Demotion is automatic; promotion is yours.** When a change Tack merged by itself is
reverted, or CI fails on its merge commit, its domain drops one level and the task's
activity says so. Only you raise a level again.

### What it needs

An administrator sets most of this up; see
[Installing and Operating Runners](../admin-guide/runners.md#after-a-succeeded-attempt) and the
[Configuration reference](../admin-guide/configuration-reference.md).

- A verifier on the runner, set to sign its packs, and the same verifier on the machine the board runs
  on, to check the signatures.
- The project's public key, which you paste into **Public key** in the Merge step.
- The board's GitHub token and polling. The automatic merge runs on each poll.
- A task linked to a GitHub issue, since that is how its pull request is opened.
- The required stages filled in (the box labelled "Stages the verifier must run"). It starts empty, and at least one
  stage is needed. **Fill the example stages** puts in a starting set.

### The ceiling of a team

The team that ran the change sets a ceiling too. A team that needs a person during a run keeps its tasks at
L3 however high the project's level is: the automatic merge holds them and says "The team that ran this
stays at L3: it needs a person during a run", and the Ceiling step lists the team and what to change; see
[Ceiling](automation.md#ceiling). The Merge button is not held by the ceiling.

## Sending a rejected change back to the agent

A project can let a change the verifier rejects go back to the agent first, with the
verifier's list of what to fix (or, when it gave none, the checks that failed), up to a number of times you set (0, the default, is never).
It stops sooner when two rejections in a row give the same reasons, when the verifier asks
for a person instead, or when you cancel the follow-up run; then the task waits for your
review with every attempt kept.

## Budgets

Each project can cap three things, set under **Daily limits** in the Merge step. Leave a box empty for no
limit; nothing has one until you set it.

| Limit | What happens when it is reached |
|---|---|
| **Tokens per day** | New runs do not start, and no automatic merge happens |
| **Items waiting at once** | New runs do not start while that many tasks wait for your review |
| **Automatic merges per day** | No further automatic merge happens that day |

**Tokens per day** is reserved when a run starts, so the runs of one day cannot add up past it. A run starts
only if the tokens counted today, what the runs already going may still use, and its own token limit fit in
the day. Otherwise it waits and says why: "Today's token limit is reserved by 2 open runs (1 600 000 of
2 000 000)". A run with no token limit of its own is given what is left of the day as its limit, and says so.
Tokens are counted as [Token limits](running-agents.md#token-limits) explains: what the cache served does
not count. A bar under the limit shows the day: counted, reserved and what is left, amber from 80%.

An agent Tack cannot stop at a limit (Codex today) can use more than it was given, so its run can still
take the day past its limit; docket, Claude Code and opencode are stopped at it.

A limit that stops new runs says which one on the task; runs already going finish. A day starts at midnight
UTC. There is no limit in dollars: a dollar figure is a price list applied to tokens, and a model with no
price would pass it.
