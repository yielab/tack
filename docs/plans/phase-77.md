# Phase 77: the agent's copy is prepared, and every run has a limit learnt from its kind

**Executes:** [ADR 0086](../adr/0086-whoever-makes-the-working-copy-prepares-it.md) (accepted
2026-10-19), Tack's side. docket's side is docket's ADR 0029 (D-65), planned and built by docket;
Tack builds against its provisional fields and sends them only when docket says it accepts them.

**Done when:** a Claude Code, Codex or opencode run of a project with a lockfile starts with its
dependencies installed, or fails before the agent with the install's output; a docket run makes no
runner checkout; every run shows a limit and where it comes from; a profile with 5 successful runs
offers a proposed limit; and Fabulada's code card passes its check through a team.

## Tasks

| Id | Task | Owns | Waits on |
|---|---|---|---|
| S | **The project's install command.** `tack_core::prepare::detect` (lockfile → command, ADR 0086 decision 3, pure); `projects.prepare_command` (migration 095: NULL detect, `''` off, text replaces); the project API and the project file (`repository.prepare`); the request snapshot carries what was resolved for the run; Settings → Automation → "Where the code is" and the Run dialog say "Prepares with `npm ci` (from package-lock.json)", "Install turned off" or "No lockfile: nothing is installed" | `tack-core` (new module), `tack-db` migration, project handlers + bundle, `tack-orch` repository snapshot, AutomationPanel's "Where the code is", Run dialog line | — |
| L | **Limits learnt per profile** (decision 5). Board setting `provisional_tokens_per_run`, default 2 000 000, empty = off (migration 096); `run_limits` falls back to it after the profile's, source `provisional`; a profile's history (last 20 successful runs, counted tokens: n, p50, p90, max, proposal = p90 × 1.5 rounded up to two significant figures, offered when n ≥ 5) in the profile API; the profile form's "Apply"; amber at 80% on a running run; a run stopped by its limit says "X of Y" and offers "Run again with more"; the limit sentence names "provisional" | `handlers/executions.rs::run_limits` and the limit sentence, `auto_start.rs` limits, `repo/metrics.rs` (one query), profile handlers + form, AutomationPanel's limit field, run detail | — |
| N | **No runner checkout for a docket run** (decision 2). For the docket harness the runner resolves the base commit in the project's repository without a worktree, takes docket's branch from that repository, computes evidence from base..head, pushes from it, and calls prill with `--repo <repository> --base --head` | `tack-runner`: `engine.rs` docket path, `git.rs`/workspace, `harness/docket.rs` (`bring_in`, `base_commit`), `evidence.rs`, `verify/prill.rs` | — |
| P | **The runner prepares the copy it makes** (decisions 1, 3, 4) for Claude Code, Codex and opencode: after the checkout and before the agent, run the snapshot's command with network; cache per (repository, lockfile sha256) under the runner's state dir, linked into the copy (`node_modules`, `.venv`; nothing for `cargo fetch`); a failure ends the attempt `failed` with reason `prepare` and the command's last output, no agent started | `tack-runner` engine (non-docket path), new `prepare.rs` | S, N |
| K | **docket gets the command.** Send the snapshot's command as the task's `prepare` when docket's contract says it accepts it; map `failureKind` `prepare` and `check_changed` (its paths) to the attempt's reason; the fake docket learns both | `harness/docket/task.rs`, outcome mapping, `tests/fake_docket/` | S, docket's D-65 cards |
| G | **Guides and changelog.** User guide (limits, provisional, proposal), Administrator guide (install command, cache, network), CHANGELOG `[Unreleased]`, roadmap | `docs/book/`, `CHANGELOG.md`, roadmap | S, L, N, P |

Waves: **1** = S, L, N (no shared type except S's new snapshot field, which N's tests may need to
fill); **2** = P, then K when docket's fields land; **3** = G. One gate, on the merged tree:
`.githooks/pre-push`.

## Rules every task carries

- Worktree from `develop`, never from `main`; `PATH=/home/ox/.cargo/bin:$PATH` and `rustc 1.98.1`;
  `CARGO_TARGET_DIR` under `/home/ox/tack-agent-targets/<id>`; `--build-jobs 4 --test-threads 4`.
- No GUI, no headed browser; never the live board, `~/.docket`, `~/.config/tack/runner.toml` or a
  `tack.db`.
- No `Co-Authored-By`; the change writes its own `CHANGELOG.md` line; code comments carry no phase,
  task or ADR numbers (`scripts/check-comments.sh`); the User and Administrator guides carry no
  source paths.
