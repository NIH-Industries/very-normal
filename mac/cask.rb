cask "very-normal-compose" do
  version "0.0.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/NIH-Industries/very-normal/releases/download/v#{version}/VeryNormalCompose-#{version}.zip"
  name "Very Normal Compose"
  desc "Menu bar compose window that counts your real typing into very-normal"
  homepage "https://github.com/NIH-Industries/very-normal"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on formula: "nih-industries/tap/very-normal"
  depends_on macos: ">= :sonoma"

  app "Very Normal Compose.app"

  uninstall quit: "industries.nih.very-normal.compose"

  zap trash: "~/Library/Preferences/industries.nih.very-normal.compose.plist"
end
