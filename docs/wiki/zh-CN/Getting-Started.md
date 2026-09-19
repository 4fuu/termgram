# 快速入门

[English](../en/Getting-Started.md) · [指南首页](Home.md)

## 安装发行版

提供 Linux x86_64/ARM64、macOS Intel/Apple silicon、Windows x64/ARM64 原生构建。
Linux/macOS 需要 Bash、`curl`、`tar`，以及 `sha256sum` 或 `shasum`；Windows 需要
PowerShell 5.1 或更高版本。终端至少 40 × 10，建议使用 80 × 24 或更大窗口。

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/iebb/termgram/releases/latest/download/install.sh | bash
```

默认安装到 `~/.local/bin`。如果希望先审阅脚本，可把相同 URL 下载为文件，阅读后执行
`bash install.sh`。选择预发布版或更改目录：

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/iebb/termgram/releases/latest/download/install.sh \
  | CHANNEL=prerelease INSTALL_DIR="$HOME/bin" bash
```

Windows PowerShell：

```powershell
$installer = Invoke-RestMethod 'https://github.com/iebb/termgram/releases/latest/download/install.ps1'
& ([scriptblock]::Create([string]$installer))
# 可选参数：
& ([scriptblock]::Create([string]$installer)) -Channel prerelease -BinDir "$HOME\bin"
```

Windows 默认目录为 `%LOCALAPPDATA%\Programs\Termgram\bin`，也可先查看 `$installer`
内容。安装器会校验发行版 `SHA256SUMS`、选择原生架构，并在需要时提示加入 PATH 的目录；
不会提权或自行修改 PATH。版本选择与升级规则见[更新](Updates.md)。

## 登录并发送消息

1. 执行 `tg`。官方发行包内置项目的 Telegram 应用凭据；源码构建需要下文所述的自有凭据。
2. 输入国际格式手机号、验证码以及可选的两步验证密码。密码输入会掩码显示。
   手机号页按 Tab 使用二维码，再用已登录的 Telegram 在 **设置 → 设备 → 连接桌面设备** 扫码。
3. 用 `j/k` 或方向键选择聊天，Enter 打开。
4. 按 `i` 进入输入框，输入消息后 Enter 发送，Ctrl-J 换行。
   Esc 退出输入，在当前账号的本次运行中保留草稿。
5. `G` 回到最新消息，`?` 打开帮助；导航状态按 `q` 退出。

二维码会自动轮换。Tab 切换紧凑字符块与较大的全单元格显示，Esc 返回手机号登录。
F2 切换已有账号，F3 添加账号；登录页和错误页也能使用。
以上均为默认按键，可通过 [Lua 配置](Configuration.md) 修改。

## 从源码构建

使用 `rustup` 安装 Rust，仓库固定 Rust 1.98。还需要平台对应的链接器/C 构建工具链，
因为 SQLite 与 Lua 随应用编译。macOS 可使用 Xcode Command Line Tools；Windows 使用 MSVC。

```sh
git clone https://github.com/iebb/termgram.git
cd termgram
cp .env.example .env
```

在 [my.telegram.org/apps](https://my.telegram.org/apps) 创建应用，把 API ID 与 API hash
填入 `.env` 的 `TELEGRAM_API_ID`、`TELEGRAM_API_HASH`，然后运行：

```sh
cargo run --release
# 构建完成后：
./target/release/tg
```

Windows 可执行文件为 `target\release\tg.exe`。应用凭据标识的是 Telegram 客户端，
你的账号登录会话保存在本机。不要提交 `.env` 或会话数据库。

路径与环境变量见[配置](Configuration.md)，图片/按键问题见[终端集成](Terminal.md)，
离线与缓存行为见[缓存与同步](Synchronization.md)。
