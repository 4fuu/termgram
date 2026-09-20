# Message reactions

[简体中文](../zh-CN/Reactions.md) · [Guide](Home.md)

Reactions appear in a compact row under their message, with emoji and counts.
Your choices have a `●` marker and an accent color. The bottom bar includes the
reaction total and the keys for the selected action.

Select a delivered message, then press `g e` or run `:react`. Existing reaction
rows are also selectable with the mouse; Enter opens their picker. The picker
loads the emoji that Telegram permits in this chat.

| In the reaction picker | Action |
| --- | --- |
| j/k or Down/Up | Select a reaction |
| PageUp/PageDown, Home/End | Move through the choices |
| Mouse click / wheel | Select a row / move selection |
| Enter or Space | Add the selected emoji, or remove it if already yours |
| u | Remove all your ordinary and custom emoji reactions from this message |
| Ctrl-R | Refresh counts, your choices and chat permissions |
| Esc or q | Close |

A mouse click selects without sending. Enter and Space submit the change.
Only one change can be pending at once. Closing a pending request leaves it
running; failures still appear in the bottom bar. After an error or uncertain
result, refresh before retrying. Hold/repeat events cannot repeatedly submit.

Each change re-fetches the exact message and Telegram's available reactions and
limits. Existing choices keep their order. When adding beyond your personal
limit, the oldest choice is replaced, matching Telegram's behavior. If your
choices changed in another client, the operation asks you to refresh instead of
overwriting that change. Unavailable emoji and the chat's unique-reaction limit
are enforced, and Telegram remains authoritative for permissions.

Counts are cached for offline reading. Live updates apply to the conversation,
search results, pins and reply previews; minimal updates preserve your choices.
Visible incoming messages with reactions additionally refresh in batches, at
most once every 30 seconds for an unchanged view. Only one batch can be pending;
hidden or unfocused views do not schedule new requests.

This revision adds and removes ordinary emoji. Existing custom emoji counts use
a text fallback, and their identities are preserved when changing other choices.
It does not offer custom emoji selection, animations, reactor lists, paid
reactions or Saved Messages tag editing. Star counters can be read; no payment
request is sent. Saved Messages tags show a read-only explanation.

The entry action is `reactions` in `conversation`; picker keys use the
`reactions` context. Its actions are `open`, `clear_reactions`, `refresh`, `up`,
`down`, `page_up`, `page_down`, `home`, `end`, and `cancel`.
