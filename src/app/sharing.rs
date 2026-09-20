use super::{App, Focus, Mode, Screen, TelegramCommand};
use crate::{event::ConnectionStatus, model::sanitize_terminal_line};

#[derive(Clone, Default)]
pub struct State {
    next_request: u64,
    copy: Option<u64>,
}

impl App {
    pub(super) fn copy_message(&mut self, link: bool) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || self.mode != Mode::Navigate
            || self.focus != Focus::Conversation
        {
            return Vec::new();
        }
        let Some(chat_id) = self.active_chat_id else {
            return Vec::new();
        };
        let Some(message_id) = self.selected_message.filter(|id| *id > 0) else {
            self.status_message = Some("Select a delivered message to copy".to_owned());
            return Vec::new();
        };
        if self.connection != ConnectionStatus::Online {
            self.status_message =
                Some("Connect to Telegram to check the message's sharing permissions".to_owned());
            return Vec::new();
        }
        if self.sharing.copy.is_some() {
            self.status_message = Some("A message copy is still being prepared".to_owned());
            return Vec::new();
        }
        self.sharing.next_request += 1;
        let request_id = self.sharing.next_request;
        self.sharing.copy = Some(request_id);
        self.status_message = Some(
            if link {
                "Loading message link…"
            } else {
                "Preparing message text…"
            }
            .to_owned(),
        );
        vec![TelegramCommand::CopyMessage {
            chat_id,
            message_id,
            link,
            request_id,
        }]
    }

    pub(super) fn message_copy_ready(
        &mut self,
        request_id: u64,
        result: Result<String, String>,
    ) -> Vec<TelegramCommand> {
        if self.sharing.copy != Some(request_id) {
            return Vec::new();
        }
        self.sharing.copy = None;
        match result {
            Ok(text) => vec![TelegramCommand::CopyText(text)],
            Err(error) => {
                self.status_message = Some(sanitize_terminal_line(&error));
                Vec::new()
            }
        }
    }
}
