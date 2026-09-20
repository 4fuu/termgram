use super::{App, Focus, Mode, Screen};
use crate::{
    actions::Action,
    event::{ConnectionStatus, NetworkEvent, TelegramCommand},
    model::ChatId,
    reactions::{Choice, Kind, Review, Summary, Update},
};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct State {
    pub panel: Option<Panel>,
    pub hit_rows: Vec<(u16, u16, u16, usize)>,
    visible: Vec<(ChatId, i32)>,
    refreshed: Vec<(ChatId, i32)>,
    refresh: Option<(ChatId, u64)>,
    due: Option<Instant>,
    next: u64,
}

#[derive(Clone)]
pub struct Panel {
    pub chat: ChatId,
    pub message: i32,
    pub title: String,
    pub review: Option<Review>,
    pub selected: usize,
    pub choices: Vec<Choice>,
    pub top: usize,
    pub loading: Option<u64>,
    pub sending: Option<u64>,
    pub ready: bool,
    pub error: Option<String>,
}

impl Panel {
    fn loaded(&mut self, review: Review) {
        self.choices.clone_from(&review.choices);
        for kind in review.summary.chosen() {
            if let Kind::Emoji(emoji) = kind
                && !self.choices.iter().any(|choice| choice.emoji == emoji)
            {
                self.choices.push(Choice {
                    emoji,
                    title: "Selected · remove only".to_owned(),
                });
            }
        }
        self.ready = !review.summary.stale && review.summary.choices_known;
        self.selected = self.selected.min(self.choices.len().saturating_sub(1));
        self.review = Some(review);
        self.error = None;
    }
}

impl App {
    fn next_reaction_request(&mut self) -> u64 {
        self.reactions.next = self.reactions.next.wrapping_add(1).max(1);
        self.reactions.next
    }

    pub(super) fn begin_reactions(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || self.mode != Mode::Navigate
            || self.focus != Focus::Conversation
        {
            return Vec::new();
        }
        if self
            .reactions
            .panel
            .as_ref()
            .is_some_and(|panel| panel.sending.is_some())
        {
            self.status_message = Some("Wait for the pending reaction to finish".to_owned());
            return Vec::new();
        }
        let Some(message) = self.inspected_message().filter(|message| message.id > 0) else {
            return Vec::new();
        };
        self.reactions.panel = Some(Panel {
            chat: message.chat_id,
            message: message.id,
            title: self
                .active_chat()
                .map_or_else(|| "Reactions".to_owned(), |chat| chat.title.clone()),
            review: None,
            choices: Vec::new(),
            selected: 0,
            top: 0,
            loading: None,
            sending: None,
            ready: false,
            error: None,
        });
        self.mode = Mode::Reactions;
        self.status_message = None;
        self.load_reactions()
    }

    fn load_reactions(&mut self) -> Vec<TelegramCommand> {
        let request_id = self.next_reaction_request();
        let Some(panel) = &mut self.reactions.panel else {
            return Vec::new();
        };
        if panel.loading.is_some() || panel.sending.is_some() {
            return Vec::new();
        }
        panel.ready = false;
        if self.connection != ConnectionStatus::Online {
            panel.error = Some("Connect to Telegram to choose reactions".to_owned());
            return Vec::new();
        }
        panel.loading = Some(request_id);
        panel.error = None;
        vec![TelegramCommand::LoadReactions {
            chat_id: panel.chat,
            message_id: panel.message,
            request_id,
        }]
    }

    pub(super) fn reaction_binding(
        &mut self,
        action: &Action,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if *action == Action::Reactions {
            return Some(self.begin_reactions());
        }
        if self.mode != Mode::Reactions {
            return None;
        }
        let mut commands = Vec::new();
        match action {
            Action::Cancel => {
                self.mode = Mode::Navigate;
                if self
                    .reactions
                    .panel
                    .as_ref()
                    .is_none_or(|panel| panel.sending.is_none())
                {
                    self.reactions.panel = None;
                }
            }
            Action::Refresh => commands = self.load_reactions(),
            Action::Open | Action::Send => commands = self.change_reaction(false),
            Action::ClearReactions => commands = self.change_reaction(true),
            Action::Up
            | Action::Down
            | Action::PageUp
            | Action::PageDown
            | Action::Home
            | Action::End => {
                if let Some(panel) = &mut self.reactions.panel {
                    let step = if matches!(action, Action::PageUp | Action::PageDown) {
                        count.saturating_mul(10)
                    } else {
                        count
                    };
                    panel.selected = match action {
                        Action::Up | Action::PageUp => panel.selected.saturating_sub(step),
                        Action::Home => 0,
                        Action::End => panel.choices.len().saturating_sub(1),
                        _ => panel
                            .selected
                            .saturating_add(step)
                            .min(panel.choices.len().saturating_sub(1)),
                    };
                }
            }
            Action::Quit | Action::Redraw => return None,
            _ => {}
        }
        Some(commands)
    }

    fn change_reaction(&mut self, clear: bool) -> Vec<TelegramCommand> {
        let request_id = self.next_reaction_request();
        let Some(panel) = &mut self.reactions.panel else {
            return Vec::new();
        };
        if !panel.ready || panel.loading.is_some() || panel.sending.is_some() {
            return Vec::new();
        }
        if self.connection != ConnectionStatus::Online {
            panel.error = Some("Connect to Telegram before changing reactions".to_owned());
            return Vec::new();
        }
        let Some(review) = &panel.review else {
            return Vec::new();
        };
        let emoji = if clear {
            None
        } else {
            let Some(choice) = panel.choices.get(panel.selected) else {
                return Vec::new();
            };
            Some(choice.emoji.clone())
        };
        if let Err(error) = review.toggle(emoji.as_deref()) {
            panel.error = Some(error);
            return Vec::new();
        }
        let expected = review.summary.chosen();
        if clear && expected.is_empty() {
            return Vec::new();
        }
        panel.sending = Some(request_id);
        panel.error = None;
        vec![TelegramCommand::ChangeReaction {
            chat_id: panel.chat,
            message_id: panel.message,
            request_id,
            expected,
            emoji,
        }]
    }

    fn apply_reactions(&mut self, update: &Update) {
        self.visit_messages(|message| {
            if (message.chat_id, message.id) == (update.chat, update.message) {
                message
                    .reactions
                    .get_or_insert_with(Summary::default)
                    .apply(&update.summary);
            }
        });
        if let Some(panel) = &mut self.reactions.panel
            && (panel.chat, panel.message) == (update.chat, update.message)
            && let Some(review) = &mut panel.review
        {
            review.summary.apply(&update.summary);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn observe_reactions(
        &mut self,
        event: &NetworkEvent,
    ) -> Option<Vec<TelegramCommand>> {
        match event {
            NetworkEvent::ReactionsChanged(update) => {
                self.apply_reactions(update);
                return Some(Vec::new());
            }
            NetworkEvent::ReactionsLoading { .. } => return Some(Vec::new()),
            NetworkEvent::ReactionsLoaded {
                chat_id,
                message_id,
                request_id,
                result,
            } => {
                if let Ok(review) = result {
                    self.apply_reactions(&Update {
                        chat: *chat_id,
                        message: *message_id,
                        summary: review.summary.clone(),
                    });
                }
                if let Some(panel) = &mut self.reactions.panel
                    && panel.loading == Some(*request_id)
                    && (panel.chat, panel.message) == (*chat_id, *message_id)
                {
                    panel.loading = None;
                    match result {
                        Ok(review) => panel.loaded(review.clone()),
                        Err(error) => {
                            panel.error = Some(crate::model::sanitize_terminal_line(error));
                        }
                    }
                }
                return Some(Vec::new());
            }
            NetworkEvent::ReactionsFinished {
                chat_id,
                message_id,
                request_id,
                error,
            } => {
                if message_id.is_none() && self.reactions.refresh == Some((*chat_id, *request_id)) {
                    self.reactions.refresh = None;
                    self.reactions.due = Some(Instant::now() + Duration::from_secs(30));
                }
                if let Some(panel) = &mut self.reactions.panel
                    && panel.sending == Some(*request_id)
                    && panel.chat == *chat_id
                    && Some(panel.message) == *message_id
                {
                    panel.sending = None;
                    panel.ready = false;
                    if let Some(error) = error {
                        panel.error = Some(format!(
                            "{} · refresh before retrying",
                            crate::model::sanitize_terminal_line(error)
                        ));
                        if self.mode != Mode::Reactions {
                            self.status_message.clone_from(&panel.error);
                        }
                    } else if self.mode == Mode::Reactions {
                        return Some(self.load_reactions());
                    } else {
                        self.reactions.panel = None;
                        self.status_message = Some("Reactions saved".to_owned());
                    }
                }
                return Some(Vec::new());
            }
            NetworkEvent::MessageUpdated(message) => {
                if let Some(summary) = &message.reactions {
                    self.apply_reactions(&Update {
                        chat: message.chat_id,
                        message: message.id,
                        summary: summary.clone(),
                    });
                }
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                if let Some(panel) = &mut self.reactions.panel
                    && channel_id.map_or(panel.chat > -1_000_000_000_000, |chat| chat == panel.chat)
                    && message_ids.contains(&panel.message)
                {
                    panel.ready = false;
                    panel.loading = None;
                    panel.error = Some("This message was deleted".to_owned());
                }
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                if let Some(panel) = &mut self.reactions.panel
                    && chat_id.is_none_or(|chat| chat == panel.chat)
                {
                    panel.ready = false;
                    panel.loading = None;
                    panel.error = Some("History changed; reopen reactions".to_owned());
                }
            }
            _ => {}
        }
        None
    }

    pub fn set_visible_reactions(&mut self, ids: Vec<(ChatId, i32)>) {
        self.reactions.visible = ids;
    }

    fn reactions_visible(&self) -> bool {
        self.screen == Screen::Main
            && self.terminal_focused
            && self.connection == ConnectionStatus::Online
            && matches!(self.mode, Mode::Navigate | Mode::Compose | Mode::Reactions)
            && !self.reactions.visible.is_empty()
    }

    pub fn request_visible_reactions(&mut self) -> Vec<TelegramCommand> {
        if !self.reactions_visible()
            || self.reactions.refresh.is_some()
            || (self.reactions.refreshed == self.reactions.visible
                && self.reactions.due.is_none_or(|due| due > Instant::now()))
            || self
                .reactions
                .panel
                .as_ref()
                .is_some_and(|panel| panel.loading.is_some() || panel.sending.is_some())
        {
            return Vec::new();
        }
        let Some(chat_id) = self.active_chat_id else {
            return Vec::new();
        };
        let message_ids: Vec<_> = self
            .reactions
            .visible
            .iter()
            .filter(|(chat, _)| *chat == chat_id)
            .map(|(_, id)| *id)
            .take(100)
            .collect();
        if message_ids.is_empty() {
            return Vec::new();
        }
        let request_id = self.next_reaction_request();
        self.reactions.refresh = Some((chat_id, request_id));
        self.reactions.refreshed.clone_from(&self.reactions.visible);
        vec![TelegramCommand::RefreshReactions {
            chat_id,
            message_ids,
            request_id,
        }]
    }

    #[must_use]
    pub fn next_reaction_deadline(&self) -> Option<Instant> {
        if !self.reactions_visible()
            || self.reactions.refresh.is_some()
            || self
                .reactions
                .panel
                .as_ref()
                .is_some_and(|panel| panel.loading.is_some() || panel.sending.is_some())
        {
            return None;
        }
        if self.reactions.refreshed == self.reactions.visible {
            self.reactions.due
        } else {
            Some(Instant::now())
        }
    }
}
