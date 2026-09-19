# Chat and folder colors

[简体中文](../zh-CN/Appearance.md)

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
