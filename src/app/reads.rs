use super::{App, ChatId, Mode, Screen, TelegramCommand};
use crate::event::ConnectionStatus;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub(super) struct State {
    pub visible: Option<(ChatId, i32)>,
    pending: BTreeMap<ChatId, i32>,
    retry_after: BTreeMap<ChatId, Instant>,
}

impl App {
    /// Called after a successful frame. Merely loading or selecting a chat is
    /// not evidence that its messages were displayed.
    pub fn request_visible_read(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || !self.terminal_focused
            || self.connection != ConnectionStatus::Online
            || !matches!(self.mode, Mode::Navigate | Mode::Compose)
        {
            return Vec::new();
        }
        let Some((chat_id, max_id)) = self
            .reads
            .visible
            .filter(|(id, _)| Some(*id) == self.active_chat_id)
        else {
            return Vec::new();
        };
        let Some(previous) = self
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| chat.read_inbox_max_id)
        else {
            return Vec::new();
        };
        if max_id <= previous
            || max_id <= 0
            || self.reads.pending.contains_key(&chat_id)
            || self
                .reads
                .retry_after
                .get(&chat_id)
                .is_some_and(|until| *until > Instant::now())
        {
            return Vec::new();
        }
        self.reads.pending.insert(chat_id, max_id);
        vec![TelegramCommand::MarkRead { chat_id, max_id }]
    }

    pub(super) fn read_finished(&mut self, chat_id: ChatId, max_id: i32, failed: bool) {
        if self.reads.pending.get(&chat_id) == Some(&max_id) {
            self.reads.pending.remove(&chat_id);
        }
        if failed {
            self.reads
                .retry_after
                .insert(chat_id, Instant::now() + Duration::from_secs(2));
        } else {
            self.reads.retry_after.remove(&chat_id);
        }
    }

    pub fn set_visible_read_boundary(&mut self, chat_id: ChatId, max_id: Option<i32>) {
        self.reads.visible = max_id.map(|id| (chat_id, id));
    }
}
