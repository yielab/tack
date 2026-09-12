# III-H3 handoff

**What this card changes, in plain language.** Before it, a task could be
assigned to a runner and then never start: the runner had no way to make a
working copy of the repository the task was about, so the coding tool it was
supposed to drive never got a directory to work in. Now every claimed task
gets its own private checkout of the exact commit the request named, the
coding tool starts inside it, and the checkout is deleted when the task ends —
so two tasks running at once cannot see or overwrite each other's files, and a
runner that is killed mid-way never leaves a half-made copy that a later task
could inherit.

- **Base SHA / branch / final SHA:** base `45ccafb` (the merge of III-H1 onto
  `agent/iii-f6-integration`, which the Wave 7 board names as the current
  line), branch `agent/iii-h3-repository-checkout`. Final SHA: uncommitted at
  the time of writing — the working tree is the deliverable; no commit was
  made because none was requested.
- **Files changed:**
  - `crates/tack-runner/src/git.rs` — **new**, the whole card:
    `GitWorktreeProvisioner`, the real `WorktreeProvisioner`.
  - `crates/tack-runner/src/workspace.rs` — owned. Declares the `git` module
    and adds five typed `WorkspaceError` variants (`GitUnavailable`, `Git`,
    `GitTimeout`, `RepositoryUnreachable`, `RevisionUnavailable`).
  - `crates/tack-runner/tests/h3_checkout.rs` — **new**. Engine-level proof
    against a real repository and a real child process.
  - **`crates/tack-runner/src/main.rs` — outside `Owns`.** One import and one
    constructor argument: the daemon now builds `GitWorktreeProvisioner`
    instead of `UnavailableWorktreeProvisioner`. Without it the card would
    ship code the product never runs; III-H1's own comment at that line
    names this card as the change. Flagged, not hidden.
  - **`crates/tack-runner/src/engine.rs` — outside `Owns`.** Nine lines in
    `recover()`. See "Schema/API/contract change requested" below.
- **Contract fixtures consumed:** `claim.response.json` (the `request.repository`
  shape — `kind`, `remote`, `base_revision`, `subdirectory` — and the whole
  request/attempt snapshot, reused verbatim in the integration test) and
  `recovery-observation.response.json`. No fixture was edited, so the
  `runner_contract.rs` pin table is untouched (18/18 still green).

## Behavior implemented

`GitWorktreeProvisioner` provisions one isolated checkout per attempt:

1. Re-proves ownership of the attempt directory — a real directory, not a
   symlink, carrying a `.tack-attempt` marker equal to this attempt's id.
   Independent of `WorkspaceManager`'s own guard on purpose, because this code
   deletes files.
2. Reuses an existing checkout only when three independent facts agree: the
   `.tack-checkout` sentinel, a live `.git`, and the commit `HEAD` actually
   points at. Otherwise it purges every entry except the attempt marker.
3. `git init` in place → `git remote add origin <remote>` → `git fetch
   --no-tags --depth 1 origin <revision>`, falling back to a full fetch when
   the remote refuses a by-commit fetch (most local and dumb remotes do) →
   `git checkout --detach <resolved>`.
4. Writes and fsyncs `.tack-checkout` with the resolved commit — the only
   evidence that a checkout is complete.

Decisions worth recording:

1. **A private clone, not `git worktree add`.** The trait's name is about the
   product requirement (an isolated working tree per attempt), not about that
   subcommand. `git worktree add` keeps administrative state inside one shared
   repository — two attempts contend on its index lock, and a runner killed
   mid-add leaves a registered-but-absent worktree that a later attempt
   inherits, which is precisely what the acceptance forbids. It also refuses a
   non-empty target directory, and every attempt directory already holds the
   `.tack-attempt` marker `WorkspaceManager` writes before provisioning. The
   full argument is in the module doc comment.
2. **The sentinel is the completion boundary.** A checkout is thousands of
   files and cannot be made atomic. A kill therefore leaves a directory that
   looks like a checkout; the sentinel, written last and fsynced, is what
   distinguishes "finished" from "looks finished".
3. **Restart reuses, it does not repair.** A partial checkout has no
   trustworthy state to repair from, so it is discarded whole. A *complete*
   checkout of the same revision is reused, which is what makes a restart
   cheap and preserves harness work in progress.
4. **A full commit id is enforced, not trusted.** If the request named a
   40-hex commit, `HEAD` must equal it after checkout, or the provision fails
   with `RevisionUnavailable`. Otherwise the attempt could run against code
   nobody asked for while reporting the requested `base_revision` upstream.
5. **Five distinguishable failures, none carrying a remote.** "git is not
   installed", "the remote is unreachable", "that revision does not exist",
   "git hung", and "a git command failed" send an operator to five different
   places. None of the messages contains a URL, a path, or git's own text.
6. **The ambient git configuration is inherited; repository-selecting state is
   not.** runner-v1 has no channel for repository credentials, so a
   runner-local `~/.gitconfig`, credential helper or SSH agent is the only way
   a private remote can work at all. `GIT_DIR`, `GIT_WORK_TREE`,
   `GIT_INDEX_FILE`, `GIT_COMMON_DIR`, `GIT_OBJECT_DIRECTORY`,
   `GIT_ALTERNATE_OBJECT_DIRECTORIES` and `GIT_CEILING_DIRECTORIES` are
   removed: a runner started from inside a repository, or under a hook, would
   otherwise silently operate on *that* repository. `GIT_TERMINAL_PROMPT=0`
   plus a wall-clock timeout with `kill_on_drop` means a credential prompt
   fails instead of hanging forever.
7. **Recovery cleanup is conditional.** A settled recovery removes the
   checkout; a quarantined or still-pending one keeps it, because it is the
   evidence an operator will be asked to look at.

## Tests added and exact commands/results

`cargo test -p tack-runner --lib git` — **21 passed, 0 failed** (unit, real
`git`, no network: every remote is a local repository or a deliberately
unroutable address).

`cargo test -p tack-runner --test h3_checkout` — **6 passed, 0 failed**
(engine-level, real repository, real child process).

| Gate | Result |
|---|---|
| `cargo test --workspace` | **1363 passed / 0 failed** (was 1337 at `45ccafb`; +26 this card) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean, exit 0 |
| `cargo fmt --all -- --check` | clean |
| `cargo test -p tack-orch --test runner_contract` | 18/18 — all 46 fixtures still byte-pinned |
| `cargo test -p tack-api --test openapi_contract` | 5/5, drift-free |
| `cargo test -p tack-api --test wave2_gate` | 5/5 |
| `./scripts/smoke.sh` | **SMOKE PASSED — 2/3 harnesses**, unmodified (`claude` 2.1.235, `opencode` 1.18.0; `codex` absent) |

### Live evidence — a real attempt, a real checkout, a real harness

`scripts/smoke.sh` "reaching step 7" proves little on its own: **steps 7–9 are
unimplemented stubs that print `SKIPPED` unconditionally**, and the file
belongs to III-H2, so this card did not touch it. The acceptance was therefore
collected by hand against a live `tack serve` + `tack-runner`
(`h3_live.sh`, kept in the session scratchpad, not committed):

- Execution request created through `POST /api/executions` with
  `repository_snapshot` naming a real local git repository at commit
  `7889f2ae…`, `selector_kind: exact_runner`, harness `opencode`, model
  `llamacpp/qwen3.6-35b-uncensored`.
- `GET /api/executions/{id}/attempts` returned attempt
  `att_5b2f0f73-…`, **`state: "running"`**, `fencing_token: 1`,
  `workspace_id: "ws_6174745f3562326630…"`, `base_revision: "7889f2ae…"`,
  `started_at` set — i.e. the harness process really started.
- On disk at that moment: `workspaces/<hex attempt id>/` mode **`700`**,
  containing `.git`, `PLAN.md` (the repository's content), `.tack-attempt`
  (mode 600) and `.tack-checkout` (mode 600). `git rev-parse HEAD` inside it
  returned `7889f2ae…` — the exact requested commit.

Two earlier runs of the same script are the negative controls: with
`agent_profile_id` naming a profile that does not exist the request is refused
(`internal_error`, FK), and with a runner that has not yet enrolled the
selector is refused (`runner_revoked`) — so the successful run is not an
artifact of the server accepting anything.

## Failure/adversarial case proved

- **The engine-level test is load-bearing in both directions.**
  `a_claimed_attempt_reaches_a_real_harness_process_with_its_own_checkout`
  asserts the child process read the repository's file content; its twin
  `without_a_real_provisioner_the_same_attempt_never_reaches_the_harness`
  runs the identical claim against `UnavailableWorktreeProvisioner` (the
  pre-III-H3 production wiring) and asserts the harness observed **nothing**.
  Reverting this card is therefore proven to break the claim.
- **A real kill mid-provision, not a simulation.**
  `a_runner_killed_mid_provision_leaves_nothing_a_restart_inherits` gives git
  a one-microsecond budget so the timeout fires and `kill_on_drop` SIGKILLs it
  mid-work, then asserts no sentinel was written and that the restart produces
  a correct checkout. `a_hanging_git_is_killed_and_reported_as_a_timeout`
  proves the kill path deterministically with a stand-in `git` that never
  returns.
- **Absence asserted directly, not via a return code.**
  `a_directory_marked_for_another_attempt_is_refused_and_untouched` asserts
  the other attempt's file still exists *and* that the directory still has
  exactly two entries — the refusal created and deleted nothing.
  `a_revision_that_does_not_exist_is_typed_and_writes_no_sentinel` asserts
  `.git` itself is gone after the failure.
- **A torn sentinel is not believed.**
  `a_sentinel_that_disagrees_with_head_is_not_trusted` rewrites the sentinel
  to a different commit and asserts the next provision rebuilds from scratch.
- **Concurrency.** `two_concurrent_attempts_cannot_see_each_others_files`
  provisions two attempts with `tokio::join!` at two different commits and
  asserts distinct paths, distinct `.git` directories, and that a file written
  into one is invisible from the other.
- **Rule 12.** `a_credential_in_the_remote_url_never_reaches_a_log_line`
  installs a capturing subscriber, provisions from
  `https://tack-user:canary@…?token=canary`, and asserts the log contains the
  failure but neither the password nor the user. It also asserts the failure
  *was* logged, so the test cannot pass by capturing nothing.

## Schema/API/contract change requested from another owner

1. **`crates/tack-runner/src/engine.rs` — nine lines in `recover()`, already
   applied, outside `Owns`, flagged rather than hidden.** The card's Tasks
   require cleanup "on completion, cancellation **and crash-recovery**". The
   first two call sites existed; the third did not — `recover()` cleaned up
   only after a *terminal replay*, so a checkout left by a killed runner
   survived every subsequent restart forever, because nothing ever revisited
   that attempt's directory again. The change cleans up when a recovery
   settles (`RunCycle::Completed`) and deliberately does **not** when the
   attempt is quarantined or still pending. Both halves are pinned by
   `a_checkout_left_by_a_killed_runner_is_removed_by_the_restart` and
   `a_quarantined_attempt_keeps_its_checkout_as_evidence`; the first fails
   without the change (observed, not assumed). Revert and re-route it if the
   owner prefers, but the leak is real.
2. **`crates/tack-runner/src/main.rs` — the provisioner wiring**, as listed
   above. Accept or re-route.
3. **`harness::claude_code::tests::discover_installed_binary_fails_typed_when_path_has_no_claude_executable`
   mutates `PATH` process-wide** while other tests run. Its comment claims the
   assertion is "single-threaded within this process", which is not true — the
   Rust test harness runs tests on many threads, so any concurrently-running
   test that resolves a bare program name can observe the empty `PATH`. This
   surfaced as a one-in-many-runs `GitUnavailable` failure of
   `a_checkout_of_a_different_revision_is_never_reused` under
   `cargo test --workspace`, reproduced and diagnosed rather than retried
   away. This card worked around it locally (its tests resolve `git` to an
   absolute path) but did not edit that test — it belongs to III-D2's file.
   The hazard remains for any future test that spawns a program by name.
4. **`docs/CONFIG.md` (III-G3's file): the git program path and the
   provisioning timeout are constructor parameters with defaults (`git`,
   600 s) and no `TACK_RUNNER_*` variable.** Adding one means adding a row to
   that table, which this card does not own. Requested, not invented.

## Known limitations or `not_measured` fields

- **Every attempt fetches from the remote; there is no shared object cache.**
  Correct and isolated, but a large repository is re-fetched per attempt. A
  local mirror with `--reference` is the obvious follow-up and was left out
  deliberately: it reintroduces shared mutable state, which is exactly what
  the crash-safety acceptance is about.
- **`repository.subdirectory` is ignored.** `RepositorySpec` (in `client.rs`,
  not owned here) carries only `remote` and `base_revision`, so the
  contract's `subdirectory` field never reaches the provisioner. A request
  setting it gets the repository root. Widening `RepositorySpec` is the
  owner's call; this card did not silently pretend to honor the field.
- **`repository.kind` is likewise not visible to the provisioner** — the same
  `RepositorySpec` narrowing. Every claim today is `kind: "git"`; a future
  non-git kind would be checked out as git rather than typed as unsupported.
  Named here because "unsupported is typed" is a rule, and this is the one
  place the type system cannot currently enforce it.
- **Files inside the checkout keep git's own modes (`0644`/`0755`).** The
  owner-only guarantee is the containing directory (`0700`, asserted), plus
  `0600` on both marker files. Forcing `0600` on every checked-out file would
  strip the executable bit from every script in the repository and break the
  harness it is meant to serve.
- **Private remotes work only through runner-local git configuration.** There
  is no credential channel in runner-v1; a remote needing authentication fails
  with `RepositoryUnreachable` unless the runner's own git can already reach
  it.
- **Observed, not fixed, during the live run:** the *request* stayed at
  `state: "leased"` while its *attempt* advanced to `running`. Server-side
  request-state propagation, outside this card; noted for III-H2 so it is not
  mistaken for a runner defect.
- **Orphaned harness children.** Killing the runner during the live run left
  two `opencode` processes behind (cleaned up by hand). That is the already
  documented `PROCESS_GROUP_CANCEL_CEILING` advisory limitation, not new.

## Secrets/logging review

No credential, prompt body, environment value or query string reaches a log
line or a typed error. Git's stderr is treated as tainted and passes through
`SecretMaterial` (seeded with the remote, its userinfo, user and password) and
`redact_query` before it can be logged; argument lists are never logged,
because `remote add` carries the URL verbatim. The five typed errors carry
static messages only. Asserted by
`a_credential_in_the_remote_url_never_reaches_a_log_line` and
`redacted_output_strips_userinfo_and_query_strings`.

## Safe merge order and likely conflicts

Merge before III-H2 — H2's steps 7–9 cannot produce evidence without this.
Conflicts are unlikely: `git.rs` and `h3_checkout.rs` are new; `workspace.rs`
is untouched by any other Wave 7 card; the `main.rs` and `engine.rs` edits are
small and localized, and both are listed above for the integrator to accept or
re-route.

**Proposed status board row (Wave 7):** III-H1 done (`984bb5f`), **III-H3 done
— a claimed attempt now gets a real, isolated git checkout and the harness
starts inside it, proven live (attempt `running`, `HEAD` = the requested
commit, workspace `0700`) and by 27 new tests against real git.** 1363
workspace tests / 0 failed, clippy and fmt clean, `runner_contract` 18/18,
`openapi_contract` 5/5, `wave2_gate` 5/5, `smoke.sh` unmodified and passing
2/3 harnesses. H3 also found and fixed a recovery-path leak in `engine.rs`
(a checkout left by a killed runner survived every restart) and escalated a
`PATH`-mutating test in `harness/claude_code.rs` that makes any concurrent
test spawning a program by name intermittently fail. **III-H2 is now
unblocked**, but its own steps 7–9 are still unimplemented stubs — "reaching
step 7" is not yet evidence of anything.

## Checklist

- No unowned file edited without being named above (two: `main.rs`,
  `engine.rs`).
- No live secret; no network in any CI test.
- No panic stub, no `unimplemented!()`, no fake success.
- No blind retry: the narrow fetch's failure is expected and falls back once;
  nothing is resent after an ambiguous state.
