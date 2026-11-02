# Managers and Threads

This page is for the person who wants an agent to look after a project and answer in words, not only by doing
a task. After reading it you can set up a manager, talk with it in a thread, let events wake it, apply the
changes it proposes, and know when a thread ends.

## What a manager is

A **manager** is an ordinary agent profile that works through the board. It has no special type. It reads
a digest of the project (what waits on a person, what is blocked, what failed), says who should do which task,
and reports to you in a thread. It changes the board only through **change sets**, which wait for you to
apply them unless the project lets agents act; see [How far an agent may
go](reviewing-runs.md#how-far-an-agent-may-go). It reaches Tack with an agent credential that can read,
propose and converse on the projects you named, and nothing else. Credentials are described in the
Administrator Guide's [Agent credentials](../admin-guide/administration.md#agent-credentials).

A manager that runs on a docket team works at L3 or L4. docket asks a person before a call such as sending
a message, and at L5 nobody is there to answer, so docket refuses the call and the run ends as **Needs a
person**.

## Set up a manager

Open **Agents** and use **Add a manager**. Choose the project it looks after and its agent profile: one you
already have, or **Make a new profile for it** on the agent you pick. Press **Add manager**. Tack then:

1. makes the profile, if you asked for a new one, with general instructions that name nothing of your project;
2. issues a credential with **Read**, **Propose** and **Converse** on that project, signed with the profile's name;
3. opens a thread with the profile;
4. adds three triggers: failed runs, a change set that has waited an hour, and a digest on weekdays at 09:00;
5. shows the MCP configuration for the agent, with the credential's secret in it.

Copy the configuration now. The secret is shown once, and Tack keeps only a hash. For Claude Code, save it as
`.mcp.json` in the folder the agent works in; other agents take the same `command`, `args` and `env`. A
credential expires after 90 days; rotate it under **Agent credentials** in Settings.

The project's **Agents on the board** setting is left as it was, **Propose** unless you changed it, so every
change the manager wants waits for you. If a step fails, the page lists what was made so you can finish the
rest by hand. Nothing here is special: the profile, the credential, the thread and the triggers appear with
the others in Settings and can be changed or removed one by one.

## Triggers

A **trigger** wakes an agent profile on its own. It waits for something that happens on a board (a run
fails, a run waits for you, a question is asked, a task is created or moves, a change set has waited for you
too long) or for a time of day on chosen weekdays. A project's triggers are in **Settings → Automation**, in
the **Agents** section. Each has a name, the profile it wakes, an on switch, how long things must stay quiet
before it starts, and the most runs it may start in a day.

- **One run takes everything since the last one.** Events that arrive close together wait until things have
  been quiet for the trigger's quiet period, then start one run that is told about all of them (up to 50,
  with a count of the rest) together with the digest. If events never stop, the run starts after an hour anyway.
- **Never twice at once.** A trigger does not start a run while its previous run is still going, and it stops
  for the day when it has started as many runs as its cap allows. What happened meanwhile waits for the next run.
- **An agent's own changes never wake it.** When a trigger names a credential, whatever that credential
  changed on the board, such as a change set it applied, is left out, so a manager does not wake itself. What
  you change does wake it.
- **Limits still apply.** A run a trigger starts is an ordinary run under the profile's token limit and the
  project's rules. A weekday time is read in the time zone of the browser that set it, and the zone is shown
  beside the time.

Turn off a trigger you do not need, or delete it. A deleted trigger takes its unread events with it.

## What a thread is

A **thread** is a conversation between you and one agent profile, in one project. You can open as many as you
like. Each has a title, which is only there to tell your threads apart, and the profile that answers.

Open **Threads** in the sidebar. The list shows each thread with the profile that answers, whether it is
open or closed, and when it was last written in. Pick one to read it. To start one, press **New thread**,
give it a title, choose who answers, and press **Start thread**. If the project has no agent profile yet, the
form stays greyed out and says so, with a link to the Agents page.

## Each message is one turn

When you send a message, the profile runs once to answer it. That run is a **turn**. It reads the digest of
the project, your latest messages in full and the earlier ones as their first line, and then it writes its
answer, which appears in the thread under the profile's name. When the profile belongs to a docket team, the
answer is what the team's last step handed off.

While the turn runs, the thread shows it as work in progress under your message. If the agent needs an
answer from you before it can go on, the question appears right there, and you answer it in the thread. A
turn that cannot answer says so under your message and the reason appears as the next message. Nothing is
lost: write again, or close the thread and start another.

Only your messages start a turn. A manager can also write in the thread on its own, for example to report
something or to raise an alert; that message simply appears in the thread, in the manager's name, and nothing
runs because of it. You answer it, or not, when you want.

A thread answers one message at a time. While a turn is running, the box for your message is greyed out and
says why; it opens again when the answer has been posted. If a message is refused anyway, because another
turn started a moment before, the thread says that the message was not sent and why, and keeps what you wrote.

## Changes an answer carries

An answer may propose changes to the board: new items, who does an item, a priority, a move, a dependency, a
comment or a due date. They appear inside the answer as a **change set**, in the same panel as everywhere
else: tick the changes you want, edit them, and press **Apply**, or press **Reject** and say why. Nothing
changes on the board until you apply it, except what the project's **Agents on the board** setting lets
through on its own. See [When an agent proposes changes](reviewing-runs.md#when-an-agent-proposes-changes).

## When a thread ends

A thread closes when:

- you press **Close thread**;
- no one has written in it for 7 days; or
- it has used up its turns (50 in all).

A closed thread stays readable, its change sets can still be applied, and its box says why it is closed. To
go on, start a new thread.
