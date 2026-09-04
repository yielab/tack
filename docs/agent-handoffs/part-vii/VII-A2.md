# VII-A2 handoff

- Base SHA / branch / final SHA: `02aa4e3` (Wave 18 base) / `agent/vii-a2-service` /
  uncommitted at handoff time (working tree has the full diff, nothing committed — see
  Next step).
- Files changed (must equal ownership list): `crates/tack-cli/src/service.rs` (new),
  one `Commands::Service` arm + `ServiceAction` enum + the early-dispatch match arm in
  `crates/tack-cli/src/main.rs`, the `dirs = "6.0.0"` line in `crates/tack-cli/Cargo.toml`
  (and the resulting `Cargo.lock` update), `docs/book/src/user-guide/cli.md` §"Service",
  `docs/DEPLOYMENT-GUIDE.md` new §"Per-user service (no root)" plus its Table of Contents
  renumbering. Matches the ownership row exactly; no other file touched.
- Contract fixtures consumed: none — this card has no wire contract.
- Behavior implemented: `tack service install|uninstall|status`. `install` creates the
  OS data root (`dirs::data_dir()/tack`, `0700` on Unix) and its `storage`/`runner`/`logs`
  subdirectories, writes a systemd **user** unit (Linux) or launchd agent plist (macOS)
  whose `ExecStart`/`ProgramArguments` point at the absolute, canonicalized path of the
  currently-running binary with `serve --with-runner`, and the four `TACK_*` folder
  variables from TODO.md §VII.0, then starts it (`systemctl --user enable --now` /
  `launchctl bootstrap gui/$UID`). `uninstall` stops and removes the unit/plist only,
  never the data root. `status` prints the unit's state and the health URL
  (`http://127.0.0.1:3210/api/health`, the server's own default). Any other OS
  (Windows included) gets the typed `service::UnsupportedPlatform` error naming the
  desktop app as the alternative. Platform dispatch is via a runtime `Platform` enum
  (`current_os()` reads `std::env::consts::OS` once) rather than `#[cfg(target_os)]`, so
  every branch — including the unsupported one — compiles and is reachable on every CI
  platform and is unit-testable without a Windows or macOS box.
- Tests added and exact commands/results: `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-A2
  cargo test -p tack-cli` → `57 passed` (lib) + `26 passed` (bin, 7 of them
  `service::tests::*`) + `11 passed` (cli_test) + `5 passed` (e6_scheduler_e2e_test) + `0`
  doc-tests, 0 failed across all five suites. The 7 new tests:
  `install_is_unsupported_on_other_platforms`, `uninstall_is_unsupported_on_other_platforms`,
  `status_is_unsupported_on_other_platforms` (each downcasts the returned `anyhow::Error`
  to `UnsupportedPlatform` and checks the message names "desktop app"),
  `systemd_unit_has_the_expected_keys` and `launchd_plist_has_the_expected_keys` (full
  `assert_eq!` byte-pins of the rendered unit file / plist against a literal, given a
  fixed binary path and data root — no filesystem or real systemd/launchd touched),
  `ensure_data_root_creates_root_and_subdirs_0700_on_unix` (creates a temp dir, asserts
  the three subdirectories exist and the root's mode is exactly `0700`),
  `env_vars_match_the_four_documented_variables` (pins the key list and order).
  `cargo fmt --check -p tack-cli` and `cargo clippy -p tack-cli -- -D warnings` both
  clean; `cargo check --workspace` clean.
- Failure/adversarial case proved: the unsupported-platform path is proved by unit test
  (see above) since this machine cannot run Windows. The "uninstall never touches the
  data root" claim is proved by direct row-count assertion in the live run (below), not
  by an automated test — `uninstall_systemd()` shells out to the real `systemctl` and
  isn't isolated from the real filesystem, so there is no automated regression test for
  this claim yet; see "Not checked".
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: launchd is implemented and byte-pin tested
  but never run live — this machine is Linux. See "Platform measured" and the amendment
  policy below if a macOS agent later runs it.
- Secrets/logging review: no secret is read, stored, or logged anywhere in this module —
  the four environment variables are filesystem paths only. `println!` output contains
  paths and a fixed loopback URL, nothing else.
- Safe merge order and likely conflicts: `crates/tack-cli/src/main.rs` is also touched by
  Part VI's VI-B1 (`tack runner secret …`), per TODO.md §VII.3 conflict 3. My diff is
  additive only — a new `mod service;` line, one new `Commands::Service` arm inserted
  after the existing `Runner` arm, a new `ServiceAction` enum inserted after
  `RunnerAction`'s closing brace, one new early-dispatch match arm inserted after the
  `Runner`/`Doctor` arm, one new `unreachable!()` exhaustiveness arm inserted after the
  `Commands::Runner` match closes, and one comment-only edit to the existing "don't need
  a live API client" comment (extended the command list and Service's own clause, did not
  reword the existing clauses). None of this overlaps VI-B1's arm; the integrator should
  be able to apply both diffs to the same file cleanly and then run one build to confirm.
  `crates/tack-cli/Cargo.toml` gets one new dependency line (`dirs`); VI-B1 owns a
  separate dependency line in the same file per the dispatch note — no overlap expected,
  but `Cargo.lock` will need regenerating once if both land (a plain `cargo build`
  regenerates it deterministically).
- Checklist: no unowned files touched, no live secret, no panic stub (`unreachable!()`
  arms are exhaustiveness-only, on branches already returned from above them — the
  existing `RunnerAction::Start`/`Doctor` arms use the identical pattern), no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `tack service install` writes a systemd user unit and starts it | Live run below: unit file content shown, `systemctl --user is-active tack` → `active` |
| The service survives the shell that installed it | Live run: `install` run in one shell invocation, `is-active`/`curl` run in a later, separate shell invocation, still `active` |
| `/api/health` answers while the service runs | Live run: `curl http://127.0.0.1:3210/api/health` → `HTTP 200`, `{"status":"ok",...}` |
| `tack service uninstall` removes the unit and stops the process, leaves data alone | Live run: unit file gone, `pgrep -af "[t]ack serve"` empty, data root file count `5` before and after |
| `tack service status` prints state and the health URL | Live run: `State:  active` / `Health: http://127.0.0.1:3210/api/health` |
| Unit file and plist have the exact documented keys | `service::tests::systemd_unit_has_the_expected_keys`, `service::tests::launchd_plist_has_the_expected_keys` (full-byte `assert_eq!`) |
| An unsupported platform gets a typed, nameable error | `service::tests::install_is_unsupported_on_other_platforms` (+ uninstall/status variants) |
| Data root is created `0700` on Unix | `service::tests::ensure_data_root_creates_root_and_subdirs_0700_on_unix` |
| `cli.md` shows real output for all three commands | `docs/book/src/user-guide/cli.md` §"Service" — output pasted verbatim from the live run below |
| DEPLOYMENT-GUIDE's system-level unit is untouched | `git diff docs/DEPLOYMENT-GUIDE.md` — original systemd block body has zero changed lines; only the new section was inserted after it |

## Measured numbers

- `cargo test -p tack-cli`: 99 tests total (57 + 26 + 11 + 5 + 0 doc-tests), 0 failed.
- Data root file count: `5` both before and after `tack service uninstall` (unchanged).
- `cargo clippy -p tack-cli -- -D warnings`: 0 warnings.

## What a stranger still cannot do

Run `tack service install` on Windows and get a working background service — there is
none; the typed error names the desktop app (VII-B1) as the answer there, and that app
did not exist yet at the time this card was worked. A stranger on macOS also cannot get
a *live-proved* launch agent from this card alone: the plist is rendered and byte-tested,
but `launchctl bootstrap` was never actually run against a real launchd, because this
machine is Linux.

## Platform measured

- OS: Ubuntu 24.04.4 LTS (Noble Numbat), kernel 6.14.0-37-generic, x86_64.
- Desktop environment: `ubuntu:GNOME` (not exercised by this card — no window is
  involved — recorded per the shared template only).
- `XDG_SESSION_TYPE`: `x11`.
- systemd version: `255 (255.4-1ubuntu8.17)`, user bus present and running
  (`systemctl --user status` showed `State: running`, `1 day 9h` uptime at test time).
- macOS / launchd: `not_measured` — no macOS machine available to this agent.
- Windows: `not_measured` live (by design — no implementation); the unsupported-platform
  behavior itself is unit-tested (see Claim → evidence).

## Daemon proof

Adapted from §VII.1 rule 3 for the terminal path (no window; "shell closed" plays the
role "window closed" plays for VII-B1/B2): attempt is the daemon itself staying up
across the invoking shell's exit, observed from a distinct, later shell.

```
2026-09-03T21:57:57-03:00  (shell A) $ tack service install
Created symlink /home/ox/.config/systemd/user/default.target.wants/tack.service → /home/ox/.config/systemd/user/tack.service.
Installed and started the tack user service.
  Unit file: /home/ox/.config/systemd/user/tack.service
  Data root: /home/ox/.local/share/tack
  Health:    http://127.0.0.1:3210/api/health
install exit: 0
                                    [shell A's process has already exited by here —
                                     this environment runs one shell process per command]
2026-09-03T21:58:05-03:00  (shell B, new process) $ pgrep -af "[t]ack serve"
3982403 /var/tmp/tack-agent-targets/VII-A2/debug/tack serve --with-runner
(shell B) $ systemctl --user is-active tack
active
(shell B) $ curl -sS -w "\nHTTP_STATUS:%{http_code}\n" http://127.0.0.1:3210/api/health
{"migrations_applied":61,"status":"ok","version":"0.1.0-beta.7"}
HTTP_STATUS:200
```

```
2026-09-03T21:58:10-03:00  (shell C) $ tack service uninstall
Removed "/home/ox/.config/systemd/user/default.target.wants/tack.service".
Removed the tack user service. The data root was left untouched.
uninstall exit: 0
2026-09-03T21:58:16-03:00  (shell D, new process) $ ls ~/.config/systemd/user/tack.service
ls: cannot access '/home/ox/.config/systemd/user/tack.service': No such file or directory
(shell D) $ pgrep -af "[t]ack serve"
(no output)
(shell D) $ systemctl --user is-active tack
inactive
(shell D) $ find ~/.local/share/tack -type f | wc -l
5   # same as the count taken immediately before uninstall
```

Re-run once more after fixing a clippy lint (renamed the internal `Os` enum to
`Platform`) to confirm the rebuilt binary still behaves identically — same install →
active/200 → uninstall → gone/inactive/file-count-unchanged sequence, timestamps
2026-09-03T22:00:36 through 22:00:52. The data root was `rm -rf`'d by this agent after
each pass so this dev machine's real `~/.local/share/tack` is not left behind; a real
user's install is not touched by that cleanup since it only ever ran under this agent's
own account during this proof.

## Process proof

| Step | `pgrep -af "[t]ack serve"` |
|---|---|
| Before first install | (empty) |
| After install | `<pid> /var/tmp/tack-agent-targets/VII-A2/debug/tack serve --with-runner` |
| After uninstall | (empty) |

Parent/session check on the running process: `ps -o pid,ppid,sid,cmd -p <pid>` showed
`PPID 1315` (the user's `systemd --user` manager, not the shell that ran `install`) and
`SID` equal to its own PID — confirming it is a properly detached daemon, not a child of
the invoking shell.

## Context spent

- Tokens read before the first edit (cold start): the dispatch block estimated ≈14k for
  the named read list; actual reading was somewhat larger because of an environment
  issue below, but the *named* reads (board prelude, VII-A2 card, ADR 0062 decisions 5+8,
  the `Commands`/`RunnerAction` shape, `doctor.rs` head, the loopback gate, the
  DEPLOYMENT-GUIDE excerpt, `cli.md` headers, `dirs::data_dir()`) matched the block's
  estimate closely once the environment issue was resolved.
- Context size at handoff: comfortably under the 120k mid-card ceiling; nowhere near the
  150k stop threshold.
- Files opened and not used: `docs/book/src/user-guide/cli.md` lines 255-284 (the
  `## Runner` section body) — opened beyond the named `grep -n "^## "` to see the
  existing doc-block *style* (prose paragraph, then ` ```sh ` / ` ```text ` pairs) before
  writing the new `## Service` section in the same voice; the content itself was not
  reused. Recording this because the read list named only the grep, not the section body.
- Read-list lines that were wrong: none — every named read resolved to real, relevant
  content once branched from the correct base (see next point).
- **Environment issue, not a read-list error:** this agent's worktree
  (`/home/ox/Sites/objetivosMios/.claude/worktrees/agent-a1c6d07896072a9f4`) was checked
  out at `e5206c7` — a commit that predates Part VII entirely (no `docs/adr/0062-*.md`,
  no `docs/agent-handoffs/part-vii/`, no Part VII section in `TODO.md`) — even though the
  dispatch instructions named base SHA `02aa4e3`. The first few reads in this session
  used an absolute path into the *sibling* checkout at `/home/ox/Sites/objetivosMios`
  (which was already past `02aa4e3`) by mistake, which is why they briefly appeared to
  succeed before later commands against the worktree's own relative paths failed to find
  the same files. Fixed by running `git switch -c agent/vii-a2-service 02aa4e3` directly
  in the worktree — `02aa4e3` was already a real commit in the shared `.git` object
  store, just not what the worktree happened to have checked out — which brought the
  named files into the worktree as expected. Everything after that point was read from
  the correct worktree path. Flagging this for whoever prepares the next agent's
  worktree: don't assume a fresh worktree is checked out at the dispatch plan's stated
  base SHA — verify with `git log -1` before trusting relative-path reads.
- Web pages fetched: none — decisions 5 and 8 of ADR 0062 and the `dirs` crate's own
  doc comment (read from its vendored source under `~/.cargo/registry/src/`, not
  docs.rs) covered everything needed; no `v2.tauri.app` or `docs.rs` fetch was required
  for this card.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

### 2026-09-04 — Wave 18 integrator: what stayed unproven

Recorded so the next reader does not mistake the byte-pin tests for platform coverage:

- The launchd path is byte-pin tested and has never been executed. No macOS machine
  was available. `launchctl bootstrap gui/$UID` and the plist's behaviour on a real
  session are unverified; the Windows path likewise.
- "`uninstall` leaves data alone" is proven by a live run — data root file count 5
  before and after — but not by an automated test, because `uninstall_systemd()`
  shells out to the real `systemctl`. A regression here would pass CI silently.

Neither blocks the merge. The first needs hardware; the second needs the systemd call
behind a seam a test can substitute, which is a small refactor a later Part VII card
can take.
