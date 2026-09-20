use super::{
    AppState, Clear, Constraint, DANGER, Frame, Layout, List, ListItem, ListState, Local, MUTED,
    Modifier, Paragraph, Position, Rect, Style, Wrap, centered, clamp_u16, pane_block, spinner,
    truncate_cells, wrapped_height,
};
use crate::app::search::{Results, Source};
use crate::keymap::Context;

pub(super) fn render_search(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let popup = centered(
        area,
        area.width.min(110),
        area.height.saturating_sub(2).max(8),
    );
    frame.render_widget(Clear, popup);
    let source = if app.search.is_cloud() {
        "Telegram search"
    } else {
        "Local regex"
    };
    let block = pane_block(format!(" {source} · {} ", app.search.scope_label), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let coverage = coverage_label(app);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(wrapped_height(&coverage, inner.width).clamp(2, 4)),
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
            .block(pane_block(
                if app.search.is_cloud() {
                    " Text "
                } else {
                    " Pattern "
                }
                .to_owned(),
                app.search.editing,
            )),
        rows[0],
    );
    if app.search.editing {
        frame.set_cursor_position(Position::new(
            rows[0].x + 1 + clamp_u16(input.cursor_display_width().saturating_sub(scroll)),
            rows[0].y + 1,
        ));
    }
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
    let messages = app.search.page.as_ref().map_or(&[][..], Results::messages);
    let items: Vec<_> = messages
        .iter()
        .skip(start)
        .take(height)
        .map(|message| result_row(message, app, usize::from(rows[2].width.saturating_sub(2))))
        .collect();
    let items = if items.is_empty() && app.search.page.is_some() && !app.search.loading {
        vec![ListItem::new(if app.search.is_cloud() {
            "No Telegram matches"
        } else {
            "No matches in the cached range"
        })]
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
    render_hints(frame, rows[3], app);
}

fn render_hints(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let hint = |action| app.keymap.hint(Context::Search, action);
    frame.render_widget(
        Paragraph::new(format!(
            "{} {} · {} close\n{} · {} edit\n{} previous · {} next",
            hint("open"),
            if app.search.editing { "search" } else { "open" },
            hint("cancel"),
            if app.search.is_cloud() {
                ":search --cloud to change filters".to_owned()
            } else {
                format!("{} scope", hint("search_scope"))
            },
            hint("search_query"),
            hint("search_previous"),
            hint("search_more")
        ))
        .style(Style::default().fg(MUTED)),
        area,
    );
}

fn coverage_label(app: &AppState) -> String {
    if let Some(error) = &app.search.error {
        error.clone()
    } else if app.search.loading {
        format!(
            "{} {}…",
            spinner(app.tick),
            if app.search.is_cloud() {
                "Loading from Telegram"
            } else {
                "Searching cached messages"
            }
        )
    } else if let Some(Results::Local(page)) = &app.search.page {
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
    } else if let Source::Cloud { filters, .. } = &app.search.source {
        let coverage = if let Some(Results::Cloud(page)) = &app.search.page {
            format!(
                "Telegram: {} matches · page {} · {} hits{}",
                page.total,
                app.search.page_number,
                page.messages.len(),
                if page.next.is_some() {
                    " · more available"
                } else {
                    ""
                }
            )
        } else {
            "Searches this chat on Telegram; text uses Telegram's search syntax.".to_owned()
        };
        let filters = filters.label();
        if filters.is_empty() {
            coverage
        } else {
            format!("{coverage}\n{filters}")
        }
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
            "{} · {} · {} · {}",
            title,
            crate::model::sanitize_terminal_line(&message.sender),
            message
                .timestamp
                .with_timezone(&Local)
                .format("%m-%d %H:%M"),
            crate::model::sanitize_terminal_line(if message.text.is_empty() {
                message
                    .attachment
                    .as_ref()
                    .map_or("Message", |media| media.display_name())
            } else {
                &message.text
            })
        ),
        width,
    ))
}
