## What this is about

A public repository's headline install command — the one line most strangers will actually
try — has been broken since the day the repository went public, and nothing would have
caught it happening again. This closes both problems: the command is proven to work end to
end, and a check now watches it continuously.

## Where it stands

The one-line installer's logic was always fine; the URL it's fetched from names a branch,
`main`, that has never existed on the remote, so every fetch of it has 404'd since
2026-03-15. Re-checking the docs found the URL is already written correctly everywhere it
appears — the fix is not a text edit, it's making the branch exist. A CI job now resolves
every install URL the docs advertise (the raw install-script URL and the releases-page link)
and fails if any comes back non-2xx; it's proven to actually catch a broken URL, not just
exist decoratively.

What still doesn't work: the branch itself. Nothing was pushed to the remote (out of scope
for this card — see Next step), so `raw.githubusercontent.com/yielab/tack/main/install.sh`
is still a 404 today, exactly as it was before this card. Everything below proves the fix is
correct and ready; it isn't live until someone with push access publishes the branch.

## What is left

**Publishing the `main` branch.** This card created it locally only, per its instructions —
outward-facing actions need explicit approval. Someone with push access needs to decide when
to push it (this handoff's Next step has the command) and then keep it updated at each future
release cut. That's a manual step for now: this card doesn't own `release.yml`, so it
couldn't wire an automatic "merge to main on tag" step even though that's the obvious next
piece of automation. Whoever owns the release workflow next should consider adding it.

**The CI check only goes green once `main` exists remotely.** Right now, if someone pushed
this branch to GitHub and opened a PR, the new `Verify install URLs` job would fail — because
the real `main` branch it's checking still doesn't exist upstream. That's correct behavior,
not a bug in the check; it'll flip green automatically the moment `main` is pushed, with no
further action needed here.

## Technical detail

**Where the code lives** — two new files, nothing else touched:
- `scripts/verify-install-urls.sh` — extracts install URLs from a fixed list of doc files
  (`README.md`, `docs/DEPLOYMENT-GUIDE.md`, `docs/book/src/user-guide/quick-start.md`,
  `docs/book/src/roadmap.md`, `install.sh`) and curls each, failing on any non-2xx.
- `.github/workflows/verify-install-urls.yml` — runs that script on push to `main`/`develop`
  touching those paths, on PRs touching them, on a daily schedule (catches rot with no doc
  change, e.g. the branch getting deleted later), and on manual dispatch.

`install.sh`, `README.md`'s install section, and every file under `docs/` were read but not
edited — see "How the main-vs-develop decision was made" below for why.

**The main-vs-develop decision** — chose (a), matching the board's recommendation: create
`main` as the stable branch the public install path resolves against, rather than repointing
every URL at `develop`. Reasoning: a stranger's `curl | sh` shouldn't track whatever's
mid-flight on the development tip, and (b) would need re-doing the same way every time this
project's branch conventions come up again (contributor docs, badges, future tooling that
assumes a `main`/`develop` split).

One discovery changed how "tracking released state" had to be implemented: `install.sh`
itself doesn't exist at the last release tag (`v0.1.0-beta.6`, cut 2026-06-22) — it was added
to `develop` afterward. So `main` cannot simply equal the latest tag today; doing that would
make the raw URL 404 for a *different* reason (file not found instead of branch not found).
The local `main` branch created here therefore points at this card's own commit (which is
`develop`'s tip plus only this card's two new files) — the first point in history where the
install mechanism is actually complete and correct. Going forward, `main` should only advance
when a release is cut (merge `develop` → `main`, tag on `main`), not track `develop`
continuously; that convention is stated here for whoever pushes it, not enforced by any
code, since it needs a `release.yml` change this card doesn't own.

**Why no doc edits were needed** — re-ran the card's own repo-wide grep after making the
decision:
```
grep -rn 'githubusercontent.com/yielab/tack' . --include='*.md' --include='*.sh'
```
All five occurrences outside `TODO.md` (`README.md:166`, `docs/DEPLOYMENT-GUIDE.md:50`,
`docs/book/src/user-guide/quick-start.md:14`, `docs/book/src/roadmap.md:3093`,
`install.sh:4`) already name `main`. No occurrence names `develop` or any other branch. The
bug was purely "the branch doesn't exist," never "the docs name the wrong branch," so
choosing (a) required zero text changes. `TODO.md`'s two occurrences (lines 101, 278) were
left untouched — it's the board file, not README/docs, and it's a historical audit record of
the bug, not an instruction a stranger follows.

**How the CI check was proven load-bearing** — pushing `main` to trigger a real Actions run
is exactly the outward-facing action this card isn't allowed to take, so the check was
exercised directly:
```
cd <worktree> && ./scripts/verify-install-urls.sh
```
Real run, today, against the actual repo (unmodified, still names `main`):
```
Checking 2 install URL(s):
  https://github.com/yielab/tack/releases
  https://raw.githubusercontent.com/yielab/tack/main/install.sh

OK   [200] https://github.com/yielab/tack/releases
FAIL [404] https://raw.githubusercontent.com/yielab/tack/main/install.sh
EXIT: 1
```
That's the correct, expected result — it's the same bug this card fixes, still live because
`main` isn't pushed. To prove the check itself is load-bearing rather than always-red for the
wrong reason, a scratch copy of the same files was made in `/tmp` with the raw URL swapped to
`develop` (real, resolves today) as a stand-in for what `main` will be once published, then
put through fail → break → revert:
```
RUN 1 (scratch copy, pointed at develop): OK/OK  → EXIT 0   (baseline pass)
RUN 2 (one URL rewritten to install-does-not-exist.sh): FAIL 404 → EXIT 1   (goes red)
RUN 3 (reverted): OK/OK → EXIT 0   (green again)
```
`shellcheck scripts/verify-install-urls.sh` — clean, no warnings.

**Test results — clean-container install proof.** Two Docker runs, `debian:bookworm-slim`,
only `curl`/`ca-certificates` installed, no repository checkout:

*Run A — mechanism proof (network-isolated, no real GitHub raw-content dependency).* A
second container served `git show main:install.sh`'s exact bytes at
`http://v-a1-mock-server/install.sh` on a private Docker network (standing in for the real
domain, which can't resolve to unpushed content). The client fetched and piped it to `sh`
exactly as the README's command does past the domain name:
```
=== fetching install.sh from the mock 'main' server and running it ===
Looking up the newest tack release for linux-x86_64…
Downloading tack-v0.1.0-beta.6-linux-x86_64.tar.gz …
Installed tack to /root/.local/bin/tack

=== tack installed, checking binary ===
tack 0.1.0-beta.6

=== starting tack and confirming it serves the UI ===
HTTP 200 from http://127.0.0.1:3210/
--- first bytes of the served UI ---
<!doctype html>
<html lang="en">
  <head>
    ...
    <meta name="description" content="Tack — lightweight, versatile project managemen
=== tack.log ===
... Running database migrations... [18 migrations applied] ...
... Server listening addr=127.0.0.1:3210
```
The release asset downloaded was the real `v0.1.0-beta.6` binary, fetched from the real
GitHub Releases API — only the raw-content fetch of `install.sh` itself was mocked, because
that's the one step that literally cannot succeed against the real domain until `main` is
pushed.

*Run B — honesty check against the real domain, no mocking, no repo checkout.*
```
=== literal exact README command against the real repo, TODAY ===
curl: (22) The requested URL returned error: 404
(exit code printed as 0 — that's sh's exit on empty stdin, not curl's; the
 "curl: (22) ... 404" line above is the real signal)

=== same command, main swapped for develop (real, today) ===
Looking up the newest tack release for linux-x86_64…
Downloading tack-v0.1.0-beta.6-linux-x86_64.tar.gz …
Installed tack to /root/.local/bin/tack
tack 0.1.0-beta.6
```
Together, Run A and Run B prove every part of the install path except "does `main` exist" —
the script logic, the GitHub Releases API lookup, the real `v0.1.0-beta.6` asset, and the
resulting binary starting and serving the UI. The only missing piece is publication.

**`git ls-remote --heads origin` — before this card:**
```
c295290a9667565f1c5704c088e65f8362b03ebb	refs/heads/agent/iii-f6-integration
7f66ccae37c222662d329817e0587565323774a0	refs/heads/dependabot/cargo/cargo-major-d3cb2910cf
ec337a11ec96aaa7c72c977932ce784f60660672	refs/heads/dependabot/github_actions/actions-major-668df130e3
a517223daec74c96a9a477ed235bbcb5af358476	refs/heads/dependabot/github_actions/actions-minor-patch-15ce6a1b36
f4a6eaa045543cc812e61c5a87e05d994f9138b0	refs/heads/dependabot/npm_and_yarn/frontend/npm-major-a40c5d7ac7
6ab5fb5b9ea725a35683af56906957558f01b491	refs/heads/develop
158980ef9b17c1de0e2e7e69d70f4852c1308c98	refs/heads/plan/harness-agnostic-agent-fleet
```
No `main`, confirmed again by `gh repo view yielab/tack --json defaultBranchRef` →
`"develop"`.

**Expected after publishing** — the same list plus one new line,
`<sha>	refs/heads/main`. The `<sha>` should NOT be this card's local `main` (which sits on
its own feature branch, not on `develop`) — it should be cut fresh from `develop`'s tip once
this card is merged in through the normal integration process, since this card's only changes
are two new, non-conflicting files.

**Not checked** — macOS/Windows install paths (the container proof is Linux x86_64 only,
matching what this environment can run; the release workflow's other platform matrix entries
were not exercised). No real GitHub Actions run was triggered (would require pushing).
`release.yml` was read but not modified — out of this card's ownership, and the "keep `main`
in sync at each release" automation it would need is called out above as a follow-up, not
built. `TODO.md`'s own two mentions of the broken URL were left as-is (board file, not
README/docs, and a historical record of the bug rather than an instruction).

## Next step

Review the decision above, and if it holds, publish the branch from a trusted checkout:
`git fetch origin develop && git checkout -b main origin/develop && git push -u origin main`
(cut fresh from `develop`'s tip after this card lands there, not from this card's local demo
branch).

Branch: `agent/v-a1-install-path`, committed (one commit, `a8715a5`). A local-only `main`
branch also exists in this worktree at the same commit, created solely to demonstrate the
mechanism above — per instructions it was never pushed and should not be treated as the
branch to publish.
