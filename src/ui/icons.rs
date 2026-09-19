//! Opt-in glyph selection; Ratatui and unicode-width still own cell layout.
//!
//! Codepoints verified against Nerd Fonts v3.4.0 glyphnames.json:
//! <https://github.com/ryanoasis/nerd-fonts/blob/v3.4.0/glyphnames.json>
//! No font assets or rendering backend are embedded. Like Yazi's Icon and
//! Codex's terminal palettes, icons remain text with a readable fallback.

use crate::model::{AttachmentKind, ChatKind};

#[derive(Clone, Copy)]
pub(super) struct Icons(pub bool);

impl Icons {
    pub(super) const fn pin(self) -> &'static str {
        if self.0 { "\u{f08d}" } else { "^" } // nf-fa-thumbtack
    }

    pub(super) const fn chat(self, kind: ChatKind) -> &'static str {
        if !self.0 {
            return "";
        }
        match kind {
            ChatKind::Direct => "\u{f075} ",  // nf-fa-comment
            ChatKind::Group => "\u{f0c0} ",   // nf-fa-users
            ChatKind::Channel => "\u{f0a1} ", // nf-fa-bullhorn
        }
    }

    pub(super) const fn folder(self, id: i32) -> &'static str {
        if !self.0 {
            ""
        } else if id == 1 {
            "\u{f187} " // nf-fa-box_archive
        } else {
            "\u{f07b} " // nf-fa-folder
        }
    }

    pub(super) const fn attachment(self, kind: AttachmentKind) -> &'static str {
        if !self.0 {
            return "";
        }
        match kind {
            AttachmentKind::Photo => "\u{f03e} ", // nf-fa-image
            AttachmentKind::File | AttachmentKind::Other => "\u{f15b} ", // nf-fa-file
            AttachmentKind::Video => "\u{f03d} ", // nf-fa-video
            AttachmentKind::Audio => "\u{f001} ", // nf-fa-music
            AttachmentKind::Sticker => "\u{f118} ", // nf-fa-face_smile
        }
    }
}
