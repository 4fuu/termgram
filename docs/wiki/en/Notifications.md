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

## Unread mentions and replies to you

Press `g m` from a chat list or conversation, or use `:mentions`. The **Unread
mentions** panel uses Telegram's [unread-mention list](https://core.telegram.org/method/messages.getUnreadMentions),
including replies to your messages. A colored `@` in the sidebar identifies chats
with unread mentions; selecting a mentioned message also shows its state in the
bottom statusline. These indicators remain recognizable without color or a Nerd Font.

Use Up/Down to select, Enter to load and select the original with surrounding
history, Ctrl-N/Ctrl-P to page, Ctrl-R to refresh, and Esc to close. `/` returns
to the retained results; `:search` switches to local regex. The panel captures
the chat you started from and preserves drafts. Its total is Telegram's count
at search time; refresh after changes on another device. Read or deleted rows
are removed as live updates arrive. Search previews do not mark mentions read.
There is no query field in this panel. The `mentions` action is configurable in
`chats` and `conversation`; panel controls use the `search` context.

Viewing a text mention in a focused conversation acknowledges its content only
after the message end was drawn. Loading history, searching, opening an overlay
or leaving the terminal unfocused does not acknowledge it. Content receipts and
ordinary inbox read boundaries are separate. Voice, round-video and disappearing
media mentions require actual consumption; displaying their message does not
mark them as played. Until Termgram's playback flow supports those receipts,
consume them in another Telegram client. `:read` marks history read but does not
pretend to play this media.

Acknowledgements received from other clients update the timeline, retained
results and local cache. Slow history/search snapshots cannot restore an older
unread flag. Mention navigation requires a connection; Ctrl-R retries after
reconnecting.

Native desktop alert delivery is being implemented separately from Telegram's
mute settings and mention receipts.
