# VII-B1 handoff

- Base SHA / branch / final SHA: branched `agent/vii-b1-desktop-skeleton` from `2958e9e`
  (the SHA the Wave 18 row names). `develop`'s actual tip at branch time was `92a42cd`,
  one commit ahead — a doc-only commit that pins this very SHA into the board text (see
  Amendments). Not committed: the branch holds an uncommitted working tree; there is no
  final SHA yet.
- Files changed (must equal ownership list):
  - `Cargo.toml` — added `crates/tack-desktop` to `[workspace].members`.
  - `Cargo.lock` — regenerated (Tauri's dependency tree; +3033/-191 lines, `--stat` only,
    not read in full per the dispatch README's trap note).
  - `Makefile` — added the `desktop-sidecar` target (`.PHONY` list updated too).
  - `.gitignore` — added the sidecar-binary, generated-icon, `gen/` and per-crate
    `target/` lines (see the escalation below on icons).
  - `crates/tack-desktop/Cargo.toml`, `build.rs`, `tauri.conf.json`,
    `src/main.rs`, `src/supervisor.rs`, `icons/placeholder-source.svg`,
    `binaries/.gitkeep` — the ownership list's named files.
  - Three files not named in §VII.2's list, added because the crate cannot exist without
    them: `crates/tack-desktop/capabilities/default.json` (Tauri v2 requires a
    capabilities file to grant the shell/dialog permissions `tauri.conf.json` and
    `main.rs` use), `crates/tack-desktop/tests/dependency_boundary.rs` (the test §VII.1
    rules 1–2 explicitly require — "a test asserts the dependency list from `cargo
    metadata`"), `crates/tack-desktop/dist/index.html` (a one-line placeholder;
    `build.frontendDist` needs *some* directory to point at even though the real window
    never loads it — see the tauri.conf.json notes below).
- Contract fixtures consumed: none. ADR 0062 states the wire contract is unaffected;
  this card touches no `docs/contracts/runner-v1/` fixture.
- Behavior implemented: `crates/tack-desktop/src/supervisor.rs` — probes
  `GET /api/health`; attaches if something answers, otherwise checks whether the port has
  *any* listener (refuses with a typed `PortOccupiedByOther` if so, native dialog in
  `main.rs`), else spawns `tack serve --with-runner` via the launcher trait and polls
  health up to 15s. `shutdown()` sends SIGTERM, polls liveness up to 5s, then hard-kills.
  `src/main.rs` wires this to the real Tauri sidecar (`tauri_plugin_shell`), opens the
  window at the server's URL once healthy, and runs `shutdown()` from
  `RunEvent::ExitRequested` when the app is in *started* mode (never in *attached* mode).
- Tests added and exact commands/results:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B1 cargo test -p tack-desktop` → 6
    passed, 0 failed (4 in `supervisor::tests` against a fake Python sidecar, 2 in
    `tests/dependency_boundary.rs`).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B1 cargo tree -p tack-cli -e normal
    | grep -ci "tauri\|webkit\|gtk"` → `0`.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B1 cargo check --workspace` → clean.
- Failure/adversarial case proved: `shutdown()`'s reaping bug was caught live during
  development, not invented after the fact —
  `spawns_and_becomes_healthy_when_nothing_is_listening` failed with `child pid … must
  not survive shutdown` when `shutdown()` returned early on a graceful SIGTERM exit
  without also calling `process.kill()` to reap the zombie. Fixing `shutdown()` to always
  call `kill()` (ignoring its error once the graceful path already confirmed exit) made
  the test pass; reverting that one line reproduces the original failure. Separately,
  `attaches_without_spawning_when_something_already_answers` and
  `refuses_to_spawn_when_the_port_is_held_by_something_else` both set the launcher's
  `fail_next: true`, so either test would fail with `SpawnFailed("poisoned for test")`
  instead of its expected outcome if attach/refuse logic ever fell through to a real
  spawn — the poison *is* the adversarial proof, not a separate step.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - **Single instance is not wired in this card.** The card's Context paragraph mentions
    `tauri-plugin-single-instance`, but §VII.2's ownership table assigns "plugin
    registration for autostart and single-instance in `main.rs`" to VII-B2, and B1's own
    Acceptance list never tests it. Read literally, Context and the ownership table
    disagree; I followed the ownership table and B1's Acceptance list (neither requires
    it here) and left it for B2. A second launch today opens a second window rather than
    focusing the first — expected until B2 lands, not a bug in this card.
  - **Icon commit policy conflicts between two sources.** The dispatch prompt states
    "Sidecar binaries and generated icons are gitignored, never committed" (called out as
    one of the rules this card establishes). §VII.2's ownership row for VII-C1 says the
    generated icon sizes are "committed as the CLI produces them." I followed the
    dispatch prompt for this card's own commit: only `icons/placeholder-source.svg` (the
    source) is tracked; `icons/{32x32,64x64,128x128,128x128@2x}.png`, `icon.{ico,icns}`
    are gitignored and must be regenerated with `cargo tauri icon
    crates/tack-desktop/icons/placeholder-source.svg -o crates/tack-desktop/icons` before
    `cargo check -p tack-desktop` or `cargo tauri build` will succeed — the same kind of
    prerequisite as `make desktop-sidecar`. **C1 should resolve this conflict explicitly**
    (and will replace the placeholder source with the real `tack.svg` regardless).
  - The four `TACK_*` folder variables point at `$TMPDIR/tack-desktop-dev`, a fixed
    temporary path — exactly what the card's Context specifies "until VII-B3 lands." Not
    a gap, a stated placeholder.
  - `PortOccupiedByOther` (native dialog naming the port) is unit-tested
    (`refuses_to_spawn_when_the_port_is_held_by_something_else`) but not proved with the
    real GUI/dialog on screen — outside B1's explicit Acceptance list, added because the
    Context paragraph calls for the behavior.
  - Graceful shutdown (SIGTERM before kill) is Unix-only; `cfg(not(unix))` hard-kills
    immediately. Windows/macOS are `not_measured` for the whole card — no such machine
    here.
  - The SIGTERM-then-liveness-poll-then-kill sequence in `shutdown()` has a theoretical
    PID-reuse race (kill targets a raw pid after the OS may have reaped and recycled it
    under heavy load) — documented in the function's doc comment, not exercised by a
    test; the window is a handful of milliseconds in practice.
  - Wayland is `not_measured`: this session's `$XDG_SESSION_TYPE` was `x11` throughout
    (see Platform measured); `GDK_BACKEND=x11` retry was not needed.
- Secrets/logging review: no secret is handled by this crate. Logs
  (`tracing::info!`/`tracing::error!`) carry `version`, `status`, `pid`, `port` only —
  never a path, credential, or request body.
- Safe merge order and likely conflicts: this is a new crate; the only shared-file touch
  is `Cargo.toml`'s workspace `members` array, which Part VI's VI-B1 does not touch (VI-B1
  and VII-A2 both add an arm to `crates/tack-cli/src/main.rs`, per §VII.3 point 3 — this
  card never touches `main.rs` in `tack-cli`). No conflict expected merging VII-B1 before
  or after VI-B1/VII-A2.
- Checklist: no unowned files (three necessary companions flagged above, not silently
  added); no live secret; no panic stub (`SupervisorError` is a typed enum, not
  `unimplemented!()`); no blind retry (the port-conflict path refuses once, no loop; the
  health-poll loop has a bounded, typed timeout).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The app attaches to an already-running Tack server instead of starting a second one | Live: hand-started `tack serve --with-runner` (pid 2061059) → launched the AppImage → `pgrep -af "tack serve"` still shows exactly one real process, pid 2061059 unchanged. Unit: `attaches_without_spawning_when_something_already_answers` (launcher poisoned with `fail_next: true`, would fail loudly if attach ever spawned). |
| Quitting an attached app never touches the server it did not start | Live: closed the attached app's window via a `_NET_CLOSE_WINDOW` client message → `tack-desktop` process gone, hand-started `tack serve` (pid 2061059) still present, `curl /api/health` still answers `{"status":"ok","version":"0.1.0-beta.7","migrations_applied":61}`. |
| When nothing is listening, the app spawns `tack serve --with-runner` and waits until healthy | Live: fresh launch with port 3210 free → `pgrep -af "tack serve"` shows one child (`/tmp/.mount_Tack_.../usr/bin/tack serve --with-runner`); `curl /api/health` from a second shell answers. Unit: `spawns_and_becomes_healthy_when_nothing_is_listening`. |
| Closing the window in this card's spawn mode stops the spawned server; no orphan | Live: sent `_NET_CLOSE_WINDOW` to the spawn-mode window → both `tack-desktop` and `tack serve` gone from `pgrep` within ~1s (polled every 0.5s, resolved on the first check); `ss -ltnp` shows port 3210 no longer listening. |
| A port held by something that is not Tack is refused, not spawned into | Unit: `refuses_to_spawn_when_the_port_is_held_by_something_else` — a bare `TcpListener` (never `.accept()`-ed, so its backlog just completes handshakes) makes `attach_or_start` return `PortOccupiedByOther`, poisoned launcher never called. Not proved live with the real dialog on screen. |
| No crate under `crates/tack-desktop` depends on `tack-api`, `tack-db`, `tack-orch` or `tack-runner` | `tack_desktop_never_links_the_server_crates` (reads `cargo metadata --offline`, asserts none of the four names appear in `tack-desktop`'s dependency list) — passes. |
| `tack-cli`'s dependency tree stays free of `tauri`/`webkit`/`gtk` | `tack_cli_stays_free_of_webview_and_gtk_dependencies` (runs `cargo tree -p tack-cli -e normal --offline`) — passes. Exact gate command also run directly: `cargo tree -p tack-cli -e normal \| grep -ci "tauri\|webkit\|gtk"` → `0`. |
| `cargo tauri build` on this machine produces real `.deb` and `.AppImage` bundles | See Measured numbers — both exist under `/var/tmp/tack-agent-targets/VII-B1/release/bundle/`. |
| The app opens its own window titled "Tack" loading the real board UI | `DISPLAY=:0 xwininfo -root -tree` lists `0x4200003 "Tack": ("tack-desktop" "Tack-desktop") 1200x800+14+49`; `curl http://127.0.0.1:3210/` (the window's URL) returns the SPA's `index.html`. |

## Measured numbers

- `.deb`: 12,369,560 bytes (~11.8 MiB) — `Tack_0.1.0-beta.7_amd64.deb`.
- `.AppImage`: 87,919,096 bytes (~83.9 MiB) — `Tack_0.1.0-beta.7_amd64.AppImage`
  (includes the bundled webview runtime via `linuxdeploy`).
- `.rpm`: 12,368,851 bytes — produced as a side effect of Tauri's default "all" bundle
  target on this host; not in the card's Acceptance list, launch not proved.
- Window-close-to-process-gone latency: ≤ ~1s in both live proofs (polled at 0.5s
  intervals, resolved on the first poll after the close event) — well inside the 5s
  `SHUTDOWN_GRACE` the SIGTERM path allows before falling back to a hard kill.
- Test suite: 6 passed, 0 failed, `cargo test -p tack-desktop` wall time 5.3–5.4s
  (dominated by one deliberate 2s HTTP timeout in the port-conflict test and the two
  fake-sidecar spawn/health-poll round trips).
- Versions actually linked: `tauri 2.11.5`, `tauri-plugin-shell 2.3.6`,
  `tauri-plugin-dialog 2.7.3`, `tauri-cli 2.11.4`, `webkit2gtk 2.52.6` (`pkg-config
  --modversion webkit2gtk-4.1`), `rustc 1.96.0`.
- `/api/health` during every live proof: `migrations_applied: 61`, `version:
  "0.1.0-beta.7"` — matches the workspace version.

## What a stranger still cannot do

Launch-at-login, a tray icon, or closing the window without the app quitting entirely —
all explicitly B2's card, not this one (Context says so: "In this card, closing the
window quits"). There is no first-run screen or persistent per-OS data folder yet — data
lives in a fixed temp directory until B3 lands, so a second machine, or this machine after
`$TMPDIR` is cleared, starts from an empty database with no prompt explaining why. A
second launch opens a second window instead of focusing the first (no single-instance
plugin yet). Nothing here has been built or run on macOS or Windows.

## Platform measured

- OS: Ubuntu 24.04.4 LTS (Noble Numbat), kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: `XDG_CURRENT_DESKTOP=ubuntu:GNOME`.
- `$XDG_SESSION_TYPE`: `x11` (not Wayland; `GDK_BACKEND=x11` retry was never needed).
- Appindicator host: not checked — this card adds no tray icon (that is VII-B2's).
- systemd: `255 (255.4-1ubuntu8.17)`.
- `rustc 1.96.0`, `tauri-cli 2.11.4`, `webkit2gtk 2.52.6` (linked), node `v22.17.1`.

## Daemon proof

Not applicable to this card as written, and not forced into a false positive: §VII.1 rule
3 ("the work outlives the window") is the daemon promise `close hides` gives you, and
this card's own Context is explicit that *"in this card, closing the window quits (B2
makes it hide)."* Proving "closed window, server still running, reopen shows the same
state" would require the hide behavior B2 adds. What this card does prove instead is the
narrower, correct-for-this-card claim: closing the window in spawn mode stops the server
deliberately and completely (see Process proof and the Claim → evidence table) — the
opposite half of the daemon promise, and the one this card is actually responsible for.

## Process proof

All commands run with `DISPLAY=:0` against the real X11 session; `pgrep -af` output
below is filtered to the two patterns that matter (the shell wrapper that runs `pgrep`
itself otherwise self-matches on its own command line).

**Spawn mode — fresh launch, no server running:**
```
# before
$ pgrep -af "tack serve"; pgrep -af "tack-desktop"
(nothing)
$ ss -ltnp | grep :3210
(nothing)

# after launching the AppImage
$ pgrep -af "tack-desktop"
2048635 tack-desktop
$ pgrep -af "tack serve"
2048669 /tmp/.mount_Tack_0FFmEKo/usr/bin/tack serve --with-runner
$ curl -s http://127.0.0.1:3210/api/health
{"migrations_applied":61,"status":"ok","version":"0.1.0-beta.7"}
```

**Spawn mode — window closed (`_NET_CLOSE_WINDOW` sent to window `0x4200003`):**
```
$ pgrep -af "tack-desktop"; pgrep -af "tack serve"
(both empty within ~1s of the close event)
$ ss -ltnp | grep :3210
(nothing — port released)
```

**Attach mode — a server already started by hand (pid 2061059), then the app launched:**
```
# before launching the app
$ pgrep -af "tack serve"
2061059 /var/tmp/tack-agent-targets/VII-B1/release/tack serve --with-runner

# after launching the app
$ pgrep -af "tack serve"      # still exactly one — the hand-started process, unchanged
2061059 /var/tmp/tack-agent-targets/VII-B1/release/tack serve --with-runner
$ pgrep -af "tack-desktop"
2069598 tack-desktop
```

**Attach mode — app quit (`_NET_CLOSE_WINDOW` sent again):**
```
$ pgrep -af "tack-desktop"
(empty)
$ pgrep -af "tack serve"      # the hand-started server survives the app's quit
2061059 /var/tmp/tack-agent-targets/VII-B1/release/tack serve --with-runner
$ curl -s http://127.0.0.1:3210/api/health
{"migrations_applied":61,"status":"ok","version":"0.1.0-beta.7"}
```

No orphan in either mode: spawn mode leaves nothing behind after quit; attach mode leaves
exactly the process that was there before the app ever ran.

## tauri.conf.json and window/sidecar wiring (for B2/B3, so they do not need to reopen this file)

`crates/tack-desktop/tauri.conf.json`, in full:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Tack",
  "version": "0.1.0-beta.7",
  "identifier": "com.yielab.tack",
  "build": { "frontendDist": "dist" },
  "app": {
    "windows": [],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "icon": [
      "icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png",
      "icons/icon.icns", "icons/icon.ico"
    ],
    "externalBin": ["binaries/tack"]
  }
}
```

- `app.windows` is deliberately empty — there is no static window. `src/main.rs`'s
  `setup` hook creates the one window (label `"main"`, matching
  `capabilities/default.json`'s `"windows": ["main"]`) only after the supervisor resolves
  attach-vs-spawn and the server is healthy: `WebviewWindowBuilder::new(&handle, "main",
  WebviewUrl::External(url)).title("Tack").inner_size(1200.0, 800.0).build()`, where
  `url` is `format!("http://127.0.0.1:{port}")` with `port = supervisor::DEFAULT_PORT`
  (`3210`) parsed to a `Url`. B2's tray/close-to-hide code and B3's first-run dialog both
  act on this same `"main"` window handle.
- `build.frontendDist: "dist"` points at `crates/tack-desktop/dist/index.html`, a
  one-line placeholder (`<!doctype html><title>Tack</title>`) that Tauri's build wants to
  exist but that the app never actually navigates to — the real window always loads the
  server's URL above, per ADR 0062 decision 4 (one UI, loaded from the local server).
- `bundle.externalBin: ["binaries/tack"]` — the platform suffix is appended by Tauri's
  own tooling; the file on disk must be named `binaries/tack-<host-triple>` (this
  machine: `binaries/tack-x86_64-unknown-linux-gnu`), produced by `make desktop-sidecar`
  and never committed (gitignored).
- Sidecar spawn, in `src/main.rs`'s `TauriLauncher::spawn`: `self.app.shell()
  .sidecar("tack")?.args(["serve", "--with-runner"]) .env("TACK_HOST", "127.0.0.1")
  .env("TACK_PORT", self.port.to_string())` plus the four `TACK_*` folder variables from
  `supervisor::ServerFolders::env_vars()`, then `.spawn()`. `capabilities/default.json`
  grants `shell:allow-execute` and `shell:allow-spawn` scoped to `"name": "binaries/tack",
  "sidecar": true, "args": ["serve", "--with-runner"]` — B3 does not need a new
  capability entry to add more env vars (env is not part of the capability's `args`
  match), but a new *argv* (e.g. a first-run flag) would need the `args` list extended
  here.
- Shutdown wiring: `RunEvent::ExitRequested` (registered in the final `.run(|app_handle,
  event| { … })` closure, not a per-window event) takes the `DesktopState` mutex's
  `ServerMode`, and calls `supervisor::shutdown(process)` only for `ServerMode::Started`
  — `ServerMode::Attached` is never touched. B2 replacing "close quits" with "close
  hides" means removing (or conditioning) whatever currently causes the last window
  closing to trigger `ExitRequested` at all — that trigger is Tauri's own default
  behavior, not code this card wrote explicitly, so B2 should search for where a
  `WindowEvent::CloseRequested` handler needs to call `.prevent_close()` on the `"main"`
  window instead of relying on the default.

## Context spent

- Tokens read before the first edit (cold start): followed the block's read list
  (`README.md` header + VII-B1 block, `TODO.md` head + §VII.0–§VII.4, ADR 0062 whole,
  the named source-file ranges) plus two small reads not in the list — root `Cargo.toml`
  lines 1–40 (the block's own recipe only bounds the `members` array; writing a
  conforming `crates/tack-desktop/Cargo.toml` with `version.workspace = true` etc.
  needed `[workspace.package]`, which sits above it) and a `grep` over root `Cargo.toml`
  for existing `reqwest`/`tracing`/`thiserror`/`anyhow` versions (to reuse workspace
  dependency versions instead of guessing). Roughly matched the block's ~18k + two Tauri
  pages estimate.
- Context size at handoff: this session's token counter is a whole-run budget (started
  at 15,000,000, session-wide, not the ~200k single-context-window the card skill's
  ≤120k/≤150k thresholds assume), so it is not a like-for-like comparison to those
  numbers. Reporting it for the record rather than omitting it: approximately 250k of
  that whole-run budget was spent by handoff time, across cold-start reads, five web
  fetches, iterative `cargo check`/`cargo test`/`cargo tauri build` cycles, and the live
  GUI proofs (each of which reads back only small, targeted command output, not full
  logs — one very large log file was produced by a hand-started `tack serve` and was
  read only as a truncated preview, never in full).
- Files opened and not used: none — every named read fed directly into either the crate
  code or this handoff.
- Read-list lines that were wrong: none found. The block's line-count estimates (~18k)
  were in the right range; the two small extra reads above were gaps in the recipe
  (workspace-level package metadata needed to write a new member's `Cargo.toml`), not
  errors in what it did name.
- Web pages fetched (all under the two allowed domains):
  - `https://v2.tauri.app/develop/sidecar/` — named by the block.
  - `https://v2.tauri.app/start/create-project/` — named by the block.
  - `https://v2.tauri.app/reference/config/` — the block's one permitted extra ("only if
    a config key is unclear"); used to confirm `build.frontendDist` semantics with no
    bundled frontend, and that `app.windows[].url` accepts a full external URL.
  - `https://docs.rs/tauri-plugin-shell/latest/tauri_plugin_shell/process/struct.CommandChild.html`
    — not named; needed the exact `CommandChild` API (`pid()`, `kill(self)`, no
    `wait`/`try_wait`) to design `SidecarHandle` correctly.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/` — not named;
    needed the minimal blocking-message-dialog API to implement the port-conflict
    dialog the card's Context calls for.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-04 — base SHA note.** `develop`'s tip when this branch was created was
`92a42cd` ("plan: pin the Wave 15 and Wave 18 base SHA"), one commit ahead of the
`2958e9e` this handoff (and the dispatch prompt) names as the base. Inspected the
commit: it only rewrites `TODO.md` and the two `docs/agent-handoffs/*/README.md` files to
spell out `2958e9e` explicitly where they previously said "pinned at dispatch" — no code
changed. Branched from `2958e9e` as instructed; recording this so the discrepancy is not
mistaken for an unnoticed rebase later.

### 2026-09-04 — Wave 18 integrator: the crate could not be built from a clean clone

Adding `crates/tack-desktop` to `[workspace].members` made `cargo build --workspace`,
`cargo test --workspace` and `cargo clippy --workspace --all-targets` fail for anyone
who had not first run `make desktop-sidecar`. This card's own verification did not
catch it because `cargo check --workspace` ran in a worktree where the sidecar had
already been staged by hand; on `develop` after the merge it failed immediately:

```
error: failed to run custom build command for `tack-desktop`
  resource path `binaries/tack-x86_64-unknown-linux-gnu` doesn't exist
```

and, once that was cleared, again on the gitignored icon set:

```
error: proc macro panicked
  message: failed to open icon .../icons/32x32.png: No such file or directory
```

Both are compile-time reads, not bundle-time ones: `tauri-build` resolves
`bundle.externalBin` and `generate_context!` opens `bundle.icon` while the crate
compiles. The MSRV job runs `cargo build --workspace --locked`, so this would have
turned CI red on the next push.

Fixed at integration, not deferred:

- `bundle.externalBin` moved out of `tauri.conf.json` into a new
  `crates/tack-desktop/tauri.bundle.conf.json`, merged only when bundling. A plain
  workspace build no longer demands a per-platform binary that is correctly
  gitignored. `make desktop` bundles with `--config tauri.bundle.conf.json`; the
  sidecar permission in `capabilities/default.json` is unaffected, since it names the
  sidecar independently of the bundle config.
- The generated icon set is now committed, and the two `.gitignore` lines that
  excluded it are gone — the resolution commit `7cc6221` had routed that removal to
  VII-C1, which is now a no-op for VII-C1. `generate_context!` reads these files at
  compile time, so they are a source input, not build output.

Standing rule for VII-B2, VII-B3 and VII-C1: anything `tauri.conf.json` names is read
while the crate compiles. A file that a clean clone does not have belongs in the
bundle overlay, never in the base config.

### 2026-09-04 — CI proved the workspace membership itself was wrong

The compile-time fixes above cleared the local build, and CI then failed three jobs on the
first push — Rust, MSRV and embed-spa — all with the same error:

```
error: failed to run custom build command for `glib-sys v0.18.1`
  The system library `glib-2.0` required by crate `glib-sys` was not found.
```

Tauri drags GTK, WebKit and glib into whatever workspace contains it. As a member,
`tack-desktop` made those system libraries a prerequisite for building **the server** —
including the musl release path for headless hosts, and any contributor's first
`cargo build`. That is precisely the bleed §VII.1 rule 2 exists to prevent; the rule was
written about crates and the violation happened one level up, at the workspace.

`crates/tack-desktop` is now excluded from the root workspace and is a workspace of its
own. Consequences, all handled:

- Its own `Cargo.lock`, added to the `tack-generated` merge set and to the pre-push
  staleness gate.
- Its own CI job (`desktop`), which installs the Linux system dependencies and runs fmt,
  clippy and tests — without it nothing would compile this crate at all. It warms the root
  registry cache first, because `dependency_boundary.rs` reads the root tree `--offline`.
- Its own Dependabot entry for `/crates/tack-desktop`.
- `tack_cli_stays_free_of_webview_and_gtk_dependencies` now passes
  `--manifest-path ../../Cargo.toml`, since `-p tack-cli` no longer resolves from here.
- Package metadata is duplicated rather than inherited, so a new test asserts the crate's
  version matches the root `[workspace.package]` **and** `tauri.conf.json`. Proven
  load-bearing: changing the version in `tauri.conf.json` alone fails it.

For VII-B2 and VII-B3: work inside `crates/tack-desktop` and its own workspace. Adding a
dependency there does not touch the root lockfile, and must not.
