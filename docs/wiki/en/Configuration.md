# Lua configuration

[简体中文](../zh-CN/Configuration.md) · [Guide](Home.md)

## Load configuration

Place `config.lua` beside Termgram's `settings.conf`, or set `TERMGRAM_CONFIG` to
an explicit file path. Restart to load changes. Start with [the example](../../../examples/config.lua).
The file returns a Lua table; tables, strings, mathematics and UTF-8 helpers are
available. Filesystem, process and plugin APIs are deliberately outside this
configuration interface. Invalid files show an error and retain default bindings.

```lua
return {
  chats = { work = -1001234567890 },
  ghost_text = "{send} to send · {newline} for a new line",
  nerd_font = false,
  keymap = {
    { context = "conversation", on = { "<C-u>" }, run = "message_up", count = 20 },
    { context = "conversation", on = { "g", "w" }, run = "jump work" },
    { context = "compose", on = { "<Enter>" }, run = "newline" },
    { context = "compose", on = { "<C-s>" }, run = "send" },
  },
}
```

Contexts are `global`, `chats`, `conversation`, `compose`, `input` (login and chat
filter), `overlay`, `preview`, `pins`, and `search`. A context takes precedence over global bindings. Rebinding
an existing key replaces that binding. Use `run = "noop"` to remove a binding.
A chord is a list of individual keys, such as `{ "g", "w" }`; ambiguous prefixes
are rejected. Chords expire after one second; Escape cancels a pending chord.
Key spelling follows Yazi: `<C-s>`, `<A-x>`, `<S-Enter>`, `<Tab>`, `<Esc>`.

Chat aliases use stable numeric Telegram IDs, including the `-100…` IDs of
supergroups. The destination must be present in the cached conversation list.
The optional `desc` field supplies the binding's label in help. Help always
reflects the effective bindings; use the arrow keys to scroll it.

Navigation defaults: `j/k` move between chats or actual messages; `20k` moves
20 messages up, fetching older pages if necessary. `G` or End returns to the
latest messages; `gg` or Home goes to the oldest message in the loaded window.
Arrow keys scroll rendered rows. PageUp/PageDown scroll ten rows. `i` starts
composing, `R` replies, `r` opens a reply target, `/` filters the chat list,
Tab switches panes, `s` opens settings, `a` opens accounts, and `?` opens help.
Counts and navigation chords never consume ordinary composer text.

Use `count = 20` with `message_up` for a single shortcut that moves up 20
messages. The optional count defaults to 1 and accepts 1–9999 for `up`, `down`,
`message_up`, `message_down`, `page_up`, and `page_down`. A typed prefix multiplies
the configured count, capped at 9999. Other actions do not accept a count.

Composer placeholders use the effective `{send}`, `{newline}` and `{cancel}`
bindings. Set `ghost_text = ""` to hide them. Managed in-app preferences remain
in their own file; Termgram does not rewrite your Lua configuration.

Press `g i` to show the current chat ID for an alias.

Set `nerd_font = true` when your terminal uses a Nerd Font Mono (v3+). It enables
chat, folder, archive, pin and attachment icons; `false` is the default. See
[Appearance](Appearance.md) for font selection and fallback behavior.

## Bottom statusline

The one-row bottom bar replaces the old application header. Configure its ordered
segments; omitted fields keep these defaults:

```lua
statusline = {
  enabled = true,
  left = { "mode", "app", "account", "context" },
  right = { "connection", "latency", "dc", "position" },
},
```

| Item | Meaning |
| --- | --- |
| `mode` | CHATS, NORMAL, SELECT, INSERT or the current overlay mode |
| `app` | Termgram |
| `account` | Local account slot and Telegram display name |
| `context` | Effective selection/help keys or an available update |
| `connection` | Connecting, online, reconnecting or offline |
| `latency` | Latest completed primary-connection Ping in milliseconds |
| `dc` | The authenticated session's home data center ID |
| `position` | Live edge, rows from the bottom or new arrivals while reading history |

Each item can appear once across both sides. Unknown or duplicate items reject
the configuration. Empty lists remove a side. At narrow widths, optional items
are removed before mode and contextual actions; their order within each side is
preserved. `enabled = false` hides the bar. Errors remain readable above it.

Latency is an optional diagnostic: at most one Ping runs every 60 seconds, with
a five-second display deadline, on the existing Telegram connection. A late
probe displays unavailable; another probe waits until the previous request
finishes, avoiding a backlog during outages. It includes SDK
queueing/retries and is **not** the time for a message to arrive. No measurement
runs during drawing. Removing `latency` from both sides, or disabling the bar,
disables these extra requests. A dash means unavailable; disconnects invalidate
measurements, samples older than 90 seconds are not displayed, and account
switches clear all observations. DC identifies the primary session, not every
server used for media transfers. The bar does not infer DC from geography.

## File locations

`config.lua`, `settings.conf`, `appearance.json` and `navigation.json` use the configuration directory.
`navigation.json` remembers the last opened chat by Telegram account ID; it stores no draft text.
The session, message database and media use the data directory. Defaults:

| OS | Configuration directory | Data directory |
| --- | --- | --- |
| Linux | `$XDG_CONFIG_HOME/termgram` or `~/.config/termgram` | `$XDG_DATA_HOME/termgram` or `~/.local/share/termgram` |
| macOS | `~/Library/Application Support/dev.termgram.Termgram` | Same |
| Windows | `%APPDATA%/termgram/Termgram/config` | `%LOCALAPPDATA%/termgram/Termgram/data` |

Account 1 uses `termgram.session`; other slots use
`accounts/<session filename>.account-N` beside it. Each gets its own appended
`.cache.sqlite3` database and `.media` directory. Session files contain account
credentials; cached messages and media are local plaintext. Cache cleanup is
described in [Synchronization](Synchronization.md).

| Environment variable | Purpose |
| --- | --- |
| `TELEGRAM_API_ID`, `TELEGRAM_API_HASH` | Source-build application credentials; environment or `.env` takes precedence over embedded values |
| `TERMGRAM_SESSION` | Override Account 1 session path; other slots derive from it |
| `TERMGRAM_CONFIG` | Override the Lua file; set in the shell before starting |
| `TERMGRAM_TMUX_PASSTHROUGH=1` | Opt into terminal passthrough setup; see [Terminal](Terminal.md) |

Lua is loaded before the credentials `.env`, so `TERMGRAM_CONFIG` must already
be in the process environment. A previous `TUIGRAM_SESSION` override and an
existing TUIGram default session remain recognized for login continuity.
Changing the session path does not move settings or colors.

Lua source is limited to 64 KiB, an 8 MiB VM budget, and approximately one million
instructions. Unknown configuration fields, actions, aliases and ambiguous
same-context prefixes are reported; the entire invalid configuration falls back
to defaults. There is no arbitrary plugin API or live reload.

## Action reference

Bind actions in an appropriate context. A custom global binding is a fallback;
a more specific binding or chord prefix takes precedence. Help labels can be
customized with `desc`. `noop` removes that context's key, so an existing global
binding can become visible again.

| Actions | Usual context and behavior |
| --- | --- |
| `quit`, `redraw`, `next_account`, `add_account` | Global lifecycle/account controls |
| `help`, `settings`, `accounts` | Navigation; open or toggle the overlay |
| `open`, `cancel`, `focus` | Contextual activation, dismissal, pane/QR switching |
| `up`, `down`, `page_up`, `page_down` | List selection, rendered-row scrolling, or search selection; accept `count` |
| `message_up`, `message_down` | Conversation message cursor; accept `count` |
| `oldest`, `latest` | First/last chat, or loaded-history start/latest conversation |
| `compose`, `send`, `newline` | Enter a draft or reply to the explicit conversation selection / send / newline |
| `preview` | Expand a selected image or sticker; `preview` context controls the expanded view |
| `home`, `end`, `left`, `right`, `backspace`, `delete`, `clear`, `delete_word` | Editors |
| `filter`, `refresh`, `chat_info` | Navigation; title filter, lists refresh, IDs |
| `folder_previous`, `folder_next` | Chat list folders |
| `pin`, `pin_up`, `pin_down`, `archive` | Chat list; native folder pins and archive |
| `reply`, `reply_target`, `open_link`, `next_action`, `previous_action`, `reveal` | Conversation actions |
| `chat_color`, `folder_color` | Appearance pickers |
| `pin`, `pins` | Conversation; confirm pin/unpin, browse all pins |
| `pins_more`, `pins_previous`, `unpin_all` | Pinned-message overlay (`pins` context) |
| `search` | Open local search |
| `search_scope`, `search_query`, `search_more`, `search_previous` | Search overlay |
| `jump ALIAS` | Open the configured stable chat ID; returns to navigation |
| `noop` | Remove a binding from its declared context |

Use [Keybindings](Keybindings.md) for defaults and [Appearance](Appearance.md)
for the `colors` table and the precedence of in-app overrides.
