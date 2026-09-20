//! Command editing reuses the normal `TextInput`, keymap and action dispatch.

use std::collections::{BTreeMap, VecDeque};

use super::{App, Focus, KeyAction, Mode, Screen, TelegramCommand, TextInput};
use crate::{
    actions::Action,
    commands::{self, COMMANDS, Kind, Spec, Target},
    event::ConnectionStatus,
    keymap::Context,
    model::{ChatId, sanitize_terminal_text},
    pins::{DialogAction, DialogScope},
};

const HISTORY_LIMIT: usize = 64;

#[derive(Clone)]
struct Origin {
    account: (u8, Option<i64>),
    focus: Focus,
    folder: i32,
    chat: Option<ChatId>,
    conversation: Option<ChatId>,
    message: Option<i32>,
    action: usize,
    sidebar_visible: bool,
}

#[derive(Clone)]
pub struct Candidate {
    pub input: String,
    pub label: String,
    pub description: String,
    pub shortcut: String,
    pub unavailable: bool,
}

#[derive(Clone, Default)]
pub struct State {
    pub input: TextInput,
    pub notice: Option<String>,
    pub selected: Option<usize>,
    pub hit_regions: Vec<(u16, u16, u16, usize)>,
    origin: Option<Origin>,
    completion: Option<Vec<Candidate>>,
    history: BTreeMap<i64, VecDeque<String>>,
    history_seed: Option<String>,
    history_index: Option<usize>,
}

impl State {
    pub(super) fn reset_session(&mut self) {
        let history = std::mem::take(&mut self.history);
        *self = Self {
            history,
            ..Self::default()
        };
    }

    fn edited(&mut self) {
        self.completion = None;
        self.selected = None;
        self.history_seed = None;
        self.history_index = None;
        self.notice = None;
    }
}

impl App {
    pub(super) fn begin_command(&mut self) -> Vec<TelegramCommand> {
        if self.screen != Screen::Main || self.mode != Mode::Navigate {
            return Vec::new();
        }
        self.commands.reset_session();
        self.commands.origin = Some(Origin {
            account: (self.active_account(), self.account_user_id),
            focus: self.focus,
            folder: self.folder_id,
            chat: if self.focus == Focus::Chats {
                self.selected_chat_entry().map(|chat| chat.id)
            } else {
                self.active_chat_id
            },
            conversation: self.active_chat_id,
            message: (self.focus == Focus::Conversation)
                .then_some(self.selected_message)
                .flatten(),
            action: self.selected_action,
            sidebar_visible: self.chat_pane_region.is_some(),
        });
        self.mode = Mode::Command;
        self.keymap.reset();
        self.status_message = None;
        Vec::new()
    }

    fn cancel_command(&mut self) {
        if let Some(origin) = &self.commands.origin {
            self.focus = origin.focus;
        }
        self.mode = Mode::Navigate;
        self.commands.reset_session();
        self.force_redraw = true;
    }

    pub(super) fn command_binding(&mut self, action: &Action) -> Option<Vec<TelegramCommand>> {
        if self.mode != Mode::Command {
            return None;
        }
        match action {
            Action::Cancel => self.cancel_command(),
            Action::Open => return Some(self.submit_command()),
            Action::CompleteNext => self.complete_command(true),
            Action::CompletePrevious => self.complete_command(false),
            Action::HistoryPrevious => self.command_history(true),
            Action::HistoryNext => self.command_history(false),
            Action::Quit
            | Action::NextAccount
            | Action::AddAccount
            | Action::Redraw
            | Action::Left
            | Action::Right
            | Action::Home
            | Action::End
            | Action::Backspace
            | Action::Delete
            | Action::Clear
            | Action::DeleteWord => return None,
            _ => {}
        }
        Some(Vec::new())
    }

    pub(super) fn edit_command(&mut self, action: KeyAction) -> Vec<TelegramCommand> {
        match action {
            KeyAction::Escape => self.cancel_command(),
            KeyAction::Enter => return self.submit_command(),
            KeyAction::Character(character) => self.commands.input.insert(character),
            KeyAction::Backspace => {
                self.commands.input.backspace();
            }
            KeyAction::Delete => {
                self.commands.input.delete();
            }
            KeyAction::Left => {
                self.commands.input.move_left();
            }
            KeyAction::Right => {
                self.commands.input.move_right();
            }
            KeyAction::Home => self.commands.input.move_home(),
            KeyAction::End => self.commands.input.move_end(),
            KeyAction::Clear => self.commands.input.clear(),
            KeyAction::DeleteWord => {
                self.commands.input.delete_word_before();
            }
            _ => return Vec::new(),
        }
        self.commands.edited();
        Vec::new()
    }

    pub(super) fn paste_command(&mut self, text: &str) {
        self.commands
            .input
            .insert_str(&sanitize_terminal_text(text).replace(['\n', '\r'], " "));
        self.commands.edited();
    }

    #[must_use]
    pub fn command_target_label(&self) -> String {
        let Some(origin) = &self.commands.origin else {
            return String::new();
        };
        let (name, _) = commands::split(self.commands.input.value());
        let target = if commands::find(name).is_some_and(|spec| {
            matches!(
                spec.target,
                Target::Conversation | Target::Message | Target::Attachment | Target::Preview
            )
        }) {
            origin.conversation
        } else {
            origin.chat
        };
        let chat = target.and_then(|id| self.chats.iter().find(|chat| chat.id == id));
        format!(
            "Account {} · {}{}",
            origin.account.0,
            chat.map_or("No chat selected", |chat| chat.title.as_str()),
            origin
                .message
                .map_or_else(String::new, |id| format!(" · #{id}"))
        )
    }

    fn target_error(&self, target: Target) -> Option<String> {
        let origin = self.commands.origin.as_ref()?;
        let chat = origin
            .chat
            .and_then(|id| self.chats.iter().find(|chat| chat.id == id));
        let message = origin
            .conversation
            .and_then(|id| self.messages.get(&id))
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| Some(message.id) == origin.message)
            });
        let error = match target {
            Target::None => return None,
            Target::Chat if chat.is_some() => return None,
            Target::Chat => "Select a chat first",
            Target::Conversation
                if origin.conversation.is_some() && origin.conversation == self.active_chat_id =>
            {
                return None;
            }
            Target::Conversation => "Open a conversation first",
            Target::Message if message.is_some() => return None,
            Target::Message => "Select a message that is still available",
            Target::Attachment if message.is_some_and(|message| message.attachment.is_some()) => {
                return None;
            }
            Target::Attachment => "Select a message with an attachment",
            Target::Preview
                if message.is_some_and(|message| {
                    message
                        .attachment
                        .as_ref()
                        .is_some_and(crate::model::Attachment::supports_preview)
                }) =>
            {
                return None;
            }
            Target::Preview => "Select an image or sticker",
        };
        Some(error.to_owned())
    }

    fn command_unavailable(&self, spec: &Spec) -> Option<String> {
        self.target_error(spec.target).or_else(|| {
            (matches!(
                spec.kind,
                Kind::Pin(_)
                    | Kind::Archive(_)
                    | Kind::Read(_)
                    | Kind::Copy
                    | Kind::Forward
                    | Kind::Mute(_)
                    | Kind::Mentions
                    | Kind::Open
                    | Kind::Join
            ) && self.connection != ConnectionStatus::Online)
                .then(|| "Connect to Telegram first".to_owned())
        })
    }

    #[must_use]
    pub fn command_candidates(&self) -> Vec<Candidate> {
        if let Some(candidates) = &self.commands.completion {
            return candidates.clone();
        }
        let (name, argument) = commands::split(self.commands.input.value());
        let Some(argument) = argument else {
            let context = if self
                .commands
                .origin
                .as_ref()
                .is_some_and(|origin| origin.focus == Focus::Chats)
            {
                Context::Chats
            } else {
                Context::Conversation
            };
            let mut candidates: Vec<_> = COMMANDS
                .iter()
                .filter(|spec| spec.name.starts_with(name) || spec.alias == Some(name))
                .map(|spec| {
                    let reason = self.command_unavailable(spec);
                    Candidate {
                        input: format!(
                            "{}{}",
                            spec.name,
                            if spec.arguments.is_empty() { "" } else { " " }
                        ),
                        label: spec.usage(),
                        description: reason
                            .clone()
                            .unwrap_or_else(|| spec.description().to_owned()),
                        shortcut: spec.action().map_or_else(String::new, |action| {
                            self.keymap.hint(context, action.name())
                        }),
                        unavailable: reason.is_some(),
                    }
                })
                .collect();
            candidates
                .sort_by_key(|candidate| (candidate.unavailable, !candidate.input.trim().eq(name)));
            return candidates;
        };
        let Some(spec) = commands::find(name) else {
            return Vec::new();
        };
        self.command_argument_candidates(spec, argument)
    }

    fn command_argument_candidates(&self, spec: &Spec, argument: &str) -> Vec<Candidate> {
        let query = argument.trim().to_lowercase();
        let mut candidates = Vec::new();
        let mut add = |value: String, label: String, description: String| {
            if value.to_lowercase().contains(&query) || label.to_lowercase().contains(&query) {
                candidates.push(Candidate {
                    input: format!("{} {value}", spec.name),
                    label,
                    description,
                    shortcut: String::new(),
                    unavailable: false,
                });
            }
        };
        match spec.kind {
            Kind::Search => Self::add_command_search(&mut add),
            Kind::Forward => {
                add(
                    "saved".to_owned(),
                    "Saved Messages".to_owned(),
                    "This account's personal storage".to_owned(),
                );
                self.add_command_chats(&mut add);
            }
            Kind::Chat => self.add_command_chats(&mut add),
            Kind::Folder => {
                for folder in &self.folders {
                    add(
                        folder.id.to_string(),
                        folder.title.clone(),
                        format!("Folder {}", folder.id),
                    );
                }
            }
            Kind::Account => {
                for slot in 1..=self.settings.account_count {
                    add(
                        slot.to_string(),
                        format!("Account {slot}"),
                        if slot == self.active_account() {
                            "Current account"
                        } else {
                            "Switch account"
                        }
                        .to_owned(),
                    );
                }
            }
            Kind::Pin(_) | Kind::Color => {
                add(
                    "chat".to_owned(),
                    "chat".to_owned(),
                    "Selected chat".to_owned(),
                );
                let (value, description) = if matches!(spec.kind, Kind::Color) {
                    ("folder", "Current folder")
                } else {
                    ("message", "Selected message")
                };
                add(value.to_owned(), value.to_owned(), description.to_owned());
            }
            Kind::Mute(true) => {
                for value in ["1h", "8h", "2d", "forever"] {
                    add(
                        value.to_owned(),
                        value.to_owned(),
                        "Mute this chat on Telegram".to_owned(),
                    );
                }
            }
            Kind::Copy => {
                for value in ["text", "link"] {
                    add(
                        value.to_owned(),
                        value.to_owned(),
                        "Copy selected message".to_owned(),
                    );
                }
            }
            Kind::Sidebar => {
                for value in ["show", "hide", "toggle"] {
                    add(
                        value.to_owned(),
                        value.to_owned(),
                        "Sidebar visibility".to_owned(),
                    );
                }
            }
            Kind::Help => {
                for spec in COMMANDS {
                    add(
                        spec.name.to_owned(),
                        spec.usage(),
                        spec.description().to_owned(),
                    );
                }
            }
            _ => {}
        }
        candidates
    }

    fn add_command_search(add: &mut impl FnMut(String, String, String)) {
        add(
            "--cloud".to_owned(),
            "Telegram search".to_owned(),
            "Current chat · sender, UTC date and media filters · requires connection".to_owned(),
        );
        add(
            "--cloud --from me".to_owned(),
            "Messages from me".to_owned(),
            "Search this chat for your messages".to_owned(),
        );
        for media in <crate::cloud_search::Media as clap::ValueEnum>::value_variants() {
            let value = clap::ValueEnum::to_possible_value(media).expect("media value");
            let name = value.get_name();
            add(
                format!("--cloud --media {name}"),
                format!("Media: {name}"),
                "Filter Telegram history; enter optional search text".to_owned(),
            );
        }
    }

    fn add_command_chats(&self, add: &mut impl FnMut(String, String, String)) {
        for (alias, id) in &self.keymap.chats {
            if let Some(chat) = self.chats.iter().find(|chat| chat.id == *id) {
                add(alias.clone(), alias.clone(), chat.title.clone());
            }
        }
        for chat in &self.chats {
            add(
                chat.id.to_string(),
                chat.title.clone(),
                format!(
                    "Chat {}{}",
                    chat.id,
                    if chat.membership.archived {
                        " · Archive"
                    } else {
                        ""
                    }
                ),
            );
        }
    }

    fn complete_command(&mut self, forward: bool) {
        let candidates = self.command_candidates();
        if candidates.is_empty() {
            return;
        }
        let index = self.commands.selected.map_or(
            if forward { 0 } else { candidates.len() - 1 },
            |index| {
                if forward {
                    (index + 1) % candidates.len()
                } else {
                    (index + candidates.len() - 1) % candidates.len()
                }
            },
        );
        self.commands
            .input
            .set_value(candidates[index].input.clone());
        self.commands.selected = Some(index);
        self.commands.completion = Some(candidates);
        self.commands.notice = None;
        self.commands.history_seed = None;
        self.commands.history_index = None;
    }

    pub(super) fn click_command(&mut self, index: usize) {
        let candidates = self.command_candidates();
        if let Some(candidate) = candidates.get(index) {
            self.commands.input.set_value(candidate.input.clone());
            self.commands.edited();
        }
    }

    fn command_history(&mut self, older: bool) {
        let account = self
            .account_user_id
            .unwrap_or(-i64::from(self.active_account()));
        let state = &mut self.commands;
        let seed = state
            .history_seed
            .get_or_insert_with(|| state.input.value().to_owned());
        let history: Vec<_> = state
            .history
            .get(&account)
            .into_iter()
            .flatten()
            .filter(|line| line.starts_with(seed.as_str()))
            .rev()
            .collect();
        let index = if older {
            Some(state.history_index.map_or(0, |index| index + 1))
        } else {
            state.history_index.and_then(|index| index.checked_sub(1))
        };
        if let Some(index) = index {
            let Some(line) = history.get(index) else {
                return;
            };
            state.input.set_value((*line).clone());
        } else {
            state.input.set_value(seed.clone());
        }
        state.history_index = index;
        state.completion = None;
        state.selected = None;
        state.notice = None;
    }

    fn command_error(&mut self, error: impl Into<String>) -> Vec<TelegramCommand> {
        self.commands.completion = None;
        self.commands.selected = None;
        self.commands.notice = Some(error.into());
        self.mode = Mode::Command;
        Vec::new()
    }

    fn resolve_command_chat(&self, value: &str) -> Result<ChatId, String> {
        if let Some(&id) = self.keymap.chats.get(value)
            && self.chats.iter().any(|chat| chat.id == id)
        {
            return Ok(id);
        }
        if let Ok(id) = value.parse::<ChatId>()
            && self.chats.iter().any(|chat| chat.id == id)
        {
            return Ok(id);
        }
        let titles: Vec<_> = self
            .chats
            .iter()
            .filter(|chat| chat.title.eq_ignore_ascii_case(value))
            .collect();
        if let [chat] = titles.as_slice() {
            return Ok(chat.id);
        }
        Err(
            "Choose a chat with Tab; Enter accepts an exact alias, ID or unique full title"
                .to_owned(),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn submit_command(&mut self) -> Vec<TelegramCommand> {
        let input = self.commands.input.value().to_owned();
        let (name, raw_argument) = commands::split(&input);
        if name.is_empty() {
            self.cancel_command();
            return Vec::new();
        }
        let Some(spec) = commands::find(name) else {
            return self.command_error("Unknown command · Tab to complete · help to browse");
        };
        let Some(origin) = self.commands.origin.clone() else {
            return Vec::new();
        };
        if origin.account != (self.active_account(), self.account_user_id) {
            return self.command_error("The account changed; close and reopen commands");
        }
        let argument = raw_argument.unwrap_or_default().trim();
        if spec.arguments.starts_with('<') && argument.is_empty() {
            self.commands.input.set_value(format!("{} ", spec.name));
            return self.command_error(format!("Usage: :{} · Tab to choose", spec.usage()));
        }
        if spec.arguments.is_empty() && !argument.is_empty() {
            return self.command_error(format!("Usage: :{}", spec.usage()));
        }
        if let Some(error) = self.command_unavailable(spec) {
            return self.command_error(error);
        }
        if matches!(spec.kind, Kind::Help) {
            let help = if argument.is_empty() {
                None
            } else {
                commands::find(argument)
            };
            if !argument.is_empty() && help.is_none() {
                return self.command_error("Unknown command in help");
            }
            self.commands
                .input
                .set_value(help.map_or_else(String::new, |spec| format!("{} ", spec.name)));
            self.commands.edited();
            self.commands.notice = Some(help.map_or_else(
                || "Tab completes · Enter runs · ↑/↓ history · Esc returns · ? opens keyboard help outside commands".to_owned(),
                |spec| format!(":{} · {}{}", spec.usage(), spec.description(), self.command_unavailable(spec).map_or_else(String::new, |reason| format!(" · {reason}")))));
            return Vec::new();
        }
        // Resolve and validate arguments before leaving COMMAND or changing focus.
        let invite =
            if matches!(spec.kind, Kind::Join) {
                match crate::invites::hash(argument) {
                    Some(hash) => Some(hash),
                    None => return self.command_error(
                        "Use :join https://t.me/+hash, t.me/joinchat/hash or tg://join?invite=hash",
                    ),
                }
            } else {
                None
            };
        let open_target = if matches!(spec.kind, Kind::Open) {
            match crate::chat_discovery::target(argument) {
                Ok(target) => Some(target),
                Err(error) => return self.command_error(error),
            }
        } else {
            None
        };
        let mute = match spec.kind {
            Kind::Mute(true) => match crate::notifications::Mute::parse(argument) {
                Some(mute) => mute,
                None => return self.command_error("Use :mute 1h, 8h, 2d or forever"),
            },
            _ => crate::notifications::Mute::Off,
        };
        let cloud_search = if matches!(spec.kind, Kind::Search) {
            if let Some(value) = argument
                .strip_prefix("--cloud")
                .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            {
                let Some(chat_id) = origin.conversation else {
                    return self.command_error("Open a chat before using :search --cloud");
                };
                match crate::cloud_search::parse(value) {
                    Ok((query, filters)) => Some((chat_id, query, filters)),
                    Err(error) => return self.command_error(error.to_string()),
                }
            } else {
                None
            }
        } else {
            None
        };
        let chat = if matches!(spec.kind, Kind::Chat | Kind::Forward) {
            let target = if matches!(spec.kind, Kind::Forward) && argument == "saved" {
                self.account_user_id
                    .ok_or_else(|| "Account identity is not ready".to_owned())
            } else {
                self.resolve_command_chat(argument)
            };
            match target {
                Ok(id) => Some(id),
                Err(error) => return self.command_error(error),
            }
        } else {
            None
        };
        let folder = if matches!(spec.kind, Kind::Folder) {
            let matches: Vec<_> = self
                .folders
                .iter()
                .filter(|folder| {
                    argument == folder.id.to_string() || folder.title.eq_ignore_ascii_case(argument)
                })
                .map(|folder| folder.id)
                .collect();
            if let [id] = matches.as_slice() {
                Some(*id)
            } else {
                return self
                    .command_error("Choose a folder with Tab or enter its ID or unique full name");
            }
        } else {
            None
        };
        let account = if matches!(spec.kind, Kind::Account) && !argument.is_empty() {
            match argument.parse::<u8>() {
                Ok(slot) if (1..=self.settings.account_count).contains(&slot) => Some(slot),
                _ => return self.command_error("Choose an existing account slot"),
            }
        } else {
            None
        };
        match spec.kind {
            Kind::Copy if !matches!(argument, "" | "text" | "link") => {
                return self.command_error(format!("Usage: :{}", spec.usage()));
            }
            Kind::Pin(_) if !matches!(argument, "chat" | "message") => {
                return self.command_error(format!("Usage: :{}", spec.usage()));
            }
            Kind::Color if !matches!(argument, "chat" | "folder") => {
                return self.command_error(format!("Usage: :{}", spec.usage()));
            }
            Kind::Sidebar if !matches!(argument, "" | "show" | "hide" | "toggle") => {
                return self.command_error(format!("Usage: :{}", spec.usage()));
            }
            _ => {}
        }
        if matches!(spec.kind, Kind::Pin(_))
            && argument == "message"
            && let Some(error) = self.target_error(Target::Message)
        {
            return self.command_error(error);
        }
        if (matches!(spec.kind, Kind::Archive(_))
            || (argument == "chat" && matches!(spec.kind, Kind::Pin(_) | Kind::Color)))
            && let Some(error) = self.target_error(Target::Chat)
        {
            return self.command_error(error);
        }
        if ((matches!(spec.kind, Kind::Pin(_)) && argument == "chat")
            || (matches!(spec.kind, Kind::Color) && argument == "folder"))
            && !self.folders.iter().any(|folder| folder.id == origin.folder)
        {
            return self.command_error("The folder was removed; select a folder again");
        }
        self.focus = origin.focus;
        if matches!(
            spec.target,
            Target::Message | Target::Attachment | Target::Preview
        ) || matches!(spec.kind, Kind::Action(Action::EditMessage))
            || (matches!(spec.kind, Kind::Pin(_)) && argument == "message")
        {
            if origin.conversation != self.active_chat_id {
                return self.command_error("The conversation changed; select the message again");
            }
            self.selected_message = origin.message;
            self.selected_action = origin.action;
            self.focus = Focus::Conversation;
        }
        if spec.target == Target::Conversation {
            self.focus = Focus::Conversation;
        }
        let history = self
            .commands
            .history
            .entry(origin.account.1.unwrap_or(-i64::from(origin.account.0)))
            .or_default();
        history.retain(|line| line != &input);
        history.push_back(input.clone());
        if history.len() > HISTORY_LIMIT {
            history.pop_front();
        }
        self.mode = Mode::Navigate;
        self.status_message = None;
        let outgoing = match &spec.kind {
            Kind::Forward => self.review_forward(chat.expect("validated destination")),
            Kind::Copy => self.copy_message(argument == "link"),
            Kind::Read(unread) => {
                self.set_chat_unread(origin.chat.expect("validated chat"), *unread)
            }
            Kind::Action(action) => self.run_action(action, 1),
            Kind::Attach | Kind::Paste => {
                let id = origin.chat.expect("validated chat target");
                let mut commands = if self.active_chat_id == Some(id) {
                    Vec::new()
                } else {
                    self.open_chat_by_id(id)
                };
                commands.extend(if matches!(spec.kind, Kind::Paste) {
                    self.paste_clipboard()
                } else {
                    self.prepare_attachments(raw_argument.unwrap_or_default().to_owned(), false)
                });
                commands
            }
            Kind::Join => self.begin_invite(invite.expect("validated invite")),
            Kind::Chat => self.open_chat_by_id(chat.expect("validated chat")),
            Kind::Open => self.activate_url(open_target.as_deref().expect("validated target")),
            Kind::Folder => {
                self.folder_id = folder.expect("validated folder");
                self.filter.clear();
                self.selected_chat = 0;
                self.focus = Focus::Chats;
                self.sidebar_hidden = false;
                self.narrow_conversation = false;
                Vec::new()
            }
            Kind::Account => {
                if let Some(account) = account {
                    self.activate_account(account, self.settings.account_count)
                } else {
                    self.run_action(&Action::Accounts, 1)
                }
            }
            Kind::Mentions => origin
                .chat
                .map_or_else(Vec::new, |id| self.start_mentions(id)),
            Kind::Mute(_) => origin
                .chat
                .map_or_else(Vec::new, |id| self.set_chat_mute(id, mute)),
            Kind::Search => {
                if let Some((chat_id, query, filters)) = cloud_search {
                    self.start_cloud_search(chat_id, query, filters)
                } else {
                    self.start_local_search(raw_argument)
                }
            }
            Kind::Pin(pinned) => {
                let current = if argument == "chat" {
                    origin
                        .chat
                        .is_some_and(|id| self.chat_pin_position_in(origin.folder, id).is_some())
                } else {
                    self.active_messages()
                        .iter()
                        .any(|message| Some(message.id) == origin.message && message.pinned)
                };
                if current == *pinned {
                    self.status_message = Some(
                        if *pinned {
                            "Already pinned"
                        } else {
                            "Already unpinned"
                        }
                        .to_owned(),
                    );
                    Vec::new()
                } else if argument == "chat" {
                    let scope = match origin.folder {
                        0 => DialogScope::Main,
                        1 => DialogScope::Archive,
                        id => DialogScope::Filter(id),
                    };
                    self.request_chat_pin(
                        origin.chat.expect("validated chat"),
                        scope,
                        DialogAction::Set(*pinned),
                    )
                } else {
                    self.run_action(&Action::Pin, 1)
                }
            }
            Kind::Archive(archived) => {
                if self
                    .chats
                    .iter()
                    .find(|chat| Some(chat.id) == origin.chat)
                    .is_some_and(|chat| chat.membership.archived == *archived)
                {
                    self.status_message = Some(
                        if *archived {
                            "Already archived"
                        } else {
                            "Already outside Archive"
                        }
                        .to_owned(),
                    );
                    Vec::new()
                } else {
                    self.set_chat_archived(origin.chat.expect("validated chat"), *archived)
                }
            }
            Kind::Sidebar => {
                let show = match argument {
                    "show" => true,
                    "hide" => false,
                    _ => !origin.sidebar_visible,
                };
                if show == origin.sidebar_visible {
                    Vec::new()
                } else {
                    self.run_action(&Action::ToggleSidebar, 1)
                }
            }
            Kind::Color => {
                let (target, label) = if argument == "folder" {
                    let folder = self
                        .folders
                        .iter()
                        .find(|folder| folder.id == origin.folder)
                        .expect("validated folder");
                    (
                        crate::appearance::Target::Folder(folder.id),
                        format!("Folder: {}", folder.title),
                    )
                } else {
                    let chat = self
                        .chats
                        .iter()
                        .find(|chat| Some(chat.id) == origin.chat)
                        .expect("validated chat");
                    (
                        crate::appearance::Target::Chat(chat.id),
                        format!("Chat: {}", chat.title),
                    )
                };
                self.begin_color_picker_for(target, label);
                Vec::new()
            }
            Kind::Status => {
                self.mode = Mode::Status;
                Vec::new()
            }
            Kind::Help => unreachable!("help stays in command entry"),
        };
        self.commands.reset_session();
        self.force_redraw = true;
        outgoing
    }
}
