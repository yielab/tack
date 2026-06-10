# Security Policy

## Scope

FlexPM is a **local-first** tool designed to run on `127.0.0.1`. It has no built-in authentication beyond the optional `FLEXPM_API_TOKEN` bearer token, and is not intended to be exposed to the public internet without an authenticating reverse proxy.

## Reporting a Vulnerability

Open a [GitHub Issue](https://github.com/santiagoyie/flexpm/issues) marked **[security]** in the title.

If you prefer private disclosure, email **info@yielab.com** with subject `[flexpm] security`. I aim to respond within 7 days.

## Supported Versions

Only the latest release on `main` is actively maintained.

## Known Design Constraints

| Area | Notes |
| --- | --- |
| Authentication | Optional bearer token (`FLEXPM_API_TOKEN`). Not a replacement for network-level controls. |
| SQLite | Single-file DB, no encryption at rest. |
| File uploads | Max 50 MB. Stored in `FLEXPM_STORAGE_DIR` with UUID filenames; no execution. |
| CORS | Configurable via `FLEXPM_ALLOWED_ORIGINS`. Defaults to localhost only. |
