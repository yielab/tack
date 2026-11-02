# Administration and Security

This page is for the person who decides who can reach Tack and how it is protected. After reading it you can set the API token, give an agent its own credential, choose the allowed origins, expose Tack safely on a network and send events to a webhook.

Tack has no identity model: there are no user accounts, no sessions, and no per-user permissions. `assignee` is a free-text label on an item, not an account, and anyone can type any name into it. Every person who authenticates is the same single operator, via one shared token. Tack is built for one operator, or a small team willing to share that one secret, not for telling users apart. The one other kind of caller is an agent you hand a key to: an [agent credential](#agent-credentials), which is a key for software and not an account for a person.

Tack is local-first by default. It binds to `127.0.0.1`, keeps everything in a single SQLite file, and on that loopback bind it creates a token of its own and requires it. This page covers what you set when you go beyond that. Every setting below is read from `tack.toml` or environment variables at startup; see [Configuration](configuration-reference.md) for how those are loaded and for every default.

All examples assume the default base URL `http://127.0.0.1:3210`. The `curl` examples leave out the `Authorization` header that every call needs; see [Authentication](#authentication).

---

## Authentication

The API always has a token, except in one case described under [Network exposure](#network-exposure-and-tls). **Every** `/api/*` route requires `Authorization: Bearer <token>` or the session cookie below. Two endpoints are open: `GET /api/health`, a liveness probe, and `GET /api/openapi.json`.

You can choose the token or let the server make one:

- **You choose it.** Set `TACK_API_TOKEN` (or `api_token` in `tack.toml`) and restart the server.
- **The server makes it.** On a loopback bind with no `TACK_API_TOKEN`, the server reads `api-token` from the directory of its database file, or creates it there on the first start (mode `0600`, one line). A server that cannot read or create the file does not start, and says which path failed. An in-memory database has no directory, so its token lives only in the process.

Answering a run's questions needs a **second secret**, kept apart from the API token. The server keeps it the same way: on a loopback bind with no `TACK_EXECUTION_DECISION_TOKEN`, it reads `decision-token` beside `api-token`, or creates it on the first start (mode `0600`). The variable still wins. The desktop app hands it to its window, so nobody pastes it there. In a browser, **Waiting for you** and the question itself ask for it once per session and name this file; open the file and paste its one line.

```sh
TACK_API_TOKEN='a-long-random-secret' tack serve
```

Requests without (or with a wrong) token receive `401 Unauthorized`. Supply the token on every call:

```sh
# Rejected — no token
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:3210/api/projects
# → 401

# Accepted
curl -s http://127.0.0.1:3210/api/projects \
  -H 'Authorization: Bearer a-long-random-secret'
```

The other end of the connection finds the token like this:

- **The browser.** The page the server answers at `/` sets the token as a `tack_session` cookie (`HttpOnly`, `SameSite=Strict`, sent only to `/api`). You never type it.
- **The `tack` command.** It looks, in order, at `--token`, `TACK_API_TOKEN`, `~/.tackrc`, and then the `api-token` file of a local server (`tack/api-token` in this system's per-user data folder). A server whose database is elsewhere needs `TACK_API_TOKEN` set on both ends.

The token value is never written to logs. Use a long, random string and rotate it by restarting with a new value. For what else stands between a web page and your board (the `Host` rule and the cross-site rule), see [What the Agent Can Reach](what-the-agent-can-reach.md#who-can-talk-to-the-board).

---

## Agent credentials

An agent that uses Tack on its own, for example through `tack mcp`, should not hold the operator's token: with it the agent is you, can read and change every project, and its changes look like yours. Give it a credential instead. Open **Settings**, find **Agent credentials**, give the credential a name, choose the projects it may reach (or **Every project**, which also covers projects made later) and tick what it may do:

| It may | What that lets the agent do |
|---|---|
| **Read** | See the project, its items, comments, dependencies and roles, the list of teams, and what a team is doing, limited to the work it holds for projects the credential can read (the work no project started is shown only to a credential that reads every project). |
| **Propose** | Propose a change set: a list of changes to the board, kept for a person to apply. |
| **Act** | Everything **Propose** does, and lets the changes the project's *Agents on the board* setting allows apply at once. |
| **Converse** | Read and write in a thread kept for the credential, and nothing else. |

The secret, which starts with `tack_agent_`, is shown once when you issue or rotate a credential, with a **Copy** button; Tack keeps only a hash of it and cannot show it again. The agent sends it the way the operator's token is sent, as `Authorization: Bearer <secret>`; for `tack mcp`, set `TACK_API_TOKEN` to the secret in the agent's configuration.

An agent changes the board only through change sets: it cannot create or edit an item, write a comment or add a dependency by calling those routes, which answer `403` and name the change-set route. Through `tack mcp`, the tools that create, update, move and comment build a change set for it. The project's *Agents on the board* setting decides which changes apply at once and which wait for a person; the stricter of that and the credential's scopes wins. You set it in the project's **Settings → Automation**, under **Agents**, and the changes that wait appear in **Waiting for you** and on the project's **Change sets** page (see [When an agent proposes changes](../user-guide/reviewing-runs.md#when-an-agent-proposes-changes)). **Agents → Add a manager** issues a credential for you and wires the rest; see [Managers and Threads](../user-guide/talking-with-a-manager.md#set-up-a-manager).

What a credential cannot do, whatever its scopes:

- It sees only the projects you named; the project list it receives holds only those, and a request for another project answers `403` naming the scope it lacks.
- It cannot answer a run's questions. Resolving a decision needs the second secret described above, which no credential holds or can reach.
- It cannot start runs, change settings, delete anything, import, back up or restore, or issue credentials. Those answer `403` with a pointer to the operator's token.
- It cannot sign as someone else. A comment an agent writes carries the credential's name as its author, whatever author the agent sent, and the items and links its change sets create or change show the same name.

A credential expires 90 days after it is issued or rotated. **Rotate** gives it a new secret and 90 more days, and the old secret stops working at once. **Revoke** ends it for good; what it already wrote stays, signed with its name. An expired, revoked or unknown secret answers `401`, and the list shows when each credential was last used.

## CORS

Browsers block cross-origin API calls unless the server explicitly allows the page's origin. Tack's allow-list is `TACK_ALLOWED_ORIGINS`, a comma-separated list of exact origins (scheme + host + port). The default is:

```
http://localhost:8080,http://127.0.0.1:8080,http://localhost:3210,http://127.0.0.1:3210
```

Change it when the browser loads the UI from a different origin than the API — for example a reverse-proxy hostname or a separate frontend dev server:

```sh
TACK_ALLOWED_ORIGINS='https://tack.example.com,https://app.example.com' tack serve
```

List every origin you serve the UI from; entries are matched exactly, with no wildcards. The bundled SPA served by the same process needs no extra entry.

---

## Network exposure and TLS

Tack binds to `TACK_HOST` (default `127.0.0.1`) on `TACK_PORT` (default `3210`), so out of the box it is reachable only from the local machine. Because Tack has no per-user accounts (see above), a bind reachable from beyond the local machine with no `TACK_API_TOKEN` configured would hand full read/write access, to the board and to the runner-scheduling surface, to anyone who can reach the port. The server makes no token of its own on such a bind, and refuses to start in that configuration:

```sh
TACK_HOST=0.0.0.0 TACK_PORT=3210 tack serve
# Error: refusing to bind 0.0.0.0 without TACK_API_TOKEN; bind to loopback,
# set TACK_API_TOKEN, or set TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1
# to accept the risk
```

To serve it on a LAN, set a token alongside the routable bind:

```sh
TACK_HOST=0.0.0.0 TACK_PORT=3210 TACK_API_TOKEN='a-long-random-secret' tack serve
```

If a token genuinely cannot be configured — for example a container reachable only on a
network you already trust — set `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` to
start anyway. This is an explicit acceptance of the risk above, not a default; leave it
unset unless you have a specific reason to widen the bind without a credential.

Tack does not terminate TLS itself. For any non-localhost deployment, place it behind a reverse proxy (Caddy, nginx, Traefik) that handles HTTPS and forwards to the local port. Keep `TACK_HOST=127.0.0.1` and let only the proxy reach it. See [Deployment](deployment.md) for full proxy and TLS setup.

---

## Request limits

Non-attachment requests are capped by `TACK_MAX_BODY_SIZE` (bytes, default `2097152` = 2 MB). This protects the JSON API from oversized payloads:

```sh
TACK_MAX_BODY_SIZE=5242880 tack serve   # raise to 5 MB
```

The file-upload endpoint (`POST /api/items/{id}/attachments`) is exempt from this limit and is always capped at 50 MB, regardless of `TACK_MAX_BODY_SIZE`.

---

## Webhooks

Set `TACK_WEBHOOK_URL` to receive an HTTP `POST` whenever work changes. Delivery is fire-and-forget: each event is sent on a background task with a 10-second timeout, and failures are logged but never block the originating request.

```sh
TACK_WEBHOOK_URL='https://hooks.example.com/tack' tack serve
```

### Event types

The event name is sent both as the `X-Tack-Event` request header and as the `event` field in the JSON body.

| Event | When it fires |
|-------|---------------|
| `item.created` | An item is created |
| `item.updated` | An item is updated (including status changes) |
| `item.deleted` | An item is deleted |
| `sprint.started` | A sprint transitions to **Active** |
| `sprint.completed` | A sprint transitions to **Closed** |
| `sprint.updated` | Any other sprint status change |
| `item.due_soon` | An item is due within the next hour (background check runs hourly) |

### Payload shapes

Every payload carries `event`, an RFC 3339 `timestamp`, and `project_id`. The remaining fields depend on the event:

```json
// item.created / item.updated / item.due_soon
{
  "event": "item.updated",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "item": { /* full item object */ }
}
```

```json
// item.deleted — carries the id only, since the item is gone
{
  "event": "item.deleted",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "item_id": "9ab3…"
}
```

```json
// sprint.started / sprint.completed / sprint.updated
{
  "event": "sprint.started",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "sprint_id": "44de…",
  "sprint_name": "Sprint 7",
  "status": "active"
}
```

### Signing

Set `TACK_WEBHOOK_SECRET` to sign every delivery. Tack computes an HMAC-SHA256 over the exact request body and sends it as:

```
X-Tack-Signature: sha256=<hex>
```

Verify it on the receiver by recomputing the HMAC of the raw body with the same secret and comparing (constant-time) against the header value. Reject any request whose signature does not match.

```sh
TACK_WEBHOOK_URL='https://hooks.example.com/tack' \
TACK_WEBHOOK_SECRET='shared-signing-secret' \
tack serve
```

---

## Logs and every other setting

Logging has three variables (`TACK_LOG_LEVEL`, `TACK_LOG_JSON`, `TACK_LOG_FILE`). Secrets (the API token, the webhook secret, the backup secret key and the GitHub token) are never written to logs at any level. The [Configuration](configuration-reference.md) page lists each `TACK_*` variable with its default and the debugging recipes. For cloud backup, see [Backup & Restore](backup-restore.md#remotecloud-backup); for proxy and TLS setup, see [Deployment](deployment.md).
