# Deployment

This page is for the person who installs Tack on a server or a workstation. After reading it you can
get the binary, run it as a service behind HTTPS, check that it is healthy, and keep it updated.
Settings are in the [Configuration Reference](https://yielab.github.io/tack/admin-guide/configuration-reference.html),
tokens and origins in [Administration & Security](https://yielab.github.io/tack/admin-guide/administration.html),
backups in [Backup & Restore](https://yielab.github.io/tack/admin-guide/backup-restore.html), and
machines that run agents in
[Installing and Operating Runners](https://yielab.github.io/tack/admin-guide/runners.html).

Tack is a **single, self-contained binary** with the web UI embedded. One process serves the REST API
(`/api/*`), the WebSocket and the web UI, all from one origin, so there is no separate frontend service
and no static host to run. All state lives in one SQLite file plus an attachments directory.
Deployment is therefore "run one binary behind a reverse proxy."

---

## Table of Contents

1. [Get the binary](#get-the-binary)
2. [Run it](#run-it)
3. [Systemd service (recommended)](#systemd-service-recommended)
4. [Per-user service (no root)](#per-user-service-no-root)
5. [Reverse proxy + HTTPS (Caddy)](#reverse-proxy--https-caddy)
6. [Reverse proxy (nginx)](#reverse-proxy-nginx)
7. [Docker](#docker)
8. [Configuration](#configuration)
9. [Backups](#backups)
10. [Monitoring & logging](#monitoring--logging)
11. [Security checklist](#security-checklist)
12. [Scaling considerations](#scaling-considerations)
13. [Maintenance](#maintenance)

---

## Get the binary

### Option A — download a release

Prebuilt binaries for Linux (x86_64, and ARM64 from the release after `v0.1.0-beta.9`), macOS (Intel and
Apple Silicon) and Windows are attached to each [GitHub Release](https://github.com/yielab/tack/releases), along with the desktop
app installers. Each archive contains the single `tack` executable, `LICENSE`, `README.md`, and a
`QUICKSTART.txt`. Each release also has a separate `tack-runner` archive, for machines that only run
agents. Releases ship a `SHA256SUMS` file, build provenance attestations and SBOMs — verify before
deploying:

```bash
sha256sum -c SHA256SUMS            # checksums
gh attestation verify tack --repo yielab/tack   # provenance (optional)
```

### Option B — one-line installer

```bash
TAG=$(curl -fsSL "https://api.github.com/repos/yielab/tack/releases?per_page=1" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
curl --proto '=https' --tlsv1.2 -fsSL \
  "https://github.com/yielab/tack/releases/download/$TAG/install.sh" \
  | TACK_VERSION="$TAG" sh
```

Fetches the installer from the release it installs and installs `tack` from that release into
`~/.local/bin` (set `TACK_INSTALL_DIR` to change it). It verifies the archive against that release's
own `SHA256SUMS` before extracting anything, and refuses to install on a mismatch — Option A's manual
check, done for you. `TACK_SKIP_CHECKSUM=1` skips the check and is the only way to do so. The README
lists the Homebrew, Scoop and PowerShell installers.

### Option C — build from source

The web UI must be built first so `--features embed-spa` can embed it. The Makefile does both steps:

```bash
git clone https://github.com/yielab/tack.git
cd tack
make build
# → npm --prefix frontend ci && npm --prefix frontend run build
#   cargo build -p tack-cli --release --features embed-spa
# Produces target/release/tack  (UI embedded)
```

For a fully static Linux binary (no glibc dependency — ideal for minimal hosts and containers):

```bash
rustup target add x86_64-unknown-linux-musl
sudo apt-get install -y musl-tools
npm --prefix frontend ci && npm --prefix frontend run build
cargo build --release --target x86_64-unknown-linux-musl -p tack-cli --features embed-spa
# → target/x86_64-unknown-linux-musl/release/tack
```

---

## Run it

```bash
# Bare `tack` (or `tack serve`) starts the server + web UI.
./tack

# Open http://127.0.0.1:3210 — the page loads and talks to /api from the same origin.
```

By default Tack binds `127.0.0.1:3210`, writes `tack.db` in the current directory, and stores
attachments in `./storage`. Point those anywhere with environment variables:

```bash
TACK_HOST=127.0.0.1 \
TACK_PORT=3210 \
TACK_DATABASE_URL="sqlite:/var/lib/tack/tack.db?mode=rwc" \
TACK_STORAGE_DIR="/var/lib/tack/storage" \
  ./tack
```

The same binary is also the CLI client (`./tack --help`, `./tack add`, `./tack list`, …) — it talks to
a running server over HTTP, never the database directly.

**The token.** On a loopback bind with no `TACK_API_TOKEN`, the server creates a token file named
`api-token` next to the database on its first start (mode `0600`) and requires it on every API
request. The web page gets it as a cookie; the CLI finds it on the same machine. See
[Administration & Security](https://yielab.github.io/tack/admin-guide/administration.html#authentication).

> **Security note:** bind to `127.0.0.1` and put a reverse proxy in front. If you must bind a
> non-loopback address (`TACK_HOST=0.0.0.0`), set `TACK_API_TOKEN` — otherwise the API (including the
> "download the whole database" endpoint) would be open to anyone who can reach the port, so the
> server refuses to start in that configuration, before any network resource opens. Set
> `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` only to accept that risk deliberately (for example
> behind a trusted authenticating proxy).

**Behind a proxy, list the public origin.** The server answers a request only when its `Host` is the
local machine, the address Tack is bound to, or the host of an origin in `TACK_ALLOWED_ORIGINS`.
Behind a reverse proxy at `tack.example.com`, set
`TACK_ALLOWED_ORIGINS=https://tack.example.com`, or every request, including the health check, is
answered `421`. The proxy must pass the original `Host` header on, as the examples below do.

---

## Systemd service (recommended)

For a native Linux host, run Tack as a hardened systemd unit bound to loopback, with a reverse proxy
terminating TLS.

```bash
# 1. Install the binary and create a service user + data dirs
sudo install -m 0755 tack /usr/local/bin/tack
sudo useradd --system --home /var/lib/tack --shell /usr/sbin/nologin tack
sudo mkdir -p /var/lib/tack/storage
sudo chown -R tack:tack /var/lib/tack

# 2. Write the unit
sudo tee /etc/systemd/system/tack.service >/dev/null <<'EOF'
[Unit]
Description=Tack project management
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=tack
Group=tack
WorkingDirectory=/var/lib/tack
Environment=TACK_HOST=127.0.0.1
Environment=TACK_PORT=3210
Environment=TACK_DATABASE_URL=sqlite:/var/lib/tack/tack.db?mode=rwc
Environment=TACK_STORAGE_DIR=/var/lib/tack/storage
Environment=TACK_ALLOWED_ORIGINS=https://tack.example.com
Environment=TACK_LOG_LEVEL=info
Environment=TACK_LOG_JSON=true
# The server creates /var/lib/tack/api-token on first start and requires it.
# To choose the token yourself instead:
# Environment=TACK_API_TOKEN=change-me-to-a-long-random-secret
ExecStart=/usr/local/bin/tack serve
Restart=always
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/tack

[Install]
WantedBy=multi-user.target
EOF

# 3. Enable and start
sudo systemctl daemon-reload
sudo systemctl enable --now tack
sudo systemctl status tack
```

Logs go to the journal: `sudo journalctl -u tack -f`.

This unit runs the board only. It hides `/home` from the process, and the embedded runner needs your
agents' sign-ins and projects, so do not add `--with-runner` to it. Attach runners as described in
[Installing and Operating Runners](https://yielab.github.io/tack/admin-guide/runners.html).

---

## Per-user service (no root)

For a personal machine — no dedicated service account, no `sudo` — `tack service` installs the same
idea as a **user** unit instead of a system one: a systemd user unit on Linux
(`~/.config/systemd/user/tack.service`, `systemctl --user`), a launchd agent on macOS. It uses this
OS's own per-user application-data folder for the database, storage, runner state, and log file, so
nothing is written next to wherever the command was run. Not supported on Windows; install the desktop
app there instead.

```sh
tack service install     # writes the unit, then `systemctl --user enable --now tack`
tack service status       # prints the unit's state and the health URL
tack service uninstall    # stops and removes the unit; the data root is left untouched
```

```text
Created symlink ~/.config/systemd/user/default.target.wants/tack.service → ~/.config/systemd/user/tack.service.
Installed and started the tack user service.
  Unit file: ~/.config/systemd/user/tack.service
  Data root: ~/.local/share/tack
  Health:    http://127.0.0.1:3210/api/health
```

`tack service status` prints:

```text
State:  active
Health: http://127.0.0.1:3210/api/health
```

`tack service uninstall` prints:

```text
Removed "~/.config/systemd/user/default.target.wants/tack.service".
Removed the tack user service. The data root was left untouched.
```

The data root holds `tack.db`, `storage/`, `runner/` and `logs/tack.log`, in a folder only your user can
open. The unit runs `tack serve --with-runner`, so agents can run on this computer at once, and sets
`TACK_DATABASE_URL`, `TACK_STORAGE_DIR`, `TACK_RUNNER_STATE_DIR` and `TACK_LOG_FILE` to paths under the
data root. It runs in the data root, so a `tack.toml` from the directory where you ran the command is
not read. To give the runner a settings file, add `Environment=TACK_LOCAL_RUNNER_CONFIG=<path>` to the
unit and reload it. `uninstall` stops the service and removes its unit file but never touches the data
root, so a later `install` picks the same database back up. Prefer the system-level unit above for a
shared or internet-facing deployment — this one runs as your own user and stops when your user
session's systemd instance does (`loginctl enable-linger` keeps it running across logouts).

---

## Reverse proxy + HTTPS (Caddy)

Caddy terminates TLS (automatic Let's Encrypt), forwards everything to the single Tack process, and
upgrades the WebSocket by itself: `reverse_proxy` needs no special block for it.

```caddyfile
tack.example.com {
    encode gzip

    # One upstream serves the API, the WebSocket, and the web UI.
    reverse_proxy 127.0.0.1:3210

    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options    "nosniff"
        X-Frame-Options           "SAMEORIGIN"
        Referrer-Policy           "strict-origin-when-cross-origin"
    }

    log {
        output file /var/log/caddy/tack.log
    }
}
```

Reload Caddy after editing (`sudo systemctl reload caddy`). Set
`TACK_ALLOWED_ORIGINS=https://tack.example.com` on Tack, as described under [Run it](#run-it).

---

## Reverse proxy (nginx)

```nginx
server {
    listen 80;
    server_name tack.example.com;

    location / {
        proxy_pass http://127.0.0.1:3210;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket upgrade (board live-updates)
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
    }
}
```

Add TLS with Certbot: `sudo certbot --nginx -d tack.example.com`. If the board does not update live,
check that the proxy forwards the `Upgrade` and `Connection` headers (the two lines above) and does
not time the connection out.

---

## Docker

A `Dockerfile` and `docker-compose.yml` ship at the repo root. The image is a three-stage build (build
the web UI, compile a static musl binary that embeds it, copy it onto a distroless base), producing a
shell-less image barely larger than the binary. There is a single service and a single volume for the
database and attachments.

The container binds `0.0.0.0` internally, so it **refuses to start until you set `TACK_API_TOKEN`**
(uncomment the example in `docker-compose.yml`) or `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1`.

```bash
# Build and run with compose (after setting TACK_API_TOKEN in docker-compose.yml)
docker compose up -d
curl http://localhost:3210/api/health
docker compose logs -f

# Or plain docker
docker build -t tack:latest .
docker run -d --name tack -p 3210:3210 -v tack-data:/data \
  -e TACK_API_TOKEN='a-long-random-secret' tack:latest
```

Everything is stored under `/data` (`/data/tack.db` + `/data/storage`), persisted by the named
volume. The container has no runner; attach runners from other machines. The distroless image has no
shell, so health-check the container from the host (`curl .../api/health`) or via a proxy.

---

## Configuration

The server reads built-in defaults, then `tack.toml` in its working directory, then `TACK_*`
environment variables, which win. [Configuration](https://yielab.github.io/tack/admin-guide/configuration.html)
explains the order, shows a `tack.toml` and says how to confirm a change took effect; the
[Configuration Reference](https://yielab.github.io/tack/admin-guide/configuration-reference.html) lists every
setting, including the optional integrations (outbound webhooks, GitHub sync, S3-compatible cloud
backup, tasks that projects start by themselves).

For a deployment you set these:

| Setting | Why |
|---|---|
| `TACK_HOST`, `TACK_PORT` | Keep `127.0.0.1` behind a proxy. |
| `TACK_DATABASE_URL`, `TACK_STORAGE_DIR` | Put the data where you back it up. |
| `TACK_API_TOKEN` | Required on a non-loopback bind; on loopback the server makes its own. |
| `TACK_ALLOWED_ORIGINS` | The public origin of a proxy, so its host name is accepted. |
| `TACK_LOG_LEVEL`, `TACK_LOG_JSON`, `TACK_LOG_FILE` | Log verbosity and format; see [Monitoring & logging](#monitoring--logging). |

---

## Backups

Tack has built-in backup and restore; you do not need to reach into SQLite yourself.

```bash
tack backup /var/backups/tack/tack.db    # a consistent snapshot of the database, while the server runs
tack restore /var/backups/tack/tack.db   # stages a restore, applied on the next restart
tack backup --remote                     # a database-plus-attachments bundle to S3-compatible storage
```

A snapshot covers the database only, so back up the storage directory too, or use the remote
bundle, which includes it. Automate with cron or a systemd timer and test a restore. Hot backup,
staged restore, remote bundles and what each includes are in
[Backup & Restore](https://yielab.github.io/tack/admin-guide/backup-restore.html); the S3 setup is
in [Administration & Security](https://yielab.github.io/tack/admin-guide/administration.html#cloud-backup-s3-compatible).

---

## Monitoring & logging

### Health check

```bash
curl http://127.0.0.1:3210/api/health
# {"status":"ok","version":"<version>","migrations_applied":<count>}
```

Point uptime monitoring (Uptime Kuma, Healthchecks.io, a load-balancer probe) at `/api/health`, which
needs no token, and alert on non-200. Probe the address Tack is bound to or an origin listed in
`TACK_ALLOWED_ORIGINS`; any other `Host` is answered `421`.

### Logs

```bash
sudo journalctl -u tack -f                       # systemd
sudo journalctl -u tack --since "24 hours ago"   # export a window
docker compose logs -f                            # Docker
```

Set `TACK_LOG_JSON=true` for structured logs an aggregator can parse. `TACK_LOG_FILE` writes a file
in addition to standard output.

### Debug endpoints

`/api/debug/info` and `/api/debug/db-stats` return build and config details and per-table row counts.
They sit behind the same token as the rest of the API.

---

## Security checklist

- [ ] Bind `TACK_HOST=127.0.0.1`; expose only through the reverse proxy.
- [ ] If binding non-loopback, set a long random `TACK_API_TOKEN`.
- [ ] Terminate TLS at Caddy/nginx (HSTS enabled).
- [ ] List the proxy's public origin, and any separate-origin browser client, in `TACK_ALLOWED_ORIGINS`.
- [ ] Run under a dedicated, unprivileged service user (the systemd unit above).
- [ ] Firewall the raw app port so only the proxy can reach it.
- [ ] Automate backups and test a restore.
- [ ] Add rate limiting at the proxy if the instance is public.
- [ ] Keep `TACK_LOG_LEVEL=info` or `warn` in production; `TACK_LOG_JSON=true`.

### Firewall (UFW)

```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw deny  3210/tcp    # block direct app access; only the proxy reaches it
sudo ufw enable
```

### Rate limiting (Caddy)

Caddy's `rate_limit` directive comes from a plugin (`github.com/mholt/caddy-ratelimit`), so you need a
Caddy build that includes it.

```caddyfile
tack.example.com {
    rate_limit {
        zone tack {
            key    {remote_host}
            events 100
            window 1m
        }
    }
    reverse_proxy 127.0.0.1:3210
}
```

---

## Scaling considerations

Tack is a single-writer SQLite application, designed for a solo developer or a small team. There is no
horizontal write scaling — that is a deliberate design choice, not a gap. Practical guidance:

- **Vertical:** the binary is small and a modest VM handles a small team comfortably.
- **Write throughput:** SQLite serializes writes. This is fine at small-team scale; it is the first
  thing you would feel under heavy concurrent writes.
- **Concurrency:** the database runs in WAL mode for better read/write concurrency. Run exactly one
  `tack` server per database file.
- Multi-user auth, per-user identity, and Postgres are out of scope for the current design.

For the numbers this design gives, see the
[benchmarks](https://github.com/yielab/tack/blob/develop/docs/BENCHMARKS.md).

---

## Maintenance

Something wrong? Start from the symptom in
[Troubleshooting](https://yielab.github.io/tack/admin-guide/troubleshooting.html).

### Update procedure

```bash
# 1. Back up first
tack backup /var/backups/tack/pre-upgrade-$(date +%F).db

# 2. Replace the binary (download a new release or rebuild)
sudo install -m 0755 tack /usr/local/bin/tack

# 3. Restart — migrations run forward automatically on startup
sudo systemctl restart tack
curl http://127.0.0.1:3210/api/health
```

Migrations only run forward. To go back to an older version, restore the backup you took in step 1.
The [Migration Guide](https://github.com/yielab/tack/blob/develop/docs/MIGRATION-GUIDE.md) says what
runs on startup and what protects a database that fails an upgrade.

### Database vacuum

```bash
sqlite3 /var/lib/tack/tack.db "VACUUM;"    # reclaim space; run occasionally
```

### Log rotation (systemd host with a file sink)

`TACK_LOG_FILE` appends to the file and never reopens it, so rotate by copying and truncating. The
unit above can write only under `/var/lib/tack`, so put the log there (for example
`TACK_LOG_FILE=/var/lib/tack/logs/tack.log`) or add its folder to `ReadWritePaths`.

```bash
sudo tee /etc/logrotate.d/tack >/dev/null <<'EOF'
/var/lib/tack/logs/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    copytruncate
}
EOF
```

If you use `journalctl` (no `TACK_LOG_FILE`), systemd already rotates the journal.

---

## Support

- [GitHub Issues](https://github.com/yielab/tack/issues)
- [API Reference](https://github.com/yielab/tack/blob/develop/docs/API-REFERENCE.md)
- [Security Policy](https://github.com/yielab/tack/blob/develop/SECURITY.md)
