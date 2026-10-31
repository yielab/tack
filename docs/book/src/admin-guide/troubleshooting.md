# Troubleshooting the Server and Runners

This page is for the person who runs Tack and its runners. Each entry gives the symptom, the cause and
what to do. After reading it you can tell a server problem from a runner problem, and fix the common
ones without leaving the page more than once.

Problems that a person sees while working in the board, such as a theme that did not change or a project
file that was refused, are in the [User Guide's troubleshooting page](../user-guide/troubleshooting.md).

---

## Find your symptom

| Symptom | Go to |
|---|---|
| The server exits at once and says the port is in use | [The server says the port is in use](#the-server-says-the-port-is-in-use) |
| The server says it is refusing to bind an address | [The server refuses to start on a network address](#the-server-refuses-to-start-on-a-network-address) |
| Errors say `database is locked` | [The database is locked](#the-database-is-locked) |
| Startup fails with a migration error | [Startup fails with a migration error](#startup-fails-with-a-migration-error) |
| You restored a backup and the old data is still there | [A staged restore did not apply](#a-staged-restore-did-not-apply) |
| The page is blank or the board never loads | [The board does not load](#the-board-does-not-load) |
| Every request answers `401 Unauthorized` | [Every request answers 401](#every-request-answers-401) |
| The browser console shows a CORS error, or the server answers `421` or `403` | [The browser reports a CORS error, 421 or 403](#the-browser-reports-a-cors-error-421-or-403) |
| An upload or import is rejected as too large | [An upload or import is rejected as too large](#an-upload-or-import-is-rejected-as-too-large) |
| A run succeeded but there is no pull request, pack or branch | [A succeeded run has no pull request, pack or branch](#a-succeeded-run-has-no-pull-request-pack-or-branch) |
| `tack runner doctor` says "needs docket 0.2.0-beta.6" | [A runner shows "needs docket 0.2.0-beta.6"](#a-runner-shows-needs-docket-020-beta6) |
| docket shows as absent, or the daemon is not started | [docket is absent and the daemon does not start](#docket-is-absent-and-the-daemon-does-not-start) |
| A team is reported as attended and tasks stay at L3 | [A team is attended](#a-team-is-attended) |
| A queued run shows "Waiting: …", or failed after thirty minutes | [A queued run says why it waits](#a-queued-run-says-why-it-waits) |
| Projects with auto-start on never start a task | [Auto-start does not tick](#auto-start-does-not-tick) |
| A run shows "Needs operator" and says the computer stopped answering | [A run says the computer stopped answering](#a-run-says-the-computer-stopped-answering) |
| You need to know what the board or its runner did | [The board's log](#the-boards-log) |

---

## The server says the port is in use

**Cause.** The server binds to `127.0.0.1:3210` unless told otherwise. Another process holds that port,
often a Tack server you started earlier and forgot. The server exits with the system's "Address already in
use" error. The desktop app shows a dialog instead: "Port 3210 is already in use by something other than Tack".

**What to do.** Find what holds the port and stop it, or start Tack on another port.

```sh
# See what is using the default port
lsof -i :3210

# Start on a different port
TACK_PORT=4000 tack serve
```

Open the board at the address you started, for example `http://127.0.0.1:4000`. If you run Tack as a
service, set the port in the service's environment. `TACK_PORT` and `TACK_HOST` are in the
[Configuration Reference](configuration-reference.md).

---

## The server refuses to start on a network address

**What you see.** The server stops before it opens the database and prints:

```text
Error: refusing to bind 0.0.0.0 without TACK_API_TOKEN; bind to loopback,
set TACK_API_TOKEN, or set TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1
to accept the risk
```

**Cause.** `TACK_HOST` names an address other than the local machine, and no `TACK_API_TOKEN` is set.
Tack will not answer anyone who can reach the port without a token.

**What to do.** Set `TACK_API_TOKEN` to a long random string, or bind to `127.0.0.1` and put a reverse
proxy in front. Set `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` only on a network you already trust.
The reasoning and the proxy setup are in [Administration & Security](administration.md#network-exposure-and-tls).

---

## The database is locked

**Cause.** SQLite lets one process write at a time. `database is locked` means another process is
holding a write on `tack.db`: usually a second Tack server pointed at the same file, a script that writes
to it, or a database tool with a transaction left open. Tack keeps the database in write-ahead mode, so
a tool that only reads does not block it.

**What to do.** Run exactly one Tack server per database file. Close any other program that has the file
open for writing. If you really need a second server, give it its own database with `TACK_DATABASE_URL`.
Do not edit the file by hand while the server runs; take a [backup](backup-restore.md) instead.

---

## Startup fails with a migration error

**Cause.** Tack brings the database's schema up to date every time it starts, and records each step in
a `_migrations` table. Startup stops before the server answers in two cases:

- **The history does not match this Tack.** The error says "migration history contains unexpected entry",
  "migration history is out of order" or "migration … checksum changed". The usual reason is that the
  database was written by a newer Tack than the one you started, or that the file is not a Tack database
  from this lineage.
- **A step failed.** Each step runs in one transaction, so a failing step leaves the earlier schema as it was.
  The log line before the error names the migration and the statement.

**What to do.** Do not edit `_migrations` and do not force the start.

1. Check that you are running the Tack version you meant to. After a downgrade, run the newer version again.
2. If a step failed, fix the cause the log names (a full disk is the common one) and start again. A step
   that cannot simply be retried is protected: before the first of them, Tack writes a whole-database
   copy beside your database, named `<your-db-file>.before-<migration-name>.sqlite`. It is never overwritten.
3. If the history is damaged, restore a backup taken before the upgrade. See
   [Backup & Restore](backup-restore.md#staged-restore).

`GET /api/health` reports `migrations_applied` once the server is up. The full upgrade path, with what runs
by itself, is in the repository's `docs/MIGRATION-GUIDE.md`.

---

## A staged restore did not apply

**What you see.** You ran `tack restore`, restarted the server, and the old data is still there. The log
says "Staged restore failed and was rolled back; the original database is intact".

**Cause.** A restore is staged beside the live database as `<db>.restore` and swapped in on the next start.
If a step of the swap fails, for example because the folder is not writable, Tack puts the original files
back and starts with the database you had.

**What to do.** Read the line that follows in the log for the reason, fix it, and stage the restore again.
When a restore does apply, the database you replaced is kept as `<db>.bak-<timestamp>`. To undo a restore
that applied, stop the server and move that copy back. The steps are in
[Backup & Restore](backup-restore.md#staged-restore).

Staging a restore is refused with `409` when the file's schema is newer than the running Tack. Upgrade
Tack first. A file over 50 MB is refused as too large; see
[An upload or import is rejected as too large](#an-upload-or-import-is-rejected-as-too-large).

---

## The board does not load

**Cause.** The web page and the API come from the same server. A blank page, a loading skeleton that
never ends, or network errors almost always mean the server is not running at the address the browser
opened.

**What to do.** Ask the server whether it is up. This route needs no token.

```sh
curl http://127.0.0.1:3210/api/health
```

A running server answers `200` with `{"status":"ok", …}`, its version and `migrations_applied`. If the
request is refused or hangs, start the server and read its console. For a background service,
`tack service status` says whether it is active and where to check its health. Query the address you
actually bound: if you changed `TACK_HOST` or `TACK_PORT`, use those. If the health check works but the
page does not, see [Every request answers 401](#every-request-answers-401) and
[The browser reports a CORS error, 421 or 403](#the-browser-reports-a-cors-error-421-or-403).

---

## Every request answers 401

**Cause.** Every `/api/*` route except `/api/health` and `/api/openapi.json` needs the API token. On a
local address with no `TACK_API_TOKEN`, the server makes a token for itself and keeps it in a file called
`api-token` beside the database. A `401` means the request carried no token, or a different one.

**What to do.** Send the token as a bearer header.

```sh
curl -H "Authorization: Bearer <token>" http://127.0.0.1:3210/api/projects
```

- The `tack` command finds the token itself on the same machine. For a server whose database is
  elsewhere, set `TACK_API_TOKEN` on both ends, or pass `--token`.
- The browser holds the token as a cookie that the page sets when it loads from a host name the server
  answers to. If the board shows `401` after you changed how you reach it, reload it from the new address.
- To choose the token yourself, set `TACK_API_TOKEN` and restart. Rotating it is the same.

Where the token comes from and how it is found is in
[Administration & Security](administration.md#authentication).

---

## The browser reports a CORS error, 421 or 403

**Cause.** Tack answers a browser only when it recognises the address the page was loaded from.

- **CORS error in the console.** The page's origin is not in `TACK_ALLOWED_ORIGINS`. The default list is
  `localhost` and `127.0.0.1` on ports 8080 and 3210, and `https://tack.test`.
- **`421`, "this server does not answer to that host name".** The request's `Host` is not the local machine,
  the address the server is bound to, or the host of an allowed origin. This usually happens behind a
  reverse proxy whose public name is not listed.
- **`403`, "a page on another site may not change anything here".** A request that changes something
  came from a page on another site, or from an `Origin` the server does not recognise.

**What to do.** Add the exact origin the browser uses, scheme, host and port, to `TACK_ALLOWED_ORIGINS`
(comma-separated, no wildcards) and restart. Adding a proxy's public origin also makes its host name
acceptable.

```sh
TACK_ALLOWED_ORIGINS="https://tack.example.com,http://127.0.0.1:3210" tack serve
```

`http://` and `https://`, and a different port, are different origins. The rules are in
[Administration & Security](administration.md#cors) and
[What the Agent Can Reach](what-the-agent-can-reach.md#who-can-talk-to-the-board).

---

## An upload or import is rejected as too large

**Cause.** There are two limits. A file attached to a task can be up to 50 MB. Every other request body,
such as a project file or a JSON import, is limited by `TACK_MAX_BODY_SIZE`, which is 2 MB (`2097152`
bytes) by default. A request over its limit is answered `413`.

**What to do.** Keep attachments under 50 MB. For a larger import or project file, raise the other limit
and restart:

```sh
TACK_MAX_BODY_SIZE=10485760 tack serve
```

Raising `TACK_MAX_BODY_SIZE` does not lift the 50 MB limit. The same 50 MB limit applies to staging a
restore through `tack restore` or the API. A reverse proxy in front of Tack may have a smaller limit of
its own; raise it there too. See [Request limits](administration.md#request-limits).

---

## A succeeded run has no pull request, pack or branch

**Cause.** Everything after a succeeded attempt is optional and best-effort, and none of it changes the
attempt's result. The attempt's timeline records why a step did not happen, as `attempt.verify_skipped`,
`attempt.verify_failed`, `attempt.push_skipped` or `attempt.push_failed`. The usual reasons: the runner has
no `[verify]` section enabled, or no `push_branches = true` under `[git]`; the verifier program is not
on the runner's `PATH`; git has no credentials for the remote; the task is not linked to a GitHub issue,
or the board has no `TACK_GITHUB_TOKEN`. A pull request needs both of those last two.

**What to do.** On the runner's machine, run `tack runner doctor`. Its last lines say whether the verifier is
on and found. Set the missing section as described in
[After a succeeded attempt](runners.md#after-a-succeeded-attempt) and the
[Configuration Reference](configuration-reference.md), then run the task again.

---

## A runner shows "needs docket 0.2.0-beta.6"

**What you see.** `tack runner doctor` prints, under `docket`:

```text
  api:         4 (needs docket 0.2.0-beta.6)
```

and the runner posts no task to docket. The full sentence is "needs docket 0.2.0-beta.6 (API 6); this
docket answers API N".

**Cause.** Tack sends each docket run to a team over docket's HTTP API, which it checks before every
task. It accepts only version `5`, which docket 0.2.0-beta.6 provides. An older or newer docket answers
another version, and Tack refuses it rather than guess.

**What to do.** Install docket 0.2.0-beta.6 on the runner's machine, restart the daemon (or the runner,
if it starts the daemon), and run `tack runner doctor` again until the line reads `api:         5`. The
install steps are in [Installing and Operating Runners](runners.md#install-docket-and-tell-the-runner).
If you run the daemon yourself, check that it is the new binary that is running, not the old one.

---

## docket is absent and the daemon does not start

**What you see.** `tack runner doctor` shows docket as absent with the reason "docket is not configured:
add a [docket] table with its binary or base_url". A docket run waits with "the runner does not offer
docket". When the `[docket]` table has a `base_url` but nothing answers there, the `daemon:` line reads
"not answering at … ; start it, or remove base_url so the runner can". When it has neither, the line reads
"not answering at … and no binary is set to start one".

**Cause.** docket is turned on for a runner only when its `[docket]` table sets `binary` or `base_url`.
A `docket` program on the `PATH` does not count, and a table with only `home`, `port` or `token_file` does
not turn it on either. The runner starts the daemon only when `binary` is set, `base_url` is not, and
nothing already answers on the port. The embedded runner of `tack serve --with-runner` reads no `[docket]`
table at all unless it has a settings file: `~/.config/tack/runner.toml`, or the one `--runner-config`
names. The Agents page says which file it read.

**What to do.** Add `binary` to the runner's `[docket]` table and restart the runner:

```toml
[docket]
binary = "docket"
```

Remove `base_url` if the runner should start the daemon. For an embedded runner, pass its settings with
`tack serve --runner-config <path>`. Run `tack runner doctor` afterwards: the docket block should show
`api: 5` and `daemon: running on 127.0.0.1:7399`. If the daemon was started but exits at once, read
`<state dir>/logs/docket-daemon.log`; the runner restarts it after 2 seconds, then 4, 8 and so on up to
a minute. All the keys are in [docket teams](runners.md#docket-teams).

---

## A team is attended

**What you see.** `tack runner doctor` prints a team as attended:

```text
  team saas-research: attended (fix: docket pod set network none) · reads nothing outside its work
```

In the project's **Settings → Automation → Ceiling**, the team is listed with "keeps its tasks at L3"
and the same fix. The project merges nothing by itself even though its level is higher.

**Cause.** A team that cannot finish a task without a person present is attended. Tack will not merge
the work of an attended team by itself, so its tasks stay at autonomy level L3. The Merge button is not
held back.

**What to do.** Run the fix the doctor prints, with docket's own commands, on the runner's machine, then
run `tack runner doctor` again. The ceiling lifts when the team reports `lights-out`. The button in
Settings that would apply a repository's teams is shown disabled, because the board has no way to reach
the runner's machine. See
[Making a team run without a person](runners.md#making-a-team-run-without-a-person).

---

## A queued run says why it waits

**What you see.** A run stays **Queued** and the task shows "Waiting: …" with a reason, for example
"the runner has not sent a heartbeat recently", "the runner does not offer docket", "docket needs a model
named", "the runner lacks the label …", "the runner is not active" or "the runner has no free capacity (it
runs 1 at a time)". With no runner online at all, the reason reads "no runner is online". After thirty
minutes the run can fail with that reason.

**Cause.** Each time a runner asks for work, Tack records why it could not take a queued run. The reason
is the one that applies to the runner. The thirty-minute clock starts when a runner first looks at the run
and cannot take it, and the run is failed when it has waited longer than that for a reason that will not
go away by itself. These do not fail the run, because they end by themselves:

- every runner is busy, which clears when one finishes;
- the project has used its tokens or dollars for the day, or its review queue is full;
- no runner has looked at the run yet, for instance because none is online.

**What to do.** Read the reason and fix that, from the runner's machine or the board.

| Reason | Fix |
|---|---|
| No runner is online, or a runner has not sent a heartbeat | Start the runner, or check that it reaches the board. See [Installing and Operating Runners](runners.md). |
| The runner is not active | A runner that is revoked, or enrolled but not yet redeemed, takes nothing. Redeem its token, or enroll a new runner. |
| The runner does not offer an agent | Install the agent or turn it on in the runner's settings, then run `tack runner doctor`. For docket, see [docket is absent](#docket-is-absent-and-the-daemon-does-not-start). |
| An agent needs a model named | Name a model in the profile or in the run. |
| The runner lacks a label | Add the label to the runner, or change what the run asks for. |
| A budget is used up | The run waits until tomorrow, or raise the project's budget. |

A run that failed after the wait can be started again once the cause is gone. To retire an attempt that
is stuck on a machine that died, use the [Recovery Runbook](recovery-runbook.md).

---

## Auto-start does not tick

**What you see.** A project has **Start a task by itself** switched on, and a task that meets every
condition in the User Guide's [list](../user-guide/troubleshooting.md#a-task-did-not-start-by-itself)
never starts. The board shows no error.

**Cause.** The board looks for such tasks on its own interval, `TACK_AUTO_START_SECONDS`, which is 30
seconds by default. The value `0` turns the look off for every project, and the Automation settings do not
say so. A value that is not a whole number counts as 30. The first look comes one interval after the board
starts, and a changed value takes effect only after a restart. The look does not need a GitHub token.

**What to do.** Check the variable in the environment the board runs in, including a service's unit file,
and set it to a positive number of seconds:

```sh
TACK_AUTO_START_SECONDS=30 tack serve
```

If the look runs and still starts nothing, two administrator causes remain. With no active runner, or with
several and no fleet named by the profile, Tack cannot tell where to run the task, so it starts none: enroll
a runner, or name a fleet. And the automatic merge that follows a started run needs `TACK_GITHUB_TOKEN`,
a non-zero `TACK_GITHUB_POLL_SECONDS` and a `prill` the board can run. See
[Starting tasks by itself](runners.md#starting-tasks-by-itself).

---

## A run says the computer stopped answering

**What you see.** A run that was **Preparing** or **Running** shows **Needs operator** and the sentence
"The computer that took this run stopped answering."

**Cause.** A runner that takes a run holds it for a minute at a time and keeps renewing that hold.
When it stops (the computer slept or shut down, the runner was stopped or crashed, or it lost the
network), the board waits two minutes more and then asks a person. It never starts the agent again by
itself, because it cannot tell whether the agent is still working on that computer.

A run that the agent refused before it started is different: it ends **Failed** with the agent's own
words, for example "docket did not accept the team saas-dev", and needs no one.

**What to do.** Decide what happens to the run:

- **To end it,** press **Cancel** on the run, or run `tack execution cancel <run>`, which answers
  `Cancelled.`. It ends at once; the computer does not need to be back.
- **To run it again,** first check on that computer that the agent is not still working, then follow
  [Resolving a `needs_operator` attempt](recovery-runbook.md#resolving-a-needs_operator-attempt).

To stop it happening again, find out why the runner went quiet: start it, check that it reaches the
board, and run `tack runner doctor` on its computer.

---

## The board's log

**What you see.** You need to know what the board and its runner did: which database the board opened,
whether a runner took a run, why an agent refused it.

**Where it is.** The board writes its log to standard output, where a service manager collects it; set `TACK_LOG_FILE` to write it to a file as well. By default it holds the lines of every
part of Tack at `info`: the server, the runner the board starts, the scheduler and the command line.
That covers enrollment, claims, refusals and the end of every run.

The first lines say which version started, on which address, and the full path of the database. A
build without the web UI says so in the next line, so a blank page at the board's address is explained
there.

**When the database is not the one you expected.** Started without `TACK_DATABASE_URL` or a database in
its settings, the board uses `tack.db` in the folder it was started in. When the per-user data folder,
which the desktop app uses, holds another `tack.db`, the board says so as it starts and names both files.
Start it from that folder, or set `TACK_DATABASE_URL`, to use the other one.

**For more detail.** Raise the level for every part with `RUST_LOG=tack=debug`, or for one part only,
as in [Configuration](configuration.md). `RUST_LOG` replaces the default, so name every part you want
to keep. A runner's credential is never written to the log.
