# ADR 0026: the runner finds its own settings, and a card says when nothing checks it

**Status: ACCEPTED 2026-10-19.** The maintainer accepted both parts on 2026-10-19 during the
Meridian onboarding rerun and asked for them to be applied the same day.

**Decide:** that the embedded runner reads **one settings file in a fixed place** when no other is
named, and every screen that matters **says which file it read**; and that **tests and gates come
from the repository or from the run itself, never from onboarding**: where nothing can check a
card, the board **says so** instead of something inventing a check.

**Why now:** the embedded runner read a settings file only when each start named it. A reset of
the data folder, a restart by hand and the unit `tack service install` writes all start it
without one, and then it has no docket, no verifier and no branch push, with no error. It
happened on every Meridian rerun. In the same rerun the onboarding skill wrote two gate scripts
and a content folder Meridian never had, to check work that had no test.

**If you do nothing:** every restart can silently turn docket and prill off; a card with no check
looks the same in the run dialog as one with three.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **A default place for the runner's settings.** With no `--runner-config` and no `TACK_LOCAL_RUNNER_CONFIG`, the embedded runner reads `<per-user config dir>/tack/runner.toml` when it exists. It is outside the data folder. The variable set but empty means no file | Write `~/.config/tack/runner.toml` once; docket and prill stay on across restarts |
| 2 | **The board names the file.** `GET /api/local-runner` carries `config_file`; the Agents page and `tack runner doctor` print it, or say there is none and what that turns off. The "prill is not installed" sentence names the `[verify]` table as well as `TACK_PRILL_PROGRAM` | "Settings from …", or "No settings file: this computer runs no docket team and verifies nothing" |
| 3 | **A card with no acceptance check says so before it runs.** The run dialog shows a note, never a blocker, when the card's brief has no acceptance criterion | "This card has no acceptance check: nothing tests what the agent delivers, so a person reviews it" |
| 4 | **Who makes a test, and when.** A bug's failing test is written by the run (docket's red step); a feature's tests are part of the change and prill judges them; an area with no tests at all gets no invented gate: the board says so (decision 3) and a person reviews. Onboarding uses the tests a repository already has, where they apply, and writes none | Onboarding prepares the project; it does not add scripts that check work |

## Not decided

Who puts a project's dependencies (for example `node_modules`) into the worktree an agent works
in. Decided on 2026-10-19 by [ADR 0027](0027-whoever-makes-the-working-copy-prepares-it.md):
whoever makes the copy prepares it, from the repository's lockfile.

## Not done, on purpose

No autodetection of docket or prill, no generated settings file, no editor for it in the UI.
