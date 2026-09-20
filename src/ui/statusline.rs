//! Small, ordered status segments inspired by Yazi's status.lua and Codex's
//! finite status-line items. Ratatui owns cell layout; no upstream code is copied.

use super::{ACCENT, DANGER, MUTED, SUCCESS, WARNING, clamp_u16, truncate_cells};
use crate::{
    app::{AppState, Focus, Mode},
    event::ConnectionStatus,
    keymap::Context,
    statusline::Item,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

struct Segment {
    text: String,
    style: Style,
    priority: u8,
    right: bool,
}

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    if area.is_empty() || !app.keymap.statusline.enabled {
        return;
    }
    let mut segments = app
        .keymap
        .statusline
        .left
        .iter()
        .map(|item| (*item, false))
        .chain(app.keymap.statusline.right.iter().map(|item| (*item, true)))
        .filter_map(|(item, right)| segment(item, right, app, area.width < 96))
        .collect::<Vec<_>>();
    let width = usize::from(area.width);
    while total_width(&segments) > width {
        let Some((index, entry)) = segments
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| entry.priority)
        else {
            break;
        };
        if entry.priority >= 90 {
            let excess = total_width(&segments).saturating_sub(width);
            segments[index].text =
                truncate_cells(&entry.text, entry.text.width().saturating_sub(excess));
            break;
        }
        segments.remove(index);
    }
    let right_width = segments
        .iter()
        .filter(|segment| segment.right)
        .map(|segment| segment.text.width() + 1)
        .sum::<usize>();
    let columns = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(clamp_u16(right_width.min(width))),
    ])
    .split(area);
    for (right, column) in [(false, columns[0]), (true, columns[1])] {
        let mut remaining = usize::from(column.width);
        let mut spans = Vec::new();
        for segment in segments.iter().filter(|segment| segment.right == right) {
            if remaining == 0 {
                break;
            }
            let text = truncate_cells(&segment.text, remaining.saturating_sub(1));
            remaining = remaining.saturating_sub(text.width() + 1);
            spans.push(Span::styled(text, segment.style));
            spans.push(Span::raw(" "));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), column);
    }
}

fn total_width(segments: &[Segment]) -> usize {
    segments
        .iter()
        .map(|segment| segment.text.width() + 1)
        .sum()
}

fn segment(item: Item, right: bool, app: &AppState, narrow: bool) -> Option<Segment> {
    let (text, color, priority) = match item {
        Item::Mode => (
            match app.mode {
                Mode::Compose => " INSERT ",
                Mode::Filter | Mode::Search => " SEARCH ",
                Mode::Navigate if app.focus == Focus::Chats => " CHATS ",
                Mode::Navigate if app.selected_message.is_some() => " SELECT ",
                Mode::Navigate => " NORMAL ",
                _ => " VIEW ",
            }
            .to_owned(),
            ACCENT,
            100,
        ),
        Item::App => ("Termgram".to_owned(), ACCENT, 10),
        Item::Account => (
            format!(
                "{} · {}",
                app.active_account(),
                app.user_name.as_deref().unwrap_or("Telegram")
            ),
            Color::Reset,
            60,
        ),
        Item::Connection => {
            let (label, color) = match app.connection {
                ConnectionStatus::Connecting => ("connecting", WARNING),
                ConnectionStatus::Online => ("online", SUCCESS),
                ConnectionStatus::Reconnecting => ("reconnecting", WARNING),
                ConnectionStatus::Offline => ("offline", DANGER),
            };
            (label.to_owned(), color, 80)
        }
        Item::Latency => (
            app.metrics
                .latency
                .filter(|sample| !sample.expired() && app.connection == ConnectionStatus::Online)
                .map_or_else(
                    || "ping —".to_owned(),
                    |sample| format!("{} ms", sample.elapsed.as_millis()),
                ),
            MUTED,
            30,
        ),
        Item::Dc => (
            app.metrics
                .dc_id
                .filter(|_| app.connection == ConnectionStatus::Online)
                .map_or_else(|| "DC —".to_owned(), |dc| format!("DC {dc}")),
            MUTED,
            20,
        ),
        Item::Position => {
            let text = if app.active_chat_id.is_none() {
                String::new()
            } else if app.message_scroll == 0 {
                "live".to_owned()
            } else if app.new_messages_while_scrolled > 0 {
                format!("+{} new", app.new_messages_while_scrolled)
            } else {
                format!("↑{} rows", app.message_scroll)
            };
            (text, MUTED, 40)
        }
        Item::Context => (context(app, narrow), MUTED, 90),
    };
    if text.is_empty() {
        return None;
    }
    let style = if item == Item::Mode {
        Style::default()
            .bg(if app.mode == Mode::Compose {
                Color::Green
            } else {
                Color::Blue
            })
            .fg(Color::Black)
            .bold()
    } else {
        Style::default().fg(color)
    };
    Some(Segment {
        text,
        style,
        priority,
        right,
    })
}

fn context(app: &AppState, narrow: bool) -> String {
    if app.status_message.is_some() {
        return String::new();
    }
    if app.mode == Mode::Navigate
        && app.focus == Focus::Conversation
        && app.selected_message.is_some()
    {
        let hint = |action| app.keymap.hint(Context::Conversation, action);
        let media = app
            .active_messages()
            .iter()
            .find(|message| Some(message.id) == app.selected_message)
            .and_then(|message| message.attachment.as_ref())
            .is_some_and(crate::model::Attachment::supports_preview);
        return if media {
            format!("{} preview · {} reply", hint("preview"), hint("compose"))
        } else {
            format!("{} reply · {} clear", hint("compose"), hint("cancel"))
        };
    }
    if let Some(version) = app.available_update() {
        return format!("Update {version} available · run tg update");
    }
    if app.mode != Mode::Navigate {
        return String::new();
    }
    let context = if app.focus == Focus::Chats {
        Context::Chats
    } else {
        Context::Conversation
    };
    let back = if narrow && app.narrow_conversation {
        format!(
            "{} chats · ",
            app.keymap.hint(Context::Conversation, "cancel")
        )
    } else {
        String::new()
    };
    format!("{back}{} help", app.keymap.hint(context, "help"))
}
