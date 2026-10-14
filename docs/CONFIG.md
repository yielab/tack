# Configuration Reference

The complete environment/TOML configuration for the API server and the runner.
Moved from CLAUDE.md (2026-08-19) so agent context stays lean; this file is the
single authority for these tables — update it, not CLAUDE.md, when adding a variable.

The API server loads configuration from `tack.toml` (if present) or environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_HOST` | `127.0.0.1` | Server bind address |
| `TACK_PORT` | `3210` | Server port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite database path |
| `TACK_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `TACK_LOG_JSON` | `false` | Structured JSON logging |
| `TACK_LOG_FILE` | _(none)_ | Write logs to this file **in addition to** stdout, in the same format `TACK_LOG_JSON` selects. Missing parent directories are created; if the directory cannot be created the server logs to stdout only rather than refusing to start. `tack service install` and the desktop app both set this so logs survive without a service manager attached |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(generated)_ | Bearer token — requires `Authorization: Bearer <token>` (or the `tack_session` cookie) on all API requests. Left unset on a loopback bind, the server reads or creates `api-token` beside the database file (see below) |
| `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` | `false` | Explicit opt-out for the startup refusal to bind a non-loopback address with no `TACK_API_TOKEN` set (see `docs/adr/0059-single-operator-identity-posture.md`). Loopback binds are unaffected either way. Off by default — this widens who can reach an unauthenticated API, so it must be a deliberate choice, never a fallback the code takes on its own |
| `TACK_ALLOWED_ORIGINS` | `http://localhost:8080,http://127.0.0.1:8080,http://localhost:3210,http://127.0.0.1:3210,https://tack.test` | Comma-separated allow-list of browser origins. Setting it replaces the default list. It gates CORS, and it gates the board's live WebSocket upgrade for any request whose `Origin` is not a loopback host — a bind to a loopback address (the default) additionally authorizes any loopback-hosted `Origin` on its own, so a local UI on any port (Vite's `5173` dev server included) receives live events with no configuration; a non-loopback bind, or a UI served from a non-loopback origin, still needs its origin listed here or the handshake is refused before it completes |
| `TACK_MAX_BODY_SIZE` | `2097152` | Global request body limit in bytes (default 2 MB; upload endpoint is always 50 MB) |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL — when set, POSTs JSON events on item create/update/delete, sprint status changes, and due-soon alerts |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>` to each delivery |
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope). When set, item status changes are pushed back to linked GitHub issues (item done ⇄ issue closed), and the inbound poll below can start. Never logged. See `docs/GITHUB-SYNC.md` |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root — override for GitHub Enterprise or to point tests at a mock. Used by import, push-back, and the inbound poll |
| `TACK_GITHUB_POLL_SECONDS` | `0` | Inbound poll interval in seconds; `0` is off. Also requires `TACK_GITHUB_TOKEN`. Moves a linked item through its project's ordinary workflow on a GitHub issue close/reopen — see `docs/GITHUB-SYNC.md`. The automatic merge (Automation → Autonomy) runs on the same tick, so it needs the poll on |
| `TACK_PRILL_PROGRAM` | `prill` | The board's own prill, looked up on `PATH`. The board runs it only to check a signed pack before an automatic merge (`prill attest verify`) and to keep prill's store of packs and outcomes (`prill record`, `prill metrics`) under `<storage>/prill/`, one per repository. Without it the board says so: no automatic merge, and the Factory page shows Tack's own figures only |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only. Values below 60 are raised to 60 with a warning — a tighter loop copies the database more often than it can change |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |
| `TACK_LOCAL_RUNNER_ENABLE` | `false` | Startup default for whether the embedded runner runs — the same gate `tack serve --with-runner` sets; either satisfies it. `1` or `true` (case-insensitive) turn it on; anything else, including unset, is off. Read into `AppConfig::local_runner_enable`; a `PUT /api/local-runner` from the UI (ADR 0061 decision 6) overrides it at runtime in `app_meta` — see [Embedded runner](#embedded-runner-tack-serve---with-runner) below. Off by default and refused outright (never silently downgraded) on a non-loopback bind |
| `TACK_EXECUTION_RETENTION_ENABLE` | `false` | Enables the execution-domain retention sweep. **Off by default** (`crates/tack-api/src/config.rs#default_execution_retention_enable`) — this sweep deletes rows **and on-disk blobs**, so data deletion must be an explicit operator opt-in. Covers four things, across two runtime tasks: (a) replay/idempotency bookkeeping and (b) terminal `execution_events` purge (`tack-orch`), plus (c) `execution_artifacts` rows **and their `TACK_STORAGE_DIR/execution-artifacts` blobs** and (d) overdue-decision expiry (`pending` → `expired`). Artifact blobs are typically the largest consumer in this domain. Decision expiry deliberately shares this one gate rather than running always-on — a test pins that posture so changing it is a reviewed diff |
| `TACK_EXECUTION_RETENTION_DAYS` | `90` | Days of history kept before the sweep purges it — applies to all four categories above (replay/idempotency bookkeeping, terminal `execution_events`, `execution_artifacts` rows and blobs, and decision expiry deadlines) |
| `TACK_EXECUTION_RETENTION_INTERVAL_SECS` | `3600` | Interval, in seconds, between execution-retention sweeps |
| `TACK_EXECUTION_HEALTH_ENABLE` | `true` | Enables the execution-domain health watch (runner/queue/lease/event counts; logs a `warn!` on stale-lease/`needs_operator` onset). On by default, unlike retention above — this reads and logs only, deletes nothing |
| `TACK_EXECUTION_HEALTH_INTERVAL_SECS` | `60` | Interval, in seconds, between execution health-watch checks |
| `TACK_EXECUTION_DECISION_TOKEN` | _(none)_ | Separate shared secret required to resolve a scoped execution decision via `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`. Distinct from `TACK_API_TOKEN`, **fail-closed when unset** (the route rejects rather than falling back to the operator token). Never logged |

With no `TACK_API_TOKEN` on a loopback bind, the server reads `api-token` from the directory of its database file, or creates it there (`0600`, one line), and requires it on every API request; the page at `/` hands it to the browser as the `tack_session` cookie. The CLI looks for the file in `<data root>/api-token` after `--token`, `TACK_API_TOKEN` and `~/.tackrc`; a server whose database lives elsewhere needs `TACK_API_TOKEN` set on both ends. An in-memory database keeps its token only in the process. See [What the Agent Can Reach](https://yielab.github.io/tack/user-guide/what-the-agent-can-reach.html).

The `tack` CLI client — every subcommand other than `serve` — talks to a running server
over HTTP and never opens the database. It resolves the server's base URL from
`--base-url`, then `TACK_API_URL`, then `~/.tackrc`, then `http://127.0.0.1:3210`.

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_API_URL` | `http://127.0.0.1:3210` | Base URL of the server the CLI talks to, unless `--base-url` overrides it |
| `TACK_API_TOKEN` | _(none)_ | Bearer token the CLI sends, when the server it talks to requires one. Same variable the server reads to *require* a token — one value, two ends of the same connection |

The `tack-runner` binary is configured separately (defaults → `TOML` → environment → CLI flags,
in that order):

| Variable | Description |
|----------|-------------|
| `TACK_RUNNER_API_URL` | Tack API base URL the runner polls |
| `TACK_RUNNER_ENROLLMENT_TOKEN` | One-time operator-issued token; exchanged for a durable credential and never persisted |
| `TACK_RUNNER_ID` | Runner identity once enrolled |
| `TACK_RUNNER_STATE_DIR` | Owner-only directory for the journal and credential |
| `TACK_RUNNER_SECRET_VALUE` | Value `tack runner secret set` stores; when unset it reads the value from stdin instead. Never a command-line argument, which would be visible in `ps` and shell history. Read once, not persisted by the variable — the store keeps it (OS keychain, or an owner-only file where none answers within `PLATFORM_STORE_TIMEOUT`; `tack runner doctor` reports which backend a given boot picked) |

Runner credentials are redacted in every log, `Debug` impl and error — the redaction is
structural (`RunnerCredential`'s `Debug`/`Display` are hardcoded to `[REDACTED]`), not
convention.

The `TACK_BACKUP_*` values are **defaults**. Cloud-backup settings (endpoint, bucket, region, access/secret key, prefix, retention) can also be edited at runtime from the UI (**Settings → Cloud Backup**) and are stored in the `app_meta` table; UI values override the env defaults. `TACK_BACKUP_INTERVAL_SECS` (automatic scheduling) remains env-only and takes effect at startup. The secret key is write-only over the API — never returned to clients.

## Embedded runner (`tack serve --with-runner`)

`tack serve --with-runner` (or `TACK_LOCAL_RUNNER_ENABLE=1`) runs the runner role as a
task inside the same process as the server, speaking runner-v1 over loopback HTTP
exactly like a remote runner would — see
[`docs/adr/0058-standalone-single-binary-runner.md`](https://github.com/yielab/tack/blob/develop/docs/adr/0058-standalone-single-binary-runner.md)
for why that HTTP hop is kept rather than shortcut. This is the fewest-steps way to see
a real agent attempt run against your own board: no second binary, no `tack runner
enroll` call, no token to copy anywhere.

- **Gate.** Off by default. `TACK_LOCAL_RUNNER_ENABLE` (table above) and `--with-runner`
  are equivalent; either turns it on. Refused outright — before any socket or database
  is opened — when the server is not bound to loopback (`TACK_HOST` other than
  `127.0.0.1`/`localhost`/an equivalent loopback address); this is a startup error, never
  a silent downgrade to a runner-less server, because an embedded runner executes
  arbitrary coding-agent processes on the host serving the UI.
- **UI toggle (ADR 0061 decision 6).** `GET`/`PUT /api/local-runner` let a
  loopback-only UI turn the embedded runner on/off after `tack serve` is already up, with
  no restart — a `PUT` persists the choice to `app_meta` (overriding
  `TACK_LOCAL_RUNNER_ENABLE` from then on) and starts or stops the runner
  task to match. Both routes is absent (a plain
  404, not a gate that refuses) on any non-loopback bind, or when the process embedding
  the server never wired an embedded runner in at all (a bare library caller of
  `tack_api::serve()`).
- **First run.** A fresh state directory with no stored session self-provisions: it
  creates its own pending-runner row and redeems its own one-time enrollment token
  in-process, so no token is ever printed, copied, or configured by hand. A later start
  against the same state directory reuses the credential already on disk instead of
  provisioning a second runner.
- **Configuration file.** `tack serve --runner-config <path>` (or `TACK_LOCAL_RUNNER_CONFIG`)
  names the TOML file the embedded runner loads — the same file `tack runner start --config`
  reads, with the same precedence (file, then environment, then the state directory this
  server scopes for it). It is the only way to give the embedded runner a `[verify]` or `[git]`
  table, since those have no environment variable. Without it the
  embedded runner runs on defaults: no verifier and no branch push.
- **State directory.** Defaults to `<TACK_STORAGE_DIR>/runner` — scoped to the same
  configuration as the database, so a server started against a different
  `TACK_DATABASE_URL` (paired, as every other per-install artifact in this crate already
  assumes, with its own `TACK_STORAGE_DIR`) never resolves to another server's runner
  state. `TACK_RUNNER_STATE_DIR` still overrides this default when set, exactly as it
  does for the standalone `tack-runner` binary (whose own default remains the bare,
  cwd-relative `.tack-runner` — it has no database or `storage_dir` to scope against).
  Holds the runner's credential (`session.json`) and its attempt journal, both written
  owner-only (`session.json` mode `0600`; the directory itself and journal entries
  `0700`/`0600`) — confirmed with `stat -c '%a'` against a real run, not assumed from the
  write path. An install upgrading from before this default existed has its already-
  enrolled state moved there automatically, once, the first time the new directory is
  found missing and the old one is not — never the reverse, and never once the new
  directory already exists.
- **Agent logins — Tack holds no model key.** Each agent signs in and reaches its model
  with its own tools. Tack has no model key, endpoint or catalog; it does not read, store,
  forward or proxy what an agent finds, embedded or standalone. There is no `[provider.*]`
  table: a runner whose config has one refuses to start and names the table. The runner's
  secret store (`tack runner secret set|list|remove`) stays, for `[environment]` entries and
  `secret_reference`. `tack runner doctor` reports, per agent, whether it is signed in and which
  models it offers; run it rather than trusting a copy here.

  | Agent | Sign in with | Where the login lives | What the sandbox binds |
  |---|---|---|---|
  | Claude Code | `claude` ([authentication](https://code.claude.com/docs/en/authentication)) | `~/.claude` | `~/.claude` |
  | Codex | `codex login` ([auth](https://developers.openai.com/codex/auth)) | `~/.codex` | `~/.codex` |
  | opencode | `opencode auth login` ([providers](https://opencode.ai/docs/providers/)) | opencode's own `auth.json` | Its `auth.json` only, in a scratch `HOME`, with the `provider`, `model` and `small_model` keys copied from `opencode debug config`. Its MCP servers, plugins and permissions do not reach the run |
  | docket | `docket setup` ([docket](https://github.com/yielab/docket)) | `~/.docket` | `docket-providers.json`, `docket-models.json` and `secrets.json` of `~/.docket`, read-only; nothing else there. A key kept in the OS keyring is not reachable inside the sandbox: use docket's file backend. Needs docket 0.2.0-beta.5 or later; an older docket does not run |

  The runner forwards no new environment variables into a run. A key that lives only in a
  shell variable does not reach an agent: use the agent's own login or config file.
  The model a run uses is one optional string in the agent's own spelling, chosen per request
  or per project; see [Choosing a
  model](https://yielab.github.io/tack/user-guide/agent-runners.html#choosing-a-model). No
  `TACK_*` variable names a model, provider or endpoint, and the API server never holds a
  model credential. See
  [`docs/adr/0050-runner-control-plane.md`](https://github.com/yielab/tack/blob/develop/docs/adr/0050-runner-control-plane.md)
  and
  [`docs/adr/0058-standalone-single-binary-runner.md`](https://github.com/yielab/tack/blob/develop/docs/adr/0058-standalone-single-binary-runner.md)
  ("Vendor credentials remain outside Tack").
- **Log visibility.** The embedded runner's own log lines (self-provisioning,
  enrollment, claim, completion — anything logged by `tack_runner::*` or by the `tack`
  binary's own `local_runner`/`local_enrollment` modules) do **not** appear under
  default logging. `init_tracing`'s default filter
  (`tack_api={level},tack_db={level},tack_core={level},tower_http=debug`) only ever
  names `tack_api`, `tack_db` and `tack_core` — `TACK_LOG_LEVEL` changes `{level}` for
  those three crates but cannot add a target the filter string never mentions, so this
  is not fixable by raising `TACK_LOG_LEVEL` alone. Set `RUST_LOG` explicitly to include
  the runner's own targets:

  ```bash
  RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info \
    tack serve --with-runner
  ```

  Verified on a fresh state directory: under default logging, `tack_runner::*` and
  `tack::local_enrollment`/`tack::local_runner` produced zero log lines while the
  embedded runner enrolled and ran a real attempt; with the `RUST_LOG` override above,
  the same run showed `tack::local_enrollment: self-provisioned a local runner for the
  embedded runner to redeem ...`, `tack_runner::runtime: runner runtime started ...` and
  `tack_runner::client::transport: runner enrolled ...`. Server-side handler logs (e.g.
  `tack_api::handlers::runner_protocol`'s own `runner enrolled runner_id=...` line) are
  visible either way, since `tack_api` is already in the default filter — only the
  *runner's own* log lines were missing.

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
agent works in; give the public key to the project (Automation → Autonomy), which checks the
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
the branch `<branch_prefix><item short id>-a<attempt number>`, commits what the harness left
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

## Debugging

```bash
# Debug logging
TACK_LOG_LEVEL=debug cargo run -p tack-cli -- serve

# Trace SQL queries
RUST_LOG=tack_db=trace,tack_api=debug cargo run -p tack-cli -- serve

# JSON logs (for log aggregators)
TACK_LOG_JSON=true cargo run -p tack-cli -- serve

# See the embedded runner's own log lines under `--with-runner` (see
# "Embedded runner" above — off by default, silent by default)
RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info \
  cargo run -p tack-cli -- serve --with-runner
```


### Run environment

Where the rules come from and what they protect is on the book page
[What the Agent Can Reach](https://yielab.github.io/tack/user-guide/what-the-agent-can-reach.html); these are the
settings.

| Table | Effect |
|---|---|
| `[secrets] request_may_reference = ["gw"]` | The `store:` secret names a request's `secret_reference` may name. Empty (the default) means a request may reference none, and an `env:` reference is always refused. |
| `[environment] NAME = { value = "x" }` or `{ secret_reference = "env:VAR" }` | Variables every run gets. Resolved once at startup; a request cannot override one, and the `env:` scheme works here because the file is yours. A request may not set `HOME`, `PATH`, `GIT_*`, `SSH_*`, `LD_*`, `DYLD_*`, `PYTHON*`, `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV` or any `*_BASE_URL`. |

### Harness sandbox

There is nothing to configure. When `bwrap` is on the runner's `PATH`, each harness that can
start inside it runs inside it, seeing the workspace and a scratch home read-write, the
directories its harness needs from your home (`~/.claude`, `~/.codex`) and the system
directories read-only. The network is not restricted. A harness that cannot start inside
`bwrap` (a program in a virtualenv or under Homebrew, whose loader or runtime lives where the
sandbox does not reach) runs unsandboxed. `tack runner doctor` prints `sandbox: bwrap` or
`sandbox: none (<why>)` for each harness, and the attempt's start report carries the same word.
