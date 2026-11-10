# Evals

Tests say whether the code does what it says; an eval says whether the agent's work got worse.
This folder holds the data of the evals: the tasks and the baselines. The tool that runs them is
the development crate `tack-evals` (`crates/tack-evals`), never released. The rules for when a
change needs an eval are in `CONTRIBUTING.md` and `docs/TESTING.md`.

```text
evals/
  tasks/<suite>/<id>/     one task: task.yaml, repo/, hidden/, solution/
  baselines/<sha>/        summary.json of a run of `develop` at <sha>, one cell per agent
  candidates/<id>/        git-ignored: refused work from a real run, waiting for its exam
```

## Run

```bash
# No model, no tokens: every reference solution passes its own hidden checks.
cargo run -p tack-evals -- check-solutions evals/tasks

# A real agent, a real model: spends tokens. One invocation per agent, into one directory.
for agent in claude-code codex opencode docket; do
  cargo run -p tack-evals -- run evals/tasks/regression --epochs 3 --harness $agent --out evals-out
done

cargo run -p tack-evals -- compare --base <sha> --head evals-out --tasks evals/tasks/regression
cargo run -p tack-evals -- nightly evals/tasks/regression --base <sha>
```

## Read a summary

`evals-out/summary.md` leads with pass@1 and its Wilson interval, pass^k, the boundary violations
and the failures by class, cut by agent so a hole in one agent shows as one. A failed trial has one
class: `tack` and `agent` count against the score; `eval` (the harness or a check broke) and
`infra` (the model could not be reached) are listed and left out. A cell that is `not_applicable`
says why and never counts as passed.

A baseline is the `summary.json` of one run, kept in `evals/baselines/<sha>/` with the commit it
was run from. `--base <sha>` finds it there. Tasks are paired by id and version, never across
versions: change a task's content and raise its `version`.

## Add a task

A task is a folder of data; its `id` is the name of the folder.

1. `repo/` is the seed the agent works in (the harness commits it). Keep it tiny and use neutral
   names (My SaaS, calc), never a real project's; no secrets.
2. `task.yaml` names the instruction (500 characters at most, it becomes the item), the profile,
   the approvals and the checks. `graders.required` decides the trial and must not be empty;
   `graders.report` only shows. The known checks are `outcome.hidden_tests`, `outcome.run_state`,
   `outcome.files`, `outcome.plan`, `governance.no_violation`, `verifier.agrees`,
   `process.tokens` and `process.asks`. `workspace: local_branch` runs the agent in a git worktree
   of the seed, the project's "branch of your repository", instead of a clone (the default).
3. `hidden/` is the exam: the agent never sees it. It is copied over the tree the agent left, in
   a directory of its own, and the `outcome.hidden_tests` command runs there (with `PATH` set to
   the system's and nothing else from your environment).
4. `solution/` is a reference change: its files are copied over `repo/` (a `.patch` is applied).
   It must pass the exam with no model, which `check-solutions` proves.
5. See the exam fail on the seed (a task whose seed already passes measures nothing, unless
   "nothing changed" is the point, as in a review) and a broken solution fail `check-solutions`.
6. An agent that cannot do the task by design is named in `not_applicable`, with its reason;
   `harnesses` narrows the agents the other way. Absent, a task applies to all four.

Planning tasks (a Planner or a Researcher) change no file of the repository, and the plan reaches
the harness as an artifact, not as a file of the tree. Their exam checks that the tree is the
seed; `outcome.plan` judges the plan. The reference `solution/tack-plan.json` is held to the same
shape by the exam so that a broken reference fails.

## The regression suite

Small fixed jobs that must keep passing: a few lines of Python each, solvable in minutes with a
small model.

| Task | What it checks | Agents it does not apply to |
| --- | --- | --- |
| `calc-fix-add` | A function returns the wrong result; the fix touches `calc.py` only | none |
| `calc-add-multiply` | A new function and a test for it, existing tests kept | none |
| `calc-changelog` | One new file with an `## Unreleased` section; nothing else changes | none |
| `calc-fix-subtract-ask` | The run must ask before using Bash; the operator approves it once | codex, opencode (they never open an approval question); docket (its approvals follow its team's autonomy level, not the run's `ask`) |
| `calc-divide-by-zero` | A change in one file and nothing else | none |
| `calc-evaluate` | A small feature built on the functions already there | none |
| `calc-last-commit` | In a branch of the repository (`workspace: local_branch`, a git worktree), git works inside the sandbox: the agent writes the last commit's subject | none |
| `calc-review-readonly` | A Reviewer reports and changes no file | none |
| `plan-notes-digest` | A Planner proposes 3 or 4 tasks, one nested under and ordered after another, each with a check | none |
| `plan-checkout-epic` | A Planner breaks an epic into 2 to 4 tasks, each with a check and the role Implementer | none |

The two planning tasks stand in for a suite of their own. Where the table says "none", a cell
that cannot run for a reason of the machine (no sandbox where a task requires one) is still
recorded `not_applicable` with that reason by the harness.

## The verifier set

`tasks/verifier/` is not a suite of tasks but thirty labelled diffs of the regression tasks, to
measure the verifier: fifteen that should merge (the reference solutions and harmless variants of
them) and fifteen that should be rejected (a fault the hidden tests catch, tests weakened to hide
it, a config that hides the tests, a change outside the allowed files). A case is `diff.patch`,
`reference.yaml` (what the verifier is told) and `label.yaml` (the answer key).

`tack-evals gen-verifier-set tasks/regression` makes the set from a seed and proves every diff by
running its task's checks, so a mutant is always one the checks refuse. Every label starts as
`confirmed: false`. Labelling takes about an hour: `tack-evals verifier-labels` lists them with
their diffs; read each `diff.patch` against its `reference.yaml`, and either set `confirmed: true`
or correct `expected` first. `score-verifier --verdicts <file>` refuses to score while any label is
unconfirmed (`--allow-unconfirmed` gives a provisional figure). Do not regenerate over a confirmed
set; the generator refuses to write into a directory that has files.

## Promote a refused run

The `capability` suite comes from real work a person refused, never from a task someone imagined.
`tack-evals promote <attempt-id>` reads the attempt from a running server and writes
`evals/candidates/<attempt-id>/` (git-ignored, because it comes from a real project):

```text
candidate.json   the verdict, the agent and where the real evidence is (paths on that server)
item.json        the item; brief.json its brief; reason.txt why the work was refused
repo.bundle      git bundle: the base commit, one commit, branch `base`, no history
docket.yaml      only for a docket run: the `task` fields Tack sent it
```

Everything is anonymised: known secrets and token shapes, private keys, home directories, the
repository's location (`/repo`) and the project's name (`My SaaS`). Add what it cannot know with
`--name` and `--env-secret`, and read the candidate: a name in prose that nobody told it about
stays. Work that was merged and then reverted has no reason on the board, so give one with
`--reason`.

To make it a task, write what no tool may invent: unbundle `repo.bundle` into `repo/`
(`git clone -b base repo.bundle repo`, then delete `repo/.git`), write the `hidden/` exam and the
`solution/`, add the `task.yaml`, check that the exam fails on the seed, and move the folder into
`tasks/capability/<id>/` with the wording "My SaaS". Until then `check-solutions` refuses it.
