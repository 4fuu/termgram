# Daily workflows

[简体中文](../zh-CN/UX.md) · [Guide](Home.md)

## Read and navigate

The chat list and conversation share a wide window. Narrow windows show one
pane at a time. A focus border and selection marker show where keys will act;
Tab switches focus. Esc clears an explicit message selection before returning
from a conversation to the list.
Use `j/k` to select messages, arrows to scroll their rendered rows, and `G` to
return to the latest page. While you read earlier messages, new arrivals retain
your reading position and show a count instead of pulling the view down.

`[`/`]` in the list switch [folders](Folders.md). `/` filters titles in that
folder. A [configured chat alias](Configuration.md) opens a stable Telegram ID
from All chats. `g i` shows the selected chat and folder IDs for configuration.

Cached content appears first. The connection state and loading indicator describe
background reconciliation. Cached search and local preferences work without a
connection; sending and server operations need an authenticated connection.
The outgoing queue is bounded, and a failed send stays visible for retry.

## Compose and reply

Click the composer or press `i` to write. From the chat list, `i` continues the
current conversation, or restores the last chat for this Telegram account after
restart. If it is unavailable, the highlighted chat is used; an empty list shows
a synchronization hint. The composer shows a configurable ghost hint when empty,
without a second permanent row repeating the send key. Enter sends; Shift-Enter
or Ctrl-J inserts a newline. Bot commands are typed here, including their `/`.
Each open chat has its own in-memory draft. Esc preserves it; drafts are not
persisted across process restarts or account switches.

Select a message and press `i` or `R` to reply, or use `R` with no selection for the
latest message. Clicking the composer preserves an existing reply draft without
creating a reply from the message selection. The composer shows the target. Esc cancels the reply first,
keeping text; another Esc returns to navigation. `r` opens a selected message's
reply target. Failed sends keep their content and reply target; activating a
failed outgoing message returns it to the composer for retry.

## Files, links and mouse input

Drop file paths from your desktop into an open conversation to upload them.
Composer text becomes the first file's caption and retains the reply target.
JPG/JPEG/PNG/WebP inputs are uploaded as Telegram photos; other files are sent
as documents. Photos, image documents and stickers preview inline. `O` reveals
the original in the system file manager; see [Attachments](Attachments.md).

Click media to select it, then `o` for a larger preview or `i` to reply. Use
`g n` / `g p` to select actionable entries, then Enter to activate. Telegram
URL entities appear as selectable link rows, including hidden-text links.
Public `t.me`, `telegram.me`, and `tg://resolve` chat/message links open in-app;
private `t.me/c` and `tg://privatepost` links work for known groups. Other HTTP(S)
links open through the operating system. Invite and broadcast-channel links are
outside scope. URL, web-view, callback and game bot buttons are supported;
payment, password-gated, contact/location and peer-selection buttons identify
that a graphical client is required.

Mouse support is optional: click a chat to open it, click a message or action to
select it, right-click a message to reply, and scroll the pane under the pointer.
Clicking the chat list or timeline leaves input mode and keeps the draft. Settings/account rows are also
clickable. Keyboard equivalents remain available.

## Settings and accounts

`s` opens automatic update checks, release channel, Enter download behavior,
and the message-ID column. `c` and `C` open [color pickers](Appearance.md).
These persist without rewriting Lua. Esc dismisses an overlay, returning focus
to the previous view. `?` shows effective bindings instead of a static cheat sheet.

`a` opens the account picker; select an account or its add row and press Enter.
F2 cycles existing slots and F3 adds a slot, up to eight. Only the active account
has a running network worker. Sessions, messages, media, and color overrides are
isolated; switching clears in-memory drafts and searches from the previous account.
