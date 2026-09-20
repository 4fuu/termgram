//! Draft review uses the same table and asynchronous image renderer as chats.
use super::{
    ACCENT, MUTED, WARNING, clamp_u16, pane_block, transcript::human_size, truncate_cells,
};
use crate::{
    app::AppState,
    keymap::Context,
    media::{MediaSlot, MediaSource},
    model::sanitize_terminal_line,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect, Size},
    style::Style,
    text::Line,
    widgets::{Cell, Clear, Paragraph, Row, Table, TableState},
};

#[allow(clippy::too_many_lines)]
pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    app.media_slots.clear();
    frame.render_widget(Clear, area);
    let rows = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(u16::from(app.status_message.is_some())),
        Constraint::Length(1),
    ])
    .split(area);
    let count = app.draft_attachments().len();
    let chat = app
        .active_chat()
        .map_or("Conversation", |chat| chat.title.as_str());
    let block = pane_block(format!(" {chat} · {count} attachments "), true);
    let inner = block.inner(rows[0]);
    frame.render_widget(block, rows[0]);
    let selected = app.attachment_draft.selected.min(count.saturating_sub(1));
    if app.attachment_draft.preview {
        if let Some(file) = app.draft_attachments().get(selected) {
            let path = file.path.clone();
            app.media_slots.push(MediaSlot {
                source: MediaSource::File(path),
                viewport: inner,
                offset: 0,
                size: Size::new(inner.width, inner.height),
            });
        }
    } else if count == 0 {
        frame.render_widget(
            Paragraph::new(if app.preparing_attachments() {
                "Preparing files…"
            } else {
                "No attachments. Add files, then review them before sending."
            })
            .style(Style::default().fg(MUTED)),
            inner,
        );
    } else {
        let start = selected
            .saturating_add(1)
            .saturating_sub(usize::from(inner.height));
        let end = count.min(start + usize::from(inner.height));
        let items = app.draft_attachments()[start..end]
            .iter()
            .map(|file| {
                let name = file.path.file_name().unwrap_or_default().to_string_lossy();
                Row::new([
                    Cell::from(truncate_cells(
                        &sanitize_terminal_line(&name),
                        usize::from(inner.width.saturating_sub(22)),
                    )),
                    Cell::from(if file.as_photo { "Photo" } else { "File" })
                        .style(Style::default().fg(ACCENT)),
                    Cell::from(Line::from(human_size(file.size)).alignment(Alignment::Right))
                        .style(Style::default().fg(MUTED)),
                ])
            })
            .collect::<Vec<_>>();
        let table = Table::new(
            items,
            [
                Constraint::Min(1),
                Constraint::Length(5),
                Constraint::Length(10),
            ],
        )
        .column_spacing(1)
        .row_highlight_style(Style::default().reversed())
        .highlight_symbol("› ");
        frame.render_stateful_widget(
            table,
            inner,
            &mut TableState::default().with_selected(selected - start),
        );
        app.attachment_draft.hit_regions = (start..end)
            .map(|index| {
                (
                    inner.x,
                    inner.right(),
                    inner.y.saturating_add(clamp_u16(index - start)),
                    index,
                )
            })
            .collect();
    }
    if let Some(notice) = &app.status_message {
        frame.render_widget(
            Paragraph::new(notice.as_str()).style(Style::default().fg(WARNING)),
            rows[1],
        );
    }
    let hint = |action| app.keymap.hint(Context::Attachments, action);
    let controls = if app.attachment_draft.preview {
        format!("{} back · {} reveal", hint("cancel"), hint("reveal"))
    } else {
        format!(
            "{} add · {} photo/file · {} remove · {} preview · {} caption · {} back",
            hint("attach"),
            hint("attachment_format"),
            hint("remove_attachment"),
            hint("preview"),
            hint("compose"),
            hint("cancel")
        )
    };
    frame.render_widget(
        Paragraph::new(controls).style(Style::default().fg(MUTED)),
        rows[2],
    );
}

pub(super) fn composer_summary(frame: &mut Frame<'_>, inner: Rect, app: &AppState) -> Rect {
    if app.preparing_attachments() || !app.draft_attachments().is_empty() {
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
        let label = if app.preparing_attachments() {
            "Preparing attachments…".to_owned()
        } else {
            format!(
                "{} attachments · {} review",
                app.draft_attachments().len(),
                app.keymap.hint(Context::Compose, "attachments")
            )
        };
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(ACCENT)),
            rows[0],
        );
        rows[1]
    } else {
        inner
    }
}
