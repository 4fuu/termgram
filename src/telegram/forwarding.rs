use super::{
    message_actions::{fetch, revision},
    peer_display_name,
    sharing::check_protection,
};
use crate::model::{Chat, ChatKind};
use anyhow::{Context, Result, ensure};
use grammers_client::{Client, message::Message, peer::Peer, tl};
use grammers_session::types::PeerRef;

pub(super) struct Destination {
    pub chat: Chat,
    pub peer: PeerRef,
    pub name: String,
}

pub(super) struct Review {
    pub original: Message,
    pub destination: Destination,
    pub revision: [u8; 32],
    pub random_id: i64,
}

pub(super) async fn destination(
    client: &Client,
    id: i64,
    peer: Option<PeerRef>,
    self_id: i64,
) -> Result<Destination> {
    let resolved = if id == self_id {
        Peer::User(client.get_me().await?)
    } else {
        client
            .resolve_peer(peer.context("The target chat is no longer available")?)
            .await?
    };
    let peer = resolved
        .to_ref()
        .await
        .map_err(anyhow::Error::from_boxed)?
        .context("Target chat cannot be addressed")?;
    ensure!(
        peer.id.bot_api_dialog_id() == Some(id),
        "Target chat identity changed"
    );
    let name = peer_display_name(&resolved);
    let chat = Chat {
        id,
        title: if id == self_id {
            "Saved Messages".to_owned()
        } else {
            name.clone()
        },
        kind: match resolved {
            Peer::User(_) => ChatKind::Direct,
            Peer::Group(_) => ChatKind::Group,
            Peer::Channel(_) => ChatKind::Channel,
        },
        membership: crate::folders::ChatMembership::default(),
        unread: 0,
        read_inbox_max_id: None,
        last_message: String::new(),
        last_message_id: None,
        last_activity: None,
    };
    Ok(Destination { chat, peer, name })
}

async fn original(client: &Client, peer: PeerRef, id: i32) -> Result<Message> {
    let message = fetch(client, peer, id).await?;
    check_protection(client, peer, &message).await?;
    let tl::enums::Message::Message(raw) = &message.raw else {
        unreachable!("protection rejects service messages")
    };
    ensure!(
        !matches!(&raw.media,
        Some(tl::enums::MessageMedia::Photo(photo)) if photo.ttl_seconds.is_some())
            && !matches!(&raw.media, Some(tl::enums::MessageMedia::Document(document)) if document.ttl_seconds.is_some()),
        "Expiring media cannot be forwarded"
    );
    Ok(message)
}

pub(super) async fn review(
    client: &Client,
    source: PeerRef,
    id: i32,
    target: Destination,
) -> Result<Review> {
    let original = original(client, source, id).await?;
    let random_id = i64::from_ne_bytes(
        getrandom::u64()
            .map_err(|error| anyhow::anyhow!("Cannot prepare Telegram request ID: {error}"))?
            .to_ne_bytes(),
    );
    Ok(Review {
        revision: revision(&original),
        original,
        destination: target,
        random_id,
    })
}

pub(super) async fn send(
    client: &Client,
    source: PeerRef,
    id: i32,
    target: PeerRef,
    expected: [u8; 32],
    random_id: i64,
) -> Result<()> {
    let message = original(client, source, id).await?;
    ensure!(
        revision(&message) == expected,
        "Original message changed; close and reopen forwarding to review it again"
    );
    client
        .forward_messages_with_random_ids(target, &[id], source, &[random_id])
        .await?;
    Ok(())
}
