use super::{App, Focus, Mode, Screen, TelegramCommand};
use crate::{
    actions::Action,
    event::{ConnectionStatus, NetworkEvent},
    forwarding::Plan,
    model::{Chat, sanitize_terminal_line},
};

#[derive(Clone, Default)]
pub struct State {
    pub review: Option<Review>,
    next_request: u64,
    pending: Option<(u64, bool)>,
    saved: Option<u64>,
}

#[derive(Clone)]
pub struct Review {
    pub source: i64,
    pub message: i32,
    pub target: i64,
    pub source_title: String,
    pub target_title: String,
    pub plan: Option<Plan>,
    pub error: Option<String>,
    invalid: bool,
}

impl App {
    pub(super) fn open_saved(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main || self.mode != Mode::Navigate {
            return Vec::new();
        }
        if let Some(id) = self
            .account_user_id
            .filter(|id| self.chats.iter().any(|chat| chat.id == *id))
        {
            return self.open_chat_by_id(id);
        }
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to open Saved Messages".to_owned());
            return Vec::new();
        }
        if self.forwarding.saved.is_some() {
            return Vec::new();
        }
        self.forwarding.next_request += 1;
        let request_id = self.forwarding.next_request;
        self.forwarding.saved = Some(request_id);
        self.status_message = Some("Opening Saved Messages…".to_owned());
        vec![TelegramCommand::OpenSaved { request_id }]
    }

    pub(super) fn saved_ready(
        &mut self,
        request_id: u64,
        result: Result<Chat, String>,
    ) -> Vec<TelegramCommand> {
        if self.forwarding.saved != Some(request_id) {
            return Vec::new();
        }
        self.forwarding.saved = None;
        match result {
            Ok(chat) if Some(chat.id) == self.account_user_id => {
                self.open_resolved_link(chat, None)
            }
            Ok(_) => {
                self.status_message =
                    Some("Saved Messages belongs to a different account".to_owned());
                Vec::new()
            }
            Err(error) => {
                self.status_message = Some(sanitize_terminal_line(&error));
                Vec::new()
            }
        }
    }

    pub(super) fn review_forward(&mut self, target: i64) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || self.mode != Mode::Navigate
            || self.focus != Focus::Conversation
        {
            return Vec::new();
        }
        if self.forwarding.pending.is_some() {
            self.status_message = Some("A forward request is still in progress".to_owned());
            return Vec::new();
        }
        let Some(source) = self.active_chat_id else {
            return Vec::new();
        };
        let Some(message) = self.selected_message.filter(|id| *id > 0) else {
            self.status_message = Some("Select a delivered message to forward".to_owned());
            return Vec::new();
        };
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to forward messages".to_owned());
            return Vec::new();
        }
        let source_title = self
            .active_chat()
            .map_or_else(|| "Conversation".to_owned(), |chat| chat.title.clone());
        let target_title = if Some(target) == self.account_user_id {
            "Saved Messages".to_owned()
        } else {
            self.chats
                .iter()
                .find(|chat| chat.id == target)
                .map_or_else(|| target.to_string(), |chat| chat.title.clone())
        };
        self.forwarding.review = Some(Review {
            source,
            message,
            target,
            source_title,
            target_title,
            plan: None,
            error: None,
            invalid: false,
        });
        self.forwarding.next_request += 1;
        let request_id = self.forwarding.next_request;
        self.forwarding.pending = Some((request_id, false));
        self.mode = Mode::ForwardPrompt;
        self.status_message = None;
        vec![TelegramCommand::ReviewForward {
            chat_id: source,
            message_id: message,
            destination: target,
            request_id,
        }]
    }

    pub(super) fn forward_ready(&mut self, request_id: u64, result: Result<Plan, String>) {
        if self.forwarding.pending != Some((request_id, false)) {
            return;
        }
        self.forwarding.pending = None;
        let Some(review) = &mut self.forwarding.review else {
            return;
        };
        if review.invalid {
            return;
        }
        match result {
            Ok(plan)
                if plan.message.chat_id == review.source
                    && plan.message.id == review.message
                    && plan.destination.id == review.target =>
            {
                review.target_title.clone_from(&plan.destination.title);
                review.plan = Some(plan);
            }
            Ok(_) => review.error = Some("Forward target changed; reopen the preview".to_owned()),
            Err(error) => review.error = Some(sanitize_terminal_line(&error)),
        }
    }

    fn send_forward(&mut self) -> Vec<TelegramCommand> {
        if self.forwarding.pending.is_some() {
            return Vec::new();
        }
        let Some(review) = &self.forwarding.review else {
            return Vec::new();
        };
        let Some(plan) = &review.plan else {
            return Vec::new();
        };
        if review.invalid {
            return Vec::new();
        }
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram before forwarding".to_owned());
            return Vec::new();
        }
        self.forwarding.next_request += 1;
        let request_id = self.forwarding.next_request;
        self.forwarding.pending = Some((request_id, true));
        self.status_message = Some("Forwarding…".to_owned());
        vec![TelegramCommand::ForwardMessage {
            chat_id: review.source,
            message_id: review.message,
            destination: review.target,
            revision: plan.revision,
            random_id: plan.random_id,
            request_id,
        }]
    }

    pub(super) fn forward_finished(&mut self, request_id: u64, error: Option<String>) {
        if self.forwarding.pending != Some((request_id, true)) {
            return;
        }
        self.forwarding.pending = None;
        let Some(review) = &mut self.forwarding.review else {
            return;
        };
        if let Some(error) = error {
            let error = sanitize_terminal_line(&error);
            review.error = Some(error.clone());
            self.status_message = Some(error);
        } else {
            self.status_message = Some(format!("Forwarded to {}", review.target_title));
            self.forwarding.review = None;
            if self.mode == Mode::ForwardPrompt {
                self.mode = Mode::Navigate;
            }
        }
    }

    #[must_use]
    pub fn forward_pending(&self) -> bool {
        self.forwarding.pending.is_some()
    }

    pub(super) fn forwarding_binding(&mut self, action: &Action) -> Option<Vec<TelegramCommand>> {
        if self.mode == Mode::ForwardPrompt {
            match action {
                Action::Send => return Some(self.send_forward()),
                Action::Cancel => {
                    self.mode = Mode::Navigate;
                    if self.forwarding.pending.is_some_and(|(_, sending)| sending) {
                        self.status_message =
                            Some("Forward request is running in the background".to_owned());
                    } else {
                        self.forwarding.pending = None;
                        self.forwarding.review = None;
                        self.status_message = None;
                    }
                }
                Action::Quit | Action::Redraw | Action::NextAccount | Action::AddAccount => {
                    return None;
                }
                _ => {}
            }
            return Some(Vec::new());
        }
        match action {
            Action::SavedMessages => Some(self.open_saved()),
            Action::SaveMessage => Some(
                self.account_user_id
                    .map_or_else(Vec::new, |id| self.review_forward(id)),
            ),
            Action::ForwardMessage => {
                if self.mode != Mode::Navigate || self.focus != Focus::Conversation {
                    return Some(Vec::new());
                }
                if self
                    .forwarding
                    .review
                    .as_ref()
                    .is_some_and(|review| review.error.is_some())
                {
                    self.mode = Mode::ForwardPrompt;
                    return Some(Vec::new());
                }
                self.begin_command();
                self.commands.input.set_value("forward ");
                Some(Vec::new())
            }
            _ => None,
        }
    }

    pub(super) fn observe_forward(&mut self, event: &NetworkEvent) {
        let Some(review) = &mut self.forwarding.review else {
            return;
        };
        let changed = match event {
            NetworkEvent::MessageUpdated(message) => {
                message.chat_id == review.source
                    && message.id == review.message
                    && review.plan.as_ref().is_none_or(|plan| {
                        message.text != plan.message.text
                            || message.edited_at != plan.message.edited_at
                            || message.attachment != plan.message.attachment
                    })
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                channel_id.map_or(review.source > -1_000_000_000_000, |chat| {
                    chat == review.source
                }) && message_ids.contains(&review.message)
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                chat_id.is_none_or(|chat| chat == review.source)
            }
            _ => false,
        };
        if changed {
            review.invalid = true;
            review.error =
                Some("Original changed or was removed; close and reopen forwarding".to_owned());
        }
    }
}
