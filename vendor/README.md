# Upstream code

Yazi is pinned to `9203fd2604f867ab5ec18f24203b918975c4c00a` from
https://github.com/sxyazi/yazi. All Yazi crates except `yazi-term` are Git
dependencies at that revision.

`yazi-term/` copies the complete upstream `yazi-term/src` and its MIT license.
Its standalone manifest resolves upstream workspace dependencies explicitly.
The only source changes expose mouse coordinates/modifiers, key state, and
`KeyEvent::new` to Rust consumers; upstream exposes some of these only to Lua.
Parsing, platform input, timeouts, and restoration code are unchanged.

When updating, replace from one upstream revision, reapply these visibility
changes, update all Yazi Git revisions together, and review the integration in
`src/terminal.rs`. Keep vendored formatting and do not run Termgram's lint or
formatting policy over upstream source.

The integration in `src/terminal.rs` adapts the mode setup/cleanup sequences from
`yazi-tui/src/raterm.rs` and the two-stage probe handling from
`yazi-actor/src/app/report.rs` and `app/passthrough.rs` at the same revision.
These are covered by `licenses/Yazi-MIT.txt`. Termgram makes tmux option changes
explicitly opt-in and removes the actor/proxy dispatch layer. The upstream
`yazi-emulator`, `yazi-tty`, parser, reader, and platform restorer remain in use.

`yazi-term/LICENSE` retains the separate Michael Davis and sxyazi attribution.

`yazi-image/icc.rs` copies `yazi-adapter/src/icc.rs` unchanged from the same
Yazi revision, under `licenses/Yazi-MIT.txt`. This private upstream helper retains
ICC-to-sRGB conversion while `ratatui-image` owns multi-image encoding and clipped
rendering. `src/media.rs` also follows Yazi's EXIF orientation handling and Kitty
cleanup sequence. Protocol selection still uses the original `Drivers::matches`;
old Kitty-only implementations and external-overlay drivers fall back to the
library's Unicode half-blocks. The former single-surface adapter initialization
and embedded Yazi presets are no longer needed.

`grammers-session/` vendors crates.io 0.10.0 from
https://codeberg.org/Lonami/grammers at
`5c6d44ff30e02d6c9295bcf1fcb51403ad77c981` (`grammers-session/`).
The original source, tests, MIT and Apache licenses are retained. A narrow patch
in `src/storages/sqlite.rs` sets a five-second SQLite busy timeout, uses immediate
write transactions, holds the connection mutex through home-DC transactions,
and creates the schema and its version in one serialized transaction. The
session format is unchanged. SQLite's existing rollback journal is retained;
WAL is not necessary for multi-process operation, and the bundled libSQL SQLite
engine predates upstream's WAL-reset fix. Reapply and review these changes when
upgrading the session crate. `tests/session_concurrency.rs` validates independent
processes opening and writing one session, including initial schema creation.

`src/message_box/mod.rs` also follows Telegram's documented startup recovery:
only common/secondary differences start immediately; channel differences are
requested when Telegram reports a gap, including `updateChannelTooLong`.
That notification preserves an existing local channel PTS instead of replacing
it with the remote PTS before recovery. The upstream connection-flow assertion
is adjusted; `tests/update_recovery.rs` checks both integration boundaries.
See https://core.telegram.org/api/updates#recovering-gaps.

`grammers-client/` vendors crates.io 0.10.0 from the same Grammers revision
(`grammers-client/`). Original source, examples, tests and both licenses are
retained. Only `src/client/updates.rs` is adapted:

- `next_batch` drains a complete update batch and returns its covered cursor;
- `restore_state` loads the application's durable cursor before polling;
- too-long differences emit a raw `PtsChanged` / `ChannelTooLong` invalidation
  before replacement messages, allowing the local store to discard stale data.

`src/telegram/local.rs` persists events before committing their cursor and reads
that cursor on restart. Telegram's session peer cache remains separate: dialog
iteration may advance its PTS without having cached any message content.

Active-group polling uses the same message-box engine. `set_active_channel`
seeds only unknown PTS, recovers the viewed group immediately, and schedules its
next final difference using the server timeout (one second when absent). Leaving
the group restores the normal recovery deadline. A watch channel wakes the
existing update receive without cancellation or a second update consumer.
The deadline helper also accounts for deadlines moved earlier than the current
minimum. `tests/update_recovery.rs` covers activation, timeout and deactivation.

`libsql/` vendors the published libSQL 0.9.30 Rust crate from
https://github.com/tursodatabase/libsql at
`0653c5788d77ef16a97c56ff3e9fdc11717a72d9` (`libsql/`), with the upstream
repository's `LICENSE.md`. The published standalone manifest, lockfile and original
source, examples and tests are retained; Cargo download metadata is omitted. An
empty workspace table allows the upstream tests to run independently of Termgram.
The root lockfile resolves the application dependency.

The only source changes backport upstream PR #2282 at
`0070ff3331cd6d09425b812e1cd3ebe32e1d4206`, including its regression test:
https://github.com/tursodatabase/libsql/pull/2282.
`src/local/connection.rs` makes disconnect idempotent, and `src/local/impls.rs`
removes the redundant connection destructor. This prevents a second
`sqlite3_close_v2` call on freed memory, reported in upstream issue #2251 and
consistent with Termgram's Windows ARM64 session-writer access violations.
The native SQLite engine and session format are unchanged. Directly using that
upstream Git revision would also require its 0.10.0-pre.4 engine upgrade; this
backport keeps the fix bounded to connection ownership. Remove this patch when
a stable release includes the fix. Keep upstream formatting and use its focused
`drop_closes_the_handle_exactly_once` test to validate the ownership boundary.

```sh
cargo test --locked --manifest-path vendor/libsql/Cargo.toml \
  --no-default-features --features core --lib \
  drop_closes_the_handle_exactly_once --target-dir target/libsql-regression
```
