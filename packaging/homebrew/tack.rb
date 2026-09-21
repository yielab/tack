class Tack < Formula
  desc "Single-binary project manager with an agent-execution runner"
  homepage "https://github.com/yielab/tack"
  version "0.1.0-beta.7"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-aarch64.tar.gz"
      sha256 "8ec107f9423b29f4579183853fd32f834cb656850a68a29ca6a32913943227ac"
    else
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-x86_64.tar.gz"
      sha256 "e0016852bcae01da032e94de57e86346fe0fcf1e877512ce8962a6f43230265d"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      # release.yml's build matrix has no linux-aarch64 leg — there is no
      # asset this formula could point at, so fail clearly instead of
      # serving the x86_64 binary to an incompatible CPU.
      odie "tack has no published Linux ARM64 build yet."
    end

    url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-linux-x86_64.tar.gz"
    sha256 "9faf72396a4a8804521ad878dacebd9925ae2ef86803866dd89c9ca5daddb074"
  end

  def install
    bin.install "tack"
    doc.install "README.md", "QUICKSTART.txt", "LICENSE"
  end

  def caveats
    <<~EOS
      tack stores its data (tack.db and a storage/ directory for attachments)
      in the directory you run it from, not under the Homebrew prefix.
      Start it with:
        tack
      then open http://localhost:3210
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tack --version")
  end
end
