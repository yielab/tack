# Fetches and repackages the release archive rather than building from
# source — release.yml's profile (lto + opt-level="z") already produces a
# small, statically-linked (musl) binary, and stdenvNoCC avoids pulling in a
# Rust toolchain just to unpack a tarball. Because the Linux binary is a
# static musl build, it needs no interpreter/rpath patching (no
# autoPatchelfHook) to run under Nix's non-FHS store layout, unlike most
# prebuilt Linux binaries.
{ lib, stdenvNoCC, fetchurl }:

# Same binary, two possible sources — keyed on the evaluating system rather
# than split into arch-suffixed attributes, since Nix (unlike a PKGBUILD)
# only ever evaluates for one platform at a time.
let
  isAarch64 = stdenvNoCC.hostPlatform.system == "aarch64-linux";
  platform = if isAarch64 then "aarch64" else "x86_64";
  sha256 = if isAarch64
    then "74ca910dab26ab16c900f45e147d482657e714b5188bfb2db1e1519eaddfe0a7"
    else "fc79508828cd7bc41150110ca35c1b13f57f325f2e72c0f21bdd09775ffbf19a";
in
stdenvNoCC.mkDerivation rec {
  pname = "tack";
  version = "0.1.0-beta.10";

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
