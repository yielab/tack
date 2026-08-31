# Fetches and repackages the release archive rather than building from
# source — release.yml's profile (lto + opt-level="z") already produces a
# small, statically-linked (musl) binary, and stdenvNoCC avoids pulling in a
# Rust toolchain just to unpack a tarball. Because the Linux binary is a
# static musl build, it needs no interpreter/rpath patching (no
# autoPatchelfHook) to run under Nix's non-FHS store layout, unlike most
# prebuilt Linux binaries.
{ lib, stdenvNoCC, fetchurl }:

stdenvNoCC.mkDerivation rec {
  pname = "tack";
  version = "0.1.0-beta.7";

  src = fetchurl {
    url = "https://github.com/yielab/tack/releases/download/v${version}/tack-v${version}-linux-x86_64.tar.gz";
    # Computed from a local build standing in for the real release asset
    # (the tag above is not yet published) — replace with the digest from
    # the published release's SHA256SUMS.
    sha256 = "1f15ac5b69fca569e268a2a9bf334a4ae0630e32853d0716a5d6910a2808df6c";
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
