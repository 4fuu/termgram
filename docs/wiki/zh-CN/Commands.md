# 冒号命令

[English](../en/Commands.md) · [使用指南](Home.md)

在聊天列表或会话的导航模式按 `:`，底部会显示命令、说明、参数用法和实际生效的快捷键。
不可用操作会说明缺少的选择或连接条件。标题保留打开命令栏时的账号和操作对象。
在输入框、搜索框、聊天筛选框或登录框输入 `:` 时，它仍是普通文字。

| 按键 | 行为 |
| --- | --- |
| Tab / Ctrl-N | 补全下一个候选 |
| Shift-Tab / Ctrl-P | 补全上一个候选 |
| Up / Down | 召回匹配当前输入前缀的较早/较新命令 |
| Enter | 执行完整命令；空输入时关闭 |
| Esc / Ctrl-C | 取消并返回会话 |
| Left/Right、Home/End、Ctrl-A/Ctrl-E | 移动光标 |
| Backspace/Delete、Ctrl-W、Ctrl-U | 编辑或清空输入 |

鼠标点击候选只填入命令，不会执行。缺少参数或出错时保留输入，继续编辑。
只执行完整命令名及明确提供的 `h`、`q` 别名；例如 `qui` 需要先补全再执行。
历史按账号在进程内保留，最多各 64 条，不写入磁盘。多行粘贴不会自动执行一串命令。

| 命令 | 行为 |
| --- | --- |
| `help [命令]`、`h` | 浏览全部命令或查看某条命令说明 |
| `chat <别名、ID 或名称>` | 打开当前账号的缓存聊天；Tab 筛选名称及 Lua 别名 |
| `folder <ID 或名称>` | 选择 All chats、Archive 或 Telegram 文件夹 |
| `account [槽位]` | 打开账号选择器，或切换到已有账号槽位 |
| `search [正则]` | 打开本地搜索，或在当前搜索范围提交正则 |
| `attach <paths...>` | 将本地文件加入目标聊天草稿，不会自动发送 |
| `paste` | 将原生剪贴板的文件、图片或文字加入目标聊天草稿 |
| `attachments` | 查看当前会话草稿中的附件 |
| `latest` | 回到已打开会话的最新消息 |
| `unread` | 定位到聊天已读边界后的首条入站消息 |
| `read` | 明确将目标聊天全部标读，并清除未读提醒 |
| `mark-unread` | 设置 Telegram 的未读提醒，不倒退消息回执 |
| `reply` | 回复明确选中的消息 |
| `preview` | 放大选中的图片或贴纸 |
| `reveal` | 必要时下载，并在 Finder、Explorer 或文件管理器中定位附件 |
| `pins` | 浏览当前会话的置顶消息 |
| `pin chat`、`unpin chat` | 设置聊天在打开命令栏时的文件夹中的置顶状态 |
| `pin message`、`unpin message` | 设置选中消息的置顶状态，沿用 Telegram 选项确认界面 |
| `archive`、`unarchive` | 将指定聊天归档，或从 Archive 恢复 |
| `sidebar [show、hide 或 toggle]` | 显示、隐藏或切换侧栏；省略参数时切换 |
| `color chat`、`color folder` | 打开对应颜色选择器 |
| `settings` | 打开应用设置 |
| `status` | 查看连接、DC、最近 Ping 和当前内存中的加载状态 |
| `refresh` | 刷新聊天和文件夹列表 |
| `quit`、`q` | 正常退出 |

直接执行聊天名称时，需要唯一且完整匹配。输入部分名称后用 Tab 选取 ID；同名聊天不会猜测。
补全使用已加载数据，输入时不会发送网络搜索。聊天列表因新消息重排时，置顶和归档仍使用
打开命令栏时的稳定对象。重复执行置顶或归档命令会保留期望状态，不会反向切换。

`search` 后第一个空格之后的内容按原样作为正则，包括反斜杠和末尾空格，不使用 shell 引号规则。
命令栏不执行 shell 命令。`status` 只读取现有观测值：没有 DC 或 Ping 已过期时显示不可用，
不会显示成零；页面也区分内存中的消息和持久缓存的搜索覆盖范围。

可在 `chats` 和 `conversation` 上下文重绑入口动作 `command`。`command` 上下文提供
`complete_next`、`complete_previous`、`history_previous`、`history_next`，以及普通编辑动作。
参见 [Lua 配置](Configuration.md)。快捷键和命令共用业务动作与说明。
