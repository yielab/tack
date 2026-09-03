# Configuration

Configuration is loaded from `tack.toml` in the working directory. Environment variables
override TOML values. Both are optional — all settings have built-in defaults.

**The complete, authoritative table of every `TACK_*` variable — server, embedded
runner, standalone runner, backup, orchestration, and the execution domain — is
[`docs/CONFIG.md`](../../../CONFIG.md).** That file is updated the moment a variable is
added; this page is not a second copy of it. What follows here is the loading order and
one worked example.

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
