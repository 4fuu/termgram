//! Receipts protect RPC snapshots begun before a content-read update. The
//! durable message flag and PTS commit together; only in-flight RPCs need IDs
//! for messages that were not cached when the receipt arrived.
use super::{ChatId, Message, NetworkEvent, Store};
use std::{borrow::Cow, collections::BTreeMap};

#[derive(Default)]
pub(super) struct Receipts(BTreeMap<(ChatId, i32), i64>);

fn scope(chat: ChatId) -> ChatId {
    if chat > -1_000_000_000_000 { 0 } else { chat }
}

impl Receipts {
    pub fn record(&mut self, channel: Option<ChatId>, ids: &[i32], revision: i64) {
        self.0.extend(
            ids.iter()
                .filter(|id| **id > 0)
                .map(|id| ((channel.unwrap_or(0), *id), revision)),
        );
    }

    pub fn supersede(&mut self, message: &Message) {
        self.0.remove(&(scope(message.chat_id), message.id));
    }

    pub fn prune(&mut self, oldest: i64) {
        self.0.retain(|_, revision| *revision > oldest);
    }

    pub fn reconcile<'a>(&self, message: &'a Message, started: i64) -> Cow<'a, Message> {
        let mut message = Cow::Borrowed(message);
        if message
            .mention
            .as_ref()
            .is_some_and(|mention| mention.unread)
            && self
                .0
                .get(&(scope(message.chat_id), message.id))
                .is_some_and(|revision| *revision > started)
            && let Some(mention) = &mut message.to_mut().mention
        {
            mention.unread = false;
        }
        message
    }
}

impl Store {
    pub(crate) fn has_message_snapshot(event: &NetworkEvent) -> bool {
        matches!(
            event,
            NetworkEvent::History { .. }
                | NetworkEvent::OlderHistory { .. }
                | NetworkEvent::ReplyPreviews { complete: true, .. }
                | NetworkEvent::PinnedMessages { .. }
                | NetworkEvent::PinnedContext { .. }
                | NetworkEvent::MessageLoaded { .. }
                | NetworkEvent::CloudSearchResults { .. }
                | NetworkEvent::CloudSearchContext { .. }
        )
    }

    /// Called before the snapshot is applied and its generation is retired.
    pub(crate) fn reconcile_contents(&self, event: &mut NetworkEvent) {
        let (messages, started): (&mut [Message], Option<i64>) = match event {
            NetworkEvent::History {
                chat_id,
                request_id,
                messages,
            }
            | NetworkEvent::OlderHistory {
                chat_id,
                request_id,
                messages,
                ..
            }
            | NetworkEvent::ReplyPreviews {
                chat_id,
                request_id,
                messages,
                complete: true,
                ..
            } => (
                messages,
                self.history_revisions
                    .get(&(*chat_id, *request_id))
                    .copied(),
            ),
            NetworkEvent::MessageLoaded {
                chat_id,
                request_id,
                message,
                ..
            } => (
                std::slice::from_mut(message),
                self.history_revisions
                    .get(&(*chat_id, *request_id))
                    .copied(),
            ),
            NetworkEvent::PinnedMessages {
                chat_id,
                request_id,
                page,
            } => (
                &mut page.messages,
                self.pin_revisions.get(&(*chat_id, *request_id)).copied(),
            ),
            NetworkEvent::PinnedContext {
                chat_id,
                request_id,
                messages,
                ..
            } => (
                messages,
                self.pin_revisions.get(&(*chat_id, *request_id)).copied(),
            ),
            NetworkEvent::CloudSearchResults {
                chat_id,
                request_id,
                page,
            } => (
                &mut page.messages,
                self.search_revision
                    .filter(|(id, chat, _)| *id == *request_id && *chat == *chat_id)
                    .map(|(_, _, revision)| revision),
            ),
            NetworkEvent::CloudSearchContext {
                chat_id,
                request_id,
                messages,
                ..
            } => (
                messages,
                self.search_revision
                    .filter(|(id, chat, _)| *id == *request_id && *chat == *chat_id)
                    .map(|(_, _, revision)| revision),
            ),
            _ => return,
        };
        if let Some(started) = started {
            for message in messages {
                if let Cow::Owned(merged) = self.contents_read.reconcile(message, started) {
                    *message = merged;
                }
            }
        }
    }
}
