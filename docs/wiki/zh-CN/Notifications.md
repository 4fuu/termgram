# 通知与静音设置

[English](../en/Notifications.md) · [指南首页](Home.md)

对聊天使用 `:mute`、`:mute 1h`、`:mute 8h`、`:mute 2d` 或 `:mute forever`。
默认是**永久静音**。`:unmute` 明确启用该聊天的通知，即使账号的私聊或群聊默认设置
为静音。在聊天列表中，命令捕获当前选择的聊天；在内容区则捕获当前会话。命令标题
显示目标，新消息造成的列表重排不会改变它。

这些命令修改 [Telegram 官方通知设置](https://core.telegram.org/method/account.updateNotifySettings)，
因此会同步到官方客户端，需要网络连接。修改静音时间时，Termgram 保留已有的预览、
静默发帖及桌面声音偏好，也保留该会话已有的故事设置。界面等待 Telegram 确认的实际
状态，不提前假定成功。请求期间若收到新的通知设置更新，将通过刷新聊天列表取得实际
状态；迟到快照不会覆盖新更新。失败时保留最后确认的状态，并允许重试。静音不会
标读消息或修改草稿。

侧栏以 `[m]` 标记静音聊天；开启 Nerd Font 后使用划线铃铛图标。可配置的底栏
`notifications` 组件显示当前焦点聊天的 **Muted forever**，或本地日期/时间的
静音截止时间。取消静音或到期后该信息消失，标记和截止时间不依赖颜色区分。
排除静音聊天的文件夹继续遵循已有的 Telegram 成员规则。

Lua 动作 `mute_chat`、`unmute_chat` 可绑定在 `chats` 或 `conversation` 上下文；
`mute_chat` 表示永久静音。默认没有分配快捷键，例如：

```lua
keymap = {
  { context = "chats", on = { "m" }, run = "mute_chat" },
  { context = "chats", on = { "M" }, run = "unmute_chat" },
},
```

## 未读提及与回复给你的消息

在聊天列表或内容区按 `g m`，或者使用 `:mentions`。**Unread mentions** 面板使用
Telegram 的[未读提及列表](https://core.telegram.org/method/messages.getUnreadMentions)，
包括回复给你的消息。侧栏中的彩色 `@` 标记有未读提及的聊天；选中提及消息后，底栏
也显示其状态。即使关闭颜色或没有 Nerd Font，这些标记仍可辨认。

用上下键选择，Enter 加载上下文并选中原消息，Ctrl-N/Ctrl-P 翻页，Ctrl-R 刷新，
Esc 关闭。`/` 返回保留的结果；`:search` 切回本地正则搜索。面板固定打开时的目标
聊天，并保留草稿。总数是搜索时 Telegram 返回的计数；其他设备发生变化后可刷新。
收到已读或删除更新时会移除对应结果。搜索中的摘要不会标读提及。本面板没有查询输入框。
Lua 动作 `mentions` 可用于 `chats`、`conversation`，面板按键使用 `search` 上下文。

只有终端获得焦点、聊天中确实绘制到消息末尾时，才确认文字提及的内容已读。加载历史、
搜索、打开浮层或终端失去焦点时不会确认。提及的内容回执和普通聊天已读边界相互独立。
语音、圆形视频和阅后即焚媒体需要实际消费；看到消息不会将其标记为已播放。在 Termgram
的播放流程接入这些回执前，可在其他 Telegram 客户端中消费。`:read` 将聊天历史标读，
不会假定这些媒体已经播放。

其他客户端的内容回执会更新消息、保留的结果和本地缓存。较慢的历史或搜索快照不会
恢复旧的未读标记。提及导航需要连接；重连后可用 Ctrl-R 重试。

原生桌面提醒将与 Telegram 静音设置及提及回执分开实现。
