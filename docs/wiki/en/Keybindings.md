# Keybindings

[简体中文](../zh-CN/Keybindings.md) · [Guide](Home.md)

These are defaults. `?` displays the current Lua bindings. Uppercase keys mean
Shift plus that letter. `g g`, `g i`, and `g o` are successive presses; a pending
chord times out after one second and Esc cancels it. Keys apply to the focused
pane or overlay, so ordinary letters in an editor remain text.

| Context | Keys | Action |
| --- | --- | --- |
| Global | Ctrl-C / Ctrl-L | Quit / redraw |
| Global | F2 / F3 | Next account / add account |
| Login | Enter / Esc | Submit / restart phone sign-in |
| Login | Tab or Shift-Tab | Start QR login or change QR display |
| Chats | j/k or Down/Up | Select next/previous chat |
| Chats | Enter or Right | Open selected chat |
| Chats | G / g g | Last / first chat in the filtered list |
| Chats | PageDown/PageUp | Move ten chats |
| Chats | / | Filter titles in this folder |
| Chats | Ctrl-F | Open local regex search |
| Chats | [ / ] | Previous / next Telegram folder |
| Chats | c / C | Chat / folder color picker |
| Conversation | j/k or ]/[ | Select next/previous message |
| Conversation | 20k | Move up 20 messages; fetch older pages if needed |
| Conversation | Up/Down | Scroll rendered rows |
| Conversation | PageUp/PageDown | Scroll ten rendered rows |
| Conversation | G or End | Return to latest messages and follow incoming messages |
| Conversation | g g or Home | Oldest message in the loaded window |
| Conversation | i | Compose |
| Conversation | Enter | Activate the selection; compose if nothing is selected |
| Conversation | R / r | Reply to selection/latest / open its reply target |
| Conversation | o / g o | Next / previous actionable item |
| Conversation | l | Open the selected or first supported link |
| Conversation | O | Reveal attachment in the system file manager |
| Conversation | / / c | Local regex search / chat color |
| Navigation | Tab or Shift-Tab | Switch between chat list and conversation |
| Conversation | Esc or Left | Focus/return to the chat list |
| Navigation | g i / Ctrl-R | Show chat and folder IDs / refresh lists |
| Navigation | ? / s / a / q | Help / settings / accounts / quit |
| Composer | Enter | Send |
| Composer | Shift-Enter or Ctrl-J | Newline |
| Composer | Esc | Cancel reply first; then leave with draft kept |
| Editors | Left/Right, Home/End, Ctrl-A/Ctrl-E | Move cursor |
| Editors | Backspace/Delete, Ctrl-W, Ctrl-U | Delete character / previous word / clear |
| Chat filter | Enter / Esc | Open match / clear filter and leave |
| Overlays | Up/Down or k/j | Select or scroll |
| Overlays | Enter or Space | Apply setting, color, or account selection |
| Overlays | Esc or ? | Close |
| Search | Enter / Esc | Search or open result / close |
| Search | Tab / Ctrl-F | Change scope / edit query |
| Search | Ctrl-N / Ctrl-P | Next / previous result page |
| Search | Up/Down, PageUp/PageDown | Select result / move ten results |

A number prefix such as `20k` works in the two navigation panes. For a single
key that jumps N messages, use `count` in [Lua configuration](Configuration.md).
`gg` is scoped to loaded history; it does not download every message back to the
start of a group. `G` reloads the latest page when reading older history.

Changed defaults from earlier Termgram versions: conversation `/` now opens
local search; type bot `/commands` after `i`. Uppercase `O` now reveals files;
previous action moves to `g o`. Use the account picker with arrows and Enter;
digit keys in navigation are counts, and the old account-number hints are gone.
There is no separate hardcoded keyboard fallback after Lua resolution.
