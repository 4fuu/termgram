use super::{App, Focus, KeyAction, Mode, TelegramCommand, TextInput};
use crate::{
    cloud_search,
    event::{ConnectionStatus, NetworkEvent},
    model::{ChatId, Message},
    search::{Cursor as LocalCursor, Request},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Scope {
    #[default]
    Chat,
    Folder,
    Account,
}

#[derive(Clone, Copy)]
enum Cursor {
    Local(LocalCursor),
    Cloud(i32),
}

#[derive(Clone, Default)]
pub enum Source {
    #[default]
    Local,
    Mentions(ChatId),
    Cloud {
        chat_id: ChatId,
        filters: cloud_search::Filters,
        sender: Option<ChatId>,
    },
}

#[derive(Clone)]
pub enum Results {
    Local(crate::search::Page),
    Cloud(cloud_search::Page),
}

impl Results {
    pub fn messages(&self) -> &[Message] {
        match self {
            Self::Local(page) => &page.messages,
            Self::Cloud(page) => &page.messages,
        }
    }
    pub(super) fn messages_mut(&mut self) -> &mut Vec<Message> {
        match self {
            Self::Local(page) => &mut page.messages,
            Self::Cloud(page) => &mut page.messages,
        }
    }
    fn next(&self) -> Option<Cursor> {
        match self {
            Self::Local(page) => page.next.map(Cursor::Local),
            Self::Cloud(page) => page.next.map(Cursor::Cloud),
        }
    }
}

#[derive(Clone, Default)]
pub struct State {
    pub query: TextInput,
    pub editing: bool,
    pub scope: Scope,
    pub scope_label: String,
    pub source: Source,
    pub page: Option<Results>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub page_number: usize,
    request_id: u64,
    chats: Option<Vec<ChatId>>,
    starts: Vec<Option<Cursor>>,
    target: Option<(ChatId, i32)>,
}

impl State {
    pub fn is_cloud(&self) -> bool {
        !matches!(self.source, Source::Local)
    }

    pub fn is_mentions(&self) -> bool {
        matches!(self.source, Source::Mentions(_))
    }
}

impl App {
    pub(super) fn focused_mentions(&mut self) -> Vec<TelegramCommand> {
        if self.screen != super::Screen::Main || self.mode != Mode::Navigate {
            return Vec::new();
        }
        let id = if self.focus == Focus::Chats {
            self.selected_chat_entry().map(|chat| chat.id)
        } else {
            self.active_chat_id
        };
        id.map_or_else(Vec::new, |id| self.start_mentions(id))
    }

    pub(super) fn start_mentions(&mut self, chat_id: ChatId) -> Vec<TelegramCommand> {
        self.search.source = Source::Mentions(chat_id);
        self.search.query.clear();
        self.mode = Mode::Search;
        self.search_submit(None, true)
    }

    pub(super) fn start_cloud_search(
        &mut self,
        chat_id: ChatId,
        query: String,
        filters: cloud_search::Filters,
    ) -> Vec<TelegramCommand> {
        self.search.request_id = self.search.request_id.wrapping_add(1);
        self.search.source = Source::Cloud {
            chat_id,
            filters,
            sender: None,
        };
        self.search.query.set_value(query);
        self.search.page = None;
        self.search.target = None;
        self.search.loading = false;
        self.search.error = None;
        self.search.editing = true;
        self.mode = Mode::Search;
        self.update_search_scope();
        if self.search.query.is_empty() {
            vec![TelegramCommand::CancelSearch]
        } else {
            self.search_submit(None, true)
        }
    }

    pub(super) fn start_local_search(&mut self, query: Option<&str>) -> Vec<TelegramCommand> {
        if self.search.is_cloud() {
            self.search.source = Source::Local;
            self.search.query.clear();
            self.search.page = None;
        }
        self.open_search();
        self.search.editing = true;
        if let Some(query) = query.filter(|query| !query.is_empty()) {
            self.search.query.set_value(query);
            self.search_submit(None, true)
        } else {
            self.search_edited()
        }
    }

    pub(super) fn open_search(&mut self) -> Vec<TelegramCommand> {
        if self.search.query.is_empty() && !self.search.is_mentions() {
            self.search.editing = true;
            self.search.scope = if self.active_chat_id.is_some() {
                Scope::Chat
            } else {
                Scope::Folder
            };
        }
        self.mode = Mode::Search;
        if self.search.page.is_none() {
            self.search.editing = !self.search.is_mentions();
            self.update_search_scope();
        }
        Vec::new()
    }

    fn update_search_scope(&mut self) {
        if let Source::Cloud { chat_id, .. } | Source::Mentions(chat_id) = &self.search.source {
            self.search.scope_label = self
                .chats
                .iter()
                .find(|chat| chat.id == *chat_id)
                .map_or_else(|| format!("Chat {chat_id}"), |chat| chat.title.clone());
            return;
        }
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
            "open" if self.search.loading => Some(Vec::new()),
            "open"
                if self.search.editing
                    || (self.search.is_mentions() && self.search.page.is_none()) =>
            {
                Some(self.search_submit(None, true))
            }
            "refresh" => Some(self.search_submit(None, true)),
            "open" => self.open_search_result(),
            "search_scope" if self.search.is_cloud() => Some(Vec::new()),
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
            "search_query" if self.search.is_mentions() => Some(Vec::new()),
            "search_query" => {
                self.search.editing = true;
                Some(Vec::new())
            }
            "search_more" if !self.search.loading => {
                if let Some(next) = self.search.page.as_ref().and_then(Results::next) {
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
                    .map_or(0, |page| page.messages().len());
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

    fn open_search_result(&mut self) -> Option<Vec<TelegramCommand>> {
        let message = self
            .search
            .page
            .as_ref()?
            .messages()
            .get(self.search.selected)?;
        let (chat_id, message_id) = (message.chat_id, message.id);
        if self.search.is_cloud() && self.connection != ConnectionStatus::Online {
            self.search.error = Some("Connect to Telegram to open this cloud result".to_owned());
            return Some(Vec::new());
        }
        self.search.request_id = self.search.request_id.wrapping_add(1);
        let request_id = self.search.request_id;
        self.search.loading = true;
        self.search.error = None;
        self.search.target = Some((chat_id, message_id));
        Some(vec![if self.search.is_cloud() {
            TelegramCommand::LoadCloudContext {
                chat_id,
                message_id,
                request_id,
            }
        } else {
            TelegramCommand::LoadCachedContext {
                chat_id,
                message_id,
                request_id,
            }
        }])
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
        self.search.target = None;
        self.search.page = None;
        if self.search.is_cloud() && self.connection != ConnectionStatus::Online {
            self.search.loading = false;
            self.search.editing = !self.search.is_mentions();
            self.search.error = Some(
                "Connect to Telegram for cloud search and mentions; :search uses the offline cache"
                    .to_owned(),
            );
            return vec![TelegramCommand::CancelSearch];
        }
        match &self.search.source {
            Source::Mentions(chat_id) => vec![TelegramCommand::SearchMentions {
                chat_id: *chat_id,
                request_id: self.search.request_id,
                before_id: match before {
                    Some(Cursor::Cloud(id)) => id,
                    _ => 0,
                },
            }],
            Source::Cloud {
                chat_id,
                filters,
                sender,
            } => {
                let mut filters = filters.clone();
                if let Some(id) = sender {
                    filters.sender = Some(cloud_search::Sender::Id(*id));
                }
                vec![TelegramCommand::SearchCloud(cloud_search::Request {
                    id: self.search.request_id,
                    chat_id: *chat_id,
                    query: self.search.query.value().to_owned(),
                    filters,
                    before_id: match before {
                        Some(Cursor::Cloud(id)) => id,
                        _ => 0,
                    },
                })]
            }
            Source::Local => vec![TelegramCommand::SearchCached(Request {
                id: self.search.request_id,
                pattern: self.search.query.value().to_owned(),
                chats: self.search.chats.clone(),
                before: match before {
                    Some(Cursor::Local(cursor)) => Some(cursor),
                    _ => None,
                },
            })],
        }
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

    pub(super) fn finish_search(&mut self, request_id: u64, result: Result<Results, String>) {
        if self.search.request_id != request_id || self.mode != Mode::Search {
            return;
        }
        self.search.loading = false;
        match result {
            Ok(mut page) => {
                if self.search.is_mentions() {
                    page.messages_mut().retain(|message| {
                        message
                            .mention
                            .as_ref()
                            .is_some_and(|mention| mention.unread)
                    });
                }
                if let (Source::Cloud { sender, .. }, Results::Cloud(page)) =
                    (&mut self.search.source, &page)
                {
                    *sender = page.sender;
                }
                self.search.page = Some(page);
                self.search.error = None;
            }
            Err(error) => {
                self.search.error = Some(crate::model::sanitize_terminal_text(&error));
                self.search.editing = !self.search.is_mentions();
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
        if self.search.target != Some((chat_id, message_id)) {
            return;
        }
        self.search.loading = false;
        self.search.target = None;
        if !messages
            .iter()
            .any(|message| message.id == message_id && message.chat_id == chat_id)
        {
            self.search.error =
                Some("Search result is no longer available; run the search again".to_owned());
            return;
        }
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
            "{} · {} latest · {} results",
            if self.search.is_mentions() {
                "Mention"
            } else if self.search.is_cloud() {
                "Cloud search result"
            } else {
                "Cached search result"
            },
            self.keymap
                .hint(crate::keymap::Context::Conversation, "latest"),
            self.keymap
                .hint(crate::keymap::Context::Conversation, "search")
        ));
    }

    pub(super) fn observe_search(&mut self, event: &NetworkEvent) {
        if let NetworkEvent::CacheInvalidated { chat_id } = event {
            let affected = match &self.search.source {
                Source::Cloud { chat_id: id, .. } | Source::Mentions(id) => {
                    chat_id.is_none_or(|chat| chat == *id)
                }
                Source::Local => chat_id.is_none_or(|chat| {
                    self.search
                        .chats
                        .as_ref()
                        .is_none_or(|chats| chats.contains(&chat))
                }),
            };
            if affected {
                self.search.request_id = self.search.request_id.wrapping_add(1);
                self.search.loading = false;
                self.search.page = None;
                self.search.target = None;
                self.search.editing = !self.search.is_mentions();
                self.search.error = Some("History is resyncing; run the search again".to_owned());
            }
        }
        let mentions = self.search.is_mentions();
        let Some(page) = self.search.page.as_mut() else {
            return;
        };
        match event {
            NetworkEvent::MessageUpdated(message) | NetworkEvent::NewMessage(message) => {
                if let Some(current) = page
                    .messages_mut()
                    .iter_mut()
                    .find(|current| current.chat_id == message.chat_id && current.id == message.id)
                {
                    *current = message.clone();
                    super::sanitize_message(current);
                }
            }
            NetworkEvent::MessageContentsRead {
                channel_id,
                message_ids,
            } => {
                for message in page.messages_mut() {
                    message.acknowledge_contents(*channel_id, message_ids);
                }
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                page.messages_mut().retain(|message| {
                    let affected = channel_id.map_or(message.chat_id > -1_000_000_000_000, |id| {
                        message.chat_id == id
                    });
                    !affected || !message_ids.contains(&message.id)
                });
                self.search.selected = self
                    .search
                    .selected
                    .min(page.messages().len().saturating_sub(1));
            }
            _ => {}
        }
        if mentions {
            page.messages_mut().retain(|message| {
                message
                    .mention
                    .as_ref()
                    .is_some_and(|mention| mention.unread)
            });
            self.search.selected = self
                .search
                .selected
                .min(page.messages().len().saturating_sub(1));
        }
    }
}
