# Administrator Guide

This guide is for the person who installs Tack, decides who can reach it, attaches the machines
that run agents, and brings it back when something breaks. After reading it you can stand up a
deployment, lock it down, run agents on it, protect its data and recover from a failed run.

If you use Tack to plan and run work, read the [User Guide](../user-guide/quick-start.md). If you
change Tack's code or integrate with it, read the [Developer Guide](../developer/index.md).

## The order of work for a new deployment

1. **Install and deploy.** Get the binary, run it as a service and put a reverse proxy in front of
   it. See [Deployment](deployment.md).
2. **Configure.** Set the address, the data directories and the logging.
   See [Configuration](configuration.md) and the [Configuration Reference](configuration-reference.md).
3. **Secure.** Decide who can reach the board, which origins may use it and what the agent can
   touch. See [Administration & Security](administration.md) and
   [What the Agent Can Reach](what-the-agent-can-reach.md).
4. **Install runners and agents.** Attach the machine that runs agents, sign each agent in, and
   set up docket if you use it. See [Installing and Operating Runners](runners.md).
5. **Back up.** Take a first backup and test a restore. See [Backup & Restore](backup-restore.md).
6. **Know the recovery path.** Read what happens when a runner or its machine dies mid-run.
   See [Recovery Runbook](recovery-runbook.md).

## I want to…

| I want to… | Go to |
|---|---|
| Run Tack as a service, behind HTTPS | [Deployment](deployment.md) |
| Run Tack in Docker | [Deployment](deployment.md#docker) |
| Change the port, the database path or the log file | [Configuration](configuration.md) |
| Look up a `TACK_*` variable or a runner setting | [Configuration Reference](configuration-reference.md) |
| Require a token, set the allowed origins or expose Tack on a network | [Administration & Security](administration.md) |
| Send events to a webhook | [Administration & Security](administration.md#webhooks) |
| Let agents run on this computer | [Installing and Operating Runners](runners.md#the-three-ways-to-attach-a-runner) |
| Run agents on another machine | [Installing and Operating Runners](runners.md#enrolling-a-runner) |
| Run several attempts at once | [Installing and Operating Runners](runners.md#running-several-attempts-at-once) |
| Use docket teams | [Installing and Operating Runners](runners.md#docket-teams) |
| Let projects start tasks by themselves | [Installing and Operating Runners](runners.md#starting-tasks-by-itself) |
| Know what an agent can read, write and reach | [What the Agent Can Reach](what-the-agent-can-reach.md) |
| Back up, or restore after a loss | [Backup & Restore](backup-restore.md) |
| Fix a run that is stuck in `needs_operator` | [Recovery Runbook](recovery-runbook.md) |

## What runs where

A deployment has up to four kinds of process. Only the first is required.

| Process | What it does | Where it runs | How you start it |
|---|---|---|---|
| The board | Serves the web page, the API and the live updates. Keeps every project in one SQLite file and the attachments in a directory. Also runs the background jobs: starting tasks by itself, the GitHub poll and automatic merge, scheduled backups, retention and the health watch. | One machine. | `tack serve` |
| The runner | Asks the board for work, prepares a workspace for each attempt, runs an agent in it, then collects the evidence and, if you turned them on, verifies it and pushes the branch. Holds the secrets and git credentials you give it. | Each machine that should run agents. | Inside the board with `tack serve --with-runner`, or on its own with `tack runner start` or `tack-runner`. |
| The agents | Claude Code, Codex and opencode, started by the runner as child processes of its machine, each signed in by you. | The runner's machine. | The runner starts one per attempt. |
| docket | A daemon that runs a long-lived team of agents. The runner posts each task to it over HTTP. | The runner's machine. | The runner starts it when its config names a `binary`; or you run it yourself. |

The board never runs an agent, a verifier or `git push`. It tells a runner what to do, and the runner
does it on its own machine. A runner reaches the board over the `/api/runner/v1` routes with its own
credential, separate from the operator token. So a runner can sit on a laptop, a build server or a
spare machine, and the board only has to be reachable from it.

The board also calls one program of its own: [prill](https://github.com/yielab/prill), looked up
with `TACK_PRILL_PROGRAM`, to check a signed pack before an automatic merge. Without it, projects
cannot merge by themselves. See [Configuration Reference](configuration-reference.md).
