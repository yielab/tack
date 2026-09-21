# Plan: Phase 65 — the release, and every open item in one sequence

Everything Tack still owes, from the roadmap, the ADRs and the harness plan, as one ordered
list of tasks. Nothing pending lives anywhere else: the roadmap points here, the harness
plan's "Upgrades" table is folded in, and each ADR's deferred item is either a task below,
a line under "Decided" or a line under "Parked". When a task lands, its row in the status
table at the bottom changes; when the phase lands, this file moves to
`docs/closed-cycles/plans/`.

Every number carries its command. Re-run it before quoting it.

## Decided — not re-opened by any task

These were settled by the user, in the ADR or plan named. A brief never asks about them; a
task that seems to need one of them reversed stops and reports.

| Decided | Where |
|---|---|
| A run can pause and ask on every harness; Tack builds it once in the core, each harness uses it when its CLI offers a stdio way, and declares `decisions: unsupported` with a measured reason until then. Only a stdio channel fits the seam; an HTTP one is refused until a second harness needs it. | ADR 0068 amendment 2026-09-18; ADR 0066 amendment |
| DAG-ordered sprint dispatch is gone and comes back only on evidence of use. | ADR 0068 amendment 2026-09-18 |
| Desktop bundles ship unsigned; signing is a separate decision with money attached. | ADR 0062 §7 |
| Windows has no background service: the desktop app supervises the server there. | ADR 0062 |
| One operator, one identity. No multi-user, no SSO. | ADR 0059 |
| `tack.toml` never reads the `--with-runner` gate; the gate belongs to `tack-cli`. | ADR 0058 |
| A provider is one module under `crates/tack-runner/src/provider/`; today `anthropic` and `vercel_ai_gateway`. | ADR 0063, amendment 2026-09-19 |
| No plugin seam, no descriptors from a file, no grammar DSL, no remote docket, no docket pods, no import from `tack_orch::adapters`, no new `TACK_*` variable for a harness, no copied docket source or fixtures. | harness plan, "Refused, by name" (archived) |
| GitHub push is best-effort and fire-and-forget; Tack-initiated last write wins on the way out. | `docs/GITHUB-SYNC.md` |
| `main`'s ruleset is updated when `develop` is released — that moment is now. | phase-64 plan, "Decisions taken" |
| Sonnet for every task except measurement and review; two agents building at once; 150 tool calls each; one gate, once. | phase-64 plan, "Limits" (repeated below) |

Two decisions this plan takes because no task can start without them. Each is one line to
reverse:

- **Inbound GitHub sync polls; there is no webhook receiver.** Tack listens on
  `127.0.0.1:3210` by default and the desktop app never exposes it, so GitHub cannot
  deliver to it. A poll with the token that push already uses reaches every install. A
  webhook route is refused until an install that is reachable from the internet asks.
- **opencode attempts share one package cache and nothing else.** Each attempt keeps its
  own `HOME` and config directory (ADR 0067 decision 4); only the npm cache that the
  `@ai-sdk/openai-compatible` install fills moves to one directory under the runner's
  state dir. Attempts see each other's downloaded packages and none of each other's
  sessions.

## Wave 0 — the user, no agent

Each of these was refused to an agent by the permission classifier, costs money, or needs
a machine this one is not. They are listed once, here, with the command.

| # | What | Command |
|---|---|---|
| 0.1 | Required checks on ruleset `17857665` (`main` and `develop`) become the six pull-request jobs; the ten old names block every pull request today, Dependabot's included. | In the ruleset's "Require status checks" list, replace the entries with: `Rust (fmt + clippy + drift gates)`, `cargo-deny (dependency policy)`, `Rust tests + coverage (llvm-cov)`, `Docs (mdBook build)`, `Frontend (typecheck + Vitest + build)`, `Security (dependency audit)` |
| 0.2 | Delete the nine merged remote branches and the local leftovers. | `git push origin --delete agent/deps-actions-major agent/deps-npm-major agent/iii-f6-integration agent/vi-c32-boot-backstops agent/vi-c33-tracing-init-once agent/vi-c34-spa-fallback-scope agent/vi-c35-deny-agreement agent/vii-c1-release-bundles plan/harness-agnostic-agent-fleet && git branch -d harness-seam-redesign && git update-ref -d refs/remotes/local/develop` |
| 0.3 | Remove two stale build directories, about 16 GB. | `rm -rf /var/tmp/tack-agent-targets/integrate /var/tmp/tack-measure/claude-ask` |
| 0.4 | Dependabot alert 21: `glib` 0.18.5 in the desktop lockfile, pinned by the GTK3 `wry` line, no upgrade path, same class as the `deny.toml` exceptions. Dismiss or keep. | `gh api -X PATCH repos/yielab/tack/dependabot/alerts/21 -f state=dismissed -f dismissed_reason=tolerable_risk` |
| 0.5 | The repository description names four harnesses. | `gh repo edit yielab/tack --description "..."` |
| 0.6 | Walk `docs/LAUNCH-CHECKLIST.md` and tag `v0.1.0-beta.8` (`grep '^version' Cargo.toml` already says so). | `git tag v0.1.0-beta.8 && git push origin v0.1.0-beta.8` |
| 0.7 | The publish list in the checklist: seven issues, four posts, two Discussions. | as written there |
| 0.8 | Install on a macOS and a Windows machine once, following `README.md`. | `curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh \| sh`, and the `.dmg` / `.msi` from the tag above |
| 0.9 | Code signing (ADR 0062 §7): an Apple developer account and a Windows certificate. Until decided, nothing is scheduled. | — |

Waves 1–4 do not wait for Wave 0. What they build ships as `v0.1.0-beta.9`.

## The waves

A wave is a set of tasks with no file in common. A task starts when the tasks it names are
in `develop`.

| Wave | Tasks, in parallel | Starts after |
|---|---|---|
| **1** | **M1** measure codex · **M2** measure opencode · **M3** read docket's contract · **C1** `tack start` · **G1** GitHub inbound state | now |
| **2** | **U1** capture cap out of the descriptor · **U4** opencode served model · **G2** GitHub comments | U1: now · U4: M2 · G2: G1 |
| **3** | **U2** codex usage and served model → **U3** codex permission policy (serial) · **U5** opencode shared install · **G3** per-project token and manual link | U2: M1, U1 · U5: U4 · G3: G2 |
| **4** | **U6** codex asks · **U7** opencode asks · **U8** docket cancel, artifacts, asks | U6: U3, M1 · U7: U5, M2 · U8: M3 says docket's `harness-v1.1` shipped |

```
M1 ──────────────► U2 ► U3 ► U6
M2 ──► U4 ► U5 ► U7
M3 ──────────────────────► U8   (only when docket ships harness-v1.1)
U1 ──► U2
C1
G1 ► G2 ► G3
```

Why this order: M1–M3 run the real CLIs and write fixtures, no product code, so they run
first and alone. U1 touches `local_process.rs` and one line in each harness file, so it goes
before any harness task. U2, U3 and U6 all edit `codex.rs`; U4, U5 and U7 all edit
`opencode.rs`; both chains are serial. G1–G3 all edit `github_sync.rs`, the migrations and
the generated API files, so they are serial with each other and independent of the runner.
C1 edits `tack-cli` only.

## How a task is handed to an agent

One agent, one task, one branch from `develop` named after the task (`u2-codex-usage`).
The brief is the task's section of this file plus these lines, and nothing else — the agent
reads `CLAUDE.md` on its own:

```
Task <id> from docs/plans/phase-65.md. Read that section and the files it lists, nothing wider.
Touch only the files the task lists. A change needed outside them is a finding: stop and report it.
Add only the tests the task names. Prefer a row in an existing table test over a new function.
Do not add a type, trait, option, flag or helper the task does not name.
Never call a real model endpoint: every run goes to a loopback fake, zero spend.
Never run a CLI against the operator's own HOME or config; use a scratch HOME.
Finish by running .githooks/pre-push once and reporting its real output. Do not commit.
Report: what changed, per file, in one line each; what you measured; what you could not do.
```

Limits, imposed from outside the prompt:

- **Model:** Sonnet for every task. M1–M3 are Sonnet too; the launching session reads what
  they captured and decides what it means before U2–U8 are dispatched.
- **One gate, once.** `.githooks/pre-push` at the end. While working, only the test binary
  being changed: `cargo nextest run --workspace -E 'binary(<name>)'`.
- **150 tool calls per agent.** One that reaches it reports where it stopped and is replaced
  by a fresh agent with a short brief, never resumed with a large context.
- **At most two agents building at once**, `--build-jobs 4`, `--test-threads 4`. Sized on
  2026-09-18 (`nproc` 16, `free -g`, `df -h /`); re-measure before raising.
- **`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/<task>`**, removed when the branch merges.
  `renice` is applied by the launcher's loop, never by the prompt. No agent opens a GUI.
  The renice loop skews any latency number an agent measures; benchmarks run from the
  launching session with the loop stopped.
- **Secrets:** any process that opens the secret store runs with
  `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`, so it can never
  touch the operator's keychain (`scripts/smoke.sh` shows the line).
- **Review before merge** is done by the launching session against the task's *done when*,
  reading the diff. Merge is `git merge --no-ff` into `develop`; the gate runs once more on
  the merge; then push.

## The tasks

### M1 — measure codex

**Files written:** `crates/tack-runner/src/harness/fixtures/codex/<version>/` (new
captures, each with a provenance line in the README) and nothing else.

Against the installed `codex` (`codex --version`), a scratch `HOME`, and
`OPENAI_BASE_URL` pointed at a loopback stand-in for the Responses wire (the wiremock
pattern in `harness/claude_code/tests.rs` shows the shape), capture three things:

1. **`codex exec --json`** on a prompt that makes one tool call: the full stdout, so that
   the lines carrying token usage and the served model id, if any, are on disk.
2. **`codex exec --help`** and one run per flag among `--sandbox`, `--ask-for-approval`,
   `--full-auto` (or whatever the installed version names them): which flag makes a tool
   call proceed without a prompt, which denies, which asks.
3. **`codex app-server`**: whether an approval request appears on stdout as one line and
   whether an answer on stdin releases it. Capture the request line and the answer line
   verbatim.

**Done when:** the three captures are on disk with their commands, and the report states,
in one line each, whether usage is in the output, whether the served model is, which flags
map to `auto`/`ask`/deny, and whether `app-server` asks over stdio.

### M2 — measure opencode

**Files written:** `crates/tack-runner/src/harness/fixtures/opencode/<version>/` and
nothing else.

Against the installed `opencode` (`opencode --version`), a scratch `HOME`, and a loopback
fake for `/v1/chat/completions` (the one in `harness/opencode/tests.rs`), capture:

1. **Served model:** the event stream of one completed run; report which event or export
   field, if any, names the model that answered rather than the one requested.
2. **Asking:** `opencode run` with the permission for `bash` set to `ask` in the config
   file; then `opencode acp` with the same, over stdio. Capture what appears on stdout when
   the tool call needs an answer and whether a line on stdin releases it.
3. **Shared install:** `du -sh` of the attempt `HOME` after one run, and which
   subdirectory holds the npm cache and the installed package. Then a second run with the
   npm cache directory pointed at the first run's (`npm_config_cache`), `du -sh` again,
   and whether the second run reached the network at all (the fake server's request log,
   plus `strace -f -e trace=connect` if installed).

**Done when:** the captures are on disk with their commands; the report states whether a
served model exists, whether either mode asks over stdio, the bytes per attempt before and
after a shared cache, and whether a warm cache makes a run network-free.

### M3 — read docket's contract

**Files written:** none. Read only; nothing in `../rack-cli` is edited or committed.

Report from `git -C ../rack-cli log --oneline -10 -- docs/contracts src/docket/core/harness.py`
and `ls ../rack-cli/docs/contracts/` whether a `harness-v1.1` exists and, if so, whether it
carries: an event per child process group started (what `cancel: Supported` needs), a
question event on stdout answered on stdin (what `decisions: Supported` needs), a path list
in the result for files its tools wrote (what `artifacts: Supported` needs), and the token
on stderr or a `--token-file`. Then run `docket harness run` once under `env -i` against
the loopback fake in `harness/docket/tests.rs` to confirm the installed binary matches.

**Done when:** one line per item above, each `present` or `absent`, with the commit that
added it where present. U8 starts only on four `present`.

### C1 — `tack start <id>`, and `tack open <id>`

**Files:** `crates/tack-cli/src/main.rs` (one `Start` and one `Open` subcommand beside
`Branch`, and `cmd_start`, `cmd_open` beside `cmd_branch`); `crates/tack-cli/src/git.rs`
only if `cmd_branch`'s checkout becomes a shared function; `crates/tack-cli/tests/cli_test.rs`
(rows in the existing wiremock table); `docs/book/src/user-guide/cli.md`.

`tack open <id>` prints the item's web URL and opens it with `$BROWSER` when set, else
prints only. `tack start <id>` moves the item to the first status of the in-progress
category through the existing `PATCH /items/{id}` path (the way `cmd_sprint_status` does
for sprints), then does what `tack branch <id> --checkout` does; `--json` reports
`{item_id, status, branch}`. A status change that the workflow refuses is reported as the
server's error and no branch is created.

**Done when:** `tack start --help`, `tack open --help` and the two wiremock tests pass;
`docs/book/src/user-guide/cli.md` lists both; `.githooks/pre-push` passes.

### G1 — GitHub inbound: issue state

**Files:** `crates/tack-api/src/github_sync.rs` and its tests; `crates/tack-api/src/config.rs`
(`github_poll_seconds`, default `0` = off); `crates/tack-api/src/lib.rs` (the poll task,
started beside the backup scheduler); `crates/tack-db/src/repo/github_links.rs`
(`synced_at`, and a `list_links_for_repo`); `crates/tack-db/src/migrations.rs` (one
migration, `075_github_links_synced_at`); `crates/tack-api/tests/handlers/import_github.rs`
(one wiremock test: closed on GitHub → item moves to the project's first done status;
reopened → first todo status); `docs/GITHUB-SYNC.md`; `docs/CONFIG.md`.

The poll runs every `github_poll_seconds` when a token is set: `GET
/repos/{repo}/issues?state=all&since=<max synced_at>` per linked repo, with the `ETag`
sent back as `If-None-Match`. A changed issue state moves the linked item through
`tack-core`'s ordinary status change, so workflow rules hold and the outbound push sees
`old_done == new_done` and stays silent — that echo suppression is the one invariant to
pin. Nothing else on the issue is read.

**Done when:** the wiremock test passes in both directions; a poll against a 304 does no
write; `docs/GITHUB-SYNC.md`'s "out of scope" list no longer names inbound state;
`docs/openapi.json` is unchanged (no route is added).

### G2 — GitHub comments, both ways

**Files:** as G1, plus `crates/tack-db/src/repo/comments.rs` (a `github_comment_id`
column, migration `076`), `crates/tack-api/src/handlers/comments.rs` (the outbound hook,
shaped like `maybe_sync_github` in `handlers/items.rs`); `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts` through `./scripts/regen-generated.sh` only.

Outbound: a new comment on a linked item is posted to the issue, best-effort, and the
returned id is stored. Inbound: the poll reads `/issues/{n}/comments?since=` and creates a
Tack comment for each id it has not stored, attributed to the GitHub login in the body's
first line. A comment that came in is never pushed back out; the stored id is the check.

**Done when:** two wiremock tests (out, in) pass; a comment mirrored in and then read by
the poll again creates nothing; the generated files are regenerated, not edited.

### G3 — per-project token and manual link

**Files:** `crates/tack-db/src/repo/projects.rs` and `migrations.rs` (`077`: a
`github_token_ref` column holding a secret reference, never the token); the secret goes
through the existing write-only secret route; `crates/tack-api/src/handlers/items.rs`
(`PUT /items/{id}/github-link` with `{repo, issue_number}`, `DELETE` to unlink);
`crates/tack-api/src/router.rs`; `crates/tack-api/src/remote_backup.rs`
(`scrub_snapshot_secrets` learns the column in the same commit); `crates/tack-api/tests/handlers/import_github.rs`;
`frontend/src/features/item/` (one "Link GitHub issue" row in the item side panel);
`docs/GITHUB-SYNC.md`; generated files via `./scripts/regen-generated.sh`.

Resolution order for the token: project reference, then `TACK_GITHUB_TOKEN`. The token is
never logged and never in a response.

**Done when:** link, unlink and a push using the project token each have one HTTP test;
`scrub_snapshot_secrets` has a row for the column; the UI row links an item and the item
then closes its issue on Done in the wiremock test.

### U1 — the capture cap leaves the descriptor

**Files:** `crates/tack-runner/src/harness/local_process.rs` and
`local_process/tests.rs`; one line in each of `codex.rs`, `claude_code.rs`, `docket.rs`,
`opencode.rs`.

Since H1, capture keeps the head and the tail of a stream, so a grammar that reads a
terminal line no longer needs a minimum cap. Remove `min_capture_bytes` from
`HarnessDescriptor` and `run_limits`; the runner's configured limits apply alone.

**Done when:** `git grep min_capture_bytes` finds nothing; one existing test row proves a
transcript over the cap still yields its last line; `.githooks/pre-push` passes.

### U2 — codex reads usage and the served model

**Files:** `crates/tack-runner/src/harness/codex.rs`, `codex/tests.rs`, the M1 fixture.
Only if M1 found them in the output.

`report()` reads the token counts into `Usage` and the served model into
`ActualExecution`, the way `docket.rs` does from its result line. `usage` and the served
model capability entries change from `Unsupported` to what was measured, with the fixture
version in the reason.

**Done when:** one fixture-driven test row per field; `tack runner doctor` prints the new
entries; `docs/book/src/user-guide/agent-runners.md`'s codex row says what it measures.

### U3 — codex applies the permission policy

**Files:** as U2. After U2.

`invocation()` maps the request's `permission_policy` onto the flags M1 measured: `auto`
to the non-prompting flag, and the network flag and tool list only where a flag exists.
What has no flag stays `Unsupported` with the measured reason. Shaped like `opencode.rs`'s
`permission` block.

**Done when:** one table row per policy value; the doc row's "Honours the permission
policy" cell is rewritten from the measurement.

### U4 — opencode confirms the served model

**Files:** `crates/tack-runner/src/harness/opencode.rs`, `opencode/tests.rs`, the M2
fixture. Only if M2 found a field.

**Done when:** the served model is read from the field, or the capability reason quotes the
measurement and its date and the task closes as "not available".

### U5 — opencode attempts share one package cache

**Files:** `crates/tack-runner/src/harness/opencode.rs` and its tests; `docs/CONFIG.md`
(the cache lives under the existing state dir, no new variable).

`invocation()` sets the npm cache to `<state_dir>/opencode-cache`; `HOME` and
`OPENCODE_CONFIG_DIR` stay per attempt. The refusal of a network-denying request stays
until M2 shows a warm cache runs network-free; if it does, the refusal becomes a check
that the cache is warm.

**Done when:** the second real-binary run's `du -sh` matches M2's warm number in a test
comment, with the command; the doc row's "Can't" cell is rewritten.

### U6 — codex asks

**Files:** as U2, and `harness/local_process.rs` only if `app-server` needs a command shape
the core lacks (then stop and report). Only if M1 found stdio asking.

The grammar's command becomes the asking one when `approvals: ask`; `signal()` recognizes
the request line, `answer()` writes the response line, the way `claude_code.rs` does.
`decisions` becomes `Supported`.

**Done when:** the fake-harness `approvals: ask` test in `local_process/tests.rs` has a
codex row; the "Run with agent" dialog offers "Ask me" for codex because the runner
reports it; the doc row says "Yes".

### U7 — opencode asks

**Files:** as U4. Only if M2 found stdio asking in `run` or `acp`.

Same shape as U6. If only `acp` asks, the grammar's command is `acp` under `ask` and `run`
otherwise.

**Done when:** as U6, for opencode.

### U8 — docket: cancel, artifacts, asks

**Files:** `crates/tack-runner/src/harness/docket.rs`, `docket/tests.rs`, a new fixture
directory for the docket version that ships `harness-v1.1`. Only on M3's four `present`.

`cancel: Supported` once the crash matrix (`crates/tack-runner/tests/crash_matrix.rs`)
shows nothing survives a `SIGKILL` of the harness with the child-group events applied;
`artifacts: Supported` from the result's path list; `decisions: Supported` from the
question event and the stdin answer.

**Done when:** each of the three has one fixture-driven test row; the doc row is rewritten.

## Parked — not scheduled, and why

One line each so nobody re-inventories them. Any of these becomes a task only when a user
of the released product asks for it, and then through an ADR where a decision is involved.

- **Full JSON Schema validation for custom fields.** Pattern, min/max, length and item
  count are enforced; nobody has asked for more.
- **OpenAI direct and OpenRouter providers.** One module each under `provider/`; the
  gateway already reaches both vendors.
- **Code signing.** Wave 0.9, the user's.
- **A GitHub webhook receiver.** See the decision above; polling covers every install.
- **Anything in the "Decided" table.**

## Status

| Task | State |
|---|---|
| M1 · M2 · G1 | open |
| C1 | landed 2026-09-20 |
| M3 | done 2026-09-20: docket ships no `harness-v1.1`; U8 does not start |
| U1 · U4 · G2 | open |
| U2 · U3 · U5 · G3 | open |
| U6 · U7 · U8 | open |
| Wave 0 | with the user |
