# Homebrew formula for harbor.
#
# Publishing checklist (see README):
#   1. push this repo to github.com/FerMPY/harbor
#   2. create a tagged release:  gh release create v0.1.0 --generate-notes
#   3. fill in the sha256 below:  curl -sL <url> | shasum -a 256
#   4. put this file in a tap repo named  github.com/FerMPY/homebrew-tap
#      then:  brew install FerMPY/tap/harbor
class Harbor < Formula
  desc "See what's docked at every local port — dev servers, grouped by project"
  homepage "https://github.com/FerMPY/harbor"
  url "https://github.com/FerMPY/harbor/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_TARBALL_SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "harbor", shell_output("#{bin}/harbor --help")
  end
end
