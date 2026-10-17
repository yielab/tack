# Configuration

This page is for the person who sets up a Tack server. After reading it you know where the settings
come from, in which order they win, and how to write a `tack.toml`. The exact name and default of
every setting are in the [Configuration Reference](configuration-reference.md); this page does not
copy that table.

Everything is optional. Tack runs with its built-in defaults, and you change only what you need.

## Where settings come from

The server reads three sources. A later one overrides an earlier one:

1. Built-in defaults.
2. `tack.toml` in the directory the server starts in.
3. Environment variables (`TACK_*`).

A runner has its own settings, with its own file and variables. They are described in
[Installing and Operating Runners](runners.md#the-three-ways-to-attach-a-runner).

## An example tack.toml

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
type, is ignored as a whole without an error, and the defaults apply. A key Tack does not know is
ignored too. After editing, confirm the change took effect, for example with `GET /api/debug/info`,
which reports the host, port and log level.

Settings that exist only as environment variables (the backup, webhook, GitHub, execution and
auto-start settings) are listed in the [Configuration Reference](configuration-reference.md).

## The API token

If you set nothing, a server bound to a loopback address creates a token file and requires it. When
`api_token` or `TACK_API_TOKEN` is set, that value is the token instead. Every request to `/api/*`
except `/api/health` and `/api/openapi.json` must include:

```text
Authorization: Bearer <token>
```

Requests without a valid token receive `401 Unauthorized`. The web page the server serves gets the
token as a cookie, so you never paste it into the browser. Where the token lives, how the CLI finds it
and what changes on a non-loopback bind are in [Administration & Security](administration.md#authentication).

## Logging

Development: plain text at debug level.

```sh
TACK_LOG_LEVEL=debug tack serve
```

Trace all SQL queries:

```sh
RUST_LOG=tack_db=trace,tack_api=debug tack serve
```

Production: JSON lines, also written to a file. The file is in addition to standard output, so a
service manager still captures the log.

```toml
log_level = "info"
log_json  = true
log_file  = "/var/log/tack/api.log"
```
