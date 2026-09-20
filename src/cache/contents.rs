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
        matches!(event, NetworkEvent::NewMessage(message) | NetworkEvent::MessageUpdated(message) | NetworkEvent::CacheMessage(message) | NetworkEvent::MessageSent { message, .. }
            if message.poll.is_some() || message.reactions.is_some())
            || matches!(
                event,
                NetworkEvent::ReactionsLoaded { .. }
                    | NetworkEvent::PollLoaded { .. }
                    | NetworkEvent::History { .. }
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
    #[allow(clippy::too_many_lines)]
    pub(crate) async fn reconcile_snapshots(&self, event: &mut NetworkEvent) -> anyhow::Result<()> {
        if let NetworkEvent::NewMessage(message)
        | NetworkEvent::MessageUpdated(message)
        | NetworkEvent::CacheMessage(message)
        | NetworkEvent::MessageSent { message, .. } = event
        {
            // Live message edits can also contain minimal media/reaction data.
            // They supersede old snapshots but preserve omitted private choices.
            super::reactions::merge_cached(&self.connection, message).await?;
            if let Some(poll) = &mut message.poll {
                super::polls::merge_cached(&self.connection, poll).await?;
            }
            return Ok(());
        }
        if let NetworkEvent::ReactionsLoaded {
            chat_id,
            message_id,
            request_id,
            result,
        } = event
        {
            if let Some(started) = self.reaction_revisions.get(request_id) {
                if let Ok(review) = result {
                    super::reactions::merge_summary(
                        &self.connection,
                        *chat_id,
                        *message_id,
                        &mut review.summary,
                    )
                    .await?;
                    super::reactions::reconcile_summary(
                        &self.reaction_updates,
                        *chat_id,
                        *message_id,
                        &mut review.summary,
                        *started,
                    );
                }
            } else if result.is_ok() {
                *result = Err("History changed during reaction lookup; reopen it".to_owned());
            }
            return Ok(());
        }
        if let NetworkEvent::PollLoaded {
            request_id, result, ..
        } = event
        {
            if let Some(started) = self.poll_revisions.get(request_id) {
                if let Ok(poll) = result {
                    super::polls::merge_cached(&self.connection, poll).await?;
                    self.poll_updates.reconcile(poll, *started);
                }
            } else if result.is_ok() {
                *result = Err("History changed during the poll lookup; reopen it".to_owned());
            }
            return Ok(());
        }
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
            _ => return Ok(()),
        };
        if let Some(started) = started {
            for message in messages {
                super::reactions::merge_cached(&self.connection, message).await?;
                super::reactions::reconcile(&self.reaction_updates, message, started);
                if let Some(poll) = &mut message.poll {
                    super::polls::merge_cached(&self.connection, poll).await?;
                    self.poll_updates.reconcile(poll, started);
                }
                if let Cow::Owned(merged) = self.contents_read.reconcile(message, started) {
                    *message = merged;
                }
            }
        }
        Ok(())
    }
}
