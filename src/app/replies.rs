//! Bounded, lazy reply excerpts. Targets never become new timeline arrivals.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    App, ChatId, Message, Mode, NetworkEvent, ReplyInfo, TelegramCommand, sanitize_message,
};

const BATCH_SIZE: usize = 32;
const CACHE_LIMIT: usize = 512;

#[derive(Clone, Debug)]
enum Preview {
    Loading,
    Ready(Box<Message>),
    Unavailable,
    Failed,
}

#[derive(Clone, Debug)]
struct Entry {
    request_id: u64,
    preview: Preview,
}

#[derive(Clone, Default)]
pub(super) struct State {
    entries: BTreeMap<(ChatId, i32), Entry>,
    pending: Option<(ChatId, u64)>,
}

impl App {
    #[must_use]
    pub fn reply_message(&self, reply: &ReplyInfo) -> Option<&Message> {
        if self.reply_is_unavailable(reply) {
            return None;
        }
        self.messages
            .get(&reply.chat_id)
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == reply.message_id)
            })
            .or_else(|| {
                match &self
                    .replies
                    .entries
                    .get(&(reply.chat_id, reply.message_id))?
                    .preview
                {
                    Preview::Ready(message) => Some(message.as_ref()),
                    _ => None,
                }
            })
    }

    pub(super) fn reply_is_unavailable(&self, reply: &ReplyInfo) -> bool {
        self.replies
            .entries
            .get(&(reply.chat_id, reply.message_id))
            .is_some_and(|entry| matches!(entry.preview, Preview::Unavailable))
    }

    #[must_use]
    pub fn reply_preview_status(&self, reply: &ReplyInfo) -> &'static str {
        if self.reply_message(reply).is_some() {
            return "";
        }
        match self
            .replies
            .entries
            .get(&(reply.chat_id, reply.message_id))
            .map(|entry| &entry.preview)
        {
            Some(Preview::Unavailable) => "Original message unavailable",
            Some(Preview::Failed) => "Original not loaded · open to retry",
            _ => "Loading original message…",
        }
    }

    /// Called after drawing. One batch is in flight; repeated frames deduplicate
    /// by stable target identity, independently of selection and list order.
    pub fn request_visible_replies(&mut self) -> Vec<TelegramCommand> {
        if self.replies.pending.is_some() || !matches!(self.mode, Mode::Navigate | Mode::Compose) {
            return Vec::new();
        }
        let visible: BTreeSet<_> = self
            .message_hit_regions
            .iter()
            .map(|region| region.3.0)
            .collect();
        let targets: BTreeSet<_> = self
            .active_messages()
            .iter()
            .filter(|message| visible.contains(&message.id))
            .filter_map(|message| message.reply_to.as_ref())
            .filter(|reply| reply.message_id > 0)
            .map(|reply| (reply.chat_id, reply.message_id))
            .collect();
        while self.replies.entries.len() > CACHE_LIMIT.saturating_sub(BATCH_SIZE) {
            let Some(key) = self
                .replies
                .entries
                .keys()
                .find(|key| !targets.contains(key))
                .copied()
            else {
                break;
            };
            self.replies.entries.remove(&key);
        }
        let missing = |target: &&(ChatId, i32)| {
            !self.replies.entries.contains_key(*target)
                && !self
                    .messages
                    .get(&target.0)
                    .is_some_and(|messages| messages.iter().any(|message| message.id == target.1))
        };
        let Some(&(chat_id, _)) = targets.iter().find(missing) else {
            return Vec::new();
        };
        let message_ids: Vec<_> = targets
            .iter()
            .filter(missing)
            .filter(|target| target.0 == chat_id)
            .map(|target| target.1)
            .take(BATCH_SIZE.min(CACHE_LIMIT.saturating_sub(self.replies.entries.len())))
            .collect();
        if message_ids.is_empty() {
            return Vec::new();
        }
        let request_id = self.next_history_request_id;
        self.next_history_request_id = request_id.wrapping_add(1).max(1);
        for id in &message_ids {
            self.replies.entries.insert(
                (chat_id, *id),
                Entry {
                    request_id,
                    preview: Preview::Loading,
                },
            );
        }
        self.replies.pending = Some((chat_id, request_id));
        vec![TelegramCommand::LoadReplyPreviews {
            chat_id,
            message_ids,
            request_id,
        }]
    }

    pub(super) fn update_reply_previews(&mut self, event: &NetworkEvent) {
        match event {
            NetworkEvent::ReplyPreviews {
                chat_id,
                request_id,
                messages,
                unavailable,
                complete,
            } => {
                if self.replies.pending != Some((*chat_id, *request_id)) {
                    return;
                }
                self.finish_reply_previews(*chat_id, *request_id, messages, unavailable, *complete);
            }
            NetworkEvent::ReplyPreviewsFailed {
                chat_id,
                request_id,
                ..
            } => {
                if self.replies.pending != Some((*chat_id, *request_id)) {
                    return;
                }
                self.replies.pending = None;
                for entry in self
                    .replies
                    .entries
                    .values_mut()
                    .filter(|entry| entry.request_id == *request_id)
                {
                    if matches!(entry.preview, Preview::Loading) {
                        entry.preview = Preview::Failed;
                    }
                }
            }
            NetworkEvent::NewMessage(message)
            | NetworkEvent::MessageUpdated(message)
            | NetworkEvent::MessageLoaded { message, .. } => {
                if let Some(entry) = self.replies.entries.get_mut(&(message.chat_id, message.id))
                    && !matches!(entry.preview, Preview::Unavailable)
                {
                    let mut message = message.clone();
                    sanitize_message(&mut message);
                    // A live edit wins over a batch begun before that edit.
                    *entry = Entry {
                        request_id: 0,
                        preview: Preview::Ready(Box::new(message)),
                    };
                }
            }
            NetworkEvent::MessageContentsRead {
                channel_id,
                message_ids,
            } => {
                for entry in self.replies.entries.values_mut() {
                    if let Preview::Ready(message) = &mut entry.preview {
                        message.acknowledge_contents(*channel_id, message_ids);
                    }
                }
            }
            NetworkEvent::MessagesDeleted {
                channel_id,
                message_ids,
            } => {
                self.delete_reply_previews(*channel_id, message_ids);
            }
            NetworkEvent::CacheInvalidated { chat_id } => {
                self.replies
                    .entries
                    .retain(|(id, _), _| chat_id.is_some_and(|chat| chat != *id));
                if self
                    .replies
                    .pending
                    .is_some_and(|(id, _)| chat_id.is_none_or(|chat| chat == id))
                {
                    self.replies.pending = None;
                }
            }
            NetworkEvent::Status(crate::event::ConnectionStatus::Online) => {
                self.replies
                    .entries
                    .retain(|_, entry| !matches!(entry.preview, Preview::Failed));
            }
            _ => {}
        }
    }

    fn finish_reply_previews(
        &mut self,
        chat_id: ChatId,
        request_id: u64,
        messages: &[Message],
        unavailable: &[i32],
        complete: bool,
    ) {
        for message in messages.iter().filter(|message| message.chat_id == chat_id) {
            if let Some(entry) = self.replies.entries.get_mut(&(chat_id, message.id))
                && entry.request_id == request_id
            {
                let mut message = message.clone();
                sanitize_message(&mut message);
                entry.preview = Preview::Ready(Box::new(message));
            }
        }
        for id in unavailable {
            if let Some(entry) = self.replies.entries.get_mut(&(chat_id, *id))
                && entry.request_id == request_id
            {
                entry.preview = Preview::Unavailable;
            }
        }
        if complete {
            self.replies.pending = None;
        }
    }

    fn delete_reply_previews(&mut self, channel_id: Option<ChatId>, message_ids: &[i32]) {
        // Record referenced targets even if they were never loaded, so
        // an excerpt or a later history page cannot resurrect them.
        let targets: BTreeSet<_> = self
            .messages
            .values()
            .flatten()
            .filter_map(|message| message.reply_to.as_ref())
            .map(|reply| (reply.chat_id, reply.message_id))
            .chain(self.replies.entries.keys().copied())
            .collect();
        for key in targets {
            if channel_id.map_or(key.0 > -1_000_000_000_000, |id| id == key.0)
                && message_ids.contains(&key.1)
            {
                self.replies.entries.insert(
                    key,
                    Entry {
                        request_id: 0,
                        preview: Preview::Unavailable,
                    },
                );
            }
        }
    }
}
