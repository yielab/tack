# Architecture Decisions

This page is for contributors and integrators who want to know why Tack is built the way it is. It explains what an architecture decision record is here and lists every one in the repository. After reading it you can find the decision behind a behaviour and tell whether a later decision has replaced it.

## What an ADR is here

An architecture decision record (ADR) is a short document in [`docs/adr/`](https://github.com/yielab/tack/tree/develop/docs/adr) that asks the maintainer to decide something and keeps the answer. Each file is named `NNNN-slug.md` and numbered in the order it was written. Numbers are never reused, and a gap in the numbering is a decision that never needed a record.

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
| 0081 | [a whole project goes in and out through one file](https://github.com/yielab/tack/blob/develop/docs/adr/0081-a-whole-project-goes-in-and-out-through-one-file.md) | Accepted 2026-10-17 | — |
| 0080 | [every run goes to a long-lived team, and a project's automation is one ladder](https://github.com/yielab/tack/blob/develop/docs/adr/0080-every-run-goes-to-a-team-and-automation-is-one-ladder.md) | Accepted 2026-10-14 | — |
| 0079 | [merge authority follows the autonomy level you set, and Tack stays one of three tools that work alone](https://github.com/yielab/tack/blob/develop/docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md) | Accepted 2026-10-13 | — |
| 0078 | [the agent brings its own model](https://github.com/yielab/tack/blob/develop/docs/adr/0078-the-agent-brings-its-own-model.md) | Accepted 2026-10-13 | — |
| 0077 | [the agent reaches only what you gave it](https://github.com/yielab/tack/blob/develop/docs/adr/0077-the-agent-reaches-only-what-you-gave-it.md) | Accepted 2026-10-12 | — |
| 0076 | [a project and its items from one outline or CSV](https://github.com/yielab/tack/blob/develop/docs/adr/0076-projects-from-an-outline-or-a-csv.md) | Accepted 2026-10-12 | — |
| 0075 | [what the first docket + prill run found, and what Tack changes for it](https://github.com/yielab/tack/blob/develop/docs/adr/0075-what-the-docket-and-prill-run-found.md) | Accepted 2026-10-12 | 0078 |
| 0074 | [a run a normal person can start](https://github.com/yielab/tack/blob/develop/docs/adr/0074-a-run-a-normal-person-can-start.md) | Accepted 2026-10-07 | 0078 |
| 0073 | [installation guidance — desktop-first onboarding, server-optional path](https://github.com/yielab/tack/blob/develop/docs/adr/0073-installation-guidance-desktop-first-server-optional.md) | Withdrawn 2026-10-03 | — |
| 0072 | [agent onboarding — prerequisites discovery, validation, and configuration clarity](https://github.com/yielab/tack/blob/develop/docs/adr/0072-agent-onboarding-prerequisites-and-validation.md) | Accepted 2026-10-03 | — |
| 0071 | [every attempt leaves evidence; an independent verifier certifies it; the runner pushes, the board opens the pull request](https://github.com/yielab/tack/blob/develop/docs/adr/0071-evidence-merge-readiness-and-the-verifier-boundary.md) | Accepted 2026-10-03 | — |
| 0070 | [docket contract 1.1 is adopted — cancel becomes per-harness evidence, a recipe is one flag, the task goes by file](https://github.com/yielab/tack/blob/develop/docs/adr/0070-docket-harness-contract-1-1.md) | Accepted 2026-10-03 | — |
| 0069 | [the brief is a first-class entity on the item, typed, and it travels](https://github.com/yielab/tack/blob/develop/docs/adr/0069-briefing-script-on-the-item.md) | Accepted 2026-10-03 | — |
| 0068 | [Tack is maintained by people — retire what only agents needed, delete what nothing calls, and rebuild testing and CI around verified behaviour](https://github.com/yielab/tack/blob/develop/docs/adr/0068-a-codebase-for-human-maintainers.md) | Accepted 2026-09-18 | — |
| 0067 | [opencode is readmitted as a fourth harness, on the shared core, with its one limitation typed](https://github.com/yielab/tack/blob/develop/docs/adr/0067-opencode-readmitted-as-a-fourth-harness.md) | Accepted 2026-09-14 | 0078 |
| 0066 | [Docket becomes a third harness — the runner spawns it, and the model axis finally has a harness that proves it](https://github.com/yielab/tack/blob/develop/docs/adr/0066-docket-as-a-third-harness.md) | Accepted 2026-09-14 | — |
| 0065 | [The Docket pipeline-dispatch trigger gets a caller, its own token, and no claim on an item](https://github.com/yielab/tack/blob/develop/docs/adr/0065-docket-pipeline-dispatch-trigger.md) | Accepted 2026-09-08 | 0068 |
| 0064 | [Tests run under nextest, build once per resolution, and group one binary per subject](https://github.com/yielab/tack/blob/develop/docs/adr/0064-test-architecture.md) | Accepted 2026-09-05 | 0068 |
| 0063 | [A harness is credentialed one of two ways, and a provider is a module](https://github.com/yielab/tack/blob/develop/docs/adr/0063-harness-credential-modes.md) | Accepted 2026-09-05 | 0078 |
| 0062 | [Tack runs as a background service, and ships as a desktop app](https://github.com/yielab/tack/blob/develop/docs/adr/0062-desktop-app-and-background-service.md) | Accepted 2026-09-03 | — |
| 0061 | [Where a model provider's API key lives, and who's allowed to touch it](https://github.com/yielab/tack/blob/develop/docs/adr/0061-provider-credentials-at-the-runner-boundary.md) | Accepted 2026-09-03 | 0078 |
| 0060 | [The Docket control plane stays a maintained, optional legacy bridge](https://github.com/yielab/tack/blob/develop/docs/adr/0060-docket-control-plane-disposition.md) | Accepted | 0068 |
| 0059 | [Single-operator identity posture](https://github.com/yielab/tack/blob/develop/docs/adr/0059-single-operator-identity-posture.md) | Accepted | — |
| 0058 | [Standalone single-binary operation via an embedded runner](https://github.com/yielab/tack/blob/develop/docs/adr/0058-standalone-single-binary-runner.md) | Accepted | — |
| 0050 | [Tack-owned scheduling and runner-owned execution](https://github.com/yielab/tack/blob/develop/docs/adr/0050-runner-control-plane.md) | Accepted | 0068, 0078 |
| 0008 | [transactional migration rebuild recovery](https://github.com/yielab/tack/blob/develop/docs/adr/0008-transactional-migration-rebuild-recovery.md) | Accepted 2026-08-06 | — |

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

1. [ADR 0050, Tack-owned scheduling and runner-owned execution](https://github.com/yielab/tack/blob/develop/docs/adr/0050-runner-control-plane.md). Why the board schedules and the runner executes. Its decisions about models were replaced by ADR 0078; the rest stands.
2. [ADR 0058, Standalone operation via an embedded runner](https://github.com/yielab/tack/blob/develop/docs/adr/0058-standalone-single-binary-runner.md). Why one `tack` binary can run both halves on the same contract.
3. [ADR 0071, Evidence, merge readiness and the verifier boundary](https://github.com/yielab/tack/blob/develop/docs/adr/0071-evidence-merge-readiness-and-the-verifier-boundary.md). What every attempt leaves behind, who certifies it, and who pushes.
4. [ADR 0077, The agent reaches only what you gave it](https://github.com/yielab/tack/blob/develop/docs/adr/0077-the-agent-reaches-only-what-you-gave-it.md). The trust boundary: the board's token, the browser's origin, and what an agent can read.
5. [ADR 0079, Merge authority follows the autonomy level you set](https://github.com/yielab/tack/blob/develop/docs/adr/0079-merge-authority-follows-the-autonomy-level-you-set.md). The levels L3 to L5 and the signed verdict a merge needs.
6. [ADR 0080, Every run goes to a team and automation is one ladder](https://github.com/yielab/tack/blob/develop/docs/adr/0080-every-run-goes-to-a-team-and-automation-is-one-ladder.md). Teams, roles that choose profiles, starting by itself, the ceiling. [Runs, Teams and Automation](runs-and-teams.md) says how it is built.

After these, [ADR 0081](https://github.com/yielab/tack/blob/develop/docs/adr/0081-a-whole-project-goes-in-and-out-through-one-file.md) explains the project file, which [The Project File Format](project-file.md) documents.
