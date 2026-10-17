# Deployment

This page is for the person who installs Tack and keeps it running. After reading it you can get the
binary, run it as a service, put HTTPS in front of it, run it in Docker, and update it safely.

Tack is a single process with no external service dependencies. The deployment model is intentionally
minimal: copy a binary, point it at a directory, run it. The deployment guide below covers the binary,
systemd, a per-user service, Caddy and nginx, Docker, backups, monitoring, a security checklist and
updates. For tokens, allowed origins, webhooks and cloud backup, see
[Administration & Security](administration.md). To run agents on a machine, see
[Installing and Operating Runners](runners.md).

---

## Local development with Caddy (.test domain)

On a workstation that serves several projects, each project declares its own host in a
`Caddyfile.local` that the workstation's global Caddyfile imports. For Tack the block is:

```text
tack.test {
    reverse_proxy 127.0.0.1:3210
}
```

Reload Caddy after importing it:

```sh
sudo systemctl reload caddy
```

The app is then available at `https://tack.test`. That name is in Tack's default list of allowed origins,
so no further setting is needed.

---

{{#include ../../../DEPLOYMENT-GUIDE.md}}
