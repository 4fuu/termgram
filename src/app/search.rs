use super::{App, Focus, KeyAction, Mode, TelegramCommand, TextInput};
use crate::{
    model::{ChatId, Message},
    search::{Cursor, Page, Request},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Scope {
    #[default]
    Chat,
    Folder,
    Account,
}

#[derive(Clone, Default)]
pub struct State {
    pub query: TextInput,
    pub editing: bool,
    pub scope: Scope,
    pub scope_label: String,
    pub page: Option<Page>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub page_number: usize,
    request_id: u64,
    chats: Option<Vec<ChatId>>,
    starts: Vec<Option<Cursor>>,
}

impl App {
    pub(super) fn open_search(&mut self) -> Vec<TelegramCommand> {
        if self.search.query.is_empty() {
            self.search.editing = true;
            self.search.scope = if self.active_chat_id.is_some() {
                Scope::Chat
            } else {
                Scope::Folder
            };
        }
        self.mode = Mode::Search;
        if self.search.page.is_none() {
            self.update_search_scope();
        }
        Vec::new()
    }

    fn update_search_scope(&mut self) {
        (self.search.chats, self.search.scope_label) = match self.search.scope {
            Scope::Chat => (
                Some(self.active_chat_id.into_iter().collect()),
                self.active_chat().map_or_else(
                    || "No open chat".to_owned(),
                    |chat| format!("Chat: {}", chat.title),
                ),
            ),
            Scope::Folder => {
                let folder = self
                    .folders
                    .iter()
                    .find(|folder| folder.id == self.folder_id);
                (
                    Some(
                        self.chats
                            .iter()
                            .filter(|chat| {
                                folder.is_none_or(|folder| {
                                    folder.contains(chat, chrono::Utc::now().timestamp())
                                })
                            })
                            .map(|chat| chat.id)
                            .collect(),
                    ),
                    format!(
                        "Folder: {}",
                        folder.map_or("All chats", |folder| folder.title.as_str())
                    ),
                )
            }
            Scope::Account => (None, "This account".to_owned()),
        };
    }

    pub(super) fn search_binding(
        &mut self,
        action: &str,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        match action {
            "cancel" => {
                self.mode = Mode::Navigate;
                self.search.loading = false;
                self.search.request_id = self.search.request_id.wrapping_add(1);
                Some(vec![TelegramCommand::CancelSearch])
            }
            "open" if self.search.editing => Some(self.search_submit(None, true)),
            "open" => {
                let message = self
                    .search
                    .page
                    .as_ref()?
                    .messages
                    .get(self.search.selected)?;
                self.search.request_id = self.search.request_id.wrapping_add(1);
                self.search.loading = true;
                Some(vec![TelegramCommand::LoadCachedContext {
                    chat_id: message.chat_id,
                    message_id: message.id,
                    request_id: self.search.request_id,
                }])
            }
            "search_scope" => {
                self.search.scope = match self.search.scope {
                    Scope::Chat => Scope::Folder,
                    Scope::Folder => Scope::Account,
                    Scope::Account => Scope::Chat,
                };
                self.update_search_scope();
                self.search.page = None;
                self.search.editing = true;
                self.search.loading = false;
                self.search.request_id = self.search.request_id.wrapping_add(1);
                Some(vec![TelegramCommand::CancelSearch])
            }
            "search_query" => {
                self.search.editing = true;
                Some(Vec::new())
            }
            "search_more" if !self.search.loading => {
                if let Some(next) = self.search.page.as_ref().and_then(|page| page.next) {
                    self.search.starts.push(Some(next));
                    self.search.page_number += 1;
                    Some(self.search_submit(Some(next), false))
                } else {
                    Some(Vec::new())
                }
            }
            "search_previous" if !self.search.loading && self.search.page_number > 1 => {
                self.search.starts.pop();
                self.search.page_number -= 1;
                Some(self.search_submit(self.search.starts.last().copied().flatten(), false))
            }
            "up" | "down" | "page_up" | "page_down" => {
                let length = self
                    .search
                    .page
                    .as_ref()
                    .map_or(0, |page| page.messages.len());
                if length > 0 {
                    self.search.editing = false;
                    let amount = if action.starts_with("page_") {
                        count.saturating_mul(10)
                    } else {
                        count
                    };
                    self.search.selected = if action.ends_with("up") {
                        self.search.selected.saturating_sub(amount)
                    } else {
                        self.search.selected.saturating_add(amount).min(length - 1)
                    };
                }
                Some(Vec::new())
            }
            _ => None,
        }
    }

    fn search_submit(&mut self, before: Option<Cursor>, fresh: bool) -> Vec<TelegramCommand> {
        self.search.request_id = self.search.request_id.wrapping_add(1);
        if fresh {
            self.search.starts = vec![None];
            self.search.page_number = 1;
            self.search.page = None;
            self.update_search_scope();
        }
        self.search.selected = 0;
        self.search.loading = true;
        self.search.editing = false;
        self.search.error = None;
        vec![TelegramCommand::SearchCached(Request {
            id: self.search.request_id,
            pattern: self.search.query.value().to_owned(),
            chats: self.search.chats.clone(),
            before,
        })]
    }

    pub(super) fn edit_search(&mut self, action: KeyAction) -> Vec<TelegramCommand> {
        if !self.search.editing {
            return Vec::new();
        }
        let before = self.search.query.value().to_owned();
        let query = &mut self.search.query;
        match action {
            KeyAction::Character(c) => query.insert(c),
            KeyAction::Backspace => _ = query.backspace(),
            KeyAction::Delete => _ = query.delete(),
            KeyAction::Left => _ = query.move_left(),
            KeyAction::Right => _ = query.move_right(),
            KeyAction::Home => query.move_home(),
            KeyAction::End => query.move_end(),
            KeyAction::Clear => query.clear(),
            KeyAction::DeleteWord => _ = query.delete_word_before(),
            _ => {}
        }
        if before == query.value() {
            Vec::new()
        } else {
            self.search_edited()
        }
    }

    pub(super) fn search_edited(&mut self) -> Vec<TelegramCommand> {
        self.search.page = None;
        self.search.error = None;
        self.search.loading = false;
        self.search.request_id = self.search.request_id.wrapping_add(1);
        vec![TelegramCommand::CancelSearch]
    }

    pub(super) fn finish_search(&mut self, request_id: u64, result: Result<Page, String>) {
        if self.search.request_id != request_id || self.mode != Mode::Search {
            return;
        }
        self.search.loading = false;
        match result {
            Ok(page) => {
                self.search.page = Some(page);
                self.search.error = None;
            }
            Err(error) => {
                self.search.error = Some(crate::model::sanitize_terminal_text(&error));
                self.search.editing = true;
            }
        }
    }

    pub(super) fn open_search_context(
        &mut self,
        request_id: u64,
        chat_id: ChatId,
        message_id: i32,
        messages: Vec<Message>,
    ) {
        if self.search.request_id != request_id || self.mode != Mode::Search {
            return;
        }
        self.search.loading = false;
        self.mode = Mode::Navigate;
        self.clear_unread_navigation();
        self.focus = Focus::Conversation;
        self.narrow_conversation = true;
        self.active_history_request = None;
        self.older_motion = None;
        self.loading_history = false;
        self.active_chat_id = Some(chat_id);
        self.history_target_message = None;
        self.merge_history(chat_id, messages, None);
        self.browsing_older = true;
        self.new_messages_while_scrolled = 0;
        self.new_messages_to_anchor = 0;
        self.select_message_id(message_id, true);
        self.message_scroll = self.message_scroll.max(1);
        self.status_message = Some(format!(
            "Cached search result · {} latest · {} results",
            self.keymap
                .hint(crate::keymap::Context::Conversation, "latest"),
            self.keymap
                .hint(crate::keymap::Context::Conversation, "search")
        ));
    }
}
