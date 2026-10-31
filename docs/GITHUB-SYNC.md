# GitHub sync

A **link** ties a Tack item to a GitHub issue. Linked items keep their state and comments in step
with the issue in both directions, and an accepted agent run opens a pull request that the same
poll then follows. The one-way GitHub _import_ (`POST /api/projects/{id}/import-github`) copies
issues in once; the link is what keeps them in step afterwards.

## What flows where

| Direction | Trigger | What happens |
| --- | --- | --- |
| Tack → GitHub | A linked item moves into a Done-category status | The issue is **closed**. |
| Tack → GitHub | A linked item moves out of Done | The issue is **reopened**. |
| Tack → GitHub | A comment is posted on a linked item | It is posted onto the issue; GitHub's comment id is stored on the Tack comment. |
| GitHub → Tack | The poll sees a closed issue whose item is not Done | The item moves to the project workflow's first Done-category status. |
| GitHub → Tack | The poll sees an open issue whose item is Done | The item moves to the workflow's first Todo-category status. |
| GitHub → Tack | The poll sees a comment whose id is not stored on a Tack comment | A Tack comment is created, starting `@<login> on GitHub:`, a blank line, then the text. |
| Tack → GitHub | An attempt is accepted | A pull request is opened (see below). |

Only the Done boundary matters: a title edit, or a move between two statuses of the same category,
calls GitHub not at all, and an item already in the right category is left where it is. Titles,
labels, assignees and issue bodies are never mirrored. System comments (status changes, edits) are
never pushed, and a comment mirrored in from GitHub is never pushed back, because the stored GitHub
id marks it as already synced.

- **No webhook receiver.** Tack listens on `127.0.0.1` by default and the desktop app never exposes
  it, so GitHub could not deliver to it. A poll with the same token the push uses reaches every
  install.
- **Best-effort.** A push is fire-and-forget, like outbound webhooks: a failure is logged and never
  blocks or fails the item update or the comment. The poll logs and skips a repository that errors
  rather than failing the whole pass.
- **Last write wins.** The poll moves an item through the project's ordinary workflow, with the same
  validation `PATCH /api/items/{id}` uses, and creates comments directly, so an inbound move or
  comment never triggers an outbound push. It only ever creates comments, never edits or deletes one,
  in either direction.

## The link

- Importing a repository records a link `(item, owner/name, issue number)` for every item it creates,
  so an imported item is linked from the start.
- Any item can be linked by hand, from the item's side panel or the API:
  `PUT /api/items/{id}/github-link` with `{"repo": "owner/name", "issue_number": 42}` creates or
  replaces the link; `GET` returns it (404 when there is none); `DELETE` removes it and is not an error
  when there was none.

## Configuration

Nothing is sent to GitHub without a token, so sync is inert by default. The poll also needs an interval.

| Variable | Default | Purpose |
| --- | --- | --- |
| `TACK_GITHUB_TOKEN` | _(none)_ | Personal access token with `repo` scope. Enables the push, and lets the poll start. Never logged. |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | The API root, for GitHub Enterprise or a mock. |
| `TACK_GITHUB_POLL_SECONDS` | `0` | Poll interval in seconds. `0` is off. |

A project can carry its own token as a reference rather than a value:
`PATCH /api/projects/{id}` with `{"github_token_ref": "store:<name>"}`, where `<name>` is a secret
stored with `tack runner secret set <name>`. Sending an empty string clears the reference. For a
project's repository, its own reference is used first, and `TACK_GITHUB_TOKEN` only when the project
has none or the reference does not resolve. The token value is never returned by any route. The poll
still starts only when `TACK_GITHUB_TOKEN` is set: a project's reference chooses which token an
already-running poll uses for that repository and cannot start the poll.

## The poll

Every `TACK_GITHUB_POLL_SECONDS` while a token is set, for each repository with at least one linked
item:

1. `GET {base}/repos/{repo}/issues?state=all&since=<latest synced_at across its links>`, sending the
   previously seen `ETag` as `If-None-Match`. The `ETag` is kept in memory for the life of the process.
2. On a `304` no issue is read. Otherwise each returned issue that matches a linked item's number is
   compared with the item as in the table above, and its `updated_at` is stored as that link's
   `synced_at`, which narrows the next poll's `since`.
3. For each linked issue the list returned (a new comment bumps the issue's `updated_at`, so an issue
   absent from the list has nothing new to read), `GET {base}/repos/{repo}/issues/{n}/comments`, with
   `since` set to the link's `synced_at` once it has one, and every comment not yet stored becomes a
   Tack comment.
4. The same list also contains pull requests. The poll records when one that Tack opened is merged or
   closed. A merged one is marked **reverted** when a later pull request whose title starts with
   `Revert` cites its number (`#<n>`) in its body.
5. For each pull request merged in the last 14 days, the poll learns its merge commit and marks the
   pull request reverted when a commit on the default branch says `This reverts commit <merge commit>`.
   For one merged in the last 24 hours it also records when CI on the merge commit first read red.
   Step 5 runs on a `304` as well.

A project's automatic merge runs on the same tick, right after the poll, so it needs the token and the
interval too. The Merge button reads the same GitHub signals; see
[Merging from Tack](book/src/user-guide/merging.md).

## Pull requests

When an attempt is accepted, in the attempt review or the merge-readiness pack review, or by a
project's automatic merge, and the runner pushed its branch, Tack opens a pull request from that branch
to the repository's default branch, with the same token as the push (the project's own, else
`TACK_GITHUB_TOKEN`). The item must be linked to a GitHub issue; an item with no link, or no token,
gets no pull request, and the review still completes. An attempt that already has a pull request
returns it without calling GitHub.

The title carries only the attempt and issue numbers, never item or agent text. The body is the
attempt's merge-readiness pack, or a plain note when there is none. The pull request's number, link
and state (open, merged, closed or reverted) show on the attempt.

## Not supported

- A GitHub webhook receiver.
- Mirroring labels, assignees or titles.
- Editing or deleting a comment on the other side.
