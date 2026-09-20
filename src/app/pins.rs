use super::{App, Focus, Mode, TelegramCommand};
use crate::{
    event::ConnectionStatus,
    pins::{DialogAction, DialogPins, DialogScope},
};

#[derive(Clone, Default)]
pub struct State {
    pub dialogs: DialogPins,
    pending_dialog: Option<u64>,
    next_request: u64,
    archived_action: bool,
}

impl App {
    pub(super) fn toggle_archive(&mut self) -> Vec<TelegramCommand> {
        if self.focus != Focus::Chats || self.mode != Mode::Navigate {
            return Vec::new();
        }
        let Some(chat) = self.selected_chat_entry() else {
            return Vec::new();
        };
        let (chat_id, archived) = (chat.id, !chat.membership.archived);
        self.set_chat_archived(chat_id, archived)
    }

    pub(super) fn set_chat_archived(
        &mut self,
        chat_id: i64,
        archived: bool,
    ) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to change the archive".to_owned());
            return Vec::new();
        }
        if self.pins.pending_dialog.is_some() {
            return Vec::new();
        }
        self.pins.next_request += 1;
        let request_id = self.pins.next_request;
        self.pins.pending_dialog = Some(request_id);
        self.pins.archived_action = true;
        self.status_message = Some("Updating Telegram archive…".to_owned());
        vec![TelegramCommand::SetArchived {
            chat_id,
            archived,
            request_id,
        }]
    }
    #[must_use]
    pub fn chat_pin_position(&self, chat_id: i64) -> Option<usize> {
        self.chat_pin_position_in(self.folder_id, chat_id)
    }

    pub(super) fn chat_pin_position_in(&self, folder: i32, chat_id: i64) -> Option<usize> {
        let order = match folder {
            0 => &self.pins.dialogs.main,
            1 => &self.pins.dialogs.archive,
            id => &self.folders.iter().find(|folder| folder.id == id)?.pinned,
        };
        order.iter().position(|id| *id == chat_id)
    }

    pub(super) fn change_chat_pin(&mut self, run: &str) -> Vec<TelegramCommand> {
        if self.focus != Focus::Chats || self.mode != Mode::Navigate {
            return Vec::new();
        }
        let Some(chat) = self.selected_chat_entry() else {
            return Vec::new();
        };
        let chat_id = chat.id;
        let pinned = self.chat_pin_position(chat_id).is_some();
        let action = match run {
            "pin" => DialogAction::Set(!pinned),
            "pin_up" if pinned => DialogAction::MoveUp,
            "pin_down" if pinned => DialogAction::MoveDown,
            _ => {
                self.status_message = Some("Pin the chat before reordering it".to_owned());
                return Vec::new();
            }
        };
        let scope = match self.folder_id {
            0 => DialogScope::Main,
            1 => DialogScope::Archive,
            id => DialogScope::Filter(id),
        };
        self.request_chat_pin(chat_id, scope, action)
    }

    pub(super) fn request_chat_pin(
        &mut self,
        chat_id: i64,
        scope: DialogScope,
        action: DialogAction,
    ) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to change pins".to_owned());
            return Vec::new();
        }
        if self.pins.pending_dialog.is_some() {
            return Vec::new();
        }
        self.pins.next_request += 1;
        let request_id = self.pins.next_request;
        self.pins.pending_dialog = Some(request_id);
        self.pins.archived_action = false;
        self.status_message = Some("Updating Telegram pins…".to_owned());
        vec![TelegramCommand::ChangeDialogPin {
            chat_id,
            scope,
            action,
            request_id,
        }]
    }

    pub(super) fn finish_dialog_pin(&mut self, request_id: u64, error: Option<String>) {
        if self.pins.pending_dialog == Some(request_id) {
            self.pins.pending_dialog = None;
            let subject = if self.pins.archived_action {
                "archive"
            } else {
                "pins"
            };
            self.status_message = Some(error.map_or_else(
                || format!("Telegram {subject} updated"),
                |error| {
                    format!(
                        "Could not update {subject}: {}",
                        crate::model::sanitize_terminal_line(&error)
                    )
                },
            ));
        }
    }

    pub(super) fn preserve_chat_selection(&mut self, selected_id: Option<i64>) {
        self.selected_chat = selected_id
            .and_then(|id| {
                self.filtered_chat_indices()
                    .iter()
                    .position(|&index| self.chats[index].id == id)
            })
            .unwrap_or_else(|| {
                self.selected_chat
                    .min(self.filtered_chat_indices().len().saturating_sub(1))
            });
    }
}
