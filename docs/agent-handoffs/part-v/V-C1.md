# V-C1 handoff

- Base SHA / branch / final SHA: base `develop` tip at branch creation (`81e66e5`), branch
  `agent/v-c1-distribution`, final SHA `fc76ca81073b7b76329bd4d5c386b241b506d4b6`.
- Files changed (must equal ownership list):
  - `packaging/homebrew/tack.rb` (new)
  - `packaging/aur/PKGBUILD` (new)
  - `packaging/nix/default.nix` (new)
  - `.github/workflows/release.yml` — one new job (`container`) appended at the end of the
    file; nothing V-A3 wrote was touched (diff is purely additive, verified below)
  - `Cargo.toml` — added `repository = "https://github.com/yielab/tack"` to
    `[workspace.package]`; needed so cargo-binstall's `{ repo }` template variable
    resolves. Not in the explicit ownership list but not owned by any other card either
    (checked `TODO.md` §V.2); no line any other card wrote was touched.
  - `crates/tack-cli/Cargo.toml` — added `repository.workspace = true` and
    `[package.metadata.binstall]` (see "Where the binstall metadata actually lives" below
    for why it's here and not in the root `Cargo.toml` as the card text says)
  - **`Dockerfile` — one unowned file, touched to fix a real, reproducible build bug found
    while testing. Flagged explicitly below and in the checklist.**
- Contract fixtures consumed: none (`docs/contracts/runner-v1/**` untouched).
- Behavior implemented: three package-manager recipes (Homebrew formula, AUR `PKGBUILD`,
  Nix derivation) that install `tack` from the GitHub Release archive `release.yml`
  produces; a new additive `container` job in `release.yml` that builds the existing
  `Dockerfile` and pushes it to `ghcr.io/yielab/tack` tagged with the release tag (plus
  `:latest` for non-prerelease tags); `cargo-binstall` metadata so `cargo binstall
  tack-cli` resolves the correct release asset per target instead of compiling from
  source.

## Where the binstall metadata actually lives

The card text says "root `Cargo.toml`". That's wrong for this repo, and following it
literally breaks the build: the root `Cargo.toml` is a **virtual manifest** (`[workspace]`
only, no `[package]`). Cargo does not allow a `[package.metadata.*]` table there — I
proved this empirically before writing anything:

```
$ cargo metadata --no-deps   # with [package.metadata.binstall] added to a virtual-manifest root
error: failed to parse manifest at `.../Cargo.toml`
Caused by:
  missing field `package.name`
```

That's not a corner case — it breaks `cargo build`/`cargo test --workspace` for the whole
repo. `cargo-binstall` itself only ever reads this table from the manifest of the crate
actually being installed (traced through its source at
`crates/binstalk-fetchers/src/gh_crate_meta.rs` in the `cargo-bins/cargo-binstall` repo),
so the metadata belongs in `crates/tack-cli/Cargo.toml` regardless. Root `Cargo.toml` only
gained one line (`repository = "..."` under `[workspace.package]`, a standard, always-valid
field), which `tack-cli` inherits via `repository.workspace = true` — that's what feeds
`cargo-binstall`'s `{ repo }` template variable.

## The Dockerfile bug (unowned file, fixed anyway)

Building the existing `Dockerfile` from a clean `rust:1-bookworm` container — required to
verify the container-publish job would actually work — failed on a stock image, with no
sandbox-specific cause:

```
error[E0463]: can't find crate for `core`
  = note: the `x86_64-unknown-linux-musl` target may not be installed
```

Root cause: the Dockerfile ran `rustup target add "$TARGET"` **before** `COPY . .`. This
repo pins its toolchain via `rust-toolchain.toml`. Before that file is copied into the
build context, `rustup target add` operates against whatever rustup treats as the image's
bare default toolchain; once `rust-toolchain.toml` is copied in, every subsequent
`cargo`/`rustc` invocation in that directory resolves a differently-identified "stable"
toolchain (confirmed with `rustup show`: `active because: overridden by
'/work/rust-toolchain.toml'`) that never received the target. This is not
sandbox-specific — it reproduces identically on an unmodified `rust:1-bookworm` container
with real `sudo`/`apt` available, so it would have failed identically on a real GitHub
Actions runner the first time `v0.1.0-beta.7`'s tag is pushed.

Fix: move `RUN rustup target add "$TARGET"` to after `COPY . .` (apt/musl-tools install
stays before `COPY`, since it doesn't depend on repo content and benefits from layer
caching). Verified by building the image twice: unpatched Dockerfile fails with the error
above; patched Dockerfile builds clean end to end (transcript below). This file isn't on
this card's ownership list, and no other Part V/IV card claims it either (checked
`TODO.md` §V.2) — flagging prominently per the checklist rather than silently expanding
scope.

## Tests added and exact commands/results

No Rust test code was added (this card is packaging, not application logic). Verification
was: (1) a compile-sanity check that the `Cargo.toml` edits don't break the workspace, and
(2) an actual install + actual execution of `tack` from each recipe, in a disposable Docker
container, using a real build of the binary as the checksum/archive stand-in (the
`v0.1.0-beta.7` tag is cut locally per V-A3's handoff but not pushed, so
`github.com/yielab/tack/releases/...` doesn't resolve yet — anticipated by this card's own
acceptance text).

**Compile sanity**: `cargo check --workspace --all-targets` (with
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C1`) — finished clean, `dev` profile, no
errors.

**The stand-in binary is the real thing, not an approximation.** V-A3's handoff could only
produce a dynamically-linked (glibc) stand-in, because building the real
`x86_64-unknown-linux-musl` target needs `musl-tools`/`apt` (root), unavailable on this
sandbox (no `sudo`). Rather than repeat that gap, I built the *actual* Dockerfile — its
backend stage installs `musl-tools` as **root inside its own build container**, sidestepping
the host's lack of sudo entirely — which produced a genuine static-musl binary:

```
$ file tack
tack: ELF 64-bit LSB pie executable, x86-64, ... static-pie linked, ... stripped
```

Every recipe below was tested against **this** binary (repackaged into a `tar.gz` matching
`release.yml`'s exact `STAGE` naming/contents), not the glibc stand-in. This closes a real
gap: a static binary needs no dynamic linker at all, so it runs even inside environments
that have none (see the Nix result below, which would not have worked with a glibc
binary).

Archive used for all three recipes' checksums:
`tack-v0.1.0-beta.7-linux-x86_64.tar.gz`, 8,399,290 bytes,
sha256 `1f15ac5b69fca569e268a2a9bf334a4ae0630e32853d0716a5d6910a2808df6c`
(binary alone: 19,323,320 bytes).

### Channel 1 — Docker / ghcr.io (fully verified, real production artifact)

```
$ docker build -t tack:v0.1.0-beta.7-test .        # patched Dockerfile, cold, no cache
...
real    3m5.873s
$ docker images tack:v0.1.0-beta.7-test --format "{{.Size}}"
21.4MB
$ docker run -d -p 13210:3210 -e TACK_API_TOKEN=<test-token> tack:v0.1.0-beta.7-test
$ curl -s -o /dev/null -w "HTTP %{http_code}\n" http://127.0.0.1:13210/ -H "Authorization: Bearer <test-token>"
HTTP 200
$ docker exec <container> /usr/local/bin/tack --version
tack 0.1.0-beta.7
```

(Without `TACK_API_TOKEN` the container correctly refused to bind `0.0.0.0` — the app's
existing network-exposure guard, not something this card added, still fires correctly
inside the container.)

### Channel 2 — Homebrew (custom tap, real binary, `brew test` passing)

`packaging/homebrew/tack.rb`, tested via `homebrew/brew:latest` (Ubuntu 22.04, glibc
2.35 — this is why the earlier glibc stand-in would *not* have run here; the static one
does), formula's `url` swapped to a `file://` mount of the real archive for the test run
only (committed formula points at the real `github.com/.../releases/download/...` URL):

```
$ brew tap-new yielab/tack && cp tack.rb .../Formula/tack.rb
$ time brew install --verbose yielab/tack/tack
==> Verifying checksum for '...tack-v0.1.0-beta.7-linux-x86_64.tar.gz'
✔︎ Formula tack (0.1.0-beta.7)
🍺  .../Cellar/tack/0.1.0-beta.7: 7 files, 18.5MB, built in 1 second
real    0m2.449s
$ tack --version
tack 0.1.0-beta.7
$ brew test yielab/tack/tack
==> /home/linuxbrew/.linuxbrew/Cellar/tack/0.1.0-beta.7/bin/tack --version    # passed
```

### Channel 3 — AUR `PKGBUILD` (Arch container, real binary, full build+install+serve)

`packaging/aur/PKGBUILD` (`tack-bin`), tested via `archlinux:latest`, same `file://`
source swap for the test only:

```
$ sudo -u builder makepkg -s --noconfirm
==> Validating source files with sha256sums... Passed
==> Finished making: tack-bin 0.1.0.beta.7-1
real    0m2.915s
$ pacman -U --noconfirm tack-bin-*.pkg.tar.zst
real    0m0.064s
$ tack --version
tack 0.1.0-beta.7
$ tack serve &   # then a raw GET / over /dev/tcp
HTTP/1.0 200 OK
```

### Channel 4 — Nix derivation (pure-Nix container, no FHS — the strongest proof)

`packaging/nix/default.nix`, tested via `nixos/nix:latest`, same `file://` source swap:

```
$ nix-build --no-out-link -E 'with import <nixpkgs> {}; callPackage /nix-src/default.nix {}'
unpacking source archive tack-v0.1.0-beta.7-linux-x86_64.tar.gz
...
real    0m2.120s
$ /nix/store/.../bin/tack --version
tack 0.1.0-beta.7
$ tack serve &   # then a raw GET / over /dev/tcp
HTTP/1.0 200 OK
```

This container has **no `/lib`, `/lib64`, or `/etc/os-release`** — a normal
dynamically-linked prebuilt Linux binary cannot execute here at all (confirmed earlier in
this session: the glibc stand-in failed with `cannot execute: required file not found`,
i.e. no ELF interpreter). The derivation works specifically *because* the binary is a
static musl build needing no interpreter — which is also why the derivation has no
`autoPatchelfHook`, the usual crutch prebuilt-Linux-binary Nix packages need. This is the
one channel where using the real artifact instead of a stand-in wasn't just nicer, it was
the difference between the test being meaningful and not.

### Channel 5 — `cargo-binstall` metadata (dry-run against the live GitHub API)

Not an "install" (nothing to download yet — the release doesn't exist), but the URL
resolution logic was proven against the *real* `yielab/tack` repository on GitHub, not a
mock:

```
$ cargo-binstall --manifest-path crates/tack-cli/Cargo.toml --targets x86_64-unknown-linux-musl \
    tack-cli --dry-run --no-confirm -v
DEBUG Resolved repo_info = ... host: "github.com", path: "/yielab/tack" ...
DEBUG Checking for package at: 'https://github.com/yielab/tack/releases/download/v0.1.0-beta.7/tack-v0.1.0-beta.7-linux-x86_64.tar.gz'
DEBUG has_release_artifact{...}: return=Ok(None)    # correct: tag not pushed yet
 WARN The package tack-cli v0.1.0-beta.7 will be installed from source (with cargo)
```

The rendered URL is byte-for-byte the asset `release.yml` will actually produce. Repeated
for the other three targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-pc-windows-msvc`) — all four render exactly the corresponding
`tack-v0.1.0-beta.7-{macos-aarch64,macos-x86_64}.tar.gz` / `...-windows-x86_64.zip` names.
Correctly reports "not found" and falls back to `cargo install` rather than fetching a
wrong/stale asset — proves the metadata doesn't lie about availability while the release is
unpublished.

## Real measurements (per channel)

| Channel | Install/build time | Installed size |
|---|---|---|
| Docker (`docker build`, cold, no cache) | 3m 5.9s | image 21.4 MB |
| Homebrew (`brew install`, cached download) | 2.45s | 18.5 MB (Cellar) |
| AUR (`makepkg -s` + `pacman -U`) | 2.92s + 0.06s | 18.4 MiB installed |
| Nix (`nix-build`) | 2.12s | — (store path, not measured separately) |
| Binary itself (static musl, release profile) | — | 19,323,320 bytes |
| Release archive (`tar.gz`, binary+LICENSE+README+QUICKSTART) | — | 8,399,290 bytes |

All install-time numbers exclude Docker's own image-pull/container-startup overhead —
they're the `real` time of the actual install command (`brew install`, `makepkg` +
`pacman -U`, `nix-build`) measured inside the running container.

## Failure/adversarial case proved

- `cargo-binstall` correctly resolves to **no asset found** and falls back to
  `cargo install` rather than serving a wrong/mismatched binary, verified against the real,
  live GitHub repository while the release genuinely doesn't exist yet (see Channel 5).
- The Docker image's existing `TACK_API_TOKEN` exposure guard still fires correctly when
  the container is run without it (`refusing to bind 0.0.0.0 without TACK_API_TOKEN`) —
  confirms this card didn't accidentally weaken that behavior by containerizing it.
- The Dockerfile bug: proved broken (exact error, reproduced on a stock, unmodified
  `rust:1-bookworm` container — not sandbox-specific) before the fix, proved fixed (full
  build + run + HTTP 200) after.

## Not verified / known limitations (`not_measured`)

- **macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`) and Windows
  (`x86_64-pc-windows-msvc`) legs**: no macOS/Windows host or cross-toolchain available
  here (same constraint V-A3 hit). The Homebrew formula's macOS `sha256` fields are
  explicit 64-zero placeholders with a comment; they are **not real digests** and must be
  replaced from the published release's `SHA256SUMS` before the formula is usable on
  macOS. The `cargo-binstall` overrides for those three targets are URL-template-verified
  (Channel 5) but never downloaded/executed.
- **Real GitHub Release URLs 404 until the tag is pushed** (`v0.1.0-beta.7` is cut locally
  per V-A3's handoff, not pushed) — every recipe's committed `url`/`source`/`pkg-url`
  points at the real `github.com/yielab/tack/releases/download/v0.1.0-beta.7/...` path;
  only the *test* copies were pointed at local `file://` archives, and the real path was
  independently confirmed correct via Channel 5's live-API dry run.
- **Homebrew's Linux-ARM64 `odie` guard** (`packaging/homebrew/tack.rb`) is written but not
  exercised — no ARM64 host available to trigger `Hardware::CPU.arm?` on Linux.
- **Multi-arch container image**: not attempted. The `Dockerfile`'s backend stage hardcodes
  `TARGET=x86_64-unknown-linux-musl` rather than deriving it from buildx's target platform,
  so the new `container` job in `release.yml` explicitly publishes `linux/amd64` only
  (commented in the workflow). Making it multi-arch is a `Dockerfile` change, out of this
  card's scope.
- **No build-provenance attestation for the container image** — V-A3 added
  `attest-build-provenance` for the `.tar.gz`/`.zip` archives; the new `container` job does
  not attest the image. Deliberately left out to keep the workflow diff minimal and
  strictly additive; a natural follow-up for whoever owns `release.yml` next.
- **AUR `.SRCINFO`** is not committed — it's a generated artifact tied to the exact
  `PKGBUILD` content (`makepkg --printsrcinfo > .SRCINFO`) and would go stale on the next
  edit; generate it fresh at actual AUR-submission time.
- **Homebrew tap / AUR submission / `ghcr.io` push**: none of these were done, per this
  card's explicit "preparation only" instruction. No tap PR was opened, no AUR package was
  submitted, and the `container` job in `release.yml` has never actually run (it only runs
  on a `v*` tag push, which hasn't happened).
- README mention: not added — I don't own `README.md` (V-A4). Noting here for V-A4's future
  consideration: once at least one channel's checksums are real (post-tag-push), Docker,
  Homebrew, AUR and Nix are all real, working install paths worth documenting; until then
  they'd be advertising 404s.

## Secrets/logging review

- The new `container` job in `release.yml` uses only the built-in `${{ secrets.GITHUB_TOKEN }}`
  for `ghcr.io` login (standard GitHub Actions pattern, `packages: write` scoped at the job
  level) — no new secret was introduced.
- The `TACK_API_TOKEN` used to test the Docker image locally
  (`test-token-for-local-verification-only`) was a throwaway string passed as an env var to
  a disposable container; it was never committed, logged to a file, or reused.
- None of the three packaging recipes handle credentials of any kind.

## Safe merge order and likely conflicts

Depends on V-A3 (already integrated locally — `v0.1.0-beta.7` tag exists on `develop`'s
history per this worktree's base). No other Part IV/V card touches `packaging/**`,
`crates/tack-cli/Cargo.toml`, or the root `Cargo.toml`'s `[workspace.package]` table
(checked `TODO.md` §V.2). `.github/workflows/release.yml` is touched additively only (one
new job appended at the end); diff the file directly to confirm nothing above the new
`container:` job changed. **The one file worth double-checking at integration time is
`Dockerfile`** — it isn't on any card's ownership list, so if another in-flight card
happens to also be touching it, that's the one real collision risk; my change is a
two-line reorder plus a comment, low surface area to conflict on.

## Checklist

- No unowned files: **one exception, flagged above and here again** — `Dockerfile` was
  modified despite not being on this card's (or apparently any card's) ownership list,
  because the existing file did not build at all (reproduced on a stock, unmodified base
  image — not a sandbox artifact) and my container-publish job depends on it building. Fix
  is two lines reordered plus a comment; verified both broken-before and fixed-after.
- No live secret: confirmed (see Secrets/logging review above).
- No panic stub: N/A — no Rust application code changed, only `Cargo.toml` metadata tables
  and packaging/CI recipe files.
- No blind retry: N/A — no retry logic introduced by this card.
