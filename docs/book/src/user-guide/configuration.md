# Configuration

Configuration is loaded from `flexpm.toml` in the working directory. Environment variables
override TOML values. Both are optional — all settings have built-in defaults.

---

## Full Reference

| Env var | TOML key | Default | Description |
|---|---|---|---|
| `FLEXPM_HOST` | `host` | `127.0.0.1` | Bind address |
| `FLEXPM_PORT` | `port` | `3210` | TCP port |
| `FLEXPM_DATABASE_URL` | `database_url` | `sqlite:flexpm.db?mode=rwc` | SQLite path; `mode=rwc` creates the file if missing |
| `FLEXPM_LOG_LEVEL` | `log_level` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `FLEXPM_LOG_JSON` | `log_json` | `false` | Structured JSON logs (for log aggregators) |
| `FLEXPM_LOG_FILE` | `log_file` | _(none)_ | Write logs to this path in addition to stdout |
| `FLEXPM_STORAGE_DIR` | `storage_dir` | `./storage` | Directory for uploaded attachment files |
| `FLEXPM_API_TOKEN` | `api_token` | _(none)_ | When set, all `/api/*` requests need `Authorization: Bearer <token>` |
| `FLEXPM_ALLOWED_ORIGINS` | `allowed_origins` | `localhost:8080,127.0.0.1:8080` | Comma-separated CORS allow-list |
| `FLEXPM_MAX_BODY_SIZE` | `max_body_size` | `2097152` | Global request body limit in bytes (2 MB). File upload endpoints are always 50 MB. |

---

## Example flexpm.toml

```toml
host         = "127.0.0.1"
port         = 3210
database_url = "sqlite:/var/data/flexpm.db?mode=rwc"
log_level    = "info"
log_json     = false
log_file     = "/var/log/flexpm/api.log"
storage_dir  = "/var/data/flexpm-storage"
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
FLEXPM_LOG_LEVEL=debug cargo run --bin flexpm-api
```

Trace all SQL queries:

```sh
RUST_LOG=flexpm_db=trace,flexpm_api=debug cargo run --bin flexpm-api
```

Production — JSON logs to a file:

```toml
log_level = "info"
log_json  = true
log_file  = "/var/log/flexpm/api.log"
```

---

## Precedence

1. Environment variables ← highest priority
2. `flexpm.toml` in the current directory
3. Built-in defaults ← lowest priority
