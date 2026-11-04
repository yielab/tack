# A Team

A **team** is whoever does a card's work: one agent, or several working together. You do not build teams in
Tack. An agent keeps its own teams and tells Tack what they are, and Tack shows you exactly that. This page
answers the six questions you ask of any team, whether you have one or five. The **Agents** page, under
**Teams**, has a sheet for each team an agent reports, with one heading for each question, and each section below
says what that part of the sheet holds. Two of the answers depend on a project (how far a team goes in it, and
which of its roles send it work), so choose the project at the top of **Teams** to read them.

Teams of several members come from docket. Claude Code, Codex and opencode are single agents, and Tack counts
each as a team of one: it has a sheet of its own, marked **A team of one**, that reads "One agent, `claude-code`"
with the model it runs on. Such a team delivers a branch, has one way of working (the agent's own loop), and
needs only what the runner's check says, that it is signed in and, where the agent will not choose one, that a
model is named. Profiles that name no agent are listed under **Profiles with no team**.
How a profile chooses its agent and team is in [Automating a Project](automation.md#teams); how docket is
installed is in [Installing and Operating Runners](../admin-guide/runners.md).

---

## What is it for?

The top of a team's sheet says which agent runs it ("Run by Docket"). Under **What is it for?** are a badge with
the team's shape, as the agent names it, where it works, and what it hands back when a task ends:

- **Works in this repository.** It writes a branch of your project, and the run ends with changes you can review
  and merge.
- **Works in a folder.** It writes a report, keeps a list or researches. There is no branch to give back, so the
  run ends with what the team's last step said it did, and nothing to merge. See
  [Teams and docket](automation.md#teams-and-docket).

A team that keeps record sets lists them with how many rows each holds. A team that names no profile has
**Make a profile for this team** at the foot of its sheet.

## Who is in it?

The members are rows; a team of one reads "One agent" and the agent's name instead. Each member shows the job the agent gives it, the model it runs on, and up to two icons:
**Reads only** or **Changes the project**, and **Can propose items**. The job named on a member is the team's own
word for what that member does. It is not a role of your project: your roles say which area of the board a card
belongs to, a member's job says who inside the team does a step.

## What happens to a card?

The sheet lists the **methods** the team offers, marks the one a card goes through when it names none, and warns
of a method with no member for one of its steps. The **steps** of that method are drawn in order, each with who
does it and what checks it. A step that waits for a set
time is a dashed box of its own. Where a failed check sends the work back, and how many times, is on the step:
"sent back to write up to 2 times". So you can read from the card what a task goes through before anyone calls it
done: written, reviewed by another member, checked by a command.

A profile can also name a different way of working for each kind of card, for example one for bugs and one for
features. The sheet reads them back ("A bug goes through tdd (Implementer)"). They are set in the profile's
editor, under **Method for each kind of card**: when the team says which ways of working it has, each kind gets a
choice among them, and otherwise you type the name and nothing checks it until a run. Saving a profile that
names a way of working the team does not have is refused, naming the ones it has; a way of working whose steps
the team has no member for is saved with a warning that stays on the form, and a run of it is refused until the
team has the member. A profile that runs a single agent has one way of working, its own loop, and the field is
there, greyed, saying so. The same names can be written in the [project file](import-export.md).

What the team did with a particular task is on the task's Execution tab: each step, who did it, what it was
given, and how long it took. See [Reviewing a Run](reviewing-runs.md).

## When does it ask?

The sheet gives the team's **Supervision**, says **Runs to the end by itself** or **Stops to ask a person**, with
what to change in docket so that it runs to the end, and says whether it reaches the network and whether it
proposes items. What the team's work still needs reads "The operator still has to…" with what to do, for example
signing an agent in on the runner's computer; a need already met reads "Ready". A team that stops to ask limits how far Tack may merge by itself: see
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
  the docket command the sheet shows, and the waiting tasks go on.

For the project you chose, the sheet also says whether the team's tasks stay at a level where a person merges, and
what to change if you want otherwise, and the most the project can reach with the teams it uses. It names the
folders the team's sandbox may read besides its own. A line the agent did not report reads "Not reported by
Docket", so "no network" and "the agent did not say" are never mixed up.

Dollars are never a limit.

## How do I send it work?

The sheet lists the roles of the chosen project that send work to the team, and the profiles that use it; press a
profile to edit it. A card goes to a team through a profile, and a profile through a role:

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
