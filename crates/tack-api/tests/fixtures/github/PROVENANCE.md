written by hand, 2026-10-13
Trimmed to the fields Tack reads, in the shapes of GitHub's REST API (version 2022-11-28) documentation:
- `GET /repos/{owner}/{repo}/commits/{ref}/check-runs` → check-runs-*.json (GitHub Actions reports here, not in the combined status)
- `GET /repos/{owner}/{repo}/commits/{ref}/status` → status-*.json (the combined status; `pending` with zero statuses is what a repository with only Actions returns)
- `PUT /repos/{owner}/{repo}/pulls/{n}/merge` → merge-200 (merged), merge-403 (token lacks Contents: write), merge-405 (branch protection), merge-409 (`sha` no longer the head), merge-422 (not mergeable)
- `GET /repos/{owner}/{repo}/commits?sha=<default branch>` → commits-default-branch-with-revert.json (a `git revert` of merge commit e5bd3914e2e596debea16f433f57875b5b90bcd6)
Head commit in every fixture: 6dcb09b5b57875f334f61aebed695e2e4193db5e. Merge commit: e5bd3914e2e596debea16f433f57875b5b90bcd6.
- `GET /repos/{owner}/{repo}/pulls/{n}` → pull-merged.json (`merge_commit_sha` of a merged pull request; added by task G)
