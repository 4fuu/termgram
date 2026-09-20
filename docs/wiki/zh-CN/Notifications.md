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

本页当前介绍 Telegram 静音设置；原生桌面提醒和未读提及导航上线后也会在此说明。
