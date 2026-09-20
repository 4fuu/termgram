//! Adapt Telegram entities to Ratatui spans, then use the same source-range
//! wrapper as plain transcript text. No Markdown parser or second text engine.
use super::{ACCENT, MUTED, wrapping};
use crate::{
    app::{AppState, MessageAction},
    entities::{Entity, Kind},
    model::Message,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) struct Row {
    pub line: Line<'static>,
    pub action: Option<MessageAction>,
}

fn block_boundaries(length: usize, entities: &[&Entity]) -> Vec<usize> {
    let mut boundaries = vec![0, length];
    for entity in entities {
        if matches!(entity.kind, Kind::Pre { .. } | Kind::Quote { .. }) {
            boundaries.extend([entity.range.start, entity.range.end]);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

pub(super) fn render(message: &Message, width: usize, app: &AppState) -> Vec<Row> {
    render_text(
        &message.text,
        &message.entities,
        width,
        app.spoilers_revealed(message),
        app.quotes_expanded(message),
    )
}

pub(super) fn render_text(
    text: &str,
    formatting: &[Entity],
    width: usize,
    revealed: bool,
    expanded: bool,
) -> Vec<Row> {
    let entities: Vec<_> = formatting
        .iter()
        .filter(|entity| entity.valid_for(text))
        .collect();
    let boundaries = block_boundaries(text.len(), &entities);
    let mut result = Vec::new();
    for boundary in boundaries.windows(2) {
        let range = boundary[0]..boundary[1];
        let block = entities.iter().find(|entity| {
            entity.overlaps(range.start, range.end)
                && matches!(entity.kind, Kind::Pre { .. } | Kind::Quote { .. })
        });
        let code = block.is_some_and(|entity| matches!(entity.kind, Kind::Pre { .. }));
        let foldable =
            block.is_some_and(|entity| matches!(entity.kind, Kind::Quote { collapsed: true }));
        let indent = if block.is_some() && width > 2 {
            "│ "
        } else {
            ""
        };
        let available = width.saturating_sub(indent.width()).max(1);
        let source = &text[range.clone()];
        // A newline adjoining a block is its row separator, not an extra
        // empty paragraph. Keep interior blank lines and a final hard break.
        let source = if range.end < text.len() {
            source.strip_suffix('\n').unwrap_or(source)
        } else {
            source
        };
        if source.is_empty() && range.end < text.len() {
            continue;
        }
        if let Some(Entity {
            kind: Kind::Pre { language },
            ..
        }) = block.copied()
            && !language.is_empty()
            && width > 2
        {
            result.push(Row {
                line: Line::styled(
                    format!(
                        "┌ {}",
                        super::truncate_cells(language, width.saturating_sub(2))
                    ),
                    Style::default().fg(MUTED),
                ),
                action: None,
            });
        }
        let rows = wrapping::ranges(source, available, !code);
        let folded = foldable && !expanded && rows.len() > 3;
        for row in rows.iter().take(if folded { 2 } else { rows.len() }) {
            let span_range = range.start + row.start..range.start + row.end;
            let spoiler = entities.iter().any(|entity| {
                entity.kind == Kind::Spoiler && entity.overlaps(span_range.start, span_range.end)
            });
            let action = if spoiler {
                Some(MessageAction::Spoilers)
            } else if foldable {
                Some(MessageAction::ExpandQuote)
            } else {
                None
            };
            let base = if code {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            let mut spans = Vec::new();
            if !indent.is_empty() {
                spans.push(Span::styled(
                    indent,
                    Style::default().fg(if code { MUTED } else { ACCENT }),
                ));
            }
            spans.extend(styled(text, span_range, &entities, base, revealed));
            result.push(Row {
                line: Line::from(spans),
                action,
            });
        }
        if folded {
            result.push(Row {
                line: Line::styled(format!("{indent}…"), Style::default().fg(ACCENT)),
                action: Some(MessageAction::ExpandQuote),
            });
        }
    }
    if result.is_empty() {
        result.push(Row {
            line: Line::default(),
            action: None,
        });
    }
    result
}

fn styled(
    text: &str,
    range: Range<usize>,
    entities: &[&Entity],
    base: Style,
    revealed: bool,
) -> Vec<Span<'static>> {
    if entities.is_empty() {
        return vec![Span::styled(text[range].to_owned(), base)];
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (offset, grapheme) in text[range.clone()].grapheme_indices(true) {
        let start = range.start + offset;
        let end = start + grapheme.len();
        let mut style = base;
        let mut hidden = false;
        for entity in entities.iter().filter(|entity| entity.overlaps(start, end)) {
            style = match entity.kind {
                Kind::Bold => style.bold(),
                Kind::Italic => style.italic(),
                Kind::Underline => style.underlined(),
                Kind::Strike => style.add_modifier(Modifier::CROSSED_OUT),
                Kind::Code | Kind::Pre { .. } => style.fg(Color::Yellow),
                Kind::Link => style.fg(ACCENT).underlined(),
                Kind::Mention | Kind::Tag => style.fg(ACCENT),
                Kind::Quote { .. } => style,
                Kind::Spoiler => {
                    hidden = !revealed;
                    style
                }
            };
        }
        let content = if hidden {
            style = Style::default().fg(MUTED);
            "▨".repeat(grapheme.width())
        } else {
            grapheme.to_owned()
        };
        if let Some(last) = spans.last_mut().filter(|last| last.style == style) {
            last.content.to_mut().push_str(&content);
        } else {
            spans.push(Span::styled(content, style));
        }
    }
    spans
}
