# Lua 配置

[English](../en/Configuration.md)

把 `config.lua` 放在 `settings.conf` 旁，或用 `TERMGRAM_CONFIG` 指定路径。
重启后生效，可以从[配置示例](../../../examples/config.lua)开始。配置返回一个 Lua table，
支持 table、字符串、数学和 UTF-8 辅助函数，不提供文件系统、进程或插件接口。
配置错误会显示提示，并继续使用默认快捷键。

```lua
return {
  chats = { work = -1001234567890 },
  ghost_text = "{send} 发送 · {newline} 换行",
  keymap = {
    { context = "conversation", on = { "g", "w" }, run = "jump work" },
    { context = "compose", on = { "<Enter>" }, run = "newline" },
    { context = "compose", on = { "<C-s>" }, run = "send" },
  },
}
```

上下文包括 `global`、`chats`、`conversation`、`compose`、`input`（登录与聊天过滤）
和 `overlay`。具体上下文优先于全局绑定。同一上下文中配置相同按键会替换默认绑定；
`run = "noop"` 删除绑定。组合键用独立按键列表表示，例如 `{ "g", "w" }`。
配置会检查前缀冲突。组合键一秒后过期，Escape 可以取消尚未完成的组合键。
按键表示法沿用 Yazi，例如 `<C-s>`、`<A-x>`、`<S-Enter>`、`<Tab>`、`<Esc>`。

聊天别名使用稳定的 Telegram 数字 ID，超级群为 `-100…` 格式；目标需要已经出现在
本地聊天列表中。绑定可选填 `desc` 作为帮助页说明。帮助页读取当前生效的绑定，
使用方向键滚动。

默认 `j/k` 在聊天之间或实际消息之间移动，`20k` 向上移动 20 条消息，必要时继续
加载更早的历史。`G` 或 End 回到最新消息；`gg` 或 Home 到当前已加载窗口的最早消息。
方向键按显示行滚动，PageUp/PageDown 每次滚动十行。`i` 输入，`R` 回复，`r` 跳转到
回复目标，`/` 在聊天列表中过滤，Tab 切换面板，`s` 设置，`a` 账号，`?` 帮助。
输入框内的普通文字不会触发导航组合键或数字前缀。

输入框占位提示中的 `{send}`、`{newline}`、`{cancel}` 会替换为实际快捷键。
`ghost_text = ""` 隐藏提示。应用内偏好单独保存，不会改写 Lua 文件。

按 `g i` 显示当前聊天 ID，可用来配置别名。
