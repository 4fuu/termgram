# Termgram

A keyboard-first Telegram client for the terminal, built with Rust, Grammers and
Ratatui. Read direct messages and groups, navigate with Vim-style keys, and keep
your configuration in Lua.

Termgram opens cached conversations immediately and synchronizes in the
background. It supports Telegram folders, local regex search, per-chat colors,
inline media previews, and file-manager reveal. This is an unofficial client;
broadcast channels and secret chats are outside its current scope.

**[English guide](docs/wiki/en/Home.md) · [简体中文指南](docs/wiki/zh-CN/Home.md)**

Documentation follows the source revision you are reading. Features on an open
PR branch may not yet be in the latest release.

## Get started

Linux / macOS:

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/iebb/termgram/releases/latest/download/install.sh | bash
```

Windows PowerShell:

```powershell
$installer = Invoke-RestMethod 'https://github.com/iebb/termgram/releases/latest/download/install.ps1'
& ([scriptblock]::Create([string]$installer))
```

Run `tg`, sign in with your phone or press Tab for QR login. Open a chat with
Enter, press `i` to compose, and `?` for help. Use `tg update` to update.

Releases support Linux x86_64/ARM64, macOS Intel/Apple silicon, and Windows
x64/ARM64. See [installation and source builds](docs/wiki/en/Getting-Started.md)
for prerequisites, installer options, and Telegram API credentials.

## Development

Rust 1.98 is pinned in `rust-toolchain.toml`. Create Telegram application
credentials at [my.telegram.org/apps](https://my.telegram.org/apps), copy
`.env.example` to `.env`, and fill in your API ID and hash.

```sh
cargo run --release
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

See [development and architecture](docs/wiki/en/Development.md),
[engineering principles](AGENTS.md), and [upstream provenance](vendor/README.md).
The [Wiki source](docs/wiki/README.md) is reviewed alongside code changes and
provides English and Simplified Chinese pages. Local working plans live in the
Git-ignored `dev-notes/` directory.
