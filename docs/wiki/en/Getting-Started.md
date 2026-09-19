# Get started

[简体中文](../zh-CN/Getting-Started.md) · [Guide](Home.md)

## Install a release

Native releases cover Linux x86_64/ARM64, macOS Intel/Apple silicon, and Windows
x64/ARM64. Linux/macOS need Bash, `curl`, `tar`, and `sha256sum` or `shasum`.
Windows needs PowerShell 5.1 or newer. Use a terminal with at least 40 × 10 cells;
80 × 24 or larger is more comfortable.

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/iebb/termgram/releases/latest/download/install.sh | bash
```

The default destination is `~/.local/bin`. To inspect the installer before
running it, save the same URL to a file, read it, then run `bash install.sh`.
For a prerelease or another destination:

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/iebb/termgram/releases/latest/download/install.sh \
  | CHANNEL=prerelease INSTALL_DIR="$HOME/bin" bash
```

Windows PowerShell:

```powershell
$installer = Invoke-RestMethod 'https://github.com/iebb/termgram/releases/latest/download/install.ps1'
& ([scriptblock]::Create([string]$installer))
# Optional:
& ([scriptblock]::Create([string]$installer)) -Channel prerelease -BinDir "$HOME\bin"
```

The Windows default is `%LOCALAPPDATA%\Programs\Termgram\bin`. Inspect `$installer`
first if desired. Installers verify the release's `SHA256SUMS`, select the native
architecture, and print the destination to add to PATH when needed. They do not
request elevation or edit PATH. Release selection and updates are described in
[Updates](Updates.md).

## Sign in and send a message

1. Run `tg`. Official release binaries include the project's Telegram application
   credentials. A source build needs your own credentials as described below.
2. Enter your phone in international format, then the login code and optional 2FA
   password. Password entry is masked. Tab on the phone screen starts QR login;
   scan it in Telegram under **Settings → Devices → Link Desktop Device**.
3. Move through chats with `j/k` or arrows, then Enter to open one.
4. Press `i`, type your message, and Enter to send. Ctrl-J adds a newline.
   Esc leaves the editor while keeping the draft in this running account.
5. `G` returns to the newest messages; `?` opens help. `q` quits from navigation.

QR codes rotate automatically. Tab switches between compact block rendering and
larger full-cell rendering. Esc returns to phone sign-in. F2 switches to the next
existing account; F3 adds an account, including from login/error screens.
All listed keys are defaults and can be [configured](Configuration.md).

## Build from source

Install Rust with `rustup`; the repository pins Rust 1.98. The platform's Rust
linker/C build toolchain is also required because SQLite and Lua are compiled
with the application. On macOS this is provided by Xcode Command Line Tools;
Windows builds use the MSVC toolchain.

```sh
git clone https://github.com/iebb/termgram.git
cd termgram
cp .env.example .env
```

Create an application at [my.telegram.org/apps](https://my.telegram.org/apps)
and fill `TELEGRAM_API_ID` and `TELEGRAM_API_HASH` in `.env`. Then run:

```sh
cargo run --release
# After building:
./target/release/tg
```

On Windows the executable is `target\release\tg.exe`. Application credentials
identify the Telegram client, not your Telegram account. Your login session stays
on your machine. Do not commit `.env` or session databases.

Use [configuration](Configuration.md) for paths and overrides,
[terminal integration](Terminal.md) for graphics/key issues, and
[synchronization](Synchronization.md) for offline/cache behavior.
