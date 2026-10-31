# Tack API Reference

This page is for integrators and contributors who call Tack's HTTP API. It covers what the generated reference cannot say: how the two kinds of caller authenticate, the live board channel, rules a schema cannot express, and a few worked calls. After reading it you can write a client that authenticates correctly and avoids the surprises.

**Spec of record:** [`docs/openapi.json`](openapi.json), generated from the router — where this page and the spec disagree, the spec is right.
**Base URL:** `http://localhost:3210/api`
**WebSocket URL:** `ws://localhost:3210/api/projects/{id}/boards/live`

> **Every path, method, parameter and request/response schema name** is generated, not written
> by hand: `GET /api/openapi.json` from a running server, the checked-in
> [`docs/openapi.json`](openapi.json), or its rendering at
> [`docs/book/src/developer/api-reference.md`](book/src/developer/api-reference.md)
> (`scripts/gen-api-reference.py`, run by `./scripts/regen-generated.sh`). CI gates all three, so
> none can drift from the code. This page does not list endpoints. It covers what a schema cannot
> say: the two authentication surfaces, the WebSocket channel, rules that span fields or routes,
> and two worked calls.

---

## Table of Contents

1. [Authentication — two surfaces](#authentication--two-surfaces)
2. [WebSocket Events](#websocket-events)
3. [Notable behaviors not visible in the schema](#notable-behaviors-not-visible-in-the-schema)
4. [Worked examples](#worked-examples)
5. [Error envelope](#error-envelope)
6. [Rate limiting, CORS, versioning](#rate-limiting-cors-versioning)

---

## Authentication — two surfaces

Tack has two structurally separate auth surfaces — a caller never crosses from one
into the other, and neither route is an exemption carved out of the other's
middleware.

### Operator surface — `/api`

Every `/api/*` route except `GET /api/health` and `GET /api/openapi.json` requires the
server's token, as a bearer header or as the `tack_session` cookie that the page at `/`
hands to the browser:

```http
Authorization: Bearer <token>
```

With `TACK_API_TOKEN` set, that is the token. Left unset on a loopback bind, the server
reads or creates an `api-token` file beside its database and requires it. A non-loopback
bind with no token is refused at startup unless `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`
is set. A request whose `Host` does not name this server is refused (`misdirected_host`), and so is
a state-changing request sent by a page on another site (`cross_site_request`). Requests
with a missing or wrong token receive `401 Unauthorized`. There are no per-user accounts —
all clients share the single token. `x-tack-principal` is
overwritten from server config by `middleware::inject_operator_principal` and never
read from the request, so a trusted-looking header cannot impersonate another
caller. See [Administration & Security](book/src/admin-guide/administration.md) for
setup.

A privileged operator action carries its **own** token, distinct from
`TACK_API_TOKEN` and fail-closed when unset (the route rejects rather than falling
back to the operator token):

| Token | Gates |
|---|---|
| `TACK_EXECUTION_DECISION_TOKEN` | `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve` |

It is never logged.

### Runner protocol surface — `/api/runner/v1`

The 14 `/api/runner/v1/*` paths (enroll, refresh, claim, heartbeat, and per-attempt
accept/start/events/decisions/decisions-poll/artifacts/artifact content/completion/cancellation/recovery-observation)
are a **sibling** of `/api` on the outer router, not nested under it — they never
traverse `require_token` at all. Each handler authenticates its own hashed bearer
credential via `handlers/runner_protocol/runner_auth.rs`; a raw enrollment token is
returned exactly once, at issue time, and only its SHA-256 hash is stored.

Every attempt-scoped mutation additionally validates runner id + attempt id +
current fencing token. A stale fence returns the stable `stale_lease` error code and
writes nothing.

See [The Runner Contract](book/src/developer/runner-contract.md) for the contract behind
this surface, [Installing and Operating Runners](book/src/admin-guide/runners.md) for the
operator-facing view, and the
[Recovery Runbook](book/src/admin-guide/recovery-runbook.md) for what drives a
`needs_operator` transition.

---

## WebSocket Events

`GET /api/projects/{id}/boards/live` — not represented in `docs/openapi.json`, which
covers REST only.

```javascript
const ws = new WebSocket(`ws://localhost:3210/api/projects/${projectId}/boards/live`, ['tack.v1']);
ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);
  switch (boardEvent.type) {
    case 'item_created': case 'item_updated': case 'item_deleted':
    case 'board_config_updated': case 'sprint_updated':
      fetchBoard(); break;   // refetch rather than patch client state
  }
};
```

A browser cannot set `Authorization` on a WebSocket. When the server requires a token, the
upgrade accepts the `tack_session` cookie, or a subprotocol `tack.auth.<base64url token>` offered
beside `tack.v1`. The server selects `tack.v1`, and a client that offers a subprotocol and gets
none back must drop the connection, so always offer it. The `Origin` header, when present, must
be in `TACK_ALLOWED_ORIGINS`, or loopback when the server is bound to loopback.

Every message is a JSON object tagged by `type`:

| `type` | Other fields |
|---|---|
| `item_created` | `project_id`, `item_id`, `status` |
| `item_updated` | `project_id`, `item_id`, `old_status` (nullable), `new_status` |
| `item_deleted` | `project_id`, `item_id` |
| `board_config_updated` | `project_id` |
| `sprint_updated` | `project_id`, `sprint_id` |

The channel is a single Tokio broadcast (capacity 100) shared by every project; each connection
filters by `project_id` server-side. Sends are fire-and-forget: a disconnected client misses events
with no replay, and reloads current state on reconnect.

---

## Notable behaviors not visible in the schema

Business rules a JSON Schema cannot express — kept here rather than in a
per-endpoint listing precisely so they do not get duplicated by one.

**Items & workflow**
- `POST /api/projects/{id}/items` — `status` is not a settable field; a new item
  always starts in the workflow's initial state. Move it with `PATCH /api/items/{id}`.
- `PATCH /api/items/{id}` — a status change runs workflow validation (including WIP
  limits) before it is written, and auto-sets `started_at`/`completed_at` on
  entering an in-progress/done category.
- `POST /api/items/{id}/dependencies` — rejects an edge that would create a cycle.

**Briefs**
- `PUT /api/items/{id}/brief` creates or replaces the item's brief (acceptance criteria,
  constraints, definition of done, risk) after validating it; a brief that breaks the rules
  is a `400`. `GET` and `DELETE` return `404` for an item that has none.
- A brief is part of a project's JSON/YAML export and import, and an imported one is
  validated by the same rules as a `PUT`.

**Attempts: merge-readiness, metrics**
- `GET .../attempts/{n}/mrp` returns the attempt's newest merge-readiness pack, or `404`
  when the verifier left none. `POST .../mrp/viewed` marks that a person opened it, and
  `POST .../mrp/review` records the verdict with a required reason: the first review is the
  record and a second is a `409`. Accepting moves the item only under the
  `done_on_mrp_accepted` status policy.
- `status_map_policy_id` on `POST /api/executions` is `done_on_success` or
  `done_on_mrp_accepted`; any other value is rejected. Without it the item's status is
  never touched.
- `GET /api/projects/{id}/metrics/factory` accepts an optional RFC 3339 `since`; every
  ratio carries its numerator and denominator, and a value that cannot be measured is
  `null` with a `null_reason`, never `0`.

**Sprints**
- `POST /api/projects/{id}/sprints` — always starts in `planning`; `status` is not a
  `CreateSprint` field.
- `PATCH /api/sprints/{id}` — full replacement of `name`/`goal`/`start_date`/
  `end_date`: omitting a field clears it rather than leaving it alone.
- `PATCH /api/sprints/{id}/status` — the only route that moves status; fires the
  sprint's lifecycle webhooks.

**Roles, profiles, auto-start and the ceiling**
- `PATCH /api/roles/{id}` may set `agent_profile_id`. `POST /api/executions` may omit
  `agent_profile_id`, the agent and the selector: the server then chooses the profile
  (the task's saved settings, its role, the project default, the built-in Implementer),
  records which in `metadata.profile_source`, and takes the profile's fleet, else the only
  active runner. `GET /api/items/{id}/agent-context` answers `profile_id` and `profile_source`.
- `GET /api/projects/{id}/autonomy` carries read-only `ceiling` and `teams` (each team a
  role uses, and `caps_at` and `fix` for one that needs a person during a run). A project's
  auto-start is set with `PATCH /api/projects/{id}` (`auto_start_enabled`,
  `auto_start_status`, `auto_start_max_open`); it is switched off when the project drops to L3.
- `GET /api/settings/rate-card` and `PUT` hold the prices usage is priced with.

**Templates, custom fields and the project file**
- Built-in templates (`is_builtin: true`) cannot be deleted.
- `POST /api/projects/from-template/{id}` applies the template's workflow,
  vocabulary, custom field definitions, default boards, roles, profiles and automation
  to the new project, through the same code as a project-file import.
- `POST /api/projects/{id}/save-as-template` snapshots the project's *current*
  configuration, not the template it was created from.
- `POST /api/projects/outline` and `POST /api/projects/{id}/outline` read the whole
  project file (`dry_run=true` checks it and writes nothing). A refusal is a `400` listing
  every problem. A section that fails to write is a `500` whose message names it. See
  [The Project File Format](book/src/developer/project-file.md).
- `GET /api/projects/{id}/export?scope=bundle&format=yaml|json` writes the project back as
  that file; `scope=snapshot` (the default) is the database dump `POST /api/projects/import` reads.
- Deleting a custom field definition cascades to every item's stored value for it.
- A custom field value's shape depends on the field's `field_type` (`text`,
  `long_text`, `number`, `date`, `boolean`, `select`, `multi_select`, `url`,
  `email`) — `select`/`multi_select` values must match the field's own `options`.

**Boards**
- Creating a board with `is_default: true` unmarks every other board on the project.
- The last board, or a default board with no other board to promote first, cannot
  be deleted.
- `GET /api/boards/{id}/view` groups items live at read time by the board's
  `grouping` — there is no materialized, cached column assignment to go stale.

**Search**
- Both search routes pass `q` to SQLite FTS5 `MATCH` over titles, descriptions and tags, best
  match first. A project search returns at most 50 items; the global search, at most 100.

**Executions**
- `GET /api/executions/{id}` — the request's `state` mirrors its live attempt's:
  `leased` on claim, then `preparing` and `running` as the runner reports them, and
  `waiting_decision` for exactly as long as the attempt has a pending decision — the
  runner opening one parks both, and the last answer or expiry returns both to
  `running`. A request in any of these states is never claimed again.

**Backup & restore**
- A local or remote restore is **staged**, not applied immediately: the swap
  happens atomically on the next server start, and the previous DB/attachments are
  kept as `<path>.bak`.
- A restore is rejected (`409`) when the snapshot's `migration_version` is newer
  than the running binary's — upgrade Tack first.
- `PUT /api/settings/backup` — leaving `secret_key` blank keeps the currently
  stored secret (so a masked UI field can be submitted untouched); any other blank
  string field clears that override back to its environment default.
- Remote backup bundles carry a generation counter and install id: an upload that
  would overwrite newer work from a *different* install is rejected unless `force`
  is set, and a restore that would clobber newer local work needs confirmation. See
  [`docs/DEPLOYMENT-GUIDE.md`](DEPLOYMENT-GUIDE.md#backups) for the full model.

**Import**
- GitHub import skips pull requests; an unauthenticated call is limited to 60
  requests/hour, a token raises that to 5,000/hour.
- Linear import is cursor-paginated at 50 issues/page; `project_id` wins over
  `team_id` when both narrow the fetch.
- `POST /api/projects/import` (JSON or YAML, dispatched on `Content-Type`) is a
  full import, not a placeholder: sprints then items (two passes — create, then
  wire `parent_id`) then dependencies, preserving each item's `source` marker and
  restoring the source project's workflow and vocabulary. A failure at any step
  deletes the partially-created project rather than leaving it half-imported.

---

## Worked examples

### Import from GitHub

```http
POST /api/projects/{id}/import-github
Content-Type: application/json

{ "repo": "owner/repo", "token": null, "import_closed": false, "label_filter": [] }
```

**Response `200`:** `{ "created": N, "skipped": N, "rate_limit_remaining": N }`

### Import from Linear

```http
POST /api/projects/{id}/import-linear
Content-Type: application/json

{ "api_key": "lin_api_...", "team_id": null, "project_id": null, "import_completed": false, "label_filter": [] }
```

**Response `200`:** `{ "created": N, "skipped": N }`

### `POST /api/executions` — start a run on an item

Only `item_id`, `idempotency_key`, `permission_policy`, `budgets`, `environment` and `metadata` are
required. Leave out the profile, the agent and where to run it and the server chooses them (see
the profile rules above). The user-facing walkthrough is [Running Agents on
Tasks](book/src/user-guide/running-agents.md).

```http
POST /api/executions
Content-Type: application/json

{
  "item_id": "1bb00bc7-82d0-43fc-8a48-f50e1bf75b7c",
  "idempotency_key": "demo-001",
  "permission_policy": { "tools": [], "network": false, "approvals": "ask" },
  "budgets": {},
  "environment": {},
  "metadata": {}
}
```

**Response:** `200 OK`
```json
{
  "protocol_version": 1,
  "request_id": "exec_1ecec9359d33039dedfd637df7fa4af87dca282c23437509516332e0adf243aa",
  "state": "queued",
  "replayed": false
}
```

Repeating the call with the same `idempotency_key` answers the same request with `"replayed": true`.
Poll `GET /api/executions/{request_id}/attempts` to follow it. docket is reached the same way as
every other agent, as `requested_harness_kind: "docket"` on a runner-v1 execution request, never
through a Tack-side dispatch route. It takes the attempt as one task in a team, over its HTTP API
version 6 (see [Runs, Teams and Automation](book/src/developer/runs-and-teams.md#the-docket-adapter)).

---

## Error envelope

Every failing response, on `/api` and on `/api/runner/v1` alike, has one shape:

```json
{
  "error": {
    "status": 404,
    "code": "not_found",
    "message": "Item not found",
    "retryable": false,
    "request_id": "req_6f1c0d2a9b7e4c3f8a5d1e0b2c4a6f80",
    "details": {}
  }
}
```

A client branches on `code` (the `StableErrorCode` enum in the spec), never on `message`, which
is plain words for a person. `details.field` names the request field, path parameter or query
parameter that failed, when one did. `request_id` is the `x-request-id` the client sent when it
was short and safe, else one the server made; the response repeats it in the same header. The
generated reference lists the status codes each endpoint can return; the common ones are `400`
(validation), `401` (missing or invalid token), `404` (not found) and `409` (workflow, WIP or
dependency-cycle conflict).

---

## Rate limiting, CORS, versioning

- **Rate limiting:** not implemented server-side. Add it at the reverse proxy
  (Caddy, nginx) for a public deployment.
- **CORS:** controlled by `TACK_ALLOWED_ORIGINS`, a comma-separated allow-list
  (the default list is in [CONFIG.md](CONFIG.md)). The bundled SPA is served
  same-origin and needs no CORS config; this only matters for a *separate-origin*
  browser client. There is no `TACK_CORS_ORIGIN` variable.
- **Versioning:** none yet — the API is implicitly v1. A breaking change would use
  an `/api/v2` prefix.
