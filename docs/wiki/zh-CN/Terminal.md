# 终端集成

[English](../en/Terminal.md) · [指南首页](Home.md)

固定版本的 Yazi 终端库负责输入解析、能力探测、TTY 输出和模式恢复。Ratatui 使用同一个
输出通道；Crossterm 仅作为输出后端，不另起输入读取器。粘贴、焦点、鼠标、窗口大小、
增强按键报告和关联 Unicode 文字进入同一个应用事件流。

Shift-Enter 需要终端能够单独报告，不能区分时用 Ctrl-J 换行。功能键或 Alt 组合也可能
被终端或桌面拦截，可在 [Lua](Configuration.md) 中换键。显示损坏时可用 Ctrl-L 重绘。

## 图片

Yazi 选择图形协议，`ratatui-image` 负责多个内联图片及滚动时的裁剪。
Ghostty/Kitty 可以使用 Kitty 图形协议，iTerm2/WezTerm 可以使用内联图片，检测到支持时
使用 Sixel；其他终端回退到 Unicode 半块字符，不需要额外图片覆盖进程。
实际选择取决于终端能力报告和中间的复用器。

解码保留上游 ICC 到 sRGB 转换并处理 EXIF 方向；解码和编码不在 UI 线程中进行。
只保留可见图片的编码缓存，调整窗口或打开浮层时清理对应图像区域。
动态贴纸使用静态缩略图。下载与重试说明见[附件](Attachments.md)。

## tmux

默认使用 tmux 自身报告的能力。如果需要 Yazi 第二阶段探测，必须在启动前设置到
**shell 环境**：

```sh
TERMGRAM_TMUX_PASSTHROUGH=1 tg
```

这会调用上游 tmux 设置，把窗格的 `allow-passthrough` 设为 `all`，服务器的
`input-buffer-size` 设为 `104857600`。退出 Termgram 后这些设置仍保留。
读取该选项时，Telegram 凭据 `.env` 尚未加载。

正常退出、初始化失败和 panic hook 都会恢复终端模式。文本与图形清理由单一所有者协调。
上游版本、许可证与局部修改记录在 [vendor 说明](../../../vendor/README.md)。
