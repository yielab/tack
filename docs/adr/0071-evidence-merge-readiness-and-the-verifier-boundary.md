# ADR 0071: every attempt leaves evidence; an independent verifier certifies it; the runner pushes, the board opens the pull request

**Status: ACCEPTED 2026-10-03, with the amendment at the end.** Written for `docs/plans/phase-66.md` (B1, E1–E3,
F1, H1–H2, G1). B1 and E1 do not wait on it; F1, H1 and H2 do.

**Decide:** approve four things. **(1)** The runner captures, before it deletes a workspace,
the patch against the base commit, the head commit, the file list and a manifest
(`evidence-v1`), for every harness kind, and uploads them as artifacts. **(2)** Merge
readiness is judged by a separate program, working name Assay, that Tack never contains: the
runner may run it after an attempt, behind `[verify] enabled = false`, and stores its
Merge-Readiness Pack (`mrp-v1`) as one more artifact; the board renders it and records a
human accept or reject **with a reason** and the time it took. **(3)** The runner may commit
and push the attempt's branch to the remote it fetched from, behind `[git] push_branches =
false`, using the operator's own git credentials; the board then opens a pull request whose
body is the pack, best-effort, and learns its fate — merged, closed, reverted — from the poll
that already runs. **(4)** The row "GitHub push is best-effort, fire-and-forget; inbound sync
polls; no webhook receiver" stands and now covers pull requests.

**Why now:** an attempt ends with a log (`crates/tack-runner/src/engine.rs:627,698`;
`local_process.rs:758-790`). Nothing else survives the `cleanup`. The business plan's whole
argument is that the bottleneck is certifying code, not writing it (§0.1, §2.1), and that the
certifier must be independent of the writer and of the harness (§3.2). Tack is the only place
the human minutes can be measured (§2.1), and it measures none today.

**If you do nothing:** a verifier has nothing to verify, a reviewer has nothing to read but a
log, no metric of the verification tax can exist, and the result of the work — was it merged,
was it reverted — is never known to the board.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | **Evidence is captured by the engine, not by a harness.** In `run_claimed`, before each cleanup that follows a terminal outcome: stage everything but `.tack-runner/`, `git diff --cached --binary <base>`, `--name-status`, `rev-parse HEAD`; write `evidence.json`; stage the three through `ArtifactStager` from scratch, never inside the workspace; upload through the existing manifest path. Failure never changes the outcome. | "Workspace ownership belongs to the engine, not a harness adapter" (`engine.rs:274-276`). Four harnesses, one capture. |
| 2 | **No local branch survives.** The workspace is a throwaway `git init` + fetch (`git.rs:3-8`). The patch and the two SHAs are the durable evidence; a pushed branch (decision 5) is the durable branch. | A branch in a clone that is deleted is not a branch. |
| 3 | **`mrp-v1` is drafted by Tack as the consumer** and pinned with fixtures; Assay adopts it or supersedes it by version. The pack maps each brief criterion to `passed|failed|manual|skipped` with an evidence reference, carries verify / mutation / static-analysis / judge sections that are `null` when not run, a risk tier, a recommendation, the verifier's own token usage, and a signature slot. No dollar field. | Assay does not exist; Tack cannot wait for it to design the seam, and a fixture is the only thing two sides can test against. `null` ≠ passed is the anti-tautology rule applied to the pack itself. |
| 4 | **The verifier runs in the runner, after the attempt, off by default.** `[verify] { enabled, program, args, timeout_seconds }` in the runner's TOML; the command receives the evidence directory, the still-present workspace and an output path; the result is parsed through `tack_core::mrp` and staged as `application/vnd.tack.mrp+json`. A clean re-checkout is the verifier's own job. No environment variable is added. | The board never executes code; the runner already runs near the code and credentials (`CLAUDE.md`). Off by default because it spends tokens. |
| 5 | **The runner pushes; the board opens.** `[git] { push_branches = false, branch_prefix, author }`; on a succeeded attempt with a non-empty patch, `checkout -b`, commit if dirty, `push origin`, all through the existing `git_ok`; the remote is the one `provision` fetched from, so the operator's credential helper or agent answers — **Tack holds no git credential**. The report carries `{branch, head_commit, pushed}`. The board, on that completion and for an item with a `github_links` row, opens the pull request with the token it already holds for issues, body = the rendered pack. | The split keeps each credential where it already is: git's with git on the runner, GitHub's API token on the board. Opening a PR is an HTTP call, not code execution. |
| 6 | **Pull-request state comes from the existing poll.** The `/issues?state=all&since=` response already lists pull requests with a `pull_request` key and `merged_at`; the poll updates `pull_requests.state`; a PR titled `Revert …` citing `#n` marks `n` reverted. | Zero new calls per tick; the "no webhook" row stands. |
| 7 | **A review is a record**: `mrp_reviews(attempt_id, artifact_id, verdict, reason NOT NULL, viewed_at, reviewed_at, reviewed_by)`, one per attempt, reason required. The UI reports `viewed_at` once; human minutes are `reviewed_at − coalesce(viewed_at, artifact.created_at)`, and the metric says which start it used. | The plan's north metric needs measured minutes, not estimates (§2.1). A reason is the input to the future corrections ledger. |
| 8 | **`status_map_policy_id` gains `done_on_mrp_accepted`** beside `done_on_success` (both defined in I1); an accepted pack may move the item; a rejected one never does. | The item's status is the board's truth; the pack is evidence for it. |

## Evidence

| Fact | Locator (read at `7718420`) |
|---|---|
| Workspace deleted after the terminal report; only the log staged; the log is written inside `<workspace>/.tack-runner/` then copied out | `crates/tack-runner/src/engine.rs:627`, `:698`, `:1417`; `harness/local_process.rs:758-790` |
| `submit_terminal_evidence` uploads exactly one `artifact` key | `engine.rs:942-957`, `:1071-1157` |
| The artifact pipeline accepts `text/x-diff` with `base_revision` metadata | `docs/contracts/runner-v1/artifact.request.json` |
| The workspace is a fresh `git init` plus fetch, refused if the root has `.git`; cleanup deletes only a marked child | `crates/tack-runner/src/git.rs:3-8`; `workspace.rs:178-215` |
| Git runs through one helper with redaction and a timeout | `git.rs:84-150` (`git`, `git_ok`) |
| Runner config is TOML → environment → CLI, `deny_unknown_fields` per table | `crates/tack-runner/src/config.rs:119-160`; `docs/CONFIG.md:52-66` |
| GitHub: issues and comments both ways by push and poll; no PRs | `crates/tack-api/src/github_sync.rs:67,109,192,227,365`; `docs/GITHUB-SYNC.md:12-30,111-116` |
| The poll moves items through `update_item_atomically` and never calls the outbound push | `github_sync.rs:10-14`, `:312-345` |
| `webhook.rs` is an outbound client, not a receiver | `crates/tack-api/src/webhook.rs:7-15` |
| The completion report's `actual_execution` accepts additive fields | `crates/tack-orch/src/execution/types.rs` (`additional`, flatten); `runner-v1/README.md` |
| Decision timestamps exist; no `viewed_at` anywhere | `crates/tack-db/src/migrations.rs:1523-1541` |
| Attempt usage is measured tokens with provenance; `cost_usd` is `not_measured` unless a harness reports it | `docs/contracts/runner-v1/completion.request.json` (`usage`) |
| The business case: verification tax, measured minutes, independence of the verifier | `../rack-cli/internal-docs/plan-fabrica-agentica-l3-l5-2026-09-29.es.md` §0, §2.1, §3.2, §3.3 |
| docket agrees: it keeps raw evidence and never scores itself | `../rack-cli/docs/adr/0017-…md` §4–5 |

## What this reverses or amends

| Earlier decision | Now |
|---|---|
| "GitHub push is best-effort and fire-and-forget; Tack-initiated last write wins on the way out" (`docs/GITHUB-SYNC.md`) | **Kept, extended** to opening pull requests (decision 5). |
| "Inbound GitHub sync polls; there is no webhook receiver" (phase-65 decision) | **Kept**; the poll learns PR state (decision 6). |
| "No new `TACK_*` variable for a harness" | **Kept**: `[verify]` and `[git]` are TOML tables with no variable. |
| ADR 0061: provider credentials at the runner boundary | **Consistent**: no credential is added; git's stays git's. |
| Only the run log is staged (Part IX, `stage_run_log`) | **Amended** (decision 1): four artifacts on success. Not a Decided row. |

## The risk this accepts

**The runner pushes with the operator's credentials.** A harness that has been prompt-injected
cannot push on its own — the push is the engine's, after the harness exited, to a branch under
`tack/` — but what it pushes is whatever the harness wrote. Off by default; the pull request
is the review gate; nothing here merges. Whether the capability may exist at all is question
2 of the plan, and this ADR is not accepted without an answer.

**The verifier is trusted to run in the runner's context.** It sees the workspace and the
evidence, with `PATH` only and the attempt's redaction set. Until it exists it is a fake script
in tests. Its own sandboxing is its own design (business plan Phase 2).

## Cut and deferred

| Item | Status | Why |
|---|---|---|
| A pull-request *check* carrying the pack | Deferred | Trigger: a signed pack (`signature.kind = in-toto`) that a check can verify. |
| Automatic merge | Parked (plan) | Needs the ODD view and its own ADR. |
| Dollar ingestion from provider usage APIs | Parked (plan) | Never an estimate meanwhile. |
| A review category in workflows | Parked (plan) | Trigger: `done_on_success` proves wrong on a real board. |

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-10-03 — accepted by the maintainer.** The runner may push a branch with the operator's
own git credentials, off by default ("The risk this accepts" is accepted). One amendment, from
ADR 0072's acceptance the same day: decision 4 and decision 5 gain a per-run side. The
runner's TOML still decides whether a verifier or a push exists at all, and the board still
executes nothing; the runner reports `verify_configured` and `push_configured` in its
capabilities, the "Run with agent" dialog shows both as controls — disabled, with the reason,
when the runner does not report them — and a request may decline either for one run. A
request can never turn on what the TOML has off. Task P2 of the plan.
