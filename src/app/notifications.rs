use super::{App, Focus, Mode, Screen, TelegramCommand};
use crate::{event::ConnectionStatus, model::ChatId, notifications::Mute};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct State {
    next_request: u64,
    pending: BTreeMap<ChatId, u64>,
}

impl App {
    pub(super) fn mute_focused_chat(&mut self, mute: Mute) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main || self.mode != Mode::Navigate {
            return Vec::new();
        }
        let id = if self.focus == Focus::Chats {
            self.selected_chat_entry().map(|chat| chat.id)
        } else {
            self.active_chat_id
        };
        id.map_or_else(Vec::new, |id| self.set_chat_mute(id, mute))
    }

    pub(super) fn set_chat_mute(&mut self, chat_id: ChatId, mute: Mute) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram first".to_owned());
            return Vec::new();
        }
        if self.notifications.pending.contains_key(&chat_id) {
            self.status_message =
                Some("This chat's notification settings are still updating".to_owned());
            return Vec::new();
        }
        self.notifications.next_request = self.notifications.next_request.wrapping_add(1);
        let request_id = self.notifications.next_request;
        self.notifications.pending.insert(chat_id, request_id);
        self.status_message = Some("Saving Telegram notification settings…".to_owned());
        vec![TelegramCommand::SetChatMute {
            chat_id,
            mute,
            request_id,
        }]
    }

    pub(super) fn finish_chat_mute(
        &mut self,
        chat_id: ChatId,
        request_id: u64,
        result: Result<Option<i64>, String>,
    ) {
        if self.notifications.pending.get(&chat_id) != Some(&request_id) {
            return;
        }
        self.notifications.pending.remove(&chat_id);
        match result {
            Ok(until) => {
                if let Some(until) = until {
                    self.apply_chat_mute(chat_id, until);
                }
                let title = self
                    .chats
                    .iter()
                    .find(|chat| chat.id == chat_id)
                    .map_or_else(|| chat_id.to_string(), |chat| chat.title.clone());
                self.status_message = Some(format!(
                    "{title} · {}",
                    until.map_or_else(
                        || "Notification setting saved; syncing…".to_owned(),
                        |until| crate::notifications::mute_label(
                            until,
                            chrono::Utc::now().timestamp()
                        )
                    )
                ));
            }
            Err(error) => self.status_message = Some(crate::model::sanitize_terminal_line(&error)),
        }
    }

    pub(super) fn apply_chat_mute(&mut self, chat_id: ChatId, until: i64) {
        let selected = self.selected_chat_entry().map(|chat| chat.id);
        if let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.membership.mute_until = until;
        }
        self.preserve_chat_selection(selected);
    }

    #[must_use]
    pub fn focused_mute_label(&self) -> Option<String> {
        let chat = if self.focus == Focus::Chats {
            self.selected_chat_entry()
        } else {
            self.active_chat()
        }?;
        let now = chrono::Utc::now().timestamp();
        (chat.membership.mute_until > now)
            .then(|| crate::notifications::mute_label(chat.membership.mute_until, now))
    }
}
