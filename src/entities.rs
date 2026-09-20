//! Telegram text formatting in terminal-safe UTF-8 byte ranges.
//!
//! Protocol UTF-16 offsets are converted once in the network adapter. Cache and
//! presentation share these ranges; unknown entities leave their text readable.
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Entity {
    pub range: Range<usize>,
    pub kind: Kind,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Kind {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Pre { language: String },
    Quote { collapsed: bool },
    Spoiler,
    Link,
    Mention,
    Tag,
}

impl Entity {
    #[must_use]
    pub fn valid_for(&self, text: &str) -> bool {
        self.range.start < self.range.end && text.get(self.range.clone()).is_some()
    }

    #[must_use]
    pub const fn overlaps(&self, start: usize, end: usize) -> bool {
        self.range.start < end && self.range.end > start
    }
}

/// Hide complete graphemes, including when an entity covers only a combining
/// mark. The returned text contains no hidden original bytes or ANSI conceal.
#[must_use]
pub fn conceal<'a>(text: &'a str, entities: &[Entity]) -> Cow<'a, str> {
    let spoilers: Vec<_> = entities
        .iter()
        .filter(|entity| entity.kind == Kind::Spoiler && entity.valid_for(text))
        .collect();
    if spoilers.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut output = String::with_capacity(text.len());
    for (start, grapheme) in text.grapheme_indices(true) {
        if grapheme != "\n"
            && spoilers
                .iter()
                .any(|entity| entity.overlaps(start, start + grapheme.len()))
        {
            output.push_str(&"▨".repeat(grapheme.width()));
        } else {
            output.push_str(grapheme);
        }
    }
    Cow::Owned(output)
}
