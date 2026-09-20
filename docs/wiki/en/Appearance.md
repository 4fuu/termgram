# Appearance

[简体中文](../zh-CN/Appearance.md) · [Guide](Home.md)

Press `c` on a chat, or in its conversation, to choose its color. Press `C` in the
chat list to color the current folder. Up/Down selects a named terminal color;
Enter applies it and Esc cancels. The list marker and selection styling remain
visible independently of the chosen color.

Chat rows and conversation titles use the terminal's default foreground unless
configured otherwise. Folder titles use a stable color chosen from six ANSI
palette entries. Your terminal theme controls the actual RGB values; the app
does not assume a dark background for these colors.

Precedence is built-in default → Lua → in-app override. Choose **Follow
configuration** to remove an override. **Terminal default** is an explicit color
choice and can override a colored Lua default.

```lua
return {
  colors = {
    chats = { [-1001234567890] = "cyan" },
    folders = { [2] = "yellow" },
  },
}
```

Values are `default`, `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`,
`gray`, `dark_gray`, `light_red`, `light_green`, `light_yellow`, `light_blue`,
`light_magenta`, `light_cyan`, and `white`. `g i` shows chat and current folder IDs.

In-app overrides live in `appearance.json` beside `settings.conf`, keyed by the
actual Telegram account ID. Switching accounts or clearing the message cache
does not mix or delete them. Writes reuse the atomic settings writer. A malformed
preferences file is reported and is not silently replaced. Your Lua configuration
and the server's folder colors are never rewritten by this picker.

## Nerd Font icons

Nerd Font support is opt-in. Install a [Nerd Font](https://www.nerdfonts.com/font-downloads)
and select its **Nerd Font Mono** variant in the terminal profile, then add this
to `config.lua` and restart Termgram:

```lua
return {
  nerd_font = true,
}
```

Use a v3+ font. Icons identify chat types, folders, Archive, pins and attachments;
text labels, key hints and terminal colors remain visible. The Mono variant keeps
icons within terminal cells. With SSH or tmux, configure the font on the terminal
that displays the session. Termgram uses the terminal's selected font and does
not install fonts or change terminal preferences.

The default `nerd_font = false` retains the ordinary text presentation and `^`
pin marker. Turn the option off if glyphs appear as boxes or overlap adjacent text.
Configuration errors are reported through the existing Lua configuration loader.

Font reference: [Nerd Fonts font variants](https://github.com/ryanoasis/nerd-fonts/wiki/FAQ-and-Troubleshooting).

## Chat list columns

Titles use the configured chat color. Time defaults to cyan and unread counts to
yellow; both are right-aligned independently of title length. A marker and an
underlined selected title identify focus without covering colors with a reversed
row. Nerd Font chat/pin icons share the title column; CJK and emoji names truncate
by terminal cells. The compact sidebar defaults to 30 columns and F4 toggles it.
See [Lua configuration](Configuration.md) for width and semantic color options.

## Conversation layout

Each message has a separate author header, with local time, outgoing delivery
state and optional message ID aligned on the right. Incoming authors use stable
colors from the terminal palette; outgoing authors use green. Body text keeps the
terminal's default foreground and background. Earlier dates include month/day;
other years include the year. On very narrow headers, optional IDs yield space to
the author and delivery/time information.

Replies occupy their own row above text/media. A thin left marker identifies the
selected message; the current reply, attachment or link action also gains emphasis.
Body, action rows and inline media share a small two-column gutter. A blank line
separates messages. File labels describe the file and active transfer state;
selection-specific keys belong in the bottom bar. `o` expands selected media,
`i` replies, and `g n` / `g p` move among actions.

Resizing and sidebar toggles retain the anchored message. If reflow removes the
old physical row, the viewport returns to that message's header rather than its
trailing separator. This is a message/row anchor, not an exact text-character
position across different wrapping widths.
