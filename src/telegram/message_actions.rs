//! Exact-peer snapshot checks shared by destructive message operations.
use anyhow::{Context, Result, ensure};
use grammers_client::{
    Client,
    message::Message,
    tl::{self, Serializable},
};
use grammers_session::types::PeerRef;
use sha2::{Digest, Sha256};

pub(super) async fn fetch(client: &Client, peer: PeerRef, id: i32) -> Result<Message> {
    ensure!(id > 0, "Select a delivered message");
    let message = client
        .get_messages_by_id(peer, &[id])
        .await?
        .pop()
        .flatten()
        .context("Message is no longer available")?;
    // messages.getMessages/deleteMessages do not accept a peer on non-channel
    // chats. Checking only the caller's supplied chat would be insufficient.
    ensure!(
        message.peer_id() == peer.id,
        "Message does not belong to the selected chat"
    );
    Ok(message)
}

pub(super) fn revision(message: &Message) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(message.text().as_bytes());
    hash.update(
        message
            .edit_date()
            .map_or(0, |date| date.timestamp())
            .to_le_bytes(),
    );
    for entity in message.fmt_entities().into_iter().flatten() {
        hash.update(entity.to_bytes());
    }
    if let tl::enums::Message::Message(raw) = &message.raw {
        let media_id = match &raw.media {
            Some(tl::enums::MessageMedia::Photo(media)) => match &media.photo {
                Some(tl::enums::Photo::Photo(photo)) => photo.id,
                _ => 0,
            },
            Some(tl::enums::MessageMedia::Document(media)) => match &media.document {
                Some(tl::enums::Document::Document(document)) => document.id,
                _ => 0,
            },
            _ => 0,
        };
        hash.update(media_id.to_le_bytes());
    }
    hash.finalize().into()
}
