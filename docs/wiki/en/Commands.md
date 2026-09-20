# Commands

[简体中文](../zh-CN/Commands.md) · [Guide](Home.md)

Press `:` while navigating chats or a conversation. The bottom command line
shows available commands, their descriptions, argument usage and effective
shortcuts. Unavailable actions explain the missing selection or connection.
The header identifies the account and target captured when commands opened.
Typing `:` in a composer, search, filter or login field inserts ordinary text.

| Key | Behavior |
| --- | --- |
| Tab / Ctrl-N | Complete the next candidate |
| Shift-Tab / Ctrl-P | Complete the previous candidate |
| Up / Down | Recall older/newer commands matching the input prefix |
| Enter | Run the complete command; empty input closes |
| Esc / Ctrl-C | Cancel and return to the conversation |
| Left/Right, Home/End, Ctrl-A/Ctrl-E | Move the input cursor |
| Backspace/Delete, Ctrl-W, Ctrl-U | Edit or clear input |

Clicking a candidate fills the command without executing it. Missing arguments
and errors stay in the editor. Use a complete command name or the explicit `h`
and `q` aliases; a prefix such as `qui` only becomes executable after completion.
Command history holds up to 64 entries per account in memory and is not written
to disk. Multiline paste cannot execute a series of commands.

| Command | Behavior |
| --- | --- |
| `help [command]`, `h` | Browse commands or explain one command |
| `chat <alias, ID or title>` | Open a cached chat in this account; Tab filters titles and configured aliases |
| `folder <ID or name>` | Choose All chats, Archive, or an official Telegram folder |
| `account [slot]` | Open the account picker or switch to an existing slot |
| `search [regex]` | Open local search or submit a pattern in the current search scope |
| `attach <paths...>` | Prepare local files in the captured chat draft; never sends automatically |
| `paste` | Paste clipboard files, an image or text into the captured chat draft |
| `attachments` | Review the open conversation’s staged files |
| `latest` | Return the open conversation to its latest messages |
| `unread` | Open the first incoming message after the chat's saved read boundary |
| `read` | Explicitly mark the entire captured chat read and clear its unread reminder |
| `mark-unread` | Set Telegram's unread reminder without rewinding message receipts |
| `edit` | Edit the selected delivered message or resume this chat’s saved edit |
| `edit-discard` | Discard this chat’s local edit, preserving the ordinary draft |
| `reply` | Reply to an explicitly selected message |
| `preview` | Expand a selected image or sticker |
| `reveal` | Download if needed and reveal the selected attachment in Finder, Explorer or the file manager |
| `pins` | Browse the open conversation's pinned messages |
| `pin chat`, `unpin chat` | Set the chat's pin state in the captured folder |
| `pin message`, `unpin message` | Set the selected message's pin state using Telegram's existing options prompt |
| `archive`, `unarchive` | Move the captured chat into or out of Archive |
| `sidebar [show, hide or toggle]` | Control the sidebar; omitted argument toggles |
| `color chat`, `color folder` | Open the target's color picker |
| `settings` | Open application settings |
| `status` | Inspect connection, DC, recent ping and current in-memory loading state |
| `refresh` | Refresh chat and folder lists |
| `quit`, `q` | Exit normally |

Chat names must match a unique full title when executing. Use Tab to choose an
ID from partial title matches; duplicate titles are never guessed. Completion
uses loaded data and does not issue network searches while typing. Archive and
pin commands use the captured stable target even if incoming messages reorder
the chat list. Already pinned/archived targets keep their requested state.

The text after the first space in `search` is the regex as written, including
backslashes and trailing spaces. This command does not use shell quoting or run
shell commands. `status` uses existing observations: unavailable DC and stale
ping measurements are shown as unavailable, not as zero. It distinguishes
messages in memory from the persisted cache's search coverage.

Rebind the entry action `command` in `chats` and `conversation`. The `command`
context has `complete_next`, `complete_previous`, `history_previous` and
`history_next` actions, in addition to normal editor actions. See
[Lua configuration](Configuration.md). Keyboard bindings and command actions
share the same application dispatch and descriptions.
