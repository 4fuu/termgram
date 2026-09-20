# Notifications and mute settings

[简体中文](../zh-CN/Notifications.md) · [Guide](Home.md)

Use `:mute`, `:mute 1h`, `:mute 8h`, `:mute 2d` or `:mute forever` on a chat.
The default is **forever**. `:unmute` explicitly enables this chat's notifications,
including when the account's default for groups or users is muted. In the chat
list the command captures the selected chat; in a conversation it captures that
conversation. The command header identifies the target, and incoming messages
that reorder the list cannot change it.

These commands update [Telegram's notification settings](https://core.telegram.org/method/account.updateNotifySettings)
and therefore synchronize with official clients. They require a connection.
Termgram preserves the existing preview, silent-post and desktop sound preferences
while changing mute time. It also preserves the peer's existing story settings.
It waits for Telegram's actual state instead of changing the UI optimistically.
If another notification update arrives while a request runs, a fresh dialog
refresh settles the state; a late snapshot cannot replace that update. A failed
request leaves the last confirmed state visible and allows retry. Muting does
not mark messages read or change drafts.

Muted chats show `[m]` in the sidebar, or the bell-slash glyph when Nerd Font is
enabled. The configurable `notifications` statusline item displays **Muted forever**
or a local date/time deadline for the focused chat. It disappears when unmuted or
when a timed mute expires. The marker and deadline are independent of color.
Folders that exclude muted chats follow their existing Telegram membership rules.

The Lua actions `mute_chat` and `unmute_chat` can be bound in `chats` or
`conversation`; `mute_chat` means forever. They have no default keyboard binding:

```lua
keymap = {
  { context = "chats", on = { "m" }, run = "mute_chat" },
  { context = "chats", on = { "M" }, run = "unmute_chat" },
},
```

This page currently covers Telegram mute settings. Native desktop alert delivery
and unread mention navigation are documented here as they become available.
