## What this is about

Since the last public release, this project grew an entire agent-execution runner fleet
that nobody could download — the release page still only offered June's build. This closes
that gap: the version is bumped, the release notes are rewritten to say plainly what's
proven and what isn't, and a tag is cut and sitting ready to publish.

## Where it stands

The workspace version is `0.1.0-beta.7` and a matching annotated tag exists locally. The
release workflow's packaging logic — build both binaries, stage both archives with their
docs, generate `SHA256SUMS`, generate both SBOMs — was exercised for real on this machine
and produces exactly what the acceptance criteria describe: a `tack-*` and a
`tack-runner-*` archive, checksummed and verifiable. `CHANGELOG.md`'s `[Unreleased]`
section (which had never shipped anything, going back to before the runner fleet existed)
is now a dated `0.1.0-beta.7` section that states which of the three coding-agent harnesses
are actually proven to work end to end, names the one that isn't and why, and says clearly
that the install one-liner still doesn't work and what to do until it does.

What still doesn't work is exactly what the two prior cards already found and left
unfixed, because fixing it wasn't in scope here: `codex` has never completed a live attempt
(an account limitation on this machine, not a code defect — V-A2), and the install command
still 404s until someone with push access publishes the `main` branch V-A1 prepared. Both
are stated in the new release notes rather than glossed over.

What's new to this card's own scope: nothing was pushed anywhere. The tag exists only in
this local worktree's git objects. Publishing it — the action that actually triggers
`release.yml` against the real repository — is a deliberate next step for a human with
push access, not something this card was allowed to do.

## What is left

**Publishing.** The tag is cut and correct but sits on a branch (`agent/v-a3-cut-release`)
that itself hasn't been merged into `develop` or pushed anywhere. Someone needs to run this
card's changes through the normal integration process (the same path V-A1 and V-A2 already
went through), then push the tag from wherever it ends up landing. That single push is what
starts the real CI run — the first time this exact packaging logic will execute on GitHub's
own macOS and Windows runners instead of just being read carefully on this machine.

**The three non-Linux platform legs (macOS aarch64, macOS x86_64, Windows msvc) have never
actually been built** — not by this card, and, as far as this card could determine, not by
any prior one either. They were read line by line and reasoned about, not executed;
nothing here found a defect in them, but "no defect found by reading" is a weaker claim
than "built successfully," and whoever publishes this tag will be the first to actually
learn whether all three build clean.

**The exact Linux target in `release.yml` (`x86_64-unknown-linux-musl`, statically linked)
was not itself built here** — see Not checked below for why and what stood in for it.

## Technical detail

**Where the code lives** — no code changed; `release.yml` was read only, never edited,
because its packaging logic was already correct (confirmed by exercising it, not just
reading it — see Test results). Six files changed, all release bookkeeping:
`Cargo.toml` (version bump), `Cargo.lock` (the six workspace-crate version entries that
follow it), `CHANGELOG.md` (Unreleased → dated `0.1.0-beta.7` section, new intro
paragraphs, new Fixed/Added entries for the V-A1/V-A2 work), `docs/openapi.json` and
`frontend/package.json`/`package-lock.json` (see next paragraph).

**Why the OpenAPI spec and the frontend version also changed, unprompted** — bumping
`Cargo.toml` changes the `info.version` field `utoipa` embeds in the generated OpenAPI
spec, which immediately broke `openapi_spec_matches_committed_file` (the spec-drift gate
`cargo test --workspace` runs, and which `release.yml`'s own `test` job would have hit on
the real tag push). Regenerated with `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`, then `npm run gen:api` in `frontend/` — the generated TypeScript came
back byte-identical, so only `docs/openapi.json`'s version line actually moved.
`frontend/package.json`'s version isn't checked by any CI gate, but every prior release
commit back to beta.1 has bumped it alongside `Cargo.toml` (confirmed with `git log -p --
frontend/package.json`), so it and its lockfile were updated the same way for consistency.
`npm install --package-lock-only` was tried first and rejected — it pulled in ~68 lines of
unrelated optional-platform lockfile entries that had drifted since the last real `npm
install`; the lockfile's two version fields were hand-edited instead, then `npm ci` was
re-run clean to confirm the result is still a valid, installable lockfile.

**How the packaging logic was proven, not assumed** — built both release binaries for
`x86_64-unknown-linux-gnu` with `cargo auditable build --release --features embed-spa`
(for `tack-cli`) and plain `cargo auditable build --release` (for `tack-runner`), after
building the frontend first, matching `release.yml`'s exact step order. Then replicated
the workflow's `Package (tar.gz)` step verbatim (same `STAGE`/`RUNNER_STAGE` naming, same
files copied: `LICENSE`, `README.md`, `QUICKSTART.txt` for `tack`; `LICENSE`,
`packaging/systemd/tack-runner.service`, `packaging/systemd/tack-runner.env.example`,
`QUICKSTART.txt` for `tack-runner`) and its `Generate SHA256SUMS` step verbatim. Extracted
both archives fresh (no repository checkout) and ran the binaries: `./tack --version` and
`./tack-runner --help` both worked.

**How the SBOM steps were proven, not assumed** — `cargo cyclonedx --format json --all`
(the exact command in the `sbom` job) produced one `.cdx.json` per workspace crate (6
files, one per crate). `npm sbom --sbom-format cyclonedx` (the exact command in the same
job) produced a valid 11,466-line CycloneDX document. Both were deleted afterward — they're
build output, not something this card should commit.

**Test results**
- `cargo test --workspace` at the final, tagged commit: **1385 passed, 0 failed**, across
  every crate (`grep -c "test result: ok"` = 80 suites, `grep -c "test result: FAILED"` =
  0). This run is what caught the OpenAPI drift in the first place — before the fix,
  exactly one test failed (`openapi_spec_matches_committed_file`); after the fix, the
  identical full run is clean.
- `frontend`: `npm run type-check` clean; `npx vitest run` — **85 test files, 726 tests,
  all passed**.
- Packaging dry run (Linux, `x86_64-unknown-linux-gnu` standing in for the real
  `x86_64-unknown-linux-musl` target — see Not checked): `tack` archive
  `tack-v0.1.0-beta.7-linux-x86_64-DRYRUN-gnu.tar.gz`, 8,362,210 bytes, containing a
  19,274,624-byte binary; `tack-runner` archive
  `tack-runner-v0.1.0-beta.7-linux-x86_64-DRYRUN-gnu.tar.gz`, 2,175,369 bytes, containing a
  4,739,048-byte binary. Both sizes are from an unstripped, dynamically-linked debug-symbol
  build on a different (glibc) target than the real release artifact and must not be
  quoted anywhere as the shipped binary's size — the real musl release build (smaller,
  static) was not produced (see Not checked). `sha256sum -c SHA256SUMS` against the two
  generated archives: both `OK`.
- `check-version` job logic, run by hand against the tagged commit: `Cargo.toml` reports
  `0.1.0-beta.7`, tag name strips to `0.1.0-beta.7` — match, the job would pass.

**Not checked**
- **The real `x86_64-unknown-linux-musl` build.** `rustup target add
  x86_64-unknown-linux-musl` succeeded, but linking failed —
  `x86_64-linux-musl-gcc` is not installed on this machine, and installing it
  (`musl-tools` via `apt-get`, what `release.yml` itself does before this step) needs
  `sudo`, which this environment doesn't have. The `x86_64-unknown-linux-gnu` build used
  instead exercises identical `cargo auditable` build flags, identical packaging and
  checksum logic, and identical embedded-SPA wiring — the only thing it does not prove is
  that the musl toolchain itself links cleanly, which was not touched by this card and had
  no reason to have changed.
- **macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`) and Windows
  (`x86_64-pc-windows-msvc`) builds.** No macOS or Windows host, and no cross-compilation
  toolchain for either, is available here. Both matrix legs were read in full, including
  the `pwsh`-flavored Windows packaging step (which mirrors the bash one's file list and
  naming exactly) — nothing unsound was found, but this is a claim proven by reading, not
  by a green build, and is weaker than the Linux claim above.
- **A real GitHub Actions run.** Never triggered, per this card's explicit instructions —
  no `gh workflow run`, no tag push, no branch push.
- **Whether the actual tagged `x86_64-unknown-linux-musl` binary, once someone builds it,
  still starts and serves the UI / enrolls a runner end to end.** V-A1's clean-container
  proof already did this against the prior release (`v0.1.0-beta.6`)'s real musl binary;
  this card did not repeat that specific proof against a `beta.7` musl artifact, because
  producing one was exactly what this environment couldn't do (see above).

## Next step

Get this branch through the normal integration process onto `develop` (same path V-A1 and
V-A2 already took), then, from a checkout that has the resulting commit, publish the
release:
`git push origin v0.1.0-beta.7`
That single push is what triggers `release.yml` for real — nothing before it does.

Branch: `agent/v-a3-cut-release`, committed (two commits: the version/changelog cut, then
the generated-spec/frontend-version follow-up the version bump required). The annotated
tag `v0.1.0-beta.7` exists locally on this branch's tip and has not been pushed.
