use super::{
    AppState, Clear, Constraint, Frame, Layout, List, ListItem, ListState, MUTED, Modifier,
    Paragraph, Rect, Style, Wrap, centered, pane_block, truncate_cells,
};
use crate::{keymap::Context, model::sanitize_terminal_line};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let popup = centered(
        area,
        area.width.saturating_sub(4).min(90),
        area.height.saturating_sub(2).min(24),
    );
    frame.render_widget(Clear, popup);
    let state = &app.message_pins;
    let count = state
        .page
        .total
        .map_or_else(|| "cached".to_owned(), |total| total.to_string());
    let block = pane_block(
        format!(
            " Pinned messages · {count} · page {} ",
            state.starts.len().max(1)
        ),
        true,
    );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(inner);
    if state.page.messages.is_empty() {
        let label = if state.loading {
            "Loading pinned messages…"
        } else if state.page.total.is_none() {
            "No cached pins · connect to synchronize"
        } else {
            "No pinned messages"
        };
        frame.render_widget(Paragraph::new(label).wrap(Wrap { trim: true }), rows[0]);
    } else {
        let items = state
            .page
            .messages
            .iter()
            .map(|message| {
                let text = message.preview_text();
                ListItem::new(truncate_cells(
                    &sanitize_terminal_line(&format!("#{} {}: {text}", message.id, message.sender)),
                    usize::from(rows[0].width.saturating_sub(2)),
                ))
            })
            .collect::<Vec<_>>();
        let mut selection = ListState::default().with_selected(Some(state.selected));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("› ")
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            rows[0],
            &mut selection,
        );
    }
    let hint = |action| app.keymap.hint(Context::Pins, action);
    let status = state.error.clone().unwrap_or_else(|| {
        format!(
            "{} unpin · {} unpin all{}",
            hint("pin"),
            hint("unpin_all"),
            if state.loading { " · syncing…" } else { "" }
        )
    });
    frame.render_widget(
        Paragraph::new(format!(
            "{} open · {} close\n{} previous · {} next\n{}",
            hint("open"),
            hint("cancel"),
            hint("pins_previous"),
            hint("pins_more"),
            truncate_cells(&status, usize::from(rows[1].width))
        ))
        .style(Style::default().fg(MUTED)),
        rows[1],
    );
}

pub(super) fn render_prompt(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(prompt) = &app.message_pins.prompt else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(4).min(72),
        area.height.saturating_sub(2).min(12),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(format!(" {} ", prompt.title), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(truncate_cells(&prompt.preview, usize::from(rows[0].width)))
            .style(Style::default().fg(MUTED)),
        rows[0],
    );
    let items = prompt
        .options
        .iter()
        .map(|(label, _)| ListItem::new(label.as_str()))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(prompt.selection));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[1],
        &mut state,
    );
    let status = app
        .message_pins
        .error
        .as_deref()
        .or(app.status_message.as_deref())
        .unwrap_or("Changes apply to Telegram");
    frame.render_widget(
        Paragraph::new(format!(
            "{} confirm · {} cancel\n{}",
            app.keymap.hint(Context::Overlay, "open"),
            app.keymap.hint(Context::Overlay, "cancel"),
            truncate_cells(status, usize::from(rows[2].width))
        ))
        .style(Style::default().fg(MUTED)),
        rows[2],
    );
}
