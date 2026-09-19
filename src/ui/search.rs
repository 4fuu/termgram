use super::{
    AppState, Clear, Constraint, DANGER, Frame, Layout, List, ListItem, ListState, Local, MUTED,
    Modifier, Paragraph, Position, Rect, Style, Wrap, centered, clamp_u16, pane_block, spinner,
    truncate_cells,
};
use crate::keymap::Context;

pub(super) fn render_search(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let popup = centered(
        area,
        area.width.min(110),
        area.height.saturating_sub(2).max(8),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(format!(" Local regex · {} ", app.search.scope_label), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(inner);
    let input = &app.search.query;
    let scroll = input
        .cursor_display_width()
        .saturating_sub(usize::from(rows[0].width.saturating_sub(3)));
    frame.render_widget(
        Paragraph::new(input.value())
            .scroll((0, clamp_u16(scroll)))
            .block(pane_block(" Pattern ".to_owned(), app.search.editing)),
        rows[0],
    );
    if app.search.editing {
        frame.set_cursor_position(Position::new(
            rows[0].x + 1 + clamp_u16(input.cursor_display_width().saturating_sub(scroll)),
            rows[0].y + 1,
        ));
    }
    let coverage = coverage_label(app);
    frame.render_widget(
        Paragraph::new(coverage)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(if app.search.error.is_some() {
                DANGER
            } else {
                MUTED
            })),
        rows[1],
    );
    let height = usize::from(rows[2].height).max(1);
    let start = app.search.selected.saturating_add(1).saturating_sub(height);
    let messages = app
        .search
        .page
        .as_ref()
        .map_or(&[][..], |page| page.messages.as_slice());
    let items: Vec<_> = messages
        .iter()
        .skip(start)
        .take(height)
        .map(|message| result_row(message, app, usize::from(rows[2].width.saturating_sub(2))))
        .collect();
    let items = if items.is_empty() && app.search.page.is_some() && !app.search.loading {
        vec![ListItem::new("No matches in the cached range")]
    } else {
        items
    };
    let mut state = ListState::default()
        .with_selected((!messages.is_empty()).then_some(app.search.selected.saturating_sub(start)));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[2],
        &mut state,
    );
    let hint = |action| app.keymap.hint(Context::Search, action);
    frame.render_widget(
        Paragraph::new(format!(
            "{} {} · {} close\n{} scope · {} edit\n{} previous · {} next",
            hint("open"),
            if app.search.editing { "search" } else { "open" },
            hint("cancel"),
            hint("search_scope"),
            hint("search_query"),
            hint("search_previous"),
            hint("search_more")
        ))
        .style(Style::default().fg(MUTED)),
        rows[3],
    );
}

fn coverage_label(app: &AppState) -> String {
    if let Some(error) = &app.search.error {
        error.clone()
    } else if app.search.loading {
        format!("{} Searching cached messages…", spinner(app.tick))
    } else if let Some(page) = &app.search.page {
        let date = |time: Option<i64>| {
            time.and_then(|time| chrono::DateTime::from_timestamp(time, 0))
                .map_or_else(
                    || "—".to_owned(),
                    |date| date.format("%Y-%m-%d").to_string(),
                )
        };
        format!(
            "{} cached messages · {} to {} UTC · page {} · {} hits{}",
            page.cached_messages,
            date(page.oldest),
            date(page.newest),
            app.search.page_number,
            page.messages.len(),
            if page.next.is_some() {
                " · more available"
            } else {
                ""
            }
        )
    } else {
        "Searches cached message text only; older uncached history is not included.".to_owned()
    }
}

fn result_row(message: &crate::model::Message, app: &AppState, width: usize) -> ListItem<'static> {
    let title = app
        .chats
        .iter()
        .find(|chat| chat.id == message.chat_id)
        .map_or("Unknown", |chat| chat.title.as_str());
    ListItem::new(truncate_cells(
        &format!(
            "{} · {} · {}",
            title,
            message
                .timestamp
                .with_timezone(&Local)
                .format("%m-%d %H:%M"),
            crate::model::sanitize_terminal_line(&message.text)
        ),
        width,
    ))
}
