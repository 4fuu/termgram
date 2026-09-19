# Telegram folders

[简体中文](../zh-CN/Folders.md)

Termgram reads the folders already configured in Telegram, including shared
folders. Their order, pinned chats and inclusion/exclusion rules are synchronized
and cached per account. Use Telegram Desktop or another official client to create
or edit them; changes appear in Termgram automatically.

In the chat list, `[` and `]` select the previous/next folder. The list border shows
the folder name and position. These keys are configurable as `folder_previous`
and `folder_next` in the `chats` context. `/` filters the selected folder.
Switching folders leaves the open conversation, reading position and draft intact.
A configured `jump alias` opens its destination from All chats, so it also works
when the destination is outside the selected folder. See [configuration](Configuration.md).

Explicit exclusions take priority, followed by explicit inclusions and pins.
Dynamic rules support contacts, non-contacts, bots, groups, unread, muted and
archived conversations. Bots are a distinct category. Muted chats with unread
mentions remain eligible unless archived, matching Telegram Desktop. Notification
defaults are inherited when the chat has no explicit mute override. Other clients'
read notifications update unread-folder membership. `Ctrl-R` refreshes folders
and conversations if a retry is needed.

Termgram currently exposes people and groups. Broadcast channels remain outside
its messaging scope, including when they appear in an upstream folder. All chats
includes the cached archived conversations; archive exclusion follows each
folder's own rule. Local appearance settings do not modify Telegram folders.

Implementation reference: [Telegram dialog filters](https://core.telegram.org/api/folders).
