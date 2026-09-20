use super::{
    AppState, Clear, Constraint, DANGER, Frame, Layout, List, ListItem, ListState, MUTED,
    Paragraph, Rect, Style, Wrap, centered, pane_block,
};
use ratatui::text::{Line, Span};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(prompt) = &app.deletion.prompt else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(76),
        area.height.saturating_sub(3).min(13),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(
        format!(" Delete #{} · {} ", prompt.message, prompt.title),
        true,
    );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    let preview = prompt.plan.as_ref().map_or_else(
        || "Loading message and available scopes…".to_owned(),
        |plan| {
            let message = &plan.message;
            let body = if message.text.is_empty() {
                message
                    .attachment
                    .as_ref()
                    .map_or("Message", |file| file.display_name())
            } else {
                &message.text
            };
            format!(
                "{} · {}\n{}",
                message.sender,
                message.timestamp.format("%Y-%m-%d %H:%M UTC"),
                body
            )
        },
    );
    frame.render_widget(
        Paragraph::new(preview)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(MUTED)),
        rows[0],
    );
    let mut options = vec![ListItem::new("Cancel")];
    if let Some(plan) = &prompt.plan {
        options.extend(
            plan.scopes
                .iter()
                .map(|scope| ListItem::new(scope.label()).style(Style::default().fg(DANGER))),
        );
    }
    let mut selection = ListState::default().with_selected(Some(prompt.selected));
    frame.render_stateful_widget(
        List::new(options)
            .highlight_symbol("› ")
            .highlight_style(Style::default().reversed()),
        rows[1],
        &mut selection,
    );
    let notice = prompt
        .error
        .as_deref()
        .unwrap_or(if app.deletion_pending() {
            "Waiting for Telegram…"
        } else {
            "Deletion cannot be undone. Choose the scope before confirming."
        });
    frame.render_widget(
        Paragraph::new(vec![Line::from(Span::styled(
            notice,
            Style::default().fg(MUTED),
        ))])
        .wrap(Wrap { trim: true }),
        rows[2],
    );
}
