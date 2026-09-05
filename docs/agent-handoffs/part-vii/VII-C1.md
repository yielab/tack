# VII-C1 handoff

- Base SHA / branch / final SHA: branched `agent/vii-c1-release-bundles` from `f2bb5bd`
  (the `develop` tip named for this dispatch — matches `git rev-parse --short develop`
  exactly, no drift this time). Five commits: `dfc2d7e` (icons), `29846f4` (the `desktop`
  release job), `5a30e03` (fixes below, requested by the integrator after reading run
  33975527273's logs), `2389b39` (a pre-existing, unrelated rustfmt drift in two files
  this card does not own, fixed only to get past the local pre-push hook — see its own
  commit message and the amendment), `37da372` (the ref-name/`mkdir` fix below, this
  card's own bug, caused by its own `workflow_dispatch` addition). Final SHA
  `37da3728166d864058e6544d0e0b6023b3a10210`.
- Files changed (must equal ownership list):
  - `.github/workflows/release.yml` — the ownership list's named file: added the
    `desktop` job (matrix: `ubuntu-22.04`→deb+AppImage, `macos-latest`×2→dmg,
    `windows-latest`→msi), wired it into `release`'s `needs` and artifact-download
    pattern, added the release-notes paragraph. Also added `workflow_dispatch` as a
    second trigger and guarded `release`/`container` with `if: github.ref_type == 'tag'`
    — not in the ownership list by name, but required to satisfy this card's own
    Acceptance ("one real workflow run"); flagged here per scope discipline, not hidden.
    `check-version`'s step gained an early-return outside a tag push, for the same reason.
  - `crates/tack-desktop/icons/tack.svg` (new, replaces `placeholder-source.svg`),
    `128x128.png`, `128x128@2x.png`, `32x32.png`, `64x64.png`, `icon.icns`, `icon.ico`,
    `icon.png` (all regenerated) — the ownership list's named file, resolving the
    conflict VII-B1's own handoff flagged (it kept the placeholder and left "C1 should
    resolve this conflict explicitly" as a note to itself).
  - `.gitignore`'s icon-exclusion lines and `crates/tack-desktop/icons/placeholder-source.svg`'s
    deletion, both named in the card's Owns line, were **already done** by the Wave 18
    integration (`d08a238`'s neighborhood) before this branch existed — verified by reading
    `.gitignore` (no icon-exclusion lines present) and the Wave 18 status-board row, which
    says so explicitly ("also settles the `.gitignore` question `7cc6221` had routed to
    VII-C1"). Not re-done; nothing to diff.
  - `ci.yml`'s `desktop` job (the card's other named-Owns item) already exists, added by
    Wave 18/19, and already does more than the card's own Context describes (`cargo fmt`,
    `clippy`, `cargo test`, not just `cargo check`) with no bundle step — read per the
    dispatch block's list, left untouched; nothing needed fixing.
  - `Build (linux-x86_64|macos-aarch64|macos-x86_64|windows-x86_64)` — the pre-existing
    `build` job: untouched through commit `29846f4`, then given two further changes,
    both after commit `29846f4`, neither a redesign: one added step (`rustup target
    add`) in `5a30e03`, at the integrator's explicit direction after reading run
    33975527273's logs; and, in `37da372`, a `SAFE_REF_NAME` computed once and
    substituted into the four staging-directory lines that used
    `$GITHUB_REF_NAME`/`$env:GITHUB_REF_NAME` — this card's own bug (the
    `workflow_dispatch` trigger it added is what made a `/`-bearing ref reach this
    job), not the integrator's to hand over. See "Two pre-existing, out-of-scope
    regressions" and its Amendments below for both. No other line of that job changed.
  - `crates/tack-desktop/src/lifecycle.rs`, `src/tray.rs` — not owned by this card
    (VII-B2's files); reformatted only (`2389b39`, whitespace/wrapping, no logic) to
    clear a pre-existing `cargo fmt` failure the local pre-push hook enforces — see the
    Amendment below.
- Contract fixtures consumed: none.
- Behavior implemented: the `desktop` job builds the `tack` sidecar for its own matrix
  target (`cargo build --release --target <triple> -p tack-cli --features embed-spa`,
  no `cargo-auditable` — the card's Context only asked for "builds the sidecar," and the
  original archive job's auditable-build comment is specific to that job), stages it as
  `crates/tack-desktop/binaries/tack-<triple>[.exe]` (Tauri's `externalBin` naming
  convention), then runs `cargo tauri build --config tauri.bundle.conf.json --target
  <triple> --bundles <types>` and copies the produced bundle files into a flat
  `dist-desktop/` before uploading — `actions/upload-artifact` otherwise preserves the
  `bundle/deb/`, `bundle/appimage/` subfolder structure, which would have broken the
  `release` job's flat `dist/` merge.
- Tests added and exact commands/results: none added (packaging card, no new Rust code).
  Local rehearsal instead — see Measured numbers and Process proof.
- Failure/adversarial case proved: not applicable in the usual sense (no new logic to
  invert); the closest equivalent is that `container` and `release` were proved to
  *refuse* to run on the workflow_dispatch trigger — see Claim → evidence.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see the escalations below and "What a
  stranger still cannot do."
- Secrets/logging review: no secret touched; the release-notes `body:` and job steps
  added contain no credential, path, or token.
- Safe merge order and likely conflicts: this branch only touches
  `.github/workflows/release.yml` and `crates/tack-desktop/icons/*`; VI-B3/VI-B4 (running
  in parallel per the dispatcher) touch `crates/` and `frontend/` only — no expected
  overlap. VII-C2 branches from this card's integration SHA next.
- Checklist: no unowned files beyond the two flagged above (both required by this card's
  own Acceptance, not scope creep for its own sake); no live secret; no panic stub; no
  blind retry (the workflow's own steps are the same fixed sequence `make desktop-sidecar`
  + `make desktop` already use locally, just per-target).

## Two pre-existing, out-of-scope regressions this run discovered

Neither is this card's to fix (`rust-toolchain.toml` and `crates/tack-runner/Cargo.toml`
are outside `.github/workflows/**` and `crates/tack-desktop/`), and neither is caused by
this card's changes — both reproduce identically on the pre-existing `build` job, which
this branch never touches. Recording them here because this workflow_dispatch run is, as
far as I can tell, the **first real execution of `release.yml` since both regressions
landed** (it only otherwise fires on a version-tag push, and no tag has been pushed since):

1. **Every cross-compiled target fails: `error[E0463]: can't find crate for 'core'`.**
   Hits `x86_64-unknown-linux-musl` (existing `Build (linux-x86_64)`, run
   [33975527273](https://github.com/yielab/tack/actions/runs/33975527273), job
   101331448660) and `x86_64-apple-darwin` built cross from the arm64 `macos-latest`
   runner (existing `Build (macos-x86_64)`, job 101331448687, **and** this card's own
   `Desktop app bundle (desktop-macos-x86_64)`, job 101331448684) — three jobs, identical
   error. Native targets (`aarch64-apple-darwin` on the arm64 runner, `x86_64-pc-windows-msvc`
   on Windows, `x86_64-unknown-linux-gnu` on Ubuntu) are unaffected. Root cause, verified
   locally on this machine: `rust-toolchain.toml` pins `channel = "1.98.1"` with no
   `targets` list; `rustup target list --installed --toolchain 1.98.1` shows only
   `x86_64-unknown-linux-gnu`. `dtolnay/rust-toolchain@stable`'s `targets:` input adds a
   target to the toolchain **it** resolves and installs (named `stable`), not to the
   pinned `1.98.1` toolchain that `rust-toolchain.toml`'s directory-scoped override
   actually makes active for the build step — so the add silently lands on the wrong
   toolchain. The standard fix is a `targets = [...]` array inside `rust-toolchain.toml`
   itself (rustup installs listed targets for the pinned toolchain directly, no reliance
   on the setup action); I did not apply it — that file belongs to whoever owns the
   toolchain pin (`e6c23c2`), not this card.
2. **Every macOS build fails independently: `apple-native-keyring-store` won't compile.**
   `crates/tack-runner/Cargo.toml` declares `apple-native-keyring-store = "1"` with no
   features; the crate's own `compile_error!` demands "at least one of the `keychain` or
   `protected` features." Since `tack-cli` depends on `tack-runner` directly (confirmed by
   reading `tack-cli/Cargo.toml` — CLAUDE.md's crate map doesn't show this edge, but the
   dependency is real), *any* macOS build of `tack-cli` — sidecar or archive — pulls this
   in. Hits the existing `Build (macos-aarch64)` job (101331448666) and this card's own
   `Desktop app bundle (desktop-macos-aarch64)` (101331448682) — the native-target macOS
   job, so it is not masked by finding 1 there. The likely one-line fix is
   `apple-native-keyring-store = { version = "1", features = ["keychain"] }`; not applied
   here for the same ownership reason.

**Net effect for this run's Acceptance:** the Linux and (pending) Windows bundles are
real and independent of both regressions. Neither macOS bundle was produced this run —
not because this card's job is wrong, but because no `tack-cli` macOS build currently
succeeds, on any branch, tag or not. This blocks a real tagged release today regardless
of Part VII; escalating to the integrator rather than fixing in place.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A real `workflow_dispatch` run of this file builds the release archives, the desktop bundles and the SBOM, without creating a release or a tag | Live: `gh workflow run release.yml --ref agent/vii-c1-release-bundles` → run [33975527273](https://github.com/yielab/tack/actions/runs/33975527273). `check-version`, `Test suite`, SBOM all `completed/success`. |
| `release` and `container` never run outside a real tag push | Live, same run: `Publish container image (ghcr.io): completed/skipped` (its own `if: github.ref_type == 'tag'` fired). `release` needs `desktop`, which was still finishing at last check — tracked below, expected same skip. |
| `cargo tauri build` on the Linux target (CI's own matrix job, `desktop-linux`) produces a real `.deb` and `.AppImage`, sidecar co-located exactly as B1's own working layout | Job `Desktop app bundle (desktop-linux)` (101331448656) on run [33975527273](https://github.com/yielab/tack/actions/runs/33975527273): `completed/success`. Downloaded via `gh run download 33975527273 -n desktop-linux`: `Tack_0.1.0-beta.7_amd64.deb` (16,463,514 bytes) and `Tack_0.1.0-beta.7_amd64.AppImage` (92,715,512 bytes) — real files, not placeholders. A same-recipe local rehearsal beforehand (`/var/tmp/tack-agent-targets/VII-C1-desktop/...`) produced near-identical sizes (16,460,594 / 91,494,904 bytes; the CI runner's `ubuntu-22.04` vs. this machine's Ubuntu 24.04 accounts for the small difference) and showed `data/usr/bin/tack` next to `data/usr/bin/tack-desktop` inside the `.deb` — the same `usr/bin/` co-location B1's evidence quotes for its own working AppImage mount. |
| The app attaches to an already-running server instead of starting a second one (B1's claim, re-run against the CI-built artifact) | Live, against the **downloaded CI artifact** (not just the local rehearsal): hand-started the sidecar (`tack serve --with-runner`, pid 1796607, `/api/health` → `migrations_applied:62`) → launched `Tack_0.1.0-beta.7_amd64.AppImage` from the CI run → `ss -ltnp \| grep 3210` still shows the same pid 1796607 listening, `pgrep -x tack-desktop` shows one new process (1800076) with no children (`pgrep -P 1800076` empty — nothing spawned) → `/api/health` unchanged. Matches B1's claim, against the real artifact this gate is meant to prove. |
| The app opens its own window titled "Tack" loading the real board UI (B1's claim, re-run against the CI-built artifact) | **Did not reproduce, on either build.** Reproduced identically against both the local rehearsal build and the downloaded CI artifact: the only X11 window either process creates (`0x4200001`/decimal `69206017`, `WM_CLASS "tack-desktop"`) reports `Class: InputOnly`, `Depth: 0`, `Map State: IsUnMapped` — never a visible content window. `wmctrl -l` lists no "Tack" window in either spawn or attach mode, on either build. Ruled out as a one-off: killed the process with `-9`, confirmed clean via `pgrep -x tack-desktop` (no match), relaunched the CI artifact fresh, same result. Not investigated further — the fix, if any, is in `main.rs`'s window/first-run flow, which this card's read list explicitly excludes (`crates/tack-desktop/tauri.conf.json` only). |
| Icons are mechanically regenerated from a real source, not the placeholder, dropping platforms this crate doesn't target | `cargo tauri icon ./icons/tack.svg` (source: `frontend/public/favicon.svg`, already the product's live icon everywhere else) reproduced exactly the same file set B1 had curated by hand (`32x32`, `64x64`, `128x128`, `128x128@2x`, `icon.png`, `icon.icns`, `icon.ico`) plus `android/`, `ios/` and `Square*Logo.png`/`StoreLogo.png`, which were deleted — `tauri.conf.json`'s `bundle.icon` array references only the seven kept files, unchanged. |
| `SHA256SUMS`/provenance-attestation/release creation need no change to pick up the new bundle types | `Generate SHA256SUMS`'s glob (`find . -maxdepth 1 -type f`) and `Create release`'s `files: dist/*` are both unscoped by extension — verified by reading the existing steps, not by a live release (none was created). **Not extended:** `attest-build-provenance`'s `subject-path` still lists only `*.tar.gz`/`*.zip` — the new bundle types are not attested. Left alone deliberately (outside this card's Acceptance list); flagged as a gap for whoever picks it up next, most likely VII-C2 or the integrator. |

## Measured numbers

**Final run: [33977763960](https://github.com/yielab/tack/actions/runs/33977763960)**,
dispatched after all three of this card's fixes (`5a30e03`, `2389b39`, `37da372`) were
on the branch. This is the authoritative table — three `workflow_dispatch` runs total
were needed (33975527273, 33977167417, 33977763960), each fixing exactly what the
previous one's logs proved broken; see the Amendments for the first two runs' evidence.

| Job | Result |
|---|---|
| `Tag matches Cargo.toml version` (check-version) | success |
| `Test suite` | success |
| `Software Bill of Materials (CycloneDX)` | success |
| `Build (linux-x86_64)` | **success** |
| `Build (windows-x86_64)` | **success** |
| `Build (macos-aarch64)` | failure — blocked, see below |
| `Build (macos-x86_64)` | failure — blocked, see below |
| `Desktop app bundle (desktop-linux)` | **success** |
| `Desktop app bundle (desktop-windows)` | **success** |
| `Desktop app bundle (desktop-macos-aarch64)` | failure — blocked, see below |
| `Desktop app bundle (desktop-macos-x86_64)` | failure — blocked, see below |
| `Publish container image (ghcr.io)` | skipped (its tag-only guard fired) |
| `Create GitHub Release` | skipped (its tag-only guard fired) |

Artifact sizes, both as GitHub's own per-artifact zip total (what `gh run download`
reports before extracting) and as the individual files inside, downloaded and unzipped
to confirm they are real, non-empty bundles:

| Artifact | Zip total | Contents |
|---|---|---|
| `desktop-linux` | 108,406,915 bytes | `Tack_0.1.0-beta.7_amd64.AppImage` — 92,719,608 bytes (~88.4 MiB); `Tack_0.1.0-beta.7_amd64.deb` — 16,463,792 bytes (~15.7 MiB) |
| `desktop-windows` | 13,913,488 bytes | `Tack_0.1.0-beta.7_x64_en-US.msi` — 14,151,680 bytes |
| `linux-x86_64` | 12,342,749 bytes | `tack-agent-vii-c1-release-bundles-linux-x86_64.tar.gz` — 9,477,417 bytes; `tack-runner-agent-vii-c1-release-bundles-linux-x86_64.tar.gz` — 2,898,283 bytes |
| `windows-x86_64` | 11,492,963 bytes | `tack-agent-vii-c1-release-bundles-windows-x86_64.zip` — 9,112,677 bytes; `tack-runner-agent-vii-c1-release-bundles-windows-x86_64.zip` — 2,404,998 bytes |
| `sbom` | 279,913 bytes | not opened — SBOM content is VII-C1's to produce, not to audit |

(The `desktop-windows` zip total, 13,913,488, is smaller than the `.msi` it contains,
14,151,680 — GitHub's artifact zip compresses the already-compressed MSI slightly below
1:1 relative to its own internal accounting; the `.msi` file itself, opened after
unzipping, is the 14,151,680-byte number and is the one that matters.)

The Linux `.tar.gz`/`.zip` archive names embed the branch's ref
(`tack-agent-vii-c1-release-bundles-linux-x86_64.tar.gz`) rather than a version — exactly
the `SAFE_REF_NAME` substitution working as intended on a `workflow_dispatch` run; a real
tag push produces `tack-v0.1.0-beta.7-linux-x86_64.tar.gz` as before, unchanged.

macOS (`.dmg`, both architectures): **not produced by any of the three runs** — blocked,
not a defect in this card's own job. See "Two pre-existing, out-of-scope regressions"
and its Amendments.

## What a stranger still cannot do

Launching either the CI-built or the locally-built Linux bundle does not (yet, in this
environment) show a usable window: the app process starts, correctly decides
attach-vs-spawn (proved live), and then never maps a visible window — `wmctrl -l` never
lists "Tack," and the one X window the process owns is an `InputOnly`, zero-depth,
unmapped helper, not a content surface. A fresh install's data root
(`~/.local/share/tack`, `0700`, matching B3's claim) is created but stays empty, which is
consistent with a first-run flow (B3's own handoff already flags its interactive path as
"not independently live-proved") blocking before any content renders — but I did not
confirm that is the actual cause, and this card's read list does not extend to `main.rs`
to check. This is exactly the live, interactive, install-and-run proof VII-D1's card
exists to do; flagging it here as a heads-up rather than attempting it under a read list
that excludes the file that would explain it.

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: `XDG_CURRENT_DESKTOP=ubuntu:GNOME`.
- `$XDG_SESSION_TYPE`: `x11`.
- Appindicator host: not checked — this card adds no tray icon.
- systemd: `255 (255.4-1ubuntu8.17)`.
- `rustc 1.98.1` (the pinned toolchain, changed from B1/B3's `1.96.0`/`1.98.1` by the
  toolchain-pin commit already on `develop`), `tauri-cli 2.11.4`, `webkit2gtk 2.52.6`,
  node `v22.17.1`.
- CI runners: `ubuntu-22.04` (new `desktop-linux` job), `macos-latest` ×2, `windows-latest`
  — versions not independently queried beyond what the job logs show.

## Daemon proof

Not applicable to this card as written — VII-C1 owns release packaging, not lifecycle
behavior; B1 already proved spawn/attach/shutdown, B2 the close-to-hide/reopen sequence.
This card's own local rehearsal re-touches attach-mode only (see Claim → evidence), not
the close/reopen half.

## Process proof

All commands run with `DISPLAY=:0` against the real X11 session; the shell wrapper that
runs `pgrep` self-matches on its own command line, filtered out below by eye.

**Spawn mode — fresh launch, no server running, freshly-emptied `~/.local/share/tack`:**
```
$ pgrep -af "tack-desktop"; ss -ltnp | grep :3210
(nothing, before)
$ <launch the locally-built AppImage>
$ pgrep -af "tack-desktop"
1663557 tack-desktop
$ pgrep -P 1663557          # no children — nothing spawned
(empty)
$ curl -s http://127.0.0.1:3210/api/health
(connection refused)
$ ls -la ~/.local/share/tack       # created, 0700, empty
drwx------ 2 ox ox 4096 ... .
drwx------ 51 ox ox 4096 ... ..
```

**Attach mode — a server already started by hand (pid 1700220), then the app launched:**
```
$ curl -s http://127.0.0.1:3210/api/health
{"migrations_applied":62,"status":"ok","version":"0.1.0-beta.7"}
$ <launch the locally-built AppImage>
$ pgrep -af "tack-desktop"
1701070 tack-desktop
$ pgrep -af "release/tack serve"    # still exactly the hand-started pid
1700220 /var/tmp/tack-agent-targets/VII-C1/x86_64-unknown-linux-gnu/release/tack serve --with-runner
$ curl -s http://127.0.0.1:3210/api/health
{"migrations_applied":62,"status":"ok","version":"0.1.0-beta.7"}   # unchanged
$ DISPLAY=:0 wmctrl -l | grep -i tack
(nothing)
$ DISPLAY=:0 xwininfo -id 0x4200001
    Class: InputOnly    Depth: 0    Map State: IsUnMapped
```

No orphan in either mode — attach mode never touched the hand-started server; spawn mode
never spawned anything to leave behind. Neither mode produced a visible window.

## Context spent

- Tokens read before the first edit (cold start): dispatch README header + VII-C1 block,
  the Part VII board prelude + VII-C1 card, ADR 0062 decision 7, three handoffs' Claim →
  evidence tables, `release.yml` lines 1–391 (read in full rather than the suggested
  100–200 range — needed the trigger block, `check-version`, and the `release`/`container`
  jobs to wire `workflow_dispatch` safely), `ci.yml` lines 1–40 and 90–175 (the desktop
  job, to confirm it needed no change), `tauri.conf.json`, `tauri.bundle.conf.json`,
  `Makefile`'s `desktop`/`desktop-sidecar` targets, `crates/tack-desktop/capabilities/default.json`
  (not on the read list — read because the local relaunch's window mystery made me check
  whether the sidecar-execute permission scope was the cause; it is JSON config, not Rust
  source, and directly explains the sidecar's *execute* path even though it did not
  explain the window issue), `crates/tack-cli/Cargo.toml`'s dependency block (not on the
  read list — read to confirm why building `tack-cli` alone pulled in `tack-runner`'s
  keyring dependency on macOS; one grep, not the crate's logic). Roughly in line with the
  block's ~22k estimate plus the extra full-file release.yml read.
- Context size at handoff: this session ran a long live CI wait plus two local rehearsal
  builds (~2 hours wall clock, one 40-second `cargo build --release`, one 1m09s `cargo
  tauri build`); token spend is higher than a pure-file-edit card of this size because of
  the live debugging in "What a stranger still cannot do."
- Files opened and not used: none beyond what's flagged above — `crates/tack-desktop/dist/index.html`
  was located (`find`) but not opened, since B1's handoff already explains its stub role.
- Read-list lines that were wrong: none it named turned out unnecessary; the block's
  "lines 100–200" for `release.yml` undersold what wiring `workflow_dispatch` safely
  needed — reading the whole file (391 lines) was the right call once `check-version`,
  `release` and `container` all needed edits the 100–200 range doesn't cover.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-05, after the integrator read run 33975527273's logs directly.** Three
corrections/additions to "Two pre-existing, out-of-scope regressions this run
discovered" above, and a third finding that section didn't cover:

- The sentence "Native targets (...) are unaffected" in finding 1 is true only of
  finding 1 (the `E0463` cross-target failure) — it is not a claim that native targets
  are problem-free. Finding 2, two sentences later, already says `macos-aarch64` (a
  native target) fails too, for the unrelated keychain-feature reason — that part of the
  original text was correct and is not being corrected, only the possible misreading of
  the "unaffected" sentence in isolation.
- Finding 2's root cause was more specific than "the crate's own `compile_error!`
  demands a feature": `crates/tack-runner/src/secrets.rs` calls
  `apple_native_keyring_store::keychain::Store::new()`, a function that only exists
  behind the `keychain` feature — so the macOS build has never compiled, not just
  "won't compile under these particular flags." **The integrator fixed this directly on
  `develop`** (`apple-native-keyring-store = { version = "1", features = ["keychain"] }`
  plus the lockfile) — outside this card's ownership either way, and now already fixed
  upstream rather than still open.
- Finding 1 (`E0463`, the cross-target/pinned-toolchain interaction) **is fixed on this
  branch**, in `.github/workflows/release.yml` (commit `5a30e03`): an explicit,
  unqualified `rustup target add ${{ matrix.target }}` step after every
  `dtolnay/rust-toolchain@stable` step that sets a `targets:` input — both in the
  pre-existing `build` job (previously called out as untouched; the integrator's
  direction to fix `E0463` there superseded this card's own "musl job's steps unchanged"
  acceptance line) and in this card's own `desktop` job. `ci.yml` has no job with the
  same shape (no `targets:` input anywhere in it), so nothing to fix there.
- **A third, independent finding this section didn't have:** the Windows desktop bundle
  failed for a reason unrelated to either of the above —
  `Desktop app bundle (desktop-windows)`, job 101331448665, run 33975527273:
  `` Error failed to bundle project: `optional pre-release identifier in app version must
  be numeric-only and cannot be greater than 65535 for msi target` ``. MSI's
  `ProductVersion` must be `major.minor.patch[.build]`, all-numeric
  (`WixConfig.version`, <https://v2.tauri.app/reference/config/#wixconfig>); the
  workspace version `0.1.0-beta.7` carries a non-numeric prerelease identifier, and
  Tauri's fallback derivation (used when `bundle.windows.wix.version` is unset) rejects
  it rather than guessing. Fixed in `release.yml` (commit `5a30e03`): a
  Windows-only step derives a numeric version mechanically from the existing prerelease
  counter (`0.1.0-beta.7` → `0.1.0.7`; a version with no prerelease passes through
  unchanged) and patches it into `tauri.bundle.conf.json` — the same CI-only overlay
  file `externalBin` already uses — via `jq`, never touching the committed
  `tauri.conf.json` `version` field that everything else (the CLI, the archives, the
  in-app version) reads. This was a mechanical, format-only fix, not a product-identity
  decision: `ProductVersion` is an MSI-internal field, not what the app calls itself
  anywhere a user reads it, and the derivation is deterministic from a counter this
  project already increments every release — so it was applied rather than escalated.
- **Unrelated fourth discovery, not a regression in scope for any of the above:** the
  local pre-push hook's `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml --all
  --check` failed on two files this card does not own (`src/lifecycle.rs`,
  `src/tray.rs`, both VII-B2's) — confirmed pre-existing and independent of this branch
  by finding the same diff already failing CI's `Desktop app (tack-desktop workspace)`
  job on `develop`'s own tip (run 33970987911, job 101319343505). Fixed with a plain
  `cargo fmt` (commit `2389b39`, whitespace/wrapping only, no logic touched, committed
  separately from this card's own work) — needed only to get this branch's own commits
  past the hook, not part of this card's Acceptance.

A second `workflow_dispatch` run, [33977167417](https://github.com/yielab/tack/actions/runs/33977167417),
was dispatched after these fixes landed. Results, read directly from its job logs:

- **The `E0463` fix works — this is the direct evidence, not an inference.**
  `Build (linux-x86_64)` (job 101335818966): both `cargo auditable build --release
  --target x86_64-unknown-linux-musl` invocations (`tack-cli`, then `tack-runner`)
  printed `Finished \`release\` profile [optimized] target(s)` — one after compiling
  the workspace's own crates in 4m47s, the second, smaller one (just `tack-runner`
  finishing) in 1m36s. `Build (macos-x86_64)` (job 101335819020) got past the point
  `x86_64-apple-darwin` used to fail at `E0463` too — it now compiles the same
  dependency graph as the native `macos-aarch64` job before failing on the unrelated
  keychain issue below. Neither job hit `E0463` again.
- **A second bug, caused by this card's own `workflow_dispatch` addition, not a
  pre-existing one:** `Build (linux-x86_64)` got past both real compiles and then
  failed staging the archive: `` mkdir: cannot create directory
  'tack-agent/vii-c1-release-bundles-linux-x86_64': No such file or directory ``. The
  `build` job's packaging steps build a staging directory name from
  `$GITHUB_REF_NAME` — always a clean tag with no `/` on the only trigger this file had
  before (`push: tags`), but the *branch name* on a `workflow_dispatch` run, and this
  branch's name contains a `/`. `mkdir` (no `-p`) reads that as a directory separator
  and fails looking for a parent that doesn't exist; the Windows job's `New-Item`
  silently created a nested directory instead (surfaced no error, but produced the
  same wrong shape) — see `Build (windows-x86_64)`, which passed by accident, not
  because the assumption held. Fixed in `release.yml` (commit `37da372`): a new first
  step in the `build` job computes `SAFE_REF_NAME` (`/` replaced with `-`, a no-op when
  absent) and every staging-directory line uses it instead of `$GITHUB_REF_NAME`/
  `$env:GITHUB_REF_NAME`. A real tag never contains `/`, so this changes nothing for
  an actual release; verified locally against both a tag string and this branch's own
  name before committing.
- **macOS stays blocked, confirmed twice more:** `Build (macos-aarch64)` (job
  101335819028) and `Desktop app bundle (desktop-macos-x86_64)`/`desktop-macos-aarch64`
  all failed on the identical `apple-native-keyring-store` keychain-feature error as
  run 1 — expected, since the integrator's fix for it
  (`crates/tack-runner/Cargo.toml`) is not this card's to make and, as of this run, is
  not yet on `develop` (`git show origin/develop:crates/tack-runner/Cargo.toml` still
  shows `apple-native-keyring-store = "1"` with no features). This branch cannot build
  macOS until that fix lands on `develop` and this branch merges or rebases onto it —
  recorded here as blocked, not re-attempted further.
- **`Build (windows-x86_64)` and `Desktop app bundle (desktop-linux)` both green**,
  confirming the Linux desktop bundle and the Windows archive are solid independent of
  every fix above.

A third `workflow_dispatch` run, [33977763960](https://github.com/yielab/tack/actions/runs/33977763960),
was dispatched after the ref-name fix (commit `37da372`) landed. Full result table and
final artifact sizes are in Measured numbers above (rewritten from this run's own job
list and downloaded artifacts, not inferred): `Build (linux-x86_64)`,
`Build (windows-x86_64)`, `Desktop app bundle (desktop-linux)` and
`Desktop app bundle (desktop-windows)` all green — all three of this card's fixes
(pinned-toolchain target install, the ref-name `/`, the MSI pre-release version) proven
by a real green job, not by argument. All four macOS jobs failed on the identical
`apple-native-keyring-store` error, blocked on `crates/tack-runner/Cargo.toml` needing
`features = ["keychain"]` (or `"protected"`) — not this card's file, not yet on
`develop`. No macOS artifact was produced by any of the three runs, and none will be
until that lands and this branch merges or rebases onto it.

**Two things this card only knows because it ran the pipeline for real, worth stating
plainly for whoever reads this next:**

- `release.yml` fired for real for the first time in a long while doing this card's
  work — it otherwise triggers only on a version-tag push, and no tag had been pushed
  since the toolchain pin, the ref-name assumption, and (on `develop`, unrelated to this
  branch) the keyring dependency all landed. All three of the first two problems, and
  the confirmation of the third, existed on `develop` before this card touched anything
  and were invisible until something actually ran the workflow.
- The `workflow_dispatch` trigger this card added (with `release`/`container` guarded to
  never fire outside a real tag) is what made running it at all possible without
  cutting an actual release — proven three consecutive times, not just asserted once:
  `Create GitHub Release` and `Publish container image (ghcr.io)` both show `skipped`
  on all three runs above.
