# Troubleshooting & FAQ

This page is for people who use Tack every day and hit something unexpected. Each entry gives the symptom, what causes it, and what to do. Problems with installing, exposing, securing or recovering the server, and with installing agents on a runner, belong to whoever runs it: the table points to the [Administrator Guide's troubleshooting page](../admin-guide/troubleshooting.md) for those.

---

## Find your symptom

| Symptom | Go to |
|---|---|
| The page is blank, or the board never loads | [The board does not load](../admin-guide/troubleshooting.md#the-board-does-not-load) (Administrator Guide) |
| A label or the theme "didn't change" | [A vocabulary or theme change did not apply](#a-vocabulary-or-theme-change-did-not-apply) |
| The Run with agent button says execution is off | [Agent execution is off](#agent-execution-is-off) |
| A run succeeded but there is no pull request, pack or branch | [A run succeeded, but there is no pull request, pack or branch](#a-run-succeeded-but-there-is-no-pull-request-pack-or-branch) |
| The Start switch is greyed out, or auto-start turned itself off | [Auto-start is off, and says why](#auto-start-is-off-and-says-why) |
| A task reached the Start column and nothing happened | [A task did not start by itself](#a-task-did-not-start-by-itself) |
| `tack runner doctor` says "needs docket 0.2.0-beta.6" | [A runner shows "needs docket 0.2.0-beta.6"](../admin-guide/troubleshooting.md#a-runner-shows-needs-docket-020-beta6) (Administrator Guide) |
| The project never merges by itself, and Settings names a team | [A team is attended](../admin-guide/troubleshooting.md#a-team-is-attended) (Administrator Guide) |
| An import says "This file cannot be imported" | [A project file was refused](#a-project-file-was-refused) |
| An import says a section could not be written | [A section failed to write](#a-section-failed-to-write) |
| Port in use, database locked, failed migration, `401`, CORS, upload limits | [Server problems](#server-problems) |

---

## A vocabulary or theme change did not apply

**Cause.** Light or dark mode and the colour palette are saved in your browser, not on the server, so they follow the browser rather than your account. Vocabulary is saved per project on the server, but a tab that was already open can still show the old words.

**What to do.** Refresh the page, with a hard refresh if needed. If a theme looks wrong only in one browser, clearing that browser's data for Tack returns it to the defaults. See [Appearance](appearance.md) and [Vocabulary](vocabulary.md).

---

## Agent execution is off

**Cause.** Tack is a project manager first. Until agents are turned on for this computer, the **Run with agent** button shows "Agent execution is off" instead of a form.

**What to do.** Open **Agents** in the sidebar and press **Turn on**, then sign in to the agent you want from its own command. [Running Agents on Tasks](running-agents.md#agents-on-this-computer) walks through it. To attach runners on other computers, see [Installing and Operating Runners](../admin-guide/runners.md).

---

## A run succeeded, but there is no pull request, pack or branch

**Cause.** Everything after a succeeded run is optional and none of it changes the run's result. The run's timeline says which step was skipped and why: verification not set up, the branch not pushed, or a task that is not linked to a GitHub issue, which a pull request needs.

**What to do.** Read the skipped step on the run's timeline. To open pull requests, link the task to its issue (in the **Link GitHub issue** section of its Overview tab). Verification, pushing and the GitHub token are set up by whoever installed the runner; the steps are in the [Administrator Guide](../admin-guide/troubleshooting.md#a-succeeded-run-has-no-pull-request-pack-or-branch). Then run the task again.

---

## Auto-start is off, and says why

**What you see.** In **Settings → Automation**, the Start rung's switch **Start a task by itself** is greyed out with the line "Choose a profile for at least one role in Teams". Or the switch was on and is now off, with a line such as "Auto-start is off: the project dropped to L3".

**Cause and fix.**

- **No profile.** Auto-start only starts a task whose role leads to a profile. Until at least one role has a profile, or the project has a default profile, the switch stays off. Choose a profile for a role in the Teams rung, then switch Start on.
- **The project dropped to L3.** When a pull request Tack merged by itself is reverted, or goes red on its merge commit, Tack takes the merge level down to L3 and switches auto-start off. The task's activity says which pull request. Look into that merge, raise the level again under **Settings → Automation**, then switch Start on again.

Auto-start is off by default for every project. A project file never switches it on; see [A project file was refused](#a-project-file-was-refused).

---

## A task did not start by itself

**Cause.** Tack starts a task only when every condition listed under [Start](automation.md#start) holds: the switch is on and the task is in the chosen status, it has a role that leads to a profile and an agent, a machine can run it, nothing is already open on it, nothing blocks it, and the project's work-in-progress, daily limits and open-run cap have room.

**What to do.** Open the task. Its **Automation** row says what happens to it: "Starts by itself when it reaches …", "By hand: this project doesn't start tasks by itself", or that a role has no profile. Tack looks about every 30 seconds, so a start can lag a little; if it never comes, the administrator can check that the look is switched on, as the [Administrator Guide](../admin-guide/troubleshooting.md#auto-start-does-not-tick) explains. Until then, start the task by hand with **Run with agent**.

---

## A project file was refused

**What you see.** Importing a file shows **This file cannot be imported** and a list, or `tack import` prints the problems and writes nothing.

**Cause.** The whole file is checked before anything is written, and every problem is listed at once. Each line names the place and what is wrong, for example `roles: "Contenido" names profile "Writer", which is in neither the file nor the board`.

**What to do.** Fix the lines, top to bottom, and import again. The usual ones:

- an unknown key, which is usually a misspelling;
- a name used twice in one section, or a name that points at nothing (a role's profile, an item's role, sprint or field);
- a status the workflow does not have in `on_finish_status` or the auto-start status;
- a field value that is not among the field's options.

The full list of refusals, and the warnings that do not stop an import, is in [Import, Export & Project Files](import-export.md#what-is-refused-and-what-is-only-a-warning). A file that asks for a higher merge level or for auto-start is not refused: Tack applies the level it can and leaves auto-start off, and the preview says so.

---

## A section failed to write

**What you see.** The import answers "The roles section could not be written", or names another section.

**Cause.** The file was valid, but writing one section failed, for example because the database was busy or the disk was full.

**What to do.** Read the rest of the message. For a new project it says "so nothing was created": fix the cause and import again. For an existing project it says what the file wrote before that section stays, and "importing it again finishes the job": sections written so far match by name and are updated, and the items are written last, in one step. If it fails again, ask the administrator to look at the server log.

---

## Server problems

These belong to whoever runs the server. If you run it yourself, the [Administrator Guide](../admin-guide/index.md) has the steps.

A blank page, a server that exits at once, a locked or damaged database, a failed migration, `401 Unauthorized` on every request, CORS errors in the browser console, uploads rejected as too large and a restore that did not apply are server-side problems. The [Troubleshooting page of the Administrator Guide](../admin-guide/troubleshooting.md) has an entry for each:

- [The board does not load](../admin-guide/troubleshooting.md#the-board-does-not-load);
- [The server says the port is in use](../admin-guide/troubleshooting.md#the-server-says-the-port-is-in-use);
- [The database is locked](../admin-guide/troubleshooting.md#the-database-is-locked);
- [Startup fails with a migration error](../admin-guide/troubleshooting.md#startup-fails-with-a-migration-error);
- [Every request answers 401](../admin-guide/troubleshooting.md#every-request-answers-401);
- [The browser reports a CORS error, 421 or 403](../admin-guide/troubleshooting.md#the-browser-reports-a-cors-error-421-or-403);
- [An upload or import is rejected as too large](../admin-guide/troubleshooting.md#an-upload-or-import-is-rejected-as-too-large);
- [A staged restore did not apply](../admin-guide/troubleshooting.md#a-staged-restore-did-not-apply).

For ports, tokens and allowed origins, [Administration & Security](../admin-guide/administration.md) has the settings; for a damaged database or a failed upgrade, see the [Recovery Runbook](../admin-guide/recovery-runbook.md) and [Backup & Restore](../admin-guide/backup-restore.md).

---

## FAQ

### Is my data sent anywhere?

Tack itself sends nothing to Anthropic or to any Tack-operated service. Projects, items and comments live in a database on the machine that runs the server. Outbound traffic happens only for features you set up: a webhook, a GitHub or Linear import, a cloud backup. When you run an agent, that agent talks to its own provider with its own sign-in, as it would if you ran it by hand; Tack holds no model key.

### Can I use Tack offline?

Yes, as far as Tack goes. It runs on your own machine or network and needs no outside service. The server has to be running, because the web UI and the CLI both talk to it. An agent you run needs whatever connection its provider needs.

### Where is my data stored?

In two places on the machine that runs the server: a database, and a folder of attachments, organised by item. By default both sit in the folder you started the server from; a background service started with `tack service install` uses your per-user application-data folder. The [Administrator Guide](../admin-guide/configuration.md) says how to move them.

### How do I back up my data?

Use Tack's backup rather than copying the database file while the server runs. See [Backup & Restore](../admin-guide/backup-restore.md). For one project's items, [export a snapshot](import-export.md#snapshots-and-csv-export).

### Is Tack for one person or a team?

Both. Several people can use the same server, and changes on the board reach every connected browser at once.

### Which platforms does it run on?

Linux, macOS and Windows, as a single binary or as the desktop app. [Quick Start](quick-start.md) has the install lines.

### How do I put Tack on a network safely?

That is the administrator's job: require a token, limit the allowed origins and put HTTPS in front. See [Administration & Security](../admin-guide/administration.md).

### Does Tack work with GitHub or Linear?

Yes. You can import issues from both, and with a GitHub token the administrator sets, linked issues and items stay in sync both ways. See [Import, Export & Project Files](import-export.md#importing-github-and-linear-issues).
