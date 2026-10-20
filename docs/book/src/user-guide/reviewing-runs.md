# Reviewing a Run

This page is for whoever checks what an agent did. After reading it you can find a finished run, judge its
changes, accept or reject them, read the merge-readiness pack a verifier leaves, see what the run used and
cost, and read the project's Factory metrics page.

## What happens when a run finishes

A run that left changes marks the task **Needs your review**. This is a flag on the task, separate from your
board columns. Board, list and table all show it. It clears when you press **Accept** or **Reject** on the
attempt. If the project chose a column for finished runs (under **Settings → Automation**, see
[When a run finishes](automation.md#when-a-run-finishes)), the task also moves there.

Open the task and go to its **Execution** tab. Chips pick the run, and the run you picked leads with the
outcome and the tools to check it:

- **Accept** and **Reject…**, in a banner above the run while it waits for you; **Reject…** asks for a note,
  which is optional.
- **Open diff** shows what changed.
- **Open folder** (or **Copy path**, when you are not in the desktop app) shows where the work is on this
  computer.
- **Copy branch** copies the branch name, when the agent worked on a branch.
- **What the agent concluded**, the agent's report as written, and **Why it stopped**.
- **What the agent did**, by tool or as a timeline.
- **Raw log**, beside the run's ids, opens the full log.

**Succeeded** is green only when the changes were captured. It is amber when a run that was not meant to
change files changed nothing, or when the changes could not be read. A run is meant to change files when its
tools include one that writes, or, when its tools are all tools, unless its profile is a Reviewer, Researcher
or Planner. A run that was meant to change files and changed nothing is shown as **Failed**, with the reason
"The agent reported success but delivered no change to review", and the number of files the agent left
uncommitted when there were any. A failed run leads with why it failed.

### What you see on the board

After a run, the task's card on the board carries these badges. List and table carry the review, merge and
automation badges; the state chip is on the board only.

| Badge | Meaning |
|---|---|
| A state chip on the card header (Queued, Starting, Running, Asking you, Finished, Failed, Cancelled) | Where the latest run is. Press it to open the task's Execution tab |
| **Needs your review** | The latest run left changes and nobody has accepted or rejected them |
| **Waiting for CI to merge**, **Not merged: …** | Tack tried to merge the change and could not. Press the badge for the reason and what to do |
| **Started by itself** | The project started the run, not a person |
| **Team: name** | The team the run's profile named |
| **Merged by itself** | Tack merged the change automatically |
| **Repeats** | The task is set to repeat when closed |

The badges that start with Started, Team and Merged are explained in
[Automating a Project](automation.md#what-tack-shows-when-it-acts).

### A Planner run

A task run with the **Planner** profile changes no files. Its attempt shows **The Planner proposes N
subtasks**, each with a checkbox and its title, description and acceptance criteria editable. Nothing is
created until you press **Create selected**, so no agent writes to your board on its own.

### Merging a reviewed run

When the run's branch was pushed and its pull request is open, press **Merge** on the run. See
[Merging from Tack](merging.md).

---

## Reviewing a merge-readiness pack

When a verifier checks an attempt, it leaves a merge-readiness pack. Open **Show questions &
files** on the attempt and the pack appears under **Merge-readiness pack**.

<img src="../screenshots/merge-readiness.png" width="50%" alt="A merge-readiness pack on a succeeded attempt: Recommendation merge, Risk low, three reasons, a criteria table with every criterion passed and its evidence file, the verify command with exit code 0, mutation score, static analysis counts, a blind judge's verdicts, and Your review with a required reason before Accept or Reject.">

From top to bottom it shows:

1. The verifier's **recommendation** and the **risk** level with its reasons.
2. Whether the change was merged and by whom: "Merged by you", or "Merged by Tack" with the level and
   domain, or "Not merged:" with the reason.
3. **Next steps**, when the verifier lists what to fix.
4. **Criteria**: one row per criterion from the task's brief, with its status and evidence.
5. **Verify**: the check command with its exit code and output.
6. **Mutation** and **Static analysis**: what the verifier measured.
7. **Judge**: a verdict on each criterion, where a judge ran.
8. **Your review**.

A section the verifier did not run says **Not run**. That is not a pass: nothing was checked.

To record your verdict, write a reason, then choose **Accept** or **Reject**. Both stay disabled until the
reason has text, and the line "Write a reason to enable Accept and Reject" says so. The first review is
final; a second one is refused. This verdict belongs to the pack. It is separate from the Accept and Reject
buttons on the attempt card, which clear **Needs your review**.

If you started the run from the command line with `--status-map-policy done_on_mrp_accepted`, accepting the
pack also moves the task to its workflow's first Done status; see [CLI](cli.md#runs).

When the verifier rejects a change, a project can send it back to the agent with the list of what to fix;
see [Sending a rejected change back to the agent](merging.md#sending-a-rejected-change-back-to-the-agent).

---

## Usage and cost

Each finished attempt has a **Usage** panel. It leads with tokens, because that is what the agent measured:

- **The tokens counted**, the figure every limit uses, with the tokens in and out beside it, and the part
  of the input that was read from the agent's cache (not counted) or written to it. Hover a figure for the
  exact count.
- **The models** the run used, and, where it matters, whether the agent named the model that answered or
  Tack only knows the one you requested.
- **Model calls** and how long the run took.
- **Cost**, in two cases. When the agent reports an estimate, as Claude Code does, the panel shows
  "≈ $x.xx (estimated)" and says it is the agent's own estimate at list price, not your bill. When the agent
  reports none, the panel says **Cost: Not measured**. Tack never shows `$0.00` for a figure it does not
  have, because zero would claim the run was free.

If an agent does not report tokens, the panel says **Tokens: Not measured**. The task's summary of its latest
run gives the counted tokens, with what the cache served beside them, and the run's token limit with who
applies it; see [Token limits](running-agents.md#token-limits).

The Factory metrics page, below, adds up cost for a whole project in a different way: it prices the tokens of
every run with a rate card you enter once.

---

## Factory metrics

**Factory metrics** is in a project's sidebar (the page is `/projects/<id>/factory`). It summarises all the
runs, questions and verification in a project, as one card per measure. The page covers all time. Each card
shows a figure, or **Not measured** with its reason when there is nothing to measure.

| Card | What it tells you |
|---|---|
| **Escalation rate** | How often a run needed you: questions asked divided by runs |
| **Human minutes per decision** | The median and the slowest tenth of the time a question waited for your answer, counted from when someone first opened it (or from when it was asked, if nobody opened it first) |
| **Human minutes per pack review** | The same wait for a merge-readiness pack and its verdict |
| **Pack acceptance rate** | Accepted packs divided by reviewed packs, with how many are still unreviewed |
| **Verification tax (tokens)** | Tokens spent on verification and rework, divided by tokens spent on first attempts |
| **Outcomes** | Pull requests opened, merged, closed and reverted, and how many were merged and not reverted |
| **Cost** | An estimate in dollars for the project: tokens priced by the rate card, plus the human minutes above at the card's hour rate, with how many of the models used have a price |
| **Quality** | The figures the verifier reports for each repository. Tack shows them and does not compute them. Where the verifier cannot answer, the card says why |

When a card has nothing to count, it says **Not measured** and why, never `0`.

### Cost by model and by team

Under the cards, **Cost by model** lists, for each model, the counted tokens in and out, the tokens the
cache served (shown, not counted), the number of attempts and the estimated cost. A run that named no model and whose agent reported none appears as **No model named**. **By team**
appears when at least one run went to a team, and groups the same figures by team.

A row shows **not priced** in its Cost column when there is no rate card, or when the card does not list
that model. The **Cost** card then says how many attempts were left out.

### The rate card

The **Rate card** form at the bottom of the page tells Tack what things cost. It applies to the whole board,
not to one project.

1. Write the date the prices were read, in **Prices as of**.
2. Add one row per model: the model's name exactly as a run names it, and its price in US dollars per
   million input tokens and per million output tokens.
3. Fill in the price of a CI minute and of a human hour.
4. Save. Every Cost figure on the page updates.

A run is priced as the model the agent reported, or else the model you requested. A model that is not on the
card cannot be priced. The CI-minute price is kept on the card but is not part of the Cost figure yet.

The project's daily dollar limit uses the same card: see [Budgets](merging.md#budgets).
