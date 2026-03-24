class Binman < Formula
  desc "Terminal UI HTTP client for .http files"
  homepage "https://github.com/bgunnarsson/binreq"
  url "https://github.com/bgunnarsson/binreq/archive/refs/tags/v0.0.1.tar.gz"
  sha256 ""
  license "MIT"

  depends_on "go" => :build

  def install
    system "go", "build", *std_go_args(ldflags: "-s -w"), "./cmd/binman"
  end

  test do
    assert_match "BINMAN", shell_output("#{bin}/binman 2>&1", 1)
  end
end
