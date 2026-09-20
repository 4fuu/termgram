//! RPC tasks own their arguments, never the worker's mutable state.
//! Results are merged by the same coordinator that consumes live updates.

use super::{
    HISTORY_LIMIT, TelegramLink, WorkerCache, activate_inline_button, apply_dialogs,
    cache_sender_name, hydrate_reply_sender, hydrate_reply_senders, map_message,
    parse_telegram_link, resolve_telegram_link,
};
use crate::{
    event::{NetworkEvent, TelegramCommand},
    model::{Chat, ChatKind},
};
use anyhow::{Context, Result, bail};
use grammers_client::peer::Dialog;
use grammers_client::{
    Client,
    message::{InputMessage, Message as TelegramMessage},
};
use grammers_session::types::{PeerKind, PeerRef};
use tokio::{sync::mpsc, task::JoinSet};

pub(super) const MAX_REQUESTS: usize = 8;

pub(super) struct Completion {
    command: TelegramCommand,
    result: Result<Response>,
}

pub(super) async fn refresh_dialogs(
    client: &Client,
    cache: &mut WorkerCache,
    events: &mpsc::Sender<NetworkEvent>,
    requests: &mut JoinSet<Completion>,
) -> Result<()> {
    if cache.dialogs.in_flight {
        cache.dialogs.dirty = true;
    } else {
        cache.dialogs.in_flight = true;
        cache.dialogs.dirty = false;
        events.send(NetworkEvent::DialogsLoading).await?;
        spawn(TelegramCommand::RefreshDialogs, client, cache, requests);
    }
    Ok(())
}

pub(super) async fn refresh_pending(
    client: &Client,
    cache: &mut WorkerCache,
    events: &mpsc::Sender<NetworkEvent>,
    requests: &mut JoinSet<Completion>,
) -> Result<()> {
    if cache.dialogs.dirty && !cache.dialogs.in_flight && requests.len() < MAX_REQUESTS {
        refresh_dialogs(client, cache, events, requests).await?;
    }
    if cache.folders.dirty && !cache.folders.in_flight && requests.len() < MAX_REQUESTS {
        cache.folders.dirty = false;
        cache.folders.in_flight = true;
        spawn(TelegramCommand::RefreshFolders, client, cache, requests);
    }
    if cache.dialog_pins.dirty && !cache.dialog_pins.in_flight && requests.len() < MAX_REQUESTS {
        cache.dialog_pins.dirty = false;
        cache.dialog_pins.in_flight = true;
        spawn(TelegramCommand::RefreshDialogPins, client, cache, requests);
    }
    if cache.message_pins.dirty
        && !cache.message_pins.in_flight
        && requests.len() < MAX_REQUESTS
        && let Some(chat_id) = cache.active_chat.filter(|id| cache.peers.contains_key(id))
    {
        cache.message_pins.dirty = false;
        cache.message_pins.in_flight = true;
        events
            .send(NetworkEvent::PinnedMessagesLoading {
                chat_id,
                request_id: 0,
            })
            .await?;
        spawn(
            TelegramCommand::LoadPinnedMessages {
                chat_id,
                before: 0,
                request_id: 0,
            },
            client,
            cache,
            requests,
        );
    }
    Ok(())
}

enum Response {
    Saved(super::forwarding::Destination),
    ForwardReview(Box<super::forwarding::Review>),
    Copied(String),
    Deletion(Box<super::deletion::Review>),
    EditSource(crate::editing::Source),
    Edited(Option<Box<TelegramMessage>>),
    PinnedMessages(Vec<TelegramMessage>, usize),
    DialogPins(crate::pins::DialogPins),
    Dialogs(Vec<Dialog>, (i64, i64)),
    Folders(Vec<crate::folders::Folder>),
    History(Vec<TelegramMessage>),
    ReplyPreviews(Vec<TelegramMessage>, Vec<i32>),
    Message(Box<TelegramMessage>),
    Link(Chat, PeerRef, Option<Box<TelegramMessage>>),
    Button(Option<String>, Option<String>),
    Applied,
    Read(Option<crate::read_state::Snapshot>),
    ChatUnread(Option<crate::read_state::Snapshot>),
}

/// Capture only the peer needed by this request. The complete cache stays with
/// the coordinator, including updates received while the RPC is running.
pub(super) fn spawn(
    command: TelegramCommand,
    client: &Client,
    cache: &WorkerCache,
    requests: &mut JoinSet<Completion>,
) {
    let chat_id = match &command {
        TelegramCommand::ReviewForward { chat_id, .. }
        | TelegramCommand::ForwardMessage { chat_id, .. }
        | TelegramCommand::CopyMessage { chat_id, .. }
        | TelegramCommand::ReviewDeletion { chat_id, .. }
        | TelegramCommand::DeleteMessage { chat_id, .. }
        | TelegramCommand::LoadEdit { chat_id, .. }
        | TelegramCommand::EditMessage { chat_id, .. }
        | TelegramCommand::LoadHistory { chat_id, .. }
        | TelegramCommand::ChangeDialogPin { chat_id, .. }
        | TelegramCommand::SetArchived { chat_id, .. }
        | TelegramCommand::LoadPinnedMessages { chat_id, .. }
        | TelegramCommand::LoadPinnedContext { chat_id, .. }
        | TelegramCommand::ChangeMessagePin { chat_id, .. }
        | TelegramCommand::LoadOlder { chat_id, .. }
        | TelegramCommand::LoadMessage { chat_id, .. }
        | TelegramCommand::LoadReplyPreviews { chat_id, .. }
        | TelegramCommand::SendMessage { chat_id, .. }
        | TelegramCommand::ActivateButton { chat_id, .. }
        | TelegramCommand::MarkRead { chat_id, .. }
        | TelegramCommand::SetChatUnread { chat_id, .. } => Some(*chat_id),
        _ => None,
    };
    let peer = chat_id.and_then(|id| cache.peers.get(&id).copied());
    let private_link = match &command {
        TelegramCommand::ResolveTelegramLink { url } => match parse_telegram_link(url) {
            Ok(TelegramLink::Private { chat_id, .. })
                if cache.visible_channel_groups.contains(&chat_id) =>
            {
                cache.peers.get(&chat_id).copied().map(|peer| {
                    let title = cache.names.get(&peer.id).cloned().unwrap_or_default();
                    (peer, title)
                })
            }
            _ => None,
        },
        _ => None,
    };
    let other_peer = match &command {
        TelegramCommand::ReviewForward { destination, .. }
        | TelegramCommand::ForwardMessage { destination, .. } => {
            cache.peers.get(destination).copied()
        }
        _ => None,
    };
    let self_id = cache.self_id;
    let client = client.clone();
    requests.spawn(async move {
        let result = Box::pin(execute(
            &command,
            &client,
            peer,
            private_link,
            self_id,
            other_peer,
        ))
        .await;
        Completion { command, result }
    });
}

#[allow(clippy::too_many_lines)]
async fn execute(
    command: &TelegramCommand,
    client: &Client,
    peer: Option<PeerRef>,
    private_link: Option<(PeerRef, String)>,
    self_id: i64,
    other_peer: Option<PeerRef>,
) -> Result<Response> {
    let peer = || peer.context("conversation is missing its Telegram peer reference");
    match command {
        TelegramCommand::OpenSaved { .. } => Ok(Response::Saved(
            super::forwarding::destination(client, self_id, None, self_id).await?,
        )),
        TelegramCommand::ReviewForward {
            message_id,
            destination,
            ..
        } => {
            let destination =
                super::forwarding::destination(client, *destination, other_peer, self_id).await?;
            Ok(Response::ForwardReview(Box::new(
                super::forwarding::review(client, peer()?, *message_id, destination).await?,
            )))
        }
        TelegramCommand::ForwardMessage {
            message_id,
            revision,
            random_id,
            ..
        } => {
            super::forwarding::send(
                client,
                peer()?,
                *message_id,
                other_peer.context("Forward target is no longer available")?,
                *revision,
                *random_id,
            )
            .await?;
            Ok(Response::Applied)
        }
        TelegramCommand::CopyMessage {
            message_id, link, ..
        } => Ok(Response::Copied(
            super::sharing::copy(client, peer()?, *message_id, *link).await?,
        )),
        TelegramCommand::ReviewDeletion { message_id, .. } => Ok(Response::Deletion(Box::new(
            super::deletion::review(client, peer()?, *message_id, self_id).await?,
        ))),
        TelegramCommand::DeleteMessage {
            message_id,
            revision,
            scope,
            ..
        } => {
            super::deletion::delete(client, peer()?, *message_id, self_id, *revision, *scope)
                .await?;
            Ok(Response::Applied)
        }
        TelegramCommand::LoadEdit { message_id, .. } => Ok(Response::EditSource(
            super::editing::load(client, peer()?, *message_id, self_id).await?,
        )),
        TelegramCommand::EditMessage {
            message_id,
            revision,
            text,
            ..
        } => Ok(Response::Edited(
            super::editing::save(client, peer()?, *message_id, self_id, *revision, text).await?,
        )),
        TelegramCommand::LoadPinnedMessages { before, .. } => {
            let mut iter = client
                .search_messages(peer()?)
                .filter(grammers_client::tl::enums::MessagesFilter::InputMessagesFilterPinned)
                .offset_id(*before)
                .limit(crate::pins::PAGE_SIZE);
            let mut messages = Vec::new();
            while let Some(message) = iter.next().await? {
                messages.push(message);
            }
            Ok(Response::PinnedMessages(messages, iter.total().await?))
        }
        TelegramCommand::LoadPinnedContext { message_id, .. } => {
            let target = client
                .get_messages_by_id(peer()?, &[*message_id])
                .await?
                .pop()
                .flatten()
                .context("Pinned message is no longer available")?;
            let mut iter = client
                .iter_messages(peer()?)
                .offset_id(*message_id)
                .limit(HISTORY_LIMIT - 1);
            let mut messages = vec![target];
            while let Some(message) = iter.next().await? {
                messages.push(message);
            }
            messages.reverse();
            Ok(Response::History(messages))
        }
        TelegramCommand::ChangeMessagePin {
            message_id, action, ..
        } => {
            super::pins::change_message(client, peer()?, *message_id, *action).await?;
            Ok(Response::Applied)
        }
        TelegramCommand::SetArchived { archived, .. } => {
            client
                .invoke(&grammers_client::tl::functions::folders::EditPeerFolders {
                    folder_peers: vec![
                        grammers_client::tl::types::InputFolderPeer {
                            peer: peer()?.into(),
                            folder_id: i32::from(*archived),
                        }
                        .into(),
                    ],
                })
                .await?;
            Ok(Response::DialogPins(
                super::pins::load_dialogs(client).await?,
            ))
        }
        TelegramCommand::RefreshDialogPins => Ok(Response::DialogPins(
            super::pins::load_dialogs(client).await?,
        )),
        TelegramCommand::ChangeDialogPin { scope, action, .. } => {
            super::pins::change_dialog(client, peer()?, *scope, *action, self_id).await?;
            if matches!(scope, crate::pins::DialogScope::Filter(_)) {
                Ok(Response::Folders(
                    super::folders::load(client, self_id).await?,
                ))
            } else {
                Ok(Response::DialogPins(
                    super::pins::load_dialogs(client).await?,
                ))
            }
        }
        TelegramCommand::RefreshFolders => Ok(Response::Folders(
            super::folders::load(client, self_id).await?,
        )),
        TelegramCommand::RefreshDialogs => {
            let mut iter = client.iter_dialogs();
            let mut dialogs = Vec::new();
            while let Some(dialog) = iter.next().await? {
                dialogs.push(dialog);
            }
            Ok(Response::Dialogs(
                dialogs,
                super::folders::default_mutes(client).await?,
            ))
        }
        TelegramCommand::LoadHistory { .. } | TelegramCommand::LoadOlder { .. } => {
            let after = match command {
                TelegramCommand::LoadHistory { after_id, .. } => *after_id,
                _ => None,
            };
            let before = match command {
                TelegramCommand::LoadOlder { before_id, .. } => *before_id,
                _ => 0,
            };
            let mut iter = client
                .iter_messages(peer()?)
                .offset_id(after.unwrap_or(before))
                .reverse(after.is_some())
                .limit(HISTORY_LIMIT);
            let mut messages = Vec::new();
            while let Some(message) = iter.next().await? {
                messages.push(message);
            }
            if after.is_none() {
                messages.reverse();
            }
            Ok(Response::History(messages))
        }
        TelegramCommand::LoadReplyPreviews {
            chat_id,
            message_ids,
            ..
        } => {
            anyhow::ensure!(
                message_ids.len() <= 32 && message_ids.iter().all(|id| *id > 0),
                "invalid reply preview batch"
            );
            let messages: Vec<_> = client
                .get_messages_by_id(peer()?, message_ids)
                .await?
                .into_iter()
                .flatten()
                .filter(|message| super::peer_id(message).ok() == Some(*chat_id))
                .collect();
            let unavailable = message_ids
                .iter()
                .copied()
                .filter(|id| !messages.iter().any(|message| message.id() == *id))
                .collect();
            Ok(Response::ReplyPreviews(messages, unavailable))
        }
        TelegramCommand::LoadMessage {
            source_message_id,
            message_id,
            ..
        } => {
            if *source_message_id <= 0 || *message_id <= 0 {
                bail!("invalid Telegram message identifier");
            }
            let source = client
                .get_messages_by_id(peer()?, &[*source_message_id])
                .await?
                .pop()
                .flatten()
                .context("replying message is unavailable")?;
            if source.reply_to_message_id() != Some(*message_id) {
                bail!("reply relation changed before navigation");
            }
            let message = client
                .get_reply_to_message(&source)
                .await?
                .context("reply target is unavailable")?;
            Ok(Response::Message(Box::new(message)))
        }
        TelegramCommand::SendMessage { text, reply_to, .. } => {
            let input = InputMessage::new().text(text.clone()).reply_to(*reply_to);
            let message = Box::pin(client.send_message(peer()?, input)).await?;
            Ok(Response::Message(Box::new(message)))
        }
        TelegramCommand::ResolveTelegramLink { url } => {
            let (chat, peer, message) = resolve_telegram_link(client, private_link, url).await?;
            Ok(Response::Link(chat, peer, message.map(Box::new)))
        }
        TelegramCommand::ActivateButton {
            message_id,
            button_index,
            ..
        } => {
            let (message, url) =
                activate_inline_button(client, peer()?, *message_id, *button_index).await?;
            Ok(Response::Button(message, url))
        }
        TelegramCommand::MarkRead { max_id, .. } => Ok(Response::Read(
            super::reads::mark(client, peer()?, *max_id).await?,
        )),
        TelegramCommand::SetChatUnread {
            unread,
            read_history,
            ..
        } => Ok(Response::ChatUnread(
            super::reads::set_unread(client, peer()?, *unread, *read_history).await?,
        )),
        _ => unreachable!("only RPC commands are submitted to request tasks"),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) async fn complete(
    completion: Completion,
    cache: &mut WorkerCache,
    events: &mpsc::Sender<NetworkEvent>,
) -> Result<()> {
    let Completion { command, result } = completion;
    if matches!(command, TelegramCommand::SetChatUnread { .. }) {
        // A failed second RPC may follow a successful first RPC. Refresh even
        // on partial failure so the authoritative mark/count can settle.
        cache.dialogs.dirty = true;
    }
    if matches!(
        command,
        TelegramCommand::LoadPinnedMessages { request_id: 0, .. }
    ) {
        cache.message_pins.in_flight = false;
        if cache.message_pins.dirty {
            if let Some(event) = command.failure("Pins changed during refresh".to_owned()) {
                events.send(event).await?;
            }
            return Ok(());
        }
    }
    if matches!(command, TelegramCommand::RefreshDialogs) {
        cache.dialogs.in_flight = false;
        if cache.dialogs.dirty {
            return Ok(());
        }
    }
    if matches!(command, TelegramCommand::RefreshFolders) {
        cache.folders.in_flight = false;
        if cache.folders.dirty {
            return Ok(());
        }
    }
    if matches!(command, TelegramCommand::RefreshDialogPins) {
        cache.dialog_pins.in_flight = false;
        if cache.dialog_pins.dirty {
            return Ok(());
        }
    }
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            if let Some(event) = command.failure(format!("Telegram request failed: {error:#}")) {
                events.send(event).await?;
            }
            return Ok(());
        }
    };
    let event = match (command, response) {
        (TelegramCommand::OpenSaved { request_id }, Response::Saved(destination)) => {
            cache.peers.insert(destination.chat.id, destination.peer);
            cache.linked_peers.insert(destination.chat.id);
            cache_sender_name(cache, destination.peer.id, destination.name);
            NetworkEvent::SavedReady {
                request_id,
                result: Ok(destination.chat),
            }
        }
        (TelegramCommand::ReviewForward { request_id, .. }, Response::ForwardReview(review)) => {
            cache
                .peers
                .insert(review.destination.chat.id, review.destination.peer);
            cache.linked_peers.insert(review.destination.chat.id);
            cache_sender_name(cache, review.destination.peer.id, review.destination.name);
            NetworkEvent::ForwardReady {
                request_id,
                result: Ok(crate::forwarding::Plan {
                    message: map_message(&review.original, cache)?,
                    destination: review.destination.chat,
                    revision: review.revision,
                    random_id: review.random_id,
                }),
            }
        }
        (TelegramCommand::ForwardMessage { request_id, .. }, Response::Applied) => {
            cache.dialogs.dirty = true;
            NetworkEvent::ForwardFinished {
                request_id,
                error: None,
            }
        }

        (TelegramCommand::CopyMessage { request_id, .. }, Response::Copied(text)) => {
            NetworkEvent::MessageCopyReady {
                request_id,
                result: Ok(text),
            }
        }
        (
            TelegramCommand::ReviewDeletion {
                chat_id,
                message_id,
                request_id,
            },
            Response::Deletion(review),
        ) => NetworkEvent::DeletionReady {
            chat_id,
            message_id,
            request_id,
            result: Ok(crate::deletion::Plan {
                message: map_message(&review.message, cache)?,
                revision: review.revision,
                scopes: review.scopes,
            }),
        },
        (
            TelegramCommand::DeleteMessage {
                chat_id,
                message_id,
                request_id,
                ..
            },
            Response::Applied,
        ) => {
            cache.dialogs.dirty = true;
            cache.message_pins.dirty = true;
            // The SDK delivers deletion IDs through the ordered update stream,
            // which persists the tombstone before the covered checkpoint.
            NetworkEvent::DeleteFinished {
                chat_id,
                message_id,
                request_id,
                error: None,
            }
        }
        (
            TelegramCommand::LoadEdit {
                chat_id,
                request_id,
                ..
            },
            Response::EditSource(source),
        ) => NetworkEvent::EditLoaded {
            chat_id,
            request_id,
            result: Ok(source),
        },
        (
            TelegramCommand::EditMessage {
                chat_id,
                request_id,
                ..
            },
            Response::Edited(message),
        ) => {
            if let Some(message) = message {
                events
                    .send(NetworkEvent::MessageUpdated(map_message(&message, cache)?))
                    .await?;
            }
            NetworkEvent::EditFinished {
                chat_id,
                request_id,
                error: None,
            }
        }
        (
            TelegramCommand::LoadReplyPreviews {
                chat_id,
                request_id,
                ..
            },
            Response::ReplyPreviews(raw, unavailable),
        ) => {
            let mut messages = raw
                .iter()
                .map(|message| map_message(message, cache))
                .collect::<Result<Vec<_>>>()?;
            hydrate_reply_senders(&mut messages, cache);
            NetworkEvent::ReplyPreviews {
                chat_id,
                request_id,
                messages,
                unavailable,
                complete: true,
            }
        }
        (
            TelegramCommand::ChangeMessagePin {
                chat_id,
                message_id,
                action,
                request_id,
            },
            Response::Applied,
        ) => {
            cache.message_pins.dirty |= cache.active_chat == Some(chat_id);
            let event = match action {
                crate::pins::MessageAction::UnpinAll => {
                    NetworkEvent::MessagePinsCleared { chat_id }
                }
                _ => NetworkEvent::MessagePinsChanged {
                    chat_id,
                    message_ids: vec![message_id],
                    pinned: matches!(action, crate::pins::MessageAction::Pin { .. }),
                },
            };
            events.send(event).await?;
            NetworkEvent::MessagePinFinished {
                chat_id,
                request_id,
                error: None,
            }
        }
        (
            TelegramCommand::LoadPinnedMessages {
                chat_id,
                before,
                request_id,
            },
            Response::PinnedMessages(raw, total),
        ) => {
            let mut messages = raw
                .iter()
                .map(|message| map_message(message, cache))
                .collect::<Result<Vec<_>>>()?;
            hydrate_reply_senders(&mut messages, cache);
            let next = (messages.len() == crate::pins::PAGE_SIZE)
                .then(|| messages.last().expect("full page").id);
            NetworkEvent::PinnedMessages {
                chat_id,
                request_id,
                page: crate::pins::MessagePage {
                    messages,
                    total: Some(total),
                    before,
                    next,
                },
            }
        }
        (
            TelegramCommand::LoadPinnedContext {
                chat_id,
                message_id,
                request_id,
            },
            Response::History(raw),
        ) => {
            let mut messages = raw
                .iter()
                .map(|message| map_message(message, cache))
                .collect::<Result<Vec<_>>>()?;
            hydrate_reply_senders(&mut messages, cache);
            NetworkEvent::PinnedContext {
                chat_id,
                message_id,
                request_id,
                messages,
            }
        }
        (
            TelegramCommand::SetArchived {
                chat_id,
                archived,
                request_id,
            },
            Response::DialogPins(pins),
        ) => {
            cache.dialogs.dirty = true;
            cache.dialog_pins.dirty = true;
            events
                .send(NetworkEvent::ArchiveChanged { chat_id, archived })
                .await?;
            events.send(NetworkEvent::DialogPins(pins)).await?;
            NetworkEvent::DialogPinFinished {
                request_id,
                error: None,
            }
        }
        (TelegramCommand::RefreshDialogPins, Response::DialogPins(pins)) => {
            NetworkEvent::DialogPins(pins)
        }
        (TelegramCommand::ChangeDialogPin { request_id, .. }, response) => {
            cache.dialog_pins.dirty = true;
            cache.folders.dirty = true;
            match response {
                Response::DialogPins(pins) => events.send(NetworkEvent::DialogPins(pins)).await?,
                Response::Folders(folders) => events.send(NetworkEvent::Folders(folders)).await?,
                _ => unreachable!("pin mutations return their authoritative server list"),
            }
            NetworkEvent::DialogPinFinished {
                request_id,
                error: None,
            }
        }
        (TelegramCommand::RefreshFolders, Response::Folders(folders)) => {
            NetworkEvent::Folders(folders)
        }
        (TelegramCommand::RefreshDialogs, Response::Dialogs(dialogs, defaults)) => {
            NetworkEvent::Dialogs(apply_dialogs(dialogs, defaults, cache)?)
        }
        (
            TelegramCommand::LoadHistory {
                chat_id,
                request_id,
                ..
            },
            Response::History(raw),
        ) => {
            let mut messages = raw
                .iter()
                .map(|message| map_message(message, cache))
                .collect::<Result<Vec<_>>>()?;
            hydrate_reply_senders(&mut messages, cache);
            NetworkEvent::History {
                chat_id,
                request_id,
                messages,
            }
        }
        (
            TelegramCommand::LoadOlder {
                chat_id,
                request_id,
                before_id,
            },
            Response::History(raw),
        ) => {
            let mut messages = raw
                .iter()
                .map(|message| map_message(message, cache))
                .collect::<Result<Vec<_>>>()?;
            hydrate_reply_senders(&mut messages, cache);
            NetworkEvent::OlderHistory {
                chat_id,
                request_id,
                before_id,
                messages,
            }
        }
        (
            TelegramCommand::LoadMessage {
                chat_id,
                message_id,
                request_id,
                ..
            },
            Response::Message(raw),
        ) => {
            let mut message = map_message(&raw, cache)?;
            hydrate_reply_sender(&mut message, cache);
            NetworkEvent::MessageLoaded {
                chat_id,
                message_id,
                request_id,
                message,
            }
        }
        (
            TelegramCommand::SendMessage {
                chat_id, local_id, ..
            },
            Response::Message(raw),
        ) => {
            if raw.id() <= 0 {
                NetworkEvent::MessageAccepted { chat_id, local_id }
            } else {
                NetworkEvent::MessageSent {
                    local_id,
                    message: map_message(&raw, cache)?,
                }
            }
        }
        (TelegramCommand::ResolveTelegramLink { .. }, Response::Link(chat, peer, raw)) => {
            cache.peers.insert(chat.id, peer);
            cache.linked_peers.insert(chat.id);
            cache_sender_name(cache, peer.id, chat.title.clone());
            if chat.kind == ChatKind::Group && peer.id.kind() == PeerKind::Channel {
                cache.visible_channel_groups.insert(chat.id);
            }
            let message = raw
                .as_ref()
                .map(|raw| map_message(raw, cache))
                .transpose()?;
            NetworkEvent::LinkResolved { chat, message }
        }
        (
            TelegramCommand::ActivateButton {
                chat_id,
                message_id,
                ..
            },
            Response::Button(message, url),
        ) => NetworkEvent::ButtonActivated {
            chat_id,
            message_id,
            message,
            url,
        },
        (
            TelegramCommand::SetChatUnread {
                chat_id,
                unread,
                request_id,
                ..
            },
            Response::ChatUnread(snapshot),
        ) => {
            cache.dialogs.dirty = true;
            NetworkEvent::ChatUnreadFinished {
                chat_id,
                unread,
                request_id,
                snapshot,
                error: None,
            }
        }
        (TelegramCommand::MarkRead { chat_id, max_id }, Response::Read(snapshot)) => {
            NetworkEvent::ReadMarked {
                chat_id,
                max_id,
                snapshot,
            }
        }
        _ => unreachable!("request and completion kinds always match"),
    };
    events.send(event).await?;
    Ok(())
}
