# Configuration

Configuration is loaded from `tack.toml` in the working directory. Environment variables
override TOML values. Both are optional — all settings have built-in defaults.

---

## Full Reference

| Env var | TOML key | Default | Description |
|---|---|---|---|
| `TACK_HOST` | `host` | `127.0.0.1` | Bind address |
| `TACK_PORT` | `port` | `3210` | TCP port |
| `TACK_DATABASE_URL` | `database_url` | `sqlite:tack.db?mode=rwc` | SQLite path; `mode=rwc` creates the file if missing |
| `TACK_LOG_LEVEL` | `log_level` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `TACK_LOG_JSON` | `log_json` | `false` | Structured JSON logs (for log aggregators) |
| `TACK_LOG_FILE` | `log_file` | _(none)_ | Write logs to this path in addition to stdout |
| `TACK_STORAGE_DIR` | `storage_dir` | `./storage` | Directory for uploaded attachment files |
| `TACK_API_TOKEN` | `api_token` | _(none)_ | When set, all `/api/*` requests need `Authorization: Bearer <token>` |
| `TACK_ALLOWED_ORIGINS` | `allowed_origins` | `localhost:8080,127.0.0.1:8080` | Comma-separated CORS allow-list |
| `TACK_MAX_BODY_SIZE` | `max_body_size` | `2097152` | Global request body limit in bytes (2 MB). File upload endpoints are always 50 MB. |
| `TACK_ORCH_ENABLE` | `orch_enable` | `false` | Enables the Agent-Factory Control Center: the orchestration reconciler background task and the `/api/control-planes`, `/api/projects/{id}/orch-link`, `/api/fleet` routes. Unset ⇒ no reconciler task is spawned and every one of those routes returns `404` |
| `TACK_ORCH_POLL_SECS` | `orch_poll_secs` | `10` | Reconciler base poll interval in seconds, before per-plane exponential backoff and jitter |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `orch_event_retention_days` | `90` | Days of mirrored orchestration event/metric history kept before the retention sweep rolls old rows into per-day aggregates and deletes them |
| `TACK_ORCH_APPROVAL_TOKEN` | `orch_approval_token` | _(none)_ | Separate shared secret for granting/denying a docket approval. Distinct from `TACK_API_TOKEN` on purpose — never logged |

---

## Example tack.toml

```toml
host         = "127.0.0.1"
port         = 3210
database_url = "sqlite:/var/data/tack.db?mode=rwc"
log_level    = "info"
log_json     = false
log_file     = "/var/log/tack/api.log"
storage_dir  = "/var/data/tack-storage"
# api_token  = "change-me"
allowed_origins = "https://pm.example.com"
max_body_size   = 4194304   # 4 MB
```

---

## API Token

When `api_token` is set, every request to `/api/*` must include:

```
Authorization: Bearer <token>
```

Requests without a valid token receive `401 Unauthorized`. The `/api/health` endpoint is always public.

**Frontend:** The bundled SPA reads the token from `VITE_API_TOKEN` in `frontend/.env`:

```sh
# frontend/.env
VITE_API_URL=http://127.0.0.1:3210
VITE_API_TOKEN=change-me
```

Rebuild the frontend after changing `.env`.

---

## Logging

Development — plain text at debug level:

```sh
TACK_LOG_LEVEL=debug cargo run -p tack-cli -- serve
```

Trace all SQL queries:

```sh
RUST_LOG=tack_db=trace,tack_api=debug cargo run -p tack-cli -- serve
```

Production — JSON logs to a file:

```toml
log_level = "info"
log_json  = true
log_file  = "/var/log/tack/api.log"
```

---

## Precedence

1. Environment variables ← highest priority
2. `tack.toml` in the current directory
3. Built-in defaults ← lowest priority
