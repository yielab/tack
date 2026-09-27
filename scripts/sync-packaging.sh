#!/usr/bin/env bash
# Re-points packaging/{nix,homebrew,aur,scoop} at a published release and
# fills in the digests from that release's own SHA256SUMS.
#
# These recipes are the install paths nothing else exercises: the install.sh
# one-liner is covered by scripts/verify-install-urls.sh, and `cargo install`
# builds from source, but a Homebrew formula, a PKGBUILD, a Nix derivation or
# a Scoop manifest is only ever checked by the person who taps/installs it.
# Left to hand-editing they drift silently, and a wrong digest is not a soft
# failure — it aborts the install with a hash mismatch, which reads to a
# stranger as a tampered download.
#
# Run it after a release's artifacts exist, not before:
#   scripts/sync-packaging.sh v0.1.0-beta.9
#   scripts/sync-packaging.sh              # newest published release
#
# It refuses to write anything unless every digest it needs is present, so a
# half-published release leaves the recipes as they were. linux-aarch64 is
# the one exception: release.yml only grew that build leg after
# v0.1.0-beta.9, so a release missing that asset is normal, not half-published
# — see the "linux-aarch64 is optional" section below.
#
# ── linux-aarch64 is optional ────────────────────────────────────────────────
# Homebrew, the AUR PKGBUILD and the Nix derivation each need a Linux ARM64
# branch eventually, and each is handled the same way: this script always
# normalizes the recipe to its "no ARM asset published" shape first, then, if
# and only if SHA256SUMS has a tack-<tag>-linux-aarch64.tar.gz entry, expands
# it into the ARM-aware shape. That normalize-then-fill order (rather than
# trying to detect and patch whatever shape the file is already in) is what
# makes a second run idempotent and a future release that *drops* the ARM
# asset again safe — the recipe just falls back to its no-ARM shape.
#   - homebrew/tack.rb: the `on_linux` block's `on_arm` branch
#     stays the `odie` placeholder (marked `tack-packaging:linux-aarch64`)
#     until an ARM digest exists, then becomes a normal url/sha256 pair.
#   - aur/PKGBUILD: stays single-arch (`arch=('x86_64')`, plain `source`/
#     `sha256sums`) until an ARM digest exists, then becomes dual-arch
#     (`arch=('x86_64' 'aarch64')`, `source_x86_64`/`source_aarch64`,
#     `sha256sums_x86_64`/`sha256sums_aarch64`, `_pkgdir` keyed on `$CARCH`).
#   - nix/default.nix: has no odie equivalent (Nix has no "fail with a
#     message" install step short of `throw`, and there's nothing to guard
#     against here since Nix's `platforms` already restricts evaluation to
#     the one architecture it lists) — this script just regenerates the whole
#     file from one of two fixed templates, single- or dual-arch, so there's
#     no drift to detect in the first place.
set -euo pipefail

REPO="yielab/tack"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

die() { printf 'sync-packaging: %s\n' "$1" >&2; exit 1; }

TAG="${1:-}"
if [ -z "$TAG" ]; then
  command -v gh >/dev/null 2>&1 || die "no tag given and gh is not installed"
  TAG="$(gh release list --repo "$REPO" --limit 1 --json tagName --jq '.[0].tagName')"
  [ -n "$TAG" ] || die "could not resolve the newest release"
fi
case "$TAG" in v*) ;; *) TAG="v$TAG" ;; esac

VERSION="${TAG#v}"
SUMS_URL="https://github.com/$REPO/releases/download/$TAG/SHA256SUMS"

printf 'Reading %s\n' "$SUMS_URL"
sums="$(curl -fsSL "$SUMS_URL")" \
  || die "no SHA256SUMS for $TAG — is the release published and its build finished?"

# "<digest>  <name>", or "<digest> *<name>" in binary mode.
digest_for() {
  printf '%s\n' "$sums" | awk -v f="$1" '$2 == f || $2 == "*" f { print $1; exit }'
}

linux="$(digest_for "tack-$TAG-linux-x86_64.tar.gz")"
mac_arm="$(digest_for "tack-$TAG-macos-aarch64.tar.gz")"
mac_x86="$(digest_for "tack-$TAG-macos-x86_64.tar.gz")"
win="$(digest_for "tack-$TAG-windows-x86_64.zip")"
arm_linux="$(digest_for "tack-$TAG-linux-aarch64.tar.gz")"

[ -n "$linux"   ] || die "SHA256SUMS has no tack-$TAG-linux-x86_64.tar.gz"
[ -n "$mac_arm" ] || die "SHA256SUMS has no tack-$TAG-macos-aarch64.tar.gz"
[ -n "$mac_x86" ] || die "SHA256SUMS has no tack-$TAG-macos-x86_64.tar.gz"
[ -n "$win"     ] || die "SHA256SUMS has no tack-$TAG-windows-x86_64.zip"

printf '  linux-x86_64    %s\n' "$linux"
printf '  macos-aarch64   %s\n' "$mac_arm"
printf '  macos-x86_64    %s\n' "$mac_x86"
printf '  windows-x86_64  %s\n' "$win"
if [ -n "$arm_linux" ]; then
  printf '  linux-aarch64   %s\n' "$arm_linux"
else
  printf '  linux-aarch64   (not published for %s — ARM recipes stay in their no-ARM shape)\n' "$TAG"
fi

TAG="$TAG" VERSION="$VERSION" LINUX="$linux" MAC_ARM="$mac_arm" MAC_X86="$mac_x86" \
WIN="$win" ARM_LINUX="$arm_linux" ROOT="$ROOT" python3 - <<'PY'
import json, os, pathlib, re

root = pathlib.Path(os.environ["ROOT"])
tag, version = os.environ["TAG"], os.environ["VERSION"]
linux, mac_arm, mac_x86 = os.environ["LINUX"], os.environ["MAC_ARM"], os.environ["MAC_X86"]
win = os.environ["WIN"]
arm_linux = os.environ["ARM_LINUX"]  # "" when the release has no ARM asset

# The three recipes were seeded with a digest computed from a local build
# standing in for the release asset, and said so in a comment. Once a real
# release backs them the comment is not just stale, it is wrong, so it goes.
STALE = re.compile(
    r'[ \t]*#[^\n]*(?:standing in for|Placeholder:)[^\n]*\n'
    r'(?:[ \t]*#[^\n]*\n)*'
)

def write(path, text):
    p = root / path
    p.write_text(text)
    print(f"  wrote {path}")

# ── homebrew: arm mac, then intel mac, then linux, in file order ─────────────
# One pass, not three substitutions: a digest written by an earlier pass
# matches the same pattern, so repeated count=1 subs all land on the first
# occurrence and the later slots keep their old value.
#
# The linux-aarch64 branch is normalized to its odie placeholder first, so
# the sha256 pass below always sees exactly three slots (mac-arm, mac-x86,
# linux-x86_64) regardless of whether a previous run had already filled the
# ARM branch in. It's filled back in afterwards, for real, if the release has
# that asset.
ARM_LINUX_RE = re.compile(r'(on_linux do\n    on_arm do\n)(.*?)(\n    end\n    on_intel do\n)', re.DOTALL)

ODIE_BODY = (
    '      # tack-packaging:linux-aarch64 — sync-packaging.sh maintains this\n'
    '      # branch. release.yml\'s build matrix has no linux-aarch64 leg yet, so\n'
    '      # there is no asset this formula could point at; fail clearly instead\n'
    '      # of serving the x86_64 binary to an incompatible CPU. Once a release\n'
    '      # publishes tack-<tag>-linux-aarch64.tar.gz, sync-packaging.sh replaces\n'
    '      # this odie with a url/sha256 pair, and restores it here if a later\n'
    '      # release ever drops that asset again.\n'
    '      odie "tack has no published Linux ARM64 build yet."'
)

def filled_arm_body(digest):
    return (
        '      # tack-packaging:linux-aarch64 — sync-packaging.sh maintains this branch.\n'
        f'      url "https://github.com/yielab/tack/releases/download/{tag}/tack-{tag}-linux-aarch64.tar.gz"\n'
        f'      sha256 "{digest}"'
    )

s = (root / "packaging/homebrew/tack.rb").read_text()
s = STALE.sub("", s)
# Only the intel branches carry a `version` line (the formula's header
# comment says why), so the tag lives literally in every URL and both are
# rewritten. The tag itself contains hyphens (v0.1.0-beta.9), so the URL
# match runs up to the platform suffix rather than to the first hyphen.
s = re.sub(r'/download/v[^/]+/tack-v[^/]+?-(?=(?:linux|macos|windows)-)', f'/download/{tag}/tack-{tag}-', s)
s = re.sub(r'version "[^"]*"', f'version "{version}"', s)

s, n = ARM_LINUX_RE.subn(lambda m: m.group(1) + ODIE_BODY + m.group(3), s)
if n != 1:
    raise SystemExit("sync-packaging: could not find the linux-aarch64 branch in tack.rb")

digests = iter((mac_arm, mac_x86, linux))
s, n = re.subn(r'sha256 "[0-9a-f]*"', lambda _: f'sha256 "{next(digests)}"', s)
if n != 3:
    raise SystemExit(f"sync-packaging: expected 3 sha256 lines in tack.rb, found {n}")

if arm_linux:
    s, n = ARM_LINUX_RE.subn(lambda m: m.group(1) + filled_arm_body(arm_linux) + m.group(3), s)
    if n != 1:
        raise SystemExit("sync-packaging: could not re-fill the linux-aarch64 branch in tack.rb")

write("packaging/homebrew/tack.rb", s)

# ── aur: pkgver forbids "-", so the tag's suffix becomes dots ────────────────
# Same normalize-then-fill approach as the Homebrew formula: the tail of the
# file (from _tag= through the sha256sums line(s)) is always rebuilt from
# scratch, single-arch unless the release has a linux-aarch64 asset.
s = (root / "packaging/aur/PKGBUILD").read_text()
s = STALE.sub("", s)
s = re.sub(r'^pkgver=.*$', f'pkgver={version.replace("-", ".")}', s, count=1, flags=re.M)

arch_line = "arch=('x86_64' 'aarch64')" if arm_linux else "arch=('x86_64')"
s, n = re.subn(r"^arch=\(.*\)$", arch_line, s, count=1, flags=re.M)
if n != 1:
    raise SystemExit("sync-packaging: could not find the arch= line in PKGBUILD")

if arm_linux:
    tail = (
        f'_tag="{tag}"\n'
        '_pkgdir="tack-${_tag}-linux-${CARCH}"\n'
        'source_x86_64=("${url}/releases/download/${_tag}/tack-${_tag}-linux-x86_64.tar.gz")\n'
        f"sha256sums_x86_64=('{linux}')\n"
        'source_aarch64=("${url}/releases/download/${_tag}/tack-${_tag}-linux-aarch64.tar.gz")\n'
        f"sha256sums_aarch64=('{arm_linux}')\n"
        '\npackage() {'
    )
else:
    tail = (
        f'_tag="{tag}"\n'
        '_pkgdir="tack-${_tag}-linux-x86_64"\n'
        'source=("${url}/releases/download/${_tag}/${_pkgdir}.tar.gz")\n'
        f"sha256sums=('{linux}')\n"
        '\npackage() {'
    )

s, n = re.subn(r'_tag="[^"]*"\n.*?\n\npackage\(\) \{', lambda _: tail, s, count=1, flags=re.DOTALL)
if n != 1:
    raise SystemExit("sync-packaging: could not find the _tag/.../package() block in PKGBUILD")
write("packaging/aur/PKGBUILD", s)

# ── nix: fully regenerated from a fixed template, no in-place surgery ────────
NIX_HEADER = (
    "# Fetches and repackages the release archive rather than building from\n"
    "# source — release.yml's profile (lto + opt-level=\"z\") already produces a\n"
    "# small, statically-linked (musl) binary, and stdenvNoCC avoids pulling in a\n"
    "# Rust toolchain just to unpack a tarball. Because the Linux binary is a\n"
    "# static musl build, it needs no interpreter/rpath patching (no\n"
    "# autoPatchelfHook) to run under Nix's non-FHS store layout, unlike most\n"
    "# prebuilt Linux binaries.\n"
)

NIX_SINGLE_TMPL = """{ lib, stdenvNoCC, fetchurl }:

stdenvNoCC.mkDerivation rec {
  pname = "tack";
  version = "__VERSION__";

  src = fetchurl {
    url = "https://github.com/yielab/tack/releases/download/v${version}/tack-v${version}-linux-x86_64.tar.gz";
    sha256 = "__LINUX_SHA__";
  };

  sourceRoot = "tack-v${version}-linux-x86_64";

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 tack "$out/bin/tack"
    install -Dm644 LICENSE "$out/share/licenses/tack/LICENSE"
    install -Dm644 QUICKSTART.txt "$out/share/doc/tack/QUICKSTART.txt"
    runHook postInstall
  '';

  meta = with lib; {
    description = "Single-binary project manager with an agent-execution runner";
    homepage = "https://github.com/yielab/tack";
    license = licenses.mit;
    platforms = [ "x86_64-linux" ];
    mainProgram = "tack";
  };
}
"""

NIX_DUAL_TMPL = """{ lib, stdenvNoCC, fetchurl }:

# Same binary, two possible sources — keyed on the evaluating system rather
# than split into arch-suffixed attributes, since Nix (unlike a PKGBUILD)
# only ever evaluates for one platform at a time.
let
  isAarch64 = stdenvNoCC.hostPlatform.system == "aarch64-linux";
  platform = if isAarch64 then "aarch64" else "x86_64";
  sha256 = if isAarch64
    then "__ARM_SHA__"
    else "__LINUX_SHA__";
in
stdenvNoCC.mkDerivation rec {
  pname = "tack";
  version = "__VERSION__";

  src = fetchurl {
    url = "https://github.com/yielab/tack/releases/download/v${version}/tack-v${version}-linux-${platform}.tar.gz";
    inherit sha256;
  };

  sourceRoot = "tack-v${version}-linux-${platform}";

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    install -Dm755 tack "$out/bin/tack"
    install -Dm644 LICENSE "$out/share/licenses/tack/LICENSE"
    install -Dm644 QUICKSTART.txt "$out/share/doc/tack/QUICKSTART.txt"
    runHook postInstall
  '';

  meta = with lib; {
    description = "Single-binary project manager with an agent-execution runner";
    homepage = "https://github.com/yielab/tack";
    license = licenses.mit;
    platforms = [ "x86_64-linux" "aarch64-linux" ];
    mainProgram = "tack";
  };
}
"""

if arm_linux:
    body = NIX_DUAL_TMPL.replace("__VERSION__", version).replace("__LINUX_SHA__", linux).replace("__ARM_SHA__", arm_linux)
else:
    body = NIX_SINGLE_TMPL.replace("__VERSION__", version).replace("__LINUX_SHA__", linux)
write("packaging/nix/default.nix", NIX_HEADER + body)

# ── scoop: version, url and hash only — checkver/autoupdate use $version ────
scoop_path = root / "packaging/scoop/tack.json"
data = json.loads(scoop_path.read_text())
data["version"] = version
data["url"] = f"https://github.com/yielab/tack/releases/download/{tag}/tack-{tag}-windows-x86_64.zip"
data["hash"] = win
write("packaging/scoop/tack.json", json.dumps(data, indent=4) + "\n")
PY

printf '\npackaging/ now points at %s. Review the diff before committing.\n' "$TAG"
