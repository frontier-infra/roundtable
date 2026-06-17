# Homebrew formula for Roundtable — a Frontier Infra project.
#
# At release this is promoted to the frontier-infra/homebrew-tap repo so the
# advertised command is:
#
#     brew install frontier-infra/tap/roundtable
#
# Update `url` to a tagged release tarball and fill in `sha256` at cut
# (compute with:  curl -fsSL <url> | shasum -a 256).
class Roundtable < Formula
  desc "A council of frontier models — for the decisions that matter"
  homepage "https://roundtable.sh"
  url "https://github.com/frontier-infra/roundtable/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_RELEASE_TARBALL_SHA256"
  license "MIT"
  head "https://github.com/frontier-infra/roundtable.git", branch: "main"

  # The council engine is bash + python3 (stdlib only); python3 also powers
  # `roundtable mcp serve`.
  depends_on "python@3.12"

  def install
    # Keep the bin/ + lib/ sibling layout intact so the dispatcher's relative
    # `../lib` lookup works; expose the command via a symlink into bin.
    libexec.install "bin", "lib"
    bin.install_symlink libexec/"bin/roundtable"
  end

  test do
    assert_match "roundtable", shell_output("#{bin}/roundtable version")
  end
end
