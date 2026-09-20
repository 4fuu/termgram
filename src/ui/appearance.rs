use super::{
    AppState, Clear, Constraint, DANGER, Frame, Layout, Line, List, ListItem, ListState, MUTED,
    Modifier, Paragraph, Rect, Span, Style, Wrap, centered, pane_block,
};
use crate::{appearance::TerminalColor, keymap::Context};

pub(super) fn render_colors(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(picker) = &app.color_picker else {
        return;
    };
    let popup = centered(
        area,
        area.width.min(62),
        area.height.saturating_sub(2).min(24),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(format!(" {} ", picker.label), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    let mut items = vec![ListItem::new("Follow configuration")];
    items.extend(TerminalColor::ALL.iter().map(|color| {
        ListItem::new(Line::from(vec![
            Span::styled("■ ", Style::default().fg(color.color())),
            Span::raw(color.label()),
        ]))
    }));
    let mut state = ListState::default().with_selected(Some(picker.selection));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[0],
        &mut state,
    );
    let hint = app.status_message.clone().unwrap_or_else(|| {
        format!(
            "{} apply · {} cancel\nLocal to this Telegram account",
            app.keymap.hint(Context::Overlay, "open"),
            app.keymap.hint(Context::Overlay, "cancel")
        )
    });
    frame.render_widget(
        Paragraph::new(hint)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(if app.status_message.is_some() {
                DANGER
            } else {
                MUTED
            })),
        rows[1],
    );
}
