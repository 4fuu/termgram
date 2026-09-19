# Terminal integration

[简体中文](../zh-CN/Terminal.md) · [Guide](Home.md)

Yazi's pinned terminal crates own input parsing, terminal capabilities, the TTY
writer, and restoration. Ratatui draws using that writer; Crossterm supplies its
output backend, not a second input reader. Paste, focus, mouse, resize, enhanced
key reports, and associated Unicode text enter one application event stream.

Shift-Enter requires a terminal that reports it separately. Ctrl-J is the newline
fallback. Function keys or Alt combinations can also be intercepted by your
terminal or desktop; choose another binding in [Lua](Configuration.md).
Ctrl-L redraws the screen after graphical corruption.

## Images

Yazi chooses the graphics protocol. `ratatui-image` renders multiple inline
images and clips them while scrolling. Ghostty/Kitty can use Kitty graphics;
iTerm2/WezTerm can use inline images; Sixel is used where detected. Other terminals
fall back to Unicode half-blocks without an external image-overlay process.
Actual selection depends on reported capabilities and any multiplexer in between.

Decoding applies EXIF orientation and retains upstream ICC-to-sRGB conversion.
Image decoding/encoding runs away from the UI thread. Only visible encoded
images are retained, and resizing or overlays clear affected image areas.
Animated stickers use a static thumbnail. For downloads and retry behavior, see
[Attachments](Attachments.md).

## tmux

By default Termgram uses tmux's reported capabilities. To opt into Yazi's
second-stage terminal probe, set this **in the shell environment before launch**:

```sh
TERMGRAM_TMUX_PASSTHROUGH=1 tg
```

This calls upstream tmux setup, setting the pane's `allow-passthrough=all` and
the server's `input-buffer-size=104857600`. Those settings remain after Termgram
exits. This option is read before the Telegram credentials `.env` is loaded.

The app restores terminal modes on normal exit, initialization failure and the
panic hook. One owner coordinates text and graphics cleanup. Upstream revisions,
licenses, and small adaptations are recorded in [vendor provenance](../../../vendor/README.md).
