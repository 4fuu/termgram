use super::message_actions::{fetch, revision};
use crate::deletion::Scope;
use anyhow::{Result, ensure};
use grammers_client::{Client, message::Message, peer::Peer, tl};
use grammers_session::types::{PeerId, PeerKind, PeerRef};

pub(super) struct Review {
    pub message: Message,
    pub revision: [u8; 32],
    pub scopes: Vec<Scope>,
}

pub(super) async fn review(
    client: &Client,
    peer: PeerRef,
    id: i32,
    self_id: i64,
) -> Result<Review> {
    let message = fetch(client, peer, id).await?;
    let tl::enums::Message::Message(raw) = &message.raw else {
        anyhow::bail!("Service messages cannot be deleted here")
    };
    let resolved = client.resolve_peer(peer).await?;
    let channel = match &resolved {
        Peer::Channel(channel) => Some(&channel.raw),
        Peer::Group(group) => match &group.raw {
            tl::enums::Chat::Channel(channel) => Some(channel),
            _ => None,
        },
        Peer::User(_) => None,
    };
    let scopes = if peer.id.kind() == PeerKind::Channel {
        ensure!(id != 1, "The first channel message cannot be deleted");
        let allowed = channel.is_some_and(|channel| {
            let rights = channel.admin_rights.as_ref().map(|rights| {
                let tl::enums::ChatAdminRights::Rights(rights) = rights;
                rights
            });
            channel.creator
                || rights.is_some_and(|rights| rights.delete_messages)
                || (raw.out && (!raw.post || rights.is_some_and(|rights| rights.post_messages)))
        });
        ensure!(
            allowed,
            "You do not have permission to delete this message for everyone"
        );
        vec![Scope::Everyone]
    } else if peer.id == PeerId::user_unchecked(self_id) {
        vec![Scope::OnlyMe]
    } else {
        let tl::enums::Config::Config(config) =
            client.invoke(&tl::functions::help::GetConfig {}).await?;
        let age = i64::from(config.date) - i64::from(raw.date);
        let revoke = match &resolved {
            Peer::User(user) => {
                let accessible = matches!(&user.raw, tl::enums::User::User(user) if !user.deleted && (!user.bot || user.support));
                accessible
                    && age < i64::from(config.revoke_pm_time_limit)
                    && (raw.out || config.revoke_pm_inbox)
            }
            Peer::Group(group) => {
                let admin = matches!(&group.raw, tl::enums::Chat::Chat(chat) if chat.creator || chat.admin_rights.as_ref().is_some_and(|rights| { let tl::enums::ChatAdminRights::Rights(rights) = rights; rights.delete_messages }));
                age < i64::from(config.revoke_time_limit) && (raw.out || admin)
            }
            Peer::Channel(_) => false,
        };
        let mut scopes = vec![Scope::OnlyMe];
        if revoke && !raw.post {
            scopes.push(Scope::Everyone);
        }
        scopes
    };
    Ok(Review {
        revision: revision(&message),
        message,
        scopes,
    })
}

pub(super) async fn delete(
    client: &Client,
    peer: PeerRef,
    id: i32,
    self_id: i64,
    expected: [u8; 32],
    scope: Scope,
) -> Result<()> {
    let latest = review(client, peer, id, self_id).await?;
    ensure!(
        latest.revision == expected,
        "Message changed; reopen deletion to review the latest message"
    );
    ensure!(
        latest.scopes.contains(&scope),
        "Deletion permissions changed; reopen deletion to review the available scope"
    );
    match scope {
        Scope::OnlyMe => {
            ensure!(
                peer.id.kind() != PeerKind::Channel,
                "Supergroups and channels only support deleting for everyone"
            );
            client
                .invoke(&tl::functions::messages::DeleteMessages {
                    revoke: false,
                    id: vec![id],
                })
                .await?;
        }
        Scope::Everyone => {
            client.delete_messages(peer, &[id]).await?;
        }
    }
    Ok(())
}
