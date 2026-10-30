# Architecture Decisions

This page is for contributors and integrators who want to know why Tack is built the way it is. It explains what an architecture decision record is here and lists every one in the repository. After reading it you can find the decision behind a behaviour and tell whether a later decision has replaced it.

## What an ADR is here

An architecture decision record (ADR) is a short document in [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr) that asks the maintainer to decide something and keeps the answer. Each file is named `NNNN-slug.md` and numbered in the order it was written, from 0001 since the renumbering of 2026-10-26 (see the table at the end). Numbers are never reused. An ADR with nothing left to decide moves to [`docs/closed-cycles/adr/`](https://github.com/yielab/tack/tree/develop/docs/closed-cycles/adr) and keeps its number, which is always above 0030.

The recent records open the same way, so that the person who must approve one reads a single screen and no more:

1. A **Decide** paragraph that says what is being asked.
2. **Why now** and **If you do nothing**, which say what putting the decision off costs.
3. A **Decisions** table, one row per decision, with what a person will see. Accepting the table accepts the ADR.
4. **Evidence**, **Alternatives not taken** and **Questions for the maintainer**, kept apart from the table so they never get in the way of the decision.

The oldest records use a plainer layout (context, decision, consequences). A record names the earlier records it amends. Read the newer record's "What this amends" section before you rely on an older one.

## Status words

| Status | Meaning |
|---|---|
| Proposed | Written and waiting for the maintainer. Nothing is built from it yet. |
| Accepted | The maintainer approved it, with the date. The Status line may name the phase that executed it, and may say that an amendment at the end of the file wins over the table. |
| Withdrawn | Not decided as an ADR. Any work it described moved to a plan. |
| Superseded | A later record replaced all or part of it. The file carries a dated note saying which decisions. |

The Status line in each file is the authority. The "Superseded by" column below only points at the later record; the older file says whether the replacement is whole or partial.

## All records, newest first

| ADR | Title | Status | Superseded by |
|---|---|---|---|
| 0030 | [the task goes to docket as fields, and a run says which prompts made it](https://github.com/yielab/tack/blob/develop/docs/adr/0030-the-task-goes-to-docket-as-fields-and-a-run-says-which-prompts-made-it.md) | Proposed 2026-10-29 | — |
| 0029 | [one test binary per crate, dependencies without debug info, agent builds without the incremental cache](https://github.com/yielab/tack/blob/develop/docs/adr/0029-one-test-binary-per-crate.md) | Accepted 2026-10-26 | — |
| 0028 | [the protocol is a crate, and the SQL has one home](https://github.com/yielab/tack/blob/develop/docs/adr/0028-the-protocol-is-a-crate-and-the-sql-has-one-home.md) | Accepted 2026-10-26 | — |
| 0027 | [whoever makes the agent's working copy prepares it](https://github.com/yielab/tack/blob/develop/docs/adr/0027-whoever-makes-the-working-copy-prepares-it.md) | Accepted 2026-10-19 | — |
| 0026 | [the runner finds its own settings, and a card says when nothing checks it](https://github.com/yielab/tack/blob/develop/docs/adr/0026-the-runner-finds-its-settings-and-a-card-says-what-checks-it.md) | Accepted 2026-10-19 | — |
| 0025 | [the project file is kept in sync, not only imported](https://github.com/yielab/tack/blob/develop/docs/adr/0025-the-project-file-is-kept-in-sync-not-only-imported.md) | Proposed 2026-10-18 | — |
| 0024 | [limits in tokens, and nothing sent to an agent twice](https://github.com/yielab/tack/blob/develop/docs/adr/0024-limits-in-tokens-and-a-lean-context.md) | Accepted 2026-10-18 | 0027 |
| 0023 | [what the first real project found, and what Tack changes for it](https://github.com/yielab/tack/blob/develop/docs/adr/0023-what-the-first-real-project-found.md) | Accepted 2026-10-17 | — |
| 0022 | [a whole project goes in and out through one file](https://github.com/yielab/tack/blob/develop/docs/adr/0022-a-whole-project-goes-in-and-out-through-one-file.md) | Accepted 2026-10-17 | 0025 |
| 0021 | [every run goes to a long-lived team, and a project's automation is one ladder](https://github.com/yielab/tack/blob/develop/docs/adr/0021-every-run-goes-to-a-team-and-automation-is-one-ladder.md) | Accepted 2026-10-14 | 0024 |
| 0020 | [merge authority follows the autonomy level you set, and Tack stays one of three tools that work alone](https://github.com/yielab/tack/blob/develop/docs/adr/0020-merge-authority-follows-the-autonomy-level-you-set.md) | Accepted 2026-10-13 | 0024 |
| 0019 | [the agent brings its own model](https://github.com/yielab/tack/blob/develop/docs/adr/0019-the-agent-brings-its-own-model.md) | Accepted 2026-10-13 | — |
| 0018 | [the agent reaches only what you gave it](https://github.com/yielab/tack/blob/develop/docs/adr/0018-the-agent-reaches-only-what-you-gave-it.md) | Accepted 2026-10-12 | — |
| 0017 | [a project and its items from one outline or CSV](https://github.com/yielab/tack/blob/develop/docs/adr/0017-projects-from-an-outline-or-a-csv.md) | Accepted 2026-10-12 | — |
| 0016 | [what the first docket + prill run found, and what Tack changes for it](https://github.com/yielab/tack/blob/develop/docs/adr/0016-what-the-docket-and-prill-run-found.md) | Accepted 2026-10-12 | 0019 |
| 0015 | [a run a normal person can start](https://github.com/yielab/tack/blob/develop/docs/adr/0015-a-run-a-normal-person-can-start.md) | Accepted 2026-10-07 | 0019 |
| 0014 | [agent onboarding — prerequisites discovery, validation, and configuration clarity](https://github.com/yielab/tack/blob/develop/docs/adr/0014-agent-onboarding-prerequisites-and-validation.md) | Accepted 2026-10-03 | 0015 |
| 0013 | [every attempt leaves evidence; an independent verifier certifies it; the runner pushes, the board opens the pull request](https://github.com/yielab/tack/blob/develop/docs/adr/0013-evidence-merge-readiness-and-the-verifier-boundary.md) | Accepted 2026-10-03 | 0015, 0018, 0020 |
| 0012 | [docket contract 1.1 is adopted — cancel becomes per-harness evidence, a recipe is one flag, the task goes by file](https://github.com/yielab/tack/blob/develop/docs/adr/0012-docket-harness-contract-1-1.md) | Accepted 2026-10-03 | 0021 |
| 0011 | [the brief is a first-class entity on the item, typed, and it travels](https://github.com/yielab/tack/blob/develop/docs/adr/0011-briefing-script-on-the-item.md) | Accepted 2026-10-03 | — |
| 0010 | [Tack is maintained by people — retire what only agents needed, delete what nothing calls, and rebuild testing and CI around verified behaviour](https://github.com/yielab/tack/blob/develop/docs/adr/0010-a-codebase-for-human-maintainers.md) | Accepted 2026-09-18 | — |
| 0009 | [opencode is readmitted as a fourth harness, on the shared core, with its one limitation typed](https://github.com/yielab/tack/blob/develop/docs/adr/0009-opencode-readmitted-as-a-fourth-harness.md) | Accepted 2026-09-14 | 0019 |
| 0008 | [Docket becomes a third harness — the runner spawns it, and the model axis finally has a harness that proves it](https://github.com/yielab/tack/blob/develop/docs/adr/0008-docket-as-a-third-harness.md) | Accepted 2026-09-14 | 0019, 0021 |
| 0007 | [Tests run under nextest, build once per resolution, and group one binary per subject](https://github.com/yielab/tack/blob/develop/docs/adr/0007-test-architecture.md) | Accepted 2026-09-05 | 0010, 0029 |
| 0006 | [Tack runs as a background service, and ships as a desktop app](https://github.com/yielab/tack/blob/develop/docs/adr/0006-desktop-app-and-background-service.md) | Accepted 2026-09-03 | — |
| 0005 | [Where a model provider's API key lives, and who's allowed to touch it](https://github.com/yielab/tack/blob/develop/docs/adr/0005-provider-credentials-at-the-runner-boundary.md) | Accepted 2026-09-03 | 0019 |
| 0004 | [Single-operator identity posture](https://github.com/yielab/tack/blob/develop/docs/adr/0004-single-operator-identity-posture.md) | Accepted | — |
| 0003 | [Standalone single-binary operation via an embedded runner](https://github.com/yielab/tack/blob/develop/docs/adr/0003-standalone-single-binary-runner.md) | Accepted | — |
| 0002 | [Tack-owned scheduling and runner-owned execution](https://github.com/yielab/tack/blob/develop/docs/adr/0002-runner-control-plane.md) | Accepted | 0010, 0019 |
| 0001 | [transactional migration rebuild recovery](https://github.com/yielab/tack/blob/develop/docs/adr/0001-transactional-migration-rebuild-recovery.md) | Accepted 2026-08-06 | — |

### Regenerating the table

The table is generated from the files, not written by hand. From the repository root, run the script below and paste its output over the table above. It reads each file's first heading, its Status line, and any sentence of the form "superseded by ADR NNNN".

```python
import pathlib, re
BLOB = "https://github.com/yielab/tack/blob/develop/docs/adr/"
rows = []
for path in sorted(pathlib.Path("docs/adr").glob("[0-9]*.md"), reverse=True):
    text = path.read_text()
    title = re.search(r"^# (?:ADR \d+: )?(.*)$", text, re.M).group(1).strip()
    status = re.search(r"^## Status\s+(.+?)$", text, re.M | re.S)
    line = (status.group(1).strip().splitlines()[0] if status else
            next(l for l in text.splitlines() if re.match(r"(- )?\**Status", l)))
    word = re.search(r"accepted|withdrawn|proposed|rejected", line, re.I)
    date = re.search(r"\d{4}-\d{2}-\d{2}", line)
    status_cell = (word.group(0).capitalize() if word else "See file") + (f" {date.group(0)}" if date else "")
    later = sorted({n for n in re.findall(r"[Ss]uperseded[^.\n]{0,40}?by ADR (\d{4})", text) if n != path.name[:4]})
    rows.append(f"| {path.name[:4]} | [{title}]({BLOB}{path.name}) | {status_cell} | {', '.join(later) or '—'} |")
print("| ADR | Title | Status | Superseded by |\n|---|---|---|---|\n" + "\n".join(rows))
```

Save it to a file and run it with `python3`, from the repository root.

## Reading order for newcomers

Six records explain most of the system. Read them in this order.

1. [ADR 0002, Tack-owned scheduling and runner-owned execution](https://github.com/yielab/tack/blob/develop/docs/adr/0002-runner-control-plane.md). Why the board schedules and the runner executes. Its decisions about models were replaced by ADR 0019; the rest stands.
2. [ADR 0003, Standalone operation via an embedded runner](https://github.com/yielab/tack/blob/develop/docs/adr/0003-standalone-single-binary-runner.md). Why one `tack` binary can run both halves on the same contract.
3. [ADR 0013, Evidence, merge readiness and the verifier boundary](https://github.com/yielab/tack/blob/develop/docs/adr/0013-evidence-merge-readiness-and-the-verifier-boundary.md). What every attempt leaves behind, who certifies it, and who pushes.
4. [ADR 0018, The agent reaches only what you gave it](https://github.com/yielab/tack/blob/develop/docs/adr/0018-the-agent-reaches-only-what-you-gave-it.md). The trust boundary: the board's token, the browser's origin, and what an agent can read.
5. [ADR 0020, Merge authority follows the autonomy level you set](https://github.com/yielab/tack/blob/develop/docs/adr/0020-merge-authority-follows-the-autonomy-level-you-set.md). The levels L3 to L5 and the signed verdict a merge needs.
6. [ADR 0021, Every run goes to a team and automation is one ladder](https://github.com/yielab/tack/blob/develop/docs/adr/0021-every-run-goes-to-a-team-and-automation-is-one-ladder.md). Teams, roles that choose profiles, starting by itself, the ceiling. [Runs, Teams and Automation](runs-and-teams.md) says how it is built.

After these, [ADR 0022](https://github.com/yielab/tack/blob/develop/docs/adr/0022-a-whole-project-goes-in-and-out-through-one-file.md) explains the project file, which [The Project File Format](project-file.md) documents.

## Numbers before 2026-10-26

On 2026-10-26 the ADRs were renumbered to run from 0001 without gaps, and the ones with nothing left to decide moved to `docs/closed-cycles/adr` and kept their number. Commit messages and anything written before that date use the old numbers; this table translates them.

| Old | New | File |
|---|---|---|
| 0008 | 0001 | `0001-transactional-migration-rebuild-recovery.md` |
| 0050 | 0002 | `0002-runner-control-plane.md` |
| 0058 | 0003 | `0003-standalone-single-binary-runner.md` |
| 0059 | 0004 | `0004-single-operator-identity-posture.md` |
| 0061 | 0005 | `0005-provider-credentials-at-the-runner-boundary.md` |
| 0062 | 0006 | `0006-desktop-app-and-background-service.md` |
| 0064 | 0007 | `0007-test-architecture.md` |
| 0066 | 0008 | `0008-docket-as-a-third-harness.md` |
| 0067 | 0009 | `0009-opencode-readmitted-as-a-fourth-harness.md` |
| 0068 | 0010 | `0010-a-codebase-for-human-maintainers.md` |
| 0069 | 0011 | `0011-briefing-script-on-the-item.md` |
| 0070 | 0012 | `0012-docket-harness-contract-1-1.md` |
| 0071 | 0013 | `0013-evidence-merge-readiness-and-the-verifier-boundary.md` |
| 0072 | 0014 | `0014-agent-onboarding-prerequisites-and-validation.md` |
| 0074 | 0015 | `0015-a-run-a-normal-person-can-start.md` |
| 0075 | 0016 | `0016-what-the-docket-and-prill-run-found.md` |
| 0076 | 0017 | `0017-projects-from-an-outline-or-a-csv.md` |
| 0077 | 0018 | `0018-the-agent-reaches-only-what-you-gave-it.md` |
| 0078 | 0019 | `0019-the-agent-brings-its-own-model.md` |
| 0079 | 0020 | `0020-merge-authority-follows-the-autonomy-level-you-set.md` |
| 0080 | 0021 | `0021-every-run-goes-to-a-team-and-automation-is-one-ladder.md` |
| 0081 | 0022 | `0022-a-whole-project-goes-in-and-out-through-one-file.md` |
| 0082 | 0023 | `0023-what-the-first-real-project-found.md` |
| 0083 | 0024 | `0024-limits-in-tokens-and-a-lean-context.md` |
| 0084 | 0025 | `0025-the-project-file-is-kept-in-sync-not-only-imported.md` |
| 0085 | 0026 | `0026-the-runner-finds-its-settings-and-a-card-says-what-checks-it.md` |
| 0086 | 0027 | `0027-whoever-makes-the-working-copy-prepares-it.md` |
| 0087 | 0028 | `0028-the-protocol-is-a-crate-and-the-sql-has-one-home.md` |
| 0088 | 0029 | `0029-one-test-binary-per-crate.md` |
| 0060 | archived | `docs/closed-cycles/adr/0060-docket-control-plane-disposition.md` |
| 0063 | archived | `docs/closed-cycles/adr/0063-harness-credential-modes.md` |
| 0065 | archived | `docs/closed-cycles/adr/0065-docket-pipeline-dispatch-trigger.md` |
| 0073 | archived | `docs/closed-cycles/adr/0073-installation-guidance-desktop-first-server-optional.md` |
