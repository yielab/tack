#!/usr/bin/env bash
# Re-points packaging/{nix,homebrew,aur} at a published release and fills in
# the digests from that release's own SHA256SUMS.
#
# These three recipes are the install paths nothing else exercises: the
# install.sh one-liner is covered by scripts/verify-install-urls.sh, and
# `cargo install` builds from source, but a Homebrew formula or a PKGBUILD is
# only ever checked by the person who taps it. Left to hand-editing they drift
# silently, and a wrong digest is not a soft failure — it aborts the install
# with a hash mismatch, which reads to a stranger as a tampered download.
#
# Run it after a release's artifacts exist, not before:
#   scripts/sync-packaging.sh v0.1.0-beta.9
#   scripts/sync-packaging.sh              # newest published release
#
# It refuses to write anything unless every digest it needs is present, so a
# half-published release leaves the recipes as they were.
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

[ -n "$linux"   ] || die "SHA256SUMS has no tack-$TAG-linux-x86_64.tar.gz"
[ -n "$mac_arm" ] || die "SHA256SUMS has no tack-$TAG-macos-aarch64.tar.gz"
[ -n "$mac_x86" ] || die "SHA256SUMS has no tack-$TAG-macos-x86_64.tar.gz"

printf '  linux-x86_64   %s\n' "$linux"
printf '  macos-aarch64  %s\n' "$mac_arm"
printf '  macos-x86_64   %s\n' "$mac_x86"

TAG="$TAG" VERSION="$VERSION" LINUX="$linux" MAC_ARM="$mac_arm" MAC_X86="$mac_x86" \
ROOT="$ROOT" python3 - <<'PY'
import os, pathlib, re

root = pathlib.Path(os.environ["ROOT"])
tag, version = os.environ["TAG"], os.environ["VERSION"]
linux, mac_arm, mac_x86 = os.environ["LINUX"], os.environ["MAC_ARM"], os.environ["MAC_X86"]

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

# ── nix ──────────────────────────────────────────────────────────────────────
s = (root / "packaging/nix/default.nix").read_text()
s = STALE.sub("", s)
s = re.sub(r'version = "[^"]*";', f'version = "{version}";', s, count=1)
s = re.sub(r'sha256 = "[0-9a-f]*";', f'sha256 = "{linux}";', s, count=1)
write("packaging/nix/default.nix", s)

# ── homebrew: arm mac, then intel mac, then linux, in file order ─────────────
# One pass, not three substitutions: a digest written by an earlier pass
# matches the same pattern, so repeated count=1 subs all land on the first
# occurrence and the later slots keep their old value.
s = (root / "packaging/homebrew/tack.rb").read_text()
s = STALE.sub("", s)
s = re.sub(r'version "[^"]*"', f'version "{version}"', s, count=1)
digests = iter((mac_arm, mac_x86, linux))
s, n = re.subn(r'sha256 "[0-9a-f]*"', lambda _: f'sha256 "{next(digests)}"', s)
if n != 3:
    raise SystemExit(f"sync-packaging: expected 3 sha256 lines in tack.rb, found {n}")
write("packaging/homebrew/tack.rb", s)

# ── aur: pkgver forbids "-", so the tag's suffix becomes dots ────────────────
s = (root / "packaging/aur/PKGBUILD").read_text()
s = STALE.sub("", s)
s = re.sub(r'^pkgver=.*$', f'pkgver={version.replace("-", ".")}', s, count=1, flags=re.M)
s = re.sub(r'^_tag=".*"$', f'_tag="{tag}"', s, count=1, flags=re.M)
s = re.sub(r"sha256sums=\('[0-9a-f]*'\)", f"sha256sums=('{linux}')", s, count=1)
write("packaging/aur/PKGBUILD", s)
PY

printf '\npackaging/ now points at %s. Review the diff before committing.\n' "$TAG"
