//! Shared grapheme-aware wrapping for plain and styled transcript text.
//! Ranges retain source offsets; code preserves spaces, prose trims row ends.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) fn wrap_cells(value: &str, max: usize) -> Vec<String> {
    ranges(value, max, true)
        .into_iter()
        .map(|range| value[range].to_owned())
        .collect()
}

pub(super) fn ranges(value: &str, max: usize, trim: bool) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for source in value.split('\n') {
        let mut start = 0;
        let mut end = 0;
        let mut width = 0_usize;
        let mut push = |start, end| {
            let text = &source[start..end];
            let length = if trim {
                text.trim_end().len()
            } else {
                text.len()
            };
            lines.push(offset + start..offset + start + length);
        };
        for word in source.split_inclusive(char::is_whitespace) {
            let word_width = word.width();
            if width.saturating_add(word_width) > max && end > start {
                push(start, end);
                start = end;
                width = 0;
            }
            if word_width > max {
                for grapheme in word.graphemes(true) {
                    let cells = grapheme.width();
                    if width.saturating_add(cells) > max && end > start {
                        push(start, end);
                        start = end;
                        width = 0;
                    }
                    end += grapheme.len();
                    width = width.saturating_add(cells);
                }
            } else {
                end += word.len();
                width = width.saturating_add(word_width);
            }
        }
        push(start, end);
        offset += source.len() + 1;
    }
    lines
}
