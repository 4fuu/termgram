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
    if !cache.dialogs_loading {
        cache.dialogs_loading = true;
        events.send(NetworkEvent::DialogsLoading).await?;
        spawn(TelegramCommand::RefreshDialogs, client, cache, requests);
    }
    Ok(())
}

enum Response {
    Dialogs(Vec<Dialog>),
    History(Vec<TelegramMessage>),
    Message(Box<TelegramMessage>),
    Link(Chat, PeerRef, Option<Box<TelegramMessage>>),
    Button(Option<String>, Option<String>),
    Read,
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
        TelegramCommand::LoadHistory { chat_id, .. }
        | TelegramCommand::LoadMessage { chat_id, .. }
        | TelegramCommand::SendMessage { chat_id, .. }
        | TelegramCommand::ActivateButton { chat_id, .. }
        | TelegramCommand::MarkRead { chat_id } => Some(*chat_id),
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
    let client = client.clone();
    requests.spawn(async move {
        let result = Box::pin(execute(&command, &client, peer, private_link)).await;
        Completion { command, result }
    });
}

async fn execute(
    command: &TelegramCommand,
    client: &Client,
    peer: Option<PeerRef>,
    private_link: Option<(PeerRef, String)>,
) -> Result<Response> {
    let peer = || peer.context("conversation is missing its Telegram peer reference");
    match command {
        TelegramCommand::RefreshDialogs => {
            let mut iter = client.iter_dialogs();
            let mut dialogs = Vec::new();
            while let Some(dialog) = iter.next().await? {
                dialogs.push(dialog);
            }
            Ok(Response::Dialogs(dialogs))
        }
        TelegramCommand::LoadHistory { .. } => {
            let mut iter = client.iter_messages(peer()?).limit(HISTORY_LIMIT);
            let mut messages = Vec::new();
            while let Some(message) = iter.next().await? {
                messages.push(message);
            }
            messages.reverse();
            Ok(Response::History(messages))
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
        TelegramCommand::MarkRead { .. } => {
            client.mark_as_read(peer()?).await?;
            Ok(Response::Read)
        }
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
    if matches!(command, TelegramCommand::RefreshDialogs) {
        cache.dialogs_loading = false;
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
        (TelegramCommand::RefreshDialogs, Response::Dialogs(dialogs)) => {
            NetworkEvent::Dialogs(apply_dialogs(dialogs, cache)?)
        }
        (
            TelegramCommand::LoadHistory {
                chat_id,
                request_id,
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
        (TelegramCommand::MarkRead { chat_id }, Response::Read) => {
            NetworkEvent::ReadMarked { chat_id }
        }
        _ => unreachable!("request and completion kinds always match"),
    };
    events.send(event).await?;
    Ok(())
}
