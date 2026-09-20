use super::{
    AppState, Clear, Constraint, Frame, Layout, List, ListItem, ListState, Paragraph, Rect, Style,
    Wrap, centered, pane_block,
};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(76),
        area.height.saturating_sub(3).min(16),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(" Invitation ".to_owned(), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(2),
        Constraint::Length(3),
    ])
    .split(inner);
    let preview = app.invites.preview.as_ref().map_or_else(
        || "Loading invitation…".to_owned(),
        |p| {
            format!(
                "{}{}\n{}{}",
                p.title,
                p.participants
                    .map_or_else(String::new, |n| format!(" · {n} members")),
                p.about,
                p.warning
                    .as_ref()
                    .map_or_else(String::new, |w| format!("\n{w}"))
            )
        },
    );
    frame.render_widget(Paragraph::new(preview).wrap(Wrap { trim: false }), rows[0]);
    let mut choices = vec![ListItem::new("Cancel / close")];
    if let Some(action) = app.invite_action() {
        choices.push(ListItem::new(action));
    }
    for index in 0..choices.len().min(rows[1].height as usize) {
        app.invites.hit_regions.push((
            rows[1].x,
            rows[1].right(),
            rows[1].y + u16::try_from(index).unwrap_or(0),
            index,
        ));
    }
    frame.render_stateful_widget(
        List::new(choices)
            .highlight_symbol("› ")
            .highlight_style(Style::default().reversed()),
        rows[1],
        &mut ListState::default().with_selected(Some(app.invites.selected)),
    );
    let notice = app
        .invites
        .error
        .as_deref()
        .or_else(|| {
            app.invites
                .preview
                .as_ref()
                .and_then(|p| p.blocked.as_deref())
        })
        .unwrap_or(if app.invite_pending() {
            "Waiting for Telegram…"
        } else {
            "Select an action, then confirm."
        });
    frame.render_widget(Paragraph::new(notice).wrap(Wrap { trim: true }), rows[2]);
}
