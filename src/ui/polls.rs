use super::{ACCENT, MUTED, centered, entities, pane_block};
use crate::{
    app::{AppState, MessageAction},
    model::Message,
    polls::Poll,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::collections::BTreeSet;
use unicode_width::UnicodeWidthStr;

pub(super) struct Row {
    pub line: Line<'static>,
    pub option: Option<usize>,
    pub spoiler: bool,
}

pub(super) fn lines(
    poll: &Poll,
    width: usize,
    revealed: bool,
    choices: Option<&BTreeSet<Vec<u8>>>,
) -> Vec<Row> {
    let now = chrono::Utc::now().timestamp();
    let visible = poll.results_visible(now);
    let mut rows = Vec::new();
    for mut row in entities::render_text(
        &poll.definition.question.text,
        &poll.definition.question.entities,
        width,
        revealed,
        true,
    ) {
        row.line.style = row.line.style.bold();
        rows.push(Row {
            line: row.line,
            option: None,
            spoiler: row.action == Some(MessageAction::Spoilers),
        });
    }
    for (index, answer) in poll.definition.answers.iter().enumerate() {
        let count = poll.count(&answer.option);
        let selected = choices.map_or_else(
            || count.is_some_and(|count| count.chosen),
            |choices| choices.contains(&answer.option),
        );
        let correct = visible && count.is_some_and(|count| count.correct);
        let mark = if correct {
            "✓ "
        } else if selected {
            "● "
        } else {
            "○ "
        };
        let color = if correct {
            Color::Green
        } else if selected {
            ACCENT
        } else {
            MUTED
        };
        let result = if visible {
            count
                .and_then(|count| count.voters)
                .map_or_else(String::new, |votes| result_label(votes, poll.results.total))
        } else {
            String::new()
        };
        let available = width.saturating_sub(2 + result.width()).max(1);
        let parts = entities::render_text(
            &answer.text.text,
            &answer.text.entities,
            available,
            revealed,
            true,
        );
        for (part, row) in parts.into_iter().enumerate() {
            let mut line = row.line;
            line.spans.insert(
                0,
                Span::styled(
                    if part == 0 { mark } else { "  " },
                    Style::default().fg(color),
                ),
            );
            if part == 0 && !result.is_empty() {
                line.spans.push(Span::raw(
                    " ".repeat(width.saturating_sub(line.width() + result.width())),
                ));
                line.spans
                    .push(Span::styled(result.clone(), Style::default().fg(MUTED)));
            }
            rows.push(Row {
                line,
                option: Some(index),
                spoiler: row.action == Some(MessageAction::Spoilers),
            });
        }
    }
    if visible && let Some(solution) = &poll.results.solution {
        rows.push(Row {
            line: Line::styled("Explanation", Style::default().fg(ACCENT)),
            option: None,
            spoiler: false,
        });
        rows.extend(
            entities::render_text(&solution.text, &solution.entities, width, revealed, true)
                .into_iter()
                .map(|row| Row {
                    line: row.line,
                    option: None,
                    spoiler: row.action == Some(MessageAction::Spoilers),
                }),
        );
    }
    rows
}

fn result_label(votes: u32, total: Option<u32>) -> String {
    total.filter(|total| *total > 0).map_or_else(
        || format!(" {votes}"),
        |total| {
            format!(
                " {:>3}% · {votes}",
                u64::from(votes) * 100 / u64::from(total)
            )
        },
    )
}

pub(super) fn transcript(message: &Message, width: usize, app: &AppState) -> Vec<Row> {
    message.poll.as_ref().map_or_else(Vec::new, |poll| {
        lines(poll, width, app.spoilers_revealed(message), None)
    })
}

pub(super) fn status(poll: &Poll) -> String {
    let now = chrono::Utc::now().timestamp();
    format!(
        "{} · {} · {}{}{}",
        if poll.definition.quiz { "quiz" } else { "poll" },
        if poll.definition.public_voters {
            "public votes"
        } else {
            "anonymous"
        },
        poll.results.total.map_or_else(
            || "votes unknown".to_owned(),
            |count| format!("{count} voters")
        ),
        if poll.closed(now) {
            " · closed"
        } else if poll.voted() {
            " · voted"
        } else if poll.definition.multiple_choice {
            " · multiple answers"
        } else {
            " · one answer"
        },
        if poll.stale { " · refreshing" } else { "" }
    )
}

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &mut AppState) {
    let revealed = app
        .inspected_message()
        .is_some_and(|message| app.spoilers_revealed(message));
    let Some(review) = &mut app.polls.review else {
        return;
    };
    let popup = centered(
        area,
        area.width.saturating_sub(2).min(84),
        area.height.saturating_sub(2).min(30),
    );
    frame.render_widget(Clear, popup);
    let block = pane_block(format!(" {} · #{} ", review.title, review.message), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    // All content scrolls, including wrapped errors and eligibility notices.
    // Fixed-height footers otherwise hide the reason voting is unavailable.
    let mut rows = info_rows(review, usize::from(inner.width.max(1)));
    rows.extend(lines(
        &review.poll,
        usize::from(inner.width.max(1)),
        revealed,
        Some(&review.choices),
    ));
    if review.follow
        && let Some(row) = rows
            .iter()
            .position(|row| row.option == Some(review.selected))
    {
        if row < review.top {
            review.top = row;
        } else if row >= review.top.saturating_add(usize::from(inner.height)) {
            review.top = row
                .saturating_add(1)
                .saturating_sub(usize::from(inner.height));
        }
    }
    review.follow = false;
    review.top = review
        .top
        .min(rows.len().saturating_sub(usize::from(inner.height)));
    let mut displayed = Vec::new();
    app.polls.hit_rows.clear();
    for (index, mut row) in rows
        .into_iter()
        .skip(review.top)
        .take(usize::from(inner.height))
        .enumerate()
    {
        if let Some(option) = row.option {
            app.polls.hit_rows.push((
                inner.x,
                inner.right(),
                inner.y + super::clamp_u16(index),
                option,
            ));
            if option == review.selected {
                row.line.style = row.line.style.add_modifier(Modifier::REVERSED);
            }
        }
        displayed.push(row.line);
    }
    frame.render_widget(Paragraph::new(displayed), inner);
}

fn info_rows(review: &crate::app::polls::Review, width: usize) -> Vec<Row> {
    let notice = review.error.clone().unwrap_or_else(|| {
        if review.loading.is_some() {
            "Refreshing poll…".to_owned()
        } else if review.sending.is_some() {
            "Saving vote…".to_owned()
        } else if review.dirty && review.choices.is_empty() {
            "Ready to retract your vote".to_owned()
        } else if review.dirty {
            format!(
                "{} answer(s) selected · vote is not sent yet",
                review.choices.len()
            )
        } else if let Some(reason) = review.poll.read_only_reason(chrono::Utc::now().timestamp()) {
            reason.to_owned()
        } else if review.poll.definition.subscribers_only {
            "Eligible subscribers only · Telegram verifies your membership".to_owned()
        } else if !review.poll.definition.countries.is_empty() {
            format!(
                "Voting limited to {}",
                review.poll.definition.countries.join(", ")
            )
        } else {
            "Choose answers, then submit your vote".to_owned()
        }
    });
    let info = format!("{}\n{}", status(&review.poll), notice);
    super::wrap_cells(&info, width)
        .into_iter()
        .map(|line| Row {
            line: Line::styled(
                line,
                Style::default().fg(if review.error.is_some() {
                    Color::Yellow
                } else {
                    MUTED
                }),
            ),
            option: None,
            spoiler: false,
        })
        .collect()
}
