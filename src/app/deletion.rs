use super::{App, Focus, Mode, Screen, TelegramCommand};
use crate::{
    actions::Action,
    deletion::Plan,
    event::{ConnectionStatus, NetworkEvent},
    model::sanitize_terminal_line,
};

#[derive(Clone, Default)]
pub struct State {
    pub prompt: Option<Prompt>,
    next_request: u64,
    pending: Option<(u64, bool)>,
}

#[derive(Clone)]
pub struct Prompt {
    pub chat: i64,
    pub message: i32,
    pub title: String,
    pub plan: Option<Plan>,
    pub selected: usize,
    pub error: Option<String>,
    invalid: bool,
}

impl App {
    pub(super) fn begin_deletion(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main
            || self.mode != Mode::Navigate
            || self.focus != Focus::Conversation
            || self.deletion.pending.is_some()
        {
            return Vec::new();
        }
        let Some(chat) = self.active_chat_id else {
            return Vec::new();
        };
        let Some(message) = self.selected_message.filter(|id| *id > 0) else {
            self.status_message = Some("Select a delivered message to delete".to_owned());
            return Vec::new();
        };
        if self.connection != ConnectionStatus::Online {
            self.status_message =
                Some("Connect to Telegram to review deletion permissions".to_owned());
            return Vec::new();
        }
        let title = self
            .active_chat()
            .map_or_else(|| "Conversation".to_owned(), |chat| chat.title.clone());
        self.deletion.prompt = Some(Prompt {
            chat,
            message,
            title,
            plan: None,
            selected: 0,
            error: None,
            invalid: false,
        });
        self.deletion.next_request += 1;
        let request_id = self.deletion.next_request;
        self.deletion.pending = Some((request_id, false));
        self.mode = Mode::DeletePrompt;
        self.status_message = None;
        vec![TelegramCommand::ReviewDeletion {
            chat_id: chat,
            message_id: message,
            request_id,
        }]
    }

    #[must_use]
    pub fn deletion_pending(&self) -> bool {
        self.deletion.pending.is_some()
    }

    pub(super) fn deletion_ready(
        &mut self,
        chat: i64,
        message: i32,
        request_id: u64,
        result: Result<Plan, String>,
    ) {
        if self.deletion.pending != Some((request_id, false)) {
            return;
        }
        let Some(prompt) = self
            .deletion
            .prompt
            .as_mut()
            .filter(|prompt| prompt.chat == chat && prompt.message == message)
        else {
            return;
        };
        self.deletion.pending = None;
        if prompt.invalid {
            return;
        }
        match result {
            Ok(plan) => {
                prompt.plan = Some(plan);
                prompt.selected = 0;
            }
            Err(error) => prompt.error = Some(sanitize_terminal_line(&error)),
        }
    }

    pub(super) fn deletion_finished(
        &mut self,
        chat: i64,
        message: i32,
        request_id: u64,
        error: Option<String>,
    ) {
        if self.deletion.pending != Some((request_id, true)) {
            return;
        }
        let Some(prompt) = self
            .deletion
            .prompt
            .as_mut()
            .filter(|prompt| prompt.chat == chat && prompt.message == message)
        else {
            return;
        };
        self.deletion.pending = None;
        if let Some(error) = error {
            let error = sanitize_terminal_line(&error);
            prompt.error = Some(error.clone());
            prompt.selected = 0;
            self.status_message = Some(error);
        } else {
            self.deletion.prompt = None;
            if self.mode == Mode::DeletePrompt {
                self.mode = Mode::Navigate;
            }
            self.status_message = Some("Message deleted".to_owned());
        }
    }

    fn close_deletion(&mut self) {
        self.mode = Mode::Navigate;
        if self.deletion.pending.is_some_and(|(_, saving)| saving) {
            self.status_message = Some("Delete request is running in the background".to_owned());
        } else {
            self.deletion.pending = None;
            self.deletion.prompt = None;
            self.status_message = None;
        }
    }

    fn confirm_deletion(&mut self) -> Vec<TelegramCommand> {
        if self.deletion.pending.is_some() {
            return Vec::new();
        }
        let Some(prompt) = &self.deletion.prompt else {
            return Vec::new();
        };
        if prompt.selected == 0 {
            self.close_deletion();
            return Vec::new();
        }
        if prompt.invalid {
            return Vec::new();
        }
        let Some(plan) = &prompt.plan else {
            return Vec::new();
        };
        let Some(&scope) = plan.scopes.get(prompt.selected - 1) else {
            return Vec::new();
        };
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram before confirming deletion".to_owned());
            return Vec::new();
        }
        self.deletion.next_request += 1;
        let request_id = self.deletion.next_request;
        self.deletion.pending = Some((request_id, true));
        self.status_message = Some("Deleting message…".to_owned());
        vec![TelegramCommand::DeleteMessage {
            chat_id: prompt.chat,
            message_id: prompt.message,
            revision: plan.revision,
            request_id,
            scope,
        }]
    }

    pub(super) fn deletion_binding(
        &mut self,
        action: &Action,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if *action == Action::DeleteMessage {
            return Some(self.begin_deletion());
        }
        if self.mode != Mode::DeletePrompt {
            return None;
        }
        match action {
            Action::Cancel => self.close_deletion(),
            Action::Open => return Some(self.confirm_deletion()),
            Action::Up | Action::Down if !self.deletion_pending() => {
                if let Some(prompt) = &mut self.deletion.prompt {
                    let maximum = prompt
                        .plan
                        .as_ref()
                        .filter(|_| !prompt.invalid)
                        .map_or(0, |plan| plan.scopes.len());
                    prompt.selected = if *action == Action::Up {
                        prompt.selected.saturating_sub(count)
                    } else {
                        prompt.selected.saturating_add(count).min(maximum)
                    };
                }
            }
            Action::Quit | Action::Redraw | Action::NextAccount | Action::AddAccount => {
                return None;
            }
            _ => {}
        }
        Some(Vec::new())
    }

    pub(super) fn observe_deletion(&mut self, event: &NetworkEvent) {
        let Some(prompt) = &mut self.deletion.prompt else {
            return;
        };
        let changed = match event {
            NetworkEvent::MessageUpdated(message) => {
                message.chat_id == prompt.chat
                    && message.id == prompt.message
                    && prompt.plan.as_ref().is_none_or(|plan| {
                        message.text != plan.message.text
                            || message.edited_at != plan.message.edited_at
                            || message.attachment != plan.message.attachment
                    })
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                channel_id.map_or(prompt.chat > -1_000_000_000_000, |chat| chat == prompt.chat)
                    && message_ids.contains(&prompt.message)
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                chat_id.is_none_or(|chat| chat == prompt.chat)
            }
            _ => false,
        };
        if changed {
            prompt.invalid = true;
            prompt.selected = 0;
            prompt.error =
                Some("Message changed or was removed; close and reopen deletion".to_owned());
        }
    }
}
