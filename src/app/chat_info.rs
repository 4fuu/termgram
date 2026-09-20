use super::{App, Mode, Screen, TelegramCommand};
use crate::{
    actions::Action,
    chat_info::{Content, Info},
    event::{ConnectionStatus, NetworkEvent},
    model::{sanitize_terminal_line, sanitize_terminal_text},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
const REFRESH: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Entry {
    pub info: Option<Info>,
    pub error: Option<String>,
    pub at: Instant,
    pub stale: bool,
}

#[derive(Clone, Default)]
pub struct State {
    pub entries: BTreeMap<i64, Entry>,
    pub view: Option<i64>,
    pub scroll: usize,
    next_request: u64,
    pending: Option<(i64, u64, bool)>,
}

impl App {
    pub(super) fn begin_chat_info(&mut self, chat_id: i64) -> Vec<TelegramCommand> {
        self.chat_info.view = Some(chat_id);
        self.chat_info.scroll = 0;
        self.mode = Mode::ChatInfo;
        self.refresh_chat_info(chat_id)
    }

    fn refresh_chat_info(&mut self, chat_id: i64) -> Vec<TelegramCommand> {
        if let Some(entry) = self.chat_info.entries.get_mut(&chat_id) {
            entry.stale = true;
        }
        self.request_visible_chat_info()
    }

    fn info_target(&self) -> Option<i64> {
        if self.screen != Screen::Main
            || self.connection != ConnectionStatus::Online
            || !self.terminal_focused
        {
            return None;
        }
        if self.mode == Mode::ChatInfo {
            self.chat_info.view
        } else if self.focus == super::Focus::Conversation
            && matches!(self.mode, Mode::Navigate | Mode::Compose)
        {
            self.active_chat_id
        } else {
            None
        }
    }

    pub fn request_visible_chat_info(&mut self) -> Vec<TelegramCommand> {
        let Some(chat_id) = self.info_target() else {
            return Vec::new();
        };
        if self.chat_info.pending.is_some()
            || self
                .chat_info
                .entries
                .get(&chat_id)
                .is_some_and(|e| !e.stale && e.at.elapsed() < REFRESH)
        {
            return Vec::new();
        }
        self.chat_info.next_request += 1;
        let request_id = self.chat_info.next_request;
        self.chat_info.pending = Some((chat_id, request_id, true));
        vec![TelegramCommand::LoadChatInfo {
            chat_id,
            request_id,
        }]
    }

    #[must_use]
    pub fn next_chat_info_deadline(&self) -> Option<Instant> {
        let target = self.info_target()?;
        if self.chat_info.pending.is_some() {
            return None;
        }
        let entry = self.chat_info.entries.get(&target)?;
        let refresh = if entry.stale {
            Instant::now()
        } else {
            entry.at + REFRESH
        };
        Some(
            if entry
                .info
                .as_ref()
                .is_some_and(|i| i.next_send_at > chrono::Utc::now().timestamp())
            {
                refresh.min(Instant::now() + Duration::from_secs(1))
            } else {
                refresh
            },
        )
    }

    pub(super) fn chat_info_binding(
        &mut self,
        action: &Action,
        count: usize,
    ) -> Option<Vec<TelegramCommand>> {
        if self.mode != Mode::ChatInfo {
            return None;
        }
        match action {
            Action::Cancel => {
                self.mode = Mode::Navigate;
                self.force_redraw = true;
            }
            Action::Up => self.chat_info.scroll = self.chat_info.scroll.saturating_sub(count),
            Action::Down => self.chat_info.scroll = self.chat_info.scroll.saturating_add(count),
            Action::Refresh => {
                return Some(
                    self.chat_info
                        .view
                        .map_or_else(Vec::new, |id| self.refresh_chat_info(id)),
                );
            }
            Action::Quit | Action::NextAccount | Action::AddAccount | Action::Redraw => {
                return None;
            }
            _ => {}
        }
        Some(Vec::new())
    }

    pub(super) fn observe_chat_info(&mut self, event: &NetworkEvent) {
        match event {
            NetworkEvent::ChatInfoReady {
                chat_id,
                request_id,
                result,
            } => {
                let Some((chat, request, valid)) = self.chat_info.pending else {
                    return;
                };
                if chat != *chat_id || request != *request_id {
                    return;
                }
                self.chat_info.pending = None;
                if !valid {
                    return;
                }
                let (info, error) = match result {
                    Ok(info) => {
                        let mut info = info.clone();
                        info.title = sanitize_terminal_line(&info.title);
                        info.about = sanitize_terminal_text(&info.about);
                        info.username = info.username.map(|name| sanitize_terminal_line(&name));
                        (Some(info), None)
                    }
                    Err(error) => (None, Some(sanitize_terminal_line(error))),
                };
                if self.chat_info.entries.len() >= 64
                    && let Some(oldest) = self
                        .chat_info
                        .entries
                        .iter()
                        .min_by_key(|(_, e)| e.at)
                        .map(|(&id, _)| id)
                {
                    self.chat_info.entries.remove(&oldest);
                }
                self.chat_info.entries.insert(
                    *chat_id,
                    Entry {
                        info,
                        error,
                        at: Instant::now(),
                        stale: false,
                    },
                );
            }
            NetworkEvent::ChatInfoInvalidated { chat_id }
            | NetworkEvent::SendFailed { chat_id, .. }
            | NetworkEvent::AttachmentSendFailed { chat_id, .. } => {
                self.invalidate_chat_info(*chat_id);
            }
            NetworkEvent::Status(status) if *status != ConnectionStatus::Online => {
                for entry in self.chat_info.entries.values_mut() {
                    entry.stale = true;
                }
                if let Some(pending) = &mut self.chat_info.pending {
                    pending.2 = false;
                }
            }
            NetworkEvent::MessageSent { message, .. } | NetworkEvent::NewMessage(message)
                if message.outgoing && message.id > 0 =>
            {
                if let Some(info) = self
                    .chat_info
                    .entries
                    .get_mut(&message.chat_id)
                    .and_then(|e| e.info.as_mut())
                {
                    info.next_send_at = info
                        .next_send_at
                        .max(message.timestamp.timestamp() + i64::from(info.slow_seconds));
                }
            }
            _ => {}
        }
    }

    fn invalidate_chat_info(&mut self, chat_id: i64) {
        self.chat_info.entries.remove(&chat_id);
        if let Some(pending) = &mut self.chat_info.pending
            && pending.0 == chat_id
        {
            pending.2 = false;
        }
    }

    #[must_use]
    pub fn draft_restriction(&self, chat_id: i64) -> Option<String> {
        let info = self
            .chat_info
            .entries
            .get(&chat_id)
            .filter(|e| e.error.is_none() && !e.stale && e.at.elapsed() < REFRESH)?
            .info
            .as_ref()?;
        let now = chrono::Utc::now().timestamp();
        let attachments = self
            .draft_data(chat_id)
            .map(|d| d.attachments.as_slice())
            .unwrap_or_default();
        if info.slow_seconds > 0 {
            if attachments.len() > 1 {
                return Some("Slow mode: send one attachment at a time".to_owned());
            }
            if self.messages.get(&chat_id).is_some_and(|messages| {
                messages
                    .iter()
                    .any(|m| m.outgoing && m.delivery == crate::model::Delivery::Pending)
            }) {
                return Some("Slow mode: wait for the pending message to finish".to_owned());
            }
        }
        if attachments.is_empty() {
            return info.restriction(Content::Text, now);
        }
        attachments.iter().find_map(|file| {
            info.restriction(
                if file.as_photo {
                    Content::Photo
                } else {
                    Content::File
                },
                now,
            )
        })
    }
}
