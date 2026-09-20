use super::{
    ACCENT, AppState, Clear, DANGER, Frame, MUTED, Paragraph, Position, Rect, Style, centered,
    clamp_u16, pane_block, truncate_cells,
};
use crate::event::ConnectionStatus;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let candidates = app.command_candidates();
    let notice = app.commands.notice.clone();
    let bottom = area
        .bottom()
        .saturating_sub(u16::from(app.keymap.statusline.enabled));
    let max_rows = area.height.saturating_sub(6).min(8);
    let count = clamp_u16(candidates.len()).min(max_rows);
    let height = 3 + count + u16::from(notice.is_some());
    let popup = Rect::new(area.x, bottom.saturating_sub(height), area.width, height);
    frame.render_widget(Clear, popup);
    let block = pane_block(format!(" : · {} ", app.command_target_label()), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let start = app
        .commands
        .selected
        .unwrap_or(0)
        .saturating_add(1)
        .saturating_sub(usize::from(count));
    for (offset, (index, candidate)) in candidates
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(count))
        .enumerate()
    {
        let row = Rect::new(inner.x, inner.y + clamp_u16(offset), inner.width, 1);
        let prefix = if app.commands.selected == Some(index) {
            "› "
        } else {
            "  "
        };
        let width = usize::from(inner.width);
        let label_width = (width / 3).clamp(8, 28);
        let label = truncate_cells(&candidate.label, label_width);
        let shortcut = if candidate.shortcut == "unbound" || candidate.shortcut.is_empty() {
            String::new()
        } else {
            format!("  {}", candidate.shortcut)
        };
        let description_width = width.saturating_sub(2 + label_width + 2 + shortcut.width());
        let description = truncate_cells(&candidate.description, description_width);
        let mut style = Style::default();
        if app.commands.selected == Some(index) {
            style = style.reversed();
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(
                        "{prefix}{label}{}  ",
                        " ".repeat(label_width.saturating_sub(label.width()))
                    ),
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    description,
                    Style::default().fg(if candidate.unavailable { DANGER } else { MUTED }),
                ),
                Span::styled(shortcut, Style::default().fg(MUTED)),
            ]))
            .style(style),
            row,
        );
        app.commands
            .hit_regions
            .push((row.x, row.right(), row.y, index));
    }
    if let Some(notice) = notice {
        frame.render_widget(
            Paragraph::new(truncate_cells(&notice, usize::from(inner.width)))
                .style(Style::default().fg(DANGER)),
            Rect::new(inner.x, inner.y + count, inner.width, 1),
        );
    }
    let input_row = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
    frame.render_widget(
        Paragraph::new(Span::styled(":", Style::default().fg(ACCENT).bold())),
        Rect::new(input_row.x, input_row.y, 1, 1),
    );
    let input = &app.commands.input;
    let scroll = input
        .cursor_display_width()
        .saturating_sub(usize::from(input_row.width.saturating_sub(2)));
    frame.render_widget(
        Paragraph::new(input.value()).scroll((0, clamp_u16(scroll))),
        Rect::new(
            input_row.x + 1,
            input_row.y,
            input_row.width.saturating_sub(1),
            1,
        ),
    );
    frame.set_cursor_position(Position::new(
        input_row.x + 1 + clamp_u16(input.cursor_display_width().saturating_sub(scroll)),
        input_row.y,
    ));
}

pub(super) fn render_status(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let popup = centered(
        area,
        area.width.min(86),
        area.height.saturating_sub(3).min(28),
    );
    frame.render_widget(Clear, popup);
    let online = app.connection == ConnectionStatus::Online;
    let unavailable = "not available";
    let dc = app
        .metrics
        .dc_id
        .filter(|_| online)
        .map_or_else(|| unavailable.to_owned(), |dc| dc.to_string());
    let latency = app
        .metrics
        .latency
        .filter(|sample| online && !sample.expired())
        .map_or_else(
            || unavailable.to_owned(),
            |sample| {
                format!(
                    "{} ms · sampled {} s ago",
                    sample.elapsed.as_millis(),
                    sample.started.elapsed().as_secs()
                )
            },
        );
    let mut lines = vec![
        format!(
            "Account {} · {}",
            app.active_account(),
            app.user_name.as_deref().unwrap_or("Telegram")
        ),
        format!("Connection: {:?}", app.connection),
        format!("DC: {dc}"),
        format!("Ping: {latency}"),
        "Ping includes SDK scheduling and retries; it is not delivery latency.".to_owned(),
        String::new(),
        format!(
            "Dialogs in memory: {} · folders: {}",
            app.chats.len(),
            app.folders.len()
        ),
        format!(
            "Messages in memory: {}",
            app.messages.values().map(Vec::len).sum::<usize>()
        ),
        format!(
            "Current history: {}",
            if app.loading_history {
                "loading"
            } else {
                "idle"
            }
        ),
        "Local search reports the persisted cache's coverage separately.".to_owned(),
    ];
    lines.extend(configuration_lines(app));
    let block = pane_block(" Status ".to_owned(), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let wrapped = lines
        .iter()
        .flat_map(|line| super::wrap_cells(line, usize::from(inner.width.max(1))))
        .map(Line::from)
        .collect::<Vec<_>>();
    app.configuration.status_scroll = app
        .configuration
        .status_scroll
        .min(wrapped.len().saturating_sub(usize::from(inner.height)));
    frame.render_widget(
        Paragraph::new(wrapped).scroll((clamp_u16(app.configuration.status_scroll), 0)),
        inner,
    );
}

fn configuration_lines(app: &AppState) -> Vec<String> {
    let state = &app.configuration;
    let mut lines = vec![
        String::new(),
        format!(
            "Lua configuration: {}",
            state.path.as_deref().map_or_else(
                || "not available".to_owned(),
                |p| crate::model::sanitize_terminal_line(&p.display().to_string())
            )
        ),
        format!(
            "Configuration revision: {}{}",
            state.revision,
            if state.loading { " · reloading" } else { "" }
        ),
        "Use :config reload or :reload to apply changes.".to_owned(),
        format!("Nerd Font icons: {}", app.keymap.nerd_font),
        format!(
            "Latency probes: {}",
            app.keymap.statusline.measures_latency()
        ),
        format!(
            "Terminal media paste: configured {} · supported {}",
            app.keymap.attachments.terminal_clipboard, state.terminal_clipboard
        ),
        format!(
            "Desktop alerts: {} · {:?} · {:?}",
            app.keymap.notifications.enabled,
            app.keymap.notifications.backend,
            app.keymap.notifications.when
        ),
        format!(
            "Alert previews: {} · sound: {}",
            app.keymap.notifications.previews, app.keymap.notifications.sound
        ),
    ];
    if let Some(error) = &state.error {
        lines.push(format!("Last configuration error: {error}"));
    }
    if let Some(at) = state.loaded_at {
        lines.push(format!("Last reload: {} s ago", at.elapsed().as_secs()));
    }
    if let Some(chat) = app.active_chat_id {
        lines.push(format!("Open chat: {chat}"));
        if let Some(reason) = app.draft_restriction(chat) {
            lines.push(reason);
        }
    }
    lines.push(String::new());
    lines.push(crate::version_description(false));
    lines
}
