# ADR 0033: the screen says what needs you first, and a project shows its teams at work

**Status: ACCEPTED 2026-11-01** (proposed and accepted the same day). Executed by Phase 84
(`docs/closed-cycles/plans/phase-84.md`). Keeps ADR 0032's rule: nothing on the screen names a harness; a team, its
members and its steps are what any agent reported, and what it did not report is shown as not reported.

**The rule behind every decision.** Every control, setting and fact on screen today stays. What changes
is order, weight and form: one run-state language, one primary action per place, summary before detail
before raw, and a graphic wherever a sentence was drawing a diagram. The one exception is the Table
view, which is removed, and its abilities move to the List.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **Starting a run can move the item.** A project setting "When a run starts" sits beside "When a run finishes", with the same choices. New projects start with the workflow's first in-progress status; existing projects keep "Do not move" | A task whose run is working is in In Progress, not in Backlog |
| 2 | **A card shows priority only when it is not the project's default.** The List, the item and the filters always show it. "Always show priority on cards" in Display & accessibility brings back today's card | Every card no longer says "Medium" |
| 3 | **"Propose items from a file" moves** from the top of every item to the item's "⋯" menu and to a drop zone on the Execution tab. It opens the same accept panel | The first screen of an item is what to do now, not a file input |
| 4 | **The Table view is removed; the List does what it did.** The List gains sorting by a column, a filter, editing a cell in place (title, status, priority, assignee, due date), choosing columns and two row heights. `/table` and a remembered "table" view open the List | Five work views instead of six, and the one that shows the hierarchy can also sort and triage |
| 5 | **A new work view, Orchestration** (a project can rename it), in two modes. **Flow**: one lane per team that works in the project, its stations being the team's own steps as the agent reported them, each task placed at the step it is in, with what waits on a person, what nobody will pick up and what each team is spending. **Map**: who does what, from roles to profiles to teams to members, and the teams compared side by side. A project where no agent has ever run shows the view disabled with its reason | The work of several teams can be followed as it moves, and the reason a task goes to one team and not another can be seen |

**Amended 2026-11-02 (Phase 86, `docs/closed-cycles/plans/phase-86.md`).** Decision 1 is reversed: a run that starts no
longer moves its item, and neither does a run that finishes. A task's status is a person's, and its
work is what its runs report; the two are shown apart. Nothing else in this ADR changes.

**Why now.** The live suite's screenshots of 2026-11-01 and the My SaaS onboarding of 2026-10-31 show
that the board keeps tasks in Backlog while their runs work, that the item and run screens put the one
thing to do below a dozen sections of equal weight, and that nothing on screen shows a team's process,
its queue or the path from a role to a team: those are read today from three settings pages and an
Execution tab at a time.

**If you do nothing.** A person running several teams reads every card's pills to find what needs
them, opens runs one by one to learn where each stands, and cannot see which tasks no team will take
until auto-start fails to start them.

## Evidence

- Rerun `rerun-local-00e2b25d-b` (git-ignored `onboarding-lab/results/docket-prill/`), shots 13, 16, 30,
  37: items with Running, Finished, Needs your review and Failed runs stay in Backlog; In Progress shows
  0 until auto-start moves the proposed items.
- Shots 22, 23, 26: a decision id with no break widens the item drawer and pushes its content off the
  left edge.
- Shots 11, 14, 22: the item's first screen is properties and a file input; "Run again" appears twice.
- Shots 15, 17, 20, 24, 25: a run is about twelve sections in one scroll; the decision-token field shows
  on runs that asked nothing.
- Shots 12, 19, 21: the run dialog repeats one capabilities sentence per team; teams with no profile
  take a full card each.
- Shots 04, 07, 08: the agents' capabilities are measurement notes in four columns; the merge ladder,
  its blockers and the ceiling say the same thing in three places.
- Shot 36: the accept panel's dependencies are numbered checkboxes.
- The user guide (`views.md`) sends a person to the Table to sort and filter because the List cannot;
  that is the Table's only use the List lacks.
- The data for Flow exists: `GET /api/teams` (members, steps, queue, works in, decides), the item's
  latest execution, and the hop events `stepMatch.ts` already matches to steps. What is missing is one
  call that returns a project's open runs with the step each is in.

## Considered and not adopted

- **A run-state swimlane on the Board.** The Board's columns are the person's workflow; a team's steps
  differ per team, so they get a view of their own.
- **Keeping the Table and hiding it.** Two views over the same rows, one of them hidden, is two things
  to keep for one use.
- **A free-form graph canvas of agents.** Teams report ordered steps, not arbitrary graphs; lanes of
  steps show exactly what was reported.
