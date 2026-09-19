use super::{App, Focus, Mode, TelegramCommand};
use crate::{
    event::ConnectionStatus,
    model::{ChatId, ChatKind, Message, sanitize_terminal_line},
    pins::{MessageAction, MessagePage},
};

#[derive(Clone, Default)]
pub struct State {
    pub chat: Option<ChatId>,
    pub page: MessagePage,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub prompt: Option<Prompt>,
    pub head: Option<(ChatId, MessagePage)>,
    pub starts: Vec<i32>,
    request_id: u64,
    next_request: u64,
    pending_mutation: Option<(ChatId, u64)>,
    dirty: bool,
    opening: Option<i32>,
}

#[derive(Clone)]
pub struct Prompt {
    pub chat_id: ChatId,
    pub message_id: i32,
    pub title: String,
    pub preview: String,
    pub options: Vec<(String, MessageAction)>,
    pub selection: usize,
    previous_mode: Mode,
}

impl App {
    pub(super) fn update_pin_preview(&mut self, message: &Message) {
        let state = &mut self.message_pins;
        if let Some((chat_id, page)) = &mut state.head
            && *chat_id == message.chat_id
        {
            for current in &mut page.messages {
                if current.id == message.id {
                    *current = message.clone();
                }
            }
        }
        if state.chat == Some(message.chat_id) {
            for current in &mut state.page.messages {
                if current.id == message.id {
                    *current = message.clone();
                }
            }
            state.dirty |= state.loading && message.pinned;
        }
    }
    pub(super) fn pin_context_chat(&self) -> Option<ChatId> {
        (self.mode == Mode::PinnedMessages && self.message_pins.opening.is_some())
            .then_some(self.message_pins.chat)
            .flatten()
    }
    pub(super) fn fail_pinned_messages(&mut self, chat_id: ChatId, request_id: u64, error: &str) {
        if request_id != 0
            && self.message_pins.chat == Some(chat_id)
            && self.message_pins.request_id == request_id
        {
            self.message_pins.loading = false;
            self.message_pins.opening = None;
            self.message_pins.error = Some(sanitize_terminal_line(error));
        }
    }

    #[must_use]
    pub fn pinned_summary(&self) -> Option<(&Message, Option<usize>)> {
        if let Some((chat, page)) = &self.message_pins.head
            && Some(*chat) == self.active_chat_id
        {
            return page.messages.first().map(|message| (message, page.total));
        }
        self.active_messages()
            .iter()
            .rev()
            .find(|message| message.pinned)
            .map(|message| (message, None))
    }
    pub(super) fn open_pinned_messages(&mut self) -> Vec<TelegramCommand> {
        let Some(chat_id) = self.active_chat_id else {
            return Vec::new();
        };
        self.message_pins.chat = Some(chat_id);
        self.message_pins.starts = vec![0];
        self.message_pins.page = MessagePage::default();
        self.mode = Mode::PinnedMessages;
        self.load_pin_page(0)
    }

    fn load_pin_page(&mut self, before: i32) -> Vec<TelegramCommand> {
        let Some(chat_id) = self.message_pins.chat else {
            return Vec::new();
        };
        let state = &mut self.message_pins;
        state.next_request = state.next_request.wrapping_add(1).max(1);
        state.request_id = state.next_request;
        state.loading = true;
        state.dirty = false;
        state.error = None;
        state.opening = None;
        state.selected = 0;
        vec![TelegramCommand::LoadPinnedMessages {
            chat_id,
            before,
            request_id: state.request_id,
        }]
    }

    pub(super) fn pin_binding(
        &mut self,
        action: &str,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if self.mode == Mode::PinPrompt {
            return self.pin_prompt_binding(action);
        }
        if self.mode != Mode::PinnedMessages {
            return None;
        }
        match action {
            "cancel" => {
                self.mode = Mode::Navigate;
                self.message_pins.request_id = 0;
                self.message_pins.loading = false;
                self.message_pins.opening = None;
            }
            "up" | "down" | "page_up" | "page_down" => {
                let state = &mut self.message_pins;
                let step = if action.starts_with("page_") {
                    count.saturating_mul(10)
                } else {
                    count
                };
                state.selected = if action.ends_with("up") {
                    state.selected.saturating_sub(step)
                } else {
                    state
                        .selected
                        .saturating_add(step)
                        .min(state.page.messages.len().saturating_sub(1))
                };
            }
            "pins_more" if !self.message_pins.loading => {
                if let Some(next) = self.message_pins.page.next {
                    self.message_pins.starts.push(next);
                    return Some(self.load_pin_page(next));
                }
            }
            "pins_previous" if !self.message_pins.loading && self.message_pins.starts.len() > 1 => {
                self.message_pins.starts.pop();
                return Some(self.load_pin_page(*self.message_pins.starts.last().unwrap_or(&0)));
            }
            "refresh" => {
                return Some(self.load_pin_page(*self.message_pins.starts.last().unwrap_or(&0)));
            }
            "pin" => return Some(self.begin_message_pin(false)),
            "unpin_all" => return Some(self.begin_message_pin(true)),
            "open" => {
                if let Some(message) = self
                    .message_pins
                    .page
                    .messages
                    .get(self.message_pins.selected)
                {
                    let (chat_id, message_id) = (message.chat_id, message.id);
                    self.message_pins.opening = Some(message_id);
                    self.active_history_request = None;
                    self.history_changes.clear();
                    self.history_read_max = 0;
                    self.message_pins.next_request =
                        self.message_pins.next_request.wrapping_add(1).max(1);
                    self.message_pins.request_id = self.message_pins.next_request;
                    self.message_pins.loading = true;
                    self.message_pins.error = None;
                    return Some(vec![TelegramCommand::LoadPinnedContext {
                        chat_id,
                        message_id,
                        request_id: self.message_pins.request_id,
                    }]);
                }
            }
            _ => return None,
        }
        Some(Vec::new())
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn begin_message_pin(&mut self, all: bool) -> Vec<TelegramCommand> {
        if self.connection != ConnectionStatus::Online {
            self.status_message = Some("Connect to Telegram to change message pins".to_owned());
            return Vec::new();
        }
        if self.message_pins.pending_mutation.is_some() {
            return Vec::new();
        }
        let message = if self.mode == Mode::PinnedMessages {
            self.message_pins
                .page
                .messages
                .get(self.message_pins.selected)
        } else if self.mode == Mode::Navigate && self.focus == Focus::Conversation {
            self.active_messages()
                .iter()
                .find(|message| Some(message.id) == self.selected_message)
        } else {
            return Vec::new();
        };
        let Some(message) = message.filter(|message| message.id > 0) else {
            self.status_message = Some("Select a sent message first".to_owned());
            return Vec::new();
        };
        let private = self
            .active_chat()
            .is_some_and(|chat| chat.kind == ChatKind::Direct);
        let own = self.account_user_id == Some(message.chat_id);
        let old = self
            .message_pins
            .head
            .as_ref()
            .filter(|(chat, _)| *chat == message.chat_id)
            .and_then(|(_, page)| page.messages.first())
            .is_some_and(|head| head.id > message.id);
        let (title, options) = if all {
            (
                "Unpin all messages?",
                vec![("Unpin all messages".to_owned(), MessageAction::UnpinAll)],
            )
        } else if message.pinned {
            (
                "Unpin this message?",
                vec![("Unpin message".to_owned(), MessageAction::Unpin)],
            )
        } else if private && !own {
            (
                "Pin this message?",
                vec![
                    (
                        "Pin only for me".to_owned(),
                        MessageAction::Pin {
                            notify: false,
                            only_self: true,
                        },
                    ),
                    (
                        "Pin for both participants".to_owned(),
                        MessageAction::Pin {
                            notify: false,
                            only_self: false,
                        },
                    ),
                ],
            )
        } else if !private && !old {
            (
                "Pin this message?",
                vec![
                    (
                        "Pin and notify members".to_owned(),
                        MessageAction::Pin {
                            notify: true,
                            only_self: false,
                        },
                    ),
                    (
                        "Pin silently".to_owned(),
                        MessageAction::Pin {
                            notify: false,
                            only_self: false,
                        },
                    ),
                ],
            )
        } else {
            (
                if old {
                    "Pin this older message?"
                } else {
                    "Pin this message?"
                },
                vec![(
                    "Pin message".to_owned(),
                    MessageAction::Pin {
                        notify: false,
                        only_self: false,
                    },
                )],
            )
        };
        self.message_pins.prompt = Some(Prompt {
            chat_id: message.chat_id,
            message_id: message.id,
            title: title.to_owned(),
            preview: sanitize_terminal_line(&format!(
                "#{} {}: {}",
                message.id, message.sender, message.text
            )),
            options,
            selection: 0,
            previous_mode: self.mode,
        });
        self.message_pins.error = None;
        self.mode = Mode::PinPrompt;
        Vec::new()
    }

    fn pin_prompt_binding(&mut self, action: &str) -> Option<Vec<TelegramCommand>> {
        if action == "cancel" {
            if let Some(prompt) = self.message_pins.prompt.take() {
                self.mode = prompt.previous_mode;
            }
            return Some(Vec::new());
        }
        let Some(prompt) = self.message_pins.prompt.as_mut() else {
            return Some(Vec::new());
        };
        if self.message_pins.pending_mutation.is_some() {
            return Some(Vec::new());
        }
        match action {
            "up" => prompt.selection = prompt.selection.saturating_sub(1),
            "down" => {
                prompt.selection =
                    (prompt.selection + 1).min(prompt.options.len().saturating_sub(1));
            }
            "open" => {
                self.message_pins.next_request =
                    self.message_pins.next_request.wrapping_add(1).max(1);
                let request_id = self.message_pins.next_request;
                self.message_pins.pending_mutation = Some((prompt.chat_id, request_id));
                self.message_pins.error = None;
                self.status_message = Some("Updating pinned messages…".to_owned());
                return Some(vec![TelegramCommand::ChangeMessagePin {
                    chat_id: prompt.chat_id,
                    message_id: prompt.message_id,
                    action: prompt.options[prompt.selection].1,
                    request_id,
                }]);
            }
            _ => return None,
        }
        Some(Vec::new())
    }

    pub(super) fn finish_message_pin(
        &mut self,
        chat_id: ChatId,
        request_id: u64,
        error: Option<String>,
    ) -> Vec<TelegramCommand> {
        if self.message_pins.pending_mutation != Some((chat_id, request_id)) {
            return Vec::new();
        }
        self.message_pins.pending_mutation = None;
        if let Some(error) = error {
            let error = sanitize_terminal_line(&error);
            self.message_pins.error = Some(error.clone());
            self.status_message = Some(error);
            return Vec::new();
        }
        if let Some(prompt) = self.message_pins.prompt.take()
            && self.mode == Mode::PinPrompt
        {
            self.mode = prompt.previous_mode;
        }
        self.status_message = Some("Pinned messages updated".to_owned());
        if self.mode == Mode::PinnedMessages {
            self.message_pins.starts = vec![0];
            return self.load_pin_page(0);
        }
        Vec::new()
    }

    pub(super) fn finish_pinned_messages(
        &mut self,
        chat_id: ChatId,
        request_id: u64,
        page: MessagePage,
    ) -> Vec<TelegramCommand> {
        if !self.message_pins.dirty
            && page.before == 0
            && self.active_chat_id == Some(chat_id)
            && (request_id == 0
                || (self.mode == Mode::PinnedMessages
                    && self.message_pins.request_id == request_id))
        {
            self.message_pins.head = Some((chat_id, page.clone()));
        }
        if request_id == 0
            || self.message_pins.chat != Some(chat_id)
            || self.message_pins.request_id != request_id
        {
            return Vec::new();
        }
        if self.message_pins.dirty && page.total.is_some() {
            self.message_pins.starts = vec![0];
            return self.load_pin_page(0);
        }
        self.message_pins.loading =
            page.total.is_none() && self.connection == ConnectionStatus::Online;
        self.message_pins.page = page;
        self.message_pins.selected = self
            .message_pins
            .selected
            .min(self.message_pins.page.messages.len().saturating_sub(1));
        Vec::new()
    }

    pub(super) fn pinned_messages_deleted(
        &mut self,
        channel_id: Option<ChatId>,
        ids: &[i32],
    ) -> Vec<TelegramCommand> {
        let mut commands = Vec::new();
        let mut chats = self
            .message_pins
            .chat
            .into_iter()
            .chain(self.message_pins.head.as_ref().map(|(id, _)| *id))
            .collect::<Vec<_>>();
        chats.sort_unstable();
        chats.dedup();
        for chat_id in chats {
            if channel_id.map_or(chat_id > -1_000_000_000_000, |id| id == chat_id) {
                commands.extend(self.pin_messages_changed(chat_id, Some(ids), false));
            }
        }
        commands
    }

    pub(super) fn pin_messages_changed(
        &mut self,
        chat_id: ChatId,
        ids: Option<&[i32]>,
        pinned: bool,
    ) -> Vec<TelegramCommand> {
        let affected = |id: i32| ids.is_none_or(|ids| ids.contains(&id));
        let changed: Vec<_> = self
            .messages
            .get_mut(&chat_id)
            .into_iter()
            .flatten()
            .filter(|message| affected(message.id))
            .map(|message| {
                message.pinned = pinned;
                message.clone()
            })
            .collect();
        for message in changed {
            self.track_snapshot_changes(&crate::event::NetworkEvent::MessageUpdated(message));
        }
        if let Some((chat, page)) = &mut self.message_pins.head
            && *chat == chat_id
            && !pinned
        {
            page.messages.retain(|message| !affected(message.id));
        }
        if self.message_pins.chat == Some(chat_id) {
            if !pinned {
                self.message_pins
                    .page
                    .messages
                    .retain(|message| !affected(message.id));
            }
            if self.message_pins.loading {
                self.message_pins.dirty = true;
            } else if self.mode == Mode::PinnedMessages {
                self.message_pins.starts = vec![0];
                return self.load_pin_page(0);
            }
        }
        Vec::new()
    }

    pub(super) fn finish_pinned_context(
        &mut self,
        chat_id: ChatId,
        message_id: i32,
        request_id: u64,
        mut messages: Vec<Message>,
    ) {
        if self.mode != Mode::PinnedMessages
            || self.message_pins.chat != Some(chat_id)
            || self.message_pins.request_id != request_id
        {
            return;
        }
        messages.retain(|message| !self.history_changes.contains_key(&message.id));
        messages.extend(
            self.history_changes
                .values()
                .flatten()
                .filter(|message| message.id <= message_id)
                .cloned(),
        );
        if !messages.iter().any(|message| message.id == message_id) {
            self.fail_pinned_messages(chat_id, request_id, "Pinned message was deleted");
            return;
        }
        self.message_pins.opening = None;
        self.message_pins.loading = false;
        self.mode = Mode::Navigate;
        self.focus = Focus::Conversation;
        self.narrow_conversation = true;
        self.active_history_request = None;
        self.older_motion = None;
        self.loading_history = false;
        self.history_target_message = None;
        self.history_changes.clear();
        self.merge_history(chat_id, messages, None);
        self.browsing_older = true;
        self.new_messages_while_scrolled = 0;
        self.new_messages_to_anchor = 0;
        self.select_message_id(message_id, true);
        self.message_scroll = self.message_scroll.max(1);
        self.status_message = Some(format!(
            "Pinned message · {} latest · {} pins",
            self.keymap
                .hint(crate::keymap::Context::Conversation, "latest"),
            self.keymap
                .hint(crate::keymap::Context::Conversation, "pins")
        ));
    }
}
