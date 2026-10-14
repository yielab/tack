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
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.10/tack-v0.1.0-beta.10-macos-aarch64.tar.gz"
      sha256 "9444af5a557fe177e7d2914a21b0ac3de4230a08d959c6315b6cf3dd491541a7"
    end
    on_intel do
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.10/tack-v0.1.0-beta.10-macos-x86_64.tar.gz"
      sha256 "5a4d5a55eecd7dd708ab6cc9d37e265fe915b897e74f70078a0db34ca526129e"
      version "0.1.0-beta.10"
    end
  end

  on_linux do
    on_arm do
      # tack-packaging:linux-aarch64 — sync-packaging.sh maintains this branch.
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.10/tack-v0.1.0-beta.10-linux-aarch64.tar.gz"
      sha256 "74ca910dab26ab16c900f45e147d482657e714b5188bfb2db1e1519eaddfe0a7"
    end
    on_intel do
      url "https://github.com/yielab/tack/releases/download/v0.1.0-beta.10/tack-v0.1.0-beta.10-linux-x86_64.tar.gz"
      sha256 "fc79508828cd7bc41150110ca35c1b13f57f325f2e72c0f21bdd09775ffbf19a"
      version "0.1.0-beta.10"
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
