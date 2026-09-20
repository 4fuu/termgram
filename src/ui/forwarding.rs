use super::{
    AppState, Clear, Constraint, Frame, Layout, MUTED, Paragraph, Rect, Style, Wrap, centered,
    pane_block, render_notice, wrapped_height,
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(review) = &app.forwarding.review else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(76),
        area.height.saturating_sub(3).min(12),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(" Forward message ".to_owned(), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let target = format!(
        "To: {} · {} · Account {}\nFrom: {} · #{}",
        review.target,
        review.target_title,
        app.active_account(),
        review.source_title,
        review.message
    );
    let rows = Layout::vertical([
        Constraint::Length(
            wrapped_height(&target, inner.width).min(inner.height.saturating_sub(3)),
        ),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);
    render_notice(frame, rows[0], &target, ratatui::style::Color::Reset);
    let preview = review.plan.as_ref().map_or_else(
        || "Loading original message…".to_owned(),
        |plan| {
            let message = &plan.message;
            let body = if message.text.is_empty() {
                message
                    .attachment
                    .as_ref()
                    .map_or("Message", |media| media.display_name())
            } else {
                &message.text
            };
            format!("{}\n{}", message.sender, body)
        },
    );
    frame.render_widget(Paragraph::new(preview).wrap(Wrap { trim: false }), rows[1]);
    let notice = review.error.as_deref().unwrap_or(if app.forward_pending() {
        "Waiting for Telegram…"
    } else {
        "Forwards with original attribution. Your existing drafts are kept."
    });
    frame.render_widget(
        Paragraph::new(notice)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(MUTED)),
        rows[2],
    );
}
