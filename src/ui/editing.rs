use super::{
    AppState, Clear, Frame, MUTED, Paragraph, Position, Rect, Style, centered, editor_lines,
    input_cursor, pane_block,
};
use ratatui::text::{Line, Text};

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let popup = centered(
        area,
        area.width.min(90),
        area.height.saturating_sub(3).min(18),
    );
    frame.render_widget(Clear, popup);
    let title = app.message_edit().map_or_else(
        || " Edit message ".to_owned(),
        |edit| {
            format!(
                " Edit {} #{} · {} ",
                if edit.source.caption {
                    "caption"
                } else {
                    "message"
                },
                edit.source.message_id,
                app.active_chat()
                    .map_or("Conversation", |chat| chat.title.as_str())
            )
        },
    );
    let block = pane_block(title, true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let Some(edit) = app.message_edit() else {
        frame.render_widget(
            Paragraph::new("Loading original…").style(Style::default().fg(MUTED)),
            inner,
        );
        return;
    };
    let input = &edit.input;
    let (row, column) = input_cursor(input, inner.width.max(1));
    let scroll = row.saturating_sub(inner.height.saturating_sub(1));
    let text = Text::from(
        editor_lines(input.value(), inner.width.max(1))
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>(),
    );
    frame.render_widget(Paragraph::new(text).scroll((scroll, 0)), inner);
    if !app.message_edit_pending() && inner.width > 0 && inner.height > 0 {
        frame.set_cursor_position(Position::new(
            inner.x + column.min(inner.width - 1),
            inner.y + row.saturating_sub(scroll).min(inner.height - 1),
        ));
    }
}
