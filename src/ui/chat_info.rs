use super::{
    AppState, Clear, Frame, MUTED, Paragraph, Rect, Style, centered, pane_block, wrap_cells,
};
use crate::{chat_info::Content, event::ConnectionStatus};
use ratatui::text::Line;

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let Some(id) = app.chat_info.view else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(86),
        area.height.saturating_sub(3).min(25),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(" Chat information ".to_owned(), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![format!("Chat ID: {id}")];
    let now = chrono::Utc::now().timestamp();
    if let Some(entry) = app.chat_info.entries.get(&id) {
        if let Some(info) = &entry.info {
            lines.push(info.title.clone());
            if let Some(username) = &info.username {
                lines.push(format!("@{username}"));
            }
            lines.push(format!("Your role: {}", info.role));
            if let Some(members) = info.members {
                lines.push(format!("Members: {members}"));
            }
            lines.push(info.about.clone());
            lines.push(String::new());
            for (label, content) in [
                ("Text", Content::Text),
                ("Photos", Content::Photo),
                ("Files", Content::File),
            ] {
                lines.push(format!(
                    "{label}: {}",
                    info.restriction(content, now)
                        .unwrap_or_else(|| "Allowed by known permissions".to_owned())
                ));
            }
            if info.slow_seconds > 0 {
                lines.push(format!(
                    "Slow mode: {} s between messages",
                    info.slow_seconds
                ));
            }
            lines.push(format!(
                "Details fetched {} s ago{}",
                entry.at.elapsed().as_secs(),
                if app.connection == ConnectionStatus::Online {
                    ""
                } else {
                    " · offline"
                }
            ));
        }
        if let Some(error) = &entry.error {
            lines.push(error.clone());
        }
    } else {
        lines.push(
            if app.connection == ConnectionStatus::Online {
                "Loading chat details…"
            } else {
                "Connect to Telegram to fetch details"
            }
            .to_owned(),
        );
    }
    if let Some(chat) = app.chats.iter().find(|c| c.id == id) {
        lines.push(crate::notifications::mute_label(
            chat.membership.mute_until,
            now,
        ));
    }
    lines.push("Telegram checks current permissions when sending. Drafts are kept when sending is restricted.".to_owned());
    let wrapped: Vec<Line<'static>> = lines
        .iter()
        .flat_map(|line| wrap_cells(line, usize::from(inner.width.max(1))))
        .map(Line::from)
        .collect();
    app.chat_info.scroll = app
        .chat_info
        .scroll
        .min(wrapped.len().saturating_sub(usize::from(inner.height)));
    frame.render_widget(
        Paragraph::new(wrapped)
            .style(Style::default().fg(MUTED))
            .scroll((u16::try_from(app.chat_info.scroll).unwrap_or(u16::MAX), 0)),
        inner,
    );
}
