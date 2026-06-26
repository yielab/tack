# GitHub Sync (v1 — push-only status)

_Status: shipping in slices · Date: 2026-06-25 · Phase 21_

Tack's one-way GitHub _import_ copies issues in once. This adds a **living link** so
status changes flow back out to GitHub. v1 is deliberately the smallest safe slice.

## v1 scope (decided)

- **Direction: push-only** (Tack → GitHub). No inbound webhook/poll yet.
- **Data: open/closed state only.** When a linked item moves into a Done-category
  status, its GitHub issue is **closed**; when it moves back out of Done, the issue
  is **reopened**. No comment/label/title mirroring in v1.
- **Best-effort.** The push is fire-and-forget (like outbound webhooks): failures are
  logged, never block or fail the item update.
- **Conflict policy:** last-write-wins, Tack-initiated. v1 only pushes; it never
  overwrites Tack state from GitHub.

## How the link is formed

- Importing a repo (`POST /api/projects/:id/import-github`) now records a row in the
  new `github_links` table for every created item: `(item_id, repo, issue_number)`.
- So any item that came from a GitHub import is automatically push-linked. (Manually
  linking arbitrary items is a future enhancement.)

## Configuration

Push is **off unless a token is configured** — there is no way for Tack to write to
GitHub without one, so the feature is opt-in and inert by default.

| Env var | Default | Purpose |
| --- | --- | --- |
| `TACK_GITHUB_TOKEN` | _(none)_ | PAT with `repo` scope; enables push. Never logged. |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | Override for GitHub Enterprise / testing against a mock. |

## Flow

1. `update_item` changes an item's status.
2. If a `TACK_GITHUB_TOKEN` is set **and** the item has a `github_links` row **and**
   the change crosses the Done boundary (was-done ≠ now-done), Tack spawns a
   best-effort `PATCH {base}/repos/{repo}/issues/{n}` with `{"state": "closed"|"open"}`.
3. A title edit or a same-category status move triggers **no** GitHub call.

## Explicitly out of scope for v1 (future slices)

- Inbound sync (GitHub → Tack) via webhook or polling.
- Mirroring comments, labels, assignees, or title.
- Per-project tokens / a UI to link individual items.
- Conflict resolution beyond "Tack pushes, last write wins."
