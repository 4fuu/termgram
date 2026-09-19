# Search cached messages

[简体中文](../zh-CN/Search.md)

Press `/` in a conversation, or `Ctrl-F` in the chat list. Type a regular
expression and press Enter. Tab cycles through the open chat, selected folder,
and this account. Searching works offline and never requests Telegram history.
The header identifies the scope; the results report the cached message count and
date range. This is cache coverage, not a claim that the account's full history
has been downloaded. A range can contain gaps.

Results are newest first, with 100 hits per page. Up/Down selects a result;
PageUp/PageDown moves ten rows; Enter opens it with cached neighboring messages.
Ctrl-N/Ctrl-P changes result pages, Ctrl-F returns to the query, and Esc closes
search. Press `/` again from the conversation to return to the retained results.
`G` returns from an old result to the latest messages. Opening a historical result
does not itself mark the chat's unseen history as read.

Examples: `(?i)invoice` (case insensitive), `error.*timeout`, `^TODO`, or
`東京|上海`. Syntax follows the maintained Rust [regex crate](https://docs.rs/regex/latest/regex/#syntax).
Lookaround and backreferences are not supported. Invalid patterns display an
editable error; patterns are limited to 4096 bytes and compiled with memory limits.
Only message text is matched, including captions; attachment contents are not indexed.

Search runs on a dedicated blocking worker with one active scan and one replaceable
pending request. Esc, a new query, or switching accounts cancels it. Deletions
and edits already persisted in the cache are reflected in new searches. A hit is
revalidated before opening; deleted or evicted hits ask you to run the search again.

All controls are configurable in the `search` context. Actions are `open`,
`cancel`, `up`, `down`, `page_up`, `page_down`, `search_scope`, `search_query`,
`search_more`, and `search_previous`.
