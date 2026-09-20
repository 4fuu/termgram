# Daily workflows

[简体中文](../zh-CN/UX.md) · [Guide](Home.md)

## Read and navigate

The compact chat list and conversation share a wide window, with the composer
aligned below the conversation. F4 toggles the sidebar. Narrow windows show one
pane at a time; F4 or returning to Chats reveals the list and retains the draft. A focus border and selection marker show where keys will act;
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


The bottom statusline shows keyboard mode, account and connection information.
Selection changes its contextual hints; INSERT identifies the live composer.
Optional Ping/DC diagnostics and reading position are configured in
[Lua](Configuration.md). Long errors expand above the bar so their cause remains
readable without replacing the current mode.

Author, reply, body and media have separate visual rows. The current message
and its active action have distinct markers. Full author names wrap. Messages
have alternating backgrounds and no empty separator row by default; time, optional
IDs, delivery and media state are shown in the bottom bar.

## Compose and reply

Click the composer or press `i` to write. From the chat list, `i` continues the
current conversation, or restores the last chat for this Telegram account after
restart. If it is unavailable, the highlighted chat is used; an empty list shows
a synchronization hint. The composer shows a configurable ghost hint when empty,
without a second permanent row repeating the send key. Enter sends; Shift-Enter
or Ctrl-J inserts a newline. Bot commands are typed here, including their `/`.
Each chat has its own local draft, including its text, reply target and cursor.
Esc, account switches and normal restarts preserve it. Drafts are keyed by the
Telegram user identity, so reusing a local account slot does not expose another
account's draft. Background saves coalesce edits over 400 ms; normal exit flushes
the final edit. A forced kill or power loss can lose edits not yet committed.
These drafts are local to this installation; cloud draft synchronization is not
part of this revision.

Select a message and press `i` or `R` to reply, or use `R` with no selection for the
latest message. Clicking the composer preserves an existing reply draft without
creating a reply from the message selection. The composer shows the target. Esc cancels the reply first,
keeping text; another Esc returns to navigation. `r` opens a selected message's reply target. Clicking the indented quote also
jumps there; Enter opens the selected reply action. Excerpts load from memory,
then local cache, then a bounded background batch for visible missing targets.
An unavailable original is distinct from an excerpt still loading. Edits and
deletions update excerpts, including while a slower request is in flight. Failed sends keep their content and reply target; activating a
failed outgoing message returns it to the composer for retry.

## Files, links and mouse input

Use `:attach <paths...>` to prepare files in the chat draft. Plain pasted paths
remain text by default. Ctrl-O in the composer opens attachment review for format,
preview and removal; Enter in the composer explicitly sends, using its text and
reply target for the first file. See [Attachments](Attachments.md).

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
and message IDs in the bottom bar. `c` and `C` open [color pickers](Appearance.md).
These persist without rewriting Lua. Esc dismisses an overlay, returning focus
to the previous view. `?` shows effective bindings instead of a static cheat sheet.

`a` opens the account picker; select an account or its add row and press Enter.
F2 cycles existing slots and F3 adds a slot, up to eight. Only the active account
has a running network worker. Sessions, messages, media, and color overrides are
isolated. Switching resets searches and conversation views while retaining each
account's local drafts.
