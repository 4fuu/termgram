# Attachments and file previews

[简体中文](../zh-CN/Attachments.md) · [Guide](Home.md)

## Prepare files to send

Use `:attach /absolute/path` in an open chat. Multiple paths can be quoted using
your platform's argument syntax; local `file:` URLs also work. Paths are parsed
by Yazi, without executing a shell or expanding variables. Plain terminal paste
in the conversation enters the composer as text by default, including paths.

Files are added to that chat's draft in the background, without uploading.
The composer shows the count. Open `:attachments` or press Ctrl-O in the composer
to review the names, sizes and chosen send format. Click a row or use j/k to select.

| Key in attachment review | Behavior |
| --- | --- |
| a | Open `:attach` to add files |
| p | Toggle a supported image between Telegram photo and original file |
| d / Delete | Remove the selected reference, or cancel pending preparation |
| o / Enter | Preview a supported image; Esc returns to the list |
| O | Reveal the original in the system file manager |
| i | Return to the caption editor |
| Esc | Return to the previous view, keeping the draft |

Enter **in the composer** sends the reviewed files. The first file carries the
composer caption and reply target. There are at most eight files and 2 GiB per
draft. Uploads and downloads share a queue of at most 32 tasks with three running
at once. Files in the same chat are uploaded in admission order. This revision sends individual messages; album grouping follows later.

File references, chosen formats and content fingerprints survive normal restarts
with the [local draft](UX.md). Removing an attachment never deletes its original.
Before any upload, Termgram checks the fingerprint while creating a private
upload copy. Missing or changed originals fail with a retryable status; reattach
changed files to review their new content. Upload copies are removed when their
transfer finishes or is cancelled. Adding files to one chat and switching chats
while preparation runs does not move the result to the new conversation.

To stage recognizable pasted paths automatically, opt in through Lua:

```lua
attachments = { auto_attach_paths = true },
```

This adds files to the draft; it still requires explicit send. Unrecognized input
remains text. Bindings use the `attachments` context and the actions `attach`,
`attachments`, `remove_attachment`, `attachment_format`, `preview` and `reveal`.

## Paste from the clipboard

Use `:paste`, Ctrl-V or Ctrl-Alt-V in the conversation/composer. In attachment
review these shortcuts also add files, and `?` opens the effective key list.
The native clipboard reads a file list first, preserving every original file
within the draft limit. If there is no file list, an image becomes a PNG draft
asset; ordinary text is inserted at the saved cursor. Nothing is sent until you
send from the composer. Lua action: `paste_clipboard`.

```lua
attachments = {
  auto_attach_paths = false,
  clipboard_as_photo = true,
},
```

Set `clipboard_as_photo = false` to start clipboard images in original-file
mode; `p` still changes individual images in review. Bitmap data is limited to
128 MiB after the OS read, the PNG asset to 16 MiB and text to 1 MiB. A ten-second
preparation timeout keeps the interface responsive. If the OS call is still
running, another read is rejected until it finishes; timeout does not spawn a
second blocked reader. Late results are discarded and their unused assets removed.

Unsent screenshots live beside the user-state database in
`<state filename>.attachments/<Telegram account ID>/`, outside download-cache
retention. Normal restart keeps referenced drafts. Removal and successful send
release owned files after the last active reference; failures keep the retry's
file. Startup removes orphaned screenshot assets for that account, preserving
persisted drafts and in-process drafts still waiting for a background save.

| Environment | Clipboard backend |
| --- | --- |
| macOS / Windows | arboard native file lists, images and text |
| Linux X11 / XWayland | arboard X11 backend |
| Linux Wayland | arboard data-control backend when the compositor supports it; X11 fallback |
| WSL | Native first, then Windows PowerShell clipboard data and `wslpath`; child processes have output limits and a four-second timeout |
| SSH / an unavailable desktop clipboard | Use files accessible to the remote host with `:attach`; terminal MIME paste is separate |

Cmd-V and Ctrl-Shift-V are usually handled by the terminal. They may deliver
text paths, or no image at all. Use `:paste` when the terminal captures the
application shortcuts. Reading the native clipboard on an SSH host does not
read the local desktop's clipboard. Cross-platform clipboard access depends on
the desktop session and its permissions; no additional terminal input reader is used.

## Received files

Select an attachment with `j/k` (or click it) and press uppercase `O` (`Shift-O`)
to reveal it in your system file manager. This is a single press, not a timed
hold gesture. Reported key-repeat events are ignored. A second activation while
the same file is downloading does not start another transfer.

An existing full-size image preview or downloaded file is reused. Otherwise
Termgram downloads the original attachment and reveals it when ready. Animated
sticker thumbnails are previews only; `O` downloads the original. Missing cached
files are downloaded again. Failures appear in the status area and can be retried.

macOS selects the file in Finder; Windows selects it in Explorer. Linux opens the
containing directory through `xdg-open` (selection support varies by file manager).
The file itself is not executed. `O` is an explicit reveal action even when the
settings menu's Enter download behavior is **Keep in cache**.

A single click selects media and shows its available keys. Press `o` to expand an
image or sticker in the terminal; Esc closes the preview, `i` replies, and `O`
reveals the original file. Selection itself does not activate links, bot buttons
or file-manager operations. Use `g n` / `g p` to cycle actionable items and Enter
to activate one. Lua actions are `preview`, `next_action`, `previous_action` and
`reveal`; the expanded view uses the `preview` context.

Original media IDs and request generations prevent an older transfer from
replacing an edited or deleted attachment. Download mappings are stored with the
message cache and survive restart. Completed files live in
`<session filename>.media/<Telegram account ID>/`. Termgram keeps at most 512
completed files and targets a 1 GiB budget per directory, removing older files.
The file just downloaded is retained even if it alone exceeds the byte budget;
a later cleanup can remove it. Copy files you want to keep outside this cache.

Partial downloads use the maintained `tempfile` crate and stay separate from
completed files. Cancellation removes partial files; interrupted-process remnants
are cleaned before the next transfer. The account cache has one process owner,
and outstanding media tasks retain its lock through cleanup. This also prevents
two clients from independently advancing one message-cache checkpoint.

Photos, image documents and stickers preview in the timeline. Only visible
previews are requested, with at most two preview downloads pending in the app;
all network transfers share a bounded worker. Animated TGS/WebM stickers show
Telegram's raster thumbnail and are labeled as static previews. Failed previews
remain retryable through `o` or Enter after selection. Ctrl-L redraws; terminal protocol details
are in [Terminal](Terminal.md). Use `:attach` to prepare local files before sending.
