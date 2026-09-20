//! Author/metadata, reply, body and media hierarchy inspired by Codex history
//! cells. Reuses Termgram's wrapping/actions and Ratatui spans; no copied runtime.
use super::{ACCENT, DANGER, MUTED, SUCCESS, WARNING, icons::Icons, truncate_cells, wrap_cells};
use crate::{
    app::{AppState, AttachmentState, MessageAction},
    model::{Attachment, AttachmentKind, Delivery, Message, MessageButtonKind},
};
use chrono::{Datelike, Local};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::hash::{DefaultHasher, Hash, Hasher};
use unicode_width::UnicodeWidthStr;

pub(super) const GUTTER_WIDTH: u16 = 2;

pub(super) struct RenderedMessage {
    pub lines: Vec<Line<'static>>,
    pub body_height: usize,
    pub body_action: Option<usize>,
    pub action_rows: Vec<(usize, usize)>,
}

pub(super) fn gutter(selected: bool) -> Span<'static> {
    Span::styled(
        if selected { "▎ " } else { "  " },
        Style::default().fg(ACCENT),
    )
}

pub(super) fn render(message: &Message, width: usize, app: &AppState) -> RenderedMessage {
    let selected = app.selected_message == Some(message.id);
    let icons = Icons(app.keymap.nerd_font);
    let actions = app.message_actions(message);
    let body_width = width.saturating_sub(usize::from(GUTTER_WIDTH)).max(1);
    let mut lines = vec![header(
        message,
        width,
        selected,
        app.settings().show_message_ids,
        icons,
    )];
    let mut action_rows = Vec::new();
    if let Some(reply) = &message.reply_to {
        let label = format!(
            "↩ #{} {}",
            reply.message_id,
            reply.sender.as_deref().unwrap_or("unknown")
        );
        let reply_action = actions
            .iter()
            .position(|action| *action == MessageAction::Reply);
        let current = selected && reply_action == Some(app.selected_action);
        for part in wrap_cells(&label, body_width) {
            if let Some(action) = reply_action {
                action_rows.push((lines.len(), action));
            }
            lines.push(Line::from(vec![
                action_gutter(selected, current),
                Span::styled(part, action_style(current)),
            ]));
        }
    }
    if let Some(attachment) = &message.attachment {
        let label = attachment_label(
            attachment,
            app.attachment_state(message.chat_id, message.id),
            icons,
        );
        let current =
            selected && actions.get(app.selected_action) == Some(&MessageAction::Attachment);
        for part in wrap_cells(&label, body_width) {
            lines.push(Line::from(vec![
                action_gutter(selected, current),
                Span::styled(part, action_style(current)),
            ]));
        }
    }
    if !message.text.is_empty() || message.attachment.is_none() {
        for part in wrap_cells(&message.text, body_width) {
            lines.push(Line::from(vec![gutter(selected), Span::raw(part)]));
        }
    }
    let body_height = lines.len();
    let body_action = actions
        .iter()
        .position(|action| matches!(action, MessageAction::Attachment | MessageAction::Reply));
    action_rows.extend(append_message_actions(
        &mut lines,
        message,
        &actions,
        selected,
        app.selected_action,
        body_width,
        if selected { "▎ " } else { "  " },
    ));
    RenderedMessage {
        lines,
        body_height,
        body_action,
        action_rows,
    }
}

fn action_gutter(selected: bool, current: bool) -> Span<'static> {
    if current {
        Span::styled("› ", Style::default().fg(ACCENT).bold())
    } else {
        gutter(selected)
    }
}

fn action_style(current: bool) -> Style {
    let style = Style::default().fg(ACCENT);
    if current { style.bold() } else { style }
}

fn header(
    message: &Message,
    width: usize,
    selected: bool,
    ids: bool,
    icons: Icons,
) -> Line<'static> {
    let timestamp = message.timestamp.with_timezone(&Local);
    let now = Local::now();
    let time = if timestamp.date_naive() == now.date_naive() {
        timestamp.format("%H:%M:%S").to_string()
    } else if timestamp.year() != now.year() {
        timestamp.format("%Y-%m-%d %H:%M").to_string()
    } else {
        timestamp.format("%m-%d %H:%M").to_string()
    };
    let mut metadata = Vec::new();
    if ids {
        metadata.push(Span::styled(
            format!("#{} · ", message.id),
            Style::default().fg(MUTED),
        ));
    }
    if message.outgoing {
        let (mark, color) = match message.delivery {
            Delivery::Pending => ("…", WARNING),
            Delivery::Sent => ("✓", MUTED),
            Delivery::Read => ("✓✓", SUCCESS),
            Delivery::Failed => ("! failed", DANGER),
        };
        metadata.push(Span::styled(
            format!("{mark} · "),
            Style::default().fg(color),
        ));
    }
    metadata.push(Span::styled(time, Style::default().fg(MUTED)));
    let pin = if message.pinned {
        format!("{} ", icons.pin())
    } else {
        String::new()
    };
    let mut metadata_width = metadata.iter().map(Span::width).sum::<usize>();
    if ids && metadata_width + pin.width() + usize::from(GUTTER_WIDTH) + 4 > width {
        metadata_width = metadata_width.saturating_sub(metadata.remove(0).width());
    }
    let sender = truncate_cells(
        &message.sender,
        width.saturating_sub(usize::from(GUTTER_WIDTH) + pin.width() + metadata_width + 1),
    );
    let gap = width
        .saturating_sub(usize::from(GUTTER_WIDTH) + pin.width() + sender.width() + metadata_width);
    let mut spans = vec![
        gutter(selected),
        Span::styled(pin, Style::default().fg(ACCENT)),
        Span::styled(sender, Style::default().fg(author_color(message)).bold()),
        Span::raw(" ".repeat(gap)),
    ];
    spans.extend(metadata);
    Line::from(spans)
}

fn author_color(message: &Message) -> Color {
    const PALETTE: [Color; 6] = [
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Blue,
        Color::LightCyan,
        Color::LightMagenta,
    ];
    if message.outgoing {
        return Color::Green;
    }
    let mut hash = DefaultHasher::new();
    message.sender.hash(&mut hash);
    PALETTE[usize::try_from(hash.finish() % 6).unwrap_or_default()]
}

fn attachment_label(attachment: &Attachment, state: AttachmentState, icons: Icons) -> String {
    if attachment.kind == AttachmentKind::Sticker {
        return format!(
            "{}{}  {}",
            icons.attachment(attachment.kind),
            attachment.fallback_emoji.as_deref().unwrap_or("◻"),
            if attachment.preview_uses_thumbnail() {
                "[sticker · static preview]"
            } else {
                "[sticker]"
            }
        );
    }
    let kind = match attachment.kind {
        AttachmentKind::Photo => "photo",
        AttachmentKind::File => "file",
        AttachmentKind::Video => "video",
        AttachmentKind::Audio => "audio",
        AttachmentKind::Sticker => "sticker",
        AttachmentKind::Other => "attachment",
    };
    let mut label = format!("{}[{kind}]", icons.attachment(attachment.kind));
    if let Some(name) = &attachment.file_name {
        label.push(' ');
        label.push_str(name);
    }
    if let Some(size) = attachment.size {
        label.push_str(" · ");
        label.push_str(&human_size(size));
    }
    match state {
        AttachmentState::Ready => {}
        AttachmentState::Downloading => label.push_str(" · downloading…"),
        AttachmentState::Downloaded => label.push_str(" · downloaded"),
    }
    label
}

#[allow(clippy::too_many_arguments)]
fn append_message_actions(
    result: &mut Vec<Line<'static>>,
    message: &Message,
    actions: &[MessageAction],
    selected: bool,
    selected_action: usize,
    body_width: usize,
    continuation: &str,
) -> Vec<(usize, usize)> {
    let mut action_rows = Vec::new();
    for (link_index, link) in message.links.iter().enumerate() {
        let Some(action_index) = actions
            .iter()
            .position(|action| *action == MessageAction::Link(link_index))
        else {
            continue;
        };
        let label = if link.label == link.url {
            format!("↗ {}", link.url)
        } else {
            format!("↗ {} → {}", link.label, link.url)
        };
        push_action_lines(
            result,
            &mut action_rows,
            continuation,
            &label,
            body_width,
            action_index,
            selected && selected_action == action_index,
            true,
        );
    }
    for (button_index, button) in message.buttons.iter().enumerate() {
        let action_index = actions
            .iter()
            .position(|action| *action == MessageAction::Button(button_index));
        let (icon, suffix) = match button.kind {
            MessageButtonKind::Url => ("↗", ""),
            MessageButtonKind::Callback => ("●", ""),
            MessageButtonKind::Game => ("▶", ""),
            MessageButtonKind::Unsupported => ("×", " · graphical client required"),
        };
        let label = format!("{icon} [ {} ]{suffix}", button.label);
        if let Some(action_index) = action_index {
            push_action_lines(
                result,
                &mut action_rows,
                continuation,
                &label,
                body_width,
                action_index,
                selected && selected_action == action_index,
                button.kind == MessageButtonKind::Url,
            );
        } else {
            result.push(Line::from(vec![
                Span::styled(continuation.to_owned(), Style::default().fg(MUTED)),
                Span::styled(label, Style::default().fg(MUTED)),
            ]));
        }
    }
    action_rows
}

#[allow(clippy::too_many_arguments)]
fn push_action_lines(
    lines: &mut Vec<Line<'static>>,
    action_rows: &mut Vec<(usize, usize)>,
    prefix: &str,
    label: &str,
    width: usize,
    action_index: usize,
    selected: bool,
    underlined: bool,
) {
    for part in wrap_cells(label, width) {
        let row = lines.len();
        let mut prefix_style = Style::default().fg(MUTED);
        let mut action_style = Style::default().fg(ACCENT);
        if underlined {
            action_style = action_style.add_modifier(Modifier::UNDERLINED);
        }
        if selected {
            prefix_style = prefix_style.fg(ACCENT).bold();
            action_style = action_style.bold();
        }
        lines.push(Line::from(vec![
            Span::styled(prefix.to_owned(), prefix_style),
            Span::styled(part, action_style),
        ]));
        action_rows.push((row, action_index));
    }
}

fn human_size(size: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if size >= GIB {
        decimal_size(size, GIB, "GiB")
    } else if size >= MIB {
        decimal_size(size, MIB, "MiB")
    } else if size >= KIB {
        decimal_size(size, KIB, "KiB")
    } else {
        format!("{size} B")
    }
}

fn decimal_size(size: u64, unit: u64, suffix: &str) -> String {
    let whole = size / unit;
    let decimal = size % unit * 10 / unit;
    format!("{whole}.{decimal} {suffix}")
}
