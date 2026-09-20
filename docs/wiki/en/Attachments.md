# Attachments and file previews

[简体中文](../zh-CN/Attachments.md) · [Guide](Home.md)

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
are in [Terminal](Terminal.md). Drop local file paths to upload as described in
[Daily workflows](UX.md).
