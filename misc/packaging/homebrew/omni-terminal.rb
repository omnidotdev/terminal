# Reference copy — CI auto-generates this in omnidotdev/homebrew-tap on each release.
# See .github/workflows/release.yml "Update Homebrew tap" step.
cask "omni-terminal" do
  version "${VERSION}"
  sha256 "${SHA256}"

  url "https://github.com/omnidotdev/terminal/releases/download/v#{version}/OmniTerminal-v#{version}.dmg"
  name "Omni Terminal"
  desc "GPU-accelerated terminal emulator built to run everywhere"
  homepage "https://terminal.omni.dev"

  app "OmniTerminal.app"

  zap trash: [
    "~/.config/omni/terminal",
    "~/Library/Caches/dev.omni.Terminal",
    "~/Library/Preferences/dev.omni.Terminal.plist",
  ]
end
