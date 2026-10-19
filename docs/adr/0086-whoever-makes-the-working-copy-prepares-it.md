# ADR 0086: whoever makes the agent's working copy prepares it

**Status: ACCEPTED 2026-10-19.** The maintainer accepted decisions 1 to 4 on 2026-10-19, after the
Fabulada onboarding rerun. Token limits by kind of task were proposed in the same conversation
and are **not** decided here (see "Not decided").

**Decide:** that a run's dependencies are installed by **whoever makes the copy of the code the
agent works in**: docket for a docket run, Tack's runner for Claude Code, Codex and opencode. That
Tack **makes no copy of its own** for a docket run. That the install command comes **from the
repository's lockfile**, can be replaced in one place per project, runs with network only while it
installs, and is reused from a cache instead of being repeated on every run.

**Why now:** a docket run of Fabulada makes three copies of the repository and installs nothing in
the one where the agent works. Measured in such a copy: `eslint` and `vitest` do not start (they
cannot find their own configuration's packages) and `tsc` runs the machine's TypeScript 7 instead
of the project's 5.9. Every code run would fail its check whatever the change, and spend tokens
fixing a failure that is not its own. ADR 0085 left the owner open and ruled out the onboarding
skill: a project is not always onboarded by it.

**If you do nothing:** no project with dependencies can run code work through Tack, and each one
that tries works around it with its own wrapper script.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **One owner per copy.** The program that makes the copy the agent works in prepares it: docket makes and prepares its task's copy; Tack's runner makes and prepares the copy for Claude Code, Codex and opencode. The verifier keeps its own copies and its own preparation on purpose: what checks the work does not share the worker's environment | Nothing new: a run starts with its dependencies in place |
| 2 | **No second copy for docket.** For a docket run, Tack reads the base commit from the project's repository, sends it to docket, takes docket's branch from that same repository, and gives the verifier the repository with the two commits. It no longer checks out a copy it only used to read those commits | One copy of the code per docket run, not two |
| 3 | **The install command comes from the lockfile.** `package-lock.json` → `npm ci`, `pnpm-lock.yaml` → `pnpm install --frozen-lockfile`, `yarn.lock` → `yarn install --immutable`, `uv.lock` → `uv sync --frozen`, `poetry.lock` → `poetry install`, `Cargo.lock` → `cargo fetch`. A project can replace it, or turn it off, in Settings → Automation → "Where the code is"; Tack sends that value to docket with the task. No lockfile means no install, and the screen says so | "Prepares with `npm ci` (from package-lock.json)", or "No lockfile: nothing is installed" |
| 4 | **Network only to install, a cache, and a clear failure.** The install has network; the agent then has its team's network. What one lockfile installs is kept once and linked into each copy, so the second run with the same lockfile prepares in seconds. A failed install ends the run before the agent starts, with the command's output as its reason. A command that runs on this machine is shown before it runs: the detected or replaced command is on the settings screen and in the Run dialog, and docket asks its review code for it like any other command | "Could not prepare the dependencies: `npm ci` exited 1" and its output, with no tokens spent |

## Not decided

- **A token limit by kind of task.** Proposed on 2026-10-19: a limit per profile learnt from that
  profile's past runs, with a starting value measured in the calibration run. It would change
  ADR 0083 decision 1 (no default limit), so it waits on the maintainer.

## Not done, on purpose

- No container image per project, no guessing without a lockfile, and nothing written by the
  onboarding skill. A project whose preparation is not an install sets its own command.
- The verifier's preparation is not merged with the agent's (decision 1).

## Evidence (2026-10-19)

- **Three copies per docket run.** Tack's runner checks out the project into
  `<state dir>/workspaces/<attempt>` (`git worktree add --detach`). For docket it only reads
  `HEAD`, resets onto docket's branch at the end and passes the folder to prill as `--repo`.
  docket makes its own worktree under `~/.docket/workspaces/projects/<pod>-implementer/tasks/<id>`;
  that is where the agent works and the team's checks run, jailed, with no network on a
  lights-out team. prill materializes its base, head and overlay trees from git and accepts any
  git repository with `--base`/`--head`.
- **Nothing installs anything.** Neither Tack's runner nor docket has a setup step; git does not
  copy ignored files, so `node_modules` is absent. Measured in a fresh worktree of Fabulada:
  `Cannot find package 'eslint'`, `Cannot find module 'vitest/config'`, `tsc` 7.0.2 (global)
  against 5.9.3 (the project's).
- **prill already does it this way:** `prepare.command` runs once, with network, in a container
  whose result is kept; every check after it has no network; its configuration is read from the
  base commit.
- **Others do the same:** Codex cloud runs a setup script with internet and the agent without it;
  GitHub's Copilot coding agent has `copilot-setup-steps.yml`; Cursor's cloud agents have an
  `install` step kept in a build; Claude Code copies ignored files into its worktrees with
  `.worktreeinclude`.
