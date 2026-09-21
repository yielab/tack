class Tack < Formula
  desc "Single-binary project manager with an agent-execution runner"
  homepage "https://github.com/yielab/tack"
  version "0.1.0-beta.9"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-aarch64.tar.gz"
      sha256 "ee7d2fb469d97350374e2d277832fada933b96e233f8e9e34c53729c56f9ff9c"
    else
      url "https://github.com/yielab/tack/releases/download/v#{version}/tack-v#{version}-macos-x86_64.tar.gz"
      sha256 "f58564fe80302b3581936d83b01d6ba08b54db8235b0ce993513a22adccde6d2"
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
    sha256 "30a70b4c97cf2b52aae86ca5d461776c312974a2408e43e993fc756853efdab4"
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
