class Tack < Formula
  desc "Single-binary project manager with an agent-execution runner"
  homepage "https://github.com/yielab/tack"
  version "0.1.0-beta.7"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-aarch64.tar.gz"
      # Placeholder: no macOS build has been produced to compute this from.
      # Replace with the real digest from the published release's
      # SHA256SUMS before this formula is submitted to a tap.
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-x86_64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
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
    sha256 "1f15ac5b69fca569e268a2a9bf334a4ae0630e32853d0716a5d6910a2808df6c"
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
