# Termgram guide

[简体中文](../zh-CN/Home.md) · [Language selection](../Home.md)

Termgram uses Telegram's MTProto user API to access your direct messages and
groups. It supports up to eight account slots, with one account connected at a
time. Cached messages are available while it reconnects.

| I want to… | Read |
| --- | --- |
| Install, sign in, or build from source | [Get started](Getting-Started.md) |
| Learn the keys or migrate older bindings | [Keybindings](Keybindings.md) |
| Remap keys, jump to a saved chat, change ghost text | [Lua configuration](Configuration.md) |
| Read, reply, send files, and switch accounts | [Daily workflows](UX.md) |
| Navigate Telegram folders | [Folders](Folders.md) |
| Find text in cached messages | [Local regex search](Search.md) |
| Color a chat or folder | [Appearance](Appearance.md) |
| Preview or reveal a file | [Attachments](Attachments.md) |
| Understand offline behavior or clear the cache | [Cache and synchronization](Synchronization.md) |
| Troubleshoot graphics, keys, or tmux | [Terminal integration](Terminal.md) |
| Update the app or understand releases | [Updates](Updates.md) |
| Contribute and maintain the implementation | [Development](Development.md) |

These pages describe their source revision; check the release you installed
with `tg --version`. The app's `?` help reflects your actual Lua bindings.
Documentation is available in English and Simplified Chinese; the interface
currently uses English text, with configurable composer ghost text.

Broadcast channels, secret chats, calls, stories, reactions, sticker/GIF pickers,
server-wide message search, contact management, chat creation, group
administration, polls, notifications, and outgoing edits/deletes/forwarding are
outside the current scope. Incoming edits and deletions are synchronized.
Folders are created and edited in an official client. Supported messages, links
and bot buttons are described in [daily workflows](UX.md).
