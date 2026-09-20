//! Yazi-style help filtering with the existing editor, keymap and cell wrapping.
use super::{ACCENT, MUTED, WARNING, centered, clamp_u16, wrapping};
use crate::{app::AppState, keymap::Context};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use std::ops::Range;

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let popup = centered(area, area.width.min(76), area.height.min(27));
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(" Shortcuts ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let entries = app.help_lines();
    let mut count = 0;
    let mut lines = Vec::new();
    for entry in &entries {
        let matches = app.help.matcher.as_ref().map_or_else(Vec::new, |matcher| {
            matcher
                .find_iter(entry)
                .map(|found| found.range())
                .collect::<Vec<_>>()
        });
        if app.help.matcher.is_some() && matches.is_empty() {
            continue;
        }
        count += 1;
        for range in wrapping::ranges(entry, usize::from(areas[2].width.max(1)), true) {
            lines.push(highlight(entry, range, &matches));
        }
    }
    let filtered = app.help.matcher.is_some();
    frame.render_widget(
        Paragraph::new(if filtered {
            format!("{count} matching entries")
        } else {
            "Search keys, descriptions, contexts and commands".to_owned()
        })
        .style(Style::default().fg(MUTED)),
        areas[1],
    );
    if lines.is_empty() {
        lines.push(Line::styled(
            "No matching shortcuts or commands",
            Style::default().fg(MUTED),
        ));
    }
    app.help.scroll = app
        .help
        .scroll
        .min(lines.len().saturating_sub(usize::from(areas[2].height)));
    frame.render_widget(
        Paragraph::new(lines).scroll((clamp_u16(app.help.scroll), 0)),
        areas[2],
    );
    render_query(frame, areas[0], app);
    let hint = if app.help.editing {
        format!(
            "{} keep filter · {} clear · {}/{} scroll",
            app.keymap.hint(Context::Input, "open"),
            app.keymap.hint(Context::Input, "cancel"),
            app.keymap.hint(Context::Input, "up"),
            app.keymap.hint(Context::Input, "down")
        )
    } else {
        format!(
            "{} search · {}/{} scroll · {} {}",
            app.keymap.hint(Context::Help, "filter"),
            app.keymap.hint(Context::Help, "up"),
            app.keymap.hint(Context::Help, "down"),
            app.keymap.hint(Context::Help, "cancel"),
            if filtered { "clear" } else { "close" }
        )
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(MUTED)),
        areas[3],
    );
}

fn render_query(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let query = &app.help.query;
    let text = if query.is_empty() && !app.help.editing {
        format!(
            "{} Search shortcuts…",
            app.keymap.hint(Context::Help, "filter")
        )
    } else {
        format!("/ {}", query.value())
    };
    let column = query.cursor_display_width().saturating_add(2);
    let scroll = column.saturating_sub(usize::from(area.width.saturating_sub(1)));
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(ACCENT))
            .scroll((0, clamp_u16(scroll))),
        area,
    );
    if app.help.editing && area.width > 0 && area.height > 0 {
        frame.set_cursor_position(Position::new(
            area.x.saturating_add(clamp_u16(column - scroll)),
            area.y,
        ));
    }
}

fn highlight(source: &str, row: Range<usize>, matches: &[Range<usize>]) -> Line<'static> {
    let mut spans = Vec::new();
    let mut cursor = row.start;
    for found in matches {
        let start = found.start.max(row.start);
        let end = found.end.min(row.end);
        if start >= end {
            continue;
        }
        if cursor < start {
            spans.push(Span::raw(source[cursor..start].to_owned()));
        }
        spans.push(Span::styled(
            source[start..end].to_owned(),
            Style::default().fg(Color::Black).bg(WARNING).bold(),
        ));
        cursor = end;
    }
    if cursor < row.end {
        spans.push(Span::raw(source[cursor..row.end].to_owned()));
    }
    Line::from(spans)
}
