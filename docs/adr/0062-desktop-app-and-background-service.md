# ADR 0062: Tack runs as a background service, and ships as a desktop app

**Decide:** approve that Tack's normal install becomes a desktop application — its own
window, its own icon, an icon in the system tray — that starts and supervises the Tack
server (the board plus the embedded runner) as a background process, so closing the
window never stops the work. The `tack` binary stays exactly what it is, for servers and
the terminal.

**Why now:** today a person starts Tack in a terminal and opens a browser tab. The board
and the runner already survive a closed tab, but nothing survives the terminal: closing
it kills a running agent attempt. That is the whole gap between "a web app I run by hand"
and "an app like Docker Desktop", and every onboarding improvement in Part VI still ends
with "open a terminal and run `tack serve`".

**If you do nothing:** Tack stays a terminal-started server, and the surface map's last
console step — starting Tack itself — never goes away.

## The eight decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | The desktop app is a separate program (`tack-desktop`, built with Tauri 2), not a mode of the `tack` binary. | A window needs the system webview and, on Linux, GTK. The server binary for a VPS or a CI box must never carry those. |
| 2 | The app bundles the `tack` binary and runs `tack serve --with-runner` as a supervised child process; it never re-implements the server. If a Tack is already serving on the port, the app attaches to it instead of starting a second one. | One server, tested once. The app is a supervisor and a window — the way Docker Desktop supervises its engine. |
| 3 | Closing the window hides it to the tray and the server keeps running. "Quit" in the tray stops the server, warning first when an agent attempt is in flight. "Launch at login" is a visible toggle, on by default. Only one instance runs. | This is the daemon promise stated where the user can see it: the work outlives the window, and only an explicit Quit ends it. |
| 4 | The window shows the same web UI, loaded from the local server. No second frontend, no app-only API. Native calls exist only for what a browser cannot do: tray, autostart, quit, open a folder. | Zero duplication. Everything the browser user can do, the app user can do, and the other way round. |
| 5 | Under the app, data lives in the operating system's per-user application folders, handed to the server through the `TACK_*` variables that already exist. The `tack` binary's own defaults (files next to where it was started) do not change. | Apps do not write next to wherever they were launched; servers and CLI users keep the behaviour they have. |
| 6 | The embedded runner obeys the same switch it has today: off until the user turns it on in the UI. The app does not turn it on by itself. | Installing the app is not consent to run agents. The safety default of ADR 0058 and the UI switch of ADR 0061 stay as they are. |
| 7 | Releases ship the app for Linux (`.deb`, `.AppImage`), macOS (`.dmg`) and Windows (`.msi`) next to the existing archives — unsigned until certificates exist. | Signing costs money (an Apple developer account, a Windows code-signing certificate) and is a separate decision. Unsigned builds work today, with a documented one-time warning on macOS and Windows. |
| 8 | The terminal path gets the same daemon: `tack service install` registers a user-level service (systemd on Linux, launchd on macOS) using the same data folders as the app. | A CLI or server user deserves "outlives the terminal" too, without installing the app. |

If you accept this table, you have accepted the ADR — record the date below. Everything
past this point is supporting detail for whoever implements or later audits one of these
eight calls; nothing above depends on anything below it.

---

- **Status:** accepted 2026-09-03 — recorded as a dated amendment at the bottom of this
  file.
- **Date:** 2026-09-03
- **Relationship to earlier ADRs:** refines ADR 0058 (`0058-standalone-single-binary-runner.md`).
  The *server* stays one binary; the app is a second program for the normal case, the same
  way `tack-runner` is a second program for the remote case. ADR 0050, 0059 and 0061 are
  unchanged.
- **Wire contract:** unaffected. `docs/contracts/runner-v1/` gains nothing from this ADR.

## Full reasoning

*(For implementers and reviewers. If you only needed to approve the eight decisions above,
you're done reading.)*

### Background

What exists today, measured in this tree:

- `tack serve --with-runner` runs the board and the runner in one process, loopback-only,
  off by default (`crates/tack-cli/src/local_runner.rs`). The web UI is embedded in the
  same binary (`embed-spa`) and served same-origin, so the browser needs no token on a
  loopback bind.
- The UI already survives a dropped connection: `frontend/src/shared/realtime/boardSocket.ts`
  reconnects with capped exponential backoff and re-fetches state. A closed tab loses
  nothing. The terminal is the only weak point.
- The server's defaults are relative to the current directory: `sqlite:tack.db`,
  `./storage`, `.tack-runner`, `logs/` (`crates/tack-api/src/config.rs`,
  `crates/tack-runner/src/config.rs`). Fine for a server started from a known folder;
  wrong for an app launched from an icon.
- Releases already build for `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`,
  `x86_64-apple-darwin` and `x86_64-pc-windows-msvc` (`.github/workflows/release.yml`),
  so every platform the app targets already has a `tack` binary to bundle.
- Tauri 2 provides what the app needs without custom native code: a sidecar mechanism
  (`bundle.externalBin`, spawned through `tauri-plugin-shell`), a tray (`tray-icon`
  feature; `TrayIconBuilder`, `Menu`, `MenuItem`), and official plugins for autostart
  (`tauri-plugin-autostart`), single instance (`tauri-plugin-single-instance`), window
  state and the updater. Linux builds need `libwebkit2gtk-4.1-dev`,
  `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev` and `libssl-dev` on the
  build machine; Windows needs WebView2, present on Windows 10 1803 and later; macOS
  needs the Xcode command-line tools. Fetched 2026-09-03; re-check before the first card
  pins versions.

### 1. A separate program, not a feature flag on `tack`

**Rejected — a `desktop` cargo feature of the `tack` binary.** It would link the webview
and GTK into the one binary that also runs on a headless VPS, and the Linux release is a
static musl build that GTK cannot join. CI would have to build two variants of the same
binary anyway, which is a second program with a confusing name.

**Rejected — Electron or any non-Rust shell.** A second runtime, hundreds of megabytes,
and a second language in a workspace whose whole pitch is one small Rust binary.

### 2. Supervise the real `tack`, never re-implement it

**Chosen:** the app ships the platform's `tack` binary as a Tauri sidecar and runs
`tack serve --with-runner` with the data-folder variables from decision 5. Before starting
one, it checks the configured port: if a Tack already answers `/api/health`, the app
attaches to it — shows it in the window, reflects its state in the tray — and never stops
a server it did not start. Attach also checks the server's version and refuses to drive a
server older than the app's bundled one, with a message that names both.

**Rejected — linking `tack-api` into the app and running the server in-process.** The
composition root (server plus embedded runner, the loopback guard, the runner gate)
lives in `tack-cli` and is tested there. Copying it into the app means two places to keep
the runner gate correct. It also means an app crash takes the server with it, which is
strictly worse than a child process the app can re-attach to after a restart.

### 3. The window is a view; Quit is the only way to stop

**Chosen:** close hides (`WindowEvent::CloseRequested` → prevent, hide); the tray menu has
*Open Tack*, the runner's status, *Launch at login*, and *Quit*. Quit asks the server for
in-flight attempts and warns before stopping when there are any. Launch at login is on by
default and visible on the first-run screen and in the tray. Single instance: a second
launch focuses the existing window.

**Rejected — Quit that only hides.** Docker's early behaviour, and the single most
confusing thing it ever did; a user must be able to actually stop the thing.

**Rejected — launch at login off by default.** Then the daemon promise fails at the first
reboot, silently. The toggle is visible; on by default is the honest default for a
service whose point is to keep running.

**Known limit:** Tauri's tray does not emit click events on Linux (the icon shows; the
menu works). Every tray action lives in the menu; nothing depends on clicking the icon.

### 4. One UI, loaded from the local server

**Chosen:** the window navigates to `http://127.0.0.1:<port>/` served by the sidecar,
exactly what a browser tab shows. The server binds loopback without a token, which is the
posture `tack serve` already has on loopback. Native features are exposed to the page
through a handful of Tauri commands and are feature-detected, so the same UI keeps
working in a plain browser.

**Rejected — bundling the SPA into the app as well.** Two builds of the same frontend
drift; the API becomes cross-origin, forking CORS and token handling into an app-only
path. The whole point of decision 2 is that there is one server and one UI.

### 5. Data in the OS's application folders

**Chosen:** the app computes the platform's per-user data directory (`~/.local/share/tack`
on Linux, `~/Library/Application Support/tack` on macOS, `%APPDATA%\tack` on Windows) and
passes `TACK_DATABASE_URL`, `TACK_STORAGE_DIR`, `TACK_RUNNER_STATE_DIR` and
`TACK_LOG_FILE` under it to the sidecar. The first-run screen shows the location and lets
the user point at an existing `tack.db` instead; that choice is the app's own setting.
`tack service install` (decision 8) uses the same folders, so a person who moves from the
terminal to the app, or back, finds their data in the same place.

**Rejected — changing the server's own defaults.** Every existing server, container and
systemd unit relies on the current-directory defaults; changing them is a migration for
people who never asked for an app.

### 6. The runner switch is unchanged

The app passes `--with-runner`, which makes the runner *available* on the loopback bind;
whether it *runs* stays behind the UI switch ADR 0061 decision 6 adds, persisted in
`app_meta`, off on a fresh install. The tray shows the switch's state; flipping it is
done in the UI. Installing an app is not consent to execute agents on your machine.

### 7. Ship unsigned, say so

**Chosen:** Tauri bundles for Linux (`.deb`, `.AppImage`), macOS (`.dmg`) and Windows
(`.msi`) are built in the release workflow and attached to the release page next to the
archives that exist today. They are unsigned. The release notes and the install page say
exactly what that means: on macOS, right-click → Open once; on Windows, SmartScreen's
"More info → Run anyway" once. Signing is a separate decision with money attached and is
not made here.

**Rejected — wait for signing before shipping any app build.** That turns a paid
certificate into a blocker for the product's main install path. Unsigned first,
signed when the decision is made.

### 8. The same daemon for the terminal

**Chosen:** `tack service install` writes a user-level unit — systemd
(`~/.config/systemd/user/tack.service`, `WantedBy=default.target`) or launchd
(`~/Library/LaunchAgents/…tack.plist`) — that runs the installed `tack` binary with
`serve --with-runner` and decision 5's folders; `uninstall` and `status` complete the set.
Windows returns a typed "unsupported: use the desktop app" rather than a half-built Task
Scheduler path.

## Consequences

- Part VI's surface map (`TODO.md` §VI.0) ends with "starting Tack itself is the one
  console step left". This ADR is the answer to that row; the row is rewritten when both
  land, by the card that reconciles Part VI's docs (VI-D1).
- `README.md`'s "Run it" leads with the app for the normal case and keeps the binary for
  servers and the terminal. The README and `docs/screenshots/**` are shared with Parts V
  and VI under the conflict rules already in `TODO.md`.
- The workspace gains `crates/tack-desktop`; CI gains the Tauri Linux prerequisites and
  per-platform bundle steps; the release page gains four artifacts.
- Part VII's board (`TODO.md` §VII.0–§VII.6) and dispatch plan
  (`docs/agent-handoffs/part-vii/README.md`) implement this ADR card by card.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-03 — ACCEPTANCE, recorded by the integrator on the user's behalf.** The user
accepted this ADR in chat with the word "listo" in reply to the integrator's explicit
request to accept ADRs 0061 and 0062, and asked for the plan to be updated and prepared for
multiple Sonnet agents. The acceptance covers the eight decisions as they stand in the
table above at this date. One implementation detail was pinned after acceptance without
changing a decision: the data folder is named lowercase `tack` on every operating system
(the prose under decision 5 used a capital on macOS and Windows; the board's table in
`TODO.md` §VII.0 is the authority). If the user disagrees with this reading, they strike
this paragraph and Wave 18 stops. **User acceptance date: 2026-09-03.**
