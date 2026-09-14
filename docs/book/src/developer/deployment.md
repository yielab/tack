# Deployment

Tack is a single process with no external service dependencies. The deployment model is
intentionally minimal: copy a binary, point it at a directory, run it.

> This page is the book's rendering of the deployment guide — for the single-binary,
> systemd, Docker, reverse-proxy, backup, and troubleshooting models, see
> [`docs/DEPLOYMENT-GUIDE.md`](../../../DEPLOYMENT-GUIDE.md), included below. Edit that
> file, not this one, except for the Local Development section, which is specific to
> this workspace and has no home there. For tokens, CORS, webhooks, and cloud backup
> configuration, see [Administration & Security](../user-guide/administration.md).

---

## Local Development with Caddy (.test domain)

For local use behind the project's `Caddyfile.local`:

```
tack.test {
    reverse_proxy 127.0.0.1:3210
}
```

Import it from the global `/home/ox/Sites/Caddyfile` and reload:

```sh
sudo systemctl reload caddy
```

The app is then available at `https://tack.test`.

---

{{#include ../../../DEPLOYMENT-GUIDE.md}}
