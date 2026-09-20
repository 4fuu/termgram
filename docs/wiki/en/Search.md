# Search messages

[简体中文](../zh-CN/Search.md) · [Guide](Home.md)

By default, `/` in a conversation or `Ctrl-F` in the chat list opens local search.
`:search [regex]` explicitly selects local search. Type a regular expression and
press Enter. Tab cycles through the open chat, selected folder,
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

Use `:search --cloud` in an open chat for Telegram's server search. The same panel
shows **Telegram search**, the captured chat, active filters and Telegram's result
count. Text follows Telegram's word search, not Rust regex syntax. This mode
requires a connection and searches only that chat; Tab does not change its scope.
Press Esc and use `:search --cloud ...` again to change filters or the target chat.
`/` returns to retained results, including cloud results; `:search` returns to local
regex search.

Examples:

```text
:search --cloud release notes
:search --cloud --from me --media file
:search --cloud --from @alice --after 2026-09-01 --before 2026-10-01 release
:search --cloud --media photo
:search --cloud -- --literal-leading-dash
```

Put filter flags before the text. `--from` accepts `me`, `@username`, or a known
Telegram peer ID. A username resolves once per search so subsequent pages keep
the same sender identity. `--media` accepts `all`, `photo`, `video`, `file`, `music`,
`voice`, `round` (round video), `gif`, `link`, and `poll`. Dates use `YYYY-MM-DD` at
midnight **UTC**: `--after` is strictly later than that instant, and `--before`
strictly earlier. Both must fit Telegram's 32-bit timestamp range; the UI validates
1970-01-02 through 2038-01-19. These filters use
[Telegram's messages.search API](https://core.telegram.org/method/messages.search).
Only cloud arguments use the existing Yazi platform quoting rules (Unix shell
quotes on Unix, Windows command-line quotes on Windows); no shell is executed.
Unprefixed local regex arguments retain their backslashes and trailing spaces.

Cloud pages contain at most 100 results, with bounded lookahead to identify the
next page. A filter-only command opens the text field; press Enter to search
without text, or enter optional words. If Telegram rejects a query, the panel
keeps the query editable. Enter on a result fetches the original and up to 79
preceding messages, then selects the original without marking unseen history read.
`G` returns to latest messages. Draft text and attachments stay intact.

Esc or a new search cancels the previous cloud task; a late result cannot replace
the new query. Cloud results are reconciled with incoming edits and deletions
before display and persistence. They populate the same bounded cache without
advancing its sync cursor. Existing results reflect live edits/deletions; rerun
search to refresh matching membership and server counts. Counts and pages are not
a frozen snapshot when chat history changes. A history resync invalidates old
results and asks you to search again. Cache limits still apply; cloud search does
not download the entire chat.

All controls are configurable in the `search` context. Actions are `open`,
`cancel`, `up`, `down`, `page_up`, `page_down`, `search_scope`, `search_query`,
`search_more`, and `search_previous`.

Ctrl-R (`refresh`) reruns the search from the first page. `g m` / `:mentions` uses
the same paging and original-message navigation for [unread mentions](Notifications.md).
