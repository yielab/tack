# ADR 0027: whoever makes the agent's working copy prepares it

**Status: ACCEPTED 2026-10-19.** The maintainer accepted decisions 1 to 4 on 2026-10-19, after the
Fabulada onboarding rerun, and decision 5 (token limits by kind of task) later the same day. Decision 5
amends ADR 0024 decision 1: a run now always has a limit. Executed by Phase 77
(`docs/plans/phase-77.md`).

**Decide:** that a run's dependencies are installed by **whoever makes the copy of the code the
agent works in**: docket for a docket run, Tack's runner for Claude Code, Codex and opencode. That
Tack **makes no copy of its own** for a docket run. That the install command comes **from the
repository's lockfile**, can be replaced in one place per project, runs with network only while it
installs, and is reused from a cache instead of being repeated on every run.

**Why now:** a docket run of Fabulada makes three copies of the repository and installs nothing in
the one where the agent works. Measured in such a copy: `eslint` and `vitest` do not start (they
cannot find their own configuration's packages) and `tsc` runs the machine's TypeScript 7 instead
of the project's 5.9. Every code run would fail its check whatever the change, and spend tokens
fixing a failure that is not its own. ADR 0026 left the owner open and ruled out the onboarding
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

| 5 | **A token limit per kind of work, learnt from its own runs.** A kind of work is a profile: a project with dozens of cards has a handful. A profile with no limit of its own runs under a **provisional limit**, 2 000 000 tokens by default, which is the cap the SWE-Effi benchmark gives one software task, and which can be changed under Settings → Automation. Once a profile has **5 successful runs**, the board **proposes** a limit: the 90th percentile of its last 20 successful runs × 1.5, rounded up. It never applies it by itself: the person does, with one click. A run can still set its own limit in the Run dialog. A run shows amber at 80% of its limit, and one stopped by it says how much it used of how much and can be run again with more | "Provisional limit: 2 000 000 (no measured runs yet)"; then "The last 7 runs used 180 000 to 520 000. Proposed limit: 720 000 [Apply]"; and "Stopped at its limit: 720 000 of 720 000 [Run again with more]" |

Decision 5 in practice: the limit is not estimated card by card, because how hard a card looks
predicts badly how many tokens it takes (the same task varies up to 30×), while a profile's own
history is a stable measure. A failed run costs about four times a successful one, so the limit is
there mostly to cut runaway failures, not to trim good runs. docket needs no change for it: given a
task's `maxTokens`, each of its steps already gets what is left of it, so docket's own per-step
setting (`AGENT_LOOP_TOKEN_BUDGET`) only applies to a task that comes with no limit, and from Tack
none does.

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
- **Token use per task (decision 5).** Bai et al. 2026 (arXiv 2604.22750): the same task varies up to
  30× in tokens between runs, a task's difficulty predicts its use weakly (Kendall τb 0.32), accuracy
  stops improving past a point, and early warnings are the recommended brake. SWE-Effi (arXiv
  2509.09853): failed runs use about 4× the tokens of successful ones; the benchmark caps a task at
  2 million tokens. SWE-bench Science (arXiv 2608.19799, Table 2): mean input 3.7–7.4 million and
  output 40 000–160 000 tokens per task, most of the input cache reads, which Tack does not count
  (ADR 0024 decision 2). docket gives each step what is left of the task's `maxTokens`
  (`serve.py`, `dispatch.py`).
- **Others do the same:** Codex cloud runs a setup script with internet and the agent without it;
  GitHub's Copilot coding agent has `copilot-setup-steps.yml`; Cursor's cloud agents have an
  `install` step kept in a build; Claude Code copies ignored files into its worktrees with
  `.worktreeinclude`.
