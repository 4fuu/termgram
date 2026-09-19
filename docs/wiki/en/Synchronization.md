# Cache and synchronization

[简体中文](../zh-CN/Synchronization.md) · [Guide](Home.md)

Termgram displays cached conversations before connecting to Telegram. Opening a
cached conversation reads its most recent local page while a network request
refreshes it. Messages remain visible during synchronization. Sending and server
operations wait for sign-in to finish.

Updates continue while history, dialog and other requests run in the background.
The currently viewed group also receives periodic difference checks using
Telegram's requested interval. Leaving the conversation or unfocusing the
terminal stops active polling. Ordinary push updates and gap recovery continue.

Each session has a separate `*.cache.sqlite3` database beside its session file.
The cache contains message text, chat metadata and downloaded attachment paths.
It does not contain authentication keys or user preferences. On Unix the file is
restricted to its owner. The database is not encrypted.

The cache contains messages Termgram has received or loaded, not the account's
entire Telegram history. Up to 100,000 message records are retained per account;
older records are discarded. An old gap that Telegram can no longer replay
invalidates the affected local history, which is fetched again when viewed.

To clear the cache, exit Termgram and remove that session's `.cache.sqlite3`
file. Media can be cleared separately by removing its `.media` directory. Keep
the `.session` file, `settings.conf` and `appearance.json` to preserve login and
preferences. Only one process may own a given account cache at a time.
The next launch reconnects and starts a fresh message cache.

Full update batches and their covered cursor are committed in order. Restarting
after an interrupted write may replay updates, but must never skip uncommitted
messages. Session peer metadata is separate from this message checkpoint.

See [Telegram's update protocol](https://core.telegram.org/api/updates) for the
rules used by the underlying Grammers update engine. No end-to-end latency
comparison against Telegram Desktop has been measured yet.
