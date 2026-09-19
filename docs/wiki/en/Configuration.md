# Lua configuration

[简体中文](../zh-CN/Configuration.md)

Place `config.lua` beside Termgram's `settings.conf`, or set `TERMGRAM_CONFIG` to
an explicit file path. Restart to load changes. Start with [the example](../../../examples/config.lua).
The file returns a Lua table; tables, strings, mathematics and UTF-8 helpers are
available. Filesystem, process and plugin APIs are deliberately outside this
configuration interface. Invalid files show an error and retain default bindings.

```lua
return {
  chats = { work = -1001234567890 },
  ghost_text = "{send} to send · {newline} for a new line",
  keymap = {
    { context = "conversation", on = { "g", "w" }, run = "jump work" },
    { context = "compose", on = { "<Enter>" }, run = "newline" },
    { context = "compose", on = { "<C-s>" }, run = "send" },
  },
}
```

Contexts are `global`, `chats`, `conversation`, `compose`, `input` (login and chat
filter), `overlay`, and `search`. A context takes precedence over global bindings. Rebinding
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

Composer placeholders use the effective `{send}`, `{newline}` and `{cancel}`
bindings. Set `ghost_text = ""` to hide them. Managed in-app preferences remain
in their own file; Termgram does not rewrite your Lua configuration.

Press `g i` to show the current chat ID for an alias.
