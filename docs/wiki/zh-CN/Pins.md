# 聊天与消息置顶

[English](../en/Pins.md) · [指南首页](Home.md)

## 聊天列表

选中聊天后按 `p`，在当前文件夹内置顶或取消置顶；`Ctrl-K` / `Ctrl-J` 调整置顶顺序。
主列表、Archive 和自定义文件夹使用独立的 Telegram 服务端顺序。其他聊天收到新消息
不会超过置顶项；重启后先读取缓存的顺序。归档和文件夹包含规则见[文件夹](Folders.md)。

## 会话内置顶

用 `j/k` 选中已发送的消息，再按 `p`。确认浮层按聊天类型提供 Telegram 对应选项：
普通私聊默认“仅为自己置顶”，另一选项为双方置顶；群内新的置顶默认通知成员，
也可以静默置顶。消息早于已知的最新置顶时，按 Desktop 的旧消息提示静默置顶。
Saved Messages 不需要选择另一参与者。用方向键选择、Enter 确认、Esc 关闭。
服务端接受后才更新状态；权限不足或服务端错误会明确显示。

在已置顶消息上按 `p` 可确认取消置顶，使用 Telegram 的常规取消置顶操作。
确认浮层会先展示消息信息。

有置顶消息时，会话顶部显示一行紧凑提示。按 `P` 打开所有置顶消息，按消息从新到旧
排列。`j/k` 选择，`Ctrl-N` / `Ctrl-P` 翻页，Enter 打开该消息及较早上下文，Esc 返回。
`p` 取消所选消息置顶，`U` 确认取消全部置顶，`Ctrl-R` 刷新或重试。
所有按键均可配置：列表使用 Lua `pins` 上下文，确认浮层使用 `overlay`。

打开置顶消息会保留草稿和阅读位置，不会把整个会话标为已读；`G` 回到最新历史。
其他客户端的置顶、取消置顶、删除操作会更新列表与顶部提示。后台请求限制并发与
每页大小，浏览置顶时仍继续加载历史和接收实时更新。

联网前也可以浏览已缓存的置顶消息，界面会标明缓存范围；消息缓存有保留上限，
因此缓存列表可能不完整。联网后用服务端分页结果更新。修改置顶需要联网。

协议参考：[Telegram 消息置顶](https://core.telegram.org/api/pin)、
[置顶选项](https://core.telegram.org/method/messages.updatePinnedMessage)。
Desktop 行为参考：[置顶提示框](https://github.com/telegramdesktop/tdesktop/blob/4d4da471fbee771c10e173a83c003ba1728989f1/Telegram/SourceFiles/boxes/pin_messages_box.cpp)。
