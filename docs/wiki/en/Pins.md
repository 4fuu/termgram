# Pinned chats and messages

[简体中文](../zh-CN/Pins.md) · [Guide](Home.md)

## Chat lists

Select a chat and press `p` to pin/unpin it in the current folder. `Ctrl-K` and
`Ctrl-J` move it within that folder's pinned order. Main, Archive, and custom
folders have independent Telegram orders. Pins remain above ordinary chats
when messages arrive, and restart uses the last cached order. See [Folders](Folders.md)
for archive behavior and custom-folder inclusion rules.

## Conversation pins

Select a sent message with `j/k`, then press `p`. A confirmation overlay offers
the applicable Telegram options. In ordinary private chats, the default is
**Pin only for me**; the second option pins for both participants. In groups,
a new pin defaults to notifying members, with a silent option. Pinning a message
older than the known top pin is silent, following Desktop's older-message prompt.
Saved Messages does not need a participant option. Arrow keys select; Enter
confirms; Esc dismisses. Changes appear after Telegram accepts them. Missing
permissions or server errors remain visible and do not report success.

Press `p` on a pinned message to confirm unpinning. This uses Telegram's normal
unpin operation. The confirmation identifies the message before any change.

The conversation shows a compact pinned-message banner when a pin is available.
Press `P` to browse all pins, newest message first. `j/k` select, `Ctrl-N` / `Ctrl-P`
page, `Enter` opens the message and older context, and `Esc` returns. `p` unpins
the selection; `U` confirms unpinning all messages. `Ctrl-R` retries a refresh.
All bindings are configurable; the list uses the `pins` Lua context, and its
confirmation uses `overlay`.

Opening a pin keeps the draft and detaches the reading position without marking
the entire conversation read. `G` returns to the latest history. Other clients'
pin/unpin and deletion updates refresh the banner/list. Pins and message previews
are fetched in bounded background requests, so history loading and live updates
continue while the list synchronizes.

If Telegram requires a fresh synchronization, the open pin list reloads and
outdated message confirmations close. Drafts are kept.

Cached pins can be browsed before connecting. The list labels cached coverage;
it may be incomplete because message retention is bounded. Server pages replace
that view once connected. Mutating pins requires a connection.

Protocol references: [Telegram message pins](https://core.telegram.org/api/pin),
[pin options](https://core.telegram.org/method/messages.updatePinnedMessage).
Desktop behavior reference: [pin message dialogs](https://github.com/telegramdesktop/tdesktop/blob/4d4da471fbee771c10e173a83c003ba1728989f1/Telegram/SourceFiles/boxes/pin_messages_box.cpp).
