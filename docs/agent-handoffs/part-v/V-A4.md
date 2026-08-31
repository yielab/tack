## What this is about

Tack's public-facing story didn't match itself: the GitHub description, the README's
first line, and the book's introduction each said something different about what the
product is, none of them mentioned the one thing no competitor has, and the genuinely
good user/developer book was built nowhere anyone could read it. This closes all three
gaps — one sentence now says the same thing everywhere it appears, the README's first
screen states what's proven and what isn't in the same honest terms the last two cards
proved, and the book has a working publish pipeline sitting one setting away from live.

## Where it stands

One sentence — "Tack assigns board items to AI coding agents — Codex, Claude Code, or
OpenCode — and keeps the run as part of the project's history, in one self-hosted
binary." — now appears identically in the README's opening line and the book's
introduction, and is the exact text recommended for the GitHub description (not
changed live — see Next step). It leads with what the product does, not with "project
manager," and names the three real tools instead of using the word "harness."

The README's first screen is reordered to answer, in order: what Tack is, what it does
that nothing else does, how to run it, and what doesn't work yet. The harness-status
claims are the precise, hedged language the last two cards proved, not an upgrade to
"three harnesses work": `claude-code` and `opencode` complete real live runs;
`codex` runs the full pipeline for the first time ever but has never finished one, and
the reason is stated (an account/model-access gap on this machine, not a defect). The
install-path caveat (the one-line installer 404s until a branch is published) is stated
plainly instead of presented as if it already works. A `docs/CONFIG.md` cross-check
confirmed the "one active SQLite writer" limitation already in the README is real
(`ARCHITECTURE.md` states "SQLite only supports one writer at a time" outright), so it
was kept, not invented.

A GitHub Pages workflow now exists and was proven to build the book with its own exact
command, but nothing was deployed and no live GitHub setting was touched — Pages isn't
enabled yet, so the workflow has never run for real. That's intentional; see Next step.

What still doesn't work: the one-line installer (still blocked on the `main` branch
publish from the prior card, not something this card touches), the book's actual public
URL (still 404, same as before — the workflow exists but hasn't run), and the GitHub
repository's live description/topics/homepage (still the old, agent-less text — a
prepared command is waiting, not yet run).

## What is left

**Enabling GitHub Pages and verifying the deploy.** The workflow
(`.github/workflows/pages.yml`) builds and would deploy on a push to `develop`, but
GitHub Pages itself has never been turned on for this repository (`homepageUrl` is
still empty, confirmed via `gh repo view`). Someone with repository-settings access
needs to flip that one switch, merge this branch, push `develop`, and watch the run —
see Next step for the exact steps. Only after that succeeds and
`https://yielab.github.io/tack/` actually returns 200 should the repository's
`homepageUrl` be set to point at it; setting it earlier would repeat the exact "linked
somewhere that 404s" mistake this whole cycle is about fixing.

**The GitHub description, topics, and homepage are still the old text.** This card
intentionally did not call the GitHub API to change them — that's explicitly an
outward-facing action reserved for the user. The exact command is in Next step.

**The stray `.sqlite` snapshot file still exists on disk, in the main checkout, not
here.** It never appeared in this worktree (worktrees don't inherit another
checkout's untracked files), so there is nothing for this branch to `git rm`. The real
fix — a `.gitignore` pattern for the whole class of file — is included in this branch.
The physical 440 KB file itself
(`/home/ox/Sites/objetivosMios/tack.db.before-037_orch_runs_rebuild.sqlite`, dated
2026-08-26) still needs a human, or an agent working directly in the main checkout, to
delete it; this card was explicitly told not to touch that directory.

**`docs/book/src/user-guide/agent-runners.md` is now stale and contradicts the
harness-status claims this card just made prominent.** Its closing section, "What
actually runs today," states that `tack-runner`'s own network transport "does not
exist yet" and that a real runner "cannot yet point at a real server and watch it pick
up work." That was true when it was written but has been false since at least
`984bb5f` (`feat(runner): implement the HTTP transport...`) landed on `develop`, and
V-A2's live runs prove it doubly false today. This card does not own that file and did
not edit it — but publishing the book more visibly (this card's whole point) means more
people will now read a page that flatly denies the runner works. Whoever owns
`docs/book/src/user-guide/` next should rewrite that section from V-A2's evidence
before or shortly after Pages goes live.

**The ten project-type presets pull the story toward "generic project manager," the
losing category.** Recorded per instruction, not acted on: construction, legal,
homework, and events presets are real, useful, and already shipped, but every one of
them argues Tack is competing with Plane/Huly/Vikunja on their turf (mature,
well-funded, losing proposition per this card's brief) rather than leading with the
one thing those tools cannot do. Whether to keep, hide, reorder, or reframe those
presets in onboarding/marketing is a product decision for the user, not something a
docs card should decide unilaterally. No preset code was touched.

## Technical detail

**Where the code lives** — `README.md` (full rewrite), `docs/book/src/introduction.md`
(opening two paragraphs rewritten, rest untouched), `.github/workflows/pages.yml`
(new), `.gitignore` (one new pattern), this handoff.

**How the one-sentence positioning was derived** — the brief asked for a sentence that
leads with the differentiator, not the category, and is comprehensible without the
word "harness." The sentence opens with the verb ("Tack assigns board items to...")
rather than "Tack is a project manager," names the three real products (Codex, Claude
Code, OpenCode) instead of the abstract term, and states the outcome ("keeps the run
as part of the project's history") instead of the mechanism. It is applied byte-for-
byte identical in `README.md` line 5 and `docs/book/src/introduction.md` line 5 (bold,
one paragraph each), and is the literal string recommended for
`gh repo edit --description` in Next step — three placements, one sentence, as asked.

**How the README's first screen is ordered** — top to bottom: the one-sentence
positioning; a short paragraph stating the category explicitly ("Tack is a project
manager...") and the differentiator in plain terms; a "Proven today" / "Not yet" pair
of short paragraphs (the condensed version of the harness/limitation claims); a longer
competitive paragraph (mature PM tools execute nothing, the open-source orchestrators
that used to compete are gone, what's left is closed-source or boardless); then `## Run
it` (quick start, cargo-install-first since that's what's proven working, the one-line
installer shown with its caveat inline); then `## Harness status` (the full,
precisely-worded three-row table); then `## Current limitations` (the existing table,
moved up, with the installer caveat folded in and the stale "blocked only on
installing codex" language removed everywhere it appeared in the file — it occurred
four times: the old dev-status blockquote, removed entirely as duplicate of the new
top section; the Delivery status Phase 50-57 and Phase 57 rows; and the Architecture
section's Phase 57 mention. All four now point at "Harness status above" instead of
repeating a claim V-A2 disproved). Screenshots, the Product Direction diagram, Delivery
status phase tables, and the rest of the original document follow, materially
unchanged, after this new top section.

**How the GitHub Pages workflow works** — `.github/workflows/pages.yml`, two jobs.
`build`: checks out the repo, runs `actions/configure-pages@v5` (fails fast with a
clear message if Pages isn't enabled for the repo, rather than a confusing failure
later), installs `mdbook 0.4.40` via `cargo install --locked` (the same pinned version
`ci.yml`'s existing `docs` job already uses, so both jobs are provably building the
identical book), runs `mdbook build docs/book`, and uploads `docs/book/book` as a Pages
artifact (`actions/upload-pages-artifact@v5`). `deploy`: depends on `build`, targets the
GitHub-managed `github-pages` environment, and calls `actions/deploy-pages@v5`. The
`environment: { name: github-pages }` block matches GitHub's own official starter
workflow for Pages verbatim (verified via
`gh api repos/actions/starter-workflows/contents/pages/static.yml`) — an IDE lint flag
on that value ("not valid") is a false positive from a static schema that doesn't know
about repository-specific dynamic environments; it is not a mistake in this file.
Trigger is `push` to `develop` only, because `develop` is this repository's actual
default branch and the only branch that exists on the remote today (confirmed via
`gh repo view yielab/tack --json defaultBranchRef` → `"develop"`, same finding V-A1
already made); `main` is referenced by install docs but was never pushed. If `main`
is published and becomes the release/publish branch later, this trigger should move to
`main` so the deployed book tracks released state instead of the development tip —
noted in the workflow's own header comment for whoever makes that call.

**How the stray `.sqlite` file was handled** — it is not a bug or leftover, it's
`create_pre_upgrade_backup_if_needed` in `crates/tack-db/src/migrations.rs` doing
exactly what it's designed to do: before running a `Rebuild`-kind migration (037 in
this case), it runs `VACUUM main INTO '<db-file>.before-<migration-name>.sqlite'` to
create a transactionally consistent recovery snapshot, and the code comment on that
function is explicit that this file must never be overwritten once created — it's the
recovery artifact, not disposable cache. The existing `.gitignore` already covers
`*.db`/`*.db-shm`/`*.db-wal` but not `*.sqlite`, so this class of file has never been
covered. Added `*.before-*.sqlite` to `.gitignore`, matching the exact naming pattern
the function produces, so every future pre-upgrade snapshot (for 037 or any later
`Rebuild` migration) is ignored automatically. `git check-ignore -v
tack.db.before-037_orch_runs_rebuild.sqlite` confirms the new rule matches. The file
itself lives only in the main checkout (`/home/ox/Sites/objetivosMios/`, 440 KB, dated
2026-08-26), which this card was told not to touch; it is untracked there too, so no
`git rm` is needed — just a filesystem delete, left for whoever next works directly in
that checkout.

**What is blocking, technically** — Nothing in this repository blocks the Pages
workflow from working; it was proven to build correctly with its own exact command.
What blocks it from being *live* is a one-time GitHub repository setting
(Settings → Pages → Source: GitHub Actions) that only someone with admin access to
`yielab/tack` can flip, plus actually pushing this branch's workflow file to `develop`
to trigger it — both explicitly reserved for the user per this card's instructions, not
a technical gap in the code delivered here.

**Test results:**
- `mdbook build docs/book` with the exact locally-installed version (0.5.3): clean, no
  errors, `docs/book/book/index.html` produced, 6.5 MB output.
- The workflow's exact command, `mdbook build docs/book`, re-run with `mdbook 0.4.40`
  installed via `cargo install mdbook --locked --version 0.4.40 --root <isolated
  path>` (the exact version `ci.yml`'s existing `docs` job pins, installed to a
  separate root so it didn't overwrite the shared `~/.cargo/bin/mdbook` other agents
  may be using concurrently): clean, no errors or warnings, 36 HTML pages, 11 MB
  output. The same broken-link check `ci.yml`'s `docs` job runs
  (`mdbook build docs/book 2>&1 | grep -i "error\|broken"`) found nothing.
- `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/pages.yml'))"` —
  parses cleanly.
- `./scripts/verify-install-urls.sh` (V-A1's check, unmodified) still finds and checks
  the same two URLs after the README rewrite, with the same expected result as before
  this card (`OK [200]` releases page, `FAIL [404]` install.sh on `main`) — proving the
  README restructure didn't accidentally break V-A1's check or change the URL text it
  depends on.
- Cold-read self-test (the card's own acceptance test): re-read only `README.md` lines
  1–15 with no other context loaded. What I could honestly say the product is from
  those lines alone: "A self-hosted, single-binary project manager (workflows,
  vocabulary, board/timeline/dashboard views) whose standout feature is handing a board
  item to a real AI coding agent — Codex, Claude Code, or OpenCode — and keeping that
  run as part of the project's history. Two of the three agent backends
  (`claude-code`, `opencode`) actually finish real runs today; the third (`codex`) can
  start one but has never finished one, for an account-level reason on this machine,
  not a bug." That's an accurate, complete answer to "what is it" and "what's the
  differentiator," and the proven/not-proven harness split is fully stated within the
  15 lines. The itemized "not yet" list (installer 404, single SQLite writer, no
  accounts) begins within the 15 lines (the installer 404 is fully stated) but its last
  two items complete on line 16–17, one to two lines past the strict cutoff — noted
  here rather than hidden; tightening further traded away clarity for a marginal line
  count and wasn't worth it.

**Claim → evidence table** (every capability claim on the README's first screen):

| Claim (README) | Evidence |
| --- | --- |
| `claude-code` and `opencode` complete real, live end-to-end attempts | V-A2 handoff, "Where it stands": "proven against the actual installed `codex`, `claude`, and `opencode` binaries"; final live smoke run shows both succeeding |
| `codex` runs the complete pipeline (claim → checkout → spawn → real network call → structured result) for the first time ever | V-A2 handoff: "this run is the first time it has ever gone through the complete pipeline... instead of being turned away before it started" |
| `codex` has never completed a live attempt; the cause is an account/model-access gap, not a code defect | V-A2 handoff, "What is blocking, technically": exact error `"The 'gpt-5-codex' model is not supported when using Codex with a ChatGPT account."`, captured by the now-working pipeline |
| The one-line installer 404s because its branch isn't published | V-A1 handoff: `raw.githubusercontent.com/yielab/tack/main/install.sh` confirmed 404 today; `git ls-remote --heads origin` shows no `main` |
| `cargo install --git` works today (installs from `develop`) | V-A1 handoff + this card's own `gh repo view yielab/tack --json defaultBranchRef` → `"develop"` (cargo's default-branch install target) |
| One active SQLite writer | `docs/ARCHITECTURE.md:319`, "SQLite only supports one writer at a time" (verified by this card, not invented) |
| No per-user accounts, one optional shared token | Pre-existing README claim, re-verified by reading `docs/CONFIG.md`'s auth section; unchanged by this card |
| Binaries not code-signed | Pre-existing README claim, unchanged; not re-verified by this card (no code change touches signing) |

**Not checked** — Star counts and shutdown/deprecation dates for competing products
(Plane, Huly, Vibe Kanban, Crystal, etc.) were given as card context, not independently
re-verified by this card, and were deliberately left out of the README's own wording
for that reason — the competitive paragraph states the shape of the landscape without
citing numbers this card didn't measure itself. `docs/book/src/user-guide/agent-
runners.md` was read but not edited (see What is left). No live GitHub Actions run of
`pages.yml` was triggered (would require pushing and enabling Pages, both reserved for
the user). macOS/Windows mdBook builds were not exercised, matching V-A1's own scope
note — this environment is Linux only. The GitHub description/topics/homepage were
read (`gh repo view`) but never written.

## Next step

Two independent things, in this order:

1. **Safe to run immediately** (no live-URL dependency):
   ```
   gh repo edit yielab/tack \
     --description "Tack assigns board items to AI coding agents — Codex, Claude Code, or OpenCode — and keeps the run as part of the project's history, in one self-hosted binary." \
     --add-topic ai-agents --add-topic agent-orchestration --add-topic mcp
   ```
2. **Publish the book, in order** — enable Pages (Settings → Pages → Source: GitHub
   Actions, or `gh api -X POST repos/yielab/tack/pages -f build_type=workflow`); merge
   this branch and push `develop` so `.github/workflows/pages.yml` runs; confirm it
   deployed with `curl -I https://yielab.github.io/tack/` (expect `200`); only then run
   `gh repo edit yielab/tack --homepage "https://yielab.github.io/tack/"` — setting the
   homepage before that URL resolves would repeat the exact "linked to something that
   404s" mistake this cycle exists to fix.

Branch: `agent/v-a4-positioning`, committed.
