# CLI

This page is for anyone who wants to use Tack from a terminal: to script it, to call it from automation, or to work without leaving the shell. After reading it you will know the commands for projects, items, sprints, roles, fields, agent profiles and runs, and what each prints.

`tack` is one binary that is both the server and the command-line client. `tack` with no arguments (or `tack serve`) starts the server and the web UI. Every other command talks to a running server over HTTP, so start the server first. Running a server for other people, and its backups and tokens, belong to the [Administrator Guide](../admin-guide/index.md).

Every command takes `--help`, and every command that prints a result takes `--json` for raw JSON. With `--json` the vocabulary mapping is bypassed and raw field names come back:

```sh
tack list --project <id> --json | jq '.[] | select(.priority == "high")'
```

## Connecting

The client finds the server in this order: `--api-url` and `--token` flags, the `TACK_API_URL` and `TACK_API_TOKEN` environment variables, the file `~/.tackrc`, and then the token file a local server keeps in its data folder. The default URL is `http://127.0.0.1:3210`. A server started with the defaults keeps its token in a file that the client reads, so on the computer that runs it you usually need nothing. If the server keeps its database somewhere else, give the client the token with `--token` or `TACK_API_TOKEN`; the [Administrator Guide](../admin-guide/administration.md) says where to find it.

```sh
# Save the URL and token to ~/.tackrc (readable by you only)
tack config --url http://127.0.0.1:3210 --token your-token

# Show what the client will use
tack config --show
```

Shell completions:

```sh
tack completions bash  >> ~/.bashrc
tack completions zsh   >> ~/.zshrc
tack completions fish  > ~/.config/fish/completions/tack.fish
```

A failed command prints its message on stderr and exits non-zero.

---

## Projects

```sh
tack projects                                 # list projects
tack init "Kitchen Reno" --type construction  # create a project
```

Project types: `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `legal`, `research`, `event`, `custom`. The type picks the starting workflow and vocabulary.

### Start from a template

```sh
tack init "My Site" --template web-product-with-content
```

The template brings its roles, profiles and automation, and `--type` is ignored. Give the template's id, or its name in any case with `-` for spaces. `tack template list` shows what exists; see [Templates](#templates).

### Import and export

```sh
tack import project.yaml --dry-run        # check the file, write nothing
tack import project.yaml                  # create the project the file names
tack import tasks.csv --name "Relaunch" -t web
tack import project.yaml -p <project-id>  # read the file into an existing project

tack export <project> -o snapshot.json              # a snapshot: items, sprints, dependencies, briefs
tack export <project> --format csv -o items.csv     # the items as a spreadsheet
tack export <project> --bundle -o project.yaml      # the whole project file
```

`tack import` reads project files, outlines and CSV files, and picks the format from the extension. `tack export` takes `json`, `yaml` or `csv`; with `--bundle` the default is YAML. Without `-o` the file goes to stdout. The formats are described in [Import, Export & Project Files](import-export.md).

An import prints one line per section the file carries, then the items:

```text
Roles: 1 new, 0 updated, 0 not mentioned
Items: 3 new, 0 updated, 0 not mentioned
Would create 3 items and 0 dependencies
```

The last line reads "Created" instead of "Would create" when you did not pass `--dry-run`. Each warning follows as `warning: …`.

### Autonomy

```sh
tack project autonomy <project-id>
```

Shows how far Tack may merge this project's changes on its own. Change the level in **Settings → Automation**; see [Merging from Tack](merging.md).

---

## Items

```sh
tack list --project <id>                  # list items; filter with --status, --item-type, --assignee
tack board --project <id>                 # the board in the terminal
tack search "login" --project <id>        # search; leave out --project to search everything

tack add "Design login page" --project <id> --type task --priority high
tack add "Send the newsletter" --project <id> --recurrence weekly

tack move <item-id> "In Progress"         # move to another status
```

`tack add` also takes `--parent`, `--assignee` and `--sprint`. Types are `task` (the default), `epic`, `feature`, `bug`, `subtask` and `requirement`; priorities are `critical`, `high`, `medium` (the default) and `low`. `--recurrence weekly|monthly|quarterly` makes the task repeat: closing it creates the next one.

### Branches

```sh
tack branch <item-id>                     # print: git checkout -b feat/<short-id>-<title-slug>
tack branch <item-id> --checkout          # create and switch to the branch
tack branch <item-id> --prefix hotfix     # use your own prefix
tack start <item-id>                      # move to the first in-progress status, then check out the branch
tack open <item-id>                       # print the item's web address, and open it in $BROWSER when set
```

`tack branch` builds a conventional name of the form `<prefix>/<short-id>-<title-slug>`, where the prefix follows the type (`feature` becomes `feat`, `bug` becomes `fix`). Without `--checkout` it prints the `git checkout -b …` command, handy to copy or `eval`. If the workflow refuses the status change, `tack start` reports the reason and creates no branch.

---

## Sprints

```sh
tack sprint create "Sprint 1" --project <id> --goal "Ship the login"
tack sprint list --project <id>
tack sprint start  <sprint-id>    # Planning → Active
tack sprint review <sprint-id>    # Active → Review
tack sprint close  <sprint-id>    # → Closed
```

---

## Templates

```sh
tack template list                          # built-in and your own
tack template list --project-type construction
tack template show <template-id>
tack template create-from <template-id> "My New Project"

# Save a project's roles, profiles, automation, workflow and vocabulary as a template
tack template save <project-id> --name "My template" --description "Optional"
```

`tack template create-from` and `tack init --template` do the same thing; the second also accepts a template's name.

---

## Roles

A role is a specialty or discipline (Designer, Engineer, Reviewer) you assign to items. A role can name the agent profile its tasks run with.

```sh
tack role list --project <project-id>
tack role create "Designer" --project <project-id>
tack role create "Engineer" --project <project-id> --color "#4A90D9" --profile <profile-id>

# Set, or clear with "none", the profile a role runs with
tack role set-profile <role-id> <profile-id>
tack role set-profile <role-id> none

tack role assign   <item-id> <role-id>
tack role unassign <item-id> <role-id>
tack role delete   <role-id>               # also removes its assignments
```

`<profile-id>` comes from `tack agent-profile list`. How a run picks its profile from a role is in [Running Agents on Tasks](running-agents.md).

---

## Comments

```sh
tack comment list <item-id>
tack comment add <item-id> "Looks good to merge"
tack comment add <item-id> "Blocked on client sign-off" --author "Alice"
```

---

## Custom fields

Custom fields give items project-specific values.

```sh
tack field list --project <project-id>

tack field create "Client Name"  --project <project-id> --field-type text
tack field create "Story Points" --project <project-id> --field-type number --required
tack field create "Phase"        --project <project-id> --field-type select \
    --options "Design,Development,QA,Done"

tack field values <item-id>                         # every value set on an item
tack field set <item-id> <field-id> "Acme Corp"
tack field set <item-id> <field-id> 8               # a number
tack field set <item-id> <field-id> true            # a boolean
tack field set <item-id> <field-id> '"Design"'      # a string that looks like JSON: quote it
tack field unset <item-id> <field-id>
tack field delete <field-id>                        # also removes that field's values
```

Types are `text`, `long_text`, `number`, `date`, `boolean`, `select`, `multi_select`, `url` and `email`.

---

## Agent profiles

A profile holds instructions, the tools an agent may use and limits. A run takes a copy when it is created, so editing a profile later never changes the history of an earlier run.

```sh
tack agent-profile create "release-notes" \
  --instructions "Summarize the diff and write docs/CHANGELOG entries." \
  --fleet <fleet-id>
tack agent-profile list
```

`--tool-policy` and `--limits` take a JSON object. `--fleet` is optional: runs with this profile go to that fleet when they name no runner or fleet. To name an agent or a docket team in the profile, set `agent` and `docket.team` in `--tool-policy`, or import a [project file](import-export.md#the-project-file).

---

## Runs

`tack execution` creates, inspects and cancels runs. The web UI's **Run with agent** button does the same with a form; see [Running Agents on Tasks](running-agents.md).

```text
Commands:
  create     Create (or idempotently replay) an execution request
  list       List execution requests (newest first)
  get        Get one execution request's current lifecycle state
  cancel     Request cancellation of an execution
  reconcile  Requeue a needs_operator execution after an audited recovery decision
```

A run needs a time limit and a permission policy, and nothing else. Everything else falls back to what the project and the item's role say:

```sh
tack execution create <item-id> \
  --permission-policy '{"tools":[],"network":false}' \
  --timeout-seconds 600
```

| Flag | Meaning when you leave it out |
|---|---|
| `--agent-profile <id>` | The item's role decides, then the project's default profile, then the built-in Implementer. |
| `--harness <agent>` | The profile's agent, else the project's default agent. Names such as `claude-code`, `codex`, `opencode` and `docket`. |
| `--model <name>` | The project's default for that agent, else the agent's own choice. Give the agent's own spelling of a model. |
| `--runner <id>` or `--fleet <id>` | The profile's fleet, else the only active runner. Use at most one of the two. |
| `--idempotency-key <key>` | A fresh key each call. Pass a stable one to retry the same request safely. |
| `--status-map-policy done_on_success\|done_on_mrp_accepted` | The item's status is never touched. With a policy it moves to the first Done status when the attempt succeeds, or when its merge-readiness pack is accepted. |

`--budgets`, `--environment` and `--metadata` take JSON objects.

```sh
tack execution list
tack execution get <execution-id>
tack execution cancel <execution-id>
```

`cancel` records the request. The runner carries it out and reports what actually happened. For docket it is a call to docket's own cancel route. For the other agents Tack signals the agent's process group, which may not reach a process the agent started apart from it. `tack execution reconcile <id> --recovery-key <key> --reason "…"` requeues a run that needs an operator after an audited decision; see the [Recovery Runbook](../admin-guide/recovery-runbook.md).

---

## Runner

A runner is the program on a computer that carries out runs. Installing, enrolling and operating runners is administrator work and is described in [Installing and Operating Runners](../admin-guide/runners.md). The command a user reaches for is the check:

```sh
tack runner doctor
```

It needs no server and enrolls nothing. It reports which agents this computer has, whether each is signed in, what models it offers, and, for docket, whether its daemon answers and which teams run lights-out. It reads the configuration a runner on this computer would use. Each agent has a block like this one:

```text
docket
  status:      absent
  reason:      docket is not configured: add a [docket] table with its binary or base_url
  credentials: docket holds its own model credentials in its home; Tack holds none and reaches docket with the daemon's bearer token.
```

When docket is configured and answers, its block gains these lines before the sign-in and model lines:

```text
  api:         5
  daemon:      running on 127.0.0.1:7399
  team saas-content: lights-out · reads /home/me/.nvm/versions/node/v22.17.1
  team saas-research: attended (fix: docket pod set network none) · reads nothing outside its work
```

- **`api`** is the version of docket's HTTP interface. Tack needs docket 0.2.0-beta.6, which speaks version 5. Any other version reads `api: 4 (needs docket 0.2.0-beta.6)` and no run is sent to it.
- **`daemon`** says whether docket's background process answers. `not answering at … ` says where Tack looked.
- **`team <name>`** appears once per team. `lights-out` means the team runs without a person. `attended` means it needs one, and the fix in brackets is the docket command that changes that. A team that is `attended` keeps its tasks at L3, however high the project's merge level; see [Merging from Tack](merging.md).

Each agent also prints a `signed in:` line and a `models:` line, read from the agent's own offline commands. A signed-out agent names the command to run, for example `signed in: no (Not logged in)` followed by the command to run. The `models:` line says whether the list came from the agent (`listed`) or from its documentation (`suggested`), and whether a run must name a model. The last lines summarise what the runner can do as a whole and whether the runner's verification step is enabled.

Other runner commands, for operators:

| Command | What it does |
|---|---|
| `tack runner enroll`, `revoke`, `revoke-token` | Create, disable or cancel a runner's credential. |
| `tack runner start` | Run the runner in this process. |
| `tack runner secret set\|list\|remove` | Manage the secrets kept on this computer for an agent's environment. |
| `tack fleet create`, `tack fleet list` | Group runners. Adding a runner to a fleet is done on the Agents page. |

All of them are covered in the [Administrator Guide](../admin-guide/runners.md).

---

## Running Tack in the background

`tack service install` runs Tack as a background service for your own user, so it keeps running after you close the terminal: a systemd user unit on Linux, a launchd agent on macOS. It is not available on Windows, where the desktop app does the same.

```sh
tack service install     # write the unit, start it, and start it at login
tack service status      # active or not, and where the health address is
tack service uninstall   # stop it and remove the unit; your data stays
```

The service keeps its database, files and log in your per-user application-data folder. A shared server for other people is set up differently; see [Deployment](../admin-guide/deployment.md).

---

## MCP server for AI assistants

`tack mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio, so an assistant such as Claude Code can read and search the board and create, update and move items or add comments. Its writes go through the same API, so the workflow rules still apply.

```sh
# Wire this into an MCP client; it reads JSON-RPC on stdin and answers on stdout.
# It uses the same connection settings as every other command.
tack mcp
```

See [the MCP guide](https://github.com/yielab/tack/blob/develop/docs/MCP.md) for the client configuration and the tool list.

---

## Backups

`tack backup`, `tack backups` and `tack restore` are covered in [Backup & Restore](../admin-guide/backup-restore.md).
