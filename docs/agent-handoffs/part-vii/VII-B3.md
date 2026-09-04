# VII-B3 handoff

- Base SHA / branch / final SHA: branched `agent/vii-b3-data-folders` from `d08a238`, the
  SHA the Wave 19 dispatch facts name as the Wave 18 integration tip (`git log` on
  `develop` confirms `d08a238` is HEAD there). Not committed: the branch holds an
  uncommitted working tree; there is no final SHA yet.
- Files changed (must equal ownership list, plus necessary companions flagged below):
  - `crates/tack-desktop/src/paths.rs` (new) — the ownership list's named file.
  - `crates/tack-desktop/src/first_run.rs` (new) — the ownership list's named file,
    also holds `settings.json` handling (the struct, load/save, the pure
    decision-from-answer function).
  - `crates/tack-desktop/src/supervisor.rs` — the version check (`VersionCheck`,
    `check_server_version`, `SupervisorError::OutdatedServer`), `attach_or_start` gained a
    `bundled_version` parameter, one stale doc comment fixed (see Amendments-style note
    below — no `not_measured` here, just a correction).
  - `crates/tack-desktop/Cargo.toml` — added `dirs = "6"` (named in ownership) and
    `semver = "1"` (not named; added because comparing prerelease versions like
    `beta.7`/`beta.10` correctly needs real semver ordering, not string comparison —
    flagged here per scope discipline, not hidden).
  - `crates/tack-desktop/Cargo.lock` (own workspace, own lockfile) — +2 lines only
    (`dirs`, `semver` as direct edges); both were already transitive dependencies via
    `tauri`, so nothing new was fetched. Root `Cargo.lock` untouched (verified via `git
    status --porcelain -- Cargo.lock` at the repo root: empty).
  - `crates/tack-desktop/src/main.rs` — **shared with VII-B2.** Wired `paths::DataPaths`
    and `first_run::ensure_settings` into `setup()` in place of B1's
    `temporary_data_root`/`temporary_folders` (deleted, per Context: "replacing B1's
    temporary root"); added the `bundled_version` argument to the `attach_or_start` call;
    added one new `Err(SupervisorError::OutdatedServer { .. })` match arm alongside the
    existing `PortOccupiedByOther` arm. **Did not touch:** the `.plugin(...)` list, the
    `.run(|app_handle, event| ...)` closure, or anything tray/lifecycle-shaped — those are
    B2's. Exact diff is in this handoff's "Safe merge order" section below for the
    integrator to check disjointness.
  - `crates/tack-desktop/capabilities/default.json` — **not in the ownership list**, added
    one line: `"dialog:allow-open"`. Necessary companion: `first_run.rs`'s file picker
    (`FileDialogBuilder::blocking_pick_file`) is a distinct Tauri permission from the
    already-present `dialog:allow-message` (confirmed via
    `https://v2.tauri.app/plugin/dialog/`, which documents `dialog:allow-open` as the
    permission gating the file-picker specifically); `generate_context!` refused to
    compile without it. B2 was not named as touching this file in the dispatch warning,
    but flagging it here in case B2's Quit dialog also needs a capability change.

- Contract fixtures consumed: none. This card touches no `docs/contracts/runner-v1/`
  fixture; the version check is app-side only, comparing two version strings that never
  cross the runner-v1 wire.
- Behavior implemented:
  - `paths::DataPaths::resolve()` computes the root as `dirs::data_dir()` + `tack`
    (lowercase on every OS, per the board's folder table overriding the ADR's prose) and
    creates it `0700` on Unix via `ensure_dir`. `DataPaths::server_folders(database_override:
    Option<&Path>)` builds the four `TACK_*` values, substituting only the database URL
    when an override is given — storage, runner state and the log stay pinned under the
    root regardless, matching the card's Context ("nothing else" besides the database path
    and port is ever overridden).
  - `first_run::ensure_settings(app, paths)` loads `settings.json` if it exists (silent,
    every launch after the first); otherwise shows a native Yes/No message dialog naming
    the data root, and on "yes" opens a native file picker filtered to `*.db`, then
    persists whatever was chosen (or `None` on "no" or a cancelled picker) plus the
    default port to `settings.json`.
  - `supervisor::check_server_version(server_version, bundled_version)` parses both as
    semver and returns `Outdated` only when both parse and the server is strictly older;
    an unparsable string on either side returns `Unknown`, which `attach_or_start` treats
    as compatible — not evidence of being outdated, so a version this app cannot even
    parse never blocks an attach. `attach_or_start` gained a `bundled_version: &str`
    parameter; on the attach path (an existing server answers health) it now returns
    `Err(SupervisorError::OutdatedServer { server_version, bundled_version })` before
    anything else touches that server, if the check says `Outdated`.
  - `main.rs` passes `env!("CARGO_PKG_VERSION")` as the bundled version — this crate's own
    compiled-in version, not a `tack --version` subprocess call. Chosen because
    `tests/dependency_boundary.rs` (from B1's Wave-18-integration amendment) already
    proves this crate's version is kept equal to the workspace version and to
    `tauri.conf.json`'s version, which is exactly the version a correctly-built release
    bundles as the sidecar; spawning the sidecar with a `--version` argv would also need a
    new `shell:allow-execute` capability scope (the current one is locked to `["serve",
    "--with-runner"]`) for no accuracy gain over the compile-time constant. Recorded as a
    design call, not hidden — see "What is left" in the final report for the one case this
    does not cover (a bundled binary swapped without rebuilding `tack-desktop`).
- Tests added and exact commands/results:
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B3 cargo test -p tack-desktop` → 23
    passed, 0 failed (20 in `src/main.rs`'s unit test tree across `paths`, `first_run` and
    `supervisor`; 3 in `tests/dependency_boundary.rs`, unchanged from B1 and still green).
  - `cargo clippy -p tack-desktop --all-targets` → clean, no warnings.
  - `cargo fmt --check` → clean (after one `cargo fmt` pass on the mechanically-edited
    test call sites in `supervisor.rs`).
  - `cargo tauri build --config tauri.bundle.conf.json` → succeeded twice (before and
    after adding `dialog:allow-open`), producing `.deb`, `.rpm` and `.AppImage`.
- Failure/adversarial case proved: `supervisor::tests::refuses_to_attach_to_a_server_older_than_the_bundled_version`
  starts a fake sidecar (Python HTTP server) reporting `0.1.0-beta.1`, calls
  `attach_or_start` with bundled version `0.1.0-beta.7`, asserts the exact
  `OutdatedServer { server_version, bundled_version }` values come back, and — the
  adversarial part — asserts the hand-started process is still running afterward and the
  launcher's `fail_next: true` poison was never tripped, proving refusal never spawns a
  second server and never signals the one it refused to use. Separately,
  `check_server_version_orders_prerelease_numbers_numerically` is the load-bearing proof
  for choosing `semver` over string comparison: `"0.1.0-beta.9"` vs `"0.1.0-beta.10"` would
  come out backwards under a naive string compare (`"9" > "10"` lexically) and the test
  pins the correct numeric ordering in both directions.
- Schema/API/contract change requested from another owner: none. Per the card's "Stop if"
  clause, checked what `GET /api/health` and `GET /api/openapi.json` carry today before
  writing any code: `crates/tack-api/src/debug.rs`'s `health` handler already returns
  `"version": env!("CARGO_PKG_VERSION")` (confirmed live: `{"status":"ok","version":"0.1.0-beta.7","migrations_applied":62}`),
  which `supervisor::HealthBody` (already defined by B1) already deserializes. No route
  was added or proposed — the existing health response was already the least invasive
  source and needed no owner request.
- Known limitations or `not_measured` fields:
  - **macOS and Windows are unit-tested only.** `paths::tests::root_appends_lowercase_tack_to_any_os_base`
    exercises this crate's own join logic (not `dirs::data_dir()`'s per-OS branch, which is
    `dirs`' own tested responsibility) against representative macOS- and Windows-shaped
    base directories. No live run on either OS — no such machine here, matching B1's
    precedent for the same gap.
  - **The version check compares this app's own compiled version, not a live `tack
    --version` of the actual bundled sidecar file on disk.** These are normally identical
    (proven by `dependency_boundary.rs`'s version-parity test) for any correctly built
    release, but if someone manually swapped `binaries/tack-<triple>` for a different
    build without rebuilding `tack-desktop`, the check would compare against the stale
    compiled-in version, not the swapped binary's real version. Not exercised by a test;
    judged low-probability and out of scope for this card (no release pipeline lets this
    happen today).
  - **First-run dialog on-screen rendering is not proved live.** Same precedent as B1's
    `PortOccupiedByOther` dialog: the decision logic (`settings_for_answer`) and the
    settings persistence are unit-tested; the actual `app.dialog().blocking_show()` /
    `blocking_pick_file()` calls were not driven through a real X11 window in this card,
    since doing so would require a second, independent GUI-automation pass beyond what
    this card's Acceptance list asks for (paths, override, version — not the dialog's
    appearance).
  - **`storage/` and `logs/tack.log` were not observed to be created merely by starting
    the server** in the live fresh-user proof — only `tack.db`, its WAL/SHM files and one
    migration backup appeared under the root within the few seconds the proof ran. Read no
    handler beyond `health` (per the read-list restriction), so this is reported as an
    observation, not diagnosed: the storage directory is plausibly created lazily on first
    attachment upload, and the log file's creation depends on logging configuration this
    card did not investigate. The four env vars are correctly *passed*; whether every one
    is eagerly consumed on startup is `not_measured` beyond `TACK_DATABASE_URL`.
  - **Server B1's temp-directory placeholder is now completely removed** — `main.rs` has
    no fallback path if `DataPaths::resolve()` fails (`dirs::data_dir()` returning `None`
    on some exotic environment); it surfaces as a setup error and the app fails to start,
    with a logged error, never a silent fallback to a wrong location. No test exercises the
    `None` branch specifically (would require faking `dirs::data_dir()`, which is not
    mockable without an abstraction this crate does not otherwise need).
- Secrets/logging review: no secret is handled by `paths.rs` or `first_run.rs`. `settings.json`
  holds only a filesystem path and a port number, both non-sensitive. Logs added
  (`tracing::error!` in `first_run.rs` and the new `main.rs` match arm) carry only error
  text, version strings and the server/bundled version — no credential, path content is
  limited to the settings-write-failure log which logs the io error, not file contents.
- Safe merge order and likely conflicts:
  - `main.rs`: my additive regions are (1) the `mod`/`use` block at the top (2 new `mod`
    lines, 1 new `use paths::DataPaths`, `DEFAULT_PORT`/`ServerFolders` dropped from the
    `supervisor::` import list since they're no longer named directly in this file), (2)
    the deletion of `temporary_data_root`/`temporary_folders` and their two call sites in
    `setup()`, replaced with `DataPaths::resolve()` + `first_run::ensure_settings(...)` +
    `paths.server_folders(...)`, (3) the `attach_or_start` call gaining a 6th argument
    (`env!("CARGO_PKG_VERSION")`), (4) one new `Err(SupervisorError::OutdatedServer {..})`
    match arm inserted between the existing `PortOccupiedByOther` and the catch-all `Err`
    arms. None of these four regions touch the `.plugin(...)` chain or the final
    `.run(|app_handle, event| ...)` closure, which is where B2's autostart/single-instance
    registration and Quit-dialog/close-to-hide changes are expected to land — verified by
    reading B1's handoff amendment describing exactly that closure as B2's target. Expect
    a clean textual merge if B2's diff stays in those regions as its own handoff should
    describe.
  - `Cargo.toml`: two lines appended after `thiserror = "2"`, before the `[target.'cfg(unix)'.dependencies]`
    header. B2 likely appends its own plugin dependency lines (`tauri-plugin-autostart`,
    `tauri-plugin-single-instance`) to the same `[dependencies]` block — a textual
    conflict is plausible if both land at the same anchor line; trivially resolved by
    keeping both sets of lines, order does not matter in a Cargo.toml dependency table.
  - `capabilities/default.json`: one line added (`"dialog:allow-open"`) to the end of the
    `permissions` array. If B2 also needs a new permission for its Quit dialog, same
    shape of trivial conflict, same trivial resolution (keep both entries).
- Checklist: no unowned files edited without flagging (capabilities/default.json flagged
  above, same pattern B1 used for its own three necessary companions); no live secret; no
  panic stub (`PathsError`/`SupervisorError::OutdatedServer` are typed enum variants, not
  `unimplemented!()` or a swallowed error); no blind retry (dialog and file-picker calls
  are one-shot; no loop anywhere in this card's new code).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A fresh user's data lands exactly under `$XDG_DATA_HOME/tack` (Linux), never the app's own working directory | Live: `XDG_DATA_HOME` pointed at a fresh temp dir, root absent beforehand, `tack serve --with-runner` started from a *different* cwd with the four env vars `paths.rs` would compute; `find $ROOT -maxdepth 2` shows `tack.db`, `tack.db-wal`, `tack.db-shm`, one migration backup; `find $CWD_TEST -maxdepth 1 -iname tack.db` empty. See "Fresh-user proof" below for the full transcript. |
| The data root is created `0700` on Unix | Unit: `paths::tests::from_base_creates_the_root_and_derives_the_four_folders` reads back `stat`'s mode bits and asserts `0o700` exactly. |
| Choosing an existing database on first run makes the server open *that* database, not the pinned default | Live: two real `tack serve` processes, one with `TACK_DATABASE_URL` pointing at a `tack.db` pre-seeded with one project via `POST /api/projects`, one with the default fresh-root URL; `GET /api/projects` returns 1 item from the override and 0 from the default. See "Override proof" below. |
| Storage, runner state and the log path are never affected by the database override | Unit: `paths::tests::server_folders_applies_a_database_override_and_leaves_the_rest_pinned` asserts all three stay equal to the un-overridden values while only `database_url` changes. |
| Attaching to a server older than this app refuses instead of proceeding, naming both versions | Unit: `supervisor::tests::refuses_to_attach_to_a_server_older_than_the_bundled_version` — fake sidecar reports `0.1.0-beta.1`, bundled `0.1.0-beta.7`, asserts `SupervisorError::OutdatedServer { server_version: "0.1.0-beta.1", bundled_version: "0.1.0-beta.7" }` and that the refused server was never touched (still running, launcher never invoked). |
| Prerelease version ordering is numeric, not lexical | Unit: `check_server_version_orders_prerelease_numbers_numerically` — `beta.9` < `beta.10` both directions. |
| An unparsable version never blocks an attach | Unit: `check_server_version_is_unknown_rather_than_outdated_when_unparseable` — both directions (`server` unparsable, `bundled` unparsable) return `Unknown`, and `attach_or_start` treats `Unknown` as `Compatible`. |
| Settings persist across launches; the dialog does not reappear | Unit: `first_run::tests::settings_round_trip_through_disk` (write then `Settings::load` reads the same struct back) plus `ensure_settings`'s own early-return when `Settings::load` succeeds (not independently live-proved — see Known limitations). |
| `cargo tauri build` still succeeds after this card's changes | Live: ran twice, both producing `.deb`, `.rpm`, `.AppImage` under `/var/tmp/tack-agent-targets/VII-B3/release/bundle/`. |

## Measured numbers

- `cargo test -p tack-desktop`: 23 passed, 0 failed, wall time ~5.3s (dominated by the
  same fake-sidecar spawn/health-poll round trips B1 measured, plus the new outdated-attach
  test's own poll loop).
- `cargo tauri build` output sizes (second run, after the capability fix): `.deb`
  15,398,406 bytes; `.rpm` 15,400,063 bytes; `.AppImage` 90,536,440 bytes — all slightly
  larger than B1's numbers (12.4MB / 12.4MB / 87.9MB), consistent with this card adding
  code, not a regression investigated further.
- `tack --version` of the freshly built sidecar: `tack 0.1.0-beta.7`, matching
  `tack-desktop`'s own `CARGO_PKG_VERSION` exactly (also asserted by
  `dependency_boundary.rs`'s `the_desktop_version_matches_the_server_workspace`).
- Override proof item counts: chosen database (override) = 1 project; default fresh root
  = 0 projects.
- `Cargo.lock` diff: +2 lines (`dirs`, `semver` as direct dependency edges only — both
  already present transitively via `tauri`, so no new crate was fetched).

## What a stranger still cannot do

Nothing in this card lets a stranger click through the first-run dialog and see it
happen — only the settings it would produce for a given answer, and the paths it would
compute, are proven; the dialog's actual appearance on screen was not driven live (see
Known limitations). A stranger also cannot yet see this on macOS or Windows: both are
unit-tested against representative base paths only, `not_measured` for a live run — no
such machine here, same gap B1 recorded. A stranger who swaps the bundled sidecar binary
without rebuilding `tack-desktop` gets a version check that still trusts the old compiled
version, not the swapped file's real one (see Known limitations).

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: `XDG_CURRENT_DESKTOP=ubuntu:GNOME`.
- `$XDG_SESSION_TYPE`: `x11` (not Wayland).
- Appindicator host: not checked — this card adds no tray icon.
- systemd: `255 (255.4-1ubuntu8.17)`.
- `rustc 1.98.1` (pinned by `rust-toolchain.toml`, changed from B1's `1.96.0` by the
  toolchain-pin commit that landed on `develop` since), `tauri-cli 2.11.4`, node
  `v22.17.1`.

## Daemon proof

Not applicable to this card as written — VII-B3 owns data paths, first run and the
version check, none of which touch the close/reopen daemon behavior §VII.1 rule 3
describes (that is B1's spawn/shutdown proof and B2's close-to-hide proof). Recording
this rather than forcing an unrelated proof into the section, per B1's own precedent for
the same non-applicability.

## Process proof

### Fresh-user proof

`XDG_DATA_HOME` pointed at a freshly created temp directory whose `tack` subdirectory did
not exist yet; the real `tack` binary started from a *separate* cwd with the four
`TACK_*` env vars set exactly as `paths::DataPaths::server_folders(None)` would compute
(the root itself pre-created `0700`, matching what `DataPaths::resolve()` does — this
script drives the plain server binary directly rather than the full Tauri app, so that
one step is reproduced by hand):

```
$ mkdir -p $ROOT && chmod 700 $ROOT
root created with mode: 700
$ TACK_DATABASE_URL="sqlite:$ROOT/tack.db?mode=rwc" TACK_STORAGE_DIR="$ROOT/storage" \
  TACK_RUNNER_STATE_DIR="$ROOT/runner" TACK_LOG_FILE="$ROOT/logs/tack.log" \
  tack serve --with-runner &
$ curl -s http://127.0.0.1:38210/api/health
{"migrations_applied":62,"status":"ok","version":"0.1.0-beta.7"}

$ find $ROOT -maxdepth 2
/var/tmp/tack-vii-b3-fresh-OmJe/xdg-data/tack
/var/tmp/tack-vii-b3-fresh-OmJe/xdg-data/tack/tack.db
/var/tmp/tack-vii-b3-fresh-OmJe/xdg-data/tack/tack.db.before-037_orch_runs_rebuild.sqlite
/var/tmp/tack-vii-b3-fresh-OmJe/xdg-data/tack/tack.db-wal
/var/tmp/tack-vii-b3-fresh-OmJe/xdg-data/tack/tack.db-shm

$ find $CWD_TEST -maxdepth 1 -iname "tack.db" -o -iname "storage" -o -iname "logs"
(empty)
```

`storage/` and `logs/tack.log` did not appear within this short-lived run — see Known
limitations; the pinned database path is what the Acceptance line is about and it is
exactly right, isolated from cwd.

### Override proof

Two real `tack serve` processes, ports 38301/38302, from two separate cwds. Server A's
`TACK_DATABASE_URL` pointed at a database pre-seeded with one project (via `POST
/api/projects`, `project_type: "software"`); server B used a fresh, empty pinned-root
database — the "no override" case:

```
$ curl -s http://127.0.0.1:38301/api/projects | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'
1
$ curl -s http://127.0.0.1:38302/api/projects | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'
0
```

`pgrep -af "tack serve"` before either proof: empty. After both proofs' explicit `kill`:
empty again — no orphan left by either script.

## Context spent

- Tokens read before the first edit (cold start): followed the dispatch block's read list
  exactly — `README.md` header + VII-B3 block, the `TODO.md` folder table (§VII.0) and the
  VII-B3 card body, ADR 0062 decision 5, `VII-B1.md` whole, `supervisor.rs` whole, the two
  named server-default ranges (`tack-api/src/config.rs:225-245`,
  `tack-runner/src/config.rs:1-70`), the two named version-source greps, and `cargo doc -p
  dirs --no-deps` for `data_dir`'s per-OS table. Roughly matched the block's ~18k + one
  Tauri page estimate; ended up fetching four `docs.rs` pages for the dialog plugin's
  exact Rust return types (`FileDialogBuilder::blocking_pick_file` returns `Option<FilePath>`,
  not `Option<PathBuf>`; `MessageDialogButtons` variants; `blocking_show`'s `bool` return),
  which the block's single named page (`v2.tauri.app/plugin/dialog/`) did not carry at
  Rust-API-signature detail — all four are under the two allowed domains
  (`https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/`,
  `.../struct.FileDialogBuilder.html`, `.../enum.FilePath.html`,
  `.../struct.MessageDialogBuilder.html`, `.../enum.MessageDialogButtons.html`).
- Context size at handoff: this session's token counter is a whole-run budget (~15M at
  start), not the ~200k single-context-window the card skill's ≤120k/≤150k thresholds
  assume — noted for the record, not a like-for-like comparison, matching B1's own framing
  of the same mismatch.
- Files opened and not used: none beyond the two small extra reads already covered by
  "Read-list lines that were wrong" below.
- Read-list lines that were wrong: none exactly wrong, one gap — the block names
  `v2.tauri.app/plugin/dialog/` for "the Tauri page," but that page documents the
  JS-first API narratively and does not give exact Rust return types (`Option<FilePath>`
  vs `Option<PathBuf>`, `MessageDialogButtons`'s variant list, `blocking_show`'s `bool`).
  Reaching for `docs.rs`'s generated Rust API pages (still under the two allowed domains)
  was necessary to avoid guessing a signature and having the compiler catch it — worth
  noting for the next agent doing Rust-side dialog work from this same block.
- Web pages fetched (all under the two allowed domains):
  - `https://v2.tauri.app/plugin/dialog/` — named by the block.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/` — not named; the
    overview page, checked first before drilling into specific structs.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/struct.FileDialogBuilder.html` — not named; exact `blocking_pick_file`/`add_filter` signatures.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/enum.FilePath.html` — not named; `FilePath::into_path()` to convert the picker's result to a `PathBuf`.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/enum.MessageDialogButtons.html` — not named; the `YesNo` variant used for the first-run question.
  - `https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/struct.MessageDialogBuilder.html` — not named; confirmed `blocking_show`'s `bool` return and that `message()` is the entry point on `DialogExt`, not a separate builder method (matching B1's existing `PortOccupiedByOther` dialog code, which this card's new `OutdatedServer` dialog mirrors).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
