//! Aligned semantic columns using Ratatui's Table; no custom table layout engine.
use super::{ACCENT, MUTED, clamp_u16, icons::Icons, pane_block, truncate_cells};
use crate::{
    app::{AppState, Focus, Mode},
    appearance::Target,
};
use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, HighlightSpacing, Paragraph, Row, Table, TableState},
};
use unicode_width::UnicodeWidthStr;

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    app.set_chat_pane_region((area.x, area.right(), area.y, area.bottom()));
    let focused = app.focus == Focus::Chats && app.mode != Mode::Compose;
    let icons = Icons(app.keymap.nerd_font);
    let folder = app.folders.iter().find(|folder| folder.id == app.folder_id);
    let title = if app.mode == Mode::Filter {
        format!("Chats · /{}", app.filter.value())
    } else {
        format!(
            "{}{}",
            icons.folder(app.folder_id),
            folder.map_or("Chats", |folder| folder.title.as_str())
        )
    };
    let block = pane_block(
        format!(
            " {} ",
            truncate_cells(&title, usize::from(area.width.saturating_sub(4)))
        ),
        focused,
    )
    .title_style(Style::default().fg(app.color(Target::Folder(app.folder_id))));
    let visible = app.filtered_chat_indices();
    if visible.is_empty() {
        app.set_chat_hit_regions(Vec::new());
        frame.render_widget(
            Paragraph::new(if app.mode == Mode::Filter {
                "No chats match"
            } else {
                "No conversations"
            })
            .style(Style::default().fg(MUTED))
            .block(block),
            area,
        );
        return;
    }
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let selected = app.selected_chat.min(visible.len().saturating_sub(1));
    let start = selected.saturating_add(1).saturating_sub(viewport_height);
    let end = start.saturating_add(viewport_height).min(visible.len());
    // Borders (2), selection marker (2), time (5), unread (4), column gaps (2).
    let title_width = usize::from(area.width.saturating_sub(15));
    let now = Local::now();
    let rows = visible[start..end]
        .iter()
        .enumerate()
        .map(|(offset, &index)| {
            row(
                app,
                &app.chats[index],
                title_width,
                focused,
                start + offset == selected,
                now,
            )
        })
        .collect::<Vec<_>>();
    let regions = (start..end)
        .enumerate()
        .map(|(offset, position)| {
            (
                area.x.saturating_add(1),
                area.right().saturating_sub(1),
                area.y.saturating_add(1).saturating_add(clamp_u16(offset)),
                position,
            )
        })
        .collect();
    app.set_chat_hit_regions(regions);
    let mut state = TableState::default().with_selected(Some(selected.saturating_sub(start)));
    let table = Table::new(
        rows,
        [
            Constraint::Min(0),
            Constraint::Length(5),
            Constraint::Length(4),
        ],
    )
    .block(block)
    .column_spacing(1)
    .highlight_spacing(HighlightSpacing::Always)
    .row_highlight_style(Style::default().bold())
    .highlight_symbol(Span::styled(
        "› ",
        Style::default().fg(if focused { ACCENT } else { MUTED }),
    ));
    frame.render_stateful_widget(table, area, &mut state);
}

fn row(
    app: &AppState,
    chat: &crate::model::Chat,
    title_width: usize,
    focused: bool,
    is_selected: bool,
    now: chrono::DateTime<Local>,
) -> Row<'static> {
    let icons = Icons(app.keymap.nerd_font);
    let pin = if app.chat_pin_position(chat.id).is_some() {
        format!("{} ", icons.pin())
    } else {
        String::new()
    };
    let icon = icons.chat(chat.kind);
    let muted = if chat.membership.mute_until > now.timestamp() {
        icons.muted()
    } else {
        ""
    };
    let title = truncate_cells(
        &chat.title,
        title_width.saturating_sub(icon.width() + pin.width() + muted.width()),
    );
    let mut title_style = Style::default().fg(app.color(Target::Chat(chat.id)));
    if chat.unread > 0 || chat.membership.unread_mark || is_selected {
        title_style = title_style.bold();
    }
    if is_selected && focused {
        title_style = title_style.add_modifier(Modifier::UNDERLINED);
    }
    let count = match chat.unread {
        0 if chat.membership.unread_mark => "•".to_owned(),
        0 => String::new(),
        1..=999 => chat.unread.to_string(),
        _ => "999+".to_owned(),
    };
    Row::new([
        Cell::from(Line::from(vec![
            Span::styled(icon, Style::default().fg(MUTED)),
            Span::styled(pin, Style::default().fg(ACCENT)),
            Span::styled(muted, Style::default().fg(MUTED)),
            Span::styled(title, title_style),
        ])),
        Cell::from(Line::from(chat.activity_label(now)).alignment(Alignment::Right))
            .style(Style::default().fg(app.keymap.sidebar.time_color.color())),
        Cell::from(Line::from(count).alignment(Alignment::Right)).style(
            Style::default()
                .fg(app.keymap.sidebar.unread_color.color())
                .bold(),
        ),
    ])
}
