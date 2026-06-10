# Deployment

FlexPM is a single process with no external service dependencies. The deployment model is
intentionally minimal: copy a binary, point it at a directory, run it.

---

## Single-Binary Deployment (Recommended)

Build one binary that embeds the SPA and serves everything:

```sh
# 1. Build the frontend
cd frontend && npm ci && npm run build && cd ..

# 2. Build the release binary with embedded SPA
cargo build --release --features embed-spa -p flexpm-api

# Result: target/release/flexpm-api (~5 MB)
```

Copy the binary to the server and run it:

```sh
scp target/release/flexpm-api user@server:/opt/flexpm/flexpm-api
ssh user@server

# On the server
mkdir -p /var/data/flexpm /var/log/flexpm
FLEXPM_DATABASE_URL=sqlite:/var/data/flexpm/flexpm.db \
FLEXPM_STORAGE_DIR=/var/data/flexpm/storage \
FLEXPM_LOG_FILE=/var/log/flexpm/api.log \
/opt/flexpm/flexpm-api
```

The binary is statically linked and has no runtime dependencies.

---

## systemd Service

Create `/etc/systemd/system/flexpm.service`:

```ini
[Unit]
Description=FlexPM API server
After=network.target

[Service]
Type=simple
User=flexpm
WorkingDirectory=/opt/flexpm
ExecStart=/opt/flexpm/flexpm-api
Restart=on-failure
RestartSec=5

Environment=FLEXPM_DATABASE_URL=sqlite:/var/data/flexpm/flexpm.db
Environment=FLEXPM_STORAGE_DIR=/var/data/flexpm/storage
Environment=FLEXPM_LOG_FILE=/var/log/flexpm/api.log
Environment=FLEXPM_LOG_JSON=true
Environment=FLEXPM_API_TOKEN=change-me

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now flexpm
sudo journalctl -u flexpm -f
```

---

## Reverse Proxy with Caddy

Put Caddy in front to get automatic HTTPS:

```
flexpm.example.com {
    reverse_proxy 127.0.0.1:3210
}
```

```sh
sudo systemctl reload caddy
```

Caddy handles TLS certificates via Let's Encrypt automatically.

---

## Reverse Proxy with nginx

```nginx
server {
    listen 443 ssl;
    server_name flexpm.example.com;

    ssl_certificate     /etc/letsencrypt/live/flexpm.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/flexpm.example.com/privkey.pem;

    location / {
        proxy_pass         http://127.0.0.1:3210;
        proxy_http_version 1.1;
        proxy_set_header   Upgrade $http_upgrade;
        proxy_set_header   Connection "upgrade";
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
    }
}
```

The `Upgrade` and `Connection` headers are required for the WebSocket endpoint (`/api/projects/{id}/boards/live`).

---

## Local Development with Caddy (.test domain)

For local use behind the project's `Caddyfile.local`:

```
flexpm.test {
    reverse_proxy 127.0.0.1:3210
}
```

Import it from the global `/home/ox/Sites/Caddyfile` and reload:

```sh
sudo systemctl reload caddy
```

The app is then available at `https://flexpm.test`.

---

## Security Checklist

- [ ] Set `FLEXPM_API_TOKEN` to a long random string
- [ ] Set `FLEXPM_ALLOWED_ORIGINS` to your domain only
- [ ] Run behind HTTPS (Caddy or nginx with Let's Encrypt)
- [ ] Run as a non-root system user
- [ ] Set database and storage paths outside the binary directory
- [ ] Schedule daily backups (see [Backup & Restore](../user-guide/backup-restore.md))
- [ ] Restrict port 3210 at the firewall — only the reverse proxy needs it

---

## Troubleshooting

**Migration errors on startup:**

```sh
sqlite3 /var/data/flexpm/flexpm.db "SELECT * FROM _migrations;"
```

If a migration record is corrupt, delete that row and restart — the migration will re-run.

**Database locked:**

SQLite allows only one writer at a time. Check for another process holding the file:

```sh
lsof /var/data/flexpm/flexpm.db
```

**FTS5 not available:**

```sh
sqlite3 /var/data/flexpm/flexpm.db "PRAGMA compile_options;" | grep FTS5
```

If FTS5 is missing, recompile SQLite with `SQLITE_ENABLE_FTS5`, or install a SQLite package that includes it.

**Binary size too large:**

The release binary uses `opt-level = "z"`, LTO, and symbol stripping. A >10 MB binary usually means `--features embed-spa` picked up a large frontend dist. Check `du -sh frontend/dist/`.
