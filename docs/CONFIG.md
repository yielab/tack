# Configuration

This page is for the person who sets up a Tack server or a runner. After reading it you know where
settings come from, which one wins, how to write a `tack.toml`, and the exact name and default of every
setting.

Everything is optional. Tack runs with its built-in defaults, and you change only what you need.

## Where settings come from

The server reads three sources. A later one overrides an earlier one:

1. Built-in defaults.
2. `tack.toml` in the directory the server starts in.
3. Environment variables (`TACK_*`).

A runner has its own settings, with its own file and variables; see [The runner](#the-runner).

An example `tack.toml`:

```toml
host         = "127.0.0.1"
port         = 3210
database_url = "sqlite:/var/data/tack.db?mode=rwc"
log_level    = "info"
log_json     = false
log_file     = "/var/log/tack/api.log"
storage_dir  = "/var/data/tack-storage"
# api_token  = "change-me"
allowed_origins = ["https://pm.example.com"]
max_body_size_bytes = 4194304   # 4 MB
```

Two things to watch. `allowed_origins` is a list in the file but a comma-separated string in the
environment variable. And a `tack.toml` that cannot be parsed, for example because a value has the wrong
type, is ignored as a whole without an error, and the defaults apply. A key Tack does not know is ignored
too. After editing, confirm the change took effect, for example with `GET /api/debug/info`, which reports
the host, port and log level.

The variable form works for every setting in the tables below. The webhook, GitHub, auto-start and backup
settings can also go in `tack.toml`, under the variable's name in lower case without `TACK_`, such as
`github_poll_seconds`.

## The API server

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_HOST` | `127.0.0.1` | Server bind address |
| `TACK_PORT` | `3210` | Server port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite database path. The default is relative to the folder `tack serve` is started in; the start-up line names the absolute path, and `tack serve` warns when nothing names a database and the data folder holds a different `tack.db` |
| `TACK_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `TACK_LOG_JSON` | `false` | Structured JSON logging |
| `TACK_LOG_FILE` | _(none)_ | Write logs to this file **in addition to** stdout, in the same format `TACK_LOG_JSON` selects. Missing parent directories are created; if the directory cannot be created the server logs to stdout only rather than refusing to start. `tack service install` and the desktop app both set this so logs survive without a service manager attached |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(generated)_ | Bearer token — requires `Authorization: Bearer <token>` (or the `tack_session` cookie) on all API requests. Left unset on a loopback bind, the server reads or creates `api-token` beside the database file (see below the table) |
| `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` | `false` | Explicit opt-out for the startup refusal to bind a non-loopback address with no `TACK_API_TOKEN` set. Loopback binds are unaffected either way. Off by default: this widens who can reach an unauthenticated API, so it must be a deliberate choice |
| `TACK_ALLOWED_ORIGINS` | `http://localhost:8080,http://127.0.0.1:8080,http://localhost:3210,http://127.0.0.1:3210` | Comma-separated allow-list of browser origins. Setting it replaces the default list. It gates CORS, and it gates the board's live WebSocket upgrade for any request whose `Origin` is not a loopback host — a bind to a loopback address (the default) additionally authorizes any loopback-hosted `Origin` on its own, so a local UI on any port (Vite's `5173` dev server included) receives live events with no configuration; a non-loopback bind, or a UI served from a non-loopback origin, still needs its origin listed here or the handshake is refused before it completes |
| `TACK_MAX_BODY_SIZE` | `2097152` | Global request body limit in bytes (default 2 MB; upload endpoint is always 50 MB) |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL — when set, every notification that opens (a question, a failed run, a change set waiting, a finished run, an item due soon) is POSTed as signed JSON and retried up to 8 times |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>` to each delivery; `X-Tack-Event` names the event |
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope). When set, item status changes are pushed back to linked GitHub issues (item done ⇄ issue closed), and the inbound poll below can start. Never logged. See [GitHub sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md) |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root — override for GitHub Enterprise or to point tests at a mock. Used by import, push-back, and the inbound poll |
| `TACK_GITHUB_POLL_SECONDS` | `0` | Inbound poll interval in seconds; `0` is off. Also requires `TACK_GITHUB_TOKEN`. Moves a linked item through its project's ordinary workflow on a GitHub issue close/reopen — see [GitHub sync](https://github.com/yielab/tack/blob/develop/docs/GITHUB-SYNC.md). A project's automatic merge (Settings → Automation → Merge) runs on the same tick, so it needs both the token and the poll on |
| `TACK_AUTO_START_SECONDS` | `30` | How often the board looks for tasks a project starts by itself (Settings → Automation → Start); `0` is off for every project, and a value that is not a whole number counts as `30`. It has its own interval and needs no GitHub token. A project starts a task that has a role or a profile when it reaches the project's start status, up to the project's cap of open runs. It runs in the board's process, not in a runner |
| `TACK_PRILL_PROGRAM` | `prill` | The board's own prill, looked up on `PATH`. Left unset, the board uses the program the embedded runner verifies with (`[verify] program`, when its `kind` is `prill`) before looking for `prill` on `PATH`; set, this wins. The board runs it only to check a signed pack before an automatic merge (`prill attest verify`) and to keep prill's store of packs and outcomes (`prill record`, `prill metrics`) under `<storage>/prill/`, one per repository. Without it the board says so: no automatic merge, and the Factory page shows Tack's own figures only |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only. Values below 60 are raised to 60 with a warning — a tighter loop copies the database more often than it can change |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |
| `TACK_LOCAL_RUNNER_ENABLE` | `false` | Startup default for whether the embedded runner runs — the same gate `tack serve --with-runner` sets; either satisfies it. `1` or `true` (case-insensitive) turn it on; anything else, including unset, is off. The **Turn on** and **Turn off** button on the Agents page overrides it while the board runs and remembers the choice — see [Embedded runner](#embedded-runner-tack-serve---with-runner) below. Off by default and refused outright (never silently downgraded) on a non-loopback bind |
| `TACK_LOCAL_RUNNER_CONFIG` | _(none: `~/.config/tack/runner.toml` when it exists)_ | The TOML file the embedded runner loads; `tack serve --runner-config <path>` sets it. Set but empty, the runner reads no file at all. See [Embedded runner](#embedded-runner-tack-serve---with-runner) |
| `TACK_EXECUTION_RETENTION_ENABLE` | `false` | Enables the execution-domain retention sweep. **Off by default**: this sweep deletes rows **and on-disk blobs**, so data deletion must be an explicit operator opt-in. Covers four things: (a) replay and idempotency bookkeeping, (b) the purge of finished `execution_events`, (c) `execution_artifacts` rows **and their `TACK_STORAGE_DIR/execution-artifacts` blobs** and (d) expiry of overdue decisions (`pending` → `expired`). Artifact blobs are typically the largest consumer in this domain. Decision expiry shares this one gate rather than running always-on. A question whose run has ended is closed when the run ends, whatever this says |
| `TACK_EXECUTION_RETENTION_DAYS` | `90` | Days of history kept before the sweep purges it — applies to all four categories above (replay/idempotency bookkeeping, terminal `execution_events`, `execution_artifacts` rows and blobs, and decision expiry deadlines) |
| `TACK_EXECUTION_RETENTION_INTERVAL_SECS` | `3600` | Interval, in seconds, between execution-retention sweeps |
| `TACK_EXECUTION_HEALTH_ENABLE` | `true` | Enables the execution-domain health watch (counts of runners, queued runs, claims and events; logs a warning when a claim goes stale or an attempt reaches `needs_operator`). On by default, unlike retention above: it reads and logs only and deletes nothing |
| `TACK_EXECUTION_HEALTH_INTERVAL_SECS` | `60` | Interval, in seconds, between execution health-watch checks |
| `TACK_EXECUTION_DECISION_TOKEN` | _(none)_ | Separate shared secret required to resolve a scoped execution decision via `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`. Distinct from `TACK_API_TOKEN`, and the route rejects rather than falling back to the operator token. Left unset on a loopback bind, the server reads `decision-token` beside the database file (next to `api-token`), or creates it there on the first start (mode `0600`, 32 random bytes); the variable still wins. The desktop app reads the file and gives it to its window; in a browser the page asks for it once per session. Never logged |

With no `TACK_API_TOKEN` on a loopback bind, the server reads `api-token` from the directory of its database file, or creates it there (`0600`, one line), and requires it on every API request; the page at `/` hands it to the browser as the `tack_session` cookie. The CLI looks for the file in `<data root>/api-token` after `--token`, `TACK_API_TOKEN` and `~/.tackrc`; a server whose database lives elsewhere needs `TACK_API_TOKEN` set on both ends. An in-memory database keeps its token only in the process. See [Administration & Security](https://yielab.github.io/tack/admin-guide/administration.html#authentication).

The `TACK_BACKUP_*` values are **defaults**. The cloud-backup settings except the interval can also be edited from **Settings → Cloud Backup** and are stored in the board's database; those values win. See [Backup & Restore](https://yielab.github.io/tack/admin-guide/backup-restore.html#remotecloud-backup).

## The `tack` command-line client

Every `tack` subcommand other than `serve`, `service`, `runner start` and `runner doctor` talks to a
running server over HTTP and never opens the database. It resolves the server's base URL from
`--api-url`, then `TACK_API_URL`, then `~/.tackrc`, then `http://127.0.0.1:3210`.

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_API_URL` | `http://127.0.0.1:3210` | Base URL of the server the CLI talks to, unless `--api-url` overrides it |
| `TACK_API_TOKEN` | _(none)_ | Bearer token the CLI sends, when the server it talks to requires one. Same variable the server reads to *require* a token — one value, two ends of the same connection |

## The runner

A runner (`tack runner start`, `tack-runner`, or the one inside `tack serve --with-runner`) takes its
settings from, in increasing priority: built-in defaults, a TOML file (`--config`, or
`--runner-config` for the embedded runner), environment variables, and command-line flags. A key the
runner does not know makes it refuse to start.

| Variable | TOML key | Flag | Default | Description |
|----------|----------|------|---------|-------------|
| `TACK_RUNNER_API_URL` | `api_base_url` | `--api-url` | `http://127.0.0.1:3210/api/runner/v1` | Tack API base URL the runner polls |
| `TACK_RUNNER_ENROLLMENT_TOKEN` | `enrollment_credential` | `--enrollment-token` | _(none)_ | One-time operator-issued token; exchanged for a durable credential and never persisted. Required at every start, even when a stored credential exists, which is tried first. Prefer the variable: the flag shows in the process list |
| `TACK_RUNNER_ID` | `runner_id` | `--runner-id` | `local-runner` | The name the runner sends to the board |
| `TACK_RUNNER_STATE_DIR` | `state_dir` | `--state-dir` | `.tack-runner` (embedded: `<TACK_STORAGE_DIR>/runner`) | Owner-only directory for the journal, credential, workspaces, staging, locks, logs and the file secret store |
| `TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS` | `max_concurrent_attempts` | `--max-concurrent-attempts` (the `tack-runner` executable only) | `1` | How many attempts the runner works at once. Below `1`, or a value that is not a whole number, is refused at start. A folder worked in place runs one attempt at a time whatever this says; parallel attempts need a branch or a clone |
| `TACK_RUNNER_SECRET_VALUE` | — | — | _(none)_ | Value `tack runner secret set` stores; when unset it reads the value from stdin instead. Never a command-line argument, which would be visible in `ps` and shell history. Read once, not persisted by the variable: the store keeps it (OS keychain, or an owner-only file where none answers within three seconds; `tack runner doctor` reports which backend a given boot picked) |

Tables `[verify]`, `[git]`, `[secrets]`, `[environment]`, `[sandbox]`, `[prepare]`, `[docket]` and `[egress]` have no environment variable;
they are described in the sections below. Runner credentials are redacted in every log and error.

## Embedded runner (`tack serve --with-runner`)

`tack serve --with-runner` (or `TACK_LOCAL_RUNNER_ENABLE=1`) runs the runner inside the server's
process, speaking runner-v1 over loopback HTTP like any remote runner. What it is and how it behaves are
in [Installing and Operating Runners](https://yielab.github.io/tack/admin-guide/runners.html#standalone-mode-tack-serve---with-runner).
The settings:

- **Gate.** Off by default. `TACK_LOCAL_RUNNER_ENABLE` and `--with-runner` are equivalent; either turns
  it on. The server refuses to start with it, before any socket or database is opened, when
  `TACK_HOST` is not a loopback address.
- **Turning it on and off while the board runs.** On a loopback bind, the Agents page (and
  `GET`/`PUT /api/local-runner`) turns the embedded runner on or off with no restart. The choice is
  stored in the board's database and overrides `TACK_LOCAL_RUNNER_ENABLE` from then on. On any
  non-loopback bind the two routes do not exist (a plain 404).
- **Configuration file.** `tack serve --runner-config <path>` (or `TACK_LOCAL_RUNNER_CONFIG`) names the
  TOML file the embedded runner loads, the same file `tack runner start --config` reads. Without it the
  runner reads `<per-user config dir>/tack/runner.toml` (`~/.config/tack/runner.toml` on Linux) when
  that file exists. It is the only way to give the embedded runner a `[verify]`, `[git]`, `[secrets]`, `[sandbox]`,
  `[environment]`, `[prepare]`, `[docket]` or `[egress]` table, since those have no environment variable. With no file it runs on
  defaults: no verifier, no branch push and no docket. `GET /api/local-runner` names the file it read
  (`config_file`, `null` with none), and `tack runner doctor` prints it first.
- **State directory.** `<TACK_STORAGE_DIR>/runner`, so two boards never share one runner's state.
  `TACK_RUNNER_STATE_DIR` overrides it, as it does for the standalone `tack-runner` binary, whose own
  default is the folder `.tack-runner`. It holds the runner's credential (`session.json`, mode `0600`)
  and its attempt journal (entries `0600`, the directory `0700`).
- **Agent logins.** Tack holds no model key. Each agent signs in with its own command; the table is in
  [Installing and Operating Runners](https://yielab.github.io/tack/admin-guide/runners.html#local-credential-handling).
  The runner forwards no new environment variables into a run, so a key that lives only in a shell
  variable does not reach an agent. The model a run uses is one optional string in the agent's own
  spelling, chosen per request or per project; see [Choosing a
  model](https://yielab.github.io/tack/user-guide/running-agents.html#choosing-a-model). No `TACK_*`
  variable names a model, provider or endpoint. A runner whose settings file has a `[provider.*]` table
  refuses to start and names the table.
- **Log visibility.** The default filter is `tack={level},tower_http={level}`: a target directive
  matches by prefix, so `tack` covers every Tack crate, the embedded runner's enrollment, claim and
  completion lines among them, and `TACK_LOG_LEVEL` sets their level. The per-request trace is `debug`,
  so it appears only at that level; a failed request is `error`. `RUST_LOG` still wins when set.

## Runner verifier (`[verify]`)

A runner can run a verifier over each attempt that succeeded, after the attempt's changes
are captured and before its workspace is deleted. The verifier reads the captured evidence
and produces a merge-readiness pack (`docs/contracts/mrp-v1/`); the runner uploads that pack
as one more artifact on the attempt. The runner runs it, on your machine: the board never
runs it. Whatever the kind, the program starts with an environment of `PATH` only, in the
attempt's workspace, under the runner's process limits and `timeout_seconds`.

```toml
[verify]
enabled = false          # off by default; there is no environment variable for it
kind = "prill"           # "prill" (the default) or "command"
program = "prill"        # looked up on PATH
args = []                # prill: its own flags, such as ["--image", "<digest-pinned image>"]
timeout_seconds = 1800
sign_key = ""            # optional: prill's private key; the pack is then signed (see below)
```

**`kind = "prill"`** (the default): the program is [prill](https://github.com/yielab/prill),
which reads commits only and speaks its own documents. For each attempt the runner:

1. commits what the agent left in the workspace through a temporary index — the agent's own
   index and `HEAD` are untouched, no ref is written, and the runner's markers
   (`.tack-runner/`, `.tack-attempt`, `.tack-checkout`) never enter the commit;
2. writes prill's `attempt-evidence/v0` document from `evidence.json` and, when the item has
   a brief, a `briefing/v1` document from `brief.json` (a metric criterion becomes prill's
   when it carries a `command` and a `pattern`; one without them, and a `max_changed_files`
   constraint, are named in its `riskHints` instead of being dropped);
3. runs `prill verify --repo <workspace> --evidence … [--brief …] --out <dir> --json`
   followed by `args`;
4. translates the `mrp/v1` pack prill prints into `mrp-v1` (`auto-merge-eligible` → `merge`,
   `human-review` → `review`, `reject` → `reject`; prill's risk level, reasons, flags and
   signals; its suite, mutation and judge sections; its stage tokens as `usage`) and uploads
   that. prill's exit 1 is a `reject` pack, not a failure; exit 2 is a failure carrying
   prill's error code and message.

`args` is where prill's sandbox goes: `["--image", "registry.example/verify@sha256:…"]`
runs the checks in that image (prill never pulls one; an image pinned by anything but a
digest or an id keeps the decision at `human-review` and puts `image_not_pinned` on the pack; a
local image's id is `docker image inspect <tag> --format '{{.Id}}'`), `["--no-sandbox"]` runs them on this
machine. With neither, prill needs a container backend on this machine and the image named
by `sandbox.image` in the repository's `prill.yaml` (prill reads that file from the attempt's base commit, so an edit the agent made to it is not applied), and refuses (`sandbox_unavailable`,
`sandbox_image_unset`) otherwise. `--sign` is `sign_key`'s, and `--db` is not the runner's:
the board keeps prill's store. prill's own `mrp.json` and
`mrp.md`, and the documents it was given, stay in the attempt's scratch directory under
`evidence/prill/` beside the translated pack; they are not uploaded.

**Signing (`sign_key`).** With a path to a key made by `prill attest keygen`, the runner passes
`--sign` and uploads prill's envelope (`mrp.intoto.json`) beside the pack, whose `signature`
then names the envelope's digest. Keep the private key with the runner, outside any folder an
agent works in; give the public key to the project (Settings → Automation → Merge), which checks the
envelope against the exact head commit before Tack merges by itself. If prill cannot sign
(`signing_unavailable`, the `sign` extra is missing), the verification fails with that reason;
it is never retried unsigned. An unsigned pack is still reviewed by a person as before.

**`kind = "command"`**: a program of your own that writes an `mrp-v1` pack itself. The
runner appends `--evidence <dir> --workspace <dir> --output <dir>/mrp.json` to `args` and
reads the pack at `--output`; a non-zero exit, or a pack that does not parse, is a failure.

```toml
[verify]
enabled = true
kind = "command"
program = "/opt/verifier/bin/verify"
args = ["--strict"]      # placed before the flags the runner adds
```

A verifier that fails, times out, is not on `PATH`, or produces no pack never changes the
attempt's outcome: the attempt still completes `succeeded` and records an
`attempt.verify_failed` event carrying the exit code and the reason. With `enabled = false`,
nothing is started. A run request can decline the verifier for one run (the run dialog's
"Verify the result" box); it can never turn on a verifier the runner has off. A request that
asks for one from a runner without it changes nothing, and the attempt records an
`attempt.verify_skipped` event saying why. A program that is not on `PATH` is logged as a
warning when the runner starts, and `tack runner doctor` prints the kind, program and
arguments the runner would use.

## Pushing the attempt's branch (`[git]`)

A runner can push the work of each succeeded attempt to the remote it fetched the repository
from, so the result is a branch you can open, review and merge. The runner pushes it, on your
machine, with your git credentials (your credential helper or SSH agent): Tack stores no git
credential, and the board never pushes.

```toml
[git]
push_branches = false    # off by default; there is no environment variable for it
branch_prefix = "tack/"
author = "Tack Runner <tack-runner@localhost>"
```

When `push_branches = true` and an attempt succeeded with a non-empty change, the runner creates
the branch `<branch_prefix><item short id>-a<attempt number>`, commits what the agent left
uncommitted (with `author` as the commit author; the message names the attempt and the item,
never the item's description) and runs `git push origin <branch>`. Git hooks in the workspace
do not run. The branch, its head commit and whether it was pushed are recorded in the attempt's
evidence and its completion report. A push that fails (no credentials, a rejected branch, an
unreachable remote) never changes the attempt's outcome: the attempt still completes
`succeeded` and records an `attempt.push_failed` event. With `push_branches = false`, nothing
is committed or pushed.

Turn it on only for agents you would trust with your git credentials. The push runs git in the
attempt's workspace, which the agent could write to while it worked: hooks, fsmonitor, a custom
SSH command and the `ext::` transport are disabled, the push goes to the URL recorded when the
workspace was made and sends exactly the commit that was verified, and it is refused if the
repository's git configuration changed during the run.

As with the verifier, a run request can decline the push for one run but never enable it; asking
for a push from a runner with `push_branches = false` changes nothing and the attempt records an
`attempt.push_skipped` event.

## Web access (`[egress]`)

Holds each run of Claude Code, Codex and opencode to the web access its profile chose (`closed`, `listed` or
`open`) through a proxy the runner starts for the run on loopback. Off by default.

```toml
[egress]
enforce = true
model_hosts = ["openrouter.ai", "*.example.com"]
```

| Key | Default | Meaning |
|---|---|---|
| `enforce` | `false` | Whether every run of a local agent goes through the proxy, in its profile's mode |
| `model_hosts` | `[]` | Hosts every mode lets through besides the agent's own model address; for an agent that takes any provider (opencode). `example.com` or `*.example.com` |

The proxy refuses this computer's own address, private networks, link-local addresses and the cloud metadata
addresses in every mode, judged on the address a name resolves to. The agent is told through `HTTP_PROXY`,
`HTTPS_PROXY` and `NO_PROXY`. In the `bwrap` sandbox the agent has no network of its own and the proxy is its only way
out, so a program that ignores them reaches nothing; without `bwrap` they are a request only. docket's traffic is not affected: docket
confines its own. What each mode covers, and what it cannot, is in
[What the Agent Can Reach](https://yielab.github.io/tack/admin-guide/what-the-agent-can-reach.html#web-access).

## Installing dependencies (`[prepare]`)

The runner installs a project's dependencies in the copy an agent works in, before the agent starts. The install
runs inside the sandbox and, where `[egress] enforce` is on, through the same proxy, reaching the package registry only.

```toml
[prepare]
board_may_replace = true
```

| Key | Default | Meaning |
|---|---|---|
| `board_may_replace` | `false` | Whether a project may replace the install command from the board. A replacement is a shell command the board chose, so without this the run ends as failed with the reason. The command the lockfile implies is never affected |

The install keeps `PATH`, `HOME`, the locale, certificate files (`SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`) and the
registry's settings (`NPM_CONFIG_REGISTRY`, `NPM_TOKEN`, `PIP_INDEX_URL`, `PIP_EXTRA_INDEX_URL`,
`UV_INDEX_URL` and the like) from the runner's environment, and nothing else. Where the machine has no `bwrap` and
`[sandbox] required = false`, it runs with the runner's own environment. See
[Installing the project's dependencies](https://yielab.github.io/tack/admin-guide/runners.html#installing-the-projects-dependencies).

## docket (`[docket]`)

docket runs as a daemon beside the runner and the runner talks to its operator API (version 6,
docket 0.3.0-beta.1). Naming a `binary` or a `base_url` turns docket on for this runner; neither means no
docket on this runner. A `docket` program on the `PATH` does not count.

```toml
[docket]
home = "/home/me/.docket"      # the daemon's DOCKET_HOME; docket's own default when unset
port = 7399                    # where the daemon listens, on 127.0.0.1
token_file = "/home/me/.docket/token"   # the daemon's bearer token
binary = "docket"              # starts `docket start --dispatch` with the runner
# base_url = "http://127.0.0.1:7399"   # a daemon someone else runs; the runner starts nothing
```

| Key | Default | Meaning |
|---|---|---|
| `home` | docket's own | The daemon's `DOCKET_HOME`, passed to it when the runner starts it |
| `port` | `7399` | The port the daemon listens on, on `127.0.0.1` |
| `token_file` | _(none)_ | The file holding the daemon's bearer token; read again on every call |
| `binary` | _(none)_ | The docket program; lets the runner start the daemon |
| `base_url` | _(none)_ | A daemon already running at this URL; the runner starts nothing |

How the runner starts and watches the daemon, and how teams are checked, is in
[Installing and Operating Runners](https://yielab.github.io/tack/admin-guide/runners.html#docket-teams).

A profile, not the runner's file, names a team: `tool_policy.docket.team`. With none, the team is
named for the project's folder.

A team's steps can also run out of their own budget — rounds, tool calls or time, as against the
tokens limit above, which stays Tack's own. Where that limit is raised depends on who set it: one
on a single step is a key on that step, in the team's own pipeline; one on a role applies to every
step that role runs; a time limit with neither falls back to the team's own turn timeout; and
anything still unset is this installation's own machine default. The run that stopped always names
which of these applies, so there is no table to keep in sync here.

## Debugging

```bash
# Debug logging
TACK_LOG_LEVEL=debug tack serve

# Trace SQL queries
RUST_LOG=tack_db=trace,tack_api=debug tack serve

# JSON logs (for log aggregators)
TACK_LOG_JSON=true tack serve

# The embedded runner's own log lines are in the default output under `--with-runner`
tack serve --with-runner
```

## Run environment

Where the rules come from and what they protect is on the book page
[What the Agent Can Reach](https://yielab.github.io/tack/admin-guide/what-the-agent-can-reach.html); these are the
settings.

| Table | Effect |
|---|---|
| `[secrets] request_may_reference = ["gw"]` | The `store:` secret names a request's `secret_reference` may name. Empty (the default) means a request may reference none, and an `env:` reference is always refused. |
| `[secrets] platform_store = "off"` | `"auto"` (the default) tries the OS credential store first and falls back to the owner-only file; `"off"` never asks it, for a machine with no session bus. |
| `[environment] NAME = { value = "x" }` or `{ secret_reference = "env:VAR" }` | Variables every run gets. Resolved once at startup; a request cannot override one, and the `env:` scheme works here because the file is yours. A request may not set `HOME`, `PATH`, `GIT_*`, `SSH_*`, `LD_*`, `DYLD_*`, `PYTHON*`, `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV` or any `*_BASE_URL`. |

## Agent sandbox

Every agent runs contained: by this computer (bubblewrap), by its own sandbox, or by docket;
Claude Code and opencode only by bubblewrap. Where nothing contains an agent it does not run,
unless the administrator turned the requirement off, and then it asks before each action.

When `bwrap` is on the runner's `PATH`, each agent that can
start inside it runs inside it, seeing the workspace and a scratch home read-write, the
directories it needs from your home (`~/.claude`, `~/.codex`) and the system
directories read-only. The sandbox closes the network only when `[egress]` enforces web access. An agent that cannot start inside
`bwrap` (a program in a virtualenv or under Homebrew, whose loader or runtime lives where the
sandbox does not reach) counts as not contained. docket is not wrapped: it confines its own agents, and the
probe asks it (`isolationPosture` in its `/health`): `sandbox: harness` only when docket says it
jails them, `sandbox: none` when it says not, or says nothing, and then the run asks; docket is never
refused. `tack runner doctor` prints `sandbox: bwrap`, `sandbox: harness (…)` or
`sandbox: none (<why>)` for each agent, and the attempt's start report carries the same word.

| Setting | Effect |
|---|---|
| `[sandbox] required = true` | The default. An agent that finds no containment on this computer does not run: the probe says "install bubblewrap" and nothing schedules it. |
| `[sandbox] required = false` | The opt-out for a computer that cannot have `bwrap`. The agent runs without it, asking before each action, and the run's timeline, the run dialog, the Agents page and `tack runner doctor` say so. An agent that cannot ask does not run even then. |
