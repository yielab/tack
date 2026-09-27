class Tack < Formula
  desc "Single-binary project manager with an agent-execution runner"
  homepage "https://github.com/yielab/tack"
  license "MIT"

  # Homebrew scans the version out of the archive URL. On the aarch64
  # archives that scan is right; on the x86_64 ones it reads "86.64" out of
  # "x86_64", which mislabels the install and fails the test below. So the
  # intel branches name the version explicitly and the arm branches don't —
  # `brew audit` rejects an explicit version wherever the scan already agrees.
  on_macos do
    on_arm do
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.9/tack-v0.1.0-beta.9-macos-aarch64.tar.gz"
      sha256 "ee7d2fb469d97350374e2d277832fada933b96e233f8e9e34c53729c56f9ff9c"
    end
    on_intel do
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.9/tack-v0.1.0-beta.9-macos-x86_64.tar.gz"
      sha256 "f58564fe80302b3581936d83b01d6ba08b54db8235b0ce993513a22adccde6d2"
      version "0.1.0-beta.9"
    end
  end

  on_linux do
    on_arm do
      # tack-packaging:linux-aarch64 — sync-packaging.sh maintains this
      # branch. release.yml's build matrix has no linux-aarch64 leg yet, so
      # there is no asset this formula could point at; fail clearly instead
      # of serving the x86_64 binary to an incompatible CPU. Once a release
      # publishes tack-<tag>-linux-aarch64.tar.gz, sync-packaging.sh replaces
      # this odie with a url/sha256 pair, and restores it here if a later
      # release ever drops that asset again.
      odie "tack has no published Linux ARM64 build yet."
    end
    on_intel do
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.9/tack-v0.1.0-beta.9-linux-x86_64.tar.gz"
      sha256 "30a70b4c97cf2b52aae86ca5d461776c312974a2408e43e993fc756853efdab4"
      version "0.1.0-beta.9"
    end
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
