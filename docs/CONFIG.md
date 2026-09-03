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
| `TACK_LOG_FILE` | _(none)_ | Optional log file path |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(none)_ | Optional Bearer token — requires `Authorization: Bearer <token>` on all API requests |
| `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` | `false` | Explicit opt-out for the startup refusal to bind a non-loopback address with no `TACK_API_TOKEN` set (see `docs/adr/0059-single-operator-identity-posture.md`). Loopback binds are unaffected either way. Off by default — this widens who can reach an unauthenticated API, so it must be a deliberate choice, never a fallback the code takes on its own |
| `TACK_ALLOWED_ORIGINS` | `localhost:8080,127.0.0.1:8080` | Comma-separated CORS allow-list |
| `TACK_MAX_BODY_SIZE` | `2097152` | Global request body limit in bytes (default 2 MB; upload endpoint is always 50 MB) |
| `TACK_ALEXA_SKILL_ID` | _(none)_ | Amazon Alexa skill ID — enables `POST /api/alexa` (see `docs/ALEXA.md`); endpoint returns 404 when unset |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL — when set, POSTs JSON events on item create/update/delete, sprint status changes, and due-soon alerts |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>` to each delivery |
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope). When set, item status changes are pushed back to linked GitHub issues (Phase 21, push-only: item done ⇄ issue closed). Never logged. See `docs/GITHUB-SYNC.md` |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root — override for GitHub Enterprise or to point tests at a mock. Used by both import and push-back |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |
| `TACK_LOCAL_RUNNER_ENABLE` | `false` | Enables the embedded runner started by `tack serve --with-runner` — the same gate the `--with-runner` flag sets; either satisfies it (`crates/tack-cli/src/local_runner.rs::with_runner_enabled`). `1` or `true` (case-insensitive) turn it on; anything else, including unset, is off. Read by the `tack` CLI binary, not `AppConfig` — there is no `tack.toml` equivalent (deliberate, see [Embedded runner](#embedded-runner-tack-serve---with-runner) below). Off by default and refused outright (never silently downgraded) on a non-loopback bind |
| `TACK_ORCH_ENABLE` | `false` | Enables the orchestration reconciler and the `/api/control-planes`, `/api/projects/{id}/orch-link`, `/api/fleet` routes (and their later-wave successors). Unset ⇒ no reconciler task spawned, every orch route 404s |
| `TACK_ORCH_POLL_SECS` | `10` | Reconciler base poll interval in seconds (before per-plane backoff + jitter) |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `90` | Days of `orch_events` (and, once ingested, `orch_metrics`) history kept before the retention sweep rolls old rows into per-day aggregates and deletes them |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Separate shared secret required to grant/deny a docket approval via `POST /api/approvals/{token}` (Wave 4). Deliberately distinct from `TACK_API_TOKEN` — granting an approval is higher-privilege than editing a card. Never logged |
| `TACK_EXECUTION_RETENTION_ENABLE` | `false` | Enables the execution-domain retention sweep. **Off by default** (Wave 5 integrator III-F6 amendment — F5 originally shipped this `true`; see `crates/tack-api/src/config.rs#default_execution_retention_enable`) — this sweep deletes rows **and on-disk blobs**, so data deletion must be an explicit operator opt-in, matching `TACK_ORCH_ENABLE`'s own off-by-default posture. Covers four things, across two runtime tasks: (a) replay/idempotency bookkeeping and (b) terminal `execution_events` purge (III-F5, `tack-orch`), plus (c) `execution_artifacts` rows **and their `TACK_STORAGE_DIR/execution-artifacts` blobs** and (d) overdue-decision expiry (`pending` → `expired`) — (c) and (d) wired by III-F6d, which found F2's and F1's sweeps had **zero callers anywhere in the tree** because F5 was authored before F2 existed. Artifact blobs are typically the largest consumer in this domain; before III-F6d they grew without bound even with retention enabled. Decision expiry deliberately shares this one gate rather than running always-on — a test pins that posture so changing it is a reviewed diff |
| `TACK_EXECUTION_RETENTION_DAYS` | `90` | Days of history kept before the sweep purges it — applies to all four categories above (replay/idempotency bookkeeping, terminal `execution_events`, `execution_artifacts` rows and blobs, and decision expiry deadlines) |
| `TACK_EXECUTION_RETENTION_INTERVAL_SECS` | `3600` | Interval, in seconds, between execution-retention sweeps |
| `TACK_EXECUTION_HEALTH_ENABLE` | `true` | Enables the execution-domain health watch (runner/queue/lease/event counts; logs a `warn!` on stale-lease/`needs_operator` onset, Wave 5 card III-F5). On by default, unlike retention above — this reads and logs only, deletes nothing |
| `TACK_EXECUTION_HEALTH_INTERVAL_SECS` | `60` | Interval, in seconds, between execution health-watch checks |
| `TACK_EXECUTION_DECISION_TOKEN` | _(none)_ | Separate shared secret required to resolve a scoped execution decision via `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve` (Wave 5 card III-F1, wired by integrator III-F6). Mirrors `TACK_ORCH_APPROVAL_TOKEN` exactly: distinct from `TACK_API_TOKEN`, **fail-closed when unset** (the route rejects rather than falling back to the operator token). Never logged |

The `tack-runner` binary is configured separately (defaults → `TOML` → environment → CLI flags,
in that order):

| Variable | Description |
|----------|-------------|
| `TACK_RUNNER_API_URL` | Tack API base URL the runner polls |
| `TACK_RUNNER_ENROLLMENT_TOKEN` | One-time operator-issued token; exchanged for a durable credential and never persisted |
| `TACK_RUNNER_ID` | Runner identity once enrolled |
| `TACK_RUNNER_STATE_DIR` | Owner-only directory for the journal and credential |

Runner credentials are redacted in every log, `Debug` impl and error — the redaction is
structural (`RunnerCredential`'s `Debug`/`Display` are hardcoded to `[REDACTED]`), not
convention.

The `TACK_BACKUP_*` values are **defaults**. Cloud-backup settings (endpoint, bucket, region, access/secret key, prefix, retention) can also be edited at runtime from the UI (**Settings → Cloud Backup**) and are stored in the `app_meta` table; UI values override the env defaults. `TACK_BACKUP_INTERVAL_SECS` (automatic scheduling) remains env-only and takes effect at startup. The secret key is write-only over the API — never returned to clients.

## Embedded runner (`tack serve --with-runner`)

`tack serve --with-runner` (or `TACK_LOCAL_RUNNER_ENABLE=1`) runs the runner role as a
task inside the same process as the server, speaking runner-v1 over loopback HTTP
exactly like a remote runner would — see
[`docs/adr/0058-standalone-single-binary-runner.md`](adr/0058-standalone-single-binary-runner.md)
for why that HTTP hop is kept rather than shortcut. This is the fewest-steps way to see
a real agent attempt run against your own board: no second binary, no `tack runner
enroll` call, no token to copy anywhere.

- **Gate.** Off by default. `TACK_LOCAL_RUNNER_ENABLE` (table above) and `--with-runner`
  are equivalent; either turns it on. Refused outright — before any socket or database
  is opened — when the server is not bound to loopback (`TACK_HOST` other than
  `127.0.0.1`/`localhost`/an equivalent loopback address); this is a startup error, never
  a silent downgrade to a runner-less server, because an embedded runner executes
  arbitrary coding-agent processes on the host serving the UI.
- **First run.** A fresh state directory with no stored session self-provisions: it
  creates its own pending-runner row and redeems its own one-time enrollment token
  in-process, so no token is ever printed, copied, or configured by hand. A later start
  against the same state directory reuses the credential already on disk instead of
  provisioning a second runner.
- **State directory.** `TACK_RUNNER_STATE_DIR` (default `.tack-runner`, relative to the
  working directory `tack serve` was started from) holds the runner's credential
  (`session.json`) and its attempt journal. Both are written owner-only
  (`session.json` mode `0600`; the directory itself and journal entries `0700`/`0600`) —
  confirmed with `stat -c '%a'` against a real run, not assumed from the write path.
- **Vendor/provider credentials — Tack is never a model gateway.** Each harness
  authenticates itself using its own mechanism; Tack does not read, store, forward, or
  proxy any of it, embedded or standalone. `tack runner doctor` reports exactly what
  this machine's own harnesses declare — run it yourself rather than trusting a stale
  copy in this file. The three current harnesses, mirrored from a real `tack runner
  doctor` run on a machine with all three installed:

  | Harness | How it authenticates |
  |---|---|
  | `codex` | Its own CLI login flow or an API key it reads from its own environment/config (`codex --help`). This adapter forwards **no ambient host environment** into a run — only entries explicitly set on the execution request's own `environment` field ever reach the process. |
  | `claude-code` | Typically an OAuth session under `$HOME/.claude` from its own login flow, or an API key from its own environment. This adapter forwards `HOME` and `PATH` from the runner process's own environment so the installed CLI can find its existing session. |
  | `opencode` | Its own credential store (default `~/.local/share/opencode`), populated by `opencode auth login` or provider-specific configuration. This adapter forwards `PATH`, `HOME` and the `XDG_*` variables. |

  OpenRouter access and local-model endpoints (llama.cpp and similar) are configured the
  same way: through the harness's own configuration or environment, never through a Tack
  setting — there is no `TACK_*` variable for a model provider or endpoint, by design.
  See [`docs/adr/0050-runner-control-plane.md`](adr/0050-runner-control-plane.md) ("the
  Tack API never starts a coding harness and never becomes a model proxy") and
  [`docs/adr/0058-standalone-single-binary-runner.md`](adr/0058-standalone-single-binary-runner.md)
  ("Vendor credentials remain outside Tack") for the decisions behind this.
- **Model selection is a separate question from credentials, and it is answered.**
  Which `(provider, model_id)` reaches the harness for a given execution request is
  resolved server-side through a four-tier precedence (request override →
  agent-profile default → project default *(no storage today)* → fleet default →
  auto-select), live-verified end to end and fully documented in [Choosing a model and
  a provider](book/src/user-guide/agent-runners.md#choosing-a-model-and-a-provider) —
  including why an auto-select request accepts today but never schedules. No `TACK_*`
  variable is involved on either side of this: routing the choice and holding the
  credential are different operations, and this file's table above has no row for a
  model provider or endpoint by design.
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

