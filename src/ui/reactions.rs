use super::{ACCENT, MUTED, centered, pane_block, truncate_cells, wrap_cells};
use crate::{
    app::{AppState, MessageAction},
    model::Message,
    reactions::{Kind, Summary},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

pub(super) fn transcript(message: &Message, width: usize) -> Vec<super::entities::Row> {
    let Some(summary) = &message.reactions else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let mut line = Line::default();
    for count in summary.counts.iter().filter(|count| count.count > 0) {
        let mine = count.chosen.is_some();
        let label = format!(
            "{}{} {}  ",
            if mine { "● " } else { "" },
            count.kind.label(),
            count.count
        );
        let label = truncate_cells(&label, width);
        let span = Span::styled(
            label,
            if mine {
                Style::default().fg(ACCENT).bold()
            } else {
                Style::default().fg(MUTED)
            },
        );
        if line.width() + span.width() > width && !line.spans.is_empty() {
            rows.push(super::entities::Row {
                line,
                action: Some(MessageAction::Reactions),
            });
            line = Line::default();
        }
        line.spans.push(span);
    }
    if !line.spans.is_empty() {
        rows.push(super::entities::Row {
            line,
            action: Some(MessageAction::Reactions),
        });
    }
    rows
}

pub(super) fn status(summary: &Summary) -> String {
    format!(
        "{} {}{}",
        summary
            .counts
            .iter()
            .map(|count| u64::from(count.count))
            .sum::<u64>(),
        if summary.as_tags { "tags" } else { "reactions" },
        if summary.stale { " · refreshing" } else { "" }
    )
}

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let Some(panel) = &mut app.reactions.panel else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(70),
        area.height.saturating_sub(2).min(24),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(
        format!(" Reactions · {} · #{} ", panel.title, panel.message),
        true,
    );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let notice = notice(panel);
    let width = usize::from(inner.width.max(1));
    let notice_height = inner
        .height
        .saturating_sub(2)
        .min(super::clamp_u16(wrap_cells(&notice, width).len()));
    frame.render_widget(
        Paragraph::new(
            wrap_cells(&notice, width)
                .into_iter()
                .map(|line| Line::styled(line, Style::default().fg(MUTED)))
                .collect::<Vec<_>>(),
        ),
        Rect {
            height: notice_height,
            ..inner
        },
    );
    let body = Rect {
        y: inner.y + notice_height,
        height: inner.height.saturating_sub(notice_height),
        ..inner
    };
    if panel.selected < panel.top {
        panel.top = panel.selected;
    }
    if panel.selected >= panel.top + usize::from(body.height) {
        panel.top = (panel.selected + 1).saturating_sub(usize::from(body.height));
    }
    app.reactions.hit_rows.clear();
    let lines: Vec<_> = panel
        .choices
        .iter()
        .enumerate()
        .skip(panel.top)
        .take(usize::from(body.height))
        .map(|(index, choice)| {
            let count = panel.review.as_ref().and_then(|review| {
                review
                    .summary
                    .counts
                    .iter()
                    .find(|count| count.kind == Kind::Emoji(choice.emoji.clone()))
            });
            let mine = count.is_some_and(|count| count.chosen.is_some());
            let label = format!(
                "{} {} · {}{}",
                if mine { "●" } else { "○" },
                crate::model::sanitize_terminal_line(&choice.emoji),
                choice.title,
                count.map_or_else(String::new, |count| format!(" · {}", count.count))
            );
            app.reactions.hit_rows.push((
                body.x,
                body.right(),
                body.y + super::clamp_u16(index - panel.top),
                index,
            ));
            let style = if mine {
                Style::default().fg(ACCENT).bold()
            } else {
                Style::default()
            };
            Line::styled(
                truncate_cells(&label, width),
                if index == panel.selected {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                },
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
}

fn notice(panel: &crate::app::reactions::Panel) -> String {
    panel.error.clone().unwrap_or_else(|| {
        if panel.loading.is_some() {
            "Loading available reactions…".to_owned()
        } else if panel.sending.is_some() {
            "Saving reactions…".to_owned()
        } else if let Some(review) = &panel.review {
            review.read_only.clone().unwrap_or_else(|| {
                if panel.choices.is_empty() {
                    "This chat has no available emoji reactions".to_owned()
                } else {
                    format!(
                        "{} per message · oldest choice is replaced at the limit",
                        review.max_chosen
                    )
                }
            })
        } else {
            "Refresh to load reactions".to_owned()
        }
    })
}
