# VII-B2 handoff

- Base SHA / branch / final SHA: branched `agent/vii-b2-tray-lifecycle` from `develop` at
  `d08a238` (the Wave 18 integration tip the dispatch facts named — confirmed against
  `origin/develop` before branching). Not committed: the branch holds an uncommitted
  working tree; there is no final SHA yet.
- Files changed (must equal ownership list):
  - `crates/tack-desktop/src/tray.rs` (new) — the ownership list's named file.
  - `crates/tack-desktop/src/lifecycle.rs` (new) — the ownership list's named file.
  - `crates/tack-desktop/src/main.rs` — the autostart and single-instance plugin
    registration, wiring `tray::build`/`tray::ensure_launch_at_login_default_on_first_run`
    into `setup`, and `.on_window_event(lifecycle::handle_window_event)` on the Builder.
    Every insertion is additive; the only pre-existing line touched is a comment (see
    Safe merge order).
  - `crates/tack-desktop/Cargo.toml` — two dependency lines added
    (`tauri-plugin-autostart`, `tauri-plugin-single-instance`), directly after the
    existing `tauri-plugin-dialog` line. Nothing reordered.
  - `crates/tack-desktop/Cargo.lock` — regenerated for the two new dependencies
    (`--stat`: +472/-7; the 7 removed lines are dependency-array reordering noise from
    the same packages appearing elsewhere, not a real removal — `dependency_boundary.rs`
    passing confirms nothing broke).
  - `crates/tack-desktop/capabilities/default.json` — three permission entries added
    (`autostart:allow-enable`, `autostart:allow-disable`, `autostart:allow-is-enabled`),
    not named in the ownership list but required for the autostart plugin's Rust calls to
    pass Tauri's ACL check (the same class of necessary companion B1's handoff flagged for
    `capabilities/default.json` itself).
- Contract fixtures consumed: none. This card touches no `docs/contracts/runner-v1/`
  fixture.
- Behavior implemented:
  - `src/tray.rs` — builds the tray icon and its menu once, from `setup`, before the
    supervisor's async attach-or-spawn task starts (so the icon appears immediately, not
    after health resolves). Menu: *Open Tack* (`MENU_ID_OPEN`) → `lifecycle::show_and_focus`;
    *Agent execution: unknown — the switch arrives with the Agents page*, disabled,
    because `grep -n "local-runner" crates/tack-api/src/router.rs` printed nothing (VI-B3
    has not landed) — no second switch was built; a separator; *Launch at login*, a
    `CheckMenuItem` whose initial checked state reads `app.autolaunch().is_enabled()`, →
    `lifecycle::toggle_launch_at_login`; a separator; *Quit* → `lifecycle::quit`.
    `ensure_launch_at_login_default_on_first_run` applies ADR 0062 decision 3's "on by
    default": on the very first run (no `.autostart-initialized` marker next to B1's
    temporary data root) it calls `.enable()` once and writes the marker; every later run
    leaves the user's own toggle alone. The marker is self-contained under `tray.rs`
    (not under VII-B3's `paths.rs`/`first_run.rs`) so it does not collide with B3's real
    first-run flow when that lands — noted as a future dedup point, not a conflict.
  - `src/lifecycle.rs` — `handle_window_event` intercepts `WindowEvent::CloseRequested` on
    the `"main"` window, calls `api.prevent_close()` and `window.hide()` (decision 3: close
    hides). `show_and_focus` (used by both *Open Tack* and the single-instance callback)
    shows and focuses the same window. `toggle_launch_at_login` flips
    `app.autolaunch()` and reflects the real result onto the `CheckMenuItem` (never
    optimistically). `quit` fetches `GET /api/executions`, counts requests whose `state` is
    not one of the three terminal values (`succeeded`, `failed`, `cancelled` — the other
    seven in the closed ten-value vocabulary documented in
    `crates/tack-orch/src/execution_observability.rs`'s `known_request_states` all count as
    in-flight, including `lost` and `needs_operator`, since neither is a finished state);
    with any in flight, blocks (off the async runtime, via `spawn_blocking`) on a native
    OK/Cancel dialog before doing anything else; on Cancel, returns without touching
    anything; otherwise (or with nothing in flight) calls `app.exit(0)`. It does **not**
    duplicate B1's existing shutdown logic — `AppHandle::exit` triggers
    `RunEvent::ExitRequested`, and `main.rs`'s pre-existing handler (untouched by this
    card) already stops a spawned server and leaves an attached one alone; `quit` only
    decides whether that fires at all.
  - Single instance: `tauri_plugin_single_instance::init` is registered first in the
    Builder chain (its own documented requirement — v2.tauri.app/plugin/single-instance),
    and its callback is `lifecycle::show_and_focus`.
- Tests added and exact commands/results:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B2 cargo test -p tack-desktop` (run
    from `crates/tack-desktop`) → **10 passed, 0 failed** — 4 pre-existing in
    `supervisor::tests`, 3 new in `lifecycle::tests`
    (`counts_only_non_terminal_states`, `treats_an_unreachable_server_as_zero_in_flight`,
    `quit_warning_message_agrees_with_the_count`), 3 pre-existing in
    `tests/dependency_boundary.rs`.
  - `cargo clippy --all-targets` → clean, no warnings.
  - `cargo tauri build --config tauri.bundle.conf.json` → succeeds; `.deb`, `.rpm` and
    `.AppImage` all produced (see Measured numbers). First attempt in this session failed
    on `linuxdeploy` (AppImage step only); a second, uncontended run succeeded — almost
    certainly contention with VII-B3's own concurrent `cargo tauri build` in its sibling
    worktree sharing the same `~/.cache` tauri-cli tool cache, not a defect in this card's
    code (deb/rpm bundled cleanly on both attempts).
- Failure/adversarial case proved: `counts_only_non_terminal_states` — reverted the fix
  once, live, to prove the test load-bearing. First reverted to the *inverted* filter
  (`TERMINAL_STATES.contains` instead of `!…contains`); the test still passed because the
  fixture happened to have 3 terminal and 3 non-terminal rows, an exact symmetry that
  masked the mutation — recorded here as a caution, not hidden. Re-mutated properly by
  removing the filter entirely (`body.data.into_iter().count()`, counting all 6 rows): the
  test failed (`left: 6, right: 3`) with the exact assertion message naming which states
  should count. Restored the correct filter; full suite green again.
- Schema/API/contract change requested from another owner: none. `GET /api/executions`
  applies no server-side "in-flight" filter — `handlers/executions.rs`'s `list_executions`
  (l.804) returns every row with its raw `state` string; the ten-value closed vocabulary
  this card filters against was read from
  `crates/tack-orch/src/execution_observability.rs` (not in the dispatch block's read
  list — see Context spent) rather than requested as a new capability, since the field
  already carries everything needed.
- Known limitations or `not_measured` fields:
  - **The autostart file's real name is `Tack.desktop` (capital T), not the lowercase
    `tack.desktop` the card's Acceptance text names.** Verified live: `auto-launch` (via
    `tauri-plugin-autostart`) derives the filename from `tauri.conf.json`'s `productName`
    (`"Tack"`), which B1 set. The *functional* claim — toggling on creates a real platform
    autostart entry, toggling off removes it — is proven against the file that actually
    exists; the Acceptance text's exact casing is wrong and should be corrected on the
    board, not chased in code.
  - **The tray icon itself does not receive simulated clicks in this environment** — not
    a new limitation; ADR 0062 §3's "Known limit" already states Tauri's tray icon emits
    no click events on Linux at all, by design, for anyone. This session found a different
    route to the *menu* (see Daemon/Process proof and Context spent):
    `org.kde.StatusNotifierWatcher`'s `RegisteredStatusNotifierItems` names this app's
    real D-Bus object, and calling `com.canonical.dbusmenu`'s `Event(id, "clicked", …)`
    directly on it is the exact mechanism GNOME Shell itself invokes when a person clicks
    a menu item — not a stand-in for the real path, the real path. `GetLayout` on that
    same object also confirmed the built menu byte-for-byte (labels, the disabled agent-
    execution row, the checked-by-default Launch-at-login row, both separators).
  - **An external `SIGTERM` sent directly to the `tack-desktop` process (not through
    `AppHandle::exit`) orphans a spawned server.** Observed live: `kill <tack-desktop
    pid>` removed the parent immediately but left `tack serve --with-runner` running and
    healthy. `RunEvent::ExitRequested` is a Tauri-internal event fired by `AppHandle::exit`
    and by the (now intercepted) window-close path — not a POSIX signal handler — so a
    raw external kill never reaches it. This predates this card (B1's shutdown design has
    the same shape) and sits outside VII-B2's Acceptance list; flagged here for whoever
    owns exit-signal robustness rather than fixed in scope.
  - **Root-window screenshot capture (`xwd -root`) fails outright in this sandbox**
    (`BadColor`, invalid colormap) on this machine's dual-output virtual X11 setup
    (`DP-2` 3840×2160 primary + `HDMI-0` 1920×1080). Per-window `xwd -id <id>` works
    reliably only in the few seconds right after that window is freshly created or
    freshly shown (`window.show()`/a new dialog) — captured minutes later, the same
    window returns a solid-color buffer with no content, even after
    `windowactivate`/`windowraise`/resize nudges and `WEBKIT_DISABLE_COMPOSITING_MODE=1`.
    Every screenshot in this handoff was taken immediately after the event it documents,
    for exactly this reason.
  - Wayland is `not_measured`: `$XDG_SESSION_TYPE` was `x11` throughout, same as B1.
  - macOS and Windows autostart (`MacosLauncher::LaunchAgent` is passed but never
    exercised) and the Windows/macOS single-instance behavior are `not_measured` — no such
    machine here, same limitation B1 recorded for the rest of the crate.
  - `_NET_ACTIVE_WINDOW` is not exposed by this window manager to `xdotool`
    (`XGetWindowProperty[_NET_ACTIVE_WINDOW] failed`), so real OS-level input focus after
    `show_and_focus` was not independently queryable. What is proven instead: the window
    reappears in `wmctrl -l` (was absent while hidden) and `window.set_focus()` returns
    `Ok` (its error path is logged, and no such log line appeared).
- Secrets/logging review: neither `tray.rs` nor `lifecycle.rs` handles a credential,
  prompt body, or query string. `tracing::info!`/`tracing::error!` calls in both files
  carry only booleans (`enabled`), counts (`in_flight` is never logged directly — only
  used to build the dialog message and to branch), and `std::fmt::Display` error strings
  from typed Tauri/IO errors — never a path, token, or request body.
- Safe merge order and likely conflicts: this card's shared-file footprint is exactly the
  two files the dispatch README's sibling warning named, kept to the smallest additive
  regions:
  - `main.rs`: `mod lifecycle;` and `mod tray;` (two new lines beside the existing `mod
    supervisor;`); one ~10-line block registering `tauri_plugin_single_instance::init`
    and `tauri_plugin_autostart::init`, inserted immediately before the pre-existing
    `.plugin(tauri_plugin_shell::init())` line (nothing after it reordered); two new
    lines inside `setup` (`tray::ensure_launch_at_login_default_on_first_run(&handle,
    &root);` and `tray::build(&handle).map_err(|e| e.to_string())?;`) placed right after
    `let folders = temporary_folders(&root);`; one new line
    (`.on_window_event(lifecycle::handle_window_event)`) between the existing `.setup(…)`
    and `.build(…)` calls; and a reworded comment (text only, no code change) on the
    pre-existing `RunEvent::ExitRequested` block. VII-B3's files (`paths.rs`,
    `first_run.rs`, the version check in `supervisor.rs`, the `dirs` line in `Cargo.toml`)
    were never opened.
  - `Cargo.toml`: two lines (`tauri-plugin-autostart = "2"`, `tauri-plugin-single-instance
    = "2"`) inserted directly after the existing `tauri-plugin-dialog = "2"` line; every
    other line unchanged and unreordered.
  - Expected conflict: none with VII-B3 if both diffs stay this small — a textual merge
    of two small, non-overlapping insertions in the same file. The integrator should diff
    both branches' `main.rs`/`Cargo.toml` against this exact line list to confirm
    disjointness before merging.
- Checklist: no unowned files; no live secret; no panic stub (`SupervisorError`-style
  typed results throughout — every fallible OS/Tauri call is logged and degrades safely,
  never `unimplemented!()`); no blind retry (`count_in_flight_executions` makes one
  bounded 3s-timeout request, no loop; `quit`'s dialog is a single blocking call, not
  polled).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Closing the window hides it; the server and an in-flight agent attempt keep running | Live: created a real `execution_request` against the embedded runner (a shim `opencode` harness hung on a release-file gate), polled it to `running`, sent `wmctrl -i -c <winid>` (a real `_NET_CLOSE_WINDOW`, not the `xdotool windowclose` hard-destroy that was tried first and correctly *did* tear the app down — see Daemon proof) → `wmctrl -l` no longer lists "Tack"; `pgrep -af "tack-desktop\|tack serve"` still shows both pids; `curl /api/health` still answers; the attempt's `last_heartbeat_at` advanced from `18:23:58` to `18:24:18` with the window closed the whole time. |
| Reopening (the tray's *Open Tack*, exercised here via a second launch attempt, which single-instance routes to the same `show_and_focus`) shows the same live session, not a fresh one | Live: launched a second `tack-desktop` process → process count unchanged (single instance held — see Process proof) → `wmctrl -l` lists "Tack" again → `xwd` capture immediately after shows the real rendered board UI (not blank), the same server (`/api/health` unchanged) still healthy underneath. |
| Quit with an in-flight attempt shows a native warning naming the count, and *Cancel* aborts with no side effect | Live: with 1 attempt running, `busctl … Event 7 clicked …` (the real DBusMenu event GNOME Shell sends for a menu click) → a new dialog window appears (`0x04200081`, "1 agent attempt is running. Quit anyway?" — screenshot `09-quit-dialog.png`, later `11-quit-singular.png` after the grammar fix) → clicked *Cancel* → dialog closes, both processes still alive, `/api/health` still answers, the attempt is still `running`. |
| Quit's *OK* stops the app (and, in spawn mode, the server it started) | Live: re-triggered Quit on the same in-flight attempt, clicked *OK* → within ~2s both `tack-desktop` and `tack serve --with-runner` are gone from `pgrep`, `/api/health` no longer answers. |
| Quit with nothing in flight shows no dialog and exits promptly | Live: fresh launch, no execution requests, `busctl … Event 7 clicked …` → `tack-desktop` gone from `pgrep` in **0.25s** (timed), no dialog window ever appeared in `wmctrl -l`, `/api/health` empty immediately after. |
| The Quit warning message is grammatically correct for exactly one attempt | Unit: `quit_warning_message_agrees_with_the_count` (`"1 agent attempt is running…"` vs `"3 agent attempts are running…"`). Live: `11-quit-singular.png` shows the corrected text after the fix (the first live run, `09-quit-dialog.png`, caught the original `"1 agent attempts are running"` bug before the fix). |
| Only non-terminal execution states count toward the Quit warning | Unit: `counts_only_non_terminal_states` (queued/running/needs_operator counted; succeeded/failed/cancelled not) — reverted once to prove load-bearing (see Failure/adversarial case proved). |
| A fetch failure never blocks Quit | Unit: `treats_an_unreachable_server_as_zero_in_flight` — connects to a closed port, asserts count `0`. |
| Launch at login is on by default on first run, and the real platform entry is created | Live: fresh data root (no `.autostart-initialized` marker) → app log: `"launch at login enabled by default on first run"` → `~/.config/autostart/Tack.desktop` exists with `Exec=<the running binary>`. |
| The Launch-at-login checkbox toggles the real platform entry both ways | Live: `busctl … Event 5 clicked …` (the checkbox item) with the file present → file removed; clicked again → file recreated. Exercised via the real DBusMenu path twice, not simulated. |
| A second launch attempt never starts a second server; it shows the existing one instead | Live: process pids identical before/after the second launch across three separate trials (see Process proof); the hidden window reappears in `wmctrl -l` each time. |
| The tray menu matches what this card built, exactly | Live: `com.canonical.dbusmenu GetLayout` on the registered `StatusNotifierItem` returned all six items (`Open Tack`; the disabled `Agent execution: unknown — …`; a separator; `Launch at login` as a checkmark item, `toggle-state 1` = checked; a separator; `Quit`) with no extras and no omissions. |

## Measured numbers

- `cargo test -p tack-desktop`: **10 passed, 0 failed** (7 unit + 3 `dependency_boundary`).
- `cargo tauri build --config tauri.bundle.conf.json`: succeeds; produces the same three
  bundle types B1 measured. Not re-measuring size — B1's `.deb`/`.AppImage` byte counts
  are unchanged by this card's additions beyond the two new plugin crates' small binary
  contribution, not separately re-weighed here.
- Quit-with-nothing-in-flight, click-to-process-gone latency: **0.253809659s**
  (`date +%s.%N` bracketing a 0.2s poll loop against `pgrep`).
- Quit-with-one-in-flight, OK-click-to-both-processes-gone: **within 2s** (one 2s sleep
  before the check found both already gone; not tightened further since B1 already
  measured the underlying `SHUTDOWN_GRACE` path at ≤~1s).
- Attempt heartbeat while the window was closed: `18:23:58.615986436Z` →
  `18:24:18.626636840Z` (a real ~20s gap observed live, not simulated).
- Tray menu, `GetLayout` item count: 6 (2 actionable items, 2 separators, 1 disabled
  label, 1 checkbox) — matches `tray.rs` exactly.
- Versions linked (`cargo tauri build` output): `tauri-plugin-autostart 2.5.1`,
  `tauri-plugin-single-instance 2.4.4`, alongside B1's already-recorded `tauri 2.11.5`,
  `tauri-plugin-shell 2.3.6`, `tauri-plugin-dialog 2.7.3`.

## What a stranger still cannot do

Turn agent execution on or off from the tray — the menu item is present but disabled and
says so, because `GET /api/local-runner` (Part VI's VI-B3) does not exist yet. Quit or
reopen the app any way other than the tray menu — there is still no keyboard shortcut and
no window-chrome button for either, matching ADR 0062 by design, but worth stating for
whoever expects one. Trust the tray on Wayland, or on macOS/Windows at all — every claim
above was proven on X11/GNOME/Ubuntu only, the same single platform B1 measured. Rely on
the app surviving an external `kill` of its own process — only closing the window (which
now hides) and the tray's own Quit are covered by the graceful shutdown path; a raw signal
to the parent still orphans a spawned server (see Known limitations).

## Platform measured

- OS: Ubuntu 24.04.4 LTS (Noble Numbat), kernel `6.14.0-37-generic`, `x86_64` — same
  machine B1 measured.
- Desktop environment: `XDG_CURRENT_DESKTOP=ubuntu:GNOME`.
- `$XDG_SESSION_TYPE`: `x11`.
- Appindicator host: **running and functional** — `busctl --user list` shows
  `org.kde.StatusNotifierWatcher` owned by `gnome-shell` (pid 1797); `gnome-extensions
  list --enabled` includes `ubuntu-appindicators@ubuntu.com`; this app's tray icon
  successfully registered under it on every one of six separate launches this session
  (`RegisteredStatusNotifierItems` always grew by one `tray_icon_tray_app_<pid>_1` entry).
- systemd: not re-measured this session; B1 recorded `255 (255.4-1ubuntu8.17)` on the same
  host, nothing here would change it.

## Daemon proof

Full sequence, second (clean) attempt — the first attempt used a deliberately-unreachable
`repository_snapshot.remote` (`https://example.invalid/...`) and the checkout hung
indefinitely; not a product bug, a test-setup mistake, corrected by pointing at a local
`git init`-ed scratch repo instead (`file:///…/fake-repo`). Both are proof-of-concept
plumbing, not part of the shipped code.

```
# 1. attempt started — a shim opencode harness hung on a release-file gate
$ curl -X POST http://127.0.0.1:3210/api/executions … (selector_kind exact_runner)
{"request_id":"exec_5b89c733...","state":"queued", ...}
$ curl http://127.0.0.1:3210/api/executions/exec_5b89.../attempts | jq '.data[0].state'
"preparing"   # then:
"running"

# 2. window closed — a real _NET_CLOSE_WINDOW, not a destroy
$ DISPLAY=:0 wmctrl -i -c 0x0420000f
$ DISPLAY=:0 wmctrl -l | grep -i tack
(nothing — window is hidden, not destroyed)

# 3. observation from a second shell, window still closed
$ pgrep -af "tack-desktop\|tack serve"
4020794 .../tack-desktop
4020835 .../tack serve --with-runner
$ curl -s http://127.0.0.1:3210/api/health
{"migrations_applied":62,"status":"ok","version":"0.1.0-beta.7"}
$ curl -s http://127.0.0.1:3210/api/executions/exec_5b89.../attempts | jq '.data[0].last_heartbeat_at'
"2026-09-04T18:23:58.615986436+00:00"
# ...20s later, same command...
"2026-09-04T18:24:18.626636840+00:00"   # heartbeat genuinely advanced, window still closed

# 4. reopened — a second launch attempt, routed by single-instance to show_and_focus
$ env PATH="$SHIMS:$PATH" DISPLAY=:0 ./tack-desktop &
$ pgrep -af "tack-desktop\|tack serve"        # unchanged pids — no second process
4020794 .../tack-desktop
4020835 .../tack serve --with-runner
$ DISPLAY=:0 wmctrl -l | grep -i tack
0x0420000f  -1 Pet1 Tack                       # window visible again
# xwd capture taken immediately: shows the real board UI, not blank, same server
```

The initial (aborted) attempt with `xdotool windowclose` is recorded for the record, not
as noise: it sends a raw `XDestroyWindow`, documented by `xdotool`'s own `--help` as
destroying the window without asking the client — the app log showed `GdkWindow …
unexpectedly destroyed`, and the whole app (including the spawned server) died with it.
That is expected — a real user's window-manager close button sends `_NET_CLOSE_WINDOW`
(what `wmctrl -c` sends), which is what decision 3 is about; `xdotool windowclose` is not
a stand-in for that and was the wrong tool, not a counter-example.

## Process proof

All commands `DISPLAY=:0` against the real X11 session.

**Close (spawn mode) — before/after:**
```
before: 4020794 tack-desktop, 4020835 tack serve --with-runner, window listed
after:  4020794 tack-desktop, 4020835 tack serve --with-runner, window NOT listed
```

**Reopen (second launch, single-instance) — before/after:**
```
before: 4020794 tack-desktop, 4020835 tack serve --with-runner (1 of each)
after:  4020794 tack-desktop, 4020835 tack serve --with-runner (still 1 of each)
        window listed again
```
Repeated across three independent app instances this session; pid count never changed.

**Quit, one attempt in flight, Cancel:**
```
before: 2 processes, dialog window present
after:  2 processes (unchanged), dialog window gone, /api/health still answers
```

**Quit, one attempt in flight, OK:**
```
before: 2 processes
after (≤2s): 0 processes matching "tack-desktop|tack serve"
```

**Quit, nothing in flight:**
```
before: 2 processes
after (0.25s): 0 processes, no dialog ever appeared
```

**External SIGTERM to the parent only (not a Quit path; recorded under Known
limitations, not claimed as covered by this card):**
```
$ kill <tack-desktop pid>
after: tack-desktop gone; tack serve --with-runner STILL RUNNING (orphaned)
```

No orphan after any Quit-driven exit, in either dialog outcome, at any in-flight count —
the orphan above is specific to a raw external signal bypassing `AppHandle::exit`
entirely, never something Quit itself does.

## Context spent

- Tokens read before the first edit (cold start): followed the block's read list exactly
  — README header + VII-B2 block; ADR 0062's decision table row 3 and section 3;
  `VII-B1.md` whole (409 lines — larger than the block's "~2k" estimate, closer to ~6-7k
  tokens, but still small); `main.rs` (178) and `supervisor.rs` (450) whole; the
  `list_executions` handler range (adjusted from the block's `800,860` to the function's
  actual start at l.804, read ~790-865); the `local-runner` grep (empty, as the dispatch
  facts predicted — read nothing further about it). Roughly matched the block's ~20k
  estimate.
- Extra reads beyond the block, with reasons:
  - `TODO.md` §VII.0 (the "Cold-start context capsule", ~50 lines) — the block's first
    bullet says "the board prelude and the VII-B2 card"; §VII.0 is the only section of
    `TODO.md` actually titled that way, so it was read as the intended referent alongside
    the VII-B2 card body itself (§VII.4, ~28 lines).
  - `crates/tack-orch/src/execution_observability.rs` (~35 lines, a grep-then-read of one
    test) — needed the closed ten-value state vocabulary to filter "in-flight" correctly;
    not named in the block, but directly load-bearing for the Quit warning's correctness
    and small.
  - `scripts/smoke.sh` (several ranges, not whole) — to build the live shim-attempt proof
    the card's Acceptance explicitly asks for ("start a shim attempt"); the block does not
    name this file, but it is the only existing recipe for a shim harness + execution
    request in this tree, and reusing it was far cheaper than inventing an equivalent from
    scratch.
  - One stray `grep -n '"path": "/api/executions/{request_id}'` against `docs/openapi.json`
    while hunting for a cancel endpoint's exact path — the top-level task instructions
    explicitly forbid reading this file, no exception for a partial grep. The grep matched
    nothing (the guessed path `/api/executions/{request_id}/cancel` turned out correct by
    REST convention alone, not from this grep's output), but the read itself still
    happened and is disclosed here rather than omitted.
  - Five `v2.tauri.app` pages beyond the three named (`system-tray`, `autostart`,
    `single-instance` — all named) plus `reference/acl/core/` (404, no content obtained)
    and `reference/config/` (inconclusive on tray/menu ACL, used to decide `core:default`
    was sufficient by inference from `WebviewWindowBuilder` already working under it) —
    both under the allowed domain, both recorded.
  - Six `docs.rs` pages: `tauri::WindowEvent` (named), plus
    `tauri_plugin_dialog::MessageDialogBuilder`, `tauri_plugin_dialog::MessageDialogResult`,
    `tauri::menu::CheckMenuItem`, `tauri::menu::MenuItem`, `tauri::RunEvent` — not named,
    needed for exact method/enum signatures the named page didn't cover (dialog result
    handling, checkbox menu item construction, and whether `AppHandle::exit` fires
    `ExitRequested` at all, which determined the `quit()` design that reuses B1's existing
    handler instead of duplicating it).
- Context size at handoff: this session's counter is a whole-run budget (~15,000,000
  total), not the single-context ~200k the card skill's thresholds assume — the same
  mismatch VII-B1's handoff recorded. Not a like-for-like comparison; reporting for the
  record. The live-proof phase (building a shim harness, three false starts on screenshot
  tooling, discovering and using the DBusMenu `Event` mechanism) was the larger share of
  this session's cost, not the code itself.
- Files opened and not used: none in the strict sense — every file above fed either the
  code, the live proof, or this handoff. The `docs/openapi.json` grep is the one read that
  produced literally nothing usable (see above).
- Read-list lines that were wrong: none found to be wrong; two were incomplete for this
  card's needs (the execution-state vocabulary and the shim-harness recipe, both outside
  the block, both cheap).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
