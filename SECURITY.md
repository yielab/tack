# Security Policy

## Scope

Tack is a **self-hosted, local-only** tool designed to run on `127.0.0.1`. It has no built-in authentication beyond the optional `TACK_API_TOKEN` bearer token — one shared token, no per-user identities — and is not intended to be exposed to the public internet without an authenticating reverse proxy.

## Reporting a Vulnerability

Open a [GitHub Issue](https://github.com/santiagoyie/tack/issues) marked **[security]** in the title.

If you prefer private disclosure, email **[yie.worker@gmail.com](mailto:yie.worker@gmail.com)** with subject `[tack] security`. I aim to respond within 7 days.

## Supported Versions

Only the latest release on `main` is actively maintained.

## Known Design Constraints

| Area | Notes |
| --- | --- |
| Authentication | Optional bearer token (`TACK_API_TOKEN`). Not a replacement for network-level controls. |
| SQLite | Single-file DB, no encryption at rest. |
| File uploads | Max 50 MB. Stored in `TACK_STORAGE_DIR` with UUID filenames; no execution. |
| CORS | Configurable via `TACK_ALLOWED_ORIGINS`. Defaults to localhost only. |
