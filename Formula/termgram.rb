class Termgram < Formula
  desc "Focused, keyboard-first Telegram client for the terminal"
  homepage "https://github.com/iebb/termgram"
  version "0.1.20"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/iebb/termgram/releases/download/v0.1.20/termgram-0.1.20-macos.tar.gz"
      sha256 "0f8441202fce1102b37fe2a911161edd2e07079ac463510a95c403b64d11c410"
    end
    on_intel do
      url "https://github.com/iebb/termgram/releases/download/v0.1.20/termgram-0.1.20-macos-x86_64.tar.gz"
      sha256 "a1e3e9d5469377e9f1e88196ac44e93f4f3aefaa669307ea4cfc4b37cffdce1d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/iebb/termgram/releases/download/v0.1.20/termgram-0.1.20-linux-aarch64.tar.gz"
      sha256 "982ce0e4ec1684b78f81649545eb83e385a01de70da115ab4c43621843cb17f4"
    end
    on_intel do
      url "https://github.com/iebb/termgram/releases/download/v0.1.20/termgram-0.1.20-linux.tar.gz"
      sha256 "1f911c98e47dda591c0918273be7d3c4f4b5b5a7d253bd3e84ded3fa5cfcdda4"
    end
  end

  def install
    bin.install "tg"
  end

  def caveats
    <<~EOS
      Run `tg` to launch Termgram.
      Update this installation with `brew upgrade termgram` instead of `tg update`
      so Homebrew can track the installed version.
    EOS
  end

  test do
    assert_match(/^(?:tg|version)\s+#{Regexp.escape(version.to_s)}$/, shell_output("#{bin}/tg --version"))
    assert_match "Open the TUI", shell_output("#{bin}/tg --help")
    assert_match "unknown argument", shell_output("#{bin}/tg invalid-command 2>&1", 1)
  end
end
