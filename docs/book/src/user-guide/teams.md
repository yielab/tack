# A Team

A **team** is whoever does a card's work: one agent, or several working together. You do not build teams in
Tack. An agent keeps its own teams and tells Tack what they are, and Tack shows you exactly that. This page
answers the six questions you ask of any team, whether you have one or five. The **Agents** page, under
**Teams**, has a card for each team an agent reports, and each section below says which part of that card to read.

Teams come from docket. A profile whose agent is Claude Code, Codex or opencode is a single agent, not a team:
it appears under **Profiles with no team**, and the questions below still have an answer for it, a shorter one.
Tack counts such an agent as a team of one: it delivers a branch, has one way of working (the agent's own loop),
and needs only what the runner's check says, that it is signed in and, where the agent will not choose one, that
a model is named.
How a profile chooses its agent and team is in [Automating a Project](automation.md#teams); how docket is
installed is in [Installing and Operating Runners](../admin-guide/runners.md).

---

## What is it for?

The top of a team's card says which agent runs it ("Run by Docket"), a badge with the team's shape, as the agent
names it, and where it works:

- **Works in this repository.** It writes a branch of your project, and the run ends with changes you can review
  and merge.
- **Works in a folder.** It writes a report, keeps a list or researches. There is no branch to give back, so the
  run ends with what the team's last step said it did, and nothing to merge. See
  [Teams and docket](automation.md#teams-and-docket).

Under the card, **Profiles that use it** lists the profiles that send work to this team. A team that no profile
names has **Make a profile for this team**.

## Who is in it?

The members are rows. Each shows the job the agent gives the member, the model it runs on, and up to two icons:
**Reads only** or **Changes the project**, and **Can propose items**. The job named on a member is the team's own
word for what that member does. It is not a role of your project: your roles say which area of the board a card
belongs to, a member's job says who inside the team does a step.

## What happens to a card?

The card's **steps** are drawn in order, each with who does it and what checks it. A step that waits for a set
time is a dashed box of its own. Where a failed check sends the work back, and how many times, is on the step:
"sent back to write up to 2 times". So you can read from the card what a task goes through before anyone calls it
done: written, reviewed by another member, checked by a command.

A profile can also name a different way of working for each kind of card, for example one for bugs and one for
features. That is set in the profile's definition in the [project file](import-export.md). When the team says
which ways of working it has, saving a profile that names another is refused, naming the ones it has; a way of
working whose steps the team has no member for is saved with a warning, and a run of it is refused until the team
has the member. A profile that runs a single agent has one way of working, its own loop, and cannot name another.

What the team did with a particular task is on the task's Execution tab: each step, who did it, what it was
given, and how long it took. See [Reviewing a Run](reviewing-runs.md).

## When does it ask?

The card says **Runs to the end by itself** or **Stops to ask a person**, with what to change in docket so that
it runs to the end. A team that stops to ask limits how far Tack may merge by itself: see
[The ceiling](automation.md#ceiling).

When a team asks, the question waits in the **Now** panel and on the task's Execution tab, under **Needs you**.
Answer it there, or in docket's own command, and the question closes either way. A team about to do something that
leaves it, such as sending an email or posting somewhere, shows the whole action before you answer, and Tack
never asks you to approve one it cannot show. See [Asking before acting](running-agents.md#asking-before-acting).

## How far does it go?

Three limits apply at three levels, and each says who applies it:

- **The run.** A token limit on the run, from the profile, the dialog or the project's provisional limit. Tack
  applies it to a single agent; docket applies it to the whole task for a team. See
  [Token limits](running-agents.md#token-limits).
- **A step.** The rounds, tool calls and time one step may take are docket's own guards. A step that runs out
  says which limit stopped it and where to raise it, and you choose **Continue** or **Start over**. See
  [Reviewing a Run](reviewing-runs.md).
- **The team.** A team may have a budget of its own in docket. When it is spent the team is **Paused: budget
  spent**: it starts nothing, and a task sent to it waits. Tack has no button for it yet; resume the team with
  the docket command the card shows, and the waiting tasks go on.

The card also says what the team may reach: its **network**, the folders it may read besides its own, and the
records it keeps. A line the agent did not report reads "Not reported by Docket", so "no network" and "the agent
did not say" are never mixed up.

Dollars are never a limit.

## How do I send it work?

A card goes to a team through a profile, and a profile through a role:

1. Create or choose a profile whose **Team** field names the team.
2. Under **Settings → Automation → Who**, point a role of your project at that profile.
3. A card with that role goes to that team: when you press **Run with agent**, which says "Chosen by the role" and
   the role's name, or by itself, if **When** starts tasks by itself.

If the team cannot do what the run needs, Tack says so before sending anything, with the reason and the change
that fixes it: for example, "The team fab-research cannot reach the web and this run needs it", "The team
fab-plan cannot propose items and a Planner delivers nothing else", "The team fab-research delivers a report
and this run changes the repository", "The method secure-build would skip steps on the team fab-build: no member
plays security-vetter", or "codex is not signed in on the runner". The run reads **Not started**, not
**Failed**: nothing was sent and nothing was used. Tack checks only what the team reported. A fact it did not
report is never a reason to refuse.

To split a large piece of work across teams, make it a parent card with sub-items, each with the role of the team
that should do it (see [Working with Items](items.md)). Items an agent proposes for a plan are always yours to
accept: see [When an agent proposes changes](reviewing-runs.md#when-an-agent-proposes-changes).
