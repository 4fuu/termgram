# 外观

[English](../en/Appearance.md) · [指南首页](Home.md)

在聊天列表或聊天内容区按 `c` 设置聊天颜色；在列表按 `C` 设置当前文件夹颜色。
Up/Down 选择带名称的终端颜色，Enter 应用，Esc 取消。列表选中标记和选中样式不依赖
颜色本身，因此颜色不是唯一的状态提示。

聊天行和聊天标题默认使用终端前景色。文件夹标题从六种 ANSI 调色板颜色中稳定
选取，终端主题决定实际 RGB 值；这些颜色不假定终端一定采用深色背景。

优先级为：内置默认值 → Lua → 应用内覆盖。选择 **Follow configuration** 移除
覆盖并重新跟随配置。**Terminal default** 则是明确使用终端前景色，可以覆盖 Lua
中指定的彩色默认值。

```lua
return {
  colors = {
    chats = { [-1001234567890] = "cyan" },
    folders = { [2] = "yellow" },
  },
}
```

可用值：`default`、`black`、`red`、`green`、`yellow`、`blue`、`magenta`、`cyan`、
`gray`、`dark_gray`、`light_red`、`light_green`、`light_yellow`、`light_blue`、
`light_magenta`、`light_cyan`、`white`。`g i` 显示聊天 ID 和当前文件夹 ID。

应用内覆盖保存在 `settings.conf` 同目录的 `appearance.json`，按真实 Telegram
账号 ID 隔离。切换账号或清理消息缓存不会混用或删除配色。保存复用现有原子写入
逻辑；格式错误会明确报错，不会悄悄覆盖损坏的文件。调色板不会改写 Lua 配置或
服务端文件夹颜色。

## Nerd Font 图标

Nerd Font 支持需要主动开启。安装 [Nerd Font 字体](https://www.nerdfonts.com/font-downloads)，
在终端配置中选择其 **Nerd Font Mono** 变体，再在 `config.lua` 中添加配置并重启 Termgram：

```lua
return {
  nerd_font = true,
}
```

使用 v3+ 字体。图标用于区分聊天类型、文件夹、Archive、置顶和附件，文字标签、按键提示
与终端配色仍会保留。Mono 变体让图标位于单个终端单元格内。通过 SSH 或 tmux 使用时，
需要在实际显示会话的本地终端设置字体。Termgram 使用终端选定的字体，不会安装字体或
修改终端偏好。

默认 `nerd_font = false` 使用普通文字显示，置顶标记为 `^`。如果图标显示为方框或与邻近
文字重叠，将此选项关闭即可恢复。配置错误通过现有 Lua 加载流程显示。

字体参考：[Nerd Fonts 字体变体](https://github.com/ryanoasis/nerd-fonts/wiki/FAQ-and-Troubleshooting)。

## 聊天列表列布局

标题使用聊天配置的颜色；时间默认青色、未读数默认黄色，独立右对齐，不随标题长度改变。
指示符和选中标题的下划线标明焦点，不用整行反色覆盖颜色。Nerd Font 聊天/置顶图标
放在标题列，中文和 emoji 名称按终端列宽截断。紧凑侧栏默认 30 列，F4 切换显示。
宽度和语义颜色选项见 [Lua 配置](Configuration.md)。

## 会话排版

每条消息单独显示作者行，本地时间、发出消息的送达状态与可选消息 ID 靠右对齐。
收到的消息按作者分配稳定的终端调色板颜色，发出消息作者为绿色。正文使用终端默认
前景与背景。较早日期显示月日，跨年时补充年份。标题很窄时，可选 ID 优先让位给
作者、送达状态和时间。

回复引用独占一行，位于正文/媒体上方。左侧细标记表示消息选择，当前回复、附件或
链接操作另有强调。正文、操作项和内嵌媒体共享两列的小缩进，消息间留一行空白。
附件标签只描述文件与当前传输状态，选择后的快捷键放在底栏。`o` 放大选中媒体，
`i` 回复，`g n` / `g p` 切换操作项。

调整窗口或切换侧栏会保留锚定的消息。如果重排后原物理行已不存在，视图回到该消息
作者行，不会落在末尾分隔空行上。这里保留消息/行锚点，不承诺不同折行宽度下的
精确文字字符位置。
