//! Native message actions, preserving Telegram content protection.
use super::message_actions::fetch;
use anyhow::{Result, ensure};
use grammers_client::{Client, peer::Peer, tl};
use grammers_session::types::{PeerKind, PeerRef};

pub(super) async fn copy(client: &Client, peer: PeerRef, id: i32, link: bool) -> Result<String> {
    let message = fetch(client, peer, id).await?;
    let tl::enums::Message::Message(raw) = &message.raw else {
        anyhow::bail!("Select a regular message to copy")
    };
    if link {
        ensure!(
            peer.id.kind() == PeerKind::Channel,
            "Telegram message links are available for supergroups and channels"
        );
        let tl::enums::ExportedMessageLink::Link(exported) = client
            .invoke(&tl::functions::channels::ExportMessageLink {
                grouped: false,
                thread: true,
                channel: peer.into(),
                id,
            })
            .await?;
        return Ok(exported.link);
    }
    let resolved = client.resolve_peer(peer).await?;
    let protected = match resolved {
        Peer::User(_) => false,
        Peer::Channel(channel) => channel.raw.noforwards,
        Peer::Group(group) => match group.raw {
            tl::enums::Chat::Chat(chat) => chat.noforwards,
            tl::enums::Chat::Channel(channel) => channel.noforwards,
            _ => true,
        },
    };
    ensure!(
        !raw.noforwards && !protected,
        "Copying is disabled by this chat's content protection"
    );
    ensure!(
        !message.text().is_empty(),
        "This message has no text or caption to copy"
    );
    Ok(message.text().to_owned())
}
