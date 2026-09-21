# GitHub Sync (status and comments, both directions)

_Status: shipping in slices · Date: 2026-06-25 · Phase 21 · inbound state and comments
both ways added 2026-09-20_

Tack's one-way GitHub _import_ copies issues in once. This adds a **living link** so
status changes and comments flow both ways: out to GitHub on an item update or a new
comment, and in from GitHub on a poll.

## Scope (decided)

- **Direction: push (Tack → GitHub) and poll (GitHub → Tack).** No webhook receiver —
  Tack listens on `127.0.0.1` by default and the desktop app never exposes it, so
  GitHub cannot deliver to it; a poll with the same token push already uses reaches
  every install. A webhook route is refused until an install reachable from the
  internet asks.
- **Data: open/closed state, and comments, either direction.** When a linked item
  moves into a Done-category status, its GitHub issue is **closed**; when it moves
  back out of Done, the issue is **reopened** — and the same mapping in reverse: a
  closed issue moves its item to the project workflow's first Done-category status, a
  reopened one to the first Todo-category status. A comment typed on a linked item is
  posted onto its issue; a comment posted on the issue is mirrored in as a Tack
  comment on the poll. No label/title mirroring yet.
- **Best-effort.** The push is fire-and-forget (like outbound webhooks): failures are
  logged, never block or fail the item update or the comment creation. The poll logs
  and skips a repo that errors rather than failing the whole poll.
- **Conflict policy:** last-write-wins, Tack-initiated for push. The poll moves an
  item through the project's ordinary workflow (the same validation `PATCH
  /items/{id}` uses) and creates inbound comments straight through the db repo, so an
  inbound move or comment never triggers an outbound push — the two directions never
  share a code path.

## How the link is formed

- Importing a repo (`POST /api/projects/:id/import-github`) now records a row in the
  new `github_links` table for every created item: `(item_id, repo, issue_number)`.
- So any item that came from a GitHub import is automatically push-linked.
- Any item can also be linked by hand, from the item's side panel or directly:
  `PUT /api/items/{id}/github-link` with `{"repo": "owner/name", "issue_number": 42}`
  creates or replaces the link; `DELETE /api/items/{id}/github-link` removes it
  (not an error when there was none); `GET /api/items/{id}/github-link` returns the
  current link, or a 404 when the item isn't linked.

## Configuration

Push is **off unless a token is configured** — there is no way for Tack to write to
GitHub without one, so the feature is opt-in and inert by default. The poll is
**additionally off unless an interval is configured** — both a token and a nonzero
interval are required before it starts.

| Env var | Default | Purpose |
| --- | --- | --- |
| `TACK_GITHUB_TOKEN` | _(none)_ | PAT with `repo` scope; enables push and poll. Never logged. |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | Override for GitHub Enterprise / testing against a mock. |
| `TACK_GITHUB_POLL_SECONDS` | `0` | Inbound poll interval in seconds. `0` is off. |

A project can also carry its own token, referenced rather than stored directly:
`PATCH /api/projects/{id}` with `{"github_token_ref": "store:<name>"}`, where
`<name>` is a secret already stored through
`PUT /api/local-runner/secrets/{name}`. A push or poll request for that project's
repo resolves the project's own reference first and falls back to
`TACK_GITHUB_TOKEN` only when the project has none, or its reference doesn't
resolve. The token value itself is never returned by any route — only the
reference. The poll still only *starts* when `TACK_GITHUB_TOKEN` is set; a
project's own reference can only choose which token an already-running poll
uses for that project's repo, never start the poll on its own.

## Flow

**Push:**

1. `update_item` changes an item's status.
2. If a `TACK_GITHUB_TOKEN` is set **and** the item has a `github_links` row **and**
   the change crosses the Done boundary (was-done ≠ now-done), Tack spawns a
   best-effort `PATCH {base}/repos/{repo}/issues/{n}` with `{"state": "closed"|"open"}`.
3. A title edit or a same-category status move triggers **no** GitHub call.

**Poll**, every `TACK_GITHUB_POLL_SECONDS` while a token is set:

1. For each repo with at least one linked item: `GET
   {base}/repos/{repo}/issues?state=all&since=<max synced_at across its links>`,
   sending back a previously seen `ETag` as `If-None-Match`.
2. A `304` response does no write. Otherwise, each returned issue that matches a
   linked item's number moves that item through the project's ordinary workflow to
   the first Done-category status (closed) or first Todo-category status (open) — an
   item already in that category is left alone. Nothing else on the issue (body,
   labels, assignee) is read.
3. Every processed issue's `updated_at` is recorded as that link's `synced_at`,
   narrowing the next poll's `since`; the `ETag` is kept in memory for the process's
   lifetime, not persisted.
4. For each linked issue the list returned (a new comment bumps the issue's
   `updated_at`, so an issue absent from the list has nothing new to read), `GET
   {base}/repos/{repo}/issues/{n}/comments?since=<that link's synced_at>` (the
   parameter is omitted while the link has never synced). Each
   returned comment whose id isn't already stored on a Tack comment becomes one,
   created straight through the db repo — never the HTTP handler, so it can't itself
   trigger the push below — with a body starting `@<login> on GitHub:` then a blank
   line then the comment text. The comment's id is stored alongside it.

**Comment push:**

1. A user posts a comment on an item (`POST /api/items/{id}/comments`).
2. If a `TACK_GITHUB_TOKEN` is set **and** the item has a `github_links` row, Tack
   spawns a best-effort `POST {base}/repos/{repo}/issues/{n}/comments` with `{"body":
   <the comment's content>}` and stores the id GitHub returns on the comment.
3. A system comment (status changes, edits) is never pushed — only a comment created
   through this endpoint is a candidate. A comment mirrored in from the poll above is
   created through the db repo, not this endpoint, so it is never a candidate either:
   the stored id is what "already mirrored" means, in both directions.

## Explicitly out of scope (future slices)

- A GitHub webhook receiver (decided against — see "Scope" above).
- Mirroring labels, assignees, or title.
- Conflict resolution beyond "Tack pushes, last write wins" for push; the poll moves an
  item only when the issue's state and the item's status category disagree, and only
  ever creates a comment, never edits or deletes one either direction.
