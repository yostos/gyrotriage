class Gyrotriage < Formula
  desc "Score DJI drone footage shake intensity and recommend Gyroflow parameters"
  homepage "https://github.com/yostos/gyrotriage"
  url "https://github.com/yostos/gyrotriage/archive/refs/tags/v1.1.0-rc.tar.gz"
  sha256 "628b144ec0499640a41aa4157bf1dd4c8b9c6c3232be47899cb98ec574ad563d"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "gyrotriage", shell_output("#{bin}/gyrotriage --help")
  end
end
