# Chat discovery and details

[简体中文](../zh-CN/Chats.md) · [Guide](Home.md)

## Open a conversation

Use `:chat` and Tab to choose a cached chat by title, configured alias or ID.
Use `:open @alice_name` to look up a username on Telegram, then `i` to compose.
`:open https://t.me/example/123` also opens a linked message. Existing drafts,
unread counters and folder membership are preserved.

Remote resolution starts only after Enter and has a timeout; typing never sends
lookup requests. Starting to compose or opening another chat cancels the pending
navigation, and superseded results cannot change your view. Cached chats can be
opened offline. Broadcast channels remain unsupported in this revision.

## Preview and join an invitation

`:join https://t.me/+hash` previews a group invitation; `t.me/joinchat/hash` and
`tg://join?invite=hash` work too. Opening an invite with `:open` or activating its
message link shows the same preview. It shows the title, available description,
member count and Telegram scam/fake warning.

Cancel is selected initially: use Down to select Join/Request to join, then
Enter to confirm. A mouse click selects an option; Enter confirms. Already joined
groups offer Open chat. The server is checked again before joining.

Admin approval remains “request submitted” until approved. Closing during
submission keeps the request running and reports its result without changing
your view. If confirmation fails or times out, reopen the invitation to check
current membership before trying again. Expired/revoked links report Telegram's
error; paid subscriptions, broadcast channels and bot verification require the
official client. Invite hashes are excluded from request logs and are not written
to preferences or disk command history.

## Inspect details and sending restrictions

`:info` opens details for the selected chat: full name, username, description,
member count when available, your role, notification mute, text/photo/file
permissions and slow mode. Scroll with j/k or the mouse wheel; Ctrl-R refreshes;
Esc closes. Long names and descriptions wrap.

The active conversation refreshes in the background at most once a minute, with
one details request at a time. Permission updates invalidate older results.
Offline details show their age. Known restrictions appear in the composer ghost
text and statusline. Sending while restricted keeps the text, reply and
attachments in the draft. Slow mode shows a countdown and allows one pending
message or attachment at a time.

Server rejections explain permission or wait errors and preserve retry content;
Telegram still checks permission at send time. This viewer does not modify roles,
ban users, or pay for messages. [Notification mute](Notifications.md) is separate
from write restrictions. Use [Commands](Commands.md) for command completion and
[Keybindings](Keybindings.md) for navigation.
