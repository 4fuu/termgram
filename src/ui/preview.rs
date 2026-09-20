//! Expanded image view using the existing asynchronous media renderer.
use super::{ACCENT, MUTED, WARNING, clamp_u16};
use crate::{app::AppState, keymap::Context, media::MediaSlot};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect, Size},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    app.media_slots.clear();
    frame.render_widget(Clear, area);
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let title = app
        .selected_message
        .and_then(|id| {
            app.active_messages()
                .iter()
                .find(|message| message.id == id)
        })
        .and_then(|message| message.attachment.as_ref())
        .map_or_else(
            || "Preview".to_owned(),
            |attachment| crate::model::sanitize_terminal_line(attachment.display_name()),
        );
    let block = Block::new()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(rows[0]);
    frame.render_widget(block, rows[0]);
    let status =
        if let (Some(chat_id), Some(message_id)) = (app.active_chat_id, app.selected_message) {
            app.media_slots.push(MediaSlot {
                chat_id,
                message_id,
                viewport: inner,
                offset: 0,
                size: Size::new(inner.width, inner.height),
            });
            app.media_previews
                .get(&(chat_id, message_id))
                .map_or("Loading preview…", |preview| preview.status.as_str())
        } else {
            "This media is no longer available"
        };
    if !status.is_empty() {
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(MUTED)),
            inner,
        );
    }
    let hint = |action| app.keymap.hint(Context::Preview, action);
    let close = format!("{} close ", hint("cancel"));
    let footer = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(clamp_u16(close.width())),
    ])
    .split(rows[1]);
    let controls = app
        .status_message
        .clone()
        .unwrap_or_else(|| format!(" {} reply · {} reveal file", hint("reply"), hint("reveal")));
    frame.render_widget(
        Paragraph::new(controls).style(Style::default().fg(if app.status_message.is_some() {
            WARNING
        } else {
            MUTED
        })),
        footer[0],
    );
    frame.render_widget(
        Paragraph::new(close).style(Style::default().fg(MUTED)),
        footer[1],
    );
}
