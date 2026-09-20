# Telegram folders

[简体中文](../zh-CN/Folders.md) · [Guide](Home.md)

Termgram reads the folders already configured in Telegram, including shared
folders. Their order, pinned chats and inclusion/exclusion rules are synchronized
and cached per account. Use Telegram Desktop or another official client to create
or edit them; changes appear in Termgram automatically.

In the chat list, `[` and `]` select the previous/next folder. The list border shows
the folder name and position. These keys are configurable as `folder_previous`
and `folder_next` in the `chats` context. `/` filters the selected folder.
Switching folders leaves the open conversation, reading position and draft intact.
A configured `jump alias` opens its destination from All chats or Archive, so it
also works when the destination is outside the selected folder. See [configuration](Configuration.md).

Explicit exclusions take priority, followed by explicit inclusions and pins.
Dynamic rules support contacts, non-contacts, bots, groups, unread, muted and
archived conversations. Bots are a distinct category. Muted chats with unread
mentions remain eligible unless archived, matching Telegram Desktop. Notification
defaults are inherited when the chat has no explicit mute override. Other clients'
read notifications update unread-folder membership. `Ctrl-R` refreshes folders
and conversations if a retry is needed.

Termgram currently exposes people and groups. Broadcast channels remain outside
its messaging scope, including when they appear in an upstream folder.

## Archive

Archive is a separate folder (ID `1`); All chats (ID `0`) excludes archived chats.
Select a chat and press `e` (`archive`) to archive it, or restore it from Archive.
Custom Telegram folders still follow their own inclusion and `exclude_archived`
rules. Changes from another client update membership automatically.

## Pinned chats

In the chat list, `p` (`pin`) toggles the selected chat's server-side pin.
`Ctrl-K` / `Ctrl-J` (`pin_up` / `pin_down`) move a pinned chat up/down. Main,
Archive, and each custom folder have independent server orders. A `^` marker
identifies pins; newer messages do not move other chats ahead of them. Selection
stays on the same chat after reordering, and cached pin order survives restart.

Unpinning in a custom folder keeps that chat explicitly included, as in Telegram
Desktop. Folder updates preserve server titles, colors, shared-folder settings
and filter rules. Pin/Archive mutations require a connection and wait for server
confirmation; server limits and permission failures are reported without a local
success indicator. Local appearance settings do not modify Telegram folders.

Implementation reference: [Telegram dialog filters](https://core.telegram.org/api/folders).
