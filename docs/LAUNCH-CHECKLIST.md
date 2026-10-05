# Launch checklist

What has to be true before tagging a release, and what to do after. Each item names
the one command that checks it. Nothing here is posted, tagged, or labeled
automatically — every step under "Publish list" is a human action.

## Before tagging

| # | What must be true | Check it with |
| --- | --- | --- |
| 1 | CI is green on `develop`, including E2E — no job cancelled or timed out | `gh api "repos/yielab/tack/actions/workflows/ci.yml/runs?event=push&branch=develop&per_page=1" --jq '.workflow_runs[0].conclusion'` (expect `success`) |
| 2 | Local `develop` matches `origin/develop` — nothing sitting unpushed | `git fetch origin && git rev-parse origin/develop develop` (both SHAs equal) |
| 3 | `main` is caught up to `develop` — `main` is what the install one-liner and the GitHub landing page actually serve | `git rev-list --count origin/main..origin/develop` (0 = current; fast-forward `main` before tagging if not) |
| 4 | `install.sh` picks the `tack-*` archive, never the `tack-runner-*` one (both match the same platform suffix), and verifies what it downloaded | `./scripts/verify-install-urls.sh` — runs the installer for real against the published release, asserts a working `tack` lands, asserts the `SHA256SUMS` check ran, and asserts `packaging/` carries the published digests |
| 5 | The workspace version is the version you're about to tag — `release.yml` refuses a tag that doesn't match `Cargo.toml` | `grep '^version' Cargo.toml` |
| 6 | The local gate and the end-to-end smoke are green | `.githooks/pre-push && cargo nextest run --workspace && ./scripts/smoke.sh` (fake mode: shim harness binaries, no model call) |
| 7 | The live GitHub repository description matches the current harness list (four: Claude Code, Codex, docket, opencode) | `gh repo view yielab/tack --json description --jq .description` — currently still names only two; update it before or alongside tagging |
| 8 | The recovery demo asset is a valid, playable file | `file docs/screenshots/recovery-demo.gif` |
| 9 | Docs site is reachable | `curl -sI https://yielab.github.io/tack/` (expect `HTTP/2 200`) |

**Not verified from this machine:** the macOS and Windows install paths. Only
`linux-x86_64` has been installed and run end to end.

## Publish list — everything a human does, in order

1. **Tag the release.** **Done 2026-09-21 — `v0.1.0-beta.9`**, the first tag with the
   desktop bundles on the releases page. `main` was fast-forwarded to the same commit
   first, since that is what the install one-liner and the landing page serve. A
   `0.1.0-beta.8` version was bumped on 2026-09-07 but never tagged, so its changes
   shipped here. For the next one:
   ```bash
   git push origin develop:main && git tag vX.Y.Z && git push origin vX.Y.Z
   ```
   `release.yml` builds and publishes the archives, the desktop bundles and the SBOMs,
   and refuses the tag outright if it doesn't match `Cargo.toml`.

2. **Write the release notes on GitHub.** The desktop app installers — `.AppImage`/`.deb`
   (Linux), `.dmg` (macOS), `.msi` (Windows) — are the first links in the release
   description, since the desktop app is the primary way to use Tack. The binary
   installers (`install.sh`, `install.ps1`) and Homebrew/Scoop links follow.
3. **Open the seven `good first issue` GitHub issues** from the drafts in
   `docs/launch/good-first-issues/`, applying the existing `good first issue` label
   (already on the repo — `gh label list --repo yielab/tack --search "good first issue"`).
4. **Post the four drafts** in `docs/launch/posts/` to their venues, in any order —
   nothing about them is time-sensitive relative to each other, but all four assume
   the tag above is already live and link to it.
5. **Post the two Discussions topics** in `docs/launch/discussions-seed.md`, into the
   `Q&A` and `Ideas` categories (both already exist on the repo — no category needs
   creating).
6. **Re-point the packaging recipes at the new release**, once its artifacts exist —
   `packaging/{nix,homebrew,aur,scoop}` name a tag and carry its digests, and nothing
   else updates them:
   ```bash
   scripts/sync-packaging.sh v0.1.0-beta.9   # then commit the diff
   ```
   Until this runs they point at the previous release. A wrong digest is not a soft
   failure: Homebrew and `makepkg` abort with a hash mismatch, which reads to a
   stranger as a tampered download. The release workflow's own `homebrew-tap` job
   also runs this script and opens a pull request on `yielab/homebrew-tap` with
   `packaging/homebrew/tack.rb` (signed commit via the API, auto-merge once the
   tap's checks pass) — but only when the repository secret `HOMEBREW_TAP_TOKEN`
   is set; without it, the job just prints the manual steps. Either way, this
   repo's own `packaging/` copy still needs the commit above. If the tap's checks
   fail, the pull request stays open with the failing job attached: fix in this
   repo, re-run `sync-packaging.sh`, and push the same branch.
